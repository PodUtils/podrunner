# Handoff

The state of podrunner for a session with no memory of the last one. Read
this page first, then `AGENTS.md`, `TODO/PROGRESS.md` (the state, the work
order, the questions), `docs/maintenance.md`, `docs/development.md` and
`docs/decisions.md`. Keep this page current: remove a line when its work
is done.

## Where podrunner stands

- Pins: podcrates `v0.32.0` (`Cargo.toml`); podvm `v0.2.0-beta.6`
  (`images/podvm.toml`, the lowest version of `probe::LOWEST`); podnet
  `v0.4.0-beta.6` (`images/podnet.toml`); the official runner v2.338.2
  (`images/runner/runner.toml`). Last beta of podrunner: `v0.1.0-beta.1`.
- The record: 98 entries, 1 open, 6 partial, 12 blocked, 79 done
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
- A musl build of the official runner (`scripts/runner-musl.sh`,
  `runner-musl.yml`) runs the canned jobs in Alpine on x86_64 and
  aarch64, and in a podvm unit on x86_64 (T-081). podvm-aarch64 beta.5
  declines it: it has no musl interposer object either (T-051).
- The run report names `claim_to_first_step_s`, `job_images` and the exit
  reason `limit` (T-075, T-074).

## In flight

- The local mode (T-046, `podrunner run`, `docs/cli.md`): T-087 to T-094
  are done (runner-image run 43227187267: the local suite and the
  containers suite in podvm units). T-095 (`--podnet`): with podnet beta.6
  and no IPv6 workaround the runner's connect to `::ffff:127.0.0.1` is
  refused (the output of the unit, run 43239962509; GitHub #69, T-077
  blocked). Held units set `DOTNET_SYSTEM_NET_DISABLEIPV6=1` again; the
  next `runner-image.yml` run must give `ok denied`. A job ends at its
  `timeout-minutes`, and a job with no result gives the output of its unit.
  The local suite now runs held (`--podnet`); its green run closes T-095
  and T-046. T-096: an absolute `-C` of tar fails in a podvm unit; sent
  to podcrates (PodUtils/podcrates@T-328). Q4 (T-098) waits for the owner.
- The gate queues for many minutes, and a newer push cancels a queued
  run. Push less often. Close an entry on the green run of a later commit
  that holds the work. A push that changes only `TODO/` or `docs/` runs
  the gate only; `runner-image.yml` and `level2.yml` run on their paths.

## What waits on whom

| Who | What | Entries |
| --- | --- | --- |
| The owner | DR-7: a test organization and App ("not now") | T-007, T-032, T-033, T-040 |
| podvm | none: beta.6 carries the aarch64 interposer; T-051 waits for its green aarch64 run | T-051, T-030 |
| podvm | the machine tier (PodUtils/podvm@T-1656) | T-037, T-036, T-039 |
| podcrates | `pod-unit`, `pod-release` (none planned for v1) | T-043 |
| actions/runner | a release with `Microsoft.AspNetCore.WebUtilities` 8.0.34 or later | T-082 |
| podnet | `::ffff:127.0.0.1` as loopback in the seccomp hold of a podvm unit (GitHub #69, COORD-011) | T-077 |

## Open issues and asks

- GitHub #69 (COORD-011, to podnet): the fix of #64 (podnet
  `v0.4.0-beta.5`, `e78bde974`) does not hold in a podvm unit with
  beta.6. T-077 is blocked; it closes on a held run with no
  `DOTNET_SYSTEM_NET_DISABLEIPV6` on the release that podnet names.
- GitHub #65, #66, #67: the trackers of the asks of cfb, githome and the
  CI plan that podrunner owns (from PodUtils/podcrates#35, #36, #47).
  Their lines map to the entries; T-001 holds them.
- GitHub #52 (COORD-005, to podvm): open, T-051; podvm `v0.2.0-beta.6` is pinned (PodUtils/podvm#286). Close both on a green aarch64 run.
- The probe of podvm names the mode of the host; the `--report` of a run
  names its rung (GitHub #68, closed; T-085).

## Talk between sessions

- podcrates: a COORD issue of this repository titled
  `COORD-NNN: subject (to podcrates)`; no direct message (the owner,
  2026-11-12, `docs/decisions.md`). Each ask for a crate, each take of a
  tag and each milestone report goes there.
- podnet: a COORD issue of this repository titled
  `COORD-NNN: subject (to podnet)`. podnet answers in its repository with
  `(to podrunner)`, and the watcher finds it.
- podvm: answers through COORD issues `(to podrunner)` in its repository
  and through the hand-offs of podcrates.
- The design copy is `docs/design/` (call C17: do not edit it).
  `docs/design-issues.md` maps each issue of the plan to its sections.
