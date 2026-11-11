# Development

Rules for the code and the tests of podrunner. `AGENTS.md` has the rules
of each session; `docs/maintenance.md` has the procedures.

## Checks before a push

Run these, and push only when each exits 0:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo todo check
```

`sh scripts/agent/check.sh` runs them, the size of the Rust files, the
check of the commit messages and the static build. CI runs the same on
each push (`.github/workflows/gate.yml`).

## Toolchain

* `rust-toolchain.toml` pins the channel of podcrates, with the musl
  targets. Raise it with `rust-version` of `Cargo.toml` when podcrates
  raises its own.
* Edition 2024. rustfmt with `max_width = 120`.
* The default target is `x86_64-unknown-linux-musl` (`.cargo/config.toml`):
  `cargo build --release` gives a static binary.
* aarch64 and each C dependency build with zig: `sh scripts/cloud-setup.sh`,
  `export PATH="$HOME/.local/bin:$PATH"`, then
  `cargo zigbuild --release --target aarch64-unknown-linux-musl -p podrunner`.
* `sh scripts/check-static.sh BINARY` proves a static binary.
* Set `CARGO_BUILD_JOBS=4`. Run one cargo build at a time.

## Crates

* Each crate of this repository has the prefix `podrunner-`; the binary
  is `podrunner`.
* A crate of podcrates comes by its tag, in `[workspace.dependencies]` of
  `Cargo.toml`. A member crate names it only when it needs it.
* Generic code that another project can use goes to podcrates as a `pod-`
  crate: ask the podcrates session.

## File size

A Rust file has 500 lines or fewer. There is no allowlist: split a longer
file. Do not remove comments to make a file shorter.

## Rules for tests

* **A test for each fix.** A fix of a defect comes with a test that fails
  on the code before the fix. Run the new test on the old code one time,
  and write in the commit message that it fails there.
* **A planted defect for each check.** A new check (a test, a CI step, a
  scan, a conformance test) must fail on a planted defect before it is
  trusted.
* **No sleep as a proof.** A test waits for an event (a channel, a
  socket) with a time limit. Level 1 uses a manual clock: no test waits
  real time.
* **Loopback only.** A unit or integration test uses loopback servers
  (`127.0.0.1:0`, a Unix socket in a temporary directory). A test of a
  live service is `#[ignore]` and runs in a live workflow.
* **No shared state.** A test makes its own temporary directory, and
  does not set a process variable.
* **No secret in a test.** A test token or password is a made-up value;
  list it in `.github/secrets-allow.txt` with the reason. A credential
  canary must appear in no output, log or argv.

## Random failures

* A test that fails one time in many runs has a defect: in the test, or
  in the code. Do not run CI again until it is green, and do not mark the
  test ignored.
* Reproduce it in a loop
  (`for i in $(seq 50); do cargo test NAME -- --exact || break; done`),
  find the race, fix the cause, and give the loop count in the commit
  message.

## Commits

* The subject is `area: text`, for example `slots: the journal`. The
  area is the crate without its prefix, or `record`, `ci`, `docs`,
  `scripts`.
* The body says what changed and why, and names the tests and their
  counts.
* A commit that does the work of an entry names it (`T-NNN`).
* No attribution line (`AGENTS.md`, Git). `.githooks/commit-msg` refuses
  one.
* Write in ASD-STE100 Simplified Technical English.
