//! End-to-end tests that run the compiled `planrisk` binary.

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use predicates::str::contains;

const CREATE_ONLY: &str = "tests/fixtures/create-only.json";

#[test]
fn reads_a_plan_file() {
    cargo_bin_cmd!("planrisk")
        .args([CREATE_ONLY, "--format", "terminal"])
        .assert()
        .success()
        .stdout(contains("3 to create"));
}

#[test]
fn reads_a_plan_from_stdin() {
    let json = std::fs::read_to_string(CREATE_ONLY).unwrap();
    cargo_bin_cmd!("planrisk")
        .args(["-", "--format", "terminal"])
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

/// A plan with exactly one finding, a critical stateful-replace. Inline so
/// each threshold's expected count is obvious; the real-plan check is
/// `fail_on_critical_fails_the_mixed_changes_fixture`.
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

#[test]
fn rules_subcommand_lists_rules() {
    cargo_bin_cmd!("planrisk")
        .arg("rules")
        .assert()
        .success()
        .stdout(contains("stateful-replace  critical"));
}

#[test]
fn rules_subcommand_takes_no_plan_options() {
    cargo_bin_cmd!("planrisk")
        .args(["rules", "--fail-on", "low"])
        .assert()
        .code(1);
}

#[test]
fn plan_is_still_required_without_a_subcommand() {
    cargo_bin_cmd!("planrisk")
        .args(["--format", "json"])
        .assert()
        .code(1)
        .stderr(contains("<PLAN_JSON>"));
}

#[test]
fn fail_on_critical_fails_the_mixed_changes_fixture() {
    // A real Terraform plan with a database replace and a bucket delete.
    cargo_bin_cmd!("planrisk")
        .args(["tests/fixtures/mixed-changes.json", "--fail-on", "critical"])
        .assert()
        .code(2)
        .stderr(contains("2 findings at or above --fail-on critical"));
}

#[test]
fn format_version_2_fixture_exits_1() {
    cargo_bin_cmd!("planrisk")
        .arg("tests/fixtures/invalid/format-version-2.json")
        .assert()
        .code(1)
        .stderr(contains("plan format version 2.0"));
}

#[test]
fn piped_output_has_no_colors() {
    // assert_cmd captures stdout, so it isn't a terminal.
    cargo_bin_cmd!("planrisk")
        .args(["tests/fixtures/mixed-changes.json", "--format", "terminal"])
        .assert()
        .success()
        .stdout(predicates::str::contains("\u{1b}[").not());
}

#[test]
fn clicolor_force_turns_colors_on_and_no_color_wins() {
    cargo_bin_cmd!("planrisk")
        .args(["tests/fixtures/mixed-changes.json", "--format", "terminal"])
        .env("CLICOLOR_FORCE", "1")
        .assert()
        .success()
        .stdout(contains("\u{1b}["));
    // https://no-color.org: NO_COLOR overrides everything.
    cargo_bin_cmd!("planrisk")
        .args(["tests/fixtures/mixed-changes.json", "--format", "terminal"])
        .env("CLICOLOR_FORCE", "1")
        .env("NO_COLOR", "1")
        .assert()
        .success()
        .stdout(predicates::str::contains("\u{1b}[").not());
}

#[test]
fn no_color_flag_turns_colors_off() {
    cargo_bin_cmd!("planrisk")
        .args([
            "tests/fixtures/mixed-changes.json",
            "--format",
            "terminal",
            "--no-color",
        ])
        .env("CLICOLOR_FORCE", "1")
        .assert()
        .success()
        .stdout(predicates::str::contains("\u{1b}[").not());
}

#[test]
fn piped_output_defaults_to_markdown() {
    // assert_cmd captures stdout, so it isn't a terminal: like CI.
    cargo_bin_cmd!("planrisk")
        .arg("tests/fixtures/mixed-changes.json")
        .assert()
        .success()
        .stdout(predicates::str::starts_with("<!-- planrisk -->\n"));
}

#[test]
fn markdown_has_no_colors_even_when_forced() {
    cargo_bin_cmd!("planrisk")
        .args(["tests/fixtures/mixed-changes.json", "--format", "markdown"])
        .env("CLICOLOR_FORCE", "1")
        .assert()
        .success()
        .stdout(predicates::str::contains("\u{1b}[").not());
}

const MIXED_CHANGES: &str = "tests/fixtures/mixed-changes.json";

/// `planrisk --format json` findings as (rule, address, severity).
fn findings(output: &[u8]) -> Vec<(String, Option<String>, String)> {
    let report: serde_json::Value = serde_json::from_slice(output).unwrap();
    report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["rule_id"].as_str().unwrap().to_string(),
                f["address"].as_str().map(str::to_string),
                f["severity"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn config_changes_severities_and_disables_findings() {
    let output = cargo_bin_cmd!("planrisk")
        .args([MIXED_CHANGES, "--format", "json", "--fail-on", "critical"])
        .args(["--config", "tests/fixtures/config/quiet-mixed-changes.toml"])
        .assert()
        .code(0)
        .get_output()
        .stdout
        .clone();
    let found = findings(&output);
    let find = |rule: &str, address: &str| {
        found
            .iter()
            .find(|(r, a, _)| r == rule && a.as_deref() == Some(address))
            .map(|(_, _, severity)| severity.as_str())
    };
    assert_eq!(
        find("stateful-replace", "aws_db_instance.main"),
        Some("high")
    );
    // stateful-delete is off for the bucket, so its plain delete shows.
    assert_eq!(find("stateful-delete", "aws_s3_bucket.logs"), None);
    assert_eq!(find("delete", "aws_s3_bucket.logs"), Some("medium"));
    // The override's brackets match the address literally.
    assert_eq!(find("delete", "aws_sqs_queue.workers[2]"), Some("low"));
    assert_eq!(find("delete", "aws_sqs_queue.unused"), Some("medium"));
}

#[test]
fn config_thresholds_change_blast_radius() {
    let has_blast_radius = |args: &[&str]| {
        let output = cargo_bin_cmd!("planrisk")
            .args(["tests/fixtures/large.json", "--format", "json"])
            .args(args)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        findings(&output)
            .iter()
            .any(|(rule, _, _)| rule == "blast-radius")
    };
    assert!(has_blast_radius(&[]));
    assert!(!has_blast_radius(&[
        "--config",
        "tests/fixtures/config/quiet-mixed-changes.toml"
    ]));
}

#[test]
fn planrisk_toml_in_the_current_directory_is_loaded() {
    let dir = "tests/fixtures/config/cwd";
    let plan = std::fs::canonicalize(MIXED_CHANGES).unwrap();
    cargo_bin_cmd!("planrisk")
        .current_dir(dir)
        .arg(&plan)
        .args(["--format", "json"])
        .assert()
        .success()
        .stdout(contains("stateful-replace").and(contains("stateful-delete").not()));
    // An explicit --config replaces it.
    let other = std::fs::canonicalize("tests/fixtures/config/quiet-mixed-changes.toml").unwrap();
    cargo_bin_cmd!("planrisk")
        .current_dir(dir)
        .arg(&plan)
        .args(["--format", "json", "--config"])
        .arg(&other)
        .assert()
        .success()
        .stdout(
            contains("stateful-delete")
                .not()
                .and(contains(r#""severity": "high""#)),
        );
}

#[test]
fn missing_config_file_exits_1() {
    cargo_bin_cmd!("planrisk")
        .args([CREATE_ONLY, "--config", "no-such.toml"])
        .assert()
        .code(1)
        .stderr(contains("could not read config file `no-such.toml`"));
}

#[test]
fn unknown_rule_in_config_exits_1() {
    cargo_bin_cmd!("planrisk")
        .args([
            CREATE_ONLY,
            "--config",
            "tests/fixtures/config/unknown-rule.toml",
        ])
        .assert()
        .code(1)
        .stderr(contains(
            "invalid config file `tests/fixtures/config/unknown-rule.toml`: \
             in [rules.stateful-delte]: unknown rule `stateful-delte`",
        ));
}

#[test]
fn typo_in_config_exits_1() {
    cargo_bin_cmd!("planrisk")
        .args([CREATE_ONLY, "--config", "tests/fixtures/config/typo.toml"])
        .assert()
        .code(1)
        .stderr(contains("invalid config file").and(contains("unknown field `severty`")));
}

/// Runs planrisk on a plan with many deletes, closing its stdout before
/// reading anything, as `planrisk plan.json | head -1` does once head is done.
/// The output is larger than a pipe buffer, so planrisk's writes fail with a
/// broken pipe.
fn run_with_closed_stdout(args: &[&str]) -> std::process::Output {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let changes: Vec<String> = (0..5000)
        .map(|i| {
            format!(r#"{{"address": "aws_instance.r{i}", "change": {{"actions": ["delete"]}}}}"#)
        })
        .collect();
    let plan = format!(
        r#"{{"format_version": "1.2", "planned_values": {{}}, "resource_changes": [{}]}}"#,
        changes.join(",")
    );

    let mut child = Command::new(env!("CARGO_BIN_EXE_planrisk"))
        .arg("-")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    child
        .stdin
        .take()
        .unwrap()
        .write_all(plan.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn closed_stdout_is_not_an_error() {
    for format in ["terminal", "markdown", "json"] {
        let output = run_with_closed_stdout(&["--format", format]);
        assert_eq!(output.status.code(), Some(0), "--format {format}");
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            "",
            "--format {format}"
        );
    }
}

#[test]
fn closed_stdout_still_exits_2_for_fail_on() {
    let output = run_with_closed_stdout(&["--format", "json", "--fail-on", "medium"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("at or above --fail-on medium"));
}
