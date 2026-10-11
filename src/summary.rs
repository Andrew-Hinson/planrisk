//! The plain-text summary: counts per action, findings, then one line per change.
//!
//! It never prints attribute values, so values that are only known after
//! apply (`after_unknown`) can't be shown as removed, and sensitive values
//! can't leak.

use std::fmt::Write;

use serde::Serialize;

use crate::action::Action;
use crate::finding::Finding;
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

/// Renders the whole summary for a checked plan.
pub fn render_text(report: &Report) -> String {
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

    let lines: Vec<String> = plan.resource_changes.iter().filter_map(describe).collect();
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
        let _ = writeln!(out, "\nFindings:");
        for finding in &report.findings {
            let _ = writeln!(out, "  {}", describe_finding(finding));
        }
    } else if !lines.is_empty() {
        let _ = writeln!(out, "\nNo findings.");
    }

    if !lines.is_empty() {
        let _ = writeln!(out, "\nChanges:");
        for line in &lines {
            let _ = writeln!(out, "  {line}");
        }
    }

    let drift: Vec<String> = plan.resource_drift.iter().filter_map(describe).collect();
    if !drift.is_empty() {
        let _ = writeln!(out, "\nChanged outside Terraform (information only):");
        for line in &drift {
            let _ = writeln!(out, "  {line}");
        }
    }

    if !plan.deferred_changes.is_empty() {
        let _ = writeln!(out, "\nDeferred to a later plan (information only):");
        for deferred in &plan.deferred_changes {
            let line = describe(&deferred.resource_change)
                .unwrap_or_else(|| deferred.resource_change.address.clone());
            let _ = writeln!(out, "  {line} (reason: {})", deferred.reason);
        }
    }

    out
}

/// One line for a resource change, e.g.
/// `replace  aws_db_instance.main (create before destroy)`.
/// Returns `None` for no-ops that aren't moves or imports: nothing happens to them.
/// One line for a finding, e.g.
/// `critical  aws_db_instance.main: will be replaced (engine_version changed) [stateful-replace]`.
fn describe_finding(finding: &Finding) -> String {
    let mut line = format!("{:<8}  ", finding.severity);
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

fn describe(rc: &ResourceChange) -> Option<String> {
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
                notes.push("create before destroy".to_string());
            }
            Some("replace".to_string())
        }
        Action::ForgetReplace { .. } => {
            notes.push("old object forgotten, not destroyed".to_string());
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

    let mut line = format!("{label:<8} {}", rc.address);
    if !notes.is_empty() {
        let _ = write!(line, " ({})", notes.join("; "));
    }
    Some(line)
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
  replace  a.cbd (create before destroy)
  replace  a.forgetrep (old object forgotten, not destroyed)
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
}
