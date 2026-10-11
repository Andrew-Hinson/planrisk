//! End-to-end tests that run the compiled `planrisk` binary.

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::str::contains;

const CREATE_ONLY: &str = "tests/fixtures/create-only.json";

#[test]
fn reads_a_plan_file() {
    cargo_bin_cmd!("planrisk")
        .arg(CREATE_ONLY)
        .assert()
        .success()
        .stdout(contains("3 to create"));
}

#[test]
fn reads_a_plan_from_stdin() {
    let json = std::fs::read_to_string(CREATE_ONLY).unwrap();
    cargo_bin_cmd!("planrisk")
        .arg("-")
        .write_stdin(json)
        .assert()
        .success()
        .stdout(contains("3 to create"));
}

#[test]
fn missing_file_exits_1_with_the_path() {
    cargo_bin_cmd!("planrisk")
        .arg("does-not-exist.json")
        .assert()
        .code(1)
        .stderr(contains("could not read plan file `does-not-exist.json`"));
}

#[test]
fn invalid_json_from_stdin_exits_1() {
    cargo_bin_cmd!("planrisk")
        .arg("-")
        .write_stdin("not json at all")
        .assert()
        .code(1)
        .stderr(contains("`stdin` is not valid JSON"));
}

#[test]
fn state_instead_of_plan_exits_1() {
    cargo_bin_cmd!("planrisk")
        .arg("-")
        .write_stdin(r#"{"format_version": "1.0", "values": {}}"#)
        .assert()
        .code(1)
        .stderr(contains("is Terraform state"));
}

#[test]
fn bad_arguments_exit_1_not_2() {
    // clap's default for usage errors is 2, which planrisk reserves for --fail-on.
    cargo_bin_cmd!("planrisk").assert().code(1);
    cargo_bin_cmd!("planrisk")
        .args([CREATE_ONLY, "--no-such-flag"])
        .assert()
        .code(1);
}

#[test]
fn help_exits_0() {
    cargo_bin_cmd!("planrisk")
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("PLAN_JSON"));
}

#[test]
fn unsupported_format_version_exits_1() {
    cargo_bin_cmd!("planrisk")
        .arg("-")
        .write_stdin(r#"{"format_version": "2.0", "planned_values": {}}"#)
        .assert()
        .code(1)
        .stderr(contains("plan format version 2.0"));
}

#[test]
fn json_format_is_valid_json() {
    let output = cargo_bin_cmd!("planrisk")
        .args([CREATE_ONLY, "--format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["format_version"], "1.0");
    assert_eq!(json["counts"]["create"], 3);
}

#[test]
fn unknown_format_exits_1() {
    cargo_bin_cmd!("planrisk")
        .args([CREATE_ONLY, "--format", "xml"])
        .assert()
        .code(1);
}

/// A plan that replaces a database: one critical finding (stateful-replace).
/// Replace with the stateful-replace fixture once it exists (issue #18).
const DB_REPLACE: &str = r#"{
    "format_version": "1.2", "planned_values": {},
    "resource_changes": [{
        "address": "aws_db_instance.main", "type": "aws_db_instance", "name": "main",
        "change": {"actions": ["delete", "create"]}
    }]
}"#;

#[test]
fn findings_do_not_fail_without_fail_on() {
    cargo_bin_cmd!("planrisk")
        .arg("-")
        .write_stdin(DB_REPLACE)
        .assert()
        .code(0)
        .stdout(contains("critical"));
}

#[test]
fn fail_on_at_or_below_the_finding_exits_2() {
    for threshold in ["low", "medium", "high", "critical"] {
        cargo_bin_cmd!("planrisk")
            .args(["-", "--fail-on", threshold])
            .write_stdin(DB_REPLACE)
            .assert()
            .code(2)
            .stderr(contains(format!(
                "1 finding at or above --fail-on {threshold}"
            )));
    }
}

#[test]
fn fail_on_above_every_finding_exits_0() {
    // The only finding in this plan is medium (a plain delete).
    let medium_only = r#"{"format_version": "1.2", "planned_values": {},
        "resource_changes": [{"address": "aws_instance.web", "type": "aws_instance",
                              "change": {"actions": ["delete"]}}]}"#;
    for threshold in ["high", "critical"] {
        cargo_bin_cmd!("planrisk")
            .args(["-", "--fail-on", threshold])
            .write_stdin(medium_only)
            .assert()
            .code(0);
    }
    cargo_bin_cmd!("planrisk")
        .args(["-", "--fail-on", "medium"])
        .write_stdin(medium_only)
        .assert()
        .code(2);
}

#[test]
fn fail_on_still_prints_the_full_output() {
    cargo_bin_cmd!("planrisk")
        .args(["-", "--fail-on", "critical", "--format", "json"])
        .write_stdin(DB_REPLACE)
        .assert()
        .code(2)
        .stdout(contains(r#""max_severity": "critical""#));
}

#[test]
fn fail_on_with_errors_still_exits_1() {
    cargo_bin_cmd!("planrisk")
        .args(["does-not-exist.json", "--fail-on", "low"])
        .assert()
        .code(1);
    cargo_bin_cmd!("planrisk")
        .args([CREATE_ONLY, "--fail-on", "severe"])
        .assert()
        .code(1);
}

#[test]
fn help_lists_exit_codes() {
    cargo_bin_cmd!("planrisk")
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("Exit codes:"))
        .stdout(contains("--fail-on <SEVERITY>"));
}
