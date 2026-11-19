#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""Compare the probe of a hosted label with the probe of a unit (T-539).

    python3 -I scripts/hosted/diff.py --allow images/hosted/allow.toml \\
        [--report-only] [--summary FILE] --pair REF UNIT NAME [--pair ...]
    python3 -I scripts/hosted/diff.py --self-test

A probe is the output of scripts/hosted/probe.sh: one "KEY<TAB>VALUE" line
for each fact. Each difference (a key that is missing in the unit, a key
that only the unit has, a changed value) needs a row of the allow list
whose `key` glob, `change` and `side` glob match it. A row that matches no
difference of the run is stale and fails too, so the list only shrinks.
The version of a tool (`tool.*`) is compared at major.minor: the hosted
image moves its patch versions each week.
"""

import argparse
import fnmatch
import pathlib
import re
import sys
import tomllib

CHANGES = ("missing", "extra", "changed", "any")

# The rows of the report: the rest is counted, so a summary stays small.
SHOWN = 400


def parse(text: str) -> dict[str, str]:
    out = {}
    for line in text.splitlines():
        if not line.strip():
            continue
        key, _, value = line.partition("\t")
        out[key] = value
    return out


def norm(key: str, value: str) -> str:
    if key.startswith("tool."):
        m = re.match(r"(\d+)(\.\d+)?", value)
        if m:
            return m.group(1) + (m.group(2) or "")
    return value


def differences(ref: dict[str, str], unit: dict[str, str]) -> list[tuple[str, str, str, str]]:
    out = []
    for key in sorted(set(ref) | set(unit)):
        if key not in unit:
            out.append((key, "missing", ref[key], ""))
        elif key not in ref:
            out.append((key, "extra", "", unit[key]))
        elif norm(key, ref[key]) != norm(key, unit[key]):
            out.append((key, "changed", ref[key], unit[key]))
    return out


def load_allow(text: str) -> list[dict]:
    rows = tomllib.loads(text).get("allow", [])
    for i, r in enumerate(rows):
        for field in ("key", "change", "side", "reason", "entry"):
            if not isinstance(r.get(field), str) or not r[field]:
                raise ValueError(f"allow row {i + 1}: no {field}")
        if r["change"] not in CHANGES:
            raise ValueError(f"allow row {i + 1}: change {r['change']!r} is not one of {CHANGES}")
        if not re.fullmatch(r"T-\d{3,}", r["entry"]):
            raise ValueError(f"allow row {i + 1}: entry {r['entry']!r} is not T-NNN")
    return rows


def judge(pairs: list[tuple[str, dict, dict]], rows: list[dict]):
    """Each difference with the row that allows it (or None), and the stale rows."""
    used = [False] * len(rows)
    found = []
    for name, ref, unit in pairs:
        for key, change, was, now in differences(ref, unit):
            hit = None
            for i, r in enumerate(rows):
                if (
                    fnmatch.fnmatchcase(key, r["key"])
                    and r["change"] in (change, "any")
                    and fnmatch.fnmatchcase(name, r["side"])
                ):
                    hit = i
                    used[i] = True
                    break
            found.append((name, key, change, was, now, hit))
    stale = [rows[i] for i, u in enumerate(used) if not u]
    return found, stale


def report(found, stale, rows) -> tuple[str, int]:
    bad = [f for f in found if f[5] is None]
    lines = [
        f"{len(found)} differences: {len(found) - len(bad)} allowed, {len(bad)} not listed; {len(stale)} stale rows.",
        "",
    ]
    if bad:
        by: dict[tuple[str, str, str], int] = {}
        for n, k, c, *_ in bad:
            by[(n, k.split(".")[0], c)] = by.get((n, k.split(".")[0], c), 0) + 1
        lines += ["| Side | Kind of key | Change | Count |", "| --- | --- | --- | --- |"]
        lines += [f"| {n} | `{p}.*` | {c} | {m} |" for (n, p, c), m in sorted(by.items())]
        lines += ["", "| Side | Key | Change | Hosted | Unit |", "| --- | --- | --- | --- | --- |"]
        lines += [f"| {n} | `{k}` | {c} | `{w[:60]}` | `{v[:60]}` |" for n, k, c, w, v, _ in bad[:SHOWN]]
        if len(bad) > SHOWN:
            lines.append(f"| … | {len(bad) - SHOWN} more | | | |")
        lines.append("")
    for r in stale:
        lines.append(f"stale row: key {r['key']!r} change {r['change']} side {r['side']!r} ({r['entry']})")
    counts: dict[str, int] = {}
    for f in found:
        if f[5] is not None:
            r = rows[f[5]]
            counts[f"{r['key']} ({r['entry']})"] = counts.get(f"{r['key']} ({r['entry']})", 0) + 1
    if counts:
        lines += ["", "Allowed, by row:"] + [f"- `{k}`: {n}" for k, n in sorted(counts.items())]
    return "\n".join(lines) + "\n", len(bad) + len(stale)


def self_test() -> int:
    ref = parse(
        "tool.node\t22.11.0\ntool.go\t1.24.7\ncap./usr/bin/ping\tcap_net_raw=ep\n"
        "systemd.state\trunning\nenv.PATH\t/usr/bin:/bin\n"
    )
    cases = {
        "a removed tool": (ref, "tool.go", "missing"),
        "a lost capability": (ref, "cap./usr/bin/ping", "missing"),
        "a failed unit": (ref, "systemd.failed.x.service", "extra"),
        "a changed major version": (ref, "tool.node", "changed"),
        "an unlisted variable": (ref, "env.SOME_NAME", "extra"),
    }
    unit_of = {
        "a removed tool": {k: v for k, v in ref.items() if k != "tool.go"},
        "a lost capability": {k: v for k, v in ref.items() if not k.startswith("cap.")},
        "a failed unit": ref | {"systemd.failed.x.service": "failed"},
        "a changed major version": ref | {"tool.node": "24.1.0"},
        "an unlisted variable": ref | {"env.SOME_NAME": "set"},
    }
    bad = []
    for what, (r, key, change) in cases.items():
        found, stale = judge([("unit", r, unit_of[what])], [])
        if [(f[1], f[2]) for f in found] != [(key, change)] or report(found, stale, [])[1] != 1:
            bad.append(f"{what}: {found}")
    # A patch version is no difference; a listed difference passes; a row
    # that matches nothing is stale.
    found, _ = judge([("unit", ref, ref | {"tool.node": "22.11.5"})], [])
    if found:
        bad.append(f"a patch version is a difference: {found}")
    rows = load_allow(
        '[[allow]]\nkey = "tool.go"\nchange = "missing"\nside = "un*"\nreason = "r"\nentry = "T-539"\n'
        '[[allow]]\nkey = "dpkg.*"\nchange = "any"\nside = "*"\nreason = "r"\nentry = "T-539"\n'
    )
    found, stale = judge([("unit", ref, unit_of["a removed tool"])], rows)
    _, n = report(found, stale, rows)
    if found[0][5] != 0 or [s["key"] for s in stale] != ["dpkg.*"] or n != 1:
        bad.append(f"the allow list: {found} {stale}")
    for text in ('[[allow]]\nkey = "x"\n', '[[allow]]\nkey="x"\nchange="gone"\nside="*"\nreason="r"\nentry="T-1"\n'):
        try:
            load_allow(text)
            bad.append(f"a bad row is taken: {text!r}")
        except ValueError:
            pass
    for b in bad:
        print(f"self-test: {b}")
    if not bad:
        print("diff.py: self-test passes")
    return 1 if bad else 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--allow", type=pathlib.Path)
    ap.add_argument("--report-only", action="store_true")
    ap.add_argument("--summary", type=pathlib.Path)
    ap.add_argument("--pair", nargs=3, action="append", metavar=("REF", "UNIT", "NAME"), default=[])
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    if not a.allow or not a.pair:
        ap.error("--allow and one --pair at least")
    rows = load_allow(a.allow.read_text())
    pairs = [(n, parse(pathlib.Path(r).read_text()), parse(pathlib.Path(u).read_text())) for r, u, n in a.pair]
    found, stale = judge(pairs, rows)
    text, n = report(found, stale, rows)
    print(text, end="")
    if a.summary:
        with a.summary.open("a") as f:
            f.write("## Hosted parity\n\n" + text)
    return 0 if a.report_only else (1 if n else 0)


if __name__ == "__main__":
    sys.exit(main())
