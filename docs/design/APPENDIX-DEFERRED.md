# Appendix: design kept for the deferred items

`SPEC.md` §15 lists what is cut from v1 and the trigger that reopens each item. This file keeps the design rules that are known now, so a reopened item does not start from nothing. Nothing here is built. Labels follow the SPEC.

## Port claims and the host-exclusive service lock

Port claims apply only where the capability `net` is `shared-host`. They are taken in `prepare_job`, when the hook learns the service ports.

- The key is `(protocol, port)`. All ports of one job are acquired atomically in sorted order, which excludes a deadlock between two jobs. A claim is released when the unit is `destroyed`.
- A port that another job holds, or that the probe sees listening, is `port-claimed`. The hook waits a bounded time, then fails the step by name. `status` says which port and which job holds it.
- A job that starts any service or binds any port on a `shared-host` unit runs with the host's service lock held exclusively: no other unit runs concurrently, and `status` shows `host-exclusive`. A claim stops two jobs from colliding, not from reading each other (Inferred, R2:1.2: held payloads reach host loopback TCP).
- A claim does not stop processes that are not `podrunner` units from using the port.

## `pod-dockerapi` and the census

1. **Census harness.** A script starts a fake Unix-socket daemon, runs each client through its container workflows and records the request lines. R4:6.4 already ran the static docker CLI 29.9.3 for the runner's verbs and produced the table of CLI call to HTTP request (`HEAD /_ping`, then `/v1.NN/` paths; `exec` is a `POST /exec/ID/start` with `Connection: Upgrade` and `Upgrade: tcp`; `docker port` crashes the CLI when the inspect answer has no `NetworkSettings`). Extend it to GitLab Runner's docker executor, the Gitea and Forgejo runners (R4:5) and testcontainers (startup: `GET /info`, `GET /version`, a Ryuk container unless `TESTCONTAINERS_RYUK_DISABLED=true`, a mountability check unless `TESTCONTAINERS_CHECKS_DISABLE` is set, R4:6.6). The output is one row for each verb and client.
2. **Minimal server.** Only the verbs of the census, with version negotiation (`HEAD /_ping` answers an `API-Version` header, paths accept a `/v1.NN/` prefix, the runner's gate needs 1.35 or newer). Every other verb answers a Docker-shaped error with a named reason. The first back end is the unit's container operations.
3. **Allow list, not deny list.** Accept only the `Config` and `HostConfig` fields of the census. Refuse any other non-default field with `container-option-refused` and the field name, in particular `Mounts`, `Volumes`, `VolumesFrom`, `CapAdd`, `SecurityOpt`, `UsernsMode`, `IpcMode`, `PidMode`, `CgroupnsMode`, `Tmpfs`, `Devices`, `DeviceCgroupRules`, `Runtime`, `Sysctls`, `Privileged`, `NetworkMode: host` and a `container:` or foreign-network mode, and `PortBindings` that name a host port the job does not hold or a `HostIp` other than loopback.
4. **Binds.** The official runner bind-mounts the literal `/var/run/docker.sock` into every job container and maps the work, tool and externals directories to `/__w`, `/__t`, `/__e`, the tool directory resolving to `/__w/_tool` when no tool cache variable is set, because the work mapping matches first (Source, `ContainerInfo.cs`, `HostContext.cs`, actions/runner at `310a370bf360`). The shim maps the socket bind, matched on that exact string, to the unit's own shim socket and refuses every other socket bind. Other runner binds are accepted only as the exact table, resolved at mount time with `openat2` and `RESOLVE_BENEATH`.
5. **Routes and limits.** Image pulls (`POST /images/create`, the `Image` field) go only to registries of the job's policy through the egress path. Routes outside the census answer `container-option-refused`. List, event and exec-inspect routes are filtered to the unit's own ids. A 1 MiB request head, a small streamed body, 16 connections and 32 containers for each unit.
6. **Socket path.** The shim's socket lives in the unit's private directory, never at `/var/run/docker.sock` or the default `DOCKER_HOST`: podvm's `system install-names` reads a socket that answers `/_ping` at those paths as a daemon and refuses the `docker` name (Source, R1:0 item 4). A conformance suite for the shim plants a request for each refused field (a `Mounts` entry for `/`, `CapAdd: SYS_ADMIN`) and passes a valid runner create.
7. **References** (R4:6.3): `dookoid` (Rust, MIT, no root, proot over ptrace, crates.io only, `build.rs` edits shell rc files unless `DOOKOID_NO_PATH_FIX=1`; interoperated with the static docker CLI for `run`, `exec`, `inspect`, `rm`, and `Aliases` came back null) and `kubedock` (Go, MIT, Kubernetes), the precedent for flat networking and copy-only volumes. Neither is a dependency. The fork's `RUNNER_CONTAINER_DAEMON_SOCKET` override (R5 F4) does not exist upstream.

## Shared caches, warm units, action mirror

- A cache key is (tier, repository owner, repository, ref class), where the ref class is `default` or `other`. A job reads its own key and the `default` key of its own repository and writes only its own. Only a trusted pipeline writes keys that others read, and units mount them read-only. An `untrusted` job reads nothing from `trusted`: at most a read-only, tier-owned snapshot of public content that a separate pipeline built.
- Caches mount only where the unit enforces `ro`. Otherwise they are copied in and discarded after the job. The userland rung of podvm lists `rw` only (Documented, R6 A20, `podvm:docs/limits.md:28`).
- A warm unit is a prepared root, never a started runner, because the credential is staged before the runner starts (§8.2). The pool holds up to `warm` prepared roots for each kind and pin and drops entries at the pin's age. M2 measured the cost for podvm: `podvm run --rm alpine true` took 4.8 to 5.4 s with an empty store and 0.26 s with the image in the store (Observed, one host). A prepared root for the runner image is not measured.
- Pre-fetched actions: the runner reads `ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE`, in two layouts (Source, `ActionManager.cs:1236-1286`): `<owner>_<repo>/<sha>.tar.gz`, copied; and with `ACTIONS_RUNNER_SYMLINK_CACHED_ACTIONS` true, `<owner>_<repo>/<sha>/<one nested dir>`, symlinked. The trusted pipeline would write it from commit SHAs and check hashes.

## Bridges for other forges

Gitea `act_runner` (MIT), the Forgejo runner (GPL-3.0-or-later README, mixed file headers: a separate process only) and GitLab Runner (custom executor scripts `config_exec`, `prepare_exec`, `run_exec`, `cleanup_exec` calling the unit driver) pick their own jobs from their own forge (Documented, R4:5). `podrunner` would run the upstream runner as a payload in a unit. There is no offer before the unit, so they implement none of `next`, `claim`, `withdraw` and `release`, and tiers for them are by host only. G-03 is stated for each bridge: the upstream runner daemon holds its own token for its life. It stays on the host and only the job's script enters the unit where the bridge allows that (GitLab custom executor), else the token is in the unit (Inferred, R4:5).

## Alternate payloads

`github-act-runner` (Go, MIT) builds for any Go target with `CGO_ENABLED=0` (Inferred from its workflow). Last commit 2026-03-21, a maintenance risk. Gaps: annotations, problem matchers, masking. It needs `node` and `bash` in the image and prints a JIT blob, so it is also a test partner. A static Go build is refused on a chroot-denied host (M2 `02-images.log`), so an alternate and the bridge runners need a dynamically linked build (Unverified that one runs on the loader family). A Rust native runner is the last option and needs the others to have failed on a target that an owner ask names.

## Hosted runner provider

The archive's airlock design (`ideas/podrunner-ideas/runner-trust/DESIGN-airlock.md`): a broker in a second organization holds the key, so no minting key is on the runner host. It is research only and never run (Documented, R5 F10, R5:8.4). It depends on a hosted runner in a second organization, is GitHub-only and does not serve a second source.

## Name-forwarding requirements (`SPEC.md` §9.4, WP-C1)

WP-C1: when the upstream is a CONNECT proxy that takes names, send the name, checked against the deny list and against a `pod-dns` resolve as a pre-check. That opens a rebinding window: the authoritative DNS can answer a public address to the pre-check and a private one to the proxy. So:
1. The upstream proxy must itself refuse non-public addresses (Documented for the measured class, R2:5.1). At start the broker tests it with a canary name that resolves to loopback in a zone the operator controls and refuses name-forwarding if the proxy connects.
2. Without that, name-forwarding runs with hold `advisory` and never serves `untrusted`.
3. Allow-list entries under attacker-registrable suffixes are flagged at load and listed in `status`.
4. A fixture test with a rebinding resolver (`podnet scripts/box/proxy.py`, `scripts/connect-proxy.py`, R2:5.2).
