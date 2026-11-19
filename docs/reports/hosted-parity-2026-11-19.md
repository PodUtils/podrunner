# Hosted parity: the study (T-538)

The work items W-1 to W-20 of section 5 are the entries T-538 to T-551 of
the record (`TODO/later.md`); `TODO/issues.md` maps the COORD issues.

The owner's ruling "The hosted snapshot" (2026-11-19, `docs/decisions.md`)
replaces the build plan of this study (W-6, W-10, W-15) and its licence
text (section 3.5, W-16): podrunner copies the root of a hosted runner 1:1.
`docs/reports/hosted-snapshot-2026-11-19.md` gives that pipeline. Its
finding: the community snapshot now holds the runner too (its exclusion of
`/home/runner/runners` is stale), unlike the row of section 1 below.

The question: can a unit of podrunner give the toolset of the GitHub-hosted
`ubuntu-latest` runner, with systemd, and can CI check the parity
mechanically? This study gives the facts, the limits and the work.

Read-only study, 2026-11-19. `main` at `8ce5dd7`. No file of the repository
changed.

Marks in this report:

- **[M]** measured: the source is a URL, a file and line, or a command
  output of this study.
- **[E]** estimate: no measure exists. A work item measures it.
- **[N]** not found: the study looked and found no figure.

## 0. The answer in short

1. The source of truth is `actions/runner-images` (MIT). Its Ubuntu image
   is a Packer build on Azure or AWS VMs. Most of its install scripts are
   plain bash and run in an OCI build with small changes. GitHub itself
   builds `ubuntu-slim` from a Dockerfile with the same script pattern.
2. **The target moves today.** `ubuntu-latest` changes from Ubuntu 24.04
   to Ubuntu 26.04. The migration started 2026-10-19 and ends 2026-11-19
   (runner-images#14748). The plan must follow the label, not a fixed OS.
3. The full hosted image uses 54 to 58 GB of `/` at job start [M], and a
   snapshot of it is 17.78 GiB compressed in 18 layers [M]. It does not fit
   a release asset, a microvm initramfs, or a standard GitHub-hosted runner
   as a build host or as a unit host. ghcr.io holds it (the owner's
   ruling).
4. systemd as PID 1 is possible on the `incus` kind and on the `microvm`
   kind, with changes in podvm. It is **not possible** on the podvm
   userland rung. On the podvm namespace rung it is limited: a user
   namespace of one uid cannot run a service that changes to its own user
   (mysql, postgres).
5. A mechanical check is possible now: a probe job on github.com and the
   same probe in a unit, a diff with an allow list, the software report of
   each image release as the expected list, and the Pester tests of
   runner-images run in the unit.
6. Licence: the hosted image holds parts that a public image must not
   redistribute. The Android SDK licence forbids redistribution (clause
   3.4) [M]. Other parts need a check. A published image must leave them
   out, and the host adds them.
7. The smallest first step with the most value for githome and cfb: the
   probe and diff on the current image (report-only), then a
   "hosted-core" image built from the runner-images scripts (toolcache,
   environment, Node, Python, Go, docker CLI, Playwright libraries), on
   ghcr.io, run on the `incus` kind first, with a `systemctl` shim until
   podvm gives systemd as PID 1.

## 1. The hosted image: the source of truth

### 1.1 Facts

| Fact | Value | Source |
| --- | --- | --- |
| Repository, licence | `actions/runner-images`, MIT ("Copyright (c) 2026 GitHub") | `LICENSE` of the repository [M] |
| Label map | `ubuntu-latest` or `ubuntu-24.04` = Ubuntu 24.04 in the README table today; `ubuntu-26.04`; `ubuntu-slim` (container) | runner-images `README.md` lines 23 to 29 [M] |
| Label move | "`ubuntu-latest` label will use Ubuntu 26.04 in November 2026": "rolled out over a period of several weeks beginning October 19, 2026. We plan to complete the migration by November 19, 2026." | runner-images#14748 [M] |
| Templates | `images/ubuntu/templates/build.ubuntu-24_04.pkr.hcl` (and `-26_04`, arm64 variants); `source.ubuntu.pkr.hcl` has two builders: `azure-arm` and `amazon-ebs` | runner-images `main` [M] |
| Build VM | `Standard_D4s_v4`; OS disk 75 GB for each Ubuntu image | `variable.ubuntu.pkr.hcl`, `locals.ubuntu.pkr.hcl` [M] |
| Base | the Ubuntu **server cloud image** (`canonical:ubuntu-24_04-lts:server`, AMI `ubuntu-noble-24.04-amd64-server-*`), not the Docker `ubuntu:24.04` | `locals.ubuntu.pkr.hcl` [M] |
| Steps | about 60 install scripts, then `install-docker.sh`, `Install-Toolset.ps1`, `Configure-Toolset.ps1`, pipx, Homebrew, snap, a **reboot**, `cleanup.sh`, the software report, the Pester tests, `configure-system.sh`, `post-build-validation.sh`, waagent deprovision | `build.ubuntu-24_04.pkr.hcl` [M] |
| Toolset | `toolset-2404.json`, `toolset-2604.json` (+ arm64): toolcache (Python 3.11 to 3.15, PyPy, Node 22/24/26, Go 1.25 to 1.27, Ruby, CodeQL), apt lists (9 vital, 31 common, 34 cmd), docker, java, android, dotnet, clang, gcc, php, node modules, postgresql, cmake, pwsh | toolset JSON [M] |
| Versions float | the toolset uses wildcards (`3.12.*`) and `latest` (containerd, buildx) | toolset JSON [M] |
| Reproducible build | no: "Current builds are not idempotent and if one tries to build a runner image using the specific tag it is not guaranteed that the build will succeed." | `README.md` line 229 [M] |
| Apt pin | optional `apt_snapshot_timestamp` → `APT::Snapshot` (24.04, 26.04 x64) | `configure-apt-sources.sh` line 41; `docs/create-image-and-azure-resources.md` line 107 [M] |
| Release | one GitHub release for each image and build, tag `ubuntu24/YYYYMMDD.N` (points to a commit, e.g. `ubuntu24/20261111.370` = `fb099341ddaf`), assets `internal.ubuntu24.json` (the software report as JSON, ~30 KB) and `sbom.ubuntu-24.04.json.zip` (~30 MB) | releases API [M] |
| Cadence | "We typically deploy weekly updates"; releases of ubuntu24 on 10-23, 10-26, 11-02, 11-11, 11-17 (pre-release) | `README.md` line 112; releases API [M] |
| Deploy lag | "image deployment takes 2-3 days" | `README.md` line 197 [M] |
| Current 24.04 | image `20261111.370.1`, systemd `255.4-1ubuntu8.18`, kernel `7.0.0-1017-azure` | `Ubuntu2404-Readme.md` [M] |
| Current 26.04 | image `20261111.196.1`, systemd `259.9-0ubuntu0.1` | release body of `ubuntu26/20261111.196` [M] |
| Software report | `internal.ubuntu24.json`: a tree of 27 header nodes, 97 tool versions, 16 version lists, 3 notes, 1 table; the Markdown report adds the apt table (74 rows) | this study's parse of the asset [M] |

### 1.2 Size and build time

| Figure | Value | Source |
| --- | --- | --- |
| `/` used at job start, ubuntu-24.04 | 54 GB (72 GB disk) to 58 GB (145 GB disk) | `df -h /` in runner-images#14492 [M, third party] |
| Snapshot of a live runner, ubuntu24 amd64 | 18 layers, 17.78 GiB compressed, largest layer 4.10 GiB | manifest of `ghcr.io/christopherhx/runner-images:ubuntu24-runner-large-latest-amd64` [M] |
| Same snapshot, extracted | "~60GB extracted" | `catthehacker/docker_images` README [claim, not measured] |
| catthehacker `act-24.04` (medium) | 6 layers, 0.54 GiB compressed | ghcr manifest [M] |
| podrunner `runner-full` archive | 462 MiB | `TODO/later.md` line 1480 (run 43609196605) [M] |
| podrunner runner image as initrd | 892 MiB to 1348 MiB | `TODO/later.md` lines 5231, 10950 [M] |
| Build time of the Packer image | **not found** [N]. The template allows 900 polls of 15 s for AWS and pauses 5 minutes before cleanup; these are limits, not a duration. | `source.ubuntu.pkr.hcl` [M] |

### 1.3 Parts that need Azure, AWS or Packer

- The builders `azure-arm` and `amazon-ebs`, the Shared Image Gallery and
  the AMI tags.
- `configure-apt-mock.sh` (a workaround for an Azure Linux extension).
- `configure-environment.sh`: waagent swap on `/mnt` (4096 MB), a
  cloud-init file for the Azure resource disk.
- The last step: `waagent -force -deprovision+user`.
- The reboot step and the 5-minute pause.
- The runner provisioner (`runner-provisioner` in `needrestart.conf`) is
  GitHub's service. Its source is not public. It starts the runner on a
  hosted VM.
- The image data file `/imagegeneration/imagedata.json` and the variables
  `ImageOS` and `ImageVersion` come from the build variables.

Each other part is bash or PowerShell that runs as root in the VM.

### 1.4 Do the scripts run in an OCI build?

Mostly yes. GitHub's own `images/ubuntu-slim/Dockerfile` runs the same
kind of scripts in `RUN` steps. It does `touch /run/.containerenv`, takes
a GitHub token as a build secret (`--mount=type=secret,id=api_pat`), and
uses `/opt/entrypoint.sh` to load `/etc/environment` [M].

What blocks the Ubuntu (full) scripts in an OCI build (census of the 84
files of `images/ubuntu/scripts/build` at `main`) [M]:

| Blocker | Where | Fix in a build |
| --- | --- | --- |
| `systemctl` (16 calls in 9 files) | `configure-apt.sh`, `configure-environment.sh`, `configure-snap.sh`, `install-apache.sh`, `install-docker.sh`, `install-mysql.sh`, `install-nginx.sh`, `install-postgresql.sh`, `post-build-validation.sh` | a build-time `systemctl` that records `enable`, `disable` and `mask` as links (as `systemctl --root` does) and does nothing for `start`/`stop`; remove it at the end |
| The docker daemon starts in the build, then `docker info` and 6 `docker pull` of `:latest` images (dependabot, gh-aw, github-mcp-server) | `install-docker.sh` lines 85 to 102 | skip the start and the pulls; the pulled images are not in the parity scope (the snapshot also leaves out `/var/lib/docker`) |
| `snapd` restart and `snap set` | `configure-snap.sh` | skip; no script runs `snap install` |
| The reboot | the template | not needed in a build |
| `invoke_tests` (Pester) at the end of 64 scripts | `helpers/invoke-tests.sh` | the service tests need systemd; make `invoke_tests` a no-op in the build and run the tests in the unit (section 4) |
| GitHub API (22 scripts) | `helpers/install.sh` (`resolve_github_release_asset_url`, no auth) | a token as a build secret, as `ubuntu-slim` does |
| `sysctl.conf` lines (`vm.max_map_count`, inotify limits, `vm.mmap_rnd_bits`) | `configure-environment.sh` lines 50 to 57 | written, but a shared kernel (podvm, incus) does not take them; a microvm guest can |
| Floating versions | toolset wildcards, `latest` | pin: the commit of the release tag, `APT_SNAPSHOT_TIMESTAMP` of the release date, and the exact toolcache versions of `internal.ubuntu24.json` patched into the toolset |
| The base | cloud image versus Docker `ubuntu:24.04` | measure the dpkg diff (the probe, section 4); the cloud rootfs tarball of cloud-images.ubuntu.com is a closer base [E] |

### 1.5 What other projects do

| Project | Method | What it keeps, what it drops |
| --- | --- | --- |
| GitHub `ubuntu-slim` | Dockerfile, official, 24.04, a `test.sh` | a small set (git, gh, node 24, python, pwsh, az, aws, gcloud, docker CLI, yq); no compilers, no toolcache, no services, no systemd |
| catthehacker `act-*`, `runner-*` | its own script (`linux/ubuntu/scripts/act.sh`), MIT | base tools, git, git-lfs, moby CLI and engine packages, Node in the tool cache, yq; drops systemd, the toolcache of Python and Go, compilers, databases, browsers; no daemon runs |
| catthehacker `full-*` | a copy of the ChristopherHX snapshot | the same image as below (same manifest) |
| ChristopherHX `runner-image-blobs` | weekly: a job on the hosted runner streams `tar` of its own rootfs into ghcr.io, 18 layers ("GitHub Hosted Runners do not have enough space for docker build / pull itself") | nearly all; leaves out `/var/lib/docker`, `/home/runner/work`, the runner, `/tmp`, `/mnt`, `/boot`, swap; its README: "this cannot be used inside GitHub Actions hosted runners, you need a self-hosted runner" |
| Kong/runner-images-kvm, wydler/runner-images-hetzner-cloud | forks with other Packer builders | VM images, not OCI |

The snapshot is exact by construction. It also copies the parts with a
restricted licence (section 3.5) and the state of a live VM. podrunner
does not take that route; it is a reference only.

## 2. systemd

### 2.1 What depends on systemd in the hosted image

| Item | State at job start | Source |
| --- | --- | --- |
| `docker.service` | enabled and running; `/run/docker.sock` link by `/etc/tmpfiles.d/docker.conf` | `install-docker.sh` lines 76 to 86 [M] |
| `mysql.service` | installed, disabled; the report says "Use ... 'sudo systemctl start mysql.service'", user root, password root | `install-mysql.sh`; `Ubuntu2404-Readme.md` [M] |
| `postgresql.service` | installed, disabled; "'sudo systemctl start postgresql.service'" | `install-postgresql.sh`; report [M] |
| `apache2`, `nginx` | installed, disabled (report: `inactive`) | scripts; report [M] |
| apt timers, `fwupd-refresh.timer`, `tmp.mount` (26.04) | disabled or masked | `configure-apt.sh`, `configure-environment.sh` [M] |
| The Pester tests | `sudo systemctl start postgresql`, `pg_isready`, `sudo systemctl start mysql` | `scripts/tests/Databases.Tests.ps1` [M] |
| `DefaultLimitNOFILE=65536`, `DefaultLimitSTACK=16M` | in `/etc/systemd/system.conf`: they hold only for processes under systemd | `configure-limits.sh` [M] |
| The runner | started by GitHub's provisioner service (not public) | `configure-system.sh` (`needrestart` rule) [M] |

User scripts that assume systemd: `sudo systemctl start mysql.service`,
`... postgresql.service`, `systemctl is-active docker`, `journalctl`, and a
`docker` socket at `/var/run/docker.sock`.

### 2.2 Each unit kind

| Kind | Can systemd be PID 1? | Why | Who must change |
| --- | --- | --- | --- |
| `podvm`, userland rung | **No.** | No root, no namespaces: podvm rewrites paths (`crates/podrunner-podvm/src/probe.rs` line 4: "Each podvm rung is `redirect`: path rewriting, no isolation"). No PID 1, no mounts, no cgroups. `sudo` fails there (`docs/local-mode.md` lines 58 to 62). | Nobody can. This is a hard limit. |
| `podvm`, namespace rung | **Limited.** | A PID namespace of `run -d` exists (podvm beta.30, T-318). systemd needs a writable cgroup subtree (delegation), tmpfs on `/run`, and mounts, as rootless podman's `--systemd=always`. The rung maps one uid: a service that drops to its own user (mysql, postgres, `_apt`) cannot (`images/runner/Dockerfile` lines 35 to 39 name this for `_apt`). | podvm: a subordinate uid range (needs `/etc/subuid` set by root once) and a systemd mode. Low value: use `incus`. |
| `microvm` (machine tier) | **Yes, with podvm changes.** | The guest is a real kernel. Today the root is an initramfs that "must fit in the guest memory twice" (`scripts/measure-machine.sh` line 11), so a 55 GB image cannot boot. podvm's own init runs the command. | podvm: a root on a disk (an image file for each digest, a copy-on-write overlay for each guest), a switch to the image's `/sbin/init`, a kernel with what systemd needs. |
| `incus` (system container) | **Yes, with a podvm change.** | systemd in an unprivileged incus container is the normal case for incus. Today podvm sets `raw.lxc lxc.init.cmd = /.podvm/podvm guest-hold` (podvm `docs/incus.md` lines 98 to 103), so podvm is PID 1 and its init relays the channel (`docs/limits.md`, "the incus tier"). | podvm: an option to boot the image's init, to run the channel relay as its own process, to wait for the boot, to run the command by the exec API, and to stop the instance at its end. |

### 2.3 How the runner starts under systemd

Two forms. Both keep the credential files of today.

1. **The exec form (incus).** systemd boots as PID 1. podvm waits for
   `systemctl is-system-running --wait` (`running` or `degraded`). Then the
   current script of `crates/podrunner-incus/src/guest.rs` (`SCRIPT`) runs
   by the exec API, as now: it writes the credential files from stdin
   into the runner root at mode 0600, and runs `run.sh` as `runner` with
   `setpriv`. Change: no manual `dockerd` when the image has
   `docker.service`; the job uses `/var/run/docker.sock`. The instance
   stops when the command ends. This form needs the least change in
   podrunner.
2. **The unit form (microvm, where no exec API exists).** The image holds
   `podrunner-job.service`: `Type=exec`, `User=runner`,
   `WorkingDirectory=/home/runner`, `ExecStartPre=+` a root step that
   copies the credential files from the read-only 9p mount (`CRED_MOUNT`)
   into the runner root at mode 0600, `ExecStart=/home/runner/run.sh`,
   `SuccessAction=exit-force`, `FailureAction=exit-force`. The exit status
   of the job must reach podvm (in a VM: through the serial port or a file
   on the 9p work mount). The unit is enabled only by a kernel argument
   or a file that podvm gives, so the image also boots for other uses.

### 2.4 A fallback with no systemd

A `systemctl` shim (0BSD, `/usr/local/bin/systemctl`) for kinds with no
systemd as PID 1. When `/run/systemd/system` exists, it runs the real
`systemctl`. Else it maps `start`, `stop`, `status`, `is-active` and
`enable` of `docker`, `mysql`, `postgresql`, `nginx` and `apache2` to the
commands of each service (`dockerd`, `mysqld_safe`, `pg_ctlcluster`,
`nginx`, `apachectl`), and gives a clear error for each other unit.
`docker-systemctl-replacement` does this too, but its licence is EUPL-1.2
[M]; write our own.

The shim needs root for the services. So it helps `incus` and `microvm`
now. It does not help the podvm kind (no root there).

## 3. Size and distribution (ghcr.io)

### 3.1 Image tiers

| Tier | Content | Size | Fits hosted CI? |
| --- | --- | --- | --- |
| `runner` (today) | Ubuntu 24.04, a small apt set, the runner | `runner-full` archive 462 MiB [M] | yes |
| `hosted-core` (new) | the runner-images scripts of the toolcache (Python, Node, Go, Ruby, PyPy), apt vital and common, git, gh, git-lfs, pwsh, nvm, pipx, yq, zstd, cmake, gcc, docker (no daemon start), Firefox and the browser libraries, the environment of `/etc/environment` | **not measured** [N]; a few GiB [E] | yes [E] |
| `hosted-full` (new) | each script of the template, less the restricted parts | ~18 GiB compressed, 54 to 58 GB on disk (the snapshot) [M as a bound] | **no**: a standard runner has 72 or 145 GB of disk with 54 to 58 GB used at start (#14492, #15238 [M]); the build, the store and the unit need about 3 times the image |

### 3.2 The distribution: ghcr.io

The owner's ruling (2026-11-19, `docs/decisions.md` line 18): the large
images go to ghcr.io. The plan:

- Names (ghcr needs lower case): `ghcr.io/podutils/podrunner-runner`
  (the image of today) and `ghcr.io/podutils/podrunner-hosted`, tags
  `ubuntu24-20261111.370-<short commit>`, `ubuntu26-...`. A tag is a
  label for people. Each pin is a digest.
- Push: the workflow that builds the image pushes it with
  `permissions: packages: write`, signs the digest with cosign (as
  `release.yml` signs `SHA256SUMS`, T-484), and writes the digest into the
  summary. `release.yml` names the digests in `podrunner-manifest.json`.
- Pull: `podvm pull --platform P ghcr.io/podutils/podrunner-hosted@sha256:D`
  into the operator store. The podvm kind already pulls by digest when the
  store does not have the image (`crates/podrunner-podvm/src/unit.rs`
  lines 228 to 235). The installer pulls with podvm, after it has podvm.
- No oras: each image is a plain OCI image.
- The release asset limit of 2 GiB no longer holds for images. The offline
  bundle keeps `podvm save` archives of the small image only.
- The pin age: `[pin] max_age_s` 30 days and `[[pin.payload]] max_age_s`
  28 days (`docs/configuration.md` lines 86 to 95) fit a weekly or
  fortnightly rebuild.

What podrunner pulls from ghcr.io when the other projects publish there
(COORD-102, GitHub #312):

| Item | Today | With ghcr.io |
| --- | --- | --- |
| podvm | `images/podvm.toml`: `version` and `sha256[arch]`; URL of the GitHub release | `reference = "ghcr.io/podutils/podvm@sha256:INDEX"` and the SHA-256 of each binary; the installer gets the one-layer image with `curl` (anonymous token, manifest, blob by digest, `sha256sum -c`, `tar -x`): no new tool |
| podnet | `images/podnet.toml`, the same | the same form |
| runner images | `runner-ARCH.tar` release assets | `ghcr.io/podutils/podrunner-runner@sha256:D` (and the tar in the offline bundle) |
| hosted images | none | `ghcr.io/podutils/podrunner-hosted@sha256:D` for each OS and arch |
| the fakes (T-511) | planned as release assets | an image each, if they are binaries |
| official `actions/runner` tarball | `images/runner/runner.toml`, GitHub release | stays inside the images; `--runner-dir` keeps the GitHub release |
| podcrates | git tag in `Cargo.toml` | no change (source, not binaries) |

### 3.3 Effects on each kind and store

| Kind | Effect of a 55 GB image | Source |
| --- | --- | --- |
| `podvm` | **Not usable as it is.** Each unit gets its own store, `podvm load -i` of the tar, `podvm extract`, and podvm copies the tree for each container: about three times the image in writes for each job. | `crates/podrunner-podvm/src/unit.rs` lines 248 to 255; podvm `docs/limits.md` (copy or `FICLONE`, spare tree) [M] |
| `microvm` | **Not usable**: the root is an initramfs that must fit twice in guest memory. | `scripts/measure-machine.sh` line 11; QEMU refused 888 MB at 4 GiB (`TODO/level2.md` line 194) [M] |
| `incus` | **Usable with a CoW pool.** podvm imports a unified tarball once for each digest (`podvm-HEX16-vN`). On the `dir` backend each instance unpacks the full image [E]; on btrfs, ZFS or LVM thin an instance is a clone [E]. The root disk size comes from `limits.disk_bytes` (`--storage-opt size=`, `crates/podrunner-incus/src/guest.rs` line 115): githome gives 40 GB per unit (`docs/HANDOFF.md`, "Facts of that host"), less than the image. | podvm `docs/incus.md`; HANDOFF [M] |

Disk on a host for one version of `hosted-full` [E]: about 18 GB of
blobs, about 55 GB extracted in the podvm store, about 55 GB more in the
incus pool, so 130 GB or more; twice that during a change of version.

Install time [E]: 18 GiB at 100 MB/s is about 3 minutes; at 10 MB/s about
30 minutes; then the extraction and the incus import. No measure exists.

### 3.4 The rule "everything static"

The images are user content (ruling C4, 2026-11-11, `docs/decisions.md`
line 80): the runner payloads are not podrunner's binaries. The rule holds
for `podrunner`, the sidecar, podvm and podnet. The installer stays POSIX
`sh` with `curl`, `tar` and `sha256sum`. The user still downloads and uses:
one command, a longer download.

### 3.5 Licence

- The Android SDK licence, clause 3.4: "you may not copy (except for
  backup purposes), modify, adapt, redistribute ... the SDK or any part of
  the SDK" [M, developer.android.com/studio/terms]. The hosted image holds
  it at `/usr/local/lib/android/sdk`.
- To check before a public push: Google Chrome, Microsoft Edge, the CodeQL
  bundle, the Copilot CLI, and each other part with a vendor licence.
- The call: the public image leaves these parts out. A host step adds them
  from the vendor (`podrunner image complete`, a local layer), so the
  operator accepts each licence. `THIRD_PARTY.md` lists each component of
  the published image (the runner-images scripts are MIT).

### 3.6 Not chosen

Split release assets of 2 GiB, an incus image (unified tarball or
squashfs) as a release asset, oras artifacts, and a build on each host as
the main path. The owner chose ghcr.io. A build from the pinned recipe
stays possible for an operator (cfb's I4, `podrunner image build`, T-500).

## 4. The mechanical parity check

### 4.1 Four methods

| Method | Catches | Misses |
| --- | --- | --- |
| (a) A probe job on github.com and the same probe in a unit, diffed | a missing tool, a version drift, `PATH` and environment, users, groups, `sudo`, enabled and active services, PID 1, paths (`GITHUB_WORKSPACE`, `HOME`, the docker socket), ulimits, sysctls; it tells which image `ubuntu-latest` is today (`ImageOS`, `ImageVersion`) | a tool that is present but fails in the unit; network differences; speed |
| (b) The software report of the release (`internal.ubuntuNN.json`) as the expected list; our image makes its own report with the runner-images `Generate-SoftwareReport.ps1`; their `Calculate-ImagesDifference.ps1` diffs two reports | each tool and version in the report, for an exact pinned release, with no github.com job; the same format both sides | environment, services, paths, the apt packages outside the table of 74 (the SBOM has the full list) |
| (c) The Pester tests of runner-images (`scripts/tests/*.Tests.ps1`, `RunAll-Tests.ps1`) of the pinned commit, run in the unit | behaviour: a tool runs, `sudo systemctl start postgresql` and `pg_isready`, `sudo systemctl start mysql` and a query, the docker client and server versions match | what the tests do not test; tests tied to the build VM need allow rows |
| (d) The user-path gate corpus (`scripts/corpus/user-gate.tsv`) | what real jobs need | breadth; and the `github` column records no image version, so a result of 24.04 is compared with 26.04 after the label move |

Use all four. (a) and (b) are cheap and run on each change. (c) runs
where the image runs. (d) stays the user-level proof.

### 4.2 The CI shape

A new workflow `hosted-parity.yml`, on a change of `images/hosted/**` or
`scripts/hosted/**`, on a daily schedule, and by dispatch:

1. Job `reference`, matrix `runs-on: [ubuntu-24.04, ubuntu-26.04,
   ubuntu-latest]`: runs `scripts/hosted/probe.sh`, uploads
   `probe-LABEL.json`, writes `ImageOS` and `ImageVersion` to the summary.
2. Job `watch`: reads the newest non-pre-release tags `ubuntu24/*` and
   `ubuntu26/*` and the README label table of runner-images; compares them
   with `images/hosted/ubuntu24.toml` and `ubuntu26.toml` (the `release`
   tag, the commit, the label). A newer release or a moved label fails the
   job, and the job opens or updates one issue ("hosted image
   ubuntu24/NNN is out"); `issue-sync.yml` and the record make it an
   entry.
3. Job `report`: diffs the pinned `internal.ubuntuNN.json` with the
   report of our image (method b).
4. Job `units` (hosted runners: `hosted-core` only): the same probe as a
   job through `podrunner run --platform ubuntu-latest=IMAGE@DIGEST` on
   each kind that runs on a hosted runner (incus first, then microvm,
   podvm with an allow list for no root); the Pester lane (method c).
5. Job `diff`: `scripts/hosted/diff.py` compares each unit probe with the
   reference of the same label. Each difference must have a row in
   `images/hosted/allow.toml` (key, kind, reason, entry `T-NNN`). An
   unlisted difference fails. A row that no longer matches fails too, so
   the list shrinks.

The version precision comes from the toolset: an exact version in the
toolset is compared exactly; a wildcard `3.12.*` is compared at
major.minor; the toolcache list of the report is compared exactly when our
build pins it.

`release.yml` needs a green `hosted-parity.yml` on its commit, as it needs
`runner-image.yml`. A red daily run with no commit means that the hosted
side moved: the `watch` issue is the work item.

`hosted-full` cannot run on a standard hosted runner (section 3.1). Its
lane is a live lane: a self-hosted host of the live gates (GitHub #74,
#75) that runs `serve` with the incus kind, at the level "proven on a
constrained host". The reference side still runs on github.com.

## 5. The work

Effort: S (a day or less), M (a few days), L (a week or more).
Status: **now** (podrunner can do it), **other** (needs another project:
a COORD issue), **no** (not feasible).

| # | Work | Files | Prove | Effort | Status | Needs |
| --- | --- | --- | --- | --- | --- | --- |
| W-1 | Follow the label: `ubuntu-latest` is 26.04 from 2026-11-19. Amend T-538 and the ruling text to name the label, not 24.04. Add `images/hosted/labels.toml` (label, OS, release tag, commit). | `TODO/later.md` (T-538), `docs/decisions.md`, `images/hosted/` | the `watch` job reads a fixture of the README table and refuses a moved label | S | now | — |
| W-2 | The probe and the diff: `scripts/hosted/probe.sh` (POSIX sh, one JSON object: os-release, tools and versions by a command map from the report's tool names, `dpkg-query -W`, the `/opt/hostedtoolcache` tree, environment, `id runner`, `sudo -n true`, enabled and active services, PID 1, ulimits, a set of sysctls, `df /`, the docker socket path, `GITHUB_WORKSPACE`); `scripts/hosted/diff.py` (`python3 -I`) with `images/hosted/allow.toml` | `scripts/hosted/`, `images/hosted/allow.toml`; a test in the gate | a fixture pair: a removed tool, a changed major version and an unlisted environment variable each fail; a stale allow row fails | M | now | — |
| W-3 | `hosted-parity.yml`: the jobs `reference`, `report`, `units` (the current `runner-full` image on incus, microvm, podvm) and `diff`, **report-only** first (`continue-on-error`) to give the gap list; blocking when the allow list is complete | `.github/workflows/hosted-parity.yml` | the first run lists the gap; a plant (a tool removed in a test image) is red | S | now | W-2 |
| W-4 | The `watch` job and its issue | `scripts/hosted/watch.py`, the workflow | a pin older than the newest release fails the job in a test with a fixture of the releases API | S | now | W-1 |
| W-5 | The environment of the hosted image in the image of today: `/etc/environment` (the hosted `PATH`, `ImageOS=ubuntu24`, an `ImageVersion` that names our build, `RUNNER_TOOL_CACHE=/opt/hostedtoolcache`, `AGENT_TOOLSDIRECTORY`), the runner `.env` made from it (a JIT runner runs no `config.sh`), `/opt/hostedtoolcache` owned by `runner`, `runner` in the groups `docker`, `adm`, `systemd-journal`, a `/var/run/docker.sock` link where an engine runs | `images/runner/Dockerfile`, `crates/podrunner-incus/src/guest.rs` (`SCRIPT`), `crates/podrunner-microvm/src/guest.rs` | the probe diff of these keys is empty on incus and microvm; a `setup-python` job takes the cache path | M | now | W-2 |
| W-6 | `hosted-core` recipe: `images/hosted/Dockerfile` (24.04 and 26.04): the base by digest; runner-images at the commit of the pinned release tag (tarball SHA-256 in `images/hosted/ubuntu24.toml`); the build `systemctl` (section 1.4); `invoke_tests` off; `APT_SNAPSHOT_TIMESTAMP` of the release date; the toolset patched to the exact toolcache versions of `internal.ubuntu24.json`; a token as a build secret; the core script list (section 3.1); then the runner layer of `images/runner/Dockerfile`. Measure the size. | `images/hosted/`, `scripts/build-image.sh`, `THIRD_PARTY.md` | the build passes on a hosted runner; the method (b) diff of the core tools is empty or allowed; T-500's Prove (a Playwright test with no download of system libraries) passes | L | now | W-2 |
| W-7 | Push to ghcr.io: build `hosted-core` and the image of today, push by digest, cosign, the digests in the summary and in `podrunner-manifest.json` | `.github/workflows/runner-image.yml` (or `hosted-image.yml`), `release.yml`, `scripts/release-assets.sh` | `podvm pull ...@sha256:D` as a user with no root in CI, then a job runs in the image | M | now | W-6 |
| W-8 | Pull by digest in each path: `install.sh --hosted[=ubuntu24]`, `config init`, the `prepare` of the incus and microvm kinds when the operator store lacks the digest | `scripts/install.sh`, `crates/podrunner-incus/src/lib.rs`, `crates/podrunner-microvm/src/lib.rs`, `docs/install.md` | `scripts/test-install.sh` with a stand-in registry; a `serve` run with an empty store pulls once | M | now | W-7 |
| W-9 | podvm and podnet from ghcr.io: the installer gets a one-layer image with `curl`; the pins take a `reference` digest | `images/podvm.toml`, `images/podnet.toml`, `scripts/install-pins.py`, `scripts/install.sh`, the fetch steps of the workflows | `test-install.sh` against a stand-in registry; a real pull when they publish | M | other (COORD-102: podvm, podnet publish) | — |
| W-10 | The `systemctl` shim (section 2.4) in `hosted-core` | `images/hosted/systemctl` (0BSD) | on incus with no systemd: `sudo systemctl start postgresql.service && pg_isready` and the same for mysql pass; an unknown unit gives the clear error | S | now | W-6 |
| W-11 | systemd as PID 1 on incus. COORD (to podvm): an option of the incus tier that boots the image's init, runs the channel relay as its own process, waits for `is-system-running`, runs the command by exec, and stops the instance at its end. podrunner: `[unit.incus] init = "systemd"`, a capability fact, no manual `dockerd` when the image has `docker.service` | `crates/podrunner-incus/src/{lib,guest}.rs`, `docs/configuration.md` | the incus suite of `runner-image.yml`: `systemctl is-system-running` is `running` or `degraded`; `sudo systemctl start postgresql.service`; `docker run` against the image's own daemon at `/var/run/docker.sock`; the Pester `Databases` tests pass | M (podrunner), M (podvm) | other (podvm) | W-6 |
| W-12 | systemd as PID 1 on microvm, and a root on a disk. COORD (to podvm): a disk root for each image digest with a copy-on-write overlay for each guest, a switch to the image's `/sbin/init`, a kernel that systemd accepts, the exit status of the payload back to the host. podrunner: the unit form of section 2.3 | `crates/podrunner-microvm/src/{lib,guest}.rs`, `images/hosted/podrunner-job.service` | a guest boots `hosted-core` with 4 GiB of memory (no longer twice the image); the probe diff; the Pester lane | M (podrunner), L (podvm) | other (podvm) | W-6 |
| W-13 | The podvm kind with a large image: one operator store and podvm's tree for each container (`FICLONE`, the spare tree) instead of a store, a load and an extraction for each unit. First find why each unit has its own store (labels, cleanup, confine). | `crates/podrunner-podvm/src/unit.rs`, `unit/cache.rs`, `unit/root.rs` | a measure in `runner-image.yml`: the writes and the start time of a unit of `hosted-core`, before and after | M | now (check the reason first) | — |
| W-14 | The Pester lane: the tests of the pinned commit, `toolset.json` at `/imagegeneration/installers/`, an allow list for each `Describe` and `It` | `scripts/hosted/pester.sh`, `images/hosted/pester-allow.toml` | the lane runs on incus; a missing tool fails its `Describe` with no allow row | M | now | W-6 |
| W-15 | `hosted-full`: the full script list, less the restricted parts; built on a large host (not a standard hosted runner); pushed to ghcr.io; the restricted parts as a host step (`podrunner image complete`) | `images/hosted/`, `crates/podrunner/src/cli.rs` (`image`), a live workflow | the probe diff and the Pester lane on the live host lane (incus kind), at the level "proven on a constrained host" | L | now for the recipe; the build host and the live lane need the live gates (GitHub #74, #75) | W-6, W-7, W-11, W-16 |
| W-16 | The licence call: record in `docs/decisions.md` that the public image leaves out each part that may not be redistributed (Android SDK clause 3.4; Chrome, Edge, CodeQL, Copilot CLI after a check), and list each component in `THIRD_PARTY.md` | `docs/decisions.md`, `THIRD_PARTY.md` | `scripts/hosted/diff.py` refuses an image that holds a path of the restricted list | S | now | — |
| W-17 | Amend T-538's Prove: `hosted-core` in hosted CI on each kind that runs it; `hosted-full` on the live lane; names and versions at the toolset precision (not "major versions" only) | `TODO/later.md` | `cargo todo check` | S | now | — |
| W-18 | Placement by label and init: `[[pin.payload]]` names its labels and needs `init: systemd`; `serve` sends a job of that label only to a kind with the fact | `crates/podrunner-schema`, the placement code, `docs/configuration.md` | a test: a job of `ubuntu-latest` with a systemd payload never goes to the podvm kind | M | other (podcrates: the capability field of the `pod-job` wire, COORD) | W-11 |
| W-19 | The corpus: record `ImageOS` and `ImageVersion` of each github.com result; read the results again on 26.04; add real public jobs that run `sudo systemctl start postgresql.service` or `mysql.service` (xfail until W-11) | `scripts/corpus/user-gate.tsv`, `scripts/user-gate.py` | `user-gate.yml` | S | now | — |
| W-20 | arm64 (`toolset-2404-arm64.json`, `toolset-2604-arm64.json`) | `images/hosted/` | the build and the probe on `ubuntu-24.04-arm` | M | now, after W-6 | W-6 |
| — | systemd on the podvm userland rung | — | — | — | **no**: no root, no namespaces, no PID 1, no mounts (section 2.2) | — |
| — | `hosted-full` as a microvm initramfs | — | — | — | **no**: the initramfs must fit twice in memory | — |
| — | `hosted-full` in a unit on a standard GitHub-hosted runner | — | — | — | **no**: 72 or 145 GB of disk, 54 to 58 GB used at start | — |
| — | The exact hosted kernel, sysctls on a shared kernel, the Azure swap and resource disk | — | — | — | **no** on `podvm` and `incus` (shared kernel); allow rows. A microvm guest can set the sysctls. | — |

COORD issues to write (podrunner does not edit the other repositories):

- (to podvm) W-11: an incus mode with the image's init as PID 1.
- (to podvm) W-12: a machine tier root on a disk, the image's init, the
  exit status.
- (to podvm) optional, for the namespace rung: a subordinate uid range and
  a systemd mode. Low priority.
- (to podcrates) W-18: an `init` fact in the unit capability of `pod-job`.
- (to podvm, podnet) W-9 is already asked by COORD-102.

### 5.1 Order

1. W-1, W-16, W-17 (records; the label move is today).
2. W-2, W-3, W-4: the mechanical check on the image of today. The first
   run gives the gap list as data.
3. Send the COORD issues of W-11 and W-12 at once: they take the longest.
4. W-5, then W-6, W-7, W-8, W-10: `hosted-core` on ghcr.io, run on the
   incus kind (githome's trusted jobs go to incus: `docs/HANDOFF.md`,
   "Facts of that host"), then microvm.
5. W-14, W-13, W-19.
6. W-11 and W-12 when podvm delivers; W-18.
7. W-15 when the live lane exists; W-20; W-9 when podvm and podnet
   publish.

## 6. Recommendation

The smallest first step with the most value for githome and cfb:

1. **W-2 and W-3 (report-only)**: the probe on github.com and in the unit
   of today. It costs about three days and turns the gap into data in CI.
2. **W-5 and W-6, then W-7 and W-10, on the incus kind**: `hosted-core`.
   It gives the hosted toolcache (Node 22, 24, 26; Python; Go), the hosted
   environment (`RUNNER_TOOL_CACHE`, `ImageOS`), Playwright's system
   libraries, the docker CLI, and `sudo systemctl start` of postgresql and
   mysql through the shim. This covers cfb's stated gate needs (Node 22,
   24, 26, npm, pnpm, TypeScript, Biome, vitest, Playwright: T-500,
   "Premise") and T-500's Prove. It fits hosted CI and ghcr.io.
3. Real systemd (W-11) and the full image (W-15) come after, on the
   answers of podvm and on the live lane.

## 7. Findings

Flaws and gaps of podrunner found in this study. None is fixed here.

- **F1 (P1, the record).** The ruling of 2026-11-19 and T-538 take the
  toolset of `ubuntu-latest` as 24.04 by implication (T-538 Problem:
  "Ubuntu 24.04"). The label is 26.04 from 2026-11-19 (runner-images#14748).
  The work must name labels and pin both 24.04 and 26.04 (W-1).
- **F2 (P2).** T-538's Prove asks for the probe "in a unit of each kind
  that can run the image" in `runner-image.yml`. The full image cannot run
  on a standard GitHub-hosted runner (disk). The Prove needs the split of
  W-17.
- **F3 (P2).** The image has no hosted environment: no `/etc/environment`
  content of the hosted image, no `RUNNER_TOOL_CACHE=/opt/hostedtoolcache`,
  no `ImageOS`, no runner `.env` (grep of the repository: no hit for
  `RUNNER_TOOL_CACHE`, `hostedtoolcache` or `ImageOS` outside a test
  workflow). Each `setup-*` action downloads its tool in each job (W-5).
- **F4 (P2, scale).** The podvm kind loads the image tar into a new store,
  extracts it and copies a tree for each unit
  (`crates/podrunner-podvm/src/unit.rs` lines 248 to 255). The cost grows
  with the image; a large image is not usable on this kind (W-13).
- **F5 (P2, scale).** The incus root disk takes `limits.disk_bytes`
  (`crates/podrunner-incus/src/guest.rs` line 115). githome gives
  40 GB per unit; a hosted-full image of about 55 GB does not fit. The
  doctor or `check` should refuse a pin whose image is larger than the
  disk limit of its kind.
- **F6 (P3).** The engine socket is `/run/podrunner-docker.sock`
  (`crates/podrunner-microvm/src/guest.rs` line 27; the incus `SCRIPT`),
  with `DOCKER_HOST` set. A script that mounts `/var/run/docker.sock`
  (a common pattern on hosted runners) fails. A link at
  `/var/run/docker.sock` fixes it (W-5).
- **F7 (P3, the corpus).** `scripts/corpus/user-gate.tsv` records the
  github.com result of each row with no image version. After the label
  move, a new github.com run of a row runs on 26.04, and the corpus cannot
  tell an image change from a podrunner change (W-19).
- **F8 (P3).** The hosted sysctls (`vm.max_map_count=262144`,
  `fs.inotify.max_user_watches=655360`, `fs.inotify.max_user_instances=1280`)
  do not hold in a unit of a shared kernel. A job of a file watcher or
  Elasticsearch can fail. The probe records them; a host note in
  `docs/install.md` can name them for the operator.

## 8. Sources

- runner-images: `README.md`, `LICENSE`, `images/ubuntu/templates/*.pkr.hcl`,
  `images/ubuntu/toolsets/toolset-2404.json`, `images/ubuntu/scripts/build/*`
  (84 files), `images/ubuntu/scripts/helpers/*`, `images/ubuntu/scripts/tests/*`,
  `images/ubuntu/Ubuntu2404-Readme.md`, `images/ubuntu-slim/*`,
  `docs/create-image-and-azure-resources.md`, the releases API, issues
  #14748, #14492; read on 2026-11-19 through forge.web.pkgforge.dev and
  fetch.web.pkgforge.dev.
- ghcr.io manifests of `christopherhx/runner-images:ubuntu24-runner-large-latest-amd64`
  and `catthehacker/ubuntu:{act,runner,full}-24.04`.
- `catthehacker/docker_images` README and `linux/ubuntu/scripts/act.sh`;
  `ChristopherHX/runner-image-blobs` README and `.github/workflows/dump.yml`.
- podvm `docs/incus.md`, `docs/limits.md` (`main`).
- The Android SDK licence (developer.android.com/studio/terms), clause 3.4.
- podrunner at `8ce5dd7`: the files named in each row.
