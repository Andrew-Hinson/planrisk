//! Snapshot tests: every fixture in `tests/fixtures/*.json`, in every output
//! format, compared against a saved copy in `tests/snapshots/`.
//!
//! Adding a fixture adds snapshots automatically. When output changes on
//! purpose, review and accept the new snapshots with `cargo insta review`
//! (install with `cargo install cargo-insta`), or run the tests with
//! `INSTA_UPDATE=always` and review the diff in git.

use std::path::Path;

use planrisk::input::read_plan;
use planrisk::json::render_json;
use planrisk::report::Report;
use planrisk::rules::{all_rules, render_rule_list};

/// Paths of every fixture, sorted so test output is stable.
fn fixtures() -> Vec<std::path::PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir("tests/fixtures")
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no fixtures found");
    paths
}

fn report(path: &Path) -> Report {
    Report::build(read_plan(path.to_str().unwrap()).unwrap(), &all_rules())
}

fn name(path: &Path) -> String {
    path.file_stem().unwrap().to_string_lossy().into_owned()
}

#[test]
fn json_output() {
    for path in fixtures() {
        let json = render_json(&report(&path));
        // The planrisk version changes with every release; keep it out of
        // the snapshots so they only change when the output does.
        let json = json.replace(
            &format!(r#""planrisk_version": "{}""#, env!("CARGO_PKG_VERSION")),
            r#""planrisk_version": "[version]""#,
        );
        insta::assert_snapshot!(format!("json__{}", name(&path)), json);
    }
}

#[test]
fn rule_list() {
    // Built from all_rules(), so a new rule shows up here as a snapshot diff.
    insta::assert_snapshot!("rules", render_rule_list(&all_rules()));
}
