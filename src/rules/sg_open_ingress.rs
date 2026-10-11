//! `sg-open-ingress`: a security group starts allowing ingress from anywhere.
//!
//! Fires when a change *adds* ingress from `0.0.0.0/0` or `::/0`. Ingress
//! that was already open before the change isn't reported again, and egress
//! to anywhere is normal, so it's ignored.
//!
//! Covers all three ways to write ingress in the AWS provider:
//! - inline `ingress` blocks on `aws_security_group`
//! - `aws_security_group_rule` with `type = "ingress"`
//! - `aws_vpc_security_group_ingress_rule` (the newer resource)
//!
//! Attribute names were checked against the AWS provider 6.68 schema.

use serde_json::Value;

use crate::finding::{Finding, Severity};
use crate::plan::{Plan, ResourceChange};
use crate::rules::Rule;

/// CIDR ranges that mean "any address".
const ANYWHERE: [&str; 2] = ["0.0.0.0/0", "::/0"];

pub struct SgOpenIngress;

impl Rule for SgOpenIngress {
    fn id(&self) -> &'static str {
        "sg-open-ingress"
    }

    fn description(&self) -> &'static str {
        "A security group starts allowing ingress from 0.0.0.0/0 or ::/0"
    }

    fn default_severity(&self) -> Severity {
        Severity::High
    }

    fn check(&self, plan: &Plan) -> Vec<Finding> {
        plan.resource_changes
            .iter()
            .filter_map(|rc| {
                let before = open_ingress(rc, Side::Before);
                let added: Vec<String> = open_ingress(rc, Side::After)
                    .into_iter()
                    .filter(|entry| !before.contains(entry))
                    .collect();
                if added.is_empty() {
                    return None;
                }
                Some(Finding {
                    rule_id: self.id(),
                    severity: self.default_severity(),
                    address: Some(rc.address.clone()),
                    message: format!("allows ingress from {}", added.join("; from ")),
                    reason: None,
                })
            })
            .collect()
    }
}

#[derive(Clone, Copy)]
enum Side {
    Before,
    After,
}

/// Every "open to anywhere" ingress entry on one side of the change, e.g.
/// `"0.0.0.0/0 to port 22 (tcp)"`.
///
/// The CIDR check uses the real values. The text, including ports and
/// protocol, is built from the redacted values, so nothing marked sensitive
/// can reach the output. The two have the same shape, so they line up.
fn open_ingress(rc: &ResourceChange, side: Side) -> Vec<String> {
    let (raw, redacted) = match side {
        Side::Before => (rc.change.before_unredacted(), rc.change.before()),
        Side::After => (rc.change.after_unredacted(), rc.change.after()),
    };

    // Each ingress rule as (raw, redacted) pairs of the same JSON object.
    let rules: Vec<(&Value, &Value)> = match rc.resource_type.as_str() {
        "aws_security_group" => match (raw.get("ingress"), redacted.get("ingress")) {
            (Some(Value::Array(raw)), Some(Value::Array(red))) => raw.iter().zip(red).collect(),
            _ => Vec::new(),
        },
        "aws_security_group_rule" if raw.get("type").and_then(Value::as_str) == Some("ingress") => {
            vec![(raw, &redacted)]
        }
        "aws_vpc_security_group_ingress_rule" => vec![(raw, &redacted)],
        _ => Vec::new(),
    };

    let mut entries = Vec::new();
    for (raw, redacted) in rules {
        for cidr in cidrs(raw) {
            if ANYWHERE.contains(&cidr) {
                entries.push(format!("{cidr} to {}", ports(redacted)));
            }
        }
    }
    entries
}

/// Every CIDR in one ingress rule, from whichever attributes its type uses.
fn cidrs(rule: &Value) -> Vec<&str> {
    let mut found = Vec::new();
    // Lists on aws_security_group and aws_security_group_rule.
    for key in ["cidr_blocks", "ipv6_cidr_blocks"] {
        if let Some(Value::Array(list)) = rule.get(key) {
            found.extend(list.iter().filter_map(Value::as_str));
        }
    }
    // Single values on aws_vpc_security_group_ingress_rule.
    for key in ["cidr_ipv4", "cidr_ipv6"] {
        if let Some(cidr) = rule.get(key).and_then(Value::as_str) {
            found.push(cidr);
        }
    }
    found
}

/// Describes the ports and protocol of one ingress rule, e.g.
/// "port 22 (tcp)", "ports 8000-8080 (tcp)" or "all ports and protocols".
fn ports(rule: &Value) -> String {
    // `protocol` on the older resources, `ip_protocol` on the newer one.
    let protocol = rule
        .get("protocol")
        .or_else(|| rule.get("ip_protocol"))
        .map(display)
        .unwrap_or_default();
    // "-1" means every protocol, and with it every port.
    if protocol == "-1" || protocol == "all" {
        return "all ports and protocols".to_string();
    }

    let from = rule.get("from_port").map(display).unwrap_or_default();
    let to = rule.get("to_port").map(display).unwrap_or_default();
    if from == to {
        format!("port {from} ({protocol})")
    } else if from == "0" && to == "65535" {
        format!("all ports ({protocol})")
    } else {
        format!("ports {from}-{to} ({protocol})")
    }
}

/// A JSON value as plain text: strings without quotes, numbers as-is.
fn display(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Findings for one resource change with the given before and after values.
    fn check(resource_type: &str, before: Value, after: Value) -> Vec<Finding> {
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
                "change": {"actions": actions, "before": before, "after": after}
            }]
        }))
        .unwrap();
        SgOpenIngress.check(&plan)
    }

    fn messages(findings: Vec<Finding>) -> Vec<String> {
        findings.into_iter().map(|f| f.message).collect()
    }

    fn sg(ingress: Value) -> Value {
        json!({"ingress": ingress})
    }

    fn rule(from: u16, to: u16, protocol: &str, cidrs: &[&str]) -> Value {
        json!({"from_port": from, "to_port": to, "protocol": protocol, "cidr_blocks": cidrs, "ipv6_cidr_blocks": []})
    }

    #[test]
    fn inline_ingress_opened_to_anywhere_fires() {
        let found = check(
            "aws_security_group",
            sg(json!([rule(443, 443, "tcp", &["10.0.0.0/8"])])),
            sg(json!([
                rule(443, 443, "tcp", &["10.0.0.0/8"]),
                rule(22, 22, "tcp", &["0.0.0.0/0"])
            ])),
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::High);
        assert_eq!(
            found[0].message,
            "allows ingress from 0.0.0.0/0 to port 22 (tcp)"
        );
    }

    #[test]
    fn already_open_ingress_does_not_fire_again() {
        let open = sg(json!([rule(443, 443, "tcp", &["0.0.0.0/0"])]));
        assert!(check("aws_security_group", open.clone(), open).is_empty());
    }

    #[test]
    fn private_ranges_do_not_fire() {
        let after = sg(json!([rule(
            22,
            22,
            "tcp",
            &["10.0.0.0/8", "192.168.0.0/16"]
        )]));
        assert!(check("aws_security_group", Value::Null, after).is_empty());
    }

    #[test]
    fn standalone_rule_ingress_fires_and_egress_does_not() {
        let mut ingress = rule(22, 22, "tcp", &[]);
        ingress["type"] = json!("ingress");
        ingress["ipv6_cidr_blocks"] = json!(["::/0"]);
        assert_eq!(
            messages(check("aws_security_group_rule", Value::Null, ingress)),
            ["allows ingress from ::/0 to port 22 (tcp)"]
        );

        let mut egress = rule(0, 0, "-1", &["0.0.0.0/0"]);
        egress["type"] = json!("egress");
        assert!(check("aws_security_group_rule", Value::Null, egress).is_empty());
    }

    #[test]
    fn vpc_ingress_rule_fires() {
        let everything = json!({"ip_protocol": "-1", "cidr_ipv4": "0.0.0.0/0",
                                "from_port": null, "to_port": null});
        assert_eq!(
            messages(check(
                "aws_vpc_security_group_ingress_rule",
                Value::Null,
                everything
            )),
            ["allows ingress from 0.0.0.0/0 to all ports and protocols"]
        );

        let private = json!({"ip_protocol": "tcp", "cidr_ipv4": "10.0.0.0/16",
                             "from_port": 5432, "to_port": 5432});
        assert!(check("aws_vpc_security_group_ingress_rule", Value::Null, private).is_empty());
    }

    #[test]
    fn port_ranges_are_described() {
        assert_eq!(
            ports(&rule(8000, 8080, "tcp", &[])),
            "ports 8000-8080 (tcp)"
        );
        assert_eq!(ports(&rule(0, 65535, "udp", &[])), "all ports (udp)");
        assert_eq!(ports(&rule(0, 0, "-1", &[])), "all ports and protocols");
    }

    #[test]
    fn several_open_entries_give_one_finding() {
        let after = sg(json!([
            {"from_port": 22, "to_port": 22, "protocol": "tcp",
             "cidr_blocks": ["0.0.0.0/0"], "ipv6_cidr_blocks": ["::/0"]}
        ]));
        assert_eq!(
            messages(check("aws_security_group", Value::Null, after)),
            ["allows ingress from 0.0.0.0/0 to port 22 (tcp); from ::/0 to port 22 (tcp)"]
        );
    }

    #[test]
    fn unknown_cidrs_do_not_fire() {
        // Known after apply: Terraform leaves the value out of `after`.
        let after = json!({"ingress": [{"from_port": 22, "to_port": 22, "protocol": "tcp"}]});
        assert!(check("aws_security_group", Value::Null, after).is_empty());
    }

    #[test]
    fn deletes_do_not_fire() {
        let before = sg(json!([rule(22, 22, "tcp", &["0.0.0.0/0"])]));
        assert!(check("aws_security_group", before, Value::Null).is_empty());
    }
}
