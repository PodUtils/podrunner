# Handoff

The state of podrunner for a session with no memory of the last one. Read
this page first, then `AGENTS.md`, `TODO/PROGRESS.md` (the state, the work
order, the questions), `docs/maintenance.md`, `docs/development.md` and
`docs/decisions.md`. Keep this page current: remove a line when its work
is done.

## Where podrunner stands

- Pins: podcrates `v0.44.0` (`Cargo.toml`); podvm `v0.2.0-beta.14`
  (`images/podvm.toml`; the podvm kind takes `probe::LOWEST` and later,
  the microvm kind `LOWEST_GUEST` (beta.10), the incus kind
  `LOWEST_INCUS` (beta.13), and an engine of the microvm kind needs
  beta.14, T-112); podnet `v0.4.0-beta.8` (`images/podnet.toml`); the
  official runner v2.338.2 (`images/runner/runner.toml`). Last beta of
  podrunner: `v0.1.0-beta.4` (release run 43506335609, `d796a94`).
- The record: 156 entries (`cargo todo check` gives the counts). The last green `runner-image.yml` run is
  43541714659 (`9f15901`): the podvm suites, the held tests, the tar
  forms, the microvm kind on real guests (with an engine), and the incus
  suite on real instances. The last green `level2.yml` run is
  43541714726 (`9f15901`).
- M-A, M-B and L2 hold (T-039 is done). The work packages of M-C and M-D
  are done. The open gates are T-032, T-033 and T-040 (the live gates:
  GitHub #74 and #75).
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
  script sets the proxy variables), so its hold is `enforced`. Before each
  job the guest script checks its mounts (`MOUNT_CHECK`). The self-test,
  the unit tests and a canned job of the official runner pass on real
  guests (T-036, T-037). With `[unit.microvm] engine = DIR`, the guest runs
  `dockerd` (`--podvm-engine`) and the runner gets `DOCKER_HOST` of a
  socket in the guest (T-103, `docs/container-engine.md`); its capability
  says `container: engine` (podcrates `v0.42.0`), and placement sends a
  job with `container_engine` only to such a kind.
- The local mode (`podrunner run`, `docs/local-mode.md`): the executors
  `--runner-dir`, `--podvm`, `--microvm [--engine]` and `--incus` (T-123:
  a front on the egress socket of a guest gives the broker); actions by
  tag pinned by `podrunner-actions.lock` into a store (T-118); artifacts
  v4 and the cache v2 served by the fake broker (T-119, T-120); secrets
  and `github.token` (T-121, T-122); the `github` context of the git tree
  and the event payload (T-125); the working tree, `--with-git` and the
  checkout of another repository (T-133); the cancel at a signal (T-141);
  images by tag (T-130); the protection of an environment (T-136); the
  export of a workspace on `--runner-dir` (T-138); `--local-repository`
  and `--action-server` (T-139); OIDC id-tokens (T-140); `--watch` and
  `--schedule` (T-142); `run.toml` and `.actrc` (T-143); an `env:` of one
  expression (T-152); the fetch of the actions through an egress grant
  (T-154); `podrunner doctor --run` (T-153).
  The parity suite (`crates/podrunner/tests/parity.rs`, T-147) gives one
  verdict for each workflow: with `--runner-dir`, or in a unit kind
  through `PODRUNNER_PARITY_UNIT`.
- The `incus` kind (`crates/podrunner-incus`, `[unit.incus]`): one system
  container of podvm's incus tier for each job, the credential files on
  stdin. It claims `confined`. It has no NIC and one channel of podvm to
  the egress socket (the socket file at mode 0666 in the 0700 socket
  directory), so its hold is `enforced` and it serves `trusted` jobs
  (T-105, T-109).
- Each report carries `cpu_s` and `peak_rss_mib`
  (`crates/podrunner-slots/src/usage.rs`, T-108); the incus kind gives
  `unknown`.
- A musl build of the official runner (`scripts/runner-musl.sh`,
  `runner-musl.yml`) runs the canned jobs in Alpine on x86_64 and
  aarch64, and in a podvm unit on x86_64 (T-081).
- The run report names `claim_to_first_step_s`, `job_images` and the exit
  reason `limit` (T-075, T-074).

## In flight

- podvm is available (the owner, 2026-11-13): it answers COORD issues
  `(to podvm)`. podssh is on break.
- T-103, T-123, T-124, T-146 (partial): their proofs run in
  `runner-image.yml` (the parity suite in guests with an engine, the local
  suite in microvm and incus units, the rerun job). Close each one on its
  green run. T-124 then keeps the load of the images from the store, not a
  pull (with T-130).
- T-131 (blocked until now): podrunner takes podcrates `v0.44.0`
  (COORD-041, GitHub #101); `Run::with_called` gives the calls of reusable
  workflows. T-125 is done with the filters of `on:` of the same tag.
- T-130 (partial): the tags check of `runner-image.yml` takes
  `busybox:1.37.0-musl`; T-156 (blocked) takes the glibc image again after
  the podvm release with PodUtils/podvm@T-1880 (GitHub #100, PodUtils/podvm#351).
  The setup of KVM and QEMU is now a step of its own, so a failed podvm
  step no longer skips it.
- T-138 (partial): the unit kinds need mounts in `StartSpec` for the
  export, the tool cache and `--keep-failed`.
- Q6 (T-148): Azure Pipelines YAML; the recommendation is no
  (`docs/azure-pipelines.md`). No work waits on it.
- T-101 (partial): the `program` source kind is in; its pair with GitHub
  runs on the host of LIVE-2 when githome gives its program (GitHub #66).
- The live gates: two agents on two constrained hosts, GitHub #74
  (LIVE-1, the remote mint and the App key) and #75 (LIVE-2, the unit
  host and the test jobs). Read their comments; fix each defect they
  find.
- The x86_64 job of `runner-image.yml` takes up to 120 minutes with the
  parity suite in guests with an engine and the measures; it has 150 minutes.
- The gate queues for many minutes, and a newer push cancels a queued
  run. Push less often. Close an entry on the green run of a later commit
  that holds the work. A push that changes only `TODO/` or `docs/` runs
  the gate only; `runner-image.yml` and `level2.yml` run on their paths.

## What waits on whom

| Who | What | Entries |
| --- | --- | --- |
| LIVE-1 and LIVE-2 | the reports of GitHub #74 and #75 | T-032, T-033, T-040 |
| podvm | a release with PodUtils/podvm@T-1880: a glibc image with no `libdl.so.2` (GitHub #100) | T-156 |
| podcrates | the crates of the action store and of the results API (GitHub #94, #95), no block | T-118, T-119, T-120 |
| githome | its source program, for the pair on the host of LIVE-2 | T-101 |
| podcrates | `pod-unit`, `pod-release` (none planned for v1) | T-043 |
| actions/runner | a release with `Microsoft.AspNetCore.WebUtilities` 8.0.34 or later | T-082 |

## Open issues and asks

- GitHub #65, #66, #67: the trackers of the asks of cfb, githome and the
  CI plan that podrunner owns (from PodUtils/podcrates#35, #36, #47).
  Their lines map to the entries; T-001 holds them.
- GitHub #94, #95, #97, #98 (COORD-034, -035, -037, -038, to
  podcrates): the asks of the local mode. GitHub #96 (COORD-036): the take
  of `v0.42.0`. GitHub #100 (COORD-040, to podvm): a glibc image where
  chroot is denied.
- GitHub #9, #32, #33, #40: wait for the live gates (#74, #75). GitHub
  #43 waits for podcrates.

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
