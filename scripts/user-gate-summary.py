#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""The summary of the user-path gate (T-243, T-528).

  user-gate-summary.py [--lanes A,B] DIR
      read each results.tsv under DIR (one for each lane, from
      `user-gate.py lane`) and print Markdown: one table check x lane,
      the why of each fail and xfail, and one table row x lane with the
      times of each row that has them (the seconds from the start of
      `run` to the first step, and the time of the install)
  user-gate-summary.py --self-test
      run the summary on a fixture of two lanes and check its output

A results.tsv has the columns lane, check, result, detail and times. The
times are NAME=N pairs, separated by commas, or `-`.

Exit 0: each check passed. Exit 1: a check failed or a lane has no
results.
"""

import argparse
import pathlib
import sys
import tempfile

MARK = {"pass": "pass", "xfail": "xfail", "skip": "skip", "fail": "**FAIL**"}
# The two times of a row (T-528), in the order of a cell.
TIMES = ("run-to-first-step-s", "install-s")


def parse_times(text: str) -> dict[str, int]:
    out = {}
    for pair in text.split(","):
        name, _, value = pair.partition("=")
        if name and value.isdigit():
            out[name] = int(value)
    return out


def read(root: pathlib.Path) -> tuple[dict, list[str]]:
    """lane -> check -> (result, detail, times), and the checks in order."""
    results: dict[str, dict[str, tuple[str, str, dict[str, int]]]] = {}
    checks: list[str] = []
    for path in sorted(root.rglob("results.tsv")):
        for line in path.read_text().splitlines()[1:]:
            lane, check, result, detail, times = (line.split("\t") + [""] * 5)[:5]
            results.setdefault(lane, {})[check] = (result, detail, parse_times(times))
            if check not in checks:
                checks.append(check)
    return results, checks


def time_cell(times: dict[str, int]) -> str:
    return " / ".join(f"{times[k]} s" if k in times else "-" for k in TIMES)


def summary(root: pathlib.Path, lanes: list[str], out) -> int:
    results, checks = read(root)
    lanes = lanes or sorted(results)
    print("## The user-path gate\n", file=out)
    print("| check | " + " | ".join(lanes) + " |", file=out)
    print("| --- |" + " --- |" * len(lanes), file=out)
    bad = 0
    for c in checks:
        cells = []
        for ln in lanes:
            r = results.get(ln, {}).get(c)
            cells.append(MARK.get(r[0], r[0]) if r else "-")
            bad += bool(r and r[0] == "fail")
        print(f"| `{c}` | " + " | ".join(cells) + " |", file=out)
    missing = [ln for ln in lanes if ln not in results]
    for ln in missing:
        print(f"\nThe lane {ln} gave no results.", file=out)
    notes = [(ln, c, d) for ln in lanes for c, (r, d, _) in results.get(ln, {}).items() if r in ("fail", "xfail")]
    if notes:
        print("\n| lane | check | why |\n| --- | --- | --- |", file=out)
        for ln, c, d in notes:
            print(f"| {ln} | `{c}` | {d.replace('|', '/')} |", file=out)
    timed = [c for c in checks if any(results.get(ln, {}).get(c, ("", "", {}))[2] for ln in lanes)]
    if timed:
        cols = [ln for ln in lanes if any(results.get(ln, {}).get(c, ("", "", {}))[2] for c in timed)]
        print("\n### The times of the rows\n", file=out)
        print("Each cell: the seconds from the start of `run` to the first step / the time of the install.\n", file=out)
        print("| row | " + " | ".join(cols) + " |", file=out)
        print("| --- |" + " --- |" * len(cols), file=out)
        for c in timed:
            cells = []
            for ln in cols:
                r = results.get(ln, {}).get(c)
                cells.append(time_cell(r[2]) if r and r[2] else "-")
            print(f"| `{c}` | " + " | ".join(cells) + " |", file=out)
    return 1 if bad or missing else 0


HEAD = "lane\tcheck\tresult\tdetail\ttimes\n"
PLAIN = HEAD + (
    "plain\thello:podvm\tpass\tok\t-\n"
    "plain\tcorpus:rbg-gate:podvm\tpass\tok, as on github.com\tinstall-s=14,run-to-first-step-s=28\n"
    "plain\tcorpus:rbg-gate:runner-dir\tpass\tok, as on github.com\tinstall-s=12,run-to-first-step-s=2\n"
)
# A results.tsv of four columns (no times) still reads.
INCUS = "lane\tcheck\tresult\tdetail\n" "serve-incus-kvm\thello:incus\tpass\tok\n"
INCUS_TIMED = HEAD + (
    "serve-incus-kvm\thello:incus\tpass\tok\t-\n"
    "serve-incus-kvm\tcorpus:rbg-gate:incus\tfail\tgithub.com: success | exit 1\trun-to-first-step-s=9\n"
)
WANT = """## The user-path gate

| check | plain | serve-incus-kvm |
| --- | --- | --- |
| `hello:podvm` | pass | - |
| `corpus:rbg-gate:podvm` | pass | - |
| `corpus:rbg-gate:runner-dir` | pass | - |
| `hello:incus` | - | pass |

### The times of the rows

Each cell: the seconds from the start of `run` to the first step / the time of the install.

| row | plain |
| --- | --- |
| `corpus:rbg-gate:podvm` | 28 s / 14 s |
| `corpus:rbg-gate:runner-dir` | 2 s / 12 s |
"""
WANT_TIMED = """
| row | plain | serve-incus-kvm |
| --- | --- | --- |
| `corpus:rbg-gate:podvm` | 28 s / 14 s | - |
| `corpus:rbg-gate:runner-dir` | 2 s / 12 s | - |
| `corpus:rbg-gate:incus` | - | 9 s / - |
"""


def self_test() -> int:
    import io

    faults = []

    def case(name, files, lanes, want_rc, want):
        n = len(faults)
        with tempfile.TemporaryDirectory() as d:
            for rel, text in files.items():
                p = pathlib.Path(d, rel)
                p.parent.mkdir(parents=True)
                p.write_text(text)
            out = io.StringIO()
            rc = summary(pathlib.Path(d), lanes, out)
        got = out.getvalue()
        if rc != want_rc:
            faults.append(f"{name}: exit {rc}, not {want_rc}")
        for w in want:
            if w not in got:
                faults.append(f"{name}: no {w!r} in:\n{got}")
        print(f"{'ok' if len(faults) == n else 'FAULT'} {name}")

    lanes = ["plain", "serve-incus-kvm"]
    case("two lanes pass", {"lane-plain/results.tsv": PLAIN, "lane-serve-incus-kvm/results.tsv": INCUS}, lanes, 0, [WANT])
    case("a timed row that failed", {"lane-plain/results.tsv": PLAIN, "lane-serve-incus-kvm/results.tsv": INCUS_TIMED},
         lanes, 1, [WANT_TIMED, "| `corpus:rbg-gate:incus` | - | **FAIL** |", "| serve-incus-kvm | `corpus:rbg-gate:incus` | github.com: success / exit 1 |"])
    case("a lane with no results", {"lane-plain/results.tsv": PLAIN}, lanes, 1, ["The lane serve-incus-kvm gave no results."])
    case("no times", {"lane-serve-incus-kvm/results.tsv": INCUS}, ["serve-incus-kvm"], 0, ["| `hello:incus` | pass |"])
    with tempfile.TemporaryDirectory() as d:
        pathlib.Path(d, "results.tsv").write_text(INCUS)
        out = io.StringIO()
        summary(pathlib.Path(d), [], out)
        if "### The times" in out.getvalue():
            faults.append("no times: the summary printed a table of times")
    for f in faults:
        print(f, file=sys.stderr)
    print(f"user-gate-summary: self-test {'failed' if faults else 'passed'}")
    return 1 if faults else 0


def main() -> int:
    p = argparse.ArgumentParser(description="The summary of the user-path gate (T-243).")
    p.add_argument("--lanes", default="")
    p.add_argument("--self-test", action="store_true")
    p.add_argument("dir", nargs="?")
    a = p.parse_args()
    if a.self_test:
        return self_test()
    if not a.dir:
        p.error("give DIR")
    return summary(pathlib.Path(a.dir), [x for x in a.lanes.split(",") if x], sys.stdout)


if __name__ == "__main__":
    sys.exit(main())
