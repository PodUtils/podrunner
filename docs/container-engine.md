# A container engine inside the unit (T-103)

githome's G-09 keeps three clients that the official container hooks do
not reach: `services:` containers, `docker build` (and Dockerfile
actions), and Docker-in-Docker (the owner, 2026-11-13). DR-8 brings an
engine back for such an ask. This page has the census of the verbs
(DR-17) and the design.

## The census of the verbs

The official runner v2.338.2 runs the `docker` command line for each
container operation (`src/Runner.Worker/Container/DockerCommandManager.cs`
of the tag; it never speaks the Engine API itself). The verbs of each
client:

| Client | Verbs |
| --- | --- |
| `services:` (`ContainerOperationProvider.cs`) | `version`, `login`, `pull`, `network create`, `create`, `start`, `inspect` (health and ports), `port`, `logs`, `ps`, `rm`, `network rm`, `network prune` |
| A Dockerfile action (`ContainerActionHandler.cs`) | `build`, `run` |
| A `container:` job with no hooks | `create`, `start`, `exec`, `rm` and the verbs of `services:` |
| Docker-in-Docker (a step that runs `docker` itself) | each verb of the Docker CLI: the job, not the runner, chooses them |

The runner needs 15 verbs of the command line. Docker-in-Docker needs the
whole Engine API, so a subset that a shim answers (`pod-dockerapi`) does
not serve G-09.

## The design

* A real engine (Docker or Podman, static where the image allows it)
  runs **inside the unit**, with its socket inside the unit only. The
  runner of the job gets `DOCKER_HOST` of that socket and no
  `ACTIONS_RUNNER_CONTAINER_HOOKS`, so it runs each container verb as on a
  GitHub-hosted runner, and a step can run `docker` itself.
* The unit is a guest: a full engine needs root, namespaces, cgroups
  and an overlay file system, which a podvm rootfs unit does not have and
  a guest of the machine tier does (the `microvm` kind, T-037), or a
  system container of incus (T-105). No socket of the host reaches the
  unit (G-09).
* The images come through the egress of the unit and its policy, as the
  rest of the traffic of a job: the registries of `[unit.*] registries`.
* A unit kind says which route it has (`container: hooks` or
  `container: engine`); a job that needs `services:`, `docker build` or
  Docker-in-Docker waits for a kind with `engine`.
* The hooks stay the route of `container:` jobs and Docker actions on the
  podvm kind.

No shim answers the Engine API, so no `pod-dockerapi`. The only crate
change is the value `engine` of `ContainerRoute` (step 3).

## Steps

1. The census and this design (done).
2. A guest with no NIC and one host channel from podvm (GitHub #83,
   PodUtils/podvm@T-1751), shared with T-037 (done: podvm
   `v0.2.0-beta.10`).
3. The runner image with an engine, started by the guest script before
   the runner, and the capability `container: engine` of the `microvm`
   kind. `engine` is a new value of `ContainerRoute` of `pod-job`: ask
   podcrates for it by a COORD issue at this step.
4. Proof in `runner-image.yml`: a `services:` job, a Dockerfile action
   and a Docker-in-Docker step pass in a guest.
