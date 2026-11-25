# Third-party components

Each component that podrunner ships or runs, with another licence than
0BSD. The crates of Cargo.lock are checked by `deny.toml`.

| Component | Licence | Where | Entry |
| --- | --- | --- | --- |
| `github.com/actions/scaleset` v0.5.0 | MIT (Copyright GitHub, Inc.) | The tests of `tools/fake-scaleset` (the client against the fake); the scale-set sidecar | T-028, T-029 |
| `github.com/actions/runner` v2.338.2 (the release tarballs) | MIT (Copyright GitHub, Inc.; `images/runner/LICENSE.actions-runner`, as the tarball has no licence file) | The unit image of `images/runner/Dockerfile` (with the licence file); the runner tarball in the offline bundle of a release and in the directory that the installer unpacks; not in the binary of podrunner (call C4) | T-030, T-202 |
| Node.js and .NET in the runner tarball (`externals/node*`, `bin/`) | Node.js: MIT and the licences of its parts (`externals/node*/LICENSE`); .NET: MIT | The runner tarball, as above | T-030, T-202 |
| Docker Engine and CLI 29.9.2, containerd 2.4.2, runc 1.5.2 and Tini 0.19.0 | Apache 2.0 for the engine, CLI, containerd and runc. MIT for Tini. Upstream notices and licences are in `images/runner/LICENSE.docker-engine`. | The pinned static Docker archives for x86_64 and aarch64 in the Ubuntu full runner image | T-672 |
| Ubuntu 24.04 (the base of the unit image) and the packages of `images/runner/Dockerfile` and `images/runner/full.txt` | The licences of its packages (GPL, LGPL, MIT, BSD and others; `/usr/share/doc/*/copyright` in the image) | The unit image of `images/runner/Dockerfile`, pinned by digest; the release assets `runner-ARCH.tar` and `runner-full-ARCH.tar`, and the base image in the offline bundle. The list of packages and their sources: T-204 | T-030, T-202, T-204 |
