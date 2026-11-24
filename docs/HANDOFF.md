# Handoff

The state of podrunner for a session with no memory of the last one. Read
this page first, then `AGENTS.md`, `TODO/PROGRESS.md` (the state, the work
order, the questions), `docs/maintenance.md`, `docs/development.md` and
`docs/decisions.md`. Keep this page current: remove a line when its work
is done.

## Where podrunner stands

- Pins: podcrates `v0.82.1` (`Cargo.toml`, T-619; the source wire and the
  credential are `pod_job::source`, the HTTP head is `pod_core::http1`;
  `status.v1` is `pod_job::Status` from podcrates `v0.73.0`);
  podvm `v0.2.0-beta.39`
  (`images/podvm.toml`; the podvm kind takes `probe::LOWEST` and later,
  the microvm kind `LOWEST_GUEST` (beta.10), the incus kind
  `LOWEST_INCUS` (beta.13), and an engine of the microvm kind needs
  beta.14, T-112); podnet `v0.4.0-beta.17` (`images/podnet.toml`); the
  official runner v2.338.2 (`images/runner/runner.toml`). Last beta of
  podrunner: `v0.1.0-beta.7` (release run 44066276664, `129310e`).
- The record: `cargo todo check` gives the counts. The last green
  `runner-image.yml` run is 45291991112 (`3c04e60`, podvm beta.38): the podvm suites,
  the held tests, the local suite on each kind, the parity suite in
  guests with an engine, the microvm kind on real guests, and the incus
  suite on real instances.
- M-A, M-B and L2 hold (T-039 is done). The work packages of M-C and M-D
  are done. The open gates are T-032, T-033 and T-040 (the live gates:
  GitHub #74 and #75).
- The podvm driver runs the official runner in a unit with no root: a
  canned job and a `container:` job through the hooks pass in
  `runner-image.yml` (T-026, T-035). The unit conformance suite passes on
  the fake unit, the podvm driver, the microvm kind and the incus kind.
- Each podvm unit and container carries the label `podrunner.unit=ID`,
  and `destroy` removes each one with it; the `run -d` of a unit writes
  podvm's report (`run_report`) (T-078). Where the rung binds a volume,
  the units share one store and each unit gets its credential files as a
  volume of its own (T-545).
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
  The installer `scripts/install.sh` and the offline bundle
  (`docs/install.md`, T-202); a release attaches them with the runner
  image archives from the green `runner-image.yml` run of its commit
  (T-205, partial: dispatch `runner-image.yml` on the commit before a
  release; the notes of the release list the new files).
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
  (`crates/podrunner-slots/src/usage.rs`, T-108); the incus kind reads
  both from `podvm inspect` of the instance (T-483), and an `unknown`
  value has its reason in `unknown_reasons`, as each absent time has
  (T-595).
- A musl build of the official runner (`scripts/runner-musl.sh`,
  `runner-musl.yml`) runs the canned jobs in Alpine on x86_64 and
  aarch64, and in a podvm unit on x86_64 (T-081).
- The run report names `claim_to_first_step_s`, `job_images` and the exit
  reason `limit` (T-075, T-074).

## Resume here (the session of 2026-11-24)

QA round 6 for `0.1.0-beta.8` is complete (T-246): agent A found 7 P2
and agent B found 4 (its P2-1 is T-629, and T-639, T-640 and T-641 are
new); both agents cleaned their users. The blockers of beta.8 now: T-627,
T-629, T-639, T-640 and T-641. Batch 1 (T-631, T-249 part, T-628, T-630,
T-632 and the lane of T-638) is done and pushed at `a6f1718`. The P3
entries are T-633 to T-636 and T-642; the untested areas are T-637.

State at the end: `main` at `c54953b`, pushed, the tree clean. Level of the
sidecar changes of T-504 and T-518: not run against GitHub (proven by unit
and fake tests only). Read first: the CI runs on `c54953b` (the gate with
`go test -race`, `level2.yml`, `runner-image.yml`).

- The ruling "Subagents" of 2026-11-24 (`docs/decisions.md`;
  `docs/maintenance.md`, "Subagents"): the lead session delegates.
  Implementation goes to Opus 5.5 at medium or high effort, recon and
  plain bug fixes to Sonnet 5.5 at high effort, reviews to Opus 5.5 at
  medium effort, blind tests and QA tests to Sonnet 5.5 at high effort.
  One cargo build at a time still holds: agents in parallel each need a
  worktree and a target directory of their own.
- Pushed in this session, each with its entry:
  - T-613 (partial), the cause and the fixes: on the hosted image `/mnt` is
    a directory of `/`, so the loop file is one file, made with no discard,
    mounted `nodiscard`, with `fstrim.timer` stopped; traces, boot
    diagnostics and time limits in `hosted-measure.yml`. Measures:
    45313526131 (`full` = all): three copies of the image fill the btrfs
    (COORD-125, #347). 45324301659 and 45333131256 (slice on btrfs): the
    import fits and the boot fails, because podvm's readiness check of
    `--podvm-init=image` returns at "offline" with no retry (COORD-127,
    #349). 45327334399 (slice on the store root, a `dir` pool): it boots.
  - T-614 and T-615 (done): the probe of `--export` and the run key (the
    machine id, else the boot id).
  - T-617 (done): the trackers #65, #66, #292 and #151 are amended. T-618:
    R-052 is asked on #292 (open, it waits for cfb).
  - The rulings of the owner (`docs/decisions.md`): Q12 (`pull_request_target`
    is an opt-in), Q13 (a personal account is an opt-in) and "Subagents".
  - T-518 (done, opt-in): the `TrustRule` trait and the asset
    `tier-cases.json`. `head_is_fork` is true when the head repository is
    not the base repository. An agent job needs `head_is_fork = no`. R8 and
    R9 of `docs/security/residuals.md`. githome is told on #151.
  - T-504 (done, opt-in `allow_personal_account`): `triggering_actor` and
    `run_attempt` are checked; the API URL comes from `config_url`; the
    owner type comes from `/users`. The queue of the sidecar: the lookups
    run in the time of `next`, a pending job blocks no job behind it,
    there are give-up limits, and each call has a budget with an ETag
    (R10 to R12). Four reviews found real P1 and P2 defects (the last at
    `c850c9d`).
  - podcrates `v0.82.1` is taken (T-619). COORD-126 (#348) is closed.
    #342 and #343 are closed (T-595, T-602).
  - T-620 (done): the short egress directory goes with its last unit.
- Open from this session:
  - T-621 (P2): a live lane that learns if GitHub sends the queued jobs
    again after a kill of the sidecar.
  - T-622 (P3): a status field for the counts of the sidecar queue; it
    needs a COORD issue to podcrates, which is not written yet.
  - T-623 (P2, done): a failed cancel of a refused run is tried again;
    the review fixes (the 409 texts, the attempt of a re-run, the share
    of the budget, the lines at a stop) are in the same entry.
  - T-625 (P2, done): the attempt of a cancel of a job offered past the
    limits (a dropped cancel, a stale run object, a clock, an absent
    start).
  - T-624 (P3): the P3 findings of the trust lookups; item 1 is done
    with T-623.
  - T-616 (P3), T-618 (waits for cfb).
- Asks open to podvm: COORD-124 (#346, T-613) is in the pinned
  `0.2.0-beta.39` (the lead closes it after its run); COORD-125 (#347,
  T-613: ask 2 in beta.39; ask 1 fixed on podvm's `main` at `cd51868e`,
  COORD-242, PodUtils/podvm#658, not in a release, for the beta after
  beta.39), COORD-127 (#349, T-613: fixed on podvm's `main`, COORD-239,
  for the beta after beta.39) and COORD-128 (#350, T-611: beta.39 gives no
  PeakMemoryBytes and 0 OomKills for an ended incus instance on a hosted
  runner, run 45359834173). T-611 is blocked on COORD-128; the live test
  takes that reason by name meanwhile.
- The work order next (`TODO/PROGRESS.md`):
  1. Read the CI on `c54953b`.
  2. The `v0.1.0-beta.8` path: the gates T-243, T-244 and T-246 (a new
     round of adversarial QA on the current `main`), then the checklist
     of `docs/maintenance.md`, "A release". A release moves 19 asks of
     githome and 11 clauses of cfb to "in a release".
  3. T-480, T-487, T-516, T-500.
  4. The local P3 code: T-598, T-597, T-616, T-603 step 1, T-610, T-609
     (it needs the option `--run-sh` or a root from the image, see its
     Approach).
- The tally of the recon of 2026-11-24 (T-617 recounted it for the
  trackers). githome, G-01 to G-41: 4 asks are in beta.7, 19 are done on
  `main` and not released, 11 are partial, 7 are open. cfb: 10 asks are in
  a release, 15 are on `main`, 6 are partial, 7 are open; W11 and W14 are
  dropped.
- Session notes:
  - A review round for each trust change: each review of T-504 and T-518
    found real P1 and P2 defects.
  - A `check.sh` run must not overlap an agent that edits the tree.
  - `issue-sync.yml` reopens an issue that the record marks open: record
    each close in `TODO/issues.md` and `TODO/github.txt` at once.

## Resume here (the session of 2026-11-23 and 2026-11-24)

- State at the end (`main` at `3c04e60` or later, the tree clean, no
  worktree and no agent in work): podvm `0.2.0-beta.38` is pinned
  (COORD-232). Green on `3c04e60`: the gate, `level2.yml`,
  `runner-image.yml` 45291991112, `secrets.yml`. Green on `f694571`:
  `user-gate.yml` 45276522625 (each lane), the dry run of `release.yml`
  45276528105 (T-553). The dry run of `runner-image-release.yml`
  45164009824 is green (T-499). The hosted capture with `--acls` and no
  `--sparse` is
  `ghcr.io/podutils/podrunner-hosted@sha256:baa8431a36ba58f3be6d15780a4a2cc0090d90cd1621df3ffe1dee5d841f4909`
  (C1 0 differences; its slice boots, `hosted-boot.yml` 45276447058); its
  runner image of before is `sha256:b1d895f4...` (T-541).
- This session dispatches workflows with the MCP tool
  `actions_run_trigger`; no `[gh action] requested` issue is necessary
  (the owner, 2026-11-23). A push that changes a crate of the units starts
  `runner-image.yml` (about 2.5 hours): cancel an older run when a newer
  run covers the same images.
- Done in this session, each with its proof in its entry: T-583, T-582,
  T-592, T-590, T-532, T-541 (a job of `serve` in the hosted runner image
  under incus sees the hosted paths and takes the tool cache), T-599,
  T-606, T-593 (CI); T-589, T-591, T-607 (development host).
- Partial, with what is left in each entry: T-588 (a start of `serve` in
  a unit of the system), T-577 (a `java` client in the lane
  `proxy-only`), T-578 (step 2, the engine route), T-596 (a live
  `container:` job through a proxy), T-483 (the peak stays "not given" by name
  until COORD-128, #350, is fixed: T-611 is blocked), T-497, T-499 (a publish run with a new
  `actions/runner`; the installer), T-553 (the run with a version and
  the run on a tag), T-556 (`systemd-remount-fs.service`, the KVM
  guest), T-557 and T-613 (the full snapshot does not fit a `dir` pool
  of a hosted runner; the loss of the runner in the place step has its
  cause, see the work order).
- COORD-123 (#345) and COORD-124 (#346, T-613) to podvm: both are in
  podvm `0.2.0-beta.39`, pinned (COORD-233, COORD-236, COORD-238). The run
  45359834173 of `runner-image.yml` showed that beta.39 gives no peak and
  no OOM count for an ended incus instance on a hosted runner: T-611 is
  blocked on COORD-128 (#350), and the live test takes the peak by name
  meanwhile; next: a run of `hosted-measure.yml` (T-613).
- The work order next: see the section of 2026-11-24 above.
- Session notes:
  - Agents in parallel worktrees: give each a target directory of its
    own outside the tree (`/workspace/target-NAME`) and a copy of the
    runner directory; remove both, and the worktree, after the merge.
  - Do not commit while `check.sh` runs: it records no tree that changed
    during the checks, and the pre-push hook then refuses the push. A
    change of only `TODO/` after a recorded tree needs no new check.
  - `issue-sync.yml` reopens an issue that the record marks open: when
    another session closes an issue of this repository, mark it closed in
    `TODO/issues.md` and `TODO/github.txt` in the same hour (#339 was
    reopened once).
  - The 99 reports of an outside author (#155 to #287) have the label
    `confirmed`, so `issue-sync.yml` writes their state.
  - The remote branch `claude/admiring-albattani-8ad4sr` and its local
    copy are of the harness, not of this project: `AGENTS.md` keeps the
    work on `main`.

## Resume here (the session of 2026-11-19 and 2026-11-20)

- The owner's rulings of this session (`docs/decisions.md`): "The hosted
  snapshot" (copy the root of a hosted runner 1:1 with its kernel, no
  build), "ghcr.io", "The snapshot keeps the Docker Hub login" (make each
  per-machine value fresh), "Retention on ghcr.io" (keep each version).
  The owner handles the disclosure to the author of the community
  snapshot: do not raise it.
- The hosted snapshot is published, public and attested
  (`docs/hosted-snapshot.md`, T-542, T-555), each capture with C1
  (0 differences) and C2 (no secret):
  - `ghcr.io/podutils/podrunner-hosted:ubuntu24-latest@sha256:dd51ce51ea1aad6f53afaae2a5650ff14fbd6636f7f29faebcecb7e75549d4f3`
    (amd64 `ubuntu24-20261116.376.1@sha256:d00306d8…`, arm64
    `ubuntu24-arm64-20261111.182.1@sha256:512b882a…`);
  - `ghcr.io/podutils/podrunner-hosted:ubuntu26-latest@sha256:9fc38c7bc474c04f82c9d90b9667c32634b47e5d44804144f1f893b1b7b6e468`
    (amd64 `ubuntu26-20261111.196.1@sha256:d35cf32b…`, arm64
    `ubuntu26-arm64-20261116.190.1@sha256:1444cf65…`).
  - Workflows: `hosted-snapshot.yml` (capture, dry run or `publish`; 1 to
    2 hours), `hosted-index.yml` (the index `OS-latest` of two digests),
    `hosted-parity.yml` (the probe diff, report-only, and the watcher of
    each label, daily), `hosted-boot.yml` (the core slice under incus,
    report-only). Scripts: `scripts/hosted/`.
- Done in this session: T-525 (C1 to C5 on microvm and incus), T-552
  (a job that the hook ends is `job-mismatch`), T-554 (TypeScript 7 on
  podvm, fixed by podvm beta.36), T-548. Partial: T-539, T-540, T-541
  (approach), T-542, T-543 (`[unit.incus] init = "image"`), T-555, T-556,
  T-561, T-500 (Chromium libraries in `runner-full`; `rbg-gate` passes on
  podvm, runner-dir and microvm, `user-gate.yml` run 44506794088).
- New entries: T-555 to T-563 (T-562: `release-index.json` with
  `pod-release`, after T-484; T-563: the placeholder credentials of
  `pod-broker`, githome's G-35).
- The hard problems of the project, for research by the owner's agent:
  `docs/reports/hard-problems-2026-11-20.md`.
- Housekeeping: #294 and #110 are closed; T-523 (after T-562) and T-524
  are open again (podcrates `v0.78.1`). #151 (githome) has all its
  entries done, its close is githome's call; #309 waits for podnet's read.
  The remote has two leftover Dependabot branches (trufflehog 3.99.8 and
  3.99.9), both superseded; the owner deletes them.
- CI notes: `gate.yml` cancels an older run of the same ref, so batch the
  pushes. A capture uses one hosted runner for 1 to 2 hours. A full
  `check.sh` takes 25 to 45 minutes here; an edit of the tree during it
  makes the push fail ("the tree changed"). Keep the disk under control:
  remove `target/*/debug/incremental`, `target/debug` and `fuzz/target`.

## Resume here (the session of 2026-11-18, after the coordinator's order)

- The coordinator (podcrates) sends its orders as COORD issues only
  (COORD-136, PodUtils/podcrates#227). Its order for podrunner: the take
  of `v0.70.0` (done), the census of T-491 (done), then T-493, T-494,
  T-486, T-495: all done (T-493 on a real podvm unit, `runner-image.yml`
  run 44349725647); COORD-092 (#302) says so to the coordinator. The move
  of the schemas (T-508) waits for podcrates.
- Done or advanced in this session (each pushed after `check.sh`):
  T-497 (the times, measures and `log_tail` of the report; open: the
  cache bytes, T-480), T-498 (done: `source restart`), T-499 (doctor
  pins and `podrunner-manifest.json`; open: an image release of its
  own), T-490 (done), T-507 (`config init`; open: `install.sh --serve`),
  T-534 (done), T-405 (done), T-482, T-483, T-485, T-488, T-515 (the
  podvm `0.2.0-beta.33` features, each open for its CI measure), T-506
  (the sandbox unit), T-510 (done: `CHANGES.md`; each commit that
  changes a contract adds its row and runs `check-notes.sh --stamp`),
  T-511 (`[unit.fake] claim`), T-526 (the incus suites block), T-503,
  T-505, T-519, T-522, T-492 (done; #65 and #66 rewritten), T-501
  (`drain --source`; open: `stop --source`), T-502 (the slots test; open:
  a conformance test), T-520 (the tolerance of `cpu_s`; open: the CI
  measure).
- CI to read first: `runner-image.yml` on the newest commit. Run
  44349725647 (`e6beea0`) was green: the incus engine ran a container
  (T-485); the `pids` test of T-488 failed on its own guest size (fixed:
  read its next run, then make it blocking).
- podvm `0.2.0-beta.39` is pinned (release run 45353882402 on `a43c947`;
  `pod-release verify --workflow nightly.yml` of its signed `SHA256SUMS`
  holds for `podvm-x86_64` and `podvm-aarch64`, Rekor entry 4073272661).
  It keeps podcrates `v0.82.0`. It was to give the use of a stopped incus
  instance (#345), but a hosted runner gives no peak and no OOM count
  (#350, COORD-128, T-611: blocked), refuses an image that cannot fit in a `dir`
  pool with the free space and the need (#346, T-613), and has no fix of
  #349 (COORD-239: for the next beta). Since `0.2.0-beta.38`: a layer of GNU tar `--acls` extracts (#339), `load` holds
  one lock at a time (#344), `inspect` gives docker's types (the podvm
  kind reads them, T-581), `exec` into an ended container is refused by
  name, and `run -d --rm` removes its container (no kind passes `--rm` to
  `run -d`). The incus tier keeps no ACL. The syscall tier is off again
  (`SYSCALL_TIER = None`, T-261): under it Node of `setup-node` fails with
  `errno 11` and a cargo build hangs (COORD-098, #308); give it again when
  podvm fixes them, and set `click-py313` and `rq-lint` to `ok` then. The
  microvm kind reads the
  OOM kills of a guest from podvm's report (T-515). T-403 and T-532
  wait for their step of `runner-image.yml` (containers on the namespace
  rung). T-496 waits for PodUtils/podvm@T-1968.
- podnet `v0.4.0-beta.17` is pinned (COORD-108, PodUtils/podnet#655);
  T-179 is done in CI (COORD-099, #309; #142 closed).
- Dependabot: #299 (rsa 0.10) is done as T-531.
- A full `check.sh` with the runner takes about 45 minutes here: batch the
  commits before a push.

## Resume here (the next session: githome and cfb first)

The last session (2026-11-18, evening) left:

- podcrates `v0.73.0` is taken (T-508 done; the take is COORD-100, #310). Its
  `pod-fake-githome` (PodUtils/podcrates@T-948) unblocks T-509 and its
  `pod-fake-github` (PodUtils/podcrates@T-921) gives a broker for a runner
  job through `serve` (T-527). Open answers: #307 (podvm's limits of `--podvm-confine`, no change; close
  on podvm's tag of PodUtils/podvm@T-1984 and PodUtils/podvm@T-1985); #304 (a tier for confined kinds);
  #308 (the syscall tier is off: podvm retries the EAGAIN of `openat2` on
  its `main`, COORD-197; with that beta set `SYSCALL_TIER`, run
  `user-gate.yml`, set `click-py313` and `rq-lint` to `ok`); #309 (the
  result of T-179 to podnet).
- Runs to read first: `user-gate.yml` on `2d3d73b` (T-537: the installer
  closes `~/.local/share`; T-527: the lane `serve-incus-kvm`; T-528: the
  rows `rbg-gate` with `--event workflow_dispatch`) and `runner-image.yml`
  on `2d3d73b` (T-525: "C1 to C5" with the session rule of the fake broker
  and the answer to `assigned`). Run 44416704184 showed the tier off
  works: the express rows and `rg-docs` pass on podvm in each lane. Make
  each measure blocking once it is green. T-526 is done (the plant run
  44417916405 was red in the incus step).
- New measure steps of `runner-image.yml` from `b24f5a3`: "The parity
  suite on the incus kind" (T-481: the real `actions/checkout`; T-485: with
  the engine, `services:` and job containers in an instance). Read their
  first run; make them block when green.
- COORD-101 (#311) asks podcrates to join `pod-fake-githome` to
  `pod-fake-github`, for a runner job through `serve` (T-527, T-509).
- The watcher of the runs of podssh (`.work/agent/watch-runs.py`) ends on a
  temporary HTTP error of the mirror (a 520): run it in a loop that starts
  it again (a defect of podssh's script; podrunner does not edit it).
- The hosted snapshot (the ruling of 2026-11-19): a second study of how
  ChristopherHX and catthehacker capture the hosted runner gives the
  pipeline; T-538 and its entries change with it.
- If a local runner test says a file of `_diag/blocks` "already exists",
  an interrupted run left it: remove `_diag/blocks` and `_work` of
  `PODRUNNER_RUNNER_DIR`.
- podvm `0.2.0-beta.35` is out (confine on the tier, `exec=` on the tier);
  podrunner gives no tier now, so its pin can wait for the beta with
  PodUtils/podvm@T-1990 (PodUtils/podvm#593).

The owner ruled on 2026-11-18 (`docs/decisions.md`, "githome and cfb
first"): podrunner accommodates each need and ask of githome and cfb, their
CI work comes first, and each unknown and limit that blocks one is settled
or lifted. From this next session, all of PodUtils works to make podrunner
complete and stable, so that the milestones of githome and cfb are met.
COORD-086 (GitHub #293) relays it to podcrates, with four asks (A1 to A4).

- The coordinator (the owner's ruling of 2026-11-18, relayed by
  podcrates in COORD-134, PodUtils/podcrates#225): from this next session
  the session of podcrates coordinates podrunner (the flagship), podssh,
  podnet and podvm. Its asks and orders come by a direct message, each
  with a COORD issue as its record; they set the work order, inside the
  rulings of `docs/decisions.md`. Read its first message before the order
  below.
- githome and cfb are away until podrunner is complete (the owner,
  2026-11-18). Their last answers are on GitHub #292: cfb answered Q-12
  to Q-14; githome's G-38 to G-41 came before the questions, so the
  recommendations of Q-1 to Q-8 are the Decision sections of their
  entries (`docs/decisions.md`, "The answers of githome and cfb before
  their pause"). githome comes back on the statement of T-512: one line
  `githome: complete in vTAG` on #292 and #151, when each ask G-01 to
  G-41 is done in a release. Name each release tag on #292 when T-493,
  T-494, T-505 and T-507 ship (cfb's request).
- The channels: GitHub #292 (cfb's questions P-1 to P-7 and githome's
  note; podrunner's one comment holds each answer and the steps C1 to C5
  of the acceptance of A-X-01 on the host of githome's pool), GitHub #151
  (COORD-054, githome; podrunner's one comment holds the table of each
  need of githome), #65 and #66 (the trackers). Each side keeps one
  comment and edits it; watch their `updated_at` (`docs/maintenance.md`,
  "Talk between sessions").
- The operator's order (cfb's R-053, GitHub #292): self-hosted CI first
  (githome gives it, podrunner runs each job); then githome becomes the
  primary forge, one project at a time; then the self-hosted parts of
  Cloudflare. cfb never uses podrunner directly: githome renders
  `podrunner.toml`, bootstraps the host of cfb's gates, pins the release
  and runs the checks C1 to C5. cfb's needs stand, as needs of the CI that
  it gets through githome.
- Facts of that host (GitHub #292): githome runs no GitHub job until
  T-493 and T-494 (its G-15 and G-16) land: they block R-051. Then githome
  sends the real trust facts of each job: the trusted jobs of the five
  private repositories (`[trust] users` holds their owner) go to the
  `incus` kind, a job of a fork to the `microvm` kind. A trusted job with
  `container:`, `services:` or `docker://` goes to a `microvm` guest with
  `engine` until T-485 gives `incus` an engine. The host has 32 GiB of RAM
  and `/dev/kvm`; each unit is 4 vCPU, 12 GiB and 40 GB of
  `disk_max_bytes`; until T-505 it takes 1 `incus` and 1 `microvm` slot.
- Order:
  1. Done: podcrates `v0.70.0` is taken (COORD-132,
     PodUtils/podcrates#222): #214 (T-331, F9), #215 (T-351, step 1),
     #288 (T-469), `Clock` (T-473), the proxy and CA lists (T-474), the
     lock of `serve`. podcrates `v0.71.0` gave the two other locks
     (T-513, done).
  2. Done: the census of T-491 (`docs/reports/census-2026-11-18.md`).
     Its gaps are T-514 to T-528; T-529 (the disk bound of an offer) is
     fixed. Asks: COORD-089 (GitHub #296, to podvm), COORD-087 amended
     (GitHub #294, `arch`).
  3. T-493 (P0: githome's G-15; it blocks R-051) is partial on `main`:
     the question of the job-started hook through the front of the egress
     socket (`crates/podrunner/src/serve/front.rs`,
     `crates/podrunner-slots/src/assigned.rs`), also from a podvm unit
     (T-530, done), and the field `assigned` of the report. T-494 is done.
     Open: a run of `serve` with two runs on a real unit in
     `runner-image.yml`. Then a beta; name its tag on #292. Then the needs, P1 first: T-496 (blocked on podvm), T-497, T-499, T-485, T-506 (G-36, a sandbox
     unit), T-507 (G-37, an install of `serve` that githome's CLI drives,
     no TCP listener), T-508 (take the `pod-job` schemas; the source
     wire moves after T-486, T-493 and T-495), T-509 (G-38, githome's
     source in CI), T-510 (G-39, `CHANGES.md`), T-511 (G-40, the fakes as
     release assets); then T-488, T-482, T-483, T-481, T-487, T-480,
     T-479, T-484, T-489, T-492, T-505 (`max_units`), and T-500 to
     T-504.
  4. A beta for githome's pair run (GitHub #151) and cfb's host, when the
     gates of `AGENTS.md`, section 12, hold. Name the tag on #292 when
     T-493, T-494, T-505 or T-507 ships (cfb's request).
  5. Last, T-512 (G-41, P0): when each ask G-01 to G-41 is done in a
     release, write `githome: complete in vTAG` on #292 and #151.
     githome comes back on that line.
- Each change that podvm or podnet must make: a COORD issue at once.
- podnet `v0.4.0-beta.17` is pinned (COORD-108, PodUtils/podnet#655): it has the fix of #142 (T-179). Next: `user-gate.yml`
  on the green `runner-image.yml` run of the pin, then the result to podnet as a COORD issue.

## The release of `v0.1.0-beta.7` (released 2026-11-17)

The rules: `AGENTS.md`, section 12, and `docs/maintenance.md`, "A
release". A release needs a green gate, `runner-image.yml`, `level2.yml`
and user-path gate (`user-gate.yml`) on its commit, and an adversarial QA
with no P1 or P2 finding (`docs/qa-brief.md`, T-246).

- `main` has the fixes of two rounds of adversarial QA: round 1 (T-265 to
  T-286) and round 2 (T-287 to T-298). Each P1 and P2 of them is done;
  T-277, T-290 and T-298 hold the P3 that are open. The notes
  `docs/releases/v0.1.0-beta.7.md` list them, with the level of proof.
- Round 3 ran (2026-11-16) on the release files of `12a1300` (T-246):
  agent A gave T-300 to T-308, agent B T-309 to T-316; the lead session
  T-301 and T-317. Each P1 and P2 of them is done (T-300 by the call of
  2026-11-16 in `docs/decisions.md`: a podvm unit off the namespace rung
  is no boundary for a static binary; the doctor warns, the documents
  name `--microvm`). Next: a round 4 on the release files of the current
  `main`, then a release request if it finds no P1 or P2.
- The kit of the QA: `/srv/qa-kit` (the files of a user-gate run, podvm
  from its release, the documents, `env.sh` with the proxy and CA of the
  host); the users `qa1` and `qa2` with no root. The reports are in the
  entries (T-308 and T-316 hold the P3s).
- podvm `v0.2.0-beta.39` is pinned. On this host (user namespaces)
  `run -d` of a unit takes the namespace rung, where the job runs as
  `runner` with the host name `podrunner-u-N` (T-317); podrunner keeps
  `TAR_OPTIONS=--no-same-owner` (and two variables that do nothing for
  `runner`). In the CI lanes the units ran on the loader family (Go finds
  no GOROOT; uv and ruff get EACCES): the rows of uv and ruff on podvm are
  `xfail` (T-261), until podvm binds the volumes of `run -d`.
- The last runs, on `43dce45` (podvm beta.27): gate 43862847171,
  level2 43862903999, runner-musl 43862847094, runner-image 43862847276
  and user-gate 43870085390 green. In the user-path gate each row passes
  or is a known xfail; `express-lint` on podvm passes in each lane now
  (T-262 done). Before a release: dispatch them again on its commit
  (user-gate with `-f images_run=` the runner-image run of that commit,
  or 43862847276 while `images/` stays the same). `bca83b4` changed
  `images/podnet.toml`: the next user-gate takes the runner-image run of
  `b5d616c` (43922356588) or of a later commit.
- On `b5d616c` (`6834836` adds documents only): gate 43926340918,
  level2 43922356619, runner-image 43922356588 and user-gate 43930264381
  green (proven in CI on GitHub-hosted runners).
- After `b5d616c`: P3 fixes of T-323 and T-316 (F9, F11, F35, F38,
  B-13, B-18; the line `Fixed:` of each entry). The runner-image run of
  `31bee8e` is 43983244604; a release commit needs its own gate, level2,
  runner-image and user-gate again.
- The owner (2026-11-17, `docs/decisions.md`): podvm `v0.2.0-beta.30`
  is pinned and T-318 is done. Green on `63d67a4`: gate 44011893532,
  level2 44011893602, runner-musl 44011893617, runner-image 44011893582;
  user-gate 44025228142 on `a238f8d` (scripts only after `63d67a4`).
  The fifth QA ran on the files of that user-gate run (users `qa6`,
  `qa7`, kit `/srv/qa-kit`): no P1; the P2 T-324 to T-338 are done
  (T-326 by a call: no compiler in the base image), except T-324, which
  waits for its `runner-image.yml` run (44043008133 on `291f702`). P3:
  T-331, T-339.
- `v0.1.0-beta.7` is released (release 44066276664 on `129310e`; gate
  44051464379, runner-image 44057392980, user-gate 44051472680, level2
  44043008110 on `291f702`, with only documents and the record after
  it). The online quick start from the release URL passed as `qa6` (no
  root, a proxy that intercepts TLS): the installer, `--version` and
  `hello.yml` ok.
- The milestones and the acceptance of githome and cfb are NOT reached.
  M-C (#32), M-D (#33) and L3 (#40) wait for the live gates LIVE-1 (#74)
  and LIVE-2 (#75); the bodies name `v0.1.0-beta.7` (2026-11-17) and the
  last comment of #75 names what to report (T-192, T-179 on H0, the
  20-job run of cfb A-X-01). githome (#66: 8 of 12; G-14 and G-12
  partial) waits for its pair run on beta.7: COORD-054 (#151), amended.
  cfb (#65: 1 of 2) waits for the 20-job run (W3 of #75). Read each
  report the day it comes, and file each defect as an entry.
- Do not idle while those wait: take the work order. Done after the
  release: T-233 (the egress module takes the proxy of the environment),
  T-237, T-239, T-234 and T-238 (by earlier work); partial: T-221 (`cpus`
  on podvm in a delegated cgroup; the measure in `runner-image.yml`),
  T-242 (the tar cache of the user), T-183 (the held self-test test),
  T-179 (`hold:podvm` of the user-path gate; the lane userns-off is H0),
  T-220 (steps 3 and 4 in the code: `--vnet`; not run). Then the
  P3 groups T-331, T-339, T-308, T-316, T-323, and T-264 (podcrates
  `v0.68.0`, COORD-129).
- A user QA of beta.7 (#203 to #213) gave T-340 to T-350. Each is done
  but T-350 (podnet `0.4.0-beta.17` is pinned on `main`; the forward is
  T-220). #203 to #212 are closed. The user measured the podvm kind on
  the unix broker and the front on a host that refuses the loopback bind
  (#170): that is `--vnet PATH` now (`docs/local-mode.md`). The
  user-gate lane `bind-denied` measures it (`lab:vnet`, an xfail until it
  passes). The CI runs of these commits were queued for an hour or more:
  read their results first.
- A second user QA of beta.7 (a report with F1 to F23) gave T-351 to
  T-371. They block the next release: T-351 (P1: a host with DNS only
  through its proxy fetches no action; the root is in `pod-broker`,
  COORD-078 #215) and T-371 (P1: the user gate has no lane of that host),
  and the P2 T-353, T-354 (devpts, COORD-079 #216), T-355 and T-356.
- COORD state: #170 (podnet) and #191 (podcrates) are closed; #189 holds
  the facts that podvm asked for (PodUtils/podvm#466), and podvm answered
  (PodUtils/podvm#470: PodUtils/podvm@T-1934 for the pty, PodUtils/podvm@T-1935 for the raw calls of the
  userland rung); #214 and #215 wait for podcrates, #216 for podvm.
- The owner (2026-11-17, Q9 in `docs/decisions.md`): the session owns the
  project and its issues, and asks the owner only for a change of a ruling.
  `issue-sync.yml` writes.
- A user read the code of beta.7 and filed #218 to #237: each is fixed on
  `main` and closed (T-373 to T-393). F1 (T-351) is fixed in podrunner:
  DNS over HTTPS through the proxy (`dns::resolver_via`), measured on this
  sandbox as a user with no resolver. Two read-only review agents (security,
  fidelity of the local mode) look for the same class of defect; file each
  finding as an entry.
- runner-image 44100036389 failed on a pull limit of Docker Hub (T-394,
  fixed: `mirror.gcr.io`); its T-221 measure showed that systemd-run reads
  `$$` as `$` (fixed: `echo 0`). Read the next runner-image run first.
- The fourth QA: agent B found T-318 to T-323 and T-203 (each P1 and P2
  done). Agent A (user
  `qa3`) stopped before its report: it gave no findings. The adversarial
  QA of the release is agent B only until a new agent A runs.
- Each push that changes the crates starts `runner-image.yml` (about 2
  hours, no concurrency group): cancel the runs of older commits with
  `actions_run_trigger` (`cancel_workflow_run`) when a newer one covers
  them.
- Then T-264 (podcrates `v0.61.0`: the issue sync, pod-lab, the chroot
  row of T-283), after the release.
- The LIVE-2 report (GitHub #75, 2026-11-15) ran W1, W3 (20 of 20) and
  W4 on the podvm kind with a fixed sidecar, and W6 held. Its defects are
  T-176 to T-189; T-176 to T-179 are P1.
- The LIVE-1 report (GitHub #74) found T-164 to T-175; the fixes are in
  `v0.1.0-beta.6`, except T-169 (the cancel of a refused run), which is on
  `main` after the tag, with T-198 (the limits of an offer). The next beta
  carries both; then point #74, #75 and githome (#151) at it.
- The last green `runner-image.yml` is 44874019156 (`e4627e6`).
- T-162 (partial, G-12): `[unit.podvm] mem` and `pids` are rlimits;
  `[unit.incus] mem`, `cpus` and `pids` are the keys of the instance
  (podvm `0.2.0-beta.20` or later, pinned `0.2.0-beta.39`). Next: a
  payload past `pids` and one past `mem` on the podvm and incus kinds.
  `cpus` on the podvm kind waits for the cgroup half
  (PodUtils/podvm@T-1747); COORD-050 (#110) stays open for it.
- T-161 (partial): `docs/alternate-payload.md` has the census of the
  calls of the runner and the call: measure an existing third-party
  runner against the fake broker first.
- T-159 (partial): the destroy of a podvm unit gave an unknown count of
  processes once; `destroy` now logs the cause (`podrunner: destroy UNIT:
  no count of processes: ...`). Conformance test 26 prints it on purpose.
  Read it at the next real failure.
- The live gates: GitHub #74 (LIVE-1, the remote mint and the App key)
  and #75 (LIVE-2, the unit host and the test jobs) name `v0.1.0-beta.7`;
  the App needs
  "Actions: read and write" (Q7). Read their comments;
  fix each defect they find.
- A cloud session can dispatch a workflow with the MCP tool
  `actions_run_trigger` (gate 43814582586, level2 43814590789 and
  user-gate 43814594849 on `9a27d59`). When a dispatch is refused, ask
  the local agent with an issue `[gh action] requested`
  (`docs/maintenance.md`, "A run of a workflow").
- podvm answers COORD issues `(to podvm)` in its repository with
  `(to podrunner)`. podssh is on break.
- The x86_64 job of `runner-image.yml` takes up to 120 minutes; it has
  150 minutes.
- A push that changes only `TODO/` or `docs/` runs the gate only;
  `runner-image.yml` and `level2.yml` run on their paths. A newer push
  cancels a queued gate run. Run `cargo todo check` with no pipe before
  a commit: a pipe hides its exit code.
- The disk of a cloud session is small: run the checks with
  `CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0
  CARGO_PROFILE_DEV_DEBUG=line-tables-only
  CARGO_PROFILE_TEST_DEBUG=line-tables-only`, and remove
  `target/x86_64-unknown-linux-musl/debug` and `target/debug` when a link
  fails with no space.

## What waits on whom

| Who | What | Entries |
| --- | --- | --- |
| podvm | the fix of `pod-extract` for a PAX value with a newline (#339), the steps of the hosted snapshot (#313, #316: PodUtils/podvm@T-1998, @T-1994), the syscall tier (#189, #304, #308) | T-556, T-543, T-544, T-261 |
| LIVE-1 and LIVE-2 | the report of GitHub #75, and a new run on the next beta | T-032, T-033, T-040 |
| podcrates | `pod-unit`, `pod-release` (none planned for v1) | T-043 |
| actions/runner | a release with `Microsoft.AspNetCore.WebUtilities` 8.0.34 or later | T-082 |

## Open issues and asks

The audits of 2026-11-21 and 2026-11-23 found the record and GitHub in agreement; `TODO/issues.md` has each row. Open:

- Asks of 2026-11-23 and 2026-11-24: #344 (COORD-122, T-606), #345 (COORD-123, T-611), #346 (COORD-124, T-613), #347 (COORD-125, T-613), #349 (COORD-127, T-613) and #350 (COORD-128, T-611) to podvm. Answers: COORD-239 (#349, PodUtils/podvm#655): fixed on podvm's `main` for the next beta; COORD-242 (#347, PodUtils/podvm#658): ask 1 fixed on podvm's `main` at `cd51868e` for the next beta (one tree, a gzip upload), ask 2 in beta.39.
- Asks to podvm not delivered or not taken: #339 (COORD-118, the PAX value of an ACL) and #344 (COORD-122, 85 layers) are in beta.38 and wait for their runs, #316 and #313 (PodUtils/podvm@T-1994, @T-1998 are not in beta.38), #308 (podvm waits for data of podrunner), #304 and #189 (T-261), #300 (`--podvm-volume-base` off the namespace rung), #296 (ask 5), #277 (T-403).
- COORD issues with entries not done: #312 (T-553), #184 (T-250), #183 (T-249), #181 (T-248).
- Defects of the live runs with entries not done: #260 (T-412), #171 (T-229), #149 (T-197), #144 (T-192), #143 (T-180), #140 and #121 (T-169), #138 (T-187), #126 (T-181).
- Trackers: #1 (T-001), #9 (T-009), #32, #33 and #40 (the milestone gates, T-032, T-033, T-040), #43 (T-043), #65, #66, #67, #292 (cfb), #293 (the owner's ruling), #74 and #75 (the live gates), #153 (T-199). #151 is the channel with githome: each entry it maps is done, and podrunner asked there for a root disk of 80 GB (T-551) on 2026-11-20.
- No question for the owner is open.

## Talk between sessions

- podcrates: a COORD issue of this repository titled
  `COORD-NNN: subject (to podcrates)`; a direct message to the session
  of podcrates is also allowed (the owner, 2026-11-18,
  `docs/decisions.md`). Each ask for a crate, each take of a
  tag and each milestone report goes there.
- podnet: a COORD issue of this repository titled
  `COORD-NNN: subject (to podnet)`. podnet answers in its repository with
  `(to podrunner)`, and the watcher finds it.
- podvm: answers through COORD issues `(to podrunner)` in its repository
  and through the hand-offs of podcrates.
- The design copy is `docs/design/` (call C17: do not edit it).
  `docs/design-issues.md` maps each issue of the plan to its sections.
