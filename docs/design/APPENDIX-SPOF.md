# Appendix: single points of failure that #47 builds in

`SPEC.md` cites these rows as `Sn`.

Each row is an assumption written into #47 or the plan, the first thing that breaks when it is false, and where this spec removes it. "Inferred" applies to the failure column unless stated. The column "Recon" gives the id of R6 section 3 where one exists.

| Id | Hardcoded in #47 | First failure | Removed by | Recon |
| --- | --- | --- | --- | --- |
| S1 | One host process holds the mint credential, per-job CA keys, the Docker API parser and the slot manager ("thin launcher") | A bug in the parser of job-controlled input is a compromise of the mint credential and of every job's TLS | §5.2 process split on hosts with distinct uids, deferred to the first such host (§15). **Not removed on H0**: there the minting credentials leave the host (§5.3) | A13 |
| S2 | One job source: GitHub, repo-level `generate-jitconfig` with admin | One revoked or rate-limited credential stops every slot. Repo-level mint needs `administration: write`, which also covers branch protection and deploy keys (R5:8.5) | §6 source seam, org-level mint, scale set | A3 |
| S3 | Job discovery is unstated, and with no inbound port it can only be a poll | Latency and GitHub secondary rate limits. A job that stays queued more than 24 hours fails (Documented, R5, line 340) | §6.2 scale set | |
| S4 | Trust follows the label | A fork job that names the trusted label lands in the trusted unit (R5:8.4). The workflow author chooses `runs-on` | §6.3 tier computed at intake, compared with the set's tier | A4 |
| S5 | One runner implementation: the official .NET runner | Runs on x64, arm, arm64 glibc only. A 30-day update rule makes every old image stop taking jobs (Documented, R4:2.3) | §6.4 payload, §11.3 image age | A7 |
| S6 | `podvm run` as a child process is the one unit start path | A flag rename in a beta CLI breaks `podrunner` at run time. `podvm` has no library surface, only argv (R1:0 item 7) | §4.2 unit seam, the podvm adapter is one module with a version range and a conformance test | A2 |
| S7 | One container route: a per-job Docker socket | Every verb of every client must match. No verb census exists | §7 hooks first, census, then API | A8 |
| S8 | One egress path: job to `pod-broker` to one upstream CONNECT proxy, dialled by checked IP, in one process | On the podnet sandbox CONNECT to an IP literal resets (R2:5.1). One process sees every job's TLS | §9 plan per unit kind, name-forwarding, one listener for each unit | A5 |
| S9 | One hold mechanism is available on every host | T-1652 (SUD, XL) is open. `podnet pod` passes unix, vsock and netlink sockets and host loopback (Observed, R2:1.2) | §9.3 the hold is a capability | A6 |
| S10 | One architecture on the no-root path | aarch64 and every non-x86_64 host has no rung when chroot is denied | §10 matrix | A10 |
| S11 | One minting-credential holder, and it also edits runner groups | The org permission that mints also replaces a group's repository list (R5:8.4). A leaked key registers runners of its own | §8.1 signer, §8.5 leak response | A11 |
| S12 | The image cycle is a monthly rebuild by CI that may run on `podrunner` | A lapsed image stops the runner that would build the new image | §11.3 hosted bootstrap rule | A18 |
| S13 | Loopback `127.x.y.z` for each slot | podvm cannot make a payload bind a chosen address (R1:4.4). `podnet pod` does not apply rules to `127.0.0.0/8` | §7.3: `services:` refused on `shared-host`, port claims deferred (§15) | A12 |
| S14 | Reports, schemas and fakes each in a different owner's repository | githome and cfb pin one schema. Ruling Q-A8 puts schemas in podcrates | §11.4, WP-C3 | A19 |
| S15 | One live test account for acceptance | Ruling Q-A9: not now | §13 three acceptance levels | |
| S16 | One name and one repository for each tool; podcrates does not edit podvm, podssh or podnet | A podvm entry that slips (T-1655, T-1695, T-1696) blocks `podrunner` phase 1 with no owner on the dependency | §3.3 names the podvm entries as dependencies, with the workaround | A1 |
| S17 | One host class: rootless, no daemon, proxy-only | A `host` unit could take a fork job if a label says so | §6.3: `local` only by explicit config and never for a computed `untrusted` job | A14 |
| S18 | One GitHub flow and a host list that does not move | A host that the policy omits breaks cache and artifact steps. The flow changed in 2025 (plan risk 5) | §9.6 default host list, marked as a moving target | A17 |
| S19 | Two hosts polling one repository | They race for jobs. Placement by architecture rests on labels | One set for each tier, architecture and host (§6.2). Whether two hosts can share one set is Unverified (U17) | A21 |
| S20 | One identity and signing path | App auth is an RS256 JWT. The repository's identity code is Ed25519 | §8.1 signer role | A11 |
| S21 | One static `docker` CLI in the image | No static CLI for riscv64, i686, loongarch64. A static Go binary is refused on a chroot-denied host | §7 hooks | A9 |
| S22 | One cache layout for all trust levels | The userland rung does not enforce `:ro`. A poisoned write lands in a shared cache | §11.8: v1 has no shared cache (§15) | A20 |
| S23 | One cache and artifact backend: the Results service of github.com | A forge source has none, and the runner calls it for artifacts and cache | Assigned to githome with G-01 (`contracts/source-driver.md`) | R5 8.6 |
| S24 | One resolver and relay on the egress path: DoH by IP at four addresses, one default relay | A hardcoded third party on the job path (R2:5.1) | §9.4: the broker resolves through `pod-dns`, the relay is never on the job path | |
| S25 | One process and one uid domain for everything on a no-root host | A step reaches every control socket and every credential of its uid | §5.3 | |
