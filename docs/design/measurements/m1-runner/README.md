# M1: official actions/runner v2.338.2 (linux-x64), measured offline

Measurement agent m1-runner. Everything was run on one machine on 2026-11-10 (machine `date`).
Labels: Observed (I ran it and read the output, log named), Source (read in the pinned source), Documented, Inferred, Unverified.

## Conditions

| Item | Value |
| --- | --- |
| Machine | kernel 6.12.113+deb13-amd64, 8 CPUs, `systemd-detect-virt` = lxc, uid 1000, `CapEff` = 0 |
| OS | Debian 13 (trixie), glibc 2.41, ICU 76, OpenSSL 3.5.7 |
| fsize limit | soft = hard = 21474836480 bytes (20 GiB). Not raisable here (see Q2) |
| Tools | strace 6.13 (installed with `sudo apt-get install -y strace`, disposable sandbox), util-linux 2.41.5 (prlimit, unshare), binutils 2.44, python 3.13.5, curl 8.14.1 |
| Runner | `actions-runner-linux-x64-2.338.2.tar.gz`, 228071396 bytes, sha256 `baebf204307bac6193480852732f0aba2ae97b6c5597c244f4d5558390147c40` (the release asset digest and the release body agree; downloaded directly from github.com, no fallback needed; the tarball sha256 was verified by `sha256sum -c`) |
| Runner source | GitHub archive of tag v2.338.2, sha256 `3b7549b6cd109cd534de3e904cb257cd8447c1d00dd96d2242b7747b3473ce1f` (recorded, not enforced: GitHub may regenerate archives). `releaseVersion` file reads 2.338.2. Listener reports commit `d1eb12d4b115`, same as R4 |
| Hooks source | `actions/runner-container-hooks` archive of commit `d4731a43a2fe` (R4 pin), sha256 `fc026e8c1fb41b9f0aad53c5adccfba26732b1a35387e90ca60a5338550a734b` |
| No token | none exists, none used. The fake server is a 60 line python server bound to 127.0.0.1 (`scripts/fakeserver.py`). The jitconfig blob is synthetic with a throwaway openssl RSA key |

Every matrix cell that is a run was repeated twice (rounds 1 and 2 in the logs). All runs started the runner with `DOTNET_EnableWriteXorExecute=0` except where a table says otherwise (reason in Q2).

Rerun everything: `scripts/run-all.sh` (needs `WORK` for downloads, default `${TMPDIR:-/tmp}/podrunner-m`; several minutes (03, 04 and 07 dominate), about 250 MB of downloads). Each script states its question and pins, exits 0 (ran), 1 (ran, expectation failed) or 2 (could not run). Logs: `logs/NN-name.log`, last line `exit=N`. `scripts/lib.sh` has the shared helpers; `fakeserver.py`, `mkjit.py`, `sampler.py` are the instruments. Runs use a hard-link copy of the extracted tree (`cp -al`); `pristine_intact` at the end of logs 03, 04, 07 and 08 shows 0 files in the pristine tree were modified.

## Results

| Q | Result | Label | Log |
| --- | --- | --- | --- |
| Q1 | With the default environment the listener, `config.sh --help` and the Worker die at start with exit 153 (SIGXFSZ) on this host. With `DOTNET_EnableWriteXorExecute=0`: `--version` prints `2.338.2`, `config.sh --help` and `run.sh --help` print help, exit 0 | Observed | 01, 02 |
| Q1 | ICU needed unless `DOTNET_SYSTEM_GLOBALIZATION_INVARIANT=1`: with ICU hidden and no invariant mode, exit 134 and the message quoted below. With invariant=1 and ICU hidden: exit 0 | Observed | 01 |
| Q1 | OpenSSL is needed only when the crypto path runs: with libssl and libcrypto hidden, the JIT/OAuth path exits 2 with `OpenSSL is required for algorithm 'RSAOpenSsl' but could not be found or loaded.` and makes no HTTP request. Control (not hidden) reaches `POST /session` | Observed | 08 |
| Q2 | W^X doublemapper does `memfd_create("doublemapper")` then `ftruncate(8, 2199023255552)` = exactly 2 TiB on that memfd. Under a 20 GiB limit this returns EFBIG and the kernel sends SIGXFSZ. The archive claim is verified for dotnet 8 in this runner | Observed | 02 |
| Q2 | Matrix (limit 20 GiB, 1 GiB, 128 MiB, 1 MiB; listener and Worker; 2 runs each): W^X default always 153 (SIGXFSZ). `DOTNET_EnableWriteXorExecute=0` always starts (listener exit 0; Worker exit 1 on its expected bad pipe handle, no signal). Same result at 1 MiB | Observed | 02 |
| Q2 | New: with SIGXFSZ ignored in the parent (`trap '' XFSZ`), the default W^X run does not die even at a 1 MiB limit. strace shows the same `ftruncate(8, 2199023255552) = -1 EFBIG` and the process carries on | Observed | 02 |
| Q2 | Unlimited cannot be tested here: the hard limit is 20 GiB and neither `prlimit` nor `sudo prlimit` can raise it (`Operation not permitted`, `CapEff` 0). Whether a limit of 2 TiB or more lets W^X start is therefore Unverified (inferred yes: the call would succeed) | Observed (refusal) / Unverified | 02 |
| Q3 | A synthetic blob, by argv or by env, makes the listener write the three files, authenticate (`POST /oauth/token`), create a session (`POST /session`), long-poll `GET /message`, and on SIGTERM send `DELETE /session`. No other request | Observed | 03 |
| Q3 | The three files are written with mode 644, owner = the runner uid, in the runner root, including `.credentials_rsaparams` (the private key). They are still there after exit | Observed | 03 |
| Q3 | Argv form: the blob is in `/proc/PID/cmdline` of `run.sh`, `run-helper.sh` and `Runner.Listener` (2 of 2 rounds) | Observed | 03 |
| Q3 | Env form: `ACTIONS_RUNNER_INPUT_JITCONFIG` and the blob are still in `/proc/PID/environ` of `run.sh`, `run-helper.sh` and `Runner.Listener` while it runs (sampled every 20 ms for 7 s, about 330 samples each, 2 of 2 rounds). The runner's own `Environment.SetEnvironmentVariable(..., null)` (`CommandSettings.cs:111-140`) does not change that block | Observed | 03 |
| Q3 | Children: the Listener spawned no child process in the idle phase in the final logs. Whether the Worker and step processes inherit the variable is Unverified (no job could be run). Inferred: the Listener's .NET environment no longer holds it, so `Process.Start` children should not | Unverified | 03 |
| Q3 | Nothing in `_diag` or the other runner files contains the blob prefix, the key prefix or the sentinel strings (leak grep, names only) | Observed | 03 |
| Q3 | Controls: no config at all: `Not configured. Run config.(sh/cmd) to configure the runner.`, `run.sh` exit 0 (it maps listener exit 1 to 0), no request. Garbage base64: listener exit 1 and the .NET base64 error text, no request. The sampler flags are True where expected (argv form, env form) and False in the control runs, so it detects both ways | Observed | 03 |
| Q4 | Exit 7 is driven by `GET /message` answering HTTP 400 with `{"source":"actions-broker-listener","errorKind":"RunnerVersionTooOld",...}`, not by session creation. Listener direct: unset = 1, `=true` = 7, `=1` = 7 (2 of 2 rounds) | Observed | 04 |
| Q4 | `POST /session` answering 403 gives exit 1 (`Failed to create session. Error: Forbidden`), not 7 | Observed | 04 |
| Q4 | `run.sh`: unset = 0, `ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE=true` = 0, `=1` = 7 (2 of 2 rounds). `run-helper.sh` compares the variable to the string `1` | Observed | 04 |
| Q5 | Gate and failure modes: source lines below. No job was run | Source | 05 |
| Q5 | The example hook, driven by me with synthetic stdin for the four commands, exits 0 and its responses pass a mirror of the runner's checks. That tests my hook and my checker, not the runner | Observed (mine) | 05 |
| Q6 | 684215501 bytes extracted (apparent), 698660 KiB on disk, 9394 files, 2156 directories, 6 symlinks. 599417976 bytes are `externals` (four Node builds) and 84786095 bytes are `bin` | Observed | 06 |
| Q6 | 21 ELF files, none static. All 17 in `bin` and the 2 glibc Node builds are dynamic with `/lib64/ld-linux-x86-64.so.2`; the 2 `_alpine` Node builds use `/lib/ld-musl-x86_64.so.1` | Observed | 06 |
| Q6 | Maximum glibc symbol version over all ELF files: `GLIBC_2.28` (Node 20 and Node 24). Over `bin` only: `GLIBC_2.17` (`libclrgc.so`, `createdump`); the listener itself needs `GLIBC_2.16`. Maximum libstdc++: `GLIBCXX_3.4.21` | Observed | 06 |
| Q7 | Pre-staged files work with no argv and no env. See Q7 | Observed | 07 |

### Q1 detail

Failure texts (verbatim):
- Default environment, any of `Runner.Listener --version`, `config.sh --help`: no output, exit 153. bash prints `File size limit exceeded` for the process.
- ICU missing and no invariant mode, exit 134: `Process terminated. Couldn't find a valid ICU package installed on the system. Please install libicu (or icu-libs) using your package manager and try again. Alternatively you can set the configuration flag System.Globalization.Invariant to true if you want to run with no globalization support. Please see https://aka.ms/dotnet-missing-libicu for more information.` followed by a stack from `GlobalizationMode+Settings..cctor` (log 01).
- ICU is found at run time by name (`libicuuc.so`, `libicui18n.so`, versioned suffix probed), OpenSSL by `libssl.so.3` and older names, Kerberos by `libgssapi_krb5.so.2`, LTTng by `liblttng-ust.so.0` (strings of the shim libraries, log 06). `ldd` reports `liblttng-ust.so.0 => not found` for `libcoreclrtraceptprovider.so`; the listener starts and runs anyway on this host (Observed, logs 01, 03).
- Kerberos was not exercised (no Negotiate auth path ran). zlib is a hard NEEDED of `libSystem.IO.Compression.Native.so` (log 06).
- `bin/installdependencies.sh` needs root and was not run. For Debian-family it installs `libkrb5-3 zlib1g`, `liblttng-ust1t64` (or older names), `libssl3t64` (or older) and a `libicuNN` (log 01, lines quoted by grep). The runtime is self-contained: `Runner.Listener.runtimeconfig.json` has `includedFrameworks` Microsoft.NETCore.App 8.0.33, so no system dotnet is needed (Observed, no `dotnet` on this host).
- Not tested: musl (no musl loader here, no musl runner exists upstream, R4), a glibc older than 2.41.

### Q2 detail

Exit codes are as the shell reports them: 153 = 128 + 25 (SIGXFSZ). Table in `logs/02-fsize.log`, with the child's `/proc/self/limits` line printed per limit. `ulimit -f` was not used; `prlimit --fsize=N` sets soft and hard.

Strace, W^X default, 20 GiB limit:
```
memfd_create("doublemapper", MFD_CLOEXEC) = 8
ftruncate(8, 2199023255552)       = -1 EFBIG (File too large)
--- SIGXFSZ {si_signo=SIGXFSZ, si_code=SI_USER, ...} ---
+++ killed by SIGXFSZ +++
```
With W^X off the only ftruncate is `ftruncate(63, 0)` and no memfd is created.

What this means for A-V-07 (`--ulimit fsize`):
- Any fsize limit below 2 TiB on the process that starts the .NET runner kills it at start, in default W^X mode. Observed down to 1 MiB.
- Two ways to keep both, both Observed to start the runner: `DOTNET_EnableWriteXorExecute=0`, or leave SIGXFSZ ignored in the runner's parent (ignored dispositions survive exec; steps then also see EFBIG instead of being killed, which is what the acceptance text asks for; Inferred, no step was run).
- Not measured: the performance cost of W^X off; whether the runtime with SIGXFSZ ignored runs W^X in a degraded mode; whether a 1 MiB limit lets a real job write its `_diag` logs and `_work` files (the 1 MiB runs were `--version` and the Worker's failed start, not a session).
- `vm.memfd_noexec` here is 0 (not tested at other values).

### Q3 detail: files and requests

Blob shape (Source `Runner.cs:234-256`): base64(UTF-8 JSON object `{ ".runner": base64(...), ".credentials": base64(...), ".credentials_rsaparams": base64(...) }`). The consumer writes each key to `<root>/<key>` with `File.WriteAllBytes` and no chmod (Source `Runner.cs:249-250`; mode 644 under umask 022 is what I observed).

Synthetic content (`scripts/mkjit.py`):
- `.runner` = JSON of `RunnerSettings` (`ConfigurationStore.cs:15-50`): AgentId, AgentName, PoolId, PoolName, DisableUpdate=true, Ephemeral=true, ServerUrl = ServerUrlV2 = `http://127.0.0.1:PORT/v2`, GitHubUrl, WorkFolder, UseV2Flow=true. ServerUrl equals ServerUrlV2 on purpose: `BrokerMessageListener.cs:126-132` opens a second connection to ServerUrl only when they differ. When they differed, I saw a `GET /v1/_apis/connectionData` loop answered 404 and `Runner connect error: Not Found. Retrying until reconnected.` (an earlier unlogged trial).
- `.credentials` = `{"Scheme":"OAuth","Data":{"clientId":...,"authorizationUrl":...,"oauthEndpointUrl":...}}` (Source `OAuthCredential.cs:28-45`, `Constants.Configuration.OAuth`).
- `.credentials_rsaparams` = JSON with keys `d dp dq exponent inverseQ modulus p q`, each a base64 byte array (Source `IRSAKeyManager.cs:44-100`, `RSAFileKeyManager.cs`). The key is a fresh 2048-bit openssl key converted by `mkjit.py`. The runner accepted it and signed a JWT with it (the token request carries `client_assertion` of 616 characters), so the layout is right. Real GitHub blobs may carry more fields (for example `authorizationUrlV2`, `EnableAuthMigrationByDefault`); not seen, since no real blob exists here.

Requests seen by the fake server in a normal run (headers with secrets redacted by the server; log 03 lists them):
1. `POST /oauth/token`, `Content-Type: application/x-www-form-urlencoded`, body `grant_type=client_credentials&client_assertion_type=urn:ietf:params:oauth:client-assertion-type:jwt-bearer&client_assertion=<redacted>`; no Authorization header. Answered 200 with a dummy token.
2. `POST /session` (singular, as R4 section 2.9 says), `Authorization: Bearer <redacted>`, `X-Actions-Session`, JSON body with `ownerName` = `<hostname> (PID: n)` and the agent (id, name, version, osDescription), `brokerSession: true`. The User-Agent carries `GitHubActionsRunner-linux-x64/2.338.2`, `CommitSHA/d1eb12d4b115...`, `ClientId/<clientId>`, `RunnerId`, `GroupId`.
3. `GET /message?sessionId&status=Online&runnerVersion=2.338.2&os=Linux&architecture=X64&disableUpdate=true`, repeated (my server answers 202 after 3 s).
4. On SIGTERM: `DELETE /session?sessionId=...`.
There was no `connectionData` request to the V2 URL, no `acknowledge` and no WebSocket probe in the idle phase.

SIGTERM to the session makes `run.sh` (bash) die at once (exit 143) and the listener finish with `DELETE /session` (seen in every run). SIGINT is ignored by background jobs of a non-interactive shell, so the scripts use SIGTERM.

What a podrunner must protect (G-03), all Observed unless marked:
1. Argv form puts the blob in the cmdline of three processes (`run.sh`, `run-helper.sh`, listener), readable by any same-uid process and, with default `hidepid=0`, by every user.
2. Env form leaves the variable and the blob in `/proc/PID/environ` of the same three processes for their whole life. Unsetting in managed code does not help.
3. Both forms write the private key and tokens to the runner root with mode 644, and they stay after exit. Ephemeral does not delete them.
4. `run.sh` re-reads `.runner` and the credentials during the whole life of the process (Source, R4 section 2.1); "read once" delivery is not enough. Q7 shows staging the files works.

### Q4 detail

Source: `BrokerHttpClient.cs:120-124` maps the broker error kind `RunnerVersionTooOld` to `AccessDeniedException { ErrorCode = 1 }`, but only in `GetRunnerMessageAsync` (the message poll). `CreateSessionAsync` (`BrokerHttpClient.cs:139-164`) throws `AccessDeniedException` with the default error code on 403, which `Program.cs:142` does not match. `BrokerMessageListener.cs:334-337` rethrows the `ErrorCode == 1` case out of the message loop to `Program.cs:142-148`, which returns `GetRunnerVersionDeprecatedExitCode()`: 7 if `ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE` converts to true, else 1.

Driven: yes, completely (fake server mode `tooold`). Response the runner accepts:
```
HTTP 400  {"source":"actions-broker-listener","errorKind":"RunnerVersionTooOld","statusCode":400,"errorMessage":"..."}
```
The `source` value must be exactly `actions-broker-listener` (`BrokerHttpClient.cs:292`). The real status code GitHub uses is Unverified; the runner keys on the body, not the status.

Exit codes (log 04, 2 rounds each, same values):

| Started as | variable unset | `=true` | `=1` |
| --- | --- | --- | --- |
| `bin/Runner.Listener run` | 1 | 7 | 7 |
| `run.sh` | 0 | 0 | 7 |

`run-helper.sh.template` and `run.sh` compare with `"$ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE" == "1"`. With `true`, the listener returns 7, the helper prints `Exiting with unknown error code: 7` and `run.sh` exits 0. A launcher that wants the signal through `run.sh` must set `=1`, or call `bin/Runner.Listener run` itself (then either value works). A 403 on `POST /session` gives 1.

### Q5 detail: container hooks gate

Source (pinned tag, lines printed in `logs/05-hooks.log`):
- Hooks are on in a job only if the job variable `DistributedTask.AllowRunnerContainerHooks` is true (`Constants.cs:170`) and the env var `ACTIONS_RUNNER_CONTAINER_HOOKS` is non-empty (`Constants.cs:280`): `FeatureManager.cs:8-12`.
- The env var is read in the Worker, not the Listener (grep in log 05: only Worker files and `Constants.cs`). The Worker inherits the Listener's environment.
- `ContainerOperationProvider.cs:36-43` picks the hook manager if the env var is set, else the docker manager. `ContainerOperationProvider.cs:64-71` takes the hook path only if both are true.
- Half-set case, Inferred from those lines, not run: env var set, job variable false: `_dockerManager` is null (`:36-43`), flow falls to `:72-74` where `_dockerManager.DockerPS` is called, a null reference. The reverse (variable true, env var unset) takes the docker path.
- `ContainerHookManager.cs:33-36` reads the path; `:185-197` requires the file to exist and to end in `.js`, `.sh` or `.ps1`, else throws `File not found at ...` or `Invalid file extension at ...`; any hook error is wrapped as `Executing the custom container implementation failed. Please contact your self hosted runner administrator.` (`:177`).
- The hook runs through the script handler with the JSON on its stdin (`ContainerHookManager.cs:149`, `ScriptHandler.cs:331`), after the runner has created an empty response file (`IOUtil.CreateEmptyFile`, `:166`). A non-zero exit fails the step.

What blocks a real run offline: a job needs the broker to deliver a `RunnerJobRequest` message on `GET /message` and answer `acknowledge`, then `POST acquirejob` on a run-service URL with a full `AgentJobRequestMessage` (steps, template tokens for `container:`/`services:`, variables including `DistributedTask.AllowRunnerContainerHooks`, a `SystemVssConnection` endpoint). Then the Worker connects to that endpoint (`JobRunner.cs:63-80`, `:95-105`) for timeline, logs (Twirp and blob upload) and `completejob`. That is the whole broker plus a results service, which the brief excludes. Also there is no `dotnet` SDK here, so the Worker's hook manager cannot be called in isolation. Unverified by a run: the null-reference case, the real stdin JSON, and whether github.com sends the job variable.

### The hook interface a podrunner hook must implement

The runner side is authoritative (C# types `HookInput.cs`, `HookResponse.cs`); the TypeScript `hooklib` at the pinned commit disagrees with it in two places (see Surprises).

Invocation: the file named by `ACTIONS_RUNNER_CONTAINER_HOOKS` (`.sh`, `.js` or `.ps1`) is run once per command by the Worker. `.sh` runs with bash if found, else sh (R4, Source). A hook must handle all four commands (ADR `docs/adrs/1891-container-hooks.md`: no partial opt in).

Stdin: ONE line of JSON, then EOF. `hooklib` reads the last line only (`utils.ts getInputFromStdin`: `rl.on('line', line => { input = line })`).
```
{ "command": "prepare_job" | "cleanup_job" | "run_script_step" | "run_container_step",
  "responseFile": "<path>",      // already exists, empty; append the response JSON to it
  "args": { ... },               // per command, below
  "state": { ... } }             // the state from the last response that had one; absent before prepare_job
```
Exit code: 0 = success. Anything else fails the step with the wrapper message above. stdout and stderr go through the script output manager into the job log (Inferred from `ScriptHandler.cs:331-345`).

`args` per command (runner `HookInput.cs`; `hooklib/src/interfaces.ts` agrees on the field names):
- `prepare_job`: `{ container?: HookContainer, services?: HookContainer[] }`. `container` is present when the job has `container:`.
- `run_container_step`: a `HookContainer` plus `dockerfile?` and `prependPath?`. `HookContainer` = `{ image, dockerfile?, entryPoint, entryPointArgs[], workingDirectory, createOptions, registry?{username,password,serverUrl}, environmentVariables{}, portMappings[], systemMountVolumes[], userMountVolumes[] }`, mounts = `{ sourceVolumePath, targetVolumePath, readOnly }`.
- `run_script_step`: `{ entryPoint, entryPointArgs[], environmentVariables{}, prependPath[], workingDirectory }`.
- `cleanup_job`: `{}`.
- `registry.password` is in the stdin JSON in clear (Source `HookInput.cs`); a hook must not log stdin unredacted. My example hook logs it for the measurement only.

Response file contents (JSON, appended once):
- `state` (object, optional, any command): replaces the stored state; returned in `state` of later commands (`ContainerHookManager.cs SaveHookState`; absent = keep the old one).
- `prepare_job` only:
  ```
  { "state": {...},
    "context": { "container": { "id": "...", "network": "...", "ports": {"<src>":"<dst>"} },
                 "services": [ { "id": "...", "network": "...", "ports": {...} }, ... ] },
    "isAlpine": false }
  ```
  `isAlpine` is required (boolean) when `args.container` exists; missing gives `The property 'isAlpine' is required but was not found in the response file.` (`HookResponse.cs:17-24`). `context.services` is a JSON array, in the same order as `args.services` (`ContainerHookManager.cs` `UpdateJobContext`, loop over `response.Context.Services[i]` and `serviceContainers[i]`). `context.container.id` and `.network` set the `job.container.id` and `.network` contexts.
- `run_script_step`, `run_container_step`, `cleanup_job`: no response needed. If the response file is missing the runner continues; if it is empty it is read as null (`GetResponse`).
- A response file is read, then deleted by the runner.

Gaps in my reading: how stdout of a `run_script_step` hook becomes the step's log lines (it goes through the normal script output manager, Source); exit code semantics for `run_container_step` beyond non-zero = failure.

### Q6 detail: what a no-root unit image must contain

From log 06 (Observed):
- Runner tree: 684215501 bytes, 9394 files. `externals/` 599 MB: `node20` 160074931, `node20_alpine` 104851671, `node24` 201757541, `node24_alpine` 132733833. `bin/` 84786095 bytes with 213 dlls and 15 localisation directories. The two `_alpine` Node builds are only for jobs in Alpine containers (R4); a host-mode image that never runs them could omit about 238 MB (Inferred; not tried).
- ELF files: 21, none static. Interpreter for the 17 in `bin` and the 2 glibc Node builds: `/lib64/ld-linux-x86-64.so.2`. So the unit image needs a glibc dynamic loader at that exact path. musl is not enough (the loader and `libc.so.6` are required by name).
- Shared libraries by soname (NEEDED): `libc.so.6 libm.so.6 libdl.so.2 libpthread.so.0 librt.so.1 libgcc_s.so.1 libstdc++.so.6 libz.so.1`, and `ld-linux-x86-64.so.2`. Loaded at run time by name: ICU (or invariant mode), `libssl.so.3` and `libcrypto` (OpenSSL), `libgssapi_krb5.so.2` (not exercised), `liblttng-ust.so.0` (optional, run works without it).
- Minimum glibc by symbol versions: overall `GLIBC_2.28` (the Node builds); `bin` needs up to `GLIBC_2.17`. This is the highest versioned symbol the dynamic table names; it is not a tested floor (I ran only on glibc 2.41). libstdc++ up to `GLIBCXX_3.4.21`.
- Tools the runner shells out to that I saw used: `id` (from `run.sh`), `cp`, `bash`, `sleep` (`run-helper.sh`, `safe_sleep.sh`), `chmod` (only when it creates its own RSA key during `config.sh`, not in the JIT path). `run.sh` needs bash. Whether jobs need git, tar or others: Unverified (no job run).

### Q7: pre-staged credential files (requested by kage)

Method: decode the synthetic blob (`decode_blob` in `scripts/lib.sh`) into the three files in a fresh copy of the root, mode 600 (or 400 in a variant), then start `./run.sh` or `bin/Runner.Listener run` with no arguments and no `ACTIONS_RUNNER_INPUT_*` variable. 2 rounds each of three forms, log 07.

Observed:
- It starts and reaches the fake server with the same request sequence as the blob runs: `POST /oauth/token`, `POST /session`, `GET /message` repeated, `DELETE /session` on SIGTERM. (`run.sh` and direct listener, modes 600 and 400.)
- The three files are not rewritten and not deleted: sha256 prefixes before and after are equal, and modes stay 600 (or 400). So the runner needs only read access to them; a read-only credential mount would do (Inferred for a read-only filesystem: not tried, only mode 400 on a writable directory).
- `/proc/PID/cmdline` and `/proc/PID/environ` of `run.sh`, `run-helper.sh` and `Runner.Listener` hold none of: the blob prefix, the `ACTIONS_RUNNER_INPUT_JITCONFIG` name, the RSA key prefix, the sentinel client id and agent name (about 330 samples per process per run; the sampler is positive in the blob runs, log 03). The check covers those markers only; it is not a proof that nothing credential-like exists in the environment (the environment of these runs held what my shell held).
- Conclusion: podrunner can avoid argv and env entirely by writing `.runner`, `.credentials`, `.credentials_rsaparams` (mode 600, owner = runner uid) into the runner root before `run.sh`. The files must persist for the runner's life and should be removed after it ends (they are not removed by the runner, Q3).
- Not shown: that the files survive a runner token refresh over hours, and how a real GitHub blob differs from my synthetic one.

## Surprises (contradictions with R4 or the plan)

1. R4 section 2.3 and the brief put exit 7 at session creation. Observed and Source: it comes from the message poll (`GET /message`, error kind `RunnerVersionTooOld`). A 403 on `POST /session` gives exit 1.
2. R4 section 2.3 says `run.sh` honours `ACTIONS_RUNNER_RETURN_VERSION_DEPRECATED_EXIT_CODE`. It does, but only for the exact string `1`. With `true` (the value the listener accepts, and the one in the task) `run.sh` exits 0 and hides the signal.
3. R4 section 2.1 says the runner removes `ACTIONS_RUNNER_INPUT_JITCONFIG` from the listener's environment. In managed code it does; the kernel's `/proc/PID/environ` of the listener, `run.sh` and `run-helper.sh` still shows it. The unset protects nothing against a same-uid reader.
4. R4 says the credential files are in the root for the life of the process. They also outlive it, mode 644, including the private key.
5. On this host the default environment cannot start the runner at all (even `config.sh --help`), because of the 20 GiB hard fsize cap and the 2 TiB W^X memfd. The archive claim is verified. The cap cannot be lifted here, so "unlimited" was not run.
6. SIGXFSZ ignored in the parent is a third way to start the runner under a small fsize limit. R5 section 8.1 lists two (apply fsize to step processes only, or W^X off).
7. The `hooklib` TypeScript response type at the pinned commit (`interfaces.ts:77-82`: `services` map, `alpine`) does not match what the runner reads (`isAlpine`, `context.services` array) nor what the official docker hook writes (`prepare-job.ts:141-178`: `isAlpine`, `context.services` array, `state.services` array). A podrunner hook written to the hooklib type fails with the `isAlpine` error. R4 section 2.6 also says `alpine`; the runner says `isAlpine`.
8. A JIT blob whose `ServerUrl` differs from `ServerUrlV2` makes the listener also open a V1 connection (`GET {ServerUrl}/_apis/connectionData`). A fake broker must serve that or set both URLs equal.
9. `POST /session` sends the hostname in `ownerName`; the User-Agent carries the client id and runner id. A podrunner that proxies or logs runner traffic will see them.

## What this does not establish

- No job ran: Worker behaviour with a job, env inheritance by the Worker and steps, hook invocation in a real run, `acknowledge` and WebSocket probe behaviour, log upload, `completejob`, ephemeral exit after a job, and issue 4309 are all unmeasured.
- No real GitHub: the real blob content, auth migration fields, token refresh, the status code GitHub returns for a too-old runner, and whether `DisableUpdate` is set in real blobs.
- One host (Debian 13, glibc 2.41, ICU 76, OpenSSL 3.5.7, LXC, 8 CPUs). No older glibc, no musl, no arm64, no other kernel. fsize above 20 GiB untestable.
- Timing was not part of the questions; no timing numbers are claimed.
- Hook example validated against my own mirror of the runner's checks, not by the runner.
- The `unshare -rm` ICU and OpenSSL hiding replaces libraries by empty files in a private mount namespace; a host that truly lacks them could differ in the message text, not in the failure.
