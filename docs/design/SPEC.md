# podrunner: corrected design and build spec

Date: 2026-11-10 (read from the machine clock). Revision: draft 5.
Replaces the plan in `PodUtils/podcrates#47` and `docs/research/ci/podrunner-plan.md`. Posts nothing, edits no repository.

How to read this file. Every factual claim carries a label and a pointer into `recon/`, `measurements/` or the issues as read (not in this copy; read the issues). A pointer `R4:2.6` means section 2.6 of the file named in the key table. `R5 F4` and `R6 B7` name a numbered finding of that report.

| Key | File | What it is |
| --- | --- | --- |
| R1 | `recon/R1-podvm.md` | podvm at commit 36b15ec034f5 |
| R2 | `recon/R2-podnet-podssh.md` | podnet cbe905374e0f and podssh 961acb1fcc85 |
| R3 | `recon/R3-podcrates.md` | podcrates ec37ed2126ec |
| R4 | `recon/R4-upstream-runners.md` | upstream runners, hooks, scale sets, Docker API servers |
| R5 | not in this copy | the owner's private `podrunner-ideas` archive; its citations stay as evidence |
| R6 | `recon/R6-asks-coverage.md` | asks of #35 and #36 against #47 |
| M1, M2 | `measurements/m1-runner/`, `measurements/m2-podvm/` | runs made for this spec |

Labels: **Observed** (a command or artefact read this session, quoted in the pointer), **Source** (code says it, path and line in the pointer), **Documented** (a document says it), **Inferred** (reasoning from labelled facts), **Unverified** (carried, not checked). A statement with no label is a design decision of this spec.

Terms. H0 is the no-root, no-userns, no-mount, no-KVM x86_64 host class that #47 targets (§10.2). `G-n` are the githome asks of #36. `A-X-n`, `A-V-n`, `A-N-n`, `A-S-n`, `A-C-n` are the cfb asks of #35. `DR-n` is a decision request (§2.1), `WP-n` a work package (§3.3), `U-n` an unknown (§14). `Cn` is a correction to #47 (`APPENDIX-CORRECTIONS.md`), `Sn` a single point of failure (`APPENDIX-SPOF.md`). A trust tier is `untrusted`, `trusted` or `local` (§6.3). Acceptance has levels 1 to 3 (§13). `Tier` alone always means trust tier.

Reading order: this block, §1, §2 (decisions, edits to #47), §3 (work plan, what can start now), then the design §4 to §11. A builder also reads §12 (the acceptance each ask is restated to), §13 (the three acceptance levels and the level 1 test list that WP-02 and WP-03 are built against) and §15 (what v1 does not build), `contracts/`, and `APPENDIX-FLOWS.md` (the configuration example). §14 and §16 are evidence. After §3, a `Cn` or `Sn` id comes with its claim in the same sentence.

---

## Read this first

0. What you do, in order. (a) DR-1 and DR-2: name this plan and lift the empty-repository wait, or nothing starts (§2.1). (b) DR-4: accept the `redirect` boundary for `trusted` jobs on no-root hosts (`redirect`: the unit's file paths are rewritten, nothing is isolated from hostile code), or those hosts run no job. (c) DR-5: an own microVM driver now, or wait for podvm T-1656. (d) DR-6: githome and cfb accept the restated G-02, G-03 and G-09. (e) DR-7: a throwaway GitHub organization for live tests, or the gates stay unverified. The other eleven DRs have a default that needs no answer: confirm them in one reply. What can start now: nothing that belongs to this plan. podvm's own record already holds three entries this plan relies on. T-1696 `--secret` and T-1695 `--report` proceed in podvm's own order and need no answer. T-1655 limits proceeds in that order too, but WP-V4, which uses its tree kill, waits for WP-C2 and so for DR-1 (§3.1). The other three podvm items (WP-V1, WP-V5, WP-V6) have no record entry yet. They are filed in podvm's record first and wait for DR-1, like everything in podcrates and podrunner (§3.1). This directory's own checks need no answer. The edits to #47 are §2.2. The replacement body is `ISSUE-47-REPLACEMENT.md`.
1. The owner ruled on 2026-11-10 "leave https://github.com/PodUtils/podcrates/issues/47 alone for now" (`docs/decisions.md`, row "CI tracker"). #47's "can start now" is false. Nothing here is posted.
2. Unless the owner answers yes to DR-4, H0 runs no job. Even with yes, the unmodified official runner does not start on podvm's default (userland) rung: M2 observed exit 154, then 137 (WP-V1). It ran only under `--unsafe-host-paths`, which is not isolation. It also needs a network hold: placement refuses a `trusted` job at hold `none`, and the `advisory` hold of `podnet pod` has never run with the runner (U9, WP-N1).
3. A job whose `limits_required` names `wall` needs a verified tree kill. podvm has none (M2). Such a job is refused with `limit-not-enforceable` until the per-unit subreaper shim passes the conformance suite or WP-V4 lands (§10.4).
4. Milestone M-A needs no account and no second machine: level 1 on fakes, then the podvm driver with a stand-in payload (§3.2).
5. G-03 (credential unreadable) is restated, not met. G-02 and G-09 are restated too (§12, DR-6).
6. Everything GitHub-side is unverified until a live test: the container-hooks job variable (U6), several labels per scale set (U7), the App permissions (U8, U12). No GitHub account was used.
7. M1 and M2 ran with no GitHub account and no job, on one Debian 13 host (the runner archive was downloaded from github.com; §8.2, §10.4, §10.5). The hook response shape was read from the runner source (`contracts/hook.md`).
8. Corrections to #47, each with its row in `APPENDIX-CORRECTIONS.md`: `pod-guest` is not a microVM (C1); `-v` is Degraded (C2); `pod-testkit` holds no fake GitHub (C9); `podci` exists (C24); container hooks come before `pod-dockerapi` (§7); the credential cannot be "read once" (§8.2); `RLIMIT_FSIZE` and the default `DOTNET_EnableWriteXorExecute` kill the .NET runner at start (§10.4); the minting key also edits runner groups (§8.1) and a fork job can name the trusted set, so the intake check is the control (§6.3).
9. Reads went through a forge mirror. The GitHub terms of service were not read (§16).

---

## 1. The answer in short

The goal of #47 is right: run GitHub CI, and other CI, on hosts that give no root, with the official runner as the payload and podvm as one way to isolate it. The plan builds it in the wrong shape. It fixes one of everything (source, runner, unit start path, container route, egress path, secret holder). It reinvents pieces that exist (`APPENDIX-EXISTING.md`), states facts that the artefacts refute (`APPENDIX-CORRECTIONS.md`, 29 rows, ids C1 to C30, C13 is unused) and builds in 25 single points of failure, each with the section that removes it (`APPENDIX-SPOF.md`). It already names some gaps (tree kill, the credential readable by steps, i686 and riscv64), and this spec credits those.

In plain terms: `podrunner` is one program that finds a CI job, picks the safest isolation the host has, starts the official runner inside it, and refuses what the host cannot isolate.

Kept from #47: the official `actions/runner` as first payload; podvm as the unit kind for hosts with no root; outbound TCP 443 only; static `podrunner` and `podvm` binaries; the asks of #35 and #36 as acceptance targets, with the changes of §12.

The changes are the items of §2.2 (`APPENDIX-EDITS-47.md`). In order of weight: two protocol seams and two capability axes, with nothing in the core that knows GitHub, podvm or .NET by name (§4); the secret holder split from the parsers of job input, with remote mint where the host cannot give distinct uids (§5); container hooks before a Docker API (§7); a scale set to find jobs (§6.2); trust from job metadata, never from a label (§6.3); G-03 restated (§8); an honest host matrix (§10); offline acceptance by levels (§13).

---

## 2. Decisions and the edits to #47

### 2.1 Decision requests

DR numbers avoid the Q-number clash (the record numbers #47's questions Q8 to Q14; Q1 to Q5 already mean other, ruled questions; Documented, R6 D3). The map from Q1 to Q7 to the DRs is in `APPENDIX-DISPOSITION.md`. Answer DR-1 to DR-7 as marked.

| Id | Question | Recommendation | Default |
| --- | --- | --- | --- |
| DR-1 | Does the owner name this plan, lifting "leave #47 alone" for it, and does it replace the plan of #47? | Yes | A document only. No repository is touched |
| DR-2 | Lift "new repositories stay empty" for `podrunner`, for WP-01 to WP-13? | Yes, WP-01 to WP-03 first | `podrunner` stays empty |
| DR-3 | Scale-set client: Go sidecar of `actions/scaleset`, or a Rust port of six endpoints? | Sidecar | Sidecar |
| DR-4 | May `trusted` run at `boundary: redirect` on hosts that offer nothing better (H0)? Accepted residuals (§5.3, `APPENDIX-THREATS.md` T2, T3, T8): a compromised dependency exfiltrates that job's secrets, reaches host loopback services and reads co-tenant jobs on one principal. A compromised trusted job holds the host key of the remote mint and obtains tokens until the operator revokes the key. A credential minted for one job may run another (U22). It can rewrite the configuration, the key files and the driver binaries of the host, and so change the jobs of every later repository. The minting side must be another machine. Cost: WP-05b, size M to L. Mitigations: offers per host capped at the configured slot count, a host key with a rotation interval and shorter tokens, a signed pin list (§5.3) | Yes, with `status` and every report naming boundary and hold, and `untrusted` fixed at `guest` | `trusted` needs `confined`. No podvm rung reports `confined` (§10.1), so podvm hosts of every class have no `trusted` tier and H0 runs no job |
| DR-5 | `microvm` unit: an own driver (Firecracker or QEMU) as a stopgap until T-1656 lands, or wait? Cost: two microVM implementations for a while | Own driver (T-1656 is XL, the archive launcher exists) | Wait |
| DR-6 | Do githome and cfb accept the restated G-02, G-03, G-09 (§12)? | Yes | They stay open for the official runner |
| DR-7 | A GitHub test organization and App for level 3, despite Q-A9? | Yes, a throwaway organization, the App key never in another repository's CI secret | Level 3 waits. G-H1 to G-H4 stay unverified |

Confirm in one line (each recommendation is the default):
- DR-3: the Go sidecar for the scale-set client (default).
- DR-8: container route hooks first, `docker build` refused by name (replaces Q3, Q6).
- DR-9: GitHub sources organization-level only, personal repositories refused.
- DR-10: the Go sidecar and runner payloads are user content, `podrunner` and `podvm` stay static (replaces Q2).
- DR-11: the one egress proxy lives in `podrunner` as a module (A-X-05). podvm's `--net broker` stays a second host of the same crate.
- DR-12: `podrunner` stays. `podci` is released or becomes the CLI name.
- DR-13: the `docker` kind is built only when the owner names a Docker or Podman host with no KVM (§15).
- DR-14: a host with root or a namespace builds its own images. An H0 host imports an image a build host made (§11.3).
- DR-15: `pod doctor` owns host qualification (A-X-25), with `podrunner doctor` a thin reader.
- DR-16: the owner reads the terms for a third-party runner version string (plan Q5) before any alternate payload ships.
- DR-17: reserve `pod-dockerapi` and any new crate name only after a census (§15).

### 2.2 Edits to #47

`ISSUE-47-REPLACEMENT.md` is the deliverable and stands alone. `APPENDIX-EDITS-47.md` is the audit trail of the change against the old body, item by item. `APPENDIX-DISPOSITION.md` says where each piece P1 to P14 of the plan, each checklist bullet of #47 and each question Q1 to Q7 went (kept, changed, cut, deferred or not on this path). Every correction of `APPENDIX-CORRECTIONS.md` appears in it (the id C13 is unused), and `tools/check_docs.py` checks that.

---

## 3. Work plan

Sizes (S, M, L) are unmeasured guesses. A `podrunner` or podcrates package starts only after DR-1 and DR-2 (the rulings "CI tracker" and "Priority": new repositories stay empty and get no session). WP-V items follow podvm's own record. The first milestone (M-A) is WP-01 to WP-04, S to M each.

### 3.1 What can start now, under the rulings as written

- **podvm's own record**, as the records order it: T-1696 (WP-V2), T-1695 (WP-V3) and T-1655 (WP-V4). R1:3.1 puts T-1695 and T-1696 behind about 26 open entries, the whole open count, so an upper bound (Inferred). T-1655 has no stated position (Unverified). "Can start now" is a queue position, not a date. WP-V4 is blocked by WP-C2, so by DR-1. WP-V1, WP-V5 and WP-V6 have no record entry yet (§3.3), so they wait for DR-1 too.
- **This directory**: contracts, schemas, checks and the measurement scripts of phase 0. They write to no repository.
- **Nothing in podcrates or podrunner** until DR-1 and DR-2 are answered.
- **An owner decision**, not a package: the release workflows of podvm, podnet and podssh pass `--deny-self-hosted-runners` to `gh attestation verify`, so dogfooding their release proofs on `podrunner` needs those flags changed.

### 3.2 Order, milestones and gates

| Phase | Content | Gate |
| --- | --- | --- |
| 0 measure | M1, M2 and the rest of §14 that needs no account | Each unknown settled or recorded with its blocker |
| 1 skeleton | WP-01 to WP-04, WP-08: traits, `slots` with fakes, podvm driver, status, drain, journal | Level 1 green. podvm unit conformance green with a stand-in payload (M-A) |
| 2 GitHub | WP-05a, WP-06, WP-07, WP-10, WP-C1, WP-C3, WP-C4, WP-N1: sidecar, tiers, image pipeline, reports. WP-05b (remote mint) only if DR-4 is yes | G-H1 to G-H4 live (DR-7) or a recorded waiver. H0 runs a job only if DR-4 is yes, WP-05b is built, the official runner starts on the userland rung (WP-V1) and WP-N1 (U9) shows an `advisory` hold. Else H0 runs no job |
| 3 containers and guests | WP-09, WP-11 to WP-13, WP-C2: hook relay, control socket, `microvm`, fake broker, tree kill in podvm's launcher | Level 2 green. Hook path measured |
| 4 to 6 other sources, census-driven API, alternates | Deferred (§15) | |

G-H1 to G-H4: hooks variable sent; scale set label count; App permissions sufficient; label race refused.

**M-A** (an H0 host, no account, no second machine): level 1 green with fakes (WP-01 to WP-03). WP-04 passes the unit conformance suite with a stand-in dynamic payload (a shell or Node script), using a per-unit `PODVM_STORE` and a per-unit subreaper shim. No official runner, no forge, no `trusted` job. It shows credential files at mode 0600 and absent from argv and environ, the placement refusals, `limit-not-enforceable` for `wall` until the shim passes conformance test 3, `status`, drain and journal replay.
**M-B**: WP-V1 lands. `config.sh --help` and `run.sh` start on the userland rung without `--unsafe-host-paths`. Still no GitHub.
**M-C**: DR-1, DR-2 and DR-7 answered, WP-05a, WP-06 and WP-07 built, `mint` in process. G-H2 to G-H4 run live against a `fake` unit: jobs are refused or released, none runs. It needs neither DR-4 nor WP-05b.
**M-D**: DR-4 yes, WP-05b built, WP-V1 landed, and WP-N1 (U9) shows the `advisory` hold. One live job runs on H0, and G-H1 is read.

### 3.3 Work packages

A WP-C or WP-V item needs a record entry with Problem, Premise, Approach and Prove (R3 section 7) and a fuzz target for any parser. Cut packages are in §15. Builder's list: WP-02 and WP-03 read §13 and the guard list of `APPENDIX-ASKS.md`. WP-04 reads §10.4 and §10.5. WP-09 reads §7.2 and `contracts/hook.md`. WP-05b reads §5.3 and `contracts/source-driver.md`.

| Id | Repo | Scope | Depends | Size | Proves |
| --- | --- | --- | --- | --- | --- |
| WP-01 | podrunner | `contracts/`, Rust traits of both seams, conformance harness | none | S | Harness runs |
| WP-02 | podrunner | `fake` source and unit: scripts, fault injection, clock trait (§13) | WP-01 | S to M | Level 1 inputs |
| WP-03 | podrunner | `slots` core: states, journal, tiers, placement, status, drain, codes, config, `audit` forwarding | WP-01, WP-02 | M | G-01, G-02, G-05, G-08, level 1 |
| WP-04 | podrunner | podvm driver: per-unit `PODVM_STORE`, per-unit subreaper shim, conformance run with a stand-in payload | WP-01 | M | Unit conformance (M-A) |
| WP-05a | podrunner | `mint` with `sign` module in process, source conformance suite (tests 1 to 5, 10 and 11) | WP-01 | S to M | Source conformance |
| WP-05b | podrunner | Remote mint, built only if DR-4 is yes: HTTPS transport, host keys and tokens, offer ledger, reconciliation, signed pin list | WP-05a | M to L | Source conformance tests 6 to 9 and 12 |
| WP-06 | podrunner | Fake scale-set service (Go, test only) | WP-01 | S to M | Input of WP-07 |
| WP-07 | podrunner | `github-scaleset` sidecar, exit-code mapping, `started` event | WP-05a, WP-06, DR-3 | S to M | Source conformance on WP-06. G-H1 to G-H4 live |
| WP-08 | podrunner | `egress` module over `pod-broker` v1, one listener per unit (§9.5) | WP-04 | S | G-04 |
| WP-09 | podrunner | Hook relay and control socket server (§7.2) | WP-04, WP-12 (`fake` path), WP-V1 and WP-13 (H0 path) | M | G-09, level 2. Starts after WP-04 on `fake`. Its H0 acceptance waits for WP-V1, WP-12 and WP-13 |
| WP-10 | podrunner | Image pipeline: pins, age. The builder needs root or a namespace (§11.3) | WP-03, WP-04 | M | §11.3 |
| WP-11 | podrunner | `microvm` driver: archive launcher port, OCI-to-rootfs, kernel, self-test. Retired at T-1656 | DR-5 | M to L | G-02, §6.6 |
| WP-12 | podrunner | Canned-message fake broker (§13) | WP-V1, M1 | M to L | Level 2 |
| WP-13 | podrunner | Measure U19 and U21 (a script in this directory first) | WP-V1 | S | U19, U21 |
| WP-C1 | podcrates | `pod-broker` upstream by name, tested against a copy of the box proxy fixture | none | S | §9.4, U10 |
| WP-C2 | podcrates | `pod-supervise`: tree kill, timeout, rusage. The mechanism of §10.4 placed in podvm's launcher | podvm agreement | M | G-12 |
| WP-C3 | podcrates | `schemas/` and crate types for offer, capability, report (one schema for githome and cfb, S14); question renumbering | WP-01 | S | S14 |
| WP-C4 | podcrates | `pod-services` entry for the Actions hosts, with a drift watch | none | S | §9.6 |
| WP-V1 | podvm | Official runner starts on the userland rung without `--unsafe-host-paths` (exit 154, then 137, §10.5). A third layer past `--help` is possible | A new record entry, DR-1 | M | A-V-04 |
| WP-V2 | podvm | T-1696 `--secret`, aware of the three files of §8.2 | none | M | A-V-11 |
| WP-V3 | podvm | T-1695 `--report` | none | M | G-06 |
| WP-V4 | podvm | T-1655 limits and tree kill, no `fsize` on .NET (§10.4) | WP-C2 (so DR-1), a podvm tag bump | L | G-12 |
| WP-V5 | podvm | One rootfs per container, or a per-run overlay (C30). Removes the per-unit store cost of WP-04, not the capability | A new record entry, DR-1 | M | Conformance test 2 |
| WP-V6 | podvm | `exec` workdir inside a volume, stderr split in `logs`, if the hook path needs them | A new record entry, DR-1 | S | Conformance test 8 |
| WP-N1 | podnet | `podnet pod` with the .NET runner, Node `fetch` and the actions `checkout` and `setup-node` run by `node` directly (`cache` and `upload-artifact` need a live job); the `pod-hold` split (podnet#414) | none | S | U9 |

---

## 4. Architecture

### 4.1 Shape

v1 is one `podrunner` binary with four modules and one child process.

- `slots`: state machine, placement, journal, status. Supervises the rest.
- `mint`, with its `sign` module: holds the source credentials, spawns the Go source sidecar as its child. The sidecar never holds the App key. It asks `sign`.
- `egress`: the broker over `pod-broker`, one listener for each unit.
- `ctl`: one control-socket server for each unit (hook route, phase 3).

Sources: `github-scaleset` (the sidecar) and `fake`. Units: `podvm`, `microvm` (DR-5) and `fake`. Separate processes for `sign`, `egress` and `ctl` are deferred until a host with distinct uids is the first target. The `docker`, `incus` and `host` kinds, the forge source, bridges and `dockerapi` are in §15.

### 4.2 Protocols exist because each seam has two implementations

A seam with one implementation is a capability field, not a protocol: the egress hold has one (`pod-broker`) and is a field of the unit record.

**Unit seam.** The day-one form is an in-process Rust trait, and the conformance suite runs against it. The JSON-lines wire of `contracts/unit-driver.md` is for out-of-tree drivers and is built when the first exists. The podvm adapter is a module that spawns `podvm` and is the only code that knows its argv. When a flag is renamed (the timeout flag is spelled two ways, R6 D14), that module and its conformance test change. Operations: `probe`, `prepare`, `start`, `signal`, `wait`, `destroy` and the container operations `container_start`, `container_exec`, `container_logs`, `container_inspect`, `container_stop`. Rules that matter to the design: `probe` is idempotent and `unknown` is treated as absent; `prepare` takes a digest, never a tag, and refuses a foreign platform unless emulation is named; `start` gives each unit a fresh root and a fresh store (C30: all containers of one image share one mutable rootfs), copies the credential files into the runner root at mode 0600 and puts no runner credential in argv or env; `signal` reaches the whole tree; after `destroy` no process descended from the unit and no file of the unit remains, counted by the driver; the container operations are scoped to one unit and validated on the host side (§7.2), and a driver without them reports `container: none`.

The capability record has the fields of `schemas/unit-capability.v1.schema.json`, listed in `contracts/unit-driver.md`. Boundary order: `none` < `redirect` < `confined` < `guest`. `confined` is a kernel-enforced unit: a container engine, or Landlock plus seccomp once `pod-confine` exists. A limit is `enforced` (the runtime stops the job) or `reported` (the driver measures and `slots` acts).

**Source seam.** A source driver is the scale-set sidecar, the fake, or a module. The wire (JSON lines) is needed on day one because the sidecar is Go. Operations: `hello`, `next` (long poll with `capacity`; returns `offer`, `cancel`, `completed` and `started` events), `ack`, `claim`, `mint`, `report`, `audit`, `withdraw`, `release` (`contracts/source-driver.md`, which lists the offer fields). Design rules of an offer (`schemas/job-offer.v1.schema.json`):

- `trust_inputs` are copied from the forge, never from a label, and the source sends no tier. `member` means a member of the organization named by `owner`. The source resolves them before it emits the offer, and a failed lookup is `unknown`.
- `labels` serve the log and set matching only, never a tier input.
- `limits` and `limits_required` are declared by the **set**.
- `payload` is optional. A source not marked `payload_from_offer` omits it, and `slots` takes the payload from the pin list entry of the set (§11.3). A `github-scaleset` source never states one. A `payload_from_offer` source may name it, never for a `host` unit.
- `policy` follows the schema of `pod-broker` (`inputs/broker-policy.v1.schema.json`). `slots` adds the control-plane hosts of the source (§9.6).
- `unit_need` is optional: `min_boundary` can only raise the tier minimum. `JobAvailable` carries no container, service, port or limit data (Source, R4:3.1), so only a source that knows them states them (a forge source, or `fake`). For GitHub they are found at hook time (§7.2) or fixed for a set by configuration (`containers`, §11.7).

A unit is one isolated environment for one job, with its own root, store, credential directory and control socket. A slot is one concurrent unit of one kind. A set is one scale set: one for each tier, architecture and host, and its tier is the tier of its runners. The hold is how a unit's network traffic reaches the broker, a word kept apart from boundary on purpose. The principal is the uid or guest a unit runs as: `shared` means control plane and units share one uid. A pin is `{digest, built_at, runner_version, max_age_days}` (§11.3). A refusal is a job this host will not run, with a stable code (§11.1).

---

## 5. Process split and trust boundaries

### 5.1 Threat model

`APPENDIX-THREATS.md` holds the table of twelve actors: a fork-PR author, a compromised dependency in a trusted job, a co-tenant job on one principal (concurrent or earlier), a leaked source credential, a hostile container request or hook input, a hostile proxy or DNS, a holder of a stolen host token, a compromised `mint` host or sidecar, a job with secrets or an unvetted author, an observer on the path to the remote `mint`, and a trusted job that kills its shim or causes `degraded`. Assets: the source key, the host token, the per-job credential, repository secrets, the host network, other jobs' data. Accepted residuals (DR-4): on a `redirect` unit a compromised dependency in a trusted job can exfiltrate that job's secrets, reach host loopback services, read other jobs on the same principal, and on H0 hold the host key of the remote mint and rewrite the files that later jobs of the host rely on (§5.3).

### 5.2 Roles and topology

Target rule: **no process that parses job-controlled bytes holds a source credential or a CA key.** In v1 the modules share one process, so the rule is met only for the sidecar, which holds session tokens and no key. That is accepted because on H0, the target class, a split removes nothing (§5.3). The split happens when a host with distinct uids is the first target (§15).

- `slots` holds the per-job runner credential in memory until it is written. It treats job metadata as data and never executes it. It listens on the control socket of §11.5.
- `mint` holds session tokens, host tokens and the offer ledger, and parses source JSON, including job-controlled strings. It is in process, or one HTTPS port in the remote form.
- `sign` holds the App key (mode 0400) and signs one request shape (§8.1). Only `mint` calls it.
- The sidecar holds session tokens, parses the same strings as `mint` and speaks stdio.
- `egress` holds job tokens as SHA-256 and parses HTTP heads of job traffic.
- `ctl` holds nothing of the source. It parses hook JSON and container requests on one unix socket for each unit.

If `egress` dies, its units fail closed (§11.6). Library and binary placement follows the podcrates design rule on libraries and binaries (rule C27 of its `docs/design.md`, not correction C27): libraries in podcrates, commands in the product.

### 5.3 Hosts with one principal

On H0 the unit, `slots`, `egress` and the host token are one principal. Observed (R2:1.2): `connect()` to a host unix socket succeeds from a held payload, and the peer-uid check passes for the same uid. A step can send `drain` on the control socket and read any file of the uid, including a key at mode 0400 (Inferred; not run with a signer).

1. Distinct uids need a service manager (H1 to H4). H0 has none.
2. **Remote mint.** `mint`, its `sign` module and the sidecar run on another machine or uid domain. The unit host runs `slots`, `egress` and the units, and reaches `mint` outbound over the source protocol served over HTTPS (`contracts/source-driver.md` holds the rules and the conformance tests):
   - `mint` computes the tier itself from the run object (§6.3). The unit host never states one.
   - `mint` keeps a ledger of offers. It pushes an offer to one host, records the assignment and issues the runner credential for that offer once. The outstanding offers of a host never exceed the slot count that `mint` has configured for it, whatever `capacity` the host states. A host token authorizes only the offers assigned to that host.
   - A host token is bound to a host key (mutual TLS with pinned keys on both sides) and meets a request and rate limit. The key has a rotation interval in the configuration of `mint`, and a token's life is shorter than the key's. `mint` reconciles its ledger with the starts that `slots` reports: a credential with no start within `start_s` plus a margin raises an alert and, past a threshold, revokes the token and deletes the runners registered with it.
   - On H0 none of this is a boundary. The host key is held by the principal that runs the steps: a step reads any file of that uid and, where ptrace is allowed (as on the M2 host, §10.5), the memory of `slots` (Inferred, not run). A compromised trusted job holds the key, not only a token, and obtains tokens until the operator revokes the key at `mint`. The audit line of `mint` carries the token id and the peer address, so a second host or a start mismatch is visible. The ledger bounds the credentials a host receives, not the jobs it runs: until U22 is settled, a credential minted for one job may run another job of the same set. That is the cost of DR-4.
   - A host that cannot reach a remote `mint` serves no `trusted` job.
3. **One principal, many jobs.** Units at `redirect` or `none` share the host principal, so co-tenant jobs can read each other's credential files, egress state and services. `max_concurrent` is the effective slot count of a kind: `min(slots, 1)` on a `shared` principal unless the source sets `same_principal_ok = true` (recorded in every report, never across tiers). Observed (M2 `05-lifecycle.log`): grandchildren survive `stop` and `rm -f`. A verified tree kill removes the processes of a job, not the files it leaves in the uid (the state directory, the pin list, `podrunner.toml`, saved image archives, driver binaries). So on a `shared` principal the host serves one repository (`affinity = "repository"`) always, unless `same_principal_ok = true`, which lifts both limits. `residual_processes` above 0 or `unknown` puts the host in `degraded` (§10.4).
   - `mint` signs the pin list (Ed25519, `pod-identity`; Source, R3), the unit host verifies the signature at every use, and the driver compares the `image_id` of `prepare` with the pin before every `start` (WP-05b). On a `shared` principal this stops a pin list that a step changes while `slots` runs, only if `slots` holds the verifying key in memory. A step that rewrites the pin list can rewrite a key file before the next start of `slots` (Inferred, not run).
   - Where the host can enforce it, the configuration, the key files and the driver binaries belong to a principal that the unit cannot write. On a `shared` principal they cannot: a compromised trusted job can replace them before the next start of `slots`, and so change the jobs of every later repository. That residual is part of DR-4.
4. `probe` and `status` report `principal`. The vocabulary has no `isolated` value.

---

## 6. Sources: job intake and credential mint

### 6.1 Source kinds

- `github-scaleset`: finds jobs by one long poll for each scale set (`JobAvailable` messages), then `acquirejobs`. Mints with `generatejitconfig` on the set. The payload is the official runner. `actions/scaleset` is Go, MIT, "Public Preview" (Documented, R4:3). It runs as a child sidecar of `mint` that speaks the source protocol.
- `fake`: a canned file or in-memory script, a canned credential, any payload. The test double of G-10, in the `podrunner` repository (T-106: fakes stay in their projects).

The forge-contract source and the bridges for other forges are in §15. `github-rest` is no source kind: a reconciler may poll queued runs through the REST API to find jobs a scale set lost, and `status` reports them. It never mints. A runner group alone cannot keep fork jobs out of a trusted group, because a fork PR runs in the base repository with the same workflow file as a push job (Inferred, R5:8.4).

### 6.2 GitHub: why a scale set is the primary path

#47 says "for each job `podrunner` mints a JIT runner config" and does not say how it learns of the job (R6 B5). With no inbound port that leaves polling or a webhook relay. A scale set is outbound only (Source and Documented, R4:3.1): one message session and one long poll of about 50 seconds for the whole set, capacity in a header, messages `JobAvailable`, `JobAssigned`, `JobStarted`, `JobCompleted` (repository, owner, event name, workflow ref, `requestLabels`, run id), `acquirejobs`, `generatejitconfig`, and a client that accepts a custom JWT signer. A job assigned and not picked up is re-queued up to three times. Organization scope needs "Self-hosted runners: read and write"; repository scope needs "Administration: read and write" (R4:3.2). Inferred (R4:3.1): a runner is not bound to a job at mint time, and a JIT runner takes the next job that matches its labels (U22).

`JobAvailable` has no fork flag, actor, visibility, container, service, port or limit data. The source reads the run object for the first three (§6.3). Consequences:

1. **Intake enforces tier and architecture only.** The source resolves the trust inputs and emits an offer. `slots` computes the tier, compares it with the set's (§6.3), then picks a unit kind (§6.5). It acquires the job only when a slot of that kind is free. A job with no slot stays queued on GitHub, and `status` says why (G-02, G-08). Container, port and limit needs are set configuration or hook-time refusals (§7.2).
2. **One scale set for each tier, architecture and host**, named by the label that workflows put in `runs-on`. The workflow author chooses that label, so a fork job can name the trusted set. The intake check stops it (§6.3). A second wall: the runner group of a `trusted` set admits named private or internal repositories only and refuses public ones (Unverified: the setting, U12). Whether github.com honours several labels per set (U7) and whether two hosts can share a set (U17) are Unverified.
3. **The minting credential still exists** (R4:3.2): an organization-level GitHub App with "Self-hosted runners: read and write", repository "Actions: read" and organization "Members: read" (§6.3). The App key sits behind `sign` (§8.1).
4. **Personal-account repositories** need repository-level administration, the largest grant in the survey (R5:8.5), and are refused (`repo-scope-refused`).

The sidecar `podrunner-source-github-scaleset` is a static Go program that imports `actions/scaleset` and speaks the source protocol on stdio. A Rust port of the six endpoints is S to M and moves protocol drift onto this project (DR-3). Unverified, each a gate (§3.2): that `actions/scaleset` is stable (public preview), that github.com honours several labels and sends the container-hooks job variable, and that the path works with only the permissions above.

### 6.3 Trust tiers

`slots` computes a tier for each job from `trust_inputs`, never from labels, and compares it with the tier of the set that delivered the job. With the remote mint, `mint` computes it too (§5.3). The rule is default deny. Its reference implementation is `tools/tier_rule.py`, its cases `examples/tier-cases.json`. `tools/check_tiers.py` compares the two blocks below with the implementation. The first block holds the event lists, the second the names of the conditions:

```tier-rule
trusted_events: push, pull_request, workflow_dispatch, schedule, merge_group
untrusted_events: pull_request
refused_events: pull_request_target, workflow_run, issue_comment, release, repository_dispatch, other, unknown
```

```tier-prose
trusted    iff ALL of:
             visible:   repo_visibility in {private, internal} OR owner/repo is listed in trusted_public
             owner:     owner is one of the configured organizations
             not_fork:  head_is_fork is false
             member:    actor_class is member
             event:     event is in trusted_events
untrusted  iff ALL of (and the job is not trusted):
             event:     event is in untrusted_events
             fork:      head_is_fork is true
refused    in every other case, including every unknown value: code event-not-allowed
local      never computed. Only by explicit configuration of a set
```

Only a fork `pull_request` is `untrusted`: GitHub withholds secrets from it (Documented by GitHub, not read this session). Every other job that is not `trusted` is refused with `event-not-allowed` and does not run in a guest, because it can carry secrets or an author nobody vetted (Inferred from GitHub's event model; not run). A guest cannot contain a job that holds secrets and runs hostile code. Moving an event into `trusted_events` reverses that. The reason for each refused event is in `APPENDIX-THREATS.md`, notes on §6.3.

`ref` and `workflow_ref` are not inputs. The reference rule compares names. When an offer carries `owner_id` and `repo_id`, WP-03 compares ids too, so a renamed or re-registered name does not inherit trust (Unverified that the run object carries them).

**Intake check.** A job with no tier is `event-not-allowed`. A computed tier less trusted than the set's is `tier-mismatch`: a fork `pull_request` delivered by a `trusted` or `local` set. Both are refused before the claim: not acquired, not released. The source gets `withdraw` with the code, and the `job_id` is suppressed for `terminal_suppress_s` (default 3600). A source that cannot fail an unacquired job by name leaves it queued until it expires (Unverified, U18). A `local` set accepts only jobs that compute to `trusted`. A job more trusted than its set runs under the set's stricter tier.

- `untrusted`: minimum `boundary` `guest`, fixed in code. Egress: allow list, hold `enforced`. No cache in v1. Secrets: none expected, because GitHub withholds them from fork pull requests. The token GitHub injects is the forge's own rule (githome rule 3).
- `trusted`: minimum `confined`, or `redirect` only if the owner answers yes to DR-4. Egress: allow list, `open` only for repositories the configuration lists. Secrets as the repository grants.
- `local`: `none` allowed, the host's own egress. The `host` kind is deferred, so no `local` set is served in v1.

GitHub trust inputs: the run object carries `event`, `actor` and `head_repository.fork` (Documented, R5:8.4). The scale-set message carries `workflowRunId`, so the source reads the run, which needs repository "Actions: read". The `member` check needs organization "Members: read" (Inferred from the GitHub permission model; not run, U12), and a key with it can read the member list: a stated cost. A failed `member` lookup leaves the actor `unknown`: a fork `pull_request` stays `untrusted`, any other job is refused. A failed run-object lookup leaves the fork flag `unknown` too, so every job, a fork `pull_request` included, is refused with `event-not-allowed`. A missing permission turns every job `unknown`: closed and total. `podrunner status` counts `trust-lookup-failed` and `trust-lookup-throttled`. The source caches the run object by `run_id`, sets an hourly lookup budget, keeps trusted jobs it already resolved when the budget is spent and refuses new `unknown` ones without more calls. The set of refused `job_id`s is bounded in count and age.

The edge cases of the trust inputs (re-runs, the merge queue, reusable workflows) are Unverified (U23), and the reason for the `guest` floor is argued in `APPENDIX-THREATS.md`, notes on §6.3.

### 6.4 Payload

The pin list names the payload: an image by digest, an argv, the credential delivery form (G-01, R6 B7). The official `actions/runner` (MIT) ships for linux x64 and arm64 glibc, and armv7 with Node 20 only: no musl, riscv64, i686, loongarch64 or ppc64le asset (R4:2.2). It must be updated within 30 days of a release (Documented). On armv7 the asset carries `node20` only and main forces `node24` for JavaScript actions (Source, `HandlerFactory.cs:58-59`), so armv7 loses JavaScript actions once an asset built from main ships. Alternate payloads and a native runner are in §15.

### 6.5 Placement

`slots` decides the unit kind in these steps over the capability records. A failing step ends with the named refusal, and a refusal before the claim goes to the source by `withdraw` with the code.

1. `arch` of the offer equals an instance of this host: else `arch-mismatch`.
2. Compute the tier. A job with no tier is `event-not-allowed`. Apply the intake check: else `tier-mismatch`.
3. `effective_boundary` is the maximum of the tier minimum and `unit_need.min_boundary`. An offer can raise the minimum, never lower it.
4. Candidates are the configured kinds, in order, whose record satisfies all of:
   - `boundary` at least `effective_boundary` (else `tier-needs-guest` for `untrusted`, `boundary-not-available` otherwise), and `arch`;
   - each limit of `limits_required` is `enforced` or `reported` (else `limit-not-enforceable`). A job that names `wall` also needs `tree_kill: verified`;
   - `egress_hold` at least `enforced` for `untrusted`, `advisory` for `trusted` with an `allow` or `open` policy, `none` only for `local` (else `hold-not-available`);
   - the payload's `credential_delivery` is in `secret_delivery` (else `unit-start-failed` at start);
   - when `unit_need.container` is true, or the set says `containers = true`, `container` is not `none` (else `container-not-supported`). `unit_need.ports` is ignored in v1, because port claims are deferred (§15), and the control socket refuses `ports` on a `shared-host` unit (§7.2);
   - `mount_modes` holds `ro` where a mount needs it.
5. The job's egress policy is intersected with the tier's ceiling. An offer can only narrow it. Path rules while the broker has no TLS end are `egress-policy-unsupported`.
6. The payload comes from the offer only for a `payload_from_offer` source. Otherwise `slots` takes it from the pin list entry of the set, ignores and audits a `payload` in the offer, and an offer naming a digest outside the pin list is `payload-not-pinned`. A pin past its age limit is `image-stale`, checked here and again at `start` (§11.3).
7. The first candidate with a free slot wins. A slot reserved for a tier (`slots_reserved`) is not given to another tier, and a tier is held to `slots_max` (§11.7). With none free the job waits (`no-slot-of-kind`) until `wait_deadline_s`, then `withdraw`. A kind on a `shared` principal has `max_concurrent` 1 (§5.3).
8. `prepare`, `claim`, create the credential directory, `mint`, write the files, `egress.load`, `start` (§11.1).

A host whose kinds satisfy no tier minimum starts no source instance, and `status` says `no-tier-served`. A host that serves another tier and meets none of this one's minimum refuses with `boundary-not-available`.

### 6.6 The `guest` floor is tested, not declared

`boundary_proof` is a string the driver supplies. The `guest` floor is therefore accepted only from driver kinds on an allowlist (`microvm` and the kinds the operator lists), and only after a start-up self-test. The test runs at `slots` start, on every driver change, and daily on the unit's own start path with a mount. It starts a unit and checks: no network interface; a canary file and a loopback listener planted by `slots`, with names the unit does not know, are not visible or connectable; a second unit cannot see the first; the kernel boot id differs from the host's and no host pid is visible. On every `guest` start `slots` verifies that every mount is read-only or a block device image and that the unit sees only the table's host paths. The result goes into `boundary_proof`. `fake` is refused unless `allow_fake = true`, and the report shows it. Driver executables are resolved from one configured directory with owner and mode checks, never from `PATH`. A driver that claims `guest` over a plain process fails the test (conformance test 11).

---

## 7. Containers: hooks first, Docker API later

### 7.1 What container jobs need

The official runner shells out to `docker` for sixteen verbs (`create`, `exec` with piped stdin, `network create`, `inspect`, `login` and others) and never speaks the Engine API itself. It requires Engine API 1.35 or newer (Source, R4:2.5). A Docker API for these verbs is deferred behind a census (§15).

### 7.2 Path C1: the official container hooks

`ACTIONS_RUNNER_CONTAINER_HOOKS` names a `.sh`, `.js` or `.ps1` script. With it set, and with the job variable `DistributedTask.AllowRunnerContainerHooks` true, the runner makes no `docker` call for job containers, service containers, container actions or pre-pulls (Source, R4:2.6, `FeatureManager.cs:8-12`). The four commands are `prepare_job`, `cleanup_job`, `run_script_step`, `run_container_step`. "There is no partial opt in" (Documented, ADR 1891). The interface as the runner reads it is `contracts/hook.md` (Source, M1 Q5; no job ran, and the example hook was checked against the author's own mirror of the runner's checks). The hook receives `environmentVariables` and `workingDirectory`, which podvm's `exec` does not inherit, and `isAlpine` in the response replaces the `docker exec` probe that picks the Node build.

**The hook relays and the socket validates.** The hook is a `.sh` script in the unit image. It sends its stdin unchanged to the unit's control socket (`curl --unix-socket`, `POST /v1/hook/COMMAND`) and appends the reply to the response file. It parses nothing. The image ships `bash` and `curl`, not a static binary: Observed (`measurements/m2-podvm/logs/02-images.log`) that on a chroot-denied host podvm refuses a static payload with exit 125, and a Debian `bash` runs (`debian-bash-echo` exit 0). `curl` under the interposer, and the visibility of a host socket inside the unit, are Unverified (U21).

The host-side server of the control socket (`ctl`) enforces the closed field set, because a step in the same uid can skip the hook (the rules are in `contracts/unit-driver.md`). The server cannot tell the runner's `prepare_job` from one that a step posts, so its rule on images is not a boundary. An image of `container_start` or of a `run_container_step` is accepted only from a registry host that the source allows (`registries`), and the pull goes through the egress policy. A forged `prepare_job` can name only such an image, which a step could fetch anyway through allowed egress. `prepare_job` is accepted once for each unit, before any step command. The runner sends none for a job with no `container:` and no `services:` (Source, `JobExtension.cs` of actions/runner main, lines 318 to 328, read 2026-11-10), so the server refuses every container operation of such a job, container actions included.

The real controls do not depend on the sender: two fixed mount tables, seven entries for a job container and five for a container action, each a directory of the unit (`contracts/unit-driver.md`; Source, `ContainerOperationProvider.cs` and `ContainerActionHandler.cs` at `310a370bf360`: the runner builds two different sets); no host path, device or privileged flag; no network mode but the default; `ports` refused on a `net: shared-host` unit. Every other field and route is `container-option-refused` with the field name. The runner adds `/var/run/docker.sock` to the system mounts of every job container and container action (Source, `ContainerInfo.cs`, `ContainerActionHandler.cs`, same pin). The server accepts that one entry, matched on source and target as the literal string, and drops it: no engine is mounted. Every other socket bind is refused. A `services:` entry on a `net: shared-host` unit is `container-not-supported` (§7.3). A refusal fails the step with a named reason in the job log. Job images may be tags: the driver resolves each to a digest and `slots` records it. Only the payload image must be a digest. On a `redirect` unit these controls are not a boundary against the job's own steps, which run as the same uid (§5.3; Inferred, not run on H0). They matter for `confined` and `guest` units. A container action runs only in a job with `container:` or `services:`; that it runs on the rung is Unverified (U19).

The socket also carries `POST /v1/job_state`. `start` sets `ACTIONS_RUNNER_HOOK_JOB_STARTED` and `ACTIONS_RUNNER_HOOK_JOB_COMPLETED` to relay scripts that post the event. The first moves the job from `started` to `running`, and only from `started`. The second records the time and changes no state, so a step cannot end its own job as `done`. For a payload that is not the official runner, `running` is set when `start` returns.

Conditions that make the route unusable:

1. **GitHub controls the job variable.** Whether github.com sends it on every job is Unverified (U6). The feature is documented (public preview, Linux only), and Actions Runner Controller's Kubernetes mode uses it (Inferred, R4:2.6). Gate G-H1 needs a live job, which Q-A9 forbids until DR-7, so the route is "probable" for planning only. If G-H1 fails, the Docker API of §15 reopens.
2. **Half-set behaviour.** With the environment variable set and the job variable false, the runner takes the docker path with a null manager (Inferred from source; U14). `podrunner` sets the variable for every unit with `container: hook`.
3. **Official runner only.** A step that runs `docker` itself, GitLab's docker executor, the Gitea and Forgejo docker labels and testcontainers need an Engine API. The official k8s hook needs Kubernetes and the docker hook wraps the docker CLI (R4:2.6), so the hook for podrunner is new, script-sized code.
4. **`container:` on the no-root rung.** Whether a job container whose image differs from the unit's can be honoured on `net: shared-host` is Unverified (U19). Where it cannot, `container-not-supported`.
5. **A `microvm` unit has no hook.** It reports `container: none` until a guest runner image exists (Documented, R5:8.3). A stated need is refused at placement (§6.5). A GitHub job discovers it at hook time, and a guest has no hook, so the runner fails with its own docker error. The `untrusted` set is configured `containers = false`, and a workflow that needs containers does not name it.

### 7.3 Services, networks and ports on shared-network units

The no-root rung shares the host network, so a service port is a host port (Source, R1:2.3). The plan's loopback addresses `127.x.y.z` for each slot are unproven: podvm cannot make a payload bind a chosen address (R1:4.4), and `podnet pod` does not apply its rules to `127.0.0.0/8` (Documented, R6 A12, R2:1.2). A service bound to `0.0.0.0` in job A is reachable from job B on `127.0.0.1` and from the LAN (Inferred, R2:1.2). So v1 refuses `services:` on a `shared-host` unit with `container-not-supported`. Port claims and a host-exclusive lock are deferred (§15). A job that needs services takes a `microvm` slot once a guest runner image exists, or a host with a real engine. Name resolution inside a unit is per-unit hosts entries, not a rootfs shared between containers.

### 7.4 `docker build`, and what the container paths do not give

`docker build` and Dockerfile actions are refused with `build-not-supported` (Q6 of #47, kept). podvm `build` and `commit` are `None` (Source, R1:3.2). docker CLI 29.9.3 defaults to BuildKit (a buildx plugin the static tarball lacks) and falls back to `POST /build` only with `DOCKER_BUILDKIT=0` (Observed, R4:6.4).

On the no-root rung, `container_exec` re-enters a root that every container of one image shares (C30, §7.2 condition 4). `probe` says `container: hook` with `net: shared-host`, and `status` repeats it. A workflow that expects Docker semantics needs a host with a real engine. On podvm, `--health-cmd` and related options are refused with `container-not-supported`: `inspect` has no health state (Source, R1:2.2, `parity.rs:278`).

---

## 8. Credentials

Three credentials exist, and the plan treats them as one:
- the **source credential** (App key, PAT, forge token): long-lived, on the `mint` host (another machine on H0, §5.3). Only the `sign` module holds the App key;
- the **session and host tokens** (scale-set session, installation token, host token of the remote mint): in memory, minutes to an hour, held by `mint`, the sidecar and, for the host token, `slots`;
- the **per-job runner credential** (the JIT blob, three files): the credential directory of one unit, one job. `slots` holds it until written, then the unit.

### 8.1 The source credential

- **Organization-level GitHub App, minimum permissions:** "Self-hosted runners: read and write" at organization scope, repository "Actions: read" for the run object, organization "Members: read" for the `member` check (a leaked key can then read the member list). Repository-level mint needs `administration: write` and is refused (`repo-scope-refused`). A PAT is not used for a `trusted` source unless fine-grained and organization-scoped.
- **A signer behind a boundary.** `sign` signs one request shape (fixed `iss`, `iat`, `exp` within ten minutes), refuses any other claim and is rate limited to one request a minute. The sidecar calls it because the `actions/scaleset` client accepts a custom JWT signer (Documented, R4:3.2). `podkey` (an empty repository) would replace it. The signer bounds persistence, not use: a caller can obtain installation tokens while it runs, and a compromised sidecar has that reach. On a one-principal host the signer is not on the unit host (§5.3).
- **The key edits policy.** The permission that mints also creates, edits and deletes runner groups and replaces a group's repository list (Documented, R5:8.4). A leaked key is a policy-editing key and a source of runners of the attacker's own. Mitigations: a set for each tier, alerts (§8.5), rotation, `status` showing App id and key age.
- **Revocation.** A refused credential (`source-revoked`) ends `mint` with the `Revoked` exit class of `pod-core` (T-101), names the credential, never prints it, stops polling and does not restart in a loop (A-C-07). `slots` drains.

### 8.2 The per-job runner credential, as the runner behaves

Observed by M1 (`APPENDIX-MEASUREMENTS.md`, which holds the detail; runner v2.338.2, synthetic blob, no job):
- Both documented routes expose the blob: `--jitconfig` in `/proc/PID/cmdline` of three processes, `ACTIONS_RUNNER_INPUT_JITCONFIG` in `/proc/PID/environ` of the same three for their whole life (2 of 2 rounds). No window closes.
- The runner writes `.runner`, `.credentials` and `.credentials_rsaparams` (the private key) into its root with mode 644 (`Runner.cs:234-256`), keeps them, and the Worker reads `.runner` from disk (Source, R4:2.1, `JobRunner.cs:47`).
- With the three files pre-staged at mode 600 or 400 and no argument or variable, the listener long-polls and `cmdline` and `environ` hold none of the markers (`logs/07-prestaged.log`).
- Inferred: steps run as the runner's uid. Unverified: whether the Worker and steps inherit the variable.

So "readable once" breaks the Worker, and "never readable by a step" is a uid boundary the official runner does not draw. **Route 1 is the only route**, pre-staged files with no argv and no environment:

1. The source driver decodes the blob. `mint` hands `slots` the decoded files (the `files` map of its answer). `slots` writes them into the unit's credential directory, mode 0600. `slots` creates that directory, mode 0700, in the state directory.
2. Before the payload starts, the driver copies the directory into the runner root (the unit's workspace root), mode 0600, owner of the payload. `destroy` removes both.
3. On H0 the directory is a plain directory, because H0 has no mount: `probe` reports `credential_store: private-dir` (Observed on the M2 host: `mount(tmpfs,/mnt)` is denied, `measurements/m2-podvm/logs/01-probe-rung.log`). Between the write and `destroy` the files are on disk: a stated limit. On a `guest` unit the directory is the guest's tmpfs.
4. The egress identity is the per-unit listener, not a token in the unit (§9.5).

The blob never reaches a log, report or audit line. Not shown by M1: that the files survive a token refresh over hours, and how a real blob differs from the synthetic one (U20).

### 8.3 What a stolen per-job credential can do

Unverified, level 3 (U11). Documented: a JIT runner performs at most one job (`gh-secure-use.txt` line 120). Inferred: a step that reads the files can impersonate the runner for this job and forge its result and log, and cannot acquire another job, because a second session for the same runner id conflicts (session conflict exit code 5, R4:2.3). The blast radius is one job only where units have distinct principals. On a shared principal a step reads every concurrent and every later unit's files, so it is every job on the host (§5.3).

### 8.4 G-03 restated

G-03 as written: "a step of the job can never read it: not from the file, not from `/proc/*/environ`, not from argv." The official runner cannot meet "not from the file" where runner and steps share a principal. Route 1 meets the other two. Acceptance for the official runner:

1. After start, no process of the unit has the credential in argv or `environ`. Test: a step dumps `/proc/*/cmdline` and `/proc/*/environ` and finds no byte of the three files. Positive control: the dump finds a blob planted in an environment-route scenario (`measurements/m1-runner/scripts/sampler.py`).
2. The files exist only in the credential directory and the runner root, and are gone after `destroy` (host scan, with a planted file as positive control).
3. A step that reads the files affects only its own job, where principals are distinct (level 3).
4. The strong form needs a payload that takes its credential from a socket the host serves. No official payload does. `credential_delivery: socket` is reserved for it (§15).

DR-6 asks githome and cfb to accept this restatement. If they do not, G-03 stays open for the official runner.

### 8.5 Leak response

A leaked App key is a policy-editing key, and the attacker can register runners that take trusted jobs with every repository secret. The host sees nothing. The runbook:

1. Rotate the key. Delete the runners and scale sets of the installation that `status` does not list.
2. Install the App on named repositories only. Verify that each `trusted` set's runner group still refuses public repositories (§6.2).
3. Alert on any registration whose name is not in a host-written list (detection, not prevention; level 3 tests it). Check the audit log of runner-group edits since the last known good key age.
4. A leaked host token or key is revoked at `mint`. It could fetch credentials only for offers in the ledger of its own host, and the number of those is the host's configured ceiling. `mint` also revokes on a credential with no matching start.

---

## 9. Egress

### 9.1 A plan per unit kind

The offer's `policy` says what the job may reach. The unit's `egress_hold` says how the hold works. `slots` joins them, and a mismatch is a refusal by name (§6.5).

- `microvm`: no NIC, a vsock channel to the broker. Hold `enforced` when the broker is the only channel (Documented for the archive launcher, R5:8.3).
- `podvm` userland with `podnet pod`: hold `advisory`. It passes unix, vsock and netlink sockets and host loopback TCP (Observed, R2:1.2 items 1, 2). Not a boundary against hostile code.
- `podvm` userland with proxy variables: hold `none`, because Node `fetch` and many tools ignore `HTTPS_PROXY` (Documented, plan §3). The job token is then an environment variable of the unit, the one named exception to "no credential in the environment".

**The fixed denies and the allow list are enforced only where the hold is `enforced`.** On `advisory` or `none` they apply to connections that reach the broker and nothing else: a compromised dependency opens a raw socket to a metadata address, a LAN address or a host loopback service and never touches the broker (R2:1.2, R1 T-1652 open). The report and `status` carry `egress_hold`. G-04 is met for v1 on `enforced` holds only. A `trusted` job on an `advisory` host runs with an allow list by default. The runner reads only `http_proxy`, `https_proxy`, `no_proxy` and their upper-case forms (Source, `RunnerWebProxy.cs:47-64`) and passes them into job containers.

### 9.2 v1 and v2 of the broker

- **v1 is T-099, done.** `pod-broker`: per-job `load`, `drop`, `list` over a unix control socket (mode 0600, peer uid check), tokens kept as SHA-256, TTL, modes `allow` and `open`, fixed denies (metadata, private, CGNAT, loopback, link-local, multicast, reserved, port 25), one JSON audit line a connection. 55 tests (Observed by R3: `cargo test -p pod-broker`, exit 0). v1 does not check SNI against the CONNECT target, so an allow list that holds a multi-tenant host (storage, CDN, pages) is an exfiltration path for the job's own data (Observed: `crates/pod-broker/src/tunnel.rs` header).
- **v2 is T-100, open.** The TLS end with a CA per job, placeholder swaps, path rules. No production code mints a certificate today (Observed, R3:1). Deferred (§15). #47's "T-100 or a CA proxy" for the phase 0 capture is dropped: the capture is needed only for a native runner.

### 9.3 The hold is a capability

`probe` reports `egress_hold` with a proof. A job whose policy needs more than the unit offers is refused (`hold-not-available`): A-V-02's rule ("refuse the run where no hold mechanism works, never run unheld") as a matching rule. `untrusted` requires `enforced`. The choice between `pod-hold` and `pod-sud` is open (T-109, podnet#414) and belongs to podcrates and podnet.

### 9.4 Name-forwarding upstream

`pod-broker`'s `upstream` takes one `Option<Proxy>`, and the broker dials a resolved and checked address (Source, `serve.rs:273-287`, `broker.rs:183-191`, R2:5.1). The podnet sandbox (measured 2026-11-01) resets CONNECT to an IP literal and answers UDP 53 (Documented, `podnet:docs/target-environment.md:24,30`). The podssh sandbox (measured 2026-10-04) has no working resolver, and its proxy allows ports 443, 80 and 8443 (Documented, `podssh:docs/target-environment.md:21,24`). Inferred: a broker that dials a checked address fails on the first host and cannot resolve on the second. Not run against the broker (U10).

WP-C1: when the upstream is a CONNECT proxy that takes names, send the name, checked against the deny list and against a `pod-dns` resolve as a pre-check. That opens a rebinding window, so four requirements apply (`APPENDIX-DEFERRED.md`, name-forwarding): the upstream proxy refuses non-public addresses and the broker tests it with a canary name at start; without that, name-forwarding runs with hold `advisory` and never serves `untrusted`; allow-list entries under attacker-registrable suffixes are flagged; a fixture test uses a rebinding resolver.

The broker resolves through `pod-dns`, never a hardcoded list. podnet's default relay (one Cloudflare Worker) and its DoH-by-IP addresses are never on the job path (R2:5.1).

### 9.5 Listeners

`egress.load` binds the job token and policy to one listener for each unit, a unix socket `egress/UNIT.sock`. The socket is the identity: no token reaches the unit on the advisory path. Whether `pod-broker` v1 serves one listener per unit or needs a thin wrapper is Unverified (U24). The forms of the `egress` object of `start` are in `contracts/unit-driver.md` (`unix`, `tcp-loopback`, `vsock`). The driver composes the redirect and reports `egress_hold` from what it set up. The podvm driver depends on `podnet` for the redirect and reports `none` without it. Shared-network units reach a unix listener through `podnet pod -x socks5://unix:PATH`. Where the operator reports that binding sockets is refused (`podssh:docs/target-environment.md:25`), the unix form is the only one. `podnet pod` has never been run with the .NET runner or any CI payload (Documented absence, R2:1.2 item 8; U9).

### 9.6 Hosts the runner reaches, and the default policies

The control-plane hosts of the official runner come from GitHub's "Self-hosted runners reference" (Documented, plan §2.1): `github.com`, `api.github.com`, `*.actions.githubusercontent.com`, `codeload.github.com`, `*.blob.core.windows.net`, `ghcr.io`, "and others". The list moves: the broker flow changed in 2025 (plan risk 5). `*.blob.core.windows.net` is multi-tenant and an exfiltration path (§9.2). `slots` appends the `control_hosts` of the source's `hello` to every `allow` list. The default `untrusted` policy is `examples/valid/job-offer.fork-pr.json`, validated against the broker schema by `tools/check_schemas.py`: mode `allow`, exactly those hosts, port 443. The default `trusted` policy is the same list, and `open` only for repositories the configuration lists. `pod-services` has no Actions hosts today (Observed, R3:2.2): WP-C4. The operator adds the registry hosts that `container:` images need.

---

## 10. Units and host classes

### 10.1 Unit kinds

Build order: `fake`, `podvm`, then the kind that serves `untrusted`. The kinds `docker`, `incus` and `host` are in §15.

- **`podvm` userland rung.** Needs no root, userns, mount, ptrace or KVM. **x86_64 only** (Source, R1:2.3). Boundary `redirect`, container `hook`, net `shared-host`. The target of A-V-04. The unmodified official runner does not start on it today (§10.5). `--secret` (T-1696), `--report` (T-1695), limits and tree kill (T-1655) are open. `-v` is Degraded, `:rw` only.
- **`podvm` namespace or chroot rung.** Same flags, boundary `redirect`, not the target class.
- **`microvm`.** Needs `/dev/kvm` for the user, a kernel, an image. **x86_64 only.** Boundary `guest`, container `none` until a guest runner image exists, net `private`. podvm `run --podvm-tier=machine` exits 125 for Linux (C1: it prints "running a guest arrives with TODO/machine.md T-1303 and T-1304"; Observed again by M2). The archive launcher runs a shell script, not the runner, and needs passwordless `sudo` and `setpriv`, so a port for a user in group `kvm` is a privilege-model rewrite (Documented, R5:8.3, `APPENDIX-EXISTING.md`). aarch64 has no `untrusted` unit.
- **`fake`.** Any declared capability. Refused in production unless `allow_fake = true` (§6.6).

Every podvm rung is `redirect`, the namespace and chroot rungs included. The chroot rung changes the filesystem path only, and the namespace rung adds a mount namespace and a private tmpfs, with no pid, user or network isolation (Source, R1:2.3). `run` applies no `NO_NEW_PRIVS`, seccomp or capability drop (Source, R1:1.3). `confined` is reserved for a kernel-enforced unit: a container engine, or Landlock plus seccomp once `pod-confine` exists. No podvm rung reports it until podvm applies confinement and a conformance test shows it.

The podvm driver maps podvm's own words to the capability record. podvm's rungs are `memfd`, `fuse`, `tmpfs`, `rundir`, `cache`, and `probe` prints a mode word (Observed, M2 `01-probe-rung.log`: `PODVM_MODE=userland` is rejected as "names no launch rung"). The driver reads `probe --json`, reports `boundary: redirect` for every rung and `unknown` for anything it cannot map, lists `mount_modes: ["rw"]`, and never passes `--unsafe-host-paths`. Each unit gets its own `PODVM_STORE` under the state directory (podvm reads it first, Source, `podvm-image/src/store.rs:220`), so containers of two units never share a rootfs. The driver loads the image into it (`podvm load` of a saved tar, Source, `parity.rs:179`) and `destroy` removes it. The cost is one extraction and one image copy of disk for each unit (WP-V5 removes the cost, not the capability). `container_exec` maps to `podvm exec -e K=V -w DIR` and refuses `stdin: true` and a workdir inside a volume (`APPENDIX-DETAIL.md`).

### 10.2 Host classes

- **H0** (x86_64, no root, userns, mount or KVM): `trusted` only, if the owner accepts `redirect` (DR-4), with the minting side on another machine (§5.3), once the official runner starts on the rung (WP-V1), and once `podnet pod` holds with it (WP-N1). Never `untrusted`. Unit `podvm` userland, container route hook. With the DR-4 default, no tier.
- **H1** (userns or chroot allowed, no KVM): `trusted` only after DR-4. Unit `podvm` namespace rung.
- **H2** (`/dev/kvm` usable, x86_64): `untrusted` and `trusted`. Unit `microvm`, container route none until a guest runner image exists.
- **H0a** (aarch64 or other non-x86_64, no root), **H3** (a Docker or Podman daemon) and **H4** (a dedicated machine the owner trusts): no tier in v1, because the `host` and `docker` kinds are deferred (§15).

A host that serves no `untrusted` tier does not poll that tier's set: a polled job would sit assigned until re-queued, up to three times (Documented), and the wait would look like a runner fault.

### 10.3 Per-job start environment

`slots` passes the start environment in `start.env`, not the image, so a pin cannot drop it: `DOTNET_EnableWriteXorExecute=0` (§10.4), the hook variables (§7.2) and `ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE=1` (§11.3). It sets none of `RUNNER_TOOL_CACHE`, `RUNNER_TOOLSDIRECTORY` and `AGENT_TOOLSDIRECTORY`, which keeps the tool mount target at `/__w/_tool` (`contracts/unit-driver.md`). The `runner_root` of the contract examples is not a requirement.

### 10.4 Limits, the .NET constraint and the tree kill

- **Never put `RLIMIT_FSIZE` on the runner, and always set `DOTNET_EnableWriteXorExecute=0`.** Observed by M1 (`APPENDIX-MEASUREMENTS.md`): with the default W^X the runtime calls `ftruncate` on a memfd with exactly 2 TiB, so any `RLIMIT_FSIZE` below that gives EFBIG, SIGXFSZ and exit 153. The runner died at start under limits of 20 GiB, 1 GiB, 128 MiB and 1 MiB, and always started with W^X off. A limit of 2 TiB or more was not run (Unverified, inferred yes). The cost of W^X off is unmeasured (U13). A-V-07's `--ulimit fsize` is not applied to the runner.
- **Other start-up requirements** (Observed, M1, `APPENDIX-MEASUREMENTS.md`): `DOTNET_SYSTEM_GLOBALIZATION_INVARIANT=1` or ICU, OpenSSL (`libssl.so.3`), and a glibc dynamic loader with symbols up to `GLIBC_2.28`, so musl is not enough. A host with `vm.memfd_noexec` set would also break the W^X memfd (Inferred, never run).
- **Disk.** H0 has no mount, so no sized tmpfs and no quota. `slots` watches the unit's workspace with `du` on an interval and ends the job with `limit`. `limits.disk` is `reported`. A job whose `limits_required` names `disk` on a unit that reports nothing for it is `limit-not-enforceable`. The report states which limits applied.
- **Wall timeout and the tree kill.** `slots` owns the wall clock: `signal`, wait the grace period, `destroy`. The 60-second acceptance of A-X-01 and G-12 needs a verified tree kill. podvm's `stop` kills the payload only, and `pod-supervise` signals one pid (Source, `launcher.rs:351`). The runner's own cleanup kills each step's tree and processes carrying `RUNNER_TRACKING_ID` at job completion (Documented, R2:3, from `podssh:docs/detach.md`, not re-read), which misses grandchildren that left the tag. Observed by M2 (`05-lifecycle.log`, 6 of 6 runs): for `sh -c 'sleep N & sleep N.5 & wait'` under `run -d`, `stop -t 1` and `rm -f` exit 0 at once, and both `sleep` processes survive with ppid 1. The launcher has ppid 1 before any stop, so a walk of the driver's children sees nothing. So the 60-second end fails for any payload that forks, which the runner does.

  The route: one subreaper shim for each unit, started by the driver as the parent of the unit's top process. It sets `PR_SET_CHILD_SUBREAPER`, so orphans of that unit reparent to that unit's shim, not to pid 1 or another unit's shim. Containers that `ctl` starts are spawned through the same shim, so they are descendants of it. Every process of the unit carries `PODRUNNER_UNIT` with the unit id in its environment. `destroy` stops the containers the driver recorded, walks the shim's descendants by the `/proc/PID/stat` parent chain, sends SIGKILL, waits and counts. A process that does not reparent to the shim is not a descendant, so while the shim lives the count is per unit by construction. WP-C2 is the same mechanism in podvm's launcher. Inferred from the kernel's reparenting rule and not run: until a run passes conformance test 3 with the M2 payload, `tree_kill` is `unverified` and `status` says so.

  The shim runs as the unit's uid on H0, so a step can kill it, and its children then reparent to pid 1 or a subreaper above it. If the shim is not alive at `destroy`, the driver cannot prove emptiness: it reports `residual_processes` as `unknown`, and `slots` treats `unknown` as above 0. The marker rules, `unknown_end` and the argv of the loader family are in `APPENDIX-DETAIL.md`. A count above 0 or `unknown` puts the host in `degraded`: `capacity` is 0 in `next`, unclaimed offers are withdrawn, and no new job starts. An operator clears it by removing the residue and restarting `slots`, whose reconcile (§11.6) leaves `degraded` only when it counts no residual process. A trusted job can cause `degraded` on purpose, a denial of service that this design accepts.

### 10.5 What M2 measured on the userland rung

Observed, one host (Debian 13, podvm 0.1.0-beta.13, uid 1000, lxc; `APPENDIX-MEASUREMENTS.md`). It matches H0 on chroot, mount and namespace mounts, which are denied. It is more permissive than H0 in three ways: `unshare(CLONE_NEWUSER)` succeeds, `/dev/kvm` opens read-write and ptrace is allowed (`01-probe-rung.log`). Results that depend on those three are not shown for H0, and no measurement exists on a host that denies ptrace and user namespaces too.

The unmodified official runner does not start on the default rung: exit 154, then exit 137 after a workaround. It runs only under `--unsafe-host-paths` (U1, WP-V1). podvm has no limit flags. Runner image runs take 5.4 to 6.1 s in the cases that extract it, and `podvm run --rm alpine true` takes 4.83 to 5.36 s with an empty store and 0.26 s with the image in the store (3 runs each). All containers of one image share one mutable rootfs (C30), and grandchildren survive `podvm stop` (U4, U5).

---

## 11. Lifecycle, images, reports, control

### 11.1 Job state machine and refusal codes

Happy path: `offered`, `matched` (tier and kind chosen, a slot free), `claimed` (the source acquired the job), `minted`, `started`, `running`, `ended` (with a reason), `destroyed`, `reported`. Others: `refused` (before the claim, the job stays on the source's side), `released` (claimed, will not run, given back), `failed`, `lost`. `lost` is a `podrunner` state, not a unit state (R6 B19). Every transition has a deadline key in `[deadlines]` (`APPENDIX-FLOWS.md`, configuration example). The values are design values, not measured.

| From | Event | To | Deadline key | On failure |
| --- | --- | --- | --- | --- |
| `offered` | tier computed, intake passed, a slot free | `matched` | `wait_limit` | A wait code until `wait_deadline_s`, then `withdraw`. A terminal code at once, by `withdraw` |
| `matched` | `prepare` answered, `claim` answered | `claimed` | `prepare_s`, `claim_s` | `release`, `unit-start-failed` |
| `claimed` | credential directory made, `mint` answered, files written | `minted` | `mint_s` | `release`, `credential-unavailable` |
| `minted` | `egress.load`, `start` answered, pin age checked again | `started` | `start_s` | `release` with `unit-start-failed` or `image-stale`. An unused registration is deleted at the source |
| `started` | `POST /v1/job_state` from the job-started hook (§7.2), and the `started` event of the source (`contracts/source-driver.md`) has the runner name that `slots` passed to `mint` and the job id of the claim | `running` | `running_s` | Deadline: `lost`. A mismatch destroys the unit at once, reports `failed` and writes the audit line `job-mismatch` |
| `running` | `wait` returns, or the wall clock ends: `signal`, grace, `destroy` | `ended` | `grace_s` | |
| `ended` | `destroy` answered, residual counts 0 | `destroyed` | `destroy_s` | A nonzero or `unknown` count is `failed`, an alert, and the host is `degraded` until an operator clears it (§10.4) |
| `destroyed` | `egress.drop`, report delivered | `reported` | `report_s` | Retry, then spool in the state directory |
| any | deadline passed | `lost` | that state's key | Destroy the unit, remove the credential directory, report `lost` |

`drain` during `minted` lets the job run. The code table is the one place that lists refusals. A `wait` code ends as `withdraw` at `wait_deadline_s`, reported `refused` with the code last current. A `terminal` code ends the job at once. A refusal before the claim goes to the source by `withdraw` with the code, and a terminal job is suppressed for `terminal_suppress_s` (default 3600) so a redelivering source cannot loop it. `tools/check_codes.py` checks the table against the report schemas.

| Code | Class | Raised | Source behaviour |
| --- | --- | --- | --- |
| `no-slot-of-kind` | wait | `offered` | queued, `status` names the kind |
| `port-claimed` | wait | reserved, no v1 path raises it (§15) | |
| `image-stale` | wait | `matched`, `minted` | queued until a fresh pin exists. At `start`: released, re-offered |
| `event-not-allowed` | terminal | intake | the job computes to no tier: not `trusted` and not a fork `pull_request` (§6.3). Refused before the claim |
| `tier-mismatch` | terminal | intake | refused before the claim, not acquired |
| `tier-needs-guest` | terminal | `matched` | an `untrusted` job on a host with no `guest` unit |
| `boundary-not-available` | terminal | `matched` | no kind reaches the tier minimum |
| `limit-not-enforceable` | terminal | `matched` | a required limit no kind enforces or reports |
| `hold-not-available` | terminal | `matched` | the policy needs a stronger hold than any kind offers |
| `egress-policy-unsupported` | terminal | `matched` | path rules while the broker has no TLS end |
| `arch-mismatch` | terminal | intake | the architecture has no instance on this host |
| `payload-not-pinned` | terminal | `matched` | the offer names a digest outside the pin list |
| `repo-scope-refused` | terminal | intake | a personal-account repository, or repository-level mint |
| `container-not-supported` | terminal | `matched`, hook | a container need the kind cannot honour, `services:` on `shared-host`, a podvm health option |
| `container-option-refused` | terminal | hook, control socket | a field outside the closed field set, with its name. Also a second `prepare_job`, a container operation before the first, and an image of a registry outside `registries` |
| `build-not-supported` | terminal | hook | `docker build` or a Dockerfile action |
| `source-revoked` | terminal | any | the source credential was refused (§8.1) |
| `credential-unavailable` | terminal | `claimed` | `mint` failed |
| `unit-start-failed` | terminal | `minted` | `prepare` or `start` failed |

A report for a refused job has no unit fields (`schemas/run-report.v1.schema.json`).

### 11.2 Lost jobs and #4309

The issue reports GitHub marking some ephemeral runs "lost communication" (R4:2.4): one reporter, 0 comments, open. The plan says "check the state and retry", which can run a job's side effects twice, so the spec does not retry. It counts `ended_without_completion` for each source (a unit that ended without `JobCompleted` before the report deadline), shows it in `status` and alerts at a configured rate.

### 11.3 Image pipeline and the 30-day rule

The official runner stops receiving jobs within 30 days of a release when auto-update is off (Documented, `self-hosted-runners.md:125-135`). The twelve releases of 2026 (2026-01-09 to 2026-10-26, one a prerelease) are 1 to 48 days apart. With the 18 older releases of the same list, the gaps over all 30 run from 1 to 72 days (Observed 2026-11-10, `APPENDIX-MEASUREMENTS.md`). `max_age_days` 28 is a decision, not derived from the cadence.

- A **pin** is `{digest, built_at, runner_version, max_age_days}` in the pin list of the state directory (owner and mode checked). `built_at` comes only from the pin, never the image. A job's age limit is the smaller of the pin's `max_age_days` and the offer's `image_max_age_days`, checked at `matched` (`image-stale`, wait) and again at `start`. The pin list names the payloads a host accepts.
- **Detect** a stale runner with `ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE=1` (Observed by M1, `APPENDIX-MEASUREMENTS.md`: exit 7 comes from `GET /message`, and `run.sh` returns 7 only for the string `1`). The runner's exit codes (Source, `Constants.cs:150-162`, R4:2.3) are 0 success, 1 error, 2 retryable, 3 and 4 updating, 5 session conflict, 6 configuration refreshed, 7 version deprecated. `slots` reports 0 as `done`, 7 as `image-stale` plus a stale pin (it drains and asks the build pipeline for a new pin), anything else with the code in `exit_code`.
- **Build** with `podrunner image build`: download the release asset by version and a checksum recorded in the `podrunner` repository by a reviewed commit, extract into a unit image, record the pin. The builder needs root or a namespace: M2 built the runner image with `sudo chroot` and `apt` (Observed, `measurements/m2-podvm/README.md` Q3), and podvm has no `commit` (Source, R1:3.2). An H0 host imports an image that a build host made (DR-14). The build workflow runs on a GitHub-hosted runner, with a hosted fallback label as a standing requirement, because a lapsed image must not stop the runner that builds the next one (S12).

### 11.4 Reports (G-06)

One JSON report for each job, delivered to the source when the job ends, validated by `schemas/run-report.v1.schema.json`. The fields are listed in `contracts/source-driver.md`: `claim_to_first_step_s` runs from the claim to the job-started hook, and `job_images` holds the digests that job images resolved to.

Reports and `status` carry no credential. No field is free text except `boundary_proof`, which the conformance suite scans for credentials. A driver that cannot supply a field marks it `unknown`. podvm's report (T-1695) has no bytes or `limit` today (R6 B19). The full report stays local and goes to a source marked `operator_controlled`. Any other source gets the public subset, validated by `schemas/run-report-public.v1.schema.json`: `job_id`, `source`, `tier`, `unit_kind`, `boundary`, `wall_s`, `exit_reason` and the refusal codes. Where github.com shows a report is Unverified. The schemas move to podcrates `schemas/` by ruling Q-A8 (WP-C3).

### 11.5 Control API, status, exit codes

- Control: JSON lines on a unix socket, mode 0600, peer uid checked, `"v": 1` first (R3:1.1). The peer check protects nothing where units share the uid (§5.3). Operations: `version`, `status`, `drain`, `stop`, `capacity`, `withdraw` (G-08, R6 B10). `metrics` and `reload` are deferred (§15). `pod-control` (T-103) replaces the module later with no wire change.
- `podrunner status --json`: by source, then tier. Each source shows its slots; each unit kind its `probe` record, `principal` and `degraded`; the `refused` jobs with code and reason; `trust-lookup-failed` and `trust-lookup-throttled` counts; pins and ages; credential ages; `ended_without_completion`; `no-tier-served`; `arch` and `container` (G-05, G-09).
- Job-controlled strings (display names, branch names, labels) print only after control characters are replaced with `\xNN` and the string is cut to 200 bytes. A test puts an ANSI escape in a display name.
- Exit codes: `pod-core`'s table, 64 to 80 (T-101, done). podnet and podssh disagree with it (R2:2.3). The mapping from GitHub 401, 403, 404, 422, 429 and a revoked App to the classes is the first test of WP-07.
- `podrunner print-unit` writes a service unit with no token in it (two writers exist and disagree on `Restart=`, R2:0 item 5, so there is no third until T-102). `podrunner doctor` is a thin reader of `podvm probe`, `pod-hostprobe` and the self-test of §6.6 (DR-15).

### 11.6 Crash and restart

`slots` is the unit of restart, under systemd or the owner's service manager. `mint` and `egress` restart with a backoff, except `mint` after a `Revoked` exit (§8.1).

1. `slots` appends each state transition to a journal in its state directory, one JSON line each: job ids, unit ids and states, no credential.
2. On start `slots` reconciles. It asks each driver for its units and destroys every unit the journal does not show as `reported`. Jobs that never reached `claimed` are dropped without a report. Jobs from `claimed` on are released, reported `lost`, their credential directories removed, and unused registrations deleted. Source sessions are re-created. A host that was `degraded` stays so unless the reconcile counts no residual process.
3. `egress` dying makes `slots` signal the units, which fail closed. Nothing is offered until `egress` is back, and job tokens reload from the journal's live jobs.
4. `sign` failing delays minting until `mint` restarts it. Running jobs are unaffected.
5. Level 1 test: kill `slots` with a running fake unit, restart, assert no residual process or directory. Planted defect: skip the replay.

### 11.7 Configuration

One file, `podrunner.toml`, parsed into types that the `slots` crate owns (G-01's `[[source]]` table). The example with key names is in `APPENDIX-FLOWS.md`. Rules: each `[[source]]` has `kind`, `key`, `tier`, `arch`, `slots`, `labels`, `wait_limit`, `containers`, `same_principal_ok`, `affinity`, `operator_controlled`, `payload_from_offer`, `registries`. The effective count of a kind on a `shared` principal is `min(slots, 1)` (§5.3). `[tier.*]` carries `min_boundary`, and for `trusted` the `organizations` and `trusted_public`, and per tier `slots_reserved`, `slots_max`. `[deadlines]` has `prepare_s`, `claim_s`, `mint_s`, `start_s`, `running_s`, `grace_s`, `destroy_s`, `report_s`. A `local` set is a `[[source]]` with `tier = "local"`, the only marking of `local`. The tier minimums are checked at start: a file that lowers `tier.untrusted.min_boundary` is refused. The schema of the file is deferred (§15).

### 11.8 Caches

v1 has no shared cache: every job starts clean. The design for a later cache is in §15. G-07 is met trivially for v1 and open for the shared form (§12).

### 11.9 Audit

Every module writes JSON audit lines, each with job id, tier, unit id and source, for: `offer`, `tier`, `refuse` (with the code), `claim`, `mint` (job id and token id, never the blob), `sign` (caller and claims `iss`, `exp`), `control` (operation and peer), `hook-refuse`, `job-mismatch`, `payload-ignored`, `destroy` (residual counts), and the broker's `broker.connection`, `broker.job.load`, `broker.job.drop` (Source, `pod-broker/src/audit.rs`). No line carries a path, query, header or credential. A planted canary credential must not appear in any line of any module. On a `principal: shared` host the log is not tamper-evident. Shipping lines to a second host is deferred (§15).

---

## 12. The asks of #35 and #36 against this spec

The full table is `APPENDIX-ASKS.md`. Restated, meaning the acceptance changes and the ask's owner must agree (DR-6):
- **G-02** (unit kind, no fallback): the ask says the source states the unit kind each label needs. This spec computes it from `trust_inputs` and the tier, never from a label, because the workflow author names the label (§6.3, §6.5). No VM on the host means the tier is not polled.
- **G-03** (credential unreadable): the official runner cannot meet "not from the file" (§8.4).
- **G-09** (services, container jobs, Docker actions): the ask names a container engine inside a unit and no host engine socket. This spec uses hooks for the official runner, refuses `services:` on `shared-host` and defers the Docker API. `microvm` has `container: none` until a guest runner image exists, so the ask's `microvm` acceptance is open. Hooks meet it only if G-H1 holds (U6) and a Docker action runs (U19).

Open: G-04 for audit forwarding and placeholders (`audit` is specified and unbuilt, placeholders need T-100), G-12 (needs a verified tree kill, §10.4), G-07 for the shared cache form, and the cfb asks A-X-05, A-X-13, A-X-14, A-X-20, A-X-21, A-X-25, A-V-01, A-V-06 to A-V-11. Deferred with a trigger (§15): G-11. Partly met: A-X-01 (the Incus backend returns as a driver, §15), A-V-04 (measured: the runner does not start on the default rung, §10.5). Met: G-01, G-05, G-06 (fields depend on T-1695), G-08, G-10, A-X-15, A-X-17, A-X-18, A-X-22, A-C-07, A-V-02.

---

## 13. Acceptance, in three levels

The ruling Q-A9 (2026-11-09): "No Cloudflare, Zero Trust or GitHub test account as a CI secret. A live acceptance step that needs one waits; each ask keeps its offline tests and the stand-ins of `pod-testkit`." `pod-testkit` holds no stand-in for GitHub (C9), so level 1 keeps them in `podrunner`. #47's phase 1 live test and phase 0 capture need what the ruling forbids (C25). Acceptance levels are not trust tiers.

**Level 1, offline, runs in CI everywhere.** The `fake` source and unit prove the core's contract with a `sleep` payload and no runner. `slots` takes a clock trait and level 1 uses a manual clock, so no test waits real time. The fake source reads a script of timed events (`offer`, `cancel`, redelivery, `mint` error). The fake unit reads a script of behaviours for each unit (`exit N after T`, `fork-setsid-grandchild`, `fail start`, `residue N`) and spawns real processes where a tree kill needs them. Script formats are part of WP-02. Tests: G-01 (two sources with 2 slots each, 10 jobs each, none holds more than its slots); G-02 (a `microvm`-needing job stays queued on a host with only other slots, and runs when one appears); G-05 (an arm64 job stays queued on x86_64-only hosts); G-08 (one signal for each job that waits past `wait_deadline_s`, a withdrawn job never starts, after `drain` none is claimed); G-12 (a job that sleeps past its wall time ends `timeout` and its unit is gone within 60 seconds). The guards, each with a planted defect, are listed in `APPENDIX-ASKS.md`. A **driver conformance suite** covers every unit and source driver (`contracts/unit-driver.md`, `contracts/source-driver.md`), and `examples/tier-cases.json`, run by `tools/check_tiers.py`, covers each row of the tier rule.

**Level 2, offline with real Actions semantics.** The archive's seven-workflow conformance contract runs against `runner.server` and its patched listener inside a unit. It proves the unit can host real workflow execution, not the official runner against the broker flow, because the `runner.server` flow is V1 (R4:4.1). A **canned-message fake broker** (WP-12, after WP-V1) serves the runner's session, message, job and results routes from recorded or hand-written job messages and does. Detail is in `APPENDIX-ASKS.md`.

**Level 3, live.** Needs a GitHub test organization and an App (DR-7). Contents: the 20-job run of A-X-01; gate G-H1; label-race and tier tests (G-H4); the stolen-credential bound (§8.3); scale-set label count; the permissions of §6.2; the runner group setting; the alert of §8.5. Nothing in this level is claimed here as observed.

---

## 14. Unknowns, and what settles each

`APPENDIX-UNKNOWNS.md` holds the table of U1 to U25: each unknown, what settles it and what blocks that. U1 to U5 are settled by M1 and M2 (§8.2, §10.4, §10.5). Of the open ones, 11 need a live GitHub job and wait for DR-7: U6, U7, U8, U11, U12, U14, U17, U18, U20, U22 and U23. U15 waits for the owner to read the GitHub terms (DR-16). The rest need no account: U9 (WP-N1), U10 (WP-C1), U13, U16, U19 and U21 (WP-13), U24 (WP-08) and U25 (WP-04).

---

## 15. Deferred, with the trigger that reopens each

Each item is cut from v1, and nothing in v1 depends on it. `APPENDIX-DEFERRED.md` keeps the design rules known now for the items marked "A".

| Item | Trigger |
| --- | --- |
| `docker` unit kind: the second implementation of the unit seam on a host with a daemon (`fake` is the second in v1) | DR-13: the owner names a Docker or Podman host |
| `incus` and `host` kinds (A-X-01, R6 B1; the `local` tier). The capability enum keeps the words. With no `host` kind no `local` set is served, and H0a, H3 and H4 serve nothing | The owner names a dedicated machine for `local`, or an ask names Incus |
| Bridges for Gitea, Forgejo and GitLab Runner (A) | An ask names one of those forges as a target |
| `forge-contract` source and its flags | githome has code (R6 0: a LICENSE only) |
| `credential_delivery: socket`, the strong form of G-03 (§8.4). The schemas keep the value. v1 drivers report `files` only | A forge names a runner that takes its credential from a socket |
| Port claims and the host-exclusive service lock (A). v1 refuses `services:` on `shared-host`. `port-claimed` stays in the vocabulary | An owner workflow needs `services:` on H0 |
| `pod-dockerapi` and the verb census (A) | Gate G-H1 fails, or an ask names a client hooks do not reach |
| T-100 TLS end: placeholder swaps and path rules (§9.2). Until then `egress-policy-unsupported` | A policy needs path rules or placeholders |
| Config schema for `podrunner.toml` | A second reader of the file exists |
| Shared caches, cache pipeline, warm units, action mirror (G-07, G-11; A) | A measurement shows start or fetch cost matters, or an ask accepts shared caches |
| `metrics`, `reload`, audit shipping to a second host. A signed pin list is built with the remote mint (§5.3), and stays deferred on a host that has none | A consumer is named, or a pin list moves between hosts |
| Remote mint (WP-05b): HTTPS transport, host keys and tokens, ledger, reconciliation | DR-4 answered yes |
| Per-tier `wall_s_max` and `per_repo_max` scheduling keys | A measured starvation of the trusted tier, or a second untrusted source |
| Alternate payloads and a native runner (A, DR-16) | An owner ask names a target (riscv64, i686, loongarch64, ppc64le, 32-bit) that no official runner serves (R6 B20: none does today) |
| Process split of `sign`, `egress`, `ctl`, each under its own uid | A host with distinct uids is the first target |
| Hosted runner provider with an airlock (A; research only, Documented, R5 F10) | The owner uses a hosted runner provider and no own hosts |
| Unit seam JSON-lines wire (`contracts/unit-driver.md`) | The first out-of-tree driver exists |
| A local mode, `podrunner run workflow.yml`, as `act` (Q4 of #47) | An owner or a cfb or githome ask names running a workflow on the user's own machine |

---

---

## 16. Method and coverage

- Six recon agents read podvm, podnet, podssh, podcrates, the owner archive, the upstream runners and the asks (`recon/`). Two measurement agents ran the official runner and podvm (`measurements/`).
- The author re-read the "CI tracker" ruling, the machine-tier refusal text, the parity row for `-v`, issue #47, the plan, issues #35 and #36, and the recon reports in the parts cited. Files of actions/runner were read directly on 2026-11-10, the mount sources at the pin `310a370bf360`.
- **Not run:** anything against a live GitHub organization. No `gh` call (reads went through `forge.web.pkgforge.dev`).
- **Not covered:** the long-tail asks of #35 outside the runner path; the `TODO/*.md` bodies beyond cited entries. Review rounds are in `review/`. The checks that guard this document are in `tools/` and run from `verify.sh`.
