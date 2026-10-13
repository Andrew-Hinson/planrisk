# planrisk — project plan

A Rust CLI that reads a Terraform plan and tells a reviewer what is risky
about it. It ships as a single binary that drops into CI with no required
configuration.

Status: draft, iterate freely. Open questions are collected at the bottom.

---

## 1. Goal

Take the output of `terraform show -json plan.out`, classify each change by
risk, and print a short summary a human can review in seconds. In CI, post
that summary as a PR comment and optionally fail the build above a chosen
severity.

**Positioning.** Tools like Checkov and Trivy check whether the *configuration*
is compliant. planrisk checks how risky *this change* is: what gets
destroyed, what gets replaced and why, what gets opened up, and how much
changes at once. The README should say this up front.

## 2. Done means

Someone adds the GitHub Action to a repo in a few lines, opens a PR with a
Terraform change, and gets a clear risk summary as a comment, following only
the README.

## 3. v1 scope

- Parse Terraform plan JSON, reading from a file or stdin.
- Handle every action kind without crashing (see §6).
- Built-in rules (AWS only for provider-specific rules, see §7):
  - deletes and forced replacements, rated higher for stateful resources
  - security groups and IAM changes that widen access
  - resources becoming publicly reachable
  - large blast radius
- Every finding says *why* (e.g. which attribute forced a replacement).
- **Sensitive values are always redacted** in every output format.
- Output formats: terminal (colored), Markdown, JSON.
- `--fail-on <severity>` with documented exit codes (§9).
- Optional config file to ignore, downgrade or upgrade rules, per rule or per
  resource address glob. The tool works with no config at all.
- GitHub Action that posts a PR comment, or updates its earlier one.
- Release binaries for Linux (x86_64, aarch64, musl) and macOS (x86_64,
  arm64); also installable via `cargo install`.
- Unit tests per rule and snapshot tests against plan fixtures.

## 4. Out of scope for v1

- custom rule language
- cost estimation
- GitLab or Bitbucket integration
- static analysis of `.tf` files
- GCP and Azure specific rules (the generic rules still work for them)
- reading binary plan files (that would need Terraform at runtime)
- Windows binaries
- OpenTofu-specific features. Its plan JSON is largely compatible, so the
  README can say "likely works, not officially tested".

## 5. Constraints

- The owner is learning Rust. Prefer clear, idiomatic code over clever
  abstractions, and explain non-obvious choices in comments or the README.
- Build in small, working increments (§15). Each milestone ends with
  something runnable.
- Adding a rule means adding one module and registering it in one place.

## 6. Reading the plan JSON

Use **typed structs for the outer structure and loose values for attributes**:

Sources: the [JSON output format docs][json-docs] and the comments in
Terraform's [`internal/command/jsonplan/plan.go`][plan-go]. Where they
disagree, the source is newer. The source lists the `forget` actions and
`format_version` `"1.2"`; the docs still say `"1.0"`.

[json-docs]: https://developer.hashicorp.com/terraform/internals/json-format
[plan-go]: https://github.com/hashicorp/terraform/blob/main/internal/command/jsonplan/plan.go

- Strongly typed with serde: `format_version`, `terraform_version`,
  `resource_changes[]` (`address`, `module_address`, `type`, `name`,
  `provider_name`, `previous_address`, `action_reason`, `change.actions`,
  `change.replace_paths`), and `resource_drift[]`.
- `before`, `after`, `after_unknown`, `before_sensitive`, `after_sensitive`
  stay `serde_json::Value`, because each provider and resource type shapes
  these differently.
- `change.importing` is checked only for presence. Terraform's source says
  the structure of its contents is subject to change.
- Do **not** use `deny_unknown_fields` on plan structs. The docs tell
  consumers to "ignore any object properties with unrecognized names to
  remain forward-compatible with future minor versions." Serde ignores
  unknown fields by default.

Map `actions` onto one enum. These are the values listed in Terraform's
source:

| JSON `actions`          | Meaning                                   |
|-------------------------|-------------------------------------------|
| `["no-op"]`             | NoOp (ignored by most rules)              |
| `["create"]`            | Create                                    |
| `["read"]`              | Read (data source)                        |
| `["update"]`            | Update                                    |
| `["delete"]`            | Delete                                    |
| `["delete","create"]`   | Replace (destroy first)                   |
| `["create","delete"]`   | Replace (create first)                    |
| `["forget"]`            | Forget (removed from state, not destroyed; `removed` block) |
| `["create","forget"]`, `["forget","create"]` | Replace where the old object is forgotten, not destroyed |
| anything else           | Unknown: reported as a finding, never a crash |

Terraform's source says replaces are lists "to allow callers to, e.g., just
scan the list for 'delete' to recognize all … situations where the object
will be deleted, allowing for any new deletion combinations that might be
added in future." Follow that: the destroy rules check whether `actions`
*contains* `"delete"`, not one exact list.

**`action_reason`** tells you why Terraform chose the action. Use it in the
finding's reason text. Values from the source:

- replace: `replace_because_cannot_update`, `replace_because_tainted`,
  `replace_by_request`, `replace_by_triggers`
- delete: `delete_because_no_resource_config`,
  `delete_because_wrong_repetition`, `delete_because_count_index`,
  `delete_because_each_key`, `delete_because_no_module`,
  `delete_because_no_move_target`
- read: `read_because_config_unknown`, `read_because_dependency_pending`,
  `read_because_check_nested`

Unknown values are shown raw.

Edge cases, each with a test:

- **Unknown values:** `after_unknown` marks unknown leaves with `true`. Such
  a value is "known after apply", not removed. Rules must not treat it as a
  deletion.
- **Moves and imports:** `previous_address` (from a `moved` block) and the
  presence of `importing` appear in the summary and are not counted as
  destroy plus create.
- **Drift:** `resource_drift` is listed separately as information only in v1.
- **Deferred changes:** `deferred_changes` exists in the source for
  experimental deferred actions. Count and list them as information only.
- **`format_version`:** accept any `1.x`. For any other major version, exit 1
  with a clear error. The docs say to "reject any input which reports an
  unsupported major version."

## 7. Rules

### Rule trait

Every rule takes the whole plan. Per-resource rules loop over
`resource_changes` themselves. Plan-level rules such as blast radius then fit
the same trait, so there is one kind of rule instead of two.

```rust
pub trait Rule {
    /// Stable identifier used in config and output, e.g. "stateful-delete".
    fn id(&self) -> &'static str;
    /// One-line human description for `planrisk rules`.
    fn description(&self) -> &'static str;
    fn check(&self, plan: &Plan) -> Vec<Finding>;
}

pub struct Finding {
    pub rule_id: &'static str,
    pub severity: Severity,
    pub address: Option<String>, // None for plan-level findings
    pub message: String,          // what is happening
    pub reason: Option<String>,   // why, e.g. "engine_version forces replacement"
}

pub enum Severity { Low, Medium, High, Critical }
```

Rules live in `src/rules/<rule_name>.rs` and are registered in one
`all_rules()` function in `src/rules/mod.rs`.

### v1 rule list

There is no industry standard for these severities or thresholds. The
defaults below are our own starting values, chosen to be conservative. Each
one is overridable in config (§10), and the README must say they are
heuristics. Revisit them after running the tool on real plans.

| ID                    | Fires when                                          | Default  |
|-----------------------|-----------------------------------------------------|----------|
| `delete`              | `actions` contains `"delete"` (plain delete)        | Medium   |
| `replace`             | any replace; reason from `action_reason` and `replace_paths` | Medium |
| `stateful-delete`     | stateful resource deleted                           | Critical |
| `stateful-replace`    | stateful resource replaced                          | Critical |
| `sg-open-ingress`     | AWS security group or rule gains `0.0.0.0/0` or `::/0` ingress | High |
| `iam-wildcard`        | AWS IAM policy gains `"Action": "*"` or `"Resource": "*"` on Allow, or a `"Principal": "*"` | High |
| `s3-public-access`    | S3 public access block removed or any flag set to false | High |
| `public-endpoint`     | `publicly_accessible = true` (RDS), internet-facing LB, or `associate_public_ip_address = true` | High |
| `blast-radius`        | changed resources exceed a threshold (default 25) or deletes exceed 5 | Medium |

When a stateful rule fires, its generic counterpart (`delete` or `replace`)
is suppressed for that resource so the reviewer sees one finding, not two.

**Stateful resources** are kept in one data table in
`src/rules/stateful.rs` (resource type and a short reason), not spread across
rules. Starting list: `aws_db_instance`, `aws_rds_cluster`,
`aws_dynamodb_table`, `aws_s3_bucket`, `aws_ebs_volume`, `aws_efs_file_system`,
`aws_elasticache_cluster`, `aws_elasticache_replication_group`,
`aws_kms_key`, `aws_secretsmanager_secret`, `aws_opensearch_domain`,
`aws_redshift_cluster`, `aws_docdb_cluster`, plus the GCP and Azure
databases and buckets, since a type-name check costs nothing.

**IAM policies** arrive as JSON *strings* inside the JSON. Parse them with
`serde_json::from_str`, handle both a single statement and an array of
statements, and handle `Action` and `Resource` as either a string or an
array. v1 only compares wildcards before and after. Full policy diffing is a
later version.

## 8. Redaction (required, with its own tests)

The [`terraform show` docs][show-docs] say: "When using the `-json`
command-line flag, any sensitive values in Terraform state will be displayed
in plain text." The JSON format docs say `before_sensitive` and
`after_sensitive` "should be combined with 'before' and 'after' to prevent
accidental display of sensitive values in user interfaces." Before any rule
message or output includes an attribute value:

[show-docs]: https://developer.hashicorp.com/terraform/cli/commands/show

- walk the sensitivity mask alongside the value and replace marked values
  with `(sensitive)`
- treat a whole attribute as sensitive if the mask is `true` at that level
- if the mask cannot be matched to the value, redact rather than print

Test with a fixture that has a sensitive variable flowing into a resource
attribute, and assert the secret string appears in no output format.

## 9. CLI and exit codes

```
planrisk [PLAN_JSON | -]              # default reads a file, "-" for stdin
  --format terminal|markdown|json     # default: terminal when stdout is a tty, else markdown
  --fail-on low|medium|high|critical  # default: none, so the tool never fails the build
  --config PATH                       # default: ./.planrisk.toml if it exists
  --no-color

planrisk rules                        # list rule IDs, descriptions, default severities
```

| Exit code | Meaning                                              |
|-----------|------------------------------------------------------|
| 0         | ran fine; no finding at or above `--fail-on`         |
| 1         | tool error: bad input, unreadable file, bad config   |
| 2         | ran fine; at least one finding at or above `--fail-on` |

## 10. Config file (`.planrisk.toml`)

TOML because it is the Rust ecosystem's default and is easy to read.

```toml
[thresholds]
blast_radius_changes = 40
blast_radius_deletes = 10

[rules.delete]
severity = "low"            # downgrade a rule everywhere

[rules.iam-wildcard]
enabled = false             # ignore a rule entirely

[[overrides]]
address = "module.sandbox.*" # glob on resource address
rule = "*"
severity = "low"

[[overrides]]
address = "aws_s3_bucket.logs"
rule = "stateful-delete"
enabled = false
```

Unknown keys and unknown rule IDs are errors (exit 1), so a typo fails
loudly instead of being silently ignored. Use `#[serde(deny_unknown_fields)]`.

Settings apply to a finding in this order, and the last one to set a value
wins: the rule's own severity, then `[rules.<id>]`, then each matching
`[[overrides]]` entry in file order. Overrides match only findings about a
resource; plan-level findings such as blast radius use `[rules.<id>]`.
Config applies before a stateful rule supersedes its generic counterpart, so
turning off `stateful-delete` for a resource shows its plain `delete`
finding instead.

In `address` globs, `*` matches any characters (dots included) and `?` one
character. Everything else is literal, so `aws_instance.web[0]` and
`module.app["eu"].*` work as written.

## 11. Output

All three formats are built from the same `Report` struct (findings, counts
per action, plan metadata), so they cannot disagree.

- **JSON:** a stable format with a `format_version` field that follows
  Terraform's rules (minor versions add fields, major versions break). Built first,
  because it is the easiest to snapshot test.
- **Terminal:** counts line, then findings grouped by severity, highest
  first, colored by severity. Respects `NO_COLOR` and `--no-color`.
- **Markdown:** a header with the highest severity and the counts, a findings
  table, and the full resource list in a `<details>` block. It starts with a
  hidden marker, `<!-- planrisk -->`, so the Action can find and update
  its own comment. GitHub rejects longer comment bodies with "Body is too
  long (maximum is 65536 characters)". That limit is seen in practice but not
  officially documented, and it is unclear whether it counts characters or
  bytes. So cap output at 60,000 **bytes** and cut on a UTF-8 character
  boundary. If cut, end with a note pointing to the CI log.

## 12. GitHub Action

A composite action in the same repo (`action.yml`):

1. Download the release binary for the runner's OS and arch (pinned to the
   action's version tag); no Docker.
2. Run `planrisk --format markdown` and `--format json` on the given plan
   JSON path.
3. Find an existing comment containing the marker and update it, or create
   one, using `gh api`. The `gh` CLI is preinstalled on GitHub-hosted runners.
4. Exit with planrisk's exit code if `fail-on` is set.

Inputs: `plan-json` (required), `fail-on`, `config`, `comment` (default
true), `github-token` (default `${{ github.token }}`).

Outputs: `report-json` (path to the JSON report) and `max-severity`. The
Action does not upload artifacts itself. Users who want that add an
`actions/upload-artifact` step using `report-json`, and the README shows how.
This keeps the Action to one job and leaves retention settings to the user.

The README must show the `permissions: pull-requests: write` block and say
that PRs from forks get a read-only token, so the comment step is skipped
with a warning there rather than failing.

Minimal usage to document:

```yaml
- run: terraform plan -out plan.out && terraform show -json plan.out > plan.json
- uses: <owner>/planrisk@v1
  with:
    plan-json: plan.json
    fail-on: critical
```

## 13. Tests and fixtures

- **Unit tests per rule** in each rule module, using small hand-written JSON
  snippets for one resource change each.
- **Snapshot tests** with the `insta` crate: each fixture in
  `tests/fixtures/*.json` is run through all three output formats.
- **Fixture generation:** `tests/fixtures/README.md` records how each fixture
  was made. Strategy:
  - Creates: real `terraform plan` with the AWS provider, fake credentials
    and `skip_credentials_validation` / `skip_requesting_account_id`.
  - Updates, deletes, replaces: a throwaway AWS account once, or hand edits
    of a real plan. Commit the JSON, sanitized.
  - Include at least: empty plan, create-only, stateful replace, sensitive
    value, module nesting, import or move, a large plan for blast radius and
    truncation, and an unknown action or format version.
- CI runs `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`.

## 14. Crates

| Crate                    | Why                                         |
|--------------------------|---------------------------------------------|
| `clap` (derive)          | argument parsing                            |
| `serde`, `serde_json`    | plan and report (de)serialization           |
| `toml`                   | config file                                 |
| `anyhow`                 | error handling in the binary, with context  |
| `globset`                | address globs in config                     |
| `anstream` / `owo-colors`| terminal color that respects `NO_COLOR`     |
| `insta` (dev)            | snapshot tests                              |

Releases: [`dist`](https://github.com/axodotdev/cargo-dist) (formerly
`cargo-dist`) builds GitHub Release binaries and installers. It is
configured in `dist-workspace.toml`, and its config format is being migrated,
so follow the current docs rather than older blog posts.

## 15. Milestones

Each milestone ends with something runnable and tested.

**M0: Skeleton**
- `cargo new`, CI workflow (fmt, clippy, test), fixtures folder with 2–3
  real plans.
- Done: CI is green on a hello-world binary.

**M1: Parse and summarize**
- Plan types (§6), action enum, file and stdin input, plain text summary:
  counts per action plus a list of addresses with their action.
- Done: runs on every fixture without panicking; bad JSON gives exit 1 and a
  readable error.

**M2: Rule engine, first rules, JSON output**
- `Rule` trait, `Finding`, `Severity`, `Report`, `all_rules()`.
- Rules: `delete`, `replace` (with reasons), `stateful-*`, `blast-radius`.
- JSON output, `--fail-on`, exit codes, `planrisk rules`.
- insta snapshot tests.
- Done: `--fail-on critical` exits 2 on the stateful-replace fixture.

**M3: Redaction and AWS access rules**
- Redaction module and tests first.
- Then `sg-open-ingress`, `iam-wildcard`, `s3-public-access`,
  `public-endpoint`.
- Done: the sensitive-value test passes for every output format; each rule
  has firing and non-firing unit tests.

**M4: Terminal and Markdown output**
- Colored terminal output, Markdown with marker, `<details>` and truncation.
- Done: snapshots for all formats; the large fixture truncates cleanly.

**M5: Config file**
- TOML loading, per-rule and per-address overrides, thresholds, strict
  validation.
- Done: tests for each override kind and for typo errors.

**M6: Packaging**
- `dist` setup, tagged release producing Linux and macOS binaries,
  `cargo install` verified.
- Done: a `v0.1.0` release exists with downloadable binaries.

**M7: GitHub Action and README**
- `action.yml`, comment create or update, fork handling.
- Example workflow in a separate test repo.
- README: install, usage, rule list, config, Action, exit codes,
  positioning.
- Done: the "Done means" test in §2, performed by following only the README.

## 16. Decisions

| Question | Decision | Basis |
|----------|----------|-------|
| `--fail-on` default | None: report only, exit 0 | Matches Trivy: "By default, Trivy exits with code 0 even when security issues are detected." Lets teams adopt the tool before letting it block merges. |
| Terraform versions | Any Terraform 1.x. Accept `format_version` 1.x, reject other majors. | Terraform's [v1.x compatibility promises](https://developer.hashicorp.com/terraform/language/v1-compatibility-promises) cover `show -json`: "we will not remove or make breaking changes" to JSON formats. |
| Unknown plan fields | Ignore | The JSON format docs require it for forward compatibility (§6). |
| Report artifact | Action outputs `report-json`; users add `actions/upload-artifact` | Keeps the Action to one job (§12). |
| Comment size cap | 60,000 bytes | Margin under GitHub's observed 65,536 limit (§11). |
| Delete severity, blast radius thresholds | Keep defaults in §7, documented as heuristics | No standard exists; configurable (§10). |
| Name | `planrisk` | Free on crates.io, no GitHub repos with the name (checked 2026-10-10). |
