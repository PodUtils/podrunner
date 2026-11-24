# The installer of the local mode

`scripts/install.sh` installs each part of the local mode (`podrunner run`,
`docs/local-mode.md`) for the user, with no root (T-202). A release attaches
it as `podrunner-install.sh`, with the pins of that release.

## Quick start

Use a release that attaches `podrunner-install.sh`. Set `VERSION` to its
version (for example `0.1.0-beta.7`):

```sh
curl -fsSLO "https://github.com/PodUtils/podrunner/releases/download/vVERSION/podrunner-install.sh"
sh podrunner-install.sh
cd YOUR-PROJECT && podrunner run .github/workflows/ci.yml --checkout .
```

GitHub gives no `latest` link for a pre-release: name the version.

## What it needs

- Linux on x86_64 or aarch64, `/bin/sh`, `tar` and `gzip`.
- `curl` or `wget` (not with `--bundle`).
- `sha256sum`, `shasum` or `openssl`.
- No root, no `gh`, no token and no Python.
- Optional: `pod-release` (a static binary of each release of podcrates;
  it needs no network) or `cosign` on `PATH`. With one of them, the
  installer checks the signature of `SHA256SUMS` (T-523).

## What it does

| Step | Result |
| --- | --- |
| podrunner | `PREFIX/bin/podrunner` |
| podvm and podnet | `DATA/bin/podvm`, `DATA/bin/podnet` |
| The official runner | `DATA/runner/VERSION`, for `--runner-dir` |
| The runner image | loaded into the podvm store `DATA/images` (`podvm load`) |
| A first workflow | `DATA/hello.yml` |
| The file of the user | `$XDG_CONFIG_HOME/podrunner/run.toml` (T-143): `podvm`, `images` and `image`, and the lines of `podnet`, `allow` and `runner-dir` as comments |
| The checks | `podrunner doctor` and `podrunner doctor --run DATA/hello.yml`, then whether the host has user namespaces and `/dev/kvm`, and the next command |

`PREFIX` is `~/.local`. `DATA` is `$XDG_DATA_HOME/podrunner`, else
`~/.local/share/podrunner`. With `--prefix DIR`, `PREFIX` is `DIR` and
`DATA` is `DIR/share/podrunner`.

The installer writes `run.toml` again on each run while its first line is
the marker line of the installer. Remove that line to keep your changes. A
`run.toml` with no marker line stays as it is; the installer writes its
keys into `run.toml.podrunner-install` beside it.

`run.toml` holds no `podnet`: a first run of a real workflow reaches its
registries. To hold the egress of each job, remove the `#` of the `podnet`
and `allow` lines, and give in `allow` each host that a job needs.

## Options

| Option | What it does |
| --- | --- |
| `--prefix DIR` | Install into `DIR/bin` and `DIR/share/podrunner`. |
| `--full` | Load the larger runner image (`images/runner/full.txt`: C and C++, CMake, Python, Node.js) in place of the base image, and use it for each job. |
| `--bundle PATH` | Install from an offline bundle: its `.tar.gz` or its unpacked directory. No network. |
| `--serve` | Install for `podrunner serve` (T-507): the same files, but no `run.toml` and no `hello.yml`. It writes `DATA/serve.json` (`podrunner`, `podvm`, `podnet`, `images`, `image`, `runner_version`, `unpacked_bytes`) for a program that runs `podrunner config init`, and prints that next step. A second run changes nothing. |

`PODRUNNER_MIRROR=URL` takes each file from `URL/NAME` in place of its
release on github.com (`file://DIR` is a directory). Each file must agree
with its pin. The mirror must also hold `SHA256SUMS` and
`release-index.json` of the release, and `SHA256SUMS.sigstore.json` when
`pod-release` or `cosign` is on `PATH`.

## An install of serve

A program (githome's CLI, for example) installs `podrunner serve` on a
machine with no prompt (T-507). The operator does these steps with root,
one time: Incus with a range for root in `/etc/subuid` and `/etc/subgid`,
the user in the group of `/dev/kvm` and of the socket of incusd, and
`loginctl enable-linger USER`. `podrunner doctor --json` names each one
that is missing. Then, as the user:

```sh
sh podrunner-install.sh --serve
podrunner config init --source NAME --program PATH --podvm PODVM --images STORE \
  --image runner@sha256:HEX --built-at-s N --unpacked-bytes N --out ~/.config/podrunner/podrunner.toml --json
mkdir -p ~/.config/systemd/user
podrunner print-unit --config ~/.config/podrunner/podrunner.toml --state ~/.local/state/podrunner \
  --binary ~/.local/bin/podrunner > ~/.config/systemd/user/podrunner.service
systemctl --user daemon-reload
systemctl --user enable --now podrunner.service
```

`podrunner-install.sh --serve` prints the `config init` line with the
paths of the install (`DATA/serve.json` has them too). `config init`
writes the kinds that this host can run, at the tier of the source; a
trusted podvm kind needs `--podnet` and a confine layer that its
self-test proves here. A second run of each step changes nothing. The
install and `serve` open no TCP listener: the host keeps no inbound port.

Proof level: `podrunner-install.sh --serve`, `config init`, `check`,
`doctor`, `print-unit` and a start of `serve` in CI on GitHub-hosted
runners as a user with no root (the lane `serve-incus-kvm` of
`user-gate.yml`, on incus and microvm). On a constrained host as a user:
the schema of the file, no TCP listener of `serve`, and a runner job
through `serve` on podvm (T-527, T-566); the lanes measure them in CI
next. `systemctl --user` of the unit is not run.

### A unit of the system

A host can run `serve` as a unit of the system in place of a user unit
(T-588). The unit runs as an account with no privilege (`--user NAME`,
default `podrunner`), not as root. Do these steps as root. Make the
account, give it the groups of `/dev/kvm` and of the socket of incusd,
and give it a range in `/etc/subuid` and `/etc/subgid`. Put the
configuration where the account can read it:

```sh
podrunner print-unit --system --user podrunner --config /etc/podrunner/podrunner.toml \
  --state /var/lib/podrunner > podrunner.service      # read it first
podrunner install-unit --system --user podrunner --config /etc/podrunner/podrunner.toml \
  --state /var/lib/podrunner
```

Give `--config` and `--state` as absolute paths: systemd starts `serve`
in `/`. `install-unit --system` writes
`/etc/systemd/system/podrunner.service`, then enables and starts it. It
refuses a caller that is not root (77), an account that the host does
not know (66), an account of uid 0 (64), and a file of the same name that
podrunner did not write (78). A unit that does not start is removed.

With `--state /var/lib/NAME`, the unit has `StateDirectory=NAME`: at each
start, systemd makes the directory, gives it and each file in it to the
account, and sets mode 0700. Do not name a directory that another
service uses, such as `/var/lib/incus`. With another `--state`, make the
directory and give it to the account first. `remove-unit --system` stops
and removes the unit; the state and the configuration stay.

The control socket (`STATE/ctl.sock`) has mode 0600 and the uid of the
account. Talk to `serve` as the account, not as root:

```sh
sudo -u podrunner podrunner status --state /var/lib/podrunner
```

The unit has `Delegate=yes` and `DelegateSubgroup=serve`: podvm makes the
cgroup of a unit beside the cgroup of `serve`. `DelegateSubgroup=` needs
systemd 254 or later. An older systemd ignores it, and podvm then holds
no CPU limit; `install-unit` says so on stderr. The lines of the
hardening that `serve` cannot take are comments with their reasons
(`docs/decisions.md`, T-588). The unit keeps `NoNewPrivileges=yes`: a
job of the podvm kind or of a runner directory cannot use `sudo`. A
guest of the microvm kind and an instance of the incus kind are not
under it, and `sudo` works there.

Proof level: the text of the unit, the audit of `pod-unit` and each step
of the install against a fake host in the unit tests; `systemd-analyze
verify` of the unit on a development host. Not run: a start of `serve` in
a unit of the system on a host.

### The disk and the sysctls of a host of the incus or podvm kind

The root disk of an incus instance is `disk_max_bytes` when the pool
holds a size (`root_quota`). An image larger than it fails each job at
its start. Give each pin its `unpacked_bytes` (`du -sxb /` in a unit of
the image): `podrunner check` and `serve` then refuse a pin above the
root disk, and `doctor` names the disk that the image needs (T-551).
`podrunner-manifest.json` of a release gives `unpacked_bytes` of each
runner image (the files of its layers), and the installer prints it in
the line of `config init` (T-572). The
incus kind needs the extracted tree plus about 2.4 times it in incusd
(podvm's measure). For the hosted snapshot of 55 to 60 GB, give each
unit a root disk of 80 GB or more, and the host about 190 GB.

The root of a microvm guest is an uncompressed initramfs. podvm needs
13/4 times the image plus 512 MiB of guest memory (COORD-219). `check`
refuses a pin over `([unit.microvm] mem - 512 MiB) × 4 / 13` (8G when
absent: about 2363 MiB), with the value of `mem` that holds it (T-573).

A unit of the incus or the podvm kind shares the kernel of the host, so
it cannot set a sysctl. A job of a file watcher or of Elasticsearch
expects the values of a GitHub-hosted runner. Set them on the host, with
root, one time:

```sh
cat > /etc/sysctl.d/90-podrunner-hosted.conf <<'EOF'
vm.max_map_count = 262144
fs.inotify.max_user_watches = 655360
fs.inotify.max_user_instances = 1280
EOF
sysctl --system
```

A guest of the microvm kind has its own kernel; these host values do
not apply to it.

## The checks

- Each version and each SHA-256 is in the block of pins of the installer.
  `scripts/install-pins.py` writes the block from `images/podvm.toml`,
  `images/podnet.toml` and `images/runner/runner.toml`. The release adds
  its version, the SHA-256 of each podrunner binary and of each image
  archive, and the manifest digest of each image. The copy in the tree has
  no pins of a release: it refuses to install, and names the installer of a
  release. `python3 -I scripts/install-pins.py --check scripts/install.sh`
  fails when the block and the pin files differ (the gate and
  `scripts/agent/check.sh` run it).
- A file goes to its place only after its SHA-256 agrees with its pin. A
  partial or wrong download fails with the name of the file and both
  digests, and leaves no file in place.
- A second run checks each binary against its pin, the marker of the runner
  directory, and the image in the store (`podvm inspect`, then
  `podvm verify`). It downloads only what is missing or wrong. A run that
  downloads nothing says so.
- `podvm load` checks each blob of the image against its digest.
- Before the first download of a file of the podrunner release (the
  podrunner binary or a runner image archive), the installer downloads
  `SHA256SUMS` and `release-index.json` of the release (T-523). A file of
  the release goes to its place only when `SHA256SUMS` gives its pin and
  `release-index.json` names its SHA-256. `SHA256SUMS` must give the
  SHA-256 of `release-index.json`, and the index must name the tag. When
  the installer runs from a file (not from a pipe), `SHA256SUMS` must give
  the SHA-256 of the installer as `podrunner-install.sh`.
- With `pod-release` on `PATH`, the installer also downloads
  `SHA256SUMS.sigstore.json` and runs
  `pod-release verify --repo PodUtils/podrunner --tag vVERSION DIR`: the
  signer must be `release.yml` at the tag of the release. With no
  `pod-release` and with `cosign` on `PATH`, it runs `cosign verify-blob`
  with the same signer (cosign can need the network for its trust root).
  With neither, it writes a `warn` line and checks no signature.
- Each failure of these checks stops the install with exit 1, a line that
  names the file and the cause, and no file of the release in place. A
  missing `SHA256SUMS`, `release-index.json` or (with a verifier)
  `SHA256SUMS.sigstore.json` is a failure. There is no option to skip a
  check. To skip the check of the signature, run the installer with a
  `PATH` that has no `pod-release` and no `cosign`.
- An install from a bundle (`--bundle`) uses no network and does not read
  `SHA256SUMS`: check the archive of the bundle with `SHA256SUMS` of the
  release first (below).

To check the installer itself and a bundle:

```sh
sha256sum -c podrunner-install.sh.sha256
gh attestation verify podrunner-install.sh --repo PodUtils/podrunner
```

Each release also has `SHA256SUMS` (one line for each asset) with its
keyless cosign bundle `SHA256SUMS.sigstore.json`, as the other projects of
PodUtils have (T-484). The identity is `release.yml` of this repository:

```sh
cosign verify-blob --bundle SHA256SUMS.sigstore.json \
  --certificate-identity-regexp '^https://github.com/PodUtils/podrunner/\.github/workflows/release\.yml@' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com SHA256SUMS
sha256sum -c --ignore-missing SHA256SUMS
```

From the next release, each release also has `release-index.json` (T-562,
githome's G-34): the tag, the commit, and each file with its SHA-256,
size and target. `SHA256SUMS` names it, so the one signature covers it,
and the signer is `release.yml` at the tag of the release. The verifier
of each product of PodUtils, `pod-release` (a static binary of each
release of podcrates), checks the signature, each file in the directory
and the index:

```sh
pod-release verify --repo PodUtils/podrunner --tag vVERSION DIR
```

## The manifest of a release

Each release attaches `podrunner-manifest.json` with its `.sha256` and an
attestation. A machine reads it to take a new runner image with no new
binary (T-499). It holds `schema` (1); `podrunner`: the version and, for
each architecture, the file and its SHA-256; `runner`: the version of the
official runner, the SHA-256 of each tarball, and for each architecture the
base and the full image (`file`, `sha256`, the manifest `digest`,
`built_at_s` and `unpacked_bytes`, or `null`); `podvm` and `podnet`: the version and the SHA-256 of each
architecture. `podrunner doctor --config FILE` names the runner of each pin
of the file, its age and its window, and warns in the last 7 days of it.

## A newer runner image

A release of `actions/runner` gets its runner images in a release of its
own, with no new podrunner binary (T-499): the tag is
`vrunner-RUNNER_VERSION-rN`, a prerelease. Its files are
`runner-ARCH.tar`, `runner-full-ARCH.tar`, the manifest
`runner-images.json` (the `runner` object has the form of
`podrunner-manifest.json`), a `.sha256` of each, `release-index.json`,
`SHA256SUMS` and `SHA256SUMS.sigstore.json`. The signer is
`runner-image-release.yml` at the tag, not `release.yml`.

The installer and `podrunner` do not look for such a release. To take one,
download its files and check them (set `TAG` and `ARCH`):

```sh
base="https://github.com/PodUtils/podrunner/releases/download/$TAG"
for f in SHA256SUMS SHA256SUMS.sigstore.json release-index.json runner-images.json "runner-$ARCH.tar"; do
  curl -fsSLO "$base/$f"
done
sha256sum -c --ignore-missing SHA256SUMS
pod-release verify --repo PodUtils/podrunner --tag "$TAG" --workflow runner-image-release.yml .
```

With no `pod-release`, use `cosign verify-blob` with the identity
`https://github.com/PodUtils/podrunner/.github/workflows/runner-image-release.yml@refs/tags/TAG`.

Load the image into the store of the installer:

```sh
PODVM_STORE=~/.local/share/podrunner/images ~/.local/share/podrunner/bin/podvm load -i "runner-$ARCH.tar"
```

The notes of the release and `runner-images.json`
(`runner.images.ARCH.base.digest`) give the manifest digest of the image.
For the larger image, use `runner-full-ARCH.tar` and the `full` digest.
Then use the image:

- `podrunner run`: give `--image runner@DIGEST`, or set
  `image = "runner@DIGEST"` in `run.toml`. Remove the marker line (the
  first line) of `run.toml` first: else the next run of the installer
  writes the image of its release again.
- `podrunner serve`: add a `[[pin.payload]]` for each architecture to
  `podrunner.toml` (the notes of the release give each one, with
  `reference`, `built_at_s`, `runner_version` and `unpacked_bytes`), and
  sign the pin list again with `podrunner pin-sign`.
  `podrunner doctor --config FILE` names the runner of each pin and its
  age.
- `--runner-dir`: this executor runs the runner tarball, not an image.
  Unpack the tarball of the release of `actions/runner`, after a check of
  its SHA-256 against `runner.tarball_sha256` of `runner-images.json`.

## The offline bundle

The release attaches `podrunner-bundle-x86_64.tar.gz` and
`podrunner-bundle-aarch64.tar.gz` (`scripts/make-bundle.sh`). Each holds
one directory with podrunner, podvm, podnet, the runner tarball, the base
runner image archive, the installer with the pins of the release, a
`SHA256SUMS`, the licences (`LICENSE`, `THIRD_PARTY.md`,
`LICENSE.actions-runner`) and the marker `PODRUNNER-BUNDLE`.

```sh
gzip -dc podrunner-bundle-x86_64.tar.gz | tar -xf -
sh podrunner-bundle-*-x86_64/podrunner-install.sh
```

The installer in a bundle directory uses that directory. For `--full`, put
`runner-full-ARCH.tar` of the release into the bundle directory first.

The format is `.tar.gz`: `tar` and `gzip` are on each Linux host (busybox
too), and a `zip` needs `unzip`, which minimal hosts do not have. The layers
of an image archive are gzip already.

## The release assets

| File | What it is |
| --- | --- |
| `podrunner-install.sh` | The installer with the pins of the release |
| `runner-ARCH.tar` | The base runner image, as `podvm load -i` takes it; load it into the store of the installer (`PODVM_STORE=~/.local/share/podrunner/images`), as `podvm` with no `PODVM_STORE` uses another store |
| `runner-full-ARCH.tar` | The larger runner image |
| `podrunner-bundle-ARCH.tar.gz` | The offline bundle |
| `tier-cases.json` | The case table of the tier rule (T-518): each row gives the trust lists, the tier of the set, the facts of a job and the result. A forge with a trust rule of its own runs each row against it. `crates/podrunner-tier` (`cases::conformance`) runs it against a Rust rule. Its source is `crates/podrunner-tier/tier-cases.json`. `docs/design/examples/tier-cases.json` is the design copy, of another format: it predates the rulings Q5 and Q12 |

Each has a `.sha256` file and a build provenance attestation. The image
archives come from the green `runner-image.yml` run of the tagged commit:
the suites of that run passed on those images (`.github/workflows/release.yml`,
job `assets`). A runner image release (`vrunner-VERSION-rN`) has the
image archives only ("A newer runner image"). The sizes, from the artifacts of runner-image run
43704601731 (the tag `v0.1.0-beta.6`): the base image about 300 MiB on
x86_64 and 210 MiB on aarch64, the larger image about 460 MiB on x86_64; a
bundle about 530 MiB on x86_64 and 360 MiB on aarch64. A release asset has
at most 2 GiB; the job `assets` fails on a larger file.

## podrunner on ghcr.io

A release also publishes its binaries and its runner images on ghcr.io
(T-553), in the form of each project of PodUtils:

| Image | What it is |
| --- | --- |
| `ghcr.io/podutils/podrunner:vVERSION` | One index of `linux/amd64` and `linux/arm64`. Each image has no base (`FROM scratch`): the release asset `podrunner-ARCH-unknown-linux-musl` at `/usr/local/bin/podrunner` with the same SHA-256, an empty `/tmp`, and `podrunner` as the entrypoint |
| `ghcr.io/podutils/podrunner-runner:vVERSION` | The base runner image of the release: `runner-ARCH.tar` with its bytes as they are. The digest of each platform is the digest that the installer and `podrunner-manifest.json` pin |
| `ghcr.io/podutils/podrunner-runner:vVERSION-full` | The larger runner image, from `runner-full-ARCH.tar` |

The notes of the release give the digest of each index and of each
platform ("The images, by digest"). A tag can move in a registry; a digest
cannot. Pin the digest. Check the provenance of an index (set `DIGEST` to
the digest of the index):

```sh
gh attestation verify oci://ghcr.io/podutils/podrunner@sha256:DIGEST --repo PodUtils/podrunner
```

Take the binary from the image with docker, and compare it with the
release asset:

```sh
docker create --name podrunner-copy ghcr.io/podutils/podrunner@sha256:DIGEST
docker cp podrunner-copy:/usr/local/bin/podrunner ./podrunner
docker rm podrunner-copy
sha256sum ./podrunner
```

`sha256sum` must give the line of `podrunner-ARCH-unknown-linux-musl` in
`SHA256SUMS` of the release. `crane export` and `podvm pull` also take
the image by its digest. The installer and the offline bundle still take
the GitHub release assets.

## The test

`sh scripts/test-install.sh` runs the installer against small fake assets
in a directory, with their own pins: a fresh install, a second run that
downloads nothing (its mirror is empty), a changed binary that is replaced,
a corrupt asset that fails by name, a `run.toml` of the user that stays, and
the offline bundle as an archive and as a directory, with `--prefix` and
`--full`. The checks of `SHA256SUMS` and `release-index.json` (T-523): a
changed asset, a `SHA256SUMS` that gives another SHA-256, no `SHA256SUMS`,
no `release-index.json`, an index with no line of the binary, a signature
that a fake `pod-release` refuses, and a changed installer each fail; with
no verifier on `PATH`, the install passes with a `warn` line. It also runs the check of the pins with a planted change, the
assets of a release from `scripts/release-assets.sh`, an install from a
local HTTP server, a lane of the user-path gate (`scripts/user-gate.py`)
on the fake assets, and `shellcheck` when it is there. The gate and
`scripts/agent/check.sh` run it.

The user-path gate (`.github/workflows/user-gate.yml`, T-243) runs the
commands of "Quick start" and "The offline bundle" of this page as they
are written, on the files of a release, as a user with no root
(`docs/maintenance.md`, "A release").
