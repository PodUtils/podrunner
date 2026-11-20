# podrunner: the hard problems (research brief)

Date: 2026-11-20. Read-only study of the record at `main`.

## 0. How to use this report

This report lists each hard, blocked or "impossible" problem of podrunner.
A local agent with more tools and network can research each problem. A
future session of podrunner can then use the answers.

Sources read: `AGENTS.md`, `docs/HANDOFF.md`, `TODO/PROGRESS.md`,
`TODO/INDEX.md`, the entries of `TODO/later.md`, `TODO/live.md`,
`TODO/core.md`, `docs/decisions.md`, `docs/non-goals.md`,
`docs/unknowns.md`, `docs/design-issues.md`, `docs/security/*.md`,
`docs/container-engine.md`, `docs/hosted-snapshot.md`, `TODO/issues.md`,
and the reports `hosted-parity-2026-11-19.md`,
`hosted-snapshot-2026-11-19.md` and `census-2026-11-18.md`. The read-only
issue mirror returned no JSON when this study ran, so the open GitHub
issues come from `TODO/issues.md` only.

Marks:

- **[M]** measured: the record names a run id or a command output.
- **[E]** estimate or claim: no measure exists.
- **Verdict** values: `impossible` (under the rules below), `partly
  impossible`, `hard`, `blocked` (waits for another project), `unproven`
  (code exists, no run proves it).

The rules that make some problems impossible (`AGENTS.md`, section 2, and
`docs/decisions.md`):

1. No root. No namespaces assumed. No docker daemon on the host.
2. Each shipped binary is static (musl). C, zig and new Rust are allowed.
3. The only way out of a unit is a proxy (the egress module, then podnet).
4. The official `actions/runner` (.NET) is the payload (an alternate
   payload is allowed by the ruling of 2026-11-14, T-161).
5. Release proof levels: CI on GitHub-hosted runners, a constrained host
   as a user, not run. "Ready" needs the second level.

Project map: podcrates (`pod-*` crates), podvm (unit runtime: userland
rungs, namespace rung, machine tier, incus tier), podnet (`podnet pod`,
the egress hold), podssh (on break). podrunner edits none of them. It
sends COORD issues.

## 1. Top 10 to research first

| Rank | Id | Title | Why first |
| --- | --- | --- | --- |
| 1 | H-7 | Does the scrubbed hosted snapshot boot with systemd as PID 1 in an unprivileged incus instance? | The owner's ruling depends on it. Four labels are published; no boot of any of them has a recorded result. |
| 2 | H-9 | A 55 to 60 GB image on units with a 40 GB disk bound | Blocks the snapshot on githome's host. No copy-on-write path is known for a no-root host. |
| 3 | H-23 | GitHub's scale set binds no runner to a job (T-175, T-169, U6 to U23) | Security and capacity hole. Needs facts about GitHub's undocumented behaviour. |
| 4 | H-16 | `podrunner run --vnet` hangs on a host that refuses the bind (T-561) | A user-visible hang in a host class that podrunner exists for. Cause unknown. |
| 5 | H-2 | The podvm syscall tier breaks Node and hangs a cargo build (T-261) | Blocks uv and ruff rows. Touches Go and Node signal handling. |
| 6 | H-8 | Boot the hosted Azure kernel under QEMU/KVM from an OCI image, with no root | The ruling asks for the hosted kernel. Needs a disk image made without root. |
| 7 | H-5 | A disk bound that gives `ENOSPC` with no root (T-496) | githome's ask G-18. No kernel feature is known for the userland rungs. |
| 8 | H-18 | Placeholders for secrets: TLS interception and clients that sign (T-563) | Some clients cannot work with a swapped placeholder. Needs a client census. |
| 9 | H-13 | ghcr.io limits for 16 GiB layers, pull rates and "keep each version" | Real limits are unmeasured. Storage grows about 55 to 65 GiB each week [E]. |
| 10 | H-20 | GitHub's refusal of an old runner (T-499) | A wrong guess takes the whole fleet down on a Monday. |

Next after these: H-24 (warm units), H-26 (an engine in an incus instance
keeps `confined`), H-12 (capture correctness), H-15 (an enforced hold with
no user namespace).

## 2. Summary table

| Id | Theme | Entries | Verdict |
| --- | --- | --- | --- |
| H-1 | Isolation | T-300, residuals R1, R2 | partly impossible |
| H-2 | Isolation | T-261, COORD-098 (#308), #304, #307 | hard |
| H-3 | Isolation | T-403, T-532, T-405, #277, #300 | partly impossible |
| H-4 | Isolation | residual R7, T-519, T-563 | impossible with the official runner |
| H-5 | Isolation | T-496, T-482, T-521, T-221, #296 | partly impossible |
| H-6 | Isolation | T-194, T-197, non-goals | impossible at usable speed |
| H-7 | Hosted | T-543, T-556, #313, #316 | unproven |
| H-8 | Hosted | T-544, T-559, #316 | blocked, hard |
| H-9 | Hosted | T-551, T-545, T-557 | hard |
| H-10 | Hosted | T-558, T-560 | partly impossible |
| H-11 | Hosted | T-538, T-539 | impossible in container modes |
| H-12 | Hosted | T-542, T-555, T-540 | hard |
| H-13 | Hosted | T-555, T-553, ruling "Retention" | unproven |
| H-14 | Hosted | T-547, T-546, T-539, T-550 | hard |
| H-15 | Egress | T-041, T-044, residual R5 | impossible with no user namespace |
| H-16 | Egress | T-561, T-220, #167, #170 | hard |
| H-17 | Egress | T-351, T-371, T-372 | hard |
| H-18 | Egress | T-563, T-524, G-04, G-35 | partly impossible |
| H-19 | Release | T-082 | blocked (upstream) |
| H-20 | Release | T-499 | unproven |
| H-21 | Release | T-484, T-553, T-562, T-523 | unproven |
| H-22 | Release | T-243, T-244, T-246, T-372, T-249 | hard |
| H-23 | GitHub, githome | T-169, T-175, T-032, T-033, T-040, U6 to U23 | blocked (live) |
| H-24 | githome, cfb | T-487, T-480 | hard |
| H-25 | githome | T-506 | unproven |
| H-26 | githome | T-485, T-516, T-517, T-103 | hard |
| H-27 | githome | T-161 | hard |
| H-28 | githome, cfb | T-533, T-197, T-151, U26 | partly impossible |
| H-29 | githome | T-512, T-509, T-493 | blocked (people) |
| H-30 | CI | T-561, T-192, T-159, T-180, T-261, T-478 | hard |

Count: 30 problems.

---

## 3. Theme A: unit isolation with no root

### H-1. No isolation on the podvm userland rungs

Entries: T-300, T-042, T-045, T-079; `docs/security/residuals.md` R1, R2;
`docs/security/threat-model.md` actors 2 and 3.

Why it is hard. The podvm userland and supervise rungs rewrite paths. They
do not isolate. Podvm says each rung is `redirect`
(`crates/podrunner-podvm/src/probe.rs`). A static binary that the job runs
can make raw system calls and read the files of the uid. The loader
interposer (`LD_PRELOAD`) does not see raw calls. Podrunner accepts this
for `trusted` jobs at `redirect` (ruling DR-4, 2026-11-11). Podvm owns the
rungs.

Known.

- `[unit.podvm] confine = true` runs a unit under `podvm --podvm-confine`
  (Landlock plus seccomp). It says `confined` only after a self-test on
  the host (T-042, T-079). Landlock is absent on an emulated host
  (ENOSYS), so no tier above `local` works there (T-194).
- The call of 2026-11-16 (T-300): a podvm unit off the namespace rung is
  no boundary for a static binary. `doctor` warns. The documents name
  `--microvm` for untrusted jobs.
- The confine layer denies `/proc/PID/environ`, `root` and `cwd` of other
  processes of the uid (T-079). It has three limits (COORD-196, #307):
  `exec=` on the syscall tier, `exec=` through the dynamic loader, and
  `chmod` and `utimes` outside the rules. Podrunner judged that these do
  not weaken its confined kind.
- GitHub-hosted runners restrict unprivileged user namespaces with
  AppArmor (Ubuntu 24.04). Many CI hosts land on the supervise rung.

Open questions.

1. Which Landlock ABI version is the floor for useful confinement (network
   rules need ABI 4, scoping of signals and abstract sockets needs ABI 6)?
   Which kernels in the field have them? Can Landlock scope rules replace
   the lost `/proc` denials for the same-uid signal problem (R2)?
2. Can seccomp user notification (`SECCOMP_RET_USER_NOTIF`) or Syscall
   User Dispatch (SUD, `PR_SET_SYSCALL_USER_DISPATCH`) cover raw calls for
   a path-rewriting layer with no root? What are the known bypasses
   (TOCTOU on user-notify path arguments, `io_uring`, `openat2`,
   `clone3` arguments)?
3. What is the practical attack surface of `io_uring` for a no-root
   confined process? Is the sysctl `kernel.io_uring_disabled` readable and
   usable as a probe fact?
4. Can `pmask` (the owner's note in DR-6) cut same-uid reads? Its design is
   not in this repository. Ask the owner.

Approaches and trade-offs.

- A: keep `redirect` and warn (today). Cheap. Honest. No isolation.
- B: require the namespace rung or a guest for `trusted` (the one-line
  change named in the T-300 call). Strong. Many hosts then serve no
  trusted job.
- C: Landlock-only confine with an exec allow list. No root. Gaps listed
  above.
- D: a distinct uid per unit. Needs root. Not allowed.

Proof of a solution. A hostile static binary (made-up test) that reads
`/proc/PID/environ` of a sibling, reads the runner credential file of a
sibling unit and writes outside its root fails on each host class. Run on
a constrained host as a user (not only in CI).

Verdict: partly impossible. Full isolation with no root, no namespaces and
no KVM does not exist. The rules accept `redirect` for `trusted` jobs.

### H-2. Raw system calls and the podvm syscall tier

Entries: T-261, T-262, T-554 (done), COORD-066 (#189), COORD-091 (#301,
closed), COORD-093 (#303, closed), COORD-094 (#304), COORD-097 (#307),
COORD-098 (#308); podvm PodUtils/podvm@T-1991, @T-1982, @T-1995.

Why it is hard. On the userland rung, a loader interposer maps the paths
of libc calls. Libraries that issue raw `syscall` instructions
(`rustix` in `tempfile`, `uv`, `ruff`; libuv with `io_uring`; Go's
runtime) skip it. Podvm added a `--podvm-tier=syscall` that traps raw calls
with a SIGSYS handler on its native-static rung (x86_64 only). The tier
fixed `click-py313` and `rq-lint` and broke other things.

Known.

- Node of `setup-node` in `_work/_tool` failed with
  `native-static rung: ... ld-linux-x86-64.so.2: errno 11` under the tier.
  Podvm says errno 11 is an `openat2` EAGAIN. It retries it on its `main`
  (COORD-197, PodUtils/podvm#593). Podvm 0.2.0-beta.36 carries the retry.
  Podrunner still gives no tier (`SYSCALL_TIER = None`).
- The cargo build of `rg-docs` hangs under the tier (user-gate run
  44393864747). Podvm does not reproduce it. Podvm asks for `uname -r`,
  `ps -eo pid,ppid,stat,wchan:32,args`, the stack and the fds of the
  waiting process.
- The TypeScript 7 (`tsgo`, a Go binary) hang with no tier was a signal
  bug: Go preempts a goroutine with SIGURG. When that stopped podvm's
  SIGSYS handler in a system call, Go's handler ran on the wrong stack and
  waited for ever. Fixed in PodUtils/podvm@T-1995 (beta.36). User-gate run
  44489222156 passes `rbg-gate` on podvm [M]. The `cargo doc` hang may have
  the same cause. Not yet re-measured with the tier on.
- The syscall tier broke the container hooks twice (relay unix socket,
  absolute symlinks). Both are fixed in beta.33 and beta.34.
- The tier is x86_64 only. An aarch64 host keeps the uv and ruff faults.
- With `--podvm-confine`, the tier made `run -d` fail (Landlock denied the
  exec of podvm). Fixed on podvm `main` (@T-1982).

Open questions.

1. Does a SIGSYS handler in a program with a Go runtime need
   `sigaltstack` and `SA_ONSTACK`? Is it safe to take SIGSYS on the
   goroutine stack? Which signals does Go forward? (The `SA_RESTART` and
   `SA_ONSTACK` flags of Go's handlers.)
2. Is SUD (Linux 5.11 and later, an unprivileged `prctl`) a better trap
   than seccomp-SIGSYS? It has a per-thread selector byte and no
   re-entrancy trouble in the handler. What breaks (vDSO, `rt_sigreturn`,
   signal frames)?
3. How does `openat2` return EAGAIN here? (`RESOLVE_CACHED`? A concurrent
   rename?) Which callers must retry?
4. Why does a cargo build hang? A futex wait in a thread that waits for a
   signal? A missing wake-up when the handler runs `futex` itself?

Approaches.

- Take the namespace rung and avoid the tier. Works only with user
  namespaces.
- Fix the tier in podvm. Needs the data of question 4 from a real hang.
- Patch the jobs: `UV_USE_IO_URING=0` (done for Node), tool-specific
  variables. Does not scale.

Proof. `user-gate.yml` with `SYSCALL_TIER` on: the rows `click-py313`,
`rq-lint`, the express rows, `rg-docs` and `rbg-gate` all pass in the lanes
`plain`, `proxy-only`, `inherited-fds`, `small-tmp` on the podvm executor.

Verdict: hard. Owner: podvm. A researcher can give the signal-stack and
SUD facts.

### H-3. A symlink race on the mount sources of container hooks

Entries: T-403 (blocked, P1), T-532 (open, P1), T-405 (done), T-404
(done), COORD-081 (#277), COORD-090 (#300).

Why it is hard. The hooks run `container:` jobs and Docker actions. The
policy checks the text of a mount source. A step can swap a directory for a
symbolic link after the check and before the host opens it. The container
then gets the root of the host, writable. The check (`HookUnit::spec`)
refuses links that exist at check time. The race remains.

Known.

- Podvm 0.2.0-beta.32 has `--podvm-volume-base DIR`: a `-v` source under
  DIR opens with no symbolic link and binds by descriptor. It holds on the
  namespace rung only. The userland rung and the guest tiers refuse it.
- The containers of a podvm unit use the supervise and userland rungs
  today (U19), so the flag cannot close the race.
- Podvm says `run -d -v` binds on the namespace rung since beta.25.
  Nobody has measured the container suite of the hooks there (T-532
  step 1).

Open questions.

1. Can `openat2(RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS)` plus a bind by
   `/proc/self/fd/N` close the race on the supervise rung with no mount
   namespace? U25 says the userland rung accepts `/proc/self/fd/N` as a
   `-v` source. Does that bind the descriptor or the path?
2. Can a step change the object after podvm opens it (rename of a parent
   while the descriptor stays valid)? What does the descriptor hold: the
   directory, not its name. Is that enough?
3. Is a `chroot` or path rewrite on the userland rung able to give the
   same guarantee? (Likely no.)

Approaches. (a) Measure the container suite on the namespace rung and use
`--podvm-volume-base` there (T-532). (b) Ask podvm to port the flag to the
supervise rung (PodUtils/podvm@T-1972). (c) Remove the dynamic mount
sources: copy the files into the container instead of binding them.

Proof. A hook test where a step swaps a mount source for a link after the
check gets exit 125 of podvm and `container-option-refused`.

Verdict: partly impossible on the userland and supervise rungs without
podvm kernel-side help. Solvable on the namespace rung.

### H-4. A step can read the credential of its own runner

Entries: `docs/security/residuals.md` R7, T-519 (done: the document),
T-563 (open), githome G-03 and G-35.

Why it is impossible with the official runner. The official runner reads
`.runner`, `.credentials` and `.credentials_rsaparams` from its root as the
user of the job. The steps run as the same user. So a step can read them
on every unit kind. The credential lives for one job (GitHub binds a JIT
configuration to one job, T-175 shows the limit of that). Podrunner stages
the files at mode 0600 and removes them at destroy.

Known. The design asks for files only (C13). The strong form,
`credential_delivery: socket`, is a non-goal (needs a forge with a runner
that takes its credential from a socket).

Open questions.

1. Does the official runner read the credential files once at start? If
   yes, can the host delete the files after the runner has read them
   (unlink after the session opens)? Does any later code path (token
   refresh of the Results service, a `.credentials` renewal) need them?
   U20 asks the same for "survive a token refresh over hours".
2. Can the runner and the steps run as two uids inside a guest or an incus
   instance (the runner as root-in-guest, the steps as `runner`)? The
   runner starts the steps itself, so it would need a drop of privilege per
   step. Does the runner support `RUNNER_ALLOW_RUNASROOT` plus a wrapper?
3. In which message does the runner send the OAuth token of the job
   (`ACTIONS_RUNTIME_TOKEN`) and the system token (`GITHUB_TOKEN`)? A step
   sees both by design. Only the runner credential is extra.

Approaches. (a) Accept (today). (b) Unlink after read, if question 1 says
yes. (c) An alternate payload (H-27) that keeps its credential in memory.
(d) Placeholders for outbound secrets (H-18) do not help with the runner
credential itself, because it is the credential for GitHub.

Proof. A step that tries to read each of the three files fails, and the job
still runs to the end, in a unit with a runner that lives for two hours.

Verdict: impossible with the official runner as it is. Options (b) and (c)
may change that.

### H-5. A disk bound that gives `ENOSPC` with no root; CPU and pids bounds

Entries: T-496 (blocked, P1), T-482 (partial), T-521 (open), T-221 (done),
T-162, T-515, T-529; COORD-089 (#296) ask 5; COORD-050 (#110).

Why it is hard. githome asks for a size for the writable layer and the
work directory (G-18). A job past it must get `ENOSPC`. Without root a
process cannot make a sized file system. Podrunner measures the bytes under
the work directory every 30 s and ends the job with `limit`
(`crates/podrunner-slots/src/disk.rs`). A fast writer can fill the disk
inside 30 s.

Known.

- Incus kind: podvm 0.2.0-beta.33 sizes the root disk
  (`--storage-opt size=N`). It refuses a size on a pool with no quota (the
  `dir` driver, exit 125). `[unit.incus] root_quota = true` enables it.
  Measure of `ENOSPC` on a real instance: not run. The cloud host of
  podvm has a `dir` pool only.
- Microvm kind: no sized disk. The guest root is an initramfs. Podvm asks
  for PodUtils/podvm@T-1968 (open).
- Podvm namespace rung: podvm proposes a tmpfs with `size=`, which uses
  memory, not disk.
- `pids` of the podvm kind is `RLIMIT_NPROC`. It counts each process of the
  uid, so two units of one user share it (T-521). The cgroup path needs a
  delegated subtree (T-221, `systemd-run --user`).
- Memory kills: podvm reports `oom_kills`; podrunner turns the job into
  `limit` (T-515). Not measured on real units.

Open questions.

1. Which file systems mount inside an unprivileged user namespace
   (`FS_USERNS_MOUNT`)? tmpfs, overlayfs (with `userxattr`), FUSE,
   `proc`, `sysfs`, `devpts`, `binderfs`, `ramfs`. Does kernel 6.x add
   others (erofs, ext4 loop with `fuse2fs`)? Which kernel versions does the
   fleet have?
2. Can `fuse2fs` or `squashfuse` with a sparse ext4 image give a hard size
   limit with no root? What is the performance cost for npm, cargo and git?
3. Does overlayfs inside a user namespace respect a size limit of its
   upper tmpfs? (Yes for tmpfs `size=`. Memory cost.)
4. XFS and ext4 project quotas need root once to set up. Can the operator
   do it one time at install, and can an unprivileged process then use a
   project id (`FS_IOC_SETFLAGS` / `FS_IOC_FSSETXATTR` with `projid`)? ext4
   and XFS allow `chattr -p` only with `CAP_SYS_RESOURCE` in the initial
   namespace. Verify.
5. Does Btrfs `qgroup` (root) or ZFS `refquota` (delegated with `zfs
   allow`) allow a delegated, no-root bound? (ZFS delegation: yes in
   principle.) Which hosts have it?

Approaches.

- A: measure and kill (today). Works everywhere. Not `ENOSPC`.
- B: tmpfs with a size for the work directory. Memory cost. Fine for small
  jobs.
- C: a sparse ext4 image mounted by `fuse2fs`. No root. Slow. Needs the
  tool in the host.
- D: incus pool with quotas (btrfs on a loop file, ZFS). Needs root once.
- E: shorten the measure interval and use `fanotify`/`inotify` for large
  writes. Not sound, but narrows the gap.

Proof. A job that writes 1 GiB with a 100 MiB bound fails with `ENOSPC` (or
`limit`) on each kind in `runner-image.yml`, and the host and neighbours
stay up. `cargo test -p podrunner-unit` conformance.

Verdict: partly impossible. Exact `ENOSPC` on the userland rungs with no
root probably has no solution. The measure-and-kill path stays the second
path on each kind.

### H-6. Untrusted jobs on a host with no KVM and no root

Entries: T-194 (done: doctor names it), T-197 (partial), T-151 and T-533;
`docs/non-goals.md` (last row), GitHub #146, #149.

Why it is impossible at usable speed. The tier rule needs `guest` for
`untrusted` jobs (forks). A guest needs KVM, or QEMU TCG. On an emulated
aarch64 host (a VM under `qemu-system-aarch64` with no KVM) the runner takes
about 2 min 45 s to start, and the default `start_s` of 120 s killed it
(#149). The hold needs seccomp, Landlock or a netns, and none of them runs
under QEMU user mode (Landlock is ENOSYS there).

Known. `start_s = 900` makes the job pass on such a host [M, #149].
`doctor` names the missing Landlock and hold.

Open questions.

1. How slow is the official runner in a TCG guest on x86_64 for a canned
   job? (T-533 measures the arm64-on-x86_64 case; U26 is open for a job.)
2. Does .NET run in `qemu-user` (not system mode)? Known problems: the JIT
   and W^X in QEMU user mode. Does `DOTNET_TieredPGO=0`,
   `DOTNET_TieredCompilation=0` or ReadyToRun images help?
3. Can nested virtualization be assumed on the hosted CI runners the
   owner uses? (`/dev/kvm` exists on GitHub-hosted Linux runners; arm64
   hosted runners: check.)

Approaches. Refuse (today). Run at TCG speed with a large `start_s` for
low-volume forks. Use a real host of the machine class.

Proof. A fork job passes in an untrusted guest on a host with no KVM within
`start_s`, or `doctor` refuses the tier by name.

Verdict: impossible at usable speed under the rules. Keep the refusal.

---

## 4. Theme B: the hosted snapshot and the hosted parity

Context. The owner's ruling (2026-11-19): copy the root of a GitHub-hosted
runner 1:1, with its kernel, fix what a unit needs, test it, publish it on
ghcr.io, and run it in the highest mode of podvm (incus system container or
VM, machine tier) as the hosted runner, with systemd, docker, podman and
FUSE. The owner accepts the licences and terms. Do not argue them again.

State of the snapshot [M] (T-542, T-555, `docs/hosted-snapshot.md`): four
labels are published and attested. `ubuntu24` amd64
`sha256:d00306d8...` (15.9 GiB zstd, 85 layers plus `boot`, capture
1,127,981 paths with C1 0 differences, 68 minutes, run 44503451991);
`ubuntu24-arm64` `sha256:512b882a...` (run 44557881914);
`ubuntu26` `sha256:d35cf32b...` and `ubuntu26-arm64` `sha256:1444cf65...`
(runs 44557874043 and 44557889203); indexes `ubuntu24-latest` and
`ubuntu26-latest`. A boot of the base slice (T-556,
`hosted-boot.yml`) has no recorded result.

### H-7. Does the scrubbed snapshot boot, with systemd as PID 1, in an unprivileged incus instance?

Entries: T-543 (blocked, P1), T-556 (partial, P1), T-541 (open, P1),
COORD-103 (#313), COORD-106 (#316), podvm PodUtils/podvm@T-1993, @T-1996,
@T-1997.

Why it is hard. Podvm is PID 1 of an incus instance today
(`raw.lxc lxc.init.cmd`). The hosted image needs its own `/sbin/init`. The
image has Azure units, a Docker daemon, MySQL, PostgreSQL, rootless podman
for `runner`, FUSE and a user with subordinate ids 165536:65536. The
scrub changes fstab, netplan, machine-id, SSH host keys and masks units.
Nobody has shown that the result boots clean.

Known.

- Podvm beta.36 has `--podvm-init=image` on the incus tier (the init of the
  image is PID 1, a bounded wait for `is-system-running`, the command by
  exec, the instance stops at its end). Podrunner has
  `[unit.incus] init = "image"` (it passes `--podvm-init=image
  --podvm-engine`, no own `dockerd`).
- Podvm main has PodUtils/podvm@T-1996: with `--podvm-init=image` an
  instance gets `security.idmap.isolated` and `security.idmap.size`
  231072, `/dev/fuse` and an empty `/etc/machine-id`. The host must give
  incusd 231,072 free subordinate ids for each instance. `doctor` should
  check it later.
- Podvm measures the disk of the first run at the extracted tree plus
  about 2.4 times it in incusd (T-1997 streams the import).
- `scripts/hosted/slice.py` makes a 2.4 GiB slice (6 of 85 layers:
  `base`, `usr-lib`, `usr-lib-arch`, `usr-share`, `snapd`, `boot`).
  `hosted-boot.yml` (report-only) should boot it. Next step is its first
  run.
- Nesting is in already: the snapshot runs with `--podvm-init=image
  --podvm-engine`.

Open questions.

1. What does `systemctl is-system-running --wait` say after the boot, and
   which units fail? Expected failures to allow: `hosted-compute-agent`,
   `primer`, `walinuxagent`, `cloud-init`, `pollinate` (no NIC). Others
   unknown.
2. How does `docker.service` start in an unprivileged incus container with
   `security.nesting=true`? Which storage driver works (overlay2 on the
   incus pool, or `fuse-overlayfs`, or `vfs`)? What breaks with a `dir`
   pool? Which AppArmor profile stacking is required?
3. Does `mysql.service` and `postgresql.service` start when the id map has
   one range? (The scrub drops the MySQL TLS keys; `mysqld` makes new
   ones at first start.) Do services that change user work in a mapped
   unprivileged container? (Yes with a full id map. Verify.)
4. How does systemd in a container treat `/etc/fstab` with `LABEL=` roots,
   `systemd-remount-fs`, `tmp.mount` (26.04) and `swap`? Which units must
   be masked?
5. Does systemd 255 (24.04) and 259 (26.04) run in a container whose
   cgroup namespace is delegated by incus? Does `DefaultLimitNOFILE=65536`
   apply?
6. Does rootless `podman run --systemd=always` work inside the instance
   (needs `newuidmap`/`newgidmap` setuid, `/dev/fuse`, sub-id ranges inside
   the instance)? What is the minimum id map?
7. What does a unit need from incus to run `losetup`, `mount -t fuse`,
   `modprobe`? (A container cannot load modules. See H-11.)
8. How long is the boot to `is-system-running`? What is the time to first
   step of the runner?

Approaches.

- A: boot the base slice (2.4 GiB) in hosted CI first (T-556). It proves the
  scrub.
- B: boot the full image on a live host with a CoW pool (btrfs/ZFS/LVM
  thin) (T-547).
- C: if systemd fails in a container, use incus VM mode (H-8).

Proof. `hosted-boot.yml` (or `hosted-snapshot.yml`): `is-system-running`
gives `running` or `degraded` with only allowed failed units; no Azure
or GitHub agent runs; a planted unmasked `walinuxagent.service` fails the
check; then, on a live host, `sudo systemctl start postgresql.service &&
pg_isready` and `docker run hello-world` pass.

Verdict: unproven. No known blocker is impossible. Needs the first run and
the answers above.

### H-8. Boot the hosted Azure kernel under QEMU/KVM from an OCI image, with no root

Entries: T-544 (blocked, P2), T-559 (blocked, P2), COORD-103 (#313),
COORD-106 (#316) asks 2 and 3, podvm PodUtils/podvm@T-1994 and @T-1998,
incus issue lxc/incus#1360.

Why it is hard. The machine tier today uses an initramfs root that must
fit twice in guest memory. The hosted rootfs is 55 GB. A root on a disk is
needed. Podvm must make that disk (GPT, an ESP from `/boot/efi`, ext4 root
labelled `cloudimg-rootfs`) with no root. Incus runs OCI images only as
containers; VM support for OCI is open (lxc/incus#1360).

Known [M, `modules.builtin` of the three hosted kernels]:
`virtio_pci`, `virtio_blk`, `virtio_net`, `virtio_scsi`, `virtio_console`,
`ext4`, `loop` and `fuse` are built in (x64 6.17 and 7.0, arm64 6.17).
`overlay`, `9p`, `virtiofs`, `vsock` are modules in
`/usr/lib/modules/KVER`. The hosted kernel also has `kvm-intel`, `kvm-amd`,
`vhost_vsock`, `nbd`, `v4l2loopback`, `br_netfilter`, `nf_tables` as
modules. Podvm today boots Alpine `vmlinuz-virt` 3.22.6.

Podvm's plan (COORD-203): T-1994 in three steps (a disk root with a
copy-on-write overlay for each guest; the image's init with a kernel that
systemd accepts; the exit status through the serial port), plus a choice to
boot the kernel and initrd of the image (`root=LABEL=cloudimg-rootfs`) with
the 9p and vsock modules of the image, and nested KVM. T-1998:
`--podvm-tier=incus --podvm-vm` makes a VM image once for each digest
(qcow2 root, ESP from `/boot/efi`, else a direct kernel boot).

Open questions.

1. Can the hosted initrd boot under QEMU with virtio only? (It was built
   for Hyper-V: `hv_vmbus`, `hv_storvsc`, `hv_netvsc`.) Is a direct kernel
   boot with `-kernel` and `-initrd` and `root=/dev/vda1` simpler?
2. Does the Azure kernel flavour (`linux-azure`) need Hyper-V enlightened
   timers or clocksource (`hyperv_clocksource_tsc_page`) and hang or run
   slow under KVM? Which command-line options help (`clocksource=kvm-clock`)?
3. How to build a GPT disk with an ESP and an ext4 root with xattrs and
   file capabilities with no root? Candidates: `mke2fs -d DIR` (does it
   copy xattrs? check e2fsprogs version), `mkfs.ext4 -d` from a tar,
   `guestfish` (not static), `genext2fs`, `erofs` images. What is the
   speed for 55 GB?
4. Does `mke2fs -d` keep `security.capability` and POSIX ACLs?
5. How to avoid a 3-times disk cost: the layers (tar), the extracted tree
   and the disk image? Is a read-only erofs/squashfs lower image plus an
   overlay on a small writable disk in the guest enough for systemd?
6. Does nested KVM work for the hosted-runner host class
   (GitHub-hosted x64 `/dev/kvm`: yes; arm64 hosted: unknown)?
7. Does the incus VM agent (`incus-agent`) run in the hosted image for
   exec? The agent is a separate Go binary; the image has none.

Approaches. (a) podvm direct kernel boot with the hosted kernel and a
virtio-blk root made by a rootless tool. (b) incus VM from a qcow2 made
once for each digest. (c) Keep Alpine's kernel and accept `uname -r`
differences (not 1:1; the ruling asks for the hosted kernel).

Proof. The base slice boots as an incus VM or machine-tier guest and
`uname -r` equals the kernel of the snapshot; then C4 to C7 on a live host.

Verdict: blocked on podvm; hard. Question 3 is a pure research item.

### H-9. A 55 to 60 GB image against a 40 GB unit disk and a host that needs about 190 GB

Entries: T-551 (open, P1), T-545 (open, P2), T-557 (open, P2), T-547
(open, P2), T-549, finding PF-4, F4, F5.

Why it is hard. The snapshot is about 55 GB on disk [M, capture:
54.5 GiB of file data]. githome gives each unit 40 GB (`disk_max_bytes`).
Podvm measured the incus tier at the extracted tree plus about 2.4 times it
in incusd: about 190 GB for one version on a host [M, COORD-203]. The podvm
userland kind loads the image tar into a new store for each unit, extracts
it, and copies the tree: about three times the image in writes for each
job (`crates/podrunner-podvm/src/unit.rs`). The microvm kind cannot hold a
55 GB initramfs.

Known. `check` and `doctor` do not refuse a pin larger than the disk bound
(T-551). The pool type matters: on `dir` each instance unpacks the full
image [E]; on btrfs, ZFS or LVM thin an instance is a clone [E].

Open questions.

1. Which incus storage drivers give a cheap clone of a 55 GB image for an
   unprivileged, mapped container (btrfs subvolume snapshot, ZFS clone,
   LVM thin snapshot, `dir` with `reflink` on XFS/btrfs)? What does the
   `incus storage` `size` limit then count: the whole root or only the
   writes? (githome's ask: "a copy-on-write pool where only the writes
   count".)
2. With idmapped mounts (kernel 5.12 and later) an image tree can be shared
   across instances with no shift of ownership. Does incus use
   `shiftfs`/idmapped mounts for the image root? Does it work for a
   rootless incusd?
3. Can a lazy format help the incus/VM modes? (They need the whole tree
   before boot, as the report says.) What about `composefs` or `erofs` with
   `fscache` for a shared read-only lower?
4. For the podvm kind: why does each unit have its own store? (Labels,
   cleanup, confine.) Can one operator store plus `FICLONE`/reflink serve
   many units?
5. On the GitHub-hosted 145 GB class, how much can a job free (the
   preinstalled trees, `/mnt`)? (T-557.) Can the full boot fit?

Approaches. (a) A host class rule: the snapshot needs a CoW pool and
more than 190 GB; refuse it elsewhere by name (T-551). (b) A slimmer
runnable image: a `hosted-core` subset of layers (the layer table makes
each tool-cache version a layer). (c) Ask githome for 80 GB per unit or a
CoW pool.

Proof. `cargo test -p podrunner --test check`: a pin over the disk bound is
refused with the fix. A measure in `runner-image.yml` or the live lane of
the writes and the start time of a unit before and after sharing.

Verdict: hard. Impossible to fit a 55 GB writable copy in a 40 GB bound.
Possible with a CoW design and an accounting of writes only.

### H-10. File capabilities and other xattrs with no root

Entries: T-558 (blocked, P2), T-560 (blocked, P3), COORD-107 (#317),
COORD-106 (#316) ask 4, COORD-109 (#319), podvm PodUtils/podvm@T-1999.

Why it is partly impossible. `ping` and `mtr-packet` carry
`security.capability` (`cap_net_raw+ep`). Community snapshots drop xattrs
(0 PAX xattr records [M]); the podrunner snapshot keeps them (C1 checks
them). Setting `security.capability` needs `CAP_SETFCAP`. A no-root
process on the userland rungs has none.

Known. podcrates `v0.76.0` sets `SCHILY.xattr.*` records in `pod-extract`;
`v0.77.0` reads a value with byte `0x0a` too; `v0.75.0` reads each gzip
member and zstd frame (T-560). Podvm main (T-1999) carries
`security.capability` and `user.*` as PAX records in the unified tarball of
the incus tier, and a file keeps `cap_net_raw+ep` in the root of an incus
instance. The machine tier comes with T-1994. Podrunner links no
`pod-extract`; podvm extracts layers.

Open questions.

1. In a user namespace with one mapped uid, can a process with
   `CAP_SETFCAP` over that namespace write `security.capability`? The
   kernel writes a v3 capability with the root id of the namespace. Does
   the file work when exec'd from the host namespace? (Namespace rung.)
2. On the userland and supervise rungs: any way to emulate file caps?
   (`net.ipv4.ping_group_range` makes `ping` work for a gid range, but only
   by a sysctl of the host.) Can podvm's interposer fake `getcap`?
3. Which hosted files carry capabilities besides `ping`, `mtr-packet` and
   `gst-ptp-helper`? (C1 lists them.)

Proof. In a unit of each kind, `getcap /bin/ping` gives `cap_net_raw=ep`
and `ping -c1 127.0.0.1` passes as `runner`.

Verdict: partly impossible. Impossible on the userland rungs. Solvable on
the namespace rung, incus and the machine tier.

### H-11. What cannot match on a shared kernel

Entries: T-538, T-539 (allow rows), T-551 (the sysctls); study findings F8.

Why it is impossible in container modes. An incus system container and a
podvm unit share the host kernel. So `uname -r`, loaded modules, the hosted
sysctls (`vm.max_map_count=1048576` via
`/etc/sysctl.d/10-map-count.conf` [M], `fs.inotify.max_user_watches=655360`
[M, study 1]), `modprobe` (v4l2loopback, nbd, kvm), swap, `losetup` and
`binfmt_misc` differ from the hosted runner. A job that loads a module or
needs a large `vm.max_map_count` (Elasticsearch, file watchers) can fail.

Known. Probe (`scripts/hosted/probe.sh`) records these values. The
allow list (`images/hosted/allow.toml`) holds the known rows. The first gap
run was 3389 differences between the old `runner-full` image and
`ubuntu-24.04` [M, run 44456462683]. The snapshot should remove most of
them.

Open questions.

1. Which sysctls can an unprivileged incus instance set (namespaced ones
   only: some `net.*`, `kernel.shm*`; `vm.*` are not namespaced)?
2. Can incus set `linux.sysctl.*` for `vm.max_map_count`? (No: not
   namespaced. Verify.) Can the operator set them on the host once at
   install, and how does `doctor` check them?
3. For VM modes, which sysctls does the hosted image set at boot, and do
   they apply in a guest? (The report says a microvm guest can.)

Approach. Allow rows for container modes; the VM mode (H-8) is the real
parity. `docs/install.md` names the host sysctls for the operator.

Verdict: impossible in container modes. Solved only by the VM modes.

### H-12. The capture is a moving target

Entries: T-542 (partial, P1), T-555 (partial, P1), T-540 (partial, P1),
T-539, T-538; study `hosted-snapshot-2026-11-19.md` sections 2 and 3.

Why it is hard. The capture reads a live hosted VM. GitHub's provisioning
changes without notice. The community snapshot leaked secret material
because a fixed exclusion list went stale when the runner moved (the
section is held back). Podrunner uses a scrub list plus a pattern scan
(C2) that fails closed, a 1:1 check (C1), and a planned two-VM diff (C3).

Known [M].

- C1 and C2 pass on the four labels. C1 found one live-state difference
  (sysstat timer); the quiesce now stops timers.
- Pools of one label carry different build numbers for one release. In
  one run `ubuntu-24.04` gave `20261111.370.1`, `20261111.127.1` and
  `20261116.133.1`, and the release is `ubuntu24/20261111.370` (run
  44459794216). The watcher matches by build number first, then by date.
- `ImageOS` of an arm64 runner is `ubuntu24-arm64`.
- `ubuntu-latest` moved from 24.04 to 26.04 on 2026-11-19
  (runner-images#14748). A job on `ubuntu-latest` ran `ubuntu-26.04`
  `20261111.196.1` while the README still said 24.04.
- The first dry run found secrets-like content in public pre-pulled images,
  gems and Homebrew (now `[[skip]]` rules) and MySQL TLS private keys made
  at build (dropped; mysqld makes new ones).
- Left per the record: `gh attestation verify` in a test, the planted
  credential that must fail C2, C3 (two-VM diff), a schedule from the
  watcher, the boot test.

Open questions.

1. Can a job on a hosted runner read all of `/`? It does today (sudo).
   Could GitHub restrict this? What is the supported way to capture
   (public documentation of the image, `/imagegeneration`)?
2. What changes in a hosted image between two VMs of one `ImageVersion`
   (the C3 data)? Which files carry per-VM state beyond those listed
   (machine-id, host keys, waagent, cloud-init, `/etc/hosts`, netplan)?
3. Does the capture quiesce enough? (Docker, containerd, snapd, cron,
   sysstat, php-fpm, unattended-upgrades, rsyslog, walinuxagent stopped;
   `hosted-compute-agent` kept.) Is a stopped `containerd` store (26.04,
   2466 MiB, whiteout nodes) consistent?
4. How to detect a new release with a changed layout (a moved runner
   directory) before the scan finds a secret? Is a diff of the layer table
   between versions a reliable early signal?
5. What is the lag between a hosted release and the first capture?
   (Target one day [E].)

Approach. Keep fail-closed checks. Add C3. Add a diff of the file list
between versions. Run the watcher daily (T-540).

Proof. A planted file `/home/runner/.credentials-test` with a made-up value
(listed in `.github/secrets-allow.txt`) fails C2. C3 passes on two VMs or
lists each new path. A new hosted release is captured within one day.

Verdict: hard, but a process problem and not an impossible one.

### H-13. ghcr.io: limits, behaviour and "keep each version"

Entries: T-555 (partial), T-553 (open, P1), T-562, ruling "Retention on
ghcr.io" (2026-11-19) and "ghcr.io" (2026-11-19), COORD-102 (#312).

Why it is hard. The images are large. The owner rules that podrunner
deletes no version. Weekly snapshots share 0 of 18 layers in the community
method [M]; with stable path layers podrunner hopes for reuse but has no
measure. Storage grows about 55 to 65 GiB each week for four images before
dedup [E].

Known.

- ghcr.io documents a 10 GB limit for each layer and a 10 minute timeout
  for each upload request (docs.github.com, "Working with the Container
  registry"). The largest podrunner layers are about 2 GB with zstd.
- A PATCH with `Content-Range` gave HTTP 416 on ghcr.io (run
  44517923954). `push-blob.py` now sends one streamed PATCH with no range;
  15.9 GiB went up in 14 minutes [M, run 44528851334].
- Tokens: the capture uses `GITHUB_TOKEN`; tokens go to curl on stdin.
- A new package can be private. An admin sets it public once (done for the
  snapshot; an anonymous client reads the manifest).
- The index is attested; Rekor log indexes are in the record.
- OCI layer type `tar+zstd` works with podvm, docker 23 or later,
  containerd and podman.

Open questions.

1. What are the real ghcr.io limits for anonymous pulls: bandwidth caps,
   per-IP rate, per-layer concurrency? What does a 16 GiB pull cost a
   user on a slow link, and do pulls resume per blob?
2. Is storage and egress for public packages free without limit? (The
   study did not read the GitHub Packages billing page.) Is there an
   unannounced cap for a package of hundreds of GiB?
3. Does a cross-repository blob mount (`POST ?mount=DIGEST&from=REPO`)
   work with `GITHUB_TOKEN` on ghcr.io? Does it save upload time between
   versions?
4. Is the 10-minute request timeout enforced for a streamed PATCH that
   keeps sending? (A 2 GB blob went in under 14 minutes in total; the
   limit may be an idle timeout.)
5. Does `zstd` layer media type work for `docker pull` on old engines and
   on `skopeo`? (docker 23 and later: yes.)
6. Does the deletion limit matter: are package versions auto-deleted when
   untagged, or kept? "Keep each version" may need each version to stay
   tagged or referenced by an index.
7. What is the GC and untagged-manifest policy for ghcr.io for
   attestations (referrers)?

Approach. Measure with a script against ghcr.io: pull a 2 GB blob 10
times anonymously from a cold host; time resume. Read the GitHub Packages
billing and limits pages. Publish a size report per release.

Proof. A report with numbers for each question, and a monitor job that
records package size each week.

Verdict: unproven. The owner's retention ruling stands. A researcher can
settle the limit questions.

### H-14. The full lane: checks C4 to C8 need a host that no hosted runner is

Entries: T-547 (open, P2), T-546 (open, P2), T-539 (partial, P1), T-550,
T-557, T-556.

Why it is hard. The full boot (probe diff, the software report, Pester,
the behaviour checks, real jobs) needs the whole image booted. The
extracted image plus store plus pool is about three times 55 GB. A
standard hosted runner has 72 GB or 145 GB with 54 to 58 GB used. A live
host of the live gates (GitHub #74, #75) has the room, but the live gates
wait for the owner's test organisation and hosts.

Known. The tests ship in the image: `/imagegeneration/tests` (11 files),
`SoftwareReport/Generate-SoftwareReport.ps1`, and
`/imagegeneration/software-report.json`. The check list C1 to C8 is in
the study (section 4).

Open questions.

1. After freeing the preinstalled trees (Android, dotnet, GHC, hostedtoolcache
   not needed by the test) and using `/mnt`, does the 145 GB class hold a
   full boot? Which step peaks, and by how much? (T-557.)
2. Which Pester tests of runner-images assume the Azure build VM (these
   need allow rows)? Which test uses systemd and which uses network?
3. Does PowerShell (`pwsh`) run reliably in the unit under the proxy-only
   network?
4. Which paid larger runner (e.g. 16 core, 600 GB disk) is cheapest to
   keep as a weekly lane? Is any free for public repositories?

Approach. T-557 first (a dispatch-only job that frees space and records
the free space at each step). If it passes, run C4 to C8 in hosted CI.
Else a live lane.

Proof. Each check passes in each mode at the level "proven on a
constrained host".

Verdict: hard. Mostly a resource problem.

---

## 5. Theme C: egress and network holds

### H-15. An enforced hold with no user namespace

Entries: T-041 (done in part), T-044, T-179 (done), residual R5, podnet
issues #142 and #149 (podnet side), COORD-099 (#309).

Why it is impossible. The hold names how a unit reaches the egress broker:
`none`, `advisory` or `enforced`. `enforced` needs a network namespace in a
user namespace, with a TUN that the supervisor holds. On a host with no
user namespace, only the seccomp hold (`advisory`) exists. The seccomp
hold does not catch unix, netlink and vsock sockets, loopback TCP and
inherited descriptors (R5). A trusted payload can go around it.
`untrusted` needs `enforced`, so untrusted jobs on such a host use a
guest (H-6).

Known. On H0 (no user namespace) the runner timed out on the broker under
the seccomp hold (T-179). The fix is in podnet `v0.4.0-beta.17`: an epoll
waiter keeps its nonblocking socket on the relay. User-gate run
44393864747 passes `hold:podvm` on each lane, also `userns-off` [M, proven
in CI]. The run on the owner's H0 host stays with the live gate.

Open questions.

1. Can SUD or seccomp user-notify give a stronger hold than the current
   seccomp backend for AF_UNIX and loopback? (Compare with podnet's
   backend.)
2. Can Landlock network rules (ABI 4) deny TCP connect/bind to ports other
   than the proxy port? Landlock covers TCP only. UDP and raw sockets
   remain. What kernel floor does that need?
3. Can `pmask` or another no-root route close R5? Ask the owner.

Approach. Landlock ABI 4 as an extra layer for `advisory`; keep the
guest rule for `untrusted`.

Proof. A hostile test payload (made-up) cannot reach a second host by a
unix socket relay, loopback proxy or inherited fd under the hold.

Verdict: impossible with no user namespace and no new kernel feature.
Landlock networking can narrow R5.

### H-16. `podrunner run --vnet` hangs on a host that refuses the bind

Entries: T-561 (partial, P2), T-220 (partial, P2), T-228, T-227, T-350,
GitHub #167, #170 (COORD-058, closed), podnet PodUtils/podnet@T-356 and
@T-357, user-gate lane `bind-denied`.

Why it is hard. On a host whose sandbox refuses `bind(2)` for AF_INET and
AF_INET6 (loopback too), the fake broker (`TcpListener::bind`) fails.
Podrunner is static, so podnet's vnet shim (`libpodnet-vnet.so`, built
against glibc, loaded by `LD_PRELOAD`) cannot reach it. Podrunner uses a
unix socket for the broker, and `--vnet PATH` maps `127.0.0.1:3128` to
that socket for the .NET runner in the unit.

Known [M, user-gate run 44506794088 at `582e827`, podvm beta.36].

- The lane `bind-denied` (podvm lab `bind-denied,getsockname-denied`)
  passes 9 of 9 now. `lab:vnet` ends after 900 s as `xfail T-561`.
- The log: the unit starts, then the runner takes no job and prints no
  line. `podrunner run` waits ("no line for 60 s" up to 840 s).
- Step (b) is in: `InUnit::reach` (600 s) ends the job with a named
  cause when the runner sends no request to its broker.
- Left: step (a), the link that fails between the runner and the broker
  through the vnet shim.
- The .NET runner connects through a dual-mode AF_INET6 socket
  (`[::ffff:127.0.0.1]`). Podnet fixed the mapping in `v0.4.0-beta.14`.
  Podnet did not measure a podvm unit, the real runner or .NET (T-220
  state of 2026-11-17).
- A user measured the podvm kind on the unix broker and the front on such a
  host and ran a hello job (#170).

Open questions.

1. Does the .NET runtime (`System.Net.Sockets`) call `getsockname`,
   `getpeername`, `epoll_ctl` or `ioctl(FIONREAD)` on a vnet socket in a
   way that the shim does not model? The lab denies `getsockname`. What
   does the shim return?
2. Does the shim load before podvm's own interposer? What does the order of
   `LD_PRELOAD` entries do on the userland rung? Does .NET use
   `connect` through a libc symbol (so the shim sees it) or a raw
   syscall (it cannot)? (CoreCLR calls through its PAL native shim
   `libSystem.Native.so`, which calls libc.)
3. Does `SocketsHttpHandler` use `Happy Eyeballs` and try IPv6 first? Which
   names does it resolve (`localhost`)? Does it resolve at all (the DNS
   path under the shim)?
4. Is the abstract name `\0podnet-vnet/4/127.0.0.1/PORT` visible in the
   unit (same network namespace as podrunner)? On the userland rung
   probably yes; on the namespace rung a new net namespace hides it.
5. Does the runner stall at `GET /_apis/connectionData` or earlier (at the
   proxy CONNECT)? An `strace -f -e trace=network` of the runner inside the
   unit would show it.

Approaches. Get the log of the runner in the unit and `strace` for the
first 60 seconds in the lab. Build a tiny dynamic probe program that
mimics the runner's connect path. Ask podnet by COORD issue with the
trace.

Proof. `lab:vnet` passes in the lane `bind-denied`: a job runs to the end
with the shim, or fails with a named cause within the bound.

Verdict: hard; cause unknown. A bounded failure already exists (step b).

### H-17. DNS only through an authenticated, TLS-intercepting proxy

Entries: T-351 (partial, P1), T-371 (open, P1), T-372 (partial, P1),
T-233, T-234, COORD-078 (#215, closed), QA report F1 and F4.

Why it is hard. A host whose only resolver is its proxy gets 502 for each
fetch when the egress module resolves names itself. The proxy asks for a
login and intercepts TLS. Each client (git, Node `fetch`, .NET, Go, curl)
treats proxy variables, CA bundles and name resolution in its own way.

Known.

- podcrates `v0.70.0` (PodUtils/podcrates@T-862) sends an allowed name
  with no local address upstream in the names mode. Podrunner took it.
  Test `names_upstream_with_no_resolver`.
- Interim fix: `dns::resolver_via(proxy)` (system resolver first, then DNS
  over HTTPS through the proxy, with the CA of the environment added).
  Measured on a sandbox with no resolver behind a TLS-intercepting proxy:
  502 before, pins `actions/upload-artifact@v4` after.
- Not proven: a proxy that asks for a login (the lane `proxy-only-dns`
  does not exist yet, T-371), and a proxy that refuses a CONNECT to a DoH
  server address.
- The user gate checks success paths only (T-372); users found the beta.7
  defects on failure paths.

Open questions.

1. Which proxies refuse `CONNECT` to a DoH IP literal (common in
   enterprise: allow list by name)? What does the fallback do then?
2. Which clients do not honour `NODE_USE_ENV_PROXY=1` (Node 24 only)? Which
   Go, Java, Python clients ignore `HTTPS_PROXY` for some hosts or take
   `NO_PROXY` with CIDR forms differently?
3. How does the egress module forward proxy credentials (Basic, NTLM,
   Kerberos negotiate)? Basic only today (presumed). Is NTLM in scope?
4. How should a lane test an authenticated proxy: mitmproxy with
   `--proxyauth`, plus `REJECT` of port 53 to each address?

Proof. A lane `proxy-only-dns` fails on `b752051` and passes after the
fix. The corpus fetches each action, `hold:podvm` passes, `image verify`
of the real release asset passes in it.

Verdict: hard (test coverage, many clients). Not impossible.

### H-18. Placeholders for secrets: TLS interception and clients that sign

Entries: T-563 (open, P2), T-524 (blocked, P3), G-04 and G-35 (githome,
PodUtils/podcrates#223), COORD-146 (#321), `docs/non-goals.md` (the TLS end
of the broker).

Why it is partly impossible. githome asks that a job hold a placeholder of
each credential, never the real value. The broker of podcrates `v0.78.1`
ends TLS for hosts that a credential names, swaps the placeholder in a
header, Basic authentication, a query or a body, and puts the placeholder
back in each answer. The broker makes one CA for each job; the job needs
the bundle and the variables of `ca::env` (`SSL_CERT_FILE`,
`GIT_SSL_CAINFO`, `NODE_EXTRA_CA_CERTS`, `REQUESTS_CA_BUNDLE`,
`CURL_CA_BUNDLE`, `AWS_CA_BUNDLE`). Podrunner today loads each job with no
secret and gives no CA bundle.

Why some clients cannot work. A client that signs a request with the
secret (AWS Signature V4, HMAC webhooks, GCP signed URLs, Docker's
registry token flow, OAuth client assertions, JWT signing with a private
key, SSH keys) computes the signature from the secret. A placeholder gives
a wrong signature that the broker cannot fix after the fact unless it
re-signs.

Open questions.

1. Which clients ignore the CA variables? Rust `reqwest` with
   `webpki-roots` or rustls with built-in roots; Java (needs a keystore);
   Go with `SSL_CERT_FILE` (works); Python `certifi` with `requests` (the
   variable works); `git` over HTTPS (works); `npm` (`cafile`); `pip`;
   `docker`; `gh`; `aws` CLI (`AWS_CA_BUNDLE`); Playwright browsers
   (their own NSS store); static Go programs with embedded roots?
2. Which protocols besides HTTP/1.1 need support: HTTP/2, gRPC, WebSocket
   upgrade, `git` over SSH (no TLS), Postgres TLS?
3. Which secrets are used only as plain headers or Basic auth (most
   forge tokens)? A census of the top 50 actions by secret use would size
   the covered fraction.
4. Can the broker re-sign AWS SigV4 (it would need the secret and the
   canonical request)? Does podcrates plan it?
5. Certificate pinning (some mobile and vendor tools) breaks. Which
   popular CI tools pin?

Approach. Make the policy refuse an offer whose credential has no
`hosts` and `in`. Document the unsupported clients. Add the CA bundle to
the unit and the variables. A conformance test per client family.

Proof. `cargo test -p podrunner-egress`: a job with a placeholder reaches a
stand-in host with the real value in its header; the job sees only the
placeholder in each answer; no audit line or log line holds the value.
Add a canary test per client (curl, Node, Python, Go, git, Java).

Verdict: partly impossible for clients that sign. Possible for header
and Basic-auth tokens.

---

## 6. Theme D: release and supply chain

### H-19. The official runner carries a vulnerable `WebUtilities` (T-082)

Entries: T-082 (blocked, P2).

Why it is blocked. Runner v2.338.2 (the newest on 2026-11-12) ships
`Microsoft.AspNetCore.WebUtilities` 8.0.25 (GHSA-x33c-w48q-qrgc,
CVE-2026-114149: denial of service, fixed in 8.0.34). Every unit image
carries it. A NuGet audit fails the build of the runner from its tag.

Known. The runner is a client and serves no HTTP. `scripts/runner-audit.sh`
compares advisories with a reviewed list; the `audit` job passes (1
advisory reviewed). The musl build turns the audit off.

Open questions.

1. Does the runner code reach the flawed parser (`FormReader`,
   `MultipartReader`, `QueryHelpers`)? A call graph of `Runner.Worker`
   and `Runner.Listener` would answer.
2. Can the image replace `Microsoft.AspNetCore.WebUtilities.dll` with
   8.0.34 safely (same major, binding redirect, deps)? Does the runner's
   self-update or integrity check object?
3. When does `actions/runner` ship a release with 8.0.34 or later? Is
   there an open issue upstream?

Approaches. Wait (today). Patch the DLL in the image with a pinned hash
and a test. Ask upstream through the owner.

Proof. `sh scripts/runner-audit.sh SRC images/runner/advisories.txt`
passes with an empty list.

Verdict: blocked on upstream; a patch path exists.

### H-20. GitHub refuses a runner that is too old (T-499)

Entries: T-499 (partial, P1), T-082, `images/runner/runner.toml`,
githome G-23.

Why it is hard. GitHub refuses a runner that is too old. A new
`actions/runner` release needs a new runner image, which today needs a new
podrunner release. A wrong guess of the window takes the fleet down.

Known. `doctor --config` names each pinned payload, its runner version,
its age, its window and `ok`, `near-end` (last 7 days) or `stale`.
`[pin] max_age_s` is 30 days; `[[pin.payload]] max_age_s` is 28 days. Each
release attaches `podrunner-manifest.json`. Step 1 is open: a runner image
as its own asset within 14 days of an `actions/runner` release, with no
new binary.

Open questions.

1. What is the exact rule? (Public docs say a runner must be updated within
   30 days of a new release; the server enforces a minimum version for
   some APIs.) How does the service behave for a JIT scale-set runner whose
   auto-update is off: a `RunnerRefresh` message, a 403, a session
   refusal? What does the runner do on the message?
2. Does the broker (`broker.actions.githubusercontent.com`) check the
   `X-GitHub-Runner-Version` or the version in the session body?
3. Can a pinned older runner still take jobs while a newer release exists?
   For how long?
4. Do new runner releases come with changes to the job message that break
   the fake broker, the hook or the images (a release gate)?

Approach. Read `actions/runner` code (`SelfUpdater`, `MessageListener`) and
test with a live runner version several releases old on the test
organisation. Automate: a workflow that builds the image and publishes it
on ghcr.io within the window.

Proof. A measured rule with a source, and a workflow run that publishes
an image for a new runner tag with no new binary, loaded with
`podvm load -i` on a running host.

Verdict: unproven.

### H-21. Release paths that never ran

Entries: T-484 (partial), T-553 (open, P1), T-562 (open, P2), T-523 (blocked
then unblocked by podcrates `v0.77.0`/`v0.78.1`), T-205, T-205's dry run.

Why it is hard. `release.yml` signs `SHA256SUMS` with a keyless cosign
bundle, attests files, will push `ghcr.io/podutils/podrunner` in the form
of COORD-142, and will write `release-index.json` verified by the static
`pod-release` command. The signing needs the token of a release run. A
dry run cannot prove it. Each path has failed once in sibling projects
(podvm: the manifest of each platform must come from the index for the
check after the push, and the login to ghcr.io must stay for
`actions/attest`).

Open questions.

1. Which cosign identity regex and OIDC issuer verify a `workflow_dispatch`
   run on a tag? (The check in `release.yml` uses the identity of the
   workflow at the ref of the dispatch.)
2. How do users check offline? Does `cosign verify-blob --bundle` work in
   a hermetic installer (the installer cannot require cosign)?
3. What is the proof that `gh attestation verify oci://...` passes for an
   index of two arch manifests?

Approach. Cut a beta only to run these paths (the ruling allows betas when
the gates are green), or add a throwaway tag in a fork.

Proof. A release run produces `SHA256SUMS`, its bundle, the index and the
ghcr.io images, and each check passes from a clean host.

Verdict: unproven. Not hard in itself; the risk is a broken first release.

### H-22. The proof gap between CI and a constrained host

Entries: T-243 (partial, P1), T-244 (partial, P1), T-246 (partial, P1),
T-372 (partial, P1), T-249 (open, P1), T-371 (open, P1), T-527 (partial,
P1), T-528 (partial, P1); `AGENTS.md` section 12.

Why it is hard. The first user runs of beta.6 found about 30 defects that
CI did not find. Green CI on GitHub-hosted runners proves that the parts
agree. A report may not say "ready" below the level "proven on a
constrained host as a user". Each host class (a refused bind, no user
namespace, a proxy only, a small `/tmp`, DNS only through a proxy,
inherited descriptors) needs a lane. The labs come from podvm
(`podvm lab run`) and `pod-lab` of podcrates (T-264).

Known. Lanes: `plain`, `proxy-only`, `inherited-fds`, `proxy-only-dns`
(partly), `small-tmp`, `userns-off`, `bind-denied`, `serve-incus-kvm`. The
run 44506794088 shows `bind-denied` passes 9 of 9 with `lab:vnet` as an
xfail. Adversarial QA rounds 1 to 5 found no P1 in the last two rounds;
each P1 and P2 is fixed. The installer is tested only in dash on easy hosts
(T-249).

Open questions.

1. Which host classes still have no lane? (No-KVM, a read-only root, a
   full disk, clock skew, IPv6 only, a corporate TLS proxy with a login,
   cgroup v1, SELinux enforcing, Alpine/busybox, aarch64 with and without
   user namespaces.)
2. How can QA find a defect class systematically: fault injection on the
   egress, the proxy and the resolver (T-372)?
3. What is the cost to run the user gate per release (runner-image.yml
   takes about 2 hours; the x86_64 job has 150 minutes)?

Proof. Each reported host class has a lane and no new defect in two
consecutive QA rounds.

Verdict: hard (breadth). A matter of process.

---

## 7. Theme E: GitHub behaviour, githome and cfb asks

### H-23. GitHub's scale set binds no runner to a job; live unknowns

Entries: T-169 (partial, P1), T-175 (partial, P1), T-032, T-033, T-040
(blocked), `docs/unknowns.md` U6, U7, U8, U11, U12, U14, U17, U18, U20, U22,
U23; GitHub #74 (LIVE-1), #75 (LIVE-2), #121, #151.

Why it is blocked. GitHub gives an assigned job to any runner of a scale
set. A JIT runner minted for job A can take job B. A fork job that the tier
rule withdrew stays assigned, and with `X-ScaleSetMaxCapacity` 1 the set
counts it as busy for about 20 minutes. No scale-set API call gives an
assigned job back (`actions/scaleset` v0.5.0 has `AcquireJobs`, no
release). The owner chose to cancel the run (Q7): the App needs "Actions:
read and write". The job-started hook ends a foreign job before its first
step (T-175 step 2, proven in a podvm unit, run 43686746902).

Known.

- Done: the hook; the cancel in the sidecar; T-493 (the hook asks `serve`
  which asks the source: `assigned`, `keep` or `end`).
- Open: step 1 of T-175, which assigned job a new runner takes (order,
  and whether the runner name or labels steer it); the live check that
  after a refused fork job the next trusted job runs within one poll.
- The live unknowns (need real github.com): the `AllowRunnerContainerHooks`
  value (U6), several labels for a scale set (U7), minimum App permissions
  (U8), the effect of a stolen credential (U11, U22), run object and member
  permissions, runner group and public repositories (U12), the hooks
  variable with the job variable false (U14), two hosts on one scale set
  (U17), failing a job by name (U18), a real JIT blob and token refresh over
  hours (U20), the trust inputs for re-runs, merge queues and reusable
  workflows (U23).
- The live gates run on a throwaway organisation and one GitHub App (DR-7).
  They need the owner's accounts, so a cloud session cannot run them.

Open questions for a researcher with network and the test organisation.

1. In `actions/actions-runner-controller` and `actions/scaleset`: how does
   the listener decide which job an ephemeral runner gets? Does a runner
   registered with a JIT config carry the `runnerRequestId` of one job? Does
   the runner's `GetNextMessage` return a message of the job that was
   acquired for that runner (for example by `runner_name` or a session id),
   or any acquired job?
2. Is there an API to delete an acquired job request, to complete it with
   `Cancelled` or `Failed` (`JobCompleted` message), or to re-queue it?
   What does a runner that fails a job with `JobCompleted` do to the run?
3. What does the platform do when a scale set has a job assigned and no
   runner takes it? (Timeout? The 20-minute stall is a measure.)
4. Does the `cancel` of a run free the capacity at once?
5. For each of U6 to U23: run the probe jobs and record the result in
   `docs/unknowns.md`.

Approach. Read the ARC listener and the runner `MessageListener` source;
then run the live probes. A fork-PR job stopping a trusted set is a
denial of service. The fix with a cancel needs an App permission.

Proof. The live check on a release: after a refused fork job, the next
trusted job runs within one poll; each U row has a measured answer.

Verdict: blocked on live access and GitHub's undocumented behaviour.
Fully researchable by reading source and running probes.

### H-24. Warm units and shared caches

Entries: T-487 (open, P2), T-480 (open, P2), T-528 (partial, P1), T-505,
githome G-11, G-20, G-25, cfb A-X-01.

Why it is hard. cfb and githome want a short time from claim to first step
and a cache for npm, pnpm, bun, Playwright and tool caches. Each job
starts in a clean unit. A guest of the machine tier takes about 30 s. A
warm unit holds a started guest. A cache directory must never carry
state from a low-trust job to a high-trust job.

Known. `claim_to_first_step_s` is in the run report (T-075). The record
decides: a cache is scoped by source, forge, repository, tier and key; an
untrusted job gets `none`; `rw` only for a push to the default branch.
The corpus has no stand-in row of cfb's gate yet (T-528, partial).

Open questions.

1. Can a VM snapshot (QEMU `-incoming`, loadvm) or an incus stateful
   snapshot start a guest in under 2 s? Does CRIU work for an
   unprivileged container (usually not)? How do warm guests reseed
   entropy and the clock after restore (VMGenID, `virtio-rng`,
   `kvm-clock` step)? A restored guest with the same random pool is a
   security issue.
2. How does a "warm unit" keep the rule "fresh unit: no job ran in it, no
   credential, no egress until a claim"? What does the unit hold of the
   host (memory of a guest)?
3. For caches: which cache layouts are content-addressed with integrity
   (npm `_cacache`: yes with SRI; pnpm store: yes; cargo registry: yes;
   Playwright browsers: archive checks)? A poisoned lower-trust write
   would be caught only where the format checks content. Is a read-only
   mount plus a per-job overlay enough?
4. What is the real time split of a cold job (`claim_to_first_step_s`,
   install, run) on each kind? (T-487 step 1; not measured.)
5. Does the `actions/cache` service (GitHub's) cover the need so that a
   host cache is not required? The local mode serves cache v2 from the fake
   broker; `serve` uses GitHub's.

Approach. Measure first. Then a warm pool behind `UnitDriver::prepare` for
the incus and microvm kinds. Caches as read-only mounts with a per-job
overlay.

Proof. `claim_to_first_step_s` before and after; the conformance suite on a
warm unit (no file of an earlier job, no credential before the mint); a
test that a file an untrusted job writes is absent from the cache of the
next trusted job.

Verdict: hard. Podvm owns the snapshot part.

### H-25. A sandbox unit for agent tasks and web terminals

Entries: T-506 (partial, P1), T-527, githome G-36, Q-7.

Why it is hard. githome wants podrunner to run its coding-agent tasks and
web terminals as units: a payload that the source names, with isolation,
an egress hold, limits and a report, an outbound WebSocket attach to a
URL of the source, an end at `idle_s` and `max_s`. The idle end belongs to
the payload: an attach is one long connection through the egress module,
which shows no idle time to podrunner.

Known. In code: an offer with `sandbox { image, command, idle_s, max_s }`
runs at `untrusted` (a guest), its image must be a pinned payload with
`sandbox = true`. `a_sandbox_ends_at_its_max_s` passes. The lane
`serve-incus-kvm` runs a sandbox offer on a real microvm unit to a report
of `success` [M, run 44462524454]. Not run: a real attach over an outbound
WebSocket.

Open questions.

1. Does the egress broker (`pod-broker` CONNECT tunnel) hold a long-lived
   WebSocket upgrade for hours? Any idle timeout, buffer, half-close
   behaviour or HTTP/2 mismatch?
2. How should the egress module count idle for a tunnel (bytes in a window)?
   Does the broker emit a "tunnel idle" audit event?
3. Which guest hardening is needed for arbitrary agent code with sudo
   inside a microvm (nested `/dev/kvm`? no; `virtiofs` vs 9p for the work
   directory; memory ballooning)?
4. How many guests can a 32 GiB host hold at 12 GiB each (memory
   overcommit, KSM)?

Proof. A sandbox on real units in `runner-image.yml` that attaches over an
outbound WebSocket, is idle for `idle_s`, and ends; a long WebSocket
survives two hours through the egress module.

Verdict: unproven.

### H-26. An engine in an incus instance, and `docker build` and Docker-in-Docker

Entries: T-485 (done, with the decision open to measures), T-516 (open,
P2), T-517 (open, P3), T-103 (done), `docs/container-engine.md`, githome
G-09 and G-21.

Why it is hard. `services:`, `container:`, Docker actions, `docker build`
and Docker-in-Docker need a real engine inside the unit, with its own root
file system and no socket of the host. For incus, the decision of T-485 is
that an engine inside an unprivileged instance with `security.nesting`
and syscall intercepts for `mknod` and `setxattr` keeps the claim
`confined`. The record says a measure can change that.

Known. The census of verbs: the runner needs 15 verbs of the command line;
Docker-in-Docker needs the whole Engine API (so no shim). In a microvm
guest `dockerd` runs with `DOCKER_RAMDISK=true` (the guest root is an
initramfs, so runc cannot `pivot_root`). The parity suite of
`runner-image.yml` runs `services:`, a job container and a Dockerfile
action in a guest and (T-485) in an instance. `docker build` and DinD
steps are not run (T-516). The engine is a host directory, not part of the
image (T-517).

Open questions.

1. What is the security of Docker inside an unprivileged incus container
   with `security.nesting=true` and `security.syscalls.intercept.mknod`/
   `setxattr`? Which CVEs and escapes are known (runc, AppArmor
   stacking, `/proc` and `/sys` writes)? Does it keep a boundary stronger
   than `redirect`?
2. Which storage drivers work (overlay2 on `dir`/ext4 in a user namespace,
   `fuse-overlayfs`, `vfs`)? What is the performance of `docker build` in
   them?
3. `docker:dind` needs `--privileged`. In a nested unprivileged
   container, does DinD start (it needs `CAP_SYS_ADMIN` in the namespace)?
4. How does the engine in the image (not a host directory) interact with
   `DOCKER_RAMDISK` once the guest root is a disk (H-8)?

Proof. Parity workflows `10-docker-build.yml` and a `docker:dind` service
pass on the engine step of `runner-image.yml` and match github.com.

Verdict: hard. The `confined` claim needs a security review.

### H-27. An alternate payload: a runner other than the official one

Entries: T-161 (partial, P3), `docs/alternate-payload.md`, ruling U15
(2026-11-14), non-goals (native runner for riscv64 and others).

Why it is hard. The official runner is .NET. It serves no riscv64, i686,
loongarch64 or ppc64le, and no static musl host. A native runner must
speak the broker protocol, the run service and the results service. These
are not documented. They change. Podrunner has a census of the calls from
the fake broker.

Known. The owner allows an alternate payload that reports a third-party
version string (U15). podcrates has `pod-actions-expr`, `-workflow`,
`-store`, `-results`; the local mode plans a run with them but does not run
steps. The call (2026-11-15): measure an existing third-party runner
against the fake broker first.

Open questions.

1. How do existing third-party runners work (for example the Go runner
   `ChristopherHX/github-act-runner`, `nektos/act`, `actions/runner` forks)?
   Which of the calls of `docs/alternate-payload.md` do they cover? Do
   they support scale-set JIT runners and the broker long poll?
2. How does github.com treat a non-official version string (U15 is a
   policy ruling; what does the server do technically)?
3. The job message uses an encrypted body (RSA-OAEP with the session key,
   AES). Which fields change over time?
4. Can a native runner keep its credential in memory (H-4)?

Proof. One canned job of the fake broker passes with the alternate
payload in a unit; then a live job.

Verdict: hard. Research first, build later.

### H-28. A job of another architecture

Entries: T-533 (open, P2), T-151 (done), T-197 (partial), U26, GitHub
#146, #149.

Why it is partly impossible. An arm64 job on an x86_64 host runs in a
`linux/arm64` guest under QEMU TCG. Podvm 0.2.0-beta.22 and later do it;
`busybox:1.37.0` boots and runs in 10 s with beta.32 [M, run
44302792725]. Nobody measured the official arm64 runner there. .NET under
TCG starts in about 2 min 45 s on an emulated host [M, #149]. Such a
host has no Landlock and no hold, so no kind meets a forge tier.

Open questions.

1. What is the start time and the canned-job time of the arm64 runner in a
   TCG guest on a GitHub-hosted x86_64 runner? (T-533 step 1.)
2. Does .NET's JIT run under QEMU TCG without errors? Known issues exist
   in QEMU user mode. Does `DOTNET_TieredCompilation` or ReadyToRun change
   the start time?
3. Is a real arm64 host (hosted `ubuntu-24.04-arm`) cheaper than TCG for
   tests?

Proof. The measure step prints `foreign runner ok` and the seconds of a
canned job. `docs/unknowns.md` U26 names the run.

Verdict: partly impossible at usable speed; a measure decides.

### H-29. githome's statement: complete in a release

Entries: T-512 (open, P0), T-509 (partial, P1), T-511 (partial, P1), T-507
(partial, P1), T-493, T-494 (done), T-497, T-499, T-505, T-496, T-510
(done), `TODO/PROGRESS.md` item 0.

Why it is blocked. githome starts its next session only on the line
"githome: complete in vTAG" on GitHub #292 and #151, when each ask
G-01 to G-41 is done in a release. No ask is done in full in a release
(census of 2026-11-18). The needs of githome and cfb come first (ruling of
2026-11-18). Three asks wait on other projects or on live work:

- G-18 (T-496): podvm's sized disk of a guest (H-5).
- G-23 (T-499): a runner image within 14 days (H-20).
- G-38 (T-509): `pod-fake-githome` from podcrates against the 15 tests of
  `source-conformance`, and a runner job through `serve` with
  `pod-fake-githome serve --github` (COORD-101, #311).

Open questions for the owner (not a researcher): which asks the owner
accepts as met at the level "proven in CI", and which need the live host.
githome's real source has not run with `assigned` (T-493 is proven with a
stand-in).

Proof. The statement on both issues with the table of each ask, entry and
tag, after the gates of `AGENTS.md` section 12.

Verdict: blocked on people and on the problems above.

---

## 8. Theme F: CI flakiness and hangs

### H-30. Unexplained hangs, slow starts and flaky infrastructure

Entries: T-561, T-192 (partial, P1), T-159 (partial), T-180 (partial),
T-261, T-554 (done), T-197, T-478, T-394 (done), T-552 (done), T-502.

Why it is hard. Several hangs were found only in one lab or one run, and
could not be reproduced elsewhere. Each needs a trace at the moment of the
hang.

List.

- T-561: `lab:vnet` hangs. The runner takes no job; see H-16.
- T-192: after a unit ended `unit-failed`, `serve` took no more jobs
  (`futex_do_wait` in the main thread, one thread in a socket receive).
  Not reproduced. A read deadline on each hook connection and a bounded
  `close` fix the nearest cause. The failure of GitHub #144 on the next beta
  is the proof; it was not rerun.
- T-159: the destroy of a podvm unit once gave an unknown count of
  processes. `destroy` now logs the cause. Read it at the next real
  failure.
- T-180: as PID 1 of a container, `serve` reaped no orphan. It forks first
  and reaps (like `tini`). A test that an orphan of a unit is reaped is
  left.
- T-261: the cargo build of `rg-docs` hangs under the syscall tier; see H-2.
- T-554 (done): the TypeScript 7 compile hung on the userland rung. Cause:
  Go's SIGURG with podvm's SIGSYS handler; fixed in beta.36.
- T-197: a slow host needs a larger `start_s`; `doctor` should measure the
  start of the payload.
- CI infrastructure: `public.ecr.aws` answered 429 (T-478; a fallback to
  `mirror.gcr.io` then Docker Hub is proven only with a fake `docker`);
  Docker Hub pull limit (T-394, fixed with `mirror.gcr.io`);
  `runner-image.yml` has no concurrency group and takes about 2 hours (150
  minute limit); a full `check.sh` with the runner takes about 45 minutes;
  GitHub Actions queues of an hour or more after a push; the watcher of
  runs ends on a 520 of the mirror.
- Test flakiness: a tick of 1 s made `run_s` 31 not 30 (T-497, fixed);
  the `pids` test sized a guest too small (T-488, fixed); a test that runs
  a child podrunner in the process of `tests/local.rs` holds the lock of
  the runner directory (rule: such a test gets a test binary of its own).

Open questions.

1. For each hang, what is the first thing to capture when it happens
   again (a `wchan` and a stack of each thread, `strace -f`, `ss -xp`,
   `/proc/PID/fd`)? Could the user gate collect these automatically on a
   timeout (a "hang bundle" artifact)?
2. Can the Go runtime be told not to preempt with signals in a unit
   (`GODEBUG=asyncpreemptoff=1`) as a workaround, and does the .NET
   runtime have a similar knob? Does that change the T-261 behaviour?
3. What registry mirror strategy gives stable base-image pulls in CI
   (a ghcr.io mirror of the base image under podrunner's own control)?

Approach. Add timeout capture to the user gate. Mirror the base images on
ghcr.io (the ruling supports it).

Proof. Each hang has a bounded failure with a named cause, and a lane that
passes. No P1 hang is unexplained.

Verdict: hard (diagnosis), not impossible.

---

## 9. Items that wait on sibling projects (not hard in themselves)

| Entry | Waits on | State |
| --- | --- | --- |
| T-043 (P3) | podcrates: `pod-unit`, `pod-release` | `pod-release` exists now (v0.78.1); `pod-unit` none planned |
| T-248 (P1) | podcrates: `pod-manual` (COORD-062, #181) | check whether the crate shipped |
| T-250 (P2) | podcrates issue sync workflow | open |
| T-523 (P2) | podcrates `pod-release` | unblocked by v0.77.0 and v0.78.1; same as T-562 |
| T-524 (P3) | podcrates TLS end of the broker | unblocked by v0.78.1; same as T-563 |
| T-543, T-544, T-558, T-559, T-560 | podvm: T-1993 to T-1999 | podvm main has most; the next beta carries them |
| T-496 | podvm: T-1968 (sized disk of a guest) | open |
| T-403 | podvm: T-1972 or the namespace rung (T-532) | blocked |
| T-261 | podvm: beta after 0.2.0-beta.36 | wait and measure |
| T-220 / T-561 | podnet: shim behaviour | hard (H-16) |
| T-509 | podcrates: `pod-fake-githome` (v0.73.0 and later) | partial |
| T-082 | upstream `actions/runner` | blocked |

## 10. Truly impossible under the rules (list)

1. Isolation of a hostile static binary on the podvm userland and supervise
   rungs with no root, no namespaces and no KVM (H-1). Accepted as
   `redirect` by the owner for `trusted` jobs.
2. An unreadable runner credential with the official runner and steps in one
   uid (H-4).
3. Exact `ENOSPC` on the userland rungs with no root (H-5). Measure-and-kill
   is the fallback.
4. A TOCTOU-free mount source on the userland and supervise rungs without
   podvm kernel-side help (H-3).
5. File capabilities on the userland rungs (H-10).
6. The hosted kernel, its modules, its sysctls and swap in container modes
   (H-11).
7. An enforced egress hold on a host with no user namespace (H-15).
8. A trusted or untrusted tier on an emulated host with no Landlock and no
   hold, and an untrusted guest at usable speed with no KVM (H-6, H-28).
9. A placeholder for a secret that a client uses to sign (H-18).
10. A 55 GB writable copy inside a 40 GB bound (H-9): only a CoW design
    that counts writes can fit.

## 11. Dangers and defects found while writing this report

These go into the record as entries (rule: a finding must not stay only in
a report).

1. T-556 has no recorded result of a boot, yet the snapshot is published and
   the docs say "a unit boots the init of the image". The statement is
   unproven (level: not run).
2. `docs/HANDOFF.md` and `TODO/PROGRESS.md` give different pins (podcrates
   `v0.78.1` against `v0.70.0`; podvm `beta.36` against `beta.32`;
   podnet `beta.17` against `beta.15`). `TODO/PROGRESS.md` is stale.
3. `TODO/INDEX.md` counts 560 entries in `TODO/PROGRESS.md`, but the record
   has T-561 to T-563. Run `cargo todo check`.
4. `docs/hosted-snapshot.md` says T-541 (the runner layer) "is not done
   yet". A unit of the published snapshot cannot run a job until it is
   done.
5. T-523 and T-524 are `blocked` in the index, but podcrates `v0.77.0` and
   `v0.78.1` unblocked them (T-562, T-563 say so). Their blockers are
   stale.
