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
| `--serve` | Install for `podrunner serve` (T-507): the same files, but no `run.toml` and no `hello.yml`. It writes `DATA/serve.json` (`podrunner`, `podvm`, `podnet`, `images`, `image`, `runner_version`) for a program that runs `podrunner config init`, and prints that next step. A second run changes nothing. |

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
  --image runner@sha256:HEX --built-at-s N --out ~/.config/podrunner/podrunner.toml --json
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
base and the full image (`file`, `sha256` and the manifest `digest`, or
`null`); `podvm` and `podnet`: the version and the SHA-256 of each
architecture. `podrunner doctor --config FILE` names the runner of each pin
of the file, its age and its window, and warns in the last 7 days of it.

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

Each has a `.sha256` file and a build provenance attestation. The image
archives come from the green `runner-image.yml` run of the tagged commit:
the suites of that run passed on those images (`.github/workflows/release.yml`,
job `assets`). The sizes, from the artifacts of runner-image run
43704601731 (the tag `v0.1.0-beta.6`): the base image about 300 MiB on
x86_64 and 210 MiB on aarch64, the larger image about 460 MiB on x86_64; a
bundle about 530 MiB on x86_64 and 360 MiB on aarch64. A release asset has
at most 2 GiB; the job `assets` fails on a larger file.

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
