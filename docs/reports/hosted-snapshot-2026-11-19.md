# The hosted snapshot: the study

Read-only study, 2026-11-19. podrunner `main` at `fceac4d`.

The public copy holds back section 2.1 and the parts that name secret
material in third-party published images. The owner decides the
disclosure to their author first.

The frame is the owner's ruling "The hosted snapshot" (2026-11-19,
`docs/decisions.md` line 18): no build; a 1:1 copy of the root file system
of a GitHub-hosted runner, with its kernel, fixed, tested and published on
ghcr.io as streamable blobs. This study replaces the build plan of
`docs/reports/hosted-parity-2026-11-19.md` (its W-6, W-10, W-15, W-16 and
the licence text of its section 3.5). Its probe, watcher, Pester and
placement items stay, with changes (section 6).

Marks:

- **[M]** measured: a URL, a file and line, or the output of a command of
  this study. Layer facts come from streams of the published blobs of
  `ghcr.io/christopherhx/runner-images` (headers only; no secret value was
  read or kept: for a credential file the study read only the key names
  and the length of each value).
- **[E]** estimate: no measure exists. A work item measures it.
- **[N]** not found.

## 0. The answer in short

1. ChristopherHX streams `sudo tar` of the live root of four hosted
   runners (`ubuntu-24.04`, `ubuntu-26.04`, `ubuntu-24.04-arm`,
   `ubuntu-26.04-arm`) through `gzip -9` into ghcr.io with a Node script,
   in 18 path layers, and writes a Docker schema 2 manifest. catthehacker
   copies the result byte for byte with `skopeo copy -a` and changes
   nothing.
2. **The published blobs hold secret material and the machine state of
   the capture VM** [M] (section 2.1, held back). The exclusion
   `/home/runner/runners` is stale: the runner now lives in
   `/home/runner/actions-runner/cached/VERSION` (x64) and
   `/home/runner/extracted` (arm64).
3. **The copy is not 1:1** [M]: 0 extended attributes (file capabilities of
   `ping` and `mtr-packet` are lost), no `/boot` (no kernel, but the
   modules are there), no `/var/lib/apt/lists`, no `/var/cache` (the
   debconf database), no `/etc/skel`, no `/var/lib/docker`. On 26.04 the
   live containerd store is copied with 122 whiteout device nodes.
4. **No record and no test** [M]: no runner-images commit, no SBOM, no
   signature, no attestation, no test that a job runs. The newest images
   are `20261012.*`, captured 2026-10-23; three hosted releases came since;
   no scheduled run after 2026-09-22. Two weekly snapshots share 0 of 18
   layers.
5. podrunner must re-mirror. Their blobs cannot be fixed by a filter:
   what they excluded (`/boot`, xattrs, apt lists, debconf, `/etc/skel`)
   is gone, and their machine state sits in the lowest layer.
6. The better pipeline: capture in the first step of a job on each hosted
   label; quiesce; read `/` through a bind mount; stream GNU tar
   (`--format=posix --xattrs --acls --sparse --numeric-owner`) through a
   scrub filter and `zstd -T0`; chunked upload; a two-VM diff that finds
   machine state automatically; a manifest-level 1:1 check in the same
   job; a kernel layer from `/boot`; an attested multi-arch index; a daily
   watcher that captures each new `ImageVersion`.
7. The hosted Azure kernels have `virtio_pci`, `virtio_blk`, `virtio_net`,
   `virtio_scsi`, `ext4`, `loop` and `fuse` built in [M] (x64 6.17 and
   7.0, arm64 6.17). They can boot under QEMU/KVM. Use the hosted kernel in
   the VM modes: `uname -r` and `modprobe` then match the hosted runner.
8. Tests: the 1:1 manifest check and a secret scan run in the capture
   job; a boot of the base slice under incus runs on a standard hosted
   runner; the full boot, the Pester tests of `/imagegeneration/tests`,
   the software report diff and the real jobs run on the live host lane.

## 1. How the community snapshots are made

### 1.1 ChristopherHX `runner-image-blobs`

Repository at `2fa1775f4dc94f170b2c26f3264f7b4350b60100` (2026-10-22). One
workflow, `.github/workflows/dump.yml`, one helper
`check-composite-up-to-date.mjs`, MIT licence. Line numbers below are of
`dump.yml` unless named.

| Step | What it does | Source |
| --- | --- | --- |
| Triggers | `push`, `schedule` `0 22 */7 * *` (days 1, 8, 15, 22, 29 of a month), `workflow_dispatch` with `FORCE_UPDATE` | lines 1 to 9 |
| Schedule gate | a scheduled run needs the repository variable `ENABLE_SCHEDULE == 'true'` | line 12 |
| Labels | `ubuntu-24.04`, `ubuntu-26.04`, `ubuntu-24.04-arm`, `ubuntu-26.04-arm`; `fail-fast: false` | lines 15 to 20 |
| Token | `permissions: write-all`; the `GITHUB_TOKEN` as Basic auth to `https://ghcr.io/token` | lines 22, 43 to 50 |
| Pipe | `tar ... \| tee >(sha256sum > sha.txt) \| gzip -9 -c \| tee >(sha256sum > sha2.txt) \| node script.js`: the diff_id and the blob digest in one pass, no disk | lines 25 to 26 |
| First action | `actions/setup-node@v6` with Node 24, before the dump | lines 29 to 32 |
| Upload | one `POST /blobs/uploads/`, **one** `PATCH` with stdin as the body, then `PUT ?digest=` with 10 retries | lines 51 to 94 |
| Environment | reads `/etc/environment` into the image `Env` | lines 98 to 115 |
| Version | `ImageOS` (suffix `-arm64` removed) and `ImageVersion` from `/etc/environment` | lines 117 to 122 |
| Labels of the image | OCI annotations: source, revision (its own commit), `version` = the friendly tag | lines 124 to 136 |
| Config | `User` = uid:gid of the job process (`1001:1001` measured), `Cmd ["bash"]`, history says "streaming the rootfs using tar and nodejs ... GitHub Hosted Runners do not have enough space for docker build / pull itself" | lines 138 to 160 |
| Manifest | Docker schema 2 (`application/vnd.docker.distribution.manifest.v2+json`), layers `application/vnd.docker.image.rootfs.diff.tar.gzip` | lines 215 to 244 |
| Tags | `IMAGEOS-TAG-RUNID.ATTEMPT-ARCH` always; `IMAGEOS-TAG-IMAGEVERSION-ARCH` only when it does not exist or `FORCE_UPDATE`; `IMAGEOS-TAG-latest-ARCH` moves with it | lines 249 to 284 |
| Base layer | `sudo tar --ignore-failed-read -C / -c --no-recursion /run /run/user /run/user/* --recursion` with the exclusions below, run twice on failure (`CMD \|\| CMD`) | lines 299 to 307 (line 304) |
| Other layers | 17 more `tar` runs, one per tree: `/usr/local`, `/opt`, `/opt/hostedtoolcache`, `/usr/lib/gcc`, `~/.rustup`+`~/.cargo`, `/usr/share/dotnet`+`~/.dotnet`, `/usr/share/swift`, `/usr/share/miniconda`, `/usr/lib/google-cloud-sdk`, `/usr/lib/jvm`, `/usr/lib/mono`, `/usr/lib/llvm-*`, `/usr/lib/firefox`, `/usr/lib/python3` (+ command-not-found), `/usr/local/lib/android`, `/usr/local/.ghcup`, `/usr/local/share/powershell`, `/snap` | lines 308 to 547 |
| Images | one config and manifest per composition: `runner-base`, `runner-large` (18 layers, no `/snap`), `runner-medium` and 12 `runner-medium-*`, `runner-usr-local-opt` | lines 548 to 821 |
| Index | job `upload-manifests` on `ubuntu-latest`: `docker login` with the token, then `docker buildx imagetools create -t BASE AMD64 ARM64` for each tag pair | lines 827 to 853; `check-composite-up-to-date.mjs` lines 67 to 71 |

Exclusions of the base layer (line 304): `$GITHUB_WORKSPACE`, `/proc/*`,
`/dev/*`, `/run`, `/sys/*`, `/tmp/*`, `/opt/*`, `/var/cache/*`,
`/var/snap`, `/var/run/*`, `/var/tmp/*`, `/var/crash/*`,
`/var/lib/docker/*`, `/var/lib/apt/lists/*`, `/usr/local/*/*`, `/mnt/*`,
`/snap/*`, `/boot`, `/data`, `/imagegeneration/installers/*.tar.*`,
`/home/runner/runners`, `/home/runner/work`, `/usr/share/dotnet`,
`/etc/skel`, `/home/runner/.rustup`, `/home/runner/.cargo`,
`/home/runner/.dotnet`, `/home/runneradmin`, `/usr/share/swift`,
`/usr/share/miniconda`, `/usr/lib/google-cloud-sdk`, `/usr/lib/jvm`,
`/usr/lib/mono`, `/usr/lib/gcc`, `/usr/lib/llvm-*`, `/usr/lib/firefox`,
`/usr/lib/python3`, the command-not-found files, `/swapfile`.

The tar flags are only `--ignore-failed-read -C / -c`: no `--xattrs`, no
`--acls`, no `--sparse`, no `--one-file-system`, no `--numeric-owner`, the
default GNU format.

What the README promises (`README.md` lines 7 to 25): the "full docker
images of act", `ubuntu24-runner-large-latest` and
`ubuntu26-runner-large-latest`; "Docker images that do not use the root
user are not supported by the actions/runner application"; "this cannot be
used inside GitHub Actions hosted runners, you need a self-hosted runner".

The handling of the items the brief names, from the line-304 list and the
layer streams:

| Item | Handling | Evidence |
| --- | --- | --- |
| `/boot`, kernel | `/boot` excluded; `/usr/lib/modules/KVER` and the `linux-image-*` dpkg records kept | [M] base layer lists `usr/lib/modules/6.17.0-1022-azure` and `var/lib/dpkg/info/linux-image-6.17.0-1022-azure.list`, no `boot/` |
| `/etc/fstab` | kept as is (root UUID, `LABEL=BOOT`, `/boot/efi` UUID, `ephemeral0 /mnt`, `/swapfile`) | [M] |
| `/etc/machine-id` | kept (33 bytes); `/var/lib/dbus/machine-id` link | [M] |
| `/etc/hostname`, `/etc/hosts` | kept: the VM name and its internal Azure DNS name | [M] |
| SSH host keys | kept: `ssh_host_{rsa,ecdsa,ed25519}_key` (mode 0600) and `.pub` | [M] |
| Runner files | intended out (`/home/runner/runners`); **in** (path moved) | [M] |
| `/home/runner/work` | excluded | line 304 |
| `/var/lib/docker` | excluded; on 26.04 `/var/lib/containerd` is in (2466 MiB raw) | [M] |
| Caches | `/var/cache/*` and `/var/lib/apt/lists/*` excluded | line 304 |
| Swap | `/swapfile` excluded; the fstab line stays | line 304; [M] fstab |
| `/mnt`, `/tmp` | contents excluded, the directories stay | [M] |
| Logs | kept: journal, syslog, auth.log, cloud-init.log, waagent.log, kern.log, dmesg | [M] |

Measured outputs [M]:

| Image (`*-runner-large-latest-ARCH`, 2026-11-19) | ImageVersion | Created | Layers | Compressed | Largest layer |
| --- | --- | --- | --- | --- | --- |
| ubuntu24 amd64 | 20261012.334.1 | 2026-10-23T19:08Z | 18 | 17.78 GiB | 4.10 GiB |
| ubuntu24 arm64 | 20261012.149.1 | 2026-10-23T14:53Z | 18 | 10.16 GiB | 2.20 GiB |
| ubuntu26 amd64 | 20261012.163.1 | 2026-10-23T15:52Z | 18 | 18.51 GiB | 4.18 GiB |
| ubuntu26 arm64 | 20261012.150.1 | 2026-10-23T15:11Z | 18 | 11.41 GiB | 3.76 GiB |

One weekly set is 57.86 GiB of new blobs [M, the sum].

Run times of the last run, 39964244981 (2026-10-23, the jobs API) [M]:
`ubuntu-24.04` 2 h 32 min (the android layer alone 31 min 14 s, the base
layer 18 min 39 s), `ubuntu-26.04` 2 h 05 min, `ubuntu-24.04-arm`
1 h 06 min, `ubuntu-26.04-arm` 1 h 24 min, then `upload-manifests`
25 min.

### 1.2 catthehacker `docker_images` (`full-*`)

Repository at `b84b223032d1cab44ba75d0d2b5bd790046c63ac`.

| Step | Source |
| --- | --- |
| Schedule `0 12 */7 * *` and dispatch with `version` (default `latest`) | `.github/workflows/copy-full-image.yml` lines 3 to 10 |
| One job on `ubuntu-latest`, `max-parallel: 1` | lines 23 to 28 |
| Matrix: `ubuntu26-runner-large-latest` to `full-latest` and `full-26.04`; `ubuntu24-...` to `full-24.04`; `ubuntu22-...` to `full-22.04` | lines 30 to 38 |
| `skopeo copy -a` with the `GITHUB_TOKEN` as source and destination credentials, to ghcr.io; to Docker Hub when secrets exist | lines 47 to 57 |
| README: "20GB compressed, ~60GB extracted"; "verify if the Image is still updated regulary by inspecting the dates" | `README.md` lines 18 to 21 |

It changes nothing. The index digests are equal [M]: the manifest bytes of
`catthehacker/ubuntu:full-24.04` and
`christopherhx/runner-images:ubuntu24-runner-large-latest` hash the same;
so do `full-26.04`, `full-latest` and `ubuntu26-runner-large-latest`.
`full-22.04` copies a tag that ChristopherHX no longer makes (its matrix
has no 22.04): it copies a frozen image each week. The `act-*` and
`runner-*` images are built by `linux/ubuntu/scripts/act.sh` and are not
snapshots.

## 2. Defects and gaps, with evidence

Each row is a behaviour difference from the hosted runner, a leak, or a
missing property. "P" is the effect on podrunner if it used their blob.

### 2.1 Secret material and machine state in the published layers

Held back from the public copy. The section lists, with key names and
lengths only, the secret material, the logs and the machine state that the
base layers of the community snapshot hold, and the cause: a fixed
exclusion list (`/home/runner/runners`) that went stale when the runner
moved. A fixed exclusion list goes stale with each change of GitHub's
provisioning. The scrub list of section 3.3 names each path of the hosted
image that podrunner drops or rewrites, and checks C2 and C3 of section 4
fail closed on a new one.

### 2.2 Lost metadata

| Gap | Evidence | Effect |
| --- | --- | --- |
| No extended attributes | 0 PAX xattr records in the base layers of 24.04 amd64 (169,556 entries), 26.04 amd64 (235,879) and 24.04 arm64 (164,239) [M] | Ubuntu's `iputils-ping` postinst runs `setcap cap_net_raw+ep` and removes the setuid bit; `mtr-tiny` does the same for `mtr-packet` [M: git.launchpad.net, `ubuntu/noble`]. In the layer both are plain `0755` [M]. `ping` then needs `net.ipv4.ping_group_range`; `mtr` as `runner` fails. `gst-ptp-helper` also loses its caps. |
| No ACLs, no sparse files | no `--acls`, no `--sparse` (line 304) | a sparse file (for example `/var/log/lastlog` with large uids) grows [E] |
| Hard links across layers | GNU tar keeps a hard link only inside one archive; 16 (24.04) and 139 (26.04) hard links in the base layers [M] | a link whose two names fall in two layers becomes two files [E] |
| Setuid kept | 24 (24.04) and 59 (26.04) setuid/setgid files in the base layers, with the right modes [M] | none |
| Device nodes captured | 122 character devices in the 26.04 base layer, all under `var/lib/containerd/io.containerd.snapshotter.v1.overlayfs/snapshots/*/fs/` (overlay whiteouts) [M] | a rootless extractor skips or fails on them (podcrates `pod-extract` skips them: `crates/pod-extract/src/apply.rs` lines 212 to 220) |
| Live containerd store | `var/lib/containerd` 2466 MiB raw in the 26.04 base layer, copied while containerd ran; `/var/lib/docker` excluded [M] | an inconsistent bolt database is possible [E] |

### 2.3 Missing trees and broken compositions

| Gap | Evidence | Effect |
| --- | --- | --- |
| No kernel | `/boot` excluded; modules and dpkg records kept [M] | no VM boot; `dpkg --verify` reports the missing `vmlinuz` [E] |
| No apt lists | `/var/lib/apt/lists/*` excluded (line 304); runner-images `cleanup.sh` keeps them (only `apt-get clean`, lines 10 to 15) [M] | `apt-get install X` with no `apt-get update` works on the hosted runner and fails in the snapshot |
| No `/var/cache` | excluded (line 304) | the debconf database (`/var/cache/debconf`) is gone [E: the debconf default path] |
| No `/etc/skel` | excluded (line 304) | `useradd -m` gives an empty home |
| No `/var/lib/docker` | excluded (line 304) | the images that `install-docker.sh` pulls on 24.04 are missing |
| No `/var/snap`, `/snap` | excluded; the `/snap` layer is the content of the mounted squashfs files, not the `.snap` files | snapd state broken; not in `runner-large` |
| `runner-base` has no Python stdlib | 0 entries under `usr/lib/python3/` in the base layer [M] | `python3` fails in the `base` tags |
| `runner-usr-local-opt-snap` is empty | its manifest has 0 layers [M]; cause: the variable is `LAYER_LAYER_DIFF_IDS` (line 565) | a published tag with no content |
| Default user | `User 1001:1001` [M]; the README tells users to pass `--user 0:0` | a different start than the hosted runner |

### 2.4 Record, signatures, tests

| Gap | Evidence |
| --- | --- |
| No runner-images commit, no release tag, no SBOM | only `ImageOS` and `ImageVersion` in the tag; the hosted release has `internal.ubuntu24.json` and `sbom.ubuntu-24.04.json.zip` [M: releases API] |
| The release tag differs from the version | `/imagegeneration/imagedata.json` of 20261012.334.1 links `releases/tag/ubuntu24/20261012.334`; the published tag is `ubuntu24/20261023.334` [M]. The build number (`.334`) maps them, not the date |
| No signature, no attestation | no referrers, no cosign tag, no `actions/attest` step in `dump.yml` [M] |
| No test | no step boots the image or runs a job [M] |
| The `setup-node` action runs before the dump | line 29; it can add a Node version to `/opt/hostedtoolcache`. The 20261012.334.1 toolcache shows only `node 22.23.3` and `24.21.0`, with build-time mtimes [M], so no change this time |

### 2.5 Format, size, tags, cadence

| Gap | Evidence |
| --- | --- |
| Docker schema 2 and gzip only | lines 215 to 244 [M] |
| Single-threaded `gzip -9` | line 26; 2 h 32 min for one x64 snapshot [M] |
| Large layers | 4.10 GiB and 4.18 GiB layers [M]; ghcr.io documents "a 10 GB size limit for each layer" and "a 10 minute timeout limit for uploads" [M: docs.github.com, "Working with the Container registry", lines 258 to 259 of the page read]; their one-`PATCH` upload of a 31-minute stream is outside the documented timeout |
| No dedup | the amd64 24.04 snapshots `20260907.300.1` and `20261012.334.1` share 0 of 18 layer digests [M] |
| Versioned tags are one arch | `ubuntu24-runner-large-20261012.334.1` is an index with only amd64; `...-20261012.149.1` only arm64 [M]: the two archs have different build numbers, so only `-latest` is multi-arch, and it mixes two hosted builds |
| Tag sprawl | 45,062 tags in the repository [M] |
| arm64 naming changed | arm64 tags were `ubuntu24-arm64-runner-*` until commit `2fa1775f` (2026-10-22) [M] |
| Lag | runs list [M]: weekly scheduled runs until 2026-09-22, none after; one dispatch on 2026-10-23. Hosted releases since: `ubuntu24/20261026.354`, `20261101.360`, `20261111.370`, and the pre-release `20261116.376` [M]. On 2026-11-19 the newest snapshot is 27 days and 3 releases old |
| 26.04 and arm64 | covered since 2026-06-22 (commit `96c58e25`) [M]; `full-latest` is already 26.04 |

## 3. A better pipeline for podrunner

No step builds or compiles a tool of the image. The scripts below only
read, filter, compress and push.

### 3.1 Capture

**Runners.** One job for each label: `ubuntu-24.04`, `ubuntu-26.04`,
`ubuntu-24.04-arm`, `ubuntu-26.04-arm`. A standard runner of a public
repository is enough: the stream writes nothing to disk. The job also
records which label `ubuntu-latest` maps to today (a fifth, tiny job on
`ubuntu-latest` that only reads `ImageOS`).

**The first step.** The capture runs before any `uses:` action, so no
action writes into `/opt/hostedtoolcache` or `/home/runner`. The scripts
come by `curl` of the repository at `github.sha` into `/mnt` (outside the
captured tree), or by `actions/checkout` into the work folder (excluded).

**Record first** (one JSON, `snapshot.json`):

- `ImageOS`, `ImageVersion` from `/etc/environment`;
  `/imagegeneration/imagedata.json`; `/etc/os-release`; `uname -r`;
  `/proc/cmdline`; `dpkg --print-architecture`; the runner label and the
  run id.
- The runner-images release: the build number `N` of
  `ImageVersion` (`20261111.370.1` gives 370), matched to the release tag
  `ubuntu24/*.370` (or `ubuntu24-arm64/*.N`, `ubuntu26/*.N`) from the
  releases API, and its `target_commitish`. Fail when no release matches.
- The live state for the conformance diff (section 4): `systemctl
  list-units --all`, `systemctl list-unit-files`, `systemctl
  is-system-running`, `swapon --show`, `df`, `ulimit -a` of `runner`,
  `sysctl -a` (a fixed key list), `getcap -r /` (the list of file
  capabilities), `find / -xdev -perm /6000`, `lsmod`, `id runner`, `getent
  group`, `sudo -l -U runner`.
- The path of the hosted runner: `readlink /proc/PPID/exe` of the step
  shell gives `.../bin/Runner.Worker`; its root is two levels up. This
  path goes into the scrub list of this run, so a move of the runner
  directory cannot leak it again.

**Quiesce.** Stop what writes to the captured tree and is not the runner:
`docker.socket`, `docker.service`, `containerd.service`, `snapd.socket`,
`snapd.service`, `cron.service`, `sysstat.service`,
`php*-fpm.service`, `unattended-upgrades.service`, `rsyslog.service`,
`walinuxagent.service`. Do not stop `hosted-compute-agent.service`: it
holds the job. Then `sync`.

**A view of `/` with no other mounts.** `mount --bind / /mnt/root`
(private). The view has no `/proc`, `/sys`, `/dev`, `/run`, `/tmp` content
of the live system and no `/boot`, `/boot/efi`, `/mnt`. `/boot` and
`/boot/efi` are separate file systems on the hosted VM (`/etc/fstab` [M]);
bind them to `/mnt/boot` and `/mnt/efi` for the kernel layer (3.2).

**Partition plan (pass 1).** `find /mnt/root -xdev -printf` with type,
mode, uid, gid, size, inode, link count, mtime and path; a SHA-256 of each
regular file. It runs in parallel (`xargs -P4 sha256sum`). Output: the
live manifest. A script (`python3 -I`) assigns each path to one layer by
the rules of 3.4, keeps each hard-link group in the layer of its first
name, and gives each layer a content key: the SHA-256 of its sorted rows
(path, type, mode, uid, gid, size, content digest, link target, xattrs),
without mtime. A layer whose key equals the key of the same layer in the
previous snapshot reuses that blob (a cross-repository mount, no upload).

**Stream (pass 2).** For each changed layer:

```sh
tar -C /mnt/root --format=posix --xattrs --xattrs-include='*' --acls \
    --sparse --numeric-owner --no-recursion --null -T layer-K.list \
    --pax-option=delete=atime,delete=ctime --sort=none -c \
  | scrub-filter \
  | tee >(sha256sum > diff_id) | zstd -T0 -10 \
  | tee >(sha256sum > digest) | push-blob
```

- `--format=posix` is needed for xattrs (PAX `SCHILY.xattr.*`).
  `--xattrs-include='*'` keeps `security.capability`.
- The lists are sorted by name in pass 1, so the tar is deterministic for
  the same files.
- `scrub-filter` (3.3) rewrites or drops members in the stream. It never
  relies on an upper layer: a whiteout in an upper layer still ships the
  secret in the lower blob.
- `push-blob`: the OCI chunked upload (`POST`, then `PATCH` with
  `Content-Range` in chunks of 256 MiB, then `PUT ?digest=`), so each
  request is far below the 10-minute limit and a failed chunk retries
  alone.
- Compression: `zstd -T0` output is one frame. The level is a measure of
  the first run [E].

**Verify in the same job (pass 3).** Parse the pushed streams again (from
the tee, not from the registry) into a layer manifest, merge the layers,
and diff with the live manifest. Each difference must be a row of the
scrub list. An unlisted difference fails the job before the manifest is
pushed. This is the 1:1 proof at the file level.

**Two-VM diff (machine state found by data).** Once for each new
`ImageVersion`, two capture jobs of the same label run pass 1 on two VMs.
The set of paths whose digest or metadata differ between them is the
machine and job state of that release. Each such path must be in the scrub
list (reset, drop or keep with a reason). A new path fails the run: the
scrub list grows by a review, not by a surprise.

### 3.2 The kernel and the boot

**Capture.** A separate layer `boot`: the content of `/boot`
(`vmlinuz-KVER`, `initrd.img-KVER`, `config-KVER`, `System.map-KVER`,
`grub/`) and of `/boot/efi` (`EFI/ubuntu/shimx64.efi`, `grubx64.efi`,
`grub.cfg`), at their paths. The modules are already in
`/usr/lib/modules/KVER` (base layer [M]). Size: about 100 MB [E].

**Can the hosted kernel boot under KVM?** Yes. `modules.builtin` [M]:

| Kernel | `virtio_pci` | `virtio_blk` | `virtio_net` | `virtio_scsi` | `virtio_console` | `ext4` | `loop` | `fuse` | `overlay`, `9p`, `virtiofs`, `vsock` |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 6.17.0-1022-azure x64 (24.04) | built in | built in | built in | built in | built in | built in | built in | built in | modules in `/usr/lib/modules` |
| 7.0.0-1017-azure x64 (26.04) | built in | built in | built in | built in | built in | built in | built in | built in | modules |
| 6.17.0-1022-azure arm64 (24.04) | built in | built in | built in | built in | built in | built in | built in | built in | modules |

A root on a virtio disk with ext4 mounts with no initrd
(`root=/dev/vda1 rootfstype=ext4`) or with the hosted initrd and
`root=LABEL=cloudimg-rootfs`. The hosted kernel also has `kvm-intel`,
`kvm-amd`, `vhost_vsock`, `nbd`, `v4l2loopback`, `br_netfilter`,
`nf_tables` as modules [M].

**Which kernel.** The hosted kernel, in each VM mode:

- `uname -r`, `/usr/lib/modules/$(uname -r)` and `modprobe` match the
  hosted runner. A job that loads `v4l2loopback`, `nbd` or `kvm`, or that
  installs `linux-headers-$(uname -r)`, behaves the same.
- podvm's machine tier boots Alpine `vmlinuz-virt` 3.22.6 today
  (podvm `crates/podvm/src/machine/guest/kernel.rs` lines 1 to 8);
  `PODVM_MACHINE_KERNEL` names another file. Alpine's kernel has other
  modules and another `uname -r`: not 1:1.
- Ubuntu's `linux-kvm` flavour boots faster but has another `uname -r` and
  fewer modules: not 1:1.

In the incus **system container** mode the kernel is the host's. That is
not fixable: `uname -r`, the module set, the sysctls of the hosted image
(`vm.max_map_count=1048576` in `/etc/sysctl.d/10-map-count.conf` [M]) and
the swap differ. These are allow rows of that mode only.

**What each consumer takes.**

| Consumer | Takes | Gap |
| --- | --- | --- |
| incus system container | a unified tarball or a split image (`metadata.yaml` + rootfs tree or squashfs) [M: incus "Image format" docs]; podvm imports a unified tarball made from its extracted tree (podvm `docs/incus.md` lines 79 to 83) | the extract and the tarball must carry xattrs (3.6 asks) |
| incus VM | a split or unified image whose root is `rootfs.img` in **qcow2** [M: incus "Image format", "Virtual machines use a single rootfs.img file ... qcow2"]; UEFI boot by default | incus runs OCI images only as application containers; VM support for OCI images is open (lxc/incus#1360, open on 2026-11-19 [M]). podvm must make the disk image (GPT, ESP from `/boot/efi`, ext4 root labelled `cloudimg-rootfs`) once for each digest |
| podvm machine tier | a kernel file, a podvm initramfs, and today a root in the initramfs | T-1994: a root on a disk for each digest; the image's own kernel and initrd (ask, 3.6) |
| docker, podman, act | the OCI image | no kernel use |

### 3.3 Fixes: reset, remove, keep

The rule: change only what is machine state, job state, a credential, or
what cannot work outside Azure. Keep the rest, byte for byte. Each row
becomes a row of `images/hosted/scrub.toml` (path, action, reason) and an
allow row of the conformance diff when it changes behaviour.

| Path | Action | Why |
| --- | --- | --- |
| the hosted runner root (`readlink` of 3.1), for example `/home/runner/actions-runner/`, `/home/runner/extracted/` | drop | credentials, logs, the job; podrunner's runner of the pinned version comes in its own layer (3.5) |
| `/home/runner/work` | drop | the job |
| `/opt/hca/` state: `.settings`, `diagnostics.json`, `egress_proxy_condition.json`, `logs/`, `.hca.lock`, `environment.dirty` | drop | tokens and SAS URIs |
| `/opt/hca/` binaries, `/opt/primer/` | drop; mask `hosted-compute-agent.service` and `primer.service` (link to `/dev/null`) | GitHub's provisioner: it calls GitHub's scheduler; no job uses it |
| `/home/runner/.docker/config.json` | rewrite to `{}` with the same owner and mode | shared Docker Hub credential; allow row: anonymous Docker Hub pulls (an operator can add a mirror or a login) |
| `/etc/ssh/ssh_host_*` | drop; a oneshot unit runs `ssh-keygen -A` before `ssh.service` | private keys |
| `/var/lib/waagent/*` except the agent egg | drop; mask `walinuxagent.service` | keys, provisioning document; it needs the Azure wire server |
| `/root/.ssh/authorized_keys`, `/home/packer/.ssh/authorized_keys` | drop | provisioning keys |
| `/etc/machine-id` | an empty file (0 bytes) | machine-id(5): an empty file gives a new id at boot and is not a first boot, so no unit presets change; `uninitialized` would be a first boot |
| `/var/lib/systemd/random-seed`, `credential.secret` | drop | per-machine seed |
| `/var/lib/cloud/` instance and data state | drop; `touch /etc/cloud/cloud-init.disabled` | `datasource_list: [ Azure ]` (`/etc/cloud/cloud.cfg.d/90_dpkg.cfg` [M]) waits for Azure IMDS; allow row: `cloud-init status` says disabled |
| `/etc/netplan/50-cloud-init.yaml` | replace with a match on any `en*`/`eth*` with DHCP | it matches one Azure MAC and `hv_netvsc` [M] |
| `/etc/hosts` line of the VM, `/etc/hostname` | drop the line; `localhost` in `/etc/hostname`; the unit sets its name | the Azure name |
| `/etc/fstab` | keep only `LABEL=cloudimg-rootfs / ext4 discard,commit=30,errors=remount-ro 0 1`; drop `/boot` (in the root now), `/boot/efi` (only in the VM disk, by label), `ephemeral0 /mnt`, `/swapfile` | the UUIDs and the Azure disk do not exist; a missing `/boot/efi` without `nofail` sends systemd to emergency mode |
| swap | VM mode: an optional swap file of the hosted size made at boot; container mode: none | allow row with the hosted `swapon --show` value |
| `/var/log/*` | truncate each file, remove rotated files and `/var/log/journal/*`, as runner-images' own `cleanup.sh` lines 17 to 28 do | logs of the snapshot VM |
| `/var/lib/docker` (24.04), `/var/lib/containerd` (26.04) | keep, captured with the engine stopped | the hosted pre-pulled images; whiteouts stay inside the content, as character devices for root extractors; a rootless extractor needs the 3.6 ask |
| `/tmp/*`, `/var/tmp/*`, `/var/crash/*`, `/run/*` except the directory tree | drop | volatile |
| `pollinate.service` | keep enabled; allow row (no NIC in a unit) | 1:1 |
| `/home/packer`, `packer` user, `/etc/sudoers.d/90-cloud-init-users`, `/home/linuxbrew`, `/opt/runner-cache`, `/opt/actionarchivecache`, `/imagegeneration`, `/var/lib/mysql`, `/var/lib/apt/lists`, `/var/cache`, `/etc/skel`, `/etc/environment`, `/opt/post-generation`, linger of `runner` | keep | they are in the hosted image; the runner uses `ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE=/opt/actionarchivecache` from `/etc/environment` [M] |

`/opt/post-generation/*.sh` (`cleanup-logs.sh`, `systemd-linger.sh`,
`environment-variables.sh` [M]) run once at provisioning on the hosted VM.
Their effects are in the live root (linger of `runner`,
`XDG_RUNTIME_DIR=/run/user/1001` in `/etc/environment` [M]), so a capture
of the live root is closer to the job environment than the build image.

### 3.4 Layering for streaming and dedup

**Facts that drive the design.**

- docker and containerd key an extracted layer by its chain id: the digest
  of the layer and of all layers under it. A changed lower layer makes
  each upper layer extract again, even when its blob is cached. So the
  stable trees go at the bottom, in a fixed order, and the weekly-changing
  base goes on top.
- ghcr.io: 10 GB per layer, 10 minutes per upload request [M].
- overlayfs and docker take about 125 lower layers [E]; keep 100 or fewer.
- podvm reads `tar`, `tar+gzip` and `tar+zstd` (podcrates
  `crates/pod-extract/src/layer.rs` lines 29 to 44). Its gzip reader is
  `flate2::read::GzDecoder` (line 102), which reads one gzip member, and
  its zstd reader is `ruzstd::decoding::StreamingDecoder` (line 103).

**Order, bottom to top** (each a layer or a group of layers; the sizes
are the gzip sizes of the ChristopherHX x64 24.04 layers [M]):

1. `/usr/local/lib/android` split by `sdk/` subdirectory (4.10 GiB in one
   layer today; target 1 to 2 GiB each).
2. `/usr/share/swift`, `/usr/share/dotnet` with `~runner/.dotnet`,
   `/usr/lib/jvm`, `/usr/local/.ghcup`, `/usr/lib/llvm-*`,
   `/usr/lib/google-cloud-sdk`, `/usr/share/miniconda`,
   `/usr/local/share/powershell`, `/usr/lib/firefox`, `/usr/lib/gcc`.
3. `/opt/hostedtoolcache`, one layer for each `TOOL/VERSION` directory (a
   version directory changes only when its version changes).
4. `/home/runner/.rustup` and `.cargo`, `/home/packer`,
   `/home/linuxbrew`, `/opt/az`, `/opt/pipx`, `/opt/microsoft`,
   `/opt/google`, the other `/opt` and `/usr/local` trees.
5. The docker or containerd store.
6. `boot` (3.2).
7. `base`: everything else (`/etc`, `/var`, `/usr` less the trees above,
   `/root`, `/home/runner` dotfiles), plus each parent directory entry
   again, so its metadata is the hosted one.
8. `podrunner`: the runner layer (3.5), in the runnable image only.

**Format.** `application/vnd.oci.image.layer.v1.tar+zstd`, OCI manifests
and index. docker 23 and later, containerd, podman, skopeo and podvm read
it. Lazy formats:

| Format | Who reads it lazily | Who reads it at all | Use |
| --- | --- | --- | --- |
| plain zstd | nobody | all of the above | **yes** |
| eStargz | containerd with stargz-snapshotter, podman | gzip readers that read all members; podcrates' `GzDecoder` reads only the first member [E: flate2 documents one member] | no |
| `zstd:chunked` | podman, CRI-O | zstd readers that read many frames and skippable frames; `ruzstd`'s `StreamingDecoder` needs a test [E] | not now |
| SOCI index | containerd with soci-snapshotter | a separate artifact; the layers stay plain | optional later |

A lazy pull gives nothing to the incus and VM modes: they need the whole
tree before the boot. It helps only a docker user of the image. So the
first version uses plain zstd.

**Size.** About 18 GiB compressed for x64 and 10 to 12 GiB for arm64 with
gzip, per OS version [M: section 1.1]. Our image adds the apt lists,
`/var/cache`, `/etc/skel`, the docker store of 24.04 and the boot layer,
and drops the 1.3 GiB hosted runner and the logs. zstd at a middle level
is about the size of gzip -9 or smaller [E]. Expect 17 to 20 GiB for x64
and 10 to 13 GiB for arm64 [E], about 55 to 65 GiB for the four images of
one week before dedup [E]. The dedup rate between weeks is a measure of
the first runs [E]; ChristopherHX's is 0 [M].

### 3.5 The runner layer

The published snapshot is the fixed hosted image with no podrunner part.
A second image adds one layer on top and shares every other blob:

- the official runner of `images/runner/runner.toml` at
  `/home/runner/actions-runner` (a fixed path for both archs);
- the hooks of `images/runner/Dockerfile` and the credential links into
  `/podrunner/.runner`;
- a runner `.env` made from `/etc/environment` of the image (a JIT runner
  runs no `config.sh`, so nothing else writes it);
- `workFolder` `/home/runner/work`, the hosted path (Findings, PF-1).

The layer is made by appending one tar blob to the manifest and the config
(a new `diff_id`): no build. A new runner pin makes a new runnable digest
in minutes, with no new capture.

### 3.6 Publish

**Names.** `ghcr.io/podutils/podrunner-hosted` (the snapshot) and
`ghcr.io/podutils/podrunner-hosted-runner` (with the runner layer), in the
PodUtils form (C789 of podcrates): attested, pinned by digest.

**Manifests and index.** One OCI manifest for each arch. One index for
each OS version and release date, with the amd64 and arm64 manifests of
the same hosted release date (the two archs have different build
numbers: `ubuntu24/20261111.370` and `ubuntu24-arm64/20261111.182` [M]).

**Tags** (labels for people; podrunner pins digests):

- `ubuntu24-20261111.370.1-amd64`, `ubuntu24-20261111.182.1-arm64`: one
  manifest each, named by `ImageVersion`;
- `ubuntu24-2026-11-11`: the index of that release date;
- `ubuntu24`, `ubuntu26`, `ubuntu-latest`: move with each new snapshot and
  with the label move.

**Annotations** on each manifest: `org.opencontainers.image.version` =
`ImageVersion`; `org.opencontainers.image.revision` = the podrunner
commit; and `ImageOS`, the runs-on label, the runner-images release tag
and commit, `uname -r`, the capture time, the run id and the SHA-256 of
the scrub list (keys under one PodUtils prefix).

**Attestations.** `actions/attest-build-provenance` with
`push-to-registry: true` on each manifest and index digest; an SBOM
attestation from the hosted release's `sbom.ubuntu-24.04.json.zip` (the
official SBOM of that version); the file manifest of pass 3 (path, type,
mode, uid, gid, size, SHA-256, xattrs) as a referrer blob. A user checks
with `gh attestation verify oci://ghcr.io/podutils/podrunner-hosted@DIGEST
--repo PodUtils/podrunner`.

**Notes.** The workflow summary and the release notes of podrunner name
each digest, the `ImageVersion`, the runner-images tag and commit.
`images/hosted/pins.toml` (new) holds, for each label and arch, the pinned
digest and its source `ImageVersion`; it changes only after the
conformance runs are green (section 4).

### 3.7 Cadence and lag

- `hosted-watch.yml`, daily: one 10-second job on each of the four labels
  reads `ImageVersion`; a fifth on `ubuntu-latest` reads `ImageOS`; one job
  reads the releases API and the label table of the runner-images
  README. A version with no snapshot in the registry dispatches the
  capture of that label. A moved `ubuntu-latest` label, or a release newer
  than the pin, opens or updates one issue, which becomes an entry.
- The hosted rollout takes 2 to 3 days (runner-images `README.md` line
  197, first study [M]). The capture records whatever version it got; the
  daily watch catches the new one when it reaches the runner. Target lag:
  one day after the version reaches the hosted fleet [E].
- The pin moves only after conformance; the pin age limits of
  `docs/configuration.md` (`[[pin.payload]] max_age_s` 28 days, first
  study [M]) fit a weekly release.
- Retention: keep each digest that a podrunner release pins and the last 8
  versions of each tag line. A delete of old package versions needs a
  ruling (Questions).

## 4. Conformance tests

### 4.1 The checks

| # | Check | Compares | Catches |
| --- | --- | --- | --- |
| C1 | Manifest 1:1 (pass 3) | the live root of the hosted VM with the merged layers, file by file: type, mode, owner, size, digest, link, xattrs, hard-link groups | any lost file, mode, capability, setuid bit, hard link; a scrub that took too much |
| C2 | Secret scan | each layer stream against the scrub list (forbidden paths) and a pattern scan (trufflehog is in the repository: `images/trufflehog`) | a leak of a new path, as the runner path moved |
| C3 | Two-VM diff (3.1) | two VMs of one `ImageVersion` | new machine or job state |
| C4 | Probe diff | `scripts/hosted/probe.sh` on the hosted label and in the booted unit; `diff.py` with the allow list | tools, versions, `PATH`, environment, users, groups, `sudo -l`, enabled and active units, `is-system-running`, failed units, PID 1, cgroup version and controllers, ulimits of `runner`, the sysctl list, `/dev` nodes (`fuse`, `kvm`, `loop*`, `net/tun`), `lsmod` and `modprobe -n` of the hosted module list, `getcap -r /`, mounts, swap, `binfmt_misc` entries, `docker info`, `podman info`, the docker socket path, hostname form |
| C5 | Software report diff | `pwsh /imagegeneration/SoftwareReport/Generate-SoftwareReport.ps1` in the unit, then `Calculate-ImagesDifference.ps1`, against `/imagegeneration/software-report.json` of the snapshot and `internal.ubuntu24.json` of the release | each tool and version of the hosted report (the image carries both scripts and the report [M]) |
| C6 | Pester | `Invoke-Pester /imagegeneration/tests` in the unit, with an allow list for each `Describe` and `It` (the image carries 11 test files [M]) | a tool that is present but does not run |
| C7 | Behaviour | in the unit as `runner`: `sudo systemctl start postgresql.service` and `pg_isready`; `sudo systemctl start mysql.service` and a query; `docker run hello-world` against `/var/run/docker.sock`; docker-in-docker (`docker run --privileged docker:dind`); rootless `podman run`; `podman run --systemd=always`; `mount -t fuse` by `sshfs` or `fuse-overlayfs`; `ping -c1 127.0.0.1` as `runner` (the capability); `losetup -f` and a loop mount (VM mode); `docker run --platform linux/arm64` after `binfmt` when the hosted runner has it | services, engines, FUSE, caps, loop, binfmt |
| C8 | Real jobs | `scripts/corpus/user-gate.tsv` rows on the snapshot kinds, with new rows for `services:`, `container:`, systemd services and Playwright | what real jobs need |

### 4.2 Where each check runs

| Check | Hosted CI (standard runner) | Live host lane |
| --- | --- | --- |
| C1, C2, C3 | yes, in the capture job: no extra disk | — |
| C2 again on the pushed blobs | yes: a job streams each blob from ghcr.io and scans it (no extraction) | — |
| C4 hosted side | yes: the probe on each label | — |
| Boot of the base slice | yes: the `base` + `boot` layers (a few GiB [E]) as an incus system container with `--podvm-init=image` (podvm T-1993) and as a KVM guest; the hosted runners have `/dev/kvm`, and `runner-image.yml` already runs incus and microvm suites on them (`docs/HANDOFF.md`). It proves the scrub: `is-system-running`, no failed `hosted-compute-agent`, `walinuxagent`, `cloud-init`, the fstab, the network config, machine-id, ssh keys | also |
| Full boot, C4 unit side, C5, C6, C7, C8 | **not proven possible** [E]: the extracted image (55 to 60 GB) plus the store and the instance pool need about three times that; a standard runner has 72 or 145 GB with 54 to 58 GB used (first study [M]). Freeing the preinstalled trees of the test runner and using `/mnt` may reach it on the 145 GB class; a work item measures it. A larger runner (paid) is the other way | yes: the live gates (GitHub #74, #75), level "proven on a constrained host"; incus container, incus VM and machine tier |

### 4.3 Allow rows known now

Container mode: kernel version, modules, the hosted sysctls, swap, `/dev`
loop and kvm nodes, no NIC (egress by the proxy channel), hostname,
`cloud-init status`, `walinuxagent`, `hosted-compute-agent`, `primer`,
`pollinate` failed or masked, the Docker Hub login. VM mode: the same
less kernel, modules, sysctls, loop and swap.

## 5. Why re-mirror instead of using their blobs

**Gains, each from section 2:**

1. No secret material or machine state of the capture VM (section 2.1).
2. The parts they excluded cannot be recovered from their blobs: `/boot`
   (the kernel), xattrs (file capabilities), apt lists, `/var/cache`,
   `/etc/skel`, the 24.04 docker store. A filter over their blobs gives a
   clean but still incomplete image.
3. A kernel and a VM boot: the ruling asks for it.
4. Provenance: the runner-images tag and commit, the official SBOM, an
   attestation, digests in the notes. Theirs have a version in a tag only.
5. Tests: the 1:1 manifest check, the secret scan and the boot run before
   a publish; theirs have none.
6. Cadence: a daily watch and a capture of each version; theirs stopped
   after 2026-09-22 and lag 27 days [M].
7. Multi-arch indexes by release, OCI zstd, layers under the ghcr limits,
   stable layers for dedup.
8. Control of the digests: a pin of podrunner names a digest that
   podrunner made and tested.

**Costs:**

- CI time: their measure is 1 h 06 min to 2 h 32 min per label with
  single-threaded `gzip -9` [M]; ours adds pass 1 (a hash of about 55 GB)
  and pass 3, and gains `zstd -T0`: about 1 to 2 hours per label per
  version [E]. Standard runners are free for a public repository; the
  arm64 labels too. The weekly two-VM diff doubles pass 1 once per
  version.
- Storage: about 55 to 65 GiB of new blobs per week before dedup [E].
  GitHub documents no storage charge for public packages [E: GitHub
  Packages billing page, not read in this study].
- Maintenance: the scrub list follows GitHub's provisioning; C2 and C3 fail
  closed, so a change costs a review, not a leak.

**Fair use of their work.**

- Their method (a tar stream into the registry with no local disk) is
  sound and is the core of ours.
- Their path layering is a good start for ours.
- For a quick local `act` run that needs many tools, their `full-*`
  images work, with the gaps above.
- Their images are a useful second reference: a diff of their file list
  with ours shows what each side excluded.

## 6. The work

Effort: S (a day or less), M (a few days), L (a week or more). "Replaces"
names the item of the first study (`W-n`, entry `T-nnn`).

| # | Work | Files | Test | Effort | Needs | Replaces |
| --- | --- | --- | --- | --- | --- | --- |
| S-1 | Record: T-538 Approach and Prove to the snapshot; T-542 and T-547 replaced by S-4 to S-10; T-548 done by the ruling (its Done section names `docs/decisions.md` line 18); T-541 cut to S-8; this study in `docs/reports/` | `TODO/later.md`, `TODO/PROGRESS.md`, `docs/reports/` | `cargo todo check` | S | — | W-1, W-16, W-17 (T-548) |
| S-2 | The capture: `scripts/hosted/capture.sh` (record, quiesce, bind mount, pass 1 and pass 2), `scripts/hosted/plan.py` (`python3 -I`: the partition, hard-link groups, content keys), `scripts/hosted/push-blob.py` (chunked upload, digest check) | `scripts/hosted/` | a fixture tree with xattrs, a setuid file, a sparse file and a hard link across two planned layers round-trips exactly; a stand-in registry takes a chunked upload and refuses a wrong digest | L | — | W-6 (T-542), W-15 (T-547) |
| S-3 | The scrub: `images/hosted/scrub.toml` and the stream filter; masks and the oneshot `ssh-keygen -A` unit; fstab and netplan rewrites | `images/hosted/`, `scripts/hosted/scrub.py` | a fixture tar with each listed file: each is dropped or rewritten; an unlisted file of a credential pattern fails | M | S-2 | W-10 (no shim) |
| S-4 | The kernel layer from `/boot` and `/boot/efi` | `scripts/hosted/capture.sh` | the layer has `vmlinuz-$(uname -r)` and the initrd; `uname -r` of the record equals the module directory | S | S-2 | — |
| S-5 | `hosted-snapshot.yml`: the four labels, the record, pass 1 to 3 (C1, C2), the two-VM diff (C3) once per version, manifests, index, tags, annotations, attestations, SBOM, summary | `.github/workflows/hosted-snapshot.yml` | a dispatch publishes one version; `gh attestation verify` passes; a planted file `/home/runner/.credentials-test` with a made-up value fails C2 (listed in `.github/secrets-allow.txt`) | M | S-2, S-3, S-4 | W-7 (T-542) |
| S-6 | The watcher: per-label `ImageVersion`, the build-number match to the release tag, the label table, dispatch of S-5, the issue | `.github/workflows/hosted-watch.yml`, `scripts/hosted/watch.py` | a fixture of the releases API where the tag date differs from the version date matches by build number; a moved label fails | S | S-5 | W-4 (T-540, amended) |
| S-7 | The runner layer and `podrunner-hosted-runner`: runner at `/home/runner/actions-runner`, `.env` from `/etc/environment`, `workFolder` `/home/runner/work`, hooks, links | `scripts/hosted/runner-layer.sh`, `images/runner/` | a canned job in the base slice on incus sees `GITHUB_WORKSPACE=/home/runner/work/R/R` and `RUNNER_TOOL_CACHE=/opt/hostedtoolcache` | M | S-5 | W-5 (T-541, the rest) |
| S-8 | The probe and the diff, with the system keys of C4 | `scripts/hosted/probe.sh`, `scripts/hosted/diff.py`, `images/hosted/allow.toml` | fixtures: a removed tool, a lost capability, a failed unit and an unlisted variable each fail; a stale allow row fails | M | — | W-2, W-3 (T-539, extended) |
| S-9 | The base-slice boot in hosted CI: incus system container with the image's init, and a KVM guest with the hosted kernel | `hosted-snapshot.yml` or `runner-image.yml` | `is-system-running` is `running` or `degraded` with only allowed failed units; no unit of Azure or GitHub runs; C4 on the slice | M | S-5; podvm T-1993 for the container; the VM ask for the guest | — |
| S-10 | The full lane: pull by digest, boot in each mode, C4 to C8 | a live workflow, `scripts/hosted/` | green at the level "proven on a constrained host" | L | live gates (#74, #75); podvm asks | W-15 (T-547) |
| S-11 | Software report and Pester from the image itself | `scripts/hosted/report.sh`, `scripts/hosted/pester.sh`, `images/hosted/pester-allow.toml` | a removed tool fails its `Describe` and its report row | M | S-10 | W-14 (T-546, changed: tests of the image, not of a checkout) |
| S-12 | Pins and placement: `images/hosted/pins.toml`; `[[pin.payload]]` by label; a systemd payload only to a kind that boots its init; a pin larger than the disk bound of its kind refused | `crates/podrunner-schema`, placement code, `docs/configuration.md` | a test: a `ubuntu-latest` job with the snapshot never goes to the podvm userland kind; a 60 GB pin on a 40 GB kind is refused at `check` | M | podcrates `init` fact (first study) | W-18 (T-549), T-551 |
| S-13 | Measure: can a standard hosted runner (145 GB class, `/mnt`) hold a full boot after freeing its own trees | a dispatch-only workflow | the measure, recorded | S | S-5 | — |
| S-14 | The corpus: `ImageVersion` of each github.com result; new rows (services, systemd, Playwright, podman) on the snapshot kinds | `scripts/corpus/user-gate.tsv`, `scripts/user-gate.py` | `user-gate.yml` | S | S-10 | W-19 (T-550) |
| S-15 | Docs: `docs/hosted-snapshot.md` (what is 1:1, the scrub list, the allow rows, the verify command) | `docs/` | `check-design.sh` | S | S-5 | — |

Dropped from the first study: W-10 (the `systemctl` shim: systemd is real
in the snapshot modes), W-16 (the licence call: the ruling), W-20 (arm64:
in the capture matrix). Kept as they are: W-11 (T-543) and W-12 (T-544),
with the asks below added; W-13 (T-545) at a lower priority (the podvm
userland kind cannot run the snapshot's systemd).

**Order.** S-1; S-8 (it also measures the image of today); S-2, S-3, S-4;
S-5 and S-6; S-7; S-9; S-13; send the COORD issues at once; S-12; S-10,
S-11, S-14 on the podvm answers and the live lane; S-15 with S-5.

### 6.1 COORD issues to write

podrunner does not edit the other repositories.

1. **(to podvm) amend COORD-200 / T-1993**, for the snapshot on the incus
   tier: an id map of at least 231,072 ids (the hosted `runner` has
   subordinate ids 165536:65536 [M], so rootless podman needs ids up to
   231071); `security.nesting` for the image's own `docker.service`;
   `/dev/fuse` in the instance; a fresh `machine-id` per instance; the
   import of a 55 to 60 GB image with no third copy (today: the store, the
   extracted tree, the unified tarball, the pool).
2. **(to podvm) amend T-1994**: boot the image's own kernel and initrd
   (`/boot/vmlinuz-KVER`, `/boot/initrd.img-KVER`) with
   `root=LABEL=cloudimg-rootfs`, as a choice beside the Alpine kernel; load
   the 9p and vsock modules from the image (`/usr/lib/modules`); pass
   nested KVM where the host has it.
3. **(to podvm) new: an incus VM mode** that turns an OCI image with a
   `/boot` into a VM image once per digest (qcow2 root, GPT with the ESP
   from `/boot/efi`, or a direct kernel boot), as incus runs OCI images only
   as containers (lxc/incus#1360).
4. **(to podvm and podcrates) xattrs**: keep the PAX `SCHILY.xattr.*`
   records (at least `security.capability`) through `pod-extract`, the
   sidecar and the unified tarball; today `crates/pod-extract/src/apply.rs`
   has no xattr path.
5. **(to podcrates) readers**: `pod-extract` reads one gzip member
   (`GzDecoder`, `crates/pod-extract/src/layer.rs` line 102); a valid layer
   of many members (eStargz, bgzip) would be cut short with no error [E:
   needs a test]. Ask for `MultiGzDecoder` and a test with a multi-frame
   zstd layer for `ruzstd`.

Not a COORD issue: githome's unit disk. `disk_max_bytes` is 40 GB per
unit on githome's host (`docs/HANDOFF.md`, "Facts of that host"); the
snapshot does not fit. Ask on GitHub #151 for 80 GB or more, or a
copy-on-write pool where only the writes count.

## 7. Findings

Flaws and gaps found in this study. None is fixed here.

- **PF-1 (P2).** The work folder of a unit is `_work` under the runner root
  (`crates/podrunner-local/src/exec.rs` line 323 mounts
  `{run_root}/_work`; the fake broker's `.runner` has `"workFolder":
  "_work"`, `crates/podrunner-broker-fake/src/files.rs` line 15). The
  hosted runner uses `/home/runner/work` (the hosted `.runner` has a
  17-character `WorkFolder` [M]; GitHub documents
  `/home/runner/work/REPO/REPO` as an example of `GITHUB_WORKSPACE` [E: not
  read in this study]). A script that names `/home/runner/work` fails in a
  unit. S-7.
- **PF-2 (P2, the first study).** `docs/reports/hosted-parity-2026-11-19.md`
  line 136 says the snapshot leaves out "the runner". It does not any
  more: the runner is in the base layer [M]. The
  same report's licence sections (0.6, 3.5, W-16) conflict with the ruling
  of 2026-11-19. A note in that report, or a pointer to this study, fixes
  the record.
- **PF-3 (P3, the first study).** Its release row (line 71) reads as if a
  release tag carries the `ImageVersion` date. The tag date can differ
  (`20261012.334.1` is the release `ubuntu24/20261023.334` [M]). T-540
  must match by the build number. S-6.
- **PF-4 (P2, T-551).** The snapshot (about 55 to 60 GB on disk [E]) is
  larger than githome's unit disk of 40 GB. T-551 (a pin larger than the
  disk bound is not refused) becomes a blocker of the snapshot on that
  host; raise its priority.
- **PF-5 (P3, T-543).** With the image's own systemd, the incus `SCRIPT`
  (`crates/podrunner-incus/src/guest.rs` lines 41 to 55) still starts its
  own `dockerd` on `/run/podrunner-docker.sock` and sets `DOCKER_HOST`
  when the engine directory is mounted. T-543 names this ("no manual
  `dockerd` when the image has `docker.service`"); the snapshot makes it
  required, not optional.
- **PF-6 (podcrates, P3).** The single-member gzip reader of 6.1 item 5.
- **PF-7 (third party).** Held back: a disclosure to the author of the
  community snapshot. The owner decides it.

## 8. Questions for the owner

- Retention of old package versions on ghcr.io needs a delete of package
  versions. `AGENTS.md` forbids a delete of a tag or a release; a package
  version is neither. Recommendation: keep each pinned digest and the last
  8 versions of each tag line, delete the rest by a workflow.
- The Docker Hub credential of the hosted image is a GitHub credential.
  Recommendation: drop it (an allow row: anonymous pulls), and offer an
  operator setting for a mirror or a login.

## 9. Sources

- ChristopherHX/runner-image-blobs at `2fa1775f4dc9`:
  `.github/workflows/dump.yml`, `check-composite-up-to-date.mjs`,
  `README.md`; its commits and its Actions runs and jobs (run
  39964244981), read through forge.web.pkgforge.dev.
- catthehacker/docker_images at `b84b223032d1`:
  `.github/workflows/copy-full-image.yml`, `README.md`.
- ghcr.io, read anonymously on 2026-11-19: the manifests, configs and tag
  list of `christopherhx/runner-images` and `catthehacker/ubuntu`; the
  streamed headers of layers 0, 2 and 16 of
  `ubuntu24-runner-large-latest-amd64`, layer 0 of
  `ubuntu26-runner-large-latest-amd64` and of
  `ubuntu24-runner-large-latest-arm64`; `modules.builtin` of the three
  kernels; `/etc/passwd`, `/etc/group`, `/etc/systemd/system.conf`.
- actions/runner-images: the releases API (tags, commits, assets);
  `images/ubuntu/scripts/build/cleanup.sh`, `configure-system.sh`;
  `images/ubuntu/assets/post-gen/*.sh` at `fb099341ddaf`.
- git.launchpad.net `ubuntu/+source/iputils` and `mtr`, branch
  `ubuntu/noble`: `debian/iputils-ping.postinst`,
  `debian/mtr-tiny.postinst`.
- Incus documentation: "Image format", "How to create instances", "How to
  use remote images"; lxc/incus#1360.
- docs.github.com: "Working with the Container registry" (the limits).
- PodUtils/podvm at `af29221f1128`: `docs/incus.md`,
  `crates/podvm/src/machine/guest/kernel.rs`; PodUtils/podvm#596
  (COORD-200).
- PodUtils/podcrates at `b2a837a61e9a`: `crates/pod-extract/src/layer.rs`,
  `apply.rs`, `sidecar.rs`.
- podrunner at `fceac4d`: the files named in each row;
  `docs/reports/hosted-parity-2026-11-19.md`; `TODO/later.md` T-538 to
  T-553.
