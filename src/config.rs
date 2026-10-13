//! The optional config file, `.planrisk.toml` (PLAN.md §10).
//!
//! planrisk works with no config. A config can change a rule's severity,
//! turn a rule off, do either for matching resource addresses only, and set
//! the blast radius thresholds.
//!
//! Settings apply to a finding in this order, and the last one to set a
//! value wins:
//!
//! 1. the rule's own severity
//! 2. `[rules.<id>]`
//! 3. each matching `[[overrides]]` entry, in the order they appear in the file
//!
//! So a later override can turn a rule back on for some addresses after an
//! earlier one turned it off. Overrides only match findings about a resource;
//! findings about the whole plan, such as blast radius, are configured with
//! `[rules.<id>]`.
//!
//! Unknown keys and unknown rule IDs are errors, so a typo fails loudly
//! instead of being silently ignored.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, bail};
use globset::{GlobBuilder, GlobMatcher};
use serde::Deserialize;

use crate::finding::{Finding, Severity};

/// The file loaded when `--config` isn't given, if it exists.
pub const DEFAULT_PATH: &str = ".planrisk.toml";

/// A loaded and checked config. `Config::default()` changes nothing.
#[derive(Debug, Default)]
pub struct Config {
    pub thresholds: Thresholds,
    rules: HashMap<String, Settings>,
    overrides: Vec<Override>,
}

/// `[thresholds]`. A threshold left out keeps the rule's default.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Thresholds {
    /// `blast-radius` fires when more than this many resources change.
    pub blast_radius_changes: Option<usize>,
    /// `blast-radius` fires when more than this many resources are destroyed.
    pub blast_radius_deletes: Option<usize>,
}

/// What `[rules.<id>]` and `[[overrides]]` entries can set.
#[derive(Debug, Default, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    severity: Option<Severity>,
    enabled: Option<bool>,
}

/// One `[[overrides]]` entry, with its address glob compiled.
#[derive(Debug)]
struct Override {
    address: GlobMatcher,
    /// A rule ID, or `None` for `rule = "*"`.
    rule: Option<String>,
    settings: Settings,
}

/// The file as written, before rule IDs and globs are checked.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    thresholds: Thresholds,
    #[serde(default)]
    rules: HashMap<String, Settings>,
    #[serde(default)]
    overrides: Vec<RawOverride>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOverride {
    address: String,
    rule: String,
    // Listed here rather than as a flattened `Settings`, because serde's
    // `flatten` doesn't work with `deny_unknown_fields`.
    severity: Option<Severity>,
    enabled: Option<bool>,
}

impl Config {
    /// Reads and checks a config file. `rule_ids` are the IDs of every
    /// rule, to catch unknown ones.
    pub fn load(path: &Path, rule_ids: &[&str]) -> anyhow::Result<Config> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("could not read config file `{}`", path.display()))?;
        Config::parse(&text, rule_ids)
            .with_context(|| format!("invalid config file `{}`", path.display()))
    }

    /// Parses and checks config text. `rule_ids` are the IDs of every rule,
    /// to catch unknown ones.
    pub fn parse(text: &str, rule_ids: &[&str]) -> anyhow::Result<Config> {
        let raw: RawConfig = toml::from_str(text)?;

        for id in raw.rules.keys() {
            check_rule_id(id, rule_ids).with_context(|| format!("in [rules.{id}]"))?;
        }

        let mut overrides = Vec::new();
        for (i, raw) in raw.overrides.into_iter().enumerate() {
            let context = || format!("in [[overrides]] entry {}", i + 1);
            let rule = if raw.rule == "*" {
                None
            } else {
                check_rule_id(&raw.rule, rule_ids).with_context(context)?;
                Some(raw.rule)
            };
            let settings = Settings {
                severity: raw.severity,
                enabled: raw.enabled,
            };
            if settings.severity.is_none() && settings.enabled.is_none() {
                return Err(anyhow::anyhow!("it sets neither `severity` nor `enabled`"))
                    .with_context(context);
            }
            overrides.push(Override {
                address: compile_address_glob(&raw.address).with_context(context)?,
                rule,
                settings,
            });
        }

        Ok(Config {
            thresholds: raw.thresholds,
            rules: raw.rules,
            overrides,
        })
    }

    /// Applies the config to one finding: returns it with its severity
    /// changed, or `None` if the config turns it off.
    pub fn apply(&self, mut finding: Finding) -> Option<Finding> {
        let mut enabled = true;
        let mut apply = |settings: &Settings| {
            if let Some(severity) = settings.severity {
                finding.severity = severity;
            }
            if let Some(on) = settings.enabled {
                enabled = on;
            }
        };

        if let Some(settings) = self.rules.get(finding.rule_id) {
            apply(settings);
        }
        if let Some(address) = finding.address.clone() {
            for o in &self.overrides {
                let rule_matches = o.rule.as_deref().is_none_or(|id| id == finding.rule_id);
                if rule_matches && o.address.is_match(&address) {
                    apply(&o.settings);
                }
            }
        }

        enabled.then_some(finding)
    }
}

fn check_rule_id(id: &str, rule_ids: &[&str]) -> anyhow::Result<()> {
    if rule_ids.contains(&id) {
        Ok(())
    } else {
        bail!("unknown rule `{id}`; run `planrisk rules` to list rule IDs")
    }
}

/// Compiles an address glob. `*` matches any characters, including dots,
/// and `?` matches one character. Everything else matches itself, so
/// `aws_instance.web[0]` and `module.x["a"]` work as written; in a plain
/// glob, the brackets would be a character class.
fn compile_address_glob(pattern: &str) -> anyhow::Result<GlobMatcher> {
    let mut escaped = String::new();
    for c in pattern.chars() {
        match c {
            '*' | '?' => escaped.push(c),
            _ => escaped.push_str(&globset::escape(c.encode_utf8(&mut [0; 4]))),
        }
    }
    let glob = GlobBuilder::new(&escaped)
        .literal_separator(false)
        .build()
        .with_context(|| format!("invalid address glob `{pattern}`"))?;
    Ok(glob.compile_matcher())
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDS: &[&str] = &["delete", "stateful-delete", "iam-wildcard", "blast-radius"];

    fn parse(text: &str) -> Config {
        Config::parse(text, IDS).unwrap()
    }

    /// The error and its causes on one line, as `main` prints them.
    fn error(text: &str) -> String {
        format!("{:#}", Config::parse(text, IDS).unwrap_err())
    }

    fn finding(rule_id: &'static str, address: Option<&str>) -> Finding {
        Finding {
            rule_id,
            severity: Severity::Medium,
            address: address.map(str::to_string),
            message: "message".to_string(),
            reason: None,
        }
    }

    /// The severity after applying `config`, or `None` if turned off.
    fn severity(config: &Config, rule_id: &'static str, address: Option<&str>) -> Option<Severity> {
        config.apply(finding(rule_id, address)).map(|f| f.severity)
    }

    #[test]
    fn empty_config_changes_nothing() {
        let config = parse("");
        assert_eq!(
            severity(&config, "delete", Some("aws_instance.a")),
            Some(Severity::Medium)
        );
        assert_eq!(config.thresholds.blast_radius_changes, None);
    }

    #[test]
    fn plan_md_example_parses() {
        let config = parse(
            r#"
            [thresholds]
            blast_radius_changes = 40
            blast_radius_deletes = 10

            [rules.delete]
            severity = "low"

            [rules.iam-wildcard]
            enabled = false

            [[overrides]]
            address = "module.sandbox.*"
            rule = "*"
            severity = "low"

            [[overrides]]
            address = "aws_s3_bucket.logs"
            rule = "stateful-delete"
            enabled = false
            "#,
        );
        assert_eq!(config.thresholds.blast_radius_changes, Some(40));
        assert_eq!(config.thresholds.blast_radius_deletes, Some(10));
        assert_eq!(config.overrides.len(), 2);
    }

    #[test]
    fn rule_severity_applies_everywhere() {
        let config = parse("[rules.delete]\nseverity = \"low\"");
        assert_eq!(
            severity(&config, "delete", Some("aws_instance.a")),
            Some(Severity::Low)
        );
        assert_eq!(
            severity(&config, "delete", Some("module.m.aws_instance.b")),
            Some(Severity::Low)
        );
        assert_eq!(
            severity(&config, "stateful-delete", Some("aws_instance.a")),
            Some(Severity::Medium)
        );
    }

    #[test]
    fn rule_can_be_disabled() {
        let config = parse("[rules.iam-wildcard]\nenabled = false");
        assert_eq!(
            severity(&config, "iam-wildcard", Some("aws_iam_policy.p")),
            None
        );
        assert_eq!(
            severity(&config, "delete", Some("aws_iam_policy.p")),
            Some(Severity::Medium)
        );
    }

    #[test]
    fn rule_settings_apply_to_plan_level_findings() {
        let config = parse("[rules.blast-radius]\nseverity = \"high\"");
        assert_eq!(
            severity(&config, "blast-radius", None),
            Some(Severity::High)
        );
    }

    #[test]
    fn override_for_one_rule_and_address() {
        let config = parse(
            r#"
            [[overrides]]
            address = "aws_s3_bucket.logs"
            rule = "stateful-delete"
            enabled = false
            "#,
        );
        assert_eq!(
            severity(&config, "stateful-delete", Some("aws_s3_bucket.logs")),
            None
        );
        assert_eq!(
            severity(&config, "stateful-delete", Some("aws_s3_bucket.data")),
            Some(Severity::Medium)
        );
        assert_eq!(
            severity(&config, "delete", Some("aws_s3_bucket.logs")),
            Some(Severity::Medium)
        );
    }

    #[test]
    fn override_for_every_rule_with_a_glob() {
        let config = parse(
            r#"
            [[overrides]]
            address = "module.sandbox.*"
            rule = "*"
            severity = "low"
            "#,
        );
        for rule in ["delete", "stateful-delete"] {
            assert_eq!(
                severity(&config, rule, Some("module.sandbox.aws_instance.a")),
                Some(Severity::Low)
            );
        }
        // `*` crosses dots, so nested modules match too.
        assert_eq!(
            severity(
                &config,
                "delete",
                Some("module.sandbox.module.inner.aws_instance.a")
            ),
            Some(Severity::Low)
        );
        assert_eq!(
            severity(&config, "delete", Some("module.prod.aws_instance.a")),
            Some(Severity::Medium)
        );
    }

    #[test]
    fn overrides_never_match_plan_level_findings() {
        let config = parse(
            r#"
            [[overrides]]
            address = "*"
            rule = "*"
            enabled = false
            "#,
        );
        assert_eq!(
            severity(&config, "blast-radius", None),
            Some(Severity::Medium)
        );
        assert_eq!(severity(&config, "delete", Some("aws_instance.a")), None);
    }

    #[test]
    fn brackets_in_addresses_match_literally() {
        let config = parse(
            r#"
            [[overrides]]
            address = 'aws_instance.web[0]'
            rule = "delete"
            severity = "low"

            [[overrides]]
            address = 'module.app["eu"].*'
            rule = "delete"
            severity = "high"
            "#,
        );
        assert_eq!(
            severity(&config, "delete", Some("aws_instance.web[0]")),
            Some(Severity::Low)
        );
        assert_eq!(
            severity(&config, "delete", Some("aws_instance.web0")),
            Some(Severity::Medium)
        );
        assert_eq!(
            severity(
                &config,
                "delete",
                Some(r#"module.app["eu"].aws_instance.a"#)
            ),
            Some(Severity::High)
        );
        assert_eq!(
            severity(
                &config,
                "delete",
                Some(r#"module.app["us"].aws_instance.a"#)
            ),
            Some(Severity::Medium)
        );
    }

    #[test]
    fn later_settings_win() {
        let config = parse(
            r#"
            [rules.delete]
            enabled = false

            [[overrides]]
            address = "aws_instance.*"
            rule = "*"
            severity = "low"

            [[overrides]]
            address = "aws_instance.important"
            rule = "delete"
            enabled = true
            severity = "critical"
            "#,
        );
        // [rules.delete] turns it off, and the first override only changes severity.
        assert_eq!(severity(&config, "delete", Some("aws_instance.a")), None);
        // The second override turns it back on.
        assert_eq!(
            severity(&config, "delete", Some("aws_instance.important")),
            Some(Severity::Critical)
        );
        assert_eq!(
            severity(&config, "stateful-delete", Some("aws_instance.a")),
            Some(Severity::Low)
        );
    }

    #[test]
    fn typo_in_a_key_is_an_error() {
        assert!(error("[rules.delete]\nseverty = \"low\"").contains("unknown field `severty`"));
        assert!(
            error("[treshholds]\nblast_radius_changes = 1").contains("unknown field `treshholds`")
        );
        assert!(
            error("[thresholds]\nblast_radius_destroys = 1")
                .contains("unknown field `blast_radius_destroys`")
        );
        assert!(
            error(
                "[[overrides]]\naddress = \"a\"\nrule = \"*\"\nseverity = \"low\"\nenable = false"
            )
            .contains("unknown field `enable`")
        );
    }

    #[test]
    fn bad_values_are_errors() {
        assert!(
            error("[rules.delete]\nseverity = \"severe\"").contains("unknown variant `severe`")
        );
        assert!(error("[thresholds]\nblast_radius_changes = -1").contains("blast_radius_changes"));
        assert!(
            error("[[overrides]]\nrule = \"*\"\nseverity = \"low\"")
                .contains("missing field `address`")
        );
    }

    #[test]
    fn unknown_rule_id_is_an_error() {
        assert_eq!(
            error("[rules.iam-wildcards]\nenabled = false"),
            "in [rules.iam-wildcards]: unknown rule `iam-wildcards`; run `planrisk rules` to list rule IDs"
        );
        assert_eq!(
            error("[[overrides]]\naddress = \"a\"\nrule = \"stateful_delete\"\nenabled = false"),
            "in [[overrides]] entry 1: unknown rule `stateful_delete`; run `planrisk rules` to list rule IDs"
        );
    }

    #[test]
    fn override_that_sets_nothing_is_an_error() {
        assert_eq!(
            error("[[overrides]]\naddress = \"a\"\nrule = \"*\""),
            "in [[overrides]] entry 1: it sets neither `severity` nor `enabled`"
        );
    }
}
