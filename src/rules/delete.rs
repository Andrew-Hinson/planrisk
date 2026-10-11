//! `delete`: a resource will be destroyed.

use crate::action::Action;
use crate::finding::{Finding, Severity};
use crate::plan::{Plan, ResourceChange};
use crate::rules::{Rule, explain};

pub struct Delete;

impl Rule for Delete {
    fn id(&self) -> &'static str {
        "delete"
    }

    fn description(&self) -> &'static str {
        "A resource will be destroyed"
    }

    fn default_severity(&self) -> Severity {
        Severity::Medium
    }

    fn check(&self, plan: &Plan) -> Vec<Finding> {
        plan.resource_changes
            .iter()
            .filter(|rc| is_plain_delete(&rc.change.action))
            .map(|rc| Finding {
                rule_id: self.id(),
                severity: self.default_severity(),
                address: Some(rc.address.clone()),
                message: message(rc, None),
                reason: reason(rc),
            })
            .collect()
    }
}

// The helpers below are shared with `stateful_delete`, so both rules
// describe a destroy the same way.

/// True when the object is destroyed and not re-created.
///
/// Replaces also destroy, but the `replace` rule reports those. Forget
/// actions remove from state without destroying, so `deletes_object` is
/// false for them.
pub(super) fn is_plain_delete(action: &Action) -> bool {
    action.deletes_object() && !matches!(action, Action::Replace { .. })
}

/// e.g. "will be destroyed", or "database will be destroyed" with a `noun`.
pub(super) fn message(rc: &ResourceChange, noun: Option<&str>) -> String {
    match (&rc.deposed, noun) {
        (Some(key), Some(noun)) => format!(
            "deposed {noun} object {key} will be destroyed (left over from a failed replacement)"
        ),
        (Some(key), None) => {
            format!("deposed object {key} will be destroyed (left over from a failed replacement)")
        }
        (None, Some(noun)) => format!("{noun} will be destroyed"),
        (None, None) => "will be destroyed".to_string(),
    }
}

/// Why Terraform plans the destroy, if known.
pub(super) fn reason(rc: &ResourceChange) -> Option<String> {
    match &rc.change.action {
        Action::Unknown(list) => Some(format!("unrecognised action list {list:?}")),
        _ => explain::action_reason(rc),
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
                "address": "aws_s3_bucket.logs",
                "type": "aws_s3_bucket",
                "name": "logs",
                "action_reason": "delete_because_no_resource_config",
                "change": {"actions": actions}
            }]
        }))
        .unwrap();
        Delete.check(&plan)
    }

    #[test]
    fn fires_on_delete_with_reason() {
        let found = findings(json!(["delete"]));
        assert_eq!(
            found,
            [Finding {
                rule_id: "delete",
                severity: Severity::Medium,
                address: Some("aws_s3_bucket.logs".to_string()),
                message: "will be destroyed".to_string(),
                reason: Some("aws_s3_bucket.logs is not in configuration".to_string()),
            }]
        );
    }

    #[test]
    fn fires_on_unknown_list_containing_delete() {
        let found = findings(json!(["delete", "teleport"]));
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].reason.as_deref(),
            Some(r#"unrecognised action list ["delete", "teleport"]"#)
        );
    }

    #[test]
    fn does_not_fire_on_replaces_forgets_or_other_actions() {
        for actions in [
            json!(["delete", "create"]),
            json!(["create", "delete"]),
            json!(["forget"]),
            json!(["forget", "create"]),
            json!(["create", "forget"]),
            json!(["create"]),
            json!(["update"]),
            json!(["no-op"]),
            json!(["read"]),
        ] {
            assert!(findings(actions.clone()).is_empty(), "{actions}");
        }
    }

    #[test]
    fn deposed_objects_say_so() {
        let plan: Plan = serde_json::from_value(json!({
            "format_version": "1.2",
            "resource_changes": [{
                "address": "aws_instance.web",
                "deposed": "00000001",
                "change": {"actions": ["delete"]}
            }]
        }))
        .unwrap();
        let found = Delete.check(&plan);
        assert_eq!(
            found[0].message,
            "deposed object 00000001 will be destroyed (left over from a failed replacement)"
        );
    }
}
