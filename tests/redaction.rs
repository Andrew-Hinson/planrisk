//! Secrets in a plan must never appear in planrisk's output.
//!
//! `tests/fixtures/sensitive.json` contains two secrets in plain text, as
//! `terraform show -json` writes them: one from a `sensitive` variable and one
//! from an attribute the provider marks sensitive.

use planrisk::input::read_plan;
use planrisk::json::render_json;
use planrisk::markdown::render_markdown;
use planrisk::redact::PLACEHOLDER;
use planrisk::report::Report;
use planrisk::rules::all_rules;
use planrisk::summary::render_text;

const FIXTURE: &str = "tests/fixtures/sensitive.json";
const SECRETS: [&str; 2] = [
    "PLANRISK-FIXTURE-SECRET-VARIABLE",
    "PLANRISK-FIXTURE-SECRET-SCHEMA",
];

fn report() -> Report {
    Report::build(read_plan(FIXTURE).unwrap(), &all_rules())
}

#[test]
fn fixture_really_contains_the_secrets() {
    // Without this, the tests below could pass just because the secrets
    // were never there.
    let raw = std::fs::read_to_string(FIXTURE).unwrap();
    for secret in SECRETS {
        assert!(raw.contains(secret), "fixture is missing {secret}");
    }
}

#[test]
fn no_output_format_contains_a_secret() {
    let report = report();
    // Add each new output format here.
    let outputs = [
        ("terminal", render_text(&report)),
        ("json", render_json(&report)),
        ("markdown", render_markdown(&report)),
    ];
    for (format, output) in outputs {
        for secret in SECRETS {
            assert!(!output.contains(secret), "{format} output leaks {secret}");
        }
    }
}

#[test]
fn attribute_accessors_hide_secrets() {
    let plan = read_plan(FIXTURE).unwrap();
    for rc in &plan.resource_changes {
        let after = rc.change.after().to_string();
        for secret in SECRETS {
            assert!(!after.contains(secret), "{} leaks {secret}", rc.address);
        }
    }

    let db = plan
        .resource_changes
        .iter()
        .find(|rc| rc.address == "aws_db_instance.main")
        .unwrap();
    assert_eq!(db.change.after()["password"], PLACEHOLDER);
    assert_eq!(db.change.after()["username"], "planrisk"); // not sensitive

    let secret = plan
        .resource_changes
        .iter()
        .find(|rc| rc.address == "aws_secretsmanager_secret_version.api")
        .unwrap();
    assert_eq!(secret.change.after()["secret_string"], PLACEHOLDER);
}
