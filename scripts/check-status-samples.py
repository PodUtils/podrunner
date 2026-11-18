#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""Validate each line of a file of `status` samples against a schema (C4 of
GitHub #292, T-525).

Usage: check-status-samples.py SCHEMA SAMPLES

Each line of SAMPLES is one JSON object. The check needs the module
`jsonschema` (draft 2020-12). It prints the count of the lines and exits 1
on the first line that the schema refuses.
"""

import json
import sys

import jsonschema


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__.strip(), file=sys.stderr)
        return 64
    with open(sys.argv[1], encoding="utf-8") as f:
        schema = json.load(f)
    validator = jsonschema.Draft202012Validator(schema)
    count = 0
    with open(sys.argv[2], encoding="utf-8") as f:
        for n, line in enumerate(f, 1):
            if not line.strip():
                continue
            errors = sorted(validator.iter_errors(json.loads(line)), key=str)
            if errors:
                print(f"{sys.argv[2]}:{n}: {errors[0].message}", file=sys.stderr)
                return 1
            count += 1
    if count == 0:
        print(f"{sys.argv[2]}: no sample", file=sys.stderr)
        return 1
    print(f"C4: {count} samples of status validate against {sys.argv[1]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
