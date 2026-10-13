//! A risk found in a plan, and how serious it is.

use std::fmt;

use serde::{Deserialize, Serialize};

/// How serious a finding is, from least to most.
///
/// Deriving `PartialOrd` and `Ord` orders the variants as they are written,
/// so `Severity::Low < Severity::Critical`. `--fail-on` and sorting rely on that.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    clap::ValueEnum,
)]
// Written as "low", "medium", ... in JSON output and config files.
// `ValueEnum` lets the CLI accept the same lowercase names, e.g.
// `--fail-on high`.
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Severity::Low => "low",
            Severity::Medium => "medium",
            Severity::High => "high",
            Severity::Critical => "critical",
        };
        // `pad` rather than `write_str`, so width specifiers like `{:<8}` work.
        f.pad(name)
    }
}

/// One risk a rule found.
///
/// Serialized as-is in JSON output, so renaming a field is a breaking
/// change to the JSON format (see `json.rs`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// The rule that found it, e.g. `"stateful-replace"`.
    pub rule_id: &'static str,
    pub severity: Severity,
    /// The resource it is about. `None` for findings about the whole plan,
    /// such as blast radius.
    pub address: Option<String>,
    /// What is happening, e.g. "database will be replaced".
    pub message: String,
    /// Why, when known, e.g. "engine_version forces replacement".
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severities_are_ordered_low_to_critical() {
        assert!(Severity::Low < Severity::Medium);
        assert!(Severity::Medium < Severity::High);
        assert!(Severity::High < Severity::Critical);
        let mut list = vec![Severity::High, Severity::Low, Severity::Critical];
        list.sort();
        assert_eq!(list, [Severity::Low, Severity::High, Severity::Critical]);
    }

    #[test]
    fn severity_displays_lowercase_and_pads() {
        assert_eq!(Severity::Critical.to_string(), "critical");
        assert_eq!(format!("[{:<6}]", Severity::Low), "[low   ]");
    }
}
