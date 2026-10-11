//! `blast-radius`: the plan changes or destroys many resources at once.
//!
//! Large plans are harder to review and harder to roll back if something
//! goes wrong. There is no industry standard for "too many"; the default
//! thresholds are heuristics (PLAN.md §7) and will be configurable (issue #25).

use crate::action::Action;
use crate::finding::{Finding, Severity};
use crate::plan::Plan;
use crate::rules::Rule;

pub struct BlastRadius {
    /// Fires when more than this many resources change.
    pub max_changes: usize,
    /// Fires when more than this many resources are destroyed.
    pub max_destroys: usize,
}

impl Default for BlastRadius {
    fn default() -> Self {
        BlastRadius {
            max_changes: 25,
            max_destroys: 5,
        }
    }
}

impl Rule for BlastRadius {
    fn id(&self) -> &'static str {
        "blast-radius"
    }

    fn description(&self) -> &'static str {
        "Many resources change or are destroyed in one plan"
    }

    fn default_severity(&self) -> Severity {
        Severity::Medium
    }

    fn check(&self, plan: &Plan) -> Vec<Finding> {
        let actions = plan.resource_changes.iter().map(|rc| &rc.change.action);

        // No-ops change nothing, and reads only fetch data.
        let changes = actions
            .clone()
            .filter(|action| !matches!(action, Action::NoOp | Action::Read))
            .count();
        // Like Terraform's "N to destroy", this counts replaces too: they
        // destroy the old object.
        let destroys = actions.filter(|action| action.deletes_object()).count();

        let mut parts = Vec::new();
        if changes > self.max_changes {
            parts.push(format!(
                "{changes} resources change, more than the threshold of {}",
                self.max_changes
            ));
        }
        if destroys > self.max_destroys {
            parts.push(format!(
                "{destroys} to destroy, more than the threshold of {}",
                self.max_destroys
            ));
        }

        if parts.is_empty() {
            return Vec::new();
        }
        vec![Finding {
            rule_id: self.id(),
            severity: self.default_severity(),
            address: None, // about the whole plan
            message: parts.join("; "),
            reason: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    /// A plan with `n` resources having each given action list.
    fn plan(groups: &[(usize, Value)]) -> Plan {
        let mut changes = Vec::new();
        for (n, actions) in groups {
            for _ in 0..*n {
                changes.push(json!({
                    "address": format!("aws_instance.r{}", changes.len()),
                    "change": {"actions": actions}
                }));
            }
        }
        serde_json::from_value(json!({"format_version": "1.2", "resource_changes": changes}))
            .unwrap()
    }

    fn messages(plan: &Plan) -> Vec<String> {
        BlastRadius::default()
            .check(plan)
            .into_iter()
            .map(|f| f.message)
            .collect()
    }

    #[test]
    fn changes_threshold_is_exclusive() {
        assert!(messages(&plan(&[(24, json!(["update"]))])).is_empty());
        assert!(messages(&plan(&[(25, json!(["update"]))])).is_empty());
        assert_eq!(
            messages(&plan(&[(26, json!(["update"]))])),
            ["26 resources change, more than the threshold of 25"]
        );
    }

    #[test]
    fn destroys_threshold_is_exclusive_and_counts_replaces() {
        assert!(messages(&plan(&[(4, json!(["delete"]))])).is_empty());
        assert!(
            messages(&plan(&[
                (3, json!(["delete"])),
                (2, json!(["create", "delete"]))
            ]))
            .is_empty()
        );
        assert_eq!(
            messages(&plan(&[
                (3, json!(["delete"])),
                (3, json!(["delete", "create"]))
            ])),
            ["6 to destroy, more than the threshold of 5"]
        );
    }

    #[test]
    fn both_thresholds_give_one_finding() {
        let found = BlastRadius::default()
            .check(&plan(&[(20, json!(["create"])), (10, json!(["delete"]))]));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].address, None);
        assert_eq!(found[0].severity, Severity::Medium);
        assert_eq!(
            found[0].message,
            "30 resources change, more than the threshold of 25; 10 to destroy, more than the threshold of 5"
        );
    }

    #[test]
    fn no_ops_and_reads_are_not_changes() {
        let quiet = plan(&[
            (50, json!(["no-op"])),
            (50, json!(["read"])),
            (25, json!(["update"])),
        ]);
        assert!(messages(&quiet).is_empty());
    }

    #[test]
    fn forgets_are_changes_but_not_destroys() {
        assert_eq!(
            messages(&plan(&[(26, json!(["forget"]))])),
            ["26 resources change, more than the threshold of 25"]
        );
    }

    #[test]
    fn thresholds_can_be_set() {
        let strict = BlastRadius {
            max_changes: 1,
            max_destroys: 0,
        };
        let found = strict.check(&plan(&[(1, json!(["delete"]))]));
        assert_eq!(
            found[0].message,
            "1 to destroy, more than the threshold of 0"
        );
    }
}
