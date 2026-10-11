#!/usr/bin/env bash
# Regenerates plan JSON fixtures from the Terraform configs in configs/.
#
# Usage: tests/fixtures/generate.sh [fixture-name ...]
# With no arguments, regenerates every fixture under configs/.
#
# No AWS account is needed:
# - Fixtures with only creates are planned with fake credentials; planning a
#   create never calls AWS.
# - Fixtures with a before/ folder start from existing resources: before/ is
#   planned first, seed_state.py turns that into a state file, and the
#   fixture is planned against it with -refresh=false, so Terraform never
#   contacts AWS. See seed_state.py for details.
set -euo pipefail

cd "$(dirname "$0")"
fixtures_dir=$(pwd)

export AWS_ACCESS_KEY_ID=fake-access-key
export AWS_SECRET_ACCESS_KEY=fake-secret-key
export TF_IN_AUTOMATION=1

names=("$@")
if [ ${#names[@]} -eq 0 ]; then
  names=()
  for dir in configs/*/; do names+=("$(basename "$dir")"); done
fi

for name in "${names[@]}"; do
  echo "==> $name"
  (
    cd "configs/$name"
    refresh=()
    rm -f terraform.tfstate

    if [ -d before ]; then
      (
        cd before
        terraform init -input=false -no-color >/dev/null
        terraform plan -input=false -no-color -out=plan.out >/dev/null
        terraform show -json plan.out > before-plan.json
        terraform providers schema -json > schema.json
        rm -f plan.out
      )
      python3 "$fixtures_dir/seed_state.py" before/before-plan.json before/schema.json > terraform.tfstate
      rm -f before/before-plan.json before/schema.json
      refresh=(-refresh=false)
    fi

    terraform init -input=false -no-color >/dev/null
    terraform plan "${refresh[@]}" -input=false -no-color -out=plan.out >/dev/null
    # Pretty-printed so diffs of regenerated fixtures are readable.
    terraform show -json plan.out | python3 -m json.tool --indent 2 > "$fixtures_dir/$name.json"
    rm -f plan.out terraform.tfstate
  )
done
