//! The terminal summary: counts per action, findings, then one line per change.
//!
//! It never prints attribute values, so values that are only known after
//! apply (`after_unknown`) can't be shown as removed, and sensitive values
//! can't leak.
//!
//! [`render_text`] gives plain text; [`render_text_colored`] adds ANSI
//! colors. Changes use the colors `terraform plan` uses for each action
//! (create green, update yellow, destroy red, read cyan); findings are
//! colored by severity. Whether colors are actually shown (terminal or not,
//! `NO_COLOR`, `--no-color`) is decided where the output is written.

use std::fmt::Write;

use anstyle::{AnsiColor, Style};
use serde::Serialize;

use crate::action::Action;
use crate::finding::{Finding, Severity};
use crate::plan::ResourceChange;
use crate::report::Report;

/// How many resources each kind of change affects.
///
/// Serialized as-is in JSON output, so renaming a field is a breaking
/// change to the JSON format (see `json.rs`).
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub create: usize,
    pub update: usize,
    /// Includes replaces where the old object is forgotten instead of destroyed.
    pub replace: usize,
    pub delete: usize,
    pub forget: usize,
    pub read: usize,
    pub import: usize,
    /// Named `move` in JSON; `move` is a Rust keyword, so it can't be the field name.
    #[serde(rename = "move")]
    pub moved: usize,
    pub unknown: usize,
}

impl Counts {
    pub fn from_changes(changes: &[ResourceChange]) -> Counts {
        let mut counts = Counts::default();
        for rc in changes {
            match &rc.change.action {
                Action::NoOp => {}
                Action::Create => counts.create += 1,
                Action::Update => counts.update += 1,
                Action::Replace { .. } | Action::ForgetReplace { .. } => counts.replace += 1,
                Action::Delete => counts.delete += 1,
                Action::Forget => counts.forget += 1,
                Action::Read => counts.read += 1,
                Action::Unknown(_) => counts.unknown += 1,
            }
            // Imports and moves are counted on top of the action: a moved
            // resource can also be updated, for example.
            if rc.change.is_importing() {
                counts.import += 1;
            }
            if rc.previous_address.is_some() {
                counts.moved += 1;
            }
        }
        counts
    }

    /// e.g. "3 to create, 0 to update, 0 to replace, 0 to delete, 1 to import".
    /// The first four are always shown, like Terraform's own summary; the
    /// rest only when non-zero.
    fn describe(&self) -> String {
        let mut parts = vec![
            format!("{} to create", self.create),
            format!("{} to update", self.update),
            format!("{} to replace", self.replace),
            format!("{} to delete", self.delete),
        ];
        for (count, what) in [
            (self.forget, "to forget"),
            (self.read, "to read"),
            (self.import, "to import"),
            (self.moved, "to move"),
            (self.unknown, "with an unknown action"),
        ] {
            if count > 0 {
                parts.push(format!("{count} {what}"));
            }
        }
        parts.join(", ")
    }
}

/// Renders the summary as plain text.
pub fn render_text(report: &Report) -> String {
    render(report, Palette { colored: false })
}

/// Renders the summary with ANSI colors.
pub fn render_text_colored(report: &Report) -> String {
    render(report, Palette { colored: true })
}

/// Colors for the summary. With `colored: false`, `paint` returns the text
/// unchanged, so one renderer produces both outputs.
#[derive(Clone, Copy)]
struct Palette {
    colored: bool,
}

impl Palette {
    fn paint(self, style: Style, text: &str) -> String {
        if self.colored {
            // `{style}` starts the style and `{style:#}` resets it.
            format!("{style}{text}{style:#}")
        } else {
            text.to_string()
        }
    }

    fn heading(self, text: &str) -> String {
        self.paint(Style::new().bold(), text)
    }

    fn severity(self, severity: Severity, text: &str) -> String {
        let style = match severity {
            Severity::Critical => color(AnsiColor::Red).bold(),
            Severity::High => color(AnsiColor::Red),
            Severity::Medium => color(AnsiColor::Yellow),
            Severity::Low => color(AnsiColor::Blue),
        };
        self.paint(style, text)
    }

    /// Terraform's color for each action, from `terraform plan` output.
    fn action(self, label: &str, text: &str) -> String {
        let style = match label {
            "create" => color(AnsiColor::Green),
            "update" => color(AnsiColor::Yellow),
            "replace" | "delete" | "forget" => color(AnsiColor::Red),
            "read" => color(AnsiColor::Cyan),
            _ => Style::new(),
        };
        self.paint(style, text)
    }
}

fn color(color: AnsiColor) -> Style {
    Style::new().fg_color(Some(color.into()))
}

fn render(report: &Report, palette: Palette) -> String {
    let plan = &report.plan;
    // `write!` to a String can't fail, so the results are safely ignored
    // with `let _ =` throughout.
    let mut out = String::new();

    let _ = writeln!(
        out,
        "Terraform {}, plan format {}",
        plan.terraform_version
            .as_deref()
            .unwrap_or("(unknown version)"),
        plan.format_version
    );

    let line = |(label, rest): (String, String)| {
        // Pad before painting: color codes would throw off the padding.
        format!("{}{rest}", palette.action(&label, &format!("{label:<8}")))
    };
    let lines: Vec<String> = plan
        .resource_changes
        .iter()
        .filter_map(describe)
        .map(line)
        .collect();
    if lines.is_empty() {
        let _ = writeln!(out, "No changes.");
    } else {
        let _ = writeln!(out, "{}", report.counts.describe());
    }

    // Each section below starts with a blank line, so spacing stays right
    // whichever sections are present.

    // Findings come before the resource list: they are what a reviewer
    // needs first. "No findings" is skipped for an empty plan, where it
    // would only repeat "No changes".
    if !report.findings.is_empty() {
        let _ = writeln!(out, "\n{}", palette.heading("Findings:"));
        for finding in &report.findings {
            let _ = writeln!(out, "  {}", describe_finding(finding, palette));
        }
    } else if !lines.is_empty() {
        let _ = writeln!(out, "\nNo findings.");
    }

    if !lines.is_empty() {
        let _ = writeln!(out, "\n{}", palette.heading("Changes:"));
        for line in &lines {
            let _ = writeln!(out, "  {line}");
        }
    }

    let drift: Vec<String> = plan
        .resource_drift
        .iter()
        .filter_map(describe)
        .map(line)
        .collect();
    if !drift.is_empty() {
        let heading = palette.heading("Changed outside Terraform (information only):");
        let _ = writeln!(out, "\n{heading}");
        for line in &drift {
            let _ = writeln!(out, "  {line}");
        }
    }

    if !plan.deferred_changes.is_empty() {
        let heading = palette.heading("Deferred to a later plan (information only):");
        let _ = writeln!(out, "\n{heading}");
        for deferred in &plan.deferred_changes {
            let text = describe(&deferred.resource_change)
                .map(line)
                .unwrap_or_else(|| deferred.resource_change.address.clone());
            let _ = writeln!(out, "  {text} (reason: {})", deferred.reason);
        }
    }

    out
}

/// One line for a finding, e.g.
/// `critical  aws_db_instance.main: database must be replaced (...) [stateful-replace]`.
fn describe_finding(finding: &Finding, palette: Palette) -> String {
    let severity = format!("{:<8}", finding.severity);
    let mut line = format!("{}  ", palette.severity(finding.severity, &severity));
    if let Some(address) = &finding.address {
        let _ = write!(line, "{address}: ");
    }
    line.push_str(&finding.message);
    if let Some(reason) = &finding.reason {
        let _ = write!(line, " ({reason})");
    }
    let _ = write!(line, " [{}]", finding.rule_id);
    line
}

/// A resource change as (label, rest of line), e.g. `("replace",
/// " aws_lb.front (create replacement and then destroy)")`. The label is
/// separate so it can be padded and colored.
/// Returns `None` for no-ops that aren't moves or imports: nothing happens to them.
fn describe(rc: &ResourceChange) -> Option<(String, String)> {
    let mut notes = Vec::new();

    let label = match &rc.change.action {
        Action::NoOp => None,
        Action::Create => Some("create".to_string()),
        Action::Update => Some("update".to_string()),
        Action::Delete => Some("delete".to_string()),
        Action::Read => Some("read".to_string()),
        Action::Forget => Some("forget".to_string()),
        Action::Replace { create_first } => {
            if *create_first {
                notes.push("create replacement and then destroy".to_string());
            }
            Some("replace".to_string())
        }
        Action::ForgetReplace { .. } => {
            notes.push("existing object will not be destroyed".to_string());
            Some("replace".to_string())
        }
        Action::Unknown(list) => Some(format!("unknown {list:?}")),
    };

    let importing = rc.change.is_importing();
    let moved_from = rc.previous_address.as_deref();

    // A pure move or import has the action no-op, but is still worth listing.
    let label = match label {
        Some(label) => {
            if importing {
                notes.push("import".to_string());
            }
            if let Some(old) = moved_from {
                notes.push(format!("moved from {old}"));
            }
            label
        }
        None if importing => "import".to_string(),
        None => {
            let old = moved_from?; // `?` returns None here: nothing to show.
            notes.push(format!("moved from {old}"));
            "move".to_string()
        }
    };

    if let Some(key) = &rc.deposed {
        notes.push(format!("deposed object {key}"));
    }

    let mut rest = format!(" {}", rc.address);
    if !notes.is_empty() {
        let _ = write!(rest, " ({})", notes.join("; "));
    }
    Some((label, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::finding::Severity;
    use crate::plan::Plan;

    fn plan(json: &str) -> Plan {
        Plan::from_json(json).unwrap()
    }

    /// A report with no rules run, so it has no findings.
    fn report(json: &str) -> Report {
        Report::build(plan(json), &[])
    }

    #[test]
    fn empty_plan_says_no_changes() {
        let text = render_text(&report(include_str!("../tests/fixtures/empty.json")));
        assert_eq!(text, "Terraform 1.16.1, plan format 1.2\nNo changes.\n");
    }

    #[test]
    fn create_only_plan() {
        let text = render_text(&report(include_str!("../tests/fixtures/create-only.json")));
        assert_eq!(
            text,
            "\
Terraform 1.16.1, plan format 1.2
3 to create, 0 to update, 0 to replace, 0 to delete

No findings.

Changes:
  create   aws_s3_bucket.assets
  create   aws_security_group.web
  create   module.queue.aws_sqs_queue.this
"
        );
    }

    /// A plan with one of every kind of change, in Terraform's JSON shape.
    const EVERYTHING: &str = r#"{
        "format_version": "1.2",
        "terraform_version": "1.16.1",
        "resource_changes": [
            {"address": "a.noop",      "change": {"actions": ["no-op"]}},
            {"address": "a.create",    "change": {"actions": ["create"]}},
            {"address": "a.update",    "change": {"actions": ["update"]}},
            {"address": "a.replace",   "change": {"actions": ["delete", "create"]}},
            {"address": "a.cbd",       "change": {"actions": ["create", "delete"]}},
            {"address": "a.forgetrep", "change": {"actions": ["forget", "create"]}},
            {"address": "a.delete",    "change": {"actions": ["delete"]}},
            {"address": "a.deposed",   "change": {"actions": ["delete"]}, "deposed": "abc123"},
            {"address": "a.forget",    "change": {"actions": ["forget"]}},
            {"address": "data.a.read", "change": {"actions": ["read"]}},
            {"address": "a.moved",     "change": {"actions": ["no-op"]}, "previous_address": "a.old"},
            {"address": "a.movedup",   "change": {"actions": ["update"]}, "previous_address": "a.old2"},
            {"address": "a.imported",  "change": {"actions": ["no-op"], "importing": {"id": "x"}}},
            {"address": "a.impup",     "change": {"actions": ["update"], "importing": {"id": "y"}}},
            {"address": "a.weird",     "change": {"actions": ["teleport"]}}
        ],
        "resource_drift": [
            {"address": "a.drifted", "change": {"actions": ["update"]}},
            {"address": "a.same",    "change": {"actions": ["no-op"]}}
        ],
        "deferred_changes": [
            {"reason": "instance_count_unknown",
             "resource_change": {"address": "a.later", "change": {"actions": ["create"]}}}
        ],
        "planned_values": {}
    }"#;

    #[test]
    fn every_kind_of_change() {
        let text = render_text(&report(EVERYTHING));
        assert_eq!(
            text,
            r#"Terraform 1.16.1, plan format 1.2
1 to create, 3 to update, 3 to replace, 2 to delete, 1 to forget, 1 to read, 2 to import, 2 to move, 1 with an unknown action

No findings.

Changes:
  create   a.create
  update   a.update
  replace  a.replace
  replace  a.cbd (create replacement and then destroy)
  replace  a.forgetrep (existing object will not be destroyed)
  delete   a.delete
  delete   a.deposed (deposed object abc123)
  forget   a.forget
  read     data.a.read
  move     a.moved (moved from a.old)
  update   a.movedup (moved from a.old2)
  import   a.imported
  update   a.impup (import)
  unknown ["teleport"] a.weird

Changed outside Terraform (information only):
  update   a.drifted

Deferred to a later plan (information only):
  create   a.later (reason: instance_count_unknown)
"#
        );
    }

    #[test]
    fn counts_match_every_fixture() {
        let empty = plan(include_str!("../tests/fixtures/empty.json"));
        assert_eq!(
            Counts::from_changes(&empty.resource_changes),
            Counts::default()
        );

        let create_only = plan(include_str!("../tests/fixtures/create-only.json"));
        assert_eq!(
            Counts::from_changes(&create_only.resource_changes),
            Counts {
                create: 3,
                ..Counts::default()
            }
        );
    }

    #[test]
    fn plain_no_ops_are_hidden() {
        let text = render_text(&report(
            r#"{"format_version": "1.2", "planned_values": {},
                "resource_changes": [{"address": "a.same", "change": {"actions": ["no-op"]}}]}"#,
        ));
        assert!(text.ends_with("No changes.\n"), "{text}");
    }

    #[test]
    fn findings_come_before_changes() {
        let mut report = report(include_str!("../tests/fixtures/create-only.json"));
        report.findings = vec![
            Finding {
                rule_id: "stateful-replace",
                severity: Severity::Critical,
                address: Some("aws_db_instance.main".to_string()),
                message: "database will be replaced".to_string(),
                reason: Some("engine_version changed".to_string()),
            },
            Finding {
                rule_id: "blast-radius",
                severity: Severity::Medium,
                address: None,
                message: "40 resources change".to_string(),
                reason: None,
            },
        ];
        let text = render_text(&report);
        assert_eq!(
            text,
            "\
Terraform 1.16.1, plan format 1.2
3 to create, 0 to update, 0 to replace, 0 to delete

Findings:
  critical  aws_db_instance.main: database will be replaced (engine_version changed) [stateful-replace]
  medium    40 resources change [blast-radius]

Changes:
  create   aws_s3_bucket.assets
  create   aws_security_group.web
  create   module.queue.aws_sqs_queue.this
"
        );
    }

    #[test]
    fn colored_output_matches_plain_once_colors_are_removed() {
        for json in [
            EVERYTHING,
            include_str!("../tests/fixtures/mixed-changes.json"),
            include_str!("../tests/fixtures/access-changes.json"),
        ] {
            let report = Report::build(plan(json), &crate::rules::all_rules());
            let colored = render_text_colored(&report);
            assert!(colored.contains("\u{1b}["), "no color codes in output");
            assert_eq!(
                anstream::adapter::strip_str(&colored).to_string(),
                render_text(&report)
            );
        }
    }

    #[test]
    fn colors_follow_terraform_and_severity() {
        let palette = Palette { colored: true };
        // ESC[32m is green, ESC[33m yellow, ESC[31m red, ESC[36m cyan.
        assert_eq!(palette.action("create", "x"), "\u{1b}[32mx\u{1b}[0m");
        assert_eq!(palette.action("update", "x"), "\u{1b}[33mx\u{1b}[0m");
        assert_eq!(palette.action("delete", "x"), "\u{1b}[31mx\u{1b}[0m");
        assert_eq!(palette.action("read", "x"), "\u{1b}[36mx\u{1b}[0m");
        assert_eq!(palette.action("move", "x"), "x");
        // Critical is bold red (ESC[1m bold, then red).
        assert_eq!(
            palette.severity(Severity::Critical, "x"),
            "\u{1b}[1m\u{1b}[31mx\u{1b}[0m"
        );
        assert_eq!(
            Palette { colored: false }.severity(Severity::Critical, "x"),
            "x"
        );
    }
}
