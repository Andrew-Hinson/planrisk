//! `replace`: a resource will be destroyed and re-created.

use crate::action::Action;
use crate::finding::{Finding, Severity};
use crate::plan::Plan;
use crate::rules::{Rule, explain};

pub struct Replace;

impl Rule for Replace {
    fn id(&self) -> &'static str {
        "replace"
    }

    fn description(&self) -> &'static str {
        "A resource will be destroyed and re-created"
    }

    fn default_severity(&self) -> Severity {
        Severity::Medium
    }

    fn check(&self, plan: &Plan) -> Vec<Finding> {
        plan.resource_changes
            .iter()
            .filter_map(|rc| {
                // Only real replaces. `ForgetReplace` keeps the old object,
                // so nothing is destroyed.
                let Action::Replace { create_first } = rc.change.action else {
                    return None;
                };
                let message = if create_first {
                    "will be replaced (new object created before the old one is destroyed)"
                } else {
                    "will be destroyed and re-created"
                };
                Some(Finding {
                    rule_id: self.id(),
                    severity: self.default_severity(),
                    address: Some(rc.address.clone()),
                    message: message.to_string(),
                    reason: explain::replace_reason(rc),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn findings(actions: serde_json::Value) -> Vec<Finding> {
        let plan: Plan = serde_json::from_value(json!({
            "format_version": "1.2",
            "resource_changes": [{
                "address": "aws_db_instance.main",
                "action_reason": "replace_because_cannot_update",
                "change": {"actions": actions, "replace_paths": [["engine"]]}
            }]
        }))
        .unwrap();
        Replace.check(&plan)
    }

    #[test]
    fn fires_on_destroy_first_replace() {
        let found = findings(json!(["delete", "create"]));
        assert_eq!(
            found,
            [Finding {
                rule_id: "replace",
                severity: Severity::Medium,
                address: Some("aws_db_instance.main".to_string()),
                message: "will be destroyed and re-created".to_string(),
                reason: Some("forced by changes to engine".to_string()),
            }]
        );
    }

    #[test]
    fn fires_on_create_first_replace() {
        let found = findings(json!(["create", "delete"]));
        assert_eq!(
            found[0].message,
            "will be replaced (new object created before the old one is destroyed)"
        );
    }

    #[test]
    fn does_not_fire_on_other_actions() {
        for actions in [
            json!(["delete"]),
            json!(["forget", "create"]),
            json!(["create", "forget"]),
            json!(["forget"]),
            json!(["create"]),
            json!(["update"]),
            json!(["no-op"]),
            json!(["teleport"]),
        ] {
            assert!(findings(actions.clone()).is_empty(), "{actions}");
        }
    }
}
