//! `stateful-delete`: a resource that holds data will be destroyed.

use crate::finding::{Finding, Severity};
use crate::plan::Plan;
use crate::rules::{Rule, delete, stateful};

pub struct StatefulDelete;

impl Rule for StatefulDelete {
    fn id(&self) -> &'static str {
        "stateful-delete"
    }

    fn description(&self) -> &'static str {
        "A resource that holds data (database, bucket, disk, ...) will be destroyed"
    }

    fn default_severity(&self) -> Severity {
        Severity::Critical
    }

    fn supersedes(&self) -> Option<&'static str> {
        Some("delete")
    }

    fn check(&self, plan: &Plan) -> Vec<Finding> {
        plan.resource_changes
            .iter()
            .filter(|rc| delete::is_plain_delete(&rc.change.action))
            .filter_map(|rc| {
                let noun = stateful::noun(&rc.resource_type)?;
                Some(Finding {
                    rule_id: self.id(),
                    severity: self.default_severity(),
                    address: Some(rc.address.clone()),
                    message: delete::message(rc, Some(noun)),
                    reason: delete::reason(rc),
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
                "change": {"actions": actions}
            }]
        }))
        .unwrap();
        StatefulDelete.check(&plan)
    }

    #[test]
    fn fires_on_deleting_a_stateful_resource() {
        let found = findings("aws_db_instance", json!(["delete"]));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Critical);
        assert_eq!(found[0].message, "database will be destroyed");
    }

    #[test]
    fn ignores_other_types_and_actions() {
        assert!(findings("aws_instance", json!(["delete"])).is_empty());
        for actions in [
            json!(["delete", "create"]),
            json!(["forget"]),
            json!(["update"]),
        ] {
            assert!(
                findings("aws_db_instance", actions.clone()).is_empty(),
                "{actions}"
            );
        }
    }
}
