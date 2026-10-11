# Plan fixtures

Real `terraform show -json` output used by the tests. Each `<name>.json` is
generated from the Terraform config in `configs/<name>/`.

## Regenerating

```sh
tests/fixtures/generate.sh            # all fixtures
tests/fixtures/generate.sh empty      # just one
```

Needs Terraform on your `PATH`, Python 3, and network access to download
providers. **No cloud account is needed:**

- Fixtures that only create resources are planned with fake AWS credentials
  and the provider's `skip_credentials_validation`,
  `skip_requesting_account_id` and `skip_metadata_api_check` settings.
  Planning a create never calls AWS.
- Fixtures that change existing resources have a `before/` folder describing
  what "already exists". `generate.sh` plans `before/`, turns the result into
  a state file with `seed_state.py`, then plans the fixture against that
  state with `-refresh=false`, so Terraform never contacts AWS. The output is
  a real Terraform plan; only the starting state is synthetic. See
  `seed_state.py` for how it fills values that are only known after apply.
- Importing AWS resources reads them from AWS, so the `import` fixture uses
  the `random` provider, which imports from the value alone.

Each config's `.terraform.lock.hcl` is committed so regenerating uses the same
provider versions.

Files starting with `hand-edited-` have no config: they cover cases real
Terraform doesn't produce. `invalid/` holds plans planrisk must reject; the
snapshot test doesn't read that folder.

## Before committing a fixture

Check the JSON for real account IDs, ARNs, local paths and secrets, for example:

```sh
grep -n -i -E '/home|/Users|[0-9]{12}|arn:' tests/fixtures/<name>.json
```

## Fixtures

| Fixture | Terraform | What it covers |
|---|---|---|
| `empty.json` | 1.16.1 | No resources. There is no `resource_changes` key at all, not an empty list. |
| `create-only.json` | 1.16.1, AWS provider 6.68 | Three creates: an S3 bucket, a security group with an inline ingress rule, and an SQS queue inside a local module (`module.queue`). |
| `sensitive.json` | 1.16.1, AWS provider 6.68 | Two secrets in plain text, as `terraform show -json` writes them: `PLANRISK-FIXTURE-SECRET-VARIABLE` from a `sensitive` variable (into `aws_db_instance.password`) and `PLANRISK-FIXTURE-SECRET-SCHEMA` from an attribute the provider marks sensitive (`aws_secretsmanager_secret_version.secret_string`). `tests/redaction.rs` checks neither appears in any output. These are fake values made for this fixture. |
| `mixed-changes.json` | 1.16.1, AWS provider 6.68, from `before/` | One of each change: stateful replace (`aws_db_instance`, forced by `availability_zone`), stateful delete (`aws_s3_bucket`), create-before-destroy replace (`aws_security_group`), plain delete, delete because a `count` index is out of range, in-place update, update inside a module, `moved` block, `removed` block with `destroy = false` (forget). |
| `large.json` | 1.16.1, AWS provider 6.68, from `before/` | 40 in-place updates and 6 `aws_ebs_volume` deletes: over both blast-radius thresholds. |
| `import.json` | 1.16.1, random provider 3.9 | Two `import` blocks: one that matches the config (pure import, `no-op`) and one that Terraform must then replace. |
| `hand-edited-unknown-action.json` | hand-edited from `create-only.json` | One resource's actions changed to `["teleport"]`, an action planrisk doesn't recognise. |
| `invalid/format-version-2.json` | hand-edited from `empty.json` | `format_version` `2.0`, which planrisk must reject. |
