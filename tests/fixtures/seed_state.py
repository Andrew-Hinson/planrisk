"""Builds a Terraform state file from a plan's planned values.

Used by generate.sh for fixtures that need resources to already exist
(updates, deletes, replaces). It lets real Terraform produce those plans
without an AWS account:

1. Plan the configs/<name>/before/ config: every resource is a create, and
   `planned_values` holds every attribute, defaults included.
2. This script turns those values into a state file, as if they had been
   applied. Values only known after apply (`after_unknown`) get a fake
   placeholder of the right type, read from the provider schema, the way a
   real apply would fill them in. Without that, Terraform would plan to
   "update" every resource just to fill them.
3. Plan configs/<name>/ against that state with `-refresh=false`, so
   Terraform compares state with config without calling AWS.

Usage:
  terraform providers schema -json > schema.json
  python3 seed_state.py before-plan.json schema.json > terraform.tfstate
"""

import json
import sys


def resources(module):
    """Every resource in a planned_values module, including child modules."""
    yield from module.get("resources", [])
    for child in module.get("child_modules", []):
        yield from resources(child)


def placeholder(attr_type, name):
    """A fake value of a schema attribute type, e.g. "string" or ["map", "string"]."""
    if attr_type == "string":
        return "fake-" + name
    if attr_type == "number":
        return 0
    if attr_type == "bool":
        return False
    kind = attr_type[0]
    if kind in ("list", "set"):
        return []
    if kind == "map":
        return {}
    return None  # objects and anything unexpected stay null


def main():
    with open(sys.argv[1]) as f:
        plan = json.load(f)
    with open(sys.argv[2]) as f:
        schemas = json.load(f)["provider_schemas"]

    unknown = {
        rc["address"]: rc["change"].get("after_unknown") or {}
        for rc in plan.get("resource_changes", [])
    }

    # State groups instances by resource; count/for_each resources have
    # several instances under one entry.
    entries = {}
    for r in resources(plan["planned_values"]["root_module"]):
        attributes = dict(r["values"])
        schema = schemas[r["provider_name"]]["resource_schemas"][r["type"]]["block"]
        for name, is_unknown in unknown.get(r["address"], {}).items():
            if is_unknown is True and name in schema.get("attributes", {}):
                attributes[name] = placeholder(schema["attributes"][name]["type"], name)
        attributes["id"] = "fake-" + r["address"]
        instance = {"schema_version": r["schema_version"], "attributes": attributes}
        if "index" in r:
            instance["index_key"] = r["index"]

        # e.g. "module.queue.aws_sqs_queue.this[0]" -> module "module.queue"
        module = r["address"].rsplit(r["type"] + "." + r["name"], 1)[0].rstrip(".")
        key = (module, r["mode"], r["type"], r["name"])
        if key not in entries:
            entry = {
                "mode": r["mode"],
                "type": r["type"],
                "name": r["name"],
                "provider": 'provider["%s"]' % r["provider_name"],
                "instances": [],
            }
            if module:
                entry["module"] = module
            entries[key] = entry
        entries[key]["instances"].append(instance)

    state = {
        "version": 4,
        "terraform_version": plan["terraform_version"],
        "serial": 1,
        "lineage": "planrisk-fixture",
        "outputs": {},
        "resources": list(entries.values()),
    }
    json.dump(state, sys.stdout, indent=2)
    print()


if __name__ == "__main__":
    main()
