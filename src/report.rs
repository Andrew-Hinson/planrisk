//! The result of checking a plan: everything any output format needs.

use std::cmp::Reverse;
use std::collections::HashSet;

use crate::finding::{Finding, Severity};
use crate::plan::Plan;
use crate::rules::Rule;
use crate::summary::Counts;

/// A checked plan. Every output format (text now; JSON, terminal and
/// Markdown later) renders from this one struct, so they can't disagree.
///
/// The report owns the plan rather than borrowing it. That keeps lifetimes
/// out of the type, and the plan isn't needed for anything else afterwards.
#[derive(Debug)]
pub struct Report {
    pub plan: Plan,
    pub counts: Counts,
    /// Sorted most severe first, then by address, like Terraform's own plan
    /// output. Plan-level findings (no address) come first within a severity.
    pub findings: Vec<Finding>,
}

impl Report {
    /// Runs every rule over the plan and collects the findings.
    pub fn build(plan: Plan, rules: &[Box<dyn Rule>]) -> Report {
        let mut findings: Vec<Finding> = Vec::new();
        // (rule ID, address) pairs whose findings a more specific rule replaced.
        let mut superseded: HashSet<(&str, String)> = HashSet::new();

        for rule in rules {
            let found = rule.check(&plan);
            if let Some(general) = rule.supersedes() {
                for finding in &found {
                    if let Some(address) = &finding.address {
                        superseded.insert((general, address.clone()));
                    }
                }
            }
            findings.extend(found);
        }

        findings.retain(|finding| match &finding.address {
            Some(address) => !superseded.contains(&(finding.rule_id, address.clone())),
            None => true,
        });
        // `Reverse` flips the severity order so the most severe come first.
        // `Option` sorts `None` before `Some`, so plan-level findings lead.
        // The sort is stable: findings for the same resource and severity
        // keep the order the rules produced them.
        findings.sort_by(|a, b| {
            (Reverse(a.severity), &a.address).cmp(&(Reverse(b.severity), &b.address))
        });

        Report {
            counts: Counts::from_changes(&plan.resource_changes),
            plan,
            findings,
        }
    }

    /// The most severe finding's severity, or `None` with no findings.
    /// `--fail-on` compares against this.
    pub fn max_severity(&self) -> Option<Severity> {
        self.findings.iter().map(|f| f.severity).max()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test-only rule that flags every created resource.
    struct FlagCreates;

    impl Rule for FlagCreates {
        fn id(&self) -> &'static str {
            "flag-creates"
        }
        fn description(&self) -> &'static str {
            "test rule"
        }
        fn default_severity(&self) -> Severity {
            Severity::Low
        }
        fn check(&self, plan: &Plan) -> Vec<Finding> {
            plan.resource_changes
                .iter()
                .filter(|rc| rc.change.action == crate::action::Action::Create)
                .map(|rc| Finding {
                    rule_id: self.id(),
                    severity: self.default_severity(),
                    address: Some(rc.address.clone()),
                    message: "will be created".to_string(),
                    reason: None,
                })
                .collect()
        }
    }

    /// A test-only plan-level rule that always reports one finding.
    struct Always(Severity);

    impl Rule for Always {
        fn id(&self) -> &'static str {
            "always"
        }
        fn description(&self) -> &'static str {
            "test rule"
        }
        fn default_severity(&self) -> Severity {
            self.0
        }
        fn check(&self, _plan: &Plan) -> Vec<Finding> {
            vec![Finding {
                rule_id: self.id(),
                severity: self.0,
                address: None,
                message: "whole plan".to_string(),
                reason: Some("because".to_string()),
            }]
        }
    }

    fn create_only() -> Plan {
        Plan::from_json(include_str!("../tests/fixtures/create-only.json")).unwrap()
    }

    #[test]
    fn dummy_rule_findings_end_up_in_the_report() {
        let rules: Vec<Box<dyn Rule>> = vec![Box::new(FlagCreates)];
        let report = Report::build(create_only(), &rules);

        let addresses: Vec<_> = report
            .findings
            .iter()
            .map(|f| f.address.as_deref())
            .collect();
        assert_eq!(
            addresses,
            [
                Some("aws_s3_bucket.assets"),
                Some("aws_security_group.web"),
                Some("module.queue.aws_sqs_queue.this"),
            ]
        );
        assert_eq!(report.counts.create, 3);
        assert_eq!(report.max_severity(), Some(Severity::Low));
    }

    #[test]
    fn findings_are_sorted_most_severe_first() {
        let rules: Vec<Box<dyn Rule>> = vec![
            Box::new(FlagCreates),
            Box::new(Always(Severity::Critical)),
            Box::new(Always(Severity::Medium)),
        ];
        let report = Report::build(create_only(), &rules);

        let severities: Vec<_> = report.findings.iter().map(|f| f.severity).collect();
        assert_eq!(
            severities,
            [
                Severity::Critical,
                Severity::Medium,
                Severity::Low,
                Severity::Low,
                Severity::Low
            ]
        );
        assert_eq!(report.max_severity(), Some(Severity::Critical));
    }

    #[test]
    fn no_rules_means_no_findings() {
        let report = Report::build(create_only(), &[]);
        assert!(report.findings.is_empty());
        assert_eq!(report.max_severity(), None);
    }

    /// A plan replacing a database and deleting a bucket.
    fn stateful_plan() -> Plan {
        Plan::from_json(
            r#"{
                "format_version": "1.2",
                "planned_values": {},
                "resource_changes": [
                    {"address": "aws_db_instance.main", "type": "aws_db_instance", "name": "main",
                     "change": {"actions": ["delete", "create"]}},
                    {"address": "aws_s3_bucket.logs", "type": "aws_s3_bucket", "name": "logs",
                     "change": {"actions": ["delete"]}},
                    {"address": "aws_instance.web", "type": "aws_instance", "name": "web",
                     "change": {"actions": ["delete"]}}
                ]
            }"#,
        )
        .unwrap()
    }

    fn rule_ids(report: &Report) -> Vec<(&str, Option<&str>)> {
        report
            .findings
            .iter()
            .map(|f| (f.rule_id, f.address.as_deref()))
            .collect()
    }

    #[test]
    fn stateful_rules_replace_the_generic_findings() {
        let report = Report::build(stateful_plan(), &crate::rules::all_rules());
        assert_eq!(
            rule_ids(&report),
            [
                ("stateful-replace", Some("aws_db_instance.main")),
                ("stateful-delete", Some("aws_s3_bucket.logs")),
                ("delete", Some("aws_instance.web")),
            ]
        );
    }

    #[test]
    fn generic_findings_remain_without_the_stateful_rules() {
        // As if config had disabled the stateful rules (issue #25).
        let rules: Vec<Box<dyn Rule>> = crate::rules::all_rules()
            .into_iter()
            .filter(|rule| !rule.id().starts_with("stateful-"))
            .collect();
        let report = Report::build(stateful_plan(), &rules);
        assert_eq!(
            rule_ids(&report),
            [
                ("replace", Some("aws_db_instance.main")),
                ("delete", Some("aws_instance.web")),
                ("delete", Some("aws_s3_bucket.logs")),
            ]
        );
    }
}
