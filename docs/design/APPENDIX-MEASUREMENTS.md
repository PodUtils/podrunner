# Appendix: measurements, M2 on podvm and M1 on the official runner

## M2 on podvm's userland rung

`SPEC.md` §10.5 summarises this. The source is `measurements/m2-podvm/README.md` and its logs. Labels follow the SPEC: Observed, Source, Documented, Inferred, Unverified.

Conditions (Observed, one host): Debian 13, podvm 0.1.0-beta.13, uid 1000, `systemd-detect-virt` lxc. The host denies chroot, mount and namespace mounts. It allows `unshare(CLONE_NEWUSER)`, which podvm does not use, opens `/dev/kvm` and `/dev/fuse` read-write and allows ptrace (`logs/01-probe-rung.log`). It matches H0 on chroot, mount and namespace mounts and is more permissive than H0 on the other three. Results that depend on user namespaces, KVM or ptrace are not shown for H0.

### Rung selection

- `probe` selects `supervise`, and the first `run` banner says `mode=userland`. The rung cannot be forced by name. `--podvm-tier=chroot` on this host silently enters userland. `--podvm-tier=machine` exits 125 (C1).
- Node, Debian bash, alpine and a static payload (with `--unsafe-host-paths` only) run. The memfd family takes static payloads only (`logs/02-images.log`).

### The official runner

- **It does not start.** Blocker 1, exit 154: the .NET apphost looks for `Runner.Listener.dll` beside the loader (`/usr/lib/x86_64-linux-gnu/Runner.Listener.dll`), because libc `realpath("/proc/self/exe")` returns the loader's path while the interposed `readlink` returns the program's (Observed: controls in `logs/03-runner-payload.log`; that the apphost calls `realpath` is Inferred). Blocker 2, exit 137 after a relative-symlink workaround: CoreCLR opens its own libraries with a rootfs prefix that the interposer prefixes again (strace, Observed; the cause is Inferred, the T-1643 class).
- With `--unsafe-host-paths` and the runner installed at its real host path, `--version` and `config.sh --help` exit 0, with `DOTNET_EnableWriteXorExecute=0`. That is not isolation, and the podvm driver never uses it.
- `-v` is Degraded: the program inside a volume must start from `sh -c`, `:ro` is refused until T-1655, a workdir inside a volume is refused.
- The first program of `run` is resolved against the image root, not `-w`.

### Limits

- podvm has no limit flags: `--ulimit`, `-m` and `--podvm-mem` exit 125. A `prlimit` on the podvm process reaches the payload.
- Observed with `--unsafe-host-paths`: W^X on gives exit 153 under the default limit and under 1 GiB, W^X off gives exit 0, and a 1 MiB limit with W^X off gives exit 153 (which process was signalled is not separated). On the anchored rung the runner never reaches the point where `fsize` matters.

### Cost

- `podvm run --rm alpine true`: 4.83 to 5.36 s with an empty store (a registry round trip through the host's proxy) and 0.26 s with the image in the store (3 runs each).
- A run of the 781 MiB runner image takes 5.4 to 6.1 s in the cases that extract it (3 runs each, `logs/03-runner-payload.log`), dominated by `--rm` re-extraction.
- `podvm doctor` exits 1 when the store's filesystem has less than 20 GiB free (the file-size ceiling), with no guest involved. The driver's `probe` does not treat that as a refusal.

### Lifecycle and shared rootfs

- For a payload `sh -c 'sleep N & sleep N.5 & wait'` under `run -d`, `stop -t 1` and `rm -f` both exit 0 at once, and both `sleep` processes survive with ppid 1, after `rm -f` too (6 of 6 runs, `logs/05-lifecycle.log`). The launcher has ppid 1 before any stop.
- All containers of one image share one mutable rootfs, `<store>/rootfs/<digest>/` (`logs/06-shared-rootfs.log`). A file written by one container is read by a concurrent container of the same image. A write survives into later runs. `--rm` deletes the shared rootfs only when no container record references it, and every foreground run leaves a record. Two concurrent runs with different `--add-host` values for one name see one value.

## M1 on the official runner

`SPEC.md` §8.2, §10.4 and §11.3 use these results. The source is `measurements/m1-runner/README.md` and its logs. Conditions (Observed): runner v2.338.2, synthetic JIT blob, no job ran, offline.

### Credential exposure (Q3, Q7)

- The runner takes the JIT blob as `--jitconfig` or `ACTIONS_RUNNER_INPUT_JITCONFIG`. The argument puts it in `/proc/PID/cmdline` of `run.sh`, `run-helper.sh` and `Runner.Listener`. The environment form leaves it in `/proc/PID/environ` of the same three processes for their whole life: about 330 samples each, 2 of 2 rounds (`logs/03-jitconfig.log`). The runner's own `Environment.SetEnvironmentVariable(..., null)` (`CommandSettings.cs:111-140`) does not change that block. No window closes.
- The blob is `base64(JSON{filename: base64 bytes})`. The runner writes `.runner`, `.credentials` and `.credentials_rsaparams` (the private key) into its root with mode 644 (`Runner.cs:234-256`, no chmod) and does not delete them at exit.
- With the three files pre-staged at mode 600 or 400 and no argument and no variable, the listener starts, authenticates, creates a session and long-polls. It does not rewrite or delete the files. `cmdline` and `environ` of the three processes hold none of the markers (`logs/07-prestaged.log`).
- Source (R4:2.1): the Worker is a child process and reads `.runner` from disk (`JobRunner.cs:47`, `ConfigurationStore.cs:285-287`). Inferred: steps run as the runner's uid. Unverified: whether the Worker and step processes inherit the variable.

### Start-up requirements and `RLIMIT_FSIZE` (Q1, Q2, Q6)

- With the default W^X the .NET runtime calls `memfd_create("doublemapper")` and `ftruncate(8, 2199023255552)`, exactly 2 TiB (`logs/02-fsize.log`, `04-fsize.log`). Under any `RLIMIT_FSIZE` below that the call returns EFBIG, the kernel sends SIGXFSZ and the process exits 153. The runner died at start under 20 GiB (the sandbox's own default hard limit), 1 GiB, 128 MiB and 1 MiB, in the listener, in `config.sh --help` and in the Worker. With `DOTNET_EnableWriteXorExecute=0` it always started.
- Ignoring SIGXFSZ in the runner's parent is a third way to start under a small limit (Observed: the runner starts). That a step then sees EFBIG instead of a kill is Inferred, no step ran. A limit of 2 TiB or more was not run (Unverified, inferred yes: the hard limit could not be raised).
- `DOTNET_SYSTEM_GLOBALIZATION_INVARIANT=1` or ICU is required (without both the process exits 134). OpenSSL (`libssl.so.3`) is required where the credential path runs (without it the JIT path exits 2). The runner needs a glibc dynamic loader at `/lib64/ld-linux-x86-64.so.2` (all 17 ELF files of `bin` are dynamic, so musl is not enough) and glibc symbols up to `GLIBC_2.28`. Observed on glibc 2.41 only.
- A host with `vm.memfd_noexec` set would also break the W^X memfd (Inferred, never run). M2 Observed `vm.memfd_noexec=0` on its host.

### The stale-runner exit (Q4)

Exit 7 comes from `GET /message` answering the error kind `RunnerVersionTooOld`, not from session creation. `run.sh` returns 7 only for the string `1`, and `Runner.Listener run` returns 7 for `true` or `1`. A `POST /session` answering 403 gives exit 1. The real status code GitHub uses is Unverified: the runner keys on the body (`"source":"actions-broker-listener"`).

### The hook and the fake server (Q5, Surprise 8)

The hook response shape is Source (`HookResponse.cs`), checked with the author's own mirror of the runner's checks. No runner drove the example hook (`hook-example/hook-log.sh`). The fake server serves `POST /oauth/token`, `POST /session`, `GET /message`, `DELETE /session`. A JIT blob whose `ServerUrl` differs from `ServerUrlV2` also makes the runner request `{ServerUrl}/_apis/connectionData`: a fake serves it or sets both URLs equal.

### Release cadence of the official runner (§11.3)

Observed 2026-11-10 by the fixer: the release list of `actions/runner` read through the GitHub releases API without an account (one request, `per_page=30`). Twelve releases from v2.331.0 to v2.338.2, one of them a prerelease (v2.338.1). Date of publication and days since the previous release:

| Tag | Published | Days after the previous |
| --- | --- | --- |
| v2.331.0 | 2026-01-09 | |
| v2.332.0 | 2026-02-25 | 47 |
| v2.333.0 | 2026-03-18 | 21 |
| v2.333.1 | 2026-03-27 | 9 |
| v2.334.0 | 2026-04-21 | 25 |
| v2.335.0 | 2026-06-08 | 48 |
| v2.335.1 | 2026-06-09 | 1 |
| v2.336.0 | 2026-07-20 | 41 |
| v2.337.0 | 2026-08-26 | 37 |
| v2.338.0 | 2026-10-06 | 41 |
| v2.338.1 (prerelease) | 2026-10-22 | 16 |
| v2.338.2 | 2026-10-26 | 4 |

The gaps among these twelve run from 1 to 48 days. The window is the releases of 2026, a cut made after the fetch. The same response (30 releases) holds 18 older ones, from v2.330.0 (2025-11-19) back to v2.316.1 (2024-05-02), two of them prereleases. Between consecutive releases of all 30 the gaps run from 1 to 72 days (the longest, 72, is v2.322.0 to v2.321.0). Observed 2026-11-10, one request, `per_page=30`. `recon/R4-upstream-runners.md` lines 91 and 443 say "5 to 7 weeks"; the table contradicts them (gaps of 1, 4, 16 and 37 days occur within the dates that line 91 lists). The recon files are frozen, so the correction is here.
