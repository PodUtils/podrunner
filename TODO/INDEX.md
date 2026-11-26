# Index

Each entry of the work record, one row each, sorted by id. The entry is in
the area file that its row links. It closes there, with its Prove run and
the result written.

`TODO/PROGRESS.md` has the only work order. `TODO/RULES.md` gives the rules
of the record. `TODO/issues.md` maps each GitHub issue to its entries.

`cargo todo set` and `cargo todo counts` derive the counts from the rows.
`cargo todo check` makes sure that the counts, the rows and the entries
agree.

## Values

**Priority.**

- **P0**: correctness fails, data is lost, or the process stops.
- **P1**: work on the path of the next milestone, or a check that the
  rules need.
- **P2**: worth doing; nothing is wrong without it.
- **P3**: worth recording, so that nobody must find it again.

**Effort.** S is less than one day. M is some days. L is one week. There
is no XL: make two entries.

**Status.** `open`, `partial`, `blocked` or `done` (`TODO/RULES.md`).

**Milestone.** As in `TODO/ROADMAP.md`: M0 Decisions and bootstrap, M-A
Level 1 and the podvm unit, M-B The official runner under podvm, M-C The
mint and the live gates, M-D One live trusted job, L2 Level 2, L3 Live
acceptance, later The crate dependencies later than v1.

**Category.** `defect`, `feature`, `measurement`, `release`, `chore`,
`research` or `docs`.

## Entries

**688 entries: 73 open, 57 partial, 18 blocked, 540 done.**

| Priority | open | partial | blocked | done | total |
| --- | --- | --- | --- | --- | --- |
| P0 | 1 | 0 | 0 | 2 | 3 |
| P1 | 0 | 23 | 2 | 100 | 125 |
| P2 | 12 | 21 | 10 | 281 | 324 |
| P3 | 60 | 13 | 6 | 157 | 236 |
| **All** | 73 | 57 | 18 | 540 | 688 |

| ID | Priority | Effort | Milestone | Category | Status | Item |
| --- | --- | --- | --- | --- | --- | --- |
| [T-001](plan.md) | P1 | S | M0 | docs | partial | The plan of podrunner v1 and the issue map (GitHub #1) |
| [T-002](plan.md) | P1 | S | M0 | docs | done | DR-1: the plan of GitHub #1 is the plan (GitHub #2) |
| [T-003](plan.md) | P1 | S | M0 | docs | done | DR-2: PodUtils/podrunner holds code (GitHub #3) |
| [T-004](plan.md) | P1 | S | M0 | research | done | DR-4: trusted jobs at boundary redirect on no-root hosts (GitHub #4) |
| [T-005](plan.md) | P2 | S | M0 | research | done | DR-5: the microVM unit for untrusted jobs (GitHub #5) |
| [T-006](plan.md) | P2 | S | M0 | research | done | DR-6: githome and cfb accept the restated G-02, G-03 and G-09 (GitHub #6) |
| [T-007](plan.md) | P2 | S | M0 | research | done | DR-7: a throwaway GitHub organization and App for live tests (GitHub #7) |
| [T-008](plan.md) | P2 | S | M0 | docs | done | The defaulted decisions DR-3 and DR-8 to DR-17 (GitHub #8) |
| [T-009](plan.md) | P2 | M | M0 | research | partial | The register of the unknowns U1 to U25 (GitHub #9) |
| [T-010](plan.md) | P3 | S | M0 | docs | done | The non-goals of v1 and the trigger of each (GitHub #10) |
| [T-011](bootstrap.md) | P1 | S | M0 | chore | done | The Cargo workspace, the pinned toolchain and the static musl build (GitHub #11) |
| [T-012](bootstrap.md) | P1 | S | M0 | chore | done | The CI gate: format, lint, tests, static probe, deny, file size (GitHub #12) |
| [T-013](bootstrap.md) | P1 | M | M0 | chore | done | The agent workflow, the decisions and the record (GitHub #13) |
| [T-014](bootstrap.md) | P2 | S | M0 | docs | done | The cleaned design spec in docs/ (GitHub #14) |
| [T-015](bootstrap.md) | P2 | S | M-A | release | done | The beta release workflow (GitHub #15) |
| [T-016](core.md) | P1 | S | M-A | feature | done | WP-01: the seam traits, the contracts v1 and the conformance harness (GitHub #16) |
| [T-017](core.md) | P1 | S | M-A | feature | done | Schemas v1: job offer, unit capability, run report, public report (GitHub #17) |
| [T-018](core.md) | P1 | S | M-A | feature | done | The tier rule: Rust implementation and the case table (GitHub #18) |
| [T-019](core.md) | P2 | S | M-A | docs | done | Security documents: threat model, accepted residuals, leak runbook (GitHub #19) |
| [T-020](core.md) | P1 | S | M-A | feature | done | WP-02: the fake source and the fake unit, scripted, with a manual clock (GitHub #20) |
| [T-021](core.md) | P1 | M | M-A | feature | done | WP-03: the slots core: states, journal, placement, status, drain, codes, config, audit (GitHub #21) |
| [T-022](core.md) | P1 | M | M-A | feature | done | CLI and control socket: serve, status, drain, stop, doctor, print-unit (GitHub #22) |
| [T-023](core.md) | P1 | M | M-A | feature | done | WP-04: the podvm unit driver with a store for each unit and a subreaper shim (GitHub #23) |
| [T-024](core.md) | P1 | M | M-A | feature | done | WP-08: the egress module over pod-broker v1, one listener for each unit (GitHub #24) |
| [T-025](core.md) | P1 | S | M-A | chore | done | Milestone M-A: level 1 green, podvm unit conformance with a stand-in payload (GitHub #25) |
| [T-026](live.md) | P2 | M | M-B | chore | done | Milestone M-B: the official runner starts under the podvm driver, no GitHub (GitHub #26) |
| [T-027](live.md) | P2 | M | M-C | feature | done | WP-05a: mint with an in-process sign module, source conformance (GitHub #27) |
| [T-028](live.md) | P2 | S | M-C | feature | done | WP-06: the fake scale-set service (Go, test only) (GitHub #28) |
| [T-029](live.md) | P2 | M | M-C | feature | done | WP-07: the github-scaleset source sidecar (static Go) (GitHub #29) |
| [T-030](live.md) | P2 | M | M-C | feature | done | WP-10: the image pipeline: pins, age limit, podrunner image build (GitHub #30) |
| [T-031](live.md) | P2 | M | M-D | feature | done | WP-05b: remote mint (only if DR-4 is yes) (GitHub #31) |
| [T-032](live.md) | P2 | S | M-C | chore | blocked | Milestone M-C: the live gates G-H2 to G-H4 on a fake unit (GitHub #32) |
| [T-033](live.md) | P2 | S | M-D | chore | blocked | Milestone M-D: one live trusted job on a no-root host (GitHub #33) |
| [T-034](level2.md) | P2 | S | L2 | measurement | done | WP-13: measure U19 and U21 on the podvm rung (GitHub #34) |
| [T-035](level2.md) | P2 | M | L2 | feature | done | WP-09: the hook relay and the control socket server of each unit (ctl) (GitHub #35) |
| [T-036](level2.md) | P2 | S | L2 | feature | done | The guest floor self-test (GitHub #36) |
| [T-037](level2.md) | P2 | M | L2 | feature | done | WP-11: the microvm unit driver (form set by DR-5) (GitHub #37) |
| [T-038](level2.md) | P2 | S | L2 | feature | done | WP-12: the canned-message fake broker (level 2) (GitHub #38) |
| [T-039](level2.md) | P2 | S | L2 | chore | done | Milestone L2: level 2 green, the hook path measured (GitHub #39) |
| [T-040](level2.md) | P3 | M | L3 | chore | blocked | Level 3: the live acceptance suite (GitHub #40) |
| [T-041](later.md) | P3 | S | later | feature | done | Hold enforced on podvm units (pod-hold or pod-sud) (GitHub #41) |
| [T-042](later.md) | P3 | S | later | feature | done | The confined boundary (pod-confine) (GitHub #42) |
| [T-043](later.md) | P3 | S | later | feature | blocked | Later crates: service unit, release index, acceptance registry (GitHub #43) |
| [T-044](plan.md) | P2 | S | M-A | chore | done | COORD-001: DR-1 and DR-2 are yes; podnet's podrunner entries can start (GitHub #46) |
| [T-045](live.md) | P1 | M | M-D | feature | done | The mitigations of DR-4 for trusted jobs at redirect (GitHub #47) |
| [T-046](later.md) | P3 | L | later | feature | done | Local mode: podrunner run workflow.yml (GitHub #48) |
| [T-047](core.md) | P2 | S | M-A | defect | done | COORD-002: podvm wait gives no exit code for a short payload on the supervise rung (GitHub #49) |
| [T-048](core.md) | P2 | S | M-D | feature | done | Egress: check the upstream of the broker; the names mode of pod-broker |
| [T-049](level2.md) | P2 | S | L2 | chore | done | COORD-003: the stand-in broker of the official runner for podvm (GitHub #50) |
| [T-050](level2.md) | P2 | S | L2 | defect | done | COORD-004: the supervise rung does not map the path of connect() for a unix socket (GitHub #51) |
| [T-051](core.md) | P2 | S | M-C | defect | done | COORD-005: podvm-aarch64 declines an arm64 payload on the supervise rung (GitHub #52) |
| [T-052](core.md) | P1 | S | M-C | chore | done | COORD-006: a podvm beta from main for podrunner (GitHub #53) |
| [T-053](core.md) | P2 | M | L2 | defect | done | The conformance suites follow the numbers of the design contracts |
| [T-054](core.md) | P1 | M | M-D | defect | done | serve: an exclusive lock on the state directory before the journal replay; stop --wait (GitHub #54) |
| [T-055](core.md) | P1 | S | M-D | defect | done | journal: write and sync the claimed line before source.claim (GitHub #55) |
| [T-056](core.md) | P1 | S | M-D | defect | done | control socket: an idle client stalls the tick of the supervisor (GitHub #56) |
| [T-057](core.md) | P1 | S | M-D | defect | done | source wire: a reply deadline for each call; restart a silent sidecar (GitHub #57) |
| [T-058](core.md) | P2 | S | M-D | feature | done | podrunner check: validate a configuration and start nothing (GitHub #58) |
| [T-059](core.md) | P2 | S | M-D | feature | done | audit.jsonl: chain each line to the one before; podrunner audit verify (GitHub #59) |
| [T-060](core.md) | P2 | S | M-D | feature | done | serve: SIGTERM and SIGINT start a drain; sd_notify (GitHub #60) |
| [T-061](core.md) | P2 | S | L2 | defect | done | COORD-007: on the supervise rung an absolute symlink in the rootfs resolves on the host (GitHub #61) |
| [T-062](core.md) | P2 | S | L2 | defect | done | COORD-008: on the supervise rung a volume given as :ro is writable (GitHub #62) |
| [T-063](core.md) | P1 | M | L2 | defect | done | The tree kill finds a unit by an environment tag, and a process can drop it |
| [T-064](level2.md) | P1 | S | L2 | feature | done | The podvm driver follows the tree of each container (PodUtils/podvm@T-1709) |
| [T-065](live.md) | P1 | M | M-D | feature | done | The podvm unit runs under podnet pod: an advisory hold for trusted jobs |
| [T-066](live.md) | P1 | S | M-C | defect | done | slots sets ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE=1 in the start environment |
| [T-067](level2.md) | P1 | S | L2 | feature | done | The podvm driver takes the root of a unit from podvm inspect |
| [T-068](core.md) | P1 | M | M-D | defect | done | serve keeps each job, report and journal line forever |
| [T-069](core.md) | P2 | S | M-D | feature | done | A report that the source refuses is retried, then spooled |
| [T-070](core.md) | P2 | S | M-D | feature | done | podrunner doctor reads the configuration and the probes of each kind |
| [T-071](live.md) | P2 | M | M-C | chore | done | The live gate harness of G-H2 to G-H4, offline on the fake scale set |
| [T-072](core.md) | P2 | S | M-D | measurement | done | U10: the egress module against a proxy that resets CONNECT to an IP literal |
| [T-073](later.md) | P3 | S | later | chore | done | Take the podcrates tag after v0.24.0: pod-confine drops each capability |
| [T-074](core.md) | P2 | M | M-D | feature | done | The disk limit of a unit |
| [T-075](core.md) | P3 | S | M-D | feature | done | The report and status fields of SPEC §11.4 and §11.5 |
| [T-076](core.md) | P1 | M | M-D | feature | done | podrunner resolves names with pod-dns: no single resolver of the host |
| [T-077](core.md) | P3 | S | later | defect | done | COORD-009: the seccomp hold of podnet pod does not take ::ffff:127.0.0.1 as loopback (GitHub #64) |
| [T-078](core.md) | P3 | S | later | feature | done | The podvm driver labels each unit and reads the run report of podvm |
| [T-079](core.md) | P3 | M | later | measurement | done | Measure podvm --podvm-confine on the supervise rung, for a confined podvm unit |
| [T-080](core.md) | P3 | S | later | defect | done | Tests 25 and 26 skip on a confined podvm unit: the orphan is not seen on the host |
| [T-081](core.md) | P3 | M | later | measurement | done | Measure a musl build of the official runner (P0-4 of GitHub #67) |
| [T-082](core.md) | P2 | S | later | defect | blocked | The official runner v2.338.2 carries Microsoft.AspNetCore.WebUtilities 8.0.25 (GHSA-x33c-w48q-qrgc) |
| [T-083](core.md) | P2 | S | later | chore | done | Take podcrates v0.29.0 (PodUtils/podcrates#87, COORD-055) |
| [T-084](core.md) | P1 | S | later | defect | done | Conformance test 26 kills the launcher of podvm before it reports the start |
| [T-085](core.md) | P2 | S | later | docs | done | COORD-010: --podvm-confine runs on the userland rung; podrunner named the supervise rung (GitHub #68) |
| [T-086](core.md) | P2 | S | later | chore | done | Take podcrates v0.30.0 (PodUtils/podcrates#89, COORD-057) |
| [T-087](later.md) | P3 | M | later | feature | done | The local source: a workflow file into job offers and job results |
| [T-088](later.md) | P3 | M | later | feature | done | A workflow job into the job message of the fake broker |
| [T-089](later.md) | P3 | M | later | feature | done | podrunner run workflow.yml: the local mode end to end |
| [T-090](later.md) | P3 | M | later | feature | done | podrunner run: each job of the local mode in a fresh podvm unit |
| [T-091](later.md) | P3 | M | later | feature | done | Actions (uses:) in the local mode |
| [T-092](later.md) | P3 | M | later | feature | done | The checkout of the local tree and ./ actions in the local mode |
| [T-093](later.md) | P3 | M | later | feature | done | Docker actions in the local mode |
| [T-094](later.md) | P3 | S | later | chore | done | The local source owns its workflow: drop the leak (podcrates v0.31.0) |
| [T-095](later.md) | P3 | M | later | feature | done | podrunner run --podvm: the egress of each unit through the egress module |
| [T-096](core.md) | P3 | S | later | defect | done | GNU tar -C DIR fails in a podvm unit after mkdir -p DIR |
| [T-097](later.md) | P3 | S | later | chore | done | Build the format() token from Template::parts() (PodUtils/podcrates@T-322) |
| [T-098](later.md) | P3 | S | later | feature | done | Q4: a Docker action in a job with no container (the order rule of the hook server) |
| [T-099](core.md) | P2 | S | later | defect | done | The confined conformance test 27 hung to its time limit on podvm beta.6 |
| [T-100](core.md) | P2 | S | later | chore | done | The beta release carries the scale-set sidecar and its notes |
| [T-101](later.md) | P1 | M | later | feature | done | G-01: a source kind for an external program that speaks the source wire |
| [T-102](later.md) | P2 | S | later | feature | done | G-13: a table of the events of a job in the tier rule |
| [T-103](later.md) | P2 | L | later | feature | done | G-09: a container engine inside the unit, and the census of the verbs (DR-8) |
| [T-104](later.md) | P2 | S | later | feature | done | G-10: the source conformance tests as a command for another source program |
| [T-105](later.md) | P3 | M | later | feature | done | G-02: an `incus` unit kind for trusted jobs (a system container) |
| [T-106](later.md) | P1 | S | later | defect | done | The actor classes of githome (agent, bot, external) in the offer schema |
| [T-107](later.md) | P1 | S | later | defect | done | The egress policy of an offer: placement, the ceiling of the operator, the socket directory |
| [T-108](later.md) | P2 | S | later | feature | done | G-06: the CPU time and the peak memory of a job in its report |
| [T-109](later.md) | P1 | M | later | defect | done | The incus kind has no egress hold, so placement gives it no trusted job |
| [T-110](later.md) | P2 | S | later | defect | done | The microvm and incus kinds do not check the version of podvm |
| [T-111](later.md) | P3 | S | later | defect | done | The configuration accepts keys that nothing reads |
| [T-112](later.md) | P2 | S | later | defect | done | serve takes a podvm below beta.14 for an engine of the microvm kind |
| [T-113](later.md) | P2 | S | later | defect | done | doctor names no driver for the microvm and incus kinds |
| [T-114](later.md) | P2 | S | later | defect | done | The steps of the local mode read the github context of the fake broker, not that of the run |
| [T-115](later.md) | P3 | S | later | defect | done | podrunner run --runner-dir does not end a job at its timeout-minutes, and it panics on an I/O error |
| [T-116](later.md) | P3 | S | later | defect | done | check and doctor do not read the version of podvm for the microvm and incus kinds |
| [T-117](later.md) | P3 | S | later | defect | done | The local run ignores fail-fast and the continue-on-error of a job |
| [T-118](later.md) | P2 | M | later | feature | done | Local mode: actions by tag, fetched through the egress module into a store with a lock file |
| [T-119](later.md) | P2 | M | later | feature | done | Local mode: artifacts v4 (the ArtifactService of the results API), one store for each run |
| [T-120](later.md) | P2 | M | later | feature | done | Local mode: the cache service of actions/cache |
| [T-121](later.md) | P2 | S | later | feature | done | Local mode: secrets from the environment and from a secret file, with the mask |
| [T-122](later.md) | P2 | S | later | feature | done | Local mode: github.token and secrets.GITHUB_TOKEN |
| [T-123](later.md) | P2 | M | later | feature | done | Local mode: run each job in a unit of the microvm or the incus kind |
| [T-124](later.md) | P2 | M | later | feature | done | Local mode: services:, docker build and Docker-in-Docker through the engine of the unit |
| [T-125](later.md) | P2 | M | later | feature | done | Local mode: the event payload, the github context of the local tree, and the filters of on: |
| [T-126](later.md) | P3 | S | later | feature | done | Local mode: the inputs of workflow_dispatch, and files of vars, env and inputs |
| [T-127](later.md) | P2 | S | later | feature | done | Local mode: run one job (--job) or some cells of a matrix (--matrix) |
| [T-128](later.md) | P3 | S | later | feature | done | Local mode: --list, --graph, --dry-run and --validate |
| [T-129](later.md) | P2 | M | later | feature | done | Local mode: runner images by the labels of runs-on (--platform) |
| [T-130](later.md) | P2 | M | later | feature | done | Local mode: container images by tag, resolved to a digest and pulled into the store |
| [T-131](later.md) | P2 | L | later | feature | done | Local mode: reusable workflows (jobs.<id>.uses) |
| [T-132](later.md) | P3 | M | later | feature | done | Local mode: jobs in parallel (--jobs N, max-parallel) |
| [T-133](later.md) | P2 | M | later | feature | done | Local mode: the checkout with the changes of the tree, the .git directory, and other repositories |
| [T-134](later.md) | P2 | M | later | feature | done | Local mode: live logs, a log directory, the job summary, and --json |
| [T-135](later.md) | P3 | M | later | feature | done | Local mode: a run directory and reruns of failed jobs |
| [T-136](later.md) | P3 | S | later | feature | done | Local mode: environments, with their secrets, vars and egress policy |
| [T-137](later.md) | P3 | S | later | feature | done | Local mode: concurrency groups and cancel-in-progress |
| [T-138](later.md) | P3 | S | later | feature | done | Local mode: keep the unit of a failed job, export the workspace, and a shared tool cache |
| [T-139](later.md) | P3 | S | later | feature | done | Local mode: actions from a local directory (--local-repository) and from another forge |
| [T-140](later.md) | P3 | S | later | feature | done | Local mode: OIDC id-tokens (permissions: id-token: write) |
| [T-141](later.md) | P2 | S | later | defect | done | podrunner run leaves its units and scratch directories at Ctrl-C |
| [T-142](later.md) | P3 | S | later | feature | done | Local mode: --watch and the cron times of on.schedule |
| [T-143](later.md) | P3 | S | later | feature | done | Local mode: a configuration file for podrunner run |
| [T-144](later.md) | P2 | S | later | defect | done | podrunner run --runner-dir starts the runner with W^X on, and dies under a small RLIMIT_FSIZE |
| [T-145](later.md) | P2 | S | later | defect | done | The fake broker answers 404 in silence to a route that it does not serve |
| [T-146](later.md) | P2 | S | later | defect | done | A job can leave files that the next run cannot remove |
| [T-147](later.md) | P2 | M | later | feature | done | Local mode: the parity suite, with checks outside the job log |
| [T-148](later.md) | P3 | M | later | research | done | Research: Azure Pipelines YAML in the local source |
| [T-149](later.md) | P3 | M | later | feature | done | Local mode: a web view of a run on the loopback |
| [T-150](later.md) | P3 | M | later | feature | done | Local mode: a pause before a step and a shell in the unit |
| [T-151](later.md) | P3 | S | later | measurement | done | Measure: a job of another architecture in the local mode |
| [T-152](later.md) | P3 | S | later | feature | done | Local mode: an env of one expression is refused |
| [T-153](later.md) | P3 | S | later | feature | done | podrunner doctor for the local mode |
| [T-154](later.md) | P3 | S | later | feature | done | Local mode: the fetch of the actions through the egress module |
| [T-155](later.md) | P3 | S | later | defect | done | An intermittent failure of the workspace tests that the check script hid |
| [T-156](later.md) | P3 | S | later | defect | done | Local mode: a glibc image by tag in a podvm unit where chroot is denied |
| [T-157](later.md) | P3 | M | later | feature | done | Local mode: load the images of a job from the store into the engine of the guest |
| [T-158](later.md) | P3 | S | later | defect | done | Local mode: a static image as a container of a podvm unit |
| [T-159](later.md) | P2 | S | later | defect | done | An intermittent unknown count of processes at the destroy of a podvm unit |
| [T-160](later.md) | P3 | S | later | feature | done | Local mode: a shell in a paused job of the incus kind |
| [T-161](later.md) | P3 | M | later | research | partial | An alternate payload: a runner other than the official one |
| [T-162](later.md) | P2 | M | later | feature | done | G-12: limits of CPU, memory and processes on the podvm and incus kinds |
| [T-163](later.md) | P3 | S | later | chore | done | Take podvm 0.2.0-beta.19 and save the images of the mirror from the store of the units |
| [T-164](live.md) | P1 | S | M-C | defect | done | The scale-set sidecar ignores `wait_s` of `next`, and the unit host polls without a wait |
| [T-165](live.md) | P1 | S | M-C | defect | done | `sign` refuses each restarted sidecar for 60 s, and the backoff of the sidecar never grows |
| [T-166](live.md) | P1 | M | M-C | defect | done | The scale-set sidecar reads no `JobAssigned` message |
| [T-167](live.md) | P1 | S | M-C | defect | done | The trust lookup reads `visibility`, which a run gives as null |
| [T-168](live.md) | P2 | S | M-C | defect | done | The sidecar names each non-HTTP error "no answer", and never closes its message session |
| [T-169](live.md) | P1 | M | M-C | defect | partial | A withdrawn job holds the capacity of the scale set |
| [T-170](live.md) | P2 | S | M-C | defect | done | A withdrawal by the tier rule leaves no trace in the mint |
| [T-171](live.md) | P2 | S | M-C | defect | done | No command checks a mint file before `mint-serve` |
| [T-172](live.md) | P3 | S | M-C | defect | done | The audit of `sign` lives only in memory |
| [T-173](live.md) | P3 | S | M-C | defect | done | `podrunner --version` does not name the beta |
| [T-174](live.md) | P3 | S | later | defect | done | A trusted source cannot use a fine-grained organization PAT |
| [T-175](live.md) | P1 | M | M-C | defect | partial | A runner of the scale set is not bound to its job: a foreign job is found only at `started` |
| [T-176](live.md) | P1 | M | M-C | defect | done | The JIT configuration is minted before the unit starts: a failed start spends it and leaves the job assigned |
| [T-177](live.md) | P1 | S | M-C | defect | done | `serve` does not pull an image that a digest pins: a `container:` job fails with no cause in the log |
| [T-178](live.md) | P1 | S | M-C | defect | done | A job on the incus and microvm kinds has `HOME=/root` for a runner that is not root: `actions/checkout` fails |
| [T-179](live.md) | P1 | M | M-C | defect | done | On an H0 host the runner times out on the broker under the seccomp hold and the egress module |
| [T-180](live.md) | P2 | S | M-C | defect | partial | As PID 1 of a container, `serve` reaps no orphan: the zombies stop the supervisor |
| [T-181](live.md) | P2 | M | M-C | defect | partial | `--podvm-confine` stops name resolution in a podvm unit: the runner cannot reach the broker |
| [T-182](live.md) | P2 | S | M-C | defect | done | A podvm unit has the host name of the host: the job log of GitHub shows it |
| [T-183](live.md) | P2 | S | M-C | defect | done | The tree-kill self-test of a held podvm unit fails, and its units stay after it |
| [T-184](live.md) | P2 | S | M-C | defect | done | `status` answers `busy` while the supervisor waits on a call of a source |
| [T-185](live.md) | P3 | S | M-C | defect | done | `check`, `doctor` and `status` give a capacity that `serve` does not have |
| [T-186](live.md) | P3 | S | M-C | defect | partial | The store of podvm keeps `machine/run-PID-*` after each guest of the microvm kind |
| [T-187](live.md) | P3 | S | later | defect | partial | The local mode refuses the netns hold |
| [T-188](live.md) | P3 | S | M-C | defect | done | The tier intake names each parse failure of an offer `event-not-allowed`, and the offer schema of the design differs from the parser |
| [T-189](live.md) | P2 | S | M-C | defect | done | A program source re-offers a refused job at the head of each `next` |
| [T-190](live.md) | P3 | S | later | defect | done | `source-conformance` needs two offers on the first `next` of each test, and its claims stay |
| [T-191](live.md) | P3 | S | later | defect | done | The tier rule takes only an organization as the owner of a trusted repository |
| [T-192](live.md) | P1 | M | M-C | defect | partial | `serve` stops after a unit ends `unit-failed` |
| [T-193](live.md) | P3 | M | later | feature | done | A `redirect` host serves one repository only, so it cannot hold two sources |
| [T-194](live.md) | P3 | S | later | feature | done | An emulated aarch64 host (QEMU user mode) serves no tier |
| [T-195](live.md) | P3 | S | M-C | defect | done | Issue #75 names the expiry of the artifacts without the time, and not `runner-full-x86_64` |
| [T-196](later.md) | P2 | S | later | defect | done | The level2 corpus fails on podcrates jobs that read live state |
| [T-197](live.md) | P2 | S | M-C | defect | partial | A unit that `start_s` ends names no cause, and a slow host needs a larger `start_s` |
| [T-198](live.md) | P2 | S | M-C | feature | done | A unit applies the limits of its offer |
| [T-199](later.md) | P2 | S | later | chore | blocked | The local mode is not tried by a user on real workflows |
| [T-200](live.md) | P2 | S | M-C | defect | done | A refused claim gets `release` before its report, so the source cannot tell a refusal from a give-back |
| [T-201](live.md) | P3 | S | M-C | defect | done | A job that the job-started hook ends has no annotation of its cause |
| [T-202](later.md) | P1 | M | later | feature | done | One installer and one offline bundle for the local mode |
| [T-203](later.md) | P2 | S | later | defect | done | A local checkout with no --checkout fails with a 404 |
| [T-204](later.md) | P3 | S | later | chore | open | The packages of the image archives and their sources |
| [T-205](later.md) | P1 | S | later | release | done | The release attaches the assets of the local mode |
| [T-206](later.md) | P2 | S | later | defect | done | `--platform` turns off each later check of the flags of `run` |
| [T-207](later.md) | P3 | S | later | defect | done | The refusal of an executor of `run` does not name the flag that clashes |
| [T-208](later.md) | P2 | S | later | defect | done | No command has its own help, and a short option of `run` becomes a file |
| [T-209](later.md) | P3 | S | later | defect | done | `run --validate` says valid for a dangling need and a cycle |
| [T-210](later.md) | P3 | S | later | defect | done | `--break` takes a job, a step and a step id that the workflow does not have |
| [T-211](later.md) | P2 | S | later | defect | done | `--local-repository` still opens the relay of the fetch |
| [T-212](later.md) | P3 | S | later | defect | done | `doctor --run` answers a missing podvm binary with a version of podvm |
| [T-213](later.md) | P2 | S | later | defect | done | A loopback bind that the host refuses is a raw error, and `doctor --run` does not try it |
| [T-214](later.md) | P3 | S | later | defect | done | `status`, `drain`, `stop`, `check` and `audit verify` do not name the path that is missing |
| [T-215](later.md) | P3 | S | later | defect | done | `stop --wait` says stopped for a state directory that does not exist |
| [T-216](later.md) | P3 | S | later | defect | done | `mint-serve` gives a TOML type error for a file of `serve` |
| [T-217](later.md) | P1 | S | later | defect | done | The artifact `runner-ARCH.tar` cannot be loaded by the pinned podvm and has no ref name |
| [T-218](later.md) | P3 | S | later | defect | done | The nits of the local-mode user run |
| [T-219](later.md) | P3 | S | later | defect | done | An unknown `--event` gets the text of the `on:` filter, and `--event-path` is read after the filter |
| [T-220](later.md) | P2 | M | later | feature | done | The local mode cannot run on a host that refuses each AF_INET bind |
| [T-221](later.md) | P3 | S | later | feature | done | `cpus` on the podvm kind |
| [T-222](live.md) | P1 | S | M-C | defect | done | A refusal of one source holds the job for each source of the host |
| [T-223](later.md) | P2 | S | later | defect | done | `doctor --run` says ok for a store that does not hold `--image` |
| [T-224](later.md) | P2 | M | later | feature | done | A run that hangs prints nothing until it ends |
| [T-225](later.md) | P2 | S | later | defect | done | A unit that has no room says `PODVM_STORE`, which podrunner sets itself |
| [T-226](later.md) | P2 | S | later | defect | done | A run does not say what egress hold its units have |
| [T-227](later.md) | P2 | S | later | defect | done | `podvm run -d` refuses a passed descriptor when podrunner runs under `podnet pod` |
| [T-228](later.md) | P2 | S | later | defect | done | A listener of the loopback needs `getsockname`, which `podnet pod` refuses |
| [T-229](later.md) | P2 | M | later | feature | open | The run should pick its transport, its stores and its hold, and say so |
| [T-230](later.md) | P1 | S | later | defect | done | With no `--platform`, a job of Windows, macOS or another architecture runs on the host |
| [T-231](later.md) | P3 | S | later | defect | done | Ctrl-C in the wait of `--schedule` exits 0 |
| [T-232](later.md) | P1 | S | later | defect | done | A tar with a link before its target fails in a podvm unit |
| [T-233](later.md) | P2 | S | later | defect | done | The egress of a job ignores the proxy of the host |
| [T-234](later.md) | P2 | S | later | defect | done | A job on `--runner-dir` drops the proxy of the host |
| [T-235](later.md) | P3 | S | later | defect | open | Each log line of a job comes two times, with a BOM |
| [T-236](later.md) | P3 | S | later | defect | open | The events of `--json` share stderr with the console of the runner |
| [T-237](later.md) | P2 | S | later | defect | done | A Dockerfile action on `--podvm` fails at its step, not before the job |
| [T-238](later.md) | P2 | S | later | defect | done | `--artifacts DIR` gives 409 on a second run |
| [T-239](later.md) | P2 | S | later | defect | done | `--action-server` on the loopback or a private address is denied |
| [T-240](later.md) | P3 | S | later | defect | open | `doctor --run --incus` does not try incus |
| [T-241](later.md) | P3 | S | later | defect | open | The small findings of the second QA report |
| [T-242](later.md) | P2 | M | later | defect | partial | Each run copies the runner image several times into a new directory |
| [T-243](later.md) | P1 | M | later | release | partial | The user-path gate: the files of the release, installed and used as a user |
| [T-244](later.md) | P1 | M | later | release | partial | The host lanes of the user-path gate |
| [T-245](later.md) | P2 | S | later | chore | partial | The command line as a contract |
| [T-246](later.md) | P1 | S | later | release | partial | An adversarial QA before each release |
| [T-247](later.md) | P2 | S | later | release | done | The notes of a release name the level of the proof of each claim |
| [T-248](later.md) | P1 | M | later | feature | done | The help and the manual of podrunner from the shared crate of PodUtils |
| [T-249](later.md) | P1 | M | later | feature | partial | The installer on each host class, and the shared core of podcrates |
| [T-250](later.md) | P2 | S | later | chore | partial | The state of each issue comes from the record by a tool |
| [T-251](later.md) | P1 | S | later | defect | done | The gate and the host of an agent run shellcheck of other versions |
| [T-252](later.md) | P2 | S | later | chore | open | No workflow is linted before it runs |
| [T-253](later.md) | P1 | S | later | defect | done | The faults of the first run of the user-path gate |
| [T-254](later.md) | P1 | S | later | defect | done | The made-up `github.token` fails each action that calls the API of a real forge |
| [T-255](later.md) | P2 | S | later | defect | done | On `--runner-dir`, the tests of express miss headers and hang |
| [T-256](later.md) | P1 | S | later | defect | done | The checks of a session do not run the suites that need the official runner |
| [T-260](later.md) | P2 | S | later | defect | done | An expression in the image of a service or a job container fails the job |
| [T-261](later.md) | P2 | S | later | defect | partial | In a podvm unit, the persist of a temporary file gets EACCES |
| [T-262](later.md) | P2 | S | later | defect | done | In a podvm unit, Node sometimes reads an invalid package.json |
| [T-263](later.md) | P1 | S | later | defect | done | The installer keeps a directory that the group can write |
| [T-264](later.md) | P2 | M | later | chore | partial | Take podcrates `v0.55.0`: the sync of the issues and pod-lab |
| [T-265](later.md) | P2 | S | later | defect | done | A job on `--runner-dir` does not get the PATH, the proxy and the CA of the user |
| [T-266](later.md) | P2 | S | later | defect | done | A job on `--runner-dir` writes into the home of the user |
| [T-267](later.md) | P2 | S | later | defect | done | `--checkout .` gives no `.git`, so a step that runs `git` fails |
| [T-268](later.md) | P2 | S | later | defect | done | The lock of the actions goes into the next checkout as a new file |
| [T-269](later.md) | P2 | S | later | defect | done | The made-up token fails `actions/setup-python` with a 404, and the run names no fix |
| [T-270](later.md) | P2 | S | later | defect | done | A Dockerfile action on `--runner-dir` starts the job and calls the docker of the host |
| [T-271](later.md) | P2 | S | later | defect | done | `--job` does not select a need that calls a reusable workflow |
| [T-272](later.md) | P2 | S | later | defect | done | `--dry-run` on `--podvm` plans a `services:` job and does not check the image |
| [T-273](later.md) | P2 | S | later | defect | done | A SIGHUP leaves the runner running and breaks the runner directory |
| [T-274](later.md) | P2 | S | later | defect | done | Two runs on one runner directory at the same time break each other |
| [T-275](later.md) | P1 | S | later | defect | done | A project file reads a host file through a symbolic link |
| [T-276](later.md) | P1 | S | later | defect | done | `.secrets` of a checkout goes into the workspace of each job |
| [T-277](later.md) | P3 | M | later | defect | partial | Some refusals name the wrong cause or a fix that does not work |
| [T-278](later.md) | P3 | S | later | defect | done | The output, the exit codes and the help disagree with the documents |
| [T-279](later.md) | P3 | S | later | defect | done | `--rerun --failed`, `--validate` and `--schedule` disagree with GitHub or the documents |
| [T-280](later.md) | P3 | S | later | defect | done | `--break` on `--runner-dir` speaks of a unit, and two flags pass together |
| [T-281](later.md) | P3 | S | later | defect | done | A runner directory with a space, and the growth of `_diag` |
| [T-282](later.md) | P3 | S | later | defect | done | Each live log line comes twice |
| [T-283](later.md) | P3 | S | later | defect | done | `doctor` reports mount and chroot as denied where a user namespace mounts |
| [T-284](later.md) | P2 | S | later | defect | partial | The user-path gate has no lane of a host with a proxy only |
| [T-285](later.md) | P2 | M | later | defect | open | A Dockerfile action on `--podvm` gets no container hooks |
| [T-286](later.md) | P3 | S | later | defect | done | A cell that a signal cancelled is not in `state.json` |
| [T-287](later.md) | P2 | S | later | defect | done | An empty token does not reach a job of `permissions: {}`, and the hint names a fix that fails |
| [T-288](later.md) | P2 | S | later | defect | done | A Docker action of the tree on `--runner-dir` starts its job |
| [T-289](later.md) | P3 | S | later | defect | done | The P3 findings of the second QA, agent A, part 1 |
| [T-290](later.md) | P3 | M | later | defect | open | The P3 findings of the second QA, agent A, part 2 |
| [T-291](later.md) | P2 | S | later | defect | done | As a user with no root, the machine tier ends before the exit code of the runner |
| [T-292](later.md) | P2 | S | later | defect | done | A project file reads a file out of the project through `.actrc` or a link |
| [T-293](later.md) | P2 | S | later | defect | done | `github.api_url` of a job names the fake broker |
| [T-294](later.md) | P2 | S | later | defect | done | A runner that a SIGKILL of podrunner leaves breaks the runner directory |
| [T-295](later.md) | P2 | S | later | defect | done | A rerun with a matrix filter rewrites a failed cell as skipped |
| [T-296](later.md) | P2 | S | later | defect | done | The lines of the results come at the end, and a SIGHUP loses them |
| [T-297](later.md) | P2 | S | later | defect | done | `--microvm` does not check its image and `/dev/kvm` before the run |
| [T-298](later.md) | P3 | M | later | defect | open | The P3 findings of the second QA, agent B |
| [T-299](later.md) | P2 | S | later | defect | done | In the lane proxy-only, the lint of rq on `--runner-dir` fails |
| [T-300](later.md) | P1 | M | later | defect | done | A job in a podvm unit writes into the home of the user on the host |
| [T-301](later.md) | P1 | S | later | defect | done | With podvm 0.2.0-beta.25 the runner of a podvm unit ends with no result |
| [T-302](later.md) | P1 | S | later | defect | done | The egress hold of the installer refuses each run where user namespaces exist |
| [T-303](later.md) | P2 | S | later | defect | done | An executor flag does not replace the executor of run.toml |
| [T-304](later.md) | P2 | S | later | defect | done | The checkout of the tree answers 404 to later jobs of a run |
| [T-305](later.md) | P2 | S | later | defect | done | actions/setup-python v7 fails with the made-up token |
| [T-306](later.md) | P2 | S | later | defect | done | On --runner-dir, a Python of setup-python cannot load libpython |
| [T-307](later.md) | P2 | S | later | defect | done | The documents load the larger image into the wrong store |
| [T-308](later.md) | P3 | M | later | defect | open | The P3 findings of the third QA, agent A |
| [T-309](later.md) | P1 | S | later | defect | done | A podvm unit reports ok for a failing test suite of npm 6 |
| [T-310](later.md) | P2 | S | later | defect | done | The live log of a step comes only when the step ends |
| [T-311](later.md) | P2 | S | later | defect | done | container: and docker:// steps on podvm are planned and then refused |
| [T-312](later.md) | P2 | S | later | defect | done | A SIGKILL during a podvm job leaves the unit and its files |
| [T-313](later.md) | P2 | S | later | defect | done | A failed --export is silent |
| [T-314](later.md) | P2 | S | later | defect | done | A relative trusted path is read from the current directory |
| [T-315](later.md) | P2 | S | later | defect | done | The runner images have no sudo and lack tools of ubuntu-latest |
| [T-316](later.md) | P3 | M | later | defect | open | The P3 findings of the third QA, agent B |
| [T-317](later.md) | P2 | S | later | defect | done | On the namespace rung of podvm the job runs as root of a one-uid namespace |
| [T-318](later.md) | P1 | M | later | defect | done | A podvm unit shares the processes of the host: a job can kill processes of the user |
| [T-319](later.md) | P1 | S | later | defect | done | The egress hold of the local mode lets a job out through a proxy on the loopback |
| [T-320](later.md) | P2 | S | later | defect | done | Under the egress hold a Node action does not take the CA of the host |
| [T-321](later.md) | P2 | S | later | defect | done | doctor --run does not check the paths of many flags |
| [T-322](later.md) | P2 | S | later | defect | done | An unwritable --log-dir is found after the run, with exit 0 |
| [T-323](later.md) | P3 | M | later | defect | open | The P3 findings of the fourth QA, agent B |
| [T-324](later.md) | P2 | M | later | defect | done | The base runner image has no make, libatomic1 or python3 |
| [T-325](later.md) | P2 | M | later | defect | done | A podvm unit needs more scratch disk than doctor says; --jobs 2 fills the disk |
| [T-326](later.md) | P2 | S | later | defect | done | A uv build in a podvm unit on the base image fails at utime of its cache |
| [T-327](later.md) | P2 | M | later | defect | done | A run pulls the images of services: before it refuses the job |
| [T-328](later.md) | P2 | S | later | defect | done | Under the egress hold, a refused host is not named with its fix |
| [T-329](later.md) | P2 | S | later | defect | done | Each unit under --podnet logs connects that the job did not make |
| [T-330](later.md) | P2 | S | later | defect | done | The refusals of services: send the user in a circle |
| [T-331](later.md) | P3 | M | later | defect | open | The P3 findings of the fifth QA, agent 1 |
| [T-332](later.md) | P3 | S | later | defect | done | podvm connects to the hosts of the image at the start of a unit |
| [T-333](later.md) | P2 | S | later | defect | done | A dry run with --state writes success for each job; --dry-run --json says success |
| [T-334](later.md) | P2 | S | later | defect | done | --artifacts DIR or --state DIR of an earlier run makes each upload of the same name fail with 409 |
| [T-335](later.md) | P2 | S | later | defect | done | --state DIR with --artifacts DIR2 keeps no artifact, and --rerun fails |
| [T-336](later.md) | P2 | S | later | defect | done | actions/checkout outside a git tree with no --checkout fails with a curl 404 |
| [T-337](later.md) | P2 | S | later | defect | done | From a subdirectory of a git tree, the checkout holds only that subdirectory |
| [T-338](later.md) | P2 | S | later | defect | done | --no-cache makes each actions/cache step retry for about a minute |
| [T-339](later.md) | P3 | M | later | defect | open | The P3 findings of the fifth QA, agent 2 |
| [T-340](later.md) | P2 | S | later | defect | done | A workflow file whose symbolic link leads out of the project is followed |
| [T-341](later.md) | P2 | S | later | defect | done | The `.secrets` file of the tree skips the checks of `--secret-file` |
| [T-342](later.md) | P3 | S | later | defect | done | `doctor --run --json` gives no JSON; `doctor --json --run` is an unknown option |
| [T-343](later.md) | P3 | S | later | defect | done | `print-unit` writes values in `ExecStart` with no quotes, and takes a relative or empty `--binary` |
| [T-344](later.md) | P3 | S | later | defect | done | `image verify` refuses the runner image of the release, and the error does not name the file it takes |
| [T-345](later.md) | P3 | M | later | defect | done | A dry run writes the artifact and cache directories, and skips the checks of some executor paths |
| [T-346](later.md) | P2 | M | later | defect | done | Nested `uses:` of a local composite action are not pinned; the refusal says offline mode with no `--offline` |
| [T-347](later.md) | P3 | S | later | defect | done | `--watch` and `--schedule` with a one-shot mode wait for ever; `--schedule` of a workflow with no `on.schedule` gives a bad message |
| [T-348](later.md) | P3 | S | later | defect | done | `--validate --json` gives no JSON for a workflow that is not valid |
| [T-349](later.md) | P3 | S | later | defect | done | A failure before the start of a job names a log in `--log-dir` that is not there |
| [T-350](later.md) | P2 | M | later | defect | done | beta.7 pins podnet `0.4.0-beta.13`; the vnet forward of #170 is in `0.4.0-beta.14` |
| [T-351](later.md) | P1 | M | later | defect | partial | The egress module resolves each name itself, so a host with DNS only through its proxy gets 502 for each fetch and each allowed host |
| [T-352](later.md) | P3 | S | later | defect | open | `image verify` cannot verify the runner image of the release |
| [T-353](later.md) | P2 | M | later | defect | partial | The runner image has no `less` |
| [T-354](later.md) | P2 | M | later | defect | done | A podvm unit has no devpts: `openpty` fails |
| [T-355](later.md) | P2 | M | later | defect | partial | A failed route of the egress module is reported as a host that the policy refused |
| [T-356](later.md) | P2 | M | later | defect | done | A run does not check `--toolcache` before its first unit |
| [T-357](later.md) | P3 | S | later | defect | open | `--state FILE` is named as `--artifacts`; a `--log-dir` with a missing parent does not name the parent |
| [T-358](later.md) | P3 | S | later | defect | open | `check --config run.toml` gives a serve error with no hint |
| [T-359](later.md) | P3 | S | later | defect | open | `podrunner --bogus` says "an unknown command" |
| [T-360](later.md) | P3 | S | later | defect | open | `--no-git`, `--with-git` and `--checkout-head` do not apply to the default checkout |
| [T-361](later.md) | P3 | S | later | defect | open | The line of a concurrency wait comes after the run, not when the wait starts |
| [T-362](later.md) | P3 | S | later | defect | open | `print-unit` writes a relative `--config` and `--state` into `ExecStart` |
| [T-363](later.md) | P3 | S | later | defect | open | The stop line of a signal names a running job when no job runs |
| [T-364](later.md) | P3 | S | later | defect | open | A raw seed file gives `invalid data`, not the format |
| [T-365](later.md) | P3 | S | later | defect | open | A `--var`, `--input` or `--env` of two config files is refused, not overridden |
| [T-366](later.md) | P3 | S | later | defect | open | `serve` gives no ready line in the foreground |
| [T-367](later.md) | P3 | S | later | defect | open | An abort of podrunner leaves no cause; two runs at once may abort |
| [T-368](later.md) | P3 | S | later | defect | open | The two executors give a different console |
| [T-369](later.md) | P3 | S | later | defect | open | The installer names no command to add its binary directory to PATH |
| [T-370](later.md) | P3 | S | later | defect | open | A run does not say which configuration files it read |
| [T-371](later.md) | P1 | M | later | defect | partial | The user gate has no lane of a host with DNS only through an authenticated proxy |
| [T-372](later.md) | P1 | M | later | release | partial | The user gate checks success paths only |
| [T-373](later.md) | P2 | S | later | defect | done | `--runner-dir` writes the runner files, a private key with them, at mode 0644 |
| [T-374](later.md) | P2 | S | later | defect | done | `--state` writes the outputs of the jobs into a file that each user can read |
| [T-375](later.md) | P2 | S | later | defect | done | `--checkout` of a directory in a git tree gets a `.git` of the whole tree |
| [T-376](later.md) | P2 | S | later | defect | done | Each job leaves the thread and the listener of its fake broker |
| [T-377](later.md) | P2 | S | later | defect | done | Two concurrency groups can share one lock file |
| [T-378](later.md) | P3 | S | later | defect | done | `--break-on-failure` pauses after each step after a failure; `--break` before an `always()` step is skipped |
| [T-379](later.md) | P3 | S | later | defect | done | `timeout-minutes` is held to 6 days |
| [T-380](later.md) | P2 | S | later | defect | done | The image of an excluded cell of a matrix is pulled |
| [T-381](later.md) | P2 | S | later | defect | done | `run DIR --checkout DIR` replaces the value of `--checkout` with a workflow file |
| [T-382](later.md) | P3 | S | later | defect | done | The test of the plants of the fake unit is not stable under load |
| [T-383](later.md) | P1 | S | later | defect | done | A container mount can name the runner root or a file of it |
| [T-384](later.md) | P1 | S | later | defect | done | A secret that starts a longer secret leaves the rest of the longer one in a log |
| [T-385](later.md) | P3 | S | later | defect | done | `--github-token-env ""` fails, though the documents say it sends no token |
| [T-386](later.md) | P2 | S | later | defect | done | A `pull_request` filter of branches reads the default branch, not the base branch |
| [T-387](later.md) | P3 | S | later | defect | done | A `workflow_dispatch` with no inputs takes each `--input` |
| [T-388](later.md) | P2 | S | later | defect | done | `check` passes a `mem` of `[unit.podvm]` or `[unit.microvm]` that `serve` refuses |
| [T-389](later.md) | P3 | S | later | defect | done | The control socket takes a field that an op does not take |
| [T-390](later.md) | P2 | S | later | defect | done | An error of a source whose text starts with "the " ends its sidecar |
| [T-391](later.md) | P2 | S | later | defect | done | A stale first payload of an architecture hides a fresh one after it |
| [T-392](later.md) | P3 | S | later | defect | done | The affinity of a shared principal compares a repository with case |
| [T-393](later.md) | P2 | S | later | defect | done | A dry run ends the processes and removes the files of a dead run |
| [T-394](later.md) | P2 | S | later | defect | done | The parity suite pulls from Docker Hub with no login and meets its limit |
| [T-395](later.md) | P2 | S | later | defect | done | The actions and images of a called workflow of the tree are not pinned before the first job |
| [T-396](later.md) | P2 | S | later | defect | done | A later matrix `include` does not overwrite a value that an earlier one added |
| [T-397](later.md) | P2 | S | later | defect | done | A matrix `exclude` with an expression drops each cell |
| [T-398](later.md) | P3 | S | later | defect | done | `--vnet` gives no `NODE_USE_ENV_PROXY` |
| [T-399](later.md) | P3 | S | later | defect | done | `--offline` with a store of actions that the user cannot write is refused |
| [T-400](later.md) | P2 | S | later | defect | done | A local composite action of `./../PATH` or a link out of the tree is read |
| [T-401](later.md) | P3 | S | later | defect | done | A dry run with a runner directory that cannot take the lock says planned |
| [T-402](later.md) | P2 | S | later | defect | done | The mint forgets a late credential that starts or goes back after its alert |
| [T-403](later.md) | P1 | M | later | defect | partial | A mount source of the hooks follows a symbolic link that the job made |
| [T-404](later.md) | P1 | M | later | defect | done | In the local mode the logs of a hook step are in the socket directory that the unit writes |
| [T-405](later.md) | P1 | M | later | defect | done | Secrets of a job and the credential of the proxy go into the argv of podvm |
| [T-406](later.md) | P1 | M | later | defect | done | The fake broker of the local mode gives the job message to each local process |
| [T-407](later.md) | P2 | M | later | defect | done | Directories of a run with a fixed name in the shared `/tmp` take a directory that another user made |
| [T-408](later.md) | P2 | M | later | defect | done | With `--export` the work directory of a job is 0777 in `/tmp` |
| [T-409](later.md) | P2 | M | later | defect | done | The signed pin list trusts the signer key of the same file |
| [T-410](later.md) | P2 | M | later | defect | done | With no `[egress] default_policy`, an offer with `mode: open` opens the egress |
| [T-411](later.md) | P3 | M | later | defect | open | The `id` of a configured organization or repository is not checked |
| [T-412](later.md) | P3 | M | later | defect | open | The front of a guest reads a request line with no limit |
| [T-413](later.md) | P3 | M | later | defect | open | Text that a job controls reaches stderr with no escape |
| [T-414](later.md) | P3 | M | later | defect | open | The audit line of a failed start carries the text of the driver to the source |
| [T-415](later.md) | P2 | M | later | defect | done | A matrix job keeps the outputs of its last cell only |
| [T-416](later.md) | P2 | M | later | defect | done | A job of a called workflow ignores the `permissions` of the caller |
| [T-417](later.md) | P2 | M | later | defect | done | A run can hold one concurrency group two times |
| [T-418](later.md) | P2 | M | later | defect | done | `github.run_id`, `run_number` and `run_attempt` are always 1 |
| [T-419](later.md) | P2 | M | later | defect | done | `--protected` misses jobs of called workflows, needs of `--job` and environment names of an expression |
| [T-420](later.md) | P2 | M | later | defect | done | An environment name of an expression is not evaluated |
| [T-421](later.md) | P2 | M | later | defect | done | No checkout tree when only a called workflow uses `actions/checkout` |
| [T-422](later.md) | P2 | M | later | defect | done | The stand-in of `actions/checkout` needs `curl` in a job container |
| [T-423](later.md) | P2 | M | later | defect | done | `--schedule` reads no `timezone` of `on.schedule` |
| [T-424](later.md) | P2 | M | later | defect | done | The minimal payload of `pull_request` has no `head.repo` and `base.repo` |
| [T-425](later.md) | P2 | M | later | defect | done | A git submodule is an empty directory in the checkout |
| [T-426](later.md) | P3 | M | later | defect | done | `github.workflow` is the file name, not the `name:` of the workflow |
| [T-427](later.md) | P3 | M | later | defect | done | `--rerun DIR --job CALLER` reruns nothing of a call |
| [T-428](later.md) | P3 | M | later | defect | done | A directory run with `--artifacts` or `--state` keeps the output of the last workflow only |
| [T-429](later.md) | P3 | M | later | defect | done | A line of a secret, var or env file with `<<` in its value is refused |
| [T-430](later.md) | P3 | M | later | defect | done | `--changed-since` does not see new files that git does not track |
| [T-431](later.md) | P3 | M | later | defect | open | The `secrets:` of a call see only the `secrets` context |
| [T-432](later.md) | P3 | M | later | defect | done | A concurrency group queues each waiter |
| [T-433](later.md) | P3 | M | later | defect | done | `--schedule` runs on the current branch and takes a cron under 5 minutes |
| [T-434](later.md) | P3 | M | later | defect | done | The OIDC claims of a called job name the caller |
| [T-435](later.md) | P2 | S | later | defect | done | mint: one thread for each TCP connection with no cap, each waiting up to 90 s |
| [T-436](later.md) | P2 | S | later | defect | done | guest: the self-test needs a loopback bind, so a host that denies loopback can never claim guest |
| [T-437](later.md) | P2 | S | later | defect | done | placement: a required limit that the kind reports is refused, although the docs allow reported |
| [T-438](later.md) | P2 | S | later | defect | done | policy: a rule with an unknown field is accepted and the field is dropped, broadening the policy |
| [T-439](later.md) | P2 | S | later | defect | done | run: a checkout of another repository with an empty, full or SHA ref is not resolved |
| [T-440](later.md) | P3 | S | later | defect | done | run: fetch::host_of cuts an IPv6 action-server address at the first colon |
| [T-441](later.md) | P2 | S | later | defect | done | run: a runs-on label of ppc64le, s390x, riscv64, i386, i686 or armv7 is not refused |
| [T-442](later.md) | P3 | S | later | defect | done | serve: a malformed offer is refused as event-not-allowed with no detail, not bad-request |
| [T-443](later.md) | P2 | S | later | defect | done | serve: a job-completed hook event is lost when the unit ends before the next tick |
| [T-444](later.md) | P2 | S | later | defect | done | podvm: a start that fails after run -d leaves the podvm unit, because destroy skips podvm rm |
| [T-445](later.md) | P3 | S | later | defect | done | source-conformance: a timed-out test leaves its thread and child process alive |
| [T-446](later.md) | P3 | S | later | defect | done | serve: two source names that map to the same spool file lose one refused report |
| [T-447](later.md) | P2 | S | later | defect | done | run: a lock key or uses: with ../x@v1 is accepted and the fetch URL normalizes to another repository |
| [T-448](later.md) | P2 | S | later | defect | done | check: a podvm config with a bad upstream_proxy, an old podvm or a bad podnet passes check and stops serve |
| [T-449](later.md) | P2 | S | later | defect | done | check_driver: a non-executable driver and a writable ancestor are accepted |
| [T-450](later.md) | P3 | S | later | defect | done | run: a dry run with a --vnet podnet binary that cannot build the shim says planned |
| [T-451](later.md) | P2 | S | later | defect | done | audit: a failed write still appears in the in-memory lines and the next line reuses its seq |
| [T-452](later.md) | P2 | S | later | defect | done | guest: the self-test skips the first unit's file check when the driver reports no unit_root |
| [T-453](later.md) | P3 | S | later | defect | done | status: no_tier_served ignores the egress hold, so it says a tier is served when every offer is refused |
| [T-454](later.md) | P2 | S | later | defect | done | serve: a spooled report of a source that is no longer configured is skipped forever |
| [T-455](later.md) | P3 | S | later | defect | done | serve: a bad journal line is counted in Replay.bad_lines and never reported |
| [T-456](later.md) | P2 | S | later | defect | done | check: a pin payload with an invalid arch (riscv64, x86-64) is accepted and then refuses every job |
| [T-457](later.md) | P3 | S | later | defect | done | check: an empty [unit.podvm] command, a relative runner_dir and a bad registry host are accepted |
| [T-458](later.md) | P3 | S | later | defect | done | guest: the microvm and incus scripts set HTTP(S)_PROXY but not NODE_USE_ENV_PROXY |
| [T-459](later.md) | P2 | S | later | defect | done | incus: the instance name is a 32-bit hash, so two state directories can collide |
| [T-460](later.md) | P2 | S | later | defect | done | microvm: destroy of a unit that is not in the map leaves its directory and reports a residue |
| [T-461](later.md) | P2 | S | later | defect | done | run: a host path with a colon breaks the -v mount of a unit (toolcache, export, container mount) |
| [T-462](later.md) | P2 | S | later | defect | done | microvm: remove_runs_of uses the podvm PID, which can be reused before the run directory is removed |
| [T-463](later.md) | P2 | S | later | defect | done | hooks: a job container with no entry point is started with tail, which a distroless image does not have |
| [T-464](later.md) | P2 | S | later | defect | done | podvm: a config without [unit.podvm] runner_dir gives the hook policy an empty work_root and refuses every container job |
| [T-465](later.md) | P2 | S | later | defect | done | check: tiers::served ignores the egress hold, so a config with no hold passes check and serves nothing |
| [T-466](later.md) | P2 | S | later | defect | done | shim: the close_range fallback closes only 3..4096, so a higher descriptor leaks into the payload |
| [T-467](later.md) | P3 | S | later | defect | done | mint: the revocation message calls the source name a credential |
| [T-468](later.md) | P2 | S | later | defect | done | check: a guest_kinds entry that is not a [unit.NAME] table passes check and drops the guest claim |
| [T-469](later.md) | P3 | S | later | defect | done | lock: an uppercase digest is accepted and then fails every action against the lowercase SHA-256 |
| [T-470](later.md) | P3 | S | later | defect | done | status: trust_unknown counts a repository or owner literally named unknown |
| [T-471](later.md) | P2 | S | later | defect | done | policy: a host with a trailing dot is not covered by a wildcard ceiling |
| [T-472](later.md) | P3 | S | later | defect | done | check: a pin payload with a duplicate reference, a future built_at_s, max_age_s 0 or a bad runner_version is accepted |
| [T-473](later.md) | P3 | S | later | chore | done | Take `pod_core::clock` in place of the crate `podrunner-clock` |
| [T-474](later.md) | P2 | M | later | chore | done | Take `pod_proxy::env` for the proxy environment of a unit |
| [T-475](later.md) | P3 | S | later | defect | open | A `uses:` with an empty part runs on GitHub and is refused here |
| [T-476](later.md) | P3 | S | later | defect | done | A `:` in TMPDIR or in an option path fails each job start, not `check` |
| [T-477](later.md) | P3 | S | later | defect | done | An audit line that fails half way leaves a fragment, and the next line joins it |
| [T-478](later.md) | P2 | S | later | defect | done | The build of the runner image fails when public.ecr.aws refuses the pull of its base image |
| [T-479](later.md) | P2 | M | later | feature | done | A JSON Schema of `podrunner.toml` for a program that writes the file |
| [T-480](later.md) | P2 | L | later | feature | partial | A cache directory of the host, kept apart by trust (cfb and githome G-07) |
| [T-481](later.md) | P2 | S | later | measurement | partial | The checkout of a job on the incus kind is proven by a test of the script only |
| [T-482](later.md) | P2 | M | later | defect | partial | The incus kind bounds no disk of the root of an instance |
| [T-483](later.md) | P2 | M | later | defect | done | The incus kind reports `unknown` for the CPU time and the peak memory of a job |
| [T-484](later.md) | P2 | S | later | release | done | A release has no `SHA256SUMS` file with a cosign bundle, as the other projects of PodUtils have |
| [T-485](later.md) | P1 | M | later | feature | done | The incus kind has no container route, though G-02 asks for an engine in the system container |
| [T-486](later.md) | P1 | M | later | feature | done | G-04: the egress verdicts of a unit do not reach the source of the job |
| [T-487](later.md) | P2 | L | later | feature | done | G-11: warm units from a pinned golden image |
| [T-488](later.md) | P2 | M | later | feature | done | G-12: the microvm kind enforces no limit of processes |
| [T-489](later.md) | P3 | S | later | chore | partial | githome and cfb read `cpu_s` and `peak_rss_mib`: a change of a field of the run report needs a new version |
| [T-490](later.md) | P1 | L | later | research | done | Settle each open unknown of `docs/unknowns.md` |
| [T-491](later.md) | P1 | M | later | research | done | Lift each limit and non-goal that a need of githome or cfb names |
| [T-492](later.md) | P2 | S | later | docs | done | The trackers of githome and cfb and the state pages name an old release |
| [T-493](later.md) | P0 | M | later | defect | done | G-15: the job-started hook ends a job that GitHub gave to a runner of a program source, when its run is not the run of the offer |
| [T-494](later.md) | P0 | M | later | defect | done | G-16: one `[trust]` list for each host: a login of githome can match a login of GitHub |
| [T-495](later.md) | P1 | M | later | feature | done | G-17: the facts of a host for its source in `hello` |
| [T-496](later.md) | P1 | M | later | feature | blocked | G-18: the disk of a unit is a measured bound, not a size: a job does not get `ENOSPC` |
| [T-497](later.md) | P1 | M | later | feature | partial | G-19: more facts of a job in the run report |
| [T-498](later.md) | P1 | M | later | feature | done | G-22: a rotation of the key of a program source with no drain |
| [T-499](later.md) | P1 | M | later | feature | partial | G-23: the runner of a GitHub job stays inside GitHub's window of updates |
| [T-500](later.md) | P2 | M | later | feature | done | G-24: a runner image with the software that cfb's gates need |
| [T-501](later.md) | P2 | S | later | feature | done | G-26: a drain and a stop of one source |
| [T-502](later.md) | P2 | S | later | feature | done | G-27: a cancel ends its unit in a bounded time, with a conformance test |
| [T-503](later.md) | P2 | S | later | feature | done | G-28: the output of `source-conformance` for CI |
| [T-504](later.md) | P3 | M | later | feature | done | G-29: a GitHub source for the repositories of a personal account |
| [T-505](later.md) | P2 | S | later | feature | done | No cap of units for the whole host across its kinds |
| [T-506](later.md) | P1 | L | later | feature | done | G-36: a sandbox unit: a payload that the source names, for an agent task or a web terminal |
| [T-507](later.md) | P1 | M | later | feature | done | G-37: an install of `serve` that another program drives, with no prompt |
| [T-508](later.md) | P1 | S | later | chore | done | COORD-135: the schemas of `status` and of the source wire move to `pod-job`; podcrates needs their types, the stable fields, and the new fields of githome's asks |
| [T-509](later.md) | P1 | M | later | feature | partial | G-38: a test of githome's source in podrunner's CI that blocks a release |
| [T-510](later.md) | P1 | S | later | release | done | G-39: one record of each change of a contract that githome reads |
| [T-511](later.md) | P1 | M | later | release | done | G-40: each fake and tool that githome's tests need, as a release asset |
| [T-512](later.md) | P0 | S | later | release | open | G-41: the statement "githome: complete in vTAG" |
| [T-513](later.md) | P3 | S | later | chore | done | Two file locks of the local mode that `pod_core::lock` cannot hold yet |
| [T-514](later.md) | P2 | S | later | feature | done | G-05 and G-06: the run report names no architecture |
| [T-515](later.md) | P2 | M | later | defect | partial | G-12: a unit past its memory limit does not end with the exit reason `limit` |
| [T-516](later.md) | P2 | S | later | feature | done | G-09: no CI proof of a `docker build` step and of Docker-in-Docker in a guest with an engine |
| [T-517](later.md) | P3 | S | later | docs | done | G-09: the engine of a guest is a directory of the host, not a part of the payload image |
| [T-518](later.md) | P2 | M | later | feature | done | G-13: each event by name, a rule of trust that can be swapped, and a published table of cases |
| [T-519](later.md) | P3 | S | later | docs | done | G-03: no document says that a step of the job can read the credential of its runner |
| [T-520](later.md) | P3 | S | later | measurement | partial | G-06: no test of `cpu_s` against a known load, and no tolerance in the contract |
| [T-521](later.md) | P3 | S | later | defect | partial | G-12: the `pids` limit of the podvm kind counts each process of the user, not of one unit |
| [T-522](later.md) | P2 | S | later | docs | done | G-31: the design copies of the schemas have the ids of `pod-job` and other fields |
| [T-523](later.md) | P2 | M | later | feature | done | G-34: take the signed release index of `pod-release` |
| [T-524](later.md) | P3 | M | later | feature | done | G-04 and G-35: the half of podrunner of the placeholders of a policy |
| [T-525](later.md) | P1 | M | later | measurement | done | C1 to C5: the acceptance of A-X-01 through `serve` on real units in CI, and the results of githome's host |
| [T-526](later.md) | P1 | S | later | chore | done | The incus suite of `runner-image.yml` does not block a release |
| [T-527](later.md) | P1 | M | later | chore | done | No lane of the user-path gate for the host class of githome's pool |
| [T-528](later.md) | P1 | S | later | measurement | done | No row of the user-path gate copies the gate of rbg |
| [T-529](later.md) | P2 | S | later | defect | done | The disk limit of an offer lifts the bound of the operator |
| [T-530](later.md) | P2 | S | later | feature | done | The job-started hook of a podvm unit cannot ask serve |
| [T-531](later.md) | P3 | S | later | chore | done | Take rsa 0.10 in the fake broker |
| [T-532](later.md) | P1 | M | later | feature | done | The containers of a podvm unit on the namespace rung, with a volume base |
| [T-533](later.md) | P2 | M | later | measurement | open | A job of another architecture in a guest of the machine tier |
| [T-534](later.md) | P2 | S | later | defect | done | The probe of the incus kind claims the kind on a host with no incusd |
| [T-535](later.md) | P3 | S | later | defect | partial | The tests leave their temporary directories in /tmp |
| [T-536](later.md) | P2 | S | later | chore | done | Take `pod_core::http1` in place of the reader of an HTTP/1.1 head of podrunner-hook |
| [T-537](later.md) | P1 | S | later | defect | done | The installer leaves the data directory writable by the group, and the microvm kind refuses its podvm |
| [T-538](later.md) | P1 | L | later | feature | partial | A unit with the toolset of the GitHub-hosted ubuntu-latest, with systemd, checked mechanically |
| [T-539](later.md) | P1 | M | later | measurement | partial | The probe and the diff of the hosted parity, report-only in CI |
| [T-540](later.md) | P1 | S | later | chore | partial | A watcher of the hosted image releases and of the label of ubuntu-latest |
| [T-541](later.md) | P1 | M | later | feature | done | The runner layer of the hosted snapshot, and the hosted work folder /home/runner/work |
| [T-542](later.md) | P1 | L | later | feature | partial | The hosted snapshot: a 1:1 capture of the root of a hosted runner, scrubbed, with its kernel layer |
| [T-543](later.md) | P1 | M | later | feature | blocked | systemd as PID 1 in an incus instance |
| [T-544](later.md) | P2 | M | later | feature | blocked | systemd as PID 1 and a root on a disk in a microvm guest |
| [T-545](later.md) | P2 | M | later | defect | blocked | The podvm kind loads, extracts and copies the whole image for each unit |
| [T-546](later.md) | P2 | M | later | measurement | open | The tests of runner-images (Pester) in a unit |
| [T-547](later.md) | P2 | L | later | feature | open | The full lane of the hosted snapshot: a boot in each mode on a live host, checks C4 to C8 |
| [T-548](later.md) | P1 | S | later | chore | done | The licence call of a published hosted image |
| [T-549](later.md) | P2 | M | later | feature | done | Placement by label and by init: a systemd payload goes only to a kind that boots it |
| [T-550](later.md) | P2 | S | later | chore | open | The corpus of the user-path gate records the image of each github.com result |
| [T-551](later.md) | P1 | S | later | defect | done | A pinned image larger than the disk bound of its kind is not refused |
| [T-552](later.md) | P1 | S | later | defect | done | A job that the job-started hook ends with no answer is reported as success |
| [T-553](later.md) | P1 | M | later | release | done | podrunner and its runner images on ghcr.io, in the form of PodUtils |
| [T-554](later.md) | P1 | S | later | defect | done | The compile of TypeScript 7 hangs on the podvm userland rung with no tier |
| [T-555](later.md) | P1 | M | later | feature | partial | The publish of the hosted snapshot: hosted-snapshot.yml, the checks C1 to C3, the attested index |
| [T-556](later.md) | P1 | M | later | measurement | partial | The base slice of the hosted snapshot boots in hosted CI |
| [T-557](later.md) | P2 | S | later | measurement | done | Measure: can a standard hosted runner hold a full boot of the snapshot |
| [T-558](later.md) | P2 | S | later | defect | blocked | The file capabilities of an image are lost through the extract of podcrates and podvm |
| [T-559](later.md) | P2 | M | later | feature | blocked | An incus VM mode for an image with a kernel |
| [T-560](later.md) | P3 | S | later | defect | blocked | pod-extract reads only the first gzip member of a layer |
| [T-561](later.md) | P2 | S | later | defect | done | `podrunner run --vnet` hangs in the lab that refuses the bind |
| [T-562](later.md) | P2 | S | later | release | done | A release of podrunner has no signed `release-index.json` |
| [T-563](later.md) | P2 | M | later | feature | done | A unit holds the real values of its secrets, not placeholders that the broker swaps |
| [T-564](later.md) | P3 | S | later | defect | done | The podvm kind gives no socket volume on the namespace rung, so `--vnet` and the container hooks are refused there |
| [T-565](later.md) | P2 | S | later | defect | done | `config init` writes no `confine` for the podvm kind, so a trusted source on a host with no incus gets no kind |
| [T-566](later.md) | P2 | M | later | defect | done | The confine self-test cannot pass on the namespace rung of podvm |
| [T-567](later.md) | P1 | S | later | defect | partial | A runner unit that exits 0 with no job is reported as success |
| [T-568](later.md) | P2 | S | later | defect | partial | A runner job that fails is reported as success |
| [T-569](later.md) | P3 | S | later | defect | done | A job that the runner abandoned is reported as success |
| [T-570](later.md) | P3 | M | later | defect | partial | The read of the unit log for each runner job has no bound |
| [T-571](later.md) | P2 | S | later | defect | done | An offer whose disk limit is below the size of its image starts a unit that cannot hold it |
| [T-572](later.md) | P3 | S | later | feature | done | The release manifest and config init do not carry the size of a runner image |
| [T-573](later.md) | P2 | S | later | defect | done | An image larger than the memory of a microvm guest is not refused |
| [T-574](later.md) | P1 | S | later | defect | done | One failed self-test of the guest tier takes the microvm kind out of `serve` for a day, with no cause |
| [T-575](later.md) | P2 | S | later | defect | open | The host facts of a microvm kind with no `mem` name no memory and no CPU |
| [T-576](later.md) | P1 | M | later | chore | partial | Take podcrates `v0.80.0` (COORD-150 of podcrates) |
| [T-577](later.md) | P3 | S | later | defect | partial | A JVM job behind an intercepting proxy of the host does not trust the CA of the proxy |
| [T-578](later.md) | P3 | S | later | defect | partial | A container step of a unit does not trust the CA of its broker |
| [T-579](later.md) | P2 | S | later | feature | open | The pins of the hosted snapshot: images/hosted/pins.toml with the labels and the init of each payload |
| [T-580](later.md) | P3 | S | later | defect | done | The test of the ninth client of the control socket fails under the load of the full suite |
| [T-581](later.md) | P1 | M | later | chore | done | The readers of podvm's output before the release after `0.2.0-beta.37` (COORD-226 of podvm) |
| [T-582](later.md) | P1 | S | later | defect | done | `hosted-boot.yml` is green when the guest does not boot |
| [T-583](later.md) | P1 | S | later | chore | done | Take podcrates `v0.81.0` (COORD-151 of podcrates) |
| [T-584](later.md) | P2 | S | later | defect | open | The conformance suite of the podvm kind fails four tests on the namespace rung as root |
| [T-585](later.md) | P3 | S | later | defect | open | The store of the units of the podvm kind keeps each image that a unit loaded |
| [T-588](later.md) | P3 | M | later | feature | partial | `install_system` of `pod-unit` for a system unit of `serve` |
| [T-589](later.md) | P3 | S | later | defect | done | Two tests depend on the load and the free disk of the host |
| [T-590](later.md) | P2 | M | later | feature | done | The hosted work folder on the podvm kind and in the runner image |
| [T-591](later.md) | P3 | S | later | defect | done | The unit tests leave directories in /tmp |
| [T-592](later.md) | P2 | S | later | defect | done | The level2 corpus fails on the advisory check of podcrates, which reads the live advisory database |
| [T-593](later.md) | P1 | S | later | defect | done | The official runner stops at its start when `no_proxy` holds the entry `::` |
| [T-594](later.md) | P3 | S | later | defect | open | The image release and `runner-image.yml` each have a copy of the build and the archive of the runner images |
| [T-595](later.md) | P3 | S | later | defect | done | A run report has no reason for a time that it does not know (`start_s`) |
| [T-596](later.md) | P2 | S | later | defect | partial | A container of `podrunner run` gets no CA file of the caller |
| [T-597](later.md) | P3 | S | later | defect | done | A container of a unit gets the CA bundle of its broker, but no trust store of a JVM |
| [T-598](later.md) | P3 | S | later | defect | done | A job on `--runner-dir` behind an intercepting proxy gives a JVM no trust store |
| [T-599](later.md) | P2 | S | later | defect | done | The capture of the hosted snapshot writes GNU sparse records, which the next podvm refuses |
| [T-600](later.md) | P3 | S | later | defect | open | The tests of Unix sockets fail when `TMPDIR` is long |
| [T-601](later.md) | P3 | S | later | defect | open | A test of the run report failed once under load |
| [T-602](later.md) | P2 | S | later | defect | done | `print-unit` refuses a config path under a directory of `mktemp -d` as a secret |
| [T-603](later.md) | P3 | S | later | defect | open | The CA directory of a confined container joins the `exec=` list, and a container may remount it writable |
| [T-604](later.md) | P3 | S | later | feature | open | A runner image release is not on ghcr.io |
| [T-605](later.md) | P3 | S | later | feature | open | `podrunner-manifest.json` names no ghcr.io reference |
| [T-606](later.md) | P2 | S | later | defect | done | `podvm load` refuses the full hosted snapshot (85 layers): 16 locks at most |
| [T-607](later.md) | P2 | S | later | defect | done | Two runs of `podrunner run` in two pid namespaces that share `/tmp` meet on one run id |
| [T-608](later.md) | P3 | S | later | defect | open | A recovery run of `release.yml` on a tag builds the binaries again |
| [T-609](later.md) | P3 | M | later | defect | open | `podrunner run` cannot run the hosted runner image as a hosted runner |
| [T-610](later.md) | P3 | S | later | defect | open | A layer of the tool cache of the snapshot names the version only |
| [T-611](later.md) | P3 | S | later | defect | done | The use of an incus unit misses the last tick, and no live test checks an OOM kill on incus |
| [T-612](later.md) | P3 | S | later | defect | open | Three live tests of the podvm kind fail on a root host |
| [T-613](later.md) | P2 | M | later | measurement | done | The full hosted snapshot does not fit the storage pool of incusd on a hosted runner |
| [T-614](later.md) | P3 | S | later | defect | done | The probe of `--export` and `--log-dir` has the pid only and follows a link |
| [T-615](later.md) | P3 | S | later | defect | done | One `TMPDIR` that two hosts share can mistake a live run for a dead one |
| [T-616](later.md) | P3 | S | later | defect | done | A container of a unit of the local mode gets the CA bundle of the caller, but no trust store of a JVM |
| [T-617](later.md) | P2 | S | later | docs | done | The hand-written state of GitHub #65, #66 and the comment on #292 is stale |
| [T-618](later.md) | P3 | S | later | research | open | R-052 of cfb is named nowhere |
| [T-619](later.md) | P1 | S | later | chore | done | Take podcrates `v0.82.1` (COORD-152 of podcrates) |
| [T-620](later.md) | P3 | S | later | defect | done | A long state path leaves an empty `podrunner-HASH` directory in the temp directory |
| [T-621](later.md) | P2 | S | later | measurement | open | A kill of the scale-set sidecar with queued jobs: does GitHub send them again |
| [T-622](later.md) | P3 | S | later | feature | open | The status of a scale-set source has no counts of the queue of its sidecar |
| [T-623](later.md) | P2 | S | later | defect | done | A cancel of a refused run that fails is never tried again |
| [T-624](later.md) | P3 | M | later | defect | open | Faults and limits of the scale-set trust lookups that end a job with no specific cause |
| [T-625](later.md) | P2 | S | later | defect | done | The attempt of a refused job of the past-limit path: a lost cancel |
| [T-626](later.md) | P2 | S | later | release | done | `release.yml` needs a green `runner-image.yml` run of the release commit, with no `images_run` input |
| [T-627](later.md) | P2 | M | later | defect | done | `podrunner man` does not exist, and the help of `run` and `config init` lacks flags that `docs/cli.md` lists |
| [T-628](later.md) | P2 | S | later | defect | done | `podrunner image verify --arch x64` is refused, and the error lists no valid value |
| [T-629](later.md) | P2 | M | later | defect | done | `config init` and `doctor --config` offer the microvm kind on a host with no KVM |
| [T-630](later.md) | P2 | S | later | defect | done | A corrupt or truncated tar in the image cache fails each later `run`, and no message names the file |
| [T-631](later.md) | P2 | S | later | defect | done | `run --changed-since` passes a ref that starts with `-` to git as an option |
| [T-632](later.md) | P2 | S | later | defect | done | A small `ulimit -f` ends a podvm unit with an empty cause, and `run` names neither the limit nor the fix |
| [T-633](later.md) | P3 | M | later | defect | open | QA round 6, agent A: the P3 findings of the installer and the install documents |
| [T-634](later.md) | P3 | M | later | defect | open | QA round 6, agent A: the P3 findings of the flags of `podrunner run` |
| [T-635](later.md) | P3 | M | later | defect | open | QA round 6, agent A: the P3 findings of `check`, `config init` and `doctor` |
| [T-636](later.md) | P3 | M | later | defect | open | QA round 6, agent A: the P3 findings of signals and of the end of a run |
| [T-637](later.md) | P3 | L | later | measurement | open | QA round 6, agent A: the areas that the round did not test |
| [T-638](later.md) | P3 | S | later | defect | partial | The user-path gate has no lane with a hard limit of file size of 1 GiB |
| [T-639](later.md) | P2 | M | later | defect | done | Ctrl-C does not cancel a `container:` job in a long step on `--podvm` |
| [T-640](later.md) | P2 | M | later | defect | done | `actions/checkout` with `ref:` on the same repository is silently ignored in the local mode |
| [T-641](later.md) | P2 | S | later | defect | done | `check` accepts the `config_url` of an organization with `.git` at the end |
| [T-642](later.md) | P3 | M | later | defect | open | QA round 6, agent B: the P3 findings of the examples, the fakes, the logs and the texts |
| [T-643](later.md) | P3 | S | later | defect | open | The local checkout names the keys of `with:` that the stand-in does not take |
| [T-644](later.md) | P2 | S | later | chore | partial | A lane of the user-path gate: Ctrl-C of a `container:` job on podvm |
| [T-645](later.md) | P3 | S | later | chore | done | The man page of the model under groff and mandoc, and in the assets of a release |
| [T-646](later.md) | P3 | M | later | defect | done | Tests that write a program and start it while other tests fork |
| [T-647](later.md) | P1 | S | later | chore | done | Take podcrates `v0.83.0` and podvm `0.2.0-beta.40` (COORD-154 of podcrates, COORD-246 of podvm) |
| [T-648](later.md) | P2 | M | later | feature | open | The cache of `serve` on the microvm and incus kinds |
| [T-649](later.md) | P3 | M | later | measurement | blocked | The bytes that a job reads from its cache |
| [T-650](later.md) | P2 | M | later | feature | blocked | A tool that writes its cache in a job with a read-only cache |
| [T-651](later.md) | P3 | S | later | feature | done | The cache in the containers of a `container:` job |
| [T-652](later.md) | P2 | S | later | defect | done | The disk check of a unit walks its root in the tick |
| [T-653](later.md) | P2 | M | later | feature | done | The ref and the repository id of a job of another run, for its cache |
| [T-654](later.md) | P3 | M | later | feature | done | A job that ended with `cache-mismatch` runs again |
| [T-655](later.md) | P3 | M | later | feature | done | A guest of the microvm kind keeps the memory of its kind for an offer that asks less |
| [T-656](later.md) | P3 | M | later | feature | open | Warm units of the podvm kind |
| [T-657](later.md) | P2 | S | later | feature | done | Warm and cold units in the run report and the status |
| [T-658](later.md) | P3 | S | later | feature | open | A warm incus instance fits an offer with lower limits |
| [T-659](later.md) | P3 | M | later | feature | open | A cold guest of the microvm kind from a saved state |
| [T-660](later.md) | P2 | S | later | defect | done | The first step of a job on a kind with no container hook |
| [T-661](later.md) | P2 | S | later | defect | done | The destroy of a unit counts a killed process that still ends as gone |
| [T-662](later.md) | P2 | S | later | defect | done | One failed suite of the microvm and incus steps hides the next suites |
| [T-663](later.md) | P2 | M | later | defect | done | A step of a unit with no job-started hook can ask `assigned` for a run that it does not run |
| [T-664](later.md) | P1 | S | later | chore | done | Take podcrates `v0.84.0` (COORD-155 of podcrates) |
| [T-665](later.md) | P3 | S | later | defect | done | The warm status counts a reserved unit as ready and hides the warm units of a kind that has target 0 |
| [T-666](later.md) | P3 | S | later | defect | done | The serve test of stop --wait can see serve not yet ended |
| [T-667](bootstrap.md) | P2 | S | M0 | chore | done | One cold start for hosted sessions |
| [T-668](core.md) | P2 | M | M-A | defect | done | The unit shim waits for an orphan on a host without the proc children file |
| [T-669](bootstrap.md) | P2 | S | M0 | chore | done | Load the hosted shell and try direct workflow dispatch |
| [T-670](bootstrap.md) | P3 | S | M0 | chore | done | Update the TruffleHog image to 3.100.0 |
| [T-671](bootstrap.md) | P3 | S | M0 | chore | done | Update the TOML parser dependencies |
| [T-672](later.md) | P2 | M | later | feature | done | G-09: bundle the Docker engine in runner-full |
| [T-673](later.md) | P3 | S | later | defect | done | Warm unit tests depend on host memory |
| [T-674](later.md) | P2 | S | later | measurement | done | Prove offline rootless Podman and FUSE in the hosted snapshot |
| [T-675](later.md) | P1 | S | later | chore | done | Take podcrates `v0.85.0` (COORD-156 of podcrates) |
| [T-676](bootstrap.md) | P2 | S | M0 | docs | done | Document hosted sessions and worker roles (COORD-157 of podcrates) |
| [T-677](later.md) | P2 | S | later | chore | done | Take podnet `v0.4.0-beta.19` (COORD-129 of podnet) |
| [T-678](later.md) | P2 | M | later | defect | done | Open disk and cache walk directories by descriptor |
| [T-679](later.md) | P3 | S | later | defect | done | A local concurrency lock error reports an interrupted wait |
| [T-680](later.md) | P3 | S | later | defect | done | Local doctor only checks the podnet file |
| [T-681](later.md) | P2 | S | later | measurement | blocked | Repeat the low-space Incus proof on the next podvm release |
| [T-682](later.md) | P1 | M | later | defect | done | A refused runner assignment must stop the job before steps |
| [T-683](later.md) | P1 | S | later | defect | done | The bundled engine fails in an instance of Incus before 6.0.6 and 6.19 |
| [T-684](later.md) | P2 | S | later | defect | done | A job hides bytes from the disk limit under a shut directory |
| [T-685](later.md) | P3 | M | later | defect | blocked | The disk walk of an incus unit skips the directories it cannot read |
| [T-686](later.md) | P2 | S | later | chore | partial | Take podnet `v0.4.0-beta.20` (COORD-132 of podnet) |
| [T-687](later.md) | P2 | S | later | chore | done | Take podcrates `v0.86.0` (COORD-158 of podcrates) |
| [T-688](later.md) | P2 | M | later | defect | partial | A job of the corpus runs out of space in a microvm guest |
| [T-689](later.md) | P3 | S | later | defect | open | The pin bound of a microvm guest leaves no space for a job |
| [T-690](later.md) | P3 | S | later | feature | open | The local mode has no flag for the memory of a guest |
| [T-691](later.md) | P3 | S | later | feature | blocked | The run report does not name the limits a unit got |
| [T-692](later.md) | P3 | M | later | feature | done | The installer does not install the man page |
| [T-693](later.md) | P3 | S | later | chore | open | The trust store of a unit of serve is encoded twice |
