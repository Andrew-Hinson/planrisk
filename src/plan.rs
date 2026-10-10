//! Types for the JSON that `terraform show -json plan.out` prints.
//!
//! Format reference: <https://developer.hashicorp.com/terraform/internals/json-format>
//! The docs lag behind Terraform's source, which is the more complete reference:
//! <https://github.com/hashicorp/terraform/blob/main/internal/command/jsonplan/plan.go>
//!
//! Two deliberate choices (see PLAN.md §6):
//!
//! - Only the outer structure is typed. The resource attributes (`before`,
//!   `after`, and their unknown/sensitive masks) are shaped differently by every
//!   provider and resource type, so they stay as `serde_json::Value`.
//! - Unknown fields are ignored, never rejected. The format docs tell consumers
//!   to "ignore any object properties with unrecognized names to remain
//!   forward-compatible with future minor versions". That is serde's default
//!   behaviour, so these types must **not** use `#[serde(deny_unknown_fields)]`.
//!
//! Terraform omits most fields when they are empty, so nearly every field here
//! is an `Option` or has `#[serde(default)]`.

use serde::Deserialize;
use serde_json::Value;

/// A whole plan: the top-level object of `terraform show -json`.
#[derive(Debug, Deserialize)]
pub struct Plan {
    /// Version of the JSON format, e.g. `"1.2"`. Checked separately (issue #7).
    pub format_version: String,

    /// Version of Terraform that produced the plan, e.g. `"1.16.1"`.
    pub terraform_version: Option<String>,

    /// Changes Terraform will make. Terraform leaves this key out entirely
    /// when there are no changes, so a missing key means an empty list.
    #[serde(default)]
    pub resource_changes: Vec<ResourceChange>,

    /// Changes made outside Terraform that it detected while refreshing.
    #[serde(default)]
    pub resource_drift: Vec<ResourceChange>,

    /// Changes Terraform postponed to a later plan (an experimental feature).
    #[serde(default)]
    pub deferred_changes: Vec<DeferredChange>,
}

/// One planned change to one resource instance.
#[derive(Debug, Deserialize)]
pub struct ResourceChange {
    /// Full address, including any module path and index,
    /// e.g. `module.queue.aws_sqs_queue.this` or `aws_instance.web[0]`.
    pub address: String,

    /// Address before a `moved` block renamed it. Only present for moves.
    pub previous_address: Option<String>,

    /// The module this resource is in, e.g. `module.queue`. Absent in the root module.
    pub module_address: Option<String>,

    /// `"managed"` for resources, `"data"` for data sources.
    #[serde(default)]
    pub mode: String,

    /// Resource type, e.g. `aws_s3_bucket`. Renamed because `type` is a Rust keyword.
    #[serde(rename = "type", default)]
    pub resource_type: String,

    /// Resource name from the config, e.g. `assets` in `aws_s3_bucket.assets`.
    #[serde(default)]
    pub name: String,

    /// Provider that manages the resource, e.g. `registry.terraform.io/hashicorp/aws`.
    #[serde(default)]
    pub provider_name: String,

    /// Set when this change is about a deposed object: an old object left over
    /// from a create-before-destroy replace that didn't finish.
    pub deposed: Option<String>,

    /// Why Terraform chose this action, e.g. `"replace_because_tainted"`.
    /// Kept as a string: new reasons may be added in later Terraform versions.
    pub action_reason: Option<String>,

    pub change: Change,
}

/// What will happen to a resource, and its values before and after.
#[derive(Debug, Deserialize)]
pub struct Change {
    /// Raw action list, e.g. `["create"]` or `["delete", "create"]` for a replace.
    /// Turned into an enum in issue #5.
    pub actions: Vec<String>,

    /// Attribute values before the change. `Null` for creates.
    #[serde(default)]
    pub before: Value,

    /// Attribute values after the change. `Null` for deletes.
    /// Values not known until apply are missing here; see `after_unknown`.
    #[serde(default)]
    pub after: Value,

    /// Same shape as `after`, with `true` at every value not known until apply.
    #[serde(default)]
    pub after_unknown: Value,

    /// Same shape as `before`, with `true` at every sensitive value.
    /// Sensitive values themselves appear in plain text in `before` (PLAN.md §8).
    #[serde(default)]
    pub before_sensitive: Value,

    /// Same shape as `after`, with `true` at every sensitive value.
    #[serde(default)]
    pub after_sensitive: Value,

    /// For replaces: the attribute paths that forced the replacement.
    /// Each path is a list of steps, and each step is a string (attribute name)
    /// or a number (list index), so steps stay as `Value`.
    #[serde(default)]
    pub replace_paths: Vec<Vec<Value>>,

    /// Present when this change imports an existing object. Terraform's source
    /// says the contents "are subject to change", so only its presence is used.
    importing: Option<Value>,
}

impl Change {
    /// True when this change imports an existing object into Terraform.
    pub fn is_importing(&self) -> bool {
        self.importing.is_some()
    }
}

/// A change Terraform postponed, with the reason why.
#[derive(Debug, Deserialize)]
pub struct DeferredChange {
    /// e.g. `"instance_count_unknown"` or `"provider_config_unknown"`.
    pub reason: String,
    pub resource_change: ResourceChange,
}

impl Plan {
    /// Parses plan JSON from a string.
    pub fn from_json(json: &str) -> serde_json::Result<Plan> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // include_str! embeds the file into the test binary at compile time.
    // The path is relative to this source file.
    const EMPTY: &str = include_str!("../tests/fixtures/empty.json");
    const CREATE_ONLY: &str = include_str!("../tests/fixtures/create-only.json");

    #[test]
    fn empty_plan_has_no_changes() {
        let plan = Plan::from_json(EMPTY).unwrap();
        assert_eq!(plan.format_version, "1.2");
        assert!(plan.resource_changes.is_empty());
        assert!(plan.resource_drift.is_empty());
        assert!(plan.deferred_changes.is_empty());
    }

    #[test]
    fn create_only_plan_parses_every_resource() {
        let plan = Plan::from_json(CREATE_ONLY).unwrap();
        let addresses: Vec<&str> = plan
            .resource_changes
            .iter()
            .map(|rc| rc.address.as_str())
            .collect();
        assert_eq!(
            addresses,
            [
                "aws_s3_bucket.assets",
                "aws_security_group.web",
                "module.queue.aws_sqs_queue.this",
            ]
        );

        for rc in &plan.resource_changes {
            assert_eq!(rc.change.actions, ["create"]);
            assert_eq!(rc.mode, "managed");
            assert!(rc.change.before.is_null());
            assert!(!rc.change.is_importing());
        }
    }

    #[test]
    fn module_address_is_set_only_inside_modules() {
        let plan = Plan::from_json(CREATE_ONLY).unwrap();
        let queue = &plan.resource_changes[2];
        assert_eq!(queue.module_address.as_deref(), Some("module.queue"));
        assert_eq!(queue.resource_type, "aws_sqs_queue");
        assert_eq!(queue.name, "this");
        assert_eq!(plan.resource_changes[0].module_address, None);
    }

    #[test]
    fn unknown_fields_are_ignored() {
        // A future Terraform version may add fields at any level.
        let json = r#"{
            "format_version": "1.9",
            "some_new_top_level_field": [1, 2, 3],
            "resource_changes": [{
                "address": "aws_s3_bucket.b",
                "some_new_resource_field": true,
                "change": { "actions": ["update"], "some_new_change_field": {} }
            }]
        }"#;
        let plan = Plan::from_json(json).unwrap();
        assert_eq!(plan.resource_changes[0].change.actions, ["update"]);
    }

    #[test]
    fn replace_paths_importing_and_moves_are_read() {
        let json = r#"{
            "format_version": "1.2",
            "resource_changes": [{
                "address": "aws_db_instance.main",
                "previous_address": "aws_db_instance.old",
                "action_reason": "replace_because_cannot_update",
                "change": {
                    "actions": ["delete", "create"],
                    "replace_paths": [["engine_version"], ["tags", 0]],
                    "importing": { "id": "db-123" }
                }
            }]
        }"#;
        let plan = Plan::from_json(json).unwrap();
        let rc = &plan.resource_changes[0];
        assert_eq!(rc.previous_address.as_deref(), Some("aws_db_instance.old"));
        assert_eq!(
            rc.action_reason.as_deref(),
            Some("replace_because_cannot_update")
        );
        assert_eq!(rc.change.replace_paths.len(), 2);
        assert_eq!(rc.change.replace_paths[1][1], 0);
        assert!(rc.change.is_importing());
    }

    #[test]
    fn deferred_changes_are_read() {
        let json = r#"{
            "format_version": "1.2",
            "deferred_changes": [{
                "reason": "instance_count_unknown",
                "resource_change": { "address": "aws_instance.web", "change": { "actions": ["create"] } }
            }]
        }"#;
        let plan = Plan::from_json(json).unwrap();
        assert_eq!(plan.deferred_changes[0].reason, "instance_count_unknown");
        assert_eq!(
            plan.deferred_changes[0].resource_change.address,
            "aws_instance.web"
        );
    }
}
