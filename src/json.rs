//! JSON output (`--format json`), for scripts and other tools.
//!
//! # Compatibility
//!
//! The output carries a `format_version`, following the same rules as
//! Terraform's own JSON output: a new minor version (`1.1`) only adds fields,
//! and a new major version (`2.0`) means a breaking change. Consumers should
//! ignore fields they don't recognise and check the major version.
//!
//! Every field below is always present; values that don't apply are `null`.
//!
//! ```json
//! {
//!   "format_version": "1.0",
//!   "planrisk_version": "0.1.0",
//!   "terraform_version": "1.16.1",
//!   "plan_format_version": "1.2",
//!   "max_severity": "critical",
//!   "counts": { "create": 0, "update": 0, "replace": 1, "delete": 0, "forget": 0,
//!               "read": 0, "import": 0, "move": 0, "unknown": 0 },
//!   "findings": [
//!     { "rule_id": "stateful-replace", "severity": "critical",
//!       "address": "aws_db_instance.main", "message": "database will be destroyed and re-created",
//!       "reason": "forced by changes to engine_version" }
//!   ]
//! }
//! ```

use serde::Serialize;

use crate::finding::{Finding, Severity};
use crate::report::Report;
use crate::summary::Counts;

/// Version of this JSON format. See the module docs for what changes it.
pub const FORMAT_VERSION: &str = "1.0";

/// The JSON document. A separate struct from `Report`, so internal changes to
/// `Report` can't change the JSON format by accident.
///
/// The `'a` lifetime means this borrows from a `Report` instead of copying
/// it; the struct can't outlive the report it was made from.
#[derive(Serialize)]
struct Output<'a> {
    format_version: &'static str,
    planrisk_version: &'static str,
    terraform_version: Option<&'a str>,
    plan_format_version: &'a str,
    max_severity: Option<Severity>,
    counts: &'a Counts,
    findings: &'a [Finding],
}

/// Renders the report as pretty-printed JSON, ending with a newline.
pub fn render_json(report: &Report) -> String {
    let output = Output {
        format_version: FORMAT_VERSION,
        planrisk_version: env!("CARGO_PKG_VERSION"),
        terraform_version: report.plan.terraform_version.as_deref(),
        plan_format_version: &report.plan.format_version,
        max_severity: report.max_severity(),
        counts: &report.counts,
        findings: &report.findings,
    };
    // Serializing these plain structs can't fail: every key is a string and
    // every value is serializable.
    let mut json = serde_json::to_string_pretty(&output).expect("report serializes to JSON");
    json.push('\n');
    json
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::Plan;
    use serde_json::{Value, json};

    #[test]
    fn findings_serialize_with_every_field() {
        let plan = Plan::from_json(
            r#"{"format_version": "1.2", "planned_values": {},
                "resource_changes": [{"address": "aws_db_instance.main", "type": "aws_db_instance",
                  "change": {"actions": ["delete", "create"], "replace_paths": [["engine_version"]]}}]}"#,
        )
        .unwrap();
        let report = Report::build(plan, &crate::rules::all_rules());
        let output: Value = serde_json::from_str(&render_json(&report)).unwrap();

        assert_eq!(output["format_version"], "1.0");
        assert_eq!(output["terraform_version"], Value::Null);
        assert_eq!(output["plan_format_version"], "1.2");
        assert_eq!(output["max_severity"], "critical");
        assert_eq!(output["counts"]["replace"], 1);
        assert_eq!(output["counts"]["move"], 0);
        assert_eq!(
            output["findings"],
            json!([{
                "rule_id": "stateful-replace",
                "severity": "critical",
                "address": "aws_db_instance.main",
                "message": "database will be destroyed and re-created",
                "reason": "forced by changes to engine_version"
            }])
        );
    }

    #[test]
    fn no_findings_means_null_max_severity_and_empty_list() {
        let plan = Plan::from_json(include_str!("../tests/fixtures/empty.json")).unwrap();
        let output: Value = serde_json::from_str(&render_json(&Report::build(
            plan,
            &crate::rules::all_rules(),
        )))
        .unwrap();
        assert_eq!(output["max_severity"], Value::Null);
        assert_eq!(output["findings"], json!([]));
    }
}
