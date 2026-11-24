# An alternate payload

The owner allowed an alternate payload on 2026-11-14 (U15,
`docs/decisions.md`): a runner other than the official `actions/runner`
can run a job of github.com and report a third-party runner version
string. T-161 plans it. The plan names its use: a target that no official
runner serves (riscv64, i686, loongarch64, ppc64le), a host with no .NET,
and a unit image with no .NET advisories (T-082).

## The census

An alternate payload must speak each call that the official runner
v2.338.2 makes for one job. The fake broker of the local mode
(`crates/podrunner-broker-fake`) serves each of them, so its routes are
the census. A route that the fake broker does not serve is logged with
its service (`service_of`, T-145).

| Service | Call | What it carries |
| --- | --- | --- |
| Credential files | `.runner`, `.credentials`, `.credentials_rsaparams` in the runner root (`files.rs`) | the broker URL, the OAuth client, and the RSA key that signs its assertion |
| OAuth | `POST /_apis/oauth2/token` | the client assertion; the answer is the access token of the runner |
| Broker session | `POST /session`, `DELETE /session` | the runner session, with its agent and its version |
| Broker messages | `GET /message` (a long poll) | the job request; a runner that is too old gets `RunnerVersionTooOld` |
| Run service | `POST /acquirejob`, `POST /renewjob`, `POST /completejob` | the job message; the lock of the job; the conclusion, the step results and the outputs |
| Results: steps | `POST /twirp/github.actions.results.api.v1.WorkflowStepUpdateService/WorkflowStepsUpdate` | the state of each step |
| Results: logs | `POST /twirp/results.services.receiver.Receiver/Get*`, `.../Create*`, and the blob URLs that they give | the log of each step and of the job |
| Actions | `POST /actions/build/.../runnerresolve/actions`, the tarball URLs | the commit and the tarball of each `uses:` |
| Artifacts | the ArtifactService of the results API and its blob URLs (T-119) | `actions/upload-artifact`, `actions/download-artifact` |
| Cache | the CacheService of the results API and its blob URLs (T-120) | `actions/cache` |
| OIDC | `POST /twirp/x.OIDC/idtoken` and the issuer routes (T-140) | the id-token of a job with `id-token: write` |
| Container hooks | the scripts of `ACTIONS_RUNNER_CONTAINER_HOOKS` (T-035) | `container:`, `services:` and Docker steps with no Docker socket |

The job message (`crates/podrunner-local/src/message.rs`) is the input:
the steps, the contexts, the variables, the secrets with their mask, and
the container and service definitions. The version string of the runner
goes into the broker session. Unverified: the other places where
github.com reads the version string (a header of each call).

## The candidates

1. **A native runner from the crates of podcrates.** The crates
   `pod-actions-expr`, `pod-actions-workflow`, `pod-actions-store` and
   `pod-actions-results` hold the expressions, the workflow model, the
   action store and the results API. A runner also needs the broker and
   run-service client, the step executor (shell steps, JavaScript actions
   with a Node.js, composite actions, Docker steps through the hooks), the
   log upload and the mask. Generic code goes to podcrates on request.
   Effort: large.
2. **An existing runner of a third party** that speaks the same calls and
   builds for the targets above. It is measured first: it costs least,
   and a failure of the measure names the calls that a native runner must
   speak correctly.

## The call

T-161 measures candidate 2 first against the fake broker of the local
mode: one canned job, then the parity suite (`crates/podrunner/tests/local/parity`).
The measure names each call that the candidate does not make as the
official runner does. Candidate 1 is asked of podcrates only when the
measure shows that no existing runner serves the targets.
