The bootstrap of the repository: the workspace, the CI gate, the agent workflow, the spec and the beta release (GitHub #11 to #15).

# T-011: The Cargo workspace, the pinned toolchain and the static musl build (GitHub #11)

**Source:** GitHub #11
**Category:** chore
**Milestone:** M0
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

The text of GitHub #11 ("Bootstrap: Cargo workspace, pinned toolchain, static musl build"), cleaned:

**What and why.** The owner's one rule: "everything must be statically compiled". Each released binary is static musl. The repository holds only LICENSE (0BSD, "Copyright (c) 2026 PodUtils").

**Content.**

- Cargo.toml workspace, edition 2024, crates with the prefix `podrunner-` (the `pod-` prefix belongs to podcrates). Generic code goes to podcrates as a `pod-` crate on request.
- rust-toolchain.toml pinned to the same channel as the sibling repositories.
- `.cargo/config.toml`: `build.target = "x86_64-unknown-linux-musl"`, `+crt-static` per target, `zig cc` as the C compiler (as podvm and podcrates do).
- One binary `podrunner` with a `--version` that prints the version and the commit.
- README.md (what podrunner is, the status "not usable yet", the link to #1), SECURITY.md, THIRD_PARTY.md (empty table, ready for `actions/scaleset` MIT in #29), `.gitignore` with `.work/`, rustfmt.toml, `.editorconfig`.
- Targets of v1: x86_64 musl (the podvm userland rung is x86_64 only). aarch64 musl builds but serves no tier in v1.

**Crate dependencies (crate item R-C0 of the study, merged here).** `[workspace.dependencies]` names the podcrates crates that exist by git tag `v0.14.0` or newer: `pod-core` (error classes, the `Revoked` class, exit codes 64 to 80), `pod-identity`, `pod-proxy`, `pod-tls`, `pod-dns`, `pod-broker` and `pod-testkit` (feature `stand`). Each member crate takes one only when it needs it. The static probe of #12 covers each binary that links them. No podcrates ask is needed for these crates.

**Acceptance.**

- `cargo build --release` gives `target/x86_64-unknown-linux-musl/release/podrunner`.
- `readelf -l` of that binary shows no `INTERP` header; `file` says "statically linked".
- `cargo build --release --target aarch64-unknown-linux-musl` succeeds.
- The podcrates crates are pinned by a tag, not a branch.
- LICENSE is unchanged; each new file is 0BSD.

Dependencies: #3. Estimate (mine): S.

## Premise

Read the text of GitHub #11 on 2026-11-11. Nothing measured.

## Approach

A workspace of edition 2024 with the crates `podrunner` (the binary, `--version` with the commit) and `podrunner-todo`; `rust-toolchain.toml` 1.99 as podcrates; `.cargo/config.toml` with the default target `x86_64-unknown-linux-musl` and `+crt-static`; aarch64 with `cargo zigbuild`; `README.md`, `SECURITY.md`, `THIRD_PARTY.md`, `.editorconfig`. The crates of podcrates come by a tag; each member crate takes one when it needs it.

## Prove

```sh
cargo build --release
sh scripts/check-static.sh target/x86_64-unknown-linux-musl/release/podrunner
cargo zigbuild --release --target aarch64-unknown-linux-musl -p podrunner
sh scripts/check-static.sh target/aarch64-unknown-linux-musl/release/podrunner
```

## Done

2026-11-11, commit `9d2e4dd`. `cargo build --release` gives `target/x86_64-unknown-linux-musl/release/podrunner` (389816 bytes, static-pie); `sh scripts/check-static.sh` passes it; `podrunner --version` prints `podrunner 0.1.0 (COMMIT)`. `cargo zigbuild --release --target aarch64-unknown-linux-musl -p podrunner` gives a static binary (309224 bytes). The podcrates crate `pod-todo` is pinned by `tag = "v0.15.0"`. The gate run 42934750114 builds both targets and checks them.


# T-012: The CI gate: format, lint, tests, static probe, deny, file size (GitHub #12)

**Source:** GitHub #12
**Category:** chore
**Milestone:** M0
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

The text of GitHub #12 ("Bootstrap: CI gate (format, lint, tests, static probe, deny, file size)"), cleaned:

**What and why.** Each push to `main` must prove the build. Level 1 tests (#25) run in this gate on every push, on GitHub-hosted runners only. podrunner never builds its own release on a podrunner host: a lapsed runner image must not stop the build of the next one (rule S12 of the spec).

**Content.** .github/workflows/gate.yml with jobs:

- `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo test --workspace` (offline; no network in tests).
- "static build, no PT_INTERP": build the release binary and fail on an `INTERP` header.
- `cargo deny check` (licenses, advisories, sources).
- A file-size check: each Rust source file has at most 500 lines (the rule of podvm and podcrates).
- From #17 on: schema and example validation. From #29 on: a static check of the Go sidecar (`CGO_ENABLED=0`, no `INTERP`).
- A `fuzz.yml` that runs each fuzz target for a short time on a schedule (A-X-21). Each parser of job-controlled bytes gets a target in its own issue.
- Each action pinned by commit SHA. Each job has `timeout-minutes`.

**Acceptance.**

- A push to `main` runs the gate, and a run link is recorded in the issue.
- A planted defect fails each job: an unformatted line, a clippy warning, a failing test, a dynamic link (for example a `-C target-feature=-crt-static` build), a 501-line file. Each plant is reverted; the run links are recorded.

Dependencies: #11. Estimate (mine): S to M.

## Premise

Read the text of GitHub #12 on 2026-11-11. Nothing measured.

## Approach

`.github/workflows/gate.yml`: fmt, clippy with `-D warnings`, docs, the tests, the record, the size of each Rust file (`scripts/check-size.py`), no attribution line in a commit (`scripts/check-commit-msg.sh`), the static build of x86_64 and aarch64 musl with no `PT_INTERP` (`scripts/check-static.sh`, with a planted dynamic binary), cargo-deny with a planted denied license. `.github/workflows/secrets.yml`: TruffleHog with a planted key. Each action pinned by its commit; each job has `timeout-minutes`.

## Prove

The run of `.github/workflows/gate.yml` and `.github/workflows/secrets.yml` on the commit is green; by hand: `sh scripts/agent/check.sh`.

## Done

2026-11-11, commit `9d2e4dd`. The gate run 42934750114 is green: lint (fmt, clippy, docs, size with its self-test, the commit messages with 6 plants), test, record, static (x86_64 and aarch64 musl, no `PT_INTERP`, a planted dynamic `/bin/ls` fails), deny (a planted denied license fails). The secrets run 42934749863 is green (a planted key is found).


# T-013: The agent workflow, the decisions and the record (GitHub #13)

**Source:** GitHub #13
**Category:** chore
**Milestone:** M0
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

The text of GitHub #13 ("Bootstrap: agent workflow, decisions and record"), cleaned:

**What and why.** podrunner is worked by agent sessions, as podvm, podcrates, podnet and podssh are. A new repository needs the same frame.

**Content.**

- AGENTS.md (rules of the owner, git rules, the static rule, safety, the watcher) and CLAUDE.md that imports it.
- `.claude/settings.json` and `.claude/session-start.sh` (sets the git identity and the hooks path), copied from podcrates and adapted.
- `.githooks/commit-msg` that refuses agent attribution lines.
- docs/decisions.md: the rulings that apply to podrunner (static, compatibility, releases are betas with no crates.io, Q-A8, Q-A9, Priority, New projects) and the answers of #2 to #8, each with its date.
- docs/HANDOFF.md.
- TODO/: one entry for each open issue of this repository, with Problem, Premise, Approach and Prove, in the `pod-todo` format of podcrates; TODO/issues.md maps issue to entry.
- The organization watcher of podcrates (scripts/agent/watch-org.sh at a pinned commit) and COORD issue numbering `COORD-NNN: subject (to OTHER)`.

**Acceptance.**

- A fresh session runs `.claude/session-start.sh` and gets the identity and the hooks.
- A commit with an attribution line is refused by the hook.
- Each open issue of this repository maps to a TODO/ entry, and a record check in the gate fails on an issue with no entry.

Dependencies: #3, #12. Estimate (mine): M.

## Premise

Read the text of GitHub #13 on 2026-11-11. Nothing measured.

## Approach

Copy the frame of podcrates and adapt it: `AGENTS.md`, `CLAUDE.md`, `.claude/settings.json`, `.claude/session-start.sh` (identity, `core.hooksPath`), `.githooks/commit-msg`, `docs/decisions.md`, `docs/HANDOFF.md`, `docs/maintenance.md`, `docs/development.md`, this record with `pod-todo` of podcrates by its tag, and the watcher of the organization at a pinned commit.

## Prove

```sh
sh scripts/check-commit-msg.sh --self-test
cargo todo check
```

## Done

2026-11-11, commit `9d2e4dd`. `.claude/session-start.sh` sets the identity and `core.hooksPath` (run by hand: `git config core.hooksPath` gives `.githooks`). `sh scripts/check-commit-msg.sh --self-test`: 6 of 6 (a `Co-Authored-By`, a `Claude-Session` and a "Generated with" line are refused). Each of the 43 issues maps to an entry; `cargo todo check` agrees; the gate run 42934750114 runs it.


# T-014: The cleaned design spec in docs/ (GitHub #14)

**Source:** GitHub #14
**Category:** docs
**Milestone:** M0
**Priority:** P2
**Effort:** S
**Status:** open

## Problem

The text of GitHub #14 ("Bootstrap: carry the cleaned design spec into docs/"), cleaned:

**What and why.** The design exists only as a review package outside every repository. The issues of #1 are self-contained, but a builder needs the full design, the contracts and the measurement scripts. GitHub is public, so the copy must hold no host detail, no account detail and no private archive text.

**Content.** docs/design/: the spec, the appendices that the design needs (corrections, single points of failure, unknowns, asks, threats, flows, detail, deferred, existing, measurements), `contracts/`, `schemas/`, `examples/`, and the measurement scripts. The consistency checks of the package (`check_schemas`, `check_contracts`, `check_fields`, `check_codes`, `check_tiers`, `check_docs`) run in the gate (#12), or are replaced by Rust tests in #17 and #18.

**Remove or redact before the commit.**

- The host name of the measurement machine. It appears in `ownerName` lines of three M1 runner logs (`03-jitconfig.log`, `04-exit7.log`, `07-prestaged.log`) and twice in the podvm recon report.
- A statement about an account of the owner (two places in the spec).
- The default clone path in `measurements/m2-podvm/scripts/00-build.sh` (use a required variable).
- The recon report of the owner's private archive, unless the owner allows its publication.
- `inputs/` (copies of issue texts; link the issues), `review/`, `process/`, the proposed body of PodUtils/podcrates#47 and its edit trail (podcrates material; that issue is "leave alone"), `MANIFEST.sha256` (regenerate). Adjust `check_docs` and the self-test, which name `review/` and `process/`.

**Acceptance.**

- A grep over docs/design/ for the measurement host name, `/home/`, `/root/`, `/Users/` and the account statement finds nothing.
- The kept checks pass in CI, and a planted defect fails each.
- Each issue of #1 links its section in docs/design/.

Dependencies: #2, #3, #12. Estimate (mine): S to M.

## Premise

Read the text of GitHub #14 on 2026-11-11. Nothing measured.

## Approach

Put a cleaned copy of the spec (draft 5) into docs/spec/, with each id of the spec (DR-n, WP-n, Un, G-n, A-X-n) kept. The owner attached the zip on 2026-11-11 (`podrunner-spec/`: `SPEC.md`, the appendices, `schemas/`, `examples/`, the review rounds); the cleaning and the copy follow the list of the Problem.

## Prove

`cargo todo check`

# T-015: The beta release workflow (GitHub #15)

**Source:** GitHub #15
**Category:** release
**Milestone:** M-A
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The text of GitHub #15 ("Bootstrap: beta release workflow"), cleaned:

**What and why.** Ruling "Releases" (2026-11-09): betas when they fit, no crates.io. No release before M-A (Ph1.9 of PodUtils/podcrates#47). Signed releases are A-X-13 and wait for podcrates `pod-release` (PodUtils/podcrates@T-104, see #43).

**Content.** `release.yml` on a tag `vX.Y.Z-beta.N`: build the static binaries on a GitHub-hosted runner, check no `INTERP`, write SHA-256 sums, attach a build provenance attestation, publish a GitHub prerelease. Later: the Go sidecar binary (#29).

**Acceptance.**

- A dry run on a test tag produces the assets and the sums; the run link is recorded.
- `gh attestation verify --deny-self-hosted-runners` passes on the asset.
- No step publishes to crates.io.

Dependencies: #12, #25. Cross-repo: podcrates `pod-release` (PodUtils/podcrates@T-104) for signed indexes, later (#43). Estimate (mine): S.

Note: the draft put this issue in M0. It depends on #25 ("no release before M-A"), so its milestone is M-A.

## Premise

Read the text of GitHub #15 on 2026-11-11. Nothing measured.

## Approach

A workflow release.yml on a dispatch: the static build of both targets, the check of `scripts/check-static.sh`, a beta tag `vX.Y.Z-beta.N` and the release with the SHA-256 of each file and an attestation. No crates.io (ruling "Releases"). Done so far (2026-11-11): `.github/workflows/release.yml`: with no input, a dry run (the conditions, and the build and the static check of both binaries); with `X.Y.Z-beta.N`, the tag, a pre-release with the binaries, their SHA-256 and a build provenance attestation (`actions/attest-build-provenance` v4.2.2 by its commit). The gate must be green at the commit. Open: a dry run on GitHub, then the first beta.

## Prove

The run of the release workflow on a beta tag, and `sh scripts/check-static.sh` on each downloaded file.

## Done

2026-11-11. The dry run 42941366880 of `9ab7fd3` is green (the build
and the static check of both targets). The release run 42942338772 of
`6df0826` is green: the tag `v0.1.0-beta.1`, a pre-release with
`podrunner-x86_64-unknown-linux-musl` and
`podrunner-aarch64-unknown-linux-musl`, the SHA-256 of each, and a build
provenance attestation. The downloaded x86_64 file matches its SHA-256,
`sh scripts/check-static.sh` passes it (1361752 bytes), and `--version`
prints `podrunner 0.1.0 (6df082637281)`.
