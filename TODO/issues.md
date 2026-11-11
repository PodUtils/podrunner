# Issues

Each GitHub issue of the record, with its entries. The State starts with the
state on GitHub: `open`, `closed` or `deleted`. `TODO/github.txt` lists the
numbers. `TODO/RULES.md` gives the rules.

| Issue | Subject | State | Entries |
| --- | --- | --- | --- |
| #1 | Index: podrunner v1 plan, milestones and issue map | open | T-001 |
| #2 | DR-1: name the podrunner plan and lift "leave podcrates#47 alone" for it | closed (`docs/decisions.md`, "podrunner online") | T-002 |
| #3 | DR-2: let PodUtils/podrunner hold code | closed (`docs/decisions.md`, "podrunner online") | T-003 |
| #4 | DR-4: allow trusted jobs at boundary `redirect` on no-root hosts | open | T-004 |
| #5 | DR-5: microVM unit for untrusted jobs: own driver now, or PodUtils/podvm@T-1656 | open | T-005 |
| #6 | DR-6: githome and cfb accept the restated G-02, G-03 and G-09 | open | T-006 |
| #7 | DR-7: a throwaway GitHub organization and App for live tests | open | T-007 |
| #8 | Defaulted decisions DR-3 and DR-8 to DR-17: confirm in one reply | open | T-008 |
| #9 | Unknowns U1 to U25: register, owners and what settles each | open | T-009 |
| #10 | v1 non-goals: deferred items and the trigger of each | closed (`docs/non-goals.md`) | T-010 |
| #11 | Bootstrap: Cargo workspace, pinned toolchain, static musl build | closed (`9d2e4dd`, gate run 42934750114) | T-011 |
| #12 | Bootstrap: CI gate (format, lint, tests, static probe, deny, file size) | closed (`9d2e4dd`, gate run 42934750114) | T-012 |
| #13 | Bootstrap: agent workflow, decisions and record | closed (`9d2e4dd`, gate run 42934750114) | T-013 |
| #14 | Bootstrap: carry the cleaned design spec into `docs/` | open | T-014 |
| #15 | Bootstrap: beta release workflow | closed (`v0.1.0-beta.1`, release run 42942338772) | T-015 |
| #16 | WP-01: seam traits, contracts v1 and the conformance harness | closed (`229d29f`, gate run 42936703374) | T-016 |
| #17 | Schemas v1: job offer, unit capability, run report, public report | open | T-017 |
| #18 | Tier rule: Rust implementation and the case table | closed (`33e6779`, gate run 42938706745) | T-018 |
| #19 | Security documents: threat model, accepted residuals, leak runbook | closed (`fe35f58`) | T-019 |
| #20 | WP-02: fake source and fake unit, scripted, with a manual clock | closed (`e8f4c0d`, gate run 42938706745) | T-020 |
| #21 | WP-03: `slots` core: states, journal, placement, status, drain, codes, config, audit | closed (`644bab7`, gate run 42940434305) | T-021 |
| #22 | CLI and control socket: `serve`, `status`, `drain`, `stop`, `doctor`, `print-unit` | closed (`644bab7`, gate run 42940434305) | T-022 |
| #23 | WP-04: podvm unit driver with a per-unit store and a subreaper shim | open | T-023 |
| #24 | WP-08: egress module over `pod-broker` v1, one listener for each unit | closed (`d69e791`, gate run 42946099969) | T-024 |
| #25 | Milestone M-A: level 1 green, podvm unit conformance with a stand-in payload | open | T-025 |
| #26 | Milestone M-B: the official runner starts under the podvm driver, no GitHub | open | T-026 |
| #27 | WP-05a: `mint` with an in-process `sign` module, source conformance | closed (`9d48ab8`, gate run 42946099969) | T-027 |
| #28 | WP-06: fake scale-set service (Go, test only) | closed (`ece89cd`, gate run 42946099969) | T-028 |
| #29 | WP-07: `github-scaleset` source sidecar (static Go) | closed (`5d740e8`, gate run 42947195332) | T-029 |
| #30 | WP-10: image pipeline: pins, age limit, `podrunner image build` | open | T-030 |
| #31 | WP-05b: remote mint (only if DR-4 is yes) | open | T-031 |
| #32 | Milestone M-C: live gates G-H2 to G-H4 on a `fake` unit | open | T-032 |
| #33 | Milestone M-D: one live trusted job on a no-root host | open | T-033 |
| #34 | WP-13: measure U19 and U21 on the podvm rung | open | T-034 |
| #35 | WP-09: hook relay and the per-unit control socket server (`ctl`) | open | T-035 |
| #36 | Guest floor self-test | open | T-036 |
| #37 | WP-11: `microvm` unit driver (form set by DR-5) | open | T-037 |
| #38 | WP-12: canned-message fake broker (level 2) | open | T-038 |
| #39 | Milestone L2: level 2 green, hook path measured | open | T-039 |
| #40 | Level 3: live acceptance suite | open | T-040 |
| #41 | Hold `enforced` on podvm units (`pod-hold` or `pod-sud`) | open | T-041 |
| #42 | `confined` boundary (`pod-confine`) | open | T-042 |
| #43 | Later crates: service unit, release index, acceptance registry | open | T-043 |
| #46 | COORD-001: DR-1 and DR-2 are yes; podnet's podrunner entries can start (to podnet) | closed (answered in PodUtils/podnet#425) | T-044 |

#44 and #45 are pull requests.
