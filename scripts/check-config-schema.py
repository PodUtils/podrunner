#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""Validate podrunner.toml files against schemas/config.v1.json (T-479).

    python3 -I scripts/check-config-schema.py [--self-test]

It validates docs/examples/podrunner.toml as a whole file, and each
```toml block of docs/configuration.md with the keys that a block may leave
out (a block shows some tables of a file); a block after the line
`<!-- not podrunner.toml -->` is another file. With --self-test, a planted
unknown key and a planted wrong type must each fail. It needs the module
`jsonschema` (draft 2020-12). `podrunner check` stays the last word.
"""

import copy
import json
import pathlib
import re
import sys
import tomllib

import jsonschema

ROOT = pathlib.Path(__file__).resolve().parent.parent
SCHEMA = ROOT / "schemas" / "config.v1.json"
EXAMPLE = ROOT / "docs" / "examples" / "podrunner.toml"
DOC = ROOT / "docs" / "configuration.md"


def validators():
    schema = json.loads(SCHEMA.read_text())
    whole = jsonschema.Draft202012Validator(schema)
    part = copy.deepcopy(schema)
    part.pop("required", None)
    return whole, jsonschema.Draft202012Validator(part)


def errors(v, doc: dict) -> list[str]:
    return [f"{'/'.join(map(str, e.absolute_path)) or '(top)'}: {e.message}" for e in v.iter_errors(doc)]


def blocks(text: str) -> list[tuple[int, str]]:
    out = []
    for m in re.finditer(r"^```toml\n(.*?)^```", text, re.M | re.S):
        # A block of another file (the file of the mint) has this mark.
        if text[: m.start()].rstrip().endswith("<!-- not podrunner.toml -->"):
            continue
        out.append((text[: m.start()].count("\n") + 1, m.group(1)))
    return out


def main() -> int:
    whole, part = validators()
    bad = []
    bad += [f"{EXAMPLE.relative_to(ROOT)}: {e}" for e in errors(whole, tomllib.loads(EXAMPLE.read_text()))]
    found = blocks(DOC.read_text())
    for line, body in found:
        try:
            doc = tomllib.loads(body)
        except tomllib.TOMLDecodeError as e:
            bad.append(f"{DOC.relative_to(ROOT)}:{line}: not TOML: {e}")
            continue
        bad += [f"{DOC.relative_to(ROOT)}:{line}: {e}" for e in errors(part, doc)]
    if "--self-test" in sys.argv[1:]:
        example = tomllib.loads(EXAMPLE.read_text())
        for what, plant in (("an unknown key", {"slotz": 1}), ("a wrong type", {"max_units": "two"})):
            if not errors(whole, {**example, **plant}):
                bad.append(f"self-test: {what} passes the schema")
    for b in bad:
        print(b)
    if bad:
        return 1
    print(f"check-config-schema: {EXAMPLE.relative_to(ROOT)} and {len(found)} blocks of {DOC.name} agree with {SCHEMA.name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
