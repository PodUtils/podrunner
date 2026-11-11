#!/usr/bin/env python3
"""Run the cases of examples/tier-cases.json through tools/tier_rule.py, and
compare the tier rule of SPEC.md section 6.3 with the implementation.

Each case names a base offer under examples/valid, overrides of its
trust_inputs, a configuration, the set tier, and the expected computed tier
(null: no tier) and intake verdict. Every example offer under examples/valid
must be used by at least one case. Every event of the event enum of the offer
schema has at least one case.

SPEC.md section 6.3 holds two fenced blocks:
  tier-rule   the lines trusted_events, untrusted_events and refused_events.
              Each list must equal the matching tuple of tools/tier_rule.py.
              The three lists together must cover the event enum of
              schemas/job-offer.v1.schema.json exactly (trusted and untrusted
              events may overlap, refused events may not overlap either).
  tier-prose  the rule in words. Under "trusted" and "untrusted" each
              condition line starts with a condition name and a colon. The
              names must equal the keys of tier_rule.TRUSTED_CONDITIONS and
              tier_rule.UNTRUSTED_CONDITIONS. The wording of a condition is not
              compared, only its presence: deleting a line of the prose rule,
              or a predicate of the code, is a difference.
The prose of section 6.3 outside the two blocks may name an event only in
backticks and only an event that the tier-rule block names. The events that
the guard recognises are the offer enum and GITHUB_EVENTS below (the event
names of GitHub's "Events that trigger workflows" page, fetched 2026-11-10).
The guard does not read what the prose says about an event, only which events
it names.
Exit 0 all as expected, 1 any difference.
"""
import json
import os
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import tier_rule  # noqa: E402

root = Path(os.environ.get("PODRUNNER_SPEC_ROOT") or Path(__file__).resolve().parent.parent)
cases = json.loads((root / "examples" / "tier-cases.json").read_text())
bad = 0
used = set()
for case in cases:
    base = json.loads((root / "examples" / "valid" / case["offer"]).read_text())
    used.add(case["offer"])
    ti = dict(base["trust_inputs"])
    ti.update(case.get("override", {}))
    got = tier_rule.compute_tier(ti, case["config"])
    verdict = tier_rule.intake_verdict(got, case["set_tier"])
    if got != case["tier"] or verdict != case["intake"]:
        bad += 1
        print(f"UNEXPECTED {case['name']}: tier {got} (want {case['tier']}), intake {verdict} (want {case['intake']})")
for p in sorted((root / "examples" / "valid").glob("job-offer.*.json")):
    if p.name not in used:
        bad += 1
        print(f"UNEXPECTED: {p.name} is used by no tier case")
GITHUB_EVENTS = {
    "branch_protection_rule", "check_run", "check_suite", "create", "delete", "deployment",
    "deployment_status", "discussion", "discussion_comment", "fork", "gollum", "issue_comment",
    "issues", "label", "merge_group", "milestone", "page_build", "public", "pull_request",
    "pull_request_review", "pull_request_review_comment", "pull_request_target", "push",
    "registry_package", "release", "repository_dispatch", "schedule", "status", "watch",
    "workflow_call", "workflow_dispatch", "workflow_run",
}
spec = (root / "SPEC.md").read_text()
schema = json.loads((root / "schemas" / "job-offer.v1.schema.json").read_text())
known = set(schema["properties"]["trust_inputs"]["properties"]["event"]["enum"])
cased = set()
for case in cases:
    base = json.loads((root / "examples" / "valid" / case["offer"]).read_text())
    cased.add(case.get("override", {}).get("event", base["trust_inputs"]["event"]))
for ev in sorted(known - cased):
    bad += 1
    print(f"UNEXPECTED: event {ev} of the offer schema has no tier case")
block = re.search(r"```tier-rule\n(.*?)```", spec, re.S)
prose = re.search(r"```tier-prose\n(.*?)```", spec, re.S)
if not block:
    bad += 1
    print("UNEXPECTED: SPEC.md has no fenced block 'tier-rule'")
else:
    parsed = {}
    for line in block.group(1).splitlines():
        key, _, value = line.partition(":")
        parsed[key.strip()] = [v.strip() for v in value.split(",") if v.strip()]
    lists = (
        ("trusted_events", tier_rule.TRUSTED_EVENTS),
        ("untrusted_events", tier_rule.UNTRUSTED_EVENTS),
        ("refused_events", tier_rule.REFUSED_EVENTS),
    )
    for key, code in lists:
        if set(parsed.get(key, [])) != set(code):
            bad += 1
            print(f"UNEXPECTED: SPEC.md {key} {sorted(parsed.get(key, []))} differs from tools/tier_rule.py {sorted(code)}")
    named = set()
    for key, _ in lists:
        named |= set(parsed.get(key, []))
    if named != known:
        bad += 1
        print(f"UNEXPECTED: the events of the tier-rule block differ from the event enum of the offer schema: {sorted(named ^ known)}")
    both = set(parsed.get("refused_events", [])) & (set(parsed.get("trusted_events", [])) | set(parsed.get("untrusted_events", [])))
    if both:
        bad += 1
        print(f"UNEXPECTED: events both refused and tiered in the tier-rule block: {sorted(both)}")
    # The prose of section 6.3 outside the blocks names no event that the block does not.
    sec = re.search(r"^### 6\.3 .*?(?=^### )", spec, re.S | re.M)
    if not sec:
        bad += 1
        print("UNEXPECTED: SPEC.md has no section 6.3")
    else:
        outside = re.sub(r"```.*?```", "", sec.group(0), flags=re.S)
        for line in outside.splitlines():
            for tok in re.findall(r"`([a-z_]+)`", line):
                if (tok in known or tok in GITHUB_EVENTS) and tok not in named:
                    bad += 1
                    print(f"UNEXPECTED: SPEC.md section 6.3 prose names the event {tok}, which the tier-rule block does not: {line.strip()[:70]}")
if not prose:
    bad += 1
    print("UNEXPECTED: SPEC.md has no fenced block 'tier-prose'")
else:
    sections, current = {}, None
    for line in prose.group(1).splitlines():
        head = re.match(r"^(trusted|untrusted)\s+iff\b", line)
        if head:
            current = head.group(1)
            sections[current] = []
            continue
        cond = re.match(r"^\s+(\w+):\s", line)
        if cond and current:
            sections[current].append(cond.group(1))
        elif not line.startswith(" "):
            current = None
    for name, conds in (("trusted", tier_rule.TRUSTED_CONDITIONS), ("untrusted", tier_rule.UNTRUSTED_CONDITIONS)):
        got = sections.get(name, [])
        if sorted(got) != sorted(conds) or len(set(got)) != len(got):
            bad += 1
            print(f"UNEXPECTED: SPEC.md tier-prose conditions of {name} {sorted(got)} differ from tools/tier_rule.py {sorted(conds)}")
print(f"tier cases: {len(cases)}, unexpected: {bad}")
sys.exit(1 if bad else 0)
