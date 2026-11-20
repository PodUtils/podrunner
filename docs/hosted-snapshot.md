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
No boot of the snapshot has a result yet (T-556), and the image holds no
runner yet (T-541): a unit of it cannot run a job now.

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

The published image holds no runner. T-541 adds the runner of podrunner
and its hooks as one more layer on top, at `/home/runner/actions-runner`,
with the work folder `/home/runner/work`; it is not done yet.

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
