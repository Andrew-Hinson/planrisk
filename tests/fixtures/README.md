# Plan fixtures

Real `terraform show -json` output used by the tests. Each `<name>.json` is
generated from the Terraform config in `configs/<name>/`.

## Regenerating

```sh
tests/fixtures/generate.sh            # all fixtures
tests/fixtures/generate.sh empty      # just one
```

Needs Terraform on your `PATH` and network access to download providers.
The script uses fake AWS credentials and the provider's
`skip_credentials_validation`, `skip_requesting_account_id` and
`skip_metadata_api_check` settings. That works because planning a *create*
never calls AWS. Updates, deletes and replaces need existing state, so those
fixtures will need a different approach (tracked in #18).

Each config's `.terraform.lock.hcl` is committed so regenerating uses the same
provider versions.

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
