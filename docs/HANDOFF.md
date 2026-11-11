# Handoff

The state of podrunner for a session with no memory of the last one. Read
this page first, then `AGENTS.md`, `TODO/PROGRESS.md` (the state, the work
order, the questions), `docs/maintenance.md`, `docs/development.md` and
`docs/decisions.md`. Keep this page current: remove a line when its work
is done.

## Where podrunner stands

- Pins: podcrates `v0.24.0` (`Cargo.toml`); podvm `v0.2.0-beta.3`
  (`images/podvm.toml`, the lowest version of `probe::LOWEST`); the
  official runner v2.338.2 (`images/runner/runner.toml`). Last beta of
  podrunner: `v0.1.0-beta.1`.
- M-A, M-B and the work packages of M-C and M-D are done. The open
  milestone gates are T-032 and T-033 (DR-7: no live account), T-039
  (a real guest: T-036, T-037) and T-040 (DR-7).
- The podvm driver runs the official runner in a unit with no root: a
  canned job and a `container:` job through the hooks pass in
  `runner-image.yml` (T-026, T-035). The unit conformance suite passes on
  the fake unit and on the podvm driver; tests 25 and 26 hold the tree
  kill (T-063).
- The confined fake (T-042, step 1): the shim applies the rules of
  `pod-confine` (`crates/podrunner-unit/src/confine.rs`); conformance test
  27 passes on it. A unit kind of real jobs at `confined` waits for
  podvm (PodUtils/podvm#259).
- A podvm unit reports `egress_hold: unknown`: `podnet pod` is not wired
  into the driver, so placement gives a podvm unit no `trusted` job
  (`hold-not-available`). The record has an entry for this gap
  (`TODO/PROGRESS.md`, work order).

## In flight

- T-064: the next podvm beta copies the image for each container
  (PodUtils/podvm#258). Take the root of a unit from `podvm inspect`
  before that beta is pinned, then pin it and run tests 25 and 26 on it.
- The podcrates tag after `v0.24.0` changes `pod-confine`: new fields of
  `Spec` (podrunner uses the builders), and `apply` drops each capability
  (PodUtils/podcrates@T-221). Take it, then remove the root guard of the
  test in `crates/podrunner-unit/src/confine.rs`.
- The gate queues for many minutes, and a newer push cancels a queued
  run. Push less often. Close an entry on the green run of a later commit
  that holds the work.

## What waits on whom

| Who | What | Entries |
| --- | --- | --- |
| The owner | DR-7: a test organization and App ("not now") | T-007, T-032, T-033, T-040 |
| podvm (on break) | the next beta: a tree for each container (PodUtils/podvm@T-1709), absolute links (PodUtils/podvm@T-1725), `:ro` on `exec` (PodUtils/podvm@T-1726) | T-064, T-061, T-062 |
| podvm | `pod-confine` in its units (PodUtils/podvm#259) | T-042 |
| podvm | the aarch64 interposer (PodUtils/podvm@T-1721) | T-051, T-030 |
| podvm | the machine tier (PodUtils/podvm@T-1656) | T-037, T-036, T-039 |
| podcrates | the tag after `v0.24.0` (`pod-confine`) | `HANDOFF` only, above |
| podcrates | `pod-actions-workflow`, `pod-actions-expr` | T-046 |
| podcrates | `pod-unit`, `pod-release` | T-043 |

## Talk between sessions

- podcrates: direct messages (`send_message`) to its session
  (`list_sessions`, title with `(podcrates)`). Each ask for a crate goes
  there; podcrates answers with the id of its entry and releases a tag.
  After each milestone, send a short report: the commit, the gate run,
  the next step.
- podnet: a COORD issue of this repository titled
  `COORD-NNN: subject (to podnet)`. podnet answers in its repository with
  `(to podrunner)`, and the watcher finds it.
- podvm, podssh: on break. A COORD issue `(to podvm)` can go; expect no
  answer.
- The design copy is `docs/design/` (call C17: do not edit it).
  `docs/design-issues.md` maps each issue of the plan to its sections.
