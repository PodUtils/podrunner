# The hook contract, version 1

The official runner runs the containers of a job through its container
hooks when `ACTIONS_RUNNER_CONTAINER_HOOKS` names a script (call C2:
the container route of v1). podrunner gives that script; the script relays
each hook call to the control socket of the unit (`ctl`, GitHub #35), and
the unit driver does the container operations of
`docs/contracts/unit-driver.md`.

The fields below are the interface of the runner's `HookInput.cs` and
`HookResponse.cs`. T-035 checks them against the source of the pinned
runner version, and a test of T-035 pins them.

## The call

The runner starts the script once for each call, with one JSON object on
stdin:

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

## Refusals

- A `dockerfile` (a container action that builds its image) is refused
  by name: `build-not-supported` (call C2).
- An `image` that is not pinned to a digest by the pin list is refused:
  `payload-not-pinned`.
- `createOptions` that the unit kind cannot apply are refused:
  `container-option-refused`.
- A hook call of a unit whose kind has `container: none` is refused:
  `container-not-supported`.
- Each refusal writes one audit line `hook-refuse` with the code, and no
  path, argument or credential.
