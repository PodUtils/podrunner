Milestones L2 and L3: the hook path, the guest self-test, the microVM unit, the fake broker and the live acceptance (GitHub #34 to #40).

# T-034: WP-13: measure U19 and U21 on the podvm rung (GitHub #34)

**Source:** GitHub #34
**Category:** measurement
**Milestone:** L2
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The text of GitHub #34 ("WP-13: measure U19 and U21 on the podvm rung"), cleaned:

**What and why.** The hook route needs three facts that no one measured.

- U21: `curl` runs under the podvm interposer; a host unix socket is visible inside the unit; bash can decode the framed exec stream (4-byte length, stream id, data). podvm PodUtils/podvm#235 (N7) (unix socket paths) is the podvm side.
- U19: a job container of another image, and a Docker action with its five mounts, can be honoured on a `net: shared-host` podvm unit; the image's own environment sets no tool cache variable. podvm PodUtils/podvm#236 (N8) (the container-hook verbs over podvm) is the podvm side.

**Content.** A script first, then a test of the podvm driver.

**Acceptance.**

- Each fact is answered with a log and recorded in #9.
- If bash cannot decode frames, #35 serves the plain mode (stdout as body, exit code from `GET /v1/exec/ID`).

Dependencies: #26. Cross-repo: podvm PodUtils/podvm#229 (N1) to PodUtils/podvm#232 (N4) (through #26), PodUtils/podvm#235 (N7), PodUtils/podvm#236 (N8). Estimate (spec): S.

## Premise

Read the text of GitHub #34 on 2026-11-11. Nothing measured.

## Approach

Measure the hook path facts U19 and U21 on the released podvm binary with `scripts/measure-hook-path.sh` and `scripts/measure-container-verbs.sh` (an image of curl and bash from `scripts/stand-in-image.py`), and write the result into `docs/unknowns.md`. The relay uses the plain mode of T-035 (the body of the answer and the header `X-Podrunner-Exit`), so bash decodes no frames. `podrunner-hook` binds and connects a socket path longer than the 108 bytes of `sun_path` through its directory.

## Prove

`sh scripts/measure-hook-path.sh PODVM DIR`, as root and as a uid with no user namespace.

## Done

2026-11-11, `2741daa` and `5e8e0c0`, podvm `v0.2.0-beta.3`. U21: curl and bash run on both rungs; a socket that the host binds in the rootfs of the unit is reachable on both rungs. U19: on the supervise rung (H0) a job container of another image runs with the seven job-container volumes (`run -d -v`); `exec -e -w` sees its environment, its working directory and the volumes (47 ms for each exec over 20); an action runs with the five action volumes and gives its exit code, its output and its logs; `rm -f` ends both. `:ro` is not applied on `exec` (T-062). On the namespace rung `-v` is refused (PodUtils/podvm@T-1653), so the container route of a podvm unit needs the supervise rung.

# T-035: WP-09: the hook relay and the control socket server of each unit (ctl) (GitHub #35)

**Source:** GitHub #35
**Category:** feature
**Milestone:** L2
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

The text of GitHub #35 ("WP-09: hook relay and the per-unit control socket server (`ctl`)"), cleaned:

**What and why.** `container:` jobs and container actions use the official container hooks (`ACTIONS_RUNNER_CONTAINER_HOOKS`), not a Docker API (DR-8, #8). The hook is a `.sh` relay in the unit image: it posts its stdin to `POST /v1/hook/COMMAND` on the unit's control socket and appends the answer to the response file. A step in the same uid can skip the hook, so the host-side server enforces everything.

**Content (the closed field set).**

- Order: `prepare_job` once for each unit, before any step command; every container operation before it is refused. A job with no `container:` and no `services:` gets no `prepare_job`, so all its container operations are refused.
- Two fixed mount tables, matched by exact target. Job container (seven): `/__w`, `/__w/_temp`, `/__w/_actions`, `/__w/_tool`, `/__e` (read-only), `/github/home`, `/github/workflow`. Container action (five): `/github/runner_temp`, `/github/home`, `/github/workflow`, `/github/file_commands`, `/github/workspace`. Sources are directories of the unit, resolved with `RESOLVE_BENEATH`. `/__t` is refused.
- The system mount `/var/run/docker.sock` (both sides that literal) is accepted and dropped. Every other socket bind is refused.
- No host path, device, privileged flag or network mode but the default. `ports` refused on `net: shared-host`. `services:` on `shared-host` is `container-not-supported`. Images only from a registry host in `registries`; a registry credential only for the image's own registry host. `docker build` and Dockerfile actions: `build-not-supported`. podvm health options: `container-not-supported`.
- `POST /v1/job_state`: `job-started` moves `started` to `running` once; `job-completed` records the time and changes no state.
- `prepare_job` answers with `isAlpine` and `context.services` as an array.
- `podrunner` sets the hooks variable for every unit with `container: hook`.

**Acceptance.**

- Unit conformance tests 8, 9, 13, 14, 16, 17, and the container parts of test 3, pass on the `fake` unit path; then on the podvm driver after #34.
- The hook parser and the HTTP server have fuzz targets.
- A registry password in stdin appears in no log, report or `status` (canary).

Dependencies: #23, #38 (fake path), #34 and podvm PodUtils/podvm#229 (N1) to PodUtils/podvm#232 (N4) (H0 path). Cross-repo: podvm PodUtils/podvm#236 (N8) (hook verbs), PodUtils/podvm#235 (N7) (the control socket reaches the unit), and the `exec` workdir inside a volume and a split stderr (WP-V6 of the spec) if the hook path needs them. Estimate (spec): M.

## Premise

Read the text of GitHub #35, `HookInput.cs`, `HookResponse.cs`, `ContainerHookManager.cs` and `ContainerInfo.cs` of `actions/runner` v2.338.2 on 2026-11-11. The hook input of the tests follows those files; no runner ran a hook here.

## Approach

The crate `podrunner-hook`:

- `input.rs`: the input of a hook call as a closed set, pinned to `HookInput.cs` of `actions/runner` v2.338.2. `policy.rs`: the checks of the host (`docs/contracts/hook.md`). A password is in no `Debug` text and in no refusal. Fuzz target `hook_input`.
- `http.rs`, `server.rs`: the control socket of each unit (one request for each connection, bounded lines and headers, a body of at most 1 MiB, no chunked body, `POST /v1/hook/COMMAND`, `POST /v1/job_state`, a refusal as HTTP 403 with its code). The relay `crates/podrunner-hook/relay/podrunner-hook.sh`. Fuzz target `hook_http`.
- `unit.rs`: the plan of each call on `UnitDriver`. The runner sends the job container with no entry point; the host starts it with `tail -f /dev/null`.
- The seam `HookPort` of slots (`crates/podrunner-slots/src/hooks.rs`, served by `crates/podrunner/src/hooks/mod.rs`): a unit of a kind with `container: hooks` gets a control socket before its start and the variables `ACTIONS_RUNNER_CONTAINER_HOOKS` and `PODRUNNER_CTL`; each refusal is an audit line `hook-refuse`; `job-started` moves `started` to `running` once. The seam gives slots refusal codes and job events only, so no text of a call reaches an audit line, a report or `status`.
- The fake unit maps each mount target to its source (`crates/podrunner-fake/src/mounts.rs`). The podvm driver has the container route on the supervise rung (`crates/podrunner-podvm/src/unit/containers.rs`: `run -d -v`, `exec -e -w`, `logs`, `inspect`, `rm -f`), and mounts the socket directory of each unit at `/run/podrunner`.
- The runner takes the hooks only with the job variable `DistributedTask.AllowRunnerContainerHooks`; the fake broker sends it. An empty `serverUrl` of a registry is `docker.io`.

## Prove

`cargo test -p podrunner-hook`

## Done

2026-11-11, `fda577d`; gate run 42991333922, `runner-image.yml` run 42991333919. The official runner v2.338.2 runs a `container:` job through the relay, the control socket, the checks and the containers: on the fake unit (`crates/podrunner-broker-fake/tests/container.rs`, `level2.yml`) and in a podvm unit with no root (`crates/podrunner-broker-fake/tests/podvm.rs`): the step runs in the job container, the job succeeds, no refusal, `destroy` counts 0 processes and 0 files. The design unit tests 3, 8, 9, 13, 14, 16 and 17 pass on the fake path (T-053); unit test 14 passes on the podvm driver. The registry password of the job reaches no log of the broker, no file of the runner and no file of the unit or of the hooks. Open elsewhere: `:ro` (T-062), absolute links (T-061).

# T-036: The guest floor self-test (GitHub #36)

**Source:** GitHub #36
**Category:** feature
**Milestone:** L2
**Priority:** P2
**Effort:** S
**Status:** blocked

## Problem

The text of GitHub #36 ("Guest floor self-test"), cleaned:

**What and why.** `untrusted` jobs need boundary `guest` (a separate kernel). `boundary_proof` is a string that the driver supplies, so the floor is tested, not declared.

**Content.**

- `guest` is accepted only from driver kinds on an allowlist (`microvm` and the kinds the operator lists), after a self-test at `slots` start, on every driver change, and daily.
- The test starts a unit and checks: no network interface; a canary file and a loopback listener that `slots` planted under names the unit does not know are not visible or connectable; a second unit cannot see the first; the boot id differs from the host's and no host pid is visible.
- On every `guest` start: every mount is read-only or a block image, and only the table's host paths are visible.
- Driver executables come from one configured directory with owner and mode checks, never from `PATH`.

**Acceptance.**

- Unit conformance test 11: a `fake` driver that claims `guest` fails; a real guest passes.
- `fake` is refused unless `allow_fake = true`.

Dependencies: #21. Estimate (mine): S to M.

## Premise

Read the text of GitHub #36 on 2026-11-11. Nothing measured.

## Approach

Done (`crates/podrunner-slots/src/guest.rs`): a test unit writes the boot id of its kernel, its interfaces other than `lo`, whether a canary file of the host is visible, whether the process of podrunner is visible, and whether a listener that the test plants on the loopback of the host takes a connection. The boot id and the kernel version string (`/proc/version`) must differ from the host's, and each other check must find nothing. The boot id alone is not a proof: LXC and incus give a system container a boot id of its own (the measure of T-105 step 1). A guest that boots the same kernel build as the host fails too: the test fails safe. A second unit beside a first must not see the process or the file of the first. `serve` accepts a claim of `guest` only from a kind on `guest_kinds` (default `microvm`) and after this test; else the claim drops to `none` (`boundary_proof: guest-self-test-failed`). Each kind that claims `guest` is tested again each day (`RETEST_MS`) and when its driver file changes. Each driver is an absolute path, in `driver_dir` when it is set, and `serve` refuses a file or a directory that another user owns or that the group or the others can write. A fake unit that claims `guest` fails the test.

Open: the read-only mounts on each `guest` start, and a pass on a real guest. Both need the `microvm` kind (T-037).

## Prove

`cargo test -p podrunner-slots guest`

## Blocker

The read-only mounts and the pass on a real guest need the `microvm` kind (T-037). podvm landed its machine tier (PodUtils/podvm@T-1656); T-037 is in work (`crates/podrunner-microvm`).

# T-037: WP-11: the microvm unit driver (form set by DR-5) (GitHub #37)

**Source:** GitHub #37
**Category:** feature
**Milestone:** L2
**Priority:** P2
**Effort:** M
**Status:** partial

## Problem

The text of GitHub #37 ("WP-11: `microvm` unit driver (form set by DR-5)"), cleaned:

**What and why.** The `untrusted` tier (fork pull requests) needs a guest. Host class H2 (`/dev/kvm` usable, x86_64) serves `untrusted` and `trusted`. aarch64 has no `untrusted` unit in v1.

**Content (either form).** No NIC; a vsock channel to the broker (hold `enforced`); the credential directory on the guest's tmpfs (`credential_store: tmpfs`, `principal: distinct`); OCI image to root filesystem; a pinned kernel; `container: none` until a guest runner image exists; a bounded timeout for every guest and no emulator left after a run.

- Own driver (if DR-5, #5, says so): port of a launcher in a private archive of the owner for a user in group `kvm`, no `sudo`; retired when PodUtils/podvm@T-1656 lands.
- Adapter (default): the podvm machine tier once PodUtils/podvm@T-1656 lands.

**Crate dependencies (crate item R-C14 of the study, merged here).** `pod-microvm` is a reserved name in podcrates with a study only (PodUtils/podcrates@T-062). podcrates was told that the `microvm` kind waits for PodUtils/podcrates@T-062 or PodUtils/podvm@T-1656: PodUtils/podvm#227, section 2.7. The `guest` floor needs the self-test of #36.

**Acceptance.**

- The #36 self-test passes on a KVM host.
- Unit conformance tests 1 to 7, 10, 11, 12 and 15 pass.
- A fork pull request offer on the untrusted set runs a `run:` job in the guest on the fake source; a `container: true` need is `container-not-supported`.

Dependencies: #5, #36. Cross-repo: PodUtils/podvm@T-1656 (adapter form), PodUtils/podcrates@T-062 (PodUtils/podvm#227 section 2.7). Estimate (spec): M to L.

## Premise

Read the text of GitHub #37 on 2026-11-11. Nothing measured.

## Approach

An adapter over the machine tier of podvm (DR-5, 2026-11-11): the `microvm` kind starts a podvm unit with `--podvm-tier=machine`, and slots accepts its `guest` claim only after the self-test of T-036. Keep it behind `UnitDriver`, so that an incus adapter (the owner's note to podvm) can be a second kind.

Steps (2026-11-13, podvm `v0.2.0-beta.6`):

1. A measure on a hosted runner with `/dev/kvm` (the udev rule of podvm's `kvm.yml`): the runner image boots in the machine tier, `/home/runner/run.sh` is there, a `-v` 9p mount takes a write back to the host, and the guest reaches a listener on the loopback of the host at `10.0.2.2`.
2. The crate `podrunner-microvm`: `MicrovmUnit` behind `UnitDriver`. `start` spawns `podvm run --rm --podvm-tier=machine -v CRED:/home/runner/.cred:ro IMAGE run.sh` as a child process; `wait` waits on it; `destroy` ends it and removes its store. The capability: `container: none`, `credential_store: tmpfs` only if the guest copy is proved, else `private-dir`; `boundary: guest` only after T-036.
3. A canned job of the fake broker in a guest: the broker listens on the loopback of the host and gives the runner `10.0.2.2:PORT`.
4. The unit conformance tests 1 to 7, 10, 11, 12 and 15 on the kind; then T-036 on it.

Step 1, run 43345530725 (`cea9541`): the guest did not boot. QEMU refused the rootfs: "initrd is too large, cannot support (max: 536707071, need 888068384)". The machine tier loads the image as an initramfs, and the runner image packs to 888 MB; the limit of QEMU follows the guest memory. The 9p volume was set up. The measure now gives `--podvm-mem 4G --podvm-cpus 2`. If a large guest still fails, ask podvm for a rootfs on a disk (virtio-blk) by a COORD issue.

Step 1 is done: in runner-image run 43355069666 of `a3999f1`, with `--podvm-mem 4G --podvm-cpus 2`, the guest boots in 4 s (kernel `6.12.110-0-virt`), the runner files are there, a 9p volume takes a write back to the host, and the guest reaches a listener on the loopback of the host at `10.0.2.2`. Step 2, the code: the crate `podrunner-microvm` (`MicrovmUnit`): `prepare` checks the pin and the platform and keeps the reference; `start` writes the credential files at mode 0600 into a private directory of the unit and spawns `podvm run --rm --podvm-tier=machine --podvm-mem 4G --podvm-cpus 2 -v CRED:/mnt/podrunner-cred:ro IMAGE /bin/sh -c SCRIPT`; the script copies the files into the runner root at 0600 and runs the payload as the user `runner` of the image (`setpriv`); `wait`, `signal` and `destroy` act on the `podvm run` process and each process with the tag of the unit; `probe` says `guest` (a claim, T-036), `container: none`, `credential_store: private-dir`, `tree_kill: unverified`. Six tests with a fake podvm hold the argv, the modes, the end and the residue. `tests/conformance.rs` runs the tests 1 to 7, 10, 11, 12 and 15 on real guests in `runner-image.yml`. In runner-image run 43360551265 (`c82bad8`) 7 of the 11 tests passed on real guests (1, 2, 4, 6, 10, 12, 15). Tests 3 and 7 asked the host for three processes of the unit, and tests 5 and 11 for a unit root on the host; a guest shows neither. The fix (`docs/decisions.md`, the call of 2026-11-13): the payload runs in a 9p work directory of the unit, which `observe` gives as the unit root; tests 3 and 7 need one host process (the emulator) for a kind that claims `guest`. A container call is refused with `container-not-supported`. Runner-image run 43372567562 (`f6d48b8`): 9 of 11 pass (3 and 7 too). Tests 5 and 11 fail: the payload, as the user `runner` of the image, wrote no marker into the 9p work directory. The driver now keeps the output of the guest (`unit_logs`; `with_kept_logs` for the suite), and `measure-machine.sh` writes into a 9p volume as `runner` too. Runner-image run 43375786253 (`8eb3905`) gave the cause in the log of the guest: "cp: not writing through dangling symlink '/home/runner/.credentials'". The runner image links the three credential files of `/home/runner` to `/podrunner/.runner/` (`images/runner/Dockerfile`), so the script stopped before the payload. A 9p write as `runner` works (`machine 9p-runner ok`). The guest now writes the files into `/podrunner/.runner`, as the podvm kind does. Step 2 is done: in runner-image run 43379538641 (`208c391`) each of the 11 tests passes on real guests under KVM. `runner-image.yml` runs them as a check in their own step. Step 3, the code: `Broker::start_seen_at("10.0.2.2")` listens on the loopback and gives that host in each URL; `crates/podrunner-broker-fake/tests/microvm.rs` runs a canned job of the official runner in a guest, in `runner-image.yml`. In runner-image run 43383337920 (`c546d37`) the 11 unit tests pass again, and the canned job fails: the guest exits 126 and the log holds no line of the runner. The test runs as a measure now (the step of the machine measure), and `measure-machine.sh` prints the owners and modes of `/home/runner`, `run.sh` and `Runner.Listener` in a guest, and runs `run.sh --version` as `runner`. Runner-image run 43388584016 (`90f3492`) gave the cause: in a guest of the machine tier each file of the image belongs to root (`/home/runner` is `root:root 750`), so `run.sh` as `runner` exits 126 ("Permission denied"). GitHub #81 (COORD-021) asks podvm to keep the owners. The guest script now gives `/home/runner` back to `runner` before it drops to that user. Step 3 is done: in runner-image run 43392618033 (`76ee1bf`) the official runner v2.338.2 in a guest under KVM reaches the fake broker at `10.0.2.2`, takes the job `in-guest` and completes it (`Succeeded`); the unit leaves no residue. It is a check in `runner-image.yml`. Step 4, the code: `[unit.microvm]` (`mem`, `cpus`, `binary`, `images`) in `podrunner.toml`; `serve` and `check` build the kind (`crates/podrunner/src/microvm_kind.rs`); `check_and_serve_refuse_a_bad_file_with_one_text` holds a bad binary and bad vCPUs; `docs/configuration.md` ("The microvm kind"). Next: the acceptance of GitHub #37 on the fake source (a fork pull request on the untrusted set runs in a guest; a `container:` need is `container-not-supported`), then T-036.

Step 5, the code (podvm `v0.2.0-beta.9`, PodUtils/podvm#303): the guest has no NIC (`--podvm-net none`). With a socket directory, `start` adds `--podvm-channel SOCKETS/egress.sock:127.0.0.1:3128` and gives the payload `http_proxy` and `https_proxy` of that address; `probe` says `net: none` and `egress_hold: enforced`. The podvm process runs with umask 077: beta.9 makes the socket of the channel in the temp directory. The guest script keeps the owners of the image (no `chown` of `/home/runner`, GitHub #81). `crates/podrunner-microvm/tests/selftest.rs` runs the self-test of T-036 on a real guest, and the canned job of `tests/microvm.rs` reaches the broker only through the channel, by a small proxy on the egress socket. Both are checks in `runner-image.yml`. Next: the proof in a runner-image run, then the acceptance of GitHub #37. Runner-image run 43417481104 (`a918fac`): the 11 unit tests and the self-test of T-036 pass on real guests with no NIC under KVM (the claim `guest` holds). The canned job fails: the runner connects to its proxy at `127.0.0.1:3128` and waits 100 s with no answer, and no connection reaches the egress socket of the host. `measure-machine.sh` now measures the channel alone (a unix socket of the host that answers each connection; `curl` in the guest, direct and as a proxy, as root and as `runner`), and the proxy of the test logs each accept. podvm `v0.2.0-beta.10` is pinned: its channel runs on the `pod-link` of podcrates `v0.39.0`, with its socket in a directory of mode 0700. Runner-image run 43423868655 (`0c1642a`, beta.9): the channel alone works under KVM (`curl` in a guest with no NIC, as root and as `runner`, direct and as a proxy, reaches a unix socket of the host), but the canned job still gets no connection. The measure now runs the channel beside the two 9p volumes of the kind, as the driver does.

## Decision

2026-11-13 (a call with no ruling): the first form keeps the user-mode NIC of the machine tier (podvm has no form with no NIC), and runs QEMU under `podnet pod`, so each connection of the guest leaves through the egress module. The form with no NIC and one host channel to the broker waits for the host channel of podvm (PodUtils/podvm@T-1750, PodUtils/podvm@T-1751). `docs/decisions.md` records the call.

## Prove

`cargo test -p podrunner-microvm`

## Correction

2026-11-13 (later): the call of the first form (the user-mode NIC under `podnet pod`) does not hold. The self-test of the guest (T-036, `crates/podrunner-slots/src/guest.rs`) refuses a unit with a network interface but `lo`, as the design of GitHub #37 asks ("no NIC"); a guest with `eth0` never gets the claim `guest`, so no untrusted job runs on it. The kind needs a guest with no NIC and one host channel to the broker (PodUtils/podvm@T-1751, asked in GitHub #83).

2026-11-13: podvm landed its machine tier (PodUtils/podvm@T-1656, done; `v0.2.0-beta.6` adds 9p volumes and `-t`). The guest has a user-mode NIC (`10.0.2.15`, the gateway `10.0.2.2` is the loopback of the host), runs only in the foreground (no `run -d`, no `exec`: PodUtils/podvm@T-1750), and needs `qemu-system-x86_64` on the host. A unix socket does not cross into the guest, so the hooks of the container path cannot reach it.

# T-038: WP-12: the canned-message fake broker (level 2) (GitHub #38)

**Source:** GitHub #38
**Category:** feature
**Milestone:** L2
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The text of GitHub #38 ("WP-12: canned-message fake broker (level 2)"), cleaned:

**What and why.** Level 2 runs the real official runner offline. A fake broker serves the runner's routes (`session`, `message`, `acquirejob`, `renewjob`, `completejob`, and the results routes) from recorded or hand-written job messages. Also, a seven-workflow conformance contract from a private archive of the owner runs against `runner.server` inside a unit (its script removes every container on the host; change it first).

**Fakes stay here (crate item R-C10 of the study, merged here).** The fakes of GitHub stay in podrunner (PodUtils/podcrates@T-106; cfb ask A-X-18). This broker can use `pod_testkit::stand::http` (podcrates `v0.14.0`, feature `stand`: an HTTP/1.1 server on the loopback that records what each client sent) as its base. No ask to podcrates (PodUtils/podvm#227, section 2.7, note 2).

**Acceptance.**

- The runner in a podvm unit takes one canned `run:` job, runs it and completes it.
- The corpus holds the `ubuntu-24.04` jobs of the podcrates workflows that need no `sudo`.

Dependencies: #26. Estimate (spec): M to L.

## Premise

Read the text of GitHub #38 on 2026-11-11. Nothing measured.

## Approach

`crates/podrunner-broker-fake` serves the routes that runner v2.338.2 calls with the JIT files of a scale set (`.runner` with `serverUrlV2` and `useV2Flow`; `.credentials` with scheme `OAuth`; `.credentials_rsaparams`): `POST /_apis/oauth2/token`, the broker session (create, delete) and the long poll of `/message`, the run service (`acquirejob`, `renewjob`, `completejob`), and the results service of the Worker (the signed blob URLs, the `PUT` of each blob, the metadata, `WorkflowStepsUpdate`). A body over 1 MiB is refused. The runner needs an environment with no proxy variables of the sandbox (`RunnerWebProxy` throws on them). A `Job` carries a name, the `env:` of the workflow, the job and each step, and the `run:` steps with their `if:`. `corpus/podcrates.json` (written by `scripts/corpus.py` from a commit of podcrates) holds the `ubuntu-24.04` jobs that the fake can run, and lists each other job with the reason; the checkout step becomes a clone of `PODRUNNER_CORPUS_SRC`. `.github/workflows/level2.yml` runs the official runner against it (two corpus jobs on each push, the whole corpus each week), with the tarball checked by `podrunner image verify`.

## Prove

`cargo test -p podrunner-broker-fake`

## Done

2026-11-11. Both parts of the acceptance hold: the runner in a podvm unit takes one canned `run:` job and completes it (T-026, `1db8bc3`, `runner-image.yml` run 42987612426, `crates/podrunner-broker-fake/tests/podvm.rs`), and the corpus holds the `ubuntu-24.04` jobs of the podcrates workflows that need no `sudo` (`corpus/podcrates.json`, the corpus job of `level2.yml`).


# T-039: Milestone L2: level 2 green, the hook path measured (GitHub #39)

**Source:** GitHub #39
**Category:** chore
**Milestone:** L2
**Priority:** P2
**Effort:** S
**Status:** blocked

## Problem

The text of GitHub #39 ("Milestone L2: level 2 green, hook path measured"), cleaned:

**Acceptance.**

- #34, #35, #36, #38 closed; #37 closed or blocked on DR-5 (#5) with a record.
- Level 2 runs in a scheduled workflow; the run link is recorded.
- A canned `container:` job passes the hook path on the podvm driver, or each refusal is named.

Dependencies: #34 to #38. Estimate: the sum of its issues.

## Premise

Read the text of GitHub #39 on 2026-11-11. Nothing measured.

## Approach

Close L2 when T-034 to T-038 are done and the level 2 suite is green.

## Prove

`cargo test --workspace -- --include-ignored level2`, and the gate run of the closing commit.

## Blocker

T-036 (the guest floor self-test needs a real guest kind) and T-037 (the microvm driver waits for DR-5). The other parts hold: T-034 and T-035 are done (a canned `container:` job passes the hook path of the podvm driver in `runner-image.yml`), T-038 is done, and level 2 runs on a schedule (`level2.yml`, each Monday).


# T-040: Level 3: the live acceptance suite (GitHub #40)

**Source:** GitHub #40
**Category:** chore
**Milestone:** L3
**Priority:** P3
**Effort:** M
**Status:** blocked

## Problem

The text of GitHub #40 ("Level 3: live acceptance suite"), cleaned:

**What and why.** The claims that need github.com. Nothing here is observed yet. It needs the throwaway organization of DR-7 (#7).

**Acceptance.**

- The 20-job run of cfb ask A-X-01.
- G-H1 (container-hooks job variable), with U14 (half-set behaviour).
- Label-race and tier tests (G-H4) on the live set.
- The stolen-credential bound (U11): a step that reads the credential files cannot take another job.
- Scale-set label count (U7), App permissions (U8, U12), the runner group refuses public repositories.
- The alert of the leak runbook (#19) fires on a foreign registration.
- U17, U18, U20, U22, U23 answered in #9.
- A container action and a `container:` job on the hook route (G-09 as restated in #6).

Dependencies: #7, #33, #39. Estimate (mine): M.

## Premise

Read the text of GitHub #40 on 2026-11-11. Nothing measured.

## Approach

A live acceptance suite against GitHub with a test organization.

## Prove

`cargo test --workspace --test acceptance -- --ignored`, in a live workflow.

## Blocker

2026-11-13: the owner ruled DR-7 yes. The live gates run on constrained hosts by two agents: GitHub #74 (LIVE-1, the remote mint and the App key) and GitHub #75 (LIVE-2, the unit host and the test jobs). This entry waits for their reports. Before: The ruling "Test accounts" (Q-A9): not now (DR-7, T-007).

# T-049: COORD-003: the stand-in broker of the official runner for podvm (GitHub #50)

**Source:** GitHub #50
**Category:** chore
**Milestone:** L2
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

podvm asked in PodUtils/podvm#248 (COORD-030, PodUtils/podvm@T-1706) for the stand-in of the broker that podrunner measured (T-038, step 1): the requests of the official runner up to the long poll, the shape of its three files, and its environment. Its session cannot read this repository. With it, podvm proves `run.sh` on its userland rung (the M-B path of T-026).

## Premise

The answer comes from the measure of T-038 on 2026-11-11 with runner v2.338.2 and `crates/podrunner-broker-fake`.

## Approach

GitHub #50 (COORD-003, to podvm) gives the requests in order with their answers, the three files with made-up values, and the environment. Close it when podvm reads it; take the result of PodUtils/podvm@T-1706 into T-026.

## Prove

`cargo todo check`

## Done

2026-11-11: podvm read GitHub #50 and closed PodUtils/podvm#248: its `experiments/runner-standin.py` follows the stand-in (token, session, a 202 long poll), and PodUtils/podvm@T-1706 is done at podvm `cd2c691`: the official runner v2.338.2 runs `run.sh` up to the long poll on the userland rung (podvm `main`, not released). GitHub #50 is closed.

# T-050: COORD-004: the supervise rung does not map the path of connect() for a unix socket (GitHub #51)

**Source:** GitHub #51
**Category:** defect
**Milestone:** L2
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

On the supervise rung of the released podvm `v0.2.0-beta.2`, a payload cannot connect to a unix socket by the path that it sees (a volume target or a path of its rootfs): the interposer maps `stat` and `open`, not the `sun_path` of `connect`. A `connect` to the host path works, though the unit cannot `stat` it. The relay of the container hooks (T-035) connects to the control socket of its unit. GitHub #51 (COORD-004, to podvm) records it; podvm is on break.

## Premise

Measured on 2026-11-11 with `scripts/measure-hook-path.sh` (T-034), as `nobody` with a seccomp filter that refuses `CLONE_NEWUSER`: `volume-socket no`, `rootfs-socket no`, `host-path-socket yes`.

## Approach

The podvm driver of T-035 puts the control socket in the rootfs of the unit, with its guest path in `PODRUNNER_CTL`, once podvm maps the path of `connect` (PodUtils/podvm@T-1710). The interposer is no boundary: a program that makes the system call itself reaches each host unix socket of its uid (PodUtils/podvm@T-1652).

## Prove

`sh scripts/measure-hook-path.sh PODVM DIR` as a uid with no user namespace gives `volume-socket yes` and `rootfs-socket yes`.

## Done

2026-11-11, `016a78f`, podvm `v0.2.0-beta.3` (PodUtils/podvm@T-1710). `scripts/measure-hook-path.sh` on the H0 profile (supervise): `volume-socket yes`, `rootfs-socket yes`, `host-path-socket no` (as podvm said: the host path is not a guest path). On the root rung (namespace, a chroot rung): `rootfs-socket yes`, and `-v` stays refused there by design (PodUtils/podvm@T-1653). So a socket that the host binds in the rootfs of the unit is reachable on both rungs: the podvm driver of T-035 puts the control socket there, with its guest path in `PODRUNNER_CTL`, and needs no host path.

# T-064: The podvm driver follows the tree of each container (PodUtils/podvm@T-1709)

**Source:** PodUtils/podvm#258
**Category:** feature
**Milestone:** L2
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

podvm `main` (`cbb9134`, PodUtils/podvm@T-1709) copies the image into `<store>/trees/<id>/` at each `run` and `create`; each container writes only in its copy. The podvm driver takes the rootfs from `podvm extract` (`crates/podrunner-podvm/src/unit.rs`): it writes the credential files there before `run` (the copy takes them), but `observe` (the unit root, the runner root) and the volume sources of the hooks (`unit/containers.rs`, relative to the runner directory) name the extracted rootfs, not the tree of the container. With such a podvm, the runner writes `_work` in its tree, and a job container mounts the extracted copy: the container route of T-035 breaks.

## Premise

Read on 2026-11-11 in PodUtils/podvm#258. Not in a podvm release yet; the next beta takes it. Not measured here.

## Approach

T-067 gives the driver the tree that `podvm inspect` names. With the next beta: raise `probe::LOWEST`, pin it in `images/podvm.toml`, and run the unit tests (25 and 26 also) in `runner-image.yml`; give the result of 25 and 26 to the session of podcrates (PodUtils/podvm#262).

## Prove

`runner-image.yml` with the new beta: the two tests of `crates/podrunner-broker-fake/tests/podvm.rs` and the unit conformance suite on podvm pass.

## Done

2026-11-11, `baab86e`; runner-image run 43100514216 and gate run 43100514057 of `baab86e` are green. podvm `v0.2.0-beta.4` is pinned (`images/podvm.toml`, `probe::LOWEST`). The two tests of `crates/podrunner-broker-fake/tests/podvm.rs` and the unit conformance suite pass on it. Test 26 found a defect, fixed in place: a killed launcher now ends the payload, so `destroy` counts the processes as unknown when podvm says `dead`, and the escape payloads write the pid before the kill. Run 43103328827 once failed test 26 with `podvm run: exit 125` at the start of a unit; it did not come again (run 43105916054), and a failed podvm call now names the last line of its error (`44df9bf`).

# T-067: The podvm driver takes the root of a unit from podvm inspect

**Source:** T-064
**Category:** feature
**Milestone:** L2
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

The podvm driver takes the rootfs of a unit from `podvm extract` (`crates/podrunner-podvm/src/unit.rs`), and uses it for `observe` and for the volume sources of the hooks (`crates/podrunner-podvm/src/unit/containers.rs`). podvm `main` (PodUtils/podvm#258, PodUtils/podvm@T-1709) copies the image into `<store>/trees/<id>/` at each `run`; `podvm inspect --format '{{.RootfsPath}}' NAME` names that tree. With the next beta, the runner writes `_work` in its tree and a job container mounts the extracted copy: the container route breaks (T-064).

## Premise

Read PodUtils/podvm#258 and the driver on 2026-11-11. `podvm inspect NAME` of `v0.2.0-beta.3` gives JSON with `RootfsPath` (`scripts/measure-hook-path.sh` reads it). Not measured: whether `RootfsPath` of beta.3 equals the path of `extract`.

## Approach

After `run -d`, read `RootfsPath` of the unit container from `podvm inspect` and keep it as the root of the unit: `observe`, the runner directory, the relative volume sources. Keep the credential files written before `run` (the copy of the next beta takes them); after `run`, check their mode 0600 under the new root, and fail the start (`unit-start-failed`) if one is missing. A test with a stand-in `podvm` script whose `inspect` names another directory than `extract` holds the logic with no new podvm. Run the unit suite on beta.3 in `runner-image.yml`. Then T-064 is only the pin of the new beta.

## Prove

`cargo test -p podrunner-podvm rootfs`, and the unit conformance suite and `crates/podrunner-broker-fake/tests/podvm.rs` in `runner-image.yml` on podvm `v0.2.0-beta.3`.

## Done

2026-11-11, `aa1d2b8`, `runner-image.yml` run 43067388243: after `run -d` the driver takes `RootfsPath` of `podvm inspect` (`crates/podrunner-podvm/src/unit/root.rs`) and checks each credential file there at mode 0600; on beta.3 it is the extracted rootfs, and the unit suite, the hold test and the broker-fake tests pass on podvm in CI.
