# R2: podnet and podssh (recon for the design review of podcrates #47)

Date of the work: 2026-11-10 (machine clock, UTC). Facts only. No design.

Labels: Observed (I ran it or read the artefact), Documented (a doc says it), Source (code says it, path:line),
Inferred (reasoning), Unverified (relayed, not checked). Paths are repo-relative (`podnet:` or `podssh:`).

## 0. Method, versions, and one broken input

- Observed: clones at podnet `cbe9053` (commit date 2026-11-10 09:22 UTC) and podssh `961acb1` (2026-11-10 06:35 UTC).
  Both are shallow with one commit (`git log --oneline | wc -l` gives 1). The SHAs that #35 names for its State lines
  (podnet `c52b8e0`, podssh `7e38d5b`) are not in the clones, so a State line cannot be diffed against its own commit.
  I compare each State line with the head.
- Observed: I built both binaries from the clones (release profile, Rust toolchain on this machine).
  `podnet --version` gives `podnet 0.4.0-beta.1 (unknown)`. `podssh --version` gives `podssh 1.1.0-beta.3`.
  Commands marked "ran" below ran on these builds. This machine is a sandbox whose only egress is an HTTP proxy
  (`podnet status` shows `proxy.source: HTTPS_PROXY`, kind `http`), with `NODE_USE_ENV_PROXY` set in the environment.
- Observed: `podrunner-spec/inputs/i47.md` does not exist and `i47-comments.md` is empty (0 bytes). I read the body of
  #47 from the forge (`.../repos/PodUtils/podcrates/issues/47`) and quote that. The plan file is present.
- Observed: both issue lists were fetched (`state=all&per_page=100`): podnet 336 issues (numbers 1 to 419),
  podssh 168 (numbers 1 to 181). One page returned a transient 403 and then worked on retry.

## 1. Question (a): the egress machinery that #47 proposes to rebuild or host

### 1.1 Inventory

| Piece | Exists? | Evidence |
| --- | --- | --- |
| `podnet pod` (program held to one upstream; seccomp and netns backends) | Yes, in release `v0.4.0-beta.1` | Observed: `podnet pod doctor` ran, both backends `ok` here. Documented: `podnet:crates/podnet-netns/README.md` |
| `podnet-netns` as a library | Yes, path crate, not published; module list `upstream`, `socks`, `dns`, `listen`, `tailscale` | Documented: `podnet:crates/podnet-netns/README.md` ("Library"); Source: `podnet:crates/podnet-netns/Cargo.toml` (edition 2021, `pod-proxy` by git tag v0.11.0) |
| Shared "hold" crate (`pod-hold`) | Not built. A peer view and a cut are written | Documented: podnet#414 ("The view on pod-hold"); `podcrates:TODO/INDEX.md:158` (T-109, research, open) |
| `podnet warp proxy`, `warp run`, `warp exec`, `warp tun`, `warp nc`, `warp forward` | Yes | Documented: `podnet:README.md:643-887`. Observed: help list of the build |
| Rules for a flow (`-r domain:/cidr:` with `direct`, `reject`, a chain; `--default-action reject`; `--flow-log`) | Yes (T-156, T-157 done) | Observed: ran `pod --default-action reject --flow-log F`; the program got `ECONNREFUSED` and F held `{"chain":[],"event":"dial","flow":1,"proto":"tcp","rule":"default","target":"example.com:443","time":...,"verdict":"reject"}` |
| `--learn`, `--fail-on-violation` | No (T-158 open) | Observed: `podnet pod --help` has neither. `podnet:TODO/INDEX.md:206` |
| Router with members, groups, health checks, kill switch (`podnet route`) | No. Plan only (T-095 to T-101, T-177, T-178, T-180 open; issues #371 to #376 open) | Observed: `podnet route --help` exits 64. `podnet:TODO/INDEX.md:143-149` |
| `podnet service install` (hardened system unit) | No (T-219 blocked on podcrates T-102) | Observed: `podnet service --help` exits 64 |
| `podnet bench` | No (T-221 open) | Observed: exit 64 |
| `--metrics` listener and `/ready` | No. `--metrics` is a usage error | Source: `podnet:src/cli/argv.rs:30`. Observed: `podnet tunnel --url http://127.0.0.1:1 --metrics 127.0.0.1:2000` exits 64 |
| A control API on a socket (`version`, `status`, `drain`, `reload`) | No, in either repo, for a long-running tool. Instances are files plus signals | Observed: no `control` command in podnet (exit 64). Source: `podnet:crates/podnet-core/src/instances.rs:135-162` (a JSON record file, a lock file, a log) |
| Local precedent for a socket protocol | Yes in podssh: `session` daemon, one JSON line per request, `schema` first, ops `info send wait capture stop` | Source: `podssh:crates/podssh-cli/src/session/wire.rs:1-36`. Precedent also in podcrates `pod-broker` control (R1 scope; not read by me) |
| Detached instances (`--detach`, `ps`, `logs`, `wait`, `stop`, supervisor with backoff) | Yes | Observed: ran `net proxy --detach --name t1`, `ps --json` (keys `dir instances podnet schema`; each record has `name pid supervisor started command args cwd state ready log restarts`), `stop --all` |
| Agent mode (`podnet agent up/status/down`) | Yes | Observed: `agent status --json` and `doctor --agent --json` ran. Keys of the second: `advice ipv6 localhost podnet proxy resolver rows schema udp_dns warp_tcp warp_up`. `rows` has 7 tasks: git (HTTPS), git (SSH), pip, npm, cargo, go modules, docker pull. Each row has `target task proxy direct warp` |
| `doctor --json` | Yes, schema 1, keys not pinned by a test, no `--quick`, no `lanes` | Observed: top keys `clock legs names next ok paths podnet proxy roads schema udp`; legs keys `leg reason status verdict`. `podnet doctor --help` shows `--json` and `--samples` and no `--quick`. Documented: `podnet:TODO/fixes.md:3539-3572` (T-216) |
| Share and serve (files, PIN, quick tunnel) | Yes. Inbound publishing, not egress | Documented: `podnet:README.md:456-565`. Quick tunnel limits: 200 requests in flight, no SSE (README line 7) |
| Relay roads (`--transport relay` as `http2+relay`, `relay pair/node/connect/stop`, WARP over the relay) | Yes, one default relay host plus a pool | Documented: `podnet:README.md:1076-1121`; `podnet:SECURITY.md` ("What the relay can see") |
| `net proxy`, `net switch`, `net tailscale`, `net socksify`, shims | Yes (virtual network, SOCKS door to a tailnet) | Documented: `podnet:README.md:915-966` |
| `pod-broker` consumer in podnet or podssh | No. Neither Cargo manifest lists it | Source: `podnet:Cargo.toml` pod deps are `pod-todo pod-core pod-tls pod-proxy pod-dns pod-services`; the `pod-*` deps of the podssh manifests are `pod-core pod-dns pod-identity pod-proxy pod-services pod-term pod-testkit pod-tls pod-todo`. `grep -l pod-broker` over every `Cargo.toml` of both repos finds nothing. `podcrates:crates/pod-broker/README.md:57-58` says a job reaches the broker through `podnet pod -x http+unix:PATH` |

### 1.2 `podnet pod`, seccomp backend: the exact capabilities and limits

Documented (all from `podnet:crates/podnet-netns/README.md` unless noted), then what I ran.

Needs: seccomp with `SECCOMP_RET_USER_NOTIF` (Linux 5.0 or newer). No root, no user namespace, no TUN. Works on static and Go binaries.

Mechanism, Source: `podnet:crates/podnet-netns/src/seccomp/filter.rs:57-77`. The filter stops these syscalls: `socket connect bind listen accept accept4 sendto sendmsg sendmmsg recvfrom recvmsg recvmmsg setsockopt getsockopt getsockname getpeername close shutdown io_uring_setup io_uring_enter io_uring_register`.
A foreign ABI (i386, x32 on x86_64) gets `ENOSYS`. `io_uring_setup` gets `EPERM`
(`podnet:crates/podnet-netns/src/seccomp/mod.rs:303-307`). `write` and `writev` are not stopped.

What a program gets:

- `socket(AF_INET or AF_INET6, STREAM or DGRAM)` returns one end of a socketpair (`SECCOMP_IOCTL_NOTIF_ADDFD`).
  The supervisor dials the upstream and relays bytes in user space (`copy_bidirectional`; issue #351 open: two copies per chunk;
  the direct-socket hand-off exists only for a single unix-socket hop, `podnet:crates/podnet-netns/src/handoff.rs`).
- DNS: any UDP to port 53 is answered from a fake-IP pool `198.18.0.0/15` (131072 addresses, each given once; the 131073rd new name gets `SERVFAIL`).
  The upstream gets the name (SOCKS5 `ATYP=3` or the CONNECT host). AAAA gets `fd63:6672:7300::/96`.
- Upstreams: SOCKS5, HTTP CONNECT, a chain (`-x` repeated), a unix socket (`socks5://unix:/p`, `http://unix:/p`), `tailscale:/sock`.
  `--serve LISTEN` serves SOCKS5, CONNECT and UDP ASSOCIATE on a unix or TCP address through the chain.
- UDP: SOCKS5 UDP ASSOCIATE, or "UDP in the stream" (command `0x83`) on a unix-socket proxy.
  Datagrams up to 65507 bytes. No ICMP error reaches the program. No ping to the internet.
- Rules: first match wins. `direct` dials from the host with the host resolver and no proxy. `reject` or `block` gives `ECONNREFUSED` (TCP) or `EACCES` (datagram).
  `--default-action reject` is default-deny.
- Proxy variables are removed from the program environment unless `--keep-proxy-env`. Observed message: `the program does not get no_proxy, https_proxy, ... (the pod is its egress...)`.
- A listener of the program becomes a unix socket in `--inbound-dir` (mode 0700 directory, refused if not the user's).

Limits and holes (each labelled):

1. Observed: `socket()` of any family that is not INET stream or datagram is passed to the kernel unchanged
   (`podnet:crates/podnet-netns/src/socket.rs:31-33`: `if !inet || (kind != SOCK_STREAM && kind != SOCK_DGRAM) { return notif_continue }`).
   Ran inside `pod --default-action reject --backend seccomp`: `connect()` to a host unix socket succeeded; `socket(AF_VSOCK)` was created;
   `socket(AF_NETLINK, SOCK_RAW)` was created; `socket(AF_INET, SOCK_RAW)` gave `EPERM` (no capability here). Direct `connect(1.1.1.1:443)` gave `ECONNREFUSED`.
   So the pod redirects IP traffic. It does not confine other socket families, and a payload can reach any host unix socket (for example a container socket, an agent socket).
2. Observed: with `--default-action reject`, a `connect` to `127.0.0.1:47123` reached a listener owned by a host process (the listener printed "accepted from pod").
   Documented: "A TCP connect to `127.0.0.0/8` with `seccomp` does not read the rules" (README table row `-r`; `README.md` "Backends"). Source: `podnet:crates/podnet-netns/src/connect.rs:179-215` (loopback goes to the program's own inbound listener, else to host TCP).
   So host loopback services are reachable from a held payload. A host listener for a broker must be a unix socket to avoid this.
3. Inferred (not run): a socket that is not in the supervisor's table (a descriptor inherited from outside, or received over a unix socket with `SCM_RIGHTS`) takes the `notif_continue` path
   (`podnet:crates/podnet-netns/src/connect.rs:140`) and connects directly.
4. Documented: the supervisor reads the sockaddr from `/proc/<pid>/mem` and checks `SECCOMP_IOCTL_NOTIF_ID_VALID`. When writes to program memory are refused,
   `getsockname` and `getpeername` fail with `EOPNOTSUPP`, a warning prints once, and `pod doctor` row `write program memory` shows `fail`. A Go program that type-asserts `LocalAddr()` can panic (issue #186, closed).
   The pod does not refuse to run in that case. A-V-02 asks for a refusal when `/proc/<pid>/mem` is denied. That refusal is not the current behaviour (README "seccomp" bullet on `getsockname`).
5. Documented: kernel UDP socket features are echoed, not real (`IP_TTL` is stored, not applied). `recvfrom` without a source buffer is not stopped (a TCP read passes straight).
6. Documented: an AppArmor policy can allow `unshare` but leave `lo` down, so `netns` fails on a GitHub `ubuntu-24.04` runner and `auto` falls back to seccomp (`podnet:docs/target-environment.md`, runner table, measured 2026-10-31 and 2026-11-01).
7. Documented: the fuzz tree has 18 targets and 3 are seccomp handlers (`seccomp_sendmsg`, `seccomp_sendto`, `seccomp_sockopt`: Observed in `podnet:fuzz/fuzz_targets`). A differential check against the host kernel exists (`podnet:tests/pod_seccomp_diff.rs`).
   Issue #353 is still open on GitHub although T-154 and T-155 are done (`podnet:TODO/INDEX.md:202-203`).
8. Not tested anywhere in the repo: the official `actions/runner` (.NET), Node `fetch`, or any CI payload under the pod. `grep` for dotnet and `actions/runner` in `podnet:docs`, `TODO`, `tests` finds only runner-hosting prose and the cosign issuer string (`podnet:src/update.rs:29`).

Backend `netns` (for comparison, Documented): needs unprivileged user namespaces and `/dev/net/tun`. A smoltcp stack on the TUN. TCP and UDP. A datagram above 1500 bytes is a fragment and does not pass.
256 flows in the UDP table, 60 s idle. `netns-raw` (a `warp run` backend) carries ICMP and raw IP.

### 1.3 What the docs say about the sandbox hosts (host-class facts that bear on any proxy design)

Measured hosts, `podnet:docs/target-environment.md` (Documented, measured 2026-11-01) and `podssh:docs/target-environment.md` (measured 2026-10-04):

- One sandbox allows only an HTTP CONNECT proxy. CONNECT to ports 443, 80, 8443 gives 200. Ports 22, 25 give 403. Private or metadata addresses give 403.
- CONNECT to an IP literal is reset (`1.1.1.1:443`, the edge `198.41.200.x:7844`). CONNECT to `cloudflare-dns.com:443` works. The proxy resolves names; the host's resolver fails.
- A proxy that answers 200 and then sends no byte exists (podnet `doctor` names it). A proxy that terminates TLS cannot carry the MASQUE or edge connection (README line 21; `SECURITY.md`).
- One host had no `bind(AF_INET)` ("Listening: the operator reports that binding sockets is not allowed"; `bind(::1)` gave `EAFNOSUPPORT`). One host had a writable home that does not run programs.
- A proxy answered 200 for one relay host name and 504 for another in the same second (podssh doc).

## 2. Question (b): what podssh offers for reaching a runner host

### 2.1 Node mode and the relay

- Documented/Observed: `podssh node NAME [TARGET...]` is reverse mode: the host dials out (WebSocket on 443 through `HTTPS_PROXY` if set) and serves a local `TARGET` (default `127.0.0.1:22`)
  to operators. Observed in `podssh node --help`: `--sshd` (a roost in front of a running sshd), `--exit`, `--routes`, `--carrier ADDR` (a byte stream that exists already: `stdio`, `fd:N`, `exec:CMD`), `--publish-tls`, `--acme`, `--detach`, `--install`, `--uninstall`, `--env-file`, `--shares FILE`.
- `podssh relay pair NAME` makes a pair. A pair lasts 72 h. `relay pair --renew NAME` renews it where the relay offers renewal.
  Documented: the default relay's contract has no renew request, so a node on it gets "exit 69 with the next pair in the last 24 h as the way" (`podssh:docs/cli.md:3862-3880`; released in `v1.1.0-beta.2`, `podssh:docs/STATUS.md:121`).
  A node renews its own pair at the start of its last 24 h, then hourly (`podssh:crates/podssh-cli/src/node_expiry.rs`, Documented).
- Relay limits, Documented (`podssh:README.md` "Relay limits"): 180 s idle cut, 12 h per session, 64 MiB per session (both directions), 72 h token, public targets only.
  Observed in the help: `podssh relay serve` is the same relay as a role of the binary, self-hostable: `--listen --cert/--key or --acme --mint (anyone|keys:FILE|keys:https://URL|net|nobody) --quota --usage --idle --session-hours --max-session-mib --pairs FILE --sshd ADDR:PORT --publish NAME=PAIR --deployment NAME=HOSTS`.
- Operator side: `podssh ssh node://[user@]NAME`, `podssh operator`, `podssh proxy` (`ProxyCommand`), `podssh pipe node:NAME`. A net (`podssh net`, T-429) is an owner-signed list of nodes with roles `node exit relay share`.
- `podssh each`, `run`, `job` refuse `node://`, `iroh:` and `derp:` destinations. Ran: all three exit 64 with `a node://, iroh: or derp: destination is for podssh ssh` (this is A-S-01; T-708 open). A node is not a `-J` hop (`podssh:crates/podssh-cli/src/ssh/hop.rs:71`).
- Operator-side platforms: release workflow builds podssh for linux x86_64, aarch64, armv7, riscv64 (musl), macOS x2, Windows x86_64 and aarch64 (Source: `podssh:.github/workflows/release.yml:101-109, 160-290, 293-302, 336-348`).
  It builds no i686, loongarch64 or powerpc64le binary (grep of `release.yml` finds none; `podssh:docs/development.md` mentions only the riscv64 and armv7 jobs). podnet's `release.yml` builds x86_64, aarch64, arm, armv7, i686, loongarch64, powerpc64le and riscv64 (all musl) and no other OS; the published `v0.4.0-beta.1` has those 8 binaries (Observed from the release asset list).

### 2.2 Service-unit rendering and hardening (T-102 `pod-unit` overlap)

Observed (Source): two renderers exist today, and neither hardens.

| Renderer | File | Unit content |
| --- | --- | --- |
| podssh user unit | `podssh:crates/podssh-cli/src/node_install/text.rs:124-152` | `StartLimitIntervalSec=0`, `ExecStart`, `WorkingDirectory=-`, `Restart=always`, `RestartSec=5`, `RestartPreventExitStatus=64 78`, `WantedBy=default.target`. No `User=`, no `NoNewPrivileges`, no `ProtectSystem`. `%` and `$` doubled |
| podnet user unit | `podnet:crates/podnet-core/src/autostart.rs:327-332` | `Wants/After=network-online.target`, `Restart=on-failure`, `RestartSec=5`, `RestartPreventExitStatus=64 78`, `StartLimitIntervalSec=0`. No hardening |

- The two disagree on `Restart=` (`always` against `on-failure`). Each also has its own cron, LaunchAgent (podssh only: `node_install/launchd.rs`) and Windows `schtasks` (podssh only: `node_install/windows.rs`) writers.
- Secrets: podssh writes variables to `node-NAME.env` beside the pair and refuses `PODSSH_RELAY_TOKEN`, `PODSSH_CF_ACCESS_TOKEN`, `PODSSH_CF_ACCESS_CLIENT_SECRET` (`podssh:crates/podssh-cli/src/node_install/env.rs:36`). podnet writes private files under `<state>/autostart/` (README "Autostart"). Neither puts a token in the unit text.
- Both repos plan the same system unit twice: podssh T-709 (`--print-unit`, `--install --system --run-as USER`, a hardening list of `NoNewPrivileges`, empty `CapabilityBoundingSet=`, `ProtectSystem=strict`, `ProtectHome=tmpfs`, `PrivateTmp`, `PrivateDevices`, `RestrictAddressFamilies`, `SystemCallFilter=@system-service`, `MemoryDenyWriteExecute`; `podssh:TODO/serve.md:1915-1965`) and podnet T-219 (`podnet service install`, `--print-unit`; `podnet:TODO/services.md:1328-1363`). Both say they wait for `pod-unit` (podcrates T-102, open: `podcrates:TODO/INDEX.md:151`) and, in podssh's case, "Until then podssh writes the text itself".
- The restart-code convention is already shared with `pod-core`: `Class::Config` and `Class::Revoked` both exit 78 "so that one `RestartPreventExitStatus=78` stops a restart loop for both" (`podcrates:crates/pod-core/src/failure.rs:125-127`).
- No `--print-unit` exists in the released podssh (Observed: `podssh node --help` has `--install` and `--uninstall` and no `--print-unit`; `grep` finds the string only in `podssh:TODO/`).

### 2.3 Exit codes and JSON conventions that exist

| Item | podnet | podssh |
| --- | --- | --- |
| Base convention | sysexits: 0, 1, 3 (no backend), 64, 69, 75 (temp fail), 78, 124 (wait or ready timeout) (`podnet:crates/podnet-core/src/exit.rs:47-57`, Observed `podnet man exit-status`) | OpenSSH rules for `ssh` (remote status, 128+signal, 255 own failure) and sysexits for the rest: 1, 64, 65, 66, 69, 70, 73, 75, 77, 78 (Observed `podssh man exit-status`; `podssh:docs/cli.md:5674-5770`) |
| Meaning of 75 | temp fail (quick tunnel 429; detach pipe lost) | a `--timeout` passed (cp, session, job wait), also quick tunnel refusal |
| Timeout | 124 | 75 |
| Login or credential refused | none | 77 |
| Revoked credential (A-C-07) | none | none |
| Not supported or not built | none (no 80) | 70 |
| Policy refusal | none (no 79) | none |
| JSON stamp | integer `schema` 1 plus `podnet` version string on each document (`podnet:src/status.rs:119-127`) | integer `schema` 1 plus `podssh` key (Observed: `status`, `doctor --json` print `schema: 1`); the `--result-file` object has `schema exit status signal end failure bytes duration_ms relay` (`podssh:crates/podssh-cli/src/ssh/result.rs:26-37`) |
| Flow log or ready line | the `--flow-log` lines carry no `schema` (Observed); the detach ready line carries `event` and no `schema` (Observed); issue #400 (closed) records the same gap for `share` | `session` socket lines carry `schema` first |

- Source: `podcrates:crates/pod-core/src/failure.rs:131-146` already defines one table (usage 64, notfound 66, unavailable 69, tempfail and timeout 75, protocol 76, refused 77, config and revoked 78, policy 79, unsupported 80; entry T-101 done).
  Neither podnet nor podssh uses that table for its exit codes. podnet and podssh use `pod_core::Class` only inside dial failures (`podnet:src/egress.rs:363`, `podssh:crates/podssh-ws/src/tls.rs:127`). So A-X-17 (one exit table) has a library table and two diverging tools.
- Signals: Documented (`podnet:README.md` "Manual, status and exit codes"): SIGINT, SIGTERM and SIGHUP each start a clean shutdown. In podssh, `relay serve` rereads `--cert/--key` on SIGHUP (`podssh --help relay`, `--cert` row). So SIGHUP means stop in one tool and reload in the other.

## 3. Question (c): existing tooling that already runs CI, workflows or jobs

Finding: nothing in either repo runs a CI workflow, speaks a forge runner protocol, or knows a GitHub Actions host. What exists is remote command execution.

| Tool | What it does | Fit and gap |
| --- | --- | --- |
| `podssh run [user@]host[:DIR] CMD...` (the command after a double hyphen) | Copies the project (`cp -r -p` rules, digest check, skips `target/ node_modules/ .git/`), then runs `cd DIR && CMD` on the same connection. Exit status is the command's. `--result-file` writes a JSON object. | Documented: `podssh:docs/cli.md:1994-2063`. No isolation, no fresh unit, no cleanup, no timeout kill of a tree on the host |
| `podssh job start/submit/list/log/wait/fetch/stop HOST` | A job in a tmux session on the far host, or `--no-tmux` under `nohup setsid`; a queue (`submit`); output in `~/.podssh/jobs/ID/out`, status in `status`. Needs tmux or `--no-tmux`. | Documented: `podssh:docs/cli.md:2064-2209`. A job queue over SSH, not a CI queue |
| `podssh each HOSTS CMD...` (the command after a double hyphen) | One command on many hosts, `--parallel` default 8 (64 max), worst status as exit, `--output-dir`. | Documented: `podssh:docs/cli.md:1692-1857` |
| `podssh mcp` | MCP stdio server: `ssh_exec`, `status`, `doctor`, `man`, `health`, `job_*`, `cp_put`, `cp_get`, and `session_*` with `--allow-session`. Host allow list, directory allow list. | Documented: `podssh:docs/cli.md:5227-5335`. An agent gate, not a runner |
| `podssh session` | A shell that outlives each call, over a Unix socket (JSON lines, `schema` first). | Source: `podssh:crates/podssh-cli/src/session/wire.rs` |
| `podnet agent up` | Tests egress, starts WARP if needed, exposes ports, prints one JSON document. | Documented: `podnet:README.md:1054-1075` |
| Test boxes and stand-in proxies | `podnet:scripts/test_in_box.sh` (a Unix user whose only egress is a policy proxy, by iptables owner match, needs sudo and iptables), `podnet:scripts/box/proxy.py`, `podnet:scripts/connect-proxy.py`, `podssh:scripts/box/proxy.py` (CONNECT only to 443, 80, 8443; names only; non-public refused), `podssh:scripts/box/seccomp.json`, `podnet:scripts/restricted-host.sh` (denies `bind(AF_INET)` by preload), `podnet:scripts/v6only/`. | Observed by reading. These are ready fixtures for #47 phase 2 "a test on a proxy-only host" |
| Self-hosted runner use | None. All 28 `runs-on` lines of podnet name GitHub-hosted `ubuntu-24.04` or a matrix runner; podssh uses `ubuntu-24.04`, `ubuntu-24.04-arm`, `windows-2025`, `macos-15`. `grep self-hosted` in both `.github` trees finds only `--deny-self-hosted-runners`. | Observed: `grep -h runs-on` counts |

- Source: both `release-proofs.yml` run `gh attestation verify ... --deny-self-hosted-runners` on each release binary (`podnet:.github/workflows/release-proofs.yml:93-99`, `podssh:.github/workflows/release-proofs.yml:82`).
  Inferred: a release built by `release.yml` on a podrunner-run self-hosted runner would fail that proof. Only a release build matters here; ordinary CI is not checked this way.
- Documented: `podssh:docs/detach.md` section 1 holds a table of what ends a background process per CI system, read from the sources of `actions/runner` (`ProcessInvoker.cs`: kills the tree after a step; `JobExtension.cs`: kills processes whose environment holds `RUNNER_TRACKING_ID` at "Complete job"), Jenkins and GitLab. Marked `[src]`. I did not re-read those upstream sources.
- Neither plan file nor #47 cites this: the plan says "Not read: ... the podssh docs beyond the README" (`podrunner-plan.md` line 337).

## 4. Question (d): the A-N and A-S asks of #35, and 25 State lines checked

The State lines are "what cfb's research saw on 2026-11-09 at podnet `c52b8e0`, podssh `7e38d5b`" (#35 body). Neither SHA is available locally. I check each line against the head.

### 4.1 State lines verified against the code

| # | Ask and the State line | Verdict | Evidence |
| --- | --- | --- | --- |
| 1 | A-N-01 "live steps use quick tunnels only" | verified | Observed: `podnet:.github/workflows/live.yml` has no named-tunnel step (grep `named`, `credentials-file`, `--token` finds one `autostart` secret test at line 347). Documented: `podnet:docs/decisions.md` 2026-11-09 "Test accounts" |
| 2 | A-N-02 "acks the version and applies nothing" | verified | Source: `podnet:src/edge_h2.rs:807-820` returns `{"lastAppliedVersion":N,"err":null}` after reading the push |
| 3 | A-N-03 "`--metrics` is a usage error" | verified | Source: `podnet:src/cli/argv.rs:30`. Observed: exit 64 |
| 4 | A-N-04 "exists (beta), schema not frozen" | verified | Observed: `doctor --json` prints `schema: 1`; `--quick` absent (`doctor --help` shows no such flag). Documented: `podnet:TODO/fixes.md:3555` "No test pins the key set" |
| 5 | A-N-05 "last release is `cfrs` v0.3.0-beta.12; 'no release now'" | stale (refuted at the head) | Observed: the forge lists `v0.4.0-beta.1` published 2026-11-10T00:52:11Z (prerelease) with 8 binaries, `SHA256SUMS`, `SHA256SUMS.sigstore.json`, `provenance.sigstore.json`. `podnet:TODO/INDEX.md:70-71` T-022 and T-023 are done. The connector profile (T-218) is still open |
| 6 | A-N-06 "the key is ignored with no warning" | verified | Source: `podnet:src/config.rs:401` lists `access` as a known key; `grep` for `access`, `audTag`, `teamName` finds no reader |
| 7 | A-N-07 "missing in podnet" | verified for podnet, rest unverified | Observed: no `access tcp` code in `podnet:src` or `crates`. The `pod-egress` clause is podcrates scope, not read |
| 8 | A-N-08 "blocked: T-003 waits for a captured exchange" | verified | `podnet:TODO/INDEX.md:51`; `podnet:docs/decisions.md` 2026-11-04 (#149) |
| 9 | A-N-11 "`autostart` writes user units and cron lines only" | verified | Source: `podnet:crates/podnet-core/src/autostart.rs:327-332`; Observed: no `service` command |
| 10 | A-N-12 "exists (beta); T-087 planned" | verified | `podnet:README.md:1076-1121`; `podnet:TODO/INDEX.md:135` T-087 open. Extra: renewal (T-161) and alternate hosts (T-200) blocked on podcrates (`INDEX.md:209,248`) |
| 11 | A-N-13 "`podnet pod` and `warp proxy` rules exist; no TLS end, no placeholder" | verified | Observed: rules and `--default-action` ran. `grep -i placeholder` finds hits only in `share/upload.rs` and `vnet/policy.rs`, not in a proxy path |
| 12 | A-N-14 "missing" (pool in ingress) | verified | Source: no pool or health key in `podnet:src/config.rs` |
| 13 | A-N-15 "missing" (bench) | verified | Observed: `podnet bench` exits 64 |
| 14 | A-S-01 "Refused with 64 (OBSERVED)" | verified | Observed: `each`, `run`, `job` with `node://x` each exit 64 |
| 15 | A-S-02 "A user unit, a crontab line or a LaunchAgent only" | verified (and a Windows task) | Source: `podssh:crates/podssh-cli/src/node_install/mod.rs:116-124` (systemd, launchd, cron; `schtasks` on Windows); `text.rs:124-152` |
| 16 | A-S-03 "Renewal in `v1.1.0-beta.2` (role only); legs unreleased; a contract relay ends a node at 72 h" | verified | `podssh:docs/STATUS.md:121` (renewal in beta.2) and `:944` (the relay-member legs `unreleased`); `podssh:docs/cli.md:3868-3870` (the default relay renews none) |
| 17 | A-S-04 "One switch for each service; n0 by default (DOCUMENTED)" | the no-`PODSSH_PUBLIC` half verified; "n0 by default" unverified | `grep PODSSH_PUBLIC` finds only `podssh:TODO/mesh.md:11611`. T-711 blocked (`INDEX.md:744`). Individual switches exist (`PODSSH_OFFLINE`, `PODSSH_DIRECT`, `PODSSH_DERP`, `PODSSH_PIPING` in `podssh man environment`) |
| 18 | A-S-05 "`PING_EVERY` is a constant of 10 s (`session/pump.rs:62`)" | verified at the exact line | Source: `podssh:crates/podssh-relay/src/session/pump.rs:62` `pub const PING_EVERY: Duration = Duration::from_secs(10);` |
| 19 | A-S-06 "Parts exist: `relay spec`, live tests, `scripts/relay-measure.py`" | verified for `relay spec`; `relay conform` absent | Observed: `relay --help` lists `spec` with `--document`; no `conform`. Live tests and the script not run |
| 20 | A-S-07 "TARGET is `HOST:PORT` or `udp:`" | partly | Observed in `podssh node --help`: `--carrier` and `--shares` exist; `grep ctl=unix-connect` in `crates` finds nothing (T-714 open). `pipe unix-connect:PATH` exists (`positionals.rs`), a client side the State line does not mention |
| 21 | A-S-08 "A net file (T-429, `v1.0.1`); URL lists only in the relay role (T-646, unreleased)" | verified | `podssh:docs/STATUS.md:141` marks `relay serve --mint keys:https://URL` `unreleased`; `INDEX.md:503` T-429 done |
| 22 | A-S-10 "`podssh ping NAME` races the roads and names the winner (T-435)" | verified | Observed: `podssh ping --help` first line "with node://NAME, each of its roads and the one that answered first" |
| 23 | A-S-11 "Parts exist (`pipe http-listen:`, `exit --listen`)" | verified, with one narrowing | `podssh:docs/cli.md:4787-4845`: `http-listen:PATH` is a CONNECT proxy on a Unix socket, other side `relay:%h:%p`, `tcp:%h:%p` or `ssh:`. `podssh exit --listen ADDR:PORT` takes a loopback address only (Observed in help), not a Unix socket. Documented limit: curl cannot use a Unix-socket proxy (`cli.md` 4833) |
| 24 | A-S-13 "`node --exit` for net members over the Noise channel (T-431)" | verified | Observed: `--exit` and `--routes` rows in `podssh node --help` |
| 25 | A-S-14 "Tokens only (`X-Relay-Token`)" | verified | Observed: `grep X-Relay-Lease` finds nothing in `crates`; `X-Relay-Token` in `podssh-relay/src/pair.rs` |
| 26 | A-S-16 "Refused (`ssh/hop.rs:69`, `mcp/allow.rs:120`)" | verified, lines drifted | The refusal is at `podssh:crates/podssh-cli/src/ssh/hop.rs:71` (function from 69) and `podssh:crates/podssh-cli/src/mcp/allow.rs:121` |
| 27 | A-S-12 "Separate: a vendored `iroh-relay`" | verified | Observed: `podssh:vendor/iroh-relay`; T-720 open |

Not verified (could not): A-N-09 ("UNKNOWN"), A-N-10 (the 37 and 22 percent figures), A-N-16, A-S-02 "MEASURED, run 41066805844" (a CI run I did not open), A-S-09 and A-S-15 (not read), the "(OBSERVED)" tags on lines that I re-observed only in part.

### 4.2 Status of each A-N and A-S ask at the head

Mapping from `podnet:TODO/PROGRESS.md` item 12 and podssh#174 (COORD-007). "Done" means the code exists and a test or build shows it.

- Done at the head: none of the P0 and P1 asks is done end to end. Parts that exist: A-N-04 (schema 1 exists, not frozen), A-N-11 (user units only), A-N-12 (relay node, no renewal), A-N-13 (rules and flow log, no TLS end), A-S-03 (renewal in the relay role), A-S-10 (`ping` races roads), A-S-11 (`pipe http-listen:`), A-S-13 (`node --exit`).
- Stale State lines: A-N-05 (release exists, 2026-11-10), A-S-16 line numbers (two lines off).
- Stale issue state on GitHub: #353 open but T-154 and T-155 done; #355 open though T-157 (default-action, flow log) is done and T-158 (`--learn`) is not (`podnet:TODO/issues.md:55-57`).
- Open entries (podnet): T-216, T-217 (doctor schema and `lanes`), T-218 (profile), T-219, T-220 (blocked), T-221, T-222. Open or blocked entries (podssh): T-708 to T-731 (`podssh:TODO/INDEX.md:741-764`); T-711, T-719, T-722, T-727, T-729 blocked.
- Held by ruling: podnet's tunnel set (A-N-02, A-N-03, A-C-01, A-C-02 and six connector asks) waits for the first crates.io release (`podnet:docs/decisions.md` 2026-11-09). Betas allowed, no crates.io. podnet stays Linux only (A-X-12 not for podnet; `decisions.md` 2026-11-09 "Platforms").
- A-S-11 has no podssh entry. COORD-007 routes it to `pod-broker` (podcrates T-099).

### 4.3 Name collision

Four record numbering schemes meet. `T-100` is podcrates' TLS end of `pod-broker` (`podcrates:TODO/INDEX.md:149`) and also podnet's router config file (`podnet:TODO/INDEX.md:148`). `T-102` is podcrates' `pod-unit` and also podnet's wormhole mailbox take (`podnet:TODO/INDEX.md:150`); `T-103` is podcrates' `pod-control` and podnet's file-transfer protocol (`INDEX.md:151`). #47 uses bare `T-100`, `T-102` to `T-106` for podcrates and `T-109` for the hold research (podcrates T-109), where podnet's own T-109 is `warp exec --detach` (`podnet:TODO/INDEX.md:157`). The plan writes "podvm A-V-02 (T-109)", which is the podcrates research entry, not a podvm entry (podvm numbers are T-16xx).

## 5. Question (e): hardcoded assumptions in #47 and the plan, and the alternatives that already exist

### 5.1 What #47 and the plan fix about egress and the control plane

Quotes from the #47 body (read from the forge) and the plan:

1. "podnet, `pod-proxy` and `pod-broker` give the way out through a proxy." (answer in short)
2. "The JIT mint over `pod-proxy` and `pod-tls`" (phase 1). "`podrunner` hosts `pod-broker`" (phase 2). "The runner host's own egress through `podnet` or `pod-proxy`; a test on a proxy-only host" (phase 2).
3. Plan section 2.1: "Network: outbound TCP 443 only, to `github.com`, `api.github.com`, `*.actions.githubusercontent.com` ..." and section 3: "Proxy-only egress ... use `podnet pod` or `pod-broker` as the only way out".
4. Plan section 3 states the control plane as GitHub's: the long poll to `broker.actions.githubusercontent.com`, "JIT mint ... the minting credential stays on the host", a GitHub App with the least permission.

The single paths this builds in (Inferred from those quotes plus the sources below):

| Assumption | Where it breaks, with evidence |
| --- | --- |
| One path: job to `pod-broker` to one upstream CONNECT proxy | `pod_broker::upstream` takes `Option<Proxy>`: one proxy or direct (`podcrates:crates/pod-broker/src/broker.rs:183-191`). podnet-netns takes a chain (`-x` repeated). The broker dials a resolved IP, not a name (`podcrates:crates/pod-broker/src/serve.rs:273-287`, `Target::Addr`) |
| The upstream proxy accepts CONNECT to an IP literal | Observed in the docs: the measured sandbox resets CONNECT to an IP literal (`podnet:docs/target-environment.md`, measured 2026-11-01). podssh sends the name unresolved on purpose (`podssh:docs/target-environment.md`, "A proxy can answer differently for each name"). podnet logs "an egress that takes names only refuses the address" (Observed log line). Inferred: a broker that resolves once, checks each address and dials the address fails on such a host. R1 should confirm against the broker tests |
| The host can resolve names, or DoH by IP works | The measured sandbox has no resolver (port 53 refused) and a proxy that refuses DoH by IP (`target-environment.md`). podnet resolves by `pin`, system, DoH by IP at 1.1.1.1, 1.0.0.1, 8.8.8.8, 8.8.4.4, then by name (`podnet:README.md:1032-1036` and the Observed log line). Those four addresses and Cloudflare and Google as resolvers are hardcoded third parties (`podnet:SECURITY.md` "What Cloudflare can see") |
| Proxy on port 443 only | The measured proxy allows 443, 80, 8443 only; 22 and 25 give 403 (`podssh:docs/target-environment.md`). A runner host that needs SSH to git (plan item "git over SSH") needs `warp ssh-config`, `podssh pipe ssh:` over 443, or a relay (`podnet:README.md:834-860`) |
| The broker or a proxy listens on loopback TCP | One measured host forbids `bind(AF_INET)`; `bind(::1)` gave `EAFNOSUPPORT`. podnet handles it with unix listeners and `net doctor` rows (README "Virtual network"). A held payload under the seccomp pod can reach any host loopback listener (limit 2 in section 1.2) |
| TLS to the proxy path is not inspected | A proxy that terminates TLS breaks MASQUE and the edge legs (README line 21), and `warp up` writes a merged `warp-ca.pem` when `SSL_CERT_FILE` names a small bundle (README "The CA bundle"). A TLS end in the broker (T-100) adds a second inspecting layer on a host that already has one |
| The runner reaches only GitHub | Neither repo lists the GitHub Actions hosts. podnet's `doctor --agent` probes `github.com:443` (git row) and docker pull, not `api.github.com`, `*.actions.githubusercontent.com`, `*.blob.core.windows.net` or `ghcr.io` (Observed: row `target` values; `src/doctor` grep finds no Actions host) |
| The default relay is reachable and fit for bulk | The relay is one Cloudflare Worker of the owner (`podssh:README.md`, `podnet:SECURITY.md`). Limits: 64 MiB and 12 h per session, 180 s idle, public targets only, 72 h pairs, no renew on the default contract. Not a path for cache or artifact traffic. One node on it is a single point for operator reach |
| One GitHub as control plane and one credential holder | #47 phase 4 lists other forges, so the forge is not single by intent. The mint credential on the host is single by design. The long poll of 50 s (plan 2.1) against a relay idle cut of 180 s is only relevant if the poll rides a relay road; it does not today |
| A single host class: Linux x86_64 or arm64 with glibc | The plan says the official runner runs on x64, arm, arm64 only. podnet is Linux only (`decisions.md` 2026-11-09). podssh covers Windows and macOS as operator clients. The plan's claim "podnet, podssh and podvm already ship static binaries for x86_64, aarch64, armv7, riscv64, i686, loongarch64, powerpc64le" holds for podnet and fails for podssh (no i686, loongarch64, powerpc64le in `podssh:.github/workflows/release.yml`). podvm not checked here |
| Hold by seccomp pod is a security boundary | Documented by the repo itself as a redirection, with the holes in section 1.2 (unix, vsock, netlink, loopback, inherited fds). `pod doctor` exits 3 only when no backend runs; there is no "refuse unheld" mode for a missing `/proc/<pid>/mem` write |
| A second egress implementation is needed in podrunner | `podnet-netns` already holds, rules, logs and chains. The path to share it is the `pod-hold` proposal (podnet#414), which podnet accepts only after its own seccomp fixes and with a cut: the loop, the memory reads, the fd table and the reply stay generic; rules, routes and the CLI stay in podnet-netns |

### 5.2 Alternatives that already exist in these repos (paths)

Unix-socket holds and listeners:

- `podnet pod -x socks5://unix:/PATH` or `http://unix:/PATH`, UDP in the stream (command `0x83`): `podnet:crates/podnet-netns/README.md` (Options, "UDP in the stream"). The hand-off installs the unix-socket upstream as the program's descriptor with no relay (`podnet:crates/podnet-netns/src/handoff.rs`).
- `podnet pod --serve unix:/PATH`: a SOCKS5, CONNECT and UDP front door through a chain.
- `podnet warp proxy --listen unix:/PATH` (each listener bound on its own, `--strict`), `--proxy-user`, `--max-connections`: `podnet:README.md:705-742`.
- `podnet net proxy` (default `unix:/tmp/podnet-vnet.socks`, mode 0600), `net tailscale` (unix SOCKS door to a tailnet), `net socksify`: `podnet:README.md:915-966`; listener table in `podnet:SECURITY.md` "What listens".
- `podssh pipe http-listen:PATH` (CONNECT proxy on a Unix socket, optional `--proxy-auth-file`, 32 clients, 403 with the refusal words): `podssh:docs/cli.md:4787-4845`. `podssh session` socket: `podssh:crates/podssh-cli/src/session/wire.rs`.

SOCKS and CONNECT sources and sinks:

- `podnet warp proxy` (SOCKS5 and HTTP CONNECT in one listener, UDP ASSOCIATE), `--l4` (HTTP/3 CONNECT to the WARP L4 door), `--dns`.
- `podssh ssh -D PORT` (SOCKS5), `podssh exit NAME --listen ADDR:PORT` (SOCKS5, SOCKS4a, CONNECT whose connections start at an exit node of the owner's net), `podssh node --exit --routes`.
- `podnet --proxy 'http+exec:CMD'` or `socks5h+exec:CMD`: a proxy at the stdin and stdout of a command (`podnet:README.md:1008-1015`). `podssh node --carrier exec:CMD` and `fd:N`.

Resolution:

- podnet: `--resolve HOST=IP` pins, system resolver, DoH by IP, DoH by name (Observed log line), the fake-IP pool in pods. podssh: `--relay-addr HOST=IP`, DoH by IP literal, pinned addresses (`podssh:docs/target-environment.md`).

Relays and other roads:

- `podnet --transport relay` (`http2+relay`), `warp ... --relay`, `relay pair/node/connect/stop`: `podnet:README.md:403-455, 1076-1121`.
- Self-hosting the relay: `podssh relay serve` (`--mint keys:FILE`, `keys:https://URL`, `net`; `--quota`, `--usage`, `--pairs`, `--deployment NAME=HOSTS` with a signed list of hosts, `--sshd ADDR:PORT` so one port serves the relay and an sshd). `podssh:docs/relay-operator.md`, `docs/relay.md`.
- A user-run forward proxy over TLS as the only road: `podssh ssh --via https://HOST` (HTTP/2 CONNECT or HTTP/1.1, Basic from a file): `podssh:docs/cli.md:1312-1361`. Also `--shadowsocks`, `--vless`, `--wstunnel`, `--chisel`, `--trojan` (`cli.md` headings at lines 1220, 1363, 1434, 1500, 1526), node carriers over iroh, DERP, Tailscale (`podssh ts`, not in release builds), pinggy, serveo, piping, OpenTunnel (`podssh:crates/podssh-cli/src/node_*.rs` file names). The Tor crate `podssh-tor` is a feature not in release builds.
- WARP as a TCP 443 egress (MASQUE over HTTP/2): `podnet warp proxy|run|exec|tun`. Third-party limits: about 10 new devices in 90 minutes before HTTP 429 (`podnet:README.md:700`, "New devices"); a TLS-terminating proxy breaks it.
- An in-pod default-deny and a flow log: `--default-action reject`, `--flow-log` (Observed). A learn mode and a fail-on-violation gate are open (T-158).

Test fixtures for "a proxy-only host": `podnet:scripts/test_in_box.sh`, `podnet:scripts/box/proxy.py`, `podssh:scripts/box/proxy.py`, `podnet:scripts/connect-proxy.py`, `podnet:scripts/restricted-host.sh`, `podnet:scripts/v6only/`.

## 6. Claims of #47 and the plan about podnet and podssh, checked

| Claim | Verdict | Evidence |
| --- | --- | --- |
| podnet gives "the way out through a proxy" with `podnet pod` (seccomp backend that sends all traffic of a program to a proxy, no root) | verified, with limits | Observed run; limits in section 1.2 |
| `pod-broker` is the way out with podnet | partly | The broker is a library that a product hosts; it has no podnet or podssh consumer (Cargo manifests). Its README composes it with `podnet pod -x http+unix:PATH` |
| "No inbound port: podssh/`pod-relay` give reach to the host for an operator. Exists" (plan table) | verified with limits | Node mode exists. Renewal only on podssh's own relay role; the default relay renews none; 64 MiB and 12 h sessions; T-710 (months of reach) open |
| "podssh has Windows builds; podnet is Linux only" (plan) | verified | `release.yml` windows x86_64 and aarch64; `podnet:docs/decisions.md` 2026-11-01 and 2026-11-09 |
| "podnet, podssh and podvm already ship static binaries for ... i686, loongarch64, powerpc64le" (plan) | refuted for podssh, verified for podnet, unverifiable here for podvm | `podnet` release assets (8 targets); podssh `release.yml` (4 Linux, 2 macOS, 2 Windows targets) |
| T-102 `pod-unit` "no token in the unit file" is new work | partly refuted | Two renderers already keep tokens out of units (section 2.2); neither hardens. T-709 and T-219 each plan the system unit and hardening and wait for `pod-unit` |
| T-103 `pod-control` is new | partly | No tool here has `version status drain reload stop`. A socket protocol with `schema` first exists in `session/wire.rs`; `pod-broker` has `control_line` (R1) |
| JSON report "with its schema in `schemas/`" | convention gap | podnet and podssh each stamp an integer `schema: 1` in the document with no schema file in either repo (`find` for `*.schema.json` is not done; I saw none referenced). `pod-broker` has `schemas/broker-policy.v1.schema.json` (R1) |
| "`podrunner` ... `status --json`, `drain`" | precedent exists | `podnet ps --json` (Observed keys), `podnet stop`, `podnet wait`; no `drain` anywhere |
| Phase 2 "test on a proxy-only host" | tooling exists | Section 5.2 fixtures |
| "The cfb tunnel set ... is not on this path" | consistent | `podnet:docs/decisions.md` 2026-11-09: the tunnel set waits for crates.io |
| "Betas are allowed; no crates.io" | consistent | `podnet:docs/decisions.md` 2026-11-09; podnet `v0.4.0-beta.1` published 2026-11-10 |

## 7. Issue lists (podnet 336, podssh 168): what I read

Method: I filtered titles by keywords (CI, runner, proxy, broker, netns, seccomp, router, relay, node, unit, systemd, install, schema, exit, json, control, agent, doctor, COORD, cfb) and read bodies of #351, #353, #355 (podnet). I did not read the comments of any issue, and I did not read the other bodies. A title is not a verdict.

Relevant open podnet issues (titles, Observed in the list): #355 pod default-deny, flow log, learn; #353 seccomp fuzz and differential check (stale: done in the record); #352 netfilter in the namespace; #351 seccomp relay copies in user space; #369 veth wire when no TUN; #371 to #376 router (health, flow terminators, member control, rule check, group policies); #377 declare the addressing mode on the Transport trait; #363 the ssh relay leg hardcodes one relay, one username and one URL rule; #360 the relay budget is a compile-time constant for one deployment; #359 relay alternates are chosen once at start; #358 a relay node pair cannot be renewed; #397 the reverse road reaches one relay host; #384 `net proxy` handshake has no deadline; #368 a WARP device saved by one uid cannot be resumed by another; #366 tunnel has no `--masque-endpoint`; #407 `warp status` reads every account type except `free` as WARP+; #222 crates.io workspace version; #413 COORD-006 (asks of podnet for podcrates); #419 COORD-011.
Closed and relevant: #354 reject action, #403 `agent status --json`, #402 detach child inherits `PODNET_DETACH_SUPERVISED`, #400 `share --json` ready line has no `schema`, #398 websocket road dials its relay directly (global proxy not applied), #386 `net proxy --forward` real dial goes straight to the socket, #379 the port of an `http://` proxy URL with no port is 8080 in podnet and 80 in podssh (settled as 80, `decisions.md` 2026-11-09), #249 exit codes (five paths answered 75, 1 or 69 where the table says 64 or 78).

Relevant open podssh issues: #177 COORD-010 (`pod-services` kinds proxy-list, proxy-node), #176 COORD-009, #174 COORD-007 (the entries of cfb's asks), #59 research: the forward road without the operator's relay, #18 transport backends (iroh/QUIC, TLS-over-TCP fallback, self-hosted relay, UDP-over-TCP).
Closed and relevant: #75 CONNECT listener (`pipe http-listen:`), #106 the third-party forwarder fallback should be opt-in ("hands the hostname to three services"), #127 `relay serve` keeps its capabilities after bind, #121 STATUS says the exit node works while the only release answers "unknown subcommand 'exit'" (a doc ahead of the release), #117 relay pair three-day expiry with no renew, #97 `--result-file` for cp, mv, run, sftp, #94 `each --json`, #10 and #9 `--help --json` and `doctor --json`, #11 `status` machine-readable.

## 8. Coverage

Read in full or in the parts named:
- podnet: `CLAUDE.md`; `README.md` lines 1-38, 640-1350; `SECURITY.md`; `docs/decisions.md`; `docs/HANDOFF.md`; `docs/target-environment.md` lines 1-60; `crates/podnet-netns/README.md` (all); `TODO/PROGRESS.md` lines 1-80; T-216 to T-222 entries; `crates/podnet-core/src/exit.rs` lines 1-80; `autostart.rs` unit text; `src/edge_h2.rs:796-822`; `seccomp/filter.rs`, `seccomp/mod.rs` (selected), `socket.rs` 1-60, `connect.rs` (grep and lines 170-215).
- podssh: `AGENTS.md` 1-80; `README.md` (all); `docs/cli.md` sections `--via`, `each`, `run`, `job`, node install and renewal, `net`, `pipe`, `mcp`, exit codes; `docs/target-environment.md` 1-60; `docs/detach.md` 1-60; `node_install/{mod,text,env}.rs`; `ssh/result.rs`; `session/wire.rs` 1-40; `TODO/serve.md` T-709; `TODO/INDEX.md` lines 741-764; release workflow targets; `docs/STATUS.md` lines 1-80 only.
- Ran (on local builds of both repos at the heads above): `--help`, `man exit-status`, `man --json` (keys), `doctor --json`, `doctor --agent --json`, `status`, `pod doctor` and `--json`, `pod` with `--default-action reject` and `--flow-log`, the probe of non-IP socket families and loopback inside the pod, `net proxy --detach` plus `ps --json` plus `stop --all`, `route`/`service`/`bench`/`control` existence, `each`/`run`/`job` with `node://`.
- Remote reads: #47 body, the release lists of both repos (podssh asset list returned 403 on two tries; I used `release.yml` instead), the issue lists of both repos.
- Cross-read, not my scope: `podcrates:crates/pod-broker/README.md`, `src/broker.rs`, `src/serve.rs` (grep and the lines cited), `podcrates:crates/pod-core/src/failure.rs`, `podcrates:TODO/INDEX.md` rows T-099 to T-109, `podcrates` crate list.

Not read or not done:
- `podssh:docs/STATUS.md` beyond line 80 and the matched rows; `docs/relay.md`, `relay-operator.md`, `mesh.md`, `reverse.md`, `design.md`, `carriers.md` (not read; the text of `docs/cli.md` and the help output stood in). `podssh:vendor/`, `fuzz/`, the 40-odd interop scripts, and the 60 workflows (counted `runs-on` only).
- `podnet:docs/protocol.md`, `virtual-network.md`, `maintenance.md`, `research/`, `reports/`; the WARP, edge and Cloudflare source beyond A-N-02 and A-N-06.
- `#36` (githome) and the comments of every issue; `podnet`/`podssh` issue bodies other than #351, #353, #355 (and #186, #185, #184, #183, #169, #168, #167, #164, #163, #162, #161, #160, #159, whose bodies were printed by mistake; I used none of them as evidence except noting #186).
- Not run: the official `actions/runner` or any CI payload under `podnet pod`; a CONNECT proxy that refuses IP literals against `pod-broker`; GitHub live runs; the CI run 41066805844 cited by #35; `podssh` releases `v1.1.0-beta.3` asset list.
- The #35 State SHAs are not in the shallow clones. State lines about the code at those SHAs are compared with the head only.
- Not verified here and passed on as the issue's claim: A-N-09, A-N-10 figures, A-N-16, A-S-09 and A-S-15 State lines.

## 9. Surprises (contradictions with #47, the plan, #35 or #36)

1. #35 A-N-05 State: "last release is `cfrs` v0.3.0-beta.12; 'no release now'". Observed: `v0.4.0-beta.1` was published 2026-11-10T00:52:11Z, signed (`SHA256SUMS.sigstore.json`, `provenance.sigstore.json`), 8 targets. `podnet:docs/HANDOFF.md` still says "No podnet release now" and "The newest release is `v0.3.0-beta.12` of cfrs", so that file is stale against `TODO/PROGRESS.md:14`.
2. Plan section 3, "Ephemeral hosts": "podnet, podssh and podvm already ship static binaries for x86_64, aarch64, armv7, riscv64, i686, loongarch64, powerpc64le". `podssh:.github/workflows/release.yml` builds x86_64, aarch64, armv7 and riscv64 for Linux, plus macOS and Windows. It builds no i686, loongarch64 or powerpc64le binary.
3. #47 "podnet, `pod-proxy` and `pod-broker` give the way out through a proxy". `pod-broker` dials a checked IP address (`podcrates:crates/pod-broker/src/serve.rs:273-287`). `podnet:docs/target-environment.md` (measured) shows a proxy that resets CONNECT to an IP literal and a host with no resolver. These two do not compose on that host class, by my reading. Not run.
4. #47 "The one missing piece is `pod-dockerapi`" and "`podnet pod` ... the way out". A pod held with `--default-action reject` still reaches host unix sockets, `AF_VSOCK`, `AF_NETLINK` and host loopback TCP (Observed). The seccomp pod is not a hold of "each connect of the payload" for those. A-V-02 and #47's "egress hold" phase are stated as a hold.
5. #47 phase 2 lists T-102 `pod-unit` as new. Two unit writers with the "no token in the unit" property already exist (`podssh:.../node_install/text.rs`, `podnet:.../autostart.rs`), they disagree on `Restart=`, and two further entries (podssh T-709, podnet T-219) plan the hardened system unit and wait for it.
6. #35 A-X-17 asks for one exit table. `pod-core` has one (T-101 done; 64 to 80). podnet uses 3, 75 as temp fail and 124 as timeout; podssh uses 255, 70 as not implemented and 75 as timeout. Neither tool uses the `pod-core` table. SIGHUP means stop in podnet and reload in podssh `relay serve`.
7. `podnet:TODO/issues.md:55` shows #353 open, but T-154 and T-155 are done and `fuzz/fuzz_targets` holds `seccomp_sendmsg`, `seccomp_sendto`, `seccomp_sockopt`. Issue text "none covers the seccomp notification handlers" is stale. #355 is open with T-157 done and T-158 open.
8. Input gap: `podrunner-spec/inputs/i47.md` is missing and `i47-comments.md` is empty. The brief names `i47.md`. I read #47 from the forge.
9. The plan (line 337) did not read the podssh docs beyond the README. `podssh:docs/cli.md` (6127 lines) already specifies a CONNECT proxy on a Unix socket, a self-hostable relay with mint policy and quotas, an exit proxy over the owner's net, a user-run HTTPS forward proxy road, and a JSON `--result-file` schema for `run`.
10. `release-proofs.yml` in both repos verifies attestations with `--deny-self-hosted-runners`. A podrunner host that builds a release would fail it (Inferred, not run).
