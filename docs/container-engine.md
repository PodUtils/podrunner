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
change is the value `engine` of `ContainerRoute` (step 3c).

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

The incus kind refuses `engine`.

## Steps

1. The census and this design. Done.
2. A guest with no NIC and one host channel from podvm (GitHub #83,
   PodUtils/podvm@T-1751), shared with T-037. Done: podvm
   `v0.2.0-beta.10`.
3. The engine in the `microvm` kind:
   1. The measure (`scripts/measure-engine.sh`) and the guest kernel of an
      engine from podvm (GitHub #92, `--podvm-engine`). Done: podvm
      `v0.2.0-beta.14`; `dockerd` runs with `overlay2`, and on a
      user-defined network one container reaches another by its name
      (runner-image run 43514371042).
   2. The code: `[unit.microvm] engine`, the mount, `dockerd` in the guest
      script and `DOCKER_HOST` for the runner. Done:
      `crates/podrunner-microvm/tests/engine.rs` runs a container as the
      user `runner` on a network in a real guest (runner-image run
      43524984859).
   3. Open: the capability `container: engine` in `probe`, and placement
      that gives a job that needs an engine only to such a kind. It needs
      `ContainerRoute::Engine` and `UnitNeed::container_engine` of
      `pod-job` (GitHub #91, PodUtils/podcrates@T-553, the tag after
      `v0.41.0`). Until then the kind says `container: none`, so a
      `container:` job is refused on it.
4. Open: proof in `runner-image.yml`: a `services:` job, a Dockerfile
   action and a Docker-in-Docker step of the official runner pass in a
   guest.
