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
- The record: 98 entries, 0 open, 6 partial, 12 blocked, 80 done
  (`cargo todo check`). The last green `runner-image.yml` run is
  43242367198 (`d229b6b`). The runs of `5518055` (podvm beta.6, the held
  suite, the aarch64 check) are open: read them first.
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
  aarch64, and in a podvm unit on x86_64 (T-081). Its aarch64 podvm job
  is a measure; podvm beta.6 has the aarch64 musl interposer (T-051).
- The run report names `claim_to_first_step_s`, `job_images` and the exit
  reason `limit` (T-075, T-074).

## In flight

- podvm is not available now (the owner, 2026-11-12, by podcrates): do
  housekeeping only until a new session starts with podvm. No new feature
  work.
- The local mode (T-046, `podrunner run`, `docs/cli.md`): T-087 to T-094
  and T-097 are done. T-097: podcrates `v0.32.0` taken at `997b001`;
  `token()` builds `format()` from `Template::parts()`.
- T-095 (`--podnet`): run 43242367198 gave `ok denied` held and
  `failure denied` unheld, with `DOTNET_SYSTEM_NET_DISABLEIPV6=1` in each
  held unit. `5518055` runs the local suite held too; its green
  `runner-image.yml` run closes T-095 and T-046. A job ends at its
  `timeout-minutes`; a job with no result gives the output of its unit.
- T-077: with podnet `v0.4.0-beta.6` and no IPv6 workaround the held run
  hung: the runner's connect to `::ffff:127.0.0.1` was refused (run
  43239962509). podnet fixes it in `v0.4.0-beta.7` (PodUtils/podnet#509,
  PodUtils/podnet@T-317). When it is out: pin it, remove the variable from
  `crates/podrunner-local/src/exec.rs` and `held.rs`, and run.
- T-096: `tar -xzf F -C /ABS` fails in a podvm unit; a relative `-C` and
  each `cd` form pass (run 43242367198). GitHub #70 (COORD-012) sends it
  to podcrates. `5518055` also measures an absolute `-C` with no `-z`.
- T-051: podvm `v0.2.0-beta.6` is pinned; the aarch64 start of
  `runner-image.yml` is a check. A green aarch64 job closes T-051, #52
  and PodUtils/podvm#286.
- Q4 (T-098) waits for the owner.
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
| podnet | `v0.4.0-beta.7`: `::ffff:127.0.0.1` dialed direct in the seccomp hold (GitHub #69, PodUtils/podnet#509) | T-077 |

## Open issues and asks

- GitHub #69 (COORD-011, to podnet): the fix of #64 did not hold in a
  podvm unit with beta.6. podnet answered (PodUtils/podnet#509): the fix
  comes in `v0.4.0-beta.7`. T-077 closes on a held run with no
  `DOTNET_SYSTEM_NET_DISABLEIPV6` on that release.
- GitHub #70 (COORD-012, to podcrates): an absolute `tar -C` fails in a
  podvm unit (T-096, PodUtils/podcrates@T-328).
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
