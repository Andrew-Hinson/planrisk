//! Risk rules. Each rule lives in its own module in this folder.
//!
//! To add a rule (see PLAN.md §7):
//! 1. Create `src/rules/<rule_name>.rs` with a struct that implements [`Rule`].
//! 2. Declare the module below with `mod <rule_name>;`.
//! 3. Add it to [`all_rules`]. That is the only place rules are registered.

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

    /// Returns every risk this rule finds in the plan; empty if none.
    fn check(&self, plan: &Plan) -> Vec<Finding>;
}

/// Every built-in rule, in the order they run.
///
/// `Box<dyn Rule>` lets one list hold different rule types: each `Box` points
/// to some type that implements `Rule`, and calls are dispatched at runtime.
pub fn all_rules() -> Vec<Box<dyn Rule>> {
    // Rules are added from issue #10 onwards.
    Vec::new()
}
