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
**Status:** done

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

## Done

2026-11-12, `5518055`. `podrunner run workflow.yml` runs the pinned local suite (`crates/podrunner/tests/local/suite.yml`: a matrix, `needs`, outputs, `if:`, a skip, the checkout stand-in and a local action) to green, each job in a fresh podvm unit of the runner image under the egress hold of `--podnet` (runner-image run 43271963245 of `5518055`, T-095). The containers suite (a `container:` job and a Docker step) is green in podvm units (T-093). The work: T-087 to T-095, T-097. Open questions of the owner: Q4 (T-098).

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
**Status:** done

## Problem

The acceptance of T-046: the local suite runs on a podvm unit "with no network other than the egress module". `podrunner run --podvm` (T-090) starts each unit on the host network, with no hold: a step of the workflow reaches each host. `serve` holds a podvm unit under `podnet pod` with `[unit.podvm] podnet` (T-065, T-041).

## Premise

Read `crates/podrunner/src/podvm_kind.rs` and `crates/podrunner-local/src/exec.rs` on 2026-11-12. The units of the local mode reach the fake broker on the loopback of the host; a hold must allow it.

## Approach

`podrunner run --podvm ... --podnet PATH [--allow HOST]...`: start each unit under `podnet pod` with a policy that allows the loopback port of the fake broker and each `--allow` host, as `serve` does for `[unit.podvm] podnet`. A step that reaches another host fails. The run report says `enforced` or `advisory`.

## Prove

`runner-image.yml` runs the local suite with `--podnet`, and a job of the suite that reaches a host that is not allowed fails (a check that the hold holds).

## Done

2026-11-12, `5518055`. `podrunner run --podvm ... --podnet PATH [--allow HOST]...` holds each unit under `podnet pod` (seccomp hold, `advisory`) and serves its egress socket from the egress module with the policy of `--allow`. In runner-image run 43271963245 of `5518055`: the local suite runs held (8 `ok` lines, the checkout and the local action from the fake broker on the loopback); `egress.yml` gives `ok denied` held and `failure denied` with no hold (the plant). A job ends at its `timeout-minutes`, and a job with no result gives the output of its unit. Held units set `DOTNET_SYSTEM_NET_DISABLEIPV6=1` until podnet `v0.4.0-beta.7` (T-077). The netns hold (the broker behind the egress) is not in this entry: the command refuses it by name.

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
**Status:** done

## Problem

On GitHub, a Docker action (`uses: docker://IMAGE`, or an action with `runs.using: docker`) runs in a job with no `container:`. With the container hooks, the runner then calls `run_container_step` with no `prepare_job` before it. The hook server of podrunner refuses each container operation before the first `prepare_job` (`docs/design/contracts/unit-driver.md`, "Order": "so the server refuses every container operation of such a job. That includes the container actions of the job"). In runner-image run 43223524199 the Docker step of a job with no container failed: "run_container_step refused: a container call before prepare_job".

## Premise

Read the contract and `crates/podrunner-hook/src/policy.rs` on 2026-11-12. The same contract says that the mount table, the port rule and the refusal of devices, privileged flags and host paths "are the real controls", and that the rule on images is not a boundary; the order rule adds no control over a `run_container_step`, whose mounts come from the container action table.

## Approach

If the owner allows it: the gate accepts `run_container_step` before `prepare_job` (its mounts from the action table, its image from the registries of the unit); `run_script_step` and `cleanup_job` stay refused before it. Change the test 16 of `crates/podrunner-hook/src/tests.rs`, and add a Correction to the contract in `docs/decisions.md` (the design copy keeps its words, call C17).

## Decision

2026-11-13, the owner: "the common sense rule is to be github actions compat, so yes allow it" (`docs/decisions.md`, Q4). Accept `run_container_step` before `prepare_job`, with its mounts from the container action table and its image from the registries of the unit; keep the refusal of `run_script_step` and `cleanup_job` before it.

## Prove

`cargo test -p podrunner-hook` with the changed test 16; the local containers suite with a Docker step in a job with no `container:` runs green in `runner-image.yml`.

## Correction

The text below was the Blocker before the ruling: The owner: Q4 of `TODO/PROGRESS.md`. The contract of the design makes the rule; a change of it is the owner's.

## Done

2026-11-13, `a3999f1`. `Gate::check` (`crates/podrunner-hook/src/policy.rs`) takes `run_container_step` before `prepare_job`, with the checks of the action mounts; `cleanup_job` and `run_script_step` before it stay refused. The test `the_mount_tables_and_the_order_of_the_design` holds both. The job `docker-step` of `crates/podrunner/tests/local/containers.yml` has no `container:` now. `docs/contracts/hook.md`, `docs/contracts/codes.md` and `docs/cli.md` say so. In runner-image run 43355069666 of `a3999f1`, the job `docker-step` (a Docker step, no `container:`) gives `ok` with the official runner through the hooks.

# T-101: G-01: a source kind for an external program that speaks the source wire

**Source:** GitHub #66 (G-01)
**Category:** feature
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** partial

## Problem

githome (GitHub #66, G-01) ships its own source program, `githome source`, that speaks the source wire of `docs/contracts/source-driver.md` on its stdio. podrunner has the kinds `fake`, `github-scaleset` and `remote` only, so no forge but GitHub can give jobs. The non-goal "the `forge-contract` source" had the trigger "githome has code"; githome has code (its T-075).

## Premise

Read GitHub #66 (comments of 2026-11-12 and 2026-11-13) on 2026-11-13. `WireSource` (`crates/podrunner-source`) already speaks the wire to the scale-set sidecar.

## Approach

A `[[source]]` kind `program`: the absolute path of the program in `driver_dir`, its argv, its credential file (passed by path, mode 0600, never in argv), its slots and its labels. `serve` spawns it and talks the wire over its stdio with `WireSource`, as for the sidecar. `podrunner status --json` groups the jobs by source. Remove the row of the non-goal.

## Prove

The 12 source conformance tests pass against a program of the test that speaks the wire; with a `github-scaleset` fake and a `program` source of 2 slots each, 10 jobs of each run and no source holds more than its slots.

## Done

Partial, 2026-11-13, this commit. The `[[source]]` kind `program` (`program`, `program_args`, an optional `key_file` whose path the program gets in `PODRUNNER_SOURCE_KEY_FILE`); `serve` runs it through `SidecarSource::program`, the restart and reply deadlines of the sidecar with no `sign` channel. `podrunner fake-source --script FILE` serves the fake source on stdio. `crates/podrunner/tests/program_source.rs`: the 12 source conformance tests pass against `podrunner fake-source` as a program source. `a_program_source_needs_its_program` holds the keys. `docs/configuration.md` ("A program source") and `docs/cli.md` say so. `status --json` lists each source with its jobs (`schemas/status.v1.json`), and slots holds the slots of each source whatever its kind (`g01_no_source_holds_more_than_its_slots`, two sources of 2 slots, 10 jobs each). Open: the same with a `program` source in `serve`. A `program` source is a forge source, so its jobs need the trusted or untrusted tier (trust facts, a signed pin list, a kind at the floor), which no fake kind of `serve` meets; the pair runs on the host of LIVE-2 (GitHub #75) when githome gives its program (its plan on #75).

# T-102: G-13: a table of the events of a job in the tier rule

**Source:** GitHub #66 (G-13)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The tier rule (`crates/podrunner-tier`) refuses each event of the class `other`. Most events that githome fires fall in it, so githome's live test uses only `push`, `pull_request` and `workflow_dispatch` (GitHub #66, G-13).

## Premise

Read GitHub #66 (the comment of 2026-11-13) on 2026-11-13.

## Approach

The offer carries a typed event (`Event` of `pod-job`): githome maps each of its events to it, and sends `other` for the rest, so podrunner needs no table of names. What G-13 needs is that more events may run as a trusted job. The tier rule takes `push`, `pull_request`, `workflow_dispatch`, `schedule` and `merge_group` (the design); `workflow_run`, `issue_comment`, `release` and `repository_dispatch` are refused. A change of that set changes which code runs with trust: Q5 asks the owner.

## Decision

2026-11-13, the owner, Q5: yes, with limits (`docs/decisions.md`).

## Prove

`cargo test -p podrunner-tier` with each event of the table, and an unknown event refused.

## Done

2026-11-13, this commit. `[trust] extra_events` (`TierConfig::extra_events`) adds events to the trusted set; the configuration refuses each event but the four of `EXTRA_EVENTS` (`workflow_run`, `issue_comment`, `release`, `repository_dispatch`). A listed event needs the facts of each trusted job. `extra_events_are_trusted_only_when_listed_and_only_those_four` holds it; the case table and the exhaustive test of `unknown` pass unchanged. `docs/configuration.md` names the key.

# T-103: G-09: a container engine inside the unit, and the census of the verbs (DR-8)

**Source:** GitHub #77, GitHub #66 (G-09)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** L
**Status:** partial

## Problem

DR-8 (call C2) keeps the official container hooks and no Docker API socket, and brings an engine back when "an ask names a client that the hooks do not reach". The owner ruled on 2026-11-13 that githome's G-09 keeps three such clients: `services:` containers, `docker build` (and Dockerfile actions) and Docker-in-Docker. Each works on a GitHub-hosted runner. G-09 refuses a socket of the host.

## Premise

Read GitHub #77 and #66 (the comments of githome of 2026-11-13) on 2026-11-13. The hooks run `container:` jobs and Docker actions (T-093, T-098); `services:` runs only on a unit with its own network; `docker build` is refused (`build-not-supported`).

## Approach

1. The census of the verbs (DR-17): the Docker CLI calls of the three clients (`docker build`, `docker run` and `docker network` of `services:`, the daemon of Docker-in-Docker), from the source of the official runner and of the actions that the asks name.
2. A design note: an engine inside the unit (a rootless Docker or Podman inside a podvm guest of the machine tier, or inside a system container of incus), with its socket in the unit only. Record the choice in `docs/decisions.md`.
3. Then the crate ask to podcrates (`pod-dockerapi`, if the census needs it) by a COORD issue.

State (2026-11-13): step 1 is done: the census of the verbs and the design (`docs/container-engine.md`): the runner runs 15 verbs of the `docker` command line, and Docker-in-Docker needs the whole Engine API, so a real engine runs inside a guest with its socket there, and no shim (`pod-dockerapi`) is needed. Step 2 is done: podvm `v0.2.0-beta.10` gives a guest with no NIC and one host channel (GitHub #83 is closed; the microvm kind uses it, T-037). Next: step 3, the runner image with an engine and the value `engine` of `ContainerRoute` (a COORD issue to podcrates). Step 3a (2026-11-13): GitHub #91 (COORD-031) asks podcrates for `ContainerRoute::Engine` and a need of an engine in `UnitNeed`. `scripts/measure-engine.sh` in `runner-image.yml` measures the static Docker binaries in a guest of the machine tier: `dockerd` with no iptables and no bridge, the storage driver and the cgroups it finds, and a container from a saved image with no network. Runner-image run 43484759471 (`60b52a2`, Docker 27.5.1): containerd boots, and `dockerd` stops with "Devices cgroup isn't mounted": the guest (kernel `6.12.110-0-virt`) mounts no cgroup (`/sys/fs/cgroup` is sysfs) and has no `overlay` in `/proc/filesystems`. The measure now mounts cgroup2 and tries `modprobe overlay` before `dockerd`. Runner-image run 43490092612 (`c08f762`): with cgroup2 mounted by the script (controllers cpuset, cpu, io, memory, hugetlb, pids), `dockerd` starts in 1 s with the storage driver `vfs` and loads an image. The guest has no `modprobe`, no `/lib/modules` and no `overlay`, so only `vfs`. A container fails with "pivot_root .: invalid argument": the root of the guest is an initramfs. The measure now starts `dockerd` with `DOCKER_RAMDISK=true` (runc moves the root and chroots). Runner-image run 43495436619 (`3037c1c`): a container runs in the guest (`container-42`). The measure now makes a user-defined bridge network, as `services:` does, and one container reaches another by its name. Runner-image run 43500428451 (`9dbd3fc`): `docker network create` fails with "operation not supported": the guest kernel has no bridge. GitHub #92 (COORD-032) asks podvm for bridge, veth, netfilter, overlay and a cgroup2 mount in the guest. podvm answered in PodUtils/podvm#321 (COORD-085, PodUtils/podvm@T-1780): `--podvm-engine` loads 33 modules of the pinned kernel (bridge, veth, overlay, nf_tables and the `xt_*` of Docker) before the command; each guest mounts cgroup2 and brings `lo` up; podvm brings no `iptables` binary. It ships in `v0.2.0-beta.14`. podvm `v0.2.0-beta.14` is pinned; the runner image has `iptables` (nft); the measure runs `dockerd` with its default bridge and rules under `--podvm-engine --podvm-net none`. Runner-image run 43514371042 (`261088e`): cgroup2 and `overlay` are there, `dockerd` starts in 1 s with `overlay2`, a container runs, and on a user-defined network one container reaches another by its name. GitHub #92 is closed. Step 3b, the code: `[unit.microvm] engine = DIR` (the static Docker binaries): the kind passes `--podvm-engine`, mounts DIR read-only at `/podrunner-engine` (in the mount table of T-036), and the guest script starts `dockerd` as root with `DOCKER_RAMDISK` and gives the runner `DOCKER_HOST` of a socket in the guest. `tests/engine.rs` runs a container as `runner` on a network in a real guest in `runner-image.yml`. Runner-image run 43524984859 (`a5867d9`): it passes: the payload runs as `runner` with `DOCKER_HOST` of the socket in the guest, `docker info` gives `overlay2`, and a container on a user network prints `engine-42`; the guest leaves no residue. podcrates `v0.42.0` (PodUtils/podcrates#145) gives `ContainerRoute::Engine` and `UnitNeed::container_engine`; podrunner takes it. The microvm kind with an engine reports `container: engine`; placement sends a job with `container_engine` only to such a kind, and a `container:` job to the hooks or to an engine; `schemas/status.v1.json` has the value. Next: the parity suite in guests with an engine in `runner-image.yml` (`04-services.yml`, `05-containers.yml`, `09-dockerfile.yml`, with T-123 and T-124).

## Prove

The census and the design are in `docs/`; a `docker build` job and a `services:` job pass in a unit in `runner-image.yml`.

# T-104: G-10: the source conformance tests as a command for another source program

**Source:** GitHub #66 (G-10)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

githome wants to run the conformance tests of the source wire in its own CI against `githome source` (GitHub #66, G-10; PodUtils/podcrates#83). The tests exist only as a test of this repository.

## Premise

Read GitHub #66 (the comment of 2026-11-13) on 2026-11-13. The 12 tests are in `crates/podrunner-source` (`conformance`).

## Approach

`podrunner source-conformance -- PROGRAM ARGS...`: spawn the program, run the 12 tests over its stdio, print one line for each test, and exit 0 only when each passes. It ships in the release binary.

## Prove

`cargo test -p podrunner` with the fake source program passing and a planted defect failing.

## Done

2026-11-13, this commit. `podrunner source-conformance -- PROGRAM ARGS...` runs the 12 source tests against a program over its stdio (`SidecarSource::program`, and a fresh process for each line of the wire tests), each test in a limit of 60 s so that a program that never answers fails by name; exit 0 when each passes, else 1. `source_conformance_passes_a_source_and_fails_an_echo` (`crates/podrunner/tests/program_source.rs`): `podrunner fake-source` passes the 12; `/bin/false` fails. It ships in the release binary. `docs/cli.md` says so.

# T-105: G-02: an `incus` unit kind for trusted jobs (a system container)

**Source:** GitHub #66 (G-02)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

githome's G-02 asks that a trusted job run in a system container with its own init, file system, network and engine, such as a system container of incus, and that the host never falls back to a weaker kind. The non-goal "The `incus` and `host` kinds" had the trigger "an ask names Incus"; G-02 names it.

## Premise

Read GitHub #66 (G-02) on 2026-11-13. podvm `v0.2.0-beta.6` has an incus tier (`--podvm-tier=incus`, with `run -d`, `exec` and volumes).

## Approach

An `incus` kind over podvm's incus tier, behind `UnitDriver`, as `microvm` is over its machine tier (T-037): the same seam, the conformance suite, and a measure on a host with incus. It can also hold the engine of T-103.

Step 1 (2026-11-13): `scripts/measure-incus.sh` in `runner-image.yml`, a measure: incus from apt on a hosted runner, then `podvm run --podvm-tier=incus` of the runner image (the files, a volume, a program as `runner`, the network and the kernel) and an instance that stays (`run -d`, `exec`, `rm -f`).

Step 1 result (2026-11-13, the run of `e8257ad`, x86_64, podvm `v0.2.0-beta.8`): each property passes. The run takes 18 s; the files of the image are there and `runner:runner` owns `/home/runner` (no `chown` as in the machine tier); a volume takes a write back to the host; `run.sh` runs as `runner`; `run -d`, `exec` and `rm -f` work. The instance has `eth0` and `lo`. Its boot id differs from the host's though it shares the kernel of the host: the measure now compares `/proc/version`, and so does the guest self-test (T-036).

The kind needs a name that it chooses, so that `destroy` removes the instance when the `podvm` client is gone (the processes of the instance belong to incusd and carry no tag), and the credential files through stdin (`/.podvm/stdin`, mode 0600, owner 0): the root of the instance is a mapped id and cannot read a host directory of mode 0700. `run -d --name` has no stdin; the foreground `run` has stdin but a random name and no label. GitHub #85 asked podvm for both.

podvm answered in PodUtils/podvm#307 (COORD-079, 2026-11-13): `run -d -i --name NAME --podvm-tier=incus` pushes the piped stdin to `/.podvm/stdin` (owner 0, mode 0600); `guest-hold --run` gives it to the command and removes it (PodUtils/podvm@T-1777). It ships in podvm `v0.2.0-beta.10`. The kind can be written against it now; the proof waits for that pin.

Step 2, the code (2026-11-13, against podvm `main` at `055dd46c`): the crate `podrunner-incus` (`IncusUnit`). `start` spawns `podvm run -d -i --name NAME --podvm-tier=incus -v WORK:/podrunner-work IMAGE /bin/sh -c SCRIPT` and writes the credential files on its stdin, one line `NAME BASE64` each; the script writes them into `/podrunner/.runner` at mode 0600 and runs the payload as `runner`. The name is `podrunner-HASH-UNIT` (FNV-1a of the state directory), so a replay of the journal finds the instance. `wait` reads `podvm inspect --format '{{.Status}} {{.ExitCode}}'`; `signal` is `stop` or `rm -f`; `destroy` is `rm -f` and counts the instance if it stays. `probe` claims `confined`, `net: nat`, no container route, no egress hold. `[unit.incus]` in `serve` and `check` (`crates/podrunner/src/incus_kind.rs`). Eight tests over a fake podvm hold the stdin, the argv, the end, the kill, the replay and the refusals. `tests/conformance.rs` runs 14 unit tests on real instances; it goes into `runner-image.yml` with the pin of beta.10.

Step 3 (2026-11-13): podvm `v0.2.0-beta.10` (PodUtils/podvm#307) is pinned; `runner-image.yml` runs `tests/conformance.rs` on real instances after the measure. Runner-image run 43434618569 (`66654aa`): 12 of 14 pass on real instances. Test 6 reads the credential files on the host, and an instance has them only inside, so it does not apply. Test 27 failed on the fixture: its `ReadOthers` payload only slept; it now does what the podvm suite does.

## Prove

`cargo test -p podrunner-incus --test conformance -- --ignored` passes on a host with incus.

## Done

2026-11-13, runner-image run 43477657803 (`6365312`, podvm `v0.2.0-beta.12`): `cargo test -p podrunner-incus --test conformance -- --ignored` passes on real instances of incus 6.0 on a hosted runner, with no NIC (`--podvm-net none`). GitHub #85 is closed. A `trusted` job on the kind needs its egress hold: T-109.

# T-106: The actor classes of githome (agent, bot, external) in the offer schema

**Source:** GitHub #66 (the comment of githome of 2026-11-13, G-02 and G-13)
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

githome states the class of the actor of each job as `agent`, `bot`, `member` or `external` in `trust_inputs.actor_class`. The offer schema (`ActorClass` of `pod-job`, podcrates `v0.37.0`) has `member`, `outside` and `unknown` only, so an offer with `agent`, `bot` or `external` does not parse, and podrunner refuses it. The tier rule (`crates/podrunner-tier`) makes a job trusted only for `member`; it must keep each other class below trusted, and an `agent` job must need `guest` (githome sets `unit_need.min_boundary = guest` for it).

## Premise

Read GitHub #66 (githome, 2026-11-13) and the file `offer.rs` of `pod-job` of podcrates `v0.37.0` on 2026-11-13.

## Approach

Ask podcrates (COORD issue, GitHub #80) for `ActorClass::Agent` and `ActorClass::Bot` in `pod-job`, and tell githome to send `outside` for `external` (the name of the schema). Then take the tag: `agent` and `bot` are never trusted, and `agent` needs `guest` in the tier rule, whatever the offer says.

## Prove

`cargo test -p podrunner-tier` with an offer of each class: only `member` can be trusted, and `agent` has the floor `guest`.

## Done

2026-11-13, this commit. podcrates `v0.38.0` (PodUtils/podcrates#136, PodUtils/podcrates@T-505) gives `ActorClass::Agent` and `ActorClass::Bot`; podrunner takes the tag. The tier rule makes a job of an `agent` of an allowed event in a configured owner `untrusted` (a guest), and never trusts a `bot`. `an_agent_needs_a_guest_and_a_bot_is_never_trusted` holds it; the case table passes unchanged. GitHub #80 is closed.

# T-107: The egress policy of an offer: placement, the ceiling of the operator, the socket directory

**Source:** GitHub #66 (the comment of githome of 2026-11-13, G-04)
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

githome sends a `policy` (`broker-policy.v1`) in each offer, `open` by default, and expects a host that cannot apply it not to claim the job. Three defects stop this:

- Placement narrowed an offer with a policy to kinds with `net: broker`. No real kind says so (`podvm`: `shared-host`, `microvm`: `none`), so `serve` refused each offer of githome with `egress-policy-unsupported`.
- `policy_of` took the policy of the offer in place of `[egress] default_policy`. SPEC §9 asks that an offer can only narrow the policy of the operator.
- slots gave a unit its socket directory only when the driver mounts it (`socket_mount`). The microvm kind mounts nothing, so under `serve` its guest got no egress channel and had no way out; a held podvm kind with no hook route was refused at the start.

## Premise

Read the comment of githome on GitHub #66 (2026-11-13), `crates/podrunner-slots/src/placement.rs`, `run.rs`, and the policy module of `pod-broker` of podcrates `v0.39.0`.

## Approach

- Placement: a policy needs a kind whose effective egress hold is not `none`, the same way out that `default_policy` already takes; a policy with a `path` rule is `egress-policy-unsupported` (no TLS end, SPEC §9.2).
- `crates/podrunner-slots/src/policy.rs`: `narrow(ceiling, offer)`. The denies of both hold. An `open` side gives the rules of the other side. With two `allow` sides, an offer rule stays only where a ceiling rule covers its host (the same host, or `*.NAME` over a name below NAME) and its ports.
- slots gives each driver its socket directory; a driver that mounts it still does.

Tell githome on GitHub #66 that `pod-broker` names the version `schema`, not `version`.

## Prove

`cargo test -p podrunner-slots policy egress`

## Done

2026-11-13, this commit. `the_ceiling_of_the_operator_narrows_the_policy_of_an_offer` and `a_policy_needs_a_hold_and_no_path_rule` hold the slots side; five tests in `policy.rs` hold `narrow`. The comment on GitHub #66 tells githome about `schema`.

# T-108: G-06: the CPU time and the peak memory of a job in its report

**Source:** GitHub #66 (the comment of githome of 2026-11-13, G-06)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

githome takes `cpu_s` and `peak_rss_mib` from `run-report.v1` for the record of a job and for its quotas. `RunReport` of `pod-job` (podcrates `v0.39.0`) has neither field, and slots measures neither.

## Premise

Read the comment of githome on GitHub #66 (2026-11-13) and the report module of `pod-job` of podcrates `v0.39.0`.

## Approach

Ask podcrates for `cpu_s` and `peak_rss_mib` in `RunReport` and `PublicReport`, each a number or `unknown` (GitHub #86, COORD-026). Then slots samples each unit at each tick: the sum of `utime` and `stime` of each process with the tag of the unit (`/proc/PID/stat`), and the highest sum of their RSS. A kind whose processes are not on the host (`incus`) reports `unknown`.

## Prove

`cargo test -p podrunner-slots report`: a unit that spins reports a `cpu_s` above 0 and a `peak_rss_mib`; the `incus` kind reports `unknown`.

## Done

2026-11-13, this commit. podcrates `v0.40.0` (PodUtils/podcrates#140, PodUtils/podcrates@T-527) gives `cpu_s` and `peak_rss_mib` (`Option<Residual>`); podrunner takes the tag. `crates/podrunner-slots/src/usage.rs` samples the processes of each running unit at each tick (`/proc/PID/stat` and `/proc/PID/status`): the CPU time is the sum of the highest `utime + stime` of each process, the peak is the highest sum of `VmRSS` of one tick. A report of a job whose unit ran carries both; a unit with no process on the host (`incus`) gives `unknown`; a job with no unit gives neither. `the_report_of_a_unit_that_spins_has_its_cpu_time_and_memory` and two tests of `usage.rs` hold it. GitHub #86 is closed.

# T-109: The incus kind has no egress hold, so placement gives it no trusted job

**Source:** T-105 (a read of `crates/podrunner-incus/src/lib.rs` and `crates/podrunner-slots/src/placement.rs`, 2026-11-13)
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

The `incus` kind claims `confined`, the floor of `trusted`, and its `probe` gives `net: nat` and `egress_hold: none`: an instance uses the default bridge of incusd with NAT. Placement asks the hold `advisory` or more for a `trusted` job (`needed_hold`), so it refuses each `trusted` offer on the kind with `hold-not-available`. The kind then serves no job of a forge, and G-02 of githome (GitHub #66) is not met. An instance with NAT also has a way out that does not pass the egress module.

## Premise

Read `crates/podrunner-incus/src/lib.rs` (`probe`), `crates/podrunner/src/incus_kind.rs` and `crates/podrunner-slots/src/placement.rs` (`needed_hold`) on 2026-11-13. Not measured: a `trusted` offer under `serve` with only `[unit.incus]`.

## Approach

- Write a slots test first: a `trusted` offer and only a kind with the record of `incus` is refused with `hold-not-available`.
- Give the instance no NIC and one way out to the egress socket of the unit, as the `microvm` kind has: an incus `proxy` device from a port on the loopback of the instance to the socket, and the proxy variables set by the script of the instance. Then the kind claims `enforced`.
- If the incus tier of podvm cannot give that (no `--podvm-net none` and no proxy device), ask podvm by a COORD issue `(to podvm)` and set this entry `blocked`.

Progress: podvm: `--podvm-net none` and `--podvm-channel` on the incus tier (GitHub #88, COORD-028). podvm answered in PodUtils/podvm#316 (COORD-082): both flags on the incus tier (PodUtils/podvm@T-1779), in `v0.2.0-beta.12`. The code is in: the kind passes `--podvm-net none`, and with a socket directory `--podvm-channel SOCKETS/egress.sock:127.0.0.1:3128` and `PODRUNNER_CHANNEL_PORT`; the script sets the proxy variables; `probe` says `net: none` and `enforced`; `serve` needs podvm `LOWEST_INCUS` (`0.2.0-beta.12`). `the_instance_has_no_nic_and_one_channel_to_the_egress_socket` holds the argv. Next: pin beta.12, then the proof on a hosted runner (a payload reaches the socket and nothing else). The slots side holds already: `a_kind_with_no_hold_gets_no_trusted_job` refuses a `trusted` offer on a kind with no hold, as the record of `incus` has. podvm `v0.2.0-beta.12` is pinned; `tests/egress.rs` runs in `runner-image.yml`: the instance has `lo` only, a request through its proxy reaches the egress socket, and a direct connection fails.

History of the work: podvm: the `proxy` device of `--podvm-channel` does not start on incus 6.0.0 of Ubuntu 24.04 ("Failed to safely open namespace file descriptor based on pidfd"), runner-image run 43477657803 (`6365312`); GitHub #90 (COORD-030) asks for a channel with no `proxy` device. The mask of the NIC works: the 13 unit tests pass with `--podvm-net none`. Until the channel runs, `probe` claims no hold. podvm answered in PodUtils/podvm#319 (COORD-084): the channel of the incus tier is a `disk` device of the socket file and a relay of podvm's init, no `proxy` device, in `v0.2.0-beta.13`. The driver now gives the egress socket mode 0666 before the start (the unit directory of mode 0700 is the guard). podvm `v0.2.0-beta.13` is pinned and `serve` needs it for the kind (`LOWEST_INCUS`). Next: read `tests/egress.rs` in the runner-image run of the pin, then claim `enforced` again.

## Prove

`cargo test -p podrunner-slots placement`, then `cargo test -p podrunner-incus --test conformance -- --ignored` on a host with incus: a payload reaches an allowed host through the egress socket and nothing else.

## Done

2026-11-13, runner-image run 43492630049 (`39b6d1d`, podvm `v0.2.0-beta.13`, incus 6.0.0 of Ubuntu 24.04). `tests/egress.rs`: the instance has `lo` only, a request through its proxy reaches the egress socket (the socket logs `GET http://egress.podrunner.test/`), and a direct connection to 1.1.1.1 fails; the 13 unit tests pass with no NIC. The kind claims `enforced` again, so placement gives it `trusted` jobs. GitHub #88 and #90 are closed.

# T-110: The microvm and incus kinds do not check the version of podvm

**Source:** A read of `crates/podrunner-microvm/src/lib.rs` and `crates/podrunner-incus/src/lib.rs` (2026-11-13)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The `podvm` kind refuses a podvm binary outside `LOWEST` to `BEYOND` in its `probe` (`crates/podrunner-podvm/src/probe.rs`), so a wrong version fails by name, not a job at run time. The `microvm` and `incus` kinds build their records with no call to podvm. Both need podvm `v0.2.0-beta.10` or later (`docs/configuration.md`): the channel of a guest with no NIC, and `run -d -i` for the credential files of an instance. With an older podvm, `serve` starts, and each job fails at `start`.

## Premise

Read the `probe` of the three kinds on 2026-11-13. Not measured: a run of the two kinds with podvm `v0.2.0-beta.9`.

## Approach

Give `podrunner-podvm` a check of the version against a lowest version of the caller. The `probe` of `microvm` and of `incus` runs `podvm --version` and refuses a version below `0.2.0-beta.10` or at `BEYOND` and later with a `UnitError` that names both versions. A test over a fake podvm that prints `0.2.0-beta.9` holds it.

## Prove

`cargo test -p podrunner-microvm` and `cargo test -p podrunner-incus`

## Done

2026-11-13, this commit. `Podvm::require` reads the version from `podvm probe --json` and refuses one below the lowest of the kind or at `BEYOND` and later. `serve` calls it with `LOWEST_GUEST` (`0.2.0-beta.10`) when it builds `[unit.microvm]` and `[unit.incus]`, not in `probe`: a probe starts no process (unit test 23). `a_kind_refuses_a_podvm_below_its_lowest_version` holds it with a fake podvm.

# T-111: The configuration accepts keys that nothing reads

**Source:** A read of `crates/podrunner-slots/src/config.rs` and `docs/configuration.md` (2026-11-13)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`docs/configuration.md` says that an unknown key is refused. Two parts of the file do not hold that:

- `[egress]` is a free map. slots reads only `default_policy` (`crates/podrunner-slots/src/run.rs`). A misspelled `default_policy` gives no ceiling of the operator, and an offer then gets its own policy with no narrowing, with no error.
- `[unit.microvm]` and `[unit.incus]` accept `podnet`, `confine`, `registries` and `runner_dir`, which their kinds do not read. Only `mem` and `cpus` are refused on `[unit.incus]`.

## Premise

Read `crates/podrunner-slots/src/config.rs`, `crates/podrunner/src/microvm_kind.rs` and `crates/podrunner/src/incus_kind.rs` on 2026-11-13. Not measured.

## Approach

Make `[egress]` a struct with `deny_unknown_fields` and one field `default_policy` (a `broker-policy.v1` object, checked at load: `schema`, `mode`, no `path` rule). In the `plan` of the `microvm` and `incus` kinds, refuse each key of `UnitConfig` that the kind does not read, with the name of the key. Tests: a misspelled key of `[egress]` and `podnet` on `[unit.incus]` exit 78.

## Prove

`cargo test -p podrunner-slots config` and `cargo test -p podrunner --test check`

## Done

2026-11-13, this commit. `[egress]` is `EgressConfig` with `deny_unknown_fields` and one field, `default_policy`; the load refuses a policy that is not version 1, a mode other than `open` or `allow`, and a path rule. The `plan` of the `microvm` and `incus` kinds refuses `registries`, `runner_dir`, `podnet` and `confine` by name. `egress_refuses_an_unknown_key_and_a_policy_it_cannot_hold` and a case of `check_and_serve_refuse_a_bad_file_with_one_text` hold it.

# T-112: serve takes a podvm below beta.14 for an engine of the microvm kind

**Source:** A read of `crates/podrunner/src/microvm_kind.rs` and `crates/podrunner-podvm/src/probe.rs` (2026-11-14)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

With `[unit.microvm] engine = DIR`, the kind passes `--podvm-engine` to `podvm run`. podvm gives that flag from `v0.2.0-beta.14` (PodUtils/podvm#321). `build` of the kind requires only `LOWEST_GUEST` (`0.2.0-beta.10`) also with an engine. So `serve` and `podrunner check` accept podvm beta.10 to beta.13 with `engine`, and each job then fails at the start of its unit, not at the start of `serve` with a named cause.

## Premise

Read `crates/podrunner/src/microvm_kind.rs` (`build` calls `require(LOWEST_GUEST)` with or without `u.engine`) and `crates/podrunner-podvm/src/probe.rs` (no constant for the engine) on 2026-11-14. `docs/configuration.md` says that `engine` needs beta.14. Not measured with an old podvm.

## Approach

Add `LOWEST_ENGINE = "0.2.0-beta.14"` to `crates/podrunner-podvm/src/probe.rs`. In `build` of the microvm kind, require it when `engine` is set, with a detail that names `engine`. A test with a fake podvm of beta.13: the kind with no engine starts, the kind with `engine` is refused (`unsupported`).

## Prove

`cargo test -p podrunner-podvm` and `cargo test -p podrunner microvm`

## Done

2026-11-14, this commit. `LOWEST_ENGINE` (`0.2.0-beta.14`) of `podrunner-podvm`: `serve` asks it of the podvm of `[unit.microvm]` when `engine` is set, else `LOWEST_GUEST`. `Podvm::require` and its test `a_kind_refuses_a_podvm_below_its_lowest_version` hold the refusal by name.

# T-113: doctor names no driver for the microvm and incus kinds

**Source:** A read of `crates/podrunner/src/doctor.rs` (2026-11-14)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`podrunner doctor --config FILE` gives the capability record of each unit kind through its driver (`docs/cli.md`). `capability` of `crates/podrunner/src/doctor.rs` knows only `fake` and `podvm`; for `[unit.microvm]` and `[unit.incus]` it says "no driver of the kind ... in this build". The build has both drivers. So doctor reports each `untrusted` source on a host with a microvm kind, and each `trusted` source on a host with only an incus kind, as "no unit kind meets the floor and the hold of its tier", which is false.

## Premise

Read `crates/podrunner/src/doctor.rs` and `crates/podrunner/src/serve.rs` (which builds both kinds) on 2026-11-14. Not run on a host with KVM or incusd.

## Approach

Give `capability` the `microvm` and `incus` kinds through the `plan` and the `probe` of their drivers (`crate::microvm_kind`, `crate::incus_kind`), with no unit started: a probe starts no process (unit test 23). For `microvm`, say that `serve` takes the claim `guest` only after the self-test and only for a kind on `guest_kinds`. A test: a file with `[unit.microvm]` and a fake podvm gives the kind its record, not "no driver".

## Prove

`cargo test -p podrunner doctor`

## Done

2026-11-14, this commit. `capability` of `crates/podrunner/src/doctor.rs` checks `[unit.microvm]` and `[unit.incus]` with the `plan` of their kind and gives the record of their `probe`, which starts no process. `doctor_knows_the_microvm_and_incus_kinds` holds it.

# T-114: The steps of the local mode read the github context of the fake broker, not that of the run

**Source:** A read of `crates/podrunner-local/src/message.rs` and `crates/podrunner-broker-fake/src/message.rs` (2026-11-14)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`podrunner run --event NAME` gives `github.event_name` to the job level only: `crates/podrunner/src/run_local.rs` puts it, with `workflow` and `repository`, into the `Base` of the local source, which evaluates `if:`, `name` and the matrix of a job. The job message does not carry that context: `broker_job` of `crates/podrunner-local/src/message.rs` puts only `matrix`, `strategy`, `needs`, `vars` and `inputs` into `contextData`. So the runner evaluates each `${{ github.* }}` of a step, and sets `GITHUB_EVENT_NAME`, `GITHUB_REPOSITORY` and `GITHUB_WORKFLOW`, from the canned context of the fake broker: `event_name` is `push`, `repository` is `example-org/app`, `workflow` is `podrunner`. With `--event pull_request`, a job `if: github.event_name == 'pull_request'` runs, and a step of it reads `push`.

## Premise

Read the two message builders and `crates/podrunner/src/run_local.rs` on 2026-11-14. The suite checks `github.event_name` in a step only with the default event (`crates/podrunner/tests/local/suite.yml`). Not run with another event.

## Approach

Keep the `github` context of the `Base` in `LocalSource` and send it in `contextData` of each job, merged over the canned one: keep `api_url` of the broker (the stand-in of `actions/checkout` reads it), and let the run give `event_name`, `workflow`, `repository`, `run_id`, `run_number` and `server_url`. The override of the broker replaces the whole key, so merge the entries in `message.rs`. A unit test: a message of a run with `--event pull_request` has `event_name` `pull_request` and the `api_url` of the broker.

## Prove

`cargo test -p podrunner-local message`

## Done

2026-11-14, this commit. `LocalSource` keeps the `github` context of its `Base` and `broker_job` sends it in `contextData`; the fake broker puts each entry of a `github` override over its canned entry of the same key, so `api_url` and `job` of the broker stay. `the_steps_read_the_github_context_of_the_run` (a run of `pull_request`) and the broker test of the merge hold it.

# T-115: podrunner run --runner-dir does not end a job at its timeout-minutes, and it panics on an I/O error

**Source:** A read of `crates/podrunner-local/src/exec.rs` and `crates/podrunner-broker-fake/src/runner.rs` (2026-11-14)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

The `RunnerDir` executor of `crates/podrunner-local/src/exec.rs` ignores its `limit` (the `timeout-minutes` of the job): a hung step holds `podrunner run --runner-dir` for ever. The `InUnit` executor ends its unit at the limit. `RunnerDir` also calls `run_once` of `crates/podrunner-broker-fake/src/runner.rs`, a helper of the tests: it writes the runner files and starts `run.sh` with `unwrap()`. A runner directory that the user cannot write, or a `run.sh` that does not start, panics the command, not a `failure` line with a reason.

## Premise

Read both files on 2026-11-14. Not run with a hung step or a read-only directory.

## Approach

Give the broker crate a function that returns a `Result` and a child (write the files, spawn `run.sh` with the clean environment), and keep `run_once` for the tests over it. `RunnerDir::run` waits for the child up to `limit`, then kills its process group and fails the job with the limit in the reason, as `InUnit` does. A unit test over a stand-in `run.sh` that sleeps: the job fails at a short limit. A unit test with a directory of mode 0500: the job fails with a reason.

## Prove

`cargo test -p podrunner-local exec`

## Done

2026-11-14, this commit. `start_runner` of `crates/podrunner-broker-fake/src/runner.rs` writes the runner files and starts `run.sh` as the leader of its own process group, and returns each fault as an error; `run_once` stays for the tests over it. `RunnerDir::run` waits for the runner up to the limit of the job, then ends its process group (TERM, then KILL after 5 s) and fails the job with the limit in the reason. `a_runner_directory_ends_a_job_at_its_limit` and `a_runner_directory_that_takes_no_file_fails_the_job` hold it.

# T-116: check and doctor do not read the version of podvm for the microvm and incus kinds

**Source:** A read of `crates/podrunner/src/serve.rs`, `crates/podrunner/src/microvm_kind.rs` and `crates/podrunner/src/doctor.rs` (2026-11-14)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`serve` requires podvm `LOWEST_GUEST` for `[unit.microvm]` and `[unit.incus]`, and `LOWEST_ENGINE` for a microvm kind with `engine` (T-110, T-112), in `build` of each kind. `podrunner check` and `podrunner doctor` call only `plan` and `probe` of the kinds, which start no podvm. So `check` passes a file whose podvm is too old for `engine`, and `serve` then refuses the same file. `docs/cli.md` says that `serve` refuses a bad file with the same text as `check`.

## Premise

Read `check` of `crates/podrunner/src/serve.rs` (it calls `crate::microvm_kind::plan`), `build` of `crates/podrunner/src/microvm_kind.rs` and `capability` of `crates/podrunner/src/doctor.rs` on 2026-11-14. Not run with an old podvm.

## Approach

Move the version requirement of each kind into a function of the kind (`require_podvm(u)`) that `build`, `check` and `doctor` call. `podvm probe --json` reads no state and starts no unit, so `check` keeps no side effect. A test with a fake podvm of beta.13: `check` of a microvm kind with `engine` exits 78 or 80 with the text of `serve`.

## Prove

`cargo test -p podrunner --test check` and `cargo test -p podrunner doctor`

## Done

2026-11-14, this commit. The version requirement is the last check of `plan` of `crates/podrunner/src/microvm_kind.rs` (`LOWEST_GUEST`, or `LOWEST_ENGINE` with `engine`) and of `crates/podrunner/src/incus_kind.rs` (`LOWEST_INCUS`); `build`, `check` and `doctor` call `plan`, so the three give one answer. `plan_refuses_a_podvm_too_old_for_an_engine` holds it.

# T-117: The local run ignores fail-fast and the continue-on-error of a job

**Source:** A read of `crates/podrunner-local/src/lib.rs` and `crates/podrunner-local/src/drive.rs` (2026-11-14)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`pod-actions-workflow` gives each cell `strategy.fail-fast` (true by default) and `continue_on_error` of its job. The local source and its loop read neither:

- After a failed cell of a matrix with `fail-fast: true`, `drive` runs each other cell of the job. GitHub cancels the cells that did not end.
- A failed job with `continue-on-error: true` makes `podrunner run` exit 1. On GitHub the run does not fail for that job.

## Premise

Read `LocalSource::complete` and `drive` on 2026-11-14 (no read of `continue_on_error` or of `fail-fast`). Not measured on GitHub: the result that `needs.ID.result` gives after a failed job with `continue-on-error: true`.

## Approach

Follow the behaviour of GitHub Actions (the rule of Q4). In `complete`, when a cell fails and `fail-fast` is true, release the offers of the job that did not start, and end them as `cancelled`. In `drive`, do not count a failed job with `continue-on-error: true` against the exit code. Read the result of `needs` for such a job in the docs of GitHub before the change, and hold it with a unit test of the source.

## Prove

`cargo test -p podrunner-local`

## Done

2026-11-14, this commit. `LocalSource::complete`: a failed cell of a job with `continue-on-error` ends as a success for the run (`needs` gives a success, as the jobs after such a job run on GitHub under the default `success()`; not measured on GitHub); a failed cell of a matrix with `fail-fast` true cancels each cell of the job that did not end, and `drive` prints those as `cancelled` and runs no unit for them. `drive` does not count a failed job with `continue-on-error` against the exit code; its line still says `failure`. `fail_fast_cancels_the_cells_that_did_not_run` and `continue_on_error_does_not_fail_the_run` hold it.

# T-118: Local mode: actions by tag, fetched through the egress module into a store with a lock file

**Source:** The owner's ghactions-local study (act: the fetch of remote actions, the action cache, `--action-offline-mode`; runner.server: the action download of the server)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

act clones each remote action at its ref (a tag, a branch or a commit) into an action cache on the host (`--action-cache-path`, `--use-new-action-cache`), and `--action-offline-mode` uses the cache with no fetch. runner.server resolves each `uses: OWNER/REPO@REF` and gives the runner the tarball URL of the forge. Most workflows name actions by a tag (`actions/checkout@v5`, `actions/setup-node@v4`). The local mode takes only `OWNER/REPO[/PATH]@SHA` with a full commit id, from archives that the user puts in `--actions DIR` (T-091): `action` of `crates/podrunner-local/src/message.rs` refuses a tag by name. A composite action that names other actions needs each of them in DIR too.

## Premise

Read `crates/podrunner-local/src/message.rs`, `crates/podrunner-broker-fake/src/actions.rs` and act's action cache (the study) on 2026-11-14. The launch route of the fake broker answers each reference with its own ref as the commit (T-091). Not measured: the resolve of a nested action of a composite action by runner v2.338.2 (the runner calls the launch route again during the job).

## Approach

- Before the first job, `podrunner run` collects each `uses:` of the workflow, and of each composite action that it gets, until no new reference is left.
- It resolves each `OWNER/REPO@REF` to a commit (the commits API or the git protocol of the forge), and gets the tarball of the commit. The host does the fetch, through the egress module with a policy of the forge hosts only. A unit never fetches an action.
- The store keeps each tarball by `OWNER_REPO/SHA.tar.gz` (the layout of `--actions DIR`) with its SHA-256. A lock file (`podrunner-actions.lock`, beside the workflow) records `NAME@REF`, the commit and the SHA-256. The launch route of the fake broker resolves `NAME@REF` from the lock.
- `--offline` uses the lock and the store only, and fails by name on a reference that is not in the lock. `--frozen` refuses a lock that the run must change. A tarball whose SHA-256 differs from the lock fails the run.
- Exceed act: the lock pins each action by commit and digest, as a forge job of podrunner does; a moved tag fails by name, not in silence; the fetch is held by the egress policy; the store is shared by each unit read-only.
- The resolve and the store are generic: ask podcrates for a crate (for example `pod-actions-store`) by a COORD issue when the code starts.

## Prove

`cargo test -p podrunner-local` (the resolve from a lock, a moved tag, `--offline` with a missing entry) and `PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner-local --test actions -- --ignored` (a composite action by tag that names a second action by tag).

## Done

2026-11-14, this commit. `crates/podrunner-local/src/lock.rs` reads and writes `podrunner-actions.lock`; `crates/podrunner-local/src/store.rs` collects each remote `uses:` of the workflow and of each composite action, resolves it against the lock, the store and a `Forge` (git: `ls-remote` with the peeled tag, then a shallow fetch and `git archive`), and keeps `OWNER_REPO/SHA.tar.gz`. The digest covers the uncompressed tar, so a version of git or of gzip does not change it. A moved tag, a tarball that differs from the lock, `--offline` with a missing entry and `--frozen` with a change fail by name. `podrunner run` pins before the first job (`--actions DIR` is the store, by default the cache directory of the user), and the launch route of the fake broker answers each `NAME@REF` from the pins. Proven: 7 unit tests of the store and 2 of the lock; `cargo test -p podrunner-local --test store -- --ignored` resolves `actions/upload-pages-artifact@v3` and the `actions/upload-artifact@v4` that it names from github.com, then frozen and offline; the parity row `06-actions.yml` runs `actions/github-script@v7` by tag on runner v2.338.2 and passes. The fetch through the egress module is T-154; the generic crate is COORD-034.

# T-119: Local mode: artifacts v4 (the ArtifactService of the results API), one store for each run

**Source:** The owner's ghactions-local study (act: the artifact server v4 and `--artifact-server-path`; runner.server: the artifact controller v2 and `--artifact-output-dir`; the reproduction of the results-service ECONNRESET)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

`actions/upload-artifact@v4` and `actions/download-artifact@v4` call the ArtifactService of the results API (twirp: `CreateArtifact`, `FinalizeArtifact`, `ListArtifacts`, `GetSignedArtifactURL`, `DeleteArtifact`) at `ACTIONS_RESULTS_URL`, and move the bytes to a signed blob URL. act serves these routes and keeps the files in `--artifact-server-path`. runner.server serves them and writes them to `--artifact-output-dir`. In the owner's study, a runner.server build with no results route failed the round trip of the artifact workflow (`Unable to make request: ECONNRESET`, then 403). The fake broker of the local mode serves only the receiver routes of the logs (`crates/podrunner-broker-fake/src/lib.rs`); each ArtifactService route gets 404. Each job also has its own broker (`InUnit::run` of `crates/podrunner-local/src/exec.rs`), so a second job cannot see the artifact of the first.

## Premise

Read the fake broker, the executors, act's artifact server (`artifacts_v4`) and the runner's handlers in the runner.server tree of the study on 2026-11-14. The runner sets `ACTIONS_RESULTS_URL` from `ResultsServiceUrl` in the endpoint data of `SystemVssConnection`, and the fake broker gives no such key. Not measured with runner v2.338.2.

## Approach

- One artifact store for each run, outside each unit, under the scratch directory of the run or `--artifacts DIR`. Each broker of the run serves the ArtifactService routes over it and gives `ResultsServiceUrl` in the endpoint data.
- The upload goes to a blob URL of the broker with a token that names the run and the job; the finalize checks the size and the SHA-256 that the client sends. A name is unique in a run, as on GitHub.
- `--artifacts DIR` keeps the artifacts after the run, one directory for each name.
- `actions/upload-artifact@v3` uses the legacy routes that GitHub turned off: refuse it by name, as GitHub does (the rule of Q4). Read the date of that change in the GitHub changelog before the refusal.
- Exceed: a limit of bytes for each run, a digest check at finalize, and a store that no unit can write except through the broker.
- The service is generic (githome and a forge mode can use it): ask podcrates for a crate (for example `pod-actions-results`) by a COORD issue when the code starts.

## Prove

`cargo test -p podrunner-broker-fake` (each route, a wrong digest, a second name) and `cargo test -p podrunner --test local -- --ignored` (the artifact workflow of the parity suite T-147: upload in one job, download in the next, content checked).

## Done

2026-11-14, this commit. `crates/podrunner-broker-fake/src/artifacts.rs` serves `CreateArtifact`, `FinalizeArtifact`, `ListArtifacts`, `GetSignedArtifactURL` and `DeleteArtifact`, and the blob routes of the signed URLs (a whole PUT, `comp=block`, `comp=blocklist`), over one `ArtifactStore` for each run that each broker of the run shares. The job message gives `ResultsServiceUrl`, and the broker reads a body of up to 16 MiB (`read_request_with` of `podrunner-hook`). Each upload and download URL carries a random signature; a name is unique in a run; the finalize checks the size and the SHA-256; a run holds at most 4 GiB. `podrunner run --artifacts DIR` keeps `DIR/NAME/artifact.zip` and `artifact.json`. v3 and older of the artifact actions are refused by name (the GitHub changelog of 2024-04-16: refused from 2025-01-30). A wrapper of protobuf is a plain string in its JSON: download-artifact v4 refused `{"value": ...}`. Proven: 5 unit tests of the service; the parity row `07-artifacts.yml` uploads in one job and downloads in the next on runner v2.338.2 with the content checked, and its plant (a changed file) gives FAIL. The run in a unit kind is in the Prove of T-123. The generic crate is COORD-035.

# T-120: Local mode: the cache service of actions/cache

**Source:** The owner's ghactions-local study (act: the cache server, `--cache-server-path`, `--no-cache-server`; runner.server: the cache controllers v1 and v2)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

`actions/cache` and the cache of the `setup-*` actions save and restore a cache by key, with `restore-keys` as prefixes. act runs a cache server (the legacy routes under `_apis/artifactcache`) with its files in `--cache-server-path`, on by default. runner.server serves the legacy routes and the CacheService of the results API (v2). The local mode has no cache service: the runner gives no `ACTIONS_CACHE_URL`, and a cache step misses each time or fails.

## Premise

Read act's cache handler and the runner's handlers in the runner.server tree of the study on 2026-11-14: the runner sets `ACTIONS_CACHE_URL` from `CacheServerUrl` of the endpoint data, and `ACTIONS_CACHE_SERVICE_V2` when the job variable `actions_uses_cache_service_v2` is true. act binds its cache server to the outbound address of the host by default, so other machines of the network can reach it. Not measured: which protocol `actions/cache@v4` takes with runner v2.338.2.

## Approach

- A cache store that stays across runs (`--cache DIR`, by default in the cache directory of the user), served by the broker of each job on the loopback: the v2 routes, and the legacy routes if a measure shows that an action still takes them.
- Match keys as GitHub does: the exact key first, then each restore key as a prefix, newest first, in the scope of the ref of the run and then of the default branch; the version of an entry covers its paths and its compression.
- A limit of bytes for the store (GitHub keeps 10 GB for each repository), with the oldest entry removed first; `--no-cache` turns it off.
- Exceed act: the store has no port on a network interface; the scope of a ref holds, so a branch cannot poison the cache of another; each entry has its digest.
- Generic: in the same podcrates crate as T-119.

## Prove

`cargo test -p podrunner-broker-fake` (the match order, the scope, the limit) and `PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner-local --test actions -- --ignored` (a save in one run and a restore in the next).

## Done

2026-11-14, this commit. `crates/podrunner-broker-fake/src/cache.rs` serves `CreateCacheEntry`, `FinalizeCacheEntryUpload` and `GetCacheEntryDownloadURL` of the CacheService (v2), the blob uploads (a whole PUT and blocks) and a download with `Range` (206 with `Content-Range`, 416). The job message sets `actions_uses_cache_service_v2`, so runner v2.338.2 gives `ACTIONS_CACHE_SERVICE_V2` to the steps; the broker reads a body of up to 160 MiB, as the client sends an archive under 128 MiB in one PUT. A lookup takes the exact key, then each restore key as a prefix, newest first, of the same version; an entry is written once; each URL carries a random signature; the oldest entry goes above 10 GB. `podrunner run --cache DIR` (default the cache directory of the user) or `--no-cache`. `actions/cache` skips a push with no `github.ref`, so the local `github` context gives `refs/heads/main` until T-125 reads the ref of the tree; the scope of a ref is in T-125. The legacy routes are not served: the toolkit of `actions/cache@v4` takes v2 when the runner gives the variable. Proven: 5 unit tests of the cache; the parity row `08-cache.yml` saves in one job and restores in the next by the exact key and by a restore key on runner v2.338.2, with the content checked, and again on the same store. The generic crate is in COORD-035.


# T-121: Local mode: secrets from the environment and from a secret file, with the mask

**Source:** The owner's ghactions-local study (act: `-s NAME[=VALUE]`, `--secret-file .secrets`, `--insecure-secrets`; runner.server: `--secret`, `--secret-file` in YAML or in the syntax of `GITHUB_ENV`)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A workflow reads `${{ secrets.NAME }}`. act takes secrets from `-s` and from a `.secrets` file, and masks them in its output. runner.server takes `--secret` and `--secret-file`, also multiline values. The local mode has no `secrets` context: the job message has no secret variable, so each secret is empty. act takes `-s NAME=VALUE` in argv, where each process of the host can read it.

## Premise

Read `crates/podrunner-local/src/message.rs` and the runner.server tree of the study on 2026-11-14: the runner builds the `secrets` context from the variables of the message that are secret, and masks each one in its logs. Not measured with runner v2.338.2.

## Approach

- `--secret NAME` takes the value from the variable NAME of the environment of podrunner. A value in argv is refused (no `NAME=VALUE` form).
- `--secret-file FILE`: one `NAME=VALUE` on each line, and `NAME<<DELIM` for a multiline value, as `GITHUB_ENV`. Refuse a file that another user can read (mode above 0600). By default read `.secrets` of the tree only with that mode.
- Each secret goes into the job message as a secret variable and into its mask list. podrunner writes no secret to a file, a log line or the argv of a unit, and masks each value again in the logs that it keeps (T-134).
- Exceed act: no secret in argv; a file mode check; the egress hold limits where a step can send a secret.

## Prove

`cargo test -p podrunner-local message` (the secret variables and the mask) and `PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner-local --test runner -- --ignored` (a step reads a secret, and the log has `***`).

## Done

2026-11-14, this commit. `crates/podrunner-local/src/secrets.rs` reads a secret file (`NAME=VALUE`, `NAME<<DELIM`; mode 0600 or less; no name with `GITHUB_` at the start) and a secret of the environment. `podrunner run` takes `.secrets` of the `--checkout` tree, `--secret-file FILE` and each `--secret NAME`, and refuses `--secret NAME=VALUE`. `LocalSource::set_secrets` gives each job the secrets as secret variables of the message (`isSecret`); runner v2.338.2 builds `secrets` from them and masks each value. Proven: the unit tests of the file and of the variables, and `cargo test -p podrunner-local --test runner -- --ignored`: a step reads `secrets.API_KEY`, an unset secret is empty, and the log has `the secret is ***` and not the value. The mask of the logs that podrunner keeps is T-134.

# T-122: Local mode: github.token and secrets.GITHUB_TOKEN

**Source:** The owner's ghactions-local study (act: the token of `gh auth token` as `GITHUB_TOKEN`; runner.server: `GITHUB_TOKEN`, `GITHUB_TOKEN_READ_ONLY` and `GITHUB_TOKEN_NONE` of the server)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

Many actions read `github.token` or `secrets.GITHUB_TOKEN`. act takes the token of the GitHub CLI of the user when no `GITHUB_TOKEN` secret is set: each job then holds a real token with all the scopes of the user. runner.server gives a configured token, read-only or none by `permissions`. The local mode gives neither: the runner reads `github.token` from the variable `system.github.token`, which the job message does not set.

## Premise

Read the runner's execution context in the runner.server tree of the study and `crates/podrunner-broker-fake/src/message.rs` on 2026-11-14. Not measured with runner v2.338.2.

## Approach

- By default `github.token` and `secrets.GITHUB_TOKEN` are a made-up token, as the job token of the fake broker; a call with it to a real forge fails.
- `--github-token-env NAME` gives a real token from the environment, as a secret (T-121), and only to the jobs whose `permissions` are not `{}`. Never read the token of a CLI with no request of the user.
- Exceed act: no real credential by default; a real token reaches only the hosts of the egress policy (the forge API), so a step cannot send it to another host.

## Prove

`cargo test -p podrunner-local message` (the default token, the token of `--github-token-env`, `permissions: {}`).

## Done

2026-11-14, this commit. The job message gives `system.github.token` and the secret variable `GITHUB_TOKEN`: the runner does not derive `secrets.GITHUB_TOKEN` from `system.github.token`. Both are the made-up token `podrunner-local-made-up-token` by default; `--github-token-env NAME` gives the token of the environment to each job whose `permissions` (of the job, or else of the workflow) are not `{}`. Proven: the unit test of the variables with no token, a real token and `permissions: {}`, and the runner test above: `github.token` is the made-up token and `secrets.GITHUB_TOKEN` equals it. The hold of a real token to the hosts of the forge is in T-123 (the egress of a unit).

# T-123: Local mode: run each job in a unit of the microvm or the incus kind

**Source:** The owner's ghactions-local study (act and runner.server run each job in a Docker container of the host)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** partial

## Problem

act runs each job in a container of the Docker daemon of the host, and runner.server runs it in Docker or on the host. `podrunner run` has two executors only: `--runner-dir` (the host) and `--podvm` (the podvm kind, on the host network or under `podnet pod`). `serve` also has the `microvm` kind (a guest with no NIC, one channel to the egress socket, an optional engine) and the `incus` kind (a system container with no NIC). The local mode cannot use them, so a local job cannot get the boundary `guest`, and it cannot get the engine that `services:` needs (T-124).

## Premise

Read `crates/podrunner/src/run_local.rs` (`in_podvm`) and `crates/podrunner-local/src/exec.rs` (`InUnit` takes each `UnitDriver`) on 2026-11-14. The microvm kind passes its unit tests on real guests (T-037), with an engine (T-103, step 3b). Not run: the local mode on either kind.

## Approach

`podrunner run FILE --microvm PODVM --images STORE --image REF [--engine DIR]` and `--incus PODVM ...`: build the driver as `serve` builds `[unit.microvm]` and `[unit.incus]` (the same version check of podvm, T-110, T-112), and give it to `InUnit`. The egress of each unit is the channel to its egress socket, with the policy of `--allow`: the fake broker must be reachable through the egress module (an allow rule for the loopback port of the broker, or a second channel). Exceed act: each job in a fresh guest with its own kernel, no NIC, and one way out.

## Prove

`cargo test -p podrunner run_local` (the argv of each kind) and, in `runner-image.yml`, `podrunner run crates/podrunner/tests/local/suite.yml --microvm ...` gives each `ok` line.

# T-124: Local mode: services:, docker build and Docker-in-Docker through the engine of the unit

**Source:** The owner's ghactions-local study (act: `services:` on a Docker network with health checks, Dockerfile actions; runner.server: the same through the runner)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** partial

## Problem

act and runner.server run `services:` containers on a network of the job, with port maps, the service name as a host name, and health checks (`options: --health-cmd`); they build the image of a Dockerfile action. The study's service workflows reach a Redis service by `localhost` and by its name. In the local mode `services:` is refused by the hook policy on a unit of the host network (T-093), and a Dockerfile action is refused (`build-not-supported`, call C2).

## Premise

Read `docs/container-engine.md` and T-103 on 2026-11-14: in a guest of the microvm kind with `engine`, `dockerd` runs with `overlay2`, and a container reaches another by its name on a user network (runner-image run 43524984859). Not measured: a `services:` job of the official runner with the engine as its container route.

## Approach

On the microvm kind with an engine (T-123), give the runner `DOCKER_HOST` of the engine in the guest and no container hooks, so the runner runs `services:`, `container:` and Docker actions with its own Docker calls. Load each image from the store into the engine before the job (no pull inside the guest, T-130). Hold the services by the same egress policy as the job. Exceed act: no Docker socket of the host; each job's engine dies with its guest; images pinned by digest.

## Prove

`runner-image.yml` runs the services and container workflows of the parity suite (T-147) with `podrunner run --microvm ... --engine DIR`: a service answers by `localhost` and by its name, and a Dockerfile action builds and runs.

# T-125: Local mode: the event payload, the github context of the local tree, and the filters of on:

**Source:** The owner's ghactions-local study (act: the event argument, `-e/--eventpath`, `--detect-event`, `-a/--actor`, `--defaultbranch`, `--remote-name`, `-W` over a directory; runner.server: `--payload`, `--no-default-payload`, `--ref`, `--sha`, `--repository`)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** partial

## Problem

act and runner.server give `github.event` from a JSON payload file, and `github.repository`, `github.ref`, `github.sha` and `github.actor` from the local git tree or from flags. act runs only the workflows of a directory whose `on:` names the event. The local mode gives `github.event_name` (`--event`) and stand-ins (`repository` `local/workflow`, `ref` `refs/heads/main` since T-120, no `event`, no `sha`); it runs one file, whatever its `on:` says. T-114 has the defect that the steps do not see even these.

## Premise

Read `crates/podrunner/src/run_local.rs` (`github`) and act's command (`cmd/root.go` of the study) on 2026-11-14. `pod-actions-workflow` keeps `on:` as data.

## Approach

- `--event-path FILE` gives `github.event` (job level and steps, after T-114). With no file, a minimal payload of the event as GitHub sends it (`ref`, `before`, `after`, `repository` for `push`; `pull_request` with `head` and `base` for `pull_request`).
- The cache of T-120 keeps one scope: when `ref` comes from the tree, keep each entry in the scope of its ref, and let a lookup read the scope of the ref and then that of the default branch, as GitHub does.
- From the git tree of `--checkout` (or the current directory): `repository` from the URL of the remote (`--remote NAME`, default `origin`), `ref` and `sha` from HEAD, `actor` from the git user; flags `--ref`, `--sha`, `--repository`, `--actor` replace each one.
- With a directory in place of FILE (default the workflows directory of the tree), run each workflow whose `on:` takes the event, after its filters (`types`, `branches`, `tags`, `paths` with their `-ignore` forms), as GitHub does. With one FILE, refuse an event that its `on:` does not take unless `--force-event`.
- The filters of `on:` are generic: ask podcrates for them in `pod-actions-workflow` by a COORD issue.

## Prove

`cargo test -p podrunner run_local` (the payload, the git facts, a workflow that the filters skip) and `PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner --test local -- --ignored`.

State (2026-11-14): `crates/podrunner/src/run_local/context.rs` gives the facts of the git tree (`repository` of the remote, `ref` and `sha` of HEAD, `actor` of `user.name`) with the flags `--remote`, `--ref`, `--sha`, `--repository` and `--actor` over them; `github.event` from `--event-path` or a minimal payload (`push`, `pull_request` with `head` and `base` on `refs/pull/1/merge`, `workflow_dispatch`); a workflow whose `on:` does not name the event is refused unless `--force-event`. The cache keeps each entry in the scope of the ref and reads the scope of the default branch next; a random id replaces an id that two stores of one second could share. Tests: the URL forms, the facts of a git tree and the flags, the payloads, the refusal, the scopes of the cache. Next: a directory of workflows with the filters of `on:` when podcrates gives them (COORD-037).

# T-126: Local mode: the inputs of workflow_dispatch, and files of vars, env and inputs

**Source:** The owner's ghactions-local study (act: `--var-file .vars`, `--env`, `--env-file .env`, `--input-file .input`; runner.server: `--var-file`, `--env-file`, `--input-file`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

act and runner.server read vars, inputs and an extra environment of each job from flags and from files. The local mode has `--var` and `--input` only. It applies no `default` of `on.workflow_dispatch.inputs`, checks no `type` (`boolean`, `number`, `choice`, `environment`) and no `required`, so an input with a default is empty.

## Premise

Read `parse` of `crates/podrunner/src/run_local.rs` on 2026-11-14.

## Approach

`--var-file FILE`, `--input-file FILE` and `--env-file FILE` (the syntax of `GITHUB_ENV`), and `--env NAME=VALUE` for the environment of each job (as act). The inputs of `workflow_dispatch` get their defaults and their types, as GitHub gives them (a boolean is a boolean in `inputs`, a string in `github.event.inputs`); a missing `required` input or a value outside a `choice` fails the run before the first job.

## Prove

`cargo test -p podrunner run_local`

# T-127: Local mode: run one job (--job) or some cells of a matrix (--matrix)

**Source:** The owner's ghactions-local study (act: `-j JOB`, `--matrix KEY:VALUE`; runner.server: `--job`, `--matrix`)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** open

## Problem

act and runner.server run one job of a workflow, and only the cells of a matrix that match a filter. `podrunner run` runs each job and each cell.

## Premise

Read `drive` of `crates/podrunner-local/src/drive.rs` on 2026-11-14. `Graph::ancestors` of `pod-actions-workflow` gives the needs of a job and their needs.

## Approach

`--job ID` (repeat it) runs the job and each job that it needs; the other jobs end as skipped with the reason "not selected". `--matrix KEY=VALUE` (repeat it) keeps the cells whose matrix has each value. Exceed: with a run directory (T-135), `--job ID --from DIR` takes the results and outputs of its needs from the earlier run and runs only the job.

## Prove

`cargo test -p podrunner-local` (a job with its ancestors, a matrix filter).

# T-128: Local mode: --list, --graph, --dry-run and --validate

**Source:** The owner's ghactions-local study (act: `-l/--list`, `-g/--graph`, `-n/--dryrun`, `--validate`, `--strict`, `--list-options`; runner.server: `--list`, `--list-options`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

act lists the jobs of the workflows with their stages and events, draws the graph of `needs`, validates the files, and plans a run with no container. The local mode has none of these: a fault (an action that is not pinned, an image not in the store) shows only when its job starts.

## Premise

Read `crates/podrunner/src/run_local.rs` on 2026-11-14. `plan` and `Graph` of `pod-actions-workflow` give a dry run and the edges.

## Approach

`--list` prints each job, its cells, its `runs-on` and its needs. `--graph` prints the stages of `needs` as text. `--validate` parses and prints each warning of `pod-actions-workflow`, exit 78 on an error. `--dry-run` runs `plan` and builds the job message of each cell with no unit, so that each refusal (an unpinned action or image, a reusable workflow) and each missing action, image or lock entry shows before a job starts. Exceed act: the dry run checks the pins and the store.

## Prove

`cargo test -p podrunner run_local`

# T-129: Local mode: runner images by the labels of runs-on (--platform)

**Source:** The owner's ghactions-local study (act: `-P LABEL=IMAGE`, the micro, medium and large images; runner.server: `-P LABEL=IMAGE` and `-P LABEL=-self-hosted`)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** open

## Problem

act maps each label of `runs-on` to an image (`-P ubuntu-latest=IMAGE`), and offers images with the tools of the hosted runners. runner.server maps a label to an image or to the host. `podrunner run` takes one `--image` for each job and does not read `runs-on`. The runner image of podrunner has the runner, `bash` and `curl`, so a step that needs a tool of `ubuntu-latest` (a compiler, `jq`, `docker` CLI, `node` outside the runner) fails.

## Premise

Read `crates/podrunner/src/run_local.rs` and `crates/podrunner-local/src/lib.rs` (the offer carries the labels) on 2026-11-14.

## Approach

`--platform LABEL=NAME@sha256:HEX` (repeat it) and `--platform LABEL=host` (the `--runner-dir` executor). Each cell takes the image of the first label that matches; `--image` is the default. A cell whose labels match nothing fails by name (GitHub would queue it for ever). `windows-*` and `macos-*` fail by name. Add a recipe of a larger runner image with the common tools of `ubuntu-latest`, built by the image pipeline (T-030) and pinned by digest. Exceed act: each image pinned by digest; a label never falls back to an image that the user did not name.

## Prove

`cargo test -p podrunner run_local` (the choice of an image by label, a label with no image).

# T-130: Local mode: container images by tag, resolved to a digest and pulled into the store

**Source:** The owner's ghactions-local study (act: `--pull`, `--container-architecture`; runner.server: the pull of the runner)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** open

## Problem

act pulls each image of `container:`, `services:` and `docker://` by its tag. The local mode takes only `NAME@sha256:HEX` that is in the store already (`pinned` of `crates/podrunner-local/src/message.rs`): a workflow with `image: redis:7-alpine` fails, and the user must pull and import each image by hand.

## Premise

Read `crates/podrunner-local/src/message.rs` on 2026-11-14. `pod-oci` of podcrates is a registry client.

## Approach

Before the first job, resolve each image tag to a digest for the platform of the unit through the egress module (the registry hosts only), pull it into the store, and record `NAME:TAG` and the digest in the lock file of T-118. The message gives the runner the digest, not the tag. `--offline` uses the lock only. Exceed act: an image is pinned by the lock; a moved tag fails by name; the units never pull.

## Prove

`cargo test -p podrunner-local` (the lock of images, `--offline` with a missing image).

# T-131: Local mode: reusable workflows (jobs.<id>.uses)

**Source:** The owner's ghactions-local study (act: local and remote reusable workflows; runner.server: reusable workflows with `secrets: inherit` and nested calls)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** L
**Status:** open

## Problem

A job can call a workflow (`uses: ./.github/workflows/x.yml` or `OWNER/REPO/.github/workflows/x.yml@REF`) with `with:` and `secrets:` (or `secrets: inherit`); the jobs of the called workflow run, and its `outputs` become `needs.<id>.outputs` of the caller. act and runner.server run both forms. The local mode refuses the job by name (`broker_job` of `crates/podrunner-local/src/message.rs`). `pod-actions-workflow` names the call and does not expand it.

## Premise

Read `crates/podrunner-local/src/message.rs` and the model of `pod-actions-workflow` (`Job::uses`, `Job::secrets`) on 2026-11-14.

## Approach

Expand the called workflow into jobs of the run (ids `CALLER/CALLEE`), with the inputs of `on.workflow_call` (defaults and types, T-126), the secrets that the call passes, `github` of the caller, and the outputs of `on.workflow_call.outputs`. A remote workflow comes from the store and the lock of T-118. Keep the nesting limit of GitHub. The expansion is generic: ask podcrates for it in `pod-actions-workflow` by a COORD issue. Exceed: each called job is still one fresh unit; a secret goes only to the called workflow that the call names.

## Prove

`cargo test -p podrunner-local` (a local call with inputs, secrets and outputs; `secrets: inherit`; a nested call) and `PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner --test local -- --ignored`.

# T-132: Local mode: jobs in parallel (--jobs N, max-parallel)

**Source:** The owner's ghactions-local study (act: `--concurrent-jobs`, the matrix in parallel; runner.server: `--parallel N`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** open

## Problem

act runs the ready jobs and the cells of a matrix at the same time, up to the number of CPUs; runner.server starts N runners. `drive` of `crates/podrunner-local/src/drive.rs` runs one job at a time, so a matrix of eight cells takes eight times as long.

## Premise

Read `drive` and the executors on 2026-11-14. Each executor holds one broker for each job; the source takes many offers.

## Approach

`--jobs N` (default: the number of CPUs, at most the memory of the host divided by the memory of a unit): run up to N offers at a time, each in its own unit and broker, and honour `strategy.max-parallel` for the cells of a job. Print each line when its job ends. With fail-fast (T-117), cancel the running cells of the job (the cancel of the runner, then destroy). Exceed act: the units of parallel jobs share no file, port or daemon.

## Prove

`cargo test -p podrunner-local drive` (N at a time, `max-parallel`, the cancel of the running cells).

# T-133: Local mode: the checkout with the changes of the tree, the .git directory, and other repositories

**Source:** The owner's ghactions-local study (act: the copy of the working tree, `--use-gitignore`, `--no-skip-checkout`; runner.server: the local checkout of the changes, `--no-copy-git-dir`, `--remote-checkout`)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** open

## Problem

act copies the working tree (with the changes that are not committed, without the files of `.gitignore`) and its `.git` into the job. runner.server checks out the current changes of the local tree, and `--remote-checkout` runs the real `actions/checkout`. The stand-in of the local mode gives `git archive` of HEAD (`tree_tarball` of `crates/podrunner/src/run_local.rs`): no change that is not committed, no `.git` (so `git describe` and `git log` fail in a step), and a checkout of another repository (`with: repository:`) gets the local tree.

## Premise

Read `crates/podrunner/src/run_local.rs` and `checkout_script` of `crates/podrunner-local/src/message.rs` on 2026-11-14.

## Approach

- By default the tarball holds each tracked file and each new file that git does not ignore, at its state on disk; `--checkout-head` gives HEAD only.
- `--with-git` adds a copy of `.git` (or a bundle of HEAD with `fetch-depth` commits) so that git works in a step.
- A checkout with `repository:` of another repository: through the store of T-118 at the ref of `with: ref:`, or with `--remote-checkout`, the real `actions/checkout` through the egress module with the token of T-122.
- Exceed act: the unit gets a copy; it never writes the tree of the user.

## Prove

`cargo test -p podrunner run_local` (a file that is not committed is in the tarball; an ignored file is not) and `PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner --test local -- --ignored` (`git rev-parse HEAD` in a step with `--with-git`).

# T-134: Local mode: live logs, a log directory, the job summary, and --json

**Source:** The owner's ghactions-local study (act: the live output of each step with the job prefix, `--json`, `-q`, `--log-prefix-job-id`; runner.server: the live web console, `--log-output-dir`)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** open

## Problem

act prints the output of each step when it comes, with the name of its job. runner.server shows live logs and writes them to `--log-output-dir`. `podrunner run` prints one line when a job ends, and the logs of the failed jobs at the end of the run (`run` of `crates/podrunner/src/run_local.rs`). The broker keeps each blob (the logs and the job summary of `GITHUB_STEP_SUMMARY`) in one text (`Broker::logs`), so the summary mixes with the logs.

## Premise

Read `crates/podrunner-broker-fake/src/lib.rs` and `crates/podrunner/src/run_local.rs` on 2026-11-14. Not measured: whether runner v2.338.2 uploads a step log in parts during the step (live) or at its end.

## Approach

Print each upload of a log as it comes, each line with the job prefix; `-q` prints only the lines of the jobs; `--json` prints one JSON object for each event (a job start, a step, a log line, an end). `--log-dir DIR` writes the log of each job and step, and `summary.md` of each job. Mask each secret again before podrunner writes a line (T-121).

## Prove

`cargo test -p podrunner-broker-fake` (the blobs by kind) and `cargo test -p podrunner run_local` (`--json` lines, `--log-dir`).

# T-135: Local mode: a run directory and reruns of failed jobs

**Source:** The owner's ghactions-local study (runner.server: `rerun` with `--failed`, `--jobid`, `--resetArtifacts`, `--refresh`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** open

## Problem

runner.server can rerun a finished run: all failed jobs, or one job, and keep the artifacts and the results of the jobs that passed. act cannot. `podrunner run` keeps no state after the run: a failure in the last job of a long workflow needs a run of each job again.

## Premise

Read `crates/podrunner/src/run_local.rs` on 2026-11-14: the scratch directories are removed at the end of the run.

## Approach

`--state DIR` keeps the run: the result, the outputs and the log of each job, the artifacts (T-119), and the workflow and inputs with their digest. `podrunner run --rerun DIR [--failed | --job ID]` runs the failed jobs (or the job) and each job after them, with the kept results and outputs of the others. A rerun of a changed workflow fails by name unless `--refresh`. Each file of DIR is written by podrunner as its user, never by a unit (see T-146). Exceed: each rerun job gets a fresh unit, so a rerun does not depend on a dirty work tree.

## Prove

`cargo test -p podrunner-local` (a rerun keeps the outputs of a good job and runs the failed one and its dependents).

# T-136: Local mode: environments, with their secrets, vars and egress policy

**Source:** The owner's ghactions-local study (runner.server: `environment:` with `--environment-secret`, `--environment-secret-file`, `--environment-var`, `--environment-var-file`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

A job with `environment: NAME` (or `{name, url}`) gets the secrets and vars of that environment, and waits for its protection rules. runner.server takes the secrets and vars of each environment from flags and files. The local mode ignores `environment:`.

## Premise

Read `crates/podrunner-local/src/message.rs` on 2026-11-14. The model of `pod-actions-workflow` keeps `environment`.

## Approach

Evaluate `environment` (name and `url`). `--environment-secret-file NAME=FILE` and `--environment-var-file NAME=FILE` (the syntax of T-121) give its secrets and vars only to the jobs of that environment. `--approve NAME` passes its protection; with no flag, a job of a protected environment asks on the terminal, or fails when stdin is not a terminal. Exceed runner.server: `--environment-allow NAME=HOST` gives the jobs of an environment their own egress policy (a deploy job reaches its target; a test job does not).

## Prove

`cargo test -p podrunner-local message` (the secrets of an environment reach only its jobs).

# T-137: Local mode: concurrency groups and cancel-in-progress

**Source:** The owner's ghactions-local study (runner.server: `concurrency` of the workflow and of the job, with `cancel-in-progress`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

GitHub runs at most one job or run of a concurrency group at a time, and cancels the older one with `cancel-in-progress: true`. runner.server follows it. act does not. The local mode ignores `concurrency`; two parallel jobs of one group (T-132) or two runs on one host can run at once.

## Premise

Read `crates/podrunner-local/src/lib.rs` on 2026-11-14. The model of `pod-actions-workflow` keeps `concurrency` of the workflow and of each job.

## Approach

Evaluate the group of each job and of the run. In a run, a job waits while another job of its group runs. Across runs on one host, a lock file of the group in the state directory of the user holds it; with `cancel-in-progress`, the new run cancels the old one (the cancel of the runner, then destroy). The group of GitHub is per repository: name it with `github.repository`.

## Prove

`cargo test -p podrunner-local` (two jobs of one group take turns; a cancel of the older run).

# T-138: Local mode: keep the unit of a failed job, export the workspace, and a shared tool cache

**Source:** The owner's ghactions-local study (act: `-r/--reuse`, `-b/--bind`, `--rm`; runner.server: `--keep-container`, `--keep-runner-directory`, `--no-shared-toolcache`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

act can keep its containers between runs (`--reuse`) and bind the tree into the container (`--bind`), so that the state and the build outputs stay. runner.server can keep a container or a runner directory for a look after the job, and shares one tool cache between its runners. The local mode destroys each unit at the end of its job: a user cannot look into a failed job, cannot take its build outputs, and each job downloads its tools again.

## Premise

Read `InUnit::run` of `crates/podrunner-local/src/exec.rs` on 2026-11-14. Not measured: a read-only mount of a host directory into a podvm unit for the tool cache.

## Approach

- `--keep-failed`: do not destroy the unit of a failed job; print its id and the command to enter it, and destroy it at the next run or with `podrunner run --clean`.
- `--export DIR`: copy the workspace of each job to `DIR/JOB` after the job, as files of the user.
- `--toolcache DIR`: mount DIR read-only at the tool cache path of the runner in each unit; a job that adds a tool writes to its own copy. runner.server warns that a shared writable cache can break a workflow; a read-only cache cannot.
- Exceed act: by default each job starts from a fresh unit; a reuse never carries state from one job into the next.

## Prove

`cargo test -p podrunner-local exec` (a kept unit is not destroyed; the export) and `runner-image.yml` with `--toolcache`.

# T-139: Local mode: actions from a local directory (--local-repository) and from another forge

**Source:** The owner's ghactions-local study (act: `--local-repository REPO@REF=DIR`, `--github-instance`, `--replace-ghe-action-with-github-com`; runner.server: `--local-repository`, `--git-server-url`, `--git-tarball-url`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

act and runner.server can serve `uses: OWNER/REPO@REF` from a local directory, for the work on an action, and can take actions from a GitHub Enterprise Server or another forge. The local mode takes archives by commit from `--actions DIR` only.

## Premise

Read `crates/podrunner-broker-fake/src/actions.rs` on 2026-11-14.

## Approach

`--local-repository OWNER/REPO@REF=DIR`: the broker resolves that reference to a made-up commit and serves a tarball of DIR made at the start of the run (the files of the tree, with one top directory). `--action-server URL` (default `https://github.com`) is the forge that T-118 asks; a forge of the type of Gitea or Forgejo takes its own tarball URL form.

## Prove

`cargo test -p podrunner-broker-fake` (a local directory served as an action).

# T-140: Local mode: OIDC id-tokens (permissions: id-token: write)

**Source:** The owner's ghactions-local study (runner.server: the OIDC controller, `.well-known/openid-configuration` and `jwks`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

A job with `permissions: id-token: write` gets `ACTIONS_ID_TOKEN_REQUEST_URL` and a token to ask for a signed JWT (for a cloud login or a signature). runner.server issues such tokens and serves the keys. The local mode does not: the runner sets the variable from `GenerateIdTokenUrl` in the endpoint data, which the fake broker does not give.

## Premise

Read the runner's handlers in the runner.server tree of the study and `crates/podrunner-broker-fake/src/message.rs` on 2026-11-14.

## Approach

The broker gives `GenerateIdTokenUrl` to a job with that permission, and signs a JWT with the claims of GitHub (`sub` of `repo:OWNER/REPO:ref:REF`, `aud` from the request, `repository`, `ref`, `sha`, `job_workflow_ref`) with a key of the run, which stays in memory. `--oidc-issuer URL` names the issuer, and `--oidc-keys FILE` writes the public keys for a relying party. Exceed: one key for each run, never on disk.

## Prove

`cargo test -p podrunner-broker-fake` (the claims and the signature of a token).

# T-141: podrunner run leaves its units and scratch directories at Ctrl-C

**Source:** The owner's ghactions-local study (act and runner.server cancel the run and remove their containers at an interrupt)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

act cancels its context at SIGINT, stops the job and removes its containers. `podrunner run` has no handler of SIGINT or SIGTERM (`crates/podrunner/src/run_local.rs`): the process ends at once, `destroy` of the unit does not run, and the scratch directories (the tree tarball, the state of the units, the socket directories) stay in the temporary directory.

## Premise

Read `crates/podrunner/src/run_local.rs` and `crates/podrunner-local/src/exec.rs` on 2026-11-14. Not measured: what stays of a podvm unit when its parent ends.

## Approach

At the first SIGINT or SIGTERM, cancel the running job (the cancel message of the broker, then its `cancel-timeout-minutes`), destroy each unit, remove the scratch directories, print the line `cancelled` for each job that did not end, and exit 130. At a second signal, destroy at once. A test with the fake unit: a signal during a job leaves no unit and no scratch directory.

## Prove

`cargo test -p podrunner run_local` (a signal during a job on the fake unit).

## Done

2026-11-14, this commit. `podrunner_local::cancel` installs a handler of SIGINT and SIGTERM that sets a flag; a third signal ends the process. `RunnerDir` ends the process group of its runner at the flag; `InUnit` waits in slices of one second and destroys its unit at once. `drive_with` prints the skips that are not printed and a `cancelled` line for each job with no result, and runs no more; `podrunner run` removes its scratch directories and exits 130. The runner gets no cancel message of the broker: the TERM of its process group makes it end its job as `Canceled`. Proven: the unit test of the drive loop (a stop during the first of three jobs), and a run here on runner v2.338.2: SIGINT during a `sleep` step gives `cancelled long`, exit 130, no runner process and no scratch directory.

# T-142: Local mode: --watch and the cron times of on.schedule

**Source:** The owner's ghactions-local study (act: `-w/--watch`; runner.server: `--watch`, the schedules of the server)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

act and runner.server run the workflow again when a file of the tree changes; runner.server also runs `on: schedule` at its cron times. The local mode runs once.

## Premise

Read `crates/podrunner/src/run_local.rs` on 2026-11-14.

## Approach

`--watch` runs again after a change of the tree (inotify, with a short delay to group changes), and cancels the run in progress (T-137). `--schedule` waits for the next cron time of `on.schedule` (UTC, as GitHub) and runs with the event `schedule`; a systemd timer of the user can do the same with `--event schedule`.

## Prove

`cargo test -p podrunner run_local` (the next time of a cron expression; a change starts a run).

# T-143: Local mode: a configuration file for podrunner run

**Source:** The owner's ghactions-local study (act: `.actrc` in the configuration directory, the home and the tree, with `--trust-project-actrc`; runner.server: reads `.actrc`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

act reads its flags from `.actrc` files; a file of the project may only set flags that do not reach host resources unless the user trusts it. runner.server reads `.actrc` too. Each `podrunner run` needs each flag on the command line.

## Premise

Read act's `cmd/actrc.go` of the study on 2026-11-14.

## Approach

A TOML file of the user (`podrunner/run.toml` in the configuration directory) and of the project (`.podrunner/run.toml`), with each unknown key refused. The project file cannot set a key that reaches the host or the network (`podvm`, `images`, `allow`, `secret-file` outside the tree, `toolcache`) unless the user file names the project as trusted. Read the flags of `.actrc` that map to podrunner (`-P`, `--secret-file`, `--var`, `--env`, `--input`) and name each one that does not.

## Prove

`cargo test -p podrunner run_local` (the order of the files; a project file that sets `allow` is refused).

# T-144: podrunner run --runner-dir starts the runner with W^X on, and dies under a small RLIMIT_FSIZE

**Source:** The owner's ghactions-local study (the reproduction of the dotnet SIGXFSZ)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The study reproduced it: on a host with a limit of file size (`ulimit -f`), .NET with the default W^X maps its code through a memfd and truncates it to a large size, and the process dies with SIGXFSZ at start. The study runs each .NET program with `DOTNET_EnableWriteXorExecute=0`. The podvm driver sets that variable (`crates/podrunner-podvm/src/unit.rs`, U2). `run_once` of `crates/podrunner-broker-fake/src/runner.rs`, which the `--runner-dir` executor calls, clears the environment and does not set it, so `podrunner run --runner-dir` dies on such a host with no reason (T-115 has the other faults of that function).

## Premise

Read both files and `docs/unknowns.md` (U2) on 2026-11-14. Not run under a small limit.

## Approach

Set `DOTNET_EnableWriteXorExecute=0` in the environment of the runner of the `--runner-dir` executor, as the podvm driver does. A runner that ends by SIGXFSZ fails its job with a reason that names the limit. Do it with T-115.

## Prove

`cargo test -p podrunner-local exec` (the environment of the runner has the variable) and `PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner-local --test runner -- --ignored` under `ulimit -f 1048576`.

## Done

2026-11-14, this commit. `start_runner` sets `DOTNET_EnableWriteXorExecute=0` for each runner of the `--runner-dir` executor and of the tests, as the podvm driver does; a runner that SIGXFSZ ends fails its job with a reason that names the limit of file size.

# T-145: The fake broker answers 404 in silence to a route that it does not serve

**Source:** The owner's ghactions-local study (the reproduction of the results-service ECONNRESET)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

In the study, an artifact step failed with `ECONNRESET` and then 403, because the server had no route of the results service: the log named a symptom, not the missing service. The fake broker of the local mode answers each route that it does not know with 404 and records it only in `seen` for the tests (`serve` of `crates/podrunner-broker-fake/src/lib.rs`). A step that calls a service of GitHub that the local mode does not give (artifacts, cache, OIDC, a route of a new runner) fails with an error of its client.

## Premise

Read `crates/podrunner-broker-fake/src/lib.rs` on 2026-11-14.

## Approach

Keep each request with no route, and add one line for each to the log of its job: the method, the path with no query, and the service that it belongs to when it is known (`ArtifactService`, `CacheService`, `_apis/artifactcache`), with the entry that gives it (T-119, T-120, T-140). The run prints the count at the end.

## Prove

`cargo test -p podrunner-broker-fake` (a request to an unknown route is in the job log).

## Done

2026-11-14, this commit. The fake broker keeps each request that it has no route for (`Broker::missing`), with the service and the entry when the path names one (`service_of`: the ArtifactService and the CacheService, the legacy cache and artifact routes that GitHub turned off, the OIDC id-token of T-140). The log of each job gets one line for each; `podrunner run` names each on stderr with its job, whatever the result, and prints the count. Proven: the unit test of the broker (a legacy cache route and an unknown route).

# T-146: A job can leave files that the next run cannot remove

**Source:** The owner's ghactions-local study (the reproduction of a rerun EACCES of another local runner: files of a container uid that the host user cannot remove)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** partial

## Problem

The study met a local runner whose second run failed with EACCES: the runner container wrote files as another uid into a directory of the host, and the host user could not remove them. The `--runner-dir` executor removes `_work` before each job and ignores the error (`RunnerDir::run` of `crates/podrunner-local/src/exec.rs`): a step that makes a directory with no write bit, or a file of another uid, makes the next job run in the stale tree of the last one, with no message. In a unit, a `container:` job writes as the uid of its container; destroy must remove those files too, or report residue.

## Premise

Read `crates/podrunner-local/src/exec.rs` on 2026-11-14. Not measured: the residue of a podvm unit after a container job that wrote as root into the workspace.

## Approach

`RunnerDir::run`: add the write bit where it is missing, then remove `_work`; a failure fails the job with the path. In a unit: a test job (`container:` as root, which writes into the workspace and makes a directory of mode 0500) runs twice on one state directory in `runner-image.yml`; both runs pass and destroy reports no residue. Each file of the run directory of T-135 is written by podrunner, never by a unit.

## Prove

`cargo test -p podrunner-local exec` (a `_work` with a directory of mode 0500 is removed, or the job fails with its path) and the double run in `runner-image.yml`.

State (2026-11-14): `podrunner_local::work::clear` gives each directory of `_work` the bits `u+rwx` again before the removal, and a failure fails the job with the path (unit test: a tree with modes 0000 and 0500). The unit half is in `runner-image.yml`: `crates/podrunner/tests/local/rerun.yml` (a container job as root that leaves a directory of mode 0500) runs twice with `--podvm`; a residue fails the job.

# T-147: Local mode: the parity suite, with checks outside the job log

**Source:** The owner's ghactions-local study (its conformance suite of seven workflows and its checks)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

The study holds a suite of seven workflows of rising scope: exit codes; expressions, functions, `fromJSON` and `needs` outputs; a matrix with `exclude` and `include` (exactly four cells); a service with a health check, reached by `localhost` and by its name; a job container and a container with a service; marketplace actions and a local composite action with outputs; an artifact round trip with a check of its content. Each workflow gives PASS, PARTIAL or FAIL with a reason. T-046 records its defects: a missing file of the actions workflow, markers that can match the echoed script, a removal of each container of the host, and images and actions pinned by tags that move. The scripts carry no licence. podrunner's suites (`crates/podrunner/tests/local/suite.yml`, `containers.yml`, `egress.yml`) cover the first scope, not services, marketplace actions or artifacts.

## Premise

Read the study's conformance runner, its check script and its seven workflows on 2026-11-14. Nothing of it is copied.

## Approach

Write a new suite in `crates/podrunner/tests/local/` (0BSD, own text), one workflow for each scope of the study, each with its own fixtures. Each check reads a value that a step computes (for example the SHA-256 of a file), printed on its own line, and the check matches only output lines, never the echoed script. Images and actions are pinned by digest and commit through the lock of T-118 and T-130. A check that needs a later entry is a known PARTIAL with the entry id, so the suite shows the gaps; each entry of this study turns its row green. Run it in level 2 (`--runner-dir`) and in `runner-image.yml` (the unit kinds).

## Prove

`PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner --test parity -- --ignored` prints one verdict for each workflow, and a plant (a wrong artifact content) gives FAIL.

## Done

2026-11-14, this commit. `crates/podrunner/tests/local/parity/` holds seven workflows and a local composite action; `crates/podrunner/tests/parity.rs` runs each with `podrunner run --runner-dir` and prints `parity FILE PASS`, `PARTIAL T-NNN` or `FAIL`. A failure is PARTIAL only while its entry is open in the record and only in the jobs that need it, so a done entry with a red row gives FAIL. Level 2 runs it. Result on runner v2.338.2: exit, expressions and matrix PASS; services PARTIAL T-124; containers PARTIAL T-123; actions PARTIAL T-118 (the local composite job passes); artifacts PARTIAL T-119; the plant (`--var PLANT=1`) gives FAIL. The marketplace actions keep their tags until the lock of T-118 pins them by commit; T-123 adds the run in `runner-image.yml`.

# T-148: Research: Azure Pipelines YAML in the local source

**Source:** The owner's ghactions-local study (runner.server: `--event azpipelines`, the templates of Azure Pipelines)
**Category:** research
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** open

## Problem

runner.server also runs a subset of Azure Pipelines YAML (`steps`, `script`, templates with `each`, `if` and `else`) on the same runner. podrunner's local source reads only GitHub workflows.

## Premise

Read the README of runner.server in the study on 2026-11-14. Not measured: the subset that runner.server covers, and whether the official runner takes the job message of such a pipeline.

## Approach

Read the expansion of runner.server (its `ExpandAzurePipelines` part) and the job message that it gives the runner. Write a note in `docs/` with the subset, the cost, and a recommendation. If the cost is real, write the question for the owner in `TODO/PROGRESS.md` with that recommendation before code.

## Prove

The note exists and `cargo todo check` passes.

# T-149: Local mode: a web view of a run on the loopback

**Source:** The owner's ghactions-local study (runner.server: the web page of runs, jobs and live logs)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** open

## Problem

runner.server shows each run, job and live log on a web page. The local mode prints lines only.

## Premise

Read the README of runner.server in the study on 2026-11-14.

## Approach

`--web ADDR` (loopback only, a random token in the URL): a static page served by podrunner with each job, its state, its steps, its live log (T-134) and its artifacts (T-119). No script from a CDN; the page is in the binary. Exceed: the page binds no address other than the loopback, and names no secret.

## Prove

`cargo test -p podrunner run_local` (the page refuses a request with no token; the job list).

# T-150: Local mode: a pause before a step and a shell in the unit

**Source:** The owner's ghactions-local study (its plan of a debugger)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** open

## Problem

The study plans a debugger: stop a job before a step and look into its environment. act and runner.server give only `--reuse` or `--keep-container`. The local mode gives neither.

## Premise

Read the README of the study on 2026-11-14. Not measured: a pause of the official runner between steps (a step that waits for a file is one way).

## Approach

`--break JOB:STEP` puts a step before STEP that waits for a file in the unit; podrunner prints the command to enter the unit (`podvm exec`) and goes on when the user removes the file or ends the shell. With `--break-on-failure`, the pause comes after a failed step. The unit keeps its egress hold during the pause.

## Prove

`cargo test -p podrunner-local message` (the wait step in the message).

# T-151: Measure: a job of another architecture in the local mode

**Source:** The owner's ghactions-local study (act: `--container-architecture`; runner.server: `--container-architecture`, `--container-platform`)
**Category:** measurement
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

act runs a container of another architecture (for example `linux/arm64` on an x86_64 host) through the emulation of Docker. The local mode runs the runner of the architecture of the host only (`arch` of `crates/podrunner/src/run_local.rs`).

## Premise

Read `crates/podrunner/src/run_local.rs` on 2026-11-14. Not measured: a guest of the machine tier of podvm with another architecture (emulated by QEMU, with no KVM).

## Approach

Measure in `runner-image.yml`: an aarch64 runner image in a guest of the microvm kind on an x86_64 host, with the time of the start and of a small job. Record the result in `docs/unknowns.md`; if it holds, add `--arch` with T-123.

## Prove

`gh workflow run runner-image.yml` runs the measure, and `docs/unknowns.md` has its row.

# T-152: Local mode: an env of one expression is refused

**Source:** The owner's ghactions-local study (runner.server's README uses `env: ${{ secrets }}` on a step)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

runner.server runs a step with `env: ${{ secrets }}` (an expression that gives a mapping). `env` of `crates/podrunner-local/src/message.rs` refuses an `env:` of one expression by name.

## Premise

Read `crates/podrunner-local/src/message.rs` on 2026-11-14. Not measured: whether GitHub and runner v2.338.2 take an `env:` of one expression at the levels of the workflow, the job and the step.

## Approach

Measure each level against the schema of the runner and the GitHub docs first (the rule of Q4). Where GitHub takes it, send the expression as one token of the mapping and let the runner evaluate it; where GitHub refuses it, keep the refusal and name the level in its text.

## Prove

`PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner-local --test runner -- --ignored` (a step `env:` of one expression).

# T-153: podrunner doctor for the local mode

**Source:** The owner's ghactions-local study (its doctor plan; act: `--bug-report`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

The study plans a doctor that finds the faults of a host before a run (the SIGXFSZ limit, a missing tool), and act prints a bug report of the system. `podrunner doctor` reads a file of `serve`; it has no check of the inputs of `podrunner run`.

## Premise

Read `docs/cli.md` (the row of `doctor`) on 2026-11-14.

## Approach

`podrunner doctor --run [the options of run]`: check the podvm binary and its version, the store and the image digest, `/dev/kvm` for the microvm kind, podnet and its hold, the runner directory (`run.sh`, `bin/Runner.Listener`), the limit of file size (T-144), the free disk of the scratch directory, and the lock file and store of T-118. Each check names its fix; nothing changes.

## Prove

`cargo test -p podrunner --test doctor`

# T-154: Local mode: the fetch of the actions through the egress module

**Source:** T-118 (its Approach: the host fetches through the egress module with a policy of the forge hosts only)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

`podrunner run` resolves and fetches each action by tag with `git` on the host (T-118). git takes the proxy of the environment of the host and reaches each host that the host reaches. The fetch has no egress policy of its own: a redirect or a submodule URL could make it reach another host.

## Premise

Read `crates/podrunner-local/src/store.rs` (`Git`) and `crates/podrunner-egress/src/lib.rs` on 2026-11-14: the egress module serves a policy on a unix socket for each job; git cannot take a unix socket as its proxy.

## Approach

- Open one egress grant for the fetch, with a policy of the forge hosts only (`github.com`, `codeload.github.com`), and give git a loopback TCP proxy that relays to its socket (`http.proxy`, `GIT_CONFIG_*` variables, no file of the user).
- Turn off submodules and other protocols for the fetch (`protocol.allow=never`, `protocol.https.allow=always`).
- Keep the `Forge` seam: a test forge needs no egress.

## Prove

`cargo test -p podrunner-local` (the git command of the fetch names the loopback proxy and the protocol rules) and `cargo test -p podrunner-local --test store -- --ignored` through the egress module.

# T-155: An intermittent failure of the workspace tests that the check script hid

**Source:** A session (2026-11-14): `sh scripts/agent/check.sh` gave `test 101` once at the commit after `8a65c48`, and passed on the next two runs
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

One run of `cargo test --workspace --all-features` failed, and the next runs passed. The check script printed only the summary lines: its pattern `failed` matched each `0 failed`, and `head -30` cut the output before the cause. The test that failed is not known. Five runs of the unit tests of `podrunner-broker-fake`, `podrunner-local` and `podrunner` gave no failure. The tests that depend on time are suspects: the cache tests with a sleep of 1.1 s (the newest entry first), the limit tests of `exec`, and the front tests (fixed for one race in `8a65c48`).

## Premise

Read `scripts/agent/check.sh` on 2026-11-14. The pattern is fixed in the same commit as this entry: it now matches `FAILED`, `panicked`, `error`, `failures:` and `----`.

## Approach

Run `cargo test --workspace --all-features` in a loop (20 runs) and keep each failed log. Fix the test that fails: no sleep as an order, a time given by the test instead of the clock.

## Prove

20 runs of `cargo test --workspace --all-features` in a row pass.

