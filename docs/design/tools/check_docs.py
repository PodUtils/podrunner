#!/usr/bin/env python3
"""Lint the documents and scripts of the spec directory.

Three file sets:
  text   every text file under the root except the work directory .work
         and __pycache__;
  lint   every *.md, *.sh and *.py under the root except recon/ and
         measurements/ (they hold evidence that quotes third-party text and
         guest paths, and are frozen);
  docs   the authored documents: *.md at the root and contracts/*.md.

Checks:
  1. text: no long dashes (em dash, en dash);
  2. lint: no spaced double hyphens and no absolute machine paths (workspace,
     home, tmp, Users, root roots). A ${TMPDIR:-...} default is allowed;
  3. docs: every relative file pointer in backticks that starts with a known
     directory (recon/, measurements/, contracts/, schemas/, examples/,
     tools/) names an existing file or directory;
  4. docs: every section pointer of the form "§N" or "§N.M" names an existing
     heading number of SPEC.md;
  5. SPEC.md carries the revision line "Revision: draft 5." and the date line,
     and is not longer than SPEC_WORD_CEILING words (wc -w);
  6. ids: every DR-n, WP-n (WP-Cn, WP-Vn, WP-Nn, with an optional letter
     suffix such as WP-05a), Un, Cn and Sn named in the docs is defined: DR and
     WP by a row or list item in SPEC.md, U in APPENDIX-UNKNOWNS.md, C in
     APPENDIX-CORRECTIONS.md, S in APPENDIX-SPOF.md. A WP id without its suffix
     is defined when any suffixed id of that number is. The WP numbers of each
     family are contiguous from 1, with no hole;
Exit 0 clean, 1 findings.
"""
import os
import re
import sys
from pathlib import Path

# A no-growth ratchet over the draft 5 text: the file must not get longer than this.
# The target of about 14000 words stays open (round 2 FIX-LOG, READ-2). Moving text to
# an appendix lowers the file and leaves this number alone.
SPEC_WORD_CEILING = 15250

root = Path(os.environ.get("PODRUNNER_SPEC_ROOT") or Path(__file__).resolve().parent.parent)
FROZEN = ("inputs", "recon", "measurements", "review")
SKIP_PARTS = {"__pycache__", ".work"}
DOUBLE_HYPHEN = " " + "-" * 2 + " "
ABS_PATH = re.compile(r"(?<![\w.])/(workspace|home|tmp|Users|root)/")
TMPDIR_DEFAULT = re.compile(r"\$\{TMPDIR:-[^}]*\}")

findings = []


def rel(path):
    return path.relative_to(root)


def top(path):
    return rel(path).parts[0]


def skipped(path):
    return any(part in SKIP_PARTS for part in rel(path).parts)


text_files, lint_files = [], []
for path in sorted(root.rglob("*")):
    if not path.is_file() or skipped(path) or top(path) == "inputs":
        continue
    try:
        path.read_text()
    except (UnicodeDecodeError, OSError):
        continue
    text_files.append(path)
    if path.suffix in (".md", ".sh", ".py") and top(path) not in FROZEN:
        lint_files.append(path)
docs = [p for p in lint_files if p.suffix == ".md"]

for path in text_files:
    for i, line in enumerate(path.read_text().splitlines(), 1):
        if "\u2014" in line or "\u2013" in line:
            findings.append(f"{rel(path)}:{i}: long dash")
for path in lint_files:
    for i, line in enumerate(path.read_text().splitlines(), 1):
        if DOUBLE_HYPHEN in line:
            findings.append(f"{rel(path)}:{i}: spaced double hyphen")
        if ABS_PATH.search(TMPDIR_DEFAULT.sub("", line)):
            findings.append(f"{rel(path)}:{i}: absolute path")

known = ("recon/", "measurements/", "contracts/", "schemas/", "examples/", "tools/")
for doc in docs:
    for m in re.finditer(r"`([A-Za-z0-9_./-]+)`", doc.read_text()):
        p = m.group(1)
        if p.startswith(known) and not (root / p.rstrip("/")).exists():
            findings.append(f"{rel(doc)}: pointer to missing path {p}")

spec_path = root / "SPEC.md"
spec = spec_path.read_text()
headings = set()
for line in spec.splitlines():
    m = re.match(r"^#{2,3} (\d+(?:\.\d+)?)[. ]", line)
    if m:
        headings.add(m.group(1))
for doc in docs:
    for m in re.finditer(r"§(\d+(?:\.\d+)?)", doc.read_text()):
        if m.group(1) not in headings:
            findings.append(f"{rel(doc)}: section pointer §{m.group(1)} has no heading")

if "Revision: draft 5." not in spec or "Date: 2026-11-10" not in spec:
    findings.append("SPEC.md: revision line 'draft 5' or date line 2026-11-10 is missing")
words = len(spec.split())
if words > SPEC_WORD_CEILING:
    findings.append(f"SPEC.md has {words} words, more than the ceiling {SPEC_WORD_CEILING}")


def read(name):
    p = root / name
    return p.read_text() if p.exists() else ""


defined = {"DR": set(), "WP": set(), "U": set(), "C": set(), "S": set()}
for m in re.finditer(r"^(?:\| |- )(DR-\d+)[ :|]", spec, re.M):
    defined["DR"].add(m.group(1))
for m in re.finditer(r"^\| (WP-[CVN]?\d+[a-z]?) \|", spec, re.M):
    defined["WP"].add(m.group(1))
    defined["WP"].add(re.sub(r"[a-z]$", "", m.group(1)))
for m in re.finditer(r"^\| (U\d+) \|", read("APPENDIX-UNKNOWNS.md"), re.M):
    defined["U"].add(m.group(1))
for m in re.finditer(r"^\| (C\d+) \|", read("APPENDIX-CORRECTIONS.md"), re.M):
    defined["C"].add(m.group(1))
for m in re.finditer(r"^\| (S\d+) \|", read("APPENDIX-SPOF.md"), re.M):
    defined["S"].add(m.group(1))

for family in ("WP-", "WP-C", "WP-V", "WP-N"):
    nums = sorted({int(x[len(family):]) for x in defined["WP"] if re.fullmatch(re.escape(family) + r"\d+", x)})
    if nums and nums != list(range(1, nums[-1] + 1)):
        findings.append(f"SPEC.md: work packages {family}nn are not contiguous from 1: {nums}")

patterns = (
    ("DR", re.compile(r"\b(DR-\d+)\b")),
    ("WP", re.compile(r"\b(WP-[CVN]?\d+[a-z]?)\b")),
    ("U", re.compile(r"\b(U\d+)\b")),
    ("C", re.compile(r"\b(C\d+)\b")),
    ("S", re.compile(r"\b(S\d+)\b")),
)
for doc in docs:
    if top(doc) == "process":
        continue
    for i, line in enumerate(doc.read_text().splitlines(), 1):
        if "is unused" in line:
            continue
        for family, pat in patterns:
            for m in pat.finditer(line):
                if m.group(1) not in defined[family]:
                    findings.append(f"{rel(doc)}:{i}: {m.group(1)} is not defined")

for f in findings:
    print(f)
print(f"docs checked: {len(docs)}, scripts and docs linted: {len(lint_files)}, text files scanned for long dashes: {len(text_files)}, findings: {len(findings)}")
sys.exit(1 if findings else 0)
