# The hosted snapshot

The hosted snapshot is a copy of the root file system of a GitHub-hosted
runner, with its kernel, as an OCI image on ghcr.io. podrunner builds and
compiles nothing for it: the owner's ruling "The hosted snapshot"
(`docs/decisions.md`). A unit of the snapshot gives a job the tools, the
services and the environment of the hosted label.

## Where it is

`ghcr.io/podutils/podrunner-hosted`, one tag for each image that a capture
took: `IMAGEOS-IMAGEVERSION`, for example `ubuntu24-20261116.376.1`
(`ImageOS` and `ImageVersion` of the hosted runner; on arm64 `ImageOS`
names the architecture, `ubuntu24-arm64`), and one multi-arch index for
each OS, `ubuntu24-latest` and `ubuntu26-latest`
(`.github/workflows/hosted-index.yml`). Pin an image by its digest. The
package is public; each version stays (the owner's ruling "Retention on
ghcr.io").

The layers are `tar+zstd`. The stable trees are at the bottom (the Android
SDK, .NET, the JVMs, each version of the tool cache), the base of the
distribution is on top, and the last layer, `boot`, holds `/boot` and
`/boot/efi`: the kernel and the initrd of the hosted runner. The config
names `/sbin/init` as the entrypoint, for a unit that boots the init of
the image (`[unit.incus] init = "image"`, podvm `--podvm-init=image`).
The core slice boots under incus to `degraded` (`hosted-boot.yml`, T-556).
The full snapshot loads with podvm `0.2.0-beta.38` or later (T-606).
Podvm `0.2.0-beta.40` imported and booted it on standard Ubuntu 24.04
hosted runners with Btrfs and `dir` pools (T-557). Use its runner image
to run a job (below).

## What is 1:1, and what the scrub changes

Each file of the root is in the layers with its type, mode, owner, size,
link target and file capabilities (`ping` keeps `cap_net_raw`). The
capture drops or changes only what is the state of the capture machine,
the job that made the capture, or a value that a new hosted runner gets
fresh (`images/hosted/scrub.toml` names each rule with its reason):

- dropped: the hosted runner and its credentials, the state of GitHub's
  hosted compute agent, the Azure agent and its keys, the provisioning
  keys, the logs and the journal, the cloud-init state, the random seed,
  the SSH host keys, the snakeoil TLS key and the keys of MySQL;
- made fresh at boot: `/etc/machine-id` (empty), the SSH host keys (a
  drop-in of `ssh.service` runs `ssh-keygen -A`), the snakeoil key
  (`ssl-cert.service`), the keys of MySQL (mysqld at its first start);
- changed for a unit: `/etc/fstab` (the root by its label only),
  `/etc/hostname`, the netplan file (DHCP on any NIC), cloud-init disabled,
  and the units of Azure and of GitHub's agents masked;
- kept as the hosted runner has it: the Docker Hub login of
  `/home/runner/.docker/config.json` (the owner's ruling "The snapshot
  keeps the Docker Hub login").

The published snapshot holds no runner and no `/home/runner/work`.

## The runner image

`.github/workflows/hosted-runner.yml` puts one layer on top of a
published snapshot manifest (T-541): no build and no new capture. Each
other blob stays the blob of the snapshot, so a new runner pin makes a
new runner image in minutes. The tag is
`IMAGEOS-IMAGEVERSION-runner-VERSION` in the same package, for example
`ubuntu24-20261116.376.1-runner-2.338.2`; the manifest names its base
(`dev.podrunner.hosted.base`) and the runner (`dev.podrunner.runner-version`).
`scripts/hosted/runner-layer.py` makes the layer:

| Path | Owner, mode | What |
| --- | --- | --- |
| `/home/runner/actions-runner` | `runner` and its group of the snapshot, as in the tarball | the official runner of `images/runner/runner.toml`, checked against its digest; one path for both archs |
| `/home/runner/actions-runner/.env`, `.path` | `runner`, 0644 | each line of the snapshot's `/etc/environment` (the hosted `PATH`, `ImageOS`, `ImageVersion`, `RUNNER_TOOL_CACHE=/opt/hostedtoolcache`, `AGENT_TOOLSDIRECTORY`), and `ACTIONS_RUNNER_HOOK_JOB_STARTED`; a JIT runner runs no `config.sh`, and the runner sets each line in the environment of each job |
| `/home/runner/actions-runner/hooks/` | `runner`, 0755 | the relay of the container hooks and the job-started hook of `images/runner/Dockerfile` |
| `/home/runner/actions-runner/.runner`, `.credentials`, `.credentials_rsaparams`, `.podrunner-job` | links | relative links to `/podrunner/.runner/`, where the unit writes the credential files |
| `/home/runner/work` | `runner`, 0755, empty | the hosted work folder |

The layer refuses a tarball of another digest, a member or a link out of
the runner root, a snapshot with no user `runner` at `/home/runner`, and
an `/etc/environment` with no line of the hosted environment.

A unit of the runner image needs the runner path and the hosted work
folder in its kind:

```toml
[unit.incus]
slots = 1
binary = "/opt/podrunner/drivers/podvm"
images = "/var/lib/podrunner/images"
init = "image"
command = ["/home/runner/actions-runner/run.sh"]
work_folder = "/home/runner/work"
```

podrunner writes `work_folder` into `.runner` before the start, so a job
sees `GITHUB_WORKSPACE=/home/runner/work/REPO/REPO` and
`RUNNER_WORKSPACE=/home/runner/work/REPO`, as on a hosted runner
(`docs/configuration.md`). `/home/runner/work` is also the default of the
podvm, the incus and the microvm kinds, and the local mode uses it in each
unit (T-590).

Proof level: in CI on GitHub-hosted runners, `hosted-runner.yml` publishes
the runner image, and `hosted-boot.yml` boots its core slice with the runner
layer under incus and runs a canned job in it (T-541).

`hosted-boot.yml` with the digest of a runner image runs that canned job
in the slice, through the slots of podrunner on the incus kind above
(`crates/podrunner-broker-fake/tests/hosted.rs`). The job must see
`GITHUB_WORKSPACE=/home/runner/work/app/app`,
`RUNNER_TOOL_CACHE=/opt/hostedtoolcache` and the `ImageOS` of the
snapshot from `.env`, and `actions/setup-python` must take the Python of
the tool cache with no download. For it the slice keeps the tool cache of
one Python (`slice.py --keep`, the input `python`, 3.12 by default). The
input `plant = work-folder` gives the kind `/home/runner/_work`: that run
must end red.

```sh
gh workflow run hosted-boot.yml -f digest=sha256:DIGEST
```

Set `rootless_podman=true` to run the offline Podman and FUSE probe on
the core slice (T-674). The workflow enables engine nesting, loads a
pinned Alpine archive in the instance and checks the guest exit and
the `runner` transcript. The `rootless-podman` artifact has the log and
exit code.

```sh
gh workflow run hosted-boot.yml -f digest=sha256:DIGEST -f rootless_podman=true
```

## The checks of each capture

`.github/workflows/hosted-snapshot.yml` runs the capture as the first step
of a job on the hosted label (`scripts/hosted/capture.sh`). A failed check
stops the capture; nothing is pushed.

- C1, 1:1 (`scripts/hosted/c1.py`): the rows of the layer streams, merged
  in order, against the root after the scrub, path by path.
- C2, no secret (`scripts/hosted/scrub.py verify`): no dropped path in any
  layer stream, each changed file with its new content, each masked unit
  masked, and no credential pattern in the state of the machine that no
  rule covers. A finding names a path and a pattern, never a value.
- No refused record (`scripts/hosted/scrub.py verify`): no `GNU.sparse.*`
  record, which podvm refuses (T-599). The capture runs GNU tar with no
  `--sparse`; a sparse file costs its full size in the layer. It runs GNU
  tar with `--acls`: podvm `0.2.0-beta.38` sets each `SCHILY.acl.*`
  record as `system.posix_acl_*` (T-556). The incus tier of podvm keeps
  only `security.capability` and `user.*`, so an ACL does not reach an
  incus instance.
- The layer table, the record of the image (`snapshot.json`) and the
  findings are the artifacts of the run.

`hosted-parity.yml` compares the probe of each hosted label with the probe
of a unit, and watches each label for a new image (`scripts/hosted/watch.py`).

## Pull and verify

```sh
gh attestation verify oci://ghcr.io/podutils/podrunner-hosted@sha256:DIGEST --repo PodUtils/podrunner
podvm pull ghcr.io/podutils/podrunner-hosted@sha256:DIGEST
```

The image is about 16 GiB with zstd and about 55 GiB on disk. A host of
the incus kind needs the extracted tree and about 2.4 times it in incusd
(T-551). Give its pin `unpacked_bytes`: `check` refuses it above the
root disk of the incus kind (`docs/install.md`, "The disk and the sysctls
of a host of the incus or podvm kind").

`.github/workflows/hosted-measure.yml` measures the disk of the pull, the
load and the import of a snapshot on a standard hosted runner (T-557):

```sh
gh workflow run hosted-measure.yml -f digest=sha256:DIGEST -f full=all -f place=btrfs
```

The places:

* `btrfs` (the default): one btrfs over a loop file on `/` and one on
  `/mnt` (data and metadata `single`) holds the store and incusd. incusd
  takes it as its default pool with the `btrfs` driver (a preseed of
  `incus admin init`). The pool keeps the image as a subvolume and the
  root of the instance as its snapshot, so the two share their blocks
  (T-613).
* `joined`: one LVM ext4 volume over the same two loop files, and a pool
  of the `dir` driver. Its first run lost its runner.
* `store-root` and `store-mnt`: the two file systems stay apart, and the
  pool has the `dir` driver. The full `store-root` run 45521078992 used
  about 119.3 GB after import. The full `btrfs` run 45521073386 used
  about 103.8 GB at boot. Both boots passed (T-557, T-613).
* `dir-small`: use `full=slice` to verify the import refusal on an
  isolated 2 GiB `dir` pool. The workflow shows the free and required
  space and checks that Incus created no instance (T-613).

podvm makes no pool: the instance takes the root disk of the default
profile. The summary and the artifact give the free space and the time
at each step; with `btrfs`, the columns of incusd are `df -B1` of the
btrfs, and `pool.txt` has `btrfs filesystem usage -b` at each step.
