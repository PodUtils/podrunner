# R4: upstream CI runner landscape

Recon only. No design. Machine date when read: 2026-11-10 (output of `date` on the recon host).
Author scope: what already exists upstream, what plan `docs/research/ci/podrunner-plan.md` (inputs copy: `podrunner-plan.md`) reinvents or gets wrong.
Issue #47 body was not on disk (`inputs/i47.md` absent, `inputs/i47-comments.md` is empty). Claims below are checked against the plan file, which the brief says the issue points at.

## 0. Method and pins

I cloned each upstream repository shallow (depth 1) into scratch space and read the source. Where a claim is about a release, I read the release page through the forge proxy. I ran a small number of experiments (section 6.4). Every "Observed" below is something I read in a file or ran.

Label meaning: Observed (read the artefact or ran it, with path:line or command), Documented (a doc says it), Source (code says it), Inferred (reasoning), Unverified (not checked).

Pinned commits (12 hex digits, HEAD of the default branch on 2026-11-10 unless a tag is named):

| Repo | Pin | Date of pin | Note |
| --- | --- | --- | --- |
| actions/runner | `310a370bf360` (main); release `v2.338.2` target `d1eb12d4b115` | 2026-11-09; 2026-10-26 | `src/runnerversion` on main reads 2.338.0 |
| actions/runner-container-hooks | `d4731a43a2fe` | 2026-09-15 | `package.json` version 0.8.1 |
| actions/scaleset | `716908e50799` | 2026-11-07 | |
| actions/actions-runner-controller | `89f34c4f782d` | 2026-10-27 | `go.mod` requires `github.com/actions/scaleset v0.5.0` |
| ChristopherHX/github-act-runner | `25b9930ac6e8`; latest tag v0.13.0 | 2026-03-21 | no commit for about 8 months |
| ChristopherHX/runner.server | `992ccbbbf9af`; latest tag v3.14.0 (`069646146c90`) | 2026-05-26 | the owner bundle vendors v3.14.0 |
| gitea act_runner (module `gitea.com/gitea/runner`) | `85d007c47f6c` | 2026-11-09 | |
| Forgejo runner | `c07a9b3669dc` | 2026-10-31 | |
| gitlab-org/gitlab-runner | `0eaaeba7607d` | 2026-11-10 | |
| gitlab-org/fleeting/fleeting | `4085ca1ae366` | 2026-11-02 | |
| woodpecker-ci/woodpecker | `7e3374ad0ca4` | 2026-11-09 | |
| buildkite/agent | `b063c621f5ab` | 2026-11-10 | |
| jenkinsci/remoting | `6e5377b0fb3c` | 2026-10-25 | |
| microsoft/azure-pipelines-agent | `43d4426e54e2` | 2026-11-10 | |
| drone-runners/drone-runner-exec | `9decf2941d42` | 2022-06-22 | stale |
| containers/podman | `933226022050` | 2026-11-09 | |
| moby/moby | `8fb3f17fe089` | 2026-11-09 | |
| docker/cli | `7c0f1b261a24` | 2026-11-06 | |
| testcontainers/moby-ryuk | `0509b379b7c4` | 2026-10-21 | |
| joyrex2001/kubedock | `4cf9479f8474` | 2026-11-09 | |
| socktainer/socktainer | `71f2415e6a90` | 2026-11-09 | |
| vindeckyy/ingot-engine | `e474f570f7ee` | 2026-09-07 | |
| crates.io `dookoid` 0.1.2 | crate sha256 prefix `fbf51c302bb92e8e` | published 2026-11-04 | no repository link |
| github/docs | `6ab6574ee892` | 2026-11-10 | docs source |
| github/rest-api-description | `fa8d7bfb041c` | 2026-11-10 | |
| testcontainers-java / -go | `f74d16772003` / `f31e98d4ee9b` | 2026-10-25 / 2026-11-09 | docs source |

Not cloned, read through other means: CircleCI (docs pages only), Docker static tarball (downloaded, `docker-29.9.3.tgz`, not a git pin).

## 1. The ten findings that matter most (full evidence in later sections)

See the reply to the caller for the ranked list. Short form:

1. Container hooks (`ACTIONS_RUNNER_CONTAINER_HOOKS`) replace every `docker` CLI call of the official runner. A hook may be a plain `.sh` file. The runner then never needs a Docker socket. Two catches: a server-side job variable must also be true, and the official docker hook is itself a wrapper over the docker CLI.
2. The plan's `POST {ServerUrlV2}/sessions` is wrong. The source says `session` (singular), and there is an `acknowledge` call and a WebSocket probe the plan does not list.
3. `actions/scaleset` removes polling per job, not the need for an admin credential. One long poll per scale set carries job-available messages with repo, event and labels. Minting stays per runner, not per job.
4. runner.server already implements the Actions service side (V1 flow, artifacts and cache v2 Twirp, timeline, logs, Gitea webhooks). It does not implement the broker V2 flow or JIT.
5. github-act-runner is a client (it can also print a JIT blob). The plan's gap list for it is stale: cache save works since 0.6.0.
6. Docker Engine API servers that need no root already exist: `dookoid` (Rust, MIT, proot, ptrace) and `kubedock` (Go, MIT, needs a cluster). I ran `dookoid` with the stock static docker CLI as an unprivileged user.
7. There is no musl or Alpine build of the official runner, and no riscv64, i686, loongarch64 or ppc64le build. Open issue #801 since 2020.
8. GitLab Runner has a real plugin interface (fleeting, HashiCorp go-plugin over gRPC, 9 RPCs). Its instance contract is SSH or WinRM plus, for docker-autoscaler, `docker system dial-stdio` on the instance.
9. Forgejo runner is declared GPL-3.0-or-later but its source files carry mixed SPDX tags. Drone's exec runner is Polyform (not open source). Gitea's runner is MIT and has a host mode and a Kubernetes backend.
10. Issue 4309 is one reporter, 0 comments, still open. Main still cancels the long poll at once after the job (`Runner.cs:595`). The plan's "5 to 10 %" is that reporter's figure.

## 2. actions/runner (official, .NET, MIT)

Pin: `310a370bf360` (main), release `v2.338.2`. License: MIT (`LICENSE`: "The MIT License (MIT) Copyright (c) 2019 GitHub"). Observed.

### 2.1 JIT config blob and how it is consumed

- Observed: `--jitconfig` is a valid argument of the `run` command only (`src/Runner.Listener/CommandSettings.cs:68`, list for `Commands.Run`). Name constant: `src/Runner.Common/Constants.cs:112`. It is in the secret-args list (`Constants.cs:118`), so it is masked.
- Observed: env var form. `CommandSettings.cs:111-140` scans the environment for the prefix `ACTIONS_RUNNER_INPUT_`, stores the value as an argument, adds secrets to the masker, and then unsets the variable from the process environment (`Environment.SetEnvironmentVariable(fullKey, null)`). So `ACTIONS_RUNNER_INPUT_JITCONFIG` works and is removed from the listener's own environment. Whether the Worker or child steps still see it is not shown by this code. Inferred: the Worker is a child of the Listener and starts after the unset, so it should not inherit it. Unverified by a run.
- Observed: the official example consumes it by env var: `scaleset:examples/dockerscaleset/scaler.go:144` sets `ACTIONS_RUNNER_INPUT_JITCONFIG=...` and `Cmd: /home/runner/run.sh`.
- Format, Observed from the consumer `src/Runner.Listener/Runner.cs:234-256`: `base64(utf8(JSON object))`. The JSON object is `{ "<file name>": "<base64 file bytes>" }`. For each key the runner writes the decoded bytes to `<runner root>/<key>` and sets the Hidden attribute (a no-op on Linux). On Windows it wraps `.credentials_rsaparams` with DPAPI (`#if OS_WINDOWS`).
- Observed, independent producer: `github-act-runner:runnerconfiguration/compat/actions_runner_compat.go:273-283` (`ToJitRunnerConfig`) builds the same structure, and lines 134-142 and 241-250 show the three keys: `.runner`, `.credentials`, `.credentials_rsaparams`. So the plan's "verify: the blob is a base64 map of the files `.runner`, `.credentials`, `.credentials_rsaparams`" is verified by two implementations.
- Observed, content of `.runner`: the `RunnerSettings` data contract in `src/Runner.Common/ConfigurationStore.cs:15-50`: `AgentId, AgentName, PoolId, PoolName, DisableUpdate, Ephemeral, ServerUrl, GitHubUrl, WorkFolder, UseV2Flow, UseRunnerAdminFlow, ServerUrlV2, SkipSessionRecover, MonitorSocketAddress`. The plan's list ("AgentId ... UseV2Flow, an RSA private key and an authorization URL") matches in substance. The RSA key and authorization URL live in `.credentials` and `.credentials_rsaparams` (Inferred from the file names and from `github-act-runner` `DotnetCredentials`, `ToRsaParameters`). I did not decode a real blob (no token).
- Consequence, Observed: the credential files are written into the runner's own root directory and stay there for the life of the process. `HostContext.cs:453-477` names `.runner`, `.credentials`, `.credentials_rsaparams` under the root. Unsetting the env var does not protect the files. The plan's P2 (`--secret ...,once`, "no argv, no env") must therefore also cover three files in the runner root, which the runner reads repeatedly (for token refresh), not once. `DockerCommandManager.Initialize` also reads `.runner` (`Container/DockerCommandManager.cs:50`) to derive a label hash. Inferred: "read once" semantics would break the runner. Unverified by a run.
- Observed: the runner root must be writable. `run.sh` copies `run-helper.sh.template` to `run-helper.sh` on every loop (`src/Misc/layoutroot/run.sh`, function `run`), and the JIT step writes the three files.
- Observed: `run-helper.sh.template` refuses uid 0 unless `RUNNER_ALLOW_RUNASROOT` is set.

### 2.2 Release assets per OS and architecture

Observed, release page `v2.338.2` (published 2026-10-26, API call `.../repos/actions/runner/releases/latest`):

| Asset | Size (bytes) |
| --- | --- |
| actions-runner-linux-arm-2.338.2.tar.gz | 77 599 742 |
| actions-runner-linux-arm64-2.338.2.tar.gz | 140 231 317 |
| actions-runner-linux-x64-2.338.2.tar.gz | 228 071 396 |
| actions-runner-osx-arm64 / osx-x64 | 128 748 638 / 132 966 038 |
| actions-runner-win-arm64 / win-x64 (.zip) | 95 289 164 / 104 210 683 |

- Observed: there is no `linux-musl-*`, no `riscv64`, no `i686`, no `loongarch64`, no `ppc64le`, no `s390x` asset. The plan says "x64, arm, arm64 only": verified.
- Observed (listing the tarballs): `linux-x64` ships `externals/node20`, `node20_alpine`, `node24`, `node24_alpine`. `linux-arm64` ships `node20` and `node24`. `linux-arm` ships only `node20`. The `_alpine` Node builds exist so that JavaScript actions can run inside Alpine job containers. They do not make the runner itself run on musl.
- Observed: release `v2.334.0` to `v2.338.2` all carry the same seven assets. Release dates: 2026-04-21, 06-08, 06-09, 07-20, 08-26, 10-06, 10-22, 10-26. Gaps between consecutive releases are 5 to 7 weeks (the 06-08 to 06-09 and 10-22 to 10-26 pairs are patch releases).
- Observed discrepancy: the release workflow on main builds only `linux-x64, linux-arm64, win-x64, osx-x64, osx-arm64, win-arm64` (`.github/workflows/release.yml:56`), and `src/Misc/externals.sh` has no `linux-arm` branch. Yet the release carries a `linux-arm` asset. How it is built is not visible in the repository I read. Unverified. Risk for armv7: main (`HandlerFactory.cs:58-59`) forces every JavaScript action to `node24`, while the `linux-arm` asset carries only `node20`. Whether a later release fixes this: Unverified.
- Musl: Observed, no musl asset and no musl runtime identifier in `.github/workflows/build.yml:24`. Open issue `actions/runner#801` "self-hosted: support Alpine", opened 2020-11-12, 43 comments, state open on 2026-11-10. Open issue `#3852` "musl libc detection doesn't work correctly" (2025-05-11). Documented, not run. The plan's unknown 5 ("does a musl .NET runner exist") has the answer: not from upstream.
- ICU: Observed, `grep -rn GLOBALIZATION src` finds no `DOTNET_SYSTEM_GLOBALIZATION_INVARIANT` in the runner source. Whether the runner starts with the invariant mode is not shown here. The owner bundle records a SIGXFSZ crash and `DOTNET_EnableWriteXorExecute=0` (`ideas/.../ghactions-local/README.md`, "Building the base"). Unverified for the official release binary.

### 2.3 The 30-day rule, `--disableupdate`, version deprecation

- Documented (`github/docs:content/actions/reference/runners/self-hosted-runners.md:125-135`): auto-update is on by default. "If you disable automatic updates, you will be required to update your runner version within 30 days of a new version being made available." Also line 173: on github.com the runner updates "when a job is assigned to the runner, or within a week of release if the runner hasn't been assigned any jobs."
- Observed: the REST body of `generate-jitconfig` has four fields: `name`, `runner_group_id`, `labels` (1 to 100), `work_folder`. There is no disable-update field (`github/rest-api-description` `api.github.com.json`, `/repos/{owner}/{repo}/actions/runners/generate-jitconfig`, request schema). The blob's `.runner` has a `DisableUpdate` field (`ConfigurationStore.cs:33`), but whether GitHub sets it true for REST-minted JIT runners is Unverified. For scale sets the field exists on the set: `scaleset:types.go` `RunnerSetting{DisableUpdate bool}` (line 143).
- Observed: the runner sends its version and the `disableUpdate` flag on every poll (`BrokerMessageListener.cs:299-306`: `GetRunnerMessageAsync(sessionId, status, BuildConstants.RunnerPackage.Version, VarUtil.OS, VarUtil.OSArchitecture, _settings.DisableUpdate, ...)`). The server decides. The 30-day cut-off itself is server-side and is not in the client code. Unverified how it is enforced beyond the docs.
- Observed, what the runner does when it is too old: `Program.cs:144-166`. An `AccessDeniedException` with `ErrorCode == 1` returns exit code 7 (`RunnerVersionDeprecated`, `Constants.cs:162`) when the environment variable `ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE` is true (`Constants.cs:280`), else exit 1. `run.sh` also honours that variable. This is a ready signal for a launcher that wants to detect a stale image. Observed, not run.
- Observed: exit codes (`Constants.cs:150-162`): 0 success, 1 terminated error, 2 retryable error, 3 and 4 updating, 5 session conflict, 6 configuration refreshed, 7 version deprecated. `run.sh` loops on code 2 ("Restarting runner...").
- The plan's statement "`--disableupdate` does not remove that limit": verified by the Documented text above.

### 2.4 Ephemeral behaviour

- Observed: ephemeral comes from `RunnerSettings.Ephemeral` (in the JIT blob) or `--once`. `Runner.cs:331`: `ExecuteRunnerAsync(settings, command.RunOnce || settings.Ephemeral || returnJobResultForHosted, ...)`.
- Observed: when the job has finished at the backend the runner cancels the message loop at once and exits (`Runner.cs:589-595`, "Job has finished at backend, the runner will exit since it is running under onetime use mode."). There is no grace period.
- Observed: `Runner.cs:717` (`catch (RunnerRequestJobNotFoundException) when (settings.Ephemeral)`) handles `RunnerRequestJobNotFoundException` for ephemeral runners by exiting. Release notes of v2.336.0 list "Exit ephemeral runners on broker acknowledge job-not-found (#4540)".
- Issue 4309: Observed via the API: title "Ephemeral/JIT runner reports "lost communication" despite successful job completion", opened 2026-03-21, state open, 0 comments, last update 2026-03-21. The body names runner versions 2.331.0 and 2.333.0 on Ubuntu 24.04, org-level JIT, V2 flow, and says "~hundreds of jobs/day, issue affects ~5-10% of runs". That figure is a single reporter's claim. The body's proposed fix is a 5 second delay before `messageQueueLoopTokenSource.Cancel()`.
- Later fix: none found. Observed, main at `310a370bf360` still calls `messageQueueLoopTokenSource.Cancel()` right after `RunOnceJobCompleted` (`Runner.cs:595`) with no delay. Release notes for v2.335.0 to v2.338.2 mention no fix for it (I grepped the bodies for ephemeral, jit, broker, long poll). Open PR `actions/runner#4461` (opened 2026-05-28, not merged, `merged: false`) is a different thing: an opt-in "drain sentinel" file for graceful listener shutdown. The plan cites it as a "graceful drain proposal": verified as such, and it is not a fix for 4309.
- Inferred: the failure is a race on the server between two GitHub services. A launcher that waits for the runner process exit cannot tell this case from a real loss. The plan's mitigation ("check the state, and retry in podrunner") has no supporting evidence either way.

### 2.5 ContainerOperationProvider and the docker CLI verbs

Observed: `src/Runner.Worker/ContainerOperationProvider.cs` and `Container/DockerCommandManager.cs`. The runner finds `docker` with `WhichUtil.Which("docker", true, ...)` (`DockerCommandManager.cs:49`) and shells out to it. It never speaks the Engine API itself. Verbs and call sites (all in `DockerCommandManager.cs`):

| docker CLI call | Line | Used for |
| --- | --- | --- |
| `version --format '{{.Server.APIVersion}}'` and `'{{.Client.APIVersion}}'` | 57, 61 | version gate, below |
| `pull IMAGE` (with `--config DIR` when registry auth) | 100, 102 | job and service images, `docker://` actions |
| `build -t TAG -f FILE CONTEXT` | 107 | `Dockerfile` actions |
| `create` with `--name --label --workdir --network --network-alias -p -e -v[:ro] --entrypoint` plus user `options:` | 189 | job and service containers |
| `run` | 261 | container actions |
| `start ID` | 266 | |
| `rm --force ID` | 271 | |
| `logs --details ID` | 276 | |
| `ps` with `--all --quiet --no-trunc --filter label=... id=... status=running --format` | 281 (callers `ContainerOperationProvider.cs:76,246,250`) | stale cleanup, liveness |
| `network create --label X NAME` (`--driver nat` on Windows) | 287, 289 | |
| `network rm`, `network prune --force --filter label=` | 295, 300 | |
| `exec [opts] ID cmd` (stdin piped, `-i`) | 305, 312 | every step in a job container |
| `inspect` with `--format` (env, health) | 355 | `ContainerOperationProvider.cs:286` and `:435` |
| `port ID` | 360 | |
| `--config DIR login REGISTRY -u USER --password-stdin` | 366 | registry auth |

- Observed, health logic (`ContainerOperationProvider.cs:433-455`): `docker inspect --format="{{if .Config.Healthcheck}}{{print .State.Health.Status}}{{end}}"`. Empty means no health check, so the job proceeds. The value `starting` is polled with exponential backoff (2 s to 32 s). `healthy` passes.
- Observed, version gate (`ContainerOperationProvider.cs:584-602`): requires Docker Engine API at least 1.35 on Linux (1.30 on Windows), for both server and client. Observed bug: the client version is parsed from `serverVersionStr` (`DockerCommandManager.cs:79`, `Match(serverVersionStr)` in the client branch), so the client check effectively re-tests the server string.
- Observed: containers are Linux only (`ContainerOperationProvider.cs:51`, "Container operations are only supported on Linux runners").
- Cross-check, Observed by running the static docker CLI 29.9.3 against a fake Unix-socket daemon (section 6.4): `docker network create` sends `POST /v1.45/networks/create`; `docker network rm` sends `GET /networks/{n}` then `DELETE /networks/{n}`; `docker exec -i` sends `GET /containers/{id}/json`, `POST /containers/{id}/exec`, then `POST /exec/{id}/start` with headers `Connection: Upgrade` and `Upgrade: tcp` (an HTTP hijack, a raw bidirectional stream), body `{"Detach":false,"Tty":false}`; `docker login` sends `POST /auth`. `docker create` for a missing image sends `POST /images/create?fromImage=...` and retries create.

### 2.6 Container hooks (the extension point)

This is the finding the brief asked to check carefully.

Observed (runner `310a370bf360`):
- Env var: `ACTIONS_RUNNER_CONTAINER_HOOKS` (`Runner.Common/Constants.cs:247`).
- Gate: hooks are on only if BOTH the env var is non-empty AND the job variable `DistributedTask.AllowRunnerContainerHooks` is true (`Runner.Worker/FeatureManager.cs:8-12`; flag name at `Constants.cs:170`). The job variable comes from the job message, so GitHub controls it. Whether github.com sends it for every job is Unverified. Documented support: GitHub docs page "Customizing the containers used by jobs" (`github/docs:content/actions/how-tos/manage-runners/self-hosted-runners/customize-containers.md`) says the feature is in public preview, Linux only, "root user access is not required", and that Kubernetes and Podman are the intended uses. Actions Runner Controller documents the Kubernetes mode on github.com, which is indirect evidence the flag is sent there (Inferred).
- Failure mode when only half is true: `ContainerOperationProvider.Initialize` (`:36-45`) picks the hook manager if the env var is set, and the docker manager only otherwise. If the env var is set but the job variable is false, `StartContainersAsync` goes to the docker path with `_dockerManager == null`. Inferred: that is a null reference, not a clean error. Not run.
- Four commands (`Container/ContainerHooks/HookInput.cs:19-29`): `prepare_job`, `cleanup_job`, `run_script_step`, `run_container_step`. The ADR says "There is no partial opt in, you must handle every hook" (`docs/adrs/1891-container-hooks.md`).
- Interface (ADR and `ContainerHookManager.cs:130-170`): the hook gets a JSON object on stdin: `command`, `responseFile`, `args`, `state`. Exit 0 is success. The hook writes JSON to `responseFile`. `prepare_job` returns `state`, `context.container{id,network}`, `context.services{name:{id,ports,network}}` and `alpine` (bool, required). `state` is stored and handed back to later commands.
- The hook file must end in `.js`, `.sh` or `.ps1` (`ContainerHookManager.cs:189-196`). It runs through the normal script handler. For `.sh` the shell is `bash` if found, else `sh`. For `.js` it is the runner's bundled Node (`Runner.Common/HostContext.cs:812-835`). So a hook can be a plain bash script that calls any binary, for example `podvm`.
- Where hooks replace docker calls: job and service containers (`ContainerOperationProvider.cs:64`, `:152`, `:322`), every step of a container job (`StepHost.cs:187-190`, `ContainerStepHost.ExecuteAsync` calls `RunScriptStepAsync`), container actions (`ContainerActionHandler.cs:43`, `:260`), and action image pulls (`ActionManager.cs:143`: pre-pull and build steps are skipped when hooks are on). Node runtime detection inside the container uses the `alpine` flag from the hook instead of a `docker exec` (`StepHost.cs:139-145`). With hooks on, the runner makes no docker CLI call for any of these.
- What hooks do not cover (Inferred from the code paths above plus the runner's own design): a `run:` step that itself calls `docker ...` still needs a daemon socket. A job with no `container:` and no `services:` never calls the hook (the docs say `prepare_job` is called only "if you have any service or job containers"), so its steps run on the host through the normal process invoker.
- Official implementations, Observed (`runner-container-hooks:packages/`): `k8s` (Kubernetes API, pods; README requires a service account, `ACTIONS_RUNNER_POD_NAME`, a persistent volume claim for `_work`, and "A job container will be required for all jobs"; "Building container actions from a dockerfile is not supported"; "Docker create options are not supported") and `docker` (README: "mirrors the original docker implementation"; it shells to the docker CLI: `docker create/start/stop/rm/build/logs/port/network/exec -i/run --rm`, `packages/docker/src/dockerCommands/container.ts:24,120,126,138,158,190,280,363,399`). So the official docker hook does not remove the need for a Docker-compatible socket. A custom hook is what removes it. License MIT. Language TypeScript. Repo notice: "right now we are not taking contributions" (README). Package version 0.8.1.
- Answer to the brief's question ("may remove the need for a Docker API socket entirely"): yes for the runner's own container features, provided the hook is written against the unit backend (podvm) and the server flag is set. No for workflow steps that call docker themselves, and no for the other runners (GitLab docker executor, Gitea/Forgejo docker labels, testcontainers).
- Also Observed: two more official hook points that need no flags: `ACTIONS_RUNNER_HOOK_JOB_STARTED` and `ACTIONS_RUNNER_HOOK_JOB_COMPLETED` (`Runner.Worker/JobExtension.cs:304`, `:573`), which run a script before and after a job.

### 2.7 Node runtime handling

- Plan says "downloads Node runtimes (node20, node24)". Refuted for the release assets. Observed: Node is bundled in the tarball under `externals/` (section 2.2). `src/Misc/externals.sh:168-173` fetches Node tarballs at build time (`NODE24_VERSION="24.22.2"`), not at run time.
- Observed on main: `Runner.Common/Util/NodeUtil.cs:7` `BuiltInNodeVersions = { "node24" }`, and `HandlerFactory.cs:58-59`: "Keep accepting legacy action manifests, but execute all JavaScript actions using node24." The released v2.338.2 still ships node20. So the Node version handling is changing between main and the last release. Env var `ACTIONS_RUNNER_FORCE_ACTIONS_NODE_VERSION` exists (`Constants.cs:290`).
- Observed: `ACTIONS_RUNNER_DISABLE_NODE_MAGLEV` exists (v2.335.1 release note: "Allow disable node v8 maglev jit compiler on node24"). Relevant to JIT-restricted hosts. Not run.
- Observed: `ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE` names a directory of pre-fetched action repositories laid out as `<dir>/<owner>_<repo>/<sha>/<one nested dir>`. With `ACTIONS_RUNNER_SYMLINK_CACHED_ACTIONS` true the runner symlinks instead of downloading (`Runner.Worker/ActionManager.cs:1230-1280`). That is an existing upstream mechanism for an offline action mirror and for caches by trust level.

### 2.8 Proxy and TLS environment

- The plan lists "env vars for proxy (`ACTIONS_RUNNER_...`)". Observed: there is no `ACTIONS_RUNNER_` proxy variable. The runner reads `http_proxy`, `HTTP_PROXY`, `https_proxy`, `HTTPS_PROXY`, `no_proxy`, `NO_PROXY` (`src/Runner.Sdk/RunnerWebProxy.cs:47-64`), takes credentials from the URL, and re-exports the variables to child processes (`:81-82`, `:115-116`). ADR `docs/adrs/0263-proxy-support.md` is the design.
- Observed: `GITHUB_ACTIONS_RUNNER_TLS_NO_VERIFY` (4 uses). `HostContext.cs:501` has a `.certificates` file name. I did not trace how a custom CA is consumed. Unverified. The runner does not set `SSL_CERT_FILE` or `NODE_EXTRA_CA_CERTS` (grep of `Runner.Common`, `Runner.Sdk`, `Runner.Worker` for those names found nothing).
- Other variables that matter for a launcher, Observed in source: `GITHUB_ACTIONS_RUNNER_HTTP_TIMEOUT`, `GITHUB_ACTIONS_RUNNER_HTTP_RETRY`, `GITHUB_ACTIONS_RUNNER_DOWNLOAD_TIMEOUT`, `GITHUB_ACTIONS_RUNNER_FORCE_GHES`, `ACTIONS_RUNNER_PRINT_LOG_TO_STDOUT`, `RUNNER_TOOL_CACHE`, `RUNNER_TOOLSDIRECTORY`, `ACTIONS_RUNNER_REQUIRE_JOB_CONTAINER` (defined, `Constants.cs:279`, consumed by the k8s hook README, not by the runner code I read).

### 2.9 Broker flow details the plan gets wrong or omits

Observed (`src/Sdk/WebApi/WebApi/BrokerHttpClient.cs`, `src/Sdk/RSWebApi/RunServiceHttpClient.cs`):

| Plan says | Source says |
| --- | --- |
| `POST {ServerUrlV2}/sessions` | `new Uri(Client.BaseAddress, "session")` at `BrokerHttpClient.cs:143`, `POST`. Delete is `DELETE .../session?sessionId=` (`:195`). Singular. `github-act-runner` also posts `serverV2URL+"/session"` (`protocol/connection.go:343`). |
| `GET .../message?sessionId=..&status=..` | `message` at `:70`. Matches. |
| (not listed) | `POST .../acknowledge` at `:225` (`AcknowledgeRunnerRequestAsync`), gated by `messageRef.ShouldAcknowledge`, a "Temporary feature flag" (`Runner.Listener/Runner.cs:711`). |
| (not listed) | `ConnectRunnerLongPollWebSocketAsync` (`:174-189`), commented "Temporary probe: validates websocket connectivity to the broker listener"; used at `Runner.Common/BrokerServer.cs:192`. Release notes v2.338.0 list "Implement broker-listener WebSocket connectivity probes (#4753)". A WebSocket long poll may be coming. |
| `POST {run_service_url}/acquirejob` | `acquirejob` at `RunServiceHttpClient.cs:85`. Matches. |
| `POST .../renewjob` | `renewjob` at `:197`. Matches. |
| "Complete job ... not covered by the sources I could read" | `completejob` at `RunServiceHttpClient.cs:152`. |

Log, summary and step-update calls (the plan's unknown 2), Observed in an independent client: `github-act-runner:protocol/results/service.go:247-254` lists the Twirp endpoints `twirp/results.services.receiver.Receiver/` with `GetStepSummarySignedBlobURL`, `CreateStepSummaryMetadata`, `GetStepLogsSignedBlobURL`, `CreateStepLogsMetadata`, `GetJobLogsSignedBlobURL`, `CreateJobLogsMetadata`, and `twirp/github.actions.results.api.v1.WorkflowStepUpdateService/WorkflowStepsUpdate`. Blob upload uses Azure headers `x-ms-blob-type: BlockBlob` or `AppendBlob`, `x-ms-blob-sealed`. A capture (plan phase 0) would still be needed for the official runner's exact payloads. Unverified against the official runner.

### 2.10 Network egress of the runner

Documented only (not measured): outbound 443 to the hosts the plan lists. I did not re-verify the host list. The runner also downloads actions as tarballs from `codeload`/API (`ActionManager.cs`, `UseBearerTokenForCodeload` feature flag at `Constants.cs`) and, when told to self-update, a runner archive from a server-supplied URL (`SelfUpdaterV2.cs:147-195`, `DownloadLatestRunner`). Observed: the update writes `runner{version}.tar.gz` into the runner root.

## 3. actions/scaleset and actions-runner-controller (the scale-set API)

Pins: scaleset `716908e50799` (MIT, Go, README says "Public Preview"), ARC `89f34c4f782d` (Apache-2.0, `LICENSE`, requires `github.com/actions/scaleset v0.5.0` in `go.mod:9`).

### 3.1 What a custom autoscaler can use instead of polling per job

Observed (scaleset README and source):
- Objects: a scale set (`RunnerScaleSet`, `types.go:81-91`) is created at repository, organization or enterprise scope (README line 15). Its name is the label that workflows use in `runs-on:`. Multiple labels are described only for GHES (README lines 189-199). Whether github.com honours extra labels is Unverified.
- One message session per scale set: `POST _apis/runtime/runnerscalesets/{id}/sessions` (`session_client.go:45`; endpoint constant `client.go:24`). The session answer carries `messageQueueUrl`, `messageQueueAccessToken` and initial `statistics` (`types.go:124-131`).
- One long poll for the whole set: `GetMessage(lastMessageID, maxCapacity)` (`session_client.go:115`). The server holds the poll about 50 s and answers 202 when empty. The SDK allows 2 minutes per poll (README, "Long Polling"). The capacity goes in header `X-ScaleSetMaxCapacity` (`client.go:39`). Responses hold at most 50 messages.
- Message types (`types.go:15-18`): `JobAvailable`, `JobAssigned`, `JobStarted`, `JobCompleted`. The payload base (`JobMessageBase`, `types.go:47-62`) has `runnerRequestId`, `repositoryName`, `ownerName`, `jobId`, `jobWorkflowRef`, `jobDisplayName`, `workflowRunId`, `eventName`, `requestLabels`, `queueTime`, `scaleSetAssignTime`, `runnerAssignTime`, `finishTime`. `JobAvailable` adds `acquireJobUrl`. `JobStarted` and `JobCompleted` add `runnerId`, `runnerName` (and `result`).
- Statistics on every response (`types.go:133-141`): `totalAvailableJobs, totalAcquiredJobs, totalAssignedJobs, totalRunningJobs, totalRegisteredRunners, totalBusyRunners, totalIdleRunners`. README: scale on `totalAssignedJobs`, never on message counts.
- Acquire: `POST .../runnerscalesets/{id}/acquirejobs` with a JSON array of `runnerRequestId` (`session_client.go:260-291`). It uses the session token as `Authorization: Bearer <messageQueueAccessToken>` (`:291`). The example scaler acquires every `JobAvailable` first, "so jobs are assigned as early as possible", and says acquiring an acquired job is a no-op (`examples/dockerscaleset/scaler.go:32-44`). The answer is the list of ids acquired.
- Ack: `DELETE .../messages/{id}` (`session_client.go:215-229`), same Bearer token. Unacked messages are redelivered.
- Reassignment, Documented in README: a job assigned but not picked up by a runner in time is cancelled and re-queued up to 3 times.
- JIT per runner, not per job: `POST .../runnerscalesets/{id}/generatejitconfig` with `{name, workFolder}` (`client.go:780-781`, `types.go:93-96`). The answer is `{runner{id,name,runnerScaleSetId}, encodedJITConfig}`.
- Inferred from the example scaler and README step 6 ("GitHub assigns a pending job to any idle runner in the scale set"): a runner is not bound to a job at mint time. Any idle runner of the set takes the next job. So "JIT mint per job" becomes "mint one ephemeral runner per assigned job (or pre-mint a warm pool)", and a launcher cannot choose which runner gets which job. Unverified that no routing exists.
- Consequence, Inferred: `JobAvailable` carries `eventName`, `repositoryName`, `requestLabels` before any unit exists. A launcher could pick a unit class from those fields. It cannot guarantee the job lands on that unit if two runners of different classes are idle in the same set. Separate scale sets per class would avoid that (more sets, more sessions).

### 3.2 Credentials, and what this does to "repo-level admin"

Observed, `scaleset:client.go`:
1. The caller supplies a GitHub App (client id, installation id, private key), an App with a custom JWT signer (KMS or HSM), or a PAT (README, "Authentication").
2. The client calls the REST endpoint `registration-token` for the scope: `/repos/{o}/{r}/actions/runners/registration-token`, `/orgs/{org}/...`, `/enterprises/{e}/...` (`client.go:1078-1093`).
3. It exchanges that token at `POST /actions/runner-registration` with `Authorization: RemoteAuth <registration token>` (`client.go:1008-1033`) for an Actions-service URL and an admin JWT (`AdminToken`, expiry parsed from the JWT, `:1069`, `:1103`).
4. Scale-set CRUD, `generatejitconfig`, runner lookup and removal use that admin JWT. Message get, delete and acquire use the session token from step "session".
So one long-lived minting credential (App key or PAT) still exists. It sits with the autoscaler. The per-session and per-message tokens are short lived.

Permission scope, Documented (`github/docs:content/actions/how-tos/manage-runners/use-actions-runner-controller/authenticate-to-the-api.md:29-38,98-107`): an App needs Repository "Administration: Read and write" only for repository scope, and Organization "Self-hosted runners: Read and write" for organization scope. A fine-grained PAT for organization runners needs "Administration: Read" and "Self-hosted runners: Read and write". Classic PAT scopes: `repo` for repository runners, `admin:org` for organization runners.

Compare the REST JIT call the plan uses: Observed (`fine-grained-pat-permissions.json` in github/docs `src/github-apps/data/fpt-2022-11-28/`): `POST /repos/{owner}/{repo}/actions/runners/generate-jitconfig` needs permission `administration: write`; `POST /orgs/{org}/actions/runners/generate-jitconfig` needs `organization_self_hosted_runners: write`. The REST description says "The authenticated user must have admin access to the repository." So "repo-level admin" is true for repository scope on both routes. At organization scope it is not repository admin, it is the organization "self-hosted runners" permission. Inferred: for a user-owned repository (no organization) only repository scope exists.

What changes with scale sets, in short: (a) polling goes from one long poll per runner to one per set; (b) job metadata arrives before the runner exists; (c) the minting credential does not go away; (d) the unit must be a runner of that set, which the workflow targets by set name.

### 3.3 ARC

Observed: ARC consumes `actions/scaleset` (`go.mod:9`). Its Helm chart documents `ACTIONS_RUNNER_CONTAINER_HOOKS` for the Kubernetes container mode (`charts/gha-runner-scale-set/values.yaml:412,442`; template tests at `tests/template_test.go:1248,1307`). So the two official autoscaling products both sit on the same two primitives named here: scale-set messages and container hooks.

The scaleset repo has a fake server for its own tests (`internal/testserver/server.go`, 176 lines, Go `internal` package, so not importable). It stubs only `/actions/runners/registration-token` and `/actions/runner-registration`. It is no stand-in for the broker.

## 4. ChristopherHX/runner.server and github-act-runner

### 4.1 runner.server (pin `992ccbbbf9af`, tag v3.14.0)

- License: MIT, copyright Christopher Homberger 2021-2025 and GitHub 2019 (`LICENSE`). Language C# (a fork of `actions/runner`). Not static: needs .NET (README: dotnet sdk 8 or newer for the tool).
- What it is, Observed (`README.md:5`): "adds two executables to this Project, Runner.Server as a runner backend like github actions and Runner.Client to schedule workflows via commandline", plus a patched `Runner.Listener`.
- Service surface it implements, Observed (`src/Runner.Server/Controllers/`): `AgentController`, `AgentPoolsController`, `AgentRequestController`, `AgentSessionController`, `AuthController`, `ConnectionDataController`, `FinishJobController`, `TimelineController`, `TimeLineWebConsoleLogController`, `LogfilesController`, `ActionDownloadInfoController`, `OidcController`, `RunnerRegistrationController`, `MessageController` (all controllers together: 9 411 lines), `ArtifactController` (v1), `ArtifactControllerV2` (`[Route("twirp/github.actions.results.api.v1.ArtifactService")]`, `CreateArtifact` at line 48) and `CacheController`/`CacheControllerV2` (`twirp/github.actions.results.api.v1.CacheService`). That is the legacy Azure-DevOps-style (V1) session and message queue plus the artifacts and cache Twirp services. It also evaluates workflow YAML (matrix, `needs`, reusable workflows, composite) inside `MessageController.cs` and has an Azure Pipelines mode.
- Not implemented, Observed: `grep -rn -i "acquirejob\|renewjob\|broker\|UseV2Flow\|jitconfig" src/Runner.Server` returns nothing. So no broker V2 flow, no `acquirejob`/`renewjob`, no JIT blob. Consequence, Inferred: the official runner at a current version uses `UseV2Flow` from its `.runner`. runner.server ships its own patched Listener for that reason (README line 138: "a modifed github actions runner Runner.Listener", sic). Whether the stock official runner can register against runner.server by `config.sh --url http://localhost/runner/server --token ...` (README line 199) is Unverified. The README does show it.
- Non-GitHub forges, Observed: `MessageController.cs` defines `GiteaHook` (line 6180) and accepts webhooks at `.../_apis/v1/Message` (README line 232: "send github / gitea webhooks"). It has a Gitea OAuth sign-in section (README lines 350-372). `grep -i forgejo` in `*.cs` returns nothing. A queue hook `OnQueueJobArgs` runs an external script per queued job and passes `RUNNER_SERVER_PAYLOAD` (README lines 454-460). That is an upstream "scale up on queue" seam for a self-hosted forge.
- So the answer to "do they already implement the Actions service side for a non-GitHub forge": yes in part. Its job source is a webhook plus a pulled workflow file, with V1 runner registration. It is the closest existing thing to the plan's P9 stand-in and to the "job-source" a forge like githome would need.
- Owner bundle: `ideas/podrunner-ideas/ghactions-local-phase0/ghactions-local/vendor/runner.server` vendors v3.14.0 (`VENDORED.md`, `.base-commit` `069646146c90`). Its `PATCHES.md` entry 1 says the Results Service is "planned, no code yet" and that w7 fails because "the route [is] missing". Observed contradiction: the vendored tree already contains `Controllers/ArtifactControllerV2.cs` (189 lines) and `CacheControllerV2.cs`, with `[Authorize(AuthenticationSchemes = "Bearer", Policy = "AgentJob")]` on the controller (line 27), and upstream tag v3.14.0 has the same file (HTTP 200 for the raw URL). The w7 log in `PATCHES.md` says `403 (Forbidden)` and `ECONNRESET`, which points at authorisation or transport, not at a missing route. Inferred. Not run.

### 4.2 github-act-runner (pin `25b9930ac6e8`, tag v0.13.0)

- License MIT (`LICENSE`). Language Go, `go 1.24.0` (`go.mod`). Builds with `CGO_ENABLED: 0` for a long matrix of GOOS and GOARCH including `linux/loong64`, netbsd, openbsd, plan9, solaris, windows (`.github/workflows/build.yml:60-78`). Observed from the workflow, not from running `file` on an artefact. "Static" is Inferred from `CGO_ENABLED=0`.
- Protocol, Observed: supports the V2 flow (`protocol/connection.go:337-372` `CreateSessionExt` with `ServerUrlV2` and `UseV2Flow` read from the agent properties; falls back to the V1 pool session; follows `BrokerMigrationMessage`). Implements `acquirejob` (`actionsrunner/runner.go:515`), `renewjob` (`:664`), `completejob` (`worker_context.go:74`), and the Results receiver Twirp calls for step summary, step logs and job logs plus `WorkflowStepsUpdate` (`protocol/results/service.go`).
- JIT: reads `--jitconfig` and env `ACTIONS_RUNNER_INPUT_JITCONFIG` (`main.go:209,348`). It can also write one: `configure --print-jitconfig` calls `ToJitRunnerConfig` (`main.go:282-283`). So a non-GitHub forge can hand a stock-format blob to either runner.
- Identity: registers with agent `Version: "3.0.0"` (`runnerconfiguration/compat/actions_runner_compat.go:199`, `runnerconfiguration/add.go:173`). The plan's concern about a third-party runner sending a version string is therefore already practised by this project. Whether GitHub tolerates it long term: Unverified. The README says it works against github.com.
- Gaps, Observed README "Known Limitations" (lines 135-152): the plan's "`actions/cache` cannot save" is struck through there ("Is now working in 0.6.0, including hashfiles"); step summaries are "now part of 0.13.1"; still open: annotations, problem matchers, expressions in some `with`/`env` positions, weaker secret masking, job outputs sent regardless of secret leakage, you must provide `node` yourself, no self-update. Observed: no commit since 2026-03-21.
- Container needs, Observed (README table lines 11-20): `run:` steps need `bash` or `sh` on PATH; Node actions need `node` on PATH (README recommends Node 20); Docker actions "Not available" on host mode; `container:` and `services:` need a Docker daemon via `DOCKER_HOST`. It executes steps through a fork of `nektos/act` (`actions-oss/act-cli`) "which adds a local task runner without the need for docker" (README "How does it work?").
- Against runner.server, Observed (README): its CI tests run against runner.server, so no GitHub token is needed.

## 5. Other CI systems

Fit labels used in this and the next section: FITS (works with no root, no userns, no mount as far as read), PARTIAL, NO, UNKNOWN. The plan's target hosts also deny ptrace and chroot (plan section 3), so a ptrace tool is marked.

| System | License | Language | How jobs are pulled (outbound only?) | Credential model | Container engine surface | Evidence |
| --- | --- | --- | --- | --- | --- | --- |
| Gitea runner (`gitea.com/gitea/runner`, was act_runner) | MIT (`LICENSE`, "Copyright (c) 2022 The Gitea Authors", MIT text; files carry `SPDX-License-Identifier: MIT`) | Go (`go 1.27`) | Connect-RPC to the forge. Registration declares labels. Outbound only. (Observed in the sibling Forgejo code: client interface `runnerv1connect.RunnerServiceClient` plus `FetchInterval()`, `internal/pkg/client/client.go`; Gitea uses the same shape, `go.mod:8` `connectrpc.com/connect/v2`.) | Registration token gives a runner UUID and token (Forgejo `register` command; Gitea registration code `internal/pkg/config/registration.go`, not read in detail) | Three backends: docker, Kubernetes pods (`act/container/kubernetes*.go`), host. Docker client calls counted in source: `ContainerCreate/Start/Inspect/List/Remove/Wait/Kill/Logs/Attach/StatPath`, `ExecCreate/Attach/Inspect`, `ImagePull/Inspect/List/Remove/Load/Build`, `NetworkCreate/Inspect/List/Remove/Connect/Disconnect`, `VolumeCreate/Inspect/List/Remove`, `CopyToContainer/FromContainer`, `Ping`, `Info`, `ServerVersion`. README prerequisite: "`docker`, or `podman` with `podman.socket` enabled". Host mode needs only bash or sh, node, git 2.34.1 | `act_runner:README.md:3-20`; `act_runner:act/container/` |
| Forgejo runner (`code.forgejo.org/forgejo/runner/v13`) | README: "distributed under the terms of the GPL version 3.0 or any later version" (`README.md:5`). Per-file SPDX is mixed: 44 `.go` files `GPL-3.0-or-later`, 44 `MIT`, 138 of 230 `.go` files carry no SPDX line | Go (`go 1.26.0`) | Connect-RPC via `forgejo/actions-proto v0.8.0`, polling at `FetchInterval`. Outbound only. `register --ephemeral` makes the forge delete the runner after one job (`internal/app/cmd/cmd.go:42`); daemon mode refuses ephemeral (`daemon.go:83-84`) | Registration token, then `UpdateTask`/`UpdateLog` with the runner's own token (`internal/pkg/report/reporter.go:460,502`) | Backends by label scheme: `host`, `docker`, `lxc` (`internal/pkg/labels/labels.go:13-19`). Docker client calls: `ImagePull, ImageInspect, ImageBuild, CopyToContainer, CopyFromContainer, ContainerCreate/Start/Wait/Logs/Kill/Inspect/List/Attach/Remove, ExecCreate/Attach/Inspect, NetworkCreate/Remove/List/Inspect, VolumeList, Ping, Info, ServerVersion` | `forgejo-runner:README.md:5,88`; `labels.go` |
| GitLab Runner | MIT (`LICENSE`, "Copyright (c) 2015-2019 GitLab Inc.") | Go | Runner polls `POST jobs/request` (`network/gitlab.go:682`), also `runners/verify` and `runners` for registration. Outbound HTTPS only. Whether the request is a long poll: Unverified | Runner auth token from registration (`runners`); job token delivered with the job (not read) | Executors: shell, ssh, custom, docker, docker-autoscaler, instance, kubernetes, virtualbox, parallels (`executors/`). `custom` needs no engine: it runs `config_exec`, `prepare_exec`, `run_exec`, `cleanup_exec` (`common/config.go:583-596`), passes job variables as `CUSTOM_ENV_*` (`executors/custom/custom.go:266`), default timeouts 1 h each (`consts.go`). `docker` executor uses the Docker Go client (HTTP API, not the CLI) for `ImagePullBlocking, ImageInspect*, ImageTag, ImageLoad/Import, ContainerCreate/Start/Stop/Kill/Remove/List/Inspect/Wait/Attach/Logs, ExecCreate/Attach, NetworkCreate/Remove/List/Inspect/Disconnect, VolumeCreate/Remove/List/Inspect, Info, ServerVersion` (counts from `grep` over `executors/docker`, `helpers/docker`), and it needs a helper image to clone and upload artifacts (`common/config.go:360`) | `gitlab-runner:common/config.go`, `executors/` |
| Woodpecker agent | Apache-2.0 (`LICENSE`) | Go | gRPC agent to server: `Next` (blocking pull), `Init`, `Wait`, `Done`, `Extend`, `Update`, `Log`, `RegisterAgent`, `ReportHealth` (`rpc/proto/woodpecker.proto:27-37`). Outbound only | Shared agent secret `WOODPECKER_AGENT_SECRET` or a per-agent token via `Auth` RPC (`proto:158`); the agent removes the secret from the step environment (`cmd/agent/core/agent_test.go:109-136`) | Backends `docker`, `kubernetes`, `local` (`pipeline/backend/`). `local` needs no engine (flags `backend-local-temp-dir`, `backend-local-isolated-home`). Docker client calls: `ImagePull, ContainerCreate/Start/Wait/Logs/Kill/Stop/Inspect/Remove, NetworkCreate/Connect/Remove, VolumeCreate/Remove`. No exec, no health | `woodpecker:pipeline/backend/docker/` |
| Buildkite agent | MIT (`LICENSE.txt`) | Go | `GET ping`, `POST register`, `connect`, `disconnect`, `heartbeat`, `PUT jobs/{id}/accept|start|finish` (`api/*.go`). Outbound only. `--disconnect-after-job` (`clicommand/agent_start.go:120,406`) gives one job per agent | Agent registration token, then an agent access token (Unverified naming) | None required by the agent. Containers come from hooks or plugins (Unverified) | `buildkite/agent:api/` |
| Jenkins inbound agent (remoting) | MIT (`LICENSE.md`) | Java | The agent dials the controller: `-webSocket` (single connection) or `-direct HOST:PORT` with `JNLP4-connect` (`docs/inbound-agent.md:43-102`). Outbound from the agent | Per-agent secret `-secret` (`:130`) | None | `remoting:docs/inbound-agent.md` |
| Drone exec runner | Polyform Small Business 1.0.0 OR Polyform Free Trial 1.0.0 (`LICENSE.md`). Not an open-source license | Go (`runner-go v1.3.1`) | Polls the Drone server via `runner-go` (not read) | Not read | Host `exec`; no engine (by name and layout; not read) | `drone-runner-exec:LICENSE.md`; last commit 2022-06-22 |
| CircleCI self-hosted runner | Unverified (binary distribution; no source repository found) | Unverified | Documented: outbound HTTPS to `runner.circleci.com` and `circleci-binary-releases.s3.amazonaws.com`, "No inbound connectivity is required"; cache, workspace and artifacts need `circleci-tasks-prod.s3.us-east-1.amazonaws.com` (`circleci.com/docs/runner/runner-faqs/`) | Resource class token (namespace plus class) (Documented, concepts page) | Machine runner 3 (task-agent, runs as the `circleci` user) and container runner (container-agent polls and starts ephemeral Kubernetes pods) | `circleci.com/docs/runner/runner-concepts/`, read 2026-11-10 |
| Azure Pipelines agent | MIT (`LICENSE`) | C# (.NET) | Long poll `GetAgentMessageAsync` on the pool (`src/Agent.Listener/MessageListener.cs:250`). Outbound | Not read (PAT or OAuth, Unverified) | Container jobs shell out to `docker`: `create, exec, info, inspect, login, logout, logs, network, port, ps, pull, rm, start, version` (`src/Agent.Worker/Container/DockerCommandManager.cs`). Same shape as the GitHub runner, which descends from it (Inferred from the shared class names) | `azure-pipelines-agent:src/Agent.Worker/Container/` |

### 5.1 Fleeting and GitLab docker-autoscaler (could podvm be a fleeting plugin)

Observed (`fleeting` pin `4085ca1ae366`, MIT, `LICENSE` "Copyright (c) 2015-present GitLab B.V."):
- The plugin interface is Go `provider.InstanceGroup` (`provider/interface.go`): `Init, Update, Increase, Decrease, ConnectInfo, Heartbeat, Suspend, Resume, Shutdown`. States: `creating, running, deleting, deleted, timeout, suspending, suspended, resuming`. Capability `suspend_resume` is optional.
- Out of process: HashiCorp go-plugin over gRPC, one service `InstanceGroup` with 9 RPCs (`internal/plugin/proto/instance_group.proto:100-109`), handshake cookie `FLEETING_INSTANCE_GROUP_PLUGIN` (`internal/plugin/plugin.go`). `DESIGN.md` says plugins "are Go interface implementations" and that go-plugin "is designed to work over a local reliable network". The wire contract is protobuf, so a plugin does not have to be Go in principle. Not tested. A non-Go plugin is Unverified.
- The instance contract is what matters for podvm. `ConnectInfo` returns OS, arch, protocol (`ssh`, `winrm`, `winrm+https` only, `provider/interface.go` `Protocol.Valid`), port, user, key or password, internal and external address. The runner then connects by SSH or WinRM (`connector/connector.go`: `Run`, `Dial`, `DialRun`).
- docker-autoscaler (`executors/docker/autoscaler`): over that connection the runner executes `docker system dial-stdio` on the instance (`executors/docker/docker.go:1603-1615`), then speaks the Engine API through it. So the instance needs a `docker` CLI and a daemon or compatible socket. The `instance` executor (`executors/instance`) runs the job script on the instance over the same SSH connection, with no engine.
- Inferred fit for podvm: an `InstanceGroup` where each "instance" is a podvm unit reachable by SSH would serve the `instance` executor with no Docker API. It would serve docker-autoscaler only with an Engine API socket on the instance. podssh exists in the PodUtils set (R2 covers it). I did not check whether podssh can act as the SSH server inside a unit. Unverified.

## 6. Docker-compatible API servers without a docker daemon, root, or userns

### 6.1 API versions and negotiation

- Docker Engine API in moby HEAD: `MaxAPIVersion = "1.57"`, `defaultMinAPIVersion = "1.40"` (overridable with `DOCKER_MIN_API_VERSION`), hard floor `MinAPIVersion = "1.24"` (`moby:daemon/config/config.go:60-72`); `api/swagger.yaml:27` `version: "1.57"`; client `MaxAPIVersion = "1.57"` (`moby:client/client.go:111`). Observed.
- Negotiation, Observed by running the docker CLI 29.9.3 against a fake daemon that answered `HEAD /_ping` with header `API-Version: 1.45`: the CLI sent `HEAD /_ping`, then prefixed every later path with `/v1.45/`. So a server must answer `/_ping` with an `API-Version` header and honour a versioned prefix. A bare path with no prefix is also valid on Docker (not tested here).
- The official runner needs at least 1.35 (section 2.5). Podman's compat tree: `CurrentAPI 1.44.0`, `MinimalAPI 1.24.0` (`podman:version/version.go:37-46`). Podman's own man page says the compat layer offers "the Docker v1.40 API" (`docs/source/markdown/podman-system-service.1.md:11`) and "the server does not reject requests with an unsupported version set". Code and man page disagree (1.44 versus 1.40).

### 6.2 podman system service

Apache-2.0 (`podman:LICENSE`). Compat plus libpod REST trees under `pkg/api/server/register_*.go` (containers, exec, images, networks, volumes, info, ping, version, events, archive, and more). Needs, Documented (`rootless_tutorial.md:30-34`, `rootless.md:12-33`): `/etc/subuid` and `/etc/subgid` ranges for rootless, user namespaces, and an OCI runtime. "Root privileges are required to add or update entries" in those files. Fit for hosts with no userns: NO (Inferred; I did not run podman). Whether podman has a mode with no userns at all: Unverified.

### 6.3 Existing Engine API servers and stubs (search result)

I searched GitHub (repository search through the forge proxy) and crates.io. Everything relevant I found:

| Project | License | Language | What it does | Needs root / userns / ptrace | Pin | Fit |
| --- | --- | --- | --- | --- | --- | --- |
| `dookoid` 0.1.2 (crates.io only, no repository link; "final release ... no further updates planned") | MIT | Rust (about 7 600 lines in `src/`) | Engine API 1.24 to 1.47 over Unix socket or TCP, with hijack for attach and exec; pulls from registries; containers run under `proot -0 -r ROOTFS` | no root. proot works by `ptrace` (README: "proot emulates a root filesystem via `ptrace`"). Shared host network | crate sha256 `fbf51c302bb92e8e...`, published 2026-11-04, 51 downloads when read | PARTIAL: no root and no userns; needs ptrace and an unprivileged proot binary. Provenance is thin (anonymous, no repository) |
| `kubedock` | MIT | Go | Engine API subset that creates Kubernetes pods; flat networking; volumes by copy; no build | needs a Kubernetes cluster, not a local root | `4cf9479f8474` | NO for hosts with no cluster. Good reference for the semantics (section 6.5) |
| `ingot-engine` | Apache-2.0 | Rust | "Docker-compatible container engine", Engine API v1.44 subset, own `clone()`, cgroups v2, overlayfs, bridge | root daemon (`ingotd`), README | `e474f570f7ee` | NO |
| `socktainer` | Apache-2.0 | Swift | Engine API on Apple `container` | macOS only | `71f2415e6a90` | NO |
| `arcbox-docker` (crates.io) | MIT OR Apache-2.0 | Rust | "Docker REST API compatibility layer for ArcBox" (VM based) | Unverified | not cloned | UNKNOWN |
| `contr` | Apache-2.0 | Rust | manages an installed Docker Engine or Podman; not a server of its own | needs an engine | `b0eb54c03d34` | NO |
| `bollard`, `docker-api` crates | n/a | Rust | clients only (search hits) | n/a | not read | n/a |

I found no Rust or Go library whose purpose is "serve the Engine API with a pluggable backend". The closest are the servers above, each tied to its own backend. `dookoid` has a `[lib]` target (`Cargo.toml`), but I did not check whether its API layer is reusable apart from proot. Unverified.

Supply-chain note, Observed: `dookoid`'s `build.rs` edits `~/.bashrc` (and `~/.profile`, `~/.zshrc` if present) on any `--release` build unless `DOOKOID_NO_PATH_FIX=1` is set (`build.rs:11-12,28-68`). I built it with that variable set and confirmed no rc file was created.

### 6.4 What I ran (experiments, this recon host only)

Conditions: Linux 6.12.113 (Debian 13 kernel), uid 1000, no root, `unshare -U true` works here, `kernel.yama.ptrace_scope = 0`, seccomp mode 2 (one filter). This host therefore allows both user namespaces and ptrace. A run here does not show that the thing works where userns or ptrace are denied.

A. Fake daemon, to see exactly what the static docker CLI 29.9.3 (statically linked ELF, `file` output "statically linked") sends. A 40-line Python Unix-socket HTTP server logged requests. Results (path prefix `/v1.45` omitted):

| CLI call | Requests sent |
| --- | --- |
| `version --format ...` | `HEAD /_ping`, `GET /version` |
| `network create --label x n1` | `POST /networks/create` (body has Driver bridge, IPAM, labels) |
| `network rm n1` | `GET /networks/n1`, `DELETE /networks/n1` |
| `network prune --force` | `POST /networks/prune` |
| `create --name c1 --label l IMG` | `POST /containers/create?name=c1`, on 404 `POST /images/create?fromImage=...&tag=...`, create again |
| `start c1` | `POST /containers/c1/start` |
| `exec -i -w /w -e X=1 c1 sh -c true` | `GET /containers/c1/json`, `POST /containers/c1/exec` (AttachStdin true), `POST /exec/e1/start` with `Connection: Upgrade`, `Upgrade: tcp` |
| `logs --details c1` | `GET /containers/c1/json`, `GET /containers/c1/logs?details=1&stderr=1&stdout=1` |
| `ps --all --quiet --no-trunc --filter label=x` | `GET /containers/json?all=1&filters={"label":{"x":true}}` |
| `ps --filter id=c1 --filter status=running` | `GET /containers/json?all=1&filters={"id":{"c1":true},"status":{"running":true}}` |
| `inspect --format=... c1` | `GET /containers/c1/json`, and on failure also `/images/c1/json`, `/networks/c1`, `/volumes/c1`, `/info`, `/plugins/c1/json` |
| `port c1` | `GET /containers/c1/json`; the CLI crashed (nil dereference) when the inspect JSON had no `NetworkSettings`, so a stub must return `NetworkSettings.Ports` |
| `pull alpine` | `POST /images/create?fromImage=docker.io%2Flibrary%2Falpine&tag=latest` |
| `rm --force c1` | `DELETE /containers/c1?force=1` |
| `login ghcr.io -u u --password-stdin` | `POST /auth` with the credentials in the body (the daemon receives them) |
| `info` | `GET /info` |
| `build -t t .` (default) | tries a BuildKit session (`POST /grpc`, then starts a `moby/buildkit` container). That used the `docker buildx` plugin found on this host (`docker buildx version` printed `github.com/docker/buildx 0.13.1+ds1`). The static tarball does not contain buildx |
| `DOCKER_BUILDKIT=0 build -t t DIR` | `POST /build?dockerfile=Dockerfile&t=t&version=1`, with a deprecation warning ("legacy builder is deprecated and will be removed") |

B. `dookoid` 0.1.2, built from the crates.io tarball with `cargo build --release` (31 s, 8 cores), run as uid 1000 with `--data-root` in scratch space and a Unix socket. Static docker CLI 29.9.3 as client. Results:
- Without `proot` on PATH: `docker version` answered `1.47 27.3.1 1.47` (API, server version, client API); `info` answered `linux amd64 dookoid`; `network create/ls/rm`, `volume create`, `pull alpine:3.19` and `images` all worked. The daemon log said "proot was not found; containers cannot start".
- With the static proot binary (downloaded from `proot.gitlab.io/proot/bin/proot`, 1 978 448 bytes, ELF "statically linked") on PATH: `docker run --rm alpine:3.19 echo hello-from-container` printed the text, rc 0; `create`, `start`, `exec c1 sh -c 'echo exec-ok; id -u'` printed `exec-ok` and `0`; `echo piped | docker exec -i c1 cat` printed `piped`; `inspect --format '{{.State.Status}} {{.Config.Labels}}'` printed `running map[l:1]`; `rm --force` worked.
- Gap found: after `docker run -d --network nn --network-alias redis`, `docker inspect` showed `"Aliases":null` for the network and `docker port` printed nothing, so a service alias like `redis` is not recorded. Name resolution between containers was not tested. The README says containers share the host network and that `-p` is accepted but not enforced.
- Not tested: health checks, `docker build`, `logs` content (the `logs --details` run printed nothing for a container that had produced no output), the official runner against it, testcontainers.
- What this establishes: a pure-Rust, MIT, no-root Engine API server and the stock static docker CLI interoperate on the verbs the official runner uses, on a host with ptrace. It does not establish behaviour on a host that denies ptrace, and it does not establish security (README: proot "is not a security boundary").

### 6.5 kubedock as a precedent for pod-dockerapi semantics

Observed (`kubedock:README.md`, `internal/server/routes/`). It is the nearest existing implementation of the plan's P6. Its routes: `_ping`, `version`, `info`, `containers/create|json|start|stop|kill|wait|logs|exec|attach|archive|top|stats|prune|rename|restart|pause|unpause|update|resize|changes|export`, `exec/{id}/start|json|resize`, `images/create|json|load|prune|{name}/json|{name}/tag`, `networks/create|connect|disconnect|prune|{id}`, `volumes`, `events`, `build`, plus a `/libpod/...` mirror. Stated semantics and limits: containers become pods; "Kubedock flattens all networking" with aliases as services; volumes are one-way copies made by an init container; "mounts to devices, or sockets are not supported (e.g. mounting a docker-socket)"; logs are always the full history with stderr folded into stdout; images are tracked, not built or imported (the router has a `POST /build` route; the README says it cannot build); "copying data to a container will implicitly start the container". The plan's P6 network model ("a name table, no real network") and volume model are the same choices.

### 6.6 testcontainers requirements

Documented and Source:
- Ryuk (`testcontainers/moby-ryuk`, MIT, Go): a reaper container started by the library. README usage: `docker run -v /var/run/docker.sock:/var/run/docker.sock -p 8080:8080 testcontainers/ryuk`. It needs the Docker socket inside the container and a TCP port reachable from the test process, and it removes containers, networks, volumes and images by label filter (client calls `ContainerList/Remove`, `Network*`, `Volume*`, `Image*` in `main.go`). Java docs: "Ryuk must be started as a privileged container" and `TESTCONTAINERS_RYUK_DISABLED=true` turns it off (`testcontainers-java:docs/features/configuration.md:82-86`; Go docs `:49-52`).
- Startup in the Java client, Source (`DockerClientFactory.java`, around lines 208-262 at `f74d16772003`): `strategy.getInfo()` (`GET /info`), `versionCmd()` (`GET /version`), `ResourceReaper.instance().init()`, then, unless `TESTCONTAINERS_CHECKS_DISABLE`, a Docker version check, and a "File should be mountable" check that creates a container with a read-only bind and copies a file out of it (`checkMountableFile`, line 284: `withBinds`, `copyArchiveFromContainerCmd`), using image `alpine:3.17` (line 82).
- Docker host detection, Documented (Go docs `:71-94`): `tc.host`, `DOCKER_HOST`, default socket, `docker.host`, rootless socket paths. Java adds `TESTCONTAINERS_DOCKER_SOCKET_OVERRIDE`.
- kubedock's own README tells users to set `TESTCONTAINERS_RYUK_DISABLED=true` and `TESTCONTAINERS_CHECKS_DISABLE=true` as optional (README lines 10-11), which shows that a stub can serve testcontainers without Ryuk.
- Network use by testcontainers (named networks, aliases): not read in source. Unverified.

### 6.7 Docker CLI as a static binary

Observed: `https://download.docker.com/linux/static/stable/{x86_64,aarch64,armhf,armel}/` list `docker-29.9.3.tgz` as the newest. `ppc64le` and `s390x` directories exist but list only `docker-18.06.3-ce.tgz` as the newest. No `riscv64`, `i386`, `loongarch64` directory. I downloaded the x86_64 tarball and ran `file` on `docker/docker`: "ELF 64-bit LSB executable, x86-64, ... statically linked", 44 320 016 bytes, unstripped; `docker --version` printed `Docker version 29.9.3, build 5457199`. The tarball also holds `dockerd`, `containerd`, `runc` (daemon side). `docker-rootless-extras-29.9.3.tgz` exists for rootless dockerd (needs user namespaces). The CLI is Apache-2.0 (`docker/cli:LICENSE`). The plan's "static docker CLI (Apache-2.0)" is verified for four architectures and refuted for the others. The plan lists podvm targets x86_64, aarch64, armv7, riscv64, i686, loongarch64, powerpc64le, so riscv64, i686, loongarch64 and current ppc64le have no upstream static CLI. Building `docker/cli` from source for them: Unverified.

## 7. The table: need, upstream answer, license, fit, evidence

Fit is for hosts with no root, no userns, no mount (the plan adds no ptrace, no chroot). "Evidence" gives repo path and pin.

| Need (plan item) | Existing upstream answer | License | Fit on such a host | Evidence and pin |
| --- | --- | --- | --- | --- |
| Run GitHub jobs with real fidelity | `actions/runner` official | MIT | PARTIAL: x64, arm, arm64 glibc only; no musl, riscv64, i686, loongarch64, ppc64le assets; needs a writable root dir and non-root uid | release `v2.338.2`; `actions/runner@310a370bf360` |
| JIT credential delivery | `--jitconfig` or `ACTIONS_RUNNER_INPUT_JITCONFIG`; blob writes `.runner`, `.credentials`, `.credentials_rsaparams` into the runner root | MIT | FITS for delivery; the three files stay on disk for the process life (affects P2 `once`) | `Runner.Listener/Runner.cs:234-256`, `CommandSettings.cs:111-140` |
| Docker CLI calls of `container:` and `services:` | Container hooks: four commands, any `.sh` or `.js` or `.ps1` hook. Gated by env var plus job variable `DistributedTask.AllowRunnerContainerHooks` | MIT (runner and hooks repo) | FITS in principle: no socket needed if the hook uses podvm. Flag availability on github.com Unverified | `Runner.Worker/FeatureManager.cs:8-12`; `runner-container-hooks@d4731a43a2fe` |
| Official ready-made hooks | `k8s` (needs a Kubernetes API, PVC) and `docker` (wraps docker CLI, needs the Engine API) | MIT | NO for both as shipped | `packages/k8s/README.md`, `packages/docker/src/dockerCommands/container.ts` |
| Mint runner credentials per job | REST `generate-jitconfig` (repo: `administration: write`; org: `organization_self_hosted_runners: write`) or scale-set `generatejitconfig` | n/a (service) | FITS (outbound HTTPS) | `rest-api-description@fa8d7bfb041c`; `scaleset@716908e50799` `client.go:780` |
| Autoscaler without per-runner polling | `actions/scaleset` Go client, one long poll per set, `JobAvailable` and `acquirejobs`, statistics | MIT | FITS for a Go process (outbound only). A Rust port would need the same endpoints: Unverified that they are stable | `scaleset@716908e50799` `session_client.go`, `types.go` |
| Kubernetes autoscaler | ARC on top of scaleset | Apache-2.0 | NO (needs Kubernetes) | `actions-runner-controller@89f34c4f782d` |
| Replace the official runner's protocol client (a native runner) | `github-act-runner` (V2 flow, JIT, results Twirp, cache save since 0.6.0, `CGO_ENABLED=0` matrix) | MIT | PARTIAL: static, many targets; needs `node` and bash on PATH; containers need a Docker daemon; stale since 2026-03-21 | `github-act-runner@25b9930ac6e8` |
| Fake GitHub or forge service for tests (P9) | `runner.server` (V1 queue, artifacts and cache v2 Twirp, timeline, logs, Gitea webhooks); `scaleset/internal/testserver` (two auth routes only) | MIT | PARTIAL: .NET, V1 flow only; no broker V2, no JIT | `runner.server@992ccbbbf9af` `src/Runner.Server/Controllers/` |
| Another forge's runner (Gitea) | `gitea.com/gitea/runner` with docker, Kubernetes and host backends | MIT | PARTIAL: host mode needs no engine; docker label needs the Engine API | `act_runner@85d007c47f6c` |
| Forgejo runner | `forgejo/runner` v13 | GPL-3.0-or-later (README), mixed file headers | PARTIAL: same as Gitea; separate process only (plan agrees) | `forgejo-runner@c07a9b3669dc` |
| GitLab CI | GitLab Runner: `custom` and `ssh` executors need no engine; `docker` needs the Engine API | MIT | `custom`: FITS. `docker`: needs P6 | `gitlab-runner@0eaaeba7607d` |
| GitLab autoscaling over custom instances | Fleeting plugin: go-plugin over gRPC, 9 RPCs; instances must be SSH or WinRM reachable | MIT | UNKNOWN: depends on an SSH endpoint in the unit | `fleeting@4085ca1ae366` |
| Woodpecker | agent with `local` backend (no engine) or `docker` | Apache-2.0 | `local`: FITS | `woodpecker@7e3374ad0ca4` |
| Buildkite, Jenkins inbound, Azure Pipelines | agents that pull outbound and need no engine for plain jobs (Azure container jobs use the docker CLI) | MIT each | FITS for plain jobs | see section 5 |
| Drone exec | Polyform | Polyform Small Business OR Free Trial | not an OSS license | `drone-runner-exec@9decf2941d42` |
| CircleCI | closed task-agent | Unverified | UNKNOWN | docs only |
| Docker Engine API server with no root (P6) | `dookoid` (proot, ptrace); `kubedock` (needs cluster); podman system service (needs userns and subuid); `dockerd` (root) | MIT; MIT; Apache-2.0; Apache-2.0 | `dookoid`: PARTIAL (needs ptrace, thin provenance). Others NO | section 6; `kubedock@4cf9479f8474`; `podman@933226022050`; `moby@8fb3f17fe089` |
| Docker CLI inside the unit | `docker` static tarball for x86_64, aarch64, armhf, armel | Apache-2.0 | FITS on those four; absent for riscv64, i386, loongarch64; ppc64le stale at 18.06 | `download.docker.com/linux/static/stable/`; `docker/cli@7c0f1b261a24` |
| Engine API version negotiation | `HEAD /_ping` with `API-Version`; versioned `/v1.NN/` prefix; runner needs 1.35 or newer; moby max 1.57 | Apache-2.0 | FITS (a server design input) | `moby:daemon/config/config.go:60-72`; Observed run |
| testcontainers | Ryuk needs socket bind mount plus a reachable TCP port; can be disabled by env var | MIT (Ryuk) | PARTIAL: set `TESTCONTAINERS_RYUK_DISABLED` | `moby-ryuk@0509b379b7c4`; `testcontainers-java@f74d16772003` |
| Action archive cache, offline actions | `ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE`, `ACTIONS_RUNNER_SYMLINK_CACHED_ACTIONS` in the runner | MIT | FITS | `Runner.Worker/ActionManager.cs:1230-1280` |
| Job start and end scripts | `ACTIONS_RUNNER_HOOK_JOB_STARTED`, `..._COMPLETED` | MIT | FITS | `Runner.Worker/JobExtension.cs:304,573` |
| Detect a too-old runner image | exit code 7 with `ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE` | MIT | FITS | `Runner.Listener/Program.cs:144-166` |

## 8. Per-claim check of the plan against upstream

| Plan claim (section) | Verdict | Evidence |
| --- | --- | --- |
| `actions/runner` is MIT, C# | verified | `LICENSE` |
| "downloads Node runtimes (node20, node24)" (2.1) | refuted for release assets: Node is bundled under `externals/` | tarball listing; `externals.sh` |
| "Platforms: Linux x64, arm, arm64; macOS; Windows" | verified | release assets |
| "Whether a musl build is shipped: unknown" | answered: none; issue 801 open | assets; `#801` |
| JIT blob = base64 map of `.runner`, `.credentials`, `.credentials_rsaparams` | verified by consumer and by `github-act-runner` producer | section 2.1 |
| `POST {ServerUrlV2}/sessions` (2.1) | refuted: `session`, singular | `BrokerHttpClient.cs:143` |
| `GET .../message` long poll up to 50 s | verified path; the 50 s figure is also in the scaleset README for the scale-set poll. Not measured for the runner poll | `BrokerHttpClient.cs:70` |
| `acquirejob`, `renewjob` | verified | `RunServiceHttpClient.cs:85,197` |
| "Complete job ... not covered" | now covered: `completejob`, results Twirp paths | section 2.9 |
| 30-day rule not removed by `--disableupdate` | verified (Documented) | `self-hosted-runners.md:135` |
| `generate-jitconfig` body fields | verified | REST schema |
| "Docker CLI: network create, create, start, exec, inspect, logs, rm, network rm" (2.1) | incomplete: also `version`, `pull`, `build`, `run`, `ps`, `port`, `network prune`, `login` | section 2.5 |
| 5 to 10 % failure from issue 4309 (8.6) | verified as the reporter's figure; single reporter; open; no fix | section 2.4 |
| "graceful drain proposal: PR 4461" | verified; not merged; not a fix for 4309 | PR API |
| `github-act-runner`: `actions/cache` cannot save, no annotations, weaker masking (2.2) | partly stale: cache save works since 0.6.0; annotations and problem matchers and masking gaps remain | README lines 135-152 |
| `github-act-runner`: needs `node` on PATH and Docker daemon for containers | verified | README table |
| `actions/scaleset`: "It starts the official runner as a process, container or VM" | misleading: the library starts nothing; the example starts Docker containers | `examples/dockerscaleset/scaler.go:138-150` |
| Gitea `act_runner` MIT (verify) | verified | `LICENSE` |
| Woodpecker Apache-2.0 (verify) | verified | `LICENSE` |
| Forgejo runner GPL-3.0-or-later | verified at the README level; source files carry mixed SPDX tags | section 5 |
| "A GPL program may run as a separate process" (2.2) | not a license reading I verified; no legal opinion given | n/a |
| "GitLab Runner ... docker-autoscaler" and four custom scripts | verified | `common/config.go:583-596` |
| "Go with `CGO_ENABLED=0` normally gives a static binary (verify)" | verified for the docker CLI download (static ELF). For `github-act-runner` inferred from the workflow, not from a binary | section 6.7, 4.2 |
| Hooks "may remove the need for a Docker API socket entirely" (brief) | partly: yes for the runner's own container features with a custom hook; no for the official docker hook, steps that call docker, and other runners | section 2.6 |
| Static docker CLI for all podvm targets (P6 proof, phase 3) | refuted for riscv64, i686, loongarch64; stale for ppc64le | section 6.7 |
| Phase 0: "Read `actions/runner` sources ... write the exact JIT blob format and the env names" | done here in sections 2.1 and 2.9 | |
| Q6/P6: `docker build` for Dockerfile actions | no upstream rootless build path found. The docker CLI's default build now wants BuildKit (`POST /grpc`); legacy `POST /build` still works with `DOCKER_BUILDKIT=0` and prints a deprecation. The official hooks k8s variant says Dockerfile actions are unsupported | sections 2.6, 6.4 |

## 9. Things that already exist and that the plan proposes to build, and single points of failure

### 9.1 Already exists upstream

- A job-container replacement point inside the official runner (hooks). Plan P6 treats an Engine API server as the only way. The hooks path should be priced as a route (section 2.6).
- A per-set autoscaling API, Go library, MIT (scaleset). The plan's P8 "slot manager, JIT mint over pod-proxy" re-implements the controller side of what `scaleset/listener` plus the example scaler already do, in Go.
- A fake Actions service (runner.server), including artifacts and cache v2 Twirp and Gitea webhooks. Plan P9 and phase 0's "capture the runner protocol" can start from two independent open clients (`github-act-runner`, official source) and one open server.
- A JIT blob writer (`github-act-runner configure --print-jitconfig`), useful for any forge stand-in.
- Action archive cache and job hooks in the runner (section 2.7, 2.6).
- An exit code for a stale runner (section 2.3).
- A rootless Engine API server prototype (`dookoid`), and a design precedent for the semantic choices (`kubedock`).
- A plugin interface for instance fleets (fleeting) for GitLab.
- A host backend in Gitea, Forgejo (host label) and Woodpecker (`local`), and GitLab `custom`/`ssh` executors, all of which run steps with no container engine.

### 9.2 Hardcoded assumptions that are single points of failure (from upstream facts)

- One runner implementation: every path in the plan runs the official .NET runner. Upstream offers alternates (github-act-runner, Gitea or Forgejo runner, GitLab Runner custom executor). The plan defers them to phase 5.
- One architecture family: the official runner ships only x64, arm, arm64 on Linux; armv7 ships Node 20 only, while runner main moves to Node 24 only (sections 2.2, 2.7).
- One libc: no musl build (issue 801 open since 2020). Plan item "musl image" has no upstream answer.
- One minting credential holder: both REST JIT and scale sets need an admin-class credential (App key or PAT) held by the launcher (section 3.2).
- One service flow: the plan encodes the broker V2 flow. The runner has a WebSocket long-poll probe in progress (`BrokerHttpClient.cs:174`, release notes 2.338). Also, runner.server serves only V1.
- One release cadence dependency: a monthly rebuild is assumed (plan 2.1). Observed gaps between consecutive releases are 5 to 7 weeks. The 30-day clock starts at each release.
- One container interface: the plan insists on an Engine API on a per-job socket. Hooks are an alternative the plan lists nowhere. Conversely, Engine API servers exist only for proot (ptrace), Kubernetes, root daemons or userns (section 6.3), so the plan's "no ptrace, no userns" hosts have no upstream answer for the socket.
- One host-side tool: the docker CLI. No static build for riscv64, i686, loongarch64.
- One egress path: the runner honours `http_proxy` family only. The plan lists `ACTIONS_RUNNER_...` proxy variables that do not exist (section 2.8).
- One forge for GitHub-flavoured work: the broker flow exists only on GitHub. For Gitea and Forgejo the protocol is Connect-RPC with a different registration model. The plan's P13 runs the upstream runners unchanged (consistent with what exists).
- One credential file location: the JIT blob writes into the runner root, so two runners in one directory collide. One root directory per unit (Inferred).

## 10. Not confirmed (Unverified)

- Whether github.com sends `DistributedTask.AllowRunnerContainerHooks` for every job.
- Behaviour of the runner when the env var is set and the job variable is false (null reference suspected, not run).
- Whether `ACTIONS_RUNNER_INPUT_JITCONFIG` is absent from the Worker and from step environments.
- How the official `linux-arm` asset is built and whether it will carry Node 24.
- Whether GitHub sets `DisableUpdate` in REST-minted JIT blobs, and how the 30-day cut-off is enforced.
- Whether the stock official runner registers against runner.server.
- Whether github.com scale sets accept more than one label.
- Whether a plugin for fleeting can be written in a language other than Go.
- Whether podssh can serve as the SSH endpoint of a podvm unit.
- Whether `dookoid` is maintained, who wrote it, whether its API layer is reusable without proot, and whether it works on a host that denies ptrace.
- Podman with no user namespace at all.
- CircleCI task-agent license and source.
- Buildkite and Azure token models, Gitea registration internals, Drone runner protocol.
- Testcontainers use of named networks and aliases.
- GitHub terms of service for third-party runner implementations (plan Q5): not read.
- The Docker `/build` path in BuildKit mode on a stub server beyond what the CLI sent.

## Coverage

Read in full or in the relevant parts: `actions/runner` (`Runner.cs` JIT and ephemeral parts, `CommandSettings.cs`, `Program.cs`, `Constants.cs`, `ConfigurationStore.cs`, `HostContext.cs` config and shell helpers, `ContainerOperationProvider.cs`, `DockerCommandManager.cs`, `ContainerHookManager.cs`, `HookInput.cs`, `FeatureManager.cs`, `StepHost.cs` hook paths, `BrokerHttpClient.cs`, `RunServiceHttpClient.cs`, `RunnerWebProxy.cs`, `externals.sh`, `run.sh`, `run-helper.sh.template`, release workflows, ADR 1891), `runner-container-hooks` (READMEs, `hooklib` interfaces, docker hook command list), `scaleset` (README, `client.go`, `session_client.go`, `types.go`, `listener/listener.go`, the docker example), ARC (go.mod, chart values), `github-act-runner` (README, `main.go`, protocol, JIT compat, workflow), `runner.server` (README, controller list, targeted greps, vendored copy comparison), Gitea and Forgejo runners (license files, labels, client, container layout, docker call counts), GitLab Runner and fleeting (interface, plugin, custom executor config, docker autoscaler dial), Woodpecker (proto, backends), Buildkite, Jenkins remoting, Azure agent (greps and README level), podman and moby (version constants, docs), docker CLI, Ryuk, kubedock, `dookoid` (README, `build.rs`, built and ran), ingot-engine and contr (README level), the plan, the owner bundle (`VENDORED.md`, `PATCHES.md`, `experiments/README.md`).

Skipped, with reason: the issue #47 body (file absent); the GitHub terms of service (not asked in this assignment; plan Q5 owns it); runner `Runner.Worker` step execution code beyond container paths (not needed for the extension-point question); a live run of the official runner (no token, no network to a test repository; no account use allowed); podman run (no podman installed; I did not install it); official runner under podvm (R1 covers podvm); Rust library inventory beyond crates.io search terms "docker engine api server", "docker-compatible api", "dockerd compatible", "docker api mock" (4 queries, 8 hits each); GitHub repository search (3 to 5 queries). My search was not exhaustive. A project under another name could exist.

Limits of the substitutions: the fake-daemon run shows what the CLI 29.9.3 sends, not what 27.x or older CLIs send, and not what the Go runner sends through its own Go client (GitLab, Gitea). The `dookoid` run happened on a host with ptrace and user namespaces available. All scale-set facts are read from client source, not from a live server.

## Surprises (contradictions with the plan, the owner bundle, or the brief)

1. Plan section 2.1: "`POST {ServerUrlV2}/sessions`". Artefact: `src/Sdk/WebApi/WebApi/BrokerHttpClient.cs:143` `new Uri(Client.BaseAddress, "session")`. The plan's source (a blog post) is wrong or outdated. The plan also omits `acknowledge` and the WebSocket probe.
2. Plan section 2.1: the runner "downloads Node runtimes". Artefact: the release tarballs contain `externals/node20`, `node24` (x64, arm64) and only `node20` (arm). Runner main forces node24 for all JavaScript actions (`HandlerFactory.cs:58-59`).
3. Plan section 2.2: github-act-runner "`actions/cache` cannot save". Artefact: `github-act-runner:README.md:136-138` strikes it through ("Is now working in 0.6.0, including hashfiles").
4. Plan section 5 P6 and section 4 option C reject any alternative to an Engine API socket. Artefact: `FeatureManager.cs:8-12` and the GitHub docs page `customize-containers.md` show an official extension point that removes every docker CLI call from the runner. The plan never mentions `ACTIONS_RUNNER_CONTAINER_HOOKS` or the runner-container-hooks repository (grep of the plan finds neither).
5. Plan section 8 item 4 and P2: credential "in files or the environment of a process that scrubs it". Artefact: `Runner.cs:239-250` writes three credential files into the runner root and keeps them; `CommandSettings.cs:138` scrubs only the env var.
6. Plan list of proxy variables, brief item (1): `ACTIONS_RUNNER_...` proxy variables. Artefact: `RunnerWebProxy.cs:47-64` reads only `http_proxy`/`https_proxy`/`no_proxy` and upper-case forms.
7. Owner bundle `PATCHES.md` entry 1: Results Service "planned, no code yet" and w7 fails because the route is missing. Artefact: `vendor/runner.server/src/Runner.Server/Controllers/ArtifactControllerV2.cs` (189 lines, Twirp `CreateArtifact` at line 48) and `CacheControllerV2.cs` already exist in the vendored tree and at upstream tag v3.14.0. The recorded symptom is 403 and ECONNRESET, so the cause is something else.
8. Plan section 2.2: Forgejo runner "GPL-3.0-or-later". Artefact: the README says so, but the source tree mixes `MIT` and `GPL-3.0-or-later` SPDX tags and 138 of 230 `.go` files have none. The plan's rule of separate process is unaffected, the "do not copy code" boundary is blurrier than the table suggests.
9. The plan's alternative-runner table (2.2) does not list Drone; the brief asks for it. Artefact: `drone-runner-exec/LICENSE.md` is Polyform, not open source.
10. Plan phases and Q6: "`docker build` ... rootless build step". Artefact (observed run): docker CLI 29.9.3 uses BuildKit by default (needs `POST /grpc` and a buildx plugin that the static tarball lacks) and falls back to legacy `POST /build` only with `DOCKER_BUILDKIT=0`, with a deprecation notice.
11. Podman man page says the compat layer is Docker v1.40; code says `CurrentAPI 1.44.0` (`version/version.go`). Not the plan's claim, but a stub author reading the man page gets the wrong number.
12. The runner's docker version check parses the client version from the server string (`DockerCommandManager.cs:79`). Not in the plan. It means a Docker-compatible server must report a server `APIVersion` of at least 1.35; the client version is not really checked.
