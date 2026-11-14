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
| #7 | DR-7: a throwaway GitHub organization and App for live tests | closed (DR-7 yes, `6348aa0`) | T-007 |
| #8 | Defaulted decisions DR-3 and DR-8 to DR-17: confirm in one reply | closed (`docs/decisions.md`, 2026-11-11) | T-008 |
| #9 | Unknowns U1 to U25: register, owners and what settles each | open (U15, U16 and the live rows are open; the live rows wait for #74 and #75) | T-009 |
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
| #30 | WP-10: image pipeline: pins, age limit, `podrunner image build` | closed (runner-image run 43271963245: aarch64 import green) | T-030 |
| #31 | WP-05b: remote mint (only if DR-4 is yes) | closed (`4e6d029`, gate run 42957378324) | T-031 |
| #32 | Milestone M-C: live gates G-H2 to G-H4 on a `fake` unit | open (blocked: the live gates #74 and #75) | T-032 |
| #33 | Milestone M-D: one live trusted job on a no-root host | open (blocked: the live gates #74 and #75) | T-033 |
| #34 | WP-13: measure U19 and U21 on the podvm rung | closed (T-034, podvm `v0.2.0-beta.3`) | T-034 |
| #35 | WP-09: hook relay and the per-unit control socket server (`ctl`) | closed (`fda577d`, gate run 42991333922) | T-035 |
| #36 | Guest floor self-test | closed (`0fd08ac`, runner-image run 43472656290) | T-036 |
| #37 | WP-11: `microvm` unit driver (form set by DR-5) | closed (`67689fa`, runner-image run 43467164062) | T-037 |
| #38 | WP-12: canned-message fake broker (level 2) | closed (T-026, runner-image run 42987612426) | T-038 |
| #39 | Milestone L2: level 2 green, hook path measured | closed (`0fd08ac`, level2 run 43475568946) | T-039 |
| #40 | Level 3: live acceptance suite | open (blocked: the live gates #74 and #75) | T-040 |
| #41 | Hold `enforced` on podvm units (`pod-hold` or `pod-sud`) | closed (`349f0ba`, gate run 43076066364, runner-image run 43076066444) | T-041 |
| #42 | `confined` boundary (`pod-confine`) | closed (`660bdf1`, runner-image run 43155326439) | T-042 |
| #43 | Later crates: service unit, release index, acceptance registry | open (blocked: `pod-unit`, `pod-release` of podcrates) | T-043 |
| #46 | COORD-001: DR-1 and DR-2 are yes; podnet's podrunner entries can start (to podnet) | closed (answered in PodUtils/podnet#425) | T-044 |
| #47 | The mitigations of DR-4: trusted jobs at boundary redirect | closed (`7d1f203`, gate run 42957378324) | T-045 |
| #48 | Local mode: podrunner run workflow.yml | closed (runner-image run 43271963245: the suite held in podvm units) | T-046 |
| #49 | COORD-002: podvm wait gives no exit code for a short payload on the supervise rung (to podvm) | closed (`016a78f`, gate run 42991333922) | T-047 |
| #50 | COORD-003: the answer to PodUtils/podvm#248: the stand-in broker of the official runner, up to the long poll (to podvm) | closed (read by podvm, PodUtils/podvm#248) | T-049 |
| #51 | COORD-004: the supervise rung does not map the path of connect() for a unix socket (to podvm) | closed (`016a78f`, gate run 42991333922) | T-050 |
| #52 | COORD-005: podvm-aarch64 v0.2.0-beta.2 declines an arm64 payload on the supervise rung: its glibc object is x86_64 (to podvm) | closed (podvm `v0.2.0-beta.6` pinned; runner-image run 43271963245) | T-051 |
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
| #68 | COORD-010: --podvm-confine runs on the userland rung, not on the supervise rung: podrunner's note was wrong (to podvm) | closed (podvm read it: PodUtils/podvm#279) | T-085 |
| #64 | COORD-009: the seccomp hold of podnet pod does not take ::ffff:127.0.0.1 as loopback (to podnet) | closed on 2026-11-12 with no fix named (T-077 stays blocked: podnet) | T-077 |
| #69 | COORD-011: with podnet v0.4.0-beta.6 a held podvm unit still cannot reach the host loopback over a dual-stack socket (to podnet) | closed (podnet `v0.4.0-beta.8` pinned; runner-image run 43337313388) | T-077 |
| #70 | COORD-012: tar with an absolute -C fails in a podvm unit; a relative -C passes (PodUtils/podcrates@T-328) (to podcrates) | closed (podvm `v0.2.0-beta.7` pinned; runner-image run 43351590729) | T-096 |
| #71 | COORD-013: podrunner housekeeping done; final commit d56b579 (to podcrates) | closed (podvm is available again; #72 has the state) | T-001 |
| #72 | COORD-014: podrunner takes podcrates v0.34.0 at 2a4f35c (the answer to PodUtils/podcrates#103 and #104) (to podcrates) | closed (podcrates closed #103 and #104 with the take) | T-094 |
| #73 | COORD-015: podrunner takes podcrates v0.35.0 at 0a400b5 (the answer to PodUtils/podcrates#127) (to podcrates) | closed (podcrates closed #127 with the take) | T-094 |
| #74 | LIVE-1: the server agent: run the remote mint for the live gates (T-032, T-033, T-040) | open | T-032, T-033, T-040 |
| #75 | LIVE-2: the runner/user agent: run the unit host and the test jobs of the live gates (T-032, T-033, T-040) | open | T-032, T-033, T-040 |
| #76 | COORD-016: podrunner takes podcrates v0.36.0 at f4064f7 (the answer to PodUtils/podcrates#129) (to podcrates) | closed (podcrates closed #129 with the take) | T-094 |
| #77 | COORD-017: podrunner's Q10 (a Docker Engine API socket for each job) is decided by DR-8: container hooks, no Docker API socket (the answer to PodUtils/podcrates#132) (to podcrates) | closed (podcrates closed #132; T-103 holds G-09) | T-035, T-103 |
| #78 | COORD-018: podrunner pins podvm v0.2.0-beta.7; the tar forms of PodUtils/podvm@T-1743 pass (the answer to PodUtils/podvm#289) (to podvm) | closed (podvm closed PodUtils/podvm#289; runner-image run 43454398354) | T-096 |
| #79 | COORD-019: podrunner takes podcrates v0.37.0 at 5fc53c4 (the answer to PodUtils/podcrates#134) (to podcrates) | closed (podcrates closed #134 with the take) | T-094 |
| #80 | COORD-020: pod-job: ActorClass needs agent and bot for githome's offers (to podcrates) | closed (podcrates `v0.38.0` gives agent and bot; taken) | T-106 |
| #81 | COORD-021: the machine tier gives each file of the image to root: a non-root user of the image cannot run its programs (to podvm) | closed (`50c084f`, runner-image run 43437838544) | T-037 |
| #82 | COORD-022: podrunner takes podcrates v0.38.0 at 28ce3b5 (the answer to PodUtils/podcrates#136) (to podcrates) | closed (podcrates closed #136 with the take) | T-094 |
| #83 | COORD-023: a machine-tier guest with no NIC and one host channel for podrunner's microvm kind (PodUtils/podvm@T-1751) (to podvm) | closed (podvm `v0.2.0-beta.10`; `50c084f`, runner-image run 43437838544) | T-037, T-036 |
| #84 | COORD-024: podrunner takes podcrates v0.39.0 at 69d7b14 (the answer to PodUtils/podcrates#137) (to podcrates) | closed (podcrates recorded the take on PodUtils/podcrates#137) | T-094 |
| #85 | COORD-025: the incus tier needs stdin for an instance that stays, for podrunner's incus kind (to podvm) | closed (`b328c4e`; runner-image run 43477657803) | T-105 |
| #86 | COORD-026: pod-job: cpu_s and peak_rss_mib in run-report.v1, for githome's G-06 (to podcrates) | closed (podcrates `v0.40.0` taken in `35595a6`) | T-108 |
| #87 | COORD-027: the channel of a machine-tier guest gets no connection when the run has 9p volumes (to podvm) | closed (`50c084f`, runner-image run 43437838544) | T-037 |
| #88 | COORD-028: --podvm-net none and --podvm-channel on the incus tier, for podrunner's incus kind (to podvm) | closed (podvm `v0.2.0-beta.13`; runner-image run 43492630049) | T-109 |
| #89 | COORD-029: podrunner takes podcrates v0.40.0 at 35595a6 (the answer to PodUtils/podcrates#140) (to podcrates) | closed (podcrates recorded the take on PodUtils/podcrates#140) | T-108 |
| #90 | COORD-030: the proxy device of --podvm-channel fails to start on incus 6.0.0 of Ubuntu 24.04 (to podvm) | closed (podvm `v0.2.0-beta.13`; runner-image run 43492630049) | T-109 |
| #91 | COORD-031: pod-job: ContainerRoute::Engine, a container engine inside the unit (to podcrates) | closed (podcrates gave it in v0.42.0; podrunner took it in COORD-036) | T-103 |
| #92 | COORD-032: the kernel of a machine-tier guest for a container engine: bridge, veth, overlay, netfilter, cgroup2 (to podvm) | closed (podvm `v0.2.0-beta.14`; `c817be5`, runner-image run 43514371042) | T-103 |
| #93 | COORD-033: podrunner takes podcrates v0.41.0 at 9f15901 (the answer to PodUtils/podcrates#143) (to podcrates) | closed (podcrates recorded the take on PodUtils/podcrates#143) | T-094 |
| #94 | COORD-034: pod-actions-store: the resolve of action references, a lock file and a store of tarballs (to podcrates) | open (podrunner keeps its copy until a tag) | T-118 |
| #95 | COORD-035: pod-actions-results: the ArtifactService of the results API and its blob routes (to podcrates) | open (podrunner keeps its copy until a tag) | T-119 |
| #96 | COORD-036: podrunner takes podcrates v0.42.0 at 05ceaec (the answer to PodUtils/podcrates#145) (to podcrates) | open (until podcrates records the take) | T-103 |
| #97 | COORD-037: pod-actions-workflow: the filters of on: (types, branches, tags, paths and their -ignore forms) (to podcrates) | closed (taken in `v0.44.0`; T-125 is done) | T-125 |
| #98 | COORD-038: pod-actions-workflow: the expansion of a reusable workflow (jobs.ID.uses) into jobs of the run (to podcrates) | closed (taken in `v0.44.0`; T-131 is done) | T-131 |
| #99 | COORD-039: podrunner takes podcrates v0.43.0 at 51f9e6b, and the additions for pod-actions-store and pod-actions-results (the answer to PodUtils/podcrates#149) (to podcrates) | closed (podcrates recorded the take on PodUtils/podcrates#149; PodUtils/podcrates@T-587 and PodUtils/podcrates@T-588 import the additions) | T-118 |
| #100 | COORD-040: podvm run of busybox:1.37.0 (glibc) fails where chroot is denied: libdl.so.2 is not in the image (to podvm) | open (podvm answered in PodUtils/podvm#351: the release after 0.2.0-beta.16) | T-156 |
| #101 | COORD-041: podrunner takes podcrates v0.44.0 at 8ef1730 (the answer to PodUtils/podcrates#154) (to podcrates) | closed (podcrates closed PodUtils/podcrates#154 when each project took the tag) | T-125 |
| #102 | COORD-042: a guest of another architecture in the machine tier (linux/arm64 on x86_64) (to podvm) | open (T-151 waits for the answer) | T-151 |
| #65 | cfb asks: tracker for the 2 asks of cfb owned by podrunner (from podcrates#35) | open (a tracker; each ask maps to the entries of its line) | T-001 |
| #66 | githome asks: tracker for the 12 asks owned by podrunner (from podcrates#36) | open (a tracker; each ask maps to the entries of its line) | T-001, T-101, T-102, T-103, T-104, T-105, T-106, T-107, T-108 |
| #67 | CI plan items: tracker for the 26 items owned by podrunner (from podcrates#47) | open (a tracker; P0-4 is T-081, the others map to the entries of their lines) | T-001, T-081 |

#44, #45 and #63 are pull requests (#63: Dependabot, base64 0.23, applied on `main`).
