# planrisk

Reads a Terraform plan and tells a reviewer what is risky about it: what
gets destroyed, what gets replaced and why, what gets opened up, and how
much changes at once. Built to drop into CI as a single binary.

**Status:** planning. See [PLAN.md](PLAN.md) for the design and the
[issues](../../issues) and [milestones](../../milestones) for progress.
