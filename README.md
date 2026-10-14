# planrisk

Reads a Terraform plan and flags risky changes: what gets destroyed, what
gets replaced and why, what gets opened to the internet, and how much
changes at once. Scanners like Checkov and Trivy check your configuration;
planrisk checks the change.

```
$ planrisk plan.json
Terraform 1.16.1, plan format 1.2
0 to create, 2 to update, 2 to replace, 3 to delete, 1 to forget, 1 to move

Findings:
  critical  aws_db_instance.main: database must be replaced (destroy and then create replacement; forced by changes to availability_zone) [stateful-replace]
  medium    aws_sqs_queue.unused: will be destroyed (aws_sqs_queue.unused is not in configuration) [delete]
  ...
```

## Install

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/Andrew-Hinson/planrisk/releases/latest/download/planrisk-installer.sh | sh
# or
cargo install planrisk
```

Binaries for Linux and macOS are on the
[releases page](https://github.com/Andrew-Hinson/planrisk/releases).

## Usage

```sh
terraform plan -out plan.out
terraform show -json plan.out | planrisk -
```

- `--format terminal|markdown|json`: Markdown is for PR comments, and is
  the default when output isn't a terminal.
- `--fail-on <severity>`: exit 2 if any finding is at least this severe.
  Without it, planrisk never fails the build.
- `planrisk rules`: list the rules and their default severities.

Exit codes: 0 OK, 1 error, 2 a finding at or above `--fail-on`.

Needs Terraform 1.1 or later. Sensitive values are always redacted.

## Config

Optional `.planrisk.toml` (or `--config PATH`):

```toml
[rules.delete]
severity = "low"            # change a rule everywhere

[rules.iam-wildcard]
enabled = false             # turn a rule off

[[overrides]]
address = "module.sandbox.*" # or just some resources
rule = "*"
severity = "low"

[thresholds]
blast_radius_changes = 40   # default 25
blast_radius_deletes = 10   # default 5
```

Later settings win. Default severities are heuristics, so tune them.

## Contributing

See [CONTRIBUTING.md](https://github.com/Andrew-Hinson/planrisk/blob/main/CONTRIBUTING.md).

## License

MIT or Apache-2.0, at your option.
