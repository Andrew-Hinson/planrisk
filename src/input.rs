//! Reading a plan from a file or stdin, with errors a CI user can act on.

use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::plan::Plan;

/// What to tell users who passed the wrong kind of file. Shown with every
/// "this isn't a plan" error.
const HOW_TO_MAKE_A_PLAN: &str = "planrisk reads the output of:\n  \
    terraform plan -out plan.out\n  \
    terraform show -json plan.out > plan.json";

/// Reads and parses a plan. `source` is a file path, or `-` for stdin.
pub fn read_plan(source: &str) -> Result<Plan> {
    let text = if source == "-" {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .context("could not read the plan from stdin")?;
        text
    } else {
        std::fs::read_to_string(Path::new(source))
            .with_context(|| format!("could not read plan file `{source}`"))?
    };

    let name = if source == "-" { "stdin" } else { source };
    parse_plan(&text, name)
}

/// Parses plan JSON. `name` is only used in error messages.
///
/// Parses in two steps: first into a generic `Value` to check this really is
/// a plan, then into the typed `Plan`. That way a state file or a log stream
/// gets a message saying what it is, instead of a confusing serde error or,
/// worse, being read as a plan with no changes.
pub fn parse_plan(text: &str, name: &str) -> Result<Plan> {
    let value: Value = serde_json::from_str(text).with_context(|| {
        format!("`{name}` is not valid JSON.\n\n{HOW_TO_MAKE_A_PLAN}\n(`terraform plan -json` prints a log stream, not a plan.)")
    })?;

    check_is_plan(&value, name)?;

    serde_json::from_value(value)
        .with_context(|| format!("`{name}` looks like a plan but could not be read"))
}

/// Rejects JSON that is valid but is not a plan.
///
/// Terraform always writes `planned_values` in a plan. (It is a Go struct,
/// and Go's `omitempty` never drops structs; see `jsonplan.Plan` in
/// Terraform's source.) `terraform show -json` on *state* writes `values`
/// instead, and a raw `terraform.tfstate` file has `lineage` and `serial`.
fn check_is_plan(value: &Value, name: &str) -> Result<()> {
    let Some(object) = value.as_object() else {
        bail!(
            "`{name}` is JSON but not a Terraform plan (expected an object).\n\n{HOW_TO_MAKE_A_PLAN}"
        );
    };

    if object.contains_key("planned_values") {
        return Ok(());
    }

    if object.contains_key("lineage") && object.contains_key("serial") {
        bail!("`{name}` is a Terraform state file, not a plan.\n\n{HOW_TO_MAKE_A_PLAN}");
    }
    if object.contains_key("values") {
        bail!(
            "`{name}` is Terraform state (from `terraform show -json` without a plan file), not a plan.\n\n{HOW_TO_MAKE_A_PLAN}"
        );
    }
    bail!(
        "`{name}` is JSON but not a Terraform plan (no `planned_values` key).\n\n{HOW_TO_MAKE_A_PLAN}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The full error message, including every cause, as the CLI prints it.
    fn error_text(json: &str) -> String {
        format!("{:#}", parse_plan(json, "plan.json").unwrap_err())
    }

    #[test]
    fn reads_a_real_plan() {
        let plan = parse_plan(
            include_str!("../tests/fixtures/create-only.json"),
            "plan.json",
        )
        .unwrap();
        assert_eq!(plan.resource_changes.len(), 3);
    }

    #[test]
    fn invalid_json_says_so() {
        let msg = error_text("{ not json");
        assert!(msg.contains("`plan.json` is not valid JSON"), "{msg}");
        assert!(msg.contains("terraform show -json"), "{msg}");
    }

    #[test]
    fn plan_json_log_stream_is_invalid_json() {
        // `terraform plan -json` prints one JSON object per line.
        let msg = error_text("{\"@level\":\"info\"}\n{\"@level\":\"info\"}\n");
        assert!(msg.contains("log stream"), "{msg}");
    }

    #[test]
    fn show_json_of_state_is_recognised() {
        let msg = error_text(r#"{"format_version": "1.0", "values": {"root_module": {}}}"#);
        assert!(msg.contains("is Terraform state"), "{msg}");
    }

    #[test]
    fn raw_state_file_is_recognised() {
        let msg = error_text(r#"{"version": 4, "serial": 3, "lineage": "abc", "resources": []}"#);
        assert!(msg.contains("is a Terraform state file"), "{msg}");
    }

    #[test]
    fn other_json_is_rejected() {
        assert!(error_text(r#"{"hello": "world"}"#).contains("no `planned_values` key"));
        assert!(error_text("[1, 2]").contains("expected an object"));
    }

    #[test]
    fn plan_shaped_but_broken_is_reported() {
        // Has planned_values but no format_version, which Plan requires.
        let msg = error_text(r#"{"planned_values": {}}"#);
        assert!(
            msg.contains("looks like a plan but could not be read"),
            "{msg}"
        );
        assert!(msg.contains("format_version"), "{msg}");
    }
}
