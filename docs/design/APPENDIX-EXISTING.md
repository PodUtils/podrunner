# Appendix: what already exists that #47 proposes to build (long form)

`SPEC.md` §1 points here. This file holds each row: the existing piece, where it lives, the state, the gap, and the SPEC section that uses it. Rows whose existing piece the spec does not use say why. Labels follow the SPEC: Observed, Source, Documented, Inferred, Unverified. Pointers `R1:6` and so on use the key table of `SPEC.md`.

## A. Inside podvm and podcrates

| #47 item | Existing piece and place | State | Gap | Used in |
| --- | --- | --- | --- | --- |
| A token and a policy for each job | `Broker::load`, `Token`, `Policy`, TTL, `drop` in `pod-broker` (Source, R3:2.2) | T-099 done | TLS end (T-100), name upstream | §9 |
| Policy as a schema | `broker-policy.v1.schema.json` in the podcrates `schemas` directory (Observed). A copy is `inputs/broker-policy.v1.schema.json` | Exists | None | §4.2, `tools/check_schemas.py` |
| One log schema | `log.v1.schema.json` in the podcrates `schemas` directory (T-101) | Done | No report schema | §11.4 |
| Image pull | `pod_oci::pull::{resolve, fetch_parallel}`, `Client` (Source, R3:2.2) | Done | None | Inside the podvm driver |
| Unit root from layers | `pod-extract` (T-057), `pod-rootfs` (T-058). podvm took both at v0.11.0 (C5) | Done | Per-run root (C30) | §10.1 |
| Entering a root | `pod-enter`: chroot, namespace, userland, ladder, memfd, fuse (T-059, T-144) | Done | None | §10.1 |
| Host probing | `pod-hostprobe`: 66 probe rows, `select::{Rung, Selection}` (Source, R3:2.2). podvm `probe --json`, `probe --rows`, `system info` (Source, R1:6) | Done | Schema publication (A-V-10) | §10.1 |
| Refusal vocabulary for Docker verbs | podvm parity table as data: `system info --format '{{json .Parity}}'` (Source, R1:6) | Done | None | §15 |
| Per-job proxy and egress broker in podvm | podvm T-1665 (`--net broker`), T-1663 (proxy-forced programs through `podnet-netns`) (R1:6) | Open | One binary has no owner (R6 F9, DR-11) | §9 |
| Supervision, remove, destroy | `pod-supervise` `remove`, `pod-extract` `Place::remove` (Source, R3:2.2) | Exists | Tree kill, timeout, rusage (C19). No slot concept | §10.4 |
| HTTP client, DNS, client TLS | `pod-proxy`, `pod-dns`, `pod-tls` (Source, R3:2.2) | Exists | Certificate and CA minting (T-100) | §9.4 |
| `status --json`, `drain` | `pod-control`, T-103 (planned, `fleet.md:129`) | Planned | Not built | §11.5 |
| Exit codes and conventions | `pod-core` table 64 to 80 (T-101) | Done | podnet and podssh disagree with it (R2:2.3) | §11.5 |
| Fuzz targets | `fuzz/fuzz_targets/` with 15 targets including `broker_target` (Observed, R3:2.2) | Exists | One per new parser | §3.3 |
| Fake job source | podvm `podvm-measure` fixtures and `experiments/180-registry-fixture.sh`, a loopback zot registry (R1:6) | Exists | No fake GitHub (C9) | §13 |
| A microVM for a job | podvm T-1303 initramfs builder, T-1304 serial exec protocol, T-1301 probe, experiments 146, 147, 361 (R1:6) | Pieces | Wiring (T-1656), kernel pin, copy-back | §10.1 |
| CI matrix of restricted hosts | podvm's `gate.yml`, `nightly.yml` (7 targets, `sudo apt-get install qemu-user-static`), `secrets.yml` (R1:6) | Exists | `nightly.yml` cannot run on H0 | §13 |
| Dogfood corpus | 13 podcrates workflows on `ubuntu-24.04` (Source, R3:5.3) | Exists | None | §13 |
| A long-lived outbound agent | `podvm remote ssh serve` (relay dial out, `--once`) (R1:6) | Exists | Not a job runner | Not used |
| Single points inside podvm | One store lock and one `containers.json` for N slots; one `~/.docker/config.json` read by the job's image pull; registries other than docker.io answered 429 in podvm's own tests (`public.ecr.aws`); refusals are English text with no machine-readable error (Documented, R1:4.6) | Open | Slot-count limit on one host, a banner parser in the adapter | §4.2, §3.3 |
| `RLIMIT_FSIZE` inside podvm | A soft ceiling that also bounds guest memory (T-1305, T-1690) (Documented, R1:4.3) | Exists | A second place where `fsize` meets the .NET constraint | §10.4 |
| Rootless Docker names | `docker_daemon()` in `names.rs:84-130` reads `$DOCKER_HOST` or `/var/run/docker.sock` and treats a 200 on `/_ping` as a daemon (Source, R1:0 item 4) | Exists | Must not read a unit-private socket (a condition of the deferred shim, `SPEC.md` §15) | §15 |

## B. podnet, podssh and the hold

| Need | Existing piece and place | State | Gap | Used in |
| --- | --- | --- | --- | --- |
| Hold by seccomp | `podnet pod` (`podnet-netns`): redirects INET sockets, `-x socks5://unix:PATH`, `--serve unix:PATH`, `--default-action reject`, `--flow-log` (Documented, R2:1.2, R2:5.2) | Exists | Passes unix, vsock, netlink and host loopback. Never run with the .NET runner. `pod-hold` (podnet#414) | §9.1 |
| Unix-socket listeners and proxies | `podnet warp proxy --listen unix:PATH`, `podnet net proxy`, `podssh pipe http-listen:PATH` (CONNECT on a unix socket, 32 clients) (Documented, R2:5.2) | Exists | None | §9.5 |
| Socket protocol with `schema` first | `podssh session` wire (ops `info send wait capture stop`), `pod-broker` control (Source, R2:2.3, R3:1.1) | Exists | None | `contracts/` |
| Egress readiness probes | `podnet doctor --agent --json` (rows for git, pip, npm, cargo, docker pull; `github.com:443` only) (Observed, R2:5.1) | Exists | No Actions host | §11.5 |
| Remote command execution | `podssh run`, `job`, `each`, `mcp` (Documented, R2:3) | Exists | No CI queue | A-X-20 |
| What ends a background process | `podssh:docs/detach.md`: a table per CI system, read from the runner source (Documented, R2:3, not re-read) | Exists | Not verified here | §10.4 |
| Fixtures for a proxy-only host | `podnet scripts/test_in_box.sh`, `scripts/box/proxy.py`, `scripts/connect-proxy.py`, `scripts/restricted-host.sh`, `scripts/v6only/` (Source, R2:5.2) | Exists | Copy needed in podcrates | §9.4 |
| Third parties on the egress path | DoH by IP at 1.1.1.1, 1.0.0.1, 8.8.8.8, 8.8.4.4; one default relay on a Cloudflare Worker (64 MiB and 12 h a session) (Documented, R2:5.1) | Exists | Never on the job path | §9.4 |
| Relays and other roads | `podnet --transport relay`, `warp`, `podssh relay serve`, `--via https://HOST` (Documented, R2:5.2) | Exists | Not used by the job path | Not used |

## C. Upstream

| Need | Existing piece and place | State | Gap | Used in |
| --- | --- | --- | --- | --- |
| Job discovery with no inbound port | `actions/scaleset` (Go, MIT, Public Preview), the example scaler (Source, R4:3.1) | Exists | Live proof | §6.2 |
| Container replacement | `ACTIONS_RUNNER_CONTAINER_HOOKS`, the official docker and k8s hooks, ADR 1891 (Source, R4:2.6, M1 Q5) | Exists | A hook for podrunner. The job variable is GitHub's | §7.2 |
| Job start and end hooks | `ACTIONS_RUNNER_HOOK_JOB_STARTED`, `ACTIONS_RUNNER_HOOK_JOB_COMPLETED` (Documented, R4:2.6) | Exists | None | §11.4 |
| Action archive cache | `ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE`, `ACTIONS_RUNNER_SYMLINK_CACHED_ACTIONS` (Source, R4:2.7) | Exists | None | §15, `APPENDIX-DEFERRED.md` |
| Stale-runner exit | Exit code 7, `RunnerVersionDeprecated` (Source, `Constants.cs:162`; Observed, M1 Q4) | Exists | Only with the string `1` through `run.sh` | §11.3 |
| JIT blob writer | `github-act-runner --print-jitconfig` (Documented, R4:4.2) | Exists | None | §13 |
| Fake Actions service | `runner.server`: V1 flow, Results v4 Twirp artifact and cache services, Gitea webhooks (Source, R4:4.1) | Exists | Official-runner flow (V2) not exercised | §13 |
| Alternate payload | `github-act-runner` (Go, MIT; last commit 2026-03-21) (Documented, R4:4.2) | Exists | Annotations, problem matchers, masking | §6.4 |
| Gitea, Forgejo, Woodpecker, GitLab host backends | Gitea and Forgejo host label, Woodpecker `local`, GitLab `custom` and `ssh` executors; `fleeting` plugin interface for GitLab autoscaling (Documented, R4:5, R4:9.1) | Exists | podvm as a fleeting `InstanceGroup` is not designed. Drone exec is Polyform (R4 Surprise 9) | §6.1 |
| Rootless Engine API server | `dookoid` (Rust, MIT, ptrace), `kubedock` (Go, MIT, Kubernetes) (Documented, R4:6.3) | Prototypes | Neither is a dependency | §15 |

## D. The owner's archive

| Need | Existing piece and place | State | Gap | Used in |
| --- | --- | --- | --- | --- |
| Linux microVM launcher | Firecracker plus vsock bundle, Python, 60 checks, 19 planted defects (Documented, R5:5.1, R5:8.3) | Research | x86_64 only. Needs passwordless `sudo` and `setpriv`. No nf_tables, PCI or TUN in its kernel. Runs a shell script as guest root: no `uses:`, artifacts, cache, `container:` or `services:`. The privacy claim is against log scrapers | §10.1 |
| Conformance contract | `ghactions-local`: `conformance.sh`, `check.sh`, seven workflows (Documented, R5:5.1) | Research | `conformance.sh:64` runs `docker rm -f $(docker ps -aq)`. w6 and w7 fail | §13 |
| Pin gate | `testdata/pins.sh`: LF-normalised sha256 of verbatim copies, never fetches (Documented, R5:5.1) | Research | None | §11.3 |
| Broker address checks | `is_public`, `resolve_checked`, `upstream_connect` with 11 test cases. R5 F8 lists four more addresses that pass and must be added (Documented, R5:5.1) | Research | None | §9.4 |
| Peer checks, fail-closed host checks | `SO_PEERCRED` on local sockets; `/dev/kvm` read and write, swap off, `ptrace_scope` (Documented, R5:5.1) | Research | `sudo -n` dependency | §6.6 |
| Exit-code contract | 64, 70, 75, 78 (Documented, R5:5.1) | Research | Differs from `pod-core` | §11.5 |
| Mutation-test method | `proto/mutants.py`, 19 planted defects all caught (Documented, R5:5.1) | Research | None | §13, `tools/selftest.sh` |
| Airlock design | `runner-trust/DESIGN-airlock.md` (Documented, R5 F10) | Research | Never run. Needs a hosted runner in a second organization. GitHub-only | §15 |

## E. Other repositories

| Repository | State | Relation |
| --- | --- | --- |
| `PodUtils/podci` ("Run CICD Actions on Pods") | LICENSE only (Observed, R3:0) | Overlaps `podrunner`. DR-12 |
| `PodUtils/podkey` | LICENSE only | The in-memory secret agent of A-X-08 would replace `sign` (§8.1) |
| `PodUtils/podnode` | LICENSE only | A-X-02. Out (§12) |
