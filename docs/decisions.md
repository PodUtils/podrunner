# Decisions

The rulings of the owner for podrunner, with the date of each, the calls
made without a ruling, and the questions for the owner. A ruling stays in
force until a later ruling supersedes it. Then move it to the table
"Superseded", with the ruling that replaced it. `AGENTS.md` has the short
form of the rulings that apply to each session. The rows from podcrates
are copies of the rows of `PodUtils/podcrates:docs/decisions.md` at
podcrates commit `eacb916d118f`; the rows of 2026-11-11 for DR-3 to DR-7,
Q10 and Q11 are copies of the rows at podcrates commit `9a3bce7`. podrunner takes the crates of podcrates
at the tag `v0.24.0`.

## In force

| Date | Area | Ruling |
| --- | --- | --- |
| 2026-11-05 | Static, C, Rust | "We use whatever is necessary, including C, zig, modern Rust, no backwards compat. Our users and clients do not care about building the tool, they care about downloading and using it. The only rule is: everything must be statically compiled." It supersedes the rules "no C" and "MSRV 1.89" of the agreed design. |
| 2026-11-05 | Toolchain | Modern and stable Rust, "even nightly/bleeding edge if its necessary to maintain performance, security whatever we need". The `rust-version` is the stable of the day. A consumer moves its toolchain to match; podcrates does not hold back for the toolchain of a consumer. |
| 2026-11-05 | Compatibility | No compatibility with released binaries for now. A name or a format of the wire (AppIDs, variables, the words of the relay contract) can change. No test against a released binary. (podssh, 2026-11-05.) |
| 2026-11-05 | Delivery | The consumers take the crates by a git tag of podcrates (`tag = "vX.Y.Z"`), not from crates.io. One version for the workspace. The forks that a crate needs are in `vendor/`. podcrates publishes nothing to crates.io now; each `pod-*` dependency keeps `path` and `version` for a later publish. (podcrates Q1.) |
| 2026-11-05 | File size | A source file has at most 500 lines. A longer file is split at import. No allowlist. (podcrates Q2.) |
| 2026-11-05 | Dependabot | Apply each judged update as an own commit on `main` (checks green). Close the pull request with a link to the commit. Do not merge the Dependabot branch. |
| 2026-11-09 | New projects (Q-A1 of PodUtils/podcrates#35) | Each project that PodUtils/podcrates#35 and #36 propose gets its own repository of PodUtils: `podrunner` (the CI job slots), `podnode` (the agent of a fleet server), `podinit`, `podlog`, `podwatch`, `podkey`, `podsnap`, `podrepl`, `podrelay-worker` and the installer `podutils`. The owner creates each repository. podcrates gives them the shared crates. |
| 2026-11-09 | Releases (Q-A3, Q-A4, Q4) | "everyone can release betas whenever needed/fits, but no crates io release". Each repository of PodUtils can cut a beta when it fits. No crate goes to crates.io now: the switch `crates-io` stays false. It supersedes the rulings "no release now" of podnet and "no release and no beta until the takes" of podssh, for betas. |
| 2026-11-09 | Schemas (Q-A8) | The JSON Schemas live in podcrates (`schemas/`), generated from the crate types and released with each tag. No npm package. |
| 2026-11-09 | Test accounts (Q-A9) | "Not now". No Cloudflare, Zero Trust or GitHub test account as a CI secret. A live acceptance step that needs one waits; each ask keeps its offline tests and the stand-ins of `pod-testkit`. |
| 2026-11-09 | Priority | "hold on to the other repos, you may create crates for them, but leave them out; priority is first podvm, podnet, podssh projects, cfb & githome can wait". The new repositories (`podrunner` to `podutils`) stay empty and get no session (podrunner: superseded by "podrunner online", 2026-11-11). The work of podcrates serves podvm, podnet and podssh first. The entries of milestone M8 and the other asks of cfb (PodUtils/podcrates#35) and githome (PodUtils/podcrates#36) wait; podcrates can build a crate for a new project when a consumer needs it. It supersedes the order of call C26. |
| 2026-11-10 | Findings | "whenever some flaw/nitpick/bug is found, if it is small, fix it right then & there otherwise file a proper entry as a followup task, and not just print it in chat and lose it". No "suggested task" card (`spawn_task`). Each subagent brief carries the rule (`AGENTS.md`, section 10). |
| 2026-11-11 | podrunner online | "`podrunner` has been attached so you can start a direct session for it, it needs to quickly cpy/adapt the working style, settings, discipline etc". PodUtils/podrunner gets a lead session, which podcrates starts and briefs. podcrates and podrunner talk by direct messages (`send_message`). podrunner copies the frame of podcrates (`AGENTS.md`, `.claude/`, the record, the checks) and adapts it. It supersedes "CI tracker" and, for podrunner only, "the new repositories stay empty and get no session" of "Priority". Call C49 reads DR-1 and DR-2 from it. |
| 2026-11-11 | The sessions of 2026-11-11 | "for podssh/podnet/podvm, please use COORD issues, but only podnet may respond as others are on break, so focus your effort on podrunner & podnet, and if other projects like `podvm` block some work, keep it deferred and pivot to working on something else". podcrates writes to podssh, podnet and podvm only by COORD issues of podcrates. Only podnet answers. The work for podrunner and podnet comes first. An entry that waits for podssh or podvm stays as it is, and the session takes the next entry. |
| 2026-11-11 | DR-1 (GitHub #2) | Yes: the plan of GitHub #1 is the plan, and it replaces the plan of PodUtils/podcrates#47. From the rulings "podrunner online" and "The sessions of 2026-11-11", read by podcrates call C49. |
| 2026-11-11 | DR-2 (GitHub #3) | Yes: PodUtils/podrunner holds code. All issues of GitHub #1 may start, in the order of `TODO/PROGRESS.md`. From the same rulings and call C49. |
| 2026-11-11 | DR-7 (GitHub #7) | Not now (the owner, also through podcrates; the ruling "Test accounts", Q-A9, stays). No test organization and no App. The live gates (T-032, T-040) keep their offline tests and wait. |
| 2026-11-11 | DR-4 (GitHub #4) | Yes, for all trusted repositories, not only private and internal ones: a `trusted` job can run at boundary `redirect` on a host with nothing better, with the mitigations of GitHub #4 (T-045). `[tier.trusted] min_boundary = "redirect"` is allowed; the default floor stays `confined`. The remote mint (T-031) opens, and it needs the mutual TLS of `pod-tls` (PodUtils/podcrates@T-165). `untrusted` stays fixed at `guest`. |
| 2026-11-11 | DR-5 (GitHub #5) | Wait for the machine tier of podvm (PodUtils/podvm@T-1656). The `microvm` kind of podrunner is an adapter over it (T-037). The owner adds: podvm must also support incus, or have an adapter for it; podcrates writes that to podvm. |
| 2026-11-11 | DR-6 (GitHub #6) and pmask | Yes to the restated G-02, G-03 and G-09. The owner adds: "https://github.com/Azathothas/pmask, this is a project in rapid development and will be used for everything/everywhere we need a trusted confidential execution but hardware doesn't support it, we will use this later for many of our crates". Do not adopt pmask now. It is the later path for G-03 and for the residual of same-uid reads at `redirect` (`docs/security/residuals.md`, R1). It is not a boundary. |
| 2026-11-11 | DR-3, DR-8 to DR-17 (GitHub #8) | Confirmed as GitHub #8 lists them, with `--deny-self-hosted-runners` kept: "confirm (but ensure all is pluggable/swappable as always so we can do huge refactor in the future if needed) & also update the issue tracker for crates & repos you need so my local agent may register them". The calls C1 to C12 are rulings now. Each seam (source, unit driver, mint, egress, report sink, image builder, sidecar) stays a trait or a wire with its conformance harness, and no backend is hard-wired. GitHub #1 lists the crates and the repositories that podrunner needs. |
| 2026-11-11 | Q10 of podcrates | A Docker Engine API socket for each job: decide at phase 3 of the plan. |
| 2026-11-11 | Q11 of podcrates | A local mode (`podrunner run workflow.yml`, as nektos/act): "yes plan it". T-046 (GitHub #48) plans it. The native runner crates (`pod-actions-*`) come forward as its later phase. |
| 2026-11-11 | Credential files (PodUtils/podvm#241) | podvm builds no `--secret` flag and no read-once mode. podrunner stages the three credential files of the official runner itself: mode 0600, not in argv, not in the environment, removed at destroy. A-V-11 belongs to podrunner. |
| 2026-11-11 | C13 to C16 (podrunner) | "Confirm, keep swappable": the calls C13 (credentials as files only), C14 (the `local` set), C15 (the sidecar asks `sign` for each App JWT over fd 3) and C16 (the host token of the remote mint is the pinned key) are rulings. Each stays behind its trait or wire with its conformance tests, so a later refactor can replace it. |
| 2026-11-11 | A podvm beta | "Ask podvm for a beta": podrunner asks podvm by a COORD issue to cut a beta from `main` when its release order allows (PodUtils/podvm@T-1706, PodUtils/podvm@T-1710, PodUtils/podvm@T-1720; GitHub #53, T-052). podrunner pins it in `images/podvm.toml`. Until then the entries that wait for podvm stay `blocked`. |
| 2026-11-11 | The spec (T-014) | The owner attached the zip of the design review (`podrunner-spec/`: `SPEC.md`, the appendices, the schemas, the examples, the review rounds). T-014 starts. The zip stays out of the repository; T-014 brings in what the code needs. |
| 2026-11-11 | DR-7 again | Still not now. The live gates (T-032, T-040) keep their offline tests and wait. |

## Calls made without a ruling

Each call stays in force until the owner rules otherwise. The defaults of
GitHub #8 (C1 to C12) are rulings since 2026-11-11 (DR-3, DR-8 to DR-17);
C13 to C16 are rulings since 2026-11-11 ("Confirm, keep swappable").

| Date | Call | What |
| --- | --- | --- |
| 2026-11-11 | C1 (DR-3) | The scale-set client is a Go sidecar over `actions/scaleset` (MIT, public preview), not a Rust port of its six endpoints (T-029). |
| 2026-11-11 | C2 (DR-8) | The container route: the official container hooks first. `docker build` and Dockerfile actions are refused by name (`build-not-supported`). |
| 2026-11-11 | C3 (DR-9) | GitHub sources are of an organization only. A repository of a personal account is refused (`repo-scope-refused`): it needs "Administration: write". |
| 2026-11-11 | C4 (DR-10) | The Go sidecar and the runner payloads (the .NET runner, `bash`, `curl` in the unit image) are user content. The released `podrunner` binary stays static musl. The sidecar is a static Go binary (`CGO_ENABLED=0`). |
| 2026-11-11 | C5 (DR-11) | The one egress proxy lives in podrunner as a module over `pod-broker` (T-024). podvm's `--net broker` (PodUtils/podvm@T-1665) stays a second host of the same crate. |
| 2026-11-11 | C6 (DR-12) | The name `podrunner` stays. `PodUtils/podci` is released, or becomes the name of the CLI. |
| 2026-11-11 | C7 (DR-13) | A `docker` unit kind is built only when the owner names a Docker or Podman host with no KVM. |
| 2026-11-11 | C8 (DR-14) | A host with root or a namespace builds the runner images. An H0 host imports an image that a build host made (T-030). |
| 2026-11-11 | C9 (DR-15) | `pod doctor` owns the qualification of a host (A-X-25). `podrunner doctor` is a thin reader (T-022). |
| 2026-11-11 | C10 (DR-16) | The owner reads the GitHub terms for a third-party runner version string before any alternate payload ships (U15). No alternate payload ships before that. |
| 2026-11-11 | C11 (DR-17) | `pod-dockerapi` and any new crate name are reserved only after a census of the verbs. podrunner asks podcrates for no crate name before that. |
| 2026-11-11 | C12 (GitHub #8, R-C18) | The flag `--deny-self-hosted-runners` of the release proofs of podvm, podnet and podssh stays. podrunner builds no release of another project. |
| 2026-11-11 | C13 (T-021) | podrunner gives a credential to a unit as files only. A kind whose record has no `files` in `secret_delivery` is no candidate; when no kind is left at that step, the code is `boundary-not-available`. A `trusted` set needs `confined` or more unless the configuration sets `redirect` (DR-4). |
| 2026-11-11 | C14 (T-018, T-021) | A `local` set runs each job at tier `local`, with no tier rule. Only a source of the operator (now: `fake`) can be `local`; the configuration refuses a forge source on a `local` set. |
| 2026-11-11 | C16 (T-031, T-045) | The host token of the remote mint is the pinned key of the host (mutual TLS, `pod_tls::mutual`): no bearer token exists to steal from a file. Its life is `not_after_s` of its `[[host]]`; a rotation pins the new key beside the old one, then removes the old one. The certificate dates are not read: the pin names the key. |
| 2026-11-11 | C15 (T-029) | The scale-set sidecar asks `sign` for each App JWT over an inherited descriptor (3): one JSON line out, one line back. The key never reaches the sidecar. GitHub 401 and 403 map to `source-revoked`, 404 to `unknown-job`, 422 to `bad-request`, 429 and each other status to `unavailable`. The job id of an offer is the runner request id of the scale set. |
| 2026-11-11 | C17 (T-014) | The design copy `docs/design/` keeps the words of the design review as written: it is a record of a time (`TODO/todo.toml`, `readings`), and the ASD-STE100 rule applies to new text. Its own checks run as written (`docs/design/verify.sh`). Each issue of the plan links its sections through one map, `docs/design-issues.md`, not through an edit of each issue body. |
| 2026-11-11 | C18 (T-053) | The design source tests 8 and 12 name the codes `tier-mismatch` and `credential-already-issued`. The remote mint refuses the same calls earlier: it withdraws an offer of a lower tier before a host sees it, and after a restart its ledger refuses a second mint; the host then gets `unknown-job`. The refusal is the same; only the code differs, and it names no detail of the other host. |

## Questions for the owner

`TODO/PROGRESS.md`, "Questions for the owner", has each open question
with its recommendation. None is open.

## Superseded

| Date | Ruling | Superseded by |
| --- | --- | --- |
| 2026-11-09 | Priority, for podrunner: the new repositories stay empty and get no session. | 2026-11-11, podrunner online. |
