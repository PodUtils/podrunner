# M2: podvm measured (podcrates#47 design review)

Subject: podvm `0.1.0-beta.13`, commit `36b15ec034f5` (shallow clone, copied to the work dir, never modified in place).

Conditions (all runs): 2026-11-10, kernel `6.12.113+deb13-amd64`, 8 cpus, `systemd-detect-virt` = `lxc`, uid 1000 in a user
namespace that maps ids 0..999 and 1000 only (`logs/01`), no capabilities. Registry access: no direct network; HTTPS goes through
the proxy in `https_proxy`, which reached Docker Hub, GitHub release assets, static.rust-lang.org, the Debian mirror, crates.io and GitHub.
The host Docker daemon cannot resolve registries and was not used. Labels: **Observed** (a log in `logs/`), **Source** (read in
the podvm tree, not run), **Inferred**, **Unverified**.

Rerun: `WORK=/some/dir scripts/run-all.sh`, or `ONLY=03 scripts/run-all.sh` for one script. Order: 00 builds, 03 prepares the runner
rootfs that 04 reuses. Scripts 03 and 04 use `sudo` (chroot + apt for the rootfs; a temporary host `/opt/runner` for the
`--unsafe-host-paths` cases, removed at the end). Script exit: 0 ran, 1 ran and the expectation failed, 2 could not run.
`run-all.sh` writes `logs/NN-name.log` and appends `exit=N` read from the process. Logs have `$WORK` substituted for the work dir.
Expectation exits: 03 exits 1 (the unmodified runner does not start), 05 exits 1 (grandchildren survive); both are results, not script errors.

## Build (00, `logs/00-build.log`)

| item | result |
| --- | --- |
| toolchain | `rust-toolchain.toml` pins channel `1.99`. It was absent (the system rustup had stable only and its home is not writable). Installed with `rustup` into the work dir: `rustc 1.99.1 (58cf8f9c3 2026-10-13)` plus target `x86_64-unknown-linux-musl`. Observed |
| other inputs | `zig 0.17.0` already installed (`ring` builds with `zig cc`); git dependency `podcrates` tag v0.11.0 fetched through the proxy. Observed |
| recipe | README: build `podvm-gate`, then `podvm-dev build`. One extra step: `cargo build --release -p podvm-gate --bin podvm-interpose-build` (podvm-dev refuses without it and names the command). Observed |
| artefact | `target/x86_64-unknown-linux-musl/release/podvm`, 4166872 bytes, static, profile release (lto, opt-level z, codegen-units 1, strip, panic abort). Observed |
| wall time | toolchain install 13.0 s, gate tools 16.7 s, `podvm-dev build` 68.9 s, total 98.6 s. 1 run, cold `CARGO_HOME`, warm proxy. An earlier manual run of the same steps: 20 s, 19 s, 89 s. Observed |

## Results

### Q1 rung selection (01, `logs/01-probe-rung.log`)

- Observed: `probe` selects `supervise`; `system info` gives `.Rung`=`supervise`, `.EnteredRung`=`chroot`; the first `run` banner says
  `mode=userland` ("entering without chroot on the loader family"). **This machine already is the chroot-denied class, and its default rung is the userland rung.**
  Why (JSON `probes`, `rejected`): `chroot(/tmp)`, `unshare(CLONE_NEWNS)`, `clone(CLONE_NEWNS)`, `mount(tmpfs)`, `mknod(chr 1:3)` all EPERM, so rung `namespace` is rejected.
  `unshare(CLONE_NEWUSER)` succeeds and is unused. `memfd_create+exec` ok, `/dev/fuse` opens.
- Observed: the userland rung cannot be forced by name and needs no forcing. `PODVM_MODE=userland` and `=chroot` are refused ("names no launch rung. The rungs are: memfd, fuse, tmpfs, rundir, cache").
  `rundir`, `cache`, `fuse`, `tmpfs` exit 125: "was forced but chroot(2) is denied on this machine, and only the memfd rung enters without it (T-1317)".
  `PODVM_MODE=memfd` is accepted only for a static payload (Q2).
- Observed: `--podvm-tier=chroot` exits 0 and still enters `mode=userland` (no refusal). `--podvm-tier=machine` exits 125 ("running a guest arrives with T-1303 and T-1304").
- Observed: `podvm doctor` exits 1 here on one check: "disk free against the file-size ceiling: free 15.77 GiB ... is below the 20.00 GiB file-size ceiling". Every other check is ok; it prints `profile: full`.
  Free space on this shared disk varied between 1 and 20 GiB during the session.

### Q2 images (02, `logs/02-images.log`), default rung = userland, 1 run each

| payload | exit | stdout | notes |
| --- | --- | --- | --- |
| alpine `true` | 0 | | |
| alpine `sh -c 'echo ok'` | 0 | `ok` | |
| debian:stable-slim `bash -c 'echo ok'` | 0 | `ok` | glibc interposer preloaded; 14 fixups, 5 degraded |
| node:lts-slim `node -e "console.log(1)"` | 0 | `1` | the tag is now node v26.13.0 |
| node `fetch('http://127.0.0.1:1').catch(e=>console.log(e.code))` | 0 | `undefined` | not a network result: undici rejects port 1 as a "bad port" before connecting; host node 20 prints the same (control in the log). Port 2 prints `undefined ECONNREFUSED` (`e.cause.code`) in both |

- Observed: every run prints a long stderr banner unless `-q` is given (per-fixup lines, "refused: ..." notes, ownership warning for every image entry). The payload does not inherit the host environment (a payload `env` held 12 variables, none of the proxy variables).
- Observed, memfd family (`PODVM_MODE=memfd`): alpine, debian bash and node all exit 125. First two: "dynamically linked against /lib/ld-musl-... a memfd exec never puts its libraries on disk"; node: "a #! script routes past fd-exec".
  A static `zig cc -static` hello: default rung exit 125 ("statically linked ... its absolute paths would resolve on the host", T-1648); with `--unsafe-host-paths` exit 0 `static-ok`, on the default path and under `PODVM_MODE=memfd`.
- "Forced userland" equals "default" here, so there is no separate forced run.

### Q3 / A-V-04, actions/runner v2.338.2 linux-x64 (03, `logs/03-runner-payload.log`)

Input: `actions-runner-linux-x64-2.338.2.tar.gz`, 228071396 bytes, sha256 `baebf204307bac6193480852732f0aba2ae97b6c5597c244f4d5558390147c40` (equals the forge mirror release API digest; `sha256sum -c` ok).
Image: debian:stable-slim plus apt `libssl3t64 zlib1g libkrb5-3 libicu76 ca-certificates` (sudo chroot of a normal extraction), then `podvm import`. 3 runs each. Wall time of image runs (4 to 5 s) is dominated by `--rm` re-extracting the 781 MiB image; `-v` runs take 0.2 to 1 s.

| case (userland rung) | exit (3 runs) | first error line |
| --- | --- | --- |
| runner in image: `/opt/runner/bin/Runner.Listener --version` | 154 x3 | `The application to execute does not exist: '/usr/lib/x86_64-linux-gnu/Runner.Listener.dll'.` |
| same: `sh -c 'cd /opt/runner && ./config.sh --help'` | 154 x3 | same |
| same with `DOTNET_EnableWriteXorExecute=0` | 154 x3 | same |
| `-w /opt/runner`, program `./bin/Runner.Listener` | 127 x3 | "chroot(2) is denied ... ./bin/Runner.Listener names no file in the image" (the first program is resolved against the image root, not `-w`) |
| `-v runner:/opt/runner`, program inside the volume | 125 x3 | "the program ... is inside the volume ... start it from a shell of the image, as `sh -c ...` (T-1653)" |
| `-v` plus `sh -c` for `--version` and `config.sh --help` | 154 x3 | same as the image case |
| `-v ...:ro`; `-w` inside the volume; `-v` with `PODVM_MODE=memfd` | 125 x3 | `:ro` refused until T-1655; workdir inside a volume refused (T-1653); "PODVM_MODE=memfd forces a rung with no path table" |
| diagnostic image: symlinks of `runner/bin/*` beside ld.so; `--version`, `config.sh --help`, and W^X off | 137 x3 | `Failed to create CoreCLR, HRESULT: 0x8007045A` |
| diagnostic image + `--unsafe-host-paths` + runner also at host `/opt/runner`: `--version` | 153 x3 | `the payload died on signal 25` (SIGXFSZ, Q4) |
| same with `DOTNET_EnableWriteXorExecute=0`: `--version`, `config.sh --help` | **0 x3** | `2.338.2`; `Commands:` |

Verdict, Observed: **the unmodified runner does not start on podvm's default rung.** Two blockers, each reproduced:

1. Exit 154. In the same image `readlink /proc/self/exe` prints `/usr/bin/readlink` but `realpath /proc/self/exe` prints `/usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2` (3 runs each). The .NET apphost looks for `Runner.Listener.dll`
   in the loader's directory. That the apphost calls libc `realpath` is Inferred from the two controls and the error text (apphost source not read).
2. Exit 137 once blocker 1 is worked around (the symlinks must be relative; an absolute target is followed by the kernel on the host). `strace` shows CoreCLR re-opening its own libraries with a path that already carries the rootfs prefix
   and the interposer prefixing it again: `$WORK/xdg/podvm/rootfs/<digest>/rootfs$WORK/xdg/podvm/rootfs/<digest>/rootfs/opt/runner/bin/libcoreclr.so` gives ENOENT (first open of `libcoreclr.so` is correct).
   This is the T-1643 double-prefix class for a path that comes from the loaded object (dladdr or maps, Inferred).
   `--unsafe-host-paths` (no anchoring) with the runner at its real host path avoids it and runs the runner (exit 0 with W^X off); that is not isolation.

Not an exit-0 path for the target design: the working route needs the host root and a host install path (T-1648, T-1407 flag semantics). `-v` is Degraded as R1 said; refusals above are verbatim.

### Q4 RLIMIT_FSIZE (04, `logs/04-fsize.log`)

- Observed: host default `Max file size 21474836480 21474836480 bytes` (soft = hard = 20 GiB, `/proc/self/limits`). It cannot be raised here (`sudo prlimit --fsize=unlimited` fails EPERM), so "unlimited" was not tested.
- Observed, native without podvm (2 reps per cell): `Runner.Listener --version` with default W^X exits **153 (SIGXFSZ) at every limit tried, including the 20 GiB host default**. With `DOTNET_EnableWriteXorExecute=0` it exits 0 at the default, 1 GiB, 64 MiB and 1 MiB.
  Mechanism, Observed in `strace`: `memfd_create("doublemapper")` then `ftruncate(fd, 2199023255552)` (2 TiB) returns EFBIG and raises SIGXFSZ. Any FSIZE below 2 TiB kills a W^X .NET process.
- Observed inside podvm (userland rung, `--unsafe-host-paths` diagnostic case, 2 reps): host default and `prlimit --fsize=1073741824` on the podvm process: W^X on gives exit 153 ("the payload died on signal 25"); W^X off gives exit 0.
  `--fsize=1048576` with W^X off gives exit 153 after about 1.3 s; whether podvm's own writes or the payload were signalled was not separated.
- On the default anchored rung the runner never reaches the point where FSIZE matters (exit 137, Q3). So the interaction is observed inside podvm only on the `--unsafe-host-paths` route.
- Observed: podvm has no limit flag. `--ulimit` exits 125 ("in the parity table with status None: per-container rlimits are not applied; the payload inherits this process's limits"), `-m` exits 125 ("resource limits need a cgroup"), `--podvm-mem` exits 125 ("needs --podvm-tier=machine").
  Control: `prlimit --fsize=1MiB` on podvm with an alpine `dd` of 3 MiB exits 153, so the limit does reach the payload.
- Inferred: a unit that sets an FSIZE ceiling below 2 TiB must also set `DOTNET_EnableWriteXorExecute=0` for the runner and for any .NET tool in a job.

### Q5 lifecycle (05, `logs/05-lifecycle.log`)

- Observed: **grandchildren survive `stop -t 1` and `rm -f`.** Payload `sh -c 'sleep N & sleep N.5 & wait'` under `run -d`; 3 repetitions each on debian:stable-slim and alpine, 6 of 6. Before stop, 4 processes carry the unique number (launcher, sh, 2 sleeps).
  After `stop -t 1` the launcher and sh are gone and both sleeps remain with ppid 1; they are still there after `rm -f`, and `ps -a` shows no container. The control (payload `sleep N` itself, 2 reps) leaves nothing.
  A 60 s destroy acceptance fails for any payload that forks, which the runner does. The leak check must match by argv word: on the loader family argv is `ld.so --library-path ... --argv0 sleep <path> N`, so a `sleep N` substring misses alpine children (my first scan did).
- Observed start cost of `podvm run --rm alpine true`, userland rung, 3 runs each: cold (image removed, re-pulled through the proxy, extracted) 5.36, 4.85, 4.83 s; warm (image in store, `--rm` re-extracts) 0.26, 0.26, 0.26 s;
  warm without `--rm` 0.26, 0.09, 0.10 s. The first cold run of the session was 5.0 s. Cold is dominated by the registry round trip through this proxy. Single warm `--rm` runs of larger images (`logs/02`): debian 2.3 s, node 4.5 s.

### Q6 shared rootfs (06, `logs/06-shared-rootfs.log`)

- Observed: **R1 confirmed.** A polls `/etc/marker` for 10 s, B writes it at t=3, C reads it. In all three concurrent scenarios (A,B,C all `--rm`; none; only C `--rm`) A saw B's write from t=4 and C, started while A ran, read `from-B`.
- Observed: a write also survives into later runs. Sequential controls with a cleared marker: B writes, then a later C reads `from-B` in all four combinations when container records exist (14 to 24 records: every foreground run leaves an Exited record). With records cleared first (0 records), B `--rm` then any C reads nothing (the rootfs was deleted), B without `--rm` then C reads `from-B`.
  So `--rm` deletes the shared rootfs only when no container record references it; the "fresh unit per job" property needs `--rm` and an empty container table. (The all-`--rm` scenario's final reader also read `from-B` because earlier plain runs had left records.)
- Observed: `rmi` does not remove the extracted rootfs (a marker survived `rmi alpine` plus re-pull in my first version of the script; the script now clears the marker instead). Unverified: whether `rmi` is meant to.
- Observed: `--add-host svc:10.0.0.1` in run 1 and `--add-host svc:10.0.0.2` in run 2 of the same image, overlapping: run 1, reading `/etc/hosts` after run 2 started, saw `10.0.0.2 svc`. Two concurrent units of one image cannot have different host entries.

## Surprises (against R1 or the plan)

1. The default rung here is userland and there is no switch to force it; the plan's "force the userland rung" has nothing to turn.
2. The unmodified official runner fails on the default rung in two layers (apphost `realpath("/proc/self/exe")`, then CoreCLR's doubled path). R1 said A-V-04 was "mostly done" on the strength of Node and gcc proofs and flagged .NET as unproven; it is now a measured failure.
3. FSIZE: any limit below 2 TiB kills a W^X-enabled .NET process (2 TiB `ftruncate` of a memfd), and the 20 GiB host default already does. The remedy is `DOTNET_EnableWriteXorExecute=0`, not a bigger number.
4. Orphaned grandchildren survive `stop` and `rm -f` (R1: "not signalled", confirmed; the new fact is that `rm -f` does not reap either and the container table forgets them).
5. `--rm` is not a clean-slate guarantee while any record exists (Q6); R1 said it "deletes and re-extracts". Every foreground run leaves a record, so records accumulate (24 after a few dozen runs).
6. The brief's node probe `fetch('http://127.0.0.1:1')` tests nothing about the network (undici bad-port). Use another port.
7. `--podvm-tier=chroot` on a chroot-denied host silently enters userland.
8. `podvm doctor` exits 1 when the store's filesystem has under 20 GiB free (the file-size ceiling), though no guest is involved.
9. The first program of `run` is resolved against the image root, not `-w`: `-w /opt/x ./prog` exits 127.

## What this does not establish

- One host class only (LXC, user namespace without chroot or mount, kernel 6.12, x86_64). No chroot rung, namespace rung or machine tier was run.
- The runner was started with `--version` and `config.sh --help` only: no registration, no job, no traffic to a forge. Whether it works under `--unsafe-host-paths` beyond that is Unverified.
- The 2 TiB `ftruncate` was observed natively; inside podvm only the exit code and signal were observed.
- The apphost `realpath` and the double-prefix producer are Inferred. Which process died at `--fsize=1MiB` is not separated.
- Timings: one host, 3 runs, shared proxy, a disk that was nearly full and shared with other work.
- `-v` was not run with a static payload and not measured on the memfd family beyond its refusal. `-u`, `exec`, `logs`, `cp` and `--name` were not measured.
