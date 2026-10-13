# planrisk

Reads a Terraform plan and tells a reviewer what is risky about it: what
gets destroyed, what gets replaced and why, what gets opened up to the
internet, and how much changes at once. It is a single binary with no
required configuration, built to run in CI.

**How it differs from Checkov, Trivy and similar scanners:** those check
whether your *configuration* follows good practice. planrisk checks how
risky *this change* is. A database that has always been fine to Checkov can
still be about to be destroyed and re-created because someone changed its
availability zone; planrisk is the tool that says so, and says why.

```
$ terraform plan -out plan.out
$ terraform show -json plan.out > plan.json
$ planrisk plan.json
Terraform 1.16.1, plan format 1.2
0 to create, 2 to update, 2 to replace, 3 to delete, 1 to forget, 1 to move

Findings:
  critical  aws_db_instance.main: database must be replaced (destroy and then create replacement; forced by changes to availability_zone) [stateful-replace]
  critical  aws_s3_bucket.logs: storage bucket will be destroyed (aws_s3_bucket.logs is not in configuration) [stateful-delete]
  medium    aws_security_group.web: must be replaced (create replacement and then destroy; forced by changes to description) [replace]
  medium    aws_sqs_queue.unused: will be destroyed (aws_sqs_queue.unused is not in configuration) [delete]
  medium    aws_sqs_queue.workers[2]: will be destroyed (index [2] is out of range for count) [delete]

Changes:
  replace  aws_db_instance.main
  delete   aws_s3_bucket.logs
  ...
```

Sensitive values are never printed: anything Terraform marks sensitive is
shown as `(sensitive value)` in every output format.

## Install

**Prebuilt binary** (Linux x86_64 and aarch64, macOS Intel and Apple
Silicon):

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/Andrew-Hinson/planrisk/releases/latest/download/planrisk-installer.sh | sh
```

Or download an archive from the
[releases page](https://github.com/Andrew-Hinson/planrisk/releases). Each
archive has a `.sha256` file next to it, and `sha256.sum` lists them all.
The Linux binaries are statically linked and run on any distribution.

**From crates.io** (needs Rust 1.88 or later):

```sh
cargo install planrisk
```

## Usage

planrisk reads the JSON form of a saved plan, from a file or from stdin
with `-`:

```sh
terraform plan -out plan.out
terraform show -json plan.out > plan.json
planrisk plan.json

# or without the intermediate file
terraform show -json plan.out | planrisk -
```

It needs the output of `terraform show -json` on a saved plan. Anything
else, such as plain `terraform plan` output, a binary plan file or a state
file, exits with code 1 and an error that shows the commands above.

| Option | What it does |
|---|---|
| `--format terminal\|markdown\|json` | Output format. Default: `terminal` when output is a terminal, `markdown` otherwise (for example in CI). |
| `--fail-on low\|medium\|high\|critical` | Exit with code 2 if any finding is this severe or worse. Default: never fail. |
| `--config PATH` | Config file. Default: `.planrisk.toml` in the current directory, if it exists. |
| `--no-color` | No colors in terminal output. The `NO_COLOR` environment variable does the same. |

`planrisk rules` lists every rule with its default severity.

### Exit codes

| Code | Meaning |
|---|---|
| 0 | Ran fine; no finding at or above `--fail-on`, or `--fail-on` not set |
| 1 | Error: bad input, unreadable file, bad config or bad arguments |
| 2 | Ran fine; at least one finding at or above `--fail-on` |

By default planrisk only reports and always exits 0 on a valid plan, so you
can add it to CI and look at its findings before letting it block merges.
Then add, for example, `--fail-on critical`.

### Output formats

- **terminal:** the summary above, colored by severity.
- **markdown:** for a pull request comment. It starts with a hidden
  `<!-- planrisk -->` marker, has a findings table, and folds the full list
  of changes into a `<details>` block. It is capped at 60,000 bytes to stay
  under GitHub's comment size limit, with a note when it was cut.
- **json:** for scripts. Every field is always present (`null` when it
  doesn't apply), and `format_version` follows Terraform's rules: a new minor
  version only adds fields, a new major version is a breaking change. Ignore
  fields you don't recognise and check the major version.

```json
{
  "format_version": "1.0",
  "planrisk_version": "0.1.0",
  "terraform_version": "1.16.1",
  "plan_format_version": "1.2",
  "max_severity": "critical",
  "counts": { "create": 0, "update": 2, "replace": 2, "delete": 3, "forget": 1,
              "read": 0, "import": 0, "move": 1, "unknown": 0 },
  "findings": [
    { "rule_id": "stateful-replace", "severity": "critical",
      "address": "aws_db_instance.main", "message": "database must be replaced",
      "reason": "destroy and then create replacement; forced by changes to availability_zone" }
  ]
}
```

## Rules

| ID | Default | Fires when |
|---|---|---|
| `stateful-delete` | critical | A resource that holds data (database, bucket, disk, ...) will be destroyed |
| `stateful-replace` | critical | A resource that holds data must be replaced (destroyed and re-created) |
| `iam-wildcard` | high | A policy starts allowing `"Action": "*"`, `"Resource": "*"` or `"Principal": "*"` |
| `public-endpoint` | high | A database, load balancer or instance becomes reachable from the internet |
| `s3-public-access` | high | S3 Block Public Access protections are turned off or removed |
| `sg-open-ingress` | high | A security group starts allowing ingress from `0.0.0.0/0` or `::/0` |
| `delete` | medium | A resource will be destroyed |
| `replace` | medium | A resource must be replaced; the finding says which attribute forced it |
| `blast-radius` | medium | More than 25 resources change, or more than 5 are destroyed, in one plan |

When a stateful rule fires, the plain `delete` or `replace` finding for the
same resource is dropped, so each resource gets one finding.

"Holds data" means a resource type in a built-in list: AWS databases,
buckets, disks, file systems, caches, keys, secrets, streams and similar,
plus the main GCP and Azure equivalents. See
[`src/rules/stateful.rs`](https://github.com/Andrew-Hinson/planrisk/blob/main/src/rules/stateful.rs).
The access rules (`iam-wildcard`, `public-endpoint`, `s3-public-access`,
`sg-open-ingress`) cover AWS only; the other rules work for any provider.

**The default severities and thresholds are heuristics.** No standard
exists for them; they are conservative starting points. Change any of them
in the config file.

## Config file

Optional. planrisk loads `.planrisk.toml` from the current directory if it
exists, or the file given with `--config`.

```toml
# Blast radius thresholds. A finding fires when a count is *more* than these.
[thresholds]
blast_radius_changes = 40   # default 25
blast_radius_deletes = 10   # default 5

# Change a rule's severity everywhere.
[rules.delete]
severity = "low"

# Turn a rule off.
[rules.iam-wildcard]
enabled = false

# Change matching resources only. `rule` is a rule ID or "*" for every rule.
[[overrides]]
address = "module.sandbox.*"
rule = "*"
severity = "low"

[[overrides]]
address = "aws_s3_bucket.logs"
rule = "stateful-delete"
enabled = false
```

- **Order:** each finding starts at its rule's severity, then `[rules.<id>]`
  applies, then every matching `[[overrides]]` entry from top to bottom. The
  last one to set a value wins, so a later override can turn a rule back on
  for some addresses.
- **Address globs:** `*` matches any characters, dots included, so
  `module.sandbox.*` also matches nested modules. `?` matches one character.
  Everything else matches itself, so `aws_instance.web[0]` and
  `module.app["eu"].*` work as written.
- **Overrides only match findings about a resource.** Use `[rules.<id>]` for
  plan-wide findings such as `blast-radius`.
- **Turning off a stateful rule brings back the plain one.** With the last
  override above, deleting `aws_s3_bucket.logs` gives a medium `delete`
  finding instead of a critical `stateful-delete`. To silence it completely,
  turn off `delete` for that address too.
- **Mistakes are errors.** A misspelled key, an unknown rule ID or a bad
  severity exits with code 1 and says what is wrong, rather than being
  ignored.

## GitHub Action

Coming soon: a GitHub Action that posts the Markdown summary as a pull
request comment and updates it on each push. Until then, run planrisk in a
workflow step and use its exit code with `--fail-on`.

## Terraform and OpenTofu

planrisk supports Terraform 1.x: it accepts plan `format_version` 1.x and
rejects other major versions. OpenTofu's plan JSON is largely compatible,
so it likely works, but it isn't tested.

## Contributing

```sh
cargo build
cargo test
cargo run -- tests/fixtures/create-only.json
```

### Adding a rule

1. Create `src/rules/<rule_name>.rs` with a struct that implements the
   `Rule` trait in [`src/rules/mod.rs`](https://github.com/Andrew-Hinson/planrisk/blob/main/src/rules/mod.rs).
   Give it tests for plans where it fires and where it doesn't.
2. Declare the module in `src/rules/mod.rs` and add it to `all_rules()`.
   That is the only place rules are registered.
3. Run the tests and accept the new `planrisk rules` snapshot (see below).

### Adding a fixture

Fixtures are real `terraform show -json` output, generated from Terraform
configs without a cloud account. See
[`tests/fixtures/README.md`](https://github.com/Andrew-Hinson/planrisk/blob/main/tests/fixtures/README.md).

### Snapshot tests

`tests/snapshots.rs` runs every plan in `tests/fixtures/` through every
output format and compares the result with the saved copy in
`tests/snapshots/`. Adding a fixture adds its snapshots automatically.

When output changes on purpose, the test fails and shows a diff. To accept
the new output, either:

- review each change interactively with `cargo insta review` (install it with
  `cargo install cargo-insta`), or
- run `INSTA_UPDATE=always cargo test` and review the changed `.snap` files
  in `git diff`.

Commit the updated `.snap` files with the change that caused them.

The design and its reasoning are in
[PLAN.md](https://github.com/Andrew-Hinson/planrisk/blob/main/PLAN.md).

## License

Licensed under either of
[Apache License, Version 2.0](https://github.com/Andrew-Hinson/planrisk/blob/main/LICENSE-APACHE)
or [MIT license](https://github.com/Andrew-Hinson/planrisk/blob/main/LICENSE-MIT)
at your option.
