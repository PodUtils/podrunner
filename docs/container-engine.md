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

No shim answers the Engine API, so no `pod-dockerapi`. `ContainerRoute` has
the value `engine`.

## The engine in a guest

`[unit.microvm] engine = DIR` names a directory of the static Docker
binaries (`docs/configuration.md`, "The microvm kind"). For each guest:

* The kind passes `--podvm-engine` (podvm `v0.2.0-beta.14` or later):
  podvm loads the modules of the pinned guest kernel that an engine needs
  (bridge, veth, overlay, netfilter), mounts cgroup2 and brings `lo` up.
* The guest mounts DIR read-only at `/podrunner-engine`, in the mount
  table of the unit (`MOUNT_CHECK`).
* The guest script starts `dockerd` as root with `DOCKER_RAMDISK=true`
  (the root of a guest is an initramfs, so runc cannot `pivot_root`). The
  socket is `/run/podrunner-docker.sock`, in the guest only. `dockerd`
  pulls through the proxy of the egress channel.
* The runner gets `DOCKER_HOST` of that socket. The runner image has
  `iptables` (nft) for the bridge rules of `dockerd`.

The incus kind takes `engine` too: each instance runs an engine of its own
(`docs/configuration.md`, "The incus kind"; T-485).

## The proof

- The measure `scripts/measure-engine.sh` runs `dockerd` with `overlay2` in
  a guest, and one container reaches another by name on a user-defined
  network.
- `crates/podrunner-microvm/tests/engine.rs` runs a container as the user
  `runner` on a network in a real guest.
- The kind with an engine says `container: engine` in `probe`. Placement
  gives a job that needs an engine (`UnitNeed::container_engine` of
  `pod-job`) only to such a kind (GitHub #91).
- The step "The parity suite in guests with an engine" of `runner-image.yml`
  runs a `services:` job, a job container and a Dockerfile action of the
  official runner in a guest, and in an incus instance with an engine.
- The same step runs a step that runs `docker build` and `docker run`
  itself (`10-docker-build.yml`), and a privileged `docker:dind` service
  that the step reaches on a port of `localhost` (`11-docker-dind.yml`):
  the step loads an image into the inner engine and runs a container in
  it. The jobs `docker-build` and `docker-dind` of `level2.yml` run the
  same text on a GitHub-hosted runner (T-516).

## Docker-in-Docker

The engine of a unit takes `--privileged` and each other option of
`docker create`: the hook policy refuses such options on the route
`hooks` only. The boundary of a job with an engine is the guest or the
unprivileged incus instance, not the options of a container
(`docs/decisions.md`, T-516). So a `docker:dind` service is allowed as on a
GitHub-hosted runner, with two differences:

* In a guest of the `microvm` kind, give the service
  `DOCKER_RAMDISK: "true"`. The root of the guest is an initramfs, and the
  engine of the unit runs its containers with no `pivot_root`, so runc of
  the inner engine cannot do a `pivot_root` either.
* The inner engine gets no proxy and no mirror. The proxy of the egress
  is on the loopback of the unit (`127.0.0.1`), and a container on a
  bridge network does not reach that address. Pull the image with the
  engine of the unit, and give it to the inner engine with `docker save`
  and `docker load`.
