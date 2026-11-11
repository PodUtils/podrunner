#!/usr/bin/env python3
"""The refusal codes of SPEC.md and of schemas/run-report.v1.schema.json agree.

SPEC.md holds one table whose header row starts with "| Code |". Its first
column holds each code in backticks, its second column is the class, wait or
terminal. The set of codes must equal the enum $defs/refusal_code of every report
schema under schemas/ (run-report*.schema.json: the full report and the public subset), each code must appear once, and every code must be named in at least
one other place of SPEC.md or contracts/ (a code that nothing raises is dead).
Exit 0 agreement, 1 differences, 2 table not found.
"""
import json
import os
import re
import sys
from pathlib import Path

root = Path(os.environ.get("PODRUNNER_SPEC_ROOT") or Path(__file__).resolve().parent.parent)
spec = (root / "SPEC.md").read_text()
lines = spec.splitlines()
codes, classes = [], {}
start = None
for i, line in enumerate(lines):
    if line.startswith("| Code |"):
        start = i
        break
if start is None:
    print("no table with header '| Code |' in SPEC.md")
    sys.exit(2)
for line in lines[start + 2:]:
    if not line.startswith("|"):
        break
    cells = [c.strip() for c in line.strip().strip("|").split("|")]
    m = re.fullmatch(r"`([a-z][a-z0-9-]*)`", cells[0])
    if not m:
        print(f"row without a code in the first column: {line[:60]}")
        sys.exit(1)
    codes.append(m.group(1))
    classes[m.group(1)] = cells[1]

findings = []
for c in set(codes):
    if codes.count(c) > 1:
        findings.append(f"code {c} appears more than once in the table")
    if classes[c] not in ("wait", "terminal"):
        findings.append(f"code {c} has class '{classes[c]}', not wait or terminal")
report_schemas = sorted((root / "schemas").glob("run-report*.schema.json"))
if len(report_schemas) < 2:
    findings.append("fewer than two report schemas (the full report and the public subset) under schemas/")
for sp in report_schemas:
    schema = json.loads(sp.read_text())
    enum = set(schema["$defs"]["refusal_code"]["enum"])
    for c in sorted(set(codes) - enum):
        findings.append(f"code {c} is in the SPEC table and not in the enum of {sp.name}")
    for c in sorted(enum - set(codes)):
        findings.append(f"code {c} is in the enum of {sp.name} and not in the SPEC table")

rest = "\n".join(lines[:start] + lines[start + 2 + len(codes):])
for doc in sorted((root / "contracts").glob("*.md")):
    rest += "\n" + doc.read_text()
for c in sorted(set(codes)):
    if f"`{c}`" not in rest:
        findings.append(f"code {c} is never named outside its table row")
for f in findings:
    print(f)
print(f"refusal codes: {len(set(codes))}, findings: {len(findings)}")
sys.exit(1 if findings else 0)
