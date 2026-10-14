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
    check_format_version(&value, name)?;
    check_not_errored(&value, name)?;

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

/// The only `format_version` major version planrisk understands.
const SUPPORTED_MAJOR: u32 = 1;

/// Rejects plans in a format version planrisk can't read.
///
/// The format docs promise that minor versions (`1.1`, `1.2`, ...) only add
/// things, and tell consumers to "reject any input which reports an
/// unsupported major version". So any `1.x` is accepted and anything else is
/// an error, rather than a guess that might misreport risk.
fn check_format_version(value: &Value, name: &str) -> Result<()> {
    let Some(found) = value.get("format_version") else {
        bail!(
            "`{name}` has no `format_version`, so it can't be read as a plan.\n\n{HOW_TO_MAKE_A_PLAN}"
        );
    };
    let Some(version) = found.as_str() else {
        bail!("`{name}` has a `format_version` that is not a string: {found}");
    };

    // Expect "MAJOR.MINOR", e.g. "1.2".
    let major = version
        .split_once('.')
        .filter(|(major, minor)| is_number(major) && is_number(minor))
        .and_then(|(major, _)| major.parse::<u32>().ok());
    let Some(major) = major else {
        bail!("`{name}` has a malformed `format_version` \"{version}\" (expected e.g. \"1.2\")");
    };

    // Terraform 1.0 and older write `0.x`; 1.1 made the format stable as `1.0`.
    if major == 0 {
        bail!(
            "`{name}` uses plan format version {version}, from Terraform 1.0 or older. \
             planrisk needs a plan made by Terraform 1.1 or later."
        );
    }
    if major != SUPPORTED_MAJOR {
        bail!(
            "`{name}` uses plan format version {version}, but planrisk only supports \
             {SUPPORTED_MAJOR}.x. A newer planrisk release may support it."
        );
    }
    Ok(())
}

/// Rejects a plan that Terraform couldn't finish.
///
/// When planning fails, Terraform can still save the partial plan, with
/// `"errored": true` and `"complete": false`. Its changes stop wherever the
/// error happened, so a report on it could miss anything, or say "No changes"
/// when Terraform didn't get far enough to find them.
fn check_not_errored(value: &Value, name: &str) -> Result<()> {
    if value.get("errored") == Some(&Value::Bool(true)) {
        bail!(
            "`{name}` is from a Terraform plan that failed with errors, so its list of \
             changes is incomplete. Fix the errors `terraform plan` reports, then run \
             planrisk on the new plan."
        );
    }
    Ok(())
}

/// True for a non-empty string of ASCII digits.
fn is_number(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The full error message, including every cause, as the CLI prints it.
    fn error_text(json: &str) -> String {
        format!("{:#}", parse_plan(json, "plan.json").unwrap_err())
    }

    #[test]
    fn errored_plan_is_rejected() {
        // As Terraform saves a plan that failed: no changes listed at all.
        let msg = error_text(
            r#"{"format_version": "1.2", "applyable": false, "complete": false,
                "errored": true, "planned_values": {}}"#,
        );
        assert!(msg.contains("failed with errors"), "{msg}");
        assert!(
            parse_plan(
                r#"{"format_version": "1.2", "errored": false, "planned_values": {}}"#,
                "p"
            )
            .is_ok()
        );
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
        // Passes the shape and version checks, but resource_changes must be a list.
        let msg = error_text(
            r#"{"format_version": "1.2", "planned_values": {}, "resource_changes": "oops"}"#,
        );
        assert!(
            msg.contains("looks like a plan but could not be read"),
            "{msg}"
        );
    }

    /// A minimal plan whose `format_version` is the given raw JSON value.
    fn plan_with_version(version_json: &str) -> String {
        format!(r#"{{"format_version": {version_json}, "planned_values": {{}}}}"#)
    }

    #[test]
    fn any_1_x_format_version_is_accepted() {
        for version in ["1.0", "1.2", "1.99"] {
            parse_plan(&plan_with_version(&format!("\"{version}\"")), "plan.json")
                .unwrap_or_else(|e| panic!("{version} rejected: {e:#}"));
        }
    }

    #[test]
    fn other_major_versions_are_rejected_by_name() {
        for version in ["2.0", "10.1"] {
            let msg = error_text(&plan_with_version(&format!("\"{version}\"")));
            assert!(
                msg.contains(&format!("plan format version {version}")),
                "{msg}"
            );
            assert!(msg.contains("only supports 1.x"), "{msg}");
        }
    }

    #[test]
    fn plans_from_terraform_1_0_and_older_say_which_terraform_is_needed() {
        // Terraform 1.0.11 writes "0.2" (checked with a real plan).
        let msg = error_text(&plan_with_version("\"0.2\""));
        assert!(msg.contains("from Terraform 1.0 or older"), "{msg}");
        assert!(msg.contains("Terraform 1.1 or later"), "{msg}");
    }

    #[test]
    fn missing_format_version_is_rejected() {
        let msg = error_text(r#"{"planned_values": {}}"#);
        assert!(msg.contains("has no `format_version`"), "{msg}");
    }

    #[test]
    fn malformed_format_version_is_rejected() {
        for version in ["1", "one.two", "1.", ".2", "1.2.3", "-1.0", ""] {
            let msg = error_text(&plan_with_version(&format!("\"{version}\"")));
            assert!(
                msg.contains("malformed `format_version`"),
                "{version:?}: {msg}"
            );
        }
    }

    #[test]
    fn non_string_format_version_is_rejected() {
        let msg = error_text(&plan_with_version("1.2"));
        assert!(msg.contains("not a string"), "{msg}");
    }
}
