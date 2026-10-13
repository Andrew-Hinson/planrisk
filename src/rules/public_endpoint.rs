//! `public-endpoint`: a resource becomes reachable from the internet.
//!
//! Fires when a change makes something public that wasn't before (a new
//! public resource, or an existing one switched to public):
//!
//! | Resource types | Attribute |
//! |---|---|
//! | `aws_db_instance`, `aws_rds_cluster_instance` | `publicly_accessible = true` |
//! | `aws_lb`, `aws_alb` | `internal = false` (internet-facing) |
//! | `aws_instance` | `associate_public_ip_address = true` |
//! | `aws_launch_template` | `associate_public_ip_address = "true"` in `network_interfaces` |
//!
//! Attribute names and types were checked against the AWS provider 6.68
//! schema. Note the launch template's flag is a *string*, not a bool.
//!
//! A new load balancer without `internal` set has `internal` unknown in the
//! plan, and the provider docs say it "Defaults to false", i.e.
//! internet-facing. That case is reported at medium severity, since the
//! final value isn't known yet.

use serde_json::Value;

use crate::action::Action;
use crate::finding::{Finding, Severity};
use crate::plan::{Plan, ResourceChange};
use crate::rules::Rule;

pub struct PublicEndpoint;

impl Rule for PublicEndpoint {
    fn id(&self) -> &'static str {
        "public-endpoint"
    }

    fn description(&self) -> &'static str {
        "A database, load balancer or instance becomes reachable from the internet"
    }

    fn default_severity(&self) -> Severity {
        Severity::High
    }

    fn check(&self, plan: &Plan) -> Vec<Finding> {
        plan.resource_changes
            .iter()
            .filter_map(|rc| self.check_one(rc))
            .collect()
    }
}

impl PublicEndpoint {
    fn check_one(&self, rc: &ResourceChange) -> Option<Finding> {
        let (is_public, message): (fn(&Value) -> bool, &str) = match rc.resource_type.as_str() {
            "aws_db_instance" | "aws_rds_cluster_instance" => (
                |v| v.get("publicly_accessible") == Some(&Value::Bool(true)),
                "will be publicly accessible (publicly_accessible = true)",
            ),
            "aws_lb" | "aws_alb" => (
                |v| is_load_balancer(v) && v.get("internal") == Some(&Value::Bool(false)),
                "load balancer will be internet-facing (internal = false)",
            ),
            "aws_instance" => (
                |v| v.get("associate_public_ip_address") == Some(&Value::Bool(true)),
                "instance will get a public IP address (associate_public_ip_address = true)",
            ),
            "aws_launch_template" => (
                launch_template_assigns_public_ip,
                "instances launched from it will get a public IP address \
                 (associate_public_ip_address = true)",
            ),
            _ => return None,
        };

        let before = rc.change.before_unredacted();
        let after = rc.change.after_unredacted();

        if is_public(after) && !is_public(before) {
            return Some(self.finding(rc, self.default_severity(), message, None));
        }

        // A new load balancer whose `internal` is only known after apply.
        let unknown_internal = rc.change.after_unknown.get("internal") == Some(&Value::Bool(true));
        if matches!(rc.resource_type.as_str(), "aws_lb" | "aws_alb")
            && rc.change.action == Action::Create
            && unknown_internal
            && is_load_balancer(after)
        {
            return Some(self.finding(
                rc,
                Severity::Medium,
                "load balancer may be internet-facing",
                Some("internal is only known after apply; it defaults to false, which is internet-facing"),
            ));
        }
        None
    }

    fn finding(
        &self,
        rc: &ResourceChange,
        severity: Severity,
        message: &str,
        reason: Option<&str>,
    ) -> Finding {
        Finding {
            rule_id: self.id(),
            severity,
            address: Some(rc.address.clone()),
            message: message.to_string(),
            reason: reason.map(str::to_string),
        }
    }
}

/// False for Gateway Load Balancers, which have no internal/internet-facing
/// scheme. `load_balancer_type` defaults to "application".
fn is_load_balancer(values: &Value) -> bool {
    values.get("load_balancer_type").and_then(Value::as_str) != Some("gateway")
}

/// True when any network interface in a launch template gets a public IP.
/// The provider stores this flag as a string ("true"); a bool is accepted too.
fn launch_template_assigns_public_ip(values: &Value) -> bool {
    let Some(Value::Array(interfaces)) = values.get("network_interfaces") else {
        return false;
    };
    interfaces.iter().any(|interface| {
        matches!(
            interface.get("associate_public_ip_address"),
            Some(Value::String(s)) if s == "true"
        ) || interface.get("associate_public_ip_address") == Some(&Value::Bool(true))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn check(
        resource_type: &str,
        before: Value,
        after: Value,
        after_unknown: Value,
    ) -> Vec<Finding> {
        let actions = if before.is_null() {
            json!(["create"])
        } else {
            json!(["update"])
        };
        let plan: Plan = serde_json::from_value(json!({
            "format_version": "1.2",
            "resource_changes": [{
                "address": format!("{resource_type}.x"),
                "type": resource_type,
                "change": {"actions": actions, "before": before, "after": after,
                           "after_unknown": after_unknown}
            }]
        }))
        .unwrap();
        PublicEndpoint.check(&plan)
    }

    fn messages(found: Vec<Finding>) -> Vec<String> {
        found.into_iter().map(|f| f.message).collect()
    }

    #[test]
    fn database_made_public_fires() {
        for ty in ["aws_db_instance", "aws_rds_cluster_instance"] {
            let found = check(
                ty,
                json!({"publicly_accessible": false}),
                json!({"publicly_accessible": true}),
                json!({}),
            );
            assert_eq!(found[0].severity, Severity::High);
            assert_eq!(
                found[0].message,
                "will be publicly accessible (publicly_accessible = true)"
            );
        }
    }

    #[test]
    fn already_public_does_not_fire_again() {
        let public = json!({"publicly_accessible": true});
        assert!(check("aws_db_instance", public.clone(), public, json!({})).is_empty());
    }

    #[test]
    fn internet_facing_load_balancer_fires() {
        for ty in ["aws_lb", "aws_alb"] {
            assert_eq!(
                messages(check(
                    ty,
                    Value::Null,
                    json!({"internal": false}),
                    json!({})
                )),
                ["load balancer will be internet-facing (internal = false)"]
            );
        }
        assert!(check("aws_lb", Value::Null, json!({"internal": true}), json!({})).is_empty());
    }

    #[test]
    fn new_load_balancer_with_unknown_internal_is_medium() {
        let found = check("aws_lb", Value::Null, json!({}), json!({"internal": true}));
        assert_eq!(found[0].severity, Severity::Medium);
        assert_eq!(found[0].message, "load balancer may be internet-facing");
        // Only for creates: an existing load balancer keeps its scheme.
        assert!(
            check(
                "aws_lb",
                json!({"internal": true}),
                json!({}),
                json!({"internal": true})
            )
            .is_empty()
        );
    }

    #[test]
    fn gateway_load_balancers_are_ignored() {
        let gateway = json!({"internal": false, "load_balancer_type": "gateway"});
        assert!(check("aws_lb", Value::Null, gateway, json!({})).is_empty());
        let gateway = json!({"load_balancer_type": "gateway"});
        assert!(check("aws_lb", Value::Null, gateway, json!({"internal": true})).is_empty());
    }

    #[test]
    fn instance_public_ip_fires() {
        assert_eq!(
            messages(check(
                "aws_instance",
                json!({"associate_public_ip_address": false}),
                json!({"associate_public_ip_address": true}),
                json!({})
            )),
            ["instance will get a public IP address (associate_public_ip_address = true)"]
        );
    }

    #[test]
    fn launch_template_public_ip_is_a_string() {
        let template =
            |flag: Value| json!({"network_interfaces": [{"associate_public_ip_address": flag}]});
        assert_eq!(
            check(
                "aws_launch_template",
                Value::Null,
                template(json!("true")),
                json!({})
            )
            .len(),
            1
        );
        assert_eq!(
            check(
                "aws_launch_template",
                Value::Null,
                template(json!(true)),
                json!({})
            )
            .len(),
            1
        );
        assert!(
            check(
                "aws_launch_template",
                Value::Null,
                template(json!("false")),
                json!({})
            )
            .is_empty()
        );
        assert!(
            check(
                "aws_launch_template",
                Value::Null,
                template(json!(null)),
                json!({})
            )
            .is_empty()
        );
        assert!(
            check(
                "aws_launch_template",
                Value::Null,
                json!({"network_interfaces": []}),
                json!({})
            )
            .is_empty()
        );
    }

    #[test]
    fn going_private_and_other_types_do_not_fire() {
        assert!(
            check(
                "aws_db_instance",
                json!({"publicly_accessible": true}),
                json!({"publicly_accessible": false}),
                json!({})
            )
            .is_empty()
        );
        assert!(
            check(
                "aws_s3_bucket",
                Value::Null,
                json!({"publicly_accessible": true}),
                json!({})
            )
            .is_empty()
        );
    }
}
