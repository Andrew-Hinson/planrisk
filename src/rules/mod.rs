//! Risk rules. Each rule lives in its own module in this folder.
//!
//! To add a rule (see PLAN.md §7):
//! 1. Create `src/rules/<rule_name>.rs` with a struct that implements [`Rule`].
//! 2. Declare the module below with `mod <rule_name>;`.
//! 3. Add it to [`all_rules`]. That is the only place rules are registered.

mod blast_radius;
mod delete;
pub mod explain;
mod iam_wildcard;
mod public_endpoint;
mod replace;
mod s3_public_access;
mod sg_open_ingress;
pub mod stateful;
mod stateful_delete;
mod stateful_replace;

use crate::finding::{Finding, Severity};
use crate::plan::Plan;

/// A check that looks at a plan and reports risks.
///
/// Every rule receives the whole plan, not one resource at a time. Most rules
/// just loop over `plan.resource_changes`, but some need the whole picture:
/// blast radius counts every change at once. Passing the whole plan lets both
/// kinds share one trait, instead of having two kinds of rule.
pub trait Rule {
    /// Stable identifier used in output and config, e.g. `"stateful-delete"`.
    /// Changing it breaks users' config files, so treat it as permanent.
    fn id(&self) -> &'static str;

    /// One line describing what the rule flags, for `planrisk rules`.
    fn description(&self) -> &'static str;

    /// Severity of this rule's findings unless config overrides it.
    fn default_severity(&self) -> Severity;

    /// The ID of a more general rule this one replaces, if any.
    ///
    /// When this rule reports a resource, the general rule's finding for the
    /// same resource is dropped, so a reviewer sees one finding, not two.
    /// For example, `stateful-delete` supersedes `delete`. Done in the report
    /// rather than inside `delete`, so if config disables `stateful-delete`,
    /// the plain `delete` finding still appears.
    fn supersedes(&self) -> Option<&'static str> {
        None
    }

    /// Returns every risk this rule finds in the plan; empty if none.
    fn check(&self, plan: &Plan) -> Vec<Finding>;
}

/// Every built-in rule, in the order they run.
///
/// `Box<dyn Rule>` lets one list hold different rule types: each `Box` points
/// to some type that implements `Rule`, and calls are dispatched at runtime.
pub fn all_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(stateful_delete::StatefulDelete),
        Box::new(stateful_replace::StatefulReplace),
        Box::new(delete::Delete),
        Box::new(replace::Replace),
        Box::new(blast_radius::BlastRadius::default()),
        Box::new(sg_open_ingress::SgOpenIngress),
        Box::new(iam_wildcard::IamWildcard),
        Box::new(s3_public_access::S3PublicAccess),
        Box::new(public_endpoint::PublicEndpoint),
    ]
}

/// The table printed by `planrisk rules`: one line per rule, sorted by ID.
///
/// Built from the rules themselves, so it can't fall out of date when a rule
/// is added.
pub fn render_rule_list(rules: &[Box<dyn Rule>]) -> String {
    let mut rules: Vec<&dyn Rule> = rules.iter().map(|rule| rule.as_ref()).collect();
    rules.sort_by_key(|rule| rule.id());

    // Pad the ID column to the longest ID, so the columns line up.
    let id_width = rules
        .iter()
        .map(|rule| rule.id().len())
        .max()
        .unwrap_or(0)
        .max("ID".len());

    let mut out = format!("{:<id_width$}  {:<8}  DESCRIPTION\n", "ID", "SEVERITY");
    for rule in rules {
        out.push_str(&format!(
            "{:<id_width$}  {:<8}  {}\n",
            rule.id(),
            rule.default_severity(),
            rule.description()
        ));
    }
    out
}
