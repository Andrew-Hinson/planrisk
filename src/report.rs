//! The result of checking a plan: everything any output format needs.

use std::cmp::Reverse;

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
    /// Sorted most severe first; ties keep the order rules produced them.
    pub findings: Vec<Finding>,
}

impl Report {
    /// Runs every rule over the plan and collects the findings.
    pub fn build(plan: Plan, rules: &[Box<dyn Rule>]) -> Report {
        let mut findings: Vec<Finding> = rules.iter().flat_map(|rule| rule.check(&plan)).collect();
        // `Reverse` flips the order so the most severe come first. The sort
        // is stable, so equal severities keep the order rules produced them.
        findings.sort_by_key(|finding| Reverse(finding.severity));

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
}
