# The hook contract, version 1

The official runner runs the containers of a job through its container
hooks when `ACTIONS_RUNNER_CONTAINER_HOOKS` names a script (call C2:
the container route of v1). podrunner gives that script; the script relays
each hook call to the control socket of the unit (`ctl`, GitHub #35), and
the unit driver does the container operations of
`docs/contracts/unit-driver.md`.

The runner takes the hooks only when the job message also has the
variable `DistributedTask.AllowRunnerContainerHooks` set to `true`
(`FeatureManager.IsContainerHooksEnabled`); else it runs the job
container with docker. The fake broker of level 2 sends it.

The fields below are the interface of the runner's `HookInput.cs` and
`HookResponse.cs` of `actions/runner` v2.338.2, in camel case. The
runner writes `null` for an unset field. `crates/podrunner-hook/src/input.rs`
reads them as a closed set: an unknown field is refused, and its tests
pin the shape.

## The call

The runner starts the script once for each call, with one JSON object on
stdin, compact, first `command`, then `responseFile`, `args` and `state`:

| Field | Meaning |
| --- | --- |
| `command` | `prepare_job`, `cleanup_job`, `run_script_step` or `run_container_step` |
| `responseFile` | The path of the file for the response (only `prepare_job` writes one) |
| `args` | The arguments of the command (below) |
| `state` | The `state` of the last response, given back |

The exit code of the script is the result of the call: 0 is success.

## Commands

| Command | Arguments | Driver operations |
| --- | --- | --- |
| `prepare_job` | `container` (`image`, `workingDirectory`, `createOptions`, `environmentVariables`, `userMountVolumes`, `systemMountVolumes`, `registry`, `portMappings`), `services` (a list of the same) | `container_start` for the job container and each service |
| `cleanup_job` | none | `container_stop` for each container of the job |
| `run_script_step` | `entryPoint`, `entryPointArgs`, `environmentVariables`, `prependPath`, `workingDirectory` | `container_exec` in the job container |
| `run_container_step` | `image` or `dockerfile`, `entryPoint`, `entryPointArgs`, `workingDirectory`, `createOptions`, `environmentVariables`, `prependPath`, `userMountVolumes`, `systemMountVolumes`, `registry` | `container_start`, `container_logs`, `container_inspect`, `container_stop` |

The runner sends the job container of `prepare_job` with no entry point.
The host then starts it with `tail -f /dev/null`: the container only has
to live while the steps run in it.

## The response of `prepare_job`

```json
{
  "state": {},
  "context": {
    "container": { "id": "ID", "network": "NAME", "ports": {} },
    "services": [ { "id": "ID", "ports": {} } ]
  },
  "isAlpine": false
}
```

## The relay and the control socket

The relay script (`crates/podrunner-hook/relay/podrunner-hook.sh`) posts
stdin to `POST /v1/hook/COMMAND` on the control socket of its unit
(`PODRUNNER_CTL`, a unix socket at mode 0600 in the unit). The answer of
`prepare_job` goes to the response file; the output of a step goes to
stdout. The header `X-Podrunner-Exit` gives the exit code of the script. A
refusal is HTTP 403 with `{"code": ..., "why": ...}`, and the script
exits 4. A job hook of the runner posts `POST /v1/job_state` with
`{"event": "job-started"}` or `{"event": "job-completed"}`. One request
for each connection; a body of at most 1 MiB; no chunked body.

## The CA bundle of the broker

When the broker of a unit ends TLS (a credential or a rule with a path,
T-563, T-524), each container of the job (the job container, each
service and each container action) trusts the CA bundle of that broker,
as the unit does (T-578):

- The host writes a copy of the bundle at mode 0644 to `LOGS/ca`, under
  the name of its credential file (`podrunner-broker-ca.pem`). LOGS is the
  directory of the output of the steps (`SOCKETS.logs`, beside the socket
  directory SOCKETS of the unit). The unit does not see LOGS. The
  directory `LOGS/ca` has mode 0755 and holds the copy only.
- Each container mounts `LOGS/ca` read-only at `/__podrunner`, by its
  host path. On the namespace rung of podvm this path is out of
  `--podvm-volume-base` (the runner directory of the unit): podvm opens a
  source under the base with no symbolic link, and binds a source out of
  it by its path (podvm `0.2.0-beta.37`, measured as a user with no root).
- Each of the ten variables of `pod_broker::ca::env` names
  `/__podrunner/podrunner-broker-ca.pem`. A variable that the job sets
  for the container keeps the value of the job.
- A unit with no bundle gives its containers no mount and no variable.
- A socket that does not bind leaves no copy. The close of the socket
  removes the copy.

`podrunner run` does the same for a unit that takes the CA files of the
caller (a unit with the network of the host, T-596). The bundle is each
CA file of the ten variables once, in the order of the variables (a link
and its target are one file); a file with no certificate stays out, and
one line on stderr names it. The copy is `LOGS/ca/podrunner-ca.pem`, and
each of the ten variables names `/__podrunner/podrunner-ca.pem`. A caller
with no CA file gives the containers no mount and no variable.

## Refusals

The host checks each call (`crates/podrunner-hook/src/policy.rs`): a step
of the job can skip the hook script, so the script checks nothing.

- A hook call of a unit whose kind has `container: none` is refused:
  `container-not-supported`.
- `cleanup_job` and `run_script_step` before `prepare_job` are refused: `container-not-supported`. `run_container_step` (a Docker action) needs no `prepare_job`: in a job with no `container:` and no `services:` the runner sends none, and GitHub runs the action (the owner, Q4). It has the checks of the action mounts.
  A second `prepare_job` is `container-option-refused`.
- A `dockerfile` (a container action that builds its image) is refused
  by name: `build-not-supported` (call C2).
- An `image` that is not `NAME@sha256:HEX` is refused:
  `payload-not-pinned`. An image of a registry host that the kind does
  not list in its registries, or a registry credential for another host,
  is `container-option-refused`.
- Mounts: a job container takes only the targets `/__w`, `/__w/_temp`,
  `/__w/_actions`, `/__w/_tool`, `/__e` (read-only), `/github/home` and
  `/github/workflow`; a container action takes only
  `/github/runner_temp`, `/github/home`, `/github/workflow`,
  `/github/file_commands` and `/github/workspace`. Each source is under
  the runner directory of the unit, with no `..` and no socket. The pair
  `/var/run/docker.sock` to `/var/run/docker.sock` is accepted and
  dropped. A volume of the workflow (`userMountVolumes`) is a host path,
  and is refused.
- `createOptions`: only `--user`, `--cpus`, `--memory`, `--shm-size`,
  `--hostname`, `--workdir` and `--init`. Each other option (a device, a
  privilege, a network mode, a host path) is
  `container-option-refused`.
- On a unit of the host network: `services:` is
  `container-not-supported`, and ports are `container-option-refused`.
- Each refusal writes one audit line `hook-refuse` with the code, and no
  path, argument or credential.
