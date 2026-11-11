Milestones L2 and L3: the hook path, the guest self-test, the microVM unit, the fake broker and the live acceptance (GitHub #34 to #40).

# T-034: WP-13: measure U19 and U21 on the podvm rung (GitHub #34)

**Source:** GitHub #34
**Category:** measurement
**Milestone:** L2
**Priority:** P2
**Effort:** S
**Status:** open

## Problem

The text of GitHub #34 ("WP-13: measure U19 and U21 on the podvm rung"), cleaned:

**What and why.** The hook route needs three facts that no one measured.

- U21: `curl` runs under the podvm interposer; a host unix socket is visible inside the unit; bash can decode the framed exec stream (4-byte length, stream id, data). podvm PodUtils/podvm#235 (N7) (unix socket paths) is the podvm side.
- U19: a job container of another image, and a Docker action with its five mounts, can be honoured on a `net: shared-host` podvm unit; the image's own environment sets no tool cache variable. podvm PodUtils/podvm#236 (N8) (the container-hook verbs over podvm) is the podvm side.

**Content.** A script first, then a test of the podvm driver.

**Acceptance.**

- Each fact is answered with a log and recorded in #9.
- If bash cannot decode frames, #35 serves the plain mode (stdout as body, exit code from `GET /v1/exec/ID`).

Dependencies: #26. Cross-repo: podvm PodUtils/podvm#229 (N1) to PodUtils/podvm#232 (N4) (through #26), PodUtils/podvm#235 (N7), PodUtils/podvm#236 (N8). Estimate (spec): S.

## Premise

Read the text of GitHub #34 on 2026-11-11. Nothing measured.

## Approach

Measure the hook path facts U19 and U21 on the userland rung of the released podvm binary, and write the result into docs/unknowns.md.

## Prove

A script of `scripts/` that runs the measurement, and its output in the Done section.

# T-035: WP-09: the hook relay and the control socket server of each unit (ctl) (GitHub #35)

**Source:** GitHub #35
**Category:** feature
**Milestone:** L2
**Priority:** P2
**Effort:** M
**Status:** blocked

## Problem

The text of GitHub #35 ("WP-09: hook relay and the per-unit control socket server (`ctl`)"), cleaned:

**What and why.** `container:` jobs and container actions use the official container hooks (`ACTIONS_RUNNER_CONTAINER_HOOKS`), not a Docker API (DR-8, #8). The hook is a `.sh` relay in the unit image: it posts its stdin to `POST /v1/hook/COMMAND` on the unit's control socket and appends the answer to the response file. A step in the same uid can skip the hook, so the host-side server enforces everything.

**Content (the closed field set).**

- Order: `prepare_job` once for each unit, before any step command; every container operation before it is refused. A job with no `container:` and no `services:` gets no `prepare_job`, so all its container operations are refused.
- Two fixed mount tables, matched by exact target. Job container (seven): `/__w`, `/__w/_temp`, `/__w/_actions`, `/__w/_tool`, `/__e` (read-only), `/github/home`, `/github/workflow`. Container action (five): `/github/runner_temp`, `/github/home`, `/github/workflow`, `/github/file_commands`, `/github/workspace`. Sources are directories of the unit, resolved with `RESOLVE_BENEATH`. `/__t` is refused.
- The system mount `/var/run/docker.sock` (both sides that literal) is accepted and dropped. Every other socket bind is refused.
- No host path, device, privileged flag or network mode but the default. `ports` refused on `net: shared-host`. `services:` on `shared-host` is `container-not-supported`. Images only from a registry host in `registries`; a registry credential only for the image's own registry host. `docker build` and Dockerfile actions: `build-not-supported`. podvm health options: `container-not-supported`.
- `POST /v1/job_state`: `job-started` moves `started` to `running` once; `job-completed` records the time and changes no state.
- `prepare_job` answers with `isAlpine` and `context.services` as an array.
- `podrunner` sets the hooks variable for every unit with `container: hook`.

**Acceptance.**

- Unit conformance tests 8, 9, 13, 14, 16, 17, and the container parts of test 3, pass on the `fake` unit path; then on the podvm driver after #34.
- The hook parser and the HTTP server have fuzz targets.
- A registry password in stdin appears in no log, report or `status` (canary).

Dependencies: #23, #38 (fake path), #34 and podvm PodUtils/podvm#229 (N1) to PodUtils/podvm#232 (N4) (H0 path). Cross-repo: podvm PodUtils/podvm#236 (N8) (hook verbs), PodUtils/podvm#235 (N7) (the control socket reaches the unit), and the `exec` workdir inside a volume and a split stderr (WP-V6 of the spec) if the hook path needs them. Estimate (spec): M.

## Premise

Read the text of GitHub #35, `HookInput.cs`, `HookResponse.cs`, `ContainerHookManager.cs` and `ContainerInfo.cs` of `actions/runner` v2.338.2 on 2026-11-11. The hook input of the tests follows those files; no runner ran a hook here.

## Approach

The crate `podrunner-hook`, in steps.

1. Done: the input of a hook call as a closed set, pinned to `HookInput.cs` of `actions/runner` v2.338.2 (`input.rs`), and the checks of the host (`policy.rs`): the order (`prepare_job` first and once), the two mount tables by exact target with each source under the runner directory, the docker socket pair dropped, no volume of the workflow, the `createOptions` allowlist, registries by host and a credential only for the host of its image, no Dockerfile, no ports and no services on the host network. A password is in no `Debug` text and in no refusal. The fuzz target `hook_input`.
2. Done: the control socket of each unit (`http.rs`, `server.rs`): one request for each connection, bounded lines and headers, a body of at most 1 MiB, no chunked body, `POST /v1/hook/COMMAND` and `POST /v1/job_state`, a refusal as HTTP 403 with its code; the relay script `crates/podrunner-hook/relay/podrunner-hook.sh` runs against a live socket in a test (the response file, the output and the exit code of a step, a refusal). The fuzz target `hook_http`.
3. Done: the plan of each call on `UnitDriver` (`unit.rs`): `prepare_job` starts the job container and each service and answers `isAlpine` and `context.services` as a list; `run_script_step` is a `container_exec` whose output (a new host file of `ExecSpec`) and exit code go to the relay; `run_container_step` starts, waits, reads the logs and stops; `cleanup_job` stops each container. `ContainerSpec` has a workdir, mounts of the unit root and the checked options. The fake unit records the mounts and the options. Tested on the fake unit, with a refusal that reaches no driver.
3b. Done: the seam `HookPort` of slots (`crates/podrunner-slots/src/hooks.rs`); `serve` puts `podrunner-hook` behind it (`crates/podrunner/src/hooks/mod.rs`). A unit of a kind with `container: hooks` gets a control socket before its start and the variables `ACTIONS_RUNNER_CONTAINER_HOOKS` (the relay beside the sockets) and `PODRUNNER_CTL`; each refusal is an audit line `hook-refuse` with its code; `job-started` moves `started` to `running` once; destroy closes the socket. `[unit.X]` has `registries` and `runner_dir`. Unit drivers are shared (`Arc`), so the thread of a socket calls the driver. Tested with a recording seam in slots, and with the real relay and the fake unit.

3c. Done: a job with `container:` runs on the official runner v2.338.2 against the fake broker of T-038 through the real relay, the control socket, the checks of the host and the fake unit (`crates/podrunner-broker-fake/tests/container.rs`, in `level2.yml`). Found on the way: the runner takes the hooks only with the job variable `DistributedTask.AllowRunnerContainerHooks` (the fake broker sends it; `docs/contracts/hook.md`), and it sends the job container with no entry point, which the driver refused as an empty argv (fixed: the host starts it with `tail -f /dev/null`). The fake unit now maps each mount target to its source in the argv, the environment and the working directory of a container (`crates/podrunner-fake/src/mounts.rs`), so a step finds its script under `/__w`. 3d. Done: the acceptance on the fake path. The unit conformance tests 1 to 17 (so 3, 8, 9, 13, 14, 16, 17) pass on the fake unit (`crates/podrunner-fake/src/tests.rs`). The canary: the container job of 3c carries a registry password (`jobContainer.credentials`, masked as GitHub masks a secret); the password reaches the checks of the host and no log of the broker, no file of the runner (`_diag`, `_work`) and no file of the unit or of the hooks. A refused credential is in no output of the relay (`crates/podrunner/src/hooks/tests.rs`). The seam `HookPort` gives slots refusal codes and job events only, so no text of a call reaches an audit line, a report or `status`. Found on the way: runner v2.338.2 sends an empty `serverUrl` for a Docker Hub image, which the check refused as another host (fixed: empty is `docker.io`). Open: the podvm driver (step 4).
4. The podvm driver, after T-034.

## Prove

`cargo test -p podrunner-hook`

## Blocker

Step 4, the podvm driver, needs the measurements of T-034 and a podvm unit of T-026; T-026 waits for a podvm release with the fixes of PodUtils/podvm#246 and PodUtils/podvm#248.

# T-036: The guest floor self-test (GitHub #36)

**Source:** GitHub #36
**Category:** feature
**Milestone:** L2
**Priority:** P2
**Effort:** S
**Status:** blocked

## Problem

The text of GitHub #36 ("Guest floor self-test"), cleaned:

**What and why.** `untrusted` jobs need boundary `guest` (a separate kernel). `boundary_proof` is a string that the driver supplies, so the floor is tested, not declared.

**Content.**

- `guest` is accepted only from driver kinds on an allowlist (`microvm` and the kinds the operator lists), after a self-test at `slots` start, on every driver change, and daily.
- The test starts a unit and checks: no network interface; a canary file and a loopback listener that `slots` planted under names the unit does not know are not visible or connectable; a second unit cannot see the first; the boot id differs from the host's and no host pid is visible.
- On every `guest` start: every mount is read-only or a block image, and only the table's host paths are visible.
- Driver executables come from one configured directory with owner and mode checks, never from `PATH`.

**Acceptance.**

- Unit conformance test 11: a `fake` driver that claims `guest` fails; a real guest passes.
- `fake` is refused unless `allow_fake = true`.

Dependencies: #21. Estimate (mine): S to M.

## Premise

Read the text of GitHub #36 on 2026-11-11. Nothing measured.

## Approach

A self-test inside a unit that proves the floor of a `guest` boundary; a fake that claims `guest` fails it. Done so far (2026-11-11): `crates/podrunner-slots/src/guest.rs`: a
test unit writes the boot id of its kernel, its interfaces other than
`lo`, whether a canary file of the host is visible, and whether the
process of podrunner is visible; the boot id must differ from the
host's, and each other check must find nothing. `serve` accepts a claim
of `guest` only from a kind on `guest_kinds` (default `microvm`) and
after this test; else the claim drops to `none` (`boundary_proof:
guest-self-test-failed`). A fake unit that claims `guest` fails it (same
boot id). A listener on the loopback of the host, which the test plants
and the unit tries with `nc` or `bash`, must see no connection; a unit on
the network of the host reaches it and fails. A second unit that runs beside a first one must not see
the process or the file of the first; a second fake unit sees both. Each
kind that claims `guest` is tested again each day (`RETEST_MS`) and when
its driver file changes (inode, size, time of change), on a free kind;
`serve` gives slots the retest. Each driver (the podvm binary, a sidecar)
is an absolute path, in `driver_dir` when it is set, and `serve` refuses a
file or a directory that another user owns or that the group or the
others can write. Open: the read-only mounts on each `guest` start, and a
pass on a real guest; both need the microvm kind (T-037).

## Prove

`cargo test -p podrunner-slots guest`

## Blocker

The read-only mounts and the pass on a real guest need the `microvm` kind (T-037), which waits for the machine tier of podvm (PodUtils/podvm@T-1656; podvm on break).

# T-037: WP-11: the microvm unit driver (form set by DR-5) (GitHub #37)

**Source:** GitHub #37
**Category:** feature
**Milestone:** L2
**Priority:** P2
**Effort:** M
**Status:** blocked

## Problem

The text of GitHub #37 ("WP-11: `microvm` unit driver (form set by DR-5)"), cleaned:

**What and why.** The `untrusted` tier (fork pull requests) needs a guest. Host class H2 (`/dev/kvm` usable, x86_64) serves `untrusted` and `trusted`. aarch64 has no `untrusted` unit in v1.

**Content (either form).** No NIC; a vsock channel to the broker (hold `enforced`); the credential directory on the guest's tmpfs (`credential_store: tmpfs`, `principal: distinct`); OCI image to root filesystem; a pinned kernel; `container: none` until a guest runner image exists; a bounded timeout for every guest and no emulator left after a run.

- Own driver (if DR-5, #5, says so): port of a launcher in a private archive of the owner for a user in group `kvm`, no `sudo`; retired when PodUtils/podvm@T-1656 lands.
- Adapter (default): the podvm machine tier once PodUtils/podvm@T-1656 lands.

**Crate dependencies (crate item R-C14 of the study, merged here).** `pod-microvm` is a reserved name in podcrates with a study only (PodUtils/podcrates@T-062). podcrates was told that the `microvm` kind waits for PodUtils/podcrates@T-062 or PodUtils/podvm@T-1656: PodUtils/podvm#227, section 2.7. The `guest` floor needs the self-test of #36.

**Acceptance.**

- The #36 self-test passes on a KVM host.
- Unit conformance tests 1 to 7, 10, 11, 12 and 15 pass.
- A fork pull request offer on the untrusted set runs a `run:` job in the guest on the fake source; a `container: true` need is `container-not-supported`.

Dependencies: #5, #36. Cross-repo: PodUtils/podvm@T-1656 (adapter form), PodUtils/podcrates@T-062 (PodUtils/podvm#227 section 2.7). Estimate (spec): M to L.

## Premise

Read the text of GitHub #37 on 2026-11-11. Nothing measured.

## Approach

An adapter over the machine tier of podvm (DR-5, 2026-11-11): the `microvm` kind starts a podvm unit with `--podvm-tier=machine`, and slots accepts its `guest` claim only after the self-test of T-036. Keep it behind `UnitDriver`, so that an incus adapter (the owner's note to podvm) can be a second kind.

## Prove

`cargo test -p podrunner-microvm`

## Blocker

podvm (on break) lands its machine tier (PodUtils/podvm@T-1656).

# T-038: WP-12: the canned-message fake broker (level 2) (GitHub #38)

**Source:** GitHub #38
**Category:** feature
**Milestone:** L2
**Priority:** P2
**Effort:** S
**Status:** blocked

## Problem

The text of GitHub #38 ("WP-12: canned-message fake broker (level 2)"), cleaned:

**What and why.** Level 2 runs the real official runner offline. A fake broker serves the runner's routes (`session`, `message`, `acquirejob`, `renewjob`, `completejob`, and the results routes) from recorded or hand-written job messages. Also, a seven-workflow conformance contract from a private archive of the owner runs against `runner.server` inside a unit (its script removes every container on the host; change it first).

**Fakes stay here (crate item R-C10 of the study, merged here).** The fakes of GitHub stay in podrunner (PodUtils/podcrates@T-106; cfb ask A-X-18). This broker can use `pod_testkit::stand::http` (podcrates `v0.14.0`, feature `stand`: an HTTP/1.1 server on the loopback that records what each client sent) as its base. No ask to podcrates (PodUtils/podvm#227, section 2.7, note 2).

**Acceptance.**

- The runner in a podvm unit takes one canned `run:` job, runs it and completes it.
- The corpus holds the `ubuntu-24.04` jobs of the podcrates workflows that need no `sudo`.

Dependencies: #26. Estimate (spec): M to L.

## Premise

Read the text of GitHub #38 on 2026-11-11. Nothing measured.

## Approach

A fake broker of GitHub Actions that serves canned messages to the official runner, for level 2.

Read so far (2026-11-11, `actions/runner` v2.338.2): the JIT files of a scale set (`.runner` with `ServerUrlV2` and `UseV2Flow`, `.credentials` with the OAuth client, `.credentials_rsaparams`) send the listener to the broker (`src/Runner.Common/BrokerServer.cs`: create and delete a session, get a message, acknowledge a request, an optional long-poll WebSocket probe) and the run service (`src/Runner.Common/RunServer.cs`: get the job message, renew, complete); `src/Runner.Listener/Configuration/OAuthCredential.cs` gets a token with a client assertion signed by the RSA key. The HTTP clients of these routes are under `src/Sdk/`. The release tarball of v2.338.2 unpacks and its dependencies (ICU) are on this host, so the broker can start against the runner as a process of a fake unit before T-026. Step 1, measured in scratch (2026-11-11): runner v2.338.2 with three made-up files (`.runner`: `agentId`, `agentName`, `poolId`, `serverUrl` and `serverUrlV2` on the loopback, `useV2Flow: true`, `ephemeral`, `disableUpdate`; `.credentials`: scheme `OAuth` with `clientId` and `authorizationUrl`; `.credentials_rsaparams`: `d`, `dp`, `dq`, `exponent`, `inverseQ`, `modulus`, `p`, `q` in base64) calls `POST /_apis/oauth2/token` (a client-credentials form with a signed assertion; the answer `{"access_token", "token_type", "expires_in"}`), then `POST /session` (a `TaskAgentSession`; the answer gives it back with a `sessionId`), then long-polls `GET /message?sessionId=..&status=Online&runnerVersion=2.338.2&os=Linux&architecture=X64&disableUpdate=true` (HTTP 202 with no body: no message) and prints "Listening for Jobs". The runner needs an environment with no proxy variables of the sandbox (`RunnerWebProxy` throws on them). Steps: (1) and (2) done: `crates/podrunner-broker-fake` serves the token, the session (create, delete), the long poll of `/message` (a `RunnerJobRequest` that names the run service), `acquirejob` (a job message of type `RunnerJobRequest`, which selects the run service path of the listener; a made-up JWT as the job token, which the runner parses), `renewjob` and `completejob` (the conclusion and the step names). The ignored test `crates/podrunner-broker-fake/tests/runner.rs` runs the official runner v2.338.2 on this host: a job of one `run:` step writes the computed marker `podrunner-step-42` and ends `succeeded`; a job whose first step exits 3 ends `failed`, and its second step does not run. `.github/workflows/level2.yml` runs it on GitHub with the tarball checked by `podrunner image verify`; its first run (42958443639, `a345102`) is green. (3) done: the variable `system.github.results_endpoint` of the job message turns on the results client of the Worker; the broker answers the twirp calls `Get*SignedBlobURL` with a blob URL of its own (`BLOB_STORAGE_TYPE_UNSPECIFIED`), keeps each `PUT` of a blob, answers `Create*Metadata` with `{"ok": true}`, and records `WorkflowStepsUpdate`; the test finds the output of the step (`podrunner-step-42`) in the uploaded logs. A body over 1 MiB is refused (the reader of `podrunner-hook`). (4) done: `corpus/podcrates.json`, written by `scripts/corpus.py` from podcrates `59bca1f`, holds 9 jobs (`ci.yml` lint, test, record; deny, live, plants, secrets; `upstream.yml` sources, mirrors) and lists 13 excluded jobs with the reason. A `Job` now carries the `env:` of the workflow and the job (`environmentVariables`) and of each step (`environment`), and the `if:` of each step (the runner evaluates it). The checkout becomes a clone of `PODRUNNER_CORPUS_SRC` at the commit. Measured on this host: `ci.yml/record` succeeds on the official runner (the tests of pod-todo, the record check, the step with `if: github.event_name == 'push'`). The corpus job of `level2.yml` runs two jobs on each push and the whole corpus each week. Fixed on the way: `WorkflowStepsUpdate` answered no `ok`, and the Worker logged an error at each update. Open: the run in a podvm unit (T-026, blocked on a podvm release).

## Prove

`cargo test -p podrunner-broker-fake`

## Blocker

The run in a podvm unit needs T-026, which waits for a podvm release with the fixes of PodUtils/podvm#246 and PodUtils/podvm#248.

# T-039: Milestone L2: level 2 green, the hook path measured (GitHub #39)

**Source:** GitHub #39
**Category:** chore
**Milestone:** L2
**Priority:** P2
**Effort:** S
**Status:** open

## Problem

The text of GitHub #39 ("Milestone L2: level 2 green, hook path measured"), cleaned:

**Acceptance.**

- #34, #35, #36, #38 closed; #37 closed or blocked on DR-5 (#5) with a record.
- Level 2 runs in a scheduled workflow; the run link is recorded.
- A canned `container:` job passes the hook path on the podvm driver, or each refusal is named.

Dependencies: #34 to #38. Estimate: the sum of its issues.

## Premise

Read the text of GitHub #39 on 2026-11-11. Nothing measured.

## Approach

Close L2 when T-034 to T-038 are done and the level 2 suite is green.

## Prove

`cargo test --workspace -- --include-ignored level2`, and the gate run of the closing commit.

# T-040: Level 3: the live acceptance suite (GitHub #40)

**Source:** GitHub #40
**Category:** chore
**Milestone:** L3
**Priority:** P3
**Effort:** M
**Status:** blocked

## Problem

The text of GitHub #40 ("Level 3: live acceptance suite"), cleaned:

**What and why.** The claims that need github.com. Nothing here is observed yet. It needs the throwaway organization of DR-7 (#7).

**Acceptance.**

- The 20-job run of cfb ask A-X-01.
- G-H1 (container-hooks job variable), with U14 (half-set behaviour).
- Label-race and tier tests (G-H4) on the live set.
- The stolen-credential bound (U11): a step that reads the credential files cannot take another job.
- Scale-set label count (U7), App permissions (U8, U12), the runner group refuses public repositories.
- The alert of the leak runbook (#19) fires on a foreign registration.
- U17, U18, U20, U22, U23 answered in #9.
- A container action and a `container:` job on the hook route (G-09 as restated in #6).

Dependencies: #7, #33, #39. Estimate (mine): M.

## Premise

Read the text of GitHub #40 on 2026-11-11. Nothing measured.

## Approach

A live acceptance suite against GitHub with a test organization.

## Prove

`cargo test --workspace --test acceptance -- --ignored`, in a live workflow.

## Blocker

The ruling "Test accounts" (Q-A9): not now (DR-7, T-007).

# T-049: COORD-003: the stand-in broker of the official runner for podvm (GitHub #50)

**Source:** GitHub #50
**Category:** chore
**Milestone:** L2
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

podvm asked in PodUtils/podvm#248 (COORD-030, PodUtils/podvm@T-1706) for the stand-in of the broker that podrunner measured (T-038, step 1): the requests of the official runner up to the long poll, the shape of its three files, and its environment. Its session cannot read this repository. With it, podvm proves `run.sh` on its userland rung (the M-B path of T-026).

## Premise

The answer comes from the measure of T-038 on 2026-11-11 with runner v2.338.2 and `crates/podrunner-broker-fake`.

## Approach

GitHub #50 (COORD-003, to podvm) gives the requests in order with their answers, the three files with made-up values, and the environment. Close it when podvm reads it; take the result of PodUtils/podvm@T-1706 into T-026.

## Prove

`cargo todo check`

## Done

2026-11-11: podvm read GitHub #50 and closed PodUtils/podvm#248: its `experiments/runner-standin.py` follows the stand-in (token, session, a 202 long poll), and PodUtils/podvm@T-1706 is done at podvm `cd2c691`: the official runner v2.338.2 runs `run.sh` up to the long poll on the userland rung (podvm `main`, not released). GitHub #50 is closed.

