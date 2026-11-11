The plan of podrunner v1, the decisions for the owner and the registers (GitHub #1 to #10).

# T-001: The plan of podrunner v1 and the issue map (GitHub #1)

**Source:** GitHub #1
**Category:** docs
**Milestone:** M0
**Priority:** P1
**Effort:** S
**Status:** partial

## Problem

The text of GitHub #1 ("Index: podrunner v1 plan, milestones and issue map"), cleaned:

**What and why.** podrunner is one static program. It finds a CI job, picks the safest isolation that the host offers, runs the official `actions/runner` inside it, and refuses what the host cannot isolate. The design has two seams (a job source and a unit driver) and two capability axes (the container route and the egress hold). The core knows no forge, no podvm and no .NET by name. This issue lists the work that must be done before podrunner runs a job, in order.

The plan comes from a design review of the plan of PodUtils/podcrates#47 ("the spec", draft 5, 2026-11-10). The spec is not in a repository. Each issue here states what it needs and does not depend on the spec files. Ids like `DR-n` (decision), `WP-n` (work package), `Un` (unknown), `G-n` (githome asks, PodUtils/podcrates#36) and `A-X-n`, `A-V-n` (cfb asks, PodUtils/podcrates#35) are ids of the spec. #14 carries a cleaned copy of the spec into docs/.

**Rulings that apply.** The owner told podvm on 2026-11-10 to study the podrunner design review and to file issues for each thing that must be done before podrunner can come into fruition. These issues are that filing. The podcrates ruling "leave https://github.com/PodUtils/podcrates/issues/47 alone for now" stands: no issue here edits PodUtils/podcrates#47. DR-1 (name the plan, #2) and DR-2 (let this repository hold code, #3) are still open. Until both are yes, this repository gets issues, labels and milestones only, and no commits.

**Terms.**

- H0: an x86_64 host with no root, no user namespaces, no mounts and no KVM. The target class of the plan.
- Tier: `untrusted` (a fork pull request only), `trusted` (a member job of a configured organization), `local` (set only by configuration). The tier comes from job metadata, never from a runner label.
- Boundary order: `none` < `redirect` < `confined` < `guest`. `redirect` rewrites file paths and is not isolation against hostile code. Every podvm rung is `redirect`.
- Hold: how the network traffic of a unit reaches the egress broker: `none`, `advisory`, `enforced`.
- Principal: `shared` (units and the control plane run as one uid) or `distinct`.
- Unit: one isolated environment for one job. Slot: one concurrent unit of one kind. Set: one GitHub scale set, one for each tier, architecture and host.

**Milestones, in order.**

| Milestone | Content | Gate |
| --- | --- | --- |
| M0 | Decisions #2 to #8, registers #9, #10. Bootstrap #11 to #14 | #2 and #3 answered yes. CI green on an empty workspace |
| M-A | #15 to #24: beta release, seam traits, schemas, tier rule, security documents, fakes, `slots` core, CLI, podvm driver with a stand-in payload, egress | #25: level 1 green, podvm unit conformance green. No account, no second machine, no official runner |
| M-B | podvm makes the official runner start on its userland rung. podrunner proves it through its driver | #26 |
| M-C | #27 to #30: `mint`, fake scale set, scale-set sidecar, image pipeline. Live gates on a `fake` unit | #32. Needs DR-7 (#7) |
| M-D | #31 remote mint. One live trusted job on H0 | #33. Needs DR-4 (#4) and M-B |
| L2 | #34 to #38: hook path measurements, hook relay and control socket, guest self-test, microVM unit, canned fake broker | #39: level 2 green |
| L3 | Live acceptance | #40. Needs DR-7 |

**Issue map.**

- Decisions: #2 (DR-1), #3 (DR-2), #4 (DR-4), #5 (DR-5), #6 (DR-6), #7 (DR-7), #8 (DR-3, DR-8 to DR-17, defaulted).
- Registers: #9 (unknowns U1 to U25), #10 (deferred items of v1).
- Bootstrap: #11 workspace, #12 CI gate, #13 agent workflow and record, #14 the cleaned spec in docs/, #15 beta release.
- M-A: #16 seams, #17 schemas, #18 tier rule, #19 security documents, #20 fakes, #21 `slots` core, #22 CLI and control socket, #23 podvm driver, #24 egress, #25 gate.
- M-B: #26. M-C: #27, #28, #29, #30, #32. M-D: #31, #33.
- L2: #34, #35, #36, #37, #38, #39. L3: #40.
- Crate dependencies later than v1 (no milestone): #41 (`enforced` hold on podvm units), #42 (`confined` boundary), #43 (service unit, release index, acceptance registry).

**Cross-repo dependencies.**

podvm (filed in PodUtils/podvm; the issues here link each one where it is needed):

- PodUtils/podvm#229 (N1): `realpath` of `/proc/self/exe` under the interposer (runner start, exit 154).
- PodUtils/podvm#230 (N2): a host path from a loaded object gets the rootfs prefix twice (runner start, exit 137).
- PodUtils/podvm#231 (N3): the official runner as a proven payload (.NET needs: ICU, OpenSSL, W^X off).
- PodUtils/podvm#232 (N4): a first program relative to `-w`.
- PodUtils/podvm#233 (N5): tree kill for `stop`, `rm -f` and a wall bound.
- PodUtils/podvm#234 (N6): a rootfs for each container (no shared mutable rootfs).
- PodUtils/podvm#235 (N7): unix socket paths reach the unit (control socket, egress socket).
- PodUtils/podvm#236 (N8): the container-hook verbs over podvm.
- PodUtils/podvm#237 (N9): `--podvm-tier=chroot` either holds or refuses.
- PodUtils/podvm#238 (N10): `.EnteredRung` names the rung that `run` enters.
- PodUtils/podvm#239 (N11): `probe --json` validated against a published schema.
- PodUtils/podvm#240 (N12): the modes of the container table and the store root.
- Existing podvm entries: `--secret` aware of the three credential files (PodUtils/podvm@T-1696), `--report` (PodUtils/podvm@T-1695), limits and tree kill (PodUtils/podvm@T-1655), microVM wiring (PodUtils/podvm@T-1656), `--net broker` (PodUtils/podvm@T-1665).

podcrates (one COORD issue, PodUtils/podvm#227):

- Section 2.1: `pod-broker` listener bound to one job, no token in the unit.
- Section 2.2: `pod-broker` name-forwarding upstream (WP-C1).
- Section 2.3: `pod-supervise` tree kill, wall timeout and rusage (WP-C2).
- Section 2.4: schemas and types of offer, capability and report (WP-C3, ruling Q-A8).
- Section 2.5: `pod-services` Actions hosts and the runner pin, with watches (WP-C4).
- Section 2.6: `pod-tls` mutual TLS with pinned keys.
- Section 2.7: podrunner as a consumer of PodUtils/podcrates@T-103 (`pod-control`), PodUtils/podcrates@T-107 (`pod-hostprobe`), PodUtils/podcrates@T-108 (`pod-confine`), PodUtils/podcrates@T-109 (hold), PodUtils/podcrates@T-062 (`pod-microvm`); no `pod-dockerapi` now; the RS256 signer question.

podnet (one COORD issue, PodUtils/podvm#228):

- Section 2.8: `podnet pod` with the official runner, and a JSON statement of the hold (WP-N1, U9).
- Section 2.9: the `pod-hold` hand-off (PodUtils/podnet#414).

**Labels.** `decision`: a question for the owner. `work-package`: build work. `unknown`: a fact to measure. `milestone`: a gate. `bootstrap`: repository set-up. `contract`: a seam contract or a schema. `security`: trust and credentials. `crate-dependency`: needs a crate of podcrates, podvm or podnet. `blocked`: waits for an owner decision other than DR-1 and DR-2, or for work in another repository that is not done.

**Acceptance.**

- Each issue of the map exists and links back here.
- Each issue of the map, except this index and the later-than-v1 issues, has one GitHub milestone.
- PodUtils/podcrates@T-146 (the record entry of PodUtils/podcrates#47) points to this index after #2 is yes.

Dependencies: none. Estimate (mine): S.

## Premise

Read the text of GitHub #1 on 2026-11-11. Nothing measured.

## Approach

Keep this record as the plan: one entry for each issue of the map, under its milestone of `TODO/ROADMAP.md`. podcrates points PodUtils/podcrates@T-146 to GitHub #1 (podcrates call C49). Close GitHub #1 when each entry of milestone M-D and L2 is done.

## Prove

`cargo todo check`, and the issue map of `TODO/issues.md` against GitHub: `python3 -I scripts/check-github-numbers.py`.

# T-002: DR-1: the plan of GitHub #1 is the plan (GitHub #2)

**Source:** GitHub #2
**Category:** docs
**Milestone:** M0
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

The text of GitHub #2 ("DR-1: name the podrunner plan and lift "leave podcrates#47 alone" for it"), cleaned:

**Question for the owner.** Do you name this plan, lift the ruling "leave https://github.com/PodUtils/podcrates/issues/47 alone for now" (podcrates docs/decisions.md, 2026-11-10, row "CI tracker") for it, and does it replace the plan of PodUtils/podcrates#47?

**Why it blocks.** The ruling says that no work on the plan of PodUtils/podcrates#47 starts until the owner names it. Every issue of the index #1 is work on that plan.

**Rulings that apply.** The owner told podvm on 2026-11-10 to study the podrunner design review and to file issues for each thing that must be done before podrunner can come into fruition. These issues are that filing. The podcrates ruling "leave https://github.com/PodUtils/podcrates/issues/47 alone for now" stands: no issue here edits PodUtils/podcrates#47. DR-1 (name the plan, #2) and DR-2 (let this repository hold code, #3) are still open. Until both are yes, this repository gets issues, labels and milestones only, and no commits. The request of 2026-11-10 to file these issues is close to a yes, but no row of a `decisions.md` records it.

**Default without an answer.** The plan stays a document and a set of issues. No repository changes.

**What changes with yes.** podvm can take its podrunner items (the runner on the userland rung, a rootfs for each container, the `exec` workdir in a volume). podcrates can schedule its asks (PodUtils/podvm#227). podnet can schedule its asks (PodUtils/podvm#228). PodUtils/podvm@T-1655 (tree kill) can wait for the `pod-supervise` tree kill.

**Acceptance.**

- The owner's answer is recorded with its date in podcrates docs/decisions.md.
- If yes: PodUtils/podcrates#47 carries a pointer to #1, written by podcrates after the answer. If no: #1 is closed as "not planned".

Dependencies: none. Estimate: owner reply.

**Recommendation:** Yes. Record it as a new row in podcrates docs/decisions.md that supersedes "CI tracker". After that row exists, podcrates points PodUtils/podcrates@T-146 and PodUtils/podcrates#47 to #1.

## Premise

Read the text of GitHub #2 on 2026-11-11. Nothing measured.

## Approach

Copy the ruling of the owner into `docs/decisions.md` ("podrunner online" and podcrates call C49). Close GitHub #2 with a link to the row.

## Prove

`cargo todo check`

## Done

2026-11-11: the ruling is in `docs/decisions.md` (rows "podrunner online" and "The sessions of 2026-11-11", and the answers of DR-1 and DR-2). GitHub #2 is closed with a link to the rows. `cargo todo check` exits 0.

# T-003: DR-2: PodUtils/podrunner holds code (GitHub #3)

**Source:** GitHub #3
**Category:** docs
**Milestone:** M0
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

The text of GitHub #3 ("DR-2: let PodUtils/podrunner hold code"), cleaned:

**Question for the owner.** Do you lift "the new repositories of PodUtils stay empty and get no session" (ruling "Priority", 2026-11-09, in podcrates and podvm) for podrunner?

**Why it blocks.** All bootstrap issues (#11 to #15) and all podrunner work packages write to this repository. Today it holds only the 0BSD LICENSE.

**Rulings that apply.** The owner told podvm on 2026-11-10 to study the podrunner design review and to file issues for each thing that must be done before podrunner can come into fruition. These issues are that filing. The podcrates ruling "leave https://github.com/PodUtils/podcrates/issues/47 alone for now" stands: no issue here edits PodUtils/podcrates#47. DR-1 (name the plan, #2) and DR-2 (let this repository hold code, #3) are still open. Until both are yes, this repository gets issues, labels and milestones only, and no commits.

**Default.** The repository stays empty. Issues can exist; no commits.

**Acceptance.**

- The answer is recorded with its date in podcrates docs/decisions.md, and the first docs/decisions.md of this repository copies it (#13).
- The answer states which issues may start (all, or a named subset).

Dependencies: #2. Estimate: owner reply.

**Recommendation:** Yes, in steps: first the bootstrap (#11 to #15) and WP-01 to WP-03 (#16, #20, #21). Keep the order of "Priority": podrunner sessions do not take work from podvm, podnet or podssh.

## Premise

Read the text of GitHub #3 on 2026-11-11. Nothing measured.

## Approach

Copy the ruling of the owner into `docs/decisions.md` ("podrunner online", "The sessions of 2026-11-11" and podcrates call C49). Close GitHub #3 with a link to the row.

## Prove

`cargo todo check`

## Done

2026-11-11: the ruling is in `docs/decisions.md` (rows "podrunner online" and "The sessions of 2026-11-11", and the answers of DR-1 and DR-2). GitHub #3 is closed with a link to the rows. `cargo todo check` exits 0.

# T-004: DR-4: trusted jobs at boundary redirect on no-root hosts (GitHub #4)

**Source:** GitHub #4
**Category:** research
**Milestone:** M0
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

The text of GitHub #4 ("DR-4: allow trusted jobs at boundary `redirect` on no-root hosts"), cleaned:

**Question for the owner.** May a `trusted` job (a member job of a configured organization) run at boundary `redirect` on a host that offers nothing better (H0: x86_64, no root, no user namespaces, no mounts, no KVM)?

**Why it matters.** `redirect` rewrites file paths. It does not isolate hostile code. Every podvm rung reports `redirect`. Without a yes, the `trusted` tier needs `confined` (a kernel-enforced boundary, see #42), no podvm host reaches it, and H0 runs no job at all.

**Accepted residuals with a yes** (threats T2, T3, T8 of the spec; #19 holds the threat model):

- A compromised dependency in a trusted job can send that job's secrets out, reach host loopback services and read other jobs on the same uid.
- It can read the host key of the remote mint and get tokens until the operator revokes the key.
- Until U22 is settled (#9), a credential minted for one job may run another job of the same set.
- It can rewrite the configuration, the key files and the driver binaries of the host, and so change the jobs of every later repository on that host.

**Cost of a yes.** #31 (remote mint, WP-05b), size M to L. The minting side must run on another machine.

**Mitigations that come with a yes.** One job at a time on a shared uid; one repository for each host (`affinity = "repository"`); outstanding offers capped at the configured slot count; a host key with a rotation interval and shorter tokens; a signed pin list; `status` and every report name the boundary and the hold; `untrusted` stays fixed at `guest`.

**Default.** `trusted` needs `confined`. H0 runs no job.

**Acceptance.**

- The answer is recorded with its date, with the list of residuals that it accepts.
- If yes: #31 and #33 open; `tier.trusted.min_boundary = "redirect"` is allowed by the configuration check of #21. If no: #31 and #33 close as "not planned".

Dependencies: #2. Blocks: #31, #33. Estimate: owner reply.

**Recommendation:** Yes, with the mitigations above (the spec's recommendation). The study adds one limit: allow it only for private and internal repositories of the owner's organizations (`trusted_public` empty).

## Premise

Read the text of GitHub #4 on 2026-11-11. Nothing measured.

## Approach

Copy the ruling into `docs/decisions.md`. Allow `[tier.trusted] min_boundary = "redirect"` in the configuration check of T-021, with the mitigations that the file can state (one job at a time on a shared uid, a slot cap on each `trusted` source). T-045 (GitHub #47) holds the other mitigations. T-031 and T-033 open.

## Decision

The owner, 2026-11-11 (through podcrates, rows at podcrates commit `9a3bce7`): yes, for all trusted repositories, not only private and internal ones, with the mitigations of GitHub #4.

## Prove

`cargo test -p podrunner-slots trusted_at_redirect`, `cargo todo check`

## Done

2026-11-11: the ruling is in `docs/decisions.md` (row "DR-4"). `podrunner_slots::config` allows `redirect` for `trusted` only when the file sets it, refuses it with `same_principal_ok = true` or a `trusted` source with no `slots`; the default floor stays `confined`. `cargo test -p podrunner-slots trusted_at_redirect`: 1 passed. The residuals (`docs/security/residuals.md`, R1) and the threat model name the ruling. GitHub #4 is closed; GitHub #47 (T-045) holds the rest of the mitigations.

# T-005: DR-5: the microVM unit for untrusted jobs (GitHub #5)

**Source:** GitHub #5
**Category:** research
**Milestone:** M0
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The text of GitHub #5 ("DR-5: microVM unit for untrusted jobs: own driver now, or PodUtils/podvm@T-1656"), cleaned:

**Question for the owner.** The `untrusted` tier (fork pull requests) needs boundary `guest`: a separate kernel. Does podrunner build its own `microvm` driver now (Firecracker or QEMU, ported from a launcher in a private archive of the owner), or wait for PodUtils/podvm@T-1656 (microVM wiring of podvm's machine tier)?

**Facts.** `podvm run --podvm-tier=machine` exits 125 for Linux payloads today. PodUtils/podvm@T-1656 is size XL. The archive launcher runs a shell script, not the runner, and needs passwordless `sudo` and `setpriv`; a port for a user in group `kvm` is a rewrite of its privilege model. Both forms are x86_64 only. KVM is free for sessions since 2026-11-09. podcrates holds `pod-microvm` as a reserved name with a study only (PodUtils/podcrates@T-062; PodUtils/podvm#227, section 2.7).

**Options.**

- Own driver now (the spec's recommendation): the `untrusted` tier comes sooner. Cost: two microVM implementations in the organization for a while; podrunner retires its driver when PodUtils/podvm@T-1656 lands.
- Wait for PodUtils/podvm@T-1656 (the study's counter-recommendation): the rulings "New projects" (2026-11-09: "`podrunner` runs podvm as a unit kind; podvm does not grow those projects") and "Scope" (2026-11-06: podvm "will extend to quickly create full on microvms") give microVMs to podvm. Milestones M-A to M-D need no `untrusted` tier. The podrunner `microvm` kind is then an adapter over podvm's machine tier.

**Default.** Wait.

**Acceptance.**

- The answer is recorded with its date.
- #37 states its form (own driver, or adapter over podvm) and its dependency.

Dependencies: #2. Blocks: #37. Estimate: owner reply.

**Recommendation:** The spec recommends an own driver now, so that `untrusted` comes sooner. This study recommends to wait for PodUtils/podvm@T-1656, because the rulings "New projects" and "Scope" give microVMs to podvm, two microVM implementations would exist for a while, and no milestone before L2 needs the `untrusted` tier.

## Premise

Read the text of GitHub #5 on 2026-11-11. Nothing measured.

## Approach

Copy the ruling into `docs/decisions.md`. T-037 states its form: an adapter over the machine tier of podvm, blocked on PodUtils/podvm@T-1656.

## Decision

The owner, 2026-11-11 (through podcrates): wait for PodUtils/podvm@T-1656; the `microvm` kind is an adapter over it. podvm must also support incus, or have an adapter for it; podcrates writes that to podvm.

## Prove

`cargo todo check`

## Done

2026-11-11: the ruling is in `docs/decisions.md` (row "DR-5"). T-037 states the adapter form and its blocker. `cargo todo check` exits 0. GitHub #5 is closed.

# T-006: DR-6: githome and cfb accept the restated G-02, G-03 and G-09 (GitHub #6)

**Source:** GitHub #6
**Category:** research
**Milestone:** M0
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The text of GitHub #6 ("DR-6: githome and cfb accept the restated G-02, G-03 and G-09"), cleaned:

**Question for the owner** (owner of the githome asks, PodUtils/podcrates#36, and the cfb asks, PodUtils/podcrates#35). Do you accept three restated acceptance texts?

- **G-02** (unit kind, no fallback). The ask says the source states the unit kind of each label. podrunner computes it from the job metadata and the tier, never from a label, because the workflow author names the label. A host with no VM does not poll the `untrusted` set.
- **G-03** (runner credential unreadable by a step). The official runner takes its credential as three files (`.runner`, `.credentials`, `.credentials_rsaparams`), keeps them for its life, and its Worker reads `.runner` from disk. Steps run as the same uid. So "not from the file" is not possible with the official runner. podrunner meets "not in argv" and "not in the environment" (pre-staged files at mode 0600, measured), and removes the files at `destroy`. A step that reads them affects only its own job only where principals are distinct.
- **G-09** (services, container jobs, Docker actions). podrunner uses the official container hooks, refuses `services:` on a shared-network unit, and defers a Docker API. The `microvm` acceptance stays open until a guest runner image exists.

**Default.** The three asks stay open for the official runner.

**Acceptance.**

- The answer is recorded, and PodUtils/podcrates#35 and PodUtils/podcrates#36 carry the restated texts or a link.

Dependencies: #2. Blocks: the claims "met" for G-02, G-03 and G-09 in #40. Estimate: owner reply.

**Recommendation:** Yes to all three.

## Premise

Read the text of GitHub #6 on 2026-11-11. Nothing measured.

## Approach

Copy the ruling into `docs/decisions.md`, with pmask as the later path for G-03 (not adopted). The tests of T-021 and the claims of T-040 use the restated texts.

## Decision

The owner, 2026-11-11 (through podcrates): yes to the restated G-02, G-03 and G-09. pmask (a ptrace supervisor that hides the command line and the environment of a process tree, and gives secrets on sealed memfds) is the later path for G-03 and for the same-uid residual at `redirect`; it is not a boundary, and it is not adopted now. Its limits: it needs ptrace, it builds static only for x86_64 and aarch64, and it must run as a separate process.

## Prove

`cargo todo check`

## Done

2026-11-11: the ruling is in `docs/decisions.md` (row "DR-6 and pmask"); T-042 names pmask as a later option. `cargo todo check` exits 0. GitHub #6 is closed.

# T-007: DR-7: a throwaway GitHub organization and App for live tests (GitHub #7)

**Source:** GitHub #7
**Category:** research
**Milestone:** M0
**Priority:** P2
**Effort:** S
**Status:** blocked

## Problem

The text of GitHub #7 ("DR-7: a throwaway GitHub organization and App for live tests"), cleaned:

**Question for the owner.** Do you lift the ruling Q-A9 ("Test accounts", 2026-11-09: "Not now. No Cloudflare, Zero Trust or GitHub test account as a CI secret") for one throwaway GitHub organization with one GitHub App, for the live gates?

**Why it matters.** Eleven unknowns need a live job (U6, U7, U8, U11, U12, U14, U17, U18, U20, U22, U23; see #9). The live gates are:

- G-H1: github.com sends the job variable `DistributedTask.AllowRunnerContainerHooks`.
- G-H2: a scale set honours several labels.
- G-H3: the App permissions are enough ("Self-hosted runners: read and write" at organization scope, repository "Actions: read", organization "Members: read").
- G-H4: a fork job that names the trusted set is refused (label race).

**Default.** Level 3 waits. G-H1 to G-H4 stay unverified. M-C (#32), M-D (#33) and #40 wait.

**Acceptance.**

- The answer is recorded with its date, and where the App key lives.

Dependencies: #2. Blocks: #32, #33, #40. Estimate: owner reply.

**Recommendation:** Yes. A throwaway organization. The App key is never a CI secret of another repository. The live tests run by hand or from a workflow of the throwaway organization only. No hurry: M-A and M-B need no account.

## Premise

Read the text of GitHub #7 on 2026-11-11. Nothing measured.

## Approach

The ruling "Test accounts" (Q-A9, 2026-11-09) answers it: not now. Each live gate (T-032, T-040) keeps its offline tests and waits.

## Prove

`cargo todo check`

## Blocker

The owner said not now on 2026-11-11 (DR-7, through podcrates). The owner must change the ruling "Test accounts" of `docs/decisions.md`. Until then no test organization and no App exist.

# T-008: The defaulted decisions DR-3 and DR-8 to DR-17 (GitHub #8)

**Source:** GitHub #8
**Category:** docs
**Milestone:** M0
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The text of GitHub #8 ("Defaulted decisions DR-3 and DR-8 to DR-17: confirm in one reply"), cleaned:

Each item has a default that equals the recommendation. Work proceeds on the default. A reply "confirm all" or a list of changes is enough.

- DR-3: the scale-set client is a Go sidecar over `actions/scaleset` (MIT, public preview), not a Rust port of its six endpoints.
- DR-8: container route: the official container hooks first. `docker build` and Dockerfile actions are refused by name (`build-not-supported`).
- DR-9: GitHub sources are organization-level only. Personal-account repositories are refused (`repo-scope-refused`), because they need repository "Administration: write".
- DR-10: the Go sidecar and the runner payloads (the .NET runner, `bash`, `curl` in the unit image) are user content. The released `podrunner` binary stays static musl. The sidecar is a static Go binary (`CGO_ENABLED=0`). (podcrates holds the same question as Q9.)
- DR-11: the one egress proxy lives in podrunner as a module over `pod-broker`. podvm's `--net broker` (PodUtils/podvm@T-1665) stays a second host of the same crate.
- DR-12: the name `podrunner` stays. `PodUtils/podci` (empty, overlaps) is released, or becomes the name of the CLI.
- DR-13: a `docker` unit kind is built only when the owner names a Docker or Podman host with no KVM.
- DR-14: a host with root or a namespace builds the runner images. An H0 host imports an image that a build host made.
- DR-15: `pod doctor` owns host qualification (A-X-25). `podrunner doctor` is a thin reader.
- DR-16: the owner reads the GitHub terms for a third-party runner version string before any alternate payload ships (settles U15).
- DR-17: `pod-dockerapi` and any new crate name are reserved only after a verb census. (podvm's ruling "Crate reservation", 2026-11-06, says to ask early for names that podvm needs. DR-17 applies to podrunner's names only. Q8, Q10 and Q14 of podcrates TODO/PROGRESS.md recommend `pod-dockerapi` now; only the owner resolves that conflict of recommendations.)

Not numbered in the spec, also for the owner (crate item R-C18 of the study, merged here):

- The release workflows of podvm, podnet and podssh pass `--deny-self-hosted-runners` to `gh attestation verify`. A release built on a podrunner host fails that proof. Dogfooding their release proofs on podrunner needs that flag changed, and only the owner's word changes it.

**Acceptance.**

- The reply is recorded in docs/decisions.md of this repository (#13), one row for each item.
- A changed default reopens the issues it names.

Dependencies: #2. Estimate: owner reply.

**Recommendation:** Confirm all defaults as listed. Keep the `--deny-self-hosted-runners` flag: podrunner does not build releases (see #12 and #15).

## Premise

Read the text of GitHub #8 on 2026-11-11. Nothing measured.

## Approach

Record each default as a call in `docs/decisions.md` (C1 to C12). The owner confirmed all: they are rulings now. Comment on GitHub #1 with the crates and the repositories that podrunner needs, so that the local agent of the owner can register them, and send the list to podcrates for PodUtils/podcrates#1.

## Decision

The owner, 2026-11-11 (through podcrates): confirm all, `--deny-self-hosted-runners` kept, and each seam stays pluggable and swappable.

## Prove

`cargo todo check`

## Done

2026-11-11: the ruling is in `docs/decisions.md` (row "DR-3, DR-8 to DR-17"); the calls C1 to C12 are rulings. GitHub #1 has the list of crates and repositories, and podcrates has it by direct message. `cargo todo check` exits 0. GitHub #8 is closed.

# T-009: The register of the unknowns U1 to U25 (GitHub #9)

**Source:** GitHub #9
**Category:** research
**Milestone:** M0
**Priority:** P2
**Effort:** M
**Status:** partial

## Problem

The text of GitHub #9 ("Unknowns U1 to U25: register, owners and what settles each"), cleaned:

**What and why.** The design rests on 25 unknowns. Each has a settling action and a blocker. A statement that depends on an open row stays labelled "Unverified" in the documents. This issue holds the table and is closed when no row is open or each open row has its own issue.

| Id | Unknown | Settles it | Blocker |
| --- | --- | --- | --- |
| U1 | Official runner runs on the podvm userland rung | Settled: no (exit 154, then 137). Fix is podvm: PodUtils/podvm#229 (N1), PodUtils/podvm#230 (N2), PodUtils/podvm#231 (N3), PodUtils/podvm#232 (N4) | podvm |
| U2 | `RLIMIT_FSIZE` kills the runner at start | Settled: yes, with default W^X | none |
| U3 | Runner starts from pre-staged files, nothing in argv or environ | Settled: yes. The env route leaks for the whole life | none |
| U4 | Grandchildren survive `podvm stop` | Settled: yes, 6 of 6. Fix is podvm PodUtils/podvm#233 (N5) and PodUtils/podvm#227, section 2.3 | none |
| U5 | Two containers of one image share a root | Settled: yes. Fix is podvm PodUtils/podvm#234 (N6) | none |
| U6 | github.com sends `AllowRunnerContainerHooks` | Live job | #7 |
| U7 | Several labels per scale set | Live job | #7 |
| U8 | `actions/scaleset` works with the minimum App permissions | Live job | #7 |
| U9 | `podnet pod` holds the .NET runner, Node `fetch`, `checkout`, `setup-node` | podnet WP-N1 (PodUtils/podvm#228, section 2.8) | podnet |
| U10 | The broker fails on a proxy that resets CONNECT to IP literals | podcrates WP-C1 test (PodUtils/podvm#227, section 2.2) | podcrates |
| U11 | A stolen per-job credential affects only its own job | Live job | #7 |
| U12 | Run object needs only "Actions: read"; member check needs "Members: read"; a runner group can refuse public repositories | Live job | #7 |
| U13 | `DOTNET_EnableWriteXorExecute=0` has a measurable cost | A bench on a unit | none (#26) |
| U14 | Runner with the hooks variable set and the job variable false | Live job | #7 |
| U15 | github.com treatment of a third-party runner version string | The owner reads the terms | #8 (DR-16) |
| U16 | `dookoid` on a host that denies ptrace | A run on such a host | none, low priority, deferred with the Docker API |
| U17 | Two hosts share one scale set | Live job | #7 |
| U18 | A job can be failed by name through the scale-set API | Live job | #7 |
| U19 | A job container of another image and a Docker action run on `net: shared-host` podvm | #34 | podvm PodUtils/podvm#236 (N8) |
| U20 | A real JIT blob differs from the synthetic one; files survive a token refresh over hours | Live job | #7 |
| U21 | `curl` runs under the podvm interposer; a host socket is visible in the unit; bash decodes framed output | #34 | podvm PodUtils/podvm#235 (N7) |
| U22 | A JIT runner is bound to the job it was minted for | Live job | #7 |
| U23 | Trust inputs behave as specified for re-runs, merge queue, reusable workflows | Live job | #7 |
| U24 | `pod-broker` v1 serves one listener for each unit | Read in the crate (study, 2026-11-10): `pod-broker` reads the job from `Proxy-Authorization` on each connection, and no call binds one listener to one job. So it needs a crate change (PodUtils/podvm#227, section 2.1) or a wrapper. #24 confirms | podcrates |
| U25 | `podvm run -v` accepts `/proc/self/fd/N` as a mount source | #23 | none |

**Acceptance.**

- Each open row links the issue that settles it.
- Each settled row names its evidence (a log, a commit, a CI run).
- The live rows are run in #40 or each has a recorded waiver.

Dependencies: none. Estimate (mine): S for the register; the rows are sized in their issues.

## Premise

Read the text of GitHub #9 on 2026-11-11. Nothing measured.

## Approach

`docs/unknowns.md` holds the 25 rows, each with what settles it, its blocker and its state. Settled, each with its evidence: U1 to U5, U9, U13, U19, U21, U24, U25. Open: the live rows (U6 to U8, U11, U12, U14, U17, U18, U20, U22, U23) wait for T-040 (DR-7); U10 needs a test against a proxy that resets CONNECT to an IP literal; U15 waits for the owner (call C10); U16 is deferred with the Docker API (`docs/non-goals.md`). The entry stays open while a row is open with no entry that settles it.

## Prove

`cargo todo check`

# T-010: The non-goals of v1 and the trigger of each (GitHub #10)

**Source:** GitHub #10
**Category:** docs
**Milestone:** M0
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

The text of GitHub #10 ("v1 non-goals: deferred items and the trigger of each"), cleaned:

**What and why.** v1 cuts these items. Nothing in v1 depends on them. Each has a trigger that reopens it. A request that hits a trigger opens its own issue.

| Item | Trigger |
| --- | --- |
| `docker` unit kind | The owner names a Docker or Podman host (DR-13) |
| `incus` and `host` kinds, and so the `local` tier | The owner names a dedicated machine, or an ask names Incus |
| Bridges for Gitea, Forgejo, GitLab Runner | An ask names one of those forges as a target |
| `forge-contract` source (githome) | githome has code |
| `credential_delivery: socket` (strong G-03) | A forge names a runner that takes its credential from a socket |
| Port claims and a host-exclusive service lock | An owner workflow needs `services:` on H0 |
| `pod-dockerapi` and the verb census | Gate G-H1 fails, or an ask names a client that hooks do not reach |
| TLS end of the broker (PodUtils/podcrates@T-100): placeholders, path rules | A policy needs path rules or placeholders |
| A schema for `podrunner.toml` | A second reader of the file exists |
| Shared caches, warm units, action mirror (G-07 shared form, G-11) | A measurement shows that start or fetch cost matters |
| `metrics`, `reload`, audit shipping to a second host | A consumer is named |
| Per-tier `wall_s_max` and `per_repo_max` | A measured starvation of the trusted tier |
| Alternate payloads and a native runner | An ask names a target no official runner serves (riscv64, i686, loongarch64, ppc64le) |
| Process split of `sign`, `egress`, `ctl` under own uids | A host with distinct uids is the first target |
| Hosted runner provider with an airlock | The owner uses a hosted provider and no own hosts |
| JSON-lines wire of the unit seam | The first out-of-tree driver exists |
| A local mode `podrunner run workflow.yml` | An ask names it |

**Docker API (crate item R-C15 of the study, merged here).** No `pod-dockerapi` in v1 (DR-8, DR-17 in #8). Reopen the row if gate G-H1 (the container-hooks job variable, #40) fails. Do not reserve a crate name before the verb census. podcrates was told the same (PodUtils/podvm#227, section 2.7, note 1).

**Acceptance.**

- The table is in docs/ of this repository (#14).
- No v1 issue depends on a row of this table.

Dependencies: #14. Estimate (mine): S.

**Recommendation:** Accept the table as the list of v1 non-goals.

## Premise

Read the text of GitHub #10 on 2026-11-11. Nothing measured.

## Approach

Write docs/non-goals.md: each deferred item with the trigger that brings it back.

## Prove

`cargo todo check`

## Done

2026-11-11. `docs/non-goals.md` holds the 17 items of GitHub #10, each with its trigger; no open entry of v1 depends on a row (the `local` tier serves only a `fake` source, call C14). `cargo todo check` agrees.


# T-044: COORD-001: DR-1 and DR-2 are yes; podnet's podrunner entries can start (GitHub #46)

**Source:** GitHub #46
**Category:** chore
**Milestone:** M-A
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

podnet answered PodUtils/podvm#228 in PodUtils/podnet#422 (COORD-014): its
entries PodUtils/podnet@T-229 (the measure of `podnet pod` with the
official runner), PodUtils/podnet@T-230 (an object `hold` in
`pod doctor --json`) and PodUtils/podnet@T-231 were blocked on DR-1 and
DR-2. Both are yes (`docs/decisions.md`). GitHub #46 (COORD-001, to
podnet) tells podnet, and asks for the measure first, then the object `hold`.

## Premise

Read PodUtils/podnet#422 on 2026-11-11 through the watcher. Nothing
measured.

## Approach

Wait for podnet's answer `(to podrunner)` on PodUtils/podnet@T-229 and PodUtils/podnet@T-230. Write each podnet commit that
podrunner takes into the entry that uses it (T-023, T-024, T-022). Close
GitHub #46 when podnet has read it.

## Prove

`cargo todo check`, and the answer of podnet named in the Done section.

## Done

2026-11-11: podnet answered in PodUtils/podnet#425 (COORD-016).
PodUtils/podnet@T-229 is done (podnet `bb9dd91`): `podnet pod --backend
seccomp --default-action reject` passes six payloads of `actions/runner`
v2.338.2 (asset `actions-runner-linux-x64-2.338.2.tar.gz`, SHA-256
`baebf204307bac6193480852732f0aba2ae97b6c5597c244f4d5558390147c40`),
`actions/checkout` v4.2.2 and `actions/setup-node` v4.4.0; each CONNECT
has an allowed flow. Not held by seccomp: DNS to the resolver of the pod,
netlink, unix sockets, the diagnostic listener of .NET, loopback TCP. A
`run.sh` to a long poll is not measured. A defect (an IPv4-mapped fake
address) is fixed in podnet `17da8c3`. PodUtils/podnet@T-230 is done
(podnet `ae5d1e3`): `pod doctor --json` has `hold.netns` (`enforced`) and
`hold.seccomp` (`advisory`), each with `level` and `not_held`, pinned by a
podnet test. T-023, T-024 and T-030 use these facts. GitHub #46 is
closed.
