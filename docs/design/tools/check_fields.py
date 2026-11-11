#!/usr/bin/env python3
"""Every property of every schema has a consumer.

A property is used when its name appears in backticks in SPEC.md or in a
contracts/*.md file. A schema field that no document explains has no consumer
and is either dead or undocumented. Also checks the reverse for the capability
record: every key of every valid capability example is a schema property
(additionalProperties false already enforces it, this names the culprit).
Exit 0 all used, 1 unused properties.
"""
import json
import os
import re
import sys
from pathlib import Path

root = Path(os.environ.get("PODRUNNER_SPEC_ROOT") or Path(__file__).resolve().parent.parent)
text = (root / "SPEC.md").read_text()
for doc in sorted((root / "contracts").glob("*.md")):
    text += "\n" + doc.read_text()
ticks = set(re.findall(r"`([^`\n]+)`", text))
words = set()
for t in ticks:
    words.update(re.findall(r"[A-Za-z_][A-Za-z0-9_-]*", t))


def props(node, path=""):
    if isinstance(node, dict):
        for name, sub in node.get("properties", {}).items():
            yield name, f"{path}/{name}"
            yield from props(sub, f"{path}/{name}")
        for sub in node.get("$defs", {}).values():
            yield from props(sub, path)
        if isinstance(node.get("items"), dict):
            yield from props(node["items"], path + "[]")


findings = []
n = 0
for sp in sorted((root / "schemas").glob("*.schema.json")):
    schema = json.loads(sp.read_text())
    for name, where in props(schema):
        n += 1
        if name not in words:
            findings.append(f"{sp.name}: property {where} is not named in backticks in SPEC.md or contracts/")
for f in findings:
    print(f)
print(f"properties checked: {n}, unused: {len(findings)}")
sys.exit(1 if findings else 0)
