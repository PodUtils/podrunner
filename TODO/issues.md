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
| #9 | Unknowns U1 to U25: register, owners and what settles each | open (U16 and the live rows are open; the live rows wait for #74 and #75) | T-009 |
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
| #74 | LIVE-1: the server agent: run the remote mint for the live gates (T-032, T-033, T-040) | open (the report of 2026-11-15 found 13 defects: T-164 to T-174) | T-032, T-033, T-040, T-164, T-165, T-166, T-167, T-168, T-169, T-170, T-171, T-172, T-173, T-174 |
| #75 | LIVE-2: the runner/user agent: run the unit host and the test jobs of the live gates (T-032, T-033, T-040) | open (the report of 2026-11-15 found 16 defects: T-176 to T-189) | T-032, T-033, T-040, T-176, T-177, T-178, T-179, T-180, T-181, T-182, T-183, T-184, T-185, T-186, T-187, T-188, T-189 |
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
| #94 | COORD-034: pod-actions-store: the resolve of action references, a lock file and a store of tarballs (to podcrates) | closed (podrunner uses `pod-actions-store` of `v0.45.0`) | T-118 |
| #95 | COORD-035: pod-actions-results: the ArtifactService of the results API and its blob routes (to podcrates) | closed (podrunner uses `pod-actions-results` of `v0.45.0`) | T-119 |
| #96 | COORD-036: podrunner takes podcrates v0.42.0 at 05ceaec (the answer to PodUtils/podcrates#145) (to podcrates) | closed | T-103 |
| #97 | COORD-037: pod-actions-workflow: the filters of on: (types, branches, tags, paths and their -ignore forms) (to podcrates) | closed (taken in `v0.44.0`; T-125 is done) | T-125 |
| #98 | COORD-038: pod-actions-workflow: the expansion of a reusable workflow (jobs.ID.uses) into jobs of the run (to podcrates) | closed (taken in `v0.44.0`; T-131 is done) | T-131 |
| #99 | COORD-039: podrunner takes podcrates v0.43.0 at 51f9e6b, and the additions for pod-actions-store and pod-actions-results (the answer to PodUtils/podcrates#149) (to podcrates) | closed (podcrates recorded the take on PodUtils/podcrates#149; PodUtils/podcrates@T-587 and PodUtils/podcrates@T-588 import the additions) | T-118 |
| #100 | COORD-040: podvm run of busybox:1.37.0 (glibc) fails where chroot is denied: libdl.so.2 is not in the image (to podvm) | closed (the glibc case is fixed in podvm `0.2.0-beta.18`; the detached run of a static image is in `0.2.0-beta.19`, PodUtils/podvm#376; its read-only bind is COORD-051, #111) | T-156, T-158 |
| #101 | COORD-041: podrunner takes podcrates v0.44.0 at 8ef1730 (the answer to PodUtils/podcrates#154) (to podcrates) | closed (podcrates closed PodUtils/podcrates#154 when each project took the tag) | T-125 |
| #102 | COORD-042: a guest of another architecture in the machine tier (linux/arm64 on x86_64) (to podvm) | closed (podvm answered no on PodUtils/podvm#366; T-151 is done) | T-151 |
| #103 | COORD-043: podrunner takes podcrates v0.45.0 at b3502d6 and uses its two crates at 5096a3b (the answer to PodUtils/podcrates#157) (to podcrates) | closed | T-118 |
| #104 | COORD-044: podvm save of an index digest refuses to choose a platform, and names an option that save does not have (to podvm) | closed (PodUtils/podvm@T-1895 in `0.2.0-beta.19`; the save from the store of the units passes in runner-image run 43669743775) | T-157, T-163 |
| #105 | COORD-045: podrunner takes podcrates v0.46.0 at 0e12826 (the answer to PodUtils/podcrates#159) (to podcrates) | closed | T-118 |
| #106 | COORD-046: podrunner takes podcrates v0.47.0 at eea3401 (the answer to PodUtils/podcrates#160) (to podcrates) | closed | T-118 |
| #107 | COORD-047: the release of `githome source` for the G-01 pair on the LIVE-2 host (to githome) | closed (githome answered: the pair ran on the LIVE-2 host; conformance 12 of 12) | T-101, T-190, T-191 |
| #108 | COORD-048: podrunner takes podcrates v0.48.0 at c3b5744 (the answer to PodUtils/podcrates#162) (to podcrates) | closed | T-118 |
| #109 | COORD-049: podrunner takes podcrates v0.49.0 at c695295 (the answer to PodUtils/podcrates#163) (to podcrates) | closed | T-118 |
| #110 | COORD-050: limits of memory, CPU and processes for a detached run and an incus instance (to podvm) | closed (podvm answered on PodUtils/podvm#382; the rlimits and the incus keys hold with podvm `0.2.0-beta.21`; the cgroup half is PodUtils/podvm@T-1747, done: `cpus` on the podvm kind is T-221) | T-162, T-221 |
| #111 | COORD-051: a read-only bind (`-v SRC:DST:ro`) on the detached run of a static image where chroot is denied (to podvm) | closed (runner-image run 43686746902 gives `ok static` with podvm `0.2.0-beta.21`) | T-158 |
| #112 | [gh action] requested | closed (gate 43683347311 and secrets 43683349837 green on `ed54ccb`; runner-image 43683637902 on `6ac1c42` red only at the static image, T-158) | T-164, T-165, T-166, T-167, T-158 |
| #113 | COORD-052: podrunner takes podcrates v0.50.0 at cbb5372 (the answer to PodUtils/podcrates#164) (to podcrates) | closed (podcrates closed PodUtils/podcrates#164) | T-118 |
| #114 | [gh action] requested | closed (runner-image 43686746902 green; gate 43686741338 red, fixed at 6372818; level2 43686749522 red, T-196) | T-158, T-162, T-175, T-177, T-178 |
| #115 | COORD-053: a host name of the run for `podvm run -d` (`--hostname`) on the rungs with no UTS namespace (to podvm) | closed (podvm `0.2.0-beta.22` has `--hostname`; T-182 done) | T-182 |
| #116 | Live run 2026-11-15: the scale-set sidecar ignores wait_s of next, and mint kills it at its first idle poll | closed (T-164 is done) | T-164 |
| #117 | Live run 2026-11-15: sign refuses each restarted sidecar for 60 s, and the backoff never grows: a livelock | closed (T-165 is done) | T-165 |
| #118 | Live run 2026-11-15: the scale-set sidecar prints "no answer" for each error that is not an HTTP status | closed (T-168 is done) | T-168 |
| #119 | Live run 2026-11-15: the scale-set sidecar does not read JobAssignedMessages, so no job of github.com is offered | closed (T-166 is done) | T-166 |
| #120 | Live run 2026-11-15: the trust lookup reads repository.visibility of a run, which is null | closed (T-167 is done) | T-167 |
| #121 | Live run 2026-11-15: a job that the tier rule refuses keeps the capacity of the scale set: one fork PR stops a trusted set | open (T-169 is partial: the cancel is on `main`, for the next beta) | T-169 |
| #122 | Live run 2026-11-15: a withdrawal of the tier rule in the mint leaves no audit line and no log line | closed (T-170 is done) | T-170 |
| #123 | Live run 2026-11-15: the sidecar never closes its message session, so the next sidecar gets 409 | closed (T-168 is done) | T-168 |
| #124 | Live run 2026-11-15: podrunner check refuses each key of a mint file | closed (T-171 is done) | T-171 |
| #125 | Live run 2026-11-15: check says a configuration is usable when no unit kind serves its source | closed (T-185 is done) | T-185 |
| #126 | Live run 2026-11-15: doctor says egress_hold unknown for the podvm kind while serve says enforced or advisory | open (T-181 is partial) | T-181 |
| #127 | Live run 2026-11-15: the tree-kill self-test of a held podvm unit fails, and its units stay after a stop | closed (fixed; the entries are done) | T-183 |
| #128 | Live run 2026-11-15: status and stop --wait answer busy while the supervisor waits on a source | closed (T-184 is done) | T-184 |
| #129 | Live run 2026-11-15: the unit host calls next of a remote mint about 10 times each second | closed (T-164 is done) | T-164 |
| #130 | Live run 2026-11-15: the audit of sign is kept in memory only | closed (T-172 is done) | T-172 |
| #131 | Live run 2026-11-15: podrunner --version does not name the beta | closed (T-173 is done) | T-173 |
| #132 | Live run 2026-11-15: no PAT path for a github-scaleset source, although the spec allows one | closed (T-174 is done) | T-174 |
| #133 | Live run 2026-11-15: issue #75 names the expiry of the runner image a day early, and not runner-full-x86_64 | closed (T-195 is done) | T-195 |
| #134 | Live run 2026-11-15: the design copy of job-offer.v1 differs from the parser, and a parse failure reads as event-not-allowed | closed (T-188 is done) | T-188 |
| #135 | Live run 2026-11-15: serve does not pull a digest-pinned container image, and the hook hides the cause | closed (T-177 is done) | T-177 |
| #136 | Live run 2026-11-15: a redirect host can serve one repository only, so the G-01 pair cannot run on it | closed (T-193 is done) | T-193 |
| #137 | Live run 2026-11-15: a podvm host runs one unit at a time while status says slots 2 | closed (T-185 is done) | T-185 |
| #138 | Live run 2026-11-15: local mode refuses the netns hold of podnet | open (T-187 is partial) | T-187 |
| #139 | Live run 2026-11-15: the tier rule trusts an organization owner only | closed (T-191 is done) | T-191 |
| #140 | Live run 2026-11-15: the JIT configuration is minted before the unit starts, and a failed start leaves the job assigned | open (T-176 is done; the job that stays assigned is T-169, Q7) | T-176, T-169 |
| #141 | Live run 2026-11-15: on incus and microvm units the runner is not root and HOME is /root | closed (T-178 is done) | T-178 |
| #142 | Live run 2026-11-15: on H0 the runner times out reaching github.com under the seccomp hold | closed (T-179 is done: the seccomp hold holds in each lane of `user-gate.yml` with podnet `0.4.0-beta.17`, in CI; the live run on H0 stays with #75) | T-179 |
| #143 | Live run 2026-11-15: podrunner as PID 1 does not reap the processes of a unit, and its supervisor stops | open (T-180 is partial) | T-180 |
| #144 | Live run 2026-11-15: serve stops after a unit ends unit-failed | open (T-192 is partial) | T-192 |
| #145 | Live run 2026-11-15: a refused offer of a program source is not withdrawn to the source | closed (T-189 is done) | T-189 |
| #146 | Live run 2026-11-15: an aarch64 host under QEMU user mode serves no tier | closed (T-194 is done) | T-194 |
| #147 | Live run 2026-11-15: a podvm unit has the host name of the host | closed (fixed; the entries are done) | T-182 |
| #148 | [gh action] requested | closed (gate 43690748922 and secrets green on `ba24d0e`; runner-image 43690754546 red at the limits measure, T-162; level2 red at the corpus, T-196 fixed after it) | T-162, T-183, T-192 |
| #149 | Live run 2026-11-15: on an emulated aarch64 host, start_s (120 s) kills the first unit, and nothing names the deadline | open | T-197 |
| #150 | [gh action] requested | closed (gate 43704598372, level2 43704604467, runner-image 43704601731 green on `88cc4d7`; release 43708786078 of `v0.1.0-beta.6`) | T-162, T-169, T-198 |
| #151 | COORD-054: githome and podrunner after the live run: the beta.6 changes and the next pair run (to githome) | open (the pair on beta.6 passed each test but one: T-222) | T-162, T-178, T-189, T-190, T-191, T-193, T-198, T-200, T-201, T-222 |
| #152 | COORD-055: podrunner takes podcrates v0.51.0 at 800f6d4 (the answer to PodUtils/podcrates#172) (to podcrates) | closed (podcrates closed PodUtils/podcrates#172) | T-118 |
| #153 | LIVE-3: a user agent of the local mode: run real workflows with `podrunner run` and report (T-199) | open (the report of 2026-11-15: no job ran on its host; 12 issues #155 to #166 and nits, T-206 to T-219) | T-199, T-218, T-219 |
| #154 | [gh action] requested | closed (replaced by #179; the gate, runner-image and level2 on `2f56267`, then the release of `v0.1.0-beta.7`) | T-162, T-169, T-198, T-200, T-201 |
| #155 | Local mode 2026-11-15: --platform turns off every later flag check, so nine refusals run and two executors can be given at once | closed (fixed; the entries are done) | T-206 |
| #156 | Local mode 2026-11-15: doctor answers a missing --podvm or --incus binary with a podvm version bump, which cannot fix a wrong path | closed (fixed; the entries are done) | T-212 |
| #157 | Local mode 2026-11-15: "one of --runner-dir DIR, or a unit kind ... is necessary" is printed while --runner-dir is given | closed (fixed; the entries are done) | T-207 |
| #158 | Local mode 2026-11-15: no subcommand has help, `run --help` is an unknown option, and an unhandled short option becomes a file | closed (fixed; the entries are done) | T-208 |
| #159 | Local mode 2026-11-15: --validate says valid for a dangling needs and a needs cycle that --graph and --dry-run refuse | closed (fixed; the entries are done) | T-209 |
| #160 | Local mode 2026-11-15: --break takes a job that the workflow does not have, a step that does not exist, and an id that is not a step | closed (fixed; the entries are done) | T-210 |
| #161 | Local mode 2026-11-15: --local-repository still opens the fetch relay, so a run with no forge fails unless --offline is also given | closed (fixed; the entries are done) | T-211 |
| #162 | Local mode 2026-11-15: a loopback bind failure is a raw os error, and doctor preflights neither the bind nor the read of the hierarchy above the runner directory | closed (fixed; the entries are done) | T-213 |
| #163 | Local mode 2026-11-15: status, drain, stop, check and audit verify do not name the path that is missing | closed (fixed; the entries are done) | T-214 |
| #164 | Local mode 2026-11-15: stop --wait prints stopped and exits 0 for a state directory that does not exist, while stop exits 69 | closed (fixed; the entries are done) | T-215 |
| #165 | Local mode 2026-11-15: mint-serve reports a TOML type error at host for a run configuration, and check calls the same file usable | closed (fixed; the entries are done) | T-216 |
| #166 | Live run 2026-11-15: the published runner-x86_64 artifact cannot be imported by the pinned podvm, and it carries no ref name | closed (fixed; the entries are done) | T-217 |
| #167 | Local mode 2026-11-15: a host that denies AF_INET bind cannot serve the fake broker, and the static binary keeps podnet's shim out | closed (`--vnet`: T-220 and T-561 done, `user-gate.yml` run 44664120384, the lane `bind-denied`) | T-220, T-228 |
| #168 | COORD-056: a store flag of `load` and `images`, and an attestation of each release (to podvm) | closed (podvm `0.2.0-beta.32`: `--store` and an attestation of each binary, COORD-185) | T-218 |
| #169 | COORD-057: podrunner takes podcrates v0.52.0 at 45bd964 (the answer to PodUtils/podcrates#176) (to podcrates) | closed (podcrates closed PodUtils/podcrates#176) | T-118 |
| #170 | COORD-058: a TCP address of the vnet that connects to a unix socket, for a payload in a podvm unit (to podnet) | closed (podnet v0.4.0-beta.14; `--vnet`, T-220) | T-220 |
| #171 | Local mode 2026-11-15: the run should pick its own transport and environment, instead of leaving that to the reader of four documents | open (T-229 is open) | T-229 |
| #172 | Local mode 2026-11-15: `doctor --run` reports `ok store` for an empty store and never checks that `--image` is in it | closed (fixed; the entries are done) | T-223 |
| #173 | Local mode 2026-11-15: a run that hangs prints nothing until it ends, so the phase it is stuck in is invisible | closed (fixed; the entries are done) | T-224 |
| #174 | Local mode 2026-11-15: the unit store is built under `$TMPDIR`, and the error tells the user to set `$PODVM_STORE`, which podrunner overrides | closed (fixed; the entries are done) | T-225 |
| #175 | Local mode 2026-11-15: a `--podvm` run never says what egress hold the unit has, and here the only hold podnet offers is the advisory one | closed (fixed; the entries are done) | T-226 |
| #176 | Local mode 2026-11-15: podnet's supervision makes podvm refuse the unit start (a passed descriptor goes onto 17) | closed (fixed; the entries are done) | T-227 |
| #177 | COORD-059: `podvm run -d` refuses "a passed descriptor goes onto 17" when podrunner runs under `podnet pod` (to podvm) | closed (podvm `0.2.0-beta.24`; T-227 done) | T-227 |
| #178 | COORD-060: on the userland rung, `tar -x` fails when a symbolic link comes before its target (to podvm) | closed (podvm `0.2.0-beta.24`; T-232 done) | T-232 |
| #179 | [gh action] requested | closed (the block of #154 ran on `037cb35`: gate 43730995552 red at shellcheck, T-251; runner-image 43730998943 green; no release) | T-251 |
| #180 | COORD-061: test labs: each hostile host class as a shared profile that any project brings up on any host (to podcrates) (to podvm) | closed (the part of podvm holds; `pod-lab` of podcrates is T-264) | T-244 |
| #181 | COORD-062: what podrunner's help and manual need of pod-manual (the answer to PodUtils/podnet#584) (to podnet) (to podcrates) | open (podcrates takes `pod-manual`: PodUtils/podcrates#181) | T-248 |
| #182 | COORD-063: the lab profiles that podrunner needs first, as `podvm lab run` (the answer to PodUtils/podvm#424) (to podvm) | closed (`podvm lab run` in the lane `bind-denied`) | T-244 |
| #183 | COORD-064: podrunner's installer and the lessons of its first user runs (the answer to PodUtils/podcrates#182) (to podcrates) | open | T-249 |
| #184 | COORD-065: how podrunner's record maps an issue to its entries (the answer to PodUtils/podcrates#183) (to podcrates) | open | T-250 |
| #185 | [gh action] requested | closed (gate 43736537842 and level2 43736544211 green on `affb8c9`; runner-image 43736540966 red at a residue of the local suite, T-159; no user-path gate, no release) | T-159, T-243, T-251 |
| #186 | [gh action] requested | closed (gate 43738992229, level2 43738999554, runner-image 43738996158 green on `f0fd5f2`; the first user-path gate 43743490768 red at the corpus, T-253 to T-255; no release) | T-159, T-243, T-253, T-254, T-255 |
| #187 | [gh action] requested | closed (gate 43750365495 green on `92a7394`; level2 43750372759 red at the token test, T-256; runner-image 43750369310 red at the parity suite: an empty token, T-254; the links of tar green, T-232; no user-path gate, no release) | T-232, T-254, T-256 |
| #188 | [gh action] requested | closed (gate 43754963454, runner-image 43754966711, level2 43754970180 green on `b5c9214`; the user-path gate 43758947099 red: `--microvm` and three corpus rows on podvm, T-261 to T-263; no release) | T-243, T-254, T-256, T-261, T-262, T-263 |
| #189 | COORD-066: three jobs fail in a podvm unit and pass on a runner directory: persist of a temporary file EACCES, Node package config (to podvm) | open (podvm answered on PodUtils/podvm#470: PodUtils/podvm@T-1935, the syscall tier for `run -d`; the rung of the Node row waits for the user gate) | T-261, T-262 |
| #190 | [gh action] requested | closed (gate 43782934101, level2 43782940547, runner-image 43782937209 green on `12cb64e`; user-gate 43787604846 red: express-lint on podvm with `UV_USE_IO_URING=0`, T-262; `--microvm` as a user with no root, T-291; no release asked) | T-243, T-261, T-262, T-263, T-291 |
| #191 | COORD-067: pod-hostprobe: a mount and a chroot probe in a user namespace with an ID map (to podcrates) | closed (podcrates v0.59.0; T-283 done) | T-283 |
| #192 | [gh action] requested | closed (gate 43789349432, level2 43789355829, runner-image 43789352527 green on `45ef3c3`; user-gate 43794547449 on `52e7356` refused the images of `45ef3c3` at prepare: a run of an older commit now serves when `images/` is the same) | T-246, T-265, T-284 |
| #193 | COORD-068: Node 26 still reads an invalid package.json with UV_USE_IO_URING=0, and the machine tier ends before the exit code as a user with no root (to podvm) | closed (Node 26: podvm beta.26, T-262 done; the machine tier: T-291 done) | T-262, T-291 |
| #194 | [gh action] requested | closed (gate 43795116133 and level2 43795119948 green on `a4b41b9`; user-gate 43795123788 red: express-node22 on runner-dir in CI (T-255), express-lint on podvm passed in one lane (T-262), rq-lint on runner-dir in proxy-only (T-299)) | T-246, T-284, T-291, T-299 |
| #195 | COORD-069: the machine tier ends before the exit code as a user with no root: the line of QEMU (to podvm) | closed (PodUtils/podvm#444; T-291 done) | T-291 |
| #196 | COORD-070: on the namespace rung of run -d, the payload is root of a one-uid namespace, with no UTS namespace (to podvm) | closed (podvm beta.26 and beta.27, PodUtils/podvm#448; T-317 done) | T-317, T-182 |
| #197 | COORD-071: a program that closes descriptor 17 gets podvm's ownership memo written into the next file it opens (to podvm) | closed (podvm beta.26, PodUtils/podvm#450; T-309 done) | T-309, T-262 |
| #198 | COORD-072: the machine tier as a user with no root, with v0.2.0-beta.25: the guest does not reach the command (to podvm) | closed (PodUtils/podvm#451; T-291 done) | T-291 |
| #199 | COORD-073: the machine tier as a user with no root: alpine boots, the runner image (initrd 1348 MiB) closes its console after 2.6 s (to podvm) | closed (PodUtils/podvm#454: 8 GiB; T-291 done) | T-291 |
| #200 | COORD-074: a PID namespace for run -d on the namespace rung: a job sees and signals the processes of the host (to podvm) | closed (podvm `0.2.0-beta.30` pinned; T-318 done) | T-318 |
| #201 | COORD-075: the seccomp hold of podnet pod lets a job connect to each port on the loopback, also a proxy (to podnet) | closed (PodUtils/podnet#605: `--loopback-rules`; podnet `0.4.0-beta.13` pinned; T-319 done) | T-319 |
| #202 | COORD-076: podvm run connects to help, archive and security.ubuntu.com:443 at the start of each unit (to podvm) | closed (PodUtils/podvm#464: the source fixup; `--no-source-fixup` passed; T-332 done) | T-332 |
| #203 | Local mode: a workflow symlink that points out of the project is followed, not refused | closed (fixed on `main`; the entry is done) | T-340 |
| #204 | Local mode: .secrets in the tree bypasses the validation an explicit --secret-file gets | closed (fixed on `main`; the entry is done) | T-341 |
| #205 | doctor: doctor --run --json emits no JSON, and doctor --json --run is an unknown option | closed (fixed on `main`; the entry is done) | T-342 |
| #206 | print-unit: unquoted/relative/empty ExecStart values, and a nonexistent --config is accepted | closed (fixed on `main`; the entry is done) | T-343 |
| #207 | image verify: the release's runner-x86_64.tar asset is rejected, and the error does not name the expected tarball | closed (fixed on `main`; the entry is done) | T-344 |
| #208 | run: a dry run writes --state/artifacts and the cache, and does not validate executor paths | closed (fixed on `main`; the entry is done) | T-345 |
| #209 | run: nested uses: of a local composite action are not pinned, and a local-repository composite misreports the refusal as offline | closed (fixed on `main`; the entry is done) | T-346 |
| #210 | run: --watch/--schedule with a one-shot mode hangs; --schedule on a schedule-less workflow says on.schedule has no cron time | closed (fixed on `main`; the entry is done) | T-347 |
| #211 | run: --validate --json emits no JSON for an invalid workflow | closed (fixed on `main`; the entry is done) | T-348 |
| #212 | run: a pre-start failure claims a log in --log-dir that is not written | closed (fixed on `main`; the entry is done) | T-349 |
| #213 | v0.1.0-beta.7 pins podnet 0.4.0-beta.13, but the vnet forward needed by #170 is in 0.4.0-beta.14 | closed (podnet `0.4.0-beta.17` pinned; the forward through `--vnet` passes in `user-gate.yml` run 44664120384) | T-350 |
| #214 | COORD-077: pod-actions-workflow: the error of an unknown need marks the job, not its needs: (to podcrates) | closed (podcrates `v0.70.0`, PodUtils/podcrates@T-859) | T-331 |
| #215 | COORD-078: pod-broker: names_upstream still resolves the name itself, so a host whose DNS is only through its proxy refuses each CONNECT (to podcrates) | closed (podcrates `v0.70.0`, PodUtils/podcrates@T-862) | T-351 |
| #216 | COORD-079: a podvm unit has no /dev/pts: pty.spawn and script fail, and pager tests fail (to podvm) | closed (podvm `0.2.0-beta.32` mounts a devpts of the unit; the pty step of `runner-image.yml` passed) | T-354 |
| #217 | COORD-080: the answer to PodUtils/podnet#649 (COORD-102): #213, #142, and what podrunner's gate proves (to podnet) | closed (podnet read it, PodUtils/podnet#649) | T-372, T-371, T-245, T-179 |
| #218 | `--runner-dir` writes the runner files, a private key with them, at mode 0644 | closed (fixed on `main`; the entry is done) | T-373 |
| #219 | `--state` writes the outputs of the jobs into a file that each user can read | closed (fixed on `main`; the entry is done) | T-374 |
| #220 | `--checkout` of a directory in a git tree gets a `.git` of the whole tree | closed (fixed on `main`; the entry is done) | T-375 |
| #221 | Each job leaves the thread and the listener of its fake broker | closed (fixed on `main`; the entry is done) | T-376 |
| #222 | Two concurrency groups can share one lock file | closed (fixed on `main`; the entry is done) | T-377 |
| #223 | `--break-on-failure` pauses after each step after a failure; `--break` before an `always()` step is skipped | closed (fixed on `main`; the entry is done) | T-378 |
| #224 | `timeout-minutes` is held to 6 days | closed (fixed on `main`; the entry is done) | T-379 |
| #225 | The image of an excluded cell of a matrix is pulled | closed (fixed on `main`; the entry is done) | T-380 |
| #226 | `run DIR --checkout DIR` replaces the value of `--checkout` with a workflow file | closed (fixed on `main`; the entry is done) | T-381 |
| #227 | A container mount can name the runner root or a file of it | closed (fixed on `main`; the entry is done) | T-383 |
| #228 | A secret that starts a longer secret leaves the rest of the longer one in a log | closed (fixed on `main`; the entry is done) | T-384 |
| #229 | `--github-token-env ""` fails, though the documents say it sends no token | closed (fixed on `main`; the entry is done) | T-385 |
| #230 | A `pull_request` filter of branches reads the default branch, not the base branch | closed (fixed on `main`; the entry is done) | T-386 |
| #231 | A `workflow_dispatch` with no inputs takes each `--input` | closed (fixed on `main`; the entry is done) | T-387 |
| #232 | `check` passes a `mem` of `[unit.podvm]` or `[unit.microvm]` that `serve` refuses | closed (fixed on `main`; the entry is done) | T-388 |
| #233 | The control socket takes a field that an op does not take | closed (fixed on `main`; the entry is done) | T-389 |
| #234 | An error of a source whose text starts with "the " ends its sidecar | closed (fixed on `main`; the entry is done) | T-390 |
| #235 | A stale first payload of an architecture hides a fresh one after it | closed (fixed on `main`; the entry is done) | T-391 |
| #236 | The affinity of a shared principal compares a repository with case | closed (fixed on `main`; the entry is done) | T-392 |
| #237 | A dry run ends the processes and removes the files of a dead run | closed (fixed on `main`; the entry is done) | T-393 |
| #238 | The actions and images of a called workflow of the tree are not pinned before the first job | closed (fixed on `main`; the entry is done) | T-395 |
| #239 | A later matrix `include` does not overwrite a value that an earlier one added | closed (fixed on `main`; the entry is done) | T-396 |
| #240 | A matrix `exclude` with an expression drops each cell | closed (fixed on `main`; the entry is done) | T-397 |
| #241 | `--vnet` gives no `NODE_USE_ENV_PROXY` | closed (fixed on `main`; the entry is done) | T-398 |
| #242 | `--offline` with a store of actions that the user cannot write is refused | closed (fixed on `main`; the entry is done) | T-399 |
| #243 | A local composite action of `./../PATH` or a link out of the tree is read | closed (fixed on `main`; the entry is done) | T-400 |
| #244 | A dry run with a runner directory that cannot take the lock says planned | closed (fixed on `main`; the entry is done) | T-401 |
| #245 | The mint forgets a late credential that starts or goes back after its alert | closed (fixed on `main`; the entry is done) | T-402 |
| #246 | mint: one thread for each TCP connection with no cap, each waiting up to 90 s | closed | T-435 |
| #247 | guest: the self-test needs a loopback bind, so a host that denies loopback can never claim guest | closed | T-436 |
| #248 | placement: a required limit that the kind reports is refused, although the docs allow reported | closed | T-437 |
| #249 | policy: a rule with an unknown field is accepted and the field is dropped, broadening the policy | closed | T-438 |
| #250 | run: the per-run and per-unit scratch directories under TMPDIR follow a pre-created symlink | closed | T-407 |
| #251 | run: a checkout of another repository with an empty, full or SHA ref is not resolved | closed (podcrates `v0.71.0`, T-439) | T-439 |
| #252 | run: fetch::host_of cuts an IPv6 action-server address at the first colon | closed | T-440 |
| #253 | run: a runs-on label of ppc64le, s390x, riscv64, i386, i686 or armv7 is not refused | closed | T-441 |
| #254 | oidc: job_workflow_ref names the caller's workflow for a job of a reusable workflow | closed | T-434 |
| #255 | serve: a malformed offer is refused as event-not-allowed with no detail, not bad-request | closed | T-442 |
| #256 | serve: a job-completed hook event is lost when the unit ends before the next tick | closed | T-443 |
| #257 | podvm: a start that fails after run -d leaves the podvm unit, because destroy skips podvm rm | closed | T-444 |
| #258 | source-conformance: a timed-out test leaves its thread and child process alive | closed | T-445 |
| #259 | serve: two source names that map to the same spool file lose one refused report | closed | T-446 |
| #260 | front: a guest can grow memory with an unbounded request line and hold threads with no read timeout or cap | open | T-412 |
| #261 | run: a lock key or uses: with ../x@v1 is accepted and the fetch URL normalizes to another repository | closed | T-447 |
| #262 | check: a podvm config with a bad upstream_proxy, an old podvm or a bad podnet passes check and stops serve | closed | T-448 |
| #263 | check_driver: a non-executable driver and a writable ancestor are accepted | closed | T-449 |
| #264 | run: a dry run with a --vnet podnet binary that cannot build the shim says planned | closed | T-450 |
| #265 | audit: a failed write still appears in the in-memory lines and the next line reuses its seq | closed | T-451 |
| #266 | guest: the self-test skips the first unit's file check when the driver reports no unit_root | closed | T-452 |
| #267 | status: no_tier_served ignores the egress hold, so it says a tier is served when every offer is refused | closed | T-453 |
| #268 | serve: a spooled report of a source that is no longer configured is skipped forever | closed | T-454 |
| #269 | serve: a bad journal line is counted in Replay.bad_lines and never reported | closed | T-455 |
| #270 | check: a pin payload with an invalid arch (riscv64, x86-64) is accepted and then refuses every job | closed | T-456 |
| #271 | check: an empty [unit.podvm] command, a relative runner_dir and a bad registry host are accepted | closed | T-457 |
| #272 | guest: the microvm and incus scripts set HTTP(S)_PROXY but not NODE_USE_ENV_PROXY | closed | T-458 |
| #273 | incus: the instance name is a 32-bit hash, so two state directories can collide | closed | T-459 |
| #274 | microvm: destroy of a unit that is not in the map leaves its directory and reports a residue | closed | T-460 |
| #275 | run: a host path with a colon breaks the -v mount of a unit (toolcache, export, container mount) | closed | T-461 |
| #276 | microvm: remove_runs_of uses the podvm PID, which can be reused before the run directory is removed | closed | T-462 |
| #277 | COORD-081: podvm run and exec: a -v source with no link, and an environment that is not argv (to podvm) | open (waits for podvm) | T-403, T-405 |
| #278 | hooks: a job container with no entry point is started with tail, which a distroless image does not have | closed | T-463 |
| #279 | podvm: a config without [unit.podvm] runner_dir gives the hook policy an empty work_root and refuses every container job | closed | T-464 |
| #280 | check: tiers::served ignores the egress hold, so a config with no hold passes check and serves nothing | closed | T-465 |
| #281 | shim: the close_range fallback closes only 3..4096, so a higher descriptor leaks into the payload | closed | T-466 |
| #282 | mint: the revocation message calls the source name a credential | closed | T-467 |
| #283 | check: a guest_kinds entry that is not a [unit.NAME] table passes check and drops the guest claim | closed | T-468 |
| #284 | lock: an uppercase digest is accepted and then fails every action against the lowercase SHA-256 | closed (podcrates `v0.70.0`, T-469) | T-469 |
| #285 | status: trust_unknown counts a repository or owner literally named unknown | closed | T-470 |
| #286 | policy: a host with a trailing dot is not covered by a wildcard ceiling | closed | T-471 |
| #287 | check: a pin payload with a duplicate reference, a future built_at_s, max_age_s 0 or a bad runner_version is accepted | closed | T-472 |
| #288 | COORD-082: pod-actions-store: Lock::parse keeps a digest in uppercase, and each fetch then fails against the lowercase SHA-256 (to podcrates) | closed (podcrates `v0.70.0`, PodUtils/podcrates@T-902) | T-469 |
| #289 | COORD-083: the answer to COORD-131 (PodUtils/podcrates#221): the copies of shared code in podrunner (to podcrates) | closed (podcrates `v0.70.0` taken: T-473, T-474) | T-473, T-474 |
| #290 | COORD-084: pod-actions-store: the ref of a checkout of another repository (empty, full ref, commit SHA) is not resolved (to podcrates) | closed (podcrates `v0.71.0`, PodUtils/podcrates@T-906) | T-439 |
| #291 | COORD-085: a reader of the zone files (TZif) for the timezone of on.schedule (to podcrates) | closed (podcrates `v0.71.0`, PodUtils/podcrates@T-907) | T-423 |
| #292 | from cfb | open (cfb's questions P-1 to P-7 of its CI host; podrunner answered on the issue) | T-479, T-480, T-481, T-482, T-483, T-484, T-485, T-489, T-493, T-494, T-495, T-496, T-497, T-498, T-499, T-500, T-501, T-502, T-503, T-504, T-505, T-506, T-507, T-508, T-509, T-510, T-511, T-512 |
| #293 | COORD-086: the owner's ruling of 2026-11-18: the needs of githome and cfb come first, and from the next session all of PodUtils makes podrunner complete and stable (to podcrates) | open (podcrates answered A1 to A4 in COORD-134, PodUtils/podcrates#225; the entries are open) | T-489, T-490, T-491 |
| #294 | COORD-087: the answer to COORD-135 (PodUtils/podcrates#226): the types of status and of the source wire, the stable fields, and the new fields of githome's asks (to podcrates) | closed (podcrates `v0.73.0` carries the fields; T-508 and T-514 are done) | T-508, T-514 |
| #295 | COORD-088: the start report of 2026-11-18 and the take of v0.70.0; two extensions of FileLock (to podcrates) | closed (podcrates `v0.71.0`, PodUtils/podcrates@T-924) | T-513 |
| #296 | COORD-089: the asks of githome's and cfb's needs: the root disk and the usage of an incus instance, an engine in an instance, pids and a sized disk in a guest, OOM facts, a foreign guest (to podvm) | open (podvm answered with COORD-187, PodUtils/podvm#583: asks 1, 2, 3, 4, 6 in `0.2.0-beta.32` and `0.2.0-beta.33`; 5 waits) | T-482, T-483, T-485, T-488, T-490, T-496, T-515 |
| #300 | COORD-090: --env-file for a value with a newline and on the guest tiers; --podvm-volume-base on the rungs with containers (to podvm) | open (podvm answered with COORD-189, PodUtils/podvm#585: `--env-file0` in `0.2.0-beta.33` is taken; the flag off the namespace rung comes later) | T-403, T-405, T-532 |
| #301 | COORD-091: under --podvm-tier=syscall on the userland rung, a payload cannot connect to a unix socket of a -v directory (to podvm) | closed (podvm `0.2.0-beta.33` maps the socket; the container steps passed under the tier, `runner-image.yml` run 44331618064) | T-261 |
| #302 | COORD-092: the take of v0.71.0; C783 confirmed; the state of podrunner's order (to podcrates) | open (a report to the coordinator; closes when T-494 and T-497 take the fields of `pod-job`) | T-494, T-497, T-506 |
| #303 | COORD-093: under --podvm-tier=syscall on the userland rung, an absolute symbolic link does not resolve in the rootfs of the unit (to podvm) | closed (podvm `0.2.0-beta.34` fixes it; `runner-image.yml` run 44376274000 is green with the tier) | T-261 |
| #304 | COORD-094: the confine layer and the syscall tier: the self-test of confine is not proven under the tier (to podvm) | open (podvm answered with COORD-194, PodUtils/podvm#590: `run -d` under both did not start; fixed on its `main`, for its next beta) | T-261 |
| #305 | COORD-095: the take of v0.72.0; three optional fields of status.v1 for pod-job (to podcrates) | closed (podcrates `v0.73.0` has them, PodUtils/podcrates@T-947; taken in COORD-100, #310) | T-508, T-536 |
| #306 | COORD-096: githome's G-38 needs githome's source as a program that podrunner's CI can pin (to podcrates) | open (podcrates answered with COORD-139, PodUtils/podcrates#230: `pod-fake-githome source`, PodUtils/podcrates@T-948, a release asset of a later tag; closes when T-509 runs it) | T-509 |
| #307 | COORD-097: the answer to COORD-196: the three limits of --podvm-confine do not weaken podrunner's confined kind (to podvm) | open (closes when podvm names the tag that carries its PodUtils/podvm@T-1983 to PodUtils/podvm@T-1985) | T-261 |
| #308 | COORD-098: under --podvm-tier=syscall, the exec of a dynamic program gives "native-static rung: ... ld-linux-x86-64.so.2: errno 11" (to podvm) | open (podvm answered with COORD-197, PodUtils/podvm#593: errno 11 is an `openat2` EAGAIN, retried on its `main` for the beta after `0.2.0-beta.35`; the hang of `rg-docs` does not reproduce there, and podvm asks for `uname -r`, `ps`, the stack and the fds of the waiting process; closes when the express rows pass with the tier) | T-261 |
| #309 | COORD-099: the user-path gate with podnet 0.4.0-beta.17: the seccomp hold holds on each lane, also with no user namespace (#142) (to podnet) | open (closes when podnet has read it) | T-179 |
| #310 | COORD-100: the take of v0.73.0; source-conformance has 15 tests for pod-fake-githome (to podcrates) | open (a report to the coordinator; closes when T-509 runs `pod-fake-githome` against the 15 tests) | T-508, T-509 |
| #311 | COORD-101: a runner job through serve: pod-fake-githome mints the JIT configuration of pod-fake-github (to podcrates) | open (podcrates `v0.74.0` has it, COORD-141, PodUtils/podcrates#237: `pod-fake-githome serve --github URL`; closes when a runner job runs through `serve` with it) | T-527, T-509 |
| #312 | COORD-102: the owner's ruling "ghcr.io": each project publishes its CLIs and assets on ghcr.io (to podcrates) (to all) | open (the coordinator gave the form in COORD-142, PodUtils/podcrates#236, call C789; closes when podrunner publishes in it, T-553) | T-538, T-553 |
| #313 | COORD-103: systemd as PID 1 in an incus instance and in a guest of the machine tier, for the GitHub-hosted toolset (to podvm) | open (podvm answered with COORD-200, PodUtils/podvm#596: the incus tier first, PodUtils/podvm@T-1993, then the machine tier in three steps, PodUtils/podvm@T-1994; closes when podrunner takes both) | T-543, T-544 |
| #314 | COORD-104: the take of v0.74.0; pod-fake-githome --github for a runner job through serve (to podcrates) | open (a report to the coordinator; closes when a runner job runs through `serve` with the fakes) | T-509, T-527 |
| #315 | COORD-105: on the userland rung with no tier, the compile of TypeScript 7 (a Go binary) hangs (to podvm) | closed (podvm `0.2.0-beta.36` fixed it, COORD-202, PodUtils/podvm@T-1995; the row `rbg-gate` passes on the podvm executor in `user-gate.yml` run 44489222156) | T-554, T-528 |
| #316 | COORD-106: the hosted snapshot on podvm: id map, nesting, FUSE and a large import on the incus tier; the image's kernel on the machine tier; an incus VM mode; xattrs (to podvm) | open (podvm's answer COORD-203, PodUtils/podvm#599: PodUtils/podvm@T-1996, @T-1999, @T-1997, @T-1994, @T-1998, in that order; closes when podrunner takes each ask) | T-543, T-544, T-556, T-558, T-559 |
| #317 | COORD-107: pod-extract keeps xattrs and reads each gzip member of a layer (to podcrates) | open (podcrates `v0.75.0` has ask 2, PodUtils/podcrates@T-956, COORD-143, PodUtils/podcrates#238; podcrates `v0.76.0` has ask 1, PodUtils/podcrates@T-957, COORD-144, PodUtils/podcrates#239; closes when podrunner takes both) | T-558, T-560 |
| #318 | COORD-108: the take of v0.75.0; pod-fake-githome pinned by ghcr.io digest; T-560 waits for podvm's take (to podcrates) | open (a report to the coordinator; closes when podvm takes `v0.75.0`, T-560) | T-560, T-553, T-509 |
| #319 | COORD-109: the take of v0.76.0; T-558 waits for podvm's take of the xattrs (to podcrates) | open (a report to the coordinator; closes when podvm sets the xattrs in a unit, T-558) | T-558, T-509 |
| #320 | COORD-110: the take of v0.77.0; release-index.json in podrunner's release is T-562 (to podcrates) | open (a report to the coordinator; closes when a release of podrunner has `release-index.json`, T-562) | T-562, T-484, T-509 |
| #321 | COORD-111: the take of v0.78.1; Load::new for a job with no secret; pod-release for T-562 (to podcrates) | open (a report to the coordinator; closes when T-562 runs `pod-release` in `release.yml`) | T-562, T-563, T-509 |
| #322 | [gh action] requested | closed (`hosted-boot.yml` run 44659969426 on `5583c93`: systemd of the image is PID 1, then the instance powers off with no file of the checks) | T-556 |
| #323 | [gh action] requested | closed (`hosted-boot.yml` run 44663656956 on `02cd495`; its `user-gate.yml` run 44663660686 stopped at `prepare`: no `images_run`, and no green `runner-image.yml` run of that commit; replaced by #324) | T-556, T-561 |
| #324 | [gh action] requested | closed (on `f084c4c`: gate 44664111796, `level2.yml` 44664115893 and `user-gate.yml` 44664120384 green; `pass lab:vnet` in the lane `bind-denied`) | T-561 |
| #325 | COORD-112: run -d --podvm-init=image on the incus tier: a degraded boot runs no command, stops the instance, and exits 0 (to podvm) | open (podvm answered with COORD-212, PodUtils/podvm#609: the Entrypoint of the snapshot ran as a prefix of the command; fixed on podvm `main`, PodUtils/podvm@T-2011; closes when the next beta boots the checks) | T-556 |
| #326 | COORD-113: exec into a run -d container of the namespace rung refuses its volumes, so a step of a container: job cannot run there (to podvm) | open (podvm answered with COORD-214, PodUtils/podvm#612: fixed on podvm `main`, PodUtils/podvm@T-2012; closes when the next beta runs the container job on the lane `userns-on`) | T-532 |
| #327 | [gh action] requested | closed (on `85ace56`: gate 44698962906, `level2.yml` 44698967314 and the dry run of `release.yml` 44698975325 green; `user-gate.yml` 44698971408: each lane passes but `userns-on`, where the rows of uv and ruff pass and the table said xfail) | T-564, T-562 |
| #328 | [gh action] requested | closed (on `dcefbfd`: gate 44708729668 and `level2.yml` 44708734686 green; `user-gate.yml` 44708738377: each lane passes, also `userns-on`, but `serve:job:KIND`, whose fakes were in a directory that others could write) | T-527, T-564, T-565 |
| #329 | COORD-114: the take of v0.79.0; pod-release with Rekor v2 for T-562; a runner job through serve with pod-fake-githome and pod-fake-github (to podcrates) | open (a report to the coordinator; closes when the run on a tag of `release.yml` verifies with `pod-release`) | T-562, T-509, T-527 |
| #330 | [gh action] requested | closed (on `3038770`: gate 44713826994 and `level2.yml` 44713833676 green; `runner-image.yml` 44713839288 red on a pull limit of public.ecr.aws, T-478; `user-gate.yml` 44722827108: `serve:job:KIND` failed with its cause hidden) | T-527, T-562, T-478 |
| #331 | [gh action] requested | closed (on `980c79b`: gate 44737569270, `level2.yml` 44737574300 and `runner-image.yml` 44737580022 green; `user-gate.yml` 44745685573 on `ab645c6`: the new checks pass, `serve:job:KIND` refused at the private address of the host, a fixed deny of `pod-broker`; a job that never ran was reported `success`, T-567) | T-527, T-566, T-478, T-567 |
| #332 | [gh action] requested | closed (on `6a84667`: gate 44788380238 and `level2.yml` 44788384919 green; `user-gate.yml` 44788390111: `serve:job` passes on incus, microvm and podvm; `serve:run:microvm` failed once, T-574) | T-527, T-566, T-567, T-568, T-574 |
| #333 | COORD-115: podvm logs streams the log and takes --limit-bytes N (to podvm) | open (podvm answered with COORD-220, PodUtils/podvm#630: PodUtils/podvm@T-2046 ships in `0.2.0-beta.37`) | T-570 |
| #334 | COORD-116: the memory of a microvm guest for an image of N unpacked bytes (to podvm) | open (podvm answered with COORD-219, PodUtils/podvm#629: memory = 3 × N + 512 MiB, from one measure; the curve is PodUtils/podvm@T-2047) | T-573 |
| #335 | [gh action] requested | closed (on `32e5ea4`: `level2.yml` 44802972349 and `user-gate.yml` 44802979062 green; the gate 44802962005 red on the word ceiling of `SPEC.md`, fixed in `f3f7c33`) | T-523, T-551, T-568, T-569, T-570, T-571, T-572, T-573, T-574 |
| #336 | [gh action] requested | open (gate, `level2.yml` and `user-gate.yml` on `19baf72`: the take of podcrates `v0.80.0`) | T-576 |
| #337 | COORD-117: the take of podcrates v0.80.0 (to podcrates) | open | T-576 |
| #65 | cfb asks: tracker for the 2 asks of cfb owned by podrunner (from podcrates#35) | open (a tracker; each ask maps to the entries of its line) | T-001, T-040, T-479, T-480, T-481, T-482, T-483, T-484, T-485, T-487, T-491, T-492, T-497, T-500, T-501, T-502, T-505, T-507, T-512, T-525, T-526, T-527, T-528 |
| #66 | githome asks: tracker for the 12 asks owned by podrunner (from podcrates#36) | open (a tracker; each ask maps to the entries of its line) | T-001, T-101, T-102, T-103, T-104, T-105, T-106, T-107, T-108, T-162, T-480, T-481, T-482, T-483, T-485, T-486, T-487, T-488, T-491, T-492, T-493, T-494, T-495, T-496, T-497, T-498, T-499, T-500, T-501, T-502, T-503, T-504, T-506, T-507, T-509, T-510, T-511, T-512, T-514, T-515, T-516, T-517, T-518, T-519, T-520, T-521, T-522, T-523, T-524, T-529 |
| #67 | CI plan items: tracker for the 26 items owned by podrunner (from podcrates#47) | open (a tracker; P0-4 is T-081, the others map to the entries of their lines) | T-001, T-081 |

#44, #45, #63, #297, #298 and #299 are pull requests (Dependabot: #63 base64 0.23, #297 trufflehog 3.99.9 and #298 actions/setup-dotnet 6.1.0 are applied on `main`; #299 rsa 0.10 is applied as T-531).
