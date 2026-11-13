# Handoff

The state of podrunner for a session with no memory of the last one. Read
this page first, then `AGENTS.md`, `TODO/PROGRESS.md` (the state, the work
order, the questions), `docs/maintenance.md`, `docs/development.md` and
`docs/decisions.md`. Keep this page current: remove a line when its work
is done.

## Where podrunner stands

- Pins: podcrates `v0.40.0` (`Cargo.toml`); podvm `v0.2.0-beta.13`
  (`images/podvm.toml`; the podvm kind takes `probe::LOWEST` and later,
  the microvm and incus kinds need beta.10, T-110); podnet
  `v0.4.0-beta.8` (`images/podnet.toml`); the official runner v2.338.2
  (`images/runner/runner.toml`). Last beta of podrunner: `v0.1.0-beta.3`
  (2026-11-13, release run 43461852923, `e8795f2`); githome and the
  live-gate agents (#74, #75) are told.
- The record: 111 entries, 3 open, 7 partial, 7 blocked, 94 done
  (`cargo todo check`). The last green `runner-image.yml` run is
  43454398354 (`e8795f2`): the podvm suites, the held tests, the 28 tar
  forms, the microvm kind on real guests and the incus suite on real
  instances.
- M-A, M-B and the work packages of M-C and M-D are done. The open
  milestone gates are T-032, T-033 and T-040 (the live gates: GitHub #74
  and #75) and T-039 (after T-036 and T-037).
- The podvm driver runs the official runner in a unit with no root: a
  canned job and a `container:` job through the hooks pass in
  `runner-image.yml` (T-026, T-035). The unit conformance suite passes on
  the fake unit, the podvm driver, the microvm kind and the incus kind.
- Each podvm unit and container carries the label `podrunner.unit=ID`,
  and `destroy` removes each one with it; the `run -d` of a unit writes
  podvm's report (`run_report`) (T-078).
- The egress hold: with `[unit.podvm] podnet`, a podvm unit runs under
  `podnet pod`: the netns hold (`enforced`) where user namespaces and
  `/dev/net/tun` exist, else the seccomp hold (`advisory`) (T-065, T-041).
  A held runner reaches an allowed host and not a denied one
  (`crates/podrunner-broker-fake/tests/held.rs`).
- The egress policy of a job: the policy of its offer narrowed by
  `[egress] default_policy` (T-107). slots gives each driver its socket
  directory.
- The `confined` boundary: the fake applies `pod-confine` through its
  shim; the podvm kind with `[unit.podvm] confine = true` runs each unit
  and container under `podvm --podvm-confine`, and says `confined` only
  after its self-test on the host (T-042, T-079). The layer denies the
  `/proc` paths of residual R2 and `env -i`, so tests 25 and 26 skip on a
  confined podvm unit (T-080).
- The `microvm` kind (`crates/podrunner-microvm`, `[unit.microvm]`): one
  guest of podvm's machine tier for each job, with no NIC and one channel
  to the egress socket of the unit (`PODRUNNER_CHANNEL_PORT`; the guest
  script sets the proxy variables). Its 11 unit tests, the guest self-test
  (which compares the boot id and `/proc/version` with the host) and a
  canned job of the official runner pass on real guests (T-037, T-036).
- The `incus` kind (`crates/podrunner-incus`, `[unit.incus]`): one system
  container of podvm's incus tier for each job, the credential files on
  stdin. It claims `confined`. Its suite passes on real instances (T-105).
  It has no egress hold, so it gets no `trusted` job yet (T-109).
- A musl build of the official runner (`scripts/runner-musl.sh`,
  `runner-musl.yml`) runs the canned jobs in Alpine on x86_64 and
  aarch64, and in a podvm unit on x86_64 (T-081).
- The run report names `claim_to_first_step_s`, `job_images` and the exit
  reason `limit` (T-075, T-074).

## In flight

- podvm is available (the owner, 2026-11-13): it answers COORD issues
  `(to podvm)`. podssh is on break.
- L2 holds: T-036, T-037 and T-039 are done (GitHub #36, #37, #39
  closed). The microvm kind passes the self-test, its unit tests, the
  acceptance of #37 and the mount check on real guests.
- T-108 is done: each report carries `cpu_s` and `peak_rss_mib`
  (podcrates `v0.40.0`).
- T-105 and T-109 (partial): the incus kind has no NIC and one channel
  on podvm `v0.2.0-beta.13`. Next: read its egress test and suite in the
  runner-image run of `6365312`; then close T-105, T-109, GitHub #88, and
  tell podvm on its issue 316 by a COORD issue.
- T-108 (blocked): podcrates did `cpu_s` and `peak_rss_mib` on its
  `main` (PodUtils/podcrates@T-527); take the tag `v0.40.0` (GitHub #86).
- T-103 (partial): steps 1 and 2 are done; next is the runner image with
  an engine in a guest and the value `engine` of `ContainerRoute`.
- T-101 (partial): the `program` source kind is in; its pair with GitHub
  runs on the host of LIVE-2 when githome gives its program (GitHub #66).
- The live gates: two agents on two constrained hosts, GitHub #74
  (LIVE-1, the remote mint and the App key) and #75 (LIVE-2, the unit
  host and the test jobs). Read their comments; fix each defect they
  find.
- The x86_64 job of `runner-image.yml` takes about 30 minutes: it has
  60 minutes, and each podvm conformance suite 900 s.
- The gate queues for many minutes, and a newer push cancels a queued
  run. Push less often. Close an entry on the green run of a later commit
  that holds the work. A push that changes only `TODO/` or `docs/` runs
  the gate only; `runner-image.yml` and `level2.yml` run on their paths.

## What waits on whom

| Who | What | Entries |
| --- | --- | --- |
| LIVE-1 and LIVE-2 | the reports of GitHub #74 and #75 | T-032, T-033, T-040 |
| podcrates | the tag `v0.40.0` with `cpu_s` and `peak_rss_mib` (GitHub #86) | T-108 |
| podcrates | `pod-unit`, `pod-release` (none planned for v1) | T-043 |
| actions/runner | a release with `Microsoft.AspNetCore.WebUtilities` 8.0.34 or later | T-082 |

## Open issues and asks

- GitHub #65, #66, #67: the trackers of the asks of cfb, githome and the
  CI plan that podrunner owns (from PodUtils/podcrates#35, #36, #47).
  Their lines map to the entries; T-001 holds them.
- GitHub #85 (COORD-025, to podvm): podvm answered in
  PodUtils/podvm#307; the lead session closes it.
- GitHub #86 (COORD-026, to podcrates): podcrates answered; it closes with
  the take of `v0.40.0` (T-108).

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
