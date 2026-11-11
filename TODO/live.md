Milestones M-B, M-C and M-D: the official runner under podvm, the mint, the scale-set sidecar, the image pipeline and the live gates (GitHub #26 to #33).

# T-026: Milestone M-B: the official runner starts under the podvm driver, no GitHub (GitHub #26)

**Source:** GitHub #26
**Category:** chore
**Milestone:** M-B
**Priority:** P2
**Effort:** M
**Status:** blocked

## Problem

The text of GitHub #26 ("Milestone M-B: the official runner starts under the podvm driver, no GitHub"), cleaned:

**What and why.** A measurement (M2) showed that the unmodified official runner does not start on podvm's default (userland) rung: exit 154 (the .NET apphost looks for `Runner.Listener.dll` beside the loader), then exit 137 after a workaround (CoreCLR library paths prefixed twice). It ran only under `--unsafe-host-paths`, which is not isolation. The fix is podvm's. podrunner proves the fix through its own driver.

podvm items:

- PodUtils/podvm#229 (N1): `realpath` of `/proc/self/exe` (exit 154).
- PodUtils/podvm#230 (N2): a host path from a loaded object gets the prefix twice (exit 137).
- PodUtils/podvm#231 (N3): the official runner as a proven payload (ICU, OpenSSL, W^X off, `RLIMIT_FSIZE`).
- PodUtils/podvm#232 (N4): a first program relative to `-w` (the runner starts as `./run.sh`).

**Content.** A podrunner integration test that starts the runner image through the podvm driver with pre-staged credential files and a fake broker endpoint (the fake server of measurement M1 answers `POST /oauth/token`, `POST /session`, `GET /message`, `DELETE /session`).

**Acceptance.**

- `config.sh --help` and `run.sh` start on the userland rung without `--unsafe-host-paths`, with `DOTNET_EnableWriteXorExecute=0`, `DOTNET_SYSTEM_GLOBALIZATION_INVARIANT=1` or ICU, and OpenSSL in the image.
- The listener reaches the long poll from pre-staged files; no marker of the credential is in any `cmdline` or `environ`.
- `ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE=1` gives exit 7 on a "version too old" answer of the fake server.
- U13 (cost of W^X off) is measured with one bench, or recorded as not measured.

Dependencies: #23, #25. Cross-repo: podvm PodUtils/podvm#229 (N1), PodUtils/podvm#230 (N2), PodUtils/podvm#231 (N3), PodUtils/podvm#232 (N4). Estimate (mine): S on the podrunner side.

## Premise

Read the text of GitHub #26 on 2026-11-11. Nothing measured.

## Approach

When podvm makes the official runner start on its userland rung (PodUtils/podvm#229 to PodUtils/podvm#231), prove it through the driver of T-023 with no GitHub.

## Prove

`cargo test -p podrunner-podvm -- --ignored runner_starts`

## Blocker

podvm (on break) fixes PodUtils/podvm#229, PodUtils/podvm#230 and PodUtils/podvm#231, and releases a binary with the fixes.

# T-027: WP-05a: mint with an in-process sign module, source conformance (GitHub #27)

**Source:** GitHub #27
**Category:** feature
**Milestone:** M-C
**Priority:** P2
**Effort:** M
**Status:** partial

## Problem

The text of GitHub #27 ("WP-05a: `mint` with an in-process `sign` module, source conformance"), cleaned:

**What and why.** `mint` holds the source credentials and spawns the source sidecar as its child. Only `sign` holds the App key (mode 0400). The sidecar asks `sign` and never holds the key.

**Content.**

- `sign` signs one JWT shape (fixed `iss`, `iat`, `exp` within ten minutes), refuses any other claim, at most one request a minute, and writes an audit line (caller, `iss`, `exp`).
- `mint` forwards the nine source operations; returns the credential as decoded files (`files` map), never `env`. The credential never appears in a log, audit line, report or `status`.
- Revocation: a refused source credential ends `mint` with the `Revoked` class of `pod-core`, names the credential, never prints it, does not restart in a loop; `slots` drains (cfb ask A-C-07).
- `mint` restarts with a backoff, except after `Revoked`.

**The RS256 signer (crate item R-C8 of the study, merged here).** The GitHub App JWT is RS256. `sign` is a module of podrunner (one consumer). The `rsa` feature of `pod-tls` verifies only, and its Cargo.toml names RUSTSEC-2023-0071 for the private-key operations of `rsa` 0.9. So `sign` signs with `ring`, not with the private-key path of `rsa` 0.9. podcrates takes a signer when a second consumer (for example `podkey`) needs one. The question is in PodUtils/podvm#227, section 2.7. Not verified yet: that `ring` signs RS256 with the key form of a GitHub App.

**Acceptance.**

- Source conformance tests 1 to 5, 10 and 11 pass against `mint` with the fake source.
- Test 3 (canary scan of every output, with a positive control) passes for `mint` and `sign`.
- A second claim shape at `sign` is refused and audited.
- `sign` signs a JWT with a test App key in PKCS#1 PEM form, and a verifier accepts it; `cargo deny` shows no `rsa` private-key path.

Dependencies: #16, #21. Cross-repo: PodUtils/podvm#227 section 2.7 (signer question). Estimate (spec): S to M.

## Premise

Read the text of GitHub #27 on 2026-11-11. Nothing measured.

## Approach

A `mint` step that makes the JIT runner configuration with an in-process `sign` module (RS256 of the App key), and the source conformance of the mint operations. Done so far (2026-11-11): `crates/podrunner-mint`: `sign` (RS256 with `ring`; one claim shape; one signature a minute; an audit line for each request; a PKCS#1 PEM key, made by `openssl` for each test run, so that no key is in the repository) and `Mint` (it forwards the nine operations; a revocation is final and names the credential, never its value). `serve` puts `Mint` before each source. The restart of the sidecar with a backoff is in `SidecarSource` (T-029).

## Prove

`cargo test -p podrunner-mint`

# T-028: WP-06: the fake scale-set service (Go, test only) (GitHub #28)

**Source:** GitHub #28
**Category:** feature
**Milestone:** M-C
**Priority:** P2
**Effort:** S
**Status:** partial

## Problem

The text of GitHub #28 ("WP-06: fake scale-set service (Go, test only)"), cleaned:

**What and why.** The scale-set sidecar (#29) needs a test partner that speaks the scale-set protocol: one message session, a long poll of about 50 s, capacity in a header, messages `JobAvailable`, `JobAssigned`, `JobStarted`, `JobCompleted`, `acquirejobs`, `generatejitconfig`, redelivery of an unacknowledged message, re-queue of an assigned job up to three times.

**Content.** A Go test server under `tools/` or `testdata/`, built only in CI, never released. It serves scripted messages and records requests.

**Fakes stay here (crate item R-C10 of the study, partly).** PodUtils/podcrates@T-106 keeps the fakes of GitHub in each project; no ask to podcrates (PodUtils/podvm#227, section 2.7, note 2).

**Acceptance.**

- The sidecar's conformance run (#29) uses it in the gate.
- It serves a scripted fork pull request on the trusted set, a redelivery and a cancel.

Dependencies: #16. Estimate (spec): S to M.

## Premise

Read the text of GitHub #28 on 2026-11-11. Nothing measured.

## Approach

A Go service for tests only that speaks the six endpoints of the scale set, with scripted answers. Read on 2026-11-11 (`actions/scaleset` v0.5.0, MIT, `go 1.27.3` in its
`go.mod`, through the Go module proxy): the endpoints live under
`_apis/runtime/runnerscalesets`; the message session has create, refresh,
delete, `GetMessage(lastMessageID, maxCapacity)` (a long poll), delete of
a message and `AcquireJobs`; the messages are `JobAvailable`,
`JobAssigned`, `JobStarted` and `JobCompleted`; `GenerateJitRunnerConfig`
gives the JIT configuration. The module holds a test server in
`internal/testserver` (a Go internal package: not importable; MIT allows a
copy with its notice in `THIRD_PARTY.md`). Done so far (2026-11-11): `tools/fake-scaleset` (Go, no dependency
but the tests): the authentication of a GitHub App, the scale set, the
message session, the queue with a short long poll and the capacity
header, `acquirejobs`, `generatejitconfig`, `JobAvailable`,
`JobAssigned`, `JobStarted` with the runner name, `JobCompleted`, a
redelivery of a message that was not deleted, a re-queue of an assigned
job (at most three times), a cancel, the run object and the membership
for the trust lookup, and a record of each request. `go test ./...`
drives it with the real client of `actions/scaleset` v0.5.0: a fork pull
request, a redelivery, a cancel, a job from available to completed. The
gate has a job `go` (format, vet, tests, a static build with no cgo).
Open: the conformance run of the sidecar (T-029) on it.

## Prove

`go test ./...` in the directory of the fake service.

# T-029: WP-07: the github-scaleset source sidecar (static Go) (GitHub #29)

**Source:** GitHub #29
**Category:** feature
**Milestone:** M-C
**Priority:** P2
**Effort:** M
**Status:** partial

## Problem

The text of GitHub #29 ("WP-07: `github-scaleset` source sidecar (static Go)"), cleaned:

**What and why.** A scale set finds jobs with no inbound port. The sidecar `podrunner-source-github-scaleset` imports `actions/scaleset` (Go, MIT, public preview), speaks the source protocol on stdio, and uses `sign` (#27) as its custom JWT signer (DR-3, #8).

**Content.**

- One sidecar for each set; one set for each tier, architecture and host. A host that serves no `untrusted` tier does not poll that set.
- Trust lookup: read the workflow run object by `workflowRunId` (event, actor, `head_repository.fork`, visibility; repository "Actions: read"), then the organization membership (organization "Members: read"). A failed lookup gives `unknown`. Cache by `run_id`, an hourly lookup budget, counts `trust-lookup-failed` and `trust-lookup-throttled`.
- Emits `offer` with no `payload` and no tier; `started` from `JobStarted` with the runner name; `completed`; `cancel`.
- Mapping of GitHub 401, 403, 404, 422, 429 and a revoked App to the `pod-core` classes: the first test.
- Personal-account repositories: `repo-scope-refused`.
- `ended_without_completion` counted for each source; no retry of a lost job (actions/runner#4309).
- Static build (`CGO_ENABLED=0`) in the gate and in the release (#12, #15); THIRD_PARTY.md lists `actions/scaleset`.

**Scale-set client (crate item R-C9 of the study, merged here).** No PodUtils crate is needed: the client is the upstream Go module `actions/scaleset` with a custom JWT signer. A Rust crate comes only if the owner picks the Rust port in DR-3. The fake scale-set service stays in podrunner (#28).

**Acceptance.**

- Source conformance against the fake scale set (#28) passes.
- The error mapping test passes for each status code.
- A fork pull request on the trusted set is withdrawn with `tier-mismatch` and never acquired (on the fake).
- The sidecar binary has no `INTERP` header.

Dependencies: #27, #28, #8 (DR-3). Estimate (spec): S to M. Live proof: #32.

## Premise

Read the text of GitHub #29 on 2026-11-11. Nothing measured.

## Approach

A static Go sidecar (`CGO_ENABLED=0`) over `actions/scaleset` (MIT) that speaks the source wire of T-016; `THIRD_PARTY.md` lists it. Call C1 (DR-3). Done so far (2026-11-11): `tools/scaleset-sidecar` (Go): the source
wire on stdio (a line limit of 1 MiB, `bad-request` for an unknown op or
field, a bad `job_id`, another schema); the scale set and its message
session; `next` turns `JobAvailable` into offers with no payload and no
tier (the job id is the runner request id), with the trust inputs of the
run object and the membership (`unknown` when a lookup fails; counted),
`JobStarted` into `started` with the runner name, `JobCompleted` into
`completed`, or `cancel` for a cancelled job; `claim` acquires the job,
`mint` gives the files of the JIT configuration. Each App JWT comes from
`sign` over the descriptor 3; the key never reaches the sidecar (call
C15). The statuses 401 and 403 map to `source-revoked`, 404 to
`unknown-job`, 422 to `bad-request`, 429 and others to `unavailable`; a
detail names the status only. Go tests: the map of the statuses, the JIT
files, the events. The source tests 1 to 12 pass against the sidecar on
the fake service (`crates/podrunner-source/tests/scaleset.rs`; the gate
runs them). The static build has no `INTERP`. `SidecarSource`
(`crates/podrunner-mint/src/sidecar.rs`) starts the sidecar with a `sign`
channel on its descriptor 3 and starts it again with a backoff (1 s to 60
s), never after a revocation; `serve` builds it from a `github-scaleset`
source (the key file must have no bit for the group or others).
`crates/podrunner/tests/scaleset.rs`: slots, `Mint`, `sign` and the
sidecar on the fake service: the fork pull request on the trusted set is
withdrawn with `tier-mismatch` and never acquired (one acquire in the
record of the fake), the member job runs to `success`, each JWT comes
from `sign` (3 runs, each green; the gate runs it). A finding of the
first run: `sign` refuses a JWT whose `iat` is not within a minute of
its clock; the test now gives `sign` the clock of the host. Open:
`ended_without_completion` and the trust lookup counts in `status`.

## Prove

`cargo test -p podrunner-source --test conformance -- --ignored scaleset`, with the sidecar and the fake service of T-028.

# T-030: WP-10: the image pipeline: pins, age limit, podrunner image build (GitHub #30)

**Source:** GitHub #30
**Category:** feature
**Milestone:** M-C
**Priority:** P2
**Effort:** M
**Status:** open

## Problem

The text of GitHub #30 ("WP-10: image pipeline: pins, age limit, `podrunner image build`"), cleaned:

**What and why.** The official runner stops receiving jobs within 30 days of a release when auto-update is off. Its 2026 releases came 1 to 48 days apart (one gap of 72 days in the older list). A pin list names the payloads a host accepts.

**Content.**

- A pin is `{digest, built_at, runner_version, max_age_days}` in the pin list of the state directory (owner and mode checked). `built_at` comes only from the pin. The job limit is the smaller of the pin's `max_age_days` (default 28) and the offer's `image_max_age_days`, checked at `matched` (`image-stale`, wait) and again at `start`.
- Exit codes of the runner: 0 `done`; 7 `image-stale` plus a stale pin (drain, ask for a new pin); else the code in `exit_code`.
- `podrunner image build`: download the release asset by version, check it against a checksum recorded in this repository by a reviewed commit, extract into a unit image (glibc loader, ICU or invariant mode, OpenSSL, `bash`, `curl`, the hook relay scripts), save the image, record the pin. The builder needs root or a namespace (DR-14); an H0 host imports the image.
- A build workflow on a GitHub-hosted runner, with a hosted fallback label as a standing rule (S12).

**Crate dependencies (crate item R-C5 of the study, the pin part).** The age of the `actions/runner` pin comes from a `pod-services-watch` watch of kind `github-release` (PodUtils/podcrates@T-050, released). podcrates ask: PodUtils/podvm#227, section 2.5 (WP-C4). The control-host part is in #24.

**Acceptance.**

- A pin past its age is `image-stale` at `matched` and at `start`.
- A changed checksum fails the build.
- The built image passes #26 under the podvm driver.

Dependencies: #21, #23, #8 (DR-14). Cross-repo: PodUtils/podvm#227 section 2.5. Estimate (spec): M.

## Premise

Read the text of GitHub #30 on 2026-11-11. Nothing measured.

## Approach

Pins of the runner payload by digest with an age limit, and `podrunner image build` on a build host (call C7, DR-14). It uses the Actions hosts of `pod-services` (PodUtils/podcrates@T-164). The first pin of the runner is the one that podnet measured (T-044, Done): `actions/runner` v2.338.2.

## Prove

`cargo test -p podrunner-image`

# T-031: WP-05b: remote mint (only if DR-4 is yes) (GitHub #31)

**Source:** GitHub #31
**Category:** feature
**Milestone:** M-D
**Priority:** P2
**Effort:** M
**Status:** blocked

## Problem

The text of GitHub #31 ("WP-05b: remote mint (only if DR-4 is yes)"), cleaned:

**What and why.** On H0 the unit, `slots`, `egress` and the host token are one uid. A step can read any file of that uid. So `mint`, `sign` and the sidecar run on another machine, and the unit host reaches `mint` outbound over HTTPS. This issue exists only if the owner answers yes to DR-4 (#4).

**Content.**

- One `POST /v1/OP` for each source operation, mutual TLS with pinned keys on both sides, 1 MiB request limit, a rate limit for each token.
- A host token names one host and one source instance; it is bound to a host key with a rotation interval; a token lives shorter than the key.
- `mint` computes the tier itself from the run object; the unit host never states one.
- An offer ledger: offers assigned to one host, capped at that host's configured slot count whatever `capacity` says; one credential for each `job_id` (`credential-already-issued`); the ledger is on disk before `mint` answers.
- Reconciliation: a credential with no start within `start_s` plus a margin raises an alert, and past a threshold revokes the token and deletes its runners.
- Signed pin list (Ed25519); the unit host verifies it at every use; the driver compares the `image_id` of `prepare` with the pin before every `start`.
- Audit line for every call with token id, peer address and job id.

**Crate dependencies (crate item R-C6 of the study, merged here).** `pod-tls` has no client authentication today (each config uses no client auth). The transport needs a client certificate and a pinned-key client verifier. podcrates ask: PodUtils/podvm#227, section 2.6, conditional on DR-4. The pin list is signed with `pod-identity` (Ed25519, exists).

**Acceptance.**

- Source conformance tests 6 to 9 and 12 pass over HTTPS.
- A host that cannot reach `mint` serves no `trusted` job.
- A pin list changed on disk is refused at the next start.

Dependencies: #27, #4 (yes). Cross-repo: PodUtils/podvm#227 section 2.6 (`pod-tls` mutual TLS), podcrates `pod-identity`. Estimate (spec): M to L.

## Premise

Read the text of GitHub #31 on 2026-11-11. Nothing measured.

## Approach

A remote `mint` over mutual TLS (`pod-tls`, PodUtils/podcrates@T-165) when DR-4 is yes.

## Prove

`cargo test -p podrunner-mint remote`

## Blocker

The owner answers Q1 (DR-4) yes; then PodUtils/podcrates@T-165 releases the mutual TLS.

# T-032: Milestone M-C: the live gates G-H2 to G-H4 on a fake unit (GitHub #32)

**Source:** GitHub #32
**Category:** chore
**Milestone:** M-C
**Priority:** P2
**Effort:** S
**Status:** blocked

## Problem

The text of GitHub #32 ("Milestone M-C: live gates G-H2 to G-H4 on a `fake` unit"), cleaned:

**What and why.** The first contact with github.com. No job runs: jobs are refused or released. It needs neither DR-4 nor the remote mint. It needs a throwaway GitHub organization and App (DR-7, #7).

**Acceptance.**

- #7 (DR-7) is yes and the throwaway organization exists.
- G-H2: the number of labels a scale set honours is recorded (U7).
- G-H3: the sidecar works with only the three permissions; U8 and U12 recorded.
- G-H4: a fork job that names the trusted set is refused with `tier-mismatch` and not acquired.
- U17, U18, U22 and U23 are run where a refused or released job is enough, or carried to #40.

Dependencies: #27, #28, #29, #30, #7. Estimate (mine): S to M.

## Premise

Read the text of GitHub #32 on 2026-11-11. Nothing measured.

## Approach

Run the live gates with a test organization and App on a `fake` unit.

## Prove

`cargo test -p podrunner-slots --test live -- --ignored gates`, in a live workflow with the gates G-H2 to G-H4.

## Blocker

The ruling "Test accounts" (Q-A9): not now. The owner must allow a test organization and App (DR-7, T-007).

# T-033: Milestone M-D: one live trusted job on a no-root host (GitHub #33)

**Source:** GitHub #33
**Category:** chore
**Milestone:** M-D
**Priority:** P2
**Effort:** S
**Status:** blocked

## Problem

The text of GitHub #33 ("Milestone M-D: one live trusted job on a no-root host"), cleaned:

**What and why.** The goal of the plan on H0 (x86_64, no root, no user namespaces, no mounts, no KVM). It needs every condition at once.

**Acceptance.**

- #4 (DR-4) is yes; #31 is closed; #26 (M-B) is closed.
- podnet WP-N1 shows an `advisory` hold for the .NET runner (U9). podnet ask: PodUtils/podvm#228, section 2.8 (crate item R-C11 of the study; without it, a host with no root runs no `trusted` job).
- One `run: echo` job of a private repository runs on H0 with the minting side on another machine; the report names boundary `redirect`, principal `shared` and the hold.
- G-H1 is read: does github.com send the container-hooks job variable (U6).
- After the job: no residual process or file; the credential directory is gone.

Dependencies: #26, #31, #32. Cross-repo: podnet PodUtils/podvm#228 section 2.8. Estimate (mine): S.

## Premise

Read the text of GitHub #33 on 2026-11-11. Nothing measured.

## Approach

Run one live trusted job on an H0 host through the podvm driver.

## Prove

`cargo test -p podrunner-slots --test live -- --ignored trusted_job`, in a live workflow.

## Blocker

Q1 (DR-4), the ruling "Test accounts" (DR-7) and milestone M-B (T-026).
