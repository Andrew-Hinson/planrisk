//! `s3-public-access`: S3 Block Public Access protections are turned off.
//!
//! Fires when a public access block, for one bucket or the whole account,
//! has a protection changed from `true` to `false`, or is destroyed while
//! any protection was on. A block whose protections were all off already
//! protected nothing, so destroying it doesn't fire.
//!
//! Resource types and attribute names were checked against the AWS provider
//! 6.68 schema.

use serde_json::Value;

use crate::finding::{Finding, Severity};
use crate::plan::{Plan, ResourceChange};
use crate::rules::{Rule, delete, explain};

const TYPES: [&str; 2] = [
    "aws_s3_account_public_access_block",
    "aws_s3_bucket_public_access_block",
];

/// The four protections, as named in Terraform (and in the S3 API).
const PROTECTIONS: [&str; 4] = [
    "block_public_acls",
    "block_public_policy",
    "ignore_public_acls",
    "restrict_public_buckets",
];

pub struct S3PublicAccess;

impl Rule for S3PublicAccess {
    fn id(&self) -> &'static str {
        "s3-public-access"
    }

    fn description(&self) -> &'static str {
        "S3 Block Public Access protections are turned off or removed"
    }

    fn default_severity(&self) -> Severity {
        Severity::High
    }

    /// A destroyed block gets this finding instead of the generic `delete`.
    fn supersedes(&self) -> Option<&'static str> {
        Some("delete")
    }

    fn check(&self, plan: &Plan) -> Vec<Finding> {
        plan.resource_changes
            .iter()
            .filter(|rc| TYPES.contains(&rc.resource_type.as_str()))
            .filter_map(|rc| self.check_one(rc))
            .collect()
    }
}

impl S3PublicAccess {
    fn check_one(&self, rc: &ResourceChange) -> Option<Finding> {
        let before = rc.change.before_unredacted();

        let (message, reason) = if delete::is_plain_delete(&rc.change.action) {
            let lost = protections_on(before);
            if lost.is_empty() {
                return None;
            }
            let mut reason = format!("turns off {}", lost.join(", "));
            if let Some(why) = explain::action_reason(rc) {
                reason = format!("{reason}; {why}");
            }
            (
                "S3 Block Public Access settings will be destroyed".to_string(),
                Some(reason),
            )
        } else {
            let after = rc.change.after_unredacted();
            // On before, and explicitly off after. A value only known after
            // apply is missing from `after`, so it doesn't count as off.
            let turned_off: Vec<&str> = protections_on(before)
                .into_iter()
                .filter(|name| after.get(name) == Some(&Value::Bool(false)))
                .collect();
            if turned_off.is_empty() {
                return None;
            }
            (
                format!(
                    "turns off S3 Block Public Access: {}",
                    turned_off.join(", ")
                ),
                None,
            )
        };

        Some(Finding {
            rule_id: self.id(),
            severity: self.default_severity(),
            address: Some(rc.address.clone()),
            message,
            reason,
        })
    }
}

/// The protections set to `true` in these attribute values.
fn protections_on(values: &Value) -> Vec<&'static str> {
    PROTECTIONS
        .into_iter()
        .filter(|name| values.get(name) == Some(&Value::Bool(true)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn check(resource_type: &str, actions: Value, before: Value, after: Value) -> Vec<Finding> {
        let plan: Plan = serde_json::from_value(json!({
            "format_version": "1.2",
            "resource_changes": [{
                "address": format!("{resource_type}.x"),
                "type": resource_type,
                "name": "x",
                "action_reason": if after.is_null() { json!("delete_because_no_resource_config") } else { json!(null) },
                "change": {"actions": actions, "before": before, "after": after}
            }]
        }))
        .unwrap();
        S3PublicAccess.check(&plan)
    }

    fn all(on: bool) -> Value {
        json!({"block_public_acls": on, "block_public_policy": on,
               "ignore_public_acls": on, "restrict_public_buckets": on})
    }

    const BUCKET: &str = "aws_s3_bucket_public_access_block";
    const ACCOUNT: &str = "aws_s3_account_public_access_block";

    #[test]
    fn turning_protections_off_fires() {
        let mut after = all(true);
        after["block_public_policy"] = json!(false);
        after["restrict_public_buckets"] = json!(false);
        let found = check(BUCKET, json!(["update"]), all(true), after);
        assert_eq!(found[0].severity, Severity::High);
        assert_eq!(
            found[0].message,
            "turns off S3 Block Public Access: block_public_policy, restrict_public_buckets"
        );
        assert_eq!(found[0].reason, None);
    }

    #[test]
    fn account_level_block_is_covered() {
        let found = check(ACCOUNT, json!(["update"]), all(true), all(false));
        assert_eq!(
            found[0].message,
            "turns off S3 Block Public Access: block_public_acls, block_public_policy, \
             ignore_public_acls, restrict_public_buckets"
        );
    }

    #[test]
    fn destroying_an_active_block_fires() {
        let found = check(BUCKET, json!(["delete"]), all(true), Value::Null);
        assert_eq!(
            found[0].message,
            "S3 Block Public Access settings will be destroyed"
        );
        assert_eq!(
            found[0].reason.as_deref(),
            Some(
                "turns off block_public_acls, block_public_policy, ignore_public_acls, \
                 restrict_public_buckets; aws_s3_bucket_public_access_block.x is not in configuration"
            )
        );
    }

    #[test]
    fn destroying_an_inactive_block_does_not_fire() {
        assert!(check(BUCKET, json!(["delete"]), all(false), Value::Null).is_empty());
    }

    #[test]
    fn replacing_with_protections_off_fires() {
        let found = check(BUCKET, json!(["delete", "create"]), all(true), all(false));
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn unchanged_new_forgotten_and_unknown_do_not_fire() {
        assert!(check(BUCKET, json!(["no-op"]), all(true), all(true)).is_empty());
        assert!(check(BUCKET, json!(["create"]), Value::Null, all(false)).is_empty());
        assert!(check(BUCKET, json!(["forget"]), all(true), Value::Null).is_empty());
        // Known after apply: left out of `after`, so not counted as off.
        assert!(check(BUCKET, json!(["update"]), all(true), json!({})).is_empty());
        // Turning protections on is fine.
        assert!(check(BUCKET, json!(["update"]), all(false), all(true)).is_empty());
    }

    #[test]
    fn other_types_are_ignored() {
        assert!(check("aws_s3_bucket", json!(["update"]), all(true), all(false)).is_empty());
    }
}
