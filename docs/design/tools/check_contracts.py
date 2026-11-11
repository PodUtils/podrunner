#!/usr/bin/env python3
"""The messages in contracts/*.md are machine-checked.

For every fenced block of every contract file, each message that starts with
{"schema" is parsed as JSON (a message that spans lines is joined until its
braces balance). A placeholder such as { ...unit-capability.v1... } is replaced
by the valid example of that schema and the nested object is validated against
the schema. Then:
  1. every message starts with the key "schema" with value 1;
  2. a request has "op" and "id"; an answer has "id" and "ok";
  3. every "op" of a request has a heading "### op" in the same file;
  4. every heading "### op" of unit-driver.md and source-driver.md that names an
     operation has at least one request example;
  5. every event type that SPEC.md section 4.2 names for `next` (the backticked
     words before the word "events") has an example `"type":"NAME"` in
     source-driver.md, so an event cannot be named without a message shape;
  6. every operation named in a backticked list of SPEC.md section 4.2 (unit
     seam) and in the source seam paragraph exists as a heading or table row.
Also: the runner builds two mount sets, a job container's (prepare_job) and a
Docker action's (run_container_step). unit-driver.md holds one table for each
and hook.md one bullet for each. Each table and its bullet must name the same
targets, have the row count of the runner source (7 and 5), and carry the mode
that the runner uses (only /__e is read-only). A mount target named elsewhere
in hook.md must be in a table, except /__t, the tool cache variant that is
refused. Every "start" example carries "registries".
Exit 0 clean, 1 findings.
"""
import json
import os
import re
import sys
from pathlib import Path

import jsonschema

root = Path(os.environ.get("PODRUNNER_SPEC_ROOT") or Path(__file__).resolve().parent.parent)
PLACEHOLDERS = {
    "unit-capability.v1": ("unit-capability.v1.schema.json", "valid/unit-capability.podvm-userland.json"),
    "job-offer.v1": ("job-offer.v1.schema.json", "valid/job-offer.trusted-push.json"),
    "run-report.v1": ("run-report.v1.schema.json", "valid/run-report.done.json"),
    "run-report-public.v1": ("run-report-public.v1.schema.json", "valid/run-report-public.done.json"),
}
EGRESS_FORMS = {
    "unix": {"path"},
    "tcp-loopback": {"host", "port"},
    "vsock": {"cid", "port"},
}
findings = []
n_msgs = 0
seen_egress = set()


def blocks(text):
    return re.findall(r"```[a-z]*\n(.*?)```", text, re.S)


def messages(block):
    """Yield the text of each message: lines joined until braces balance."""
    cur, depth = [], 0
    for line in block.splitlines():
        if not cur and not line.lstrip().startswith('{"schema"'):
            continue
        cur.append(line)
        depth += line.count("{") - line.count("}")
        if depth <= 0:
            yield "\n".join(cur)
            cur, depth = [], 0
    if cur:
        yield "\n".join(cur)


for doc in sorted((root / "contracts").glob("*.md")):
    text = doc.read_text()
    headings = set(re.findall(r"^### (\w+)", text, re.M))
    ops_seen = set()
    for block in blocks(text):
        for raw in messages(block):
            n_msgs += 1
            nested = []

            def repl(m):
                key = m.group(1)
                if key not in PLACEHOLDERS:
                    findings.append(f"{doc.name}: unknown placeholder {key}")
                    return "null"
                schema_file, example = PLACEHOLDERS[key]
                nested.append((schema_file, json.loads((root / "examples" / example).read_text())))
                return json.dumps(nested[-1][1])

            body = re.sub(r"\{ \.\.\.([a-z0-9.-]+)\.\.\. \}", repl, raw)
            try:
                msg = json.loads(body)
            except json.JSONDecodeError as e:
                findings.append(f"{doc.name}: message does not parse ({e.msg}): {raw[:70]}")
                continue
            if not isinstance(msg, dict) or list(msg)[:1] != ["schema"] or msg.get("schema") != 1:
                findings.append(f"{doc.name}: message does not start with \"schema\":1: {raw[:70]}")
                continue
            if "op" in msg:
                ops_seen.add(msg["op"])
                if "id" not in msg:
                    findings.append(f"{doc.name}: request without id: {raw[:70]}")
                if msg["op"] not in headings:
                    findings.append(f"{doc.name}: op {msg['op']} has no heading '### {msg['op']}'")
            elif "id" in msg:
                if "ok" not in msg:
                    findings.append(f"{doc.name}: answer without ok: {raw[:70]}")
            else:
                findings.append(f"{doc.name}: message is neither request nor answer: {raw[:70]}")
            if msg.get("op") == "start":
                egress = msg.get("egress")
                form = EGRESS_FORMS.get(egress.get("kind")) if isinstance(egress, dict) else None
                if form is None or set(egress) != {"kind"} | form:
                    findings.append(f"{doc.name}: start message with an egress object that is not one of {sorted(EGRESS_FORMS)}: {raw[:60]}")
                else:
                    seen_egress.add(egress["kind"])
            for schema_file, obj in nested:
                schema = json.loads((root / "schemas" / schema_file).read_text())
                for e in jsonschema.Draft202012Validator(schema).iter_errors(obj):
                    findings.append(f"{doc.name}: nested {schema_file}: {e.message}")
    if doc.name in ("unit-driver.md", "source-driver.md"):
        wire_ops = {h for h in headings if h.islower() and not h.startswith("container")}
        for h in sorted(wire_ops - ops_seen):
            findings.append(f"{doc.name}: heading '### {h}' has no request example")

for kind in sorted(set(EGRESS_FORMS) - seen_egress):
    findings.append(f"no start example in the contracts shows the egress form {kind}")
spec = (root / "SPEC.md").read_text()
unit = (root / "contracts" / "unit-driver.md").read_text()
unit_ops = {"probe", "prepare", "start", "signal", "wait", "destroy"}
for op in sorted(unit_ops):
    if f"### {op}" not in unit or f"`{op}`" not in spec:
        findings.append(f"unit operation {op} is missing from unit-driver.md or from SPEC.md")
for op in ("container_start", "container_exec", "container_logs", "container_inspect", "container_stop"):
    if f"`{op}`" not in unit or f"`{op}`" not in spec:
        findings.append(f"container operation {op} is missing from unit-driver.md or from SPEC.md")
src = (root / "contracts" / "source-driver.md").read_text()
for op in ("hello", "next", "ack", "claim", "mint", "report", "audit", "withdraw", "release"):
    if f"### {op}" not in src or f"`{op}`" not in spec:
        findings.append(f"source operation {op} is missing from source-driver.md or from SPEC.md")

m = re.search(r"`next` \(long poll[^)]*returns ([^)]*?) events\)", spec)
if not m:
    findings.append("SPEC.md section 4.2 has no 'returns ... events' clause for `next`")
else:
    for ev in re.findall(r"`([a-z]+)`", m.group(1)):
        if f'"type":"{ev}"' not in src:
            findings.append(f"event type {ev} of `next` has no example message in source-driver.md")

hook = (root / "contracts" / "hook.md").read_text()
# The runner builds two mount sets. unit-driver.md holds one table for each, hook.md one bullet for each.
# The read-only targets are the ones that the runner marks read-only (MountWellKnownDirectories, Linux branch).
READ_ONLY_TARGETS = {"/__e"}
NUMBER_WORDS = {7: "Seven", 5: "Five"}
TABLES = (
    ("job container", r"The mount table\. The job container table[^\n]*\n\n((?:\|[^\n]*\n)+)", r"^- Job container mounts[^\n]*", 7),
    ("container action", r"The container action table[^\n]*\n\n((?:\|[^\n]*\n)+)", r"^- Container action mounts[^\n]*", 5),
)
all_targets = set()
for name, table_re, bullet_re, count in TABLES:
    table = re.search(table_re, unit)
    bullet = re.search(bullet_re, hook, re.M)
    if not table:
        findings.append(f"unit-driver.md has no {name} mount table")
        continue
    if not bullet:
        findings.append(f"hook.md has no '{name} mounts' bullet")
        continue
    rows = re.findall(r"^\| `(/[^`]+)` \| ([^|]+) \| (ro|rw) \|$", table.group(1), re.M)
    in_table = {t for t, _, _ in rows}
    all_targets |= in_table
    in_hook = {t for t in re.findall(r"`(/[^`]+)`", bullet.group(0)) if t.startswith(("/__", "/github/"))}
    for t in sorted(in_table - in_hook):
        findings.append(f"{name} mount target {t} is in the table of unit-driver.md and not named in hook.md")
    for t in sorted(in_hook - in_table):
        findings.append(f"{name} mount target {t} is named in hook.md and not in the table of unit-driver.md")
    if len(rows) != count:
        findings.append(f"the {name} mount table has {len(rows)} rows, the runner source gives {count}")
    if f"{NUMBER_WORDS[count]} entries" not in bullet.group(0):
        findings.append(f"hook.md '{name} mounts' bullet does not say '{NUMBER_WORDS[count]} entries'")
    for target, _, mode in rows:
        want = "ro" if target in READ_ONLY_TARGETS else "rw"
        if mode != want:
            findings.append(f"{name} mount table: {target} has mode {mode}, the runner mounts it {want}")
if all_targets:
    # A mount target named anywhere else in hook.md must belong to a table, except the one tool cache variant
    # that hook.md explains and refuses.
    for t in sorted({t for t in re.findall(r"`(/[^`]+)`", hook) if t.startswith(("/__", "/github/"))} - all_targets - {"/__t"}):
        findings.append(f"mount target {t} is named in hook.md and is in no table of unit-driver.md")

for doc in sorted((root / "contracts").glob("*.md")):
    for block in blocks(doc.read_text()):
        for raw in messages(block):
            if '"op":"start"' in raw and '"registries"' not in raw:
                findings.append(f"{doc.name}: a start example without registries: {raw[:60]}")

for f in findings:
    print(f)
print(f"contract messages checked: {n_msgs}, findings: {len(findings)}")
sys.exit(1 if findings else 0)
