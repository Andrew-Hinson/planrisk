//! Markdown output (`--format markdown`), for pull request comments.
//!
//! Written for GitHub-flavored Markdown:
//! - It starts with a hidden marker, [`MARKER`], so the GitHub Action can
//!   find its earlier comment and update it instead of adding a new one.
//! - Findings go in a table; the full list of changes is folded into a
//!   `<details>` block, as plan-comment tools commonly do.
//! - Text from the plan (addresses, messages) is escaped, so it can't break
//!   the table, turn into formatting, or inject HTML.
//! - Output is capped at [`MAX_BYTES`]. GitHub rejects comments over 65,536
//!   characters; the limit is widely observed but not officially documented,
//!   and it's unclear whether it counts characters or bytes, so the cap is in
//!   bytes with a margin.

use std::fmt::Write;

use crate::finding::{Finding, Severity};
use crate::report::Report;
use crate::summary::{ChangeLine, describe};

/// First line of every comment. Changing it breaks comment updates.
pub const MARKER: &str = "<!-- planrisk -->";

/// Largest output in bytes; see the module docs.
pub const MAX_BYTES: usize = 60_000;

/// Ends a comment that was cut short.
const TRUNCATED_NOTE: &str =
    "\n_This comment was cut to fit GitHub's size limit. The full report is in the CI log._\n";

/// Renders the report as Markdown, cut to [`MAX_BYTES`] if needed.
pub fn render_markdown(report: &Report) -> String {
    render_markdown_with_limit(report, MAX_BYTES)
}

/// Like [`render_markdown`], with a different size limit. The limit should
/// be at least a few hundred bytes, so the heading and marker fit.
pub fn render_markdown_with_limit(report: &Report, limit: usize) -> String {
    let full = render_full(report);
    if full.len() <= limit {
        full
    } else {
        truncate(&full, limit)
    }
}

fn render_full(report: &Report) -> String {
    let plan = &report.plan;
    let changes: Vec<ChangeLine> = plan.resource_changes.iter().filter_map(describe).collect();
    // `write!` to a String can't fail, so the results are ignored with `let _ =`.
    let mut out = String::new();

    let _ = writeln!(out, "{MARKER}");
    let _ = writeln!(
        out,
        "### planrisk: {}\n",
        headline(report, changes.is_empty())
    );

    if !changes.is_empty() {
        let _ = write!(out, "**Plan:** {}", report.counts.describe());
        if let Some(version) = &plan.terraform_version {
            let _ = write!(out, " · Terraform {}", escape(version));
        }
        let _ = writeln!(out, "\n");
    }

    if !report.findings.is_empty() {
        let _ = writeln!(out, "| Severity | Resource | Finding | Rule |");
        let _ = writeln!(out, "|---|---|---|---|");
        for finding in &report.findings {
            let _ = writeln!(out, "{}", finding_row(finding));
        }
        let _ = writeln!(out);
    }

    if !changes.is_empty() {
        details(&mut out, &format!("Changes ({})", changes.len()), &changes);
    }

    let drift: Vec<ChangeLine> = plan.resource_drift.iter().filter_map(describe).collect();
    if !drift.is_empty() {
        let title = format!(
            "Changed outside Terraform ({}, information only)",
            drift.len()
        );
        details(&mut out, &title, &drift);
    }

    if !plan.deferred_changes.is_empty() {
        let deferred: Vec<ChangeLine> = plan
            .deferred_changes
            .iter()
            .map(|d| {
                let mut line = describe(&d.resource_change).unwrap_or_else(|| ChangeLine {
                    label: String::new(),
                    address: d.resource_change.address.clone(),
                    notes: Vec::new(),
                });
                line.notes.push(format!("reason: {}", d.reason));
                line
            })
            .collect();
        let title = format!(
            "Deferred to a later plan ({}, information only)",
            deferred.len()
        );
        details(&mut out, &title, &deferred);
    }

    out
}

/// e.g. "2 critical, 3 medium", "no findings" or "no changes".
fn headline(report: &Report, no_changes: bool) -> String {
    if report.findings.is_empty() {
        return if no_changes {
            "no changes"
        } else {
            "no findings"
        }
        .to_string();
    }
    let mut parts = Vec::new();
    for severity in [
        Severity::Critical,
        Severity::High,
        Severity::Medium,
        Severity::Low,
    ] {
        let count = report
            .findings
            .iter()
            .filter(|f| f.severity == severity)
            .count();
        if count > 0 {
            parts.push(format!("{count} {severity}"));
        }
    }
    parts.join(", ")
}

fn finding_row(finding: &Finding) -> String {
    let resource = match &finding.address {
        Some(address) => code(address),
        None => "whole plan".to_string(),
    };
    let mut text = escape(&finding.message);
    if let Some(reason) = &finding.reason {
        let _ = write!(text, " ({})", escape(reason));
    }
    format!(
        "| **{}** | {resource} | {text} | {} |",
        finding.severity,
        code(finding.rule_id)
    )
}

/// A folded table of changes. GitHub needs the blank line after `<summary>`
/// to render Markdown inside `<details>`.
fn details(out: &mut String, title: &str, lines: &[ChangeLine]) {
    let _ = writeln!(out, "<details><summary>{}</summary>\n", escape(title));
    let _ = writeln!(out, "| Action | Resource | Notes |");
    let _ = writeln!(out, "|---|---|---|");
    for line in lines {
        let notes: Vec<String> = line.notes.iter().map(|n| escape(n)).collect();
        let _ = writeln!(
            out,
            "| {} | {} | {} |",
            escape(&line.label),
            code(&line.address),
            notes.join("; ")
        );
    }
    let _ = writeln!(out, "\n</details>\n");
}

/// Escapes text so Markdown and HTML in it are shown literally. GitHub's
/// Markdown lets any ASCII punctuation be escaped with a backslash. This
/// covers formatting (`*`, `_`, `~`, `` ` ``), links (`[`, `]`), HTML (`<`,
/// `>`, `&`) and table cells (`|`).
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(
            c,
            '\\' | '`' | '*' | '_' | '~' | '[' | ']' | '<' | '>' | '&' | '|'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Shows text as inline code. Backslash escapes don't work inside code, so
/// a backtick in the text is handled with a longer fence instead. `|` still
/// needs escaping, since tables are split into cells before code is parsed.
fn code(text: &str) -> String {
    let text = text.replace('|', "\\|");
    // Use one more backtick than the longest run inside the text.
    let longest_run = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest_run + 1);
    // A space keeps a leading or trailing backtick from joining the fence.
    let pad = if text.starts_with('`') || text.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{fence}{pad}{text}{pad}{fence}")
}

/// Cuts `full` to at most `limit` bytes, at the end of a line, then closes any
/// `<details>` left open and adds a note.
fn truncate(full: &str, limit: usize) -> String {
    const CLOSE: &str = "\n</details>\n";
    // Room for the note and for closing every `<details>` the output has.
    let reserve = TRUNCATED_NOTE.len() + CLOSE.len() * full.matches("<details>").count();
    let mut cut = limit.saturating_sub(reserve);
    // Never split a multi-byte character.
    while !full.is_char_boundary(cut) {
        cut -= 1;
    }
    // Back up to the end of the last whole line.
    let kept = match full[..cut].rfind('\n') {
        Some(end) => &full[..=end],
        None => "",
    };

    let mut out = kept.to_string();
    let open = kept.matches("<details>").count() - kept.matches("</details>").count();
    for _ in 0..open {
        out.push_str(CLOSE);
    }
    out.push_str(TRUNCATED_NOTE);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::Plan;
    use crate::rules::all_rules;

    fn report(json: &str) -> Report {
        Report::build(Plan::from_json(json).unwrap(), &all_rules())
    }

    #[test]
    fn starts_with_the_marker() {
        for json in [
            include_str!("../tests/fixtures/empty.json"),
            include_str!("../tests/fixtures/mixed-changes.json"),
        ] {
            assert!(render_markdown(&report(json)).starts_with("<!-- planrisk -->\n"));
        }
    }

    #[test]
    fn headline_counts_findings_by_severity() {
        let md = render_markdown(&report(include_str!(
            "../tests/fixtures/mixed-changes.json"
        )));
        assert!(md.contains("### planrisk: 2 critical, 3 medium\n"), "{md}");
        let md = render_markdown(&report(include_str!("../tests/fixtures/create-only.json")));
        assert!(md.contains("### planrisk: no findings\n"));
        let md = render_markdown(&report(include_str!("../tests/fixtures/empty.json")));
        assert!(md.contains("### planrisk: no changes\n"));
    }

    #[test]
    fn text_is_escaped() {
        assert_eq!(
            escape(r#"grants "*:*" to a|b <b>&"#),
            r#"grants "\*:\*" to a\|b \<b\>\&"#
        );
        assert_eq!(
            escape("[link](x) _x_ ~x~ `x` \\"),
            r"\[link\](x) \_x\_ \~x\~ \`x\` \\"
        );
    }

    #[test]
    fn addresses_are_code_with_pipes_escaped() {
        assert_eq!(code("aws_instance.web[0]"), "`aws_instance.web[0]`");
        assert_eq!(code(r#"x.y["a|b"]"#), r#"`x.y["a\|b"]`"#);
        assert_eq!(code("x.y[\"a`b\"]"), "``x.y[\"a`b\"]``");
        assert_eq!(code("`x"), "`` `x ``");
    }

    #[test]
    fn short_output_is_not_truncated() {
        let report = report(include_str!("../tests/fixtures/large.json"));
        let md = render_markdown(&report);
        assert!(md.len() < MAX_BYTES);
        assert!(!md.contains("cut to fit"));
    }

    #[test]
    fn large_fixture_truncates_cleanly() {
        let report = report(include_str!("../tests/fixtures/large.json"));
        let full = render_markdown(&report);
        for limit in [1_500, 2_000, 3_000, full.len() - 1] {
            let md = render_markdown_with_limit(&report, limit);
            assert!(md.len() <= limit, "{} > {limit}", md.len());
            assert!(md.starts_with(MARKER));
            assert!(md.ends_with(TRUNCATED_NOTE));
            assert_eq!(
                md.matches("<details>").count(),
                md.matches("</details>").count(),
                "unbalanced <details> at limit {limit}"
            );
            // Cut at the end of a whole line of the full output.
            let kept = md.split(TRUNCATED_NOTE).next().unwrap();
            let kept = kept
                .trim_end_matches("\n</details>\n")
                .trim_end_matches('\n');
            assert!(full.starts_with(kept));
        }
    }

    #[test]
    fn truncation_never_splits_a_character() {
        // Lines of 3-byte characters, so most byte limits fall mid-character.
        let full = format!("{MARKER}\n{}", "€€€€€€€€€€\n".repeat(50));
        for limit in 200..400 {
            let md = truncate(&full, limit);
            assert!(md.len() <= limit);
            // A split character would have panicked when slicing; check the
            // kept lines are whole.
            assert!(
                md.lines().all(|l| l.is_empty()
                    || l == MARKER
                    || l == "€€€€€€€€€€"
                    || l.starts_with('_'))
            );
        }
    }

    #[test]
    fn real_limit_holds_for_a_huge_plan() {
        // 3,000 changes with long addresses: far over 60,000 bytes.
        let changes: Vec<String> = (0..3000)
            .map(|i| {
                format!(
                    r#"{{"address": "module.very_long_module_name_{i}.aws_sqs_queue.queue_with_a_long_name", "change": {{"actions": ["update"]}}}}"#
                )
            })
            .collect();
        let json = format!(
            r#"{{"format_version": "1.2", "planned_values": {{}}, "resource_changes": [{}]}}"#,
            changes.join(",")
        );
        let md = render_markdown(&report(&json));
        assert!(md.len() <= MAX_BYTES, "{}", md.len());
        assert!(
            md.len() > MAX_BYTES - 1_000,
            "cut far too early: {}",
            md.len()
        );
        assert!(md.ends_with(TRUNCATED_NOTE));
        assert_eq!(
            md.matches("<details>").count(),
            md.matches("</details>").count()
        );
    }
}
