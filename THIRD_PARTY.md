# Third-party components

Each component that podrunner ships or runs, with another licence than
0BSD. The crates of Cargo.lock are checked by `deny.toml`.

| Component | Licence | Where | Entry |
| --- | --- | --- | --- |
| `github.com/actions/scaleset` v0.5.0 | MIT (Copyright GitHub, Inc.) | The tests of `tools/fake-scaleset` (the client against the fake); the scale-set sidecar | T-028, T-029 |
| `github.com/actions/runner` v2.338.2 (the release tarballs) | MIT (Copyright GitHub, Inc.) | The unit image of `images/runner/Dockerfile`; not in the binary of podrunner (call C4) | T-030 |
| Ubuntu 24.04 (the base of the unit image) | The licences of its packages | The unit image of `images/runner/Dockerfile`, pinned by digest | T-030 |
