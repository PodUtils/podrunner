#!/usr/bin/env python3
# SPDX-License-Identifier: 0BSD
"""The checks of the boot of the hosted slice (T-582, T-556).

    python3 -I scripts/hosted/boot-checks.py --rc N --out DIR \\
        [--console FILE] [--state TEXT]
    python3 -I scripts/hosted/boot-checks.py --self-test

N is the exit code of `podvm run -d`. DIR is the directory that the
checks in the instance write (mounted at /out). FILE is the console log
of the instance (`incus console --show-log`). TEXT is the last status of
`podvm inspect`.

The script prints the job summary in Markdown: a table of the checks
that block, then the files of the checks (they only report). Each check
that did not hold goes to stderr as an error annotation of Actions.

Exit 0: each check holds. Exit 1: a check did not hold.
"""

import argparse
import pathlib
import sys
import tempfile

# The marker that checks.sh writes to the console at its start.
MARKER = "checks: start"
# The files of the checks that only report, in the order of the summary.
REPORT = ("state", "pid1", "kernel", "os", "machine-id", "cap", "ssh-keys", "failed")


def read(path: pathlib.Path) -> str | None:
    """The text of a file, or None when it is absent or unreadable."""
    try:
        return path.read_text(errors="replace")
    except OSError:
        return None


def checks(rc: int, out: pathlib.Path, console: pathlib.Path | None) -> list[tuple[str, bool, str]]:
    """Each check that blocks: its name, whether it holds, and why."""
    started = read(out / "started")
    trace = read(out / "trace")
    log = read(console) if console else None
    marker = log is not None and MARKER in log
    if log is None:
        why = "no console log"
    elif marker:
        why = f"`{MARKER}` in the console log"
    else:
        why = f"no `{MARKER}` in the console log"
    return [
        ("podvm run -d exits 0", rc == 0, f"exit {rc}"),
        ("/out/started is present", bool(started and started.strip()),
         started.strip() if started and started.strip() else "absent or empty"),
        ("/out/trace is present", bool(trace and trace.strip()),
         f"{len(trace.splitlines())} lines" if trace and trace.strip() else "absent or empty"),
        ("the console has the marker", marker, why),
    ]


def summary(rc: int, out: pathlib.Path, console: pathlib.Path | None, state: str, w, err) -> int:
    """Write the summary to w and an annotation for each fault to err;
    give 1 when a check did not hold."""
    results = checks(rc, out, console)
    faults = [name for name, ok, _ in results if not ok]
    w.write("## The boot of the core slice\n\n")
    w.write(f"The instance at its end: {state or '(none)'}\n\n")
    w.write("| Check | Result | Detail |\n|---|---|---|\n")
    for name, ok, why in results:
        w.write(f"| {name} | {'pass' if ok else '**FAIL**'} | {why.replace('|', '/')} |\n")
    w.write("\n")
    if faults:
        w.write("Checks that did not hold: " + "; ".join(faults) + ".\n")
        for name in faults:
            err.write(f"::error title=hosted-boot::the check did not hold: {name}\n")
    else:
        w.write("Each check holds.\n")
    w.write("\n### The files of the checks (report only)\n")
    for f in (*REPORT, "trace"):
        text = read(out / f)
        w.write(f"\n#### {f}\n\n```\n{text.rstrip() if text else '(none)'}\n```\n")
    return 1 if faults else 0


def self_test() -> int:
    import io

    faults = []
    good = {"started": "checks: start as 0 in /\n", "trace": "+ systemctl is-system-running\n",
            "state": "running\n", "pid1": "systemd\n"}
    console = "systemd starting\nchecks: start as 0 in /\nchecks: end\n"

    def case(name, rc, files, log, want_rc, want, absent=()):
        n = len(faults)
        with tempfile.TemporaryDirectory() as d:
            out = pathlib.Path(d, "out")
            out.mkdir()
            for rel, text in files.items():
                (out / rel).write_text(text)
            con = None
            if log is not None:
                con = pathlib.Path(d, "console.log")
                con.write_text(log)
            w, err = io.StringIO(), io.StringIO()
            got_rc = summary(rc, out, con, "Exited 0", w, err)
        got = w.getvalue() + err.getvalue()
        if got_rc != want_rc:
            faults.append(f"{name}: exit {got_rc}, not {want_rc}")
        for s in want:
            if s not in got:
                faults.append(f"{name}: no {s!r} in:\n{got}")
        for s in absent:
            if s in got:
                faults.append(f"{name}: {s!r} in:\n{got}")
        print(f"{'ok' if len(faults) == n else 'FAULT'} {name}")

    case("a good boot", 0, good, console, 0,
         ["Each check holds.", "| podvm run -d exits 0 | pass | exit 0 |", "#### pid1\n\n```\nsystemd\n```"],
         ["**FAIL**", "::error"])
    case("rc 125 and no trace", 125, {}, None, 1,
         ["| podvm run -d exits 0 | **FAIL** | exit 125 |", "| /out/trace is present | **FAIL** |",
          "| /out/started is present | **FAIL** |", "| the console has the marker | **FAIL** | no console log |",
          "Checks that did not hold: podvm run -d exits 0; /out/started is present; /out/trace is present;",
          "::error title=hosted-boot::the check did not hold: podvm run -d exits 0", "#### state\n\n```\n(none)\n```"])
    case("no marker", 0, good, "systemd starting\nPower off\n", 1,
         ["| the console has the marker | **FAIL** | no `checks: start` in the console log |",
          "Checks that did not hold: the console has the marker."],
         ["podvm run -d exits 0 | **FAIL**"])
    case("an empty trace", 0, {**good, "trace": ""}, console, 1,
         ["| /out/trace is present | **FAIL** | absent or empty |"])
    for f in faults:
        print(f, file=sys.stderr)
    print(f"boot-checks: self-test {'failed' if faults else 'passed'}")
    return 1 if faults else 0


def main() -> int:
    p = argparse.ArgumentParser(description="The checks of the boot of the hosted slice (T-582).")
    p.add_argument("--rc", type=int)
    p.add_argument("--out")
    p.add_argument("--console")
    p.add_argument("--state", default="")
    p.add_argument("--self-test", action="store_true")
    a = p.parse_args()
    if a.self_test:
        return self_test()
    if a.rc is None or not a.out:
        p.error("give --rc and --out")
    con = pathlib.Path(a.console) if a.console else None
    return summary(a.rc, pathlib.Path(a.out), con, a.state, sys.stdout, sys.stderr)


if __name__ == "__main__":
    sys.exit(main())
