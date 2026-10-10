#!/usr/bin/env bash
# Regenerates plan JSON fixtures from the Terraform configs in configs/.
#
# Usage: tests/fixtures/generate.sh [fixture-name ...]
# With no arguments, regenerates every fixture under configs/.
#
# Uses fake AWS credentials: the configs only plan creates, which never call AWS.
set -euo pipefail

cd "$(dirname "$0")"

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
    terraform init -input=false -no-color >/dev/null
    terraform plan -input=false -no-color -out=plan.out >/dev/null
    # Pretty-printed so diffs of regenerated fixtures are readable.
    terraform show -json plan.out | python3 -m json.tool --indent 2 > "../../$name.json"
    rm -f plan.out
  )
done
