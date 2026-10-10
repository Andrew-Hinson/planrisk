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
        .stdout(contains("3 resource changes"));
}

#[test]
fn reads_a_plan_from_stdin() {
    let json = std::fs::read_to_string(CREATE_ONLY).unwrap();
    cargo_bin_cmd!("planrisk")
        .arg("-")
        .write_stdin(json)
        .assert()
        .success()
        .stdout(contains("3 resource changes"));
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
