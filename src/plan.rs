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

use crate::action::Action;
use crate::redact::redact;

/// A whole plan: the top-level object of `terraform show -json`.
#[derive(Debug, Deserialize)]
pub struct Plan {
    /// Version of the JSON format, e.g. `"1.2"`. Checked before parsing, in
    /// `input::check_format_version`.
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

    /// The configuration the plan was made from. Kept as raw JSON: only
    /// [`Plan::config_sets`] reads it.
    #[serde(default)]
    configuration: Value,
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

    /// Instance key for resources using `count` (a number) or `for_each`
    /// (a string). `Null` for resources using neither.
    #[serde(default)]
    pub index: Value,

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
    /// What will happen, parsed from the JSON `actions` list,
    /// e.g. `["delete", "create"]`. Singular here because it is one `Action`.
    #[serde(rename = "actions")]
    pub action: Action,

    // The values below are private: sensitive values appear in them in plain
    // text. Read them through `before()` / `after()`, which hide secrets.
    /// Attribute values before the change. `Null` for creates.
    #[serde(default)]
    before: Value,

    /// Attribute values after the change. `Null` for deletes.
    /// Values not known until apply are missing here; see `after_unknown`.
    #[serde(default)]
    after: Value,

    /// Same shape as `after`, with `true` at every value not known until apply.
    #[serde(default)]
    pub after_unknown: Value,

    /// Same shape as `before`, with `true` at every sensitive value.
    #[serde(default)]
    before_sensitive: Value,

    /// Same shape as `after`, with `true` at every sensitive value.
    #[serde(default)]
    after_sensitive: Value,

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
    /// Attribute values before the change, with sensitive values replaced by
    /// `(sensitive value)`. Safe to print. `Null` for creates.
    pub fn before(&self) -> Value {
        redact(&self.before, &self.before_sensitive)
    }

    /// Attribute values after the change, with sensitive values replaced by
    /// `(sensitive value)`. Safe to print. `Null` for deletes.
    pub fn after(&self) -> Value {
        redact(&self.after, &self.after_sensitive)
    }

    /// The real values before the change, **including secrets**.
    ///
    /// Only for comparisons inside rules, e.g. "is this CIDR 0.0.0.0/0?".
    /// Never put anything from here into a finding or any other output;
    /// use [`Change::before`] for that.
    pub fn before_unredacted(&self) -> &Value {
        &self.before
    }

    /// The real values after the change, **including secrets**. See
    /// [`Change::before_unredacted`] for the rules on using it.
    pub fn after_unredacted(&self) -> &Value {
        &self.after
    }

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

    /// Whether the configuration sets `attribute` on this resource, or
    /// `None` if the resource's configuration can't be found.
    ///
    /// Tells "the config sets this to something not known until apply" apart
    /// from "the config leaves it out and the provider fills it in", which
    /// look the same in `after_unknown`.
    pub fn config_sets(&self, rc: &ResourceChange, attribute: &str) -> Option<bool> {
        // Walk down `module_calls` along the module path. Configuration is
        // per module, not per instance, so `for_each` and `count` keys are
        // dropped: `module.a["x"].module.b[0]` looks up `a`, then `b`.
        let mut module = self.configuration.get("root_module")?;
        if let Some(path) = &rc.module_address {
            for name in module_names(path) {
                module = module.get("module_calls")?.get(name)?.get("module")?;
            }
        }
        // Resource addresses here are relative to the module, e.g.
        // `aws_iam_role_policy.app`, or `data.aws_iam_policy_document.admin`.
        let prefix = if rc.mode == "data" { "data." } else { "" };
        let address = format!("{prefix}{}.{}", rc.resource_type, rc.name);
        let resource = module
            .get("resources")?
            .as_array()?
            .iter()
            .find(|r| r.get("address").and_then(Value::as_str) == Some(&address))?;
        Some(
            resource
                .get("expressions")
                .is_some_and(|e| e.get(attribute).is_some()),
        )
    }
}

/// The module names in a module address, without instance keys:
/// `module.a["x.y"].module.b[0]` gives `a`, `b`. Keys can contain dots and
/// brackets inside their quotes, so this steps through the address rather
/// than splitting it.
fn module_names(address: &str) -> Vec<&str> {
    let mut names = Vec::new();
    let mut rest = address;
    while let Some(after) = rest.strip_prefix("module.") {
        let end = after.find(['.', '[']).unwrap_or(after.len());
        names.push(&after[..end]);
        rest = &after[end..];
        if rest.starts_with('[') {
            rest = skip_key(rest);
        }
        rest = rest.strip_prefix('.').unwrap_or(rest);
    }
    names
}

/// Skips an instance key like `[0]` or `["a\"]b"]` at the start of `s`.
fn skip_key(s: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            ']' if !in_string => return &s[i + 1..],
            _ => {}
        }
    }
    ""
}

#[cfg(test)]
mod tests {
    use super::*;

    // include_str! embeds the file into the test binary at compile time.
    // The path is relative to this source file.
    const EMPTY: &str = include_str!("../tests/fixtures/empty.json");
    const CREATE_ONLY: &str = include_str!("../tests/fixtures/create-only.json");

    #[test]
    fn module_names_skip_instance_keys() {
        assert_eq!(module_names("module.a"), ["a"]);
        assert_eq!(module_names("module.a[0].module.b"), ["a", "b"]);
        assert_eq!(
            module_names(r#"module.a["x.y].module.z"].module.b["q\"]"]"#),
            ["a", "b"]
        );
    }

    #[test]
    fn config_sets_finds_attributes_in_nested_modules() {
        let plan = Plan::from_json(
            r#"{
                "format_version": "1.2",
                "resource_changes": [
                    {"address": "aws_kms_key.k", "type": "aws_kms_key", "name": "k",
                     "mode": "managed", "change": {"actions": ["create"]}},
                    {"address": "module.m[\"a.b\"].module.n.aws_kms_key.k",
                     "module_address": "module.m[\"a.b\"].module.n",
                     "type": "aws_kms_key", "name": "k", "mode": "managed",
                     "change": {"actions": ["create"]}},
                    {"address": "aws_kms_key.missing", "type": "aws_kms_key", "name": "missing",
                     "mode": "managed", "change": {"actions": ["create"]}}
                ],
                "configuration": {"root_module": {
                    "resources": [{"address": "aws_kms_key.k", "expressions": {"description": {}}}],
                    "module_calls": {"m": {"module": {"module_calls": {"n": {"module": {
                        "resources": [{"address": "aws_kms_key.k", "expressions": {"policy": {}}}]
                    }}}}}}
                }}
            }"#,
        )
        .unwrap();
        let rc = &plan.resource_changes;
        assert_eq!(plan.config_sets(&rc[0], "policy"), Some(false));
        assert_eq!(plan.config_sets(&rc[0], "description"), Some(true));
        assert_eq!(plan.config_sets(&rc[1], "policy"), Some(true));
        assert_eq!(plan.config_sets(&rc[2], "policy"), None);
    }

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
            assert_eq!(rc.change.action, Action::Create);
            assert_eq!(rc.mode, "managed");
            assert!(rc.change.before().is_null());
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
        assert_eq!(plan.resource_changes[0].change.action, Action::Update);
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
        assert_eq!(
            rc.change.action,
            Action::Replace {
                create_first: false
            }
        );
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
