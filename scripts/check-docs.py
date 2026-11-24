#!/usr/bin/env python3
# 0BSD
"""Check the documents for stale and narrative text.

  python3 -I scripts/check-docs.py [--self-test]

The scan reads README.md and each `.md` file under docs/, but not
docs/design/, docs/releases/ and docs/reports/ (frozen or dated records).
Rules (AGENTS.md section 5: no narrative, no history):

  handoff   docs/HANDOFF.md has at most HANDOFF_LIMIT lines, and no line
            has "the session of", "Resume here" or "Done in this session".
  narrative A line outside code spans, fenced blocks and HTML comments has
            a marker of history or story (NARRATIVE below). A line that
            ends with `<!-- ok: REASON -->` is exempt. In docs/decisions.md
            the text in double quotes is the words of the owner: it is not
            read.
  pin       A version next to the word "podcrates" is not the `tag` of
            podcrates in Cargo.toml; or a version 0.2.0-beta.N (0.4.0-beta.N)
            next to "pin" is not the `version` of images/podvm.toml
            (images/podnet.toml). One sentence is the scope.
  path      A `.github/workflows/NAME.yml` or a `scripts/PATH` that a
            document names does not exist.

A scan that reads fewer than MIN_FILES files is a fault: a scan of nothing
proves nothing.

--self-test runs the check on temporary trees with a planted fault for
each rule. It exits 0 only when the check finds each fault and passes the
control tree.

Exit 0: no fault. Exit 1: a fault, one line for each: FILE:LINE: rule: text.
"""

import re
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MIN_FILES = 5
HANDOFF = "docs/HANDOFF.md"
HANDOFF_LIMIT = 200
SKIP_DIRS = ("docs/design/", "docs/releases/", "docs/reports/")

HANDOFF_BAD = re.compile(r"the session of|Resume here|Done in this session")
NARRATIVE = re.compile(
    r"\bwe\b"
    r"|\bwas (?:added|fixed|made|found|taken)\b"
    r"|\bwere (?:added|fixed|made|found)\b"
    r"|\(was [0-9A-Za-z]"
    r"|\bused to\b"
    r"|\bno longer\b"
    r"|\bearlier\b"
    r"|\bat first\b"
    r"|\bfrom the next (?:release|beta|session)\b"
    r"|\bhas been\b"
    r"|\b(?:the )?session of\b",
    re.IGNORECASE,
)
OK_MARK = re.compile(r"<!--\s*ok:\s*\S.*?-->")
HTML_COMMENT = re.compile(r"<!--.*?-->")
CODE_SPAN = re.compile(r"(`+)(?:(?!\1).)+?\1")
QUOTED = re.compile(r'"[^"\n]*"|“[^”\n]*”')
SENTENCE = re.compile(r"(?<=[.!?:;])\s+")
PODCRATES_VER = re.compile(r"\bv?0\.\d+\.\d+\b(?!-|`?\s+(?:and|or) later)")
PIN_WORD = re.compile(r"\bpin(?:s|ned|ning)?\b", re.IGNORECASE)
PODVM_VER = re.compile(r"\b0\.2\.0-beta\.(\d+)\b")
PODNET_VER = re.compile(r"\b0\.4\.0-beta\.(\d+)\b")
USER_WORKFLOW = re.compile(r"\brun\s+$")  # a workflow of the user project
WORKFLOW = re.compile(r"\.github/workflows/([A-Za-z0-9_.-]+?\.ya?ml)\b")
SCRIPT = re.compile(r"(?<![A-Za-z0-9_./-])scripts/[A-Za-z0-9_./-]+(?![A-Za-z0-9_./*<>{$-])")


def doc_files(root: Path) -> list[str]:
    out = []
    if (root / "README.md").is_file():
        out.append("README.md")
    docs = root / "docs"
    if docs.is_dir():
        for p in docs.rglob("*.md"):
            rel = p.relative_to(root).as_posix()
            if not rel.startswith(SKIP_DIRS) and p.is_file():
                out.append(rel)
    return sorted(out)


def prose(lines: list[str]) -> list[tuple[int, str]]:
    """(line number, raw line) of each line outside fenced blocks."""
    out = []
    fence = None
    for i, raw in enumerate(lines, 1):
        m = re.match(r"\s*(`{3,}|~{3,})", raw)
        if fence:
            if m and m.group(1)[0] == fence[0] and len(m.group(1)) >= len(fence):
                fence = None
            continue
        if m:
            fence = m.group(1)
            continue
        out.append((i, raw))
    return out


def read_pin(path: Path, key: str) -> str | None:
    if not path.is_file():
        return None
    m = re.search(rf'^{key}\s*=\s*"([^"]+)"', path.read_text(encoding="utf-8"), re.M)
    return m.group(1) if m else None


def podcrates_tag(root: Path) -> str | None:
    cargo = root / "Cargo.toml"
    if not cargo.is_file():
        return None
    m = re.search(r'PodUtils/podcrates"[^}\n]*tag\s*=\s*"v?([0-9][^"]*)"', cargo.read_text(encoding="utf-8"))
    return m.group(1) if m else None


def check(root: Path, min_files: int) -> list[str]:
    files = doc_files(root)
    faults = []
    if len(files) < min_files:
        faults.append(f"docs: scan: {len(files)} files read, fewer than {min_files}: the scan read too little to pass")
    crates = podcrates_tag(root)
    podvm = read_pin(root / "images/podvm.toml", "version")
    podnet = read_pin(root / "images/podnet.toml", "version")
    for rel in files:
        lines = (root / rel).read_text(encoding="utf-8", errors="replace").splitlines()
        if rel == HANDOFF:
            if len(lines) > HANDOFF_LIMIT:
                faults.append(f"{rel}:{len(lines)}: handoff: {len(lines)} lines, over {HANDOFF_LIMIT}: cut it")
            for i, line in enumerate(lines, 1):
                m = HANDOFF_BAD.search(line)
                if m:
                    faults.append(f"{rel}:{i}: handoff: '{m.group(0)}': state, not a story")
        for i, raw in prose(lines):
            ok = bool(OK_MARK.search(raw))
            text = HTML_COMMENT.sub(" ", raw)
            text = CODE_SPAN.sub(" ", text)
            if rel == "docs/decisions.md":
                text = QUOTED.sub(" ", text)
            if not ok:
                m = NARRATIVE.search(text)
                if m:
                    faults.append(f"{rel}:{i}: narrative: '{m.group(0)}': {raw.strip()[:100]}")
            if not ok:
                faults += pins(rel, i, raw, crates, podvm, podnet)
        for i, raw in enumerate(lines, 1):
            faults += paths(root, rel, i, raw)
    return faults


def pins(rel, i, raw, crates, podvm, podnet) -> list[str]:
    """Stale versions. Code spans stay: a pin is often a code span."""
    out = []
    text = HTML_COMMENT.sub(" ", raw)
    for sent in SENTENCE.split(text):
        if crates and re.search(r"podcrates", sent, re.IGNORECASE):
            for m in PODCRATES_VER.finditer(sent):
                if m.group(0).lstrip("v") != crates:
                    out.append(f"{rel}:{i}: pin: podcrates {m.group(0)}, Cargo.toml has v{crates}")
        if PIN_WORD.search(sent):
            for pat, want, name in ((PODVM_VER, podvm, "podvm"), (PODNET_VER, podnet, "podnet")):
                if not want:
                    continue
                for m in pat.finditer(sent):
                    if m.group(0) != want:
                        out.append(f"{rel}:{i}: pin: {name} {m.group(0)}, images/{name}.toml has {want}")
    return out


def paths(root: Path, rel: str, i: int, raw: str) -> list[str]:
    out = []
    for m in WORKFLOW.finditer(raw):
        if USER_WORKFLOW.search(raw[: m.start()]):
            continue
        if not (root / ".github/workflows" / m.group(1)).is_file():
            out.append(f"{rel}:{i}: path: .github/workflows/{m.group(1)} does not exist")
    for m in SCRIPT.finditer(raw):
        name = m.group(0).rstrip(".,;:)")
        if name.endswith("/"):
            if not (root / name).is_dir():
                out.append(f"{rel}:{i}: path: {name} does not exist")
        elif not (root / name).exists():
            out.append(f"{rel}:{i}: path: {name} does not exist")
    return out


def tree(root: Path, files: dict[str, str]) -> None:
    for rel, body in files.items():
        p = root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(body, encoding="utf-8")


CONTROL = {
    "Cargo.toml": '[workspace.dependencies]\npod-core = { git = "https://github.com/PodUtils/podcrates", tag = "v0.82.1" }\n',
    "images/podvm.toml": 'version = "0.2.0-beta.39"\n',
    "images/podnet.toml": 'version = "0.4.0-beta.17"\n',
    ".github/workflows/gate.yml": "on: push\n",
    "scripts/check-size.py": "",
    "scripts/hosted/diff.py": "",
    "README.md": "Run `scripts/check-size.py`. See .github/workflows/gate.yml.\n",
    "docs/HANDOFF.md": "# State\n\nOne line.\n",
    "docs/a.md": "# A\n\nThe tool runs a job.\n",
    "docs/b.md": "# B\n\nPodcrates is at v0.82.1. The podvm pin is 0.2.0-beta.39.\n",
    "docs/c.md": "# C\n\nThe podnet pin is 0.4.0-beta.17. See scripts/hosted/. Needs podcrates `v0.16.0` and later.\n\n`podrunner run .github/workflows/ci.yml`\n",
    "docs/d.md": "# D\n\nDo not run `we added` this.\n\n```sh\nwe was added here\n```\n\nOne item <!-- ok: kept on purpose, we say -->\n",
    "docs/decisions.md": '# Decisions\n\nThe owner says "we use what is needed". The rule is set.\n',
    "docs/design/x.md": "we was added\n",
    "docs/releases/x.md": "we was added\n",
    "docs/reports/x.md": "we was added\n",
}


def self_test() -> int:
    def with_(files):
        def setup(root):
            tree(root, {**CONTROL, **files})
        return setup

    cases = [
        ("control", with_({}), None),
        ("an empty tree", lambda root: None, "fewer than 5"),
        ("a HANDOFF over the limit", with_({HANDOFF: "x\n" * (HANDOFF_LIMIT + 1)}),
         f"docs/HANDOFF.md:{HANDOFF_LIMIT + 1}: handoff: {HANDOFF_LIMIT + 1} lines"),
        ("a HANDOFF with a story", with_({HANDOFF: "# S\nResume here.\n"}), "docs/HANDOFF.md:2: handoff: 'Resume here'"),
        ("a HANDOFF with the session of", with_({HANDOFF: "# S\nFrom the session of x.\n"}), "docs/HANDOFF.md:2: handoff: 'the session of'"),
        ("narrative: we", with_({"docs/a.md": "# A\n\nThen we run it.\n"}), "docs/a.md:3: narrative: 'we'"),
        ("narrative: was added", with_({"docs/a.md": "# A\n\nThe flag was added.\n"}), "narrative: 'was added'"),
        ("narrative: were fixed", with_({"docs/a.md": "# A\n\nBugs were fixed.\n"}), "narrative: 'were fixed'"),
        ("narrative: (was x)", with_({"docs/a.md": "# A\n\nIt is 5 (was 3).\n"}), "narrative: '(was 3'"),
        ("narrative: used to", with_({"docs/a.md": "# A\n\nIt used to fail.\n"}), "narrative: 'used to'"),
        ("narrative: no longer", with_({"docs/a.md": "# A\n\nIt is no longer set.\n"}), "narrative: 'no longer'"),
        ("narrative: earlier", with_({"docs/a.md": "# A\n\nAn earlier run.\n"}), "narrative: 'earlier'"),
        ("narrative: at first", with_({"docs/a.md": "# A\n\nAt first it fails.\n"}), "narrative: 'At first'"),
        ("narrative: from the next beta", with_({"docs/a.md": "# A\n\nFrom the next beta it works.\n"}), "narrative: 'From the next beta'"),
        ("narrative: has been", with_({"docs/a.md": "# A\n\nIt has been set.\n"}), "narrative: 'has been'"),
        ("narrative: session of", with_({"docs/a.md": "# A\n\nAsk the session of x.\n"}), "narrative: 'the session of'"),
        ("narrative in README", with_({"README.md": "We build it.\n"}), "README.md:1: narrative: 'We'"),
        ("narrative outside the owner's quote", with_({"docs/decisions.md": '# D\n\n"We use it." Then we ran it.\n'}), "docs/decisions.md:3: narrative: 'we'"),
        ("a marker with no reason", with_({"docs/a.md": "# A\n\nWe run it <!-- ok: -->\n"}), "narrative: 'We'"),
        ("a stale podcrates tag", with_({"docs/b.md": "# B\n\nPodcrates is at v0.80.0.\n"}), "docs/b.md:3: pin: podcrates v0.80.0, Cargo.toml has v0.82.1"),
        ("a stale podcrates tag in a code span", with_({"docs/b.md": "# B\n\nThe podcrates tag is `v0.80.0`.\n"}), "pin: podcrates v0.80.0"),
        ("a stale podvm pin", with_({"docs/b.md": "# B\n\nThe pinned podvm is 0.2.0-beta.12.\n"}), "docs/b.md:3: pin: podvm 0.2.0-beta.12, images/podvm.toml has 0.2.0-beta.39"),
        ("a stale podnet pin", with_({"docs/c.md": "# C\n\nThe pinned podnet is `0.4.0-beta.3`.\n"}), "docs/c.md:3: pin: podnet 0.4.0-beta.3"),
        ("a missing workflow", with_({"docs/a.md": "# A\n\nSee `.github/workflows/gone.yml`.\n"}), "docs/a.md:3: path: .github/workflows/gone.yml does not exist"),
        ("a missing script", with_({"docs/a.md": "# A\n\nRun `sh scripts/gone.sh` now.\n"}), "docs/a.md:3: path: scripts/gone.sh does not exist"),
        ("a missing script directory", with_({"docs/a.md": "# A\n\nSee scripts/nodir/.\n"}), "path: scripts/nodir/ does not exist"),
    ]
    failed = 0
    for name, setup, expect in cases:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            setup(root)
            faults = check(root, MIN_FILES)
        ok = not faults if expect is None else any(expect in f for f in faults)
        print(f"{'ok  ' if ok else 'FAIL'}  {name}: {faults if faults else 'no fault'}")
        failed += not ok
    return 1 if failed else 0


def main(argv: list[str]) -> int:
    if argv == ["--self-test"]:
        return self_test()
    if argv:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    faults = check(ROOT, MIN_FILES)
    for f in faults:
        print(f)
    print(f"check-docs: {len(doc_files(ROOT))} files, {len(faults)} faults", file=sys.stderr)
    return 1 if faults else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
