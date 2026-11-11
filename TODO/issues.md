# Issues

Each GitHub issue of the record, with its entries. The State starts with the
state on GitHub: `open`, `closed` or `deleted`. `TODO/github.txt` lists the
numbers. `TODO/RULES.md` gives the rules.

| Issue | Subject | State | Entries |
| --- | --- | --- | --- |
| #1 | Index: podrunner v1 plan, milestones and issue map | open (closes with the gates of M-D and L2) | T-001 |
| #2 | DR-1: name the podrunner plan and lift "leave podcrates#47 alone" for it | closed (`docs/decisions.md`, "podrunner online") | T-002 |
| #3 | DR-2: let PodUtils/podrunner hold code | closed (`docs/decisions.md`, "podrunner online") | T-003 |
| #4 | DR-4: allow trusted jobs at boundary `redirect` on no-root hosts | closed (`docs/decisions.md`, 2026-11-11) | T-004 |
| #5 | DR-5: microVM unit for untrusted jobs: own driver now, or PodUtils/podvm@T-1656 | closed (`docs/decisions.md`, 2026-11-11) | T-005 |
| #6 | DR-6: githome and cfb accept the restated G-02, G-03 and G-09 | closed (`docs/decisions.md`, 2026-11-11) | T-006 |
| #7 | DR-7: a throwaway GitHub organization and App for live tests | open (DR-7: not now) | T-007 |
| #8 | Defaulted decisions DR-3 and DR-8 to DR-17: confirm in one reply | closed (`docs/decisions.md`, 2026-11-11) | T-008 |
| #9 | Unknowns U1 to U25: register, owners and what settles each | open (U10, U15, U16 and the live rows are open) | T-009 |
| #10 | v1 non-goals: deferred items and the trigger of each | closed (`docs/non-goals.md`) | T-010 |
| #11 | Bootstrap: Cargo workspace, pinned toolchain, static musl build | closed (`9d2e4dd`, gate run 42934750114) | T-011 |
| #12 | Bootstrap: CI gate (format, lint, tests, static probe, deny, file size) | closed (`9d2e4dd`, gate run 42934750114) | T-012 |
| #13 | Bootstrap: agent workflow, decisions and record | closed (`9d2e4dd`, gate run 42934750114) | T-013 |
| #14 | Bootstrap: carry the cleaned design spec into `docs/` | closed (`d88b5d9`, gate run 42976461463) | T-014 |
| #15 | Bootstrap: beta release workflow | closed (`v0.1.0-beta.1`, release run 42942338772) | T-015 |
| #16 | WP-01: seam traits, contracts v1 and the conformance harness | closed (`229d29f`, gate run 42936703374) | T-016 |
| #17 | Schemas v1: job offer, unit capability, run report, public report | closed (`pod-job` of podcrates v0.17.0, `0457ed0`, gate run 42953651407) | T-017 |
| #18 | Tier rule: Rust implementation and the case table | closed (`33e6779`, gate run 42938706745) | T-018 |
| #19 | Security documents: threat model, accepted residuals, leak runbook | closed (`fe35f58`) | T-019 |
| #20 | WP-02: fake source and fake unit, scripted, with a manual clock | closed (`e8f4c0d`, gate run 42938706745) | T-020 |
| #21 | WP-03: `slots` core: states, journal, placement, status, drain, codes, config, audit | closed (`644bab7`, gate run 42940434305) | T-021 |
| #22 | CLI and control socket: `serve`, `status`, `drain`, `stop`, `doctor`, `print-unit` | closed (`644bab7`, gate run 42940434305) | T-022 |
| #23 | WP-04: podvm unit driver with a per-unit store and a subreaper shim | closed (`b5ff350`, gate run 42957378324) | T-023 |
| #24 | WP-08: egress module over `pod-broker` v1, one listener for each unit | closed (`d69e791`, gate run 42946099969) | T-024 |
| #25 | Milestone M-A: level 1 green, podvm unit conformance with a stand-in payload | closed (`b5ff350`, gate run 42957378324) | T-025 |
| #26 | Milestone M-B: the official runner starts under the podvm driver, no GitHub | closed (`1db8bc3`, runner-image run 42987612426) | T-026 |
| #27 | WP-05a: `mint` with an in-process `sign` module, source conformance | closed (`9d48ab8`, gate run 42946099969) | T-027 |
| #28 | WP-06: fake scale-set service (Go, test only) | closed (`ece89cd`, gate run 42946099969) | T-028 |
| #29 | WP-07: `github-scaleset` source sidecar (static Go) | closed (`5d740e8`, gate run 42947195332) | T-029 |
| #30 | WP-10: image pipeline: pins, age limit, `podrunner image build` | open (blocked: the aarch64 interposer of podvm, T-051) | T-030 |
| #31 | WP-05b: remote mint (only if DR-4 is yes) | closed (`4e6d029`, gate run 42957378324) | T-031 |
| #32 | Milestone M-C: live gates G-H2 to G-H4 on a `fake` unit | open (blocked: DR-7) | T-032 |
| #33 | Milestone M-D: one live trusted job on a no-root host | open (blocked: DR-7, the hold of a podvm unit, T-065) | T-033 |
| #34 | WP-13: measure U19 and U21 on the podvm rung | closed (T-034, podvm `v0.2.0-beta.3`) | T-034 |
| #35 | WP-09: hook relay and the per-unit control socket server (`ctl`) | closed (`fda577d`, gate run 42991333922) | T-035 |
| #36 | Guest floor self-test | open (blocked: a real guest, T-037) | T-036 |
| #37 | WP-11: `microvm` unit driver (form set by DR-5) | open (blocked: PodUtils/podvm@T-1656) | T-037 |
| #38 | WP-12: canned-message fake broker (level 2) | closed (T-026, runner-image run 42987612426) | T-038 |
| #39 | Milestone L2: level 2 green, hook path measured | open (blocked: T-036, T-037) | T-039 |
| #40 | Level 3: live acceptance suite | open (blocked: DR-7) | T-040 |
| #41 | Hold `enforced` on podvm units (`pod-hold` or `pod-sud`) | open (later than v1; starts after T-065) | T-041 |
| #42 | `confined` boundary (`pod-confine`) | open (blocked: PodUtils/podvm#259) | T-042 |
| #43 | Later crates: service unit, release index, acceptance registry | open (blocked: `pod-unit`, `pod-release` of podcrates) | T-043 |
| #46 | COORD-001: DR-1 and DR-2 are yes; podnet's podrunner entries can start (to podnet) | closed (answered in PodUtils/podnet#425) | T-044 |
| #47 | The mitigations of DR-4: trusted jobs at boundary redirect | closed (`7d1f203`, gate run 42957378324) | T-045 |
| #48 | Local mode: podrunner run workflow.yml | open (blocked: `pod-actions-workflow`, `pod-actions-expr` of podcrates) | T-046 |
| #49 | COORD-002: podvm wait gives no exit code for a short payload on the supervise rung (to podvm) | closed (`016a78f`, gate run 42991333922) | T-047 |
| #50 | COORD-003: the answer to PodUtils/podvm#248: the stand-in broker of the official runner, up to the long poll (to podvm) | closed (read by podvm, PodUtils/podvm#248) | T-049 |
| #51 | COORD-004: the supervise rung does not map the path of connect() for a unix socket (to podvm) | closed (`016a78f`, gate run 42991333922) | T-050 |
| #52 | COORD-005: podvm-aarch64 v0.2.0-beta.2 declines an arm64 payload on the supervise rung: its glibc object is x86_64 (to podvm) | open (blocked: PodUtils/podvm@T-1721) | T-051 |
| #53 | COORD-006: please cut a podvm beta from main for podrunner, when your release order allows (to podvm) | closed (podvm `v0.2.0-beta.3` pinned; T-026 green) | T-052 |
| #54 | serve: an exclusive lock on the state directory before the journal replay; stop --wait | closed (`1f1d389`, gate run 42979592980) | T-054 |
| #55 | journal: write and sync the claimed line before source.claim | closed (`a464cea`, gate run 42979592980) | T-055 |
| #56 | control socket: an idle client stalls the tick of the supervisor | closed (`1f1d389`, gate run 42979592980) | T-056 |
| #57 | source wire: a reply deadline for each call; restart a silent sidecar | closed (`1ddd4c4`, gate run 42979592980) | T-057 |
| #58 | podrunner check: validate a configuration and start nothing | closed (`4f09320`, gate run 42979592980) | T-058 |
| #59 | audit.jsonl: chain each line to the one before; podrunner audit verify | closed (`1773096`, gate run 42979592980) | T-059 |
| #60 | serve: SIGTERM and SIGINT start a drain; sd_notify | closed (`1f1d389`, gate run 42979592980) | T-060 |
| #61 | COORD-007: on the supervise rung an absolute symlink in the rootfs resolves on the host (to podvm) | closed (podvm `v0.2.0-beta.4` pinned, `baab86e`; runner-image run 43100514216) | T-061 |
| #62 | COORD-008: on the supervise rung a volume given as :ro is writable (to podvm) | closed (podvm `v0.2.0-beta.4` pinned, `baab86e`; runner-image run 43100514216) | T-062 |
| #64 | COORD-009: the seccomp hold of podnet pod does not take ::ffff:127.0.0.1 as loopback (to podnet) | open (blocked: podnet) | T-077 |

#44, #45 and #63 are pull requests (#63: Dependabot, base64 0.23, applied on `main`).
