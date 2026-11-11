# R3: podcrates recon (issue #47 and `docs/research/ci/podrunner-plan.md`)

Machine date when read: 2026-11-10 (UTC), from `date`.
Repository read: `podcrates` at `ec37ed2` (shallow clone, one commit, 2026-11-10 09:26 UTC), workspace version 0.13.0
(`podcrates:Cargo.toml`). Last pushed tag: `v0.12.0` at `393158f` (Observed: forge mirror `tags` call).
Labels: Observed (read or ran), Documented (a doc says it), Source (code says it), Inferred, Unverified.

Input note. `podrunner-spec/inputs/i47.md` does not exist (only `i48.md`, `i35*.md`, `i36.md`, the plan). I read the body of #47
from the forge mirror (`issues/47`, 83 lines, `comments: 0`, `created_at` = `updated_at` = 2026-11-10T04:37:11Z). The local
`podrunner-plan.md` is byte-identical to `podcrates:docs/research/ci/podrunner-plan.md` (Observed: `diff -q`, no output).

## 0. The ten findings that matter most

See the end of the report for the same list in the reply format. Short form here:

1. The owner ruled "leave #47 alone" about 11 minutes after #47 was filed. #47 still says phase 0 "can start now".
2. `pod-guest` is not a microVM. It is a Windows/DOS QEMU guest with no network. `pod-microvm` is only a reserved name.
3. `pod-testkit` has no fake forge and no fake GitHub. T-106 says the fake GitHub API stays in the projects.
4. `pod-broker` has `load`/`drop`/`list`, job tokens and policy (T-099 done, 55 tests pass). T-100 (TLS end) has no CA-minting code to build on.
5. `-v` is not "not supported" in podvm: `podvm:crates/podvm/src/parity.rs:222` marks `run -v` as Degraded (works on one rung).
6. Q1 to Q7 in #47 are Q8 to Q14 in the record. Q1 to Q7 already mean other, ruled questions.
7. "Reserve them with the existing check" is wrong: the script reads names from `docs/design.md` and reserves nothing.
8. A live test on a GitHub test repository contradicts the ruling "Test accounts (Q-A9): Not now".
9. `pod-supervise` kills one pid, with no process group, no tree kill, no rlimit and no run timeout. No podcrates entry owns it.
10. #47 inverts cfb's first backend (Incus container) and drops githome's x86_64/aarch64 scope. `podci` (an org project) overlaps `podrunner`.

## 1. Question (a): inventory of crates (verified)

Method. For each crate I read `README.md`, `src/lib.rs` (module and `pub use` list) and the tests directory. Test counts are
`grep` counts of `#[test]` and `#[tokio::test]` attributes (Observed, not a run result). The only runs I did:
`cargo test --locked -p pod-broker -p pod-testkit` (toolchain `stable` 1.99.1, because `rust-toolchain.toml` pins `1.99`
and that toolchain is not installed here). Result (Observed, exit 0): pod-broker 8 unit + 8 + 2 + 6 + 7 + 8 + 6 + 4 + 6
integration = 55 passed, 0 failed; pod-testkit 5 passed. `cargo todo check` printed
"the record agrees: 150 entries: 55 open, 3 partial, 0 blocked, 92 done." (exit 0).
`python3 -I scripts/check-design-paths.py` printed "ok" (exit 0).

"Reuse" column: Y = directly usable by a podrunner design, P = partly, N = no direct use, G = gap (named).

| Crate | Public API as it stands (Source: README and `lib.rs`) | State | Reuse for podrunner |
| --- | --- | --- | --- |
| `pod-broker` | `Policy`/`Mode`/`Rule`/`Compiled`/`Verdict`; `DenyList` (`fixed`, `host_addresses`, `parse`, `read`, `check_*`); `target::{parse_host, parse_authority, parse_absolute, normal_path}`; `Broker::{new, with_audit, with_tunnel_end, load, drop_job, list, handle, serve_tcp, serve_unix, control_line}`; `Load{job, token, ttl, policy}`, `JobInfo`, `Token`; `control::{bind_unix, serve_control}` (Unix only); `upstream(app, proxy, resolve)`; `Audit`, `Line`, `JsonLines`, `Tracing`, `Memory`; `TunnelEnd`, `Tunnel`, `Splice`, `Counts`. 12 source files, 2,341 lines, 55 tests. | Exists, T-099 done 2026-11-09. TLS end (T-100) open. | Y for per-job token, policy, audit, drop. G: no TLS end, no CA minting. |
| `pod-supervise` | `Settings`; `create`, `start`, `kill`, `stop`, `wait`, `remove`, `list`, `get`, `matching`; `logs`, `follow*`; `table::{Container<X>, State, Table, Any}`; `launcher::{Launch, Payload, start, signal, wait}`; `lock::Lock`. 10 files, 2,116 lines, 23 test attrs, `tests/lifecycle.rs` with real payloads. Linux only. | Exists, T-060 done, taken by podvm. | P: lifecycle table, pidfd wait, logs. G: signals go to one pid (`podcrates:crates/pod-supervise/src/launcher.rs:351` `sys::kill(pid, sig)`), no group kill, no rlimit, no run timeout, no report. |
| `pod-oci` | `Reference`, `Digest`/`Verifier`, `spec::*` (index, manifest, descriptor), `Platform`, `credentials::{Credential, Logins, Files}`, `transport::Policy`, `trust`, `registry::Client` (`manifest`, `tags`, `blob`), `pull::{resolve, Blobs, fetch_parallel}`, `layout`. Linux only. 19 files, 4,360 lines, 95 test attrs, own registry fixture (`tests/common/registry.rs`). | Exists, T-056 done. | Y for `images/create` (pull into a caller store). N for push, build, commit: none exist. |
| `pod-extract` | `extract(&Settings, image, &[Layer], &Place)`, `Layer`, `Place`, `Extracted`, `apply`, `safety`, `sidecar`, `whiteout`. No `tests/` dir, 54 unit attrs. | Exists, T-057 done. | Y (unit root from image layers). |
| `pod-rootfs` | `complete(rootfs, &Settings, &mut probe) -> Report`, `Names`, `HostBundle`, `Fixup`, `Step`, `IDS`. 74 unit attrs. | Exists, T-058 done. | Y (root completion: `/dev`, resolv.conf, CA bundle, passwd). |
| `pod-enter` | `Plan`, `Names`, `RootDir`, `spawn*`/`run*` (chroot, namespace, userland, ladder), `userland::{Volume, Maps}`, `abi::GuestRoot`, `ladder`, `memfd`, `fuse`, `device`, `binfmt`. 37 files, 8,959 lines, 156 test attrs. | Exists, T-059 done; T-144 (volume maps) done in v0.12.0. | Y for entering a payload. P for volumes: `Maps` gives guest-to-host path rewrite for the userland rung only (podvm parity row). |
| `pod-hostprobe` | `run(&Settings, ..) -> Findings`, `probes::PROBES` (66 rows), `select::{Rung, Selection}`, `machine::{assess, Profile}`, `supervise::assess`, `identity`, `binfmt`, `space`, `exec`, `sys` raw syscalls (`statfs` with `f_flags`). 27 files, 7,063 lines, 113 test attrs. | Exists, T-055 done; T-145, T-148 done. | Y for "which tier can this host run" (a `podrunner doctor` input). |
| `pod-guest` | `Settings`, `Plan`, `Accel`, `argv`, `Request`/`Outcome`, `mailbox`, `stage`, `run`, `stop`, `provision`, `agent`, `dos`, `fat16`, `fetch`, `qmp`, `emulator`. UEFI q35/OVMF flavour with a `cmd.exe` agent, and a FreeDOS flavour. `-nic none`: no network (`podcrates:crates/pod-guest/src/plan.rs:104`, `:142`). No `tests/` dir, 66 unit attrs. | Exists, T-061 done. | N for a Linux microVM job. Only for a Windows or DOS guest. See finding 2. |
| `pod-identity` | `Identity`, `PublicKey`, `file::{Place, load, existing}`, `pins`, `allow::Allowlist`. Ed25519 only. | Exists. | N (could hold a node key, no role in the job path). |
| `pod-proxy` | Dial, race, HTTP CONNECT (Basic, Digest) and SOCKS5, `NO_PROXY` rules, HTTP/1.1 heads, HTTP client (GET and any method to http/https), blocking facade (feature `blocking`), STUN codec, SOCKS5 UDP. 39 files, 7,798 lines, 202 test attrs, 23 test files. | Exists. | Y for the JIT mint call and for the host's own egress. Redirects and keep-alive exist (T-076, T-077). |
| `pod-tls` | Trust and roots, client configs over the caller's provider, `server::{acceptor, server_config, certified, Served}`, ECH, pure provider, blocking handshake. No tokio with `default-features = false`. | Exists. | Y for client TLS. G: no certificate or CA minting in production code. Test certs are hand-encoded in `crates/pod-tls/tests/common/cert_for_test.rs` and `cert_encoding.rs`. `rcgen` and `x509` appear in no `Cargo.toml` (Observed: grep). |
| `pod-dns` | `Resolver`, pins, system, DoH JSON and RFC 8484 wire, HTTPS/SRV records. 70 test attrs. | Exists. | Y (resolver handed to `pod-broker` as `pod_core::Resolve`). |
| `pod-core` | `Dial`, `Resolve`, `Datagrams`, `Failure`/`Stage`/`Class`, `Event`/`Observer`/`Report`, `Metered`, `files`, `entropy`, `clock`, `text`, `names`. T-101 added classes Revoked, Policy, Unsupported and one JSON log schema. | Exists. | Y (error and exit-code vocabulary). |
| `pod-services` | Data files, one per service (51 in `services/`), `get(id)`, `of_kind(kind)`, overlay. No GitHub Actions hosts: only `github-ssh.toml`, `ghcr-registry.toml` and `docker-hub-registry.toml` mention GitHub-side hosts (Observed: grep for `actions`, `api.github`, `blob.core` found only a `cloudflared` API source and `github-ssh`). | Exists. | G: the plan lists GitHub hosts in prose. The ruling "Services crate" says third-party hosts are data with a watch. |
| `pod-testkit` | `need_loopback!`, `need_localhost!`, `need_userns!`, `need_tool!`, `need_root!`, `host::*`, feature `screen` (a VT terminal oracle). 3 files, 395 lines. `Cargo.toml` description says "and the stand-ins of the services that the tests need". | Exists. No stand-in exists. | G: no fake GitHub, no fake forge, no fake registry. See section 1.2. |
| `pod-todo` | The checker of `TODO/` (`check`, `set`, `counts`, `next`, `remap`). No dependency. | Exists. | N (record tool). T-105 would add an acceptance registry. |
| `pod-ws` | WebSocket frames, session, relay client over TLS, HTTP/2 extended CONNECT, cleartext, server handshake, bridge. | Exists (T-016 partial, v0.13.0 notes written, tag not pushed). | N for the runner path. |
| `pod-term` | Line discipline and output grammar for a session with no pty. | Exists. | N. |
| `pod-gsr` | GSRN relay client over TLS with SRP (static OpenSSL), blocking face. | Exists (T-131 open on Windows ARM64). | N. |

Not crates yet, per README and `docs/design.md` section 4: `pod-quic`, `pod-session`, `pod-e2e`, `pod-wormhole`, `pod-netstack`,
`pod-wg`, `pod-relay`, `pod-transit`, `pod-warp`, `pod-cloudflare`, `pod-derp`, `pod-piping`, `pod-opentunnel`, `pod-acme`,
`pod-egress`, `pod-relay-serve`, `pod-iroh`, `pod-tor`, `pod-ts`, `pod-microvm`, `pod-sud`, `pod-confine`, `pod-netlab`, `pod-pcap`,
`pod-sandprobe`, `pod-unit`, `pod-control`, `pod-release` (Observed: `ls crates/` shows 19 directories).

### 1.1 `pod-broker`: the per-job control surface and what T-099 and T-100 mean

Observed (Source: `podcrates:crates/pod-broker/src/jobs.rs`, `control.rs`, `broker.rs`, README).

- A job is an id, a token, a TTL and a policy. `Broker::load(Load)` adds or replaces it. `Broker::drop_job(job)` removes it and ends
  each of its open connections (the entry's `watch::Sender` is dropped, `jobs.rs` `Table::remove`). `Broker::list()` shows
  `JobInfo{job, mode, rules, expires_in, open}`. TTL is 1 s to 7 days (`MAX_TTL`).
- A token is printable ASCII, 16 to 4096 characters, kept only as SHA-256, one token for one job, wiped on drop. The job sends it as
  Basic password or `Bearer`.
- The control is JSON lines on a Unix socket (mode 0600, peer uid check with `geteuid`), ops `load`, `drop`, `list`, every answer
  `"v": 1`. `control` is `cfg(unix)` only. There is no Windows named pipe.
- Policy v1 (`schemas/broker-policy.v1.schema.json`): mode `allow` or `open`, rules `host` (name, `*.NAME`, address), `path` prefix,
  `ports`, extra `deny`. Fixed denies that no policy lifts: metadata addresses and names, private, CGNAT, loopback, link-local,
  multicast, reserved, port 25.
- The audit is one JSON line per connection, `load` and `drop`, with no path, query or header.
- The "drop" is the drop of a broker job (policy plus token plus connections). It does not stop or destroy a unit. A podrunner
  slot must call it and destroy the unit itself.

What T-099 and T-100 mean (Documented: `podcrates:TODO/fleet.md:5` and `:57`):

- T-099, P1, L, M8, done: the broker core above, in the library form. Done text: "55 passed (8 unit, 47 in `tests/`), 30 runs in a
  loop with no failure". I reproduced 55 passed once (Observed).
- T-100, P1, L, M8, open: the TLS end with a CA for each job, placeholder-credential swap in header, Basic, query and body, put the
  placeholder back in answers, scrub. Start condition "T-099 is done". The seam exists: `TunnelEnd` trait with
  `reads_requests()` and `run(tunnel, client, upstream)` (`src/tunnel.rs:60`). The default `Splice` copies bytes.
- Consequence for #47 phase 0 (Inferred from the two lines above plus the CA search): the capture "through `pod-broker` with a TLS end
  (T-100)" needs T-100 first. #47 offers "or a CA proxy" as the alternative, which is a tool outside podcrates and not named.
- A broker `Policy` with a `path` rule is refused on CONNECT (`policy:path-needs-tls-end`) until a `TunnelEnd` reads requests
  (Documented: ruling C29, README "The policy").
- The broker is HTTP proxy only. It has no job-source notion, no GitHub knowledge, no slot or unit notion.

### 1.2 `pod-testkit`: which stand-ins exist

Observed. `podcrates:crates/pod-testkit/src/` holds `host.rs`, `lib.rs`, `screen.rs`. No fake GitHub, forge, relay, edge, registry or
control plane. `lib.rs:13` says a future stand-in "speaks bytes and HTTP only, and takes no type of a pod crate". The word
"stand-in" appears in no testkit source other than that line.

Where fakes live today (Observed: grep for `fake` and `stand-in`):
- `podcrates:crates/pod-oci/tests/common/registry.rs`: a registry fixture (used by `registry_fixture.rs`, `proxy_pull.rs`). Useful for a
  `pod-dockerapi` `images/create` test, but it is test-private, not in testkit.
- `podcrates:crates/pod-broker/tests/common/mod.rs`: in-memory far end, counting resolver, a client over a pipe.
- `podcrates:scripts/names-proxy.py`: a CONNECT proxy for live tests.
- podssh has `scripts/fake-*.py` (9 files, Observed `ls podssh/scripts`). Not imported.

Planned (T-106, P2, M8, open, `podcrates:TODO/fleet.md:195`): stand-ins of the edge, the relay and the WARP API "as binaries". It says
"The fake GitHub API and the fake control plane stay in their projects. The HTTP base serves the fake forge job source of G-10"
(`:218`). Its start condition is "T-025, T-028 and T-029 moved their stand-ins to `pod-testkit`" (`:230`). Those three are M2 and M3
entries, all `open` (INDEX). So T-106 cannot start before the relay and WARP crates exist.
`podcrates:docs/research/fleet/asks.md:124` (A-X-18) says "T-106; the new projects own their fakes".

#47 and the plan against this: #47 phase 4 says "a fake forge in `pod-testkit` (T-106; G-01, G-10)". The plan row P9 says "A stand-in
GitHub (`generate-jitconfig`, broker `sessions`, `message`, `acquirejob`, `renewjob`) and a stand-in forge, on loopback" in
`pod-testkit`. T-106 and A-X-18 put the fake GitHub in the new project, not in testkit. Refuted as stated for GitHub; the "fake
forge job source" reading is supported by T-106 line 218 only as "the HTTP base serves it". Also the testkit design rule forbids
a stand-in from taking a pod type, so a stand-in for the broker session protocol is allowed only as bytes and HTTP.

## 2. Question (b): crates named in #47 and the plan

Observed. `docs/design.md` section 4 and `TODO/*.md` searched for each name; crates.io read on 2026-11-10 with `GET /api/v1/crates/NAME`.

| Name | In #47 or plan | Exists in `crates/` | Planned in podcrates | crates.io on 2026-11-10 | Verdict |
| --- | --- | --- | --- | --- | --- |
| `pod-unit` | T-102 | no | T-102, M8, P1, M, open. Source: podssh `node_install/`, podnet `autostart.rs`. Scope: systemd system and user units, launchd, Windows services, no secret in a unit, uninstall leaves nothing. `design.md` row says Pod deps: pod-core | 200, 0.0.0, repo `PodUtils/podcrates` | Planned, matches #47 ("no token in the unit file") |
| `pod-control` | T-103 | no | T-103, M8, P1, M, open. JSON lines on Unix socket or Windows named pipe; `version`, `status`, `metrics`, `reload`, `drain`, `stop`; peer user check; schema; conformance test | 200, 0.0.0 | Planned. Overlaps in kind with the existing `pod_broker::control` (see 2.2) |
| `pod-release` | T-104 | no | T-104, M8, P1, M. Release index, CI writer, verifier (SHA-256, Sigstore bundle, minisign) | 200, 0.0.0 | Planned, matches |
| `pod-microvm` | plan section 3 and 7 ("A-V-08 `pod-microvm` (M7)") | no | M7 row in design.md (podvm#168); only a study entry T-062 (P3, L, open, start condition "crates of M6 released"); no crate entry | 200, 0.0.0 | Reserved name, no entry, no scope beyond one line |
| `pod-confine` | plan section 7 (A-V-01, T-108) | no | T-108, M7, P1, M, open: Landlock ABI 3+, seccomp deny filter, `no_new_privs`, rlimits, one profile. "The study and the crate go in one entry" | 200, 0.0.0 | Planned. It owns rlimits, which A-V-07 (kill tree, `--ulimit`) also wants |
| `pod-hold` | not named | no | held name in #1 comment 2026-11-09 and T-109 (research): the hold of each `connect` of a payload from `podnet-netns` | 404 | Held, overlaps `pod-broker` plus `podnet pod` in plan P7 |
| `pod-dockerapi` | #47 phase 3, Q7, T-146 | no | none. Only T-146 and Q10, Q14 in PROGRESS name it. No `design.md` row, no README row | 404 (free) | Invented by the plan. No clash on crates.io. No duplicate in TODO (grep `docker` finds only PROGRESS and fleet.md T-146) |
| `pod-actions-proto`, `-expr`, `-exec` | #47 phase 5, Q7 | no | none | 404 (free) | Invented by the plan |
| `pod-actions-workflow` | plan section 6 phase 5 optional | no | none | 404 (free) | Invented by the plan; not in #47 |
| `podrunner` | the product | repo `PodUtils/podrunner`, only a `LICENSE` | ruling "New projects", A-X-01, G-01 to G-12 | 200, 0.0.0, repo `PodUtils/podrunner` | Reserved, empty |
| `podci` | not named in #47 or the plan | repo `PodUtils/podci`, only a `LICENSE`, created 2026-11-09T09:21Z (4 hours before `podrunner`) | not in decisions "New projects" list. Listed in #1 table "Project names" | 200, 0.0.0 | Overlap, see 2.1 |

No name clash: nothing in `crates/` or `design.md` uses `pod-dockerapi` or `pod-actions-*`.

### 2.1 Duplicates of planned scope

- `podci` vs `podrunner`. Org profile README (`.github/profile/README.md`, local clone `repos/.github`): "podci: Run CI/CD actions on
  pods", status `soon`. Repo description (Observed, forge mirror): "Run CICD Actions on Pods", 0 KB, LICENSE only. The `podrunner`
  profile line: "Ephemeral CI runners on your own servers, one fresh unit per job". #1 reserves `podci` under "Project names ... reserved
  because cfb's asks, podcrates#35, name them" but `grep -i podci` over `i35.md`, `i35-comments.md`, `i36.md` finds nothing
  (Observed: no hits). `docs/decisions.md` row "New projects" (line 45) lists ten projects and not `podci`. #47 and the plan do
  not mention `podci`. Unverified: whether `podci` is the same product under another name, a CLI over `podrunner`, or a stale
  placeholder. A design must not build a second CI product beside it without asking.
- `podkey` vs the minting credential. #47: "the minting credential stays on the host, mode 0600, never in argv or logs". The org
  profile and A-X-08 describe `podkey` as "an in-memory secret agent with a time window". #47 does not mention it.
- `pod-hold` (T-109) vs plan P7 "all job traffic goes to the broker via `podnet pod`". T-109 is the study that decides between
  `pod-hold` and `pod-sud`; the plan assumes the answer.
- `pod-control` (T-103) vs `pod_broker::control` (exists, own JSON-lines `load/drop/list`). Two control protocols in one process
  unless T-103 absorbs the broker's. Not addressed in #47.
- `pod-confine` (T-108, rlimits) vs plan P5 ("`--ulimit`, kill the whole tree" as podvm plus `pod-supervise`). Two owners for rlimits.
- `pod-testkit` fake GitHub (plan P9) vs T-106 line 218 (stays in the project).
- Kill the tree and timeouts: the plan assigns "`pod-supervise` (T-060 family)" but T-060 is done, `grep -i "ulimit|whole tree|kill the
  tree|process tree"` over `TODO/`, `design.md`, `docs/research` finds only `asks.md:80` and the plan. No open entry owns it
  (Observed). The signal path is one pid: `launcher.rs:351` `let _ = sys::kill(pid, sig);` and `supervise.rs:131,152` `sys::kill(pid, 9)`.
- Docker CLI/API: no existing crate, entry or research page in podcrates. podvm's parity table has `network` and `volume` verbs as
  None (`podvm:crates/podvm/src/parity.rs:188`, `:189`) and `run --network` as None (`:224`); no Engine API socket entry in podvm
  `TODO/` (grep for `docker.sock`, `Engine API`, `system service` finds only `TODO/cli.md:335` about reading `$DOCKER_HOST`).

### 2.2 Things #47 proposes that already exist in some form

| #47 item | What already exists | Source |
| --- | --- | --- |
| "a token and a policy for each job" (`podrunner` hosts `pod-broker`) | `Broker::load`, `Token`, `Policy`, TTL, `drop` | `pod-broker/src/jobs.rs`, `broker.rs` |
| "A JSON report for each job, with its schema in `schemas/`" | `schemas/log.v1.schema.json` (one log schema, T-101), `schemas/broker-policy.v1.schema.json`. No report schema. | `podcrates:schemas/` |
| "status --json, drain" | planned as T-103 `pod-control` (G-08) | `fleet.md:129` |
| "the JIT mint over `pod-proxy` and `pod-tls`" | HTTP client with any method over HTTPS (pod-proxy README) | `pod-proxy/README.md` |
| "`images/create` through `pod-oci`" | `pod_oci::pull::{resolve, fetch_parallel}`, `Client` | `pod-oci/README.md` |
| "static probe line, fuzz target of the request parser" | `examples/static-probe`, `fuzz/fuzz_targets/` with 15 targets including `broker_target` | Observed `ls fuzz/fuzz_targets` |
| "betas allowed, no crates.io" | ruling "Releases", switch `crates-io = false` | `decisions.md:47`, `Cargo.toml` |
| "a fresh unit for each job, destroy and clean" | `pod-supervise` `remove`, `pod-extract` `Place::remove`; no slot concept | READMEs |

## 3. Question (c): rules and rulings, and how #47 uses them

Quotes are from `podcrates:docs/decisions.md` unless noted. Line numbers are of that file.

| Rule | Text (short) | How #47 uses it | Verdict |
| --- | --- | --- | --- |
| Priority (`:53`, 2026-11-09) | "hold on to the other repos, you may create crates for them, but leave them out; priority is first podvm, podnet, podssh projects, cfb & githome can wait". The new repositories "stay empty and get no session". M8 entries and the other asks "wait"; "podcrates can build a crate for a new project when a consumer needs it". Supersedes C26. | Uses it as the gate: "`podrunner`, `pod-dockerapi` and T-100, T-102 to T-106 are M8 and wait"; asks Q1 to lift it. | Partly accurate. `pod-dockerapi` is not an M8 entry and not in the record at all. T-099 (also M8) was built the same day under the "when a consumer needs it" clause, so "M8 waits" is not absolute. T-105 (acceptance registry) is in the wait list and in no checklist item. |
| CI tracker (`:55`, 2026-11-10) | "leave https://github.com/PodUtils/podcrates/issues/47 alone for now": T-146 and the questions Q8 to Q14 wait; "no work on the plan of #47 until the owner names it". | Not mentioned. #47 says phase 0 and the podvm items of phase 1 "can start now". | Contradicted. See 4.1. |
| New projects (`:45`, 2026-11-09) | Ten repositories, the owner creates each; `podrunner` is first in the list; podcrates gives them shared crates. | "`podrunner` ... Its repository exists and is empty." | Verified (4.3). `podci` is not in the list. |
| Test accounts (`:52`, Q-A9) | "Not now". No Cloudflare, Zero Trust or GitHub test account as a CI secret. A live acceptance step that needs one waits. | Phase 1 "A live test on GitHub: a test repository, 20 jobs, each in a fresh unit"; Phase 3 "A live test". Risks: "The minting token needs admin on the repository". | Contradicted, not mentioned, and not in Q1 to Q7. A JIT mint test needs exactly that secret. |
| Releases (`:47`) | "everyone can release betas ... but no crates io release". | "A beta release of `podrunner` (betas are allowed; no crates.io)". | Correct use. |
| KVM (`:48`) | The owner lifts the KVM deferral of podvm. A session can start a KVM guest where `/dev/kvm` exists. | Plan only ("KVM is free"). | Correct, but concerns podvm, not `pod-guest`'s Linux support (none). |
| Static rule (`:18`, `AGENTS.md:§2`) | "The only rule is: everything must be statically compiled." `design.md` rule 1: "Each crate links into a static binary on each target of section 7." Section 7 targets: x86_64, aarch64, riscv64gc, i686, loongarch64, powerpc64le, armv7, arm (musl); gnu x86_64, aarch64 (podssh); macOS and Windows (podssh). | Q2: the .NET runner "as a payload ... user content, as Node is", recommendation yes. | Honest question, correct to ask. But the rule is about crates and consumer binaries, so the reading "payload is not covered" is plausible, not established. The owner must rule (Q9 in the record). It leaves unstated that the official runner covers only x64, arm, arm64 (plan section 3), and the rule is "each target" of section 7. |
| Rule 1 pure-Rust claim | Plan section 9: "`pod-dockerapi` and `pod-actions-*` are pure Rust." | Not stated in #47. | Inferred: a Rust crate can satisfy rule 1 only with a static probe line on each target; none yet. |
| Rule 4 (binary chooses the crypto provider), 5 (caller gives the way out), 6 (settings, not env), 8 (no output), 9 (typed failures), 10 (names are parameters) | design.md section 1 | Plan says `pod-actions-proto` takes "Settings, not environment. Typed failures (rules 6, 9)". `pod-dockerapi` is silent on rule 6 (`DOCKER_HOST` must be a caller value) and rule 5 (the back end dials nothing). | Gap, not misuse. |
| C12 (`:75`) | "The forks go into `vendor/` with their patches, their upstream commit and a weekly check (podssh `check-vendored.py`)." | Plan option D: a fork of `github-act-runner` "would live in `vendor/` of `podrunner` with its patches (rule C12)". | Misapplied. C12 and `design.md` section 2 describe `vendor/` of podcrates, with forks as path dependencies. `check-vendored.py` is podssh's and does not exist in podcrates `scripts/` (Observed `ls scripts`). A Go fork in another repository is not under C12. |
| C25 (`:88`) | The egress broker is a new crate `pod-broker`; `pod-egress` keeps the roads. | Used correctly. | OK |
| C27 (`:90`) | "podcrates gives libraries, schemas and shared CI scripts. Each command line, unit file and live step stays in its product. A broker binary is a thin `main` in the product that hosts it." | Q3 answer: "a library of podcrates, hosted by the product". Phase 1 `podrunner serve --config` and `status --json`. | Consistent with C27 for the library split. Note precedent: `pod-broker` binds listeners itself (`control::bind_unix`, `serve_unix`) so a library that serves is accepted. |
| C28 to C32 (`pod-broker`) | One resolve; no path on a tunnel until a TLS end; deny file belongs to the host; token 16 to 4096 chars, SHA-256 only; audit with no path or query; plain request rules. | #47 relies on "token and policy for each job" and the TLS end T-100. | Consistent. C29 means a policy with paths needs T-100, which #47 puts in phase 2 while phase 0 capture wants it. |
| 0BSD (`:16`, `AGENTS.md:§6`) | The project is 0BSD. A fork in `vendor/` keeps its own licence. | Forgejo runner (GPL-3.0) "a separate process only, never linked"; MIT notice in `THIRD_PARTY`; docker CLI Apache-2.0. | Correct. The `podrunner` repo itself is 0BSD (Observed: forge mirror `license.spdx_id` = 0BSD). |
| Services crate (`:34`) | "they hardcode/measure many relays/third party services, it risks drift and staleness, so we move that code & related script here". | The plan hardcodes `github.com`, `api.github.com`, `*.actions.githubusercontent.com`, `broker.actions.githubusercontent.com`, `ghcr.io`, `*.blob.core.windows.net` in prose. | Not used. These belong in `pod-services` data files with a watch (the plan itself says the broker protocol changed in 2025). No such file exists. |
| Capabilities (`:21`) | No feature or capability of the source code may be lost. | n/a | n/a |
| Findings (`:54`) and `AGENTS.md:§10` | A flaw found goes into the record, not a report. | n/a | The mistakes in section 4 and 5 below would need entries if the owner names #47. |
| `RULES.md` section 4 | A GitHub issue is an inbox. "Each request of an issue becomes one entry or more." | #47 has 37 checkboxes and 7 questions; the record has one entry, T-146, L, research ("each item gets its own entry and proof" later). | Consistent but unfinished: no per-item entries exist. |
| `AGENTS.md:§7` | The egress of a cloud session is an allowlisted HTTPS proxy; "Do not try to go around it. Live tests run in `live.yml` on GitHub." | Phase 0 capture of the runner protocol "through `pod-broker` with a TLS end or a CA proxy" and "on a machine the owner controls" (plan). | Not addressed which machine and with which egress. A capture holds runner credentials (the JIT blob has an RSA private key), so `AGENTS.md:§7` (no credential in output, commits, issues) applies to `docs/research/ci/`. |

## 4. Question (d): statuses the issue relies on

### 4.1 "M8 waits" and the ruling "CI tracker"

- "M8 waits": Documented, `podcrates:TODO/PROGRESS.md:24` "M8: `pod-broker` exists (T-099, calls C28 to C32). The rest of M8
  waits (the owner, 2026-11-09, "Priority")." and `:62` "T-100, T-102 to T-120 wait, unless a consumer needs one first".
  Verified with a caveat: T-099 is M8 and is done, so M8 did not wait entirely.
- "phase 0 and the podvm items of phase 1 are podvm work and can start now": Refuted by the later ruling. Timeline (Observed, commit
  dates from the forge mirror):
  - `04:37:11Z` #47 created, body never edited since (`updated_at` equal).
  - `04:39:06Z` commit `d36df86f0`: the plan, T-146, Q8 to Q14.
  - `04:48:35Z` commit `3426b5cc9`: decisions row "CI tracker": leave #47 alone, "no work on the plan of #47 until the owner names it".
  So the ruling came about 11 minutes after the issue. The issue text was not updated. The ruling covers "the plan of #47", which
  includes phase 0 (Inferred: "no work on the plan"). Phase 0 says "no new code in podcrates", so a podvm session doing A-V-04
  on its own record is not podcrates work. The ruling's reach into podvm's record is unclear. Unverified.
- Numbering. #47 says Q1 to Q7. The record renumbered them Q8 to Q14 (`PROGRESS.md:84` to `:102`, note: "Number the questions in
  sequence from Q8 (the owner ruled Q1 to Q7: `docs/decisions.md`)"). In `decisions.md`, "Q1" is the Delivery ruling (git tag, no
  crates.io, `:15`), "Q2" the file size (`:16`), "Q3" who edits (`:14`), "Q4" in the Releases row (`:47`), "Q5" COORD issues (`:39`)
  (Observed). An owner reply "Q1: yes" on #47 collides with those. The plan itself still says Q1 to Q7 (`podrunner-plan.md` section 9).
- `decisions.md:108` says "None open." under "Questions for the owner" while `PROGRESS.md` holds seven that "wait with #47".
  The rule (`AGENTS.md:§9`, `RULES.md`) puts questions in `PROGRESS.md`, so this is consistent, but a reader of `decisions.md` alone sees none.

### 4.2 "T-100, T-102 to T-106"

Verified one by one (`podcrates:TODO/INDEX.md`):

| Entry | Row | Matches #47 |
| --- | --- | --- |
| T-100 | P1 L M8 feature open: TLS end, CA per job, placeholders, scrub | yes |
| T-101 | P1 S M1 feature done: classes, exit codes, log schema | not in the range, correct |
| T-102 | P1 M M8 open `pod-unit` | yes |
| T-103 | P1 M M8 open `pod-control` | yes (G-08) |
| T-104 | P1 M M8 open `pod-release` | yes |
| T-105 | P1 M M8 open, `pod-todo` acceptance registry (foundation.md) | named in the wait list, in no checklist item |
| T-106 | P2 M M8 open, `pod-testkit` stand-ins, start condition T-025, T-028, T-029 (all open, M2/M3) | named; start condition not mentioned in #47 |
| T-144 | P2 M M6 done (v0.12.0) | #47 lists "(A-V-06; with podcrates T-144)" as future work |

T-099 is absent from #47's list, though it is the part that exists.

### 4.3 "The `podrunner` repository exists and is empty"

Verified (with the meaning "one LICENSE commit"). Observed: forge mirror `repos/PodUtils/podrunner`: `size` 1, `created_at`
2026-11-09T13:49:38Z, `pushed_at` 13:49:41Z, `default_branch` main, license 0BSD, `open_issues_count` 0, description "Ephemeral CI
runners on your own servers: one fresh container or microVM per job, for GitHub and forge job sources, outbound 443 only." Local
clone: one commit `bf5f5a3` "Initial commit" with `LICENSE` only (`git ls-tree -r HEAD | wc -l` = 1).
The description says "container or microVM", not "podvm unit". Same for `podci`, `pod`, `podinit`, `podnode`, `podutils`, `podwatch`: one file.

### 4.4 `docs/research/fleet/asks.md`

Verified it exists (152 lines) and is the routing page named by `PROGRESS.md` and T-098. Its header: "routes the 96 asks of cfb (GitHub #35,
84 asks) and githome (GitHub #36, 12 asks)". Rows checked: A-X-01 "new project podrunner" (`:107`); G-01 to G-12 all "new project
podrunner" with side routes (`:141` to `:152`): G-04 "T-099, T-100", G-08 "T-103", G-09 "new project podrunner; podvm (A-V-04)", G-10 "T-106",
G-11 "T-104". Notes:

- G-09 (container engine for `services:`) is routed to the project and to podvm A-V-04 (a Node job). It is not routed to a podcrates
  crate. `pod-dockerapi` is therefore a new routing, not a route that the page holds.
- A-X-18 "fake edge, relay, GitHub, control plane": "T-106; the new projects own their fakes" (`:124`).
- A-V-07 (`--ulimit`, kill the tree) is routed "podvm; T-060" (`:80`); T-060 is closed, so the route points at a done entry.
- The page counts 96 asks (84 + 12). Not re-counted by me; Unverified.

### 4.5 Other status claims in #47 and the plan

| Claim | Verdict | Evidence |
| --- | --- | --- |
| "podvm marks `network`, `volume`, `-v` and `--network` as not supported today" | Partly refuted. `network` verb: None (`podvm:crates/podvm/src/parity.rs:188`). `volume` verb: None (`:189`). `run --network`: None (`:224`). `run -v, --volume`: Degraded, not None (`:222`): "a path rewrite through PODVM_MAPS on the userland rung's loader family, never a mount or a copy ... Refused by name: on the chroot rung, the memfd family, a forced PODVM_MODE and the machine tier; `:ro`, until ... T-1655; ... a working directory or a first program inside a volume." Observed at podvm `36b15ec`. |
| "A-V-06 `-v` ... with podcrates T-144" as future | Stale. T-144 and T-145 are done (v0.12.0, INDEX). #48 (COORD-023) says `-w` and the first program can now be in a volume once podvm adopts `Maps`. The podvm parity row at `36b15ec` still lists them as refused (adoption not yet done there). |
| Plan: "`pod-extract` and `pod-rootfs` wait for the take" | Stale within an hour. `PROGRESS.md`: podvm took `pod-extract` and `pod-rootfs` at `0682c0c`; `issues.md` #46 closed with podvm `0682c0c`. Plan committed 04:39Z, #46 closed 05:06Z (Observed). |
| "`pod-guest` and the machine tier of podvm give a microVM" | Refuted for `pod-guest` (finding 2). `pod-microvm` has a reserved name and no entry. The "machine tier for a Linux guest" is named in `pod-guest/src/plan.rs:7` and lives in podvm (not read by me). |
| "podvm's rule 'no daemon'" (Q3) | Not found as a ruling. No hit for "daemon" in `podvm:README.md`, `docs/decisions.md`, `docs/architecture.md`, `docs/limits.md`. "no daemon" is a stated property in podcrates READMEs (`pod-enter` README:4, `pod-oci` README:3, `pod-guest` README:5) and in podvm entries (`TODO/image.md:2072`, about a verb). A per-job Unix socket server is a daemon by function; the Q3 wording treats the scope (host-wide vs one job), not the property. Inferred. |
| "`pod-dockerapi` is the one missing piece" | Unverified and contradicted in part by 5.2 (`pod-supervise` tree kill, `pod-broker` TLS end, microVM, Linux guest are also missing). |
| "T-100 ... a CA for each job" is reachable now | G: no CA minting code exists in any production crate. |
| Plan table: "`pod-broker` T-099 done; TLS end T-100 open" | Verified. |
| README: state | Contradicts the record: `README.md:13` says "Version 0.8.0 (`v0.8.0`) is the last release". Last tag is v0.12.0 (`PROGRESS.md`, mirror tags). `check-design-paths.py` does not catch it. Not part of #47, but a reader of README gets a wrong baseline. |

## 5. Question (e): structure of #47 as a tracker, and what is hardcoded

### 5.1 Structure (Observed: `issues/47` body)

- Six phases, 37 checkboxes, all unchecked: phase 0 has 7, phase 1 has 9, phase 2 has 5, phase 3 has 7, phase 4 has 5, phase 5 has 4.
- Seven questions Q1 to Q7, each with a recommendation. Four recommend yes or accept (Q1, Q2, Q3, Q7). Q4 recommends "not now",
  Q5 "the official runner first; decide before phase 5", Q6 "refuse by name first, then measure".
- No checkbox carries an id, an owner, a repository or a record entry. The only record entry is T-146 (`podcrates:TODO/fleet.md:232`,
  P2, L, research, open). Its Approach says each checklist item "gets its own entry and proof" later. Until then the issue body is the
  only list, and a checkbox cannot be ticked by a test.
- Items of three owners are mixed in one phase: podvm's record (phase 0 reads, phase 1 first bullet: "podvm (its own record): A-V-04
  ... A-V-11"), podcrates entries (T-100, T-102 to T-106), and the unborn `podrunner` repository. Phase 1's first bullet is a single
  checkbox that stands for five podvm entries.
- Dependency order is not stated per item. Visible mismatches:
  - Phase 0 capture (item 1) names T-100, which phase 2 builds.
  - Phase 2 lists T-100, T-102, T-103, T-104 as work. The question that would lift their wait is not in Q1 to Q7. The plan's Q1
    included them ("`podrunner` and for the crates `pod-dockerapi`, T-100, T-102, T-103, T-104, T-106"); #47's Q1 says only
    "`podrunner` phase 1 and `pod-dockerapi`, after the podvm items". Observed: plan section 9 Q1 against #47 Q1. The record's Q8
    follows #47's narrower text (`PROGRESS.md:84`).
  - Phase 4 needs T-106, whose start condition is T-025, T-028, T-029 (M2 and M3, all open).
  - The plan's Q2 has an "If no: go to phase 5 at once (option B or D)" branch; #47's Q2 drops it. The record's Q9 drops it too.
  - Q3 (Docker socket) is asked before the phase 3 study entry that lists each docker call.
  - Q7 asks the owner to accept three crate names for a native runner before the capture (phase 0) and Q5 (terms) are decided.
- The issue has no `state:` line and no `(to X)` title, so it is not a COORD issue (`docs/session-protocol.md`). It carries no label.
- The record says which question blocks what (`PROGRESS.md`): Q8 and Q9 block T-146, Q10 phase 3, Q12 phase 5 only. #47 does not.

### 5.2 "The one missing piece is `pod-dockerapi`"

Not supported by the artefacts. These are also missing or unowned (each Observed in section 1 to 4 unless noted):

| Gap | State |
| --- | --- |
| A tree kill, rlimit or run timeout for a payload | absent in `pod-supervise`; no open podcrates entry; routed to a closed entry (T-060) |
| A TLS end and CA minting for the broker | T-100 open; no CA code in production |
| A Linux microVM from an OCI image | `pod-microvm` is a name only; `pod-guest` is Windows/DOS with no network |
| A job-source trait and a fake GitHub | none; T-106 behind M2 and M3 |
| `-v` on every rung | Degraded on one rung, refused on chroot, memfd and machine tier (parity row `:222`) |
| `--secret ...,once` | Unverified here: relayed by the plan ("verb is refused today"), podvm record not read by me |
| A job report schema | only `log.v1` and `broker-policy.v1` exist in `schemas/` |
| GitHub endpoints as data with a watch | no `pod-services` file |
| A control API shared across tools | T-103 open; `pod_broker::control` is a second, private one |
| Loopback address per slot, health of `services:` | unmeasured (plan section 10, items 7 and 9) |

### 5.3 Hardcoded assumptions that become single points of failure

Each row: where it is fixed, what breaks, evidence. "Inferred" means my reading, not a statement in a document.

| # | Hardcoded | Where | What fails | Evidence |
| --- | --- | --- | --- | --- |
| 1 | One job source (GitHub JIT) first | Phase 1 "The JIT mint over `pod-proxy` and `pod-tls`"; the job-source trait arrives in phase 4 | A forge (githome G-01 P0: "GitHub and a forge at once") needs a second shape of mint, session and report. Code written for the first shape leaks into the core. | `#47` phases 1 and 4; `i36.md` G-01; Inferred for the leak |
| 2 | One runner implementation: the official .NET `actions/runner` | "The tool is `podrunner`... runs the official `actions/runner`"; native runner conditional | Covers x64, arm, arm64 only (plan section 3). Requires glibc or a musl build that is unproven (plan section 10 item 5), the 30-day update rule, and the broker protocol that "changed in 2025". If GitHub breaks or revokes an old version, every slot stops at once. | plan sections 2.1, 3, 8, 10; Documented |
| 3 | One unit class first: a podvm unit started as `podvm run --rm --secret ... IMAGE ./run.sh` | Phase 1 unit kinds; Q2; Q3 | podrunner depends on the podvm CLI flags, one of which (`--secret`) the plan says is refused today. cfb's own first backend was an Incus container (A-X-01: "one fresh `Incus` container (later a VM, or a podvm payload)"); the plan rejects it ("needs root or a KVM group") and keeps it "only as a backend". | `i35-comments.md:560`; plan section 4 "Rejected"; Documented |
| 4 | One host class: "a host that denies root, namespaces, mounts, chroot and ptrace" | #47 intro, phase 1 title | cfb asks for "the operator's own servers" and githome: "Jobs run on the owner's servers first", "Linux on x86_64 and aarch64 only". The target set of #47 differs from the target set of the two asks that it serves. A design for no-root hosts can cost fidelity on the rented servers that the asks name. | `i35-comments.md:560`; `i36.md` "The owner decided" items 1 and 6; Observed |
| 5 | One architecture list that is too wide for the runner and too narrow for the rule | Phase 5 trigger "(i686, riscv64, loongarch64, ppc64le, Windows)" | githome excludes these targets; `design.md` section 7 includes them for podnet; the official runner excludes them. A native runner (L to XL, 15 to 25 kLoC per the plan) is justified by targets that no ask names. | plan sections 3, 4; `design.md` section 7 |
| 6 | One egress path: a proxy through `podnet`, `pod-proxy`, `pod-broker` | Intro; phase 2 | `pod-broker` is an HTTP proxy. Tools that ignore `HTTPS_PROXY` (the plan says Node does) escape it unless `podnet pod` or a hold (`pod-hold`, T-109, undecided) is in place. Proxy plus TLS end puts the job's TLS under a per-job CA; every client needs the CA variable. | `pod-broker/README.md`; plan P7, section 10 item 6 |
| 7 | One credential holder and one process: `podrunner` holds the mint credential, hosts `pod-broker`, hosts the per-job Docker socket, hosts the control API | Phase 1, 2, 3; Q3 | A job-facing server (Docker socket, broker listener) lives in the process that holds the credential whose isolation is the P0 acceptance (A-X-01: "the mint credential is not readable from inside a job"). Inferred. `podkey` (an in-memory secret agent) is the project built for this and is not mentioned. | `i35-comments.md:563`; org profile README |
| 8 | Two hosts for the same broker | #47: "`podrunner` hosts `pod-broker`"; `pod-broker/README.md` Adoption map: "podvm: `--net broker` ... the payload's egress goes to the Unix socket of a broker that podvm hosts" | Two token authorities and two audit streams for one job. | Documented; podvm T-1665 open, no `pod-broker` dependency in podvm yet (Observed grep) |
| 9 | Crate boundary and direction: `pod-dockerapi` in podcrates with "a podvm back end (`podvm run` as a child)" | Phase 3; plan P6 "Depends on ... podvm CLI or crates" | podcrates does not edit or depend on its consumers (`AGENTS.md:§1`). A back end that shells out to podvm belongs in podvm or podrunner, so the crate holds a trait and a stand-in and nothing else it can test end to end. Inferred. | `AGENTS.md:§1`; `design.md` section 2 |
| 10 | Names fixed before the studies: `pod-dockerapi`, `pod-actions-proto/-expr/-exec`, `podrunner serve --config`, `status --json`, `drain` | Q7; phase 1 | The crate split of a native runner is chosen before the capture that is its only spec. The three-crate split mirrors the plan's guess. | #47 Q7, phase 5; plan section 6 |
| 11 | One forge hosts everything, and the tracker, the coordination channel, the reads and the writes go through it | Tracker is a GitHub issue; `AGENTS.md:§4` reads through `forge.web.pkgforge.dev` and writes through GitHub MCP; the minting call is GitHub's | A tracker on one forge cannot track work on a runner meant to serve other forges. Inferred, with the brief as Documented. | `BRIEF.md`; `AGENTS.md:§4`; `docs/maintenance.md` "Talk between sessions" |
| 12 | One acceptance corpus: "a test repository, 20 jobs" | Phase 1 live test | Needs a GitHub test repository and an admin-scope token as a CI secret. Ruling Q-A9 says not now. The 20-job count comes from cfb's A-X-01 acceptance, not a measurement. | `decisions.md:52`; `i35-comments.md:563` |
| 13 | One test repository instead of the repositories that exist | Phase 1 | The 13 workflows of podcrates and the workflows of podvm, podnet and podssh are a ready corpus: all podcrates workflows use `runs-on: ubuntu-24.04` or a matrix with `macos-15`, `macos-15-intel`, `windows-2025`, `windows-11-arm`; `secrets.yml:57` and `:71` run `docker run --rm -v ... "$SCANNER"`; `actions/checkout` is used 19 times, `actions/upload-artifact` once, `rust-lang/crates-io-auth-action` once. A Linux runner covers the `ubuntu` jobs only; the macOS and Windows jobs are out of #47's scope. Observed by grep over `.github/workflows`. | `podcrates:.github/workflows/*.yml`; podvm inventory H01 to H05 (`docs/research/services/podvm-inventory.md:151`) |
| 14 | One time window and one count in the text | "30-day update rule", "5 to 10 % of jobs fail" (actions/runner#4309), "20 jobs", "burst of 10", "destroy in 60 s" | The first two are facts of the upstream; the plan says they need measurement. The others are cfb's acceptance numbers. | plan sections 2.1, 8, 10 |
| 15 | Control on a Unix socket only | `pod-broker` `control` is `cfg(unix)`; T-103 plans a Windows named pipe | #47 puts Windows out of scope in the plan (section 3), so no break now. Listed because phase 4 names Windows and macOS targets nowhere. | `control.rs:100` to `:104` |

## 6. The issues in scope (Observed: forge mirror, 48 issues listed with `state=all&per_page=100`)

| # | State | Subject | Relevance to #47 |
| --- | --- | --- | --- |
| 1 | open | crates.io names and trusted publishing; 6 comments; "71 of 71 reserved. Last updated 2026-11-09 16:05 UTC" | `pod-dockerapi`, `pod-actions-*` are not in it. The owner reserves names (`decisions.md:31`); `podci` is in it. |
| 35 | open, label `enhancement`, 5 comments | cfb meta request, 84 asks | A-X-01 and A-V-xx text is the source of #47's scope |
| 36 | open, 0 comments | githome meta request, 12 asks | G-01 to G-12 are the acceptance of #47. Verified: `i36.md` holds G-01 to G-12. |
| 44 | open | COORD-020 (to podssh): Wisp answer, proxy lists | none |
| 47 | open, 0 comments, not edited since creation | the tracker | subject |
| 48 | open | COORD-023 (to all): v0.12.0, podvm moves to the table of path maps (#224); `pod-ws` in v0.13.0 | T-144 and T-145 are the podcrates side of A-V-06; this supersedes the "with podcrates T-144" line of #47 |

`podcrates:TODO/github.txt` agrees with the mirror for all 48 numbers on open versus closed in the cases I compared (1, 35, 36,
44, 47, 48 open; the others closed). `podcrates:TODO/issues.md` row #47: "open | T-146"; row #48: "open | T-137, T-144, T-145, T-148".
No COORD issue mentions `podrunner`, `pod-dockerapi` or #47 (titles read, all 48).

## 7. Rules that apply to a later change and that #47 does not mention

Documented, for the design stage; no design here.

- A new crate needs: an entry (`RULES.md`), a row in `design.md` section 4 and in `README.md` (checked by `check-design-paths.py`: "README.md
  names each directory of crates/ in a code span"), a line in `examples/static-probe` (checked by `check-probe.py`), a `SOURCES` file when
  it imports, a trial in `adoption.yml` when a consumer takes it, the fuzz target in `fuzz/`, and the name reserved on crates.io by the owner.
- `check-crates-io-names.py` (`podcrates:scripts/check-crates-io-names.py`) reads names from the first cell of the tables of
  `design.md` section 4 and from "Names held, with no crate", asks crates.io, and exits 1 when a name is missing. It reserves nothing.
  It is wired into no workflow (grep of `.github` finds no use). #47 Q7 says "reserve them on crates.io with the existing check". The
  owner reserves (`decisions.md:31`: "The owner reserves each name with a 0.0.0 placeholder"). Refuted as worded.
- Files are at most 500 lines; no allowlist (`check-size.py`).
- A test uses loopback only and starts with `need_loopback!` when it binds; a live test is `#[ignore]` and runs in `live.yml`
  (`design.md` rule 11).
- Every commit that does the work of an entry names it; an entry needs `Problem`, `Premise`, `Approach`, `Prove` with a command.
- All text is ASD-STE100.

## Coverage

Read in full: `AGENTS.md`, `README.md`, `docs/decisions.md`, `docs/design.md`, `TODO/todo.toml`, `TODO/INDEX.md`, `TODO/RULES.md`,
`TODO/PROGRESS.md`, `TODO/ROADMAP.md`, `TODO/issues.md`, `TODO/github.txt`, `TODO/fleet.md`, `docs/HANDOFF.md`, `docs/research/ci/podrunner-plan.md`,
`docs/research/fleet/asks.md` (header, A-V, A-X and G tables; the A-N, A-S, A-C tables only by grep), issue #47 body, #1 body and comments, #44,
`i48.md`, the A-X-01, A-V-01 and A-V-02 text of #35, the head of #36.

Read in part: crate READMEs for `pod-broker` (all), `pod-supervise`, `pod-oci`, `pod-rootfs` (first 75 lines each), `pod-extract`, `pod-enter`,
`pod-guest`, `pod-hostprobe` (head plus API table), `pod-core`, `pod-identity`, `pod-term`, `pod-todo`, `pod-gsr`, `pod-tls`, `pod-proxy`, `pod-dns`,
`pod-ws`, `pod-services` (first 14 lines), `pod-testkit` (all); `lib.rs` for `pod-broker`, `pod-core`, `pod-supervise`, `pod-oci`, `pod-testkit`;
source for `pod-broker` `jobs.rs`, `tunnel.rs`, `control.rs`; `pod-supervise` signal paths by grep; `pod-guest` `plan.rs` head. For the other crates the API
column comes from README tables, not from `lib.rs`. Crate tests: counted by grep for attributes; only `pod-broker` and `pod-testkit` were run.
`.github/workflows`: runners, actions and `docker` use by grep, not each file read. `docs/maintenance.md`, `docs/development.md`,
`docs/session-protocol.md`: headings and the "Talk between sessions" and "Shared CI scripts" sections. `docs/research/services/*`,
`docs/research/podvm/m6-imports.md`: grep only. `TODO/podvm.md`: T-062, T-108, T-109, T-119, T-107 headers and bodies. Podvm: `parity.rs` rows
by grep, "daemon" by grep over `*.md`. Org profile README and the local `podrunner`, `podci` clones.

Skipped, with the reason: the other area files (`TODO/transport.md` 2,400 lines, `relay.md`, `roads.md`, `adapters.md`, `foundation.md` 1,369,
`upkeep.md`, `podvm.md` apart from the entries above): no #47 claim depends on them beyond the INDEX rows. Per-file reads of `crates/*/SOURCES`
(18 exist; `pod-broker` has none, which is correct: no imported file). `schemas/*.json` contents. `adoption/`. Issues #2 to #34, #37 to #46 (titles
read, bodies not). #35 and #36 bodies beyond the lines named (R1 and R2 style recon covers them). `scripts/` contents except
`check-crates-io-names.py` and `check-design-paths.py`. No CI run logs read. I did not re-run `cargo test --workspace`.

Not verifiable from here: whether the podvm `--secret` verb is refused today; A-V-04 measurements; what the "CI tracker" ruling means for podvm's own record;
the content of `PodUtils/podci` (403 on its readme, `contents` shows only `LICENSE`); whether `Cargo.toml` `rust-version` 1.99 builds on the pinned `1.99`
channel (I used `stable` 1.99.1 because `rustup` could not install `1.99` here: permission denied on the toolchain directory).
Side effects: build output went to a scratch directory outside the repositories; the repository tree stayed clean (`git status --short` empty after the runs).

## Surprises

1. Quote (#47, "The owner's rulings and the questions"): "phase 0 and the podvm items of phase 1 are podvm work and can start now."
   Artefact: `podcrates:docs/decisions.md:55`, "leave https://github.com/PodUtils/podcrates/issues/47 alone for now ... no work on the plan of #47
   until the owner names it", committed `04:48:35Z`, 11 minutes after #47 (`04:37:11Z`). The issue was never edited.
2. Quote (#47): "`pod-guest` and the machine tier of podvm give a microVM for an untrusted job." Artefact: `podcrates:crates/pod-guest/README.md` (UEFI with a
   `cmd.exe` agent, FreeDOS) and `plan.rs:104,142` (`-nic none`). No Linux guest in `pod-guest`.
3. Quote (#47 phase 4): "The job-source trait and a fake forge in `pod-testkit` (T-106 ...)". Artefact: `podcrates:TODO/fleet.md:218`, "The fake GitHub API and the fake
   control plane stay in their projects"; start condition `:230` names three open M2/M3 entries; `pod-testkit` has no stand-in.
4. Quote (#47 intro): "podvm marks `network`, `volume`, `-v` and `--network` as not supported today". Artefact: `podvm:crates/podvm/src/parity.rs:222`, `-v, --volume`
   is Degraded and works on the userland rung.
5. Quote (#47, Q1 to Q7). Artefact: `podcrates:TODO/PROGRESS.md:84` to `:102` (Q8 to Q14) and `decisions.md` rows that already use Q1 to Q5 for other rulings.
6. Quote (#47 Q7): "reserve them on crates.io with the existing check". Artefact: `podcrates:scripts/check-crates-io-names.py` docstring: "Check that each name of docs/design.md,
   section 4, is on crates.io". It checks. The owner reserves.
7. Quote (#47 phase 1): "A live test on GitHub: a test repository, 20 jobs". Artefact: `decisions.md:52`, "Test accounts (Q-A9) ... Not now. No ... GitHub test account as a CI secret."
8. Quote (plan P5): "`pod-supervise` (T-060 family) ... a timeout that kills the whole tree". Artefact: `launcher.rs:351` `sys::kill(pid, sig)`; T-060 `done`; no open entry.
9. Quote (plan option D): "A fork would live in `vendor/` of `podrunner` with its patches (rule C12)". Artefact: `decisions.md:75`, C12 is the `vendor/` of podcrates with podssh's `check-vendored.py`
   (not present in `podcrates:scripts/`).
10. Quote (#47): "Its repository exists and is empty." Verified (LICENSE only), but `PodUtils/podci` ("Run CICD Actions on Pods", LICENSE only, created 4 hours earlier, reserved in #1
    as a project name that "cfb's asks name", named in no ask) overlaps it and neither #47 nor the plan mentions it.
11. Quote (#35 A-X-01): "starts one fresh `Incus` container (later a VM, or a podvm payload)". #47 reverses the order and drops Incus as a unit kind (`podvm`, `docker`, `host`).
12. `podcrates:README.md:13`, "Version 0.8.0 (`v0.8.0`) is the last release", against `PROGRESS.md` (v0.12.0, `393158f`) and the mirror tags.
13. The plan row "Taken by podvm up to `pod-oci`; `pod-extract` and `pod-rootfs` wait for the take" was stale 27 minutes after it was committed (`issues.md` #46, podvm `0682c0c`).
