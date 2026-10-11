//! `stateful-replace`: a resource that holds data must be replaced.
//!
//! A replace creates a new, empty object, so the data in the old one is lost
//! unless it is restored from a backup. That holds even for create-first
//! replaces.

use crate::action::Action;
use crate::finding::{Finding, Severity};
use crate::plan::Plan;
use crate::rules::{Rule, explain, replace, stateful};

pub struct StatefulReplace;

impl Rule for StatefulReplace {
    fn id(&self) -> &'static str {
        "stateful-replace"
    }

    fn description(&self) -> &'static str {
        "A resource that holds data (database, bucket, disk, ...) must be replaced"
    }

    fn default_severity(&self) -> Severity {
        Severity::Critical
    }

    fn supersedes(&self) -> Option<&'static str> {
        Some("replace")
    }

    fn check(&self, plan: &Plan) -> Vec<Finding> {
        plan.resource_changes
            .iter()
            .filter_map(|rc| {
                if !matches!(rc.change.action, Action::Replace { .. }) {
                    return None;
                }
                let noun = stateful::noun(&rc.resource_type)?;
                Some(Finding {
                    rule_id: self.id(),
                    severity: self.default_severity(),
                    address: Some(rc.address.clone()),
                    message: replace::message(Some(noun)),
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

    fn findings(resource_type: &str, actions: serde_json::Value) -> Vec<Finding> {
        let plan: Plan = serde_json::from_value(json!({
            "format_version": "1.2",
            "resource_changes": [{
                "address": format!("{resource_type}.main"),
                "type": resource_type,
                "name": "main",
                "change": {"actions": actions, "replace_paths": [["engine_version"]]}
            }]
        }))
        .unwrap();
        StatefulReplace.check(&plan)
    }

    #[test]
    fn fires_on_both_replace_orders() {
        let found = findings("aws_db_instance", json!(["delete", "create"]));
        assert_eq!(found[0].severity, Severity::Critical);
        assert_eq!(found[0].message, "database must be replaced");
        assert_eq!(
            found[0].reason.as_deref(),
            Some("destroy and then create replacement; forced by changes to engine_version")
        );

        let found = findings("aws_db_instance", json!(["create", "delete"]));
        assert_eq!(found[0].message, "database must be replaced");
        assert!(
            found[0]
                .reason
                .as_deref()
                .unwrap()
                .starts_with("create replacement and then destroy")
        );
    }

    #[test]
    fn ignores_other_types_and_actions() {
        assert!(findings("aws_instance", json!(["delete", "create"])).is_empty());
        for actions in [
            json!(["delete"]),
            json!(["forget", "create"]),
            json!(["update"]),
        ] {
            assert!(
                findings("aws_db_instance", actions.clone()).is_empty(),
                "{actions}"
            );
        }
    }
}
