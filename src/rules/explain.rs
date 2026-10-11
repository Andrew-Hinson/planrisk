//! Plain-English reasons for why Terraform chose an action.
//!
//! The wording follows what `terraform plan` itself prints for each
//! `action_reason` (see `internal/command/jsonformat/plan.go` in Terraform's
//! source), so planrisk's output matches what reviewers already know.

use serde_json::Value;

use crate::action::Action;
use crate::plan::ResourceChange;

/// Why Terraform plans to replace or delete this resource, if it says.
///
/// Unknown reasons, from newer Terraform versions, are returned as-is.
/// `replace_because_cannot_update` returns `None`: the attribute paths from
/// [`replace_paths`] explain that case better.
pub fn action_reason(rc: &ResourceChange) -> Option<String> {
    let reason = rc.action_reason.as_deref()?;
    let text = match reason {
        "replace_because_cannot_update" => return None,
        "replace_because_tainted" => "resource is tainted".to_string(),
        "replace_by_request" => "replacement was requested".to_string(),
        "replace_by_triggers" => "changes in replace_triggered_by".to_string(),
        "delete_because_no_resource_config" => {
            format!("{}.{} is not in configuration", rc.resource_type, rc.name)
        }
        "delete_because_no_move_target" => format!(
            "{} was moved to {}, which is not in configuration",
            rc.previous_address.as_deref().unwrap_or("it"),
            rc.address
        ),
        "delete_because_no_module" => format!(
            "{} is not in configuration",
            rc.module_address.as_deref().unwrap_or("its module")
        ),
        "delete_because_wrong_repetition" => match &rc.index {
            Value::Number(_) => "resource does not use count".to_string(),
            Value::String(_) => "resource does not use for_each".to_string(),
            _ => "resource uses count or for_each".to_string(),
        },
        "delete_because_count_index" => format!("index [{}] is out of range for count", rc.index),
        "delete_because_each_key" => format!("key [{}] is not in for_each map", rc.index),
        other => other.to_string(),
    };
    Some(text)
}

/// The attributes that forced a replacement, e.g.
/// "forced by changes to engine_version, ebs_block_device[0].size".
pub fn replace_paths(rc: &ResourceChange) -> Option<String> {
    if rc.change.replace_paths.is_empty() {
        return None;
    }
    let paths: Vec<String> = rc
        .change
        .replace_paths
        .iter()
        .map(|path| format_path(path))
        .collect();
    Some(format!("forced by changes to {}", paths.join(", ")))
}

/// Everything that explains a replace, joined, e.g. "destroy and then create
/// replacement; resource is tainted; forced by changes to engine_version".
///
/// In order: which comes first (Terraform's legend wording), the warning
/// Terraform gives when an imported object is replaced, `action_reason`, and
/// the attributes that forced it.
pub fn replace_reason(rc: &ResourceChange) -> Option<String> {
    let order = match rc.change.action {
        Action::Replace { create_first: true } => Some("create replacement and then destroy"),
        Action::Replace {
            create_first: false,
        } => Some("destroy and then create replacement"),
        _ => None,
    };
    let import_warning = rc
        .change
        .is_importing()
        .then_some("this will destroy the imported resource");

    let parts: Vec<String> = [
        order.map(str::to_string),
        import_warning.map(str::to_string),
        action_reason(rc),
        replace_paths(rc),
    ]
    .into_iter()
    .flatten() // drops the `None`s
    .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}

/// Formats one replace path. Each step is an attribute name (string) or a
/// list index (number): `["ebs_block_device", 0, "size"]` becomes
/// `ebs_block_device[0].size`.
fn format_path(path: &[Value]) -> String {
    let mut out = String::new();
    for step in path {
        match step {
            Value::Number(n) => out.push_str(&format!("[{n}]")),
            Value::String(s) => {
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(s);
            }
            // Not expected from Terraform; shown raw rather than dropped.
            other => out.push_str(&format!("[{other}]")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A resource change built from JSON, with the fields a test cares about.
    fn rc(json: Value) -> ResourceChange {
        serde_json::from_value(json).unwrap()
    }

    fn reason_for(reason_code: &str, extra: Value) -> Option<String> {
        let mut value = json!({
            "address": "aws_instance.web",
            "type": "aws_instance",
            "name": "web",
            "action_reason": reason_code,
            "change": {"actions": ["delete"]}
        });
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        action_reason(&rc(value))
    }

    #[test]
    fn replace_reasons_match_terraform_wording() {
        assert_eq!(
            reason_for("replace_because_tainted", json!({})).unwrap(),
            "resource is tainted"
        );
        assert_eq!(
            reason_for("replace_by_request", json!({})).unwrap(),
            "replacement was requested"
        );
        assert_eq!(
            reason_for("replace_by_triggers", json!({})).unwrap(),
            "changes in replace_triggered_by"
        );
        assert_eq!(reason_for("replace_because_cannot_update", json!({})), None);
    }

    #[test]
    fn delete_reasons_match_terraform_wording() {
        assert_eq!(
            reason_for("delete_because_no_resource_config", json!({})).unwrap(),
            "aws_instance.web is not in configuration"
        );
        assert_eq!(
            reason_for(
                "delete_because_no_move_target",
                json!({"previous_address": "aws_instance.old"})
            )
            .unwrap(),
            "aws_instance.old was moved to aws_instance.web, which is not in configuration"
        );
        assert_eq!(
            reason_for(
                "delete_because_no_module",
                json!({"module_address": "module.app"})
            )
            .unwrap(),
            "module.app is not in configuration"
        );
        assert_eq!(
            reason_for("delete_because_count_index", json!({"index": 3})).unwrap(),
            "index [3] is out of range for count"
        );
        assert_eq!(
            reason_for("delete_because_each_key", json!({"index": "blue"})).unwrap(),
            r#"key ["blue"] is not in for_each map"#
        );
    }

    #[test]
    fn wrong_repetition_depends_on_index_type() {
        let reason = |index: Value| {
            reason_for("delete_because_wrong_repetition", json!({"index": index})).unwrap()
        };
        assert_eq!(reason(json!(null)), "resource uses count or for_each");
        assert_eq!(reason(json!(0)), "resource does not use count");
        assert_eq!(reason(json!("a")), "resource does not use for_each");
    }

    #[test]
    fn unknown_reasons_are_shown_raw() {
        assert_eq!(
            reason_for("replace_because_of_the_moon", json!({})).unwrap(),
            "replace_because_of_the_moon"
        );
    }

    #[test]
    fn replace_paths_are_formatted() {
        let change = rc(json!({
            "address": "aws_db_instance.main",
            "action_reason": "replace_because_tainted",
            "change": {
                "actions": ["delete", "create"],
                "replace_paths": [["engine_version"], ["ebs_block_device", 0, "size"]]
            }
        }));
        assert_eq!(
            replace_reason(&change).unwrap(),
            "destroy and then create replacement; resource is tainted; \
             forced by changes to engine_version, ebs_block_device[0].size"
        );
    }

    #[test]
    fn order_is_always_given() {
        let change = rc(json!({"address": "a.b", "change": {"actions": ["create", "delete"]}}));
        assert_eq!(
            replace_reason(&change).as_deref(),
            Some("create replacement and then destroy")
        );
    }

    #[test]
    fn replacing_an_import_warns_like_terraform() {
        let change = rc(json!({
            "address": "random_string.x",
            "change": {"actions": ["delete", "create"], "importing": {"id": "abc"}}
        }));
        assert_eq!(
            replace_reason(&change).as_deref(),
            Some("destroy and then create replacement; this will destroy the imported resource")
        );
    }

    #[test]
    fn not_a_replace_has_no_order() {
        let change = rc(json!({"address": "a.b", "change": {"actions": ["update"]}}));
        assert_eq!(replace_reason(&change), None);
    }
}
