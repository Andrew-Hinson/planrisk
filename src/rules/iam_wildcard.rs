//! `iam-wildcard`: a policy starts granting wildcard access.
//!
//! Fires when an `Allow` statement gains `"Action": "*"`, `"Resource": "*"`
//! or `"Principal": "*"`, compared with the policy before the change.
//! `Action` and `Resource` both `"*"` is reported as full "*:*"
//! administrative privileges, the wording CIS and other scanners use.
//! `Deny` statements only restrict access, so they're ignored.
//!
//! v1 compares wildcards only. It doesn't evaluate `NotAction`,
//! `NotResource`, service wildcards like `"s3:*"`, or what a `Condition`
//! actually allows. When a statement has a `Condition`, the finding says so.
//!
//! Policy documents are JSON *strings* inside the plan, so each one is parsed
//! here. A policy that can't be parsed gives a low-severity finding rather
//! than an error.

use serde_json::Value;

use crate::finding::{Finding, Severity};
use crate::plan::{Plan, ResourceChange};
use crate::redact::PLACEHOLDER;
use crate::rules::Rule;

/// Resource types with a policy document, and the attribute holding it.
/// Checked against the AWS provider 6.68 schema: each is a JSON string.
const POLICY_ATTRIBUTES: &[(&str, &str)] = &[
    ("aws_ecr_repository_policy", "policy"),
    ("aws_iam_group_policy", "policy"),
    ("aws_iam_policy", "policy"),
    ("aws_iam_role", "assume_role_policy"),
    ("aws_iam_role_policy", "policy"),
    ("aws_iam_user_policy", "policy"),
    ("aws_kms_key", "policy"),
    ("aws_s3_bucket_policy", "policy"),
    ("aws_secretsmanager_secret_policy", "policy"),
    ("aws_sns_topic_policy", "policy"),
    ("aws_sqs_queue_policy", "policy"),
];

pub struct IamWildcard;

impl Rule for IamWildcard {
    fn id(&self) -> &'static str {
        "iam-wildcard"
    }

    fn description(&self) -> &'static str {
        r#"A policy starts allowing "Action": "*", "Resource": "*" or "Principal": "*""#
    }

    fn default_severity(&self) -> Severity {
        Severity::High
    }

    fn check(&self, plan: &Plan) -> Vec<Finding> {
        plan.resource_changes
            .iter()
            .filter_map(|rc| self.check_one(plan, rc))
            .collect()
    }
}

impl IamWildcard {
    fn check_one(&self, plan: &Plan, rc: &ResourceChange) -> Option<Finding> {
        let attribute = POLICY_ATTRIBUTES
            .iter()
            .find(|(ty, _)| *ty == rc.resource_type)
            .map(|(_, attribute)| *attribute)?;

        // Only known after apply, usually because the policy refers to a
        // resource this plan creates. Say it wasn't checked rather than
        // staying silent: it may well grant a wildcard. But a policy the
        // config leaves out (a KMS key's, say) is unknown because AWS fills
        // in a default, which is nothing to report. If the config can't be
        // found, report it anyway.
        if rc.change.after_unknown.get(attribute) == Some(&Value::Bool(true))
            && plan.config_sets(rc, attribute) != Some(false)
        {
            return Some(Finding {
                rule_id: self.id(),
                severity: Severity::Low,
                address: Some(rc.address.clone()),
                message: format!("{attribute} is only known after apply, so it wasn't checked"),
                reason: None,
            });
        }

        // Not set, or deleted: nothing to check.
        let after = rc.change.after_unredacted().get(attribute)?.as_str()?;
        let after_grants = match grants(after) {
            Ok(grants) => grants,
            Err(err) => {
                return Some(Finding {
                    rule_id: self.id(),
                    severity: Severity::Low,
                    address: Some(rc.address.clone()),
                    message: format!("{attribute} could not be parsed, so it wasn't checked"),
                    // serde_json's messages give a position, not the content.
                    reason: Some(err.to_string()),
                });
            }
        };

        // A policy that couldn't be parsed before counts as granting nothing.
        let before_grants = rc
            .change
            .before_unredacted()
            .get(attribute)
            .and_then(Value::as_str)
            .and_then(|before| grants(before).ok())
            .unwrap_or_default();

        let added: Vec<&Grant> = after_grants
            .iter()
            .filter(|grant| !before_grants.iter().any(|old| old.kind == grant.kind))
            .collect();
        if added.is_empty() {
            return None;
        }

        // Statement IDs come from the policy text. If the attribute is marked
        // sensitive, leave them out of the output.
        let sensitive = rc.change.after().get(attribute) == Some(&Value::from(PLACEHOLDER));

        let message = format!(
            "{attribute} grants {}",
            added
                .iter()
                .map(|grant| grant.kind.describe())
                .collect::<Vec<_>>()
                .join("; ")
        );
        let mut notes = Vec::new();
        for grant in &added {
            if let Some(detail) = grant.kind.detail() {
                notes.push(detail.to_string());
            }
            if let (Some(sid), false) = (&grant.sid, sensitive) {
                notes.push(format!("statement {sid}"));
            }
            if grant.has_condition {
                notes.push("has a Condition, which planrisk doesn't evaluate".to_string());
            }
        }
        Some(Finding {
            rule_id: self.id(),
            severity: self.default_severity(),
            address: Some(rc.address.clone()),
            message,
            reason: (!notes.is_empty()).then(|| notes.join("; ")),
        })
    }
}

/// Which wildcard a statement grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// "Action": "*" and "Resource": "*" together.
    Admin,
    Action,
    Resource,
    Principal,
}

impl Kind {
    /// What the policy grants, for the finding's message.
    fn describe(self) -> &'static str {
        match self {
            Kind::Admin => r#"full "*:*" administrative privileges"#,
            Kind::Action => r#""Action": "*""#,
            Kind::Resource => r#""Resource": "*""#,
            Kind::Principal => r#"access to everyone with "Principal": "*""#,
        }
    }

    /// Extra detail for the reason, when the message doesn't already say it.
    fn detail(self) -> Option<&'static str> {
        match self {
            Kind::Admin => Some(r#""Action": "*" on "Resource": "*""#),
            _ => None,
        }
    }
}

/// One wildcard grant found in a policy.
#[derive(Debug, PartialEq)]
struct Grant {
    kind: Kind,
    sid: Option<String>,
    has_condition: bool,
}

/// Every wildcard grant in a policy document's `Allow` statements.
fn grants(policy: &str) -> Result<Vec<Grant>, serde_json::Error> {
    let document: Value = serde_json::from_str(policy)?;

    // `Statement` may be a single object or a list of them.
    let statements: Vec<&Value> = match document.get("Statement") {
        Some(Value::Array(list)) => list.iter().collect(),
        Some(single @ Value::Object(_)) => vec![single],
        _ => Vec::new(),
    };

    let mut found = Vec::new();
    for statement in statements {
        if statement.get("Effect").and_then(Value::as_str) != Some("Allow") {
            continue;
        }
        let action = has_star(statement.get("Action"));
        let resource = has_star(statement.get("Resource"));
        let mut kinds = Vec::new();
        match (action, resource) {
            (true, true) => kinds.push(Kind::Admin),
            (true, false) => kinds.push(Kind::Action),
            (false, true) => kinds.push(Kind::Resource),
            (false, false) => {}
        }
        if principal_is_anyone(statement.get("Principal")) {
            kinds.push(Kind::Principal);
        }

        for kind in kinds {
            found.push(Grant {
                kind,
                sid: statement
                    .get("Sid")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                has_condition: statement.get("Condition").is_some(),
            });
        }
    }
    Ok(found)
}

/// True for `"*"` or a list containing `"*"`. (`Action` and `Resource` may
/// be either a string or a list of strings.)
fn has_star(value: Option<&Value>) -> bool {
    match value {
        Some(Value::String(s)) => s == "*",
        Some(Value::Array(list)) => list.iter().any(|v| v.as_str() == Some("*")),
        _ => false,
    }
}

/// True for `"Principal": "*"` or a principal map with `"*"` under any key,
/// e.g. `{"AWS": "*"}` or `{"AWS": ["*"]}`.
fn principal_is_anyone(value: Option<&Value>) -> bool {
    match value {
        Some(Value::String(s)) => s == "*",
        Some(Value::Object(map)) => map.values().any(|v| has_star(Some(v))),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn check(
        resource_type: &str,
        attribute: &str,
        before: Option<Value>,
        after: Value,
    ) -> Vec<Finding> {
        let policy_value = |doc: Value| match doc {
            Value::String(s) => Value::String(s), // already a string (e.g. malformed)
            doc => Value::String(doc.to_string()),
        };
        let before = before.map(|doc| json!({ attribute: policy_value(doc) }));
        let actions = if before.is_some() {
            json!(["update"])
        } else {
            json!(["create"])
        };
        let plan: Plan = serde_json::from_value(json!({
            "format_version": "1.2",
            "resource_changes": [{
                "address": format!("{resource_type}.x"),
                "type": resource_type,
                "change": {
                    "actions": actions,
                    "before": before,
                    "after": { attribute: policy_value(after) }
                }
            }]
        }))
        .unwrap();
        IamWildcard.check(&plan)
    }

    fn statement(statement: Value) -> Value {
        json!({"Version": "2012-10-17", "Statement": [statement]})
    }

    #[test]
    fn admin_action_and_resource_star() {
        let found = check(
            "aws_iam_policy",
            "policy",
            None,
            statement(json!({"Sid": "All", "Effect": "Allow", "Action": "*", "Resource": "*"})),
        );
        assert_eq!(found[0].severity, Severity::High);
        assert_eq!(
            found[0].message,
            r#"policy grants full "*:*" administrative privileges"#
        );
        assert_eq!(
            found[0].reason.as_deref(),
            Some(r#""Action": "*" on "Resource": "*"; statement All"#)
        );
    }

    #[test]
    fn action_or_resource_star_alone() {
        let found = check(
            "aws_iam_role_policy",
            "policy",
            None,
            statement(json!({"Effect": "Allow", "Action": ["logs:PutLogEvents"], "Resource": "*"})),
        );
        assert_eq!(found[0].message, r#"policy grants "Resource": "*""#);

        let found = check(
            "aws_iam_policy",
            "policy",
            None,
            statement(json!({"Effect": "Allow", "Action": ["*"], "Resource": ["arn:aws:s3:::b"]})),
        );
        assert_eq!(found[0].message, r#"policy grants "Action": "*""#);
    }

    #[test]
    fn principal_star_in_every_form() {
        for principal in [
            json!("*"),
            json!({"AWS": "*"}),
            json!({"AWS": ["arn:aws:iam::1:root", "*"]}),
        ] {
            let found = check(
                "aws_s3_bucket_policy",
                "policy",
                None,
                statement(
                    json!({"Effect": "Allow", "Principal": principal, "Action": "s3:GetObject",
                                 "Resource": "arn:aws:s3:::b/*"}),
                ),
            );
            assert_eq!(
                found[0].message, r#"policy grants access to everyone with "Principal": "*""#,
                "{principal}"
            );
        }
    }

    #[test]
    fn conditions_are_noted() {
        let found = check(
            "aws_iam_role",
            "assume_role_policy",
            None,
            statement(
                json!({"Effect": "Allow", "Action": "sts:AssumeRole", "Principal": {"AWS": "*"},
                             "Condition": {"StringEquals": {"sts:ExternalId": "x"}}}),
            ),
        );
        assert_eq!(
            found[0].message,
            r#"assume_role_policy grants access to everyone with "Principal": "*""#
        );
        assert_eq!(
            found[0].reason.as_deref(),
            Some("has a Condition, which planrisk doesn't evaluate")
        );
    }

    #[test]
    fn single_statement_object_is_read() {
        let doc = json!({"Statement": {"Effect": "Allow", "Action": "*", "Resource": "*"}});
        assert_eq!(check("aws_iam_policy", "policy", None, doc).len(), 1);
    }

    #[test]
    fn existing_wildcards_do_not_fire_again() {
        let doc = statement(json!({"Effect": "Allow", "Action": "ec2:Describe*", "Resource": "*"}));
        assert!(check("aws_iam_policy", "policy", Some(doc.clone()), doc).is_empty());
    }

    #[test]
    fn widening_an_existing_policy_fires() {
        let before = statement(
            json!({"Effect": "Allow", "Action": "s3:GetObject", "Resource": "arn:aws:s3:::b/*"}),
        );
        let after = statement(json!({"Effect": "Allow", "Action": "*", "Resource": "*"}));
        assert_eq!(
            check("aws_iam_policy", "policy", Some(before), after).len(),
            1
        );
    }

    #[test]
    fn deny_and_specific_statements_do_not_fire() {
        for doc in [
            statement(json!({"Effect": "Deny", "Action": "*", "Resource": "*"})),
            statement(json!({"Effect": "Allow", "Action": "s3:*", "Resource": "arn:aws:s3:::b"})),
            statement(
                json!({"Effect": "Allow", "Principal": {"Service": "ec2.amazonaws.com"},
                             "Action": "sts:AssumeRole"}),
            ),
        ] {
            assert!(
                check("aws_iam_policy", "policy", None, doc.clone()).is_empty(),
                "{doc}"
            );
        }
    }

    #[test]
    fn malformed_policy_is_a_low_finding() {
        let found = check("aws_iam_policy", "policy", None, json!("{ not json"));
        assert_eq!(found[0].severity, Severity::Low);
        assert_eq!(
            found[0].message,
            "policy could not be parsed, so it wasn't checked"
        );
        assert!(found[0].reason.as_deref().unwrap().contains("line 1"));
    }

    #[test]
    fn policy_known_only_after_apply_is_reported_as_unchecked() {
        // As Terraform plans a policy that refers to a bucket it creates.
        let plan: Plan = serde_json::from_value(json!({
            "format_version": "1.2",
            "resource_changes": [{
                "address": "aws_iam_role_policy.app",
                "type": "aws_iam_role_policy",
                "change": {
                    "actions": ["update"],
                    "before": {"policy": statement(json!({"Effect": "Allow", "Action": "s3:GetObject", "Resource": "*"})).to_string()},
                    "after": {"role": "app"},
                    "after_unknown": {"policy": true}
                }
            }]
        }))
        .unwrap();
        let found = IamWildcard.check(&plan);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Low);
        assert_eq!(
            found[0].message,
            "policy is only known after apply, so it wasn't checked"
        );
    }

    #[test]
    fn other_resource_types_are_ignored() {
        let doc = statement(json!({"Effect": "Allow", "Action": "*", "Resource": "*"}));
        assert!(check("aws_instance", "policy", None, doc).is_empty());
    }

    #[test]
    fn statement_ids_are_hidden_when_the_policy_is_sensitive() {
        let doc = statement(
            json!({"Sid": "SecretSid", "Effect": "Allow", "Action": "*", "Resource": "*"}),
        )
        .to_string();
        let plan: Plan = serde_json::from_value(json!({
            "format_version": "1.2",
            "resource_changes": [{
                "address": "aws_iam_policy.x", "type": "aws_iam_policy",
                "change": {"actions": ["create"], "after": {"policy": doc},
                           "after_sensitive": {"policy": true}}
            }]
        }))
        .unwrap();
        let found = IamWildcard.check(&plan);
        assert_eq!(found.len(), 1);
        // The statement ID is left out; the generic detail stays.
        assert_eq!(
            found[0].reason.as_deref(),
            Some(r#""Action": "*" on "Resource": "*""#)
        );
    }

    #[test]
    fn table_is_sorted() {
        for pair in POLICY_ATTRIBUTES.windows(2) {
            assert!(pair[0].0 < pair[1].0, "{} before {}", pair[1].0, pair[0].0);
        }
    }
}
