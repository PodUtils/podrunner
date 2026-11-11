# R1: podvm recon for podcrates#47 (podrunner, pod-dockerapi, the podvm path)

Machine date at start: 2026-11-10 (`date`). Scope: `repos/podvm` (HEAD 36b15ec, committed 2026-11-10), the podvm
issue list from the forge mirror (184 issues, pages 1 and 2), and the consumers of podvm in `repos/podcrates`
where they decide a claim (`pod-supervise`, `pod-guest`, `pod-hostprobe`, TODO T-062).

Labels: Observed (artefact read or command output), Source (code says it, path:line), Documented (a doc says it),
Inferred, Unverified. Nothing was built or run: there is no podvm binary on disk, and a build needs zig and
git dependencies. Every behaviour claim below is Source or Documented, not a run. Paths are `podvm:` (repos/podvm)
or `podcrates:`.

Inputs note: `inputs/i47.md` is not on disk and `i47-comments.md` is empty. The issue body was read from the
forge mirror (podcrates issue 47, open, 0 comments, 9158 bytes) and the plan from `inputs/podrunner-plan.md`.
Where "the issue" appears below it means that body; "the plan" means the plan file.

## 0. The 10 findings that matter most (short form; detail in the sections)

1. Machine tier from `run` boots nothing for Linux. `run --podvm-tier=machine IMAGE` always exits 125
   (podvm:crates/podvm/src/tier.rs:137-160). The only guests podvm boots today are Windows and DOS disk
   images (`windows run`, or `run --platform windows/amd64`) and a `machine ssh` guest from a caller-supplied
   kernel and initramfs. The issue's "pod-guest and the machine tier give a microVM" is false today. Source.
2. All containers of one image share one mutable rootfs: `<store>/rootfs/<manifest digest>/`, no upper layer, the
   completion layer rewrites `/etc/hosts`, `resolv.conf` and passwd in it on every run. "A fresh unit for each
   job" does not hold without `--rm` (which deletes and re-extracts), and two concurrent jobs from one image
   collide on files and on `/etc/hosts` names. Source (podvm:crates/podvm-extract/src/lib.rs:46,
   podvm:crates/podvm/src/run/prepare.rs:156,395, podvm:TODO/image.md T-1322).
3. "No daemon" is not a ruling in `docs/decisions.md`. It is a reason string in 8 parity rows, a doc comment, a
   citation of `TOOL.md` section 2.0 (an external repository, not on disk), and a ruling of 2026-09-08 in
   `names.rs` that says a machine with a working docker daemon is one where podvm is the wrong tool. The only
   ruling that touches podrunner is 2026-11-09 "New projects": "podvm does not grow those projects". Q3 of the
   issue asks about a rule that is weaker and located differently than the issue says. Observed.
4. `podvm system install-names` refuses to install the `docker` name where a socket at `$DOCKER_HOST` or
   `/var/run/docker.sock` answers `/_ping` (podvm:crates/podvm/src/names.rs:84-130). A `pod-dockerapi` socket
   in a unit would read as "a working daemon" to podvm's own check. Source.
5. The issue's podvm status line is stale in two places. `-v` is Degraded (loader family only), not None
   (podvm:crates/podvm/src/parity.rs:222). A-V-04 is mostly done: T-1643 to T-1650 and T-1697 are `done`
   (podvm:TODO/INDEX.md:319-326,373). Verified refuted.
6. podvm takes `pod-extract`, `pod-rootfs`, `pod-oci`, `pod-enter`, `pod-supervise`, `pod-guest`, `pod-hostprobe`
   at tag v0.11.0 today (podvm:Cargo.lock:407-520; COORD-019, podvm issue 225). The plan table's "pod-extract and
   pod-rootfs wait for the take" is stale. Refuted.
7. The one orchestration layer for `run` (decide rung, prepare, completion, interposer env, record writes) is in
   the `podvm` bin crate, 5830 lines in `lifecycle/`, `run/`, `ladder.rs`, `interpose.rs`. It has no `[lib]`
   (podvm:crates/podvm/Cargo.toml). A podcrates consumer can reuse the `pod-*` crates, not podvm's glue. The only
   stable podvm "API" is its argv. Source.
8. No health model, no label store, no JSON run report, no event stream, no network model, no HTTP/API/socket
   server exist in podvm. What exists: a per-container detached launcher with a Unix control socket that speaks
   two verbs (`SIG`, `WAIT`), a flat PascalCase string-valued `inspect` document, and `probe --json` and
   `system info --format '{{json .Parity}}'`. Source.
9. Tiers differ sharply. On a chroot-denied host (the target class) the only `-v`, `exec`, `run -d` path is the
   userland loader family on x86_64 only. A static payload is refused at 125 unless `--unsafe-host-paths`; the
   memfd family has no detached driver and no `-v`. Every other architecture has no no-chroot rung at all
   (interposer pair is x86_64 only). Source and Documented.
10. `exec` does not inherit the container's environment or working directory (env base empty, workdir default
    `/`), shares host pids and network, has no `-d`, and `logs` is one interleaved file with no stdout/stderr
    split. A Docker API `exec` and `logs` (stream multiplexing) cannot be faithful on top of it without new
    podvm work. Source (podvm:crates/podvm/src/exec/enter.rs:46-52,136; parity.rs:153).

## 1. Question (a): what podvm does today

### 1.1 Parity table: shape and counts

- The statuses are `Native`, `Degraded`, `Stub`, `None`, not "Full" (podvm:crates/podvm/src/parity.rs:42-52). Source.
- 264 rows: Native 166, Degraded 18, Stub 10, None 70 (counted with grep on the `Row {` lines of
  podvm:crates/podvm/src/parity.rs, sum 264). 75 are verb rows (`flag: None`). Observed (command output).
- A flag with no row is refused by the parser (podvm:crates/podvm/src/parity.rs:6-12). So `--report`, `--timeout`,
  `--secret` are refused today because they have no row. Source.
- The table is machine readable: `podvm system info --format '{{json .Parity}}'`
  (podvm:crates/podvm/src/system.rs:5-10). Source.
- Row citations in TODO entries drift. T-1655 cites `parity.rs:370` for `--read-only`, now line 257.
  T-1695 cites `parity.rs:254` for `--label`, which is right. The gate checks that a cited line exists, not
  what is on it (Documented, podvm:TODO/INDEX.md:15-17). Observed.

### 1.2 The verbs the issue's workflow needs

"Today" is the source at HEAD. Each row cites the parity line.

| Verb | Status | What it does (Source) |
| --- | --- | --- |
| `run` | Degraded (121) | Enters a chroot, userland loader or memfd; never a namespace. Foreground run leaves a record under `--name` (T-1421). `-d` starts a detached launcher and prints the id. |
| `create` / `start` | Native (145, 146) | `create` writes a `created` record and starts nothing; `create` is served by run's parser. `start` returns when the payload has reached execve (pipe, not sleep). |
| `exec` | Degraded (122) | Fresh re-entry over the stored rootfs: shares the filesystem only. Env base is empty, workdir default `/`, no `-d`, `-i` is a no-op. It works on a created or exited container too (warns only when `Created`). |
| `ps` | Degraded (152) | Reads the container table. `-f name=` is a substring match; `label=` matches nothing. `--format` is `{{.Field}}` only. |
| `logs` | Degraded (153) | One file, stdout and stderr interleaved. `-f` and `--tail` only. No `-t`, `--since`, no stream split. |
| `stop` | Degraded (147) | SIGTERM then SIGKILL to the payload only. Grandchildren that reparent are not signalled (no PID namespace). `-t 0` is SIGKILL at once (call C12). |
| `kill` | Degraded | Signals the payload; pidfd addresses one process. |
| `rm` | Native (151) | Removes the record and the container's directory. `-f` kills first. `-v` is a Stub. The shared rootfs stays. |
| `wait` | Degraded (154) | Blocks on the launcher, bounded. A container podvm did not see end has no exit code and `wait` refuses. |
| `inspect` | Degraded (129) | One flat document for an image or container. See 1.4. |
| `cp` | Degraded (155) | One file or a tree with `-r`; a container or image on either side. |
| `commit` | None (168) | "podvm cannot restore ownership, so a committed layer would misrepresent what it holds". |
| `build` | None (167) | "podvm can run one but cannot commit one". |
| `push` | None | Reads registries, never writes. |
| `network` | None (188) | "the payload shares this machine's network namespace, so there is nothing to create or attach". |
| `volume` | None (189) | "a volume would be a copy pretending to be a mount". |
| `compose` | None (190) | "it needs the lifecycle and a network". |
| `events` | None (186) | "there is no daemon to emit events". |
| `top`, `attach`, `pause`, `stats`, `diff`, `port`, `rename`, `update`, `history`, `search` | None | See parity.rs:157-186. |
| `secret`, `config`, `context`, `plugin`, `service`, `stack`, `node`, `swarm` | None | "no daemon" or "out of the shape" reasons (parity.rs:193-205). |
| `pull`, `images`, `rmi`, `tag`, `save`, `load`, `import`, `login`, `logout`, `verify`, `extract`, `prune`, `system` (`info`, `df`, `abi`, `install-names`) | Native, `system`/`info` Degraded | Image side is real. `pull` is HTTPS only; a registry offering only http is refused. |
| `docker`, `podman` (argv0 names) | Degraded (parity.rs, last two rows) | `podvm system install-names` makes symlinks. It refuses the `docker` name where a daemon answers unless `--force`. |
| `windows`, `machine ssh`, `remote ssh` | Native | podvm's own verbs, not docker verbs. See 2.4. |

### 1.3 The run flags the issue's workflow needs

| Flag | Status (line) | Truth |
| --- | --- | --- |
| `-v, --volume` | Degraded (222) | `HOST:GUEST[:rw]`, a path rewrite in `PODVM_MAPS` on the userland loader family. Not a mount; static programs and raw `syscall` instructions see the image path; `/proc/mounts` does not list it. Refused by name: chroot rung, memfd family, forced `PODVM_MODE`, machine tier, `:ro`, named or anonymous volumes, `-w` inside a volume, a first program inside a volume. |
| `-p, --publish` | None (223) | The payload shares the host network namespace. |
| `--network` | None (224) | No network namespace. |
| `--link`, `--hostname`, `--dns*`, `--mac-address`, `--domainname` | None (231, 267-270) | Same reason. |
| `--add-host` | Native (235) | `name:ip` appended to `/etc/hosts` the completion layer writes. Only mechanism for service names. |
| `--label` | Stub (254) | Accepted and dropped (podvm:crates/podvm/src/run/parse.rs:222). Records carry no labels; `--strict` refuses. |
| `--health-cmd` | None (278) | "there is no daemon to run health checks". |
| `--ulimit` | None (272) | "per-container rlimits are not applied". |
| `-m/--memory`, `--cpus`, `--pids-limit` | None (228, 229, 271) | Needs a cgroup. |
| `--read-only`, `--tmpfs`, `--mount`, `--shm-size` | None (257-259, 266) | No mount(2). |
| `--cap-add/--cap-drop`, `--security-opt`, `--privileged`, `--userns`, `--pid`, `--init` | None | Not applied; no `PR_SET_NO_NEW_PRIVS`, no seccomp applied by `run` (grep of podvm:crates/podvm/src and podvm-interpose found none). |
| `-u, --user` | Degraded (225) | Identity faked through the interposer memo; reachable payloads only. |
| `-e, --env-file` | Native (210, 253) | Values go into the payload environment and into the container record `env` (see 1.6). |
| `--restart`, `--stop-signal`, `--stop-timeout` | None (230, 283, 284) | |
| `--log-driver` | Stub (281) | |
| `-d` | Native (220) | Needs the loader family on a chroot-denied host. |
| `-t` | Degraded (217) | Refused by name where `/dev/ptmx` is unusable. |
| `--report`, `--timeout`, `--secret` | no row | Refused by the parser. Filed as T-1695, T-1655, T-1696, all open. |

### 1.4 Status of every entry the issue and plan cite

All statuses are from podvm:TODO/INDEX.md (the derived list; line numbers are INDEX lines) and the entry bodies.
The PROGRESS header (311 entries: 26 open, 10 partial, 4 blocked, 271 done) is the same snapshot (Observed).

| Id | Claim in issue or plan | True state | Verdict |
| --- | --- | --- | --- |
| A-V-04 | "open (measured failures on beta.12)" | Maps to T-1643..T-1649, T-1652. T-1643, 1644, 1645, 1646, 1647, 1648, 1649, 1650 `done` (INDEX 319-326). T-1651 and T-1652 `open`. T-1697 `done` (INDEX 373). T-1646 `Done 2026-11-10`: node 22.23.4 on node:22-alpine (musl), `npm ci` and `npm test` exit 0 as uid 65534 (podvm:TODO/interpose.md:2286, 2373). | Stale: plan treats it as open. Not proven for a glibc Node image or .NET (see 3.3). |
| T-1643 | relative path double prefix | `done` | Verified done. |
| T-1644 | glibc NEEDED closure | `done`; `gcc` image compiles and runs a hello program at 0 (interpose.md:2280-2286) | Verified done. |
| T-1645 | dlopen/dlmopen | `done` | Verified done. |
| T-1646 | dynamic grandchild via image loader | `done` 2026-11-10 | Verified done. |
| T-1647 | memfd 128 MiB ceiling | `done`; COORD-016 and COORD-019 record a 161523528 byte node run at 0 (podvm issue 225) | Verified done. Full experiment 356 on a live registry was blocked by HTTP 429 at that time. |
| T-1648 | static Go payload refusal | `done`, P0. Static payload on a chroot-denied host refused at 125 unless `--unsafe-host-paths` (limits.md) | Verified. |
| T-1649 | `#!` entrypoint refusal names the way out | `done` | Verified. |
| T-1652 | SUD native-static rung | `open` (INDEX 328). A program that issues `syscall` itself passes the interposer (limits.md) | Verified open. |
| T-1697 | raw statx of libuv | `done` 2026-11-10. `syscall()` exported; 51 path numbers routed, 25 answer ENOSYS, `io_uring_setup` ENOSYS (interpose.md:2373). Plan text "passes the interposer, T-1697" is stale. | Stale. |
| A-V-06 | `-v` and `:ro` | T-1653 `partial` (INDEX 329). `-v HOST:GUEST[:rw]` works on the loader family. `:ro` waits for T-1655 (`open`). `-w` and first program in a volume wait for COORD-018 (podvm issue 224, open) and the podcrates tag v0.12.0 (inputs/i48.md). | Plan says "Open"; true state is partial. Plan table row "-v ... `None`" is refuted. |
| A-V-07 | limits, kill tree, RSS, CPU | T-1655 `open` (INDEX 331). No `--ulimit`, no `--timeout`, no subreaper, no `wait4` rusage anywhere (grep of podvm and podcrates `crates/` for SUBREAPER, ru_maxrss, rusage: no hits). | Verified open. |
| A-V-09 | JSON report and labels | T-1695 `open`, P1 (INDEX 371). No row for `--report`. | Verified open. |
| A-V-11 | `--secret` | T-1696 `open`, P2 (INDEX 372). Plan P2 cites "T-1667-area entry". T-1667 is "Doctor and installers" (INDEX 343). | Refuted citation. The entry is T-1696. |
| A-V-08 | microVM from OCI image through `run` | T-1656 `open` (INDEX 332), T-1682 `open` (INDEX 358). `run --podvm-tier=machine IMAGE` refuses at 125 (tier.rs:138-141). | See finding 1. |
| T-1655 | hardening line | `open`, effort L | Verified. |
| T-1656 | microVM | `open`, effort XL. Priority P2 in podvm; A-V-08 asks P1 | Verified. |
| T-1682 | kvm.yml workflow | `open`. Premise "GitHub hosted runners expose /dev/kvm ... not measured here". | Verified; note it is a workflow that needs a runner, so it is itself a CI consumer. |
| T-062 | "microVM from an OCI image planned (T-062)" | T-062 is a podcrates entry, a P3 research study, `open`, milestone M7, start condition "the crates of M6 are released" (podcrates:TODO/podvm.md:840-875). It studies `pod-microvm`, `pod-sud`, `pod-confine`, `pod-netlab`, `pod-pcap`, `pod-sandprobe`. It is not an implementation. | Partly verified: it exists, it is a study. |
| T-1653 | volumes | `partial` (see A-V-06) | Verified. |
| T-1667 | the plan's `--secret` home | doctor entry | Refuted. |

Also found: `docs/HANDOFF.md` says `podvm-enter` and `podvm-windows` are frozen and lists the M6 imports as to do; the
tree and issue 225 say all seven are taken. HANDOFF is stale (Observed; podvm:docs/HANDOFF.md:30-35 against
podvm:Cargo.lock).

### 1.5 What podvm really does for each lifecycle verb (execution model)

- `run` foreground: probe, decide rung (`chroot`, `namespace`, `userland` loader, `userland` memfd), prepare,
  completion layer, interposer env, enter, forward SIGINT and SIGTERM, return payload exit code. Docker's 125,
  126, 127 exit codes are kept. Source.
- `run -d` and `start`: one detached launcher process per container, double-forked, holds the container lock, a
  pidfd, a Unix control socket, writes `running` and `exited` into the table. Control protocol: `SIG` and `WAIT`,
  one line each way (podcrates:crates/pod-supervise/src/launcher.rs:32-34; launcher doc at 1-20). Source.
- `exec`: a fresh process, not a join. See 1.3 and finding 10.
- State is files: `<store>/containers.json` (one table for all containers, replaced by rename, written under the
  store lock that the image index also takes, invariant I1) plus a directory per container (log, lock, control
  socket, ownership memo) (podvm:crates/podvm-supervise/src/table.rs:12-15,130-165). Source.
- `rmi`, `prune` refuse an image any container record references (T-1322, `done`).
- Open lock issues that matter to concurrency: T-0204 (store GC cannot delete a running rootfs) `partial`; T-0211
  (image lock outlives a forking holder) `partial` (podvm:TODO/PROGRESS.md:127-135; INDEX). Documented.

### 1.6 Records, labels and secrets

- A container record holds `id, name, argv, env, working_dir, created_at, started_at, finished_at, state, pid,
  launcher_pid, exit_code, noticed` plus podvm's `image, manifest_digest, rootfs, rung, userland_exec,
  completion, completion_degraded` (podcrates:crates/pod-supervise/src/table.rs:66-88; podvm:crates/podvm-supervise/src/table.rs:25-52).
  No label field. No mount list; volume pairs ride in `env` as `PODVM_MAPS` text and are re-read for `exec`
  (podvm:crates/podvm/src/exec/enter.rs:88-95). Source.
- `env` is stored in `containers.json`. A credential given with `-e` or `--env-file` is therefore at rest in
  the table, in a file created with default modes (no `mode(0` in the table writer, grep of
  podcrates:crates/pod-supervise/src/table.rs). Inferred from source; not run.
- `inspect` document: an array of one object, PascalCase keys, every value a string, with `Exec.Mode`,
  `Exec.Shares`, `Contains`, `Complete.Fixups`, `Interpose.Emulated.*` and a dash for unknown
  (podvm:crates/podvm/src/lifecycle/copy_tree.rs:270-345; entry.rs:1-20). There is no `State.Running`,
  `State.Health`, `Config.Labels`, `Config.Env`, `NetworkSettings`, `Mounts`, `HostConfig`. A
  `docker inspect --format '{{.State.Health.Status}}'` has no equivalent. Source.
- `--format` is `{{.Field}}` and literal text; pipelines, functions, `table`, `json .` are refused by name
  (podvm:crates/podvm/src/format.rs:1-12,47-55). Only `info` has `json .Field`. Source.
- Image labels: no code reads OCI config labels; `images --filter label=` matches nothing (parity.rs:322;
  T-1695 Premise). Source.

## 2. Question (b): what could a Docker Engine API subset sit on

### 2.1 Present building blocks (real)

| Block | Where | State | What a Docker API would use it for |
| --- | --- | --- | --- |
| Per-container launcher with control socket | `pod-supervise` (podcrates), podvm settings in `podvm-supervise` | Real. One process per detached container; `SIG` and `WAIT` only. | `kill`, `wait`. No `logs`, `exec`, `stats` verbs on the socket. |
| Container table with lock and reconcile | `pod-supervise::table` | Real. Single `containers.json`. | `containers/json`, create, delete. Needs labels and richer fields. |
| Image store and pull | `podvm-image` over `pod-oci`, `pod-proxy`, `pod-tls` | Real, Native. HTTPS only; `save`/`load` tar; `import` plain rootfs tar. | `images/create`, `images/json`, `images/{name}/json`. |
| Probe JSON | `probe --json` | Real. Schema is podvm's own; published schema is ask A-V-10 (T-1670 `done` is the first cloud-session measurement; no published schema file was found in the tree). | Host qualification. |
| Parity table as data | `system info --format '{{json .Parity}}'` | Real. | The "refuse by name" vocabulary the plan wants in P6. |
| Docker name multicall | `podvm system install-names`, `names.rs` | Real. | A `docker` CLI in the unit that is podvm itself (see 2.5). |
| Relay and SSH transport | `podvm-ssh`: `remote ssh serve|connect|forward`, relay legs over WebSocket, mux `reverse-v1` | Real, outbound dial only; `serve` registers a pair name; `--once`. | A long-running outbound process exists in podvm. It is an SSH tunnel, not an API. |
| Windows or DOS guest driver | `pod-guest`, `podvm-windows`, `podvm windows run` | Real for Windows and DOS disk images: overlay, FAT16 mailbox, cmd.exe agent, QMP, bounded stop. | Not a Linux microVM. |
| Linux guest with serial exec protocol | experiments 146, 147 (shell), T-1303, T-1304 `done` | Experiment scripts, not product code. Serial pair, per-command 128-bit nonce, bounded waits. | Spec of a guest exec channel; not wired into `run` or `exec`. |
| QEMU user-mode networking | experiments/361-guest-usernet.sh (shell) | Measured in a script: UDP datagram both ways, TCG. | Evidence only. |
| Linux guest SSH | `podvm machine ssh --kernel K --initramfs I --user-key KEY` | Real. One session per boot, over a serial unix socket, real ssh client. | One command per boot. No network in the guest. |

### 2.2 Absent (verified absent)

| Item | Evidence |
| --- | --- |
| Long-running podvm mode, HTTP server, REST or Docker API | No listener in `crates/podvm/src` (grep for `UnixListener`/`TcpListener` hits only `names.rs` client and the launcher control socket in pod-supervise). `events` None (parity.rs:186). `context` None. Source. |
| Event stream | `events` None. No bus in the table. Source. |
| JSON run report | T-1695 open. The only JSON writers are `probe --json`, `inspect`, `system info`. Source. |
| Label store | `--label` Stub; no field in the record (1.6). Source. |
| Health model | No HEALTHCHECK parse in `crates/` (grep "HEALTHCHECK|Healthcheck" found no hit); `--health-cmd` None (parity.rs:278); `podvm-image/src/health.rs` is store integrity (T-1321), not container health. Source. |
| Network model | `network` None, `--network` None, `-p` None, `port` None. Rootfs tiers share the host network. Name resolution is `/etc/hosts` lines only. Source. |
| Machine tier running a Linux OCI image | tier.rs:137-170: every branch of `enter_machine` returns 125. T-1656 open. Source. |
| Guest agent for Linux | No Linux agent in the product. The `cmd.exe` agent of `pod-guest` is Windows only. T-1304 is a shell protocol. Source. |
| Per-run resource report (CPU, RSS) | None. Source. |
| Subreaper, tree kill | None. `stop` signals the payload only. Source. |

### 2.3 How the tiers differ (what `exec`, `network`, `-v`, ports, services can do)

Rung names are `namespace`, `supervise`, `chroot`, `userland`, `interpose`, `unsupported`
(podcrates:crates/pod-hostprobe/src/select.rs:34-44). "Chroot-denied host" is the class of podvm#114 (no
chroot, no user namespace, no mount, no ptrace, no `/dev/fuse`, no `/dev/kvm`, egress only by an HTTP proxy,
uid 966, kernel 7.2.2). Source and Documented.

| Capability | chroot or namespace rung | userland loader family (chroot-denied) | userland memfd family (static payload, `--unsafe-host-paths`) | machine tier |
| --- | --- | --- | --- | --- |
| Isolation | Filesystem path change only. Namespace rung adds a mount namespace and private tmpfs only. No pid, user or network isolation (limits.md, architecture.md). | Host root retained. Interposer is not a boundary. Raw `syscall` instruction passes it (T-1652 open). | Host root. | Emulator process under TCG; hardware under KVM. Not usable for Linux from `run`. |
| `run -d` / `start` | Yes | Yes (T-1411, `done`) | No detached driver (podvm:crates/podvm/src/lifecycle/userland.rs:214) | Refused for Windows (`should_drive` needs not detach) and for Linux (125) |
| `exec` | Fresh re-entry, shares filesystem only | Fresh re-entry; works on a record with the container's map table | Same, under the family rules | Refused 125 |
| `-v HOST:GUEST` | Refused by name | Path rewrite via `PODVM_MAPS`, `:rw` only | Refused | Refused by name (prepare.rs:43-52) |
| `-v ...:ro` | Refused | Refused until T-1655 | Refused | Refused |
| `-w` or first program in a volume | Refused | Refused until podcrates v0.12.0 adoption (COORD-018 open) | Refused | n/a |
| Ports (`-p`) | None | None | None | Not wired. QEMU user-mode forwarding exists only in an experiment script. |
| Service name resolution | `--add-host name:ip` into the shared rootfs `/etc/hosts` | same | same | n/a |
| Network between containers | All share the host network | All share the host network | same | Windows guest: `-nic none`, no network (podcrates:crates/pod-guest/src/plan.rs:142-143; podvm:docs/limits.md). A caller `--podvm-qemu-arg -nic user` lands last in the argv (pod-guest plan.rs:301-310). |
| Architectures | Any arch that builds podvm (7 musl targets in podvm:.github/workflows/nightly.yml:34-52) | x86_64 only: the interposer pair is x86_64 only (T-1327 `done`, podvm:TODO/interpose.md, "the pair stays x86_64-only"; limits.md) | x86_64 | KVM or TCG x86_64 in tests; foreign arch via binfmt qemu-user (T-1651 open) |
| Resource limits | None | None | None | Guest memory `--podvm-mem`, judged against `RLIMIT_FSIZE`; no `--cpus` |

Conclusion for the issue: a `services:` job needs a shared name table and per-container network identity.
Nothing in podvm gives either on any tier today. Inferred from the table above.

### 2.4 Machine tier facts (the issue relies on "pod-guest and the machine tier give a microVM")

- `podvm run --podvm-tier=machine IMAGE cmd` is not wired. Quote (podvm:crates/podvm/src/tier.rs:138-141):
  "the machine tier holds on this machine; running a guest arrives with TODO/machine.md T-1303 and T-1304".
  Same text pattern for TCG (tier.rs:144-157). The doc comment says the same (tier.rs:95-101). Source.
- T-1656 Premise states it: "`podvm run --podvm-tier=machine` probes the tier and then refuses: running a guest
  from `run` is not wired" (podvm:TODO/machine.md:1226-1230). Documented.
- What does boot: `windows run` and `run --platform windows/amd64 DISK cmd` (the positional is a disk image; the
  result is the guest's own stdout, stderr and exit code; podvm:crates/podvm/src/run/prepare.rs:59-79), and
  `machine ssh` for a kernel plus initramfs the caller supplies. `README.md` line "podvm run --rm
  --podvm-tier=machine --platform windows/amd64 win.podvm.qcow2 cmd /c ver" is therefore right for Windows only.
- The Windows guest has no network, and "fresh 2026-09-30 KVM runs failed at setup or command shutdown"
  (podvm:docs/limits.md, Guests). T-1350 (`run ver` hangs after firmware handoff under watchdog) and T-1641
  are still partial or open (PROGRESS items 9-11). Documented.
- The Linux guest exec protocol (T-1303, T-1304) is proven by experiment scripts, with the kernel measured in
  an Alpine netboot pin (6.12.110), not by `podvm` code. T-1702 (done 2026-11-10) records that a TCG guest
  printed no console byte until `poweroff -f` (halts on `-M pc,acpi=off`) was replaced by `reboot -f`.
  So: machine-tier guests under TCG were measured to boot in tens of seconds on the cloud host, 61 s for the whole
  `podvm-workload` run (podvm:TODO/machine.md:1360-1380). Documented, not re-run.
- KVM: the owner lifted the KVM deferral on 2026-11-09 (podvm:docs/decisions.md). The cloud host of 2026-11-09
  has no `/dev/kvm` (PROGRESS item 9). The #114 target host has no `/dev/kvm`. The issue's "KVM is free"
  means a session may start a KVM guest where `/dev/kvm` exists; it does not mean the target hosts have one.
  Documented.
- The microVM kernel is not pinned for `run`. T-1656 Approach: "Pin a small virtio-mmio kernel by digest" is
  future work. Booting reliability of microVM flags is an open measurement (F15 in issue 168: 5 of 6 boots
  crashed with `rtc=on` and 24 of 24 booted with `pit=off,pic=off,tsc=reliable` on one host).

## 3. Question (c): facts in the issue or plan that are wrong or stale

Verified against artefacts. "R" = refuted, "S" = stale, "V" = verified, "U" = unverifiable from this repo.

| # | Claim (source) | Verdict | Evidence |
| --- | --- | --- | --- |
| 1 | Issue: "podvm marks `network`, `volume`, `-v` and `--network` as not supported today". Plan section 3: "`network`, `volume`, `-v`, `--network`, `-p` are `None`". | `-v` R, rest V | `-v` is Degraded (parity.rs:222). `network` (188), `volume` (189), `--network` (224), `-p` (223) are None. |
| 2 | Plan: "`exec` is `Degraded` (re-entry, not a running container)". | V | parity.rs:122. Plus it ignores the container env and workdir (finding 10). |
| 3 | Plan: "`--label` is a stub". | V | parity.rs:254. |
| 4 | Plan: "pod-extract and pod-rootfs wait for the take". | R | Cargo.lock:427-433 and 491-497 at tag v0.11.0; podvm issue 225. |
| 5 | Issue: "`pod-guest` and the machine tier of podvm give a microVM for an untrusted job". | R for Linux | Finding 1, section 2.4. `pod-guest` is the Windows/DOS driver (podvm:crates/podvm-windows/src/lib.rs:1-12). |
| 6 | Plan: "A-V-04: ... open (measured failures on beta.12; raw statx of libuv passes the interposer, T-1697)". | S | T-1697 `done` (2026-11-10). |
| 7 | Plan P2: "podvm T-1653" as dependency of `--secret` and "Filed as podvm T-1667-area entry (cli.md)". | R for T-1667 | The entry is T-1696. T-1653 is the path map it needs (V). |
| 8 | Plan P2 table: "`experiments/427-secret-flag.sh`; tests `secret_*` (already written in the entry)". | U/V | T-1696 names `experiments/427-secret-flag.sh` and the four test names in Prove. They do not exist yet (T-1696 `open`). "Already written in the entry" means the Prove text only. Checked: `ls experiments` shows no 427. |
| 9 | Plan: "`podvm run --rm --secret src=,target=,mode=0400,once` ... the verb is refused today". | V | No row; the parser refuses. |
| 10 | Plan: "A back end trait with a podvm back end (`podvm run` as a child)". | V as a constraint | podvm is bin-only (Cargo.toml). The child process is the only podvm-level path. Nothing wrong, but it is a hardcoded single path (4.2). |
| 11 | Plan 3: "The chroot tier shares the host network namespace, so a port is a host port" and "chroot tier" used for the whole no-root class. | Imprecise | On the target class the tier is the userland rung, not chroot. Chroot is denied there (podvm#114). The row "chroot tier" does not apply to the hosts the plan targets. Source. |
| 12 | Plan: "`exec` is a fresh re-entry ... A service container that a later `exec` must see needs the same root directory. This works for the chroot tier." | Imprecise | A fresh re-entry sees the same filesystem for every container of one image (2: shared rootfs), so it sees *all* containers' state, and none of the process state or environment. |
| 13 | Plan P6: "`containers/...|exec/start`... health state from the `HEALTHCHECK` of the image". | R as "exists" | No podvm health model; no `HEALTHCHECK` parse. This is new work in P6, not reuse. |
| 14 | Plan 3: "podnet, podssh and podvm already ship static binaries for x86_64, aarch64, armv7, riscv64, i686, loongarch64, powerpc64le". | V for the binary; R for the no-root path | podvm builds 7 targets (nightly.yml:34-52). The interposer is x86_64 only. On the other six, a host that denies chroot has no rung. |
| 15 | Plan: "A-V-08 `pod-microvm` (M7) ... T-062" | S/V | T-062 is a P3 study (4.1). KVM and `pod-microvm` do not exist. |
| 16 | Plan: "`pod-broker` is an egress broker ... T-099 done". | U | Not podvm. See R-other. Note podvm T-1665 (`open`) still plans its own "egress broker" with `pod-broker` of podcrates. |
| 17 | Issue/plan: "podvm's rule 'no daemon'" (Q3). | Imprecise | Section 5. |
| 18 | Plan: "Docker `exec` ... the runner uses `docker exec` for each step". | U | About `actions/runner`, not podvm. Not checked. Marked Unverified. |
| 19 | Plan 3 row "Process supervision, hard timeout, destroy in 60 s: `pod-supervise` ... Taken by podvm". | Half | Taken (V). The hard timeout and kill tree do not exist (T-1655 `open`); `stop` kills the payload only. |
| 20 | Issue: "`podrunner` ... Its repository exists and is empty." | V (via brief) | `repos/podrunner` holds only a LICENSE per the brief; not re-checked here. |
| 21 | Plan 6 phase 3: "`podvm build` is `None`: decide to refuse by name". | V | parity.rs:167. Also `commit` None (168) means a "Docker action" whose build step is `docker build` cannot even be emulated by commit. |
| 22 | Plan: "`--secret` ... `once` ... interposer serves the first `open`". | U | T-1696 Approach. The entry itself states the limit: a raw syscall can read the staged file if it learns its host path. |
| 23 | Plan 3: "microVM `T-062`: KVM is free (2026-11-09); a Windows guest needs an operator image". | V | decisions.md row KVM; limits.md. The guest has no network (limits). |

### 3.1 What the issue does not say but the repo does

- The issue lists the podvm items A-V-04, A-V-06, A-V-07, A-V-09, A-V-11 as podvm "own record" items that "can
  start now". The `Priority` ruling in podvm (2026-11-09) puts the asks of cfb and githome (T-1694 to T-1696 and
  clauses of podcrates#39) after podvm's own work order, which is T-1648 first, then the cluster of the restricted hosts,
  then T-1654 to T-1668 (PROGRESS items 13, 14, 16). T-1695 and T-1696 are therefore behind a queue of about
  26 open entries (INDEX). Documented.
- Several A-V entries are mapped to podvm entries of lower priority than the ask. A-V-06 asks P1 and T-1653 is
  P2; A-V-07 asks P1 and T-1655 is P2; A-V-08 P1, T-1656 P2; A-V-11 P2 (T-1696 P2, while the peer asked for P0
  for G-03). Documented in the entries' "Filed" texts.
- podvm issue 224 (COORD-018) is open and needs podcrates v0.12.0 changes for `-w` and first program inside a
  volume. `inputs/i48.md` says the tag exists (`v0.12.0` at 393158f) and that podvm must move its calls
  (breaking: `spawn_userland` and `run_userland` take `&Maps`). Documented. podvm HEAD still pins v0.11.0
  (Cargo.lock), so the adoption is not done at HEAD. Observed.

### 3.2 The `pod-dockerapi` verbs against podvm

The plan section 2.1 lists the calls of the official runner for `container:` and `services:`:
`network create`, `create`, `start`, `exec`, `inspect` (health), `logs`, `rm`, `network rm`, `build`,
`pull`. The runner side is Unverified (not read). Against podvm today:

| docker call | podvm answer |
| --- | --- |
| `network create` / `network rm` | None (parity.rs:188). |
| `create` with `--network`, `--label`, `-e`, `-v`, `--workdir`, `--entrypoint`, `--name` | `create` is Native but `--network` None, `--label` Stub (dropped), `-v` Degraded loader family only. |
| `start` | Native; the memfd family cannot do detached start. |
| `exec` | Degraded; env and workdir not inherited; `-d` None; `-i` no-op. |
| `inspect --format '{{json .State.Health}}'` or `.State.Health.Status` | Not available. Template engine refuses pipelines and unknown fields. |
| `logs` | Degraded; interleaved; no `--since`, no `-t`. |
| `rm` | Native, rootfs stays. |
| `pull` | Native, HTTPS only. |
| `build`, `commit`, `push`, `export` | None. |
| `docker cp` | Degraded. |
| `docker events` | None. |
| `docker info`, `version` | Degraded, Native. The `info` JSON is podvm-shaped (Rung, Parity), not docker-shaped. |

### 3.3 What was not measured for the plan's phase 0 (still unknown in podvm)

- No podvm record mentions `dotnet`, `.NET`, `ICU` or `actions/runner` (grep of `TODO`, `docs`, `experiments`
  found no hit). The A-V-04 acceptance image is `node`, not `actions/runner`. A glibc Node job has a proof only for
  glibc `gcc` and musl `node:22-alpine`; there is no recorded `node:*-slim` (glibc) drive (grep of
  `experiments/results` for `slim`, `node:lts` found nothing). Observed.
- No recorded run of any program that creates threads through raw `clone` or `vfork` under the userland tier at
  runner scale (`.NET` uses its own host process APIs). Unverified; do not assume the interposer covers it.
- `podvm` has no measurement of start time of a unit, disk per run, or peak memory (plan 10, item 12).
  `podvm-workload` and `experiments/360-perf-harness.sh` measure other things (TCG spread, pulls).

## 4. Question (e): hardcoded assumptions that are single points of failure

These are assumptions the issue or plan make about podvm, each with the evidence that bears on it.

### 4.1 Single tier or single path

| Assumption | Evidence |
| --- | --- |
| "The unit is a podvm payload" with one `unit = "podvm"` back end that starts `podvm run --rm --secret ... IMAGE ./run.sh` | `--secret`, `--report`, `--timeout`, `--label`, `-v :ro`, `--ulimit` do not exist yet. Each is a podvm change and each ships in a podvm release; the launcher's only integration surface is argv and exit code. podvm has a 0.1.0-beta.13 version and the owner's ruling "No compatibility with released binaries. A name, a variable or a flag can change" (podvm:docs/decisions.md, 2026-11-05). A flag that podrunner scripts can be renamed. Documented. |
| One tier for jobs | Three families on the target class (chroot, namespace, loader, memfd) answer differently (2.3). `podvm run` selects the rung by probe, and the launcher has to read the banner on stderr to know which ran (parity.rs:121; T-1695 Problem). Without T-1695, a launcher cannot read the boundary of a job. Documented. |
| "Fresh unit for each job" | One shared mutable rootfs per image digest (finding 2). `--rm` removes it after the run unless a record references it; the next run re-extracts. So a fresh unit costs a re-extraction per job (T-0204 and T-1322 text), or is not fresh. Cost per extraction of the node:22-alpine image is not measured in the record. Unverified. |
| "microVM for untrusted jobs (G-02, no fallback)" | Not wired (finding 1). `--podvm-tier` accepts `machine` or `chroot` only (tier.rs:22-30). A run that asks for the machine tier never falls back to a rootfs tier (T-1656 Amended). Both are intended, but no Linux guest runs today. |

### 4.2 Single start path (CLI child process)

- podvm is bin only; `crates/podvm` has `[[bin]]` and no `[lib]` (podvm:crates/podvm/Cargo.toml). 5830 lines of
  orchestration (`lifecycle/`, `run/`, `ladder.rs`, `interpose.rs`) are not importable. `pod-dockerapi`'s "podvm
  back end" has two choices: spawn `podvm` and parse argv/exit/banner/`inspect` strings, or reimplement the glue
  on `pod-enter`, `pod-supervise`, `pod-rootfs`, `pod-extract`, `pod-hostprobe`. Both are real work. Source.
- The table and the launcher are libraries (`pod-supervise`, takes `Settings`). Everything above them
  (`prepare`, `decide_entry`, the interposer environment from `PODVM_MAPS` and `PODVM_LIB_PATH`, the embedded
  interposer objects, banner) is in the bin and in `podvm-interpose` (not a workspace member; embedded as byte
  arrays by `build.rs`). A podcrates crate cannot link it. Source (podvm:Cargo.toml:75-83 comment).
- Process model of a job: each `podvm run -d` creates a double-forked launcher and a payload; `exec` creates more
  fresh processes that no launcher watches. A `docker exec` that outlives its client would be an unwatched
  process (T-0604 principle quoted in parity.rs:295).

### 4.3 Host class assumptions

| Assumption | Evidence |
| --- | --- |
| x86_64 hosts for the no-root path | Interposer pair x86_64 only; T-1327 `done` with "stays x86_64-only and every other arch keeps the honest decline" (podvm:TODO/interpose.md T-1327; limits.md). |
| glibc .NET runner "needs glibc ... or a musl build" | podvm admits glibc payloads on the loader family (T-1644), via the image's own loader and an `LD_PRELOAD` interposer built for glibc and musl (architecture.md). `ld.so` of the image is run by path; the plan's .NET specifics are unmeasured (3.3). |
| `/state` or similar noexec work directories | #114: `/state` rw but `execve` refused. A payload execs from a volume only where the source mount allows exec; the banner names a noexec source (T-1653 Decision). `memfd` exec needs `vm.memfd_noexec` and is measured and refused before staging (T-1688 `done`). |
| Host denies chroot AND the runner host itself needs to run podvm | podvm itself is a static musl binary that needs a store dir (`$PODVM_STORE`), space (checked by `space.rs`), a soft `RLIMIT_FSIZE` ceiling that also bounds guest memory (T-1305, T-1690). |
| KVM available for microVM | Not on the target hosts (#114) or the cloud host. TCG is 3x to 21x slower (T-1308, Documented). |
| Egress through an HTTP CONNECT proxy | #114: "egress only through an HTTP proxy on the loopback"; podvm pulls through `pod-proxy`; tcp/80 is broken, so podvm refuses http registries (T-0201 text in podvm-image doc comment). |
| Host kernel features: SUD, `openat2`, memfd, seccomp | Measured on one host (kernel 7.2.2). The probe has the rows; there is no matrix of other hosts. Documented. |

### 4.4 Per-job loopback addresses 127.x.y.z

- Nothing in podvm uses or reserves loopback ranges. The completion layer writes `127.0.0.1 localhost <name>`
  and `::1 localhost` (podvm:TODO/complete.md T-0403, `pod-rootfs/src/net.rs`) and `--add-host name:ip`. Observed.
- podvm cannot make a payload bind a chosen address. A `redis` image binds what its config says. A per-slot
  `127.x.y.z` would need the service image to bind that address (config change) or a connect-time rewrite
  (a SUD or connect interposer: T-1652, `open`; T-1663 plans a connect filter). Inferred.
- Whether the host allows binding `127.0.0.2` and up is a kernel default on Linux (the whole 127/8 is local) but
  the plan lists it as "verify" (plan section 10, item 7). The recon found no podvm measurement of it.
  Unverified.
- Name to address on a shared rootfs: `/etc/hosts` is part of the shared image rootfs, so two jobs of the same
  image with different `--add-host` lines rewrite one file. The completion rewrite is idempotent per run, not per
  container (Inferred from podvm:crates/podvm/src/run/prepare.rs:389-395 and 156).

### 4.5 memfd assumptions

- The memfd family exists only for a static payload on a chroot-denied host, behind `--unsafe-host-paths`
  (T-1648). It "runs from a staged memfd with the host root" and "reads its own exe as the staged memfd path, so
  a self-locating runtime cannot resolve itself there" (limits.md). Go and .NET runtimes self-locate. Documented.
- The memfd rung measures whether a memfd executes, names `vm.memfd_noexec`, and refuses before staging
  (T-1688 `done`). A `noexec` memfd policy on a target host removes the rung (Documented).
- The memfd family has no detached driver, no `-v`, no `exec` volume table. A service container that is a static
  binary on the target class cannot be started as a detached container. Source.
- Static payload streaming over 128 MiB: fixed by T-1647 (`done`, 161 MB run at 0).

### 4.6 Other single points

| Item | Evidence and effect |
| --- | --- |
| Single store lock for the table and the image index | `containers.json` and the image index share one lock (invariant I1; table.rs:129-136). N parallel jobs all serialise on create, stop, rm. T-0211 (a lock outlives a forking holder) is `partial`. A runner with many slots hits this. Inferred contention; not measured. |
| Single `containers.json` | One file for all containers, rewritten whole by rename. `ps` and `reconcile` read it all. Size grows with records; foreground `run` leaves a record (T-1421). Not measured at 100+ records. |
| Single registry assumption | `docker.io`, `registry-1.docker.io`, `index.docker.io` are the only non-github hosts compiled into non-test code (podvm issue 196 inventory). A job image on GHCR or ECR works through `pod-oci` with `login`, but `public.ecr.aws` answered HTTP 429 twice in podvm's own tests (T-1687, PROGRESS). Documented. |
| Single credential store | `podvm login` writes `~/.docker/config.json` or a credential helper (parity.rs:171). A job image pulled with a private token reads the host user's file. Source. |
| Secrets in the table | See 1.6. |
| Single report channel | The banner on stderr is the only place the entered rung is named, other than `inspect`'s `Rung` field for records. Source. |
| No stable machine-readable error | Docker's 125/126/127 and refusal text. Refusals are English text on stderr. |
| Single OS | Linux payloads only, plus Windows and DOS disk guests behind `windows`. `ensure_linux_guest` refuses other platforms (lifecycle.rs). macOS, BSD hosts: not covered. |
| "Docker `exec` inside job container for each step" | `exec` re-runs `decide_entry`, the probe (`podvm_probe::run()`) and completion for each call, as a fresh process (exec/enter.rs:100-140). The per-step cost is not measured. |
| Interposer is not a boundary | Said in limits.md, SECURITY.md and the T-1696 Decision. Any design that needs isolation of a credential from a step cannot rely on the chroot, namespace or userland tiers. |

## 5. Question (d): crates, libraries, handed to podcrates; and the "no daemon" rule

### 5.1 podvm workspace crates (podvm:Cargo.toml:1-30; podvm:docs/runtime-state.md)

| Crate | Kind | State |
| --- | --- | --- |
| `podvm` | bin only | The CLI. No library. |
| `podvm-probe` | lib | Thin wrapper over `pod-hostprobe` (report, exit codes, JSON writer, non-goals stay). |
| `podvm-image` | lib | Store and image acquisition; the registry client is `pod-oci`; the `Lock` moved to `pod-supervise`. Own store layout, credentials, platform, probe cache. |
| `podvm-extract` | lib | Thin over `pod-extract`; gives it podvm's store layout (`rootfs/<digest>/`). |
| `podvm-complete` | lib | Thin over `pod-rootfs`; podvm's names, CA bundle, banner, `Report` JSON. |
| `podvm-enter` | lib | Re-exports `pod-enter` with podvm's `NAMES` and exit codes. |
| `podvm-supervise` | lib | Re-exports `pod-supervise` with podvm's `Settings`, `Detail`, memo, `launcher::enter`. |
| `podvm-windows` | lib | Thin over `pod-guest`. |
| `podvm-ssh` | lib plus 4 bins (`node`, `operator`, `proxy`, `shell`) | Not handed to podcrates; `pod-ws` (T-016) comes in v0.13.0 with patches (inputs/i48.md). Relay mux `reverse-v1`. |
| `podvm-gate`, `podvm-buildstate`, `podvm-release`, `podvm-measure` | tools | Gate, build input state, release tools, benchmark. `podvm-measure` has `podvm-workload`. |
| `podvm-interpose` | cdylib, excluded from the workspace | Separate; embedded as byte arrays; stays in podvm (decision C8). |

Locked podcrates deps at tag v0.11.0: `pod-core`, `pod-enter`, `pod-extract`, `pod-guest`, `pod-hostprobe`,
`pod-oci`, `pod-proxy`, `pod-rootfs`, `pod-supervise`, `pod-term`, `pod-tls` (podvm:Cargo.lock:407-520).
Observed. Not taken: `pod-broker`, `pod-dns`, `pod-services`, `pod-relay`, `pod-ws`, `pod-testkit`, `pod-todo`
(deps entries T-1678 to T-1680 `blocked` on podcrates releases). Documented.

Handed over (decision "Shared crates", 2026-11-06; PROGRESS item 16): the seven generic crates of T-1681 are with
podcrates (`pod-hostprobe`, `pod-guest`, `pod-enter`, `pod-supervise`, `pod-oci`, `pod-extract`, `pod-rootfs`
plus `pod-term`, `pod-tls`, `pod-proxy`). The files of podvm that podcrates imported "take fixes only" through
COORD issues. podvm issue 221 (COORD-015: whiteout escape fix in `podvm-extract`, T-1698) is open to podcrates.
Documented.

Consequence for the plan: P1 to P5 are "podvm record" items, but the code for `-v` volume pairs, workdir, first
program resolution, `Statfs::f_flags` lives in `pod-enter` and `pod-hostprobe` of podcrates (COORD-018 open;
`inputs/i48.md` says v0.12.0 has them). A podvm limits or timeout feature (T-1655) belongs in
`pod-supervise` (launcher) and `pod-enter`, so it is a podcrates change that podvm then takes. The plan's note
"podcrates does not edit podvm; podcrates ports the generic parts into crates when podvm hands them over" is
the old direction; since T-1681 the direction is reversed for these crates (podvm files frozen, fixes go to
podcrates). Documented (podvm:AGENTS.md:130-139).

### 5.2 The "no daemon" rule: where it is and what it says

Searched `AGENTS.md`, `README.md`, `docs/`, `TODO/RULES.md`, `TODO/cli.md`, `crates/`.

| Place | Text |
| --- | --- |
| podvm:crates/podvm/src/system.rs:13-15 | "podvm has no daemon, so docker's server half of `info` is not withheld and not faked: what stands in its place is the rung this machine permits". |
| parity.rs rows 132, 186, 193, 202, 203, 205, 278 | `info`, `events`, `context`, `secret`, `config`, `plugin`, `--health-cmd`: "there is no daemon to ...". |
| parity.rs:195-205 | Ten more verbs None "on the existing no-daemon and out-of-shape reasons". |
| parity.rs:133 (`version`) | "one artefact, so there is one version and no client/server split". |
| podvm:crates/podvm/src/names.rs:20-24 | "RULED BY THE OPERATOR ON 2026-09-08: podvm refuses to take the `docker` name where a working docker daemon is reachable ... A machine with a working daemon is a machine where podvm is the wrong tool." |
| podvm:TODO/RULES.md:132 | "Refuse the docker name beside a reachable daemon unless explicitly selected" (T-0803). |
| podvm:AGENTS.md:227 and TODO/cli.md:289 | Cite `TOOL.md` section 2.0 and 6.8 of `Azathothas/container-research` for "the shape" podvm is in. That file is not in the tree (searched). The text of section 2.0 is therefore not available here; Unverified. |
| podvm:docs/decisions.md | No row says "no daemon". The row "New projects" (2026-11-09) says: "`podrunner` runs podvm as a unit kind. podvm does not grow those projects." |
| podvm:TODO/image.md:2072 | "No daemon, no background sweeping" as a design choice of a store GC (T-0211-era), not a global rule. |

Reading: "no daemon" is a stated property of podvm's design (it is a CLI with a launcher per container), enforced
in the parity table by named refusals, plus one owner ruling about the `docker` argv0 name. It is not an owner
ruling that forbids a socket server in another project. The plan's Q3 ("despite podvm's rule 'no daemon'") asks
about a rule that is a design property and a table of refusal reasons. Also: owner statement 2026-11-09
"`podrunner` runs podvm as a unit kind. podvm does not grow those projects" bounds where a `pod-dockerapi`
could live (podrunner or podcrates, not podvm). Inferred.

The rule on `docker` name: with a unit that exposes a Docker API socket, podvm's own `docker_daemon()`
(names.rs:84-130) connects to `$DOCKER_HOST` or `/var/run/docker.sock` and treats an answer to `GET /_ping` with
200 as "a working daemon". So inside such a unit `podvm system install-names` would refuse `docker`. The plan's
alternative (a static real `docker` CLI in the golden image) avoids the symlink but means two CLIs: the real
docker CLI against `pod-dockerapi`, or podvm as `docker` direct. Both are existing paths (4.2). Source.

## 6. What already exists that issue #47 proposes to build

| Proposed in the issue | Already in podvm or podcrates | Gap |
| --- | --- | --- |
| P2 `--secret` | Nothing for the flag. Existing: `--env-file` (env only); `login --password-stdin` shows the "never argv" posture; staged-file mode is a design in T-1696. | The flag, `once`, the staging. |
| P3 `-v` through the path map | T-1653 `partial`, done for `:rw` on the loader family (`lifecycle/volume.rs`; "[the drive](../experiments/results/volumes-1653.txt)": before each `-v` exited 125, after a read, a write that lands in HOST, `cd /data`, grandchildren from the volume, `run -d` then `exec`). | `:ro`, `-w`, first program, memfd family, chroot rung. |
| P4 report and labels | Nothing. T-1695 `open` with a field list. | All. |
| P5 limits | Nothing. T-1655 `open`. | `--ulimit`, tree kill, RSS and CPU. |
| Probe JSON, tier word | `probe --json`, `probe --rows`, `system info`. | A-V-10 schema publication. |
| Parity as data | `system info --format '{{json .Parity}}'`. | None for a dockerapi shim's refusal vocabulary. |
| Name table for services | `--add-host`. | Per-container, not per-image rootfs. |
| Per-job proxy and egress broker | podvm T-1665 `open` (egress broker, loopback-only proxy), T-1663 (proxy-forced programs via `podnet-netns`). `pod-broker` T-099 done in podcrates. | Not in podvm. |
| Health | None. | New. |
| A long-lived outbound agent | `podvm remote ssh serve` (relay dial out, `--once`). `podnode` is planned. | Not a job runner. |
| Fake job source | podvm has `podvm-measure` fixtures and `experiments/180-registry-fixture.sh` (a loopback zot registry). | A stand-in GitHub is not in podvm. |
| Linux microVM for a job | Pieces: T-1303 initramfs builder, T-1304 serial exec protocol, T-1301 probe, experiments 146, 147, 361. | Wiring (T-1656), kernel pin, workspace copy-back, stdout and stderr split, stdin. |
| CI matrix of restricted hosts | podvm's own CI: `gate.yml` (ubuntu-latest, checkout), `nightly.yml` (7 targets, qemu-user-static, `sudo apt-get`), `secrets.yml`. `record-issues.yml`, `remote-items.yml`. No `container:` or `services:` anywhere (grep of `.github/workflows`). | Dogfood corpus: podvm's workflows need `sudo apt-get install qemu-user-static`, `rustup`, `cosign`, uploads. A podvm-in-podvm run of `nightly.yml` would need root. |
| `pod-dockerapi` | Nothing (no listener, no verbs). | All. But podvm has the table that records the refusal text for each verb. |

## 7. Surprises (contradictions with issue #47, the plan, #35 or #36)

1. Issue text: "`pod-guest` and the machine tier of podvm give a microVM for an untrusted job."
   Artefact: podvm:crates/podvm/src/tier.rs:138-141 (every branch returns 125 for a Linux image), and the entry
   T-1656 "running a guest from `run` is not wired" (podvm:TODO/machine.md:1226-1230). `pod-guest` is the
   Windows and DOS driver (podvm:crates/podvm-windows/src/lib.rs:1-12).
2. Issue text: "podvm marks `network`, `volume`, `-v` and `--network` as not supported today."
   Artefact: podvm:crates/podvm/src/parity.rs:222 (`-v` is Degraded, T-1653 partial since 2026-11-10).
3. Plan section 3: "`pod-extract` and `pod-rootfs` wait for the take."
   Artefact: podvm:Cargo.lock:427-433, 491-497; podvm issue 225 (COORD-019, commit 0682c0c).
4. Plan: "A-V-04 ... open ... `statx` ... T-1697".
   Artefact: podvm:TODO/INDEX.md:319-326, 373 (all `done`), podvm:TODO/interpose.md:2373.
5. Plan: "Filed as podvm T-1667-area entry (cli.md)" for `--secret`.
   Artefact: podvm:TODO/cli.md:2350 (`### T-1696`), INDEX 343 (T-1667 is the doctor).
6. Plan: "a podvm unit ... fresh unit for each job".
   Artefact: podvm:crates/podvm-extract/src/lib.rs:46 and TODO/image.md T-1322 ("the shared rootfs", "manifest-keyed
   rootfs").
7. Plan: "podvm decided 'no daemon'".
   Artefact: no row in podvm:docs/decisions.md; it is a reason string in parity.rs and a citation of an external
   spec. The one ruling is names.rs:20-24 (2026-09-08) about the `docker` name.
8. Plan P6: "health state from the HEALTHCHECK of the image" and "labels".
   Artefact: no HEALTHCHECK parser or label store anywhere in podvm or in `pod-supervise`'s `Container`.
9. Plan 3: "podnet, podssh and podvm already ship static binaries for ... loongarch64, powerpc64le ... "
   Artefact: the binaries build (nightly.yml:34-52), the interposer does not (T-1327: "pair stays x86_64-only"),
   so the no-root path is not on those six.
10. `inputs/i48.md` (the podcrates session to podvm, 2026-11-10) asks podvm to move to v0.12.0 and changes
    `spawn_userland` and `run_userland` to take `&Maps` (breaking). podvm HEAD at 36b15ec is still on v0.11.0
    (Cargo.lock). Any plan item that reads "podvm will use X from v0.12.0" has a pending podvm commit between.
11. The issue says the podvm items A-V-04, A-V-06, A-V-07, A-V-09, A-V-11 "can start now" under the ruling
    "Priority". The podvm record orders them after T-1648 and the cluster of the restricted hosts (PROGRESS items 13 to 16).
12. podvm:README.md line 22 ("The runtime has ... a machine tier") and the AGENTS.md section 1 list "a machine tier
    (QEMU on KVM or TCG)" next to the other tiers. The tier is for Windows and DOS disk images and `machine ssh`.
    A reader (the plan's author) can take "machine tier" as "runs a Linux container as a VM". The doc does not say
    "not wired" in the README. Documented ambiguity, observed.
13. Stale handoff: podvm:docs/HANDOFF.md lists the M6 imports as pending while Cargo.lock holds them all.
14. Issue 168 acceptance text still spells `--podbox-tier=machine` (the flag is `--podvm-tier`, call C2).
    Stale text in an open issue, no code impact.

## 8. Coverage

Read in full or substantially: `README.md`, `AGENTS.md`, `docs/limits.md`, `docs/decisions.md`,
`docs/architecture.md`, `docs/runtime-state.md`, `docs/HANDOFF.md`, `docs/containers.md` (it is the Windows and
WSL development lane, not container features), `docs/hosted-sessions.md`, `crates/podvm/src/parity.rs` (whole),
`crates/podvm/src/tier.rs`, `machine.rs`, `names.rs` (head and daemon check), `system.rs` (head), `exec.rs`,
`exec/command.rs`, `exec/enter.rs` (to line 140), `run/prepare.rs` (to line 95, with greps), `lifecycle/entry.rs`
(head), `lifecycle/copy_tree.rs` (container fields), `crates/podvm-supervise/src/{lib,table,launcher}.rs`,
the headers of every other podvm crate `lib.rs`, `Cargo.toml` and `Cargo.lock` (pod-* rows), `.github/workflows/*`
by grep. TODO entries read: cli T-1655, T-1695, T-1696 and the T-0801 text; enter T-1653, T-1652 (tail), the
Amended text on docker and services; interpose T-1646 and T-1697, T-1699; machine T-1303, T-1304, T-1656, T-1682,
T-1702; net T-1662 and T-1663 heads; image T-1322; complete T-0403; supervise heading list; INDEX rows 319-375;
PROGRESS items 8-16. Forge issues: list of all 184 titles and states; bodies read for 167, 168, 184, 186 (head and
the podvm section), 193, 196 (head), 224, 225, and 114 (head 9000 characters plus grep). Also read: the issue body
of podcrates#47 from the forge mirror, `inputs/podrunner-plan.md`, `inputs/i48.md`, the A-V rows of `inputs/i35.md`.
Podcrates, read to decide a podvm claim: `pod-supervise` (launcher, table), `pod-guest/plan.rs` (nic lines),
`pod-hostprobe/select.rs` (Rung), `docs/design.md` rows 302-314, `TODO/podvm.md` T-062.

Not covered, and why:
- No podvm binary was built or run (no target dir; the build needs zig and network git dependencies). Behaviour
  claims are from source and entries; none is a run. A cheap next step for a later pass: build musl podvm and run
  `podvm system info --format '{{json .Parity}}'`, `podvm run --podvm-tier=machine alpine true`, and a two-container
  `/etc/hosts` collision on one image.
- `TODO/*.md` is 26 000 lines; the entries not named above were not read (`gate`, `deps`, `packaging`, `probe`,
  `extract`, most of `complete`, `supervise`, `enter`, `interpose` bodies). A search was used to locate the ids
  asked for; a search hit was never taken as a verdict.
- The `podvm-ssh` internals (mux, ws, server), `podvm-probe`, `podvm-gate`, `podvm-release`, `podvm-measure`
  source bodies, and the interposer source (`crates/podvm-interpose`): headers and docs only.
- `experiments/` scripts and `experiments/results/`: only 361, 290 and a header grep. The result files were not
  opened, so every "done" in the table is the entry's own record plus INDEX status.
- `docs/history/`, `references/`, `vendor/`: skipped as the brief and AGENTS.md place them outside the live tree.
- Issues #166, #169 to #178, #180, #182, #188, #189 (scope issues): titles only, except via the TODO entries that
  quote them (T-1655, T-1656, T-1662, T-1663). The body of #114 beyond its first 9000 characters was searched for
  loopback and proxy words only.
- `actions/runner` behaviour, GitHub terms, the docker CLI calls of the runner: not podvm, not read. Marked
  Unverified wherever the plan states them.
- The `podrunner` repository state (LICENSE only) is taken from the brief.
