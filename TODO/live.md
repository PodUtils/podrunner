Milestones M-B, M-C and M-D: the official runner under podvm, the mint, the scale-set sidecar, the image pipeline and the live gates (GitHub #26 to #33).

# T-026: Milestone M-B: the official runner starts under the podvm driver, no GitHub (GitHub #26)

**Source:** GitHub #26
**Category:** chore
**Milestone:** M-B
**Priority:** P2
**Effort:** M
**Status:** done

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

Prove through the driver of T-023, with no GitHub, that the official runner starts in a podvm unit with no root. podvm `v0.2.0-beta.3` carries the fixes (PodUtils/podvm@T-1703, PodUtils/podvm@T-1706, PodUtils/podvm@T-1707, PodUtils/podvm@T-1710, PodUtils/podvm@T-1719; PodUtils/podvm#254). The unit gets the JIT files of the fake broker of T-038 as its credential files (`crates/podrunner-broker-fake/tests/podvm.rs`). The runner reads its files from `/home/runner`, and the driver writes them to `/podrunner/.runner`: the image links them with relative links, because an absolute link resolves on the host on the supervise rung (`images/runner/Dockerfile`, T-061). The driver sets `DOTNET_EnableWriteXorExecute=0` and no file size limit (U2, U13).

Open from the acceptance of GitHub #26: exit 7 on a "version too old" answer needs `ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE=1` in the start environment, which slots does not set (T-066).

## Prove

`cargo test -p podrunner-podvm -- --ignored runner_starts`

## Done

2026-11-11, `1db8bc3`, `runner-image.yml` run 42987612426 (x86_64, green). On a GitHub-hosted runner with no root (the supervise rung of podvm `v0.2.0-beta.3`), the podvm driver starts the real image of `images/runner/` (`runner@sha256:7f4385a56b0a19a005b6d5707adcbedeb1abfd9bf8207ef58c8bbdabfdbe687c`, runner 2.338.2) with the JIT files of the fake broker; the official runner connects, takes the canned job, runs its step, uploads the logs and step updates, completes the job `Succeeded` and deletes its session; `destroy` counts 0 processes and 0 files. The same passes on the H0 profile of this host with the stand-in image. Milestone M-B holds.


# T-027: WP-05a: mint with an in-process sign module, source conformance (GitHub #27)

**Source:** GitHub #27
**Category:** feature
**Milestone:** M-C
**Priority:** P2
**Effort:** M
**Status:** done

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

`crates/podrunner-mint`: `sign` (RS256 with `ring`; one claim shape; one signature a minute; an audit line for each request; a PKCS#1 PEM key that `openssl` makes for each test run, so that no key is in the repository) and `Mint` (it forwards the nine operations; a revocation is final and names the credential, never its value). `serve` puts `Mint` before each source. The restart of the sidecar with a backoff is in `SidecarSource` (T-029).

## Prove

`cargo test -p podrunner-mint`

## Done

2026-11-11, commits `6df0826` and `9d48ab8`; the gate run 42946099969 is green. `cargo test -p podrunner-mint`: 9 passed: source tests 1 to 5, 10 and 11 through `Mint` over the fake source; the key and the token reach no output (a positive control: the token holds the claims); a second claim shape is refused and audited; a JWT of a PKCS#1 PEM test key (made by `openssl` in each run) verifies with the public key; a revocation is final and names no secret; the sidecar restarts with a backoff. `ring` signs; no `rsa` crate is in `Cargo.lock`.


# T-028: WP-06: the fake scale-set service (Go, test only) (GitHub #28)

**Source:** GitHub #28
**Category:** feature
**Milestone:** M-C
**Priority:** P2
**Effort:** S
**Status:** done

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

`tools/fake-scaleset` (Go, test only, no dependency but the tests) speaks the endpoints of `actions/scaleset` v0.5.0 (MIT) under `_apis/runtime/runnerscalesets`: the authentication of a GitHub App, the scale set, the message session with a short long poll and the capacity header, `acquirejobs`, `generatejitconfig`, the messages `JobAvailable`, `JobAssigned`, `JobStarted` (with the runner name) and `JobCompleted`, a redelivery of a message that was not deleted, a re-queue of an assigned job (at most three times), a cancel, the run object and the membership for the trust lookup, and a record of each request. `go test ./...` drives it with the real client of `actions/scaleset`. The gate job `go` runs format, vet, the tests and a static build with no cgo.

## Prove

`go test ./...` in the directory of the fake service.

## Done

2026-11-11, commit `ece89cd`; the gate run 42946099969 is green: its job `go` runs `go test ./...` of `tools/fake-scaleset` (the client of `actions/scaleset` v0.5.0 against the fake: a fork pull request, a redelivery, a cancel, a job from available to completed) and the source tests of the sidecar on the fake (`crates/podrunner-source/tests/scaleset.rs`: 12 of 12).


# T-029: WP-07: the github-scaleset source sidecar (static Go) (GitHub #29)

**Source:** GitHub #29
**Category:** feature
**Milestone:** M-C
**Priority:** P2
**Effort:** M
**Status:** done

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

`tools/scaleset-sidecar` (Go, `CGO_ENABLED=0`) over `actions/scaleset` (MIT, `THIRD_PARTY.md`; call C1). It speaks the source wire on stdio (a line limit of 1 MiB; `bad-request` for an unknown op or field, a bad `job_id`, another schema). `next` turns `JobAvailable` into offers with no payload and no tier (the job id is the runner request id), with the trust inputs of the run object and the membership (`unknown` when a lookup fails; counted; an hourly budget, `-trust-budget`, 1000). `JobStarted` becomes `started` with the runner name, `JobCompleted` becomes `completed`, a cancelled job becomes `cancel`. `claim` acquires the job; `mint` gives the files of the JIT configuration. Each App JWT comes from `sign` over descriptor 3 (call C15). The statuses map as call C15 says; a detail names the status only. A personal account is refused at start (`repo-scope-refused`, exit 78). `SidecarSource` (`crates/podrunner-mint/src/sidecar.rs`) starts the sidecar with the `sign` channel and starts it again with a backoff (1 s to 60 s), never after a revocation. `serve` builds it from a `github-scaleset` source; the key file must have no bit for the group or others. `status` counts the offers with `unknown` trust inputs and `ended_without_completion` for each source.

## Prove

`cargo test -p podrunner-source --test scaleset -- --ignored` and `cargo test -p podrunner --test scaleset -- --ignored`, with `PODRUNNER_FAKE_SCALESET` and `PODRUNNER_SCALESET_SIDECAR` naming the two Go binaries (the gate job `go`).

## Done

2026-11-11, commits `0930dad`, `9d48ab8` and `5d740e8`; the gate run
42947195332 is green. Its job `go`: `go test` of `tools/scaleset-sidecar`
(the map of 401, 403, 404, 422, 429 and 500 to the codes of the wire,
with a planted secret in the URL and the body that no detail shows; a
personal account refused; the trust budget; the JIT files), the static
build with no `INTERP`, the source tests 1 to 12 against the sidecar on
the fake service, and a run of slots in which a fork pull request on the
trusted set is withdrawn with `tier-mismatch` and never acquired.
`THIRD_PARTY.md` lists `actions/scaleset`.


# T-030: WP-10: the image pipeline: pins, age limit, podrunner image build (GitHub #30)

**Source:** GitHub #30
**Category:** feature
**Milestone:** M-C
**Priority:** P2
**Effort:** M
**Status:** done

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

Done:

- A pin has `runner_version` and its own `max_age_s` (the smaller of it and `[pin] max_age_s` holds), both in the signed pin message. The age is checked at `matched` and again at the start (`image-stale`).
- Exit code 7 of the runner makes slots `degraded` until the operator pins a new payload and clears it.
- `images/runner/runner.toml` (reviewed) holds the version and the SHA-256 of each release tarball; a test holds its version equal to the runner pin of `pod-services`, so a new pin of podcrates reaches podrunner with its tag. `podrunner image verify --tarball FILE --arch ARCH` checks a tarball against it.
- `images/runner/Dockerfile` builds the unit image on Ubuntu 24.04 (pinned by digest, through ECR Public), checks the tarball, adds the hook relay, and runs as uid 1001. A build is not bit for bit the same twice, so a host pins the digest of the archive that it imports.
- `.github/workflows/runner-image.yml` builds an OCI archive for each architecture on a GitHub-hosted runner, imports it into a podvm store with no root (podvm of `images/podvm.toml`, each binary checked by its SHA-256), and starts it. x86_64 is green (T-026).

Open: a green import on aarch64 (T-051). The aarch64 run stays a measure.

## Prove

`cargo test -p podrunner-image`

## Correction

The image code is in `crates/podrunner/src/image.rs` (`podrunner image verify`), not in a crate `podrunner-image`. The Prove reads `cargo test -p podrunner image`.

## Done

2026-11-12, `5518055`. The open part was a green import on aarch64. In runner-image run 43271963245 of `5518055` the aarch64 job imports the OCI archive into a podvm store with no root and starts it as a check (T-051, podvm `v0.2.0-beta.6`). x86_64 was green before (T-026). `cargo test -p podrunner image` passes.

# T-031: WP-05b: remote mint (only if DR-4 is yes) (GitHub #31)

**Source:** GitHub #31
**Category:** feature
**Milestone:** M-D
**Priority:** P2
**Effort:** M
**Status:** done

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

`crates/podrunner-remote`, on `pod_tls::mutual` (TLS 1.3, no resumption, ALPN `podrunner-mint/1`, pinned keys on both sides; PodUtils/podcrates@T-165):

- The gate and the ledger of the mint side (`gate.rs`, `ledger.rs`). Each host (the name of its pinned key) holds its own offers, at most its slot count. The mint computes the tier of each offer (`podrunner_tier::intake`) and withdraws one that the set may not run. A host cannot touch the offer of another host (`unknown-job`). One credential for each job id (`credential-already-issued`), synced to the ledger before the answer. Each call is an audit line with the host, the peer and the job id.
- The transport (`transport.rs`): `POST /v1/source` with one request line, one request on each connection, a body of at most 1 MiB, 120 calls of a host in 10 s. The client is `Wire` over the `Exchange` `Https`. The source wire to a sidecar is serial, so a long poll of one host holds each other call to that source.
- `podrunner mint-serve --config FILE` (`crates/podrunner/src/mint_serve.rs`) and a source kind `remote` on the unit host. A host that cannot reach the mint gets no offer.
- The reconciliation: a credential with no start after `reconcile_after_s` is an alert `credential-with-no-start`; `revoke_after` alerts of one host revoke it (`source-revoked`, an alert `host-revoked`). A revoked host gives back each of its offers, and the sidecar removes the runner of each minted job at a release (`RemoveRunner`).
- The host key and its rotation (call C16): `not_after_s` on each `[[host]]` pin; two keys of one host during a rotation; a key past its life gets HTTP 403.

## Prove

`cargo test -p podrunner-remote` (the gate, the reconciliation, the key life, the source tests over mutual TLS), `cargo test -p podrunner mint_serve`, and with the Go binaries `cargo test -p podrunner-source --test scaleset -- --ignored` (the removal of a runner at a release)

## Done

2026-11-11, `4e6d029`: the remote mint is in `crates/podrunner-remote` and `podrunner mint-serve`; a unit host names it with a source of kind `remote`. The acceptance of GitHub #31 holds: the source tests 1 to 12 pass over mutual TLS; a host that cannot reach the mint gets no offer; a pin list changed on disk fails the check of the next start (T-045). `cargo test -p podrunner-remote`: 6 passed; `cargo test -p podrunner`: each test passes; the ignored scale-set tests pass with the Go binaries (2 and 1).

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

The offline form of G-H2 to G-H4 runs in the gate (T-071: `crates/podrunner/tests/live.rs`, `crates/podrunner/tests/scaleset.rs`). For the live form: a new workflow `live.yml` on dispatch only, which ends with "skipped: no secrets" when the secrets of the test organization do not exist, and the same checks against github.com.

## Prove

`cargo test -p podrunner-slots --test live -- --ignored gates`, in a live workflow with the gates G-H2 to G-H4.

## Blocker

2026-11-13: the owner ruled DR-7 yes. The live gates run on constrained hosts by two agents: GitHub #74 (LIVE-1, the remote mint and the App key) and GitHub #75 (LIVE-2, the unit host and the test jobs). This entry waits for their reports. Before: The ruling "Test accounts" (Q-A9): not now. The owner must allow a test organization and App (DR-7, T-007).

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

2026-11-13: the owner ruled DR-7 yes. The live gates run on constrained hosts by two agents: GitHub #74 (LIVE-1, the remote mint and the App key) and GitHub #75 (LIVE-2, the unit host and the test jobs). This entry waits for their reports. Before: The ruling "Test accounts" (DR-7: not now). The hold of a podvm unit is done (T-065, T-041).

# T-045: The mitigations of DR-4 for trusted jobs at redirect (GitHub #47)

**Source:** GitHub #47
**Category:** feature
**Milestone:** M-D
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

The text of GitHub #47 ("The mitigations of DR-4: trusted jobs at boundary redirect"), cleaned:

The owner answered DR-4 (#4) yes on 2026-11-11, for all trusted repositories, with the mitigations of #4. When `[tier.trusted] min_boundary = "redirect"`:

- One job at a time on a shared uid: the configuration refuses `same_principal_ok = true`.
- Outstanding offers capped at the slot count: each `trusted` source sets `slots`.
- One repository for each host (`affinity = "repository"`): a host takes jobs of one repository only, and refuses an offer of another repository.
- A host key with a rotation interval, and short tokens (with #31).
- A signed pin list (`pod-identity`, Ed25519).
- `status` and each report name the boundary and the hold.
- `untrusted` stays fixed at `guest`.

**Acceptance.** The configuration check refuses `redirect` for `trusted` without the first two items (tests). The affinity, the key rotation and the signed pin list have tests on the fakes.

## Premise

Read GitHub #4 and the ruling of 2026-11-11. The configuration part is measured by its test; the rest is not built.

## Approach

Done: the configuration check of `podrunner_slots::config` (`check_redirect`) refuses `redirect` for `trusted` with no `repository`, with `same_principal_ok = true`, or with a `trusted` source with no `slots`. Placement refuses an offer of another repository than `repository` with `repo-scope-refused` (terminal). A trusted job at `redirect` runs, and its report and the status name `redirect`, `shared` and the hold. The pin list is signed with an Ed25519 key of `pod-identity` (`podrunner pin-sign`); `redirect` needs a signer, and a changed pin list fails the check at each read of the file. The host key and its life are the pinned key of the remote mint and its `not_after_s` (T-031, call C16).

## Prove

`cargo test -p podrunner-slots trusted_at_redirect`, `cargo test -p podrunner-slots one_repository`, `cargo test -p podrunner-slots a_job_at_redirect`, `cargo test -p podrunner-slots signed_list`, `cargo test -p podrunner pin_sign`, `cargo test -p podrunner-remote` (the slot cap of the mint, the key life)

## Done

2026-11-11, `7d1f203`: each mitigation of GitHub #4 has its test. One job at a time on a shared uid, a slot count on each `trusted` source, one repository for the host (`repo-scope-refused`) and a signed pin list are checks of the configuration at `redirect`; the remote mint caps the offers of each host at its slots (T-031); the host key is the pinned key of the remote mint, with its life and its rotation (call C16); the report and `status` name the boundary, the principal and the hold; `untrusted` stays at `guest`. Each command of the Prove passes (`podrunner-slots`: 35 tests, `podrunner-remote`: 5, `podrunner`: 13).

# T-065: The podvm unit runs under podnet pod: an advisory hold for trusted jobs

**Source:** T-033
**Category:** feature
**Milestone:** M-D
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

The podvm driver reports `egress_hold: unknown` (`crates/podrunner-podvm/src/probe.rs`: "The hold of `podnet pod` is not wired into this driver"). Placement reads `unknown` as absent, and a `trusted` job needs at least `advisory` (`crates/podrunner-slots/src/placement.rs`, `needed_hold`). So a podvm unit gets no `trusted` job (`hold-not-available`), and milestone M-D (T-033) cannot pass on an H0 host, also when DR-7 is yes. The unit gets `PODRUNNER_EGRESS=http://unix:PATH`, but nothing sends its traffic there: the official runner ignores that variable, and Node and many tools ignore `HTTPS_PROXY`. SPEC §9.1 and §9.5: the podvm driver composes the redirect with `podnet pod -x http://unix:PATH` and reports the hold from what it set up, else `none`.

## Premise

Read the code and SPEC §9 on 2026-11-11. podnet measured `podnet pod --backend seccomp` with the official runner v2.338.2 (T-044, Done): `advisory`, each CONNECT has a flow; `pod doctor --json` gives `hold.seccomp.level`. podnet `v0.4.0-beta.3` is the latest release. Not measured: `podnet pod` around `podvm run -d` on the supervise rung. Both use seccomp: the supervise rung of podvm and the seccomp backend of podnet can each install a filter that returns `SECCOMP_RET_USER_NOTIF`, and which supervisor gets a socket call is not known.

## Approach

1. Measure first (`scripts/measure-podnet-pod.sh PODNET PODVM DIR`, in `runner-image.yml` on x86_64 with no root): `podnet pod --backend seccomp -x http://unix:SOCK -- podvm run -d IMAGE sh -c 'curl ...'` against a loopback CONNECT stand-in on SOCK. Record: does the CONNECT reach SOCK; does a direct connect to a host loopback port pass (it must, at `advisory`); does `podvm wait`, `exec` and `rm -f` still work. A COORD issue `(to podnet)` names each conflict.
2. Pin podnet in a new file images/podnet.toml (the version and the SHA-256 of the x86_64 binary), as `images/podvm.toml` does.
3. `[unit.podvm] podnet = "PATH"` (in `driver_dir` when it is set). The driver starts `podvm run -d` under `podnet pod -x http://unix:EGRESS_SOCKET` of the unit, and reports `egress_hold: advisory` with the proof of `pod doctor --json` (`hold.seccomp`). With no `podnet`, the hold stays `unknown`.
4. Add a case to `crates/podrunner-broker-fake/tests/podvm.rs`: a step of the canned job runs `curl` to an allowed and a denied host of the policy; the audit of the broker shows both, and the denied one fails.

Done: steps 1 to 3. Measured with podvm `v0.2.0-beta.3` and podnet `v0.4.0-beta.3` on the supervise rung, as a uid with no user namespace (`scripts/measure-podnet-pod.sh`, `scripts/connect-stand-in.py`): under `podnet pod --backend seccomp`, `podvm run -d` starts, the payload connect reaches the egress socket also after `podnet pod` returns, a plain `podvm exec` is not held, an `exec` under a `podnet pod` of its own is held, a loopback connect passes (`advisory`), and `inspect` and `rm -f` work; no podnet process is left. `images/podnet.toml` pins podnet. `[unit.podvm] podnet = "PATH"` gives `PodvmUnit::with_podnet` (`crates/podrunner-podvm/src/unit/hold.rs`): `egress_hold: advisory` from `podnet pod doctor --json`, and the unit, each container `run -d` and each `exec` run under `podnet pod -x http://unix:EGRESS`; a held driver refuses a unit with no socket directory. `crates/podrunner-podvm/tests/hold.rs` passes on H0. `runner-image.yml` run 43067388243 runs the measure and the hold test on a GitHub-hosted runner (passed). Open: step 4.

## Prove

`sh scripts/measure-podnet-pod.sh PODNET PODVM DIR` as a uid with no user namespace, `cargo test -p podrunner-podvm hold`, and the podvm tests of `crates/podrunner-broker-fake/tests/podvm.rs` in `runner-image.yml`.

## Done

2026-11-11, `e39526d`; runner-image run 43105916054 and gate run 43105916088 of `44df9bf` are green. Step 4: `crates/podrunner-broker-fake/tests/held.rs`, the official runner in a held podvm unit with the egress module on its socket. The allowed step gets the body through the upstream; the denied step fails and its name never reaches the upstream. It passes on H0 and runs in `runner-image.yml`. It found T-077 (podnet, GitHub #64), and `destroy` no longer takes the `podnet pod` of the hold for a process of the payload.

# T-066: slots sets ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE=1 in the start environment

**Source:** GitHub #26
**Category:** defect
**Milestone:** M-C
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

slots makes the host `degraded` when the runner exits 7 ("version too old", `RUNNER_TOO_OLD` in `crates/podrunner-slots/src/run.rs`). The runner returns 7 only when `ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE` is the string `1` (SPEC §11.3, measured in M1). No code and no image sets it (`grep -rn RETURN_VERSION crates images` finds nothing). So a stale runner never exits 7, and the check of T-030 does not run. SPEC §10.3 puts the variable in `start.env`, so that a pin cannot drop it. The acceptance of GitHub #26 names it: "`ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE=1` gives exit 7 on a 'version too old' answer of the fake server".

## Premise

Read `crates/podrunner-slots/src/run.rs`, `crates/podrunner-podvm/src/unit.rs` and SPEC §10.3, §11.3 on 2026-11-11. Not measured: the answer of the broker that makes runner v2.338.2 exit 7.

## Approach

slots puts `ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE=1` into the environment of each unit, beside `PODRUNNER_UNIT`. The fake broker gets a job kind "version too old" (the answer of `GET /message` that M1 recorded, `docs/design/APPENDIX-MEASUREMENTS.md`). A level 2 test runs the official runner against it and expects exit 7, and slots `degraded`. Plant: no variable; the runner then exits with another code.

## Prove

`cargo test -p podrunner-slots start_env`, and `cargo test -p podrunner-broker-fake --test runner -- --ignored version_too_old` in `level2.yml`.

## Done

2026-11-11, `61d3c6d`; gate run 43071088850 of `9fb9d17` is green. slots puts `ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE=1` in the start environment; the broker fake answers 403 `RunnerVersionTooOld`. `start_env_lets_a_stale_runner_exit_7` and `version_too_old_gives_exit_7_with_the_variable` pass.

# T-071: The live gate harness of G-H2 to G-H4, offline on the fake scale set

**Source:** GitHub #32
**Category:** chore
**Milestone:** M-C
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

The Prove of T-032 names `cargo test -p podrunner-slots --test live -- --ignored gates` in a live workflow; neither the test nor the workflow exists. When the owner allows a test organization (DR-7), the gates G-H2 (labels of a scale set), G-H3 (the minimum App permissions) and G-H4 (a fork job on the trusted set is refused and never acquired) need code first. G-H4 runs on the fake scale set already (`crates/podrunner/tests/scaleset.rs`).

## Premise

Read T-032, T-029 and `tools/fake-scaleset` on 2026-11-11. Nothing measured on github.com (DR-7: not now).

## Approach

A new test target, crates/podrunner/tests/live.rs, on the fake scale set with the real sidecar. The fake records each request with its body. G-H2: the record has one creation of the scale set, with one label, its name. G-H3: each path of the record is a route of the minimum permissions of SPEC §6.2; a unit test plants a route beyond them. G-H4 stays in `tests/scaleset.rs`. The gate job `go` runs the target. The live form against github.com (a workflow on dispatch only) is part of T-032.

## Prove

`cargo test -p podrunner --test live -- --ignored` with `PODRUNNER_FAKE_SCALESET` and `PODRUNNER_SCALESET_SIDECAR` (the gate job `go`).

## Done

2026-11-11, `e1910fa`; gate run 43073532205 of `296bfd0` is green. `crates/podrunner/tests/live.rs` runs in the gate job `go` on the fake scale set with the real sidecar: one creation of the scale set with one label, its name (G-H2); each request on a route of the minimum permissions, with a planted route that the check names (G-H3).
