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

**89 entries: 1 open, 4 partial, 12 blocked, 72 done.**

| Priority | open | partial | blocked | done | total |
| --- | --- | --- | --- | --- | --- |
| P0 | 0 | 0 | 0 | 0 | 0 |
| P1 | 0 | 1 | 0 | 29 | 30 |
| P2 | 1 | 1 | 9 | 32 | 43 |
| P3 | 0 | 2 | 3 | 11 | 16 |
| **All** | 1 | 4 | 12 | 72 | 89 |

| ID | Priority | Effort | Milestone | Category | Status | Item |
| --- | --- | --- | --- | --- | --- | --- |
| [T-001](plan.md) | P1 | S | M0 | docs | partial | The plan of podrunner v1 and the issue map (GitHub #1) |
| [T-002](plan.md) | P1 | S | M0 | docs | done | DR-1: the plan of GitHub #1 is the plan (GitHub #2) |
| [T-003](plan.md) | P1 | S | M0 | docs | done | DR-2: PodUtils/podrunner holds code (GitHub #3) |
| [T-004](plan.md) | P1 | S | M0 | research | done | DR-4: trusted jobs at boundary redirect on no-root hosts (GitHub #4) |
| [T-005](plan.md) | P2 | S | M0 | research | done | DR-5: the microVM unit for untrusted jobs (GitHub #5) |
| [T-006](plan.md) | P2 | S | M0 | research | done | DR-6: githome and cfb accept the restated G-02, G-03 and G-09 (GitHub #6) |
| [T-007](plan.md) | P2 | S | M0 | research | blocked | DR-7: a throwaway GitHub organization and App for live tests (GitHub #7) |
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
| [T-030](live.md) | P2 | M | M-C | feature | blocked | WP-10: the image pipeline: pins, age limit, podrunner image build (GitHub #30) |
| [T-031](live.md) | P2 | M | M-D | feature | done | WP-05b: remote mint (only if DR-4 is yes) (GitHub #31) |
| [T-032](live.md) | P2 | S | M-C | chore | blocked | Milestone M-C: the live gates G-H2 to G-H4 on a fake unit (GitHub #32) |
| [T-033](live.md) | P2 | S | M-D | chore | blocked | Milestone M-D: one live trusted job on a no-root host (GitHub #33) |
| [T-034](level2.md) | P2 | S | L2 | measurement | done | WP-13: measure U19 and U21 on the podvm rung (GitHub #34) |
| [T-035](level2.md) | P2 | M | L2 | feature | done | WP-09: the hook relay and the control socket server of each unit (ctl) (GitHub #35) |
| [T-036](level2.md) | P2 | S | L2 | feature | blocked | The guest floor self-test (GitHub #36) |
| [T-037](level2.md) | P2 | M | L2 | feature | blocked | WP-11: the microvm unit driver (form set by DR-5) (GitHub #37) |
| [T-038](level2.md) | P2 | S | L2 | feature | done | WP-12: the canned-message fake broker (level 2) (GitHub #38) |
| [T-039](level2.md) | P2 | S | L2 | chore | blocked | Milestone L2: level 2 green, the hook path measured (GitHub #39) |
| [T-040](level2.md) | P3 | M | L3 | chore | blocked | Level 3: the live acceptance suite (GitHub #40) |
| [T-041](later.md) | P3 | S | later | feature | done | Hold enforced on podvm units (pod-hold or pod-sud) (GitHub #41) |
| [T-042](later.md) | P3 | S | later | feature | done | The confined boundary (pod-confine) (GitHub #42) |
| [T-043](later.md) | P3 | S | later | feature | blocked | Later crates: service unit, release index, acceptance registry (GitHub #43) |
| [T-044](plan.md) | P2 | S | M-A | chore | done | COORD-001: DR-1 and DR-2 are yes; podnet's podrunner entries can start (GitHub #46) |
| [T-045](live.md) | P1 | M | M-D | feature | done | The mitigations of DR-4 for trusted jobs at redirect (GitHub #47) |
| [T-046](later.md) | P3 | L | later | feature | partial | Local mode: podrunner run workflow.yml (GitHub #48) |
| [T-047](core.md) | P2 | S | M-A | defect | done | COORD-002: podvm wait gives no exit code for a short payload on the supervise rung (GitHub #49) |
| [T-048](core.md) | P2 | S | M-D | feature | done | Egress: check the upstream of the broker; the names mode of pod-broker |
| [T-049](level2.md) | P2 | S | L2 | chore | done | COORD-003: the stand-in broker of the official runner for podvm (GitHub #50) |
| [T-050](level2.md) | P2 | S | L2 | defect | done | COORD-004: the supervise rung does not map the path of connect() for a unix socket (GitHub #51) |
| [T-051](core.md) | P2 | S | M-C | defect | blocked | COORD-005: podvm-aarch64 declines an arm64 payload on the supervise rung (GitHub #52) |
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
| [T-077](core.md) | P3 | S | later | defect | blocked | COORD-009: the seccomp hold of podnet pod does not take ::ffff:127.0.0.1 as loopback (GitHub #64) |
| [T-078](core.md) | P3 | S | later | feature | done | The podvm driver labels each unit and reads the run report of podvm |
| [T-079](core.md) | P3 | M | later | measurement | done | Measure podvm --podvm-confine on the supervise rung, for a confined podvm unit |
| [T-080](core.md) | P3 | S | later | defect | done | Tests 25 and 26 skip on a confined podvm unit: the orphan is not seen on the host |
| [T-081](core.md) | P3 | M | later | measurement | done | Measure a musl build of the official runner (P0-4 of GitHub #67) |
| [T-082](core.md) | P2 | S | later | defect | blocked | The official runner v2.338.2 carries Microsoft.AspNetCore.WebUtilities 8.0.25 (GHSA-x33c-w48q-qrgc) |
| [T-083](core.md) | P2 | S | later | chore | done | Take podcrates v0.29.0 (PodUtils/podcrates#87, COORD-055) |
| [T-084](core.md) | P1 | S | later | defect | done | Conformance test 26 kills the launcher of podvm before it reports the start |
| [T-085](core.md) | P2 | S | later | docs | done | COORD-010: --podvm-confine runs on the userland rung; podrunner named the supervise rung (GitHub #68) |
| [T-086](core.md) | P2 | S | later | chore | open | Take podcrates v0.30.0 (PodUtils/podcrates#89, COORD-057) |
| [T-087](later.md) | P3 | M | later | feature | done | The local source: a workflow file into job offers and job results |
| [T-088](later.md) | P3 | M | later | feature | done | A workflow job into the job message of the fake broker |
| [T-089](later.md) | P3 | M | later | feature | partial | podrunner run workflow.yml: the local mode end to end |
