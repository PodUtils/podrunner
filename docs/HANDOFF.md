# Handoff

The state of podrunner for a session with no memory of the last one. Read
this page first, then `AGENTS.md`, `TODO/PROGRESS.md` (the state, the work
order, the questions), `docs/maintenance.md`, `docs/development.md` and
`docs/decisions.md`. Keep this page current: remove a line when its work
is done.

## Where podrunner stands

- Pins: podcrates `v0.28.0` (`Cargo.toml`); podvm `v0.2.0-beta.5`
  (`images/podvm.toml`, the lowest version of `probe::LOWEST`); podnet
  `v0.4.0-beta.3` (`images/podnet.toml`); the official runner v2.338.2
  (`images/runner/runner.toml`). Last beta of podrunner: `v0.1.0-beta.1`.
- The record: 82 entries, 0 open, 4 partial, 12 blocked, 66 done
  (`cargo todo check`). `main` is green: gate, level2 and runner-image.
- M-A, M-B and the work packages of M-C and M-D are done. The open
  milestone gates are T-032 and T-033 (DR-7: no live account), T-039
  (a real guest: T-036, T-037) and T-040 (DR-7).
- The podvm driver runs the official runner in a unit with no root: a
  canned job and a `container:` job through the hooks pass in
  `runner-image.yml` (T-026, T-035). The unit conformance suite passes on
  the fake unit and on the podvm driver.
- Each podvm unit and container carries the label `podrunner.unit=ID`,
  and `destroy` removes each one with it; the `run -d` of a unit writes
  podvm's report (`run_report`) (T-078).
- The egress hold: with `[unit.podvm] podnet`, a podvm unit runs under
  `podnet pod`: the netns hold (`enforced`) where user namespaces and
  `/dev/net/tun` exist, else the seccomp hold (`advisory`) (T-065, T-041).
  A held runner reaches an allowed host and not a denied one
  (`crates/podrunner-broker-fake/tests/held.rs`).
- The `confined` boundary: the fake applies `pod-confine` through its
  shim; the podvm kind with `[unit.podvm] confine = true` runs each unit
  and container under `podvm --podvm-confine`, and says `confined` only
  after its self-test on the host (T-042, T-079). The layer denies the
  `/proc` paths of residual R2 and `env -i`, so tests 25 and 26 skip on a
  confined podvm unit (T-080).
- The run report names `claim_to_first_step_s`, `job_images` and the exit
  reason `limit` (T-075, T-074).

## In flight

- T-081 (partial): `scripts/runner-musl.sh` builds a musl layout of the
  runner; `.github/workflows/runner-musl.yml` runs it in Alpine and in a
  podvm unit. Read its last run, then close T-081 or write each failure.
- T-082 (partial): the job `audit` of `runner-image.yml` holds the
  advisories of the runner to `images/runner/advisories.txt`. Read its
  run.
- The gate queues for many minutes, and a newer push cancels a queued
  run. Push less often. Close an entry on the green run of a later commit
  that holds the work. A push that changes only `TODO/` or `docs/` runs
  the gate only; `runner-image.yml` and `level2.yml` run on their paths.

## What waits on whom

| Who | What | Entries |
| --- | --- | --- |
| The owner | DR-7: a test organization and App ("not now") | T-007, T-032, T-033, T-040 |
| podvm | the aarch64 interposer (PodUtils/podvm@T-1721); beta.5 still declines the runner image on aarch64 | T-051, T-030 |
| podvm | the machine tier (PodUtils/podvm@T-1656) | T-037, T-036, T-039 |
| podcrates | `pod-actions-workflow`, `pod-actions-expr` and the expansion of jobs (asked; PodUtils/podcrates@T-251) | T-046 |
| podcrates | `pod-unit`, `pod-release` (none planned for v1) | T-043 |
| podnet | `::ffff:127.0.0.1` as loopback in the seccomp hold (GitHub #64, COORD-009) | T-077 |

## Open issues and asks

- GitHub #64 (COORD-009, to podnet): open. The held test sets
  `DOTNET_SYSTEM_NET_DISABLEIPV6=1` until a fix.
- GitHub #65, #66, #67: the trackers of the asks of cfb, githome and the
  CI plan that podrunner owns (from PodUtils/podcrates#35, #36, #47).
  Their lines map to the entries; T-001 holds them.
- GitHub #52 (COORD-005, to podvm): open, T-051.
- podcrates: the crate ask of T-046 is PodUtils/podcrates@T-251. A note for podvm, sent
  through podcrates: `podvm run --help` says that only a rung with a
  chroot or the loader family takes `--podvm-confine`; the supervise rung
  takes it too.

## Talk between sessions

- podcrates: direct messages (`send_message`) to its session
  (`list_sessions`, title with `(podcrates)`). Each ask for a crate goes
  there; podcrates answers with the id of its entry and releases a tag.
  After each milestone, send a short report: the commit, the gate run,
  the next step. This session cannot write to the issues of other
  repositories: podcrates notes each take of a tag on its COORD issue.
- podnet: a COORD issue of this repository titled
  `COORD-NNN: subject (to podnet)`. podnet answers in its repository with
  `(to podrunner)`, and the watcher finds it.
- podvm: answers through COORD issues `(to podrunner)` in its repository
  and through the hand-offs of podcrates.
- The design copy is `docs/design/` (call C17: do not edit it).
  `docs/design-issues.md` maps each issue of the plan to its sections.
