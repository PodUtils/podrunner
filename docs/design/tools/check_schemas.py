#!/usr/bin/env python3
"""Validate examples/valid and examples/invalid against schemas/.

The schema is chosen from the file name prefix: NAME.something.json uses
schemas/NAME.v1.schema.json. Checks:
  1. every valid example passes its schema;
  2. every invalid example fails, and for the reason named in
     examples/invalid/EXPECT.json (a substring of the failing JSON path or
     message), so an example cannot pass as invalid for the wrong reason;
  3. the policy object of every job-offer example, valid and invalid, is also
     validated against schemas/external/broker-policy.v1.schema.json, the verbatim copy
     of the pod-broker policy schema. Valid offers must pass. An invalid offer
     whose EXPECT entry starts with "broker:" must fail there.
Field use is checked by tools/check_fields.py. Refusal codes by
tools/check_codes.py.
Exit 0 all as expected, 1 any unexpected, 2 cannot run.

Environment: PODRUNNER_SPEC_ROOT overrides the directory to check (used by the
planted-defect tests of the guards themselves).
"""
import json
import os
import sys
from pathlib import Path

try:
    import jsonschema
except ImportError:
    print("python3 module jsonschema is missing", file=sys.stderr)
    sys.exit(2)

root = Path(os.environ.get("PODRUNNER_SPEC_ROOT") or Path(__file__).resolve().parent.parent)
bad = 0
for schema_path in sorted((root / "schemas").glob("*.schema.json")):
    jsonschema.Draft202012Validator.check_schema(json.loads(schema_path.read_text()))

broker_schema_path = root / "schemas" / "external" / "broker-policy.v1.schema.json"
if not broker_schema_path.exists():
    print("schemas/external/broker-policy.v1.schema.json is missing", file=sys.stderr)
    sys.exit(2)
broker = jsonschema.Draft202012Validator(json.loads(broker_schema_path.read_text()))
expect = json.loads((root / "examples" / "invalid" / "EXPECT.json").read_text())


def schema_for(example: Path) -> Path:
    return root / "schemas" / (example.name.split(".")[0] + ".v1.schema.json")


def error_text(e) -> str:
    return "/".join(str(p) for p in e.absolute_path) + " :: " + e.message


n_valid = n_invalid = n_policy = 0
seen_invalid = set()
for kind, want_ok in (("valid", True), ("invalid", False)):
    for ex in sorted((root / "examples" / kind).glob("*.json")):
        if ex.name == "EXPECT.json":
            continue
        doc = json.loads(ex.read_text())
        validator = jsonschema.Draft202012Validator(json.loads(schema_for(ex).read_text()))
        errors = list(validator.iter_errors(doc))
        n_valid += want_ok
        n_invalid += not want_ok
        if bool(errors) == want_ok:
            bad += 1
            print(f"UNEXPECTED: {kind}/{ex.name} was {'rejected' if errors else 'accepted'}")
            for e in errors[:3]:
                print(f"  {error_text(e)}")
        want = expect.get(ex.name) if not want_ok else None
        if not want_ok:
            seen_invalid.add(ex.name)
            if want is None:
                bad += 1
                print(f"UNEXPECTED: invalid/{ex.name} has no entry in EXPECT.json")
            elif not want.startswith("broker:") and errors and not any(want in error_text(e) for e in errors):
                bad += 1
                print(f"UNEXPECTED: invalid/{ex.name} was rejected, but not for '{want}':")
                for e in errors[:3]:
                    print(f"  {error_text(e)}")
        if ex.name.startswith("job-offer.") and isinstance(doc.get("policy"), dict):
            n_policy += 1
            perrors = list(broker.iter_errors(doc["policy"]))
            if want_ok and perrors:
                bad += 1
                print(f"UNEXPECTED: policy of valid/{ex.name} fails the broker schema: {perrors[0].message}")
            if want is not None and want.startswith("broker:") and not perrors:
                bad += 1
                print(f"UNEXPECTED: policy of invalid/{ex.name} passes the broker schema")

for name in expect:
    if name not in seen_invalid:
        bad += 1
        print(f"UNEXPECTED: EXPECT.json names {name}, which is not in examples/invalid")

print(f"schemas: {n_valid} valid examples accepted, {n_invalid} invalid examples rejected for their stated reason, "
      f"{n_policy} offer policies checked against the broker schema, unexpected: {bad}")
sys.exit(1 if bad else 0)
