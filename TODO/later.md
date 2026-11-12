The crate dependencies later than v1 (GitHub #41 to #43).

# T-041: Hold enforced on podvm units (pod-hold or pod-sud) (GitHub #41)

**Source:** GitHub #41
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

The text of GitHub #41 ("Hold `enforced` on podvm units (`pod-hold` or `pod-sud`)"), cleaned:

**What and why.** The hold says how the network traffic of a unit reaches the egress broker: `none`, `advisory` (a seccomp redirect that a payload can go around), or `enforced`. The `untrusted` tier needs `enforced`. A podvm unit on a no-root host reaches `advisory` at most today (through `podnet pod`). With an `enforced` hold on podvm units, podrunner can serve more than `trusted` jobs at `advisory` there, and the fixed denies of the broker hold.

**Dependencies in other repositories.**

- PodUtils/podcrates@T-109 (hold research) decides the crate (`pod-hold` or `pod-sud`). The hold reports a level and a proof. podcrates ask: PodUtils/podvm#227, section 2.7.
- podnet said in PodUtils/podnet#414 that the import of the generic hold comes after its backend split (PodUtils/podnet@T-040) and seccomp fixes (PodUtils/podnet@T-142 to PodUtils/podnet@T-158). Those entries look done. podnet ask: PodUtils/podvm#228, section 2.9.

**Content (podrunner side).** The podvm driver (#23) reports `egress_hold: enforced` with its proof when the crate gives one; placement (#21) and egress (#24) read it. No change to the seam contract (#16): `egress_hold` exists already.

**Acceptance.**

- PodUtils/podcrates@T-109 names the crate and its proof form.
- The podvm driver reports `enforced` with a proof only where the crate proves it, and a test plants a false claim that fails.

Later than v1: no milestone. Dependencies: PodUtils/podvm#227 section 2.7 (PodUtils/podcrates@T-109), PodUtils/podvm#228 section 2.9. Estimate (mine): S on the podrunner side.

## Premise

Read the text of GitHub #41 on 2026-11-11. Measured on 2026-11-11 with `scripts/measure-podnet-pod.sh` (podvm `v0.2.0-beta.3`, podnet `v0.4.0-beta.3`): as root (the namespace rung, `hold.netns` `enforced`), `BACKEND=netns` holds the payload after `podnet pod` returns, and a direct connect of the payload or of a held exec to the loopback of the host fails (curl exit 7); as uid 65534 with no user namespace, `seccomp` lets both through.

## Approach

`pod-hold` is in podcrates `v0.24.0`: the seccomp hold (`advisory`) and the netns hold (`enforced`: a new network namespace in a new user namespace, its only route a TUN that the supervisor holds). The netns hold needs unprivileged user namespaces and `/dev/net/tun`, so H0 gets `advisory` at most; a host with user namespaces (the namespace rung of podvm) can get `enforced`. Measure `podnet pod --backend netns` around `podvm run -d` on such a host; when it holds, the podvm driver reports `egress_hold: enforced` with the proof of `pod doctor --json` (`hold.netns`), and a test plants a false claim that fails.

## Prove

`cargo test -p podrunner-podvm hold`, and `cargo test -p podrunner-podvm --test hold -- --ignored` as root and as a uid with no user namespace

## Start condition

T-065 is done: the podvm driver runs its units under `podnet pod` with the `advisory` hold.

## Done

2026-11-11, `349f0ba`: `with_podnet` takes the backend `netns` (`egress_hold: enforced`) when `podnet pod doctor --json` says `hold.netns` `enforced`, else `seccomp` (`advisory` at most, also when it claims more: the plant of `the_hold_comes_from_pod_doctor`). The hold test checks the claim: a direct loopback connect of the payload reaches the host only under `advisory`. It passes as root (netns) and as uid 65534 under `nouserns` (seccomp). Gate run 43076066364 and runner-image run 43076066444 (the hold test) of `e5bdddb` are green.

# T-042: The confined boundary (pod-confine) (GitHub #42)

**Source:** GitHub #42
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

The text of GitHub #42 ("`confined` boundary (`pod-confine`)"), cleaned:

**What and why.** The boundary order is `none` < `redirect` < `confined` < `guest`. `redirect` rewrites file paths and is not isolation against hostile code; every podvm rung is `redirect`. `confined` is a kernel-enforced boundary with no root (Landlock, seccomp, rlimits). With it, `trusted` jobs run on a no-root host above `redirect` (DR-4, #4).

**Dependencies in other repositories.** PodUtils/podcrates@T-108 (`pod-confine`) gives a capability record with `boundary: confined` and a proof string. podcrates ask: PodUtils/podvm#227, section 2.7.

**Content (podrunner side).** A unit kind or a podvm driver option that reports `boundary: confined` with the proof of PodUtils/podcrates@T-108. The capability schema (#17) and placement (#21) take `confined` already.

**Acceptance.**

- A unit reports `confined` only with a proof from `pod-confine`, and a planted false claim fails.
- A `trusted` job runs on a `confined` unit when the operator keeps the default floor (DR-4 allows `redirect` only as a choice).

Later than v1: no milestone. Dependencies: PodUtils/podvm#227 section 2.7 (PodUtils/podcrates@T-108). Estimate (mine): -.

## Premise

Read the text of GitHub #42 on 2026-11-11. Nothing measured.

## Approach

A unit kind with `boundary: confined` and the proof of `pod-confine`.

Done (podcrates `v0.24.0`): `crates/podrunner-unit/src/confine.rs` builds the rules of a unit (read and execute the system directories, read `/proc`, read and write the unit root, a few devices; the default deny list of `pod-confine`). The shim applies them in the payload between the fork and the exec (`shim::under_shim_confined`); a refusal of the kernel fails the spawn. `FakeUnit::confined` gives a fake of `boundary: confined` with the proof `pod-confine: landlock abi N`. Conformance test 27 (`a-confined-unit-reads-no-other-root`) passes on it, and its plant (the record claims `confined`, no rule applies) fails it. With podcrates `v0.25.0`, `apply` drops each capability that the mask does not name, so the test also runs as root (T-073). With `v0.27.0`, `for_unit` also gives the socket directory of the unit a read and write rule: from Landlock ABI 9 a connect to a Unix socket made outside the domain needs it (`a_confined_payload_reaches_the_sockets_of_its_unit`).

Open: a unit kind for real jobs. The podvm driver says `confined` when podvm applies `pod-confine` in its units (PodUtils/podvm#259) and test 27 passes on it.

pmask (the owner's note to DR-6, not adopted) is no boundary and gives no `confined`. As a later launcher option behind `UnitDriver` it can hide the command line and the environment of a job from the other jobs of a shared uid. It needs ptrace: measure `pmask probe` on the host first.

## Prove

`cargo test -p podrunner-unit confined`

## Done

2026-11-12, `660bdf1`; gate run 43155326553, level2 run 43155326441 and runner-image run 43155326439 of `660bdf1` are green. The podvm kind with `confine = true` reports `boundary: confined` only after its self-test (T-079); the confined fake and pod-confine of podcrates `v0.27.0` stay for the fake kind, with the socket directory of the unit (`3ef4db9`). A trusted job runs on the confined podvm unit: the canned job and the container job of `crates/podrunner-broker-fake/tests/podvm.rs` pass under the layer in `runner-image.yml`.

# T-043: Later crates: service unit, release index, acceptance registry (GitHub #43)

**Source:** GitHub #43
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** blocked

## Problem

The text of GitHub #43 ("Later crates: service unit, release index, acceptance registry"), cleaned:

**What and why.** Three podcrates entries serve podrunner after v1. None is on the path of v1.

- `pod-unit` (PodUtils/podcrates@T-102): a service unit for a hosted `podrunner` service. Until then `podrunner print-unit` (#22) writes the unit.
- `pod-release` (PodUtils/podcrates@T-104): signed release indexes (cfb ask A-X-13). Until then #15 publishes betas with SHA-256 sums and a build provenance attestation.
- `pod-todo` registry (PodUtils/podcrates@T-105): the "Proves" column as a registry of acceptance (cfb ask A-X-14). Until then the record of #13 maps each issue to an entry.

**Acceptance.**

- When each entry is released, an issue here takes it, or this issue records why not.

Later than v1: no milestone. Dependencies: PodUtils/podcrates@T-102, PodUtils/podcrates@T-104, PodUtils/podcrates@T-105 (entries exist; no new ask). Estimate (mine): -.

## Premise

Read the text of GitHub #43 on 2026-11-11. Nothing measured.

## Approach

Take each crate when podcrates releases it.

## Prove

`cargo test --workspace`

## Blocker

podcrates builds the crates (PodUtils/podvm#227, section 2.7); none is planned for v1.

# T-046: Local mode: podrunner run workflow.yml (GitHub #48)

**Source:** GitHub #48
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** L
**Status:** partial

## Problem

The text of GitHub #48 ("Local mode: podrunner run workflow.yml"), cleaned:

The owner answered podcrates Q11 yes on 2026-11-11: "yes plan it". A local mode runs a workflow file on the host, as nektos/act does, with no forge.

**Shape** (from the study of the owner's archive `ghactions-local`, PodUtils/podcrates:docs/research/ci/ghactions-local.md):

- A `local` job source compiles the workflow (`pod-actions-workflow`, `pod-actions-expr`) into job offers.
- The official runner takes each job offline through the fake broker of #38, in a podvm unit (M-B, #26).
- `pod-actions-exec` comes later, for targets with no official runner.
- An artifacts and cache service is necessary (a crate or a module: to decide).
- runner.server (MIT, .NET, not static) fits only as a payload in a unit, on loopback only.

**Defects of the archive's conformance suite to fix before use:** w6 needs an `app.js` that is missing; markers can match the echoed script (w4, w5, w7): print computed markers and match output lines only; `docker rm -f $(docker ps -aq)` removes each container of the host; images and actions are pinned by tags that move (pin by digest and by commit); the scripts carry no licence (ask the owner before a copy).

**Acceptance.** `podrunner run workflow.yml` runs a pinned workflow suite to green on a podvm unit, with no network other than the egress module.

## Premise

Read GitHub #48 and the podcrates study note of 2026-11-11. Nothing built, nothing measured.

## Approach

A third source kind, `local`, behind `JobSource` (the source conformance suite applies), that reads one workflow file and offers its jobs at tier `local` (call C14). The unit is the podvm unit of T-026 with the fake broker of T-038 as the Actions service. Fix the suite defects above before a copy.

## Decision

2026-11-12: the work is three entries, in this order. T-087: the `local` source (`crates/podrunner-local`), a workflow file into offers and job results, with the source conformance suite. T-088: a workflow job into the job message of the fake broker; the runner evaluates the expressions of the steps from the context data (`matrix`, `strategy`, `needs`, `vars`, `inputs`, `env`). T-089: `podrunner run workflow.yml`, the source, the fake broker and a unit end to end, with a pinned suite (the defects of the archive's suite fixed). A `uses:` step (an action) needs a copy of the action: T-089 names that gap. This entry closes with T-089. Changed on 2026-11-12: T-089 runs the jobs on a runner directory; T-090 runs each job in a podvm unit; T-091 to T-093 give actions, the checkout and containers; T-095 (the egress module) closes this entry.

## Prove

`cargo test -p podrunner-source local`

## Start condition

Met on 2026-11-12: podcrates `v0.30.0` (T-086) has `pod-actions-workflow` and `pod-actions-expr` (PodUtils/podcrates@T-251, calls C105 to C109 of podcrates). Its PodUtils/podcrates@T-280 (a live run of their vectors against GitHub) is open, so the expansion of a matrix (C107, C108) is not measured against the official runner.

# T-073: Take the podcrates tag after v0.24.0: pod-confine drops each capability

**Source:** PodUtils/podcrates@T-221
**Category:** chore
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

Root keeps `CAP_SYS_ADMIN` and `CAP_PERFMON`, which pass Landlock for an unconfined ancestor, so the confine test of `crates/podrunner-unit/src/confine.rs` has a root guard. The next tag of podcrates changes `pod-confine`: `apply` drops each capability, and `Spec` gets new fields (scope, udp; podrunner uses the builders).

## Premise

Read the answer of podcrates (PodUtils/podcrates@T-221) on 2026-11-11. Not in `v0.24.0`.

## Approach

Move each `pod-*` dependency of `Cargo.toml` to the new tag, build, remove the root guard of the confine test, and run conformance test 27 as root and as a user.

## Prove

`cargo test -p podrunner-unit confined`, as root and as a user.

## Done

2026-11-11, `02a1c0b`: each `pod-*` dependency is at podcrates `v0.25.0` (COORD-051, PodUtils/podcrates#76); the build needs no change of code. The root guard of the confine test is gone: `cargo test -p podrunner-unit confined` passes as root and as uid 65534.

# T-087: The local source: a workflow file into job offers and job results

**Source:** T-046
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

The local mode (T-046) needs a job source of kind `local` that reads one workflow file and offers its jobs. The order of the jobs comes from `needs`; a job with a matrix is one offer for each cell; a job whose `if:` is false ends as skipped, with its reason. The result of each job decides the jobs after it (`needs.<id>.result`, `success()`, `failure()`).

## Premise

Read `pod-actions-workflow` and `pod-actions-expr` of podcrates `v0.30.0` on 2026-11-12: `parse`, `Run::new`, `ready`, `expand` (into `Instance` cells or a `Skip`), `finish` with `Done`, `JobResult::combine`. `Run` borrows its `Workflow`. Nothing built.

## Approach

A crate `podrunner-local` with `LocalSource`: `JobSource` over a `Run`. `next` expands each ready job; each cell is one offer (job id `JOB.N`, the labels of `runs-on`, tier `local` by the source set, call C14). `complete(job_id, result, outputs)` records the end of a cell; when each cell of a job ended, the job finishes with the combined result, and the jobs after it become ready. `mint` gives the credential of a minter that the caller sets (T-089: the files of the fake broker). The source conformance suite runs on a workflow of two jobs.

## Prove

`cargo test -p podrunner-local`: the conformance suite, the order of `needs`, a matrix, a skip with its reason, and the result of a failed need.

## Done

2026-11-12. `crates/podrunner-local`: `LocalSource` passes the 12 tests of the source conformance suite on a workflow of two jobs; podrunner-local: 7 passed. The tests hold the labels of `runs-on`, one offer for each cell of a matrix, a job that waits for each cell of its need, a skip with its condition (`failure()` has no implicit `success() &&`), a failed cell that fails its job and runs the job of `failure()`, and the outputs of a job in `needs` of the next. A plant that finishes a job at its first cell fails two tests. `Run` borrows its workflow, so each source leaks one workflow; podcrates has the ask of a `Run` that owns it.

# T-088: A workflow job into the job message of the fake broker

**Source:** T-046
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

The official runner takes a job message (`AgentJobRequestMessage`) from the fake broker. The message of `crates/podrunner-broker-fake/src/message.rs` carries plain `run:` scripts with no context. A workflow job has `env`, `defaults.run`, step `shell`, `working-directory`, `id`, `if:`, `continue-on-error`, and `${{ }}` in each of them. GitHub sends expressions as tokens, and the runner evaluates them in its contexts (`matrix`, `strategy`, `needs`, `vars`, `inputs`, `steps`, `env`, `runner`, `job`).

## Premise

Read the message builder of the fake broker on 2026-11-12: steps are script actions with a literal script, `contextData` holds only `github`. Not measured: the token forms that runner v2.338.2 takes for a basic expression and for a text with expressions.

## Approach

Build the step inputs and the environment as template tokens (a literal string, or a basic expression; a text with expressions as `format()`), and put `matrix`, `strategy`, `needs`, `vars` and `inputs` into `contextData`. Measure each form with the official runner offline (`tests/runner.rs`). A `uses:` step is refused by name until T-089 gives actions.

## Prove

`PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner-local --test runner -- --ignored`: a job with a matrix, an env with an expression, a step `if:` on `steps.X.outputs`, and `needs.X.outputs` give the values of GitHub.

## Done

2026-11-12. `crates/podrunner-local/src/message.rs` makes the job of the fake broker: each step a script action (its id or `__run`, `__run_2` as its context name, `shell` and `working-directory` with `defaults.run`, `env`, `continue-on-error`, `timeout-minutes`); a text with `${{ }}` is a basic expression, or `format()` with escaped literal text; the env of the workflow and of the job, and `jobOutputs`, as tokens; `matrix`, `strategy`, `needs`, `vars` and `inputs` in `contextData`. The fake broker takes these as `Job::overrides`, keeps the job outputs of `completejob`, and gives a step condition with a status function no implicit `success() &&` (`a_status_function_gets_no_implicit_success` fails on the old rule: `success() && (always())`). The official runner v2.338.2 runs the test workflow offline (1 passed): the matrix, the env of the workflow and of the job, a step output in an `if:` and in an `env`, the job output, `needs.build.outputs.out` and `needs.build.result`; the step of `failure()` does not run. A plant with no `matrix` in the context data fails the test. podrunner-local: 12 passed; level2 run 43200349015 of `903d55d` runs it green. `uses:`, `container:` and `services:` are refused by name (T-089).

# T-089: podrunner run workflow.yml: the local mode end to end

**Source:** T-046
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

The acceptance of T-046: `podrunner run workflow.yml` runs a pinned workflow suite to green on a podvm unit, with no network other than the egress module.

## Premise

T-087 and T-088 give the source and the messages; the fake broker (T-038), the podvm unit (T-026) and the slots exist. Nothing built for the command.

## Approach

A subcommand that builds a source set of kind `local` from the file, starts one fake broker, mints the runner files of the broker for each claimed job, runs each job in a unit of the configured kind, and reads its result from the broker. A `uses:` step needs its action: a local copy (a directory or a pinned archive) first; a fetch through the egress module later. The pinned suite fixes the defects of the archive's suite (T-046).

## Prove

`podrunner run tests/local/suite.yml` passes on the fake unit in the gate, and on a podvm unit in `runner-image.yml`.

## Correction

2026-11-12: the fake unit runs no official runner, so the Prove on "the fake unit in the gate" cannot hold. The command runs on a runner directory (tier `local`) in level 2; the run in a podvm unit and the acceptance of T-046 are T-090; actions are T-091. The suite is `crates/podrunner/tests/local/suite.yml`.

## Done

2026-11-12. `podrunner run FILE --runner-dir DIR [--event NAME] [--var NAME=VALUE]... [--input NAME=VALUE]...` (`crates/podrunner/src/run_local.rs`): `podrunner_local::drive` takes each offer of the local source, runs it with an executor (`exec::RunnerDir`: the official runner in DIR against a fake broker on the loopback, one job at a time), and records its end; a job that cannot become a message (an action, a container) fails with its reason; one line for each job (`ok`, `failure`, `skipped` with its condition); exit 0, 1 when a job failed, or the code of the class (usage, not found, config). The key of the runner is a new RSA key in Rust (`rsa` 0.9.10, key generation only; `deny.toml` ignores RUSTSEC-2023-0071 with the reason), each number at the length that .NET takes (the old helper stripped leading zero bytes: a short `d` about once in 256 keys). `PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner --test local -- --ignored` runs `crates/podrunner/tests/local/suite.yml` to green on runner v2.338.2 here: a matrix with `include` and `exclude` (4 cells), job outputs into `needs`, `defaults.run`, a step `working-directory` and `shell`, `continue-on-error` with `outcome` and `conclusion`, `failure()` and `always()` steps, a skipped job with its reason, and a job after it with `always()`. A plant with no `strategy` context fails the 4 cells. podrunner-local: 15 passed; `run_local`: 4 passed. Level 2 runs the suite.

# T-090: podrunner run: each job of the local mode in a fresh podvm unit

**Source:** T-089
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

The acceptance of T-046: `podrunner run workflow.yml` runs a pinned workflow suite to green on a podvm unit, with no network other than the egress module. T-089 runs the jobs on a runner directory of the host, as the user of podrunner, with no unit and no egress hold.

## Premise

T-089 gives the loop (`podrunner_local::drive`) and the trait `Executor`. `crates/podrunner-broker-fake/tests/podvm.rs` runs a canned job in a podvm unit with the runner image (T-026). Not built: an executor over `UnitDriver`, the reach of the fake broker from the unit, the egress module.

## Approach

`exec::Unit`: an executor over a `UnitDriver` (podvm first): prepare the runner image, start a unit with the runner files of the broker for the job, wait, read the result from the broker, destroy, and refuse a run with residue. The command takes `--podvm PATH --images STORE --image REF` in place of `--runner-dir`. The broker listens where the unit reaches it (the loopback of a shared network now; the egress module with `[unit.podvm] podnet` after).

## Prove

`runner-image.yml` runs `podrunner run crates/podrunner/tests/local/suite.yml --podvm ... --image ...` to green on x86_64, each job in its own unit, with no residue.

## Done

2026-11-12, `17d596d`; runner-image run 43202857513 is green: `podrunner run crates/podrunner/tests/local/suite.yml --podvm ... --image ...` runs each job of the suite in a fresh podvm unit of the runner image on x86_64 (the supervise host of GitHub), 7 `ok` lines, each unit destroyed with no residue. `podrunner_local::exec::InUnit`: an executor over a `UnitDriver` (a fresh unit id for each job, the runner files of the broker as the credential files, `run.sh` as the top process, wait, `destroy`; residue fails the job); `a_unit_with_no_result_fails_its_job_and_is_destroyed` holds the end of a unit with no result on the fake unit. The egress module of the unit (`[unit.podvm] podnet`) is not in the command: the units share the host network as the canned jobs of T-026 do; T-095 holds it.

# T-091: Actions (uses:) in the local mode

**Source:** T-089
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

A step with `uses:` (an action: `actions/checkout`, a JavaScript, composite or Docker action) fails in the local mode with "an action is not supported yet". Most workflows use `actions/checkout` and some JavaScript actions. The archive's suite pins actions by tags that move (T-046).

## Premise

The job message of T-088 has script steps only. The runner downloads an action through the action service of the broker (`actionDownloadInfo`), and takes a cached action from its `_work/_actions` directory. Not measured: the routes of the action service that runner v2.338.2 calls, and the cache layout.

## Approach

Give the runner each action from a local copy: a directory or an archive pinned by commit, named in a file of the run, served by the fake broker as the download of the action (or put into `_actions`). `actions/checkout` of the local tree first; a fetch through the egress module later. Refuse an action that is not pinned by a full commit id.

Read in the sources of runner v2.338.2 on 2026-11-12 (for a job that is not a run service job): `ActionManager.GetDownloadInfoAsync` calls `ResolveActionDownloadInfoAsync` of the task client (POST, location id `27d7f831-88c1-4719-8ca1-6a061dad90eb`, API 6.0-preview.1, route values `scopeIdentifier`, `hubName`, `planId`), which the VSS client finds through the location service of the endpoint (`OPTIONS {base}/_apis`). The answer (`ActionDownloadInfoCollection`) maps each `NAME@REF` to `ResolvedNameWithOwner`, `ResolvedSha` and `TarballUrl`. Before a download the runner reads `$ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE/OWNER_REPO/SHA.tar.gz` (an archive with one top directory, as GitHub's tarballs). So: the fake broker serves the location data and the resolve route from a map of the run (name, ref, commit), and `podrunner run` gives the runner an archive cache directory. An action step of the message is `reference: {type: repository, name, ref}`.

## Prove

The local suite gets a job with `actions/checkout` pinned by commit and a JavaScript action from a local copy; `podrunner run` runs it to green in level 2.

## Correction

2026-11-12, measured with runner v2.338.2: a run service job (as each job of the fake broker) resolves its actions with the launch client, not the task client: the variable `system.github.launch_endpoint` names it, and the runner POSTs `{"actions": [{"action", "version", "path"}]}` to `/actions/build/PLAN/jobs/JOB/runnerresolve/actions`; the answer maps `NAME@VERSION` to `name`, `resolved_name`, `resolved_sha`, `version`, `tar_url`, `zip_url` and `authentication`. With no launch endpoint the runner fails with "Launch client is not initialized". The real `actions/checkout` fetches from github.com, which the local mode has not: the Prove takes a composite action and a JavaScript action from archives; the checkout of the local tree and `./` actions are T-092.

## Done

2026-11-12. `crates/podrunner-broker-fake/src/actions.rs`: the launch route answers each reference with its version as the commit, and serves each tarball from `DIR/OWNER_REPO/SHA.tar.gz` (one name and one file: no other path of the directory); the job message names the broker as the launch endpoint. `podrunner-local`: a `uses:` step is an action step with a repository reference (`OWNER/REPO[/PATH]@SHA`, its `with:` as inputs, a context name `__OWNER_REPO`, then `_2`); a tag, a short ref, `OWNER@SHA` and `docker://` are refused by name. `podrunner run --actions DIR` gives the directory to both executors. `PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner-local --test actions -- --ignored` passes on runner v2.338.2: a composite action with an input and an output, and a JavaScript action on the node 24 of the runner with a default input and an output, read by the next step; a plant with a wrong tarball name fails it. Level 2 runs it. podrunner-local: 17 passed; podrunner-broker-fake lib: 7 passed.

# T-092: The checkout of the local tree and ./ actions in the local mode

**Source:** T-091
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

Most workflows start with `actions/checkout`, which fetches the repository from github.com with the job token. The local mode has no forge and no network, so the real action fails. A `uses: ./path` action reads `action.yml` of the checked-out tree, so it needs the checkout too. A Docker action (`docker://` or a `Dockerfile`) needs the container hooks of the unit.

## Premise

T-091 gives actions from archives pinned by commit; a `./` action is a `self` reference with no tree; `docker://` is refused. Read on 2026-11-12. Not measured: what the runner does with a `self` reference when `github.repository` and the workspace have no checkout.

## Approach

A checkout from the local tree: `podrunner run --checkout DIR` makes an archive of the tree (or a git bundle of its HEAD), and the broker serves it as the tarball of `actions/checkout` at a commit that the run names; the job message gives `github.repository`, `github.sha` and `github.workspace` that agree. Then `./` actions read the checked-out tree. Docker actions follow the container route of the unit (T-035).

## Prove

The local suite gets a job with `uses: actions/checkout@SHA` of the local tree and a `./` action of the suite; `podrunner run` runs it to green in level 2.

## Done

2026-11-12. In the local mode each `uses: actions/checkout@REF` is a stand-in script step (context name `__actions_checkout`): it fetches the tarball of the local tree from the fake broker (`github.api_url` names it; route `_checkout.tar.gz`) and unpacks it into the workspace, or into a plain relative `with: path:` (an expression or `..` is refused); the other inputs of checkout have no meaning for a local tree. `podrunner run --checkout DIR` makes the tarball (`git archive` of HEAD in a git tree, else each file of DIR) and gives it to both executors. A `./` action reads its `action.yml` from the checked-out tree. `the_checkout_stand_in_gives_the_local_tree_and_its_actions` passes on runner v2.338.2 (the tree at the root and in `sub`, a `./` composite action with an output); a plant with no tree fails it. The local suite has a job `checkout` (the tree of this repository, the action `crates/podrunner/tests/local/act` with an input and an output): `podrunner run --checkout` runs the suite to green (9 lines). Docker actions: T-093.

# T-093: Docker actions in the local mode

**Source:** T-092
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

A Docker action (`uses: docker://IMAGE`, or an action whose `runs.using` is `docker`) fails in the local mode: `docker://` is refused by name, and an action with a `Dockerfile` needs a build, which the container route refuses (`build-not-supported`, call C2). A Docker action with an image runs through the container hooks of the unit on GitHub's runners.

## Premise

Read on 2026-11-12: the podvm unit runs `container:` jobs through the hooks (T-035, `runner-image.yml`); the local mode refuses `container:`, `services:` and `docker://` by name. Not measured: a Docker action through the hooks of a podvm unit.

## Approach

In the `--podvm` executor, give each unit the hook relay and the control socket (as `crates/podrunner-broker-fake/tests/podvm.rs` does for a `container:` job), and send `container:`, `services:` and `docker://IMAGE@sha256:HEX` steps in the job message. Refuse a tag and a `Dockerfile` by name. The `--runner-dir` executor keeps the refusal: it has no unit.

## Prove

The local suite gets a job with `container:` and a `docker://` step pinned by digest; `podrunner run --podvm` runs it to green in `runner-image.yml`.

## Done

2026-11-12, `146e5be`; runner-image run 43227187267 is green for the containers suite. The message: `container:` as `jobContainer` and `services:` as `jobServiceContainers` (template tokens of their nodes), and `uses: docker://IMAGE` as a step with a `containerRegistry` reference (its `with:` as inputs, context name `__docker`); each image must be pinned by digest (a tag, a short digest, an expression and no image are refused by name). `InUnit` gives a unit that needs containers its container hooks: the relay and the control socket in its socket directory, a hook server with the policy of the unit (the host network, the registries of the images of the job) until the end of the job, and the refusals of the hook calls in the log of the job. `RunnerDir` refuses such a job ("run it in a unit"). `services:` stays refused by the hook policy on a unit of the host network (`container-not-supported`): a service needs a network of its own. `crates/podrunner/tests/local/containers.yml` (a `container:` job and a Docker step) builds its messages in the gate (`the_containers_suite_gives_its_messages`); `runner-image.yml` runs it with `--podvm`. In runner-image run 43205758271 of `9e6e76d` the local suite failed in the podvm units at the checkout stand-in only: GNU `tar -xz -C "$GITHUB_WORKSPACE"` gave "Cannot open: No such file or directory" after `mkdir -p` of the same directory; the stand-in now changes into the directory and runs `gzip -dc | tar -xf -`. Level2 run 43205758221 hung: the two tests of `tests/actions.rs` shared one runner directory at once; a lock makes them take turns (both pass, 4.7 s). In runner-image run 43223524199 of `98bc496` the checkout stand-in and the `container:` job pass in podvm units; the Docker step of a job with no `container:` is refused by the order rule of the hook server (T-098, Q4), so the suite gives that job a `container:`. In runner-image run 43225522050 of `ba2ff2f` the Docker step starts its container through the hooks and `/bin/sh` runs; its quoted `args` broke, because runner v2.338.2 splits the arguments of a container step at each space on the hooks path (`ContainerHookManager.cs:101`, `entryPointArgs.Split(' ')`), as for the official hooks; the suite uses words with no space (`/usr/bin/test 9 -eq 9`). Proven in runner-image run 43227187267 of `146e5be`: in podvm units the containers suite passes (`ok in-container`, `ok docker-step`), and the checkout stand-in passes in the local suite.

# T-094: The local source owns its workflow: drop the leak (podcrates v0.31.0)

**Source:** T-087
**Category:** chore
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`LocalSource` leaks one `Workflow` for each source (`Box::leak`), because `Run<'w>` of `pod-actions-workflow` `v0.30.0` borrows its workflow. podcrates made the ask (PodUtils/podcrates@T-309, podcrates `bcdf488`): `Run<W: Borrow<Workflow>>` owns a `Workflow` or an `Arc<Workflow>`; `Run::workflow()` gives it back. Breaking: `ready()` gives `Vec<String>`.

## Premise

Read the message of the podcrates session on 2026-11-12: it comes with the tag `v0.31.0`, not yet released.

## Approach

Take `v0.31.0` (move each tag, as T-086). `LocalSource` keeps a `Run<Arc<Workflow>>` and reads the workflow through it; pass `&id` to `expand` and `finish`.

## Prove

`cargo test -p podrunner-local` passes with no `Box::leak` in `crates/podrunner-local/src/lib.rs`.

## Done

2026-11-12. podcrates `v0.31.0` (tag at `45b145a`, PodUtils/podcrates#90, COORD-058): each `pod-*` dependency moves to it; `Cargo.lock` moves 13 `pod-*` crates and no other dependency. `LocalSource` keeps a `Run<Arc<Workflow>>` and reads the workflow through `Run::workflow()`; `ready()` gives owned ids. No `Box::leak` is left in `crates/podrunner-local/src/lib.rs`; podrunner-local: 21 passed.

# T-095: podrunner run --podvm: the egress of each unit through the egress module

**Source:** T-090
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** partial

## Problem

The acceptance of T-046: the local suite runs on a podvm unit "with no network other than the egress module". `podrunner run --podvm` (T-090) starts each unit on the host network, with no hold: a step of the workflow reaches each host. `serve` holds a podvm unit under `podnet pod` with `[unit.podvm] podnet` (T-065, T-041).

## Premise

Read `crates/podrunner/src/podvm_kind.rs` and `crates/podrunner-local/src/exec.rs` on 2026-11-12. The units of the local mode reach the fake broker on the loopback of the host; a hold must allow it.

## Approach

`podrunner run --podvm ... --podnet PATH [--allow HOST]...`: start each unit under `podnet pod` with a policy that allows the loopback port of the fake broker and each `--allow` host, as `serve` does for `[unit.podvm] podnet`. A step that reaches another host fails. The run report says `enforced` or `advisory`.

## Prove

`runner-image.yml` runs the local suite with `--podnet`, and a job of the suite that reaches a host that is not allowed fails (a check that the hold holds).

## Done

Partial, 2026-11-12. `podrunner run --podvm ... --podnet PATH [--allow HOST]...` holds each unit under `podnet pod` (`PodvmUnit::with_podnet`) and serves its egress socket from the egress module with the policy of `--allow` (`broker-policy.v1`, mode `allow`); the socket directory of the unit holds the egress socket and the hooks, and goes at the end of the job. The command takes only the seccomp hold (`advisory`) now: the fake broker listens on the loopback of the host, which the netns hold does not reach; it refuses the netns hold by name. `crates/podrunner/tests/local/egress.yml` checks that a step does not reach `example.com`; `runner-image.yml` runs it with the hold (it must pass) and with no hold (it must fail: the plant). In runner-image run 43227187267 of `146e5be` the held run hung to its time limit with no line: the runner had no `DOTNET_SYSTEM_NET_DISABLEIPV6=1`, the workaround of GitHub #64 (T-077) that the held test sets; `InUnit` then set it for a held unit. Run 43230901394 of `1ff9223` (podnet `v0.4.0-beta.3`, the variable set) passed: `ok denied` with the hold, `failure denied` with no hold. Run 43232215654 of `1f1d349` (podnet `v0.4.0-beta.6`, no variable, T-077) hung again to the limit of 600 s with no line. The executor of a unit now ends the unit at the `timeout-minutes` of its job and gives its log; `egress.yml` sets 4 minutes, so the next hang shows the log of the runner. Run 43236725634 of `ed03e05`: the job ended at 240 s, and its broker log was empty: the runner did not reach the fake broker. A job with no result now also gives the end of the output of its unit (`UnitDriver::unit_logs`, `podvm logs` of the payload), so the next run shows where the runner stops. Open: the cause of the hang with beta.6; the netns hold (the broker behind the egress).

# T-097: Build the format() token from Template::parts() (PodUtils/podcrates@T-322)

**Source:** T-088
**Category:** chore
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`crates/podrunner-local/src/message.rs` keeps its own copy of the scan of `${{ }}` (`split`), because the literal parts of a `pod_actions_expr::Template` are private, and checks it against `Template::parse`. podcrates made the ask (PodUtils/podcrates@T-322): `Template::parts()` and `TemplatePart::{Text, Expr}`.

## Premise

Read the message of the podcrates session on 2026-11-12: on podcrates main after `v0.31.0`; it comes with the next tag.

## Approach

Take the next tag (move each tag, as T-094), build the `format()` token from `Template::parts()`, and remove `split` and its check.

## Prove

`cargo test -p podrunner-local` passes with no `fn split` in `crates/podrunner-local/src/message.rs`.

## Done

2026-11-12, this commit. Each crate of podcrates moves to `v0.32.0` (`032e86f`). `token()` builds the `format()` token from `Template::parts()`; `split` and its check are removed. `sh scripts/agent/check.sh` passes.


# T-098: Q4: a Docker action in a job with no container (the order rule of the hook server)

**Source:** T-093
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** blocked

## Problem

On GitHub, a Docker action (`uses: docker://IMAGE`, or an action with `runs.using: docker`) runs in a job with no `container:`. With the container hooks, the runner then calls `run_container_step` with no `prepare_job` before it. The hook server of podrunner refuses each container operation before the first `prepare_job` (`docs/design/contracts/unit-driver.md`, "Order": "so the server refuses every container operation of such a job. That includes the container actions of the job"). In runner-image run 43223524199 the Docker step of a job with no container failed: "run_container_step refused: a container call before prepare_job".

## Premise

Read the contract and `crates/podrunner-hook/src/policy.rs` on 2026-11-12. The same contract says that the mount table, the port rule and the refusal of devices, privileged flags and host paths "are the real controls", and that the rule on images is not a boundary; the order rule adds no control over a `run_container_step`, whose mounts come from the container action table.

## Approach

If the owner allows it: the gate accepts `run_container_step` before `prepare_job` (its mounts from the action table, its image from the registries of the unit); `run_script_step` and `cleanup_job` stay refused before it. Change the test 16 of `crates/podrunner-hook/src/tests.rs`, and add a Correction to the contract in `docs/decisions.md` (the design copy keeps its words, call C17).

## Prove

`cargo test -p podrunner-hook` with the changed test 16; the local containers suite with a Docker step in a job with no `container:` runs green in `runner-image.yml`.

## Blocker

The owner: Q4 of `TODO/PROGRESS.md`. The contract of the design makes the rule; a change of it is the owner's.
