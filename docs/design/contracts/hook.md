# Container hook contract, version 1

The runner-side interface that the `podrunner` hook implements, and what it calls on the host. Labels are those of the SPEC: Observed, Source, Documented, Inferred, Unverified. Sources: M1 Q5 and `measurements/m1-runner/README.md` (the runner's own types `HookInput.cs` and `HookResponse.cs` at the pinned tag, read, not run against a job).

## When the runner calls it

The runner calls the hook only if both are true: the job variable `DistributedTask.AllowRunnerContainerHooks` is true and the environment variable `ACTIONS_RUNNER_CONTAINER_HOOKS` is non-empty (Source: `FeatureManager.cs:8-12`). The environment variable is read in the Worker, which inherits the listener's environment. The file named by the variable must exist and end in `.js`, `.sh` or `.ps1`. Any hook error fails the step with "Executing the custom container implementation failed. Please contact your self hosted runner administrator." A hook must handle all four commands: there is no partial opt in (Documented, ADR 1891).

The half-set case (variable set, job variable false) takes the docker path with a null manager (Inferred from the source, not run). `podrunner` sets the environment variable for every unit whose capability says `container: hook`, and a live job settles the case (gate G-H1, SPEC §3.2).

## Invocation

The hook is a `.sh` script in the unit image. The image ships `bash` and `curl`. It is not a static binary: podvm refuses a static payload on a chroot-denied host (Observed, `measurements/m2-podvm/logs/02-images.log`). The runner runs it once for each command with one line of JSON on stdin, then EOF:

```
{"command":"prepare_job","responseFile":"<path>","args":{ ... },"state":{ ... }}
```

`responseFile` exists and is empty. The hook appends the response JSON to it. `state` is the state of the last response that had one and is absent before `prepare_job`. Exit 0 is success. Any other exit fails the step.

stdin holds `registry.password` in clear. The hook never logs stdin and never puts a password in a log, a report or `status`. Which registry credential the server accepts is the server's rule (below).

## Commands and arguments

- `prepare_job`: `{container?, services?[]}`. `container` is present when the job has `container:`.
- `run_container_step`: a container object, in which `image` is null when `dockerfile` is set (`prependPath` belongs to `run_script_step` only; Source, `HookInput.cs`, actions/runner at `310a370bf360`). With `dockerfile` set there is no image for the image rule to match, and the request is `build-not-supported`.
- `run_script_step`: `{entryPoint, entryPointArgs[], environmentVariables{}, prependPath[], workingDirectory}`.
- `cleanup_job`: `{}`.

A container object is `{image, dockerfile?, entryPoint, entryPointArgs[], workingDirectory, createOptions, registry?{username,password,serverUrl}, environmentVariables{}, portMappings[], systemMountVolumes[], userMountVolumes[]}`. A mount is `{sourceVolumePath, targetVolumePath, readOnly}`. `portMappings` is a list of strings. `registry.serverUrl` is the registry host that the runner parses out of the image name (Source, `ContainerInfo.cs`), so the rule that its host equals the image's registry host is vacuous for the runner's own request and bites only on a forged one.

## Responses

For `prepare_job` the response is:

```
{"state":{ ... },
 "context":{"container":{"id":"...","network":"...","ports":{"<src>":"<dst>"}},
            "services":[{"id":"...","network":"...","ports":{ ... }}]},
 "isAlpine":false}
```

- `isAlpine` is required when `args.container` exists. Without it the runner fails with "The property 'isAlpine' is required but was not found in the response file." The field is `isAlpine`, not `alpine`.
- `context.services` is a JSON array in the order of `args.services`. It is not a map.
- This differs from the TypeScript `hooklib` type at the pinned commit (a `services` map and `alpine`) and from what the official docker hook writes (`isAlpine`, an array). The runner's types decide.
- `run_script_step`, `run_container_step` and `cleanup_job` need no response. An empty response file is read as null. The runner reads the response file and then deletes it.

## What the hook does with the arguments

The script is a relay. It sends its stdin unchanged as the body of `POST /v1/hook/COMMAND` to the unit's control socket and appends the response body to `responseFile`. It parses nothing and enforces nothing. The host-side server of the socket (`unit-driver.md`, "The control socket of one unit") applies the closed field set to every request, including one that a step posts directly. The closed set is: image, entrypoint, entrypoint arguments, workdir, env, `ports` (refused on a `net: shared-host` unit, loopback `HostIp` only on `net: private`), and mounts from a fixed table that depends on the command, because the runner builds two different sets (Source, actions/runner at `310a370bf360`):

- Job container mounts, for the `container` of `prepare_job` only: `/__w`, `/__w/_temp`, `/__w/_actions`, `/__w/_tool`, `/__e` (read-only), `/github/home`, `/github/workflow`. Seven entries that `MountWellKnownDirectories` of `ContainerOperationProvider.cs` adds.
- Container action mounts, for every `run_container_step`: `/github/runner_temp`, `/github/home`, `/github/workflow`, `/github/file_commands`, `/github/workspace`. Five entries that `ContainerActionHandler.cs` adds.

The tool cache target of the job container is `/__w/_tool` because `HostContext.cs` puts the tool directory under the work directory when no tool cache variable is set, and `ContainerInfo.cs` maps the work directory first (Source, same pin). It is the target `/__t` only when a tool cache variable points outside the work directory. `start.env` sets none (SPEC §10.3), so `/__t` is not in the table and a request that carries it is refused. A `services:` container gets none of the seven, so any mount on it is a user mount and is refused. The server picks the table by the command. A target of the other table is refused, so `/__w` in a `run_container_step` and `/github/workspace` in a `prepare_job` are refused. The image must be of a registry host that the unit's `registries` list allows. The server accepts `prepare_job` once for each unit and before any step command, and refuses every container operation before it. The `prepare_job` request is the first declaration the server sees, and a step can forge it, so the image rule bounds a job to the allowed registries and the egress policy, not to the workflow file (`unit-driver.md`). Anything else is refused with `container-option-refused` and the field name:

- any `createOptions` token outside an allow list;
- any mount whose target is not in the table of the command, or whose source is not that table's directory;
- `--privileged`, a device, `--pid`, `--network host`, a bind of a socket other than the one below;
- any registry credential whose `serverUrl` host is not the registry host of the image it came with.

The runner adds the system mount `/var/run/docker.sock` to `systemMountVolumes` of every job container (Source, `ContainerInfo.cs`, `IsJobContainer`) and of every container action (Source, `ContainerActionHandler.cs`). The server accepts exactly that entry, matched when source and target are both that literal string, and drops it without mounting anything: no Docker API exists on the hook route. Every other socket bind is refused.

A `Dockerfile` action is refused with `build-not-supported`. A container action reaches the server only in a job that has `container:` or `services:`, because the runner sends no `prepare_job` otherwise (SPEC §7.2); in any other job the order rule refuses it. That a Docker action runs on the rung is Unverified (U19). A network mode other than the default is `container-option-refused`. A `services:` entry on a unit whose `net` is `shared-host` is `container-not-supported` (SPEC §7.3). On a driver that reports `container: hook` over podvm, `--health-cmd` and related options are `container-not-supported`, because podvm `inspect` has no health state, and "healthy" means that the container runs and a TCP connect to each declared port succeeds. A refusal fails the step with a named reason in the job log.

## Calls to the host

The hook calls the control socket of its unit (`unit-driver.md`) with `curl --unix-socket`. The mapping:

| Hook command | Calls |
| --- | --- |
| `prepare_job` | `container_start` for the job container and for each service, then `container_inspect` until running, within a bounded wait |
| `run_script_step` | `container_exec` with the arguments `environmentVariables` and `workingDirectory`, output streamed to the hook's stdout and stderr |
| `run_container_step` | `container_start` and `container_logs` with `follow`, then `container_inspect` for the exit code. The image follows the registry rule, the mounts are checked against the container action table, and the step is refused before the first `prepare_job` |
| `cleanup_job` | `container_stop` for each container the job started |

Refusals happen in `prepare_job`, which may block for the bounded wait. Job-level images (`services:`, `container:`) may be tags: the driver resolves them to a digest at pull and `slots` records the digest in the report. Only the payload image must be a digest.

## Two more hook points

`ACTIONS_RUNNER_HOOK_JOB_STARTED` and `ACTIONS_RUNNER_HOOK_JOB_COMPLETED` need no flags (Documented, ADR 1751). `start` sets both to relay scripts of the image that post `{"event":"job-started"}` and `{"event":"job-completed"}` to `POST /v1/job_state` on the control socket. The first moves the job from `started` to `running` and is the measurement point of `claim_to_first_step_s`. The second records the time and changes no state. For a payload that is not the official runner, `running` is set when `start` returns.

## Example

`measurements/m1-runner/hook-example/hook-log.sh` answers the four commands with synthetic input. M1 drove it with a mirror of the runner's checks. It was not driven by the runner (Observed, mine).
