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
**Status:** done

## Problem

githome (GitHub #66, G-01) ships its own source program, `githome source`, that speaks the source wire of `docs/contracts/source-driver.md` on its stdio. podrunner has the kinds `fake`, `github-scaleset` and `remote` only, so no forge but GitHub can give jobs. The non-goal "the `forge-contract` source" had the trigger "githome has code"; githome has code (its T-075).

## Premise

Read GitHub #66 (comments of 2026-11-12 and 2026-11-13) on 2026-11-13. `WireSource` (`crates/podrunner-source`) already speaks the wire to the scale-set sidecar.

## Approach

A `[[source]]` kind `program`: the absolute path of the program in `driver_dir`, its argv, its credential file (passed by path, mode 0600, never in argv), its slots and its labels. `serve` spawns it and talks the wire over its stdio with `WireSource`, as for the sidecar. `podrunner status --json` groups the jobs by source. Remove the row of the non-goal.

## Prove

The 12 source conformance tests pass against a program of the test that speaks the wire; with a `github-scaleset` fake and a `program` source of 2 slots each, 10 jobs of each run and no source holds more than its slots.

## Done

2026-11-15: githome answered on GitHub #107 (COORD-047): on the LIVE-2 host one `serve` held the `remote` source of the scale set and two `program` sources of githome (one for each tier, each with its own host token and 1 slot); `podrunner source-conformance` passed 12 of 12 against `githome source`; githome's jobs ran on the podvm, incus and microvm kinds and got their reports. The notes of githome are T-190 (the conformance binary) and T-191 (an owner that is a person).

Partial, 2026-11-13, this commit. The `[[source]]` kind `program` (`program`, `program_args`, an optional `key_file` whose path the program gets in `PODRUNNER_SOURCE_KEY_FILE`); `serve` runs it through `SidecarSource::program`, the restart and reply deadlines of the sidecar with no `sign` channel. `podrunner fake-source --script FILE` serves the fake source on stdio. `crates/podrunner/tests/program_source.rs`: the 12 source conformance tests pass against `podrunner fake-source` as a program source. `a_program_source_needs_its_program` holds the keys. `docs/configuration.md` ("A program source") and `docs/cli.md` say so. `status --json` lists each source with its jobs (the schema of `status.v1`), and slots holds the slots of each source whatever its kind (`g01_no_source_holds_more_than_its_slots`, two sources of 2 slots, 10 jobs each). Open: the same with a `program` source in `serve`. A `program` source is a forge source, so its jobs need the trusted or untrusted tier (trust facts, a signed pin list, a kind at the floor), which no fake kind of `serve` meets; the pair runs on the host of LIVE-2 (GitHub #75) when githome gives its program (its plan on #75).

State (2026-11-14): COORD-047 (GitHub #107) asks githome for the release of `githome source`, its conformance output and the configuration of the pair on the LIVE-2 host. githome answers by a comment.

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
**Status:** done

## Problem

DR-8 (call C2) keeps the official container hooks and no Docker API socket, and brings an engine back when "an ask names a client that the hooks do not reach". The owner ruled on 2026-11-13 that githome's G-09 keeps three such clients: `services:` containers, `docker build` (and Dockerfile actions) and Docker-in-Docker. Each works on a GitHub-hosted runner. G-09 refuses a socket of the host.

## Premise

Read GitHub #77 and #66 (the comments of githome of 2026-11-13) on 2026-11-13. The hooks run `container:` jobs and Docker actions (T-093, T-098); `services:` runs only on a unit with its own network; `docker build` is refused (`build-not-supported`).

## Approach

1. The census of the verbs (DR-17): the Docker CLI calls of the three clients (`docker build`, `docker run` and `docker network` of `services:`, the daemon of Docker-in-Docker), from the source of the official runner and of the actions that the asks name.
2. A design note: an engine inside the unit (a rootless Docker or Podman inside a podvm guest of the machine tier, or inside a system container of incus), with its socket in the unit only. Record the choice in `docs/decisions.md`.
3. Then the crate ask to podcrates (`pod-dockerapi`, if the census needs it) by a COORD issue.

State (2026-11-13): step 1 is done: the census of the verbs and the design (`docs/container-engine.md`): the runner runs 15 verbs of the `docker` command line, and Docker-in-Docker needs the whole Engine API, so a real engine runs inside a guest with its socket there, and no shim (`pod-dockerapi`) is needed. Step 2 is done: podvm `v0.2.0-beta.10` gives a guest with no NIC and one host channel (GitHub #83 is closed; the microvm kind uses it, T-037). Next: step 3, the runner image with an engine and the value `engine` of `ContainerRoute` (a COORD issue to podcrates). Step 3a (2026-11-13): GitHub #91 (COORD-031) asks podcrates for `ContainerRoute::Engine` and a need of an engine in `UnitNeed`. `scripts/measure-engine.sh` in `runner-image.yml` measures the static Docker binaries in a guest of the machine tier: `dockerd` with no iptables and no bridge, the storage driver and the cgroups it finds, and a container from a saved image with no network. Runner-image run 43484759471 (`60b52a2`, Docker 27.5.1): containerd boots, and `dockerd` stops with "Devices cgroup isn't mounted": the guest (kernel `6.12.110-0-virt`) mounts no cgroup (`/sys/fs/cgroup` is sysfs) and has no `overlay` in `/proc/filesystems`. The measure now mounts cgroup2 and tries `modprobe overlay` before `dockerd`. Runner-image run 43490092612 (`c08f762`): with cgroup2 mounted by the script (controllers cpuset, cpu, io, memory, hugetlb, pids), `dockerd` starts in 1 s with the storage driver `vfs` and loads an image. The guest has no `modprobe`, no `/lib/modules` and no `overlay`, so only `vfs`. A container fails with "pivot_root .: invalid argument": the root of the guest is an initramfs. The measure now starts `dockerd` with `DOCKER_RAMDISK=true` (runc moves the root and chroots). Runner-image run 43495436619 (`3037c1c`): a container runs in the guest (`container-42`). The measure now makes a user-defined bridge network, as `services:` does, and one container reaches another by its name. Runner-image run 43500428451 (`9dbd3fc`): `docker network create` fails with "operation not supported": the guest kernel has no bridge. GitHub #92 (COORD-032) asks podvm for bridge, veth, netfilter, overlay and a cgroup2 mount in the guest. podvm answered in PodUtils/podvm#321 (COORD-085, PodUtils/podvm@T-1780): `--podvm-engine` loads 33 modules of the pinned kernel (bridge, veth, overlay, nf_tables and the `xt_*` of Docker) before the command; each guest mounts cgroup2 and brings `lo` up; podvm brings no `iptables` binary. It ships in `v0.2.0-beta.14`. podvm `v0.2.0-beta.14` is pinned; the runner image has `iptables` (nft); the measure runs `dockerd` with its default bridge and rules under `--podvm-engine --podvm-net none`. Runner-image run 43514371042 (`261088e`): cgroup2 and `overlay` are there, `dockerd` starts in 1 s with `overlay2`, a container runs, and on a user-defined network one container reaches another by its name. GitHub #92 is closed. Step 3b, the code: `[unit.microvm] engine = DIR` (the static Docker binaries): the kind passes `--podvm-engine`, mounts DIR read-only at `/podrunner-engine` (in the mount table of T-036), and the guest script starts `dockerd` as root with `DOCKER_RAMDISK` and gives the runner `DOCKER_HOST` of a socket in the guest. `tests/engine.rs` runs a container as `runner` on a network in a real guest in `runner-image.yml`. Runner-image run 43524984859 (`a5867d9`): it passes: the payload runs as `runner` with `DOCKER_HOST` of the socket in the guest, `docker info` gives `overlay2`, and a container on a user network prints `engine-42`; the guest leaves no residue. podcrates `v0.42.0` (PodUtils/podcrates#145) gives `ContainerRoute::Engine` and `UnitNeed::container_engine`; podrunner takes it. The microvm kind with an engine reports `container: engine`; placement sends a job with `container_engine` only to such a kind, and a `container:` job to the hooks or to an engine; the schema of `status.v1` has the value. Next: the parity suite in guests with an engine in `runner-image.yml` (`04-services.yml`, `05-containers.yml`, `09-dockerfile.yml`, with T-123 and T-124).

## Prove

The census and the design are in `docs/`; a `docker build` job and a `services:` job pass in a unit in `runner-image.yml`. Runner-image run 43557460656 (`5dedd26`): in guests with an engine the runner takes the engine (`docker network create`, `docker pull` of the pinned digest), and the pull fails on the blobs: Docker Hub sends each blob to `production.cloudfront.docker.com`, and the allow list named `production.cloudflare.docker.com`. The step did not fail: a `run` step with no `shell:` has no `pipefail`, so `tee` hid the exit of the test; the step now sets `pipefail`. The parity suite ran inside the step of the engine measure, which has `continue-on-error`: a failure there could not fail the workflow. It is now a step of its own with no `continue-on-error` (a check), and the job has 90 minutes.

## Done

2026-11-14, `5096a3b`: runner-image run 43609196605 is green. In guests of the microvm kind with an engine the official runner uses the engine for `services:`, a job container and Docker actions: 01 to 09 of the parity suite pass. The census and the design are in `docs/container-engine.md`. The load of images from the store into the engine is T-157.


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
**Status:** done

## Problem

act runs each job in a container of the Docker daemon of the host, and runner.server runs it in Docker or on the host. `podrunner run` has two executors only: `--runner-dir` (the host) and `--podvm` (the podvm kind, on the host network or under `podnet pod`). `serve` also has the `microvm` kind (a guest with no NIC, one channel to the egress socket, an optional engine) and the `incus` kind (a system container with no NIC). The local mode cannot use them, so a local job cannot get the boundary `guest`, and it cannot get the engine that `services:` needs (T-124).

## Premise

Read `crates/podrunner/src/run_local.rs` (`in_podvm`) and `crates/podrunner-local/src/exec.rs` (`InUnit` takes each `UnitDriver`) on 2026-11-14. The microvm kind passes its unit tests on real guests (T-037), with an engine (T-103, step 3b). Not run: the local mode on either kind.

## Approach

`podrunner run FILE --microvm PODVM --images STORE --image REF [--engine DIR]` and `--incus PODVM ...`: build the driver as `serve` builds `[unit.microvm]` and `[unit.incus]` (the same version check of podvm, T-110, T-112), and give it to `InUnit`. The egress of each unit is the channel to its egress socket, with the policy of `--allow`: the fake broker must be reachable through the egress module (an allow rule for the loopback port of the broker, or a second channel). Exceed act: each job in a fresh guest with its own kernel, no NIC, and one way out.

## Prove

`cargo test -p podrunner run_local` (the argv of each kind) and, in `runner-image.yml`, `podrunner run crates/podrunner/tests/local/suite.yml --microvm ...` gives each `ok` line.

## Done

2026-11-14, `6cbfd5d` and `8a65c48`. `podrunner run --microvm PODVM [--engine DIR]` and `--incus PODVM` build the driver as `serve` builds `[unit.microvm]` and `[unit.incus]` (`run_local/guest.rs`), and `podrunner_local::front` serves the egress socket of each guest: `broker.podrunner.test` goes to the fake broker, each other host to the egress module of `--allow`, or 403. Runner-image run 43557460656 (`5dedd26`): `crates/podrunner/tests/local/suite.yml` gives each `ok` line in fresh microvm guests and in fresh incus instances; a guest reaches no host but the broker (`ok denied`), and with `--allow example.com` the same step reaches the host (`failure denied`).

# T-124: Local mode: services:, docker build and Docker-in-Docker through the engine of the unit

**Source:** The owner's ghactions-local study (act: `services:` on a Docker network with health checks, Dockerfile actions; runner.server: the same through the runner)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

act and runner.server run `services:` containers on a network of the job, with port maps, the service name as a host name, and health checks (`options: --health-cmd`); they build the image of a Dockerfile action. The study's service workflows reach a Redis service by `localhost` and by its name. In the local mode `services:` is refused by the hook policy on a unit of the host network (T-093), and a Dockerfile action is refused (`build-not-supported`, call C2).

## Premise

Read `docs/container-engine.md` and T-103 on 2026-11-14: in a guest of the microvm kind with `engine`, `dockerd` runs with `overlay2`, and a container reaches another by its name on a user network (runner-image run 43524984859). Not measured: a `services:` job of the official runner with the engine as its container route.

## Approach

On the microvm kind with an engine (T-123), give the runner `DOCKER_HOST` of the engine in the guest and no container hooks, so the runner runs `services:`, `container:` and Docker actions with its own Docker calls. Load each image from the store into the engine before the job (no pull inside the guest, T-130). Hold the services by the same egress policy as the job. Exceed act: no Docker socket of the host; each job's engine dies with its guest; images pinned by digest.

## Prove

`runner-image.yml` runs the services and container workflows of the parity suite (T-147) with `podrunner run --microvm ... --engine DIR`: a service answers by `localhost` and by its name, and a Dockerfile action builds and runs.

State (2026-11-14): runner-image run 43570649182 (`e31f9e5`), the parity suite in guests with an engine: 01, 02, 03, 07 and 08 pass. 04, 05 and 09 fail on the pull of busybox in the guest: the egress module denied a host of the blobs of Docker Hub, which now sends them to `production.cloudflare.docker.com` and to its R2 bucket of layers; the step now allows both, beside CloudFront. 06: the job `local` (a checkout, then a `./` composite action) stopped for 10 minutes after "Getting action download info", while `market` of the same file passed; not explained yet. Next: the run of the next commit.

State (2026-11-14, later): the stop of 10 minutes of the job `local` (06) is found, and the same stop failed the job `checkout` of the local suite in a microvm guest (run 43576346151, `2eee435`). The front of a guest rewrites only the first request of a connection to the origin form; a later request of a kept-alive connection comes in the absolute form `http://broker.podrunner.test/PATH`. `read_request_with` of `podrunner-hook` refused a target that does not start with `/`, and the broker closed the connection with no answer; the runner retried it for minutes. Now the parser takes the absolute form, the broker reads `/PATH` from it (and marks it in `seen`), and a refused request gets its status. A failed job of a unit names the hosts that its egress policy denied (`Egress::denials`, the audit of `pod-broker`) and the last requests of its broker. The pull of the guests (04, 05, 09) was still denied with the CDN hosts of today allowed: the next failed log names the host.

State (2026-11-14, later): runner-image run 43594485325 (`dcc266f`) names both causes with the new diagnostics. (1) The egress policy denied `docker-images-prod.s3.dualstack.us-east-1.amazonaws.com:443`: from the hosted runners of GitHub, Docker Hub sends the layers to S3; the step now allows it. (2) The jobs with a checkout in a guest (`checkout` of the suite, `local` of 06) sent no request for 10 minutes after the start, while the listener polled: the stand-in of `actions/checkout` downloaded a tree of more than a gigabyte through the channel of the guest, because the image steps write `runner-x86_64.tar`, `oci/` and `load.tar` into the root of the checkout and git did not ignore them. `.gitignore` now ignores them, and `podrunner run` warns when the tree of `--checkout` is 100 MiB or more. The absolute form of the broker (`dcc266f`) stays: it is right for a kept-alive connection through the front.

State (2026-11-14, later): runner-image run 43603507382 (`ccf7f42`): the unit tests of the microvm kind pass (the tree of the checkout is small now), and in guests with an engine 01 to 08 of the parity suite pass, `services:`, a job container and the actions among them. 09 failed after its image built: the runner names the container of a Docker action `Sanitize(TAG)_RANDOM`, where TAG is `LABEL:STEP-ID` and `Sanitize` leaves out each leading digit; the label of the runner was `804063` and the step ids of the message were `00000000-...`, so the name began with `_`, which docker refuses. The step ids now begin with a letter (`a0000000-...`), as a GUID of GitHub can.

## Done

2026-11-14, `5096a3b`: runner-image run 43609196605 is green. In guests with an engine, 01 to 09 of the parity suite pass: a service answers by `localhost` and by its name, a job container runs, and a Dockerfile action builds and runs. The guest still pulls its images through the egress policy; the load from the store is T-157.


# T-125: Local mode: the event payload, the github context of the local tree, and the filters of on:

**Source:** The owner's ghactions-local study (act: the event argument, `-e/--eventpath`, `--detect-event`, `-a/--actor`, `--defaultbranch`, `--remote-name`, `-W` over a directory; runner.server: `--payload`, `--no-default-payload`, `--ref`, `--sha`, `--repository`)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

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

## Done

2026-11-14, this commit. podcrates `v0.44.0` gives `Workflow::takes(&Trigger)` (GitHub #97, COORD-037). `context::takes` builds the trigger: the event, the activity type of the payload (`--event-path`, or `opened` of the minimal payload of a pull request), the ref of the tree (the base branch for a pull request), the changed files of `--changed-since REF` (`git diff --name-only REF`; none: a `paths` filter does not apply) and the workflow of a `workflow_run` payload. `podrunner run FILE` refuses a workflow that does not take the event after its filters, unless `--force-event`. `podrunner run DIR` (`run_local/many.rs`) runs each workflow of the directory that takes the event, by name, and names each one that does not. Tests: `cargo test -p podrunner run_local` (a `branches` filter that keeps a ref out and lets another one through) and `cargo test -p podrunner many` (a directory with a workflow that runs, another event, a tag filter on a branch, a file that is not a workflow, an empty directory).


# T-126: Local mode: the inputs of workflow_dispatch, and files of vars, env and inputs

**Source:** The owner's ghactions-local study (act: `--var-file .vars`, `--env`, `--env-file .env`, `--input-file .input`; runner.server: `--var-file`, `--env-file`, `--input-file`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

act and runner.server read vars, inputs and an extra environment of each job from flags and from files. The local mode has `--var` and `--input` only. It applies no `default` of `on.workflow_dispatch.inputs`, checks no `type` (`boolean`, `number`, `choice`, `environment`) and no `required`, so an input with a default is empty.

## Premise

Read `parse` of `crates/podrunner/src/run_local.rs` on 2026-11-14.

## Approach

`--var-file FILE`, `--input-file FILE` and `--env-file FILE` (the syntax of `GITHUB_ENV`), and `--env NAME=VALUE` for the environment of each job (as act). The inputs of `workflow_dispatch` get their defaults and their types, as GitHub gives them (a boolean is a boolean in `inputs`, a string in `github.event.inputs`); a missing `required` input or a value outside a `choice` fails the run before the first job.

## Prove

`cargo test -p podrunner run_local`

## Done

2026-11-14, this commit. `--var-file`, `--input-file` and `--env-file` read the syntax of `GITHUB_ENV` (`secrets::parse_pairs`), and each flag of the same name wins; `--env NAME=VALUE` gives the environment of each job, under the `env:` of the workflow (`LocalSource::set_env`, the first mapping of `environmentVariables`). With `workflow_dispatch`, `context::dispatch_inputs` gives each input its default and its type (boolean, number, choice, string); a missing `required` input, a value outside a `choice`, a value of the wrong type and an input that `on:` does not declare fail the run before the first job; `github.event.inputs` has the strings. Tests: the typed inputs and each refusal, the files under the flags, the env in the job message; a run here on runner v2.338.2 reads `inputs.debug` as `true`, `inputs.count == 3`, `github.event.inputs.count` and `$E` of `--env`.


# T-127: Local mode: run one job (--job) or some cells of a matrix (--matrix)

**Source:** The owner's ghactions-local study (act: `-j JOB`, `--matrix KEY:VALUE`; runner.server: `--job`, `--matrix`)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

act and runner.server run one job of a workflow, and only the cells of a matrix that match a filter. `podrunner run` runs each job and each cell.

## Premise

Read `drive` of `crates/podrunner-local/src/drive.rs` on 2026-11-14. `Graph::ancestors` of `pod-actions-workflow` gives the needs of a job and their needs.

## Approach

`--job ID` (repeat it) runs the job and each job that it needs; the other jobs end as skipped with the reason "not selected". `--matrix KEY=VALUE` (repeat it) keeps the cells whose matrix has each value. Exceed: with a run directory (T-135), `--job ID --from DIR` takes the results and outputs of its needs from the earlier run and runs only the job.

## Prove

`cargo test -p podrunner-local` (a job with its ancestors, a matrix filter).

## Done

2026-11-14, this commit. `LocalSource::set_filter` keeps the jobs of `--job ID` with their ancestors (`Graph::ancestors`) and the cells of `--matrix KEY=VALUE` (a cell with no such key stays); a job that the workflow does not have is refused by name. The drive loop ends each other cell as skipped (`Line::NotSelected`, `skipped NAME: not selected ...`). Test: a job with its need and a matrix filter (three cells run, three are kept out). The run of one job with the results of an earlier run is the `--rerun DIR --job ID` of T-135.


# T-128: Local mode: --list, --graph, --dry-run and --validate

**Source:** The owner's ghactions-local study (act: `-l/--list`, `-g/--graph`, `-n/--dryrun`, `--validate`, `--strict`, `--list-options`; runner.server: `--list`, `--list-options`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

act lists the jobs of the workflows with their stages and events, draws the graph of `needs`, validates the files, and plans a run with no container. The local mode has none of these: a fault (an action that is not pinned, an image not in the store) shows only when its job starts.

## Premise

Read `crates/podrunner/src/run_local.rs` on 2026-11-14. `plan` and `Graph` of `pod-actions-workflow` give a dry run and the edges.

## Approach

`--list` prints each job, its cells, its `runs-on` and its needs. `--graph` prints the stages of `needs` as text. `--validate` parses and prints each warning of `pod-actions-workflow`, exit 78 on an error. `--dry-run` runs `plan` and builds the job message of each cell with no unit, so that each refusal (an unpinned action or image, a reusable workflow) and each missing action, image or lock entry shows before a job starts. Exceed act: the dry run checks the pins and the store.

## Prove

`cargo test -p podrunner run_local`

## Done

2026-11-14, this commit. `crates/podrunner/src/run_local/inspect.rs`: `--list` prints each job with its name, `runs-on` and needs; `--graph` prints the stages of `needs` (each job one stage after its last need) and each edge; `--validate` prints each warning of `pod-actions-workflow`, and an error has the class of a config fault; none of them needs an executor. `--dry-run` resolves the pins (with `--offline` or `--frozen` it changes nothing) and drives the run with an executor that runs no unit: each job message is built, a job that would start is `planned`, and each refusal shows. Test: the four modes on one workflow, with an image by tag that the dry run refuses with no unit kind.


# T-129: Local mode: runner images by the labels of runs-on (--platform)

**Source:** The owner's ghactions-local study (act: `-P LABEL=IMAGE`, the micro, medium and large images; runner.server: `-P LABEL=IMAGE` and `-P LABEL=-self-hosted`)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

act maps each label of `runs-on` to an image (`-P ubuntu-latest=IMAGE`), and offers images with the tools of the hosted runners. runner.server maps a label to an image or to the host. `podrunner run` takes one `--image` for each job and does not read `runs-on`. The runner image of podrunner has the runner, `bash` and `curl`, so a step that needs a tool of `ubuntu-latest` (a compiler, `jq`, `docker` CLI, `node` outside the runner) fails.

## Premise

Read `crates/podrunner/src/run_local.rs` and `crates/podrunner-local/src/lib.rs` (the offer carries the labels) on 2026-11-14.

## Approach

`--platform LABEL=NAME@sha256:HEX` (repeat it) and `--platform LABEL=host` (the `--runner-dir` executor). Each cell takes the image of the first label that matches; `--image` is the default. A cell whose labels match nothing fails by name (GitHub would queue it for ever). `windows-*` and `macos-*` fail by name. Add a recipe of a larger runner image with the common tools of `ubuntu-latest`, built by the image pipeline (T-030) and pinned by digest. Exceed act: each image pinned by digest; a label never falls back to an image that the user did not name.

## Prove

`cargo test -p podrunner run_local` (the choice of an image by label, a label with no image).

State (2026-11-14): `--platform LABEL=NAME@sha256:HEX` and `--platform LABEL=host` (repeat it); `podrunner_local::dispatch::Dispatch` gives each job the route of its first label that has one, else the default (`--image`, else `--runner-dir`), through `Executor::select`; a job with no route and no default, or with a label of Windows or macOS that no route names, fails by name before a runner starts. Each route of a unit kind has its own state directory. Tests: the routes of `Dispatch`, the parse of `--platform`, a Windows job refused by `podrunner run`; a run here gives `ok` for `runs-on: [self-hosted, gpu]` with `--platform gpu=host`. Next: the recipe of a larger runner image with the common tools of `ubuntu-latest`, built by the image pipeline (T-030) and pinned by digest.

State (2026-11-14, later): the recipe of the larger image is the base `images/runner/Dockerfile` with `--build-arg EXTRA_PACKAGES="$(cat images/runner/full.txt)"` (a C and C++ tool chain, CMake, Python 3 with pip and venv, Node.js with npm, zip, xz, bzip2, rsync, an SSH client, wget, file, patch): one recipe, so the two images keep the same runner and the same user. `runner-image.yml` builds it on x86_64 as a measure, prints the size of its archive, loads it into the store and runs a job of `runs-on: ubuntu-latest` with `--platform ubuntu-latest=runner@DIGEST` that calls each tool. Next: read the measure, then publish the larger image with the base one (T-030) and name its digest in `docs/local-mode.md`.

State (2026-11-14, later): runner-image run 43609196605 (`5096a3b`): the larger image is 462 MiB (the archive), and the job of `runs-on: ubuntu-latest` gives `ok tools`. The step is now a check, and it uploads the archive for `podvm load -i` as the artifact `runner-full-x86_64` with its sha256, beside `runner-x86_64`; `docs/local-mode.md` names it. The larger image is x86_64 only. Next: the green run of the upload, then close this entry.

State (2026-11-14, later): runner-image run 43623211536 (`94fddee`): the larger image passes as a check; its upload was skipped, because a failed step before it made `success()` false. The upload now runs after `!cancelled()` when the step passed. Next: the run with the artifact.

## Done

2026-11-14, `47c0daa`: runner-image run 43626860120: the step "The larger runner image by label" passes as a check (a job of `runs-on: ubuntu-latest` with `--platform ubuntu-latest=runner@DIGEST` runs python3, node, make, cmake and zip), and the run has the artifact `runner-full-x86_64` (459 MiB) for `podvm load -i`. `cargo test -p podrunner run_local` covers the choice of an image by label.


# T-130: Local mode: container images by tag, resolved to a digest and pulled into the store

**Source:** The owner's ghactions-local study (act: `--pull`, `--container-architecture`; runner.server: the pull of the runner)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

act pulls each image of `container:`, `services:` and `docker://` by its tag. The local mode takes only `NAME@sha256:HEX` that is in the store already (`pinned` of `crates/podrunner-local/src/message.rs`): a workflow with `image: redis:7-alpine` fails, and the user must pull and import each image by hand.

## Premise

Read `crates/podrunner-local/src/message.rs` on 2026-11-14. `pod-oci` of podcrates is a registry client.

## Approach

Before the first job, resolve each image tag to a digest for the platform of the unit through the egress module (the registry hosts only), pull it into the store, and record `NAME:TAG` and the digest in the lock file of T-118. The message gives the runner the digest, not the tag. `--offline` uses the lock only. Exceed act: an image is pinned by the lock; a moved tag fails by name; the units never pull.

## Prove

`cargo test -p podrunner-local` (the lock of images, `--offline` with a missing image).

State (2026-11-14): `crates/podrunner-local/src/images.rs` collects each image of `container:`, `services:` and `docker://` steps that a tag names (`NAME` alone is `NAME:latest`; an expression is refused), pins it by the lock (`docker://NAME:TAG sha256:HEX`), and pulls it with `podvm pull --platform` into the store of the units (`PodvmPull`); the job message gives `NAME@sha256:HEX` (`pin_image`, `pin_node`). A moved tag fails by name; `--offline` and `--frozen` hold. Not `pod-oci`: podvm pulls into its own store, so the host runs no second registry client. Tests: the images of a workflow, the lock and a moved tag with a fake source, the pinned message; `podvm pull` of `busybox:1.37.0` here gives `sha256:a7dbc319…`, the digest of the registry. Next: the run of `crates/podrunner/tests/local/tags.yml` with `--podvm` in `runner-image.yml`. The pull goes through the proxy of the host, not the egress module (as T-154 for the actions).
Runner-image run 43560211006 (`034fbfb`): the pull and the pin work (the lock has `docker://busybox:1.37.0`), and the container fails: busybox is pinned by the digest of its index, `podvm save` keeps its platform manifest, and `podvm run` of the index digest in the unit store finds no image. The podvm driver now runs a container by `NAME@` the manifest digest of the `index.json` of the saved tar (`loaded_reference`), so an image that a user pins by its index works too. Runner-image run 43562100819 (`cb31ca7`): the container still fails with `container-not-supported`; the refusal carries the code only, so the hook unit now keeps the text of each driver error (`driver_errors`), and the log of a local run names it.
Runner-image run 43565195597 (`872b43f`): the log names the error: `podvm run` of `busybox:1.37.0` (glibc) fails where chroot is denied, because the image has no `libdl.so.2` (T-156, COORD-040). `tags.yml` now uses `busybox:1.37.0-musl`, a static binary. The same run skipped the setup of KVM and QEMU after the failed podvm step, so the engine checks found no `qemu-system-x86_64`: the setup is now a step of its own before the podvm step.
Runner-image run 43570649182 (`e31f9e5`): `busybox:1.37.0-musl` fails too: podvm routes a static binary to its memfd family, which has no detached drive (`run -d`), and the podvm driver runs each container detached. Both cases went to podvm on GitHub #100. `tags.yml` takes `busybox:1.37.0` again, and its check is a step of its own, a measure until T-156: the failed check had skipped the rerun, egress and tar checks after it in the podvm step.

## Done

2026-11-14, `c007cce`: runner-image run 43610257672 is green. `tags.yml` names `busybox:1.37.0` by tag: podrunner pins it by the lock, `podvm pull` puts it into the store, and the job container runs in a podvm unit (the step "An image by tag in a podvm unit", a check). `cargo test -p podrunner-local` covers the lock and `--offline`.


# T-131: Local mode: reusable workflows (jobs.<id>.uses)

**Source:** The owner's ghactions-local study (act: local and remote reusable workflows; runner.server: reusable workflows with `secrets: inherit` and nested calls)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** L
**Status:** done

## Problem

A job can call a workflow (`uses: ./.github/workflows/x.yml` or `OWNER/REPO/.github/workflows/x.yml@REF`) with `with:` and `secrets:` (or `secrets: inherit`); the jobs of the called workflow run, and its `outputs` become `needs.<id>.outputs` of the caller. act and runner.server run both forms. The local mode refuses the job by name (`broker_job` of `crates/podrunner-local/src/message.rs`). `pod-actions-workflow` names the call and does not expand it.

## Premise

Read `crates/podrunner-local/src/message.rs` and the model of `pod-actions-workflow` (`Job::uses`, `Job::secrets`) on 2026-11-14.

## Approach

Expand the called workflow into jobs of the run (ids `CALLER/CALLEE`), with the inputs of `on.workflow_call` (defaults and types, T-126), the secrets that the call passes, `github` of the caller, and the outputs of `on.workflow_call.outputs`. A remote workflow comes from the store and the lock of T-118. Keep the nesting limit of GitHub. The expansion is generic: ask podcrates for it in `pod-actions-workflow` by a COORD issue. Exceed: each called job is still one fresh unit; a secret goes only to the called workflow that the call names.

## Prove

`cargo test -p podrunner-local` (a local call with inputs, secrets and outputs; `secrets: inherit`; a nested call) and `PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner --test local -- --ignored`.

## Done

2026-11-14, this commit. podcrates `v0.44.0` gives `Run::with_called` (GitHub #98, COORD-038; taken at `8ef1730`, COORD-041). `crates/podrunner-local/src/calls.rs`: `Texts` gives the text of each called workflow (a `./` path of the tree, or `OWNER/REPO/PATH@REF` from the tarball of its pinned commit in the store; a `./` call inside a remote workflow reads that repository); `Calls` keeps the workflow and the id of each called job, so that its job message, its permissions, its environment and its concurrency come from the called workflow; `cell_id` maps `CALLER/CALLEE` and `CALLER[N]/CALLEE` to an offer id (`CALLER__CALLEE.0`, `CALLER-N__CALLEE.0`); `values` gives the secrets that a call passes (its templates over the secrets of the caller), or each secret of the run. `LocalSource::new_with` sets the texts before the first jobs expand. `podrunner run` pins the repository of each remote `jobs.ID.uses` of the workflow (a tree, T-133) before it builds the source; a remote call in a called workflow needs a line of the lock already. Tests: `cargo test -p podrunner-local calls` (a local call with an input, a passed secret and an output to the job after it; the ids; the secrets of `inherit` and of a template; a remote call with no pin) and `cargo test -p podrunner a_call_of` (a dry run plans `ci / build`, `ci / test`, then `after`).


# T-132: Local mode: jobs in parallel (--jobs N, max-parallel)

**Source:** The owner's ghactions-local study (act: `--concurrent-jobs`, the matrix in parallel; runner.server: `--parallel N`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

act runs the ready jobs and the cells of a matrix at the same time, up to the number of CPUs; runner.server starts N runners. `drive` of `crates/podrunner-local/src/drive.rs` runs one job at a time, so a matrix of eight cells takes eight times as long.

## Premise

Read `drive` and the executors on 2026-11-14. Each executor holds one broker for each job; the source takes many offers.

## Approach

`--jobs N` (default: the number of CPUs, at most the memory of the host divided by the memory of a unit): run up to N offers at a time, each in its own unit and broker, and honour `strategy.max-parallel` for the cells of a job. Print each line when its job ends. With fail-fast (T-117), cancel the running cells of the job (the cancel of the runner, then destroy). Exceed act: the units of parallel jobs share no file, port or daemon.

## Prove

`cargo test -p podrunner-local drive` (N at a time, `max-parallel`, the cancel of the running cells).

State (2026-11-14): `drive_parallel` (`crates/podrunner-local/src/drive/parallel.rs`) runs the queued jobs on a slot each, each slot an executor on its own thread; the source stays on the caller's thread (`settle`, `prepare` and `finish` of `drive.rs` serve both loops), so each line comes when its job ends and `needs` holds; `max-parallel` (from `strategy` of the cell) holds the running cells of a job; a panic of an executor fails its job. `Executor` is `Send`, and the count of units is one for the process, so two slots never share a unit id, a socket directory or an egress grant. `podrunner run --jobs N` builds N slots (`run_local/slots.rs`), each with its own state directory; `--runner-dir` refuses it. The default stays one job at a time (a call in `docs/decisions.md`). Tests: `cargo test -p podrunner-local parallel` (3 at a time and `needs` last; `max-parallel` 2 with 4 slots; fail-fast cancels the cells that did not start) and `cargo test -p podrunner slots`; `runner-image.yml` runs the local suite with `--jobs 3` in podvm units. 

## Done

2026-11-14, this commit. The rest: `Executor::set_stop` gives each running job a stop flag (`cancel::Stop`); `InUnit` and `RunnerDir` end the job at the flag as at a signal (`cancel::stopped`), and `Dispatch` and `Grouped` pass it on. After each end, `drive_parallel` sets the flag of each running cell that `fail-fast` cancelled in the source, and gives such a cell the line `cancelled` with no second `complete`. Test: `cargo test -p podrunner-local parallel` (fail-fast stops two running cells within 4 s; five cells are `cancelled`).


# T-133: Local mode: the checkout with the changes of the tree, the .git directory, and other repositories

**Source:** The owner's ghactions-local study (act: the copy of the working tree, `--use-gitignore`, `--no-skip-checkout`; runner.server: the local checkout of the changes, `--no-copy-git-dir`, `--remote-checkout`)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

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

## Done

2026-11-14, this commit. `crates/podrunner-local/src/tree.rs` writes the tree of `--checkout`: in a git tree each tracked file and each new file that git does not ignore, at its state on disk (`git ls-files --cached --others --exclude-standard`); `--checkout-head` gives `git archive` of HEAD; `--with-git` adds a `.git` from a clone of the tree with no checkout and an index of HEAD, so the unit never writes the `.git` of the user. A checkout with `with: repository:` of another repository (plain values; HEAD of its default branch with no `ref`) is resolved and pinned by the lock of T-118 and kept in the store (`store::checkout_target`, `workflow_trees`, `add_trees`); the stand-in unpacks that tarball from the broker with its top directory stripped. Proven: the unit tests of the tree (a change on disk and a new file are in, an ignored file is out, HEAD only, a `.git` where `git rev-parse HEAD` and `git status` work), of the checkout targets and of the job message; `cargo test -p podrunner --test local -- --ignored the_checkout` on runner v2.338.2: `git rev-parse HEAD` equals `github.sha` in a step, and `actions/upload-pages-artifact@v3` lands in `other/` at its pinned commit. The real `actions/checkout` through the egress module (`--remote-checkout`) is not taken: the store gives the same tree with a pin.


# T-134: Local mode: live logs, a log directory, the job summary, and --json

**Source:** The owner's ghactions-local study (act: the live output of each step with the job prefix, `--json`, `-q`, `--log-prefix-job-id`; runner.server: the live web console, `--log-output-dir`)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

act prints the output of each step when it comes, with the name of its job. runner.server shows live logs and writes them to `--log-output-dir`. `podrunner run` prints one line when a job ends, and the logs of the failed jobs at the end of the run (`run` of `crates/podrunner/src/run_local.rs`). The broker keeps each blob (the logs and the job summary of `GITHUB_STEP_SUMMARY`) in one text (`Broker::logs`), so the summary mixes with the logs.

## Premise

Read `crates/podrunner-broker-fake/src/lib.rs` and `crates/podrunner/src/run_local.rs` on 2026-11-14. Not measured: whether runner v2.338.2 uploads a step log in parts during the step (live) or at its end.

## Approach

Print each upload of a log as it comes, each line with the job prefix; `-q` prints only the lines of the jobs; `--json` prints one JSON object for each event (a job start, a step, a log line, an end). `--log-dir DIR` writes the log of each job and step, and `summary.md` of each job. Mask each secret again before podrunner writes a line (T-121).

## Prove

`cargo test -p podrunner-broker-fake` (the blobs by kind) and `cargo test -p podrunner run_local` (`--json` lines, `--log-dir`).

## Done

2026-11-14, this commit. The fake broker keeps a summary of `GITHUB_STEP_SUMMARY` as a blob of its own (`summary-ID`), leaves it out of `logs()`, and gives each blob (`blobs()`) and the text that each log blob got since the last call (`take_new()`). The executors (`exec/host.rs` holds `RunnerDir` now) give each new part to a live sink while the job runs and keep the blobs of each job (`Executor::set_live`, `take_blobs`). `podrunner run` prints each live line on stderr with the job as its prefix (`-q` turns it off), `--json` writes one JSON object for each log line and each end of a job, and `--log-dir DIR` writes `JOB/step-ID.log`, `JOB/job.log` and `JOB/summary.md`; each secret is masked again. The console of the runner goes to stderr, so stdout holds only the lines of the jobs. Tests: the blobs by kind and the new text once, the mask, the JSON line and the files of `--log-dir`; a run here on runner v2.338.2 gives the live line `live-42 the key ***`, a `summary.md` with the summary of the step, and no secret in a file.


# T-135: Local mode: a run directory and reruns of failed jobs

**Source:** The owner's ghactions-local study (runner.server: `rerun` with `--failed`, `--jobid`, `--resetArtifacts`, `--refresh`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

runner.server can rerun a finished run: all failed jobs, or one job, and keep the artifacts and the results of the jobs that passed. act cannot. `podrunner run` keeps no state after the run: a failure in the last job of a long workflow needs a run of each job again.

## Premise

Read `crates/podrunner/src/run_local.rs` on 2026-11-14: the scratch directories are removed at the end of the run.

## Approach

`--state DIR` keeps the run: the result, the outputs and the log of each job, the artifacts (T-119), and the workflow and inputs with their digest. `podrunner run --rerun DIR [--failed | --job ID]` runs the failed jobs (or the job) and each job after them, with the kept results and outputs of the others. A rerun of a changed workflow fails by name unless `--refresh`. Each file of DIR is written by podrunner as its user, never by a unit (see T-146). Exceed: each rerun job gets a fresh unit, so a rerun does not depend on a dirty work tree.

## Prove

`cargo test -p podrunner-local` (a rerun keeps the outputs of a good job and runs the failed one and its dependents).

## Done

2026-11-14, this commit. `crates/podrunner-local/src/rerun.rs`: the source records the end of each cell (`ends`), and a rerun gives each cell that it does not run its kept result and outputs (`set_kept`; the drive loop prints `kept NAME (result)` and runs no unit). `rerun_jobs` takes the jobs with a failed or cancelled cell (or the jobs of `--job`), each job that the state does not know, and each job after one of them. `podrunner run --state DIR` writes `DIR/state.json` (the digest of the workflow and each end), the logs in `DIR/logs` and the artifacts in `DIR/artifacts`; `--rerun DIR [--failed | --job ID]` reads it, refuses a changed workflow unless `--refresh`, and writes the new state; the artifact store of a rerun replaces an artifact of the same name (`ArtifactStore::replacing`). Each file of DIR is written by podrunner. Tests: the rerun set and the state file, a changed workflow, the flags; a run here on runner v2.338.2: a run with a failed job `b`, then `--rerun` with a fix gives `kept a (success)`, `ok b`, `ok c`, and `c` reads the kept output of `a`.


# T-136: Local mode: environments, with their secrets, vars and egress policy

**Source:** The owner's ghactions-local study (runner.server: `environment:` with `--environment-secret`, `--environment-secret-file`, `--environment-var`, `--environment-var-file`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

A job with `environment: NAME` (or `{name, url}`) gets the secrets and vars of that environment, and waits for its protection rules. runner.server takes the secrets and vars of each environment from flags and files. The local mode ignores `environment:`.

## Premise

Read `crates/podrunner-local/src/message.rs` on 2026-11-14. The model of `pod-actions-workflow` keeps `environment`.

## Approach

Evaluate `environment` (name and `url`). `--environment-secret-file NAME=FILE` and `--environment-var-file NAME=FILE` (the syntax of T-121) give its secrets and vars only to the jobs of that environment. `--approve NAME` passes its protection; with no flag, a job of a protected environment asks on the terminal, or fails when stdin is not a terminal. Exceed runner.server: `--environment-allow NAME=HOST` gives the jobs of an environment their own egress policy (a deploy job reaches its target; a test job does not).

## Prove

`cargo test -p podrunner-local message` (the secrets of an environment reach only its jobs).

State (2026-11-14): `LocalSource::environment_of` reads `environment: NAME` or its `name:` (an expression is refused when a flag gives an environment); `set_environments` gives the secrets of `--environment-secret-file NAME=FILE` (mode 0600) and the vars of `--environment-var-file NAME=FILE` (over those of the run) only to the jobs of that environment; `--environment-allow NAME=HOST` gives those jobs their own egress policy on a unit kind that holds egress (`Executor::environment`, `InUnit::environment_policies`). Tests: the secrets and vars of an environment in its job and not in another, the flags. Next: the protection of an environment (`--approve NAME`, a prompt on a terminal): GitHub keeps which environment is protected in its settings, so the local mode needs a rule for it first, for example `--protected NAME`.

## Done

2026-11-14, this commit. `--protected NAME` marks an environment as protected; `--approve NAME` passes it; with no flag, `podrunner run` asks on the terminal before the first job, for each protected environment that a selected job names (`crates/podrunner/src/run_local/protect.rs`). With no terminal, or a refusal, `Environment::refused` holds the reason and `broker_job` refuses each job of that environment by name; the other jobs run. A dry run asks nothing. Tests: `cargo test -p podrunner protect` (an approved, an unused, a refused and an unselected environment) and `cargo test -p podrunner-local message` (a held environment refuses its job, not another).


# T-137: Local mode: concurrency groups and cancel-in-progress

**Source:** The owner's ghactions-local study (runner.server: `concurrency` of the workflow and of the job, with `cancel-in-progress`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

GitHub runs at most one job or run of a concurrency group at a time, and cancels the older one with `cancel-in-progress: true`. runner.server follows it. act does not. The local mode ignores `concurrency`; two parallel jobs of one group (T-132) or two runs on one host can run at once.

## Premise

Read `crates/podrunner-local/src/lib.rs` on 2026-11-14. The model of `pod-actions-workflow` keeps `concurrency` of the workflow and of each job.

## Approach

Evaluate the group of each job and of the run. In a run, a job waits while another job of its group runs. Across runs on one host, a lock file of the group in the state directory of the user holds it; with `cancel-in-progress`, the new run cancels the old one (the cancel of the runner, then destroy). The group of GitHub is per repository: name it with `github.repository`.

## Prove

`cargo test -p podrunner-local` (two jobs of one group take turns; a cancel of the older run).

## Done

2026-11-14, this commit. `crates/podrunner-local/src/concurrency.rs` evaluates the `concurrency` of the workflow and of each job (a string, or `group:` and `cancel-in-progress:`) with `github`, `inputs`, `vars` and the `matrix` of a cell. `crates/podrunner/src/run_local/concurrency.rs` holds each group across runs by `flock` on a lock file in the cache directory of the user, named by the repository and the group: the run group for the whole run, the group of a job while it runs (`Grouped`, through `Executor::concurrency`). A second run of a held group waits; with `cancel-in-progress` it sends SIGTERM to the pid in the lock file, and that run cancels as at Ctrl-C (T-141). A dry run holds no group. Within one run the jobs run one at a time, so a group holds there with no wait. Tests: the groups with their contexts, a second holder that waits; a run here: a second run of a group with `cancel-in-progress` cancels the first (`cancelled long`, exit 130) and then runs.


# T-138: Local mode: keep the unit of a failed job, export the workspace, and a shared tool cache

**Source:** The owner's ghactions-local study (act: `-r/--reuse`, `-b/--bind`, `--rm`; runner.server: `--keep-container`, `--keep-runner-directory`, `--no-shared-toolcache`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

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

State (2026-11-14): `--export DIR` with `--runner-dir`: after each job, also a failed one, `RunnerDir` copies each workspace of `_work` (each entry whose name does not start with `_`) to `DIR/JOB` (`crates/podrunner-local/src/work.rs` `export`); the modes and the symbolic links stay; a failure of the copy goes into the log of the job. A unit kind refuses `--export`. Tests: `cargo test -p podrunner-local export`. Next: the unit kinds. No driver can copy a file out of a unit or mount a host directory into it, and `podvm run -v` can: add the mounts to `StartSpec` (`podrunner-unit`), with `-v` in the podvm and microvm drivers. Then `--export DIR` mounts `DIR/JOB` at the work directory of the runner, `--toolcache DIR` mounts DIR read-only at a second path that `RUNNER_TOOL_CACHE` does not name (a read-only mount at the tool cache path breaks `setup-*`; a copy of the tools into the cache path at the start of the job is the measure), and `--keep-failed` skips `destroy` of a failed unit and prints its id.

State (2026-11-14, later): `StartSpec::mounts` (host directories, read-write); the podvm driver passes each one as `podvm run -v`; the microvm and incus kinds refuse a start with one, because the self-check of the guest (`MOUNT_CHECK`) refuses a mount that it does not know. `InUnit` with `export` mounts a scratch work directory (mode 0777) at `_work` of the runner root, and after the job copies its workspaces to `DIR/JOB` with `work::export`, also for a failed job; the scratch goes. `--export` now takes `--podvm` and refuses `--microvm` and `--incus`. Tests: `cargo test -p podrunner-local exec` (the export of a failed job, the scratch goes); `runner-image.yml` measures a job that writes `out/app` in a podvm unit ("The export of a workspace from a podvm unit"). Next: `--toolcache DIR` and `--keep-failed`; a mount in a guest needs a change of its self-check.

State (2026-11-14, later): `--toolcache DIR` with `--podvm`: `InUnit` mounts DIR read-only at `/podrunner-toolcache` (a mount target `PATH:ro` of `StartSpec`) and starts the runner by `toolcache_argv`: `cp -as` makes `_work/_tool` a tree of links to the files of the mount, so a `setup-*` action finds each tool and adds a new one beside them. A guest and `--runner-dir` refuse it. Tests: `cargo test -p podrunner-local tool_cache`; runner-image run 43615355943 (`3ce59bc`): in a podvm unit the job runs a tool of the cache and adds a new one, and the host directory stays as it was; the step "A tool cache in a podvm unit" is now a check. Next: `--keep-failed`. `--keep-failed` and the shell of T-150 need one command to enter a unit: no driver gives one yet.

State (2026-11-14, later): `--keep-failed` is in: it is `--break-on-failure` (`docs/decisions.md`), and the pause gives a shell in the unit (T-150). Next: the green runner-image run of the shell (`links.rs`), then close this entry.

## Done

2026-11-14, `aeac0a3`: runner-image run 43628702784 is green. `--export DIR` (the step "The export of a workspace from a podvm unit") and `--toolcache DIR` (the step "A tool cache in a podvm unit", a check) pass in podvm units; `--keep-failed` is `--break-on-failure` (`docs/decisions.md`), and the pause enters the unit (T-150): `links.rs` runs a command of `UnitDriver::enter` in a running podvm unit. `cargo test -p podrunner-local exec` covers the export and the tool cache. A guest of `--microvm` or `--incus` takes no host directory.


# T-139: Local mode: actions from a local directory (--local-repository) and from another forge

**Source:** The owner's ghactions-local study (act: `--local-repository REPO@REF=DIR`, `--github-instance`, `--replace-ghe-action-with-github-com`; runner.server: `--local-repository`, `--git-server-url`, `--git-tarball-url`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

act and runner.server can serve `uses: OWNER/REPO@REF` from a local directory, for the work on an action, and can take actions from a GitHub Enterprise Server or another forge. The local mode takes archives by commit from `--actions DIR` only.

## Premise

Read `crates/podrunner-broker-fake/src/actions.rs` on 2026-11-14.

## Approach

`--local-repository OWNER/REPO@REF=DIR`: the broker resolves that reference to a made-up commit and serves a tarball of DIR made at the start of the run (the files of the tree, with one top directory). `--action-server URL` (default `https://github.com`) is the forge that T-118 asks; a forge of the type of Gitea or Forgejo takes its own tarball URL form.

## Prove

`cargo test -p podrunner-broker-fake` (a local directory served as an action).

## Done

2026-11-14, this commit. `--local-repository OWNER/REPO@REF=DIR` (repeat it): the forge `Local` of `crates/podrunner-local/src/localrepo.rs` serves the tar of DIR (one top directory; in a git tree the tracked and new files that git does not ignore) with a made-up commit, the first 40 hex digits of its SHA-256; `fetch` of the store puts it in the store and keeps it out of the lock (`Forge::is_local`), so `--offline` and `--frozen` take it and the forge is not asked. A composite action in DIR names its own `uses:`, which resolve as usual. `--action-server URL` replaces the base of `Git`: `git ls-remote` and a fetch work on GitHub Enterprise Server, Gitea and Forgejo, so no other tarball URL form is necessary. Tests: `cargo test -p podrunner-local localrepo` (the action in the store, not in the lock; a change gives a new commit; the flag syntax) and `cargo test -p podrunner a_local_repository` (a dry run with `--offline` plans the job and writes no lock).


# T-140: Local mode: OIDC id-tokens (permissions: id-token: write)

**Source:** The owner's ghactions-local study (runner.server: the OIDC controller, `.well-known/openid-configuration` and `jwks`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

A job with `permissions: id-token: write` gets `ACTIONS_ID_TOKEN_REQUEST_URL` and a token to ask for a signed JWT (for a cloud login or a signature). runner.server issues such tokens and serves the keys. The local mode does not: the runner sets the variable from `GenerateIdTokenUrl` in the endpoint data, which the fake broker does not give.

## Premise

Read the runner's handlers in the runner.server tree of the study and `crates/podrunner-broker-fake/src/message.rs` on 2026-11-14.

## Approach

The broker gives `GenerateIdTokenUrl` to a job with that permission, and signs a JWT with the claims of GitHub (`sub` of `repo:OWNER/REPO:ref:REF`, `aud` from the request, `repository`, `ref`, `sha`, `job_workflow_ref`) with a key of the run, which stays in memory. `--oidc-issuer URL` names the issuer, and `--oidc-keys FILE` writes the public keys for a relying party. Exceed: one key for each run, never on disk.

## Prove

`cargo test -p podrunner-broker-fake` (the claims and the signature of a token).

## Done

2026-11-14, this commit. `crates/podrunner-broker-fake/src/oidc.rs`: `Issuer` makes one P-256 key for each run (ring; ring makes no RSA key, and ES256 is a JWS algorithm that relying parties take), signs each token with ES256 and serves the token route (`/_oidc/token?api-version=2.0&audience=AUD`, the answer `{"value": JWT}`), the discovery document and the JWK set. The key never goes to disk. `crates/podrunner-local/src/oidc.rs`: `may_ask` reads `id-token: write` or `write-all` (no `permissions` gives no token, as on GitHub), and `claims` gives the claims of GitHub from the `github` context (`sub` with the environment or the pull request, `job_workflow_ref`, `repository`, `ref`, `sha` and more). The broker job carries the claims; the message gives `GenerateIdTokenUrl` in the data of the system endpoint only then. `--oidc-issuer URL` and `--oidc-keys FILE` in `podrunner run`. Tests: `cargo test -p podrunner-broker-fake oidc` (the claims, the audience, the signature against the JWK set, no token with no permission), `cargo test -p podrunner-local` (the permissions, the claims, the URL in the message of one job and not of another), and `cargo test -p podrunner --test local -- --ignored` in level 2 (the official runner gets a token with curl; a job with no permission has no URL).


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
**Status:** done

## Problem

act and runner.server run the workflow again when a file of the tree changes; runner.server also runs `on: schedule` at its cron times. The local mode runs once.

## Premise

Read `crates/podrunner/src/run_local.rs` on 2026-11-14.

## Approach

`--watch` runs again after a change of the tree (inotify, with a short delay to group changes), and cancels the run in progress (T-137). `--schedule` waits for the next cron time of `on.schedule` (UTC, as GitHub) and runs with the event `schedule`; a systemd timer of the user can do the same with `--event schedule`.

## Prove

`cargo test -p podrunner run_local` (the next time of a cron expression; a change starts a run).

## Done

2026-11-14, this commit. the cron of `podrunner-local` (removed by T-423): `Cron::parse` takes the five fields with values, ranges, steps, lists and the names of months and days (7 is Sunday; both day fields restricted match either, as POSIX cron); `next` gives the first matching minute after a time, in UTC, with no new dependency (`civil` is the civil date of a day). `crates/podrunner/src/run_local/watch.rs`: `--watch` polls a stamp of the tree (`tree::stamp`: the names, sizes and times of the files of the tarball) and of the workflow file every 500 ms, waits until a change rests, and runs again; a change during a run calls `cancel::request`, so the run cancels as at a signal (T-141), and `cancel::reset` clears it for the next run. `--schedule` waits for the next cron time and runs with `--event schedule`. SIGINT or SIGTERM ends the loop. Tests: `cargo test -p podrunner-local cron` (the next times, a leap day, both day fields, the bad fields) and `cargo test -p podrunner watch` (the crons of `on.schedule`, the line of a bad cron, the stamp of a change). Checked by hand: a dry run with `--watch` runs once, once again after a new file, and ends at SIGINT; `--schedule` names the next time and ends at SIGINT.


# T-143: Local mode: a configuration file for podrunner run

**Source:** The owner's ghactions-local study (act: `.actrc` in the configuration directory, the home and the tree, with `--trust-project-actrc`; runner.server: reads `.actrc`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

act reads its flags from `.actrc` files; a file of the project may only set flags that do not reach host resources unless the user trusts it. runner.server reads `.actrc` too. Each `podrunner run` needs each flag on the command line.

## Premise

Read act's `cmd/actrc.go` of the study on 2026-11-14.

## Approach

A TOML file of the user (`podrunner/run.toml` in the configuration directory) and of the project (`.podrunner/run.toml`), with each unknown key refused. The project file cannot set a key that reaches the host or the network (`podvm`, `images`, `allow`, `secret-file` outside the tree, `toolcache`) unless the user file names the project as trusted. Read the flags of `.actrc` that map to podrunner (`-P`, `--secret-file`, `--var`, `--env`, `--input`) and name each one that does not.

## Prove

`cargo test -p podrunner run_local` (the order of the files; a project file that sets `allow` is refused).

## Done

2026-11-14, this commit. `crates/podrunner/src/run_config.rs`: `expand` puts the flags of `~/.actrc`, of `podrunner/run.toml` in the configuration directory of the user, of `.actrc` of the project and of `.podrunner/run.toml` of the project before the flags of the command line (the workflow FILE stays first), so a flag of the command line wins; `cli.rs` calls it for `run`, also with `--watch` and `--schedule`. Each key is a flag of `run`; an unknown key and a key of one call (`dry-run`, `list`, `rerun`) fail. A project file may set only the keys of `SAFE`; a key of `HOST` (a binary, a store, a host, a secret, an approval, a checkout) needs `trusted` in the file of the user, and a path of an untrusted project must stay in it. Of `.actrc`, the flags of the same name are taken; `-s NAME=VALUE`, each other flag and a host flag of an untrusted project are named on stderr. `--no-config` reads no file. Tests: `cargo test -p podrunner run_config` (the values of the keys; a project file that sets `allow` is refused and a trusted one is not; a path out of the project; the flags of an `.actrc`; the order of the files and the command line).


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
**Status:** done

## Problem

The study met a local runner whose second run failed with EACCES: the runner container wrote files as another uid into a directory of the host, and the host user could not remove them. The `--runner-dir` executor removes `_work` before each job and ignores the error (`RunnerDir::run` of `crates/podrunner-local/src/exec.rs`): a step that makes a directory with no write bit, or a file of another uid, makes the next job run in the stale tree of the last one, with no message. In a unit, a `container:` job writes as the uid of its container; destroy must remove those files too, or report residue.

## Premise

Read `crates/podrunner-local/src/exec.rs` on 2026-11-14. Not measured: the residue of a podvm unit after a container job that wrote as root into the workspace.

## Approach

`RunnerDir::run`: add the write bit where it is missing, then remove `_work`; a failure fails the job with the path. In a unit: a test job (`container:` as root, which writes into the workspace and makes a directory of mode 0500) runs twice on one state directory in `runner-image.yml`; both runs pass and destroy reports no residue. Each file of the run directory of T-135 is written by podrunner, never by a unit.

## Prove

`cargo test -p podrunner-local exec` (a `_work` with a directory of mode 0500 is removed, or the job fails with its path) and the double run in `runner-image.yml`.

State (2026-11-14): `podrunner_local::work::clear` gives each directory of `_work` the bits `u+rwx` again before the removal, and a failure fails the job with the path (unit test: a tree with modes 0000 and 0500). The unit half is in `runner-image.yml`: `crates/podrunner/tests/local/rerun.yml` (a container job that leaves a directory of mode 0500) runs twice with `--podvm`; a residue fails the job. Runner-image run 43556484747 (`8a65c48`): the podvm kind refuses `--user 0` (`container-option-refused`), so a podvm unit runs no container as root; the job now runs as the user of the image.

## Done

2026-11-14, `11aaeff` and `5dedd26`. `podrunner_local::work::clear` gives each directory of `_work` its write bit back before the removal, and a failure fails the job with the path. Runner-image run 43557460656 (`5dedd26`): `crates/podrunner/tests/local/rerun.yml` (a container job that leaves a directory of mode 0500 in the workspace) passes twice in a row with `--podvm`, and no unit leaves residue. The podvm kind refuses `--user 0`, so a podvm unit runs no container as root.

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
**Status:** done

## Problem

runner.server also runs a subset of Azure Pipelines YAML (`steps`, `script`, templates with `each`, `if` and `else`) on the same runner. podrunner's local source reads only GitHub workflows.

## Premise

Read the README of runner.server in the study on 2026-11-14. Not measured: the subset that runner.server covers, and whether the official runner takes the job message of such a pipeline.

## Approach

Read the expansion of runner.server (its `ExpandAzurePipelines` part) and the job message that it gives the runner. Write a note in `docs/` with the subset, the cost, and a recommendation. If the cost is real, write the question for the owner in `TODO/PROGRESS.md` with that recommendation before code.

## Prove

The note exists and `cargo todo check` passes.

## Done

2026-11-14, this commit. Read `src/Sdk/AzurePipelines` (28 files, about 4,000 lines) and the message of `MessageController` in the runner.server tree of the study. runner.server sends an Azure job as `AgentJobRequestMessage` to the Azure Pipelines agent; its `script`, `bash` and `pwsh` steps are task steps, which the GitHub runner does not run. The note `docs/azure-pipelines.md` gives the subset, the cost (a second payload, a second fake broker, a store of tasks, a template expansion) and the recommendation: not now. The question is Q6 of `TODO/PROGRESS.md`; no work waits on it.


# T-149: Local mode: a web view of a run on the loopback

**Source:** The owner's ghactions-local study (runner.server: the web page of runs, jobs and live logs)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

runner.server shows each run, job and live log on a web page. The local mode prints lines only.

## Premise

Read the README of runner.server in the study on 2026-11-14.

## Approach

`--web ADDR` (loopback only, a random token in the URL): a static page served by podrunner with each job, its state, its steps, its live log (T-134) and its artifacts (T-119). No script from a CDN; the page is in the binary. Exceed: the page binds no address other than the loopback, and names no secret.

## Prove

`cargo test -p podrunner run_local` (the page refuses a request with no token; the job list).

## Done

2026-11-14, this commit. `crates/podrunner/src/run_local/web.rs` and `web.html`: `--web IP:PORT` refuses an address that is not of the loopback, binds it, and serves under a random token of 16 bytes: the page (in the binary; no script from another host; it reads `state.json` each second), `state.json` (each job with its state and its live log, masked as on stderr, at most 4 MiB for each job; the names of the artifacts) and `artifacts/NAME` (the zip of a finalized artifact; `ArtifactStore::zip_of` refuses a name with a path part). Each other path, and another token, gets 404. After the run, the view stays until SIGINT (`web::hold`). Tests: `cargo test -p podrunner web` (the loopback rule, the token, the masked log, the state of a job, the decode of a name, no zip for a path). Checked by hand with a dry run.


# T-150: Local mode: a pause before a step and a shell in the unit

**Source:** The owner's ghactions-local study (its plan of a debugger)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

The study plans a debugger: stop a job before a step and look into its environment. act and runner.server give only `--reuse` or `--keep-container`. The local mode gives neither.

## Premise

Read the README of the study on 2026-11-14. Not measured: a pause of the official runner between steps (a step that waits for a file is one way).

## Approach

`--break JOB:STEP` puts a step before STEP that waits for a file in the unit; podrunner prints the command to enter the unit (`podvm exec`) and goes on when the user removes the file or ends the shell. With `--break-on-failure`, the pause comes after a failed step. The unit keeps its egress hold during the pause.

## Prove

`cargo test -p podrunner-local message` (the wait step in the message).

State (2026-11-14): `--break JOB:STEP` (the number of the step from 1, or its `id`) and `--break-on-failure`. `run_local/pause.rs`: the executor `Paused` (the outermost wrapper of each slot) puts a pause step into the message of each job: before the step, or after each step with `failure()`. The pause step asks `GET ${{ github.api_url }}_podrunner/hold/CELL` each second with curl or wget; `crates/podrunner-broker-fake/src/hold.rs` answers 503 until the release, from one table of the process, so it works on each executor and kind with no change of their own. `pause::watch` names each paused job on stderr and releases it at Enter; a signal releases each one; with no terminal the run is refused. Tests: `cargo test -p podrunner pause` (the place of the steps, another job, the script) and `cargo test -p podrunner-broker-fake hold`. Next: the command to enter a paused unit (measure `podvm exec` on a running unit of the podvm driver, and a shell in a guest), and the path of the workspace for `--runner-dir`.

State (2026-11-14, later): `UnitDriver::enter` gives a command that runs in the running unit under its egress hold; the podvm kind runs `podvm exec -i [-t]`. `podrunner_local::enter` keeps the unit of each running job. At a pause, `s` and Enter opens a shell in the unit on the terminal of podrunner; Enter goes on. The machine tier of podvm refuses `exec`, so `--microvm` has no shell; the incus kind has none yet. Tests: `cargo test -p podrunner-local enter`; `crates/podrunner-podvm/tests/links.rs` enters a running podvm unit in `runner-image.yml`. Next: its green run.

State (2026-11-14, later): runner-image run 43623211536 (`94fddee`): `enter` ran its command in the podvm unit (`in-unit-42`), but the test read a path relative to the work directory of the payload, and `podvm exec` starts elsewhere. The test now reads the file by its absolute path. Next: the green run.

## Done

2026-11-14, `aeac0a3`: runner-image run 43628702784 is green: `crates/podrunner-podvm/tests/links.rs` enters a running podvm unit with `UnitDriver::enter` and reads a file of its tree (`in-unit-42`). At a pause, `s` and Enter open a shell in the unit on the terminal of podrunner. `cargo test -p podrunner run_local::pause` and `cargo test -p podrunner-local enter` cover the pause steps and the table of units. The machine tier of podvm refuses `exec`, so a guest of `--microvm` has no shell; the incus kind has none yet.


# T-151: Measure: a job of another architecture in the local mode

**Source:** The owner's ghactions-local study (act: `--container-architecture`; runner.server: `--container-architecture`, `--container-platform`)
**Category:** measurement
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

act runs a container of another architecture (for example `linux/arm64` on an x86_64 host) through the emulation of Docker. The local mode runs the runner of the architecture of the host only (`arch` of `crates/podrunner/src/run_local.rs`).

## Premise

Read `crates/podrunner/src/run_local.rs` on 2026-11-14. Not measured: a guest of the machine tier of podvm with another architecture (emulated by QEMU, with no KVM).

## Approach

Measure in `runner-image.yml`: an aarch64 runner image in a guest of the microvm kind on an x86_64 host, with the time of the start and of a small job. Record the result in `docs/unknowns.md`; if it holds, add `--arch` with T-123.

## Prove

`gh workflow run runner-image.yml` runs the measure, and `docs/unknowns.md` has its row.

State (2026-11-14): `scripts/measure-foreign.sh` pulls `busybox:1.37.0` for `linux/arm64` into a store of its own and runs it in the machine tier of podvm with `--platform linux/arm64`; it prints the exit, the seconds, the accelerator and `uname -m` of the guest. `runner-image.yml` runs it on the x86_64 host as the step "A guest of another architecture (a measure)", with `qemu-system-arm`. Next: read the step summary of the run, record the result in `docs/unknowns.md`, and then measure the aarch64 runner image with a small job.

Measure (runner-image run 43594485325, `dcc266f`): the pull of `busybox:1.37.0` for `linux/arm64` into a store of its own takes 1 s; `podvm run --podvm-tier=machine --platform linux/arm64` of podvm `0.2.0-beta.14` on the x86_64 host exits 125 after 1 s: podvm starts `qemu-system-x86_64` under KVM, and the guest gives no exit marker. So the machine tier of this podvm takes no guest of another architecture. Asked podvm on GitHub #102 (COORD-042): a guest of another architecture (QEMU of that architecture, with no KVM). Next: with no, the local mode refuses a job of another architecture by name, and `docs/unknowns.md` records it; with yes, a choice of the architecture and its measure.

## Done

2026-11-14, this commit. podvm answered on PodUtils/podvm#366 (COORD-112, to GitHub #102): no guest of another architecture yet; PodUtils/podvm@T-1889 refuses `--platform linux/arm64` in the machine tier by name, and PodUtils/podvm@T-1890 plans the guest. `dispatch::other_arch` names the platform of a label of another architecture than the host (`ARM64`, `aarch64`, `*-arm`, `*-arm64` on x86_64; `X64`, `x86_64`, `amd64`, `*-x64` on aarch64), and `Dispatch::route` fails such a job by name unless a `--platform` route names its label. `docs/unknowns.md` records U26. Test: `cargo test -p podrunner-local dispatch`. When podvm amends PodUtils/podvm#366 with a release, a new entry measures the guest and adds the choice of the architecture.


# T-152: Local mode: an env of one expression is refused

**Source:** The owner's ghactions-local study (runner.server's README uses `env: ${{ secrets }}` on a step)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

runner.server runs a step with `env: ${{ secrets }}` (an expression that gives a mapping). `env` of `crates/podrunner-local/src/message.rs` refuses an `env:` of one expression by name.

## Premise

Read `crates/podrunner-local/src/message.rs` on 2026-11-14. Not measured: whether GitHub and runner v2.338.2 take an `env:` of one expression at the levels of the workflow, the job and the step.

## Approach

Measure each level against the schema of the runner and the GitHub docs first (the rule of Q4). Where GitHub takes it, send the expression as one token of the mapping and let the runner evaluate it; where GitHub refuses it, keep the refusal and name the level in its text.

## Prove

`PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner-local --test runner -- --ignored` (a step `env:` of one expression).

## Done

2026-11-14, this commit. Measured: the workflow schema of the runner (`workflow-v1.0.json`, the copies in the runner.server tree of the study) gives `workflow-env`, `job-env` and `step-env` a `context`, so one expression may stand for the whole mapping; `container-env` has no context. `env` of `crates/podrunner-local/src/message.rs` now gives one token: a mapping, or the expression token (type 3) that the runner evaluates to a mapping, at the three levels; another text fails the job with the line. Tests: `cargo test -p podrunner-local message` (the tokens of the workflow and the step, two bad texts) and `PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner-local --test runner -- --ignored` in level 2 (`fromJSON` at the workflow and the job, `env: ${{ secrets }}` at a step).


# T-153: podrunner doctor for the local mode

**Source:** The owner's ghactions-local study (its doctor plan; act: `--bug-report`)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

The study plans a doctor that finds the faults of a host before a run (the SIGXFSZ limit, a missing tool), and act prints a bug report of the system. `podrunner doctor` reads a file of `serve`; it has no check of the inputs of `podrunner run`.

## Premise

Read `docs/cli.md` (the row of `doctor`) on 2026-11-14.

## Approach

`podrunner doctor --run [the options of run]`: check the podvm binary and its version, the store and the image digest, `/dev/kvm` for the microvm kind, podnet and its hold, the runner directory (`run.sh`, `bin/Runner.Listener`), the limit of file size (T-144), the free disk of the scratch directory, and the lock file and store of T-118. Each check names its fix; nothing changes.

## Prove

`cargo test -p podrunner --test doctor`

## Done

2026-11-14, this commit. `podrunner doctor --run FILE [the options of run]` (`crates/podrunner/src/run_local/doctor.rs`) checks the limit of file size (T-144), the free disk of the scratch directory, the workflow and its lock file, the runner directory, the podvm binary and its lowest version for the kind (`Podvm::require`), the store, `/dev/kvm` for `--microvm`, the engine directory and podnet. Each check gives `ok` or `FAIL` with its fix; nothing changes; the exit is 1 when a check fails. Test: `cargo test -p podrunner --test doctor` (a runner directory that holds and one that is missing).


# T-154: Local mode: the fetch of the actions through the egress module

**Source:** T-118 (its Approach: the host fetches through the egress module with a policy of the forge hosts only)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

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

## Done

2026-11-14, this commit. `crates/podrunner-local/src/fetch.rs`: `Fetch::open(HOST)` makes an egress module of the host (its way out: the proxy that the environment names for HOST through `pod_proxy::Environment`, with names sent to it, or a direct connection), opens one grant with a policy of HOST only on a unix socket, and starts `Relay`, a loopback TCP port that relays each connection to that socket; drop ends the grant, the module and the socket. `Git` takes `proxy`: `Git::command` gives git `http.proxy`, `protocol.allow=never` with only the protocol of the base, and no submodules, through `GIT_CONFIG_*` variables (no file of the user), and removes the proxy variables of the environment. `podrunner run` opens the grant for the host of the forge (`--action-server`, else github.com) before the resolve, unless `--offline`. Tests: `cargo test -p podrunner-local` (the git command: the proxy, the protocol rules, the removed variables; the relay both ways; the host of a URL) and `cargo test -p podrunner-local --test store -- --ignored` (the resolve and the fetch of two actions through the grant; a host outside the policy is refused), green here through the proxy of the environment.


# T-155: An intermittent failure of the workspace tests that the check script hid

**Source:** A session (2026-11-14): `sh scripts/agent/check.sh` gave `test 101` once at the commit after `8a65c48`, and passed on the next two runs
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

One run of `cargo test --workspace --all-features` failed, and the next runs passed. The check script printed only the summary lines: its pattern `failed` matched each `0 failed`, and `head -30` cut the output before the cause. The test that failed is not known. Five runs of the unit tests of `podrunner-broker-fake`, `podrunner-local` and `podrunner` gave no failure. The tests that depend on time are suspects: the cache tests with a sleep of 1.1 s (the newest entry first), the limit tests of `exec`, and the front tests (fixed for one race in `8a65c48`).

## Premise

Read `scripts/agent/check.sh` on 2026-11-14. The pattern is fixed in the same commit as this entry: it now matches `FAILED`, `panicked`, `error`, `failures:` and `----`.

## Approach

Run `cargo test --workspace --all-features` in a loop (20 runs) and keep each failed log. Fix the test that fails: no sleep as an order, a time given by the test instead of the clock.

## Prove

20 runs of `cargo test --workspace --all-features` in a row pass.

State (2026-11-14): 10 runs of `cargo test --workspace --all-features` at `fca65c9` passed. Not reproduced yet; the next failure of the check names its test.

## Done

2026-11-14, this commit. 10 runs of `cargo test --workspace --all-features` at `fca65c9` and 10 at `78c5cb2` passed: 20 green runs, with the new tests of T-136 to T-154. The failure did not come again. The check script names a failed test now (its pattern takes `FAILED`, `panicked`, `failures:`), so a new failure gets a new entry with its test.


# T-156: Local mode: a glibc image by tag in a podvm unit where chroot is denied

**Source:** Runner-image run 43565195597 (`872b43f`): the job container `busybox:1.37.0` failed in the podvm unit
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

On a host that denies chroot(2), `podvm run` refuses a glibc image that has no `libdl.so.2` in its library directories (`busybox:1.37.0`): the no-chroot loader family of podvm asks for `libdl.so.2`. glibc 2.34 and later has no separate libdl, so many glibc images fail as a job container, a service or a Docker step of the podvm kind.

## Premise

The log of the run names the driver error (T-130, `driver_errors`). podvm `0.2.0-beta.14`.

A static image (`busybox:1.37.0-musl`) fails as well: the memfd family of podvm has no detached drive (the second case on GitHub #100).

## Approach

Ask podvm for the fix (GitHub #100, COORD-040). Take the podvm release that has it in `images/podvm.toml`. Make the step "An image by tag in a podvm unit" of `runner-image.yml` a check again (no `continue-on-error`).

## Prove

`runner-image.yml`: the run of `tags.yml` with `busybox:1.37.0` gives `ok tagged`.

State (2026-11-14): podvm `v0.2.0-beta.17` carries PodUtils/podvm@T-1880 (PodUtils/podvm#351): on its fixture that denies chroot, `podvm-x86_64` runs `busybox:1.37.0`. `images/podvm.toml` pins `v0.2.0-beta.18`; `tags.yml` takes `busybox:1.37.0`, and the step "An image by tag in a podvm unit" of `runner-image.yml` is a check again (no `continue-on-error`). Next: its green run closes T-156 and T-130. A static image (the memfd family, the second case of GitHub #100) is still open in podvm.

## Done

2026-11-14, `c007cce`: runner-image run 43610257672 is green: with podvm `v0.2.0-beta.18` (PodUtils/podvm@T-1880) the glibc `busybox:1.37.0` runs as a job container where chroot is denied, and the step is a check. The static image (the memfd family of podvm) is T-158.


# T-157: Local mode: load the images of a job from the store into the engine of the guest

**Source:** T-124 (its approach: no pull inside the guest)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

In a guest with an engine, the runner pulls each image of `services:`, `container:` and Docker actions through the egress policy of the job. The policy must allow the hosts of each registry and its CDN (Docker Hub sends layers to Cloudflare, R2 or S3), and an offline run (`--offline`) cannot use the engine.

## Premise

Runner-image run 43609196605 (`5096a3b`): the parity suite passes in guests with an engine, and the guest pulls its images through the egress policy with the CDN hosts of Docker Hub allowed. The image store of `pod-actions-store` keeps images by digest.

## Approach

The official runner always runs `docker pull` for a service, a job container and a Docker step, so a `docker load` alone does not stop the pull. The broker of each job is the registry mirror of Docker Hub for the engine of the guest (`dockerd --registry-mirror`, through the channel and the front). Before the first job, podrunner saves each image of Docker Hub of the run from the store of the units (`podvm save`) into one OCI layout, and the broker serves its blobs and manifests by digest (`podrunner_broker_fake::mirror`). The store keeps the manifest of the platform, not the index: the job message names that manifest. A miss of the mirror goes to Docker Hub through the egress policy. Another registry than Docker Hub still pulls through the egress policy: dockerd has a mirror only for Docker Hub.

## Prove

`runner-image.yml`: the parity rows 04, 05 and 09 pass in guests with an engine with no registry host in the egress allows, and with `--offline` after one run that fills the store.

State (2026-11-14): the mirror is in (`podrunner-local/src/mirror.rs`, `podrunner-broker-fake/src/mirror.rs`, the flags of dockerd in the guest script). The step "The images of a guest engine from the store" of `runner-image.yml` runs 04 and 05 in a guest with an engine and no `--allow`. Next: its green run.

State (2026-11-14, later): runner-image runs 43614533662 (`0750948`) and 43615355943 (`3ce59bc`): the fill of the mirror failed, so the guest pulled from Docker Hub and the egress policy denied it. `podvm save` of an index digest refused to choose: the store had an image of two platforms for it. The mirror and the podvm driver now give `podvm save` the platform of the unit. The other steps of both runs passed, also "A tool cache in a podvm unit" (T-138).

State (2026-11-14, later): runner-image run 43621400822 (`dcbc53f`): `podvm save` has no `--platform` (exit 125), although its message names it; the podvm driver failed each unit start with it. The driver has `save` with no platform again. The mirror pulls each image into a private store of one platform and saves it from there. COORD-044 (GitHub #104) asks podvm for `save --platform`. Next: the green run.


## Correction

2026-11-15: runner-image runs 43657352190 (`c3b5744`) and 43658370121 (`v0.1.0-beta.5`) failed: Docker Hub refused the pulls of the host and of the guests with HTTP 429 (its limit for a host with no login), and the second pull of the mirror into its private store adds to it. The mirror now pulls each image of Docker Hub through `mirror.gcr.io` (the same digest), and through Docker Hub only when that fails. T-163 removes the second pull.

## Done

2026-11-14, `94fddee`: runner-image run 43623211536: the step "The images of a guest engine from the store" passes. 04 (`services:`) and 05 (a job container and a Docker step) run in a microvm guest with an engine and no `--allow`: the guest reaches no registry, so the engine took each image from the mirror of the broker. A Dockerfile action still pulls its base image through the egress policy, and another registry than Docker Hub too.


# T-158: Local mode: a static image as a container of a podvm unit

**Source:** T-156 (the second case of GitHub #100)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

podvm routes a static binary (`busybox:1.37.0-musl`) to its memfd family, which has no detached run (`podvm run -d`). The podvm driver runs each container detached, so a static image fails as a job container, a service or a Docker step of the podvm kind.

## Premise

Runner-image run 43570649182 (`e31f9e5`): `busybox:1.37.0-musl` failed as a job container in a podvm unit. podvm `0.2.0-beta.14`; not measured again on `0.2.0-beta.18`.

## Approach

Ask podvm for a detached run of the memfd family (GitHub #100, COORD-040). Take the podvm release that has it in `images/podvm.toml`, and add `busybox:1.37.0-musl` to `tags.yml`.

## Prove

`runner-image.yml`: the step "An image by tag in a podvm unit" gives `ok` for `busybox:1.37.0-musl`.

State (2026-11-15): podvm `0.2.0-beta.19` (PodUtils/podvm#376) is pinned in `images/podvm.toml`. `tags.yml` has the job `static` with `busybox:1.37.0-musl`, and the step "An image by tag in a podvm unit" checks `ok static` and its line in the lock. Next: its green run.

State (2026-11-15, later): runner-image run 43669743775 (`fa84341`) is red at that step. podvm `0.2.0-beta.19` starts the static image detached, but refuses the mount `/__e:ro` of the runner: "--read-only and :ro need the read-only layer of the interposer, which only the userland loader family has" (PodUtils/podvm@T-1655). podrunner keeps `/__e` read-only (`crates/podrunner-hook/src/policy.rs`). COORD-051 (GitHub #111) asks podvm for a read-only bind on that rung.

State (2026-11-15, podvm's answer PodUtils/podvm#391): the `:ro` volume holds on podvm `main` and comes in its next release. The `chmod: cannot access .../trees/.trash-*` lines were the `chmod -R` of `destroy` racing the background removal of podvm's tree. Each podvm command of the driver now sets `PODVM_SPARE_TREE=0` (`Podvm::command`), so podvm removes the tree in the run and leaves no background process.

State (2026-11-15, later): podvm `0.2.0-beta.21` (PodUtils/podvm@T-1901) is pinned in `images/podvm.toml`. Next: the step "An image by tag in a podvm unit" of `runner-image.yml` gives `ok static`; then close GitHub #111.

## Done

2026-11-15: runner-image run 43686746902 (`9a7fb3c`, podvm `0.2.0-beta.21`) is green: the step "An image by tag in a podvm unit" gives `ok tagged` and `ok static`; the static image runs as a job container with the `:ro` mount of the runner where chroot is denied. COORD-051 (GitHub #111) closes.

# T-159: An intermittent unknown count of processes at the destroy of a podvm unit

**Source:** Runner-image run 43626860120 (`47c0daa`), the step "The runner in a podvm unit runs a canned job and a container job"
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The local suite in a podvm unit with an egress hold failed once: the cell `matrix (alpha, 1)` passed each step, then `destroy` gave `Residue { processes: Unknown, files: Known(0) }`, so the job failed with "left residue" and `fail-fast` cancelled the other cells. The same step passed on the run before (43623211536, `94fddee`), with the same code of the driver. Two checks give `Unknown`: tagged processes with no podvm process (orphans), or podvm names the launcher `dead`. The log does not say which.

## Premise

Read `destroy` of `crates/podrunner-podvm/src/unit.rs` on 2026-11-14. podvm `v0.2.0-beta.18`. One failure in the runs of the day.

## Approach

Log the cause of an unknown count: which check, and the names of the tagged processes. Then fix the check that gives a false `Unknown`, or ask podvm by a COORD issue when podvm leaves the state.

## Prove

Ten runner-image runs in a row with no "left residue" in the local suite of the podvm kind.

State (2026-11-14): `destroy` writes `podrunner: destroy UNIT: no count of processes: orphans B, launcher dead B, tagged [NAMES]` to stderr when the count is unknown. Next: the log of the next failure.

State (2026-11-14, later): runner-image run 43628702784 (`aeac0a3`) is green. The log line came once, from conformance test 26 (`a-killed-shim-is-not-clean`), which kills the launcher on purpose: `orphans false, launcher dead true, tagged ["podvm"]`. So the line works. Next: the log of a failure of the local suite.

State (2026-11-15): podvm removed the tree of a run in a background process after its exit (PodUtils/podvm#391). A process of that kind can carry the tag of the unit and outlive it: a possible cause of `orphans true`. The driver now sets `PODVM_SPARE_TREE=0` on each podvm command, so no such process stays. Next: ten green runs in a row.

## Done

2026-11-15: the cause is in runner-image run 43736540966 (`affb8c9`), the local suite with `--jobs`: `podrunner: destroy u-3: no count of processes: orphans true, launcher dead false, tagged [""]`. A tagged process ended between the list of the tree and the read of its name; its name read as empty, and it counted as a live orphan. `destroy` now reads each name once (`labels::live_names`) and drops a process with no name; `labels::orphaned` reads only those names. `labels::tests::a_process_that_ended_is_no_orphan`.

# T-160: Local mode: a shell in a paused job of the incus kind

**Source:** T-150 (its Done: the incus kind has no shell)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

At a pause, `s` and Enter open a shell only in a podvm unit. The incus kind refuses `UnitDriver::enter`, so a paused job of `--incus` has no shell.

## Premise

The incus tier of podvm takes `exec` (the limits page of podvm, PodUtils/podvm@T-1722). The instance has no NIC: its one way out is the channel to the egress socket (T-109).

## Approach

`IncusUnit::enter` runs `podvm exec -i [-t] --podvm-tier=incus NAME ARGV`. The command has the way out of the payload, as the instance has no NIC.

## Prove

`crates/podrunner-incus/tests/egress.rs` in `runner-image.yml`: a command of `enter` runs in the instance and sees `lo` only.

State (2026-11-14): `IncusUnit::enter` is in; `egress.rs` keeps the payload up after its checks and enters it. Next: its green run.

## Done

2026-11-14, `7530b2c`: runner-image run 43632726810 is green: `crates/podrunner-incus/tests/egress.rs` enters the running instance with `IncusUnit::enter`, and the command gives `in-unit-42 lo`: it runs in the instance, which has no NIC.

# T-161: An alternate payload: a runner other than the official one

**Source:** The owner's ruling on U15 (2026-11-14): "Alternate payload allowed"
**Category:** research
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** partial

## Problem

podrunner runs only the official `actions/runner` (.NET) in a unit. It serves no target that the official runner does not serve (riscv64, i686, loongarch64, ppc64le, a static musl host with no .NET), and each unit image carries the advisories of its .NET packages (T-082). The owner allowed an alternate payload that reports a third-party runner version string to github.com (U15); the plan lists "alternate payloads and a native runner" with that trigger.

## Premise

Read `TODO/plan.md` (the table of non-goals) and `docs/unknowns.md` (U15) on 2026-11-14. podcrates has the native crates of the workflow model (`pod-actions-expr`, `pod-actions-workflow`, `pod-actions-store`, `pod-actions-results`); the local mode uses them to plan a run, not to run its steps. Not measured: which messages of the broker and of the run service an alternate payload must speak, and how github.com treats its version string.

## Approach

1. Write the census: each call of the official runner to the broker, the run service and the results service of a job (from the fake broker of the local mode, which serves them all), with its fields.
2. Decide the payload: a native runner from the `pod-actions-*` crates (a request to podcrates), or another existing runner. Record the call in `docs/decisions.md`.
3. Measure it against the fake broker of the local mode first, then in a live gate.

## Prove

`docs/alternate-payload.md` has the census and the call; one canned job of the fake broker passes with the alternate payload in a unit.

State (2026-11-15): steps 1 and 2: `docs/alternate-payload.md` has the census of the calls of the official runner (from the routes of the fake broker) and two candidates; the call (`docs/decisions.md`) measures an existing third-party runner first. Next: step 3, the candidate and its measure against one canned job of the fake broker.


# T-162: G-12: limits of CPU, memory and processes on the podvm and incus kinds

**Source:** githome's G-12 (GitHub #66): limits of CPU, memory and processes on container and microVM units, and a hard end within 60 s
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

G-12 asks for limits of CPU, memory and processes on each unit. podrunner enforces the wall time (a hard end, T-021) and the disk (T-074) on each kind, and the memory and the vCPUs of a guest of the `microvm` kind (`[unit.microvm] mem`, `cpus`). A unit of the `podvm` kind and an instance of the `incus` kind have no limit of CPU, memory or processes: a job can take the memory of the host.

## Premise

Read `docs/configuration.md` and the drivers on 2026-11-15. The `incus` tier of podvm refuses `--podvm-mem`. Not measured: which limits the userland and supervise rungs of podvm can hold with no root (rlimits, a cgroup that the user owns), and which keys of incusd podvm passes (`limits.cpu`, `limits.memory`, `limits.processes`).

## Approach

1. Measure on the CI host: the limits that podvm applies to a `run -d` with no root, and the keys of an incus instance.
2. Ask podvm by a COORD issue for each limit that it lacks (a cgroup v2 leaf of the user, or rlimits for `RLIMIT_AS` and `RLIMIT_NPROC`; the incus keys).
3. Add `cpu`, `mem` and `pids` to `[unit.podvm]` and `[unit.incus]`, and to the limits of an offer; `slots` refuses an offer whose limit a kind cannot hold.

## Prove

A unit conformance test on each kind: a payload that forks past `pids`, and one that allocates past `mem`, end with the code `limit`; `runner-image.yml` runs it on the podvm and incus kinds.

State (2026-11-15): step 1 is read from the limits page of podvm: the CLI refuses `--memory` and `--cpus` with exit 125 (no cgroup that the runtime grants), and the incus tier refuses `--ulimit` and `--podvm-mem`. So no limit of memory, CPU or processes exists for the native tiers or the incus tier. Step 2: COORD-050 (GitHub #110) asks podvm for rlimits on `run -d`, a cgroup leaf where the host delegates one, the incus keys, and a report of the held limits.

State (2026-11-15, later): podvm answered on PodUtils/podvm#382. (1) `--ulimit nproc=N` and `--ulimit as=BYTES` already hold on `run -d` of the native tiers (also where chroot is denied); each child keeps them. The cgroup half (`--memory`, `--cpus`, `--pids-limit` on a delegated subtree) is PodUtils/podvm@T-1747, open. (2) The incus tier: `--memory`, `--cpus` and `--pids-limit` become `limits.memory`, `limits.cpu` and `limits.processes` (PodUtils/podvm@T-1897, on podvm `main`, in the next release). (3) The `limits` field of `--report` names the held limits. Next: `[unit.podvm] mem` and `pids` as `--ulimit as=` and `--ulimit nproc=`, with the conformance tests of the Prove; the incus keys after the podvm release with PodUtils/podvm@T-1897.

State (2026-11-15, step 3): `[unit.podvm] mem` and `pids` become `--ulimit as=` and `--ulimit nproc=` of `podvm run -d` (with `DOTNET_GCHeapHardLimit` at three quarters of `mem`); `cpus` on the podvm kind is refused (no cgroup). `[unit.incus] mem`, `cpus` and `pids` become `--memory`, `--cpus` and `--pids-limit` (podvm `0.2.0-beta.20` or later, `LOWEST_INCUS_LIMITS`). The microvm kind says `memory` and `cpu` `enforced` (the size of its guest) and refuses `pids`. Each kind records its held limits as `enforced`, so placement refuses an offer that needs another. A kind holds its own value, not the value of the offer. `docs/configuration.md`, "The limits of a unit". Tests: `podrunner_unit::limits` (sizes), `the_limits_become_rlimits_of_the_payload`, `the_limits_become_the_keys_of_the_instance`, and the cases of `tests/check.rs`. Next: the measure in `runner-image.yml`: the runner under `[unit.podvm] mem` (does .NET start under `RLIMIT_AS`), and a payload that forks past `pids` or allocates past `mem` on the podvm and incus kinds. A process that an rlimit stops gets an error (`EAGAIN`, `ENOMEM`), not the end of the unit: the Prove reads the error, not the code `limit`.

State (2026-11-15, the measure): `the_runner_runs_under_the_limits_of_the_unit` (`crates/podrunner-broker-fake/tests/podvm.rs`, in the podvm step of `runner-image.yml`) runs a canned job of the official runner in a podvm unit with `mem` 16 GiB and `pids` 4096: the job must pass, and `ulimit -v` in the step must name the limit. Next: its run; the incus keys on real instances (the incus step).

State (2026-11-15, the measure ran): runner-image run 43690754546 (`ba24d0e`): under `[unit.podvm] mem` 16 GiB the runner did not start (the broker saw no request). Measured here with the runner v2.338.2 and `prlimit --as=16G`: CoreCLR fails ("GC heap initialization failed with error 0x8007000E") with `DOTNET_GCHeapHardLimit` alone, and starts with `DOTNET_GCRegionRange` at the same value: the GC reserves its regions (256 GiB by default) at start. `ulimit_args` now sets both. The test prints the log of the unit. Next: the measure again.

State (2026-11-15, green): runner-image run 43704601731 (`88cc4d7`) passed `the_runner_runs_under_the_limits_of_the_unit`. The pinned podvm `0.2.0-beta.21` has the incus keys (`LOWEST_INCUS_LIMITS` is `0.2.0-beta.20`). Next: a payload past `pids` and one past `mem` on the podvm and incus kinds (the Prove); `cpus` on the podvm kind waits for the cgroup half of podvm (PodUtils/podvm@T-1747, COORD-050).

State (2026-11-15, the Prove): `crates/podrunner-podvm/tests/limits.rs` and `crates/podrunner-incus/tests/limits.rs` start a payload with the limits of an offer (T-198): 1 GiB of `dd` under `memory` 256 MiB, and 1000 forks under `pids` 256, each with a control with no limit. On podvm `0.2.0-beta.21` as a user with no root here, both pass (the fork ends with "Cannot fork"). `runner-image.yml` runs both. Next: their run, the incus one on real instances.

## Done

2026-11-15: runner-image run 43711862741 (`b1c058f`) passed `crates/podrunner-podvm/tests/limits.rs` and `crates/podrunner-incus/tests/limits.rs` on real units and instances, after `the_runner_runs_under_the_limits_of_the_unit` in run 43704601731. `cpus` on the podvm kind waits for the cgroup half of podvm: T-221.


# T-163: Take podvm 0.2.0-beta.19 and save the images of the mirror from the store of the units

**Source:** podvm's answer to COORD-044 (PodUtils/podvm#375)
**Category:** chore
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

The mirror of a guest engine (T-157) pulls each image again into a private store, because `podvm save` of an index digest refused: "names 2 images in this store". podvm found the cause: a pull by tag and a pull by the index digest gave two records of one manifest, and the lookup counted them as two images. The pull again costs time and the network on each run.

## Premise

PodUtils/podvm#375 (2026-11-15): PodUtils/podvm@T-1895 keeps one record for each manifest and platform; `podvm save --platform` is on `main` since PodUtils/podvm@T-1841. Both go into podvm `0.2.0-beta.19`.

## Approach

Pin podvm `0.2.0-beta.19` in `images/podvm.toml`. Save each image of the mirror from the store of the units (`podvm save --platform`), with no private store and no second pull.

## Prove

`runner-image.yml`: the step "The images of a guest engine from the store" passes with the save from the store of the units.

State (2026-11-15): podvm `0.2.0-beta.19` is pinned. `mirror::fill` saves each pinned image from the store of the units with `podvm save --platform`; an image that a digest names is pulled into that store first (through `mirror.gcr.io`, then Docker Hub). No private store and no second pull. Next: the green run of "The images of a guest engine from the store".

## Done

2026-11-15, `fa84341`: runner-image run 43669743775: the step "The images of a guest engine from the store" passes with podvm `0.2.0-beta.19`. 04 (`services:`) and 05 (a job container and a Docker step) run in a microvm guest with an engine; `mirror::fill` saves each image from the store of the units with `podvm save --platform`, with no private store and no second pull.

# T-196: The level2 corpus fails on podcrates jobs that read live state

**Source:** level2 run 43686749522 (`9a7fb3c`, GitHub #114)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The corpus of the fake broker (`crates/podrunner-broker-fake/corpus/podcrates.json`, podcrates `59bca1f`) failed on 3 of 9 jobs: `plants.yml/plants` ("STALE workflows-missing: the patch does not apply ... live.yml"), `secrets.yml/secrets` ("the allowed URI ... was not found; the entry may be stale") and `upstream.yml/sources` (it compares the pins with the live upstream repositories). The same corpus passed in run 43669743789 (`fa84341`) that day. The corpus pins the commit of podcrates, but these jobs read state that moves: the upstream repositories, and the history of the clone.

## Premise

The log of level2 run 43686749522, read on 2026-11-15. The corpus logic of podrunner did not change between the two runs.

## Approach

Find for each of the three jobs what it reads besides the pinned tree (the network, the git history, a file of another job). Exclude a job that reads live state in `scripts/corpus.py` with that reason, as the script does for each job that the fake cannot run as GitHub would; or move the corpus to a newer commit of podcrates when the job is stable there.

## Decision

The red corpus was the first full run on this pin: on a push, `level2.yml` runs only `ci.yml/record,upstream.yml/mirrors`, and the dispatch of #114 took the default `all`. The three jobs check the upkeep of podcrates against state that moves (the live upstream repositories; the whole history against the allowlist of podcrates, with a check of found secrets on the network; the plants of podcrates, and `workflows-missing` does not apply even at `59bca1f`). `scripts/corpus.py` excludes them by name with the reason (`LIVE`), as it excludes each job that the fake cannot run as GitHub would; the corpus keeps 6 jobs.

## Prove

`python3 -I scripts/corpus.py DIR` gives the corpus with the reason of each excluded job, and `level2.yml` passes its corpus job twice on two days.

## Done

2026-11-15: `LIVE` of `scripts/corpus.py`; `crates/podrunner-broker-fake/corpus/podcrates.json` is the output of the script at podcrates `59bca1f` (the check of `level2.yml` diffs it). The proof of two days is the next runs of `level2.yml` (the schedule takes `all`).


# T-199: The local mode is not tried by a user on real workflows

**Source:** the owner (2026-11-15), GitHub #153
**Category:** chore
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** blocked

## Problem

The live gates of `serve` (#74, #75) found 16 defects that CI did not find. The local mode (`podrunner run`) has CI proof only: the parity suite, the local suite on each kind and the level2 corpus. No user ran it on the real workflows of other projects.

## Premise

A user agent on another machine can run `podrunner run` on public projects with no forge, no App and no token.

## Approach

GitHub #153 (LIVE-3) gives the steps: the host, the binaries of `v0.1.0-beta.6`, the runner images, `doctor --run`, three or more real projects on each executor of the host, the features of `run`, the egress hold, and a comparison with act. Each fault becomes an entry.

## Prove

The report of #153: each step has a result, and each fault has an entry `T-NNN` that is done.

## Blocker

2026-11-15: waits for the report of a user agent on GitHub #153.


# T-202: One installer and one offline bundle for the local mode

**Source:** the owner (2026-11-15): the first two testers of the local mode
**Category:** feature
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

Two testers started on the local mode. Each hit the same wall first: the time from "I know about podrunner" to real work. A user must download podrunner, podvm, podnet, the official runner tarball and the runner image archive from four places (the runner image is only a workflow artifact, which needs `gh` and a token, and expires), check each SHA-256 and attestation by hand, load the image into a podvm store with manual OCI steps, and then type a long `podrunner run` command. It takes tens of minutes.

The owner asks for: one POSIX installer (x86_64 and aarch64, `/bin/sh`, curl or wget, a SHA-256 tool, tar; no root, no `gh`, no token, no Python), hermetic (each version and SHA-256 pinned in the script by a generator, with a check that fails when they differ) and idempotent (a second run downloads nothing that is there and correct; a partial download never counts). The default installs the binaries, the podvm store with the runner image, the runner directory and the `run.toml` of the user (T-143), then runs `podrunner doctor` and prints what works and the next command. Options: a prefix, `--full`, `--bundle PATH`. One offline bundle for each architecture, and the runner image archives as release assets with their `.sha256` and attestations. A test against a local fixture, the docs and `THIRD_PARTY.md`.

## Premise

Read on 2026-11-15: `.github/workflows/release.yml`, `.github/workflows/runner-image.yml` (the step "Import into a podvm store"), `images/podvm.toml`, `images/podnet.toml`, `images/runner/runner.toml`, `docs/releases/v0.1.0-beta.6.md`, GitHub #75 and #153 (their steps 2 and 3 are the manual install). Measured here with the real assets of `v0.1.0-beta.6`, podvm `v0.2.0-beta.21` and podnet `v0.4.0-beta.8`: podrunner 7.9 MB, podvm 7.2 MB, podnet 30.7 MB, the runner tarball 228 MB (x64) and 140 MB (arm64), downloaded in 15 s through the proxy of this sandbox; the unpack of the runner takes 5 s. The artifacts of runner-image run 43704601731: `runner-x86_64` 306 MB, `runner-aarch64` 215 MB, `runner-full-x86_64` 482 MB. `podvm inspect runner@DIGEST` exits 0 for an image of the store and 1 else; `podvm verify` hashes each blob. The artifact `runner-ARCH` held the raw archive of buildx, not the archive that `podvm load -i` takes. `podrunner doctor --run` did not read `run.toml`. Not measured: the load of a real runner image (no Docker daemon here, and the artifacts need a token), a run of the release workflow.

## Approach

- `scripts/install.sh` (the release asset `podrunner-install.sh`), POSIX sh: a block of pins; each file to `DEST.part`, checked, then moved; the binaries to `PREFIX/bin` and `DATA/bin`; the runner unpacked with a marker of its SHA-256; the image loaded when `podvm inspect` does not find it, else `podvm verify`; `run.toml` with a marker line (a file of the user stays); `doctor`, `doctor --run`, and the next command. `--prefix`, `--full`, `--bundle`, and `PODRUNNER_MIRROR` for a mirror and the test.
- `scripts/install-pins.py`: the block from `images/*.toml`, and for a release the SHA-256 of each podrunner binary and image archive and the digest of each image; `--check` for the tree.
- `scripts/make-bundle.sh`: the bundle of one architecture from the stamped installer, each file checked against its pin.
- The release assets: T-205.
- `podrunner doctor --run` reads the configuration files of `run` first, so that the installer checks the run that it configured.
- `scripts/test-install.sh` in the gate and in `scripts/agent/check.sh`.

## Prove

```sh
sh scripts/test-install.sh
python3 -I scripts/install-pins.py --check scripts/install.sh
cargo test -p podrunner --test doctor
```

## Done

2026-11-15, `5c8720a`. `scripts/install.sh`, `scripts/install-pins.py`, `scripts/make-bundle.sh` and `docs/install.md`; `podrunner doctor --run` reads `run.toml` (`crates/podrunner/src/cli.rs`). `sh scripts/test-install.sh`: 10 passed (the pins of the tree and a planted pin; the copy of the tree refuses; a fresh install; a second run with an empty mirror downloads nothing; a changed binary and a partial file are replaced; a corrupt asset fails by name and leaves no file; a `run.toml` of the user stays; the bundle as an archive and as a directory, with `--prefix` and `--full`; shellcheck 0.11.0). A planted pass of a corrupt file and a planted download on the second run each fail the test. `cargo test -p podrunner --test doctor`: 3 passed; the new test fails on the code before (exit 64). `sh scripts/agent/check.sh`: green. With the real assets of `v0.1.0-beta.6`, podvm `v0.2.0-beta.21`, podnet `v0.4.0-beta.8` and a small image: the install takes 6 s from a local mirror (the downloads of 275 MB took 15 s here, so about 35 s more with the base image), a second run 0.1 s; `podrunner doctor --run` of this commit passes each check on the installed `run.toml`, and `podrunner run DATA/hello.yml --runner-dir DATA/runner/2.338.2` gives `ok hello` in 3 s. Not proven: the load of a real runner image (T-205).


# T-203: A local checkout with no --checkout fails with a 404

**Source:** T-202 (found in the work)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The installer writes `run.toml` so that `podrunner run FILE` needs no flag of the executor. A real workflow starts with `uses: actions/checkout@...`. With no `--checkout`, the broker answers the route of the tree with 404 (`crates/podrunner-broker-fake/src/actions.rs`, the route `_checkout.tar.gz`), and the step fails with an error of curl that does not name the cause. A `run.toml` of the user cannot give "the current directory": a relative path is read from the directory of the file. So the next command of the installer must be `podrunner run .github/workflows/ci.yml --checkout .`.

## Premise

Read `crates/podrunner/src/run_local.rs` (the tree of `--checkout`) and `crates/podrunner-local/src/message.rs` (`checkout_script`) on 2026-11-15. Not measured: the cost of a tarball of a large tree for each run.

## Approach

Choose one, and record the call in `docs/decisions.md`:

- With no `--checkout`, use the git tree of the current directory when a step of the run uses the local checkout (as act copies the working tree), and name it on stderr.
- Or fail the job before it starts, with the cause and the fix (`--checkout .`).

Then the installer prints `podrunner run .github/workflows/ci.yml` as the next command.

## Prove

`cargo test -p podrunner --test local` (a workflow with `uses: actions/checkout@v4` and no `--checkout`: the step gets the tree, or the job fails with the fix before a unit starts).

State (2026-11-16): the fourth QA (agent B, F3) met it again: `--dry-run` says `planned`, `doctor --run` says `ok`, and the step fails with a raw curl 404.

## Done

2026-11-16: the first choice of the Approach: with no `--checkout`, a workflow that uses `actions/checkout@` takes the git tree of the current directory, as act does, and a line on stderr says so (`args::default_checkout`, for `run`, the dry run and `doctor --run`); out of a git tree nothing changes. The call is in `docs/decisions.md`. Proven on a constrained host as a user (`qa5`, podvm): a job with `actions/checkout@v4` and no `--checkout` lists the workflow directory of the tree and ends `ok`.


# T-204: The packages of the image archives and their sources

**Source:** T-202 (found in the work)
**Category:** chore
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

From T-202, a release redistributes the runner image archives (`runner-ARCH.tar`, `runner-full-ARCH.tar`) as assets. They hold Ubuntu packages under the GPL and other licences (`images/runner/Dockerfile`, `images/runner/full.txt`). A redistribution of a GPL binary must tell where its source is. `THIRD_PARTY.md` names the packages in general only.

## Premise

Read `images/runner/Dockerfile` and `THIRD_PARTY.md` on 2026-11-15. The base image is pinned by digest; the packages of `apt-get install` are not pinned, so each build can take other versions. Not measured: the list of packages of a built image.

## Approach

In `runner-image.yml`, after the build, write the list of the packages of each image (`dpkg-query -W -f '${Package} ${Version} ${source:Package} ${source:Version}\n'` in the image) to `runner-ARCH.packages.txt`, upload it with the archive, and let the release attach it beside each archive. `THIRD_PARTY.md` names the list and the Ubuntu archive (`apt-get source`, and snapshot.ubuntu.com for an older version) as the place of the sources.

## Prove

`sh scripts/test-install.sh` stays green, and a `runner-image.yml` run uploads `runner-x86_64.packages.txt` with one line for each package of the image.


# T-205: The release attaches the assets of the local mode

**Source:** T-202 (the owner, 2026-11-15)
**Category:** release
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** partial

## Problem

The installer of T-202 needs its files at fixed places with no token: the runner image archives are workflow artifacts of `runner-image.yml` only, which need `gh` and a token, and expire after 30 days. The owner asks the release to attach the image archives, the installer with the pins of the release and one offline bundle for each architecture, with their `.sha256` files and attestations, under the limit of 2 GiB for a release asset.

## Premise

Read `.github/workflows/release.yml` and `.github/workflows/runner-image.yml` on 2026-11-15. The artifact `runner-ARCH` holds the raw archive of buildx; the step "Import into a podvm store" makes the archive that `podvm load -i` takes and does not upload it. The larger image is built on x86_64 only. The sizes of the artifacts of runner-image run 43704601731: 306 MB, 215 MB and 482 MB. No release run and no `runner-image.yml` run of this change ran.

## Approach

- `runner-image.yml`: the artifact `runner-ARCH` is the archive that `podvm load -i` takes (the ref name in `index.json`, the files of the layout only); the larger image is built, loaded and run on aarch64 too.
- `release.yml`: the job `check` finds the green `runner-image.yml` run of the commit (a release fails with no such run); the job `assets` downloads its archives, checks their sums, stamps the installer (`scripts/install-pins.py --release`), makes the bundles (`scripts/make-bundle.sh`) and fails on a file of 2 GiB or more; the job `release` attaches each file and attests the installer, the bundles and the image archives.

## Prove

A `runner-image.yml` run of the commit uploads `runner-ARCH` and `runner-full-ARCH` for x86_64 and aarch64; a dry run of `release.yml` makes the artifact `local-mode` (`gh run download RUN -n local-mode`); the next release attaches `podrunner-install.sh`, `runner-ARCH.tar`, `runner-full-ARCH.tar` and `podrunner-bundle-ARCH.tar.gz` with their `.sha256` files and attestations, and on a clean host:

```sh
sh podrunner-install.sh && podrunner run "$HOME/.local/share/podrunner/hello.yml"
```

# T-206: `--platform` turns off each later check of the flags of `run`

**Source:** GitHub #155 (a user agent of the local mode, #153, 2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`parse` of `run` returned after the checks of `--platform`, before the checks of `--jobs`, `--toolcache`, `--export`, `--environment-allow`, `--allow`, `--podnet` and `--engine`. One `--platform` let each of these combinations run.

## Premise

The report of GitHub #155, read on 2026-11-15, and the code of `main` at `e9b0cff`.

## Approach

The checks of each flag run whatever `--platform` holds; `--platform` keeps its own two checks.

## Prove

`cargo test -p podrunner --lib a_platform_keeps_each_check_of_the_flags`: each combination is refused with and with no `--platform`.

## Done

2026-11-15: `crates/podrunner/src/run_local/args.rs`: the `--platform` branch returns only after the checks of the flags. `flags_tests::a_platform_keeps_each_check_of_the_flags`.

# T-207: The refusal of an executor of `run` does not name the flag that clashes

**Source:** GitHub #157 (a user agent of the local mode, #153, 2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`--runner-dir` with `--image`, `--images` or a unit kind got "one of --runner-dir DIR, or a unit kind ... is necessary", which denies a flag on the command line and names no flag that clashes.

## Premise

The report of GitHub #157, read on 2026-11-15, and the code of `main` at `e9b0cff`.

## Approach

Each case has its own text: two executors, `--images` or `--image` with no kind, a kind with no `--images` or no `--image`, and no executor.

## Prove

`cargo test -p podrunner --lib the_refusal_of_an_executor_names_its_flag`.

## Done

2026-11-15: `args.rs` names the flag of each case (`--runner-dir and --podvm are two executors: give one, or map labels to each with --platform`). `flags_tests::the_refusal_of_an_executor_names_its_flag`.

# T-208: No command has its own help, and a short option of `run` becomes a file

**Source:** GitHub #158 (a user agent of the local mode, #153, 2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`run --help` was an unknown option, `doctor --help` needed a value, `version` and `help` were unknown commands, and `run -h` or `run -x` became the workflow file.

## Premise

The report of GitHub #158, read on 2026-11-15, and the code of `main` at `e9b0cff`.

## Approach

`podrunner help [COMMAND]`, `COMMAND --help` and `COMMAND -h` print the usage of the command (for `run`, `run_local::USAGE`) at exit 0; `version` prints the version; a short option that `run` does not have is an unknown option.

## Prove

`cargo test -p podrunner --lib each_command_has_its_help` and `the_refusal_of_an_executor_names_its_flag` (the short option).

## Done

2026-11-15: `crates/podrunner/src/help.rs`; `cli.rs` asks it first; `args.rs` refuses an unknown short option. `USAGE` names `version` and `help`.

# T-209: `run --validate` says valid for a dangling need and a cycle

**Source:** GitHub #159 (a user agent of the local mode, #153, 2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`--validate` read the file and its warnings only; `--graph` and `--dry-run` refused a need that is not a job and a cycle of needs.

## Premise

The report of GitHub #159, read on 2026-11-15, and the code of `main` at `e9b0cff`.

## Approach

`--validate` builds the graph of the needs, as a plan does, and refuses with the same text.

## Prove

`cargo test -p podrunner --lib validate_refuses_a_dangling_need_and_a_cycle`.

## Done

2026-11-15: `inspect::answer` (`crates/podrunner/src/run_local/inspect.rs`) checks the graph for `--validate`; `run_local.rs` gives it `--validate`, `--list` and `--graph`.

# T-210: `--break` takes a job, a step and a step id that the workflow does not have

**Source:** GitHub #160 (a user agent of the local mode, #153, 2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`--break JOB:STEP` checked the form only: a job that is not in the file, a step past the last one, or an id of no step planned a run whose pause never came.

## Premise

The report of GitHub #160, read on 2026-11-15, and the code of `main` at `e9b0cff`.

## Approach

Check each `--break` against the parsed workflow before the run, in the words of `--job`.

## Prove

`cargo test -p podrunner --lib a_break_names_a_job_and_a_step_of_the_file`.

## Done

2026-11-15: `pause::check` (`crates/podrunner/src/run_local/pause.rs`): the job of the file and a step by number or by id; usage exit 64 with the file, the jobs or the steps.

# T-211: `--local-repository` still opens the relay of the fetch

**Source:** GitHub #161 (a user agent of the local mode, #153, 2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A workflow whose every reference `--local-repository` serves still opened the egress grant and the relay of the fetch; on a host that denies a loopback bind the run stopped before the local action was read.

## Premise

The report of GitHub #161, read on 2026-11-15, and the code of `main` at `e9b0cff`.

## Approach

The call of 2026-11-15 (`docs/decisions.md`): when `--local-repository` serves each reference of the file, the run asks no forge and opens no relay; a reference that a local action names comes from the lock and the store, as with `--offline`.

## Prove

`cargo test -p podrunner --lib a_local_repository_opens_no_relay`: a forge with no host (`--action-server http://`) fails the old code, and the run plans now.

## Done

2026-11-15: `crates/podrunner/src/run_local/actions.rs`: the mode is `Offline` when each reference is local. `flags_tests::a_local_repository_opens_no_relay`.

# T-212: `doctor --run` answers a missing podvm binary with a version of podvm

**Source:** GitHub #156 (a user agent of the local mode, #153, 2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

Each failure of the probe became "take podvm VERSION or later", also for a path that is not a file or a binary that is not podvm, and `--incus` was named `podvm`.

## Premise

The report of GitHub #156, read on 2026-11-15, and the code of `main` at `e9b0cff`.

## Approach

Only a podvm that answers with a low version gets the version fix; a path that is not a file, or a binary that does not answer, gets a path fix that names its flag.

## Prove

`cargo test -p podrunner --lib doctor_names_a_missing_podvm_as_a_path`.

## Done

2026-11-15: `crates/podrunner/src/run_local/doctor.rs`: the fix by the code of the error and by the file; the text names `--podvm`, `--microvm` or `--incus`.

# T-213: A loopback bind that the host refuses is a raw error, and `doctor --run` does not try it

**Source:** GitHub #162 (a user agent of the local mode, #153, 2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A run failed with `the fake broker: Permission denied (os error 13)` and `--web` the same, with no address and no fix. `doctor --run` did not try the bind, nor the read of each directory above the runner directory that the runner needs.

## Premise

The report of GitHub #162, read on 2026-11-15, and the code of `main` at `e9b0cff`.

## Approach

The errors name the bind of `127.0.0.1:0` and the fix; `doctor --run` binds `127.0.0.1:0` (`loopback`) and reads each directory above the runner directory (`runner-dir-parents`).

## Prove

`cargo test -p podrunner --lib doctor_names_a_missing_podvm_as_a_path` (the `loopback` check is in the output).

## Done

2026-11-15: `podrunner_local::exec::broker_error`; `web.rs`; the two checks of `doctor.rs`.

# T-214: `status`, `drain`, `stop`, `check` and `audit verify` do not name the path that is missing

**Source:** GitHub #163 (a user agent of the local mode, #153, 2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

A missing state directory, a directory with no socket and a missing socket gave one line, `the control socket: entity not found`; `check` and `audit verify` named no file.

## Premise

The report of GitHub #163, read on 2026-11-15, and the code of `main` at `e9b0cff`.

## Approach

Name the path: `--state DIR: not a directory`, `the control socket PATH: ...`, `the configuration PATH: ...`, `the audit file PATH: ...`. The class stays.

## Prove

`cargo test -p podrunner --lib each_failure_names_its_path`.

## Done

2026-11-15: `verbs::no_socket` (`crates/podrunner/src/verbs.rs`); the configuration in `serve.rs`, `mint_serve.rs`, `doctor.rs` and `pin-sign`; the audit file in `audit_verify`. `stop_wait`, `audit_verify` and `human` moved from `cli.rs` to `verbs.rs`.

# T-215: `stop --wait` says stopped for a state directory that does not exist

**Source:** GitHub #164 (a user agent of the local mode, #153, 2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`stop --wait --state DIR` ignored the missing socket, read the lock of a directory that does not exist as free, and printed `stopped` at exit 0; `stop` with no `--wait` exits 69.

## Premise

The report of GitHub #164, read on 2026-11-15, and the code of `main` at `e9b0cff`.

## Approach

A path that is not a directory is refused before the wait, in the words of `stop`.

## Prove

`cargo test -p podrunner --lib each_failure_names_its_path`.

## Done

2026-11-15: `stop_wait` refuses `--state` that is not a directory, exit 69.

# T-216: `mint-serve` gives a TOML type error for a file of `serve`

**Source:** GitHub #165 (a user agent of the local mode, #153, 2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`mint-serve --config` on a file of `serve` failed with a type error at `host`; `check` said "the configuration is usable" and did not say which form it read.

## Premise

The report of GitHub #165, read on 2026-11-15, and the code of `main` at `e9b0cff`.

## Approach

`mint-serve` says that a file with no `cert_file` is not a mint configuration; `check` names the form that it read.

## Prove

`cargo test -p podrunner --lib each_failure_names_its_path`.

## Done

2026-11-15: `mint_serve::check` refuses a file with no `cert_file` by name; `check_any` says `(a file of serve)` or `(a file of mint-serve)`.

# T-217: The artifact `runner-ARCH.tar` cannot be loaded by the pinned podvm and has no ref name

**Source:** GitHub #166 (a user agent of the local mode, #153, 2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

`runner-image.yml` uploaded `runner-ARCH.tar` as `docker buildx --output type=oci` writes it: with the directory entries `blobs/` and `blobs/sha256/`, which podvm `0.2.0-beta.21` refuses, and with no `org.opencontainers.image.ref.name`. The import step built a good archive (`load.tar`) for its own check only. `runner-full-ARCH.tar` was good.

## Premise

The report of GitHub #166, read on 2026-11-15, and `.github/workflows/runner-image.yml` at `e9b0cff`.

## Approach

Upload the archive that the import step builds: the files of the layout only, with the ref name.

## Prove

`runner-image.yml` loads the artifact itself with `podvm load -i runner-ARCH.tar` before the upload; the installer of T-202 loads the release asset as it is.

## Done

2026-11-15: the change of T-202 (`runner-image.yml`): the import step writes `runner-ARCH.tar` (files only, with the ref name), loads it, and that file is the artifact. It holds from the first run of `runner-image.yml` on the merge; the archives of run 43704601731 keep the defect. Whether podvm should take the directory entries of a layout belongs to podvm; podrunner needs no change of podvm.

# T-218: The nits of the local-mode user run

**Source:** the report of GitHub #153 (step 10, "Nitpicks measured and deliberately not filed", 2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

The user agent of #153 measured nits that it did not file: `--list --graph` printed both views; `doctor` said `0 GiB free` for 661 MiB; a second `--input` of one name took the last value with no word; the `run` line of `docs/cli.md` did not name `--keep-failed`; a dry run on `--runner-dir` planned a job with `services:` or `container:`, which its run refuses.

## Premise

The comments of GitHub #153, read on 2026-11-15.

## Approach

Refuse two views at once and a name of `--input` or `--var` given two times; give the free disk in MiB below 1 GiB; name `--keep-failed`; the dry run on a runner directory fails a job with containers as the run does.

## Prove

`cargo test -p podrunner --lib the_nits_of_the_user_run`.

## Done

2026-11-15: `args.rs` (the views, the names), `doctor.rs` (`free_disk`), `inspect::DryRun { host }` and `slots.rs`, `docs/cli.md`. `flags_tests::the_nits_of_the_user_run`. Two nits of podvm (no `--store` flag of `load` and `images`, and no attestation of its release) go to podvm by a COORD issue.

# T-219: An unknown `--event` gets the text of the `on:` filter, and `--event-path` is read after the filter

**Source:** the report of GitHub #153 (step 6, 2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`run --event notanevent` is refused with the text of the `on:` filter, not as an event that GitHub does not have. A missing `--event-path` file shows only after the filter takes the event.

## Premise

The comment of step 6 of GitHub #153, read on 2026-11-15. Not measured here.

## Approach

Read `--event-path` before the filter. Refuse an event name that is not an event of GitHub (the list of `pod-actions-workflow`, if it has one; else ask podcrates for it by a COORD issue) with the list.

## Prove

A test of `run`: an unknown event and a missing payload file each fail with their own text before the filter.

## Done

2026-11-15: `parse` of `run` refuses an event that `pod_actions_workflow::event_keys` does not know, and an `--event-path` that is not a file, before the filter. `flags_tests::an_unknown_event_and_a_missing_payload_fail_first`.


# T-220: The local mode cannot run on a host that refuses each AF_INET bind

**Source:** GitHub #167 (a user agent of the local mode, #153, 2026-11-15)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** partial

## Problem

On a host whose sandbox refuses `bind(2)` for AF_INET and AF_INET6 (loopback too), and allows AF_UNIX and connect, `run` ends before the first unit: the fake broker is `TcpListener::bind("127.0.0.1:0")` (`crates/podrunner-broker-fake/src/lib.rs`). podvm units run on such a host (the userland rung), and podnet's vnet shim gives a dynamic payload a listener with no kernel bind. podrunner is static, so the shim cannot reach it.

## Premise

The report of GitHub #167, read on 2026-11-15: `podnet net doctor` and a control script on the host of the user agent; podvm `0.2.0-beta.21` runs a unit there. Not measured here. Since T-213, `doctor --run` names the refused bind with a fix.

## Approach

1. The fake broker also serves a unix socket in the scratch directory of the job (no bind of AF_INET on the host).
2. The guest kinds (`microvm`, `incus`): the front of the egress socket connects to that unix socket for `broker.podrunner.test`, not to a TCP port of the host (`crates/podrunner-local/src/front.rs`).
3. The podvm kind: a unit shares the network of the host, and on such a host a process of a unit can neither bind nor connect on AF_INET (measured on #153 with a dynamic build of podrunner under podnet's shim: the unit started and the runner ran, but its connect to the broker got EACCES). So the runner of a unit runs under podnet's vnet shim, with the library mounted into the unit, and the vnet maps `127.0.0.1:PORT` to the unix socket of the broker (COORD-058, GitHub #170).
4. `doctor --run` says which executor works on a host that refuses the bind.

## Prove

A test of the guest kinds with the broker on a unix socket only. On a host that refuses AF_INET bind (a seccomp filter in the test), `run --podvm` with the shim of podnet runs a canned job.

History of the blocker:

2026-11-15: podnet: the release of PodUtils/podnet@T-356 (a mapped IPv6 address gives the IPv4 name of the vnet, and `PODNET_VNET_FORWARD`), PodUtils/podnet#582 (COORD-058, GitHub #170). Steps 1 and 2 (the unix socket of the broker, and the front of the guest kinds) need no other project.

State (2026-11-15): steps 1 and 2 are in: `Broker::start_unix` (`crates/podrunner-broker-fake/tests/unix.rs`), `front::Upstream` (`the_broker_on_a_unix_socket_gets_its_requests`), and a job of the guest kinds starts its broker on a unix socket, so a guest run binds no AF_INET socket on the host. Next: the guest steps of `runner-image.yml` on these commits; then step 3 with the answer of podnet.

State (2026-11-15, podnet answered on PodUtils/podnet#582): under the shim each AF_INET socket is an AF_UNIX socket; a connect to `127.0.0.1:PORT` goes to the abstract name `\0podnet-vnet/4/127.0.0.1/PORT`, and podrunner (static, no shim) can listen on that name itself, where the unit shares the network namespace of podrunner (to measure on the userland rung). The .NET runner connects through a dual-mode AF_INET6 socket (`[::ffff:127.0.0.1]`), which the shim does not map to the IPv4 name yet: PodUtils/podnet@T-356 fixes it, and adds `PODNET_VNET_FORWARD=127.0.0.1:PORT=unix:PATH`. The unit needs one file, `libpodnet-vnet.so` (`podnet net shim --out DIR`, built against glibc for the runner), after podvm's interposer in `LD_PRELOAD`. The egress proxy goes the same way. Next: the release of podnet with that fix; then the broker of a podvm job on the abstract name (or a forward), the shim in the unit, and a measure on a host that refuses AF_INET bind.

State (2026-11-16): the state block of PodUtils/podnet#582 says PodUtils/podnet@T-356 and PodUtils/podnet@T-357 are done and go into the next tag of podnet. Waits for that tag.

State (2026-11-17): the state block names the tag `v0.4.0-beta.14` for PodUtils/podnet@T-356 and PodUtils/podnet@T-357; the newest release is still `v0.4.0-beta.13`. podnet did not measure a podvm unit, the real runner or .NET. Waits for the release of `v0.4.0-beta.14`.

State (2026-11-17): podnet `v0.4.0-beta.14` is released with PodUtils/podnet@T-356 and PodUtils/podnet@T-357 (`PODNET_VNET_FORWARD`); `beta.15` is the newest. Steps 1 and 2 are in the code: a guest reaches the broker on a unix socket through its front (`Broker::start_unix`, `exec.rs`). Next, step 3: the runner of a podvm unit under the vnet shim (`podnet net shim --out DIR`), with `PODNET_VNET_FORWARD=127.0.0.1:PORT=unix:SOCK` to the broker. The library and the socket must reach the unit with no volume, or the unit leaves the namespace rung (T-261): a design question first. Measure it in the lane `bind-denied` of the user-path gate.

State (2026-11-17): step 3 is in the code, after the measure of a user on #170 (the podvm kind on the unix broker and the front ran a hello job on a host that refuses the loopback bind). `--vnet PATH` (with `--podvm`, not with `--podnet`): the broker of each job on a unix socket, the front in the socket directory of the unit (the broker, and the egress module for `--allow`), the shim `libpodnet-vnet.so` copied into that directory, and `LD_PRELOAD`, `PODNET_VNET_FORWARD=127.0.0.1:3128=unix:MOUNT/egress.sock` and the proxy variables in the unit (`exec/vnet.rs`, `run_local/vnet.rs`); the container hooks stay. PATH is the shim or a podnet binary that builds it (`podnet net shim --json --out DIR`). Proven in CI on GitHub-hosted runners: the environment, the shim path and the refusal with no socket volume (unit tests). Not run: a job under `--vnet`. Next: a step of the lane `bind-denied` of the user-gate with `--vnet`; step 4 (`doctor --run` names `--vnet` on a host that refuses the bind).

State (2026-11-17): step 4: the fix of a refused bind (`doctor --run` and the error of the broker) names `--podvm` with `--vnet PODNET`; with `--vnet` or a guest, the `loopback` row of `doctor --run` says that the broker needs no bind (`exec::BIND_FIX`). Open: a run with `--vnet` (the `lab:vnet` measure of the user-gate, and a user on #170).

# T-221: `cpus` on the podvm kind

**Source:** T-162 (the cgroup half of COORD-050, GitHub #110)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

The podvm kind refuses `[unit.podvm] cpus`, and takes no `cpu` of an offer: podvm `run -d` holds no CPU limit with no root (no cgroup that the user owns).

## Premise

podvm answered COORD-050 on PodUtils/podvm#382: the cgroup half (`--memory`, `--cpus`, `--pids-limit` on a delegated subtree) is PodUtils/podvm@T-1747, open.

## Approach

When podvm releases it: give `--cpus` to `podvm run -d` where the host delegates a cgroup, say `cpu` `enforced` in the capability of the kind there, and keep the refusal elsewhere.

## Prove

A unit test of the arguments, and a measure in `runner-image.yml`: a payload that spins on two CPUs under `cpus = 1` uses one.

History of the blocker:

2026-11-15: PodUtils/podvm@T-1747 (COORD-050, GitHub #110). 2026-11-17: podvm `0.2.0-beta.23` holds `--memory`, `--cpus` and `--pids-limit` in a delegated cgroup v2 subtree (PodUtils/podvm@T-1747).

State (2026-11-17): the podvm kind takes `[unit.podvm] cpus` and the `cpu` of an offer where this process is in a cgroup v2 directory of its user with the `cpu` controller and podvm is `0.2.0-beta.23` or later (`unit/limits.rs`, `cpu_held`); a unit gets `--cpus N`; elsewhere `cpus` is refused with the reason and an offer's `cpu` is not held. `a_cpu_limit_needs_a_delegated_cgroup` holds the detection; `tests/limits.rs` has `a_cpu_limit_holds_in_a_delegated_cgroup`, which the measure of `runner-image.yml` runs in a transient service of systemd with `Delegate=yes`. Next: that measure.

State (2026-11-17, the measure): runner-image 44075936648: in a service of systemd with `Delegate=yes`, podvm refused `--cpus`: it makes the leaf of a run beside the cgroup of its caller (`system.slice/podvm-...`: Permission denied), so the parent must be the delegated directory, as `app.slice` of a systemd user session. `delegated` now checks the parent, and the measure moves the test into a leaf of the service first. Next: that measure.

State (2026-11-17): the measure of runner-image 44085737348 (`8a4ad2f`) did not reach its test: the shell of the delegated unit ended at `echo: I/O error` before the test (a write to the cgroup files). The step now writes each controller alone, and prints the cgroup, its controllers and the modes, so the next run names the cause.

State (2026-11-17): the cause, from runner-image 44100036389: the delegated cgroup has each controller (`cpuset cpu io memory hugetlb pids ...`), but systemd-run read `$$` of the command as one `$`, so the shell wrote `$` into `cgroup.procs` (EIO). The step writes `0` now, which moves the shell itself.
State (2026-11-17): runner-image 44153617910 reached the test: podvm made the cgroup of the run in the delegated directory (`run-u29.service/podvm-...`), but the payload could not read its `cpu.max` (exit 13): a unit has no cgroup file system in its root. The test now reads `cpu.max` of the new `podvm-*` cgroup beside its own, on the host, while the unit sleeps. Next: that measure.

## Done

2026-11-17: runner-image 44170614365 (`5095d91`): in a transient service of systemd with `Delegate=yes`, a podvm unit with `cpus = 1` runs in the cgroup `run-u27.service/podvm-...`, whose `cpu.max` is `100000 100000` (one CPU); `a_cpu_limit_holds_in_a_delegated_cgroup` passes. Level: proven in CI on GitHub-hosted runners. Elsewhere `cpus` stays refused with its reason.

# T-223: `doctor --run` says ok for a store that does not hold `--image`

**Source:** GitHub #172 (a user agent of the local mode, #153, 2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The `store` row of `doctor --run` checked that `--images` is a directory only; an empty store and a store with no `--image` were `ok`, and the run failed later in `podvm extract`.

## Premise

The report of GitHub #172 (2026-11-15).

## Approach

A row `image`: `podvm inspect IMAGE` in the store, with a fix that names the load.

## Prove

`cargo test -p podrunner --lib doctor_checks_the_image_of_the_store_and_names_the_hold`.

## Done

2026-11-15: `image_in_store` (`crates/podrunner/src/run_local/doctor.rs`), with the test above (a fake podvm).

# T-224: A run that hangs prints nothing until it ends

**Source:** GitHub #173 (a user agent of the local mode, #153, 2026-11-15)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

The output of a unit, and the lines of the broker, come only when the job ends. A run that waits until `timeout-minutes` prints nothing for minutes, also with `--json`, so its phase is invisible.

## Premise

The report of GitHub #173 (2026-11-15): 180 s with no byte on stdout or stderr.

## Approach

1. Stream the output of the unit as it comes (the console of the runner), with the job as its prefix. 2. `--json`: phase events (`unit_started`, `runner_connected`, `job_received`, `broker_request`, `unit_ended`) with a time. 3. A line after N seconds with no event, that names the phase and the count of requests of the broker. 4. A job that `timeout-minutes` ends says what it waited for.

## Prove

A test of `run` with a fake unit that never takes its job: a phase line on stderr before the end, and the `--json` events in order.

## Done

2026-11-17: the live sink writes `podrunner: no line for 60 s; the run waits for its job (N s so far; Ctrl-C ends it)` (with `--json`, `{"event":"quiet",...}`) after each 60 s with no line of a job, until the run ends (`logs::watch_quiet`); `-q` writes none. Each line of a step comes live since T-310, and the start of a unit has its line since T-325. Proven on a constrained host as a user (`qa5`, podvm): a step `sleep 75` gave one such line between `before` and `after`.

# T-225: A unit that has no room says `PODVM_STORE`, which podrunner sets itself

**Source:** GitHub #174 (a user agent of the local mode, #153, 2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The store of each unit of `run` is under `TMPDIR`; podvm's advice on a full store names `PODVM_STORE`, which podrunner sets for each unit, so the advice cannot help.

## Premise

The report of GitHub #174 (2026-11-15); `crates/podrunner/src/run_local.rs` (the state of the units under `std::env::temp_dir()`).

## Approach

The failure of a start that names `PODVM_STORE` adds the fix of podrunner: set `TMPDIR`, with the path. A flag for the directory of the units is part of T-229.

## Prove

`cargo test -p podrunner-local --lib a_full_store_names_tmpdir`.

## Done

2026-11-15: `start_error` (`crates/podrunner-local/src/exec.rs`).

# T-226: A run does not say what egress hold its units have

**Source:** GitHub #175 (a user agent of the local mode, #153, 2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A run with `--podvm` and no `--podnet` gave its units the network of the host with no word; with `--podnet` the hold is the advisory seccomp hold, also with no word; `doctor --run` had no row.

## Premise

The report of GitHub #175 (2026-11-15).

## Approach

One line at the start of a run (`podrunner: the egress hold: ...`) and a row `hold` of `doctor --run`, from one function.

## Prove

`cargo test -p podrunner --lib doctor_checks_the_image_of_the_store_and_names_the_hold` (the `hold` row).

## Done

2026-11-15: `doctor::hold` (`crates/podrunner/src/run_local/doctor.rs`); `run_local.rs` writes the line on stderr at the start (not with `-q`, not in a dry run).

# T-227: `podvm run -d` refuses a passed descriptor when podrunner runs under `podnet pod`

**Source:** GitHub #176 (a user agent of the local mode, #153, 2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

Under `podnet pod` (the seccomp backend), `run --podvm` fails at the start of the unit: `podvm run: exit 125: a passed descriptor goes onto 17, the number of the root file system or of the memfd`. podvm alone under the same supervisor starts the same command.

## Premise

The report of GitHub #176 (2026-11-15). podrunner passes no descriptor to `podvm run` (no `pre_exec`, no raw descriptor; each descriptor of Rust is close-on-exec).

## Approach

Ask podvm by COORD-059 (GitHub #177) how `run -d` numbers its descriptors and to take a range that cannot collide; then a run under `podnet pod` on a host with no AF_INET bind.

## Prove

A run under `podnet pod --egress direct` starts its unit (a measure on a host of the class of #167).


State (2026-11-15): podvm `0.2.0-beta.24` (pinned) holds descriptor 17 from its start. The lane `inherited-fds` of `user-gate.yml` runs podvm units with descriptors 3 to 15 open (the class of #176). Next: that lane green, and a run under `podnet pod` on a host of the class of #167.

## Done

2026-11-17: the lane `inherited-fds` of the user-path gate (descriptors 3 to 15 open) is green in user-gate 44051472680 on podvm `0.2.0-beta.30` (proven in CI on GitHub-hosted runners); runs under `podnet pod` start each unit (proven on a constrained host as a user, `qa5`). GitHub #177 closed.


# T-228: A listener of the loopback needs `getsockname`, which `podnet pod` refuses

**Source:** GitHub #167 (comment of 2026-11-15) (a user agent of the local mode, #153, 2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The broker and the fetch relay bind `127.0.0.1:0` and read the port with `local_addr` (`getsockname`); under `podnet pod` that call fails (`EOPNOTSUPP`), so neither can listen, although the bind holds.

## Premise

The comment of 2026-11-15 on GitHub #167: with a fixed port the broker binds under the supervisor.

## Approach

`bind_loopback`: bind port 0 and read it; when the read fails, bind ports of the dynamic range from a start that each process draws.

## Prove

`cargo test -p podrunner-local --lib bind_loopback_gives_a_port_that_answers` (the usual path; the fallback needs a host of the class of #167).

## Done

2026-11-15: `podrunner_broker_fake::bind_loopback`, in `Broker::start_seen_at` and `fetch::Relay::start`.

# T-229: The run should pick its transport, its stores and its hold, and say so

**Source:** GitHub #171 (a user agent of the local mode, #153, 2026-11-15)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** open

## Problem

On a host that refuses AF_INET bind, a run needs the user to know the transport, `TMPDIR` and the hold from four documents. The fetch relay is a second loopback listener that the plan of T-220 does not cover.

## Premise

The report of GitHub #171 (2026-11-15) and the call of 2026-11-15 in `docs/decisions.md` (the release stays static).

## Approach

1. The fetch relay through the same transport as the broker (a unix socket, as `podrunner-egress` serves). 2. At the start of a run, and in `doctor --run`: the transport, the store, the directory of the units and the hold (the hold is T-226). 3. A flag (or a key of `run.toml`) for the directory of the units, and a default with room. 4. The choice of the transport from the host (a loopback bind, else a unix socket and podnet), with an override.

## Prove

On a host that refuses AF_INET bind: `podrunner run FILE --checkout .` with the `run.toml` of the installer runs a job, or refuses before the run with the transport it could not make.

# T-230: With no `--platform`, a job of Windows, macOS or another architecture runs on the host

**Source:** the QA report of a second user agent of the local mode (Debian 13 in LXC, user namespaces, KVM, a host behind an authenticated proxy; podrunner `0.1.0-beta.6`), given by the owner on 2026-11-15
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

`routes::build` returned the default executor with no dispatch when no `--platform` was given, so the check of the labels of `Dispatch::route` did not run: a `windows-latest`, `macos-*` or `ARM64` job ran on the host and could end `ok`.

## Premise

Code read: `crates/podrunner/src/run_local/routes.rs`, `crates/podrunner-local/src/dispatch.rs`.

## Approach

Wrap the default executor in a `Dispatch` with no routes.

## Prove

`cargo test -p podrunner --lib a_foreign_label_fails_with_no_platform` (it fails on the code of before).

## Done

2026-11-15: `routes::build`; `flags_tests::a_foreign_label_fails_with_no_platform`.

# T-231: Ctrl-C in the wait of `--schedule` exits 0

**Source:** the QA report of a second user agent of the local mode (Debian 13 in LXC, user namespaces, KVM, a host behind an authenticated proxy; podrunner `0.1.0-beta.6`), given by the owner on 2026-11-15
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

A signal while `run --schedule` waits for the next time of its crons returned the result of the last run, exit 0 before a first run.

## Premise

Code read: `crates/podrunner/src/run_local/watch.rs`.

## Approach

Exit 130 at a signal in the wait, as at a signal in a run.

## Prove

By hand: `podrunner run FILE --schedule --runner-dir DIR`, Ctrl-C in the wait, `echo $?` gives 130.

## Done

2026-11-15: `watch.rs`: `Out { code: 130, ..last }`.

# T-232: A tar with a link before its target fails in a podvm unit

**Source:** the QA report of a second user agent of the local mode (Debian 13 in LXC, user namespaces, KVM, a host behind an authenticated proxy; podrunner `0.1.0-beta.6`), given by the owner on 2026-11-15
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

In a `--podvm` unit, `tar -x` of an archive with a symbolic link before its target, or a dangling link, exits 2 on `utime` and `chmod`; `--microvm` and the host pass. It breaks `actions/setup-python` (`pallets/click`).

## Premise

The claim of the QA report (its `fixtures/tarprobe/`); not measured here. A likely cause is that the interposer of podvm follows the link and drops `AT_SYMLINK_NOFOLLOW`.

## Approach

A COORD issue to podvm with the probe of four tars; then pin the podvm that fixes it.

## Prove

`tar -x` of each tar of the probe in a podvm unit exits 0 (a test of `runner-image.yml`).


State (2026-11-15): podvm `0.2.0-beta.24` (pinned) has the fix. `runner-image.yml` extracts four tars (a link first, the target first, a dangling link, a hard link first) in a podvm unit of the runner image (step "The links of tar in a podvm unit"). Next: that step green.

## Done

2026-11-17: the step "The links of tar in a podvm unit" of `runner-image.yml` is green in run 44057392980 on podvm `0.2.0-beta.30` (proven in CI on GitHub-hosted runners). GitHub #178 closed.


# T-233: The egress of a job ignores the proxy of the host

**Source:** the QA report of a second user agent of the local mode (Debian 13 in LXC, user namespaces, KVM, a host behind an authenticated proxy; podrunner `0.1.0-beta.6`), given by the owner on 2026-11-15
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

On a host that reaches the internet only through a proxy, the egress module of a job connects directly (`Egress::host(None, ...)` in `run_local/guest.rs` and `routes.rs`), so an allowed host gets 502. The fetch of the actions reads the proxy of the environment (`fetch.rs`).

## Premise

The QA report; the code read confirms the two calls.

## Approach

Give the egress module of a job the upstream proxy of `pod_proxy::Environment`, as the fetch does.

## Prove

`cargo test -p podrunner-local egress_upstream`: an allowed host goes through the upstream proxy of the environment.

## Done

2026-11-17: the egress module of a job of the local mode takes the proxy that the environment names as its way out (`fetch::env_proxy`, in `routes.rs` and `guest.rs`), as the fetch of the actions does. Proven on a constrained host as a user (`qa5`, `--podnet --allow pypi.org`, the way out through the proxy of the host that intercepts TLS): `allowed host: http 200`, `other host: refused`. The check `hold:podvm` of the user-path gate runs it in the lane `proxy-only`, where a direct connection gets nothing.

# T-234: A job on `--runner-dir` drops the proxy of the host

**Source:** the QA report of a second user agent of the local mode (Debian 13 in LXC, user namespaces, KVM, a host behind an authenticated proxy; podrunner `0.1.0-beta.6`), given by the owner on 2026-11-15
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`runner.rs` clears the environment of the runner (`env_clear`), so a job on a runner directory has no proxy; the work-around with `--env-file` puts the credential of the proxy in the job.

## Premise

The QA report; code read confirms `env_clear`.

## Approach

Serve the job through a local relay of the proxy (as the fetch does) and give the runner `https_proxy=http://127.0.0.1:PORT`, with no credential in the job.

## Prove

A test: a step that reads `https_proxy` sees the relay, not the credential.

## Done

2026-11-17: fixed by T-265 and T-266: a job on a runner directory gets `PATH`, the proxy variables and the CA files of the environment (`PASSED` of `exec/host.rs`), with the loopback in `NO_PROXY`. Proven on a constrained host as a user by the fifth QA (agent 1: the proxy variables and the CA files reach the job on `--runner-dir`).

# T-235: Each log line of a job comes two times, with a BOM

**Source:** the QA report of a second user agent of the local mode (Debian 13 in LXC, user namespaces, KVM, a host behind an authenticated proxy; podrunner `0.1.0-beta.6`), given by the owner on 2026-11-15
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

The live log relays the blob of the job log and the blob of each step (`take_new`, `logs` of the broker), so each line comes two times; a U+FEFF of the runner leaks into the text.

## Premise

The QA report (`json.err`); code read confirms the two families.

## Approach

Relay one family of blobs, and drop a leading BOM.

## Prove

`cargo test -p podrunner-broker-fake live_log_once`: each line of a canned job comes once, with no U+FEFF.

# T-236: The events of `--json` share stderr with the console of the runner

**Source:** the QA report of a second user agent of the local mode (Debian 13 in LXC, user namespaces, KVM, a host behind an authenticated proxy; podrunner `0.1.0-beta.6`), given by the owner on 2026-11-15
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

With `--json`, the events of each log line and the raw console of the runner both go to stderr, so a reader cannot parse the stream.

## Premise

The QA report; code read (`logs.rs`, `runner.rs`).

## Approach

Each line of the console under `--json` becomes an event `{"event":"console"}`, or the events go to stdout.

## Prove

A test: with `--json`, each line of stderr is a JSON object.

# T-237: A Dockerfile action on `--podvm` fails at its step, not before the job

**Source:** the QA report of a second user agent of the local mode (Debian 13 in LXC, user namespaces, KVM, a host behind an authenticated proxy; podrunner `0.1.0-beta.6`), given by the owner on 2026-11-15
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`needs_containers` reads only the `containerRegistry` steps of a message, so a job with an action of `using: docker` and a Dockerfile goes to a podvm unit with no hooks and fails with `docker: command not found`.

## Premise

The QA report; code read (`crates/podrunner-local/src/exec.rs`).

## Approach

Read the `action.yml` of each fetched action: `using: docker` with `image: Dockerfile` (or a path) is refused before the job with `build-not-supported` on each kind but `microvm` with an engine.

## Prove

A test of `run --dry-run` on `--podvm` with a Dockerfile action: the job fails with `build-not-supported`.

## Done

2026-11-17: a job with an action of `using: docker` whose image is a Dockerfile (also in a composite action, and a `./` action of the tree) is refused before its unit starts on `--podvm`, in the run and in the dry run (`docker::build_refusal`): the hooks of a podvm unit run images and build none; an action of a `docker://` image still runs through the hooks. Proven on a constrained host as a user (`qa5`, podvm): `failure d: the job uses the Docker action ./act, which builds a Dockerfile: ...` in a dry run and a run, with no unit started.

# T-238: `--artifacts DIR` gives 409 on a second run

**Source:** the QA report of a second user agent of the local mode (Debian 13 in LXC, user namespaces, KVM, a host behind an authenticated proxy; podrunner `0.1.0-beta.6`), given by the owner on 2026-11-15
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The store of `--artifacts DIR` has no run id, so a second run that uploads the same name gets 409.

## Premise

The QA report; code read (`crates/podrunner/src/run_local.rs`).

## Approach

Key the store as `DIR/RUN_ID`; `--rerun` keeps the run id of its state.

## Prove

A test: two runs into one `--artifacts DIR` both pass, with two directories.

## Done

2026-11-17: fixed by T-334: a new run removes the artifacts of an earlier run from its artifact directory first, by name on stderr; a rerun replaces what it uploads again. Proven on a constrained host as a user (`qa5`, two runs of one upload into one `--artifacts DIR`).

# T-239: `--action-server` on the loopback or a private address is denied

**Source:** the QA report of a second user agent of the local mode (Debian 13 in LXC, user namespaces, KVM, a host behind an authenticated proxy; podrunner `0.1.0-beta.6`), given by the owner on 2026-11-15
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The fetch through the egress module refuses a forge on the loopback or a private network (`fixed:loopback`, `fixed:private`), so a local Gitea or Forgejo cannot serve the actions.

## Premise

The claim of the QA report; not measured here.

## Approach

Grant the one address of `--action-server` (the same one-target grant that T-187 needs of pod-broker).

## Prove

A test with a local git server on the loopback named by `--action-server`.

## Done

2026-11-17: when `--action-server` names the loopback, `localhost` or a private or link-local address, the fetch takes no egress grant: git reaches the forge directly, and a line says so (`actions.rs`, `on_local_network`); each other forge keeps the grant of its one host. Proven on a constrained host as a user (`qa5`, podvm): a composite action served by `git http-backend` on `127.0.0.1` ran (`from-the-local-forge`, `ok a`) and the lock pins it. A name that resolves to a private address is not looked up.

# T-240: `doctor --run --incus` does not try incus

**Source:** the QA report of a second user agent of the local mode (Debian 13 in LXC, user namespaces, KVM, a host behind an authenticated proxy; podrunner `0.1.0-beta.6`), given by the owner on 2026-11-15
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

`doctor --run` checks the podvm binary of `--incus`, not that an instance can start.

## Premise

The QA report; code read (`run_local/doctor.rs`).

## Approach

A row `incus` that starts and removes one instance of the image.

## Prove

`cargo test -p podrunner --lib doctor_tries_incus`: with a fake podvm whose incus tier fails, the row is FAIL with its fix.

# T-241: The small findings of the second QA report

**Source:** the QA report of a second user agent of the local mode (Debian 13 in LXC, user namespaces, KVM, a host behind an authenticated proxy; podrunner `0.1.0-beta.6`), given by the owner on 2026-11-15
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

`--env-file` keeps one of `FOO` and `foo` with no word; `doctor` with no configuration exits 0; `--json` does nothing with `--list` and `doctor --run`; the runner images have no Rust; nothing says that the userland rung of podvm gives no isolation.

## Premise

The claims of the QA report; not measured here.

## Approach

Refuse names that differ only by case in `--env-file`; a JSON form of `--list` and `doctor --run`; list the tools that the images do not have (or add a Rust row to `runner-full`); a row of `doctor --run` with the isolation that `podvm probe` names.

## Prove

`cargo test -p podrunner --lib the_small_findings_of_the_second_qa_report`: one case for each change.

# T-242: Each run copies the runner image several times into a new directory

**Source:** the QA report of a second user agent of the local mode (Debian 13 in LXC, user namespaces, KVM, a host behind an authenticated proxy; podrunner `0.1.0-beta.6`), given by the owner on 2026-11-15
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** partial

## Problem

The state of the units of a run is under a directory of the process, so its cache of image tars is never used again: each run does `podvm save` of the image (about 300 MB), and each unit `load` and `extract` (about 1.2 GB), about 1.5 GB of writes for each job. A user agent ran out of its write budget (23.7 of 26.8 GB) in one session.

## Premise

The transcript of a user agent of #153, read by an agent on 2026-11-15; code read (`crates/podrunner/src/run_local.rs`, `crates/podrunner-podvm/src/unit.rs`).

## Approach

Keep the tar of each image in a cache of the user, keyed by its digest, across runs; extract one tree for each run and give each unit a copy-on-write or a fresh copy only where the rung needs it.

## Prove

A measure in `runner-image.yml`: `podrunner run` of one job two times on `--podvm`, and the bytes written (`/proc/self/io` of the run): the second run writes no tar of the image.

State (2026-11-17): the local mode keeps the tar of each image in the cache of the user (`~/.cache/podrunner/image-tars/DIGEST.tar`; `PodvmUnit::with_image_cache`, `unit/cache.rs`; the 4 newest by time of use stay), so a run saves an image once, not once for each run (about 300 MB of writes); a tar is written under a name of its process, then renamed. Proven on a constrained host as a user (`qa5`): two runs, one tar. Next: the load and the extract of each unit (about 1.2 GB for each job), which need one tree of the image for each run and a copy only where the rung needs it: a COORD issue to podvm.

# T-243: The user-path gate: the files of the release, installed and used as a user

**Source:** the owner, 2026-11-15: "why did podrunner not survive a contact with real users"
**Category:** release
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** partial

## Problem

No step of CI used the files that a release publishes as a user does. The runner-image workflow loaded a copy of the image that it made, not the artifact that it uploaded (T-217); the release asset that a user downloads was never installed and run.

## Premise

The first user runs of `v0.1.0-beta.6` (GitHub #153, #155 to #178, and the QA report of 2026-11-15).

## Approach

A workflow `user-gate.yml`, on the commit of a release candidate: build the assets as `release.yml` does (or take them from its dry run); install them with the installer as a user with no root; run `doctor --run`; run each command of the quick start of `README.md` and `docs/install.md` as it is written; run a pinned corpus of real public projects (ripgrep, click, express, requests, and one with `services:`) on `--runner-dir`, `--podvm` and `--microvm`, and compare the result of each job with the result of GitHub at that commit. `release.yml` refuses a commit with no green run of it.

## Prove

`gh workflow run user-gate.yml`: green on a commit; red on a commit that publishes an image archive that the pinned podvm cannot load (the defect of T-217, put back on a branch).

State (2026-11-15, the first draft): `.github/workflows/user-gate.yml` (dispatch; the inputs `images_run` and `version`): `build` and `assets` make the files with `scripts/release-assets.sh`, which `release.yml` now calls too (the binaries, the image archives of the green `runner-image.yml` run of the commit, the installer with its pins, the bundles); `lanes` runs `scripts/user-gate.py lane` as a user with no root on each host class of T-244; `summary` writes one table, check x lane. A lane runs the quick start of `docs/install.md` as it is written (online, from a local HTTP server with the paths of the release), the bundle with no network (`--bundle`, then the block of "The offline bundle", then `--full`), `doctor --run` and `hello.yml` on each executor (`--microvm` in `plain`), and the corpus `scripts/corpus/user-gate.tsv`: ripgrep (rustfmt, docs), click (tests, Python 3.13), express (lint, Node.js 22), rq (lint; valkey-test with `services:` as xfail, T-260, T-124, T-218) at pinned commits, on `--podvm` and `--runner-dir` (rustfmt on `--microvm` too), against the result of github.com. `release.yml` refuses a version with no green `user-gate.yml` run on its commit; the input `no_user_gate` overrides it, and the notes must then say `user-path gate: not run`. Found and fixed in the same work: the job `check` of `release.yml` had no `actions: read` for the runs that it reads; it checked the notes after the push of the tag, so a release with no notes left a tag that never moves; a dry run gave the installer a version (`-dry`) that the binaries did not have. Measured in this sandbox: `sh scripts/test-install.sh` 13 passed (new: the assets from `release-assets.sh`, an install from a local HTTP server, a lane on fake assets with the quick start and the bundle, and a lane with no larger image that fails); a lane with the corpus on fake binaries clones each project at its commit; the real `podrunner` of this commit plans each corpus row on `--runner-dir` with `--dry-run`, and refuses rq-valkey (T-260); actionlint 1.7.7 with shellcheck 0.11.0 passes both workflows. Not run: `user-gate.yml` itself. A run must prove the lab scripts on a hosted runner, the installs and the corpus as the lab user, the expected results on each executor, the KVM step for `--microvm`, the disk room, and the times (the lane has 240 minutes); then the planted defect of the Prove.

State (2026-11-15, the first run): user-gate 43743490768 on `f0fd5f2` (podvm beta.22): in each of the four lanes the install from the bundle and from a mirror, the quick start, `doctor --run` and `hello` on podvm and on a runner directory passed. The corpus failed: the links of tar on podvm (T-232, T-253), the made-up token (T-254), the rate limit of the API (T-253), and express on `--runner-dir` (T-255); `--microvm` refused the binary of the install (T-253). Next: the run on the fixes and podvm beta.24.


# T-244: The host lanes of the user-path gate

**Source:** the owner, 2026-11-15: "why did podrunner not survive a contact with real users"
**Category:** release
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** partial

## Problem

Each defect class that the users found comes from a host that CI does not have: a refused AF_INET bind and connect, no user namespace, a proxy as the only way out (with a credential), a small `/tmp`, a `/` that cannot be read, a user with no root.

## Premise

The reports of GitHub #153, #162, #167, #174 and the QA report of 2026-11-15.

## Approach

Lanes of `user-gate.yml` on GitHub runners, each with the smoke of T-243: no user namespace (`kernel.apparmor_restrict_unprivileged_userns` or `user.max_user_namespaces=0`); egress through an authenticated proxy only (an iptables drop of direct traffic, and a proxy on the loopback); a seccomp filter that refuses AF_INET bind and connect (a small static wrapper); `TMPDIR` on a 1 GiB tmpfs; a user with no root; aarch64. A new host class of a report becomes a lane.

## Decision

The owner, 2026-11-15: the labs are shared crates or scripts, so that each project brings up each host class on each host; podcrates holds them and podvm gives the bulk. podrunner writes its lanes as a script for each profile (a new directory `lab` under `scripts`) in the form of COORD-061 meanwhile, as the first draft of the profiles.

## Prove

`gh workflow run user-gate.yml`: each lane has its own job, and a lane fails on the code before the fix of its class (for example the proxy lane on the code before T-233).

History of the blocker:

2026-11-15: podcrates: part 1 of `pod-lab` in the tag after `v0.53.0` (PodUtils/podcrates#180), and podvm's `podvm lab run` (PodUtils/podvm#424); COORD-061 (GitHub #180). The scripts of podrunner can go first.

State (2026-11-15): podvm answered on PodUtils/podvm#424 (PodUtils/podvm@T-1916): the machine tier takes the knobs of a profile (`--podvm-lab PROFILE`), and with no KVM and no root podvm holds `inherited-fds`, `small-tmp`, `userns-off`, a seccomp filter of chosen calls (`bind-denied`, `getsockname-denied`) as `podvm lab run PROFILE -- CMD`; `root-unreadable` and `nonroot-nocaps` need a user namespace or a guest. podvm adds `chroot-denied`, `userns-mount-denied`, `cgroup-v1`, `landlock-old`, `proc-masked`, `noexec-tmp` and `wsl2`. podrunner asked for `bind-denied` with `getsockname-denied`, `userns-off`, `small-tmp` with `noexec-tmp`, and `inherited-fds` first (#180). Next: the answer of podcrates (the crate), and `podvm lab run` in a release.

State (2026-11-15, podcrates): podcrates takes `pod-lab` (PodUtils/podcrates#180, PodUtils/podcrates@T-692, P1). Part 1 in the tag after `v0.53.0`: the profiles as data, `check` against `pod-hostprobe`, `run` of the parts that a host with no root holds, and a static `podlab`. Part 2: the step of Actions, the machine-tier form (podvm), the network profiles (podnet). The scripts of podrunner are the first draft of the profiles.

State (2026-11-15, the first scripts): `scripts/lab/` has `small-tmp.sh`, `userns-off.sh`, `nonroot.sh` and `inherited-fds.sh`; each checks its class first and exits 70 when the class does not hold. Measured in this sandbox: `small-tmp` mounts 1 GiB, `nonroot` runs with `CapEff` 0, `inherited-fds` opens 3 to 15; `userns-off` refuses (exit 70: the sysctl cannot be written here). Next: `bind-denied` (a seccomp filter; with `podvm lab run` when podvm has it), `proxy-only` (podnet), and the lanes of `user-gate.yml` (T-243).

State (2026-11-15, the lanes): `user-gate.yml` (T-243) has the lanes `plain` (`nonroot.sh`), `small-tmp` (`small-tmp.sh`, then `nonroot.sh`), `userns-off` (`userns-off.sh`, then `nonroot.sh`) and `inherited-fds` (`nonroot.sh`, then `inherited-fds.sh`; `scripts/user-gate.py` keeps the descriptors open in each command), each a job of its own on `ubuntu-24.04` x86_64 with the checks of T-243. The matrix of the job `lanes` has the place of `bind-denied` and `proxy-only` in a comment; a lane is one matrix row with its lab command. `small-tmp.sh` uses `[:lower:]` and `[:upper:]` for shellcheck, and `scripts/test-install.sh` runs shellcheck on `scripts/lab/`. Not run: the lanes on a hosted runner; aarch64 has no lane yet.

State (2026-11-15, podvm beta.24): podvm `0.2.0-beta.24` has `podvm lab run`, `lab check` and `lab profiles` (PodUtils/podvm#430) and is pinned. The lane `bind-denied` of `user-gate.yml` installs as a user with no root, then runs `podvm lab check bind-denied,getsockname-denied`, `doctor --run` (it must say `FAIL loopback` with a fix) and `podrunner run` (it must fail and name the bind and its fix; an xfail of T-220, which fails the lane when the run passes) in that lab (`scripts/user-gate.py --podvm-lab`). Measured here with podvm beta.24 and a build of this tree: each part of the lab holds, and doctor and the run give those lines. Next: the first run of the gate; `userns-off`, `small-tmp` and `inherited-fds` from `podvm lab run` in place of the scripts; `proxy-only` (podnet).

State (2026-11-17): the part of podvm is done: `podvm lab run` and `lab check` in the lane `bind-denied`, green in user-gate 44051472680; GitHub #180 and #182 closed (PodUtils/podvm#466). The rest is the crate `pod-lab` of podcrates, which T-264 takes with the next tag of podcrates.


# T-245: The command line as a contract

**Source:** the owner, 2026-11-15: "why did podrunner not survive a contact with real users"
**Category:** chore
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** partial

## Problem

The tests of `run` took each flag on its own path. A combination (`--platform` with another flag), a short option, `--help` of a command, and the text of an error were not tested; the first user found eleven defects there in one hour.

## Premise

GitHub #155 to #165.

## Approach

A table of each pair of the flags of `run`, with the answer that each pair must give (accept, or refuse with a text); a test that runs each pair. A test that each command has `--help`. A test that each error of the command line names its path or its flag, and a fix where one exists.

## Prove

`cargo test -p podrunner --lib the_flags_of_run_as_a_contract`.

Also (2026-11-17, PodUtils/podnet#649, COORD-102 of podnet: its testers found 40 defects in one hour where no check looked): each flag that `USAGE`, `run --help` and `docs/cli.md` name is a flag of the parser, and the reverse; each example of `docs/cli.md` and `docs/local-mode.md` parses; each `--json` output of a command has one schema (`schemas/`). The user QA of beta.7 found the same class (#203 to #212, F9, F10). The model: PodUtils/podssh `tests/man_flag_parity.rs` and `src/man/examples.rs` (named on PodUtils/podnet#649).

In the code (2026-11-17): `run_local/contract_tests.rs`: each flag of `USAGE` is a flag of the parser (or of `run_config::expand` and `watch::repeat`), each flag of the parser is in `USAGE` (`--quiet` is the long form of `-q`), and the synopsis of `run` in `docs/cli.md` names the same flags. Open: the pairs of the flags, `--help` of each command, the examples of the documents, one schema for each `--json` output.

# T-246: An adversarial QA before each release

**Source:** the owner, 2026-11-15: "why did podrunner not survive a contact with real users"
**Category:** release
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** partial

## Problem

Two agents with no context of the work found about 30 defects in a few hours, after the release. Beta testers are not always there.

## Premise

The QA of GitHub #153 and the QA report of 2026-11-15.

## Approach

Before each request for a release, the lead session starts two or more agents with a fresh context. Each gets only the notes of the release, the documents and the built binaries, on a constrained host (this sandbox: an egress proxy only), and the brief to break podrunner as a user (`docs/maintenance.md`, "A release"). Each finding is an entry; a P1 or P2 blocks the release. The brief is kept in `docs/qa-brief.md`.

## Prove

The record of each release names the QA run and its findings: `grep -n 'adversarial QA' docs/releases/v*.md`.

State (2026-11-16): the first QA, two agents on `12cb64e` on this sandbox (`--runner-dir` only; no image of podvm, no installer before the release), found 2 P1, 11 P2 and 7 groups of P3: T-265 to T-284. The release of `v0.1.0-beta.7` waits for them and for a second QA. 2026-11-16: the second QA (two new agents, new users, the release build of `f5d1de7` with the version `0.1.0-beta.7`) found 9 P2 and many P3: T-287 to T-298; each P2 is fixed. The third QA runs on `0305f3c`.

State (2026-11-16, round 3): two agents with a fresh context, as `qa1` and `qa2` with no root, on the release files of `12a1300` (user-gate 43816078517) with podvm `0.2.0-beta.24`, behind a proxy that intercepts TLS, no KVM. Agent A: T-300 to T-308 (P1: T-300, T-302) and T-260. Agent B: T-309 to T-316 (P1: T-309), and B-03 is T-302. The lead session: T-301 and T-317 (podvm `0.2.0-beta.25`). The release waits for each P1 and P2 of them.


# T-247: The notes of a release name the level of the proof of each claim

**Source:** the owner, 2026-11-15: "why did podrunner not survive a contact with real users"
**Category:** release
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The notes of `v0.1.0-beta.6` listed what CI proves next to what users need, and a reader took the first for the second.

## Premise

The owner, 2026-11-15.

## Approach

Each claim of the notes names its level: CI on GitHub-hosted runners, a lane of the user-path gate, or not run. A check of `release.yml` refuses notes with no section "What is not proven".

## Prove

`sh scripts/agent/check.sh`: a note of a release with no level of proof fails the check.

## Done

2026-11-16: `scripts/check-notes.sh` refuses notes with no section "What is proven in CI", "What is proven on a constrained host as a user" or "What is not proven", or with an empty "What is not proven". `release.yml` checks the notes of the version; the gate and `scripts/agent/check.sh` check the newest notes. Measured: the notes of beta.7 pass; the notes of beta.6 (no section of a user host) and a file with an empty "What is not proven" fail. Proven in CI: gate 43819358691 on `6527879` (the step "The levels of proof of the notes").

# T-248: The help and the manual of podrunner from the shared crate of PodUtils

**Source:** the ruling of the owner of 2026-11-15 (one crate for the help and the manual of each command line), PodUtils/podnet#584 (COORD-090)
**Category:** feature
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** blocked

## Problem

podrunner writes its usage, its help of each command (T-208) and its rules between options by hand, apart from its parser; the first user run found eleven defects there. The other projects of PodUtils share one crate for the help and the manual.

## Premise

podnet's `src/manual/` (a model of `clap`, roff, text, JSON, a pager, a table of variables and exit codes) is the source of the crate. podrunner has no `clap`.

## Approach

The ruling "Discipline" (`docs/decisions.md`, 2026-11-15). Take `pod-manual` when podcrates gives it (PodUtils/podcrates#181, PodUtils/podcrates@T-700), with a model that podrunner fills from one table of its options (the needs of COORD-062, GitHub #181): the parser, `--help`, `podrunner man` (`--no-pager`, `--json`), the man page, the keys of `run.toml` and the contract tests of T-245 come from that table. The environment variables and the exit codes of podrunner become one table of facts. A test fails when a generated file differs from the model, or when the code reads a variable or gives an exit code that has no fact.

## Prove

`cargo test -p podrunner --lib the_help_comes_from_the_table`: the help of each command and the man page come from the table, and a change of the table changes both; `podrunner man --no-pager` and `podrunner man --json` answer; the test of the facts fails on a variable that has no fact.

## Blocker

2026-11-15: podcrates: `pod-manual` in the first tag after part 1 of `pod-lab` (PodUtils/podcrates#181), with the needs of COORD-062 (GitHub #181).

# T-249: The installer on each host class, and the shared core of podcrates

**Source:** the ruling "Installer" of the owner (2026-11-15, PodUtils/podcrates#182); COORD-064 (GitHub #183)
**Category:** feature
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** open

## Problem

The installer of T-202 ran only in dash, on hosts that allow each operation. It does not check the free space of a target before a download or a load, it has no test of a run cut in the middle, of busybox `sh` or of aarch64, and its downloads of the image need the proxy of the environment in each case (T-233, T-234).

## Premise

The ruling "Installer" (`docs/decisions.md`): one POSIX installer for each project, hermetic, idempotent and fast, run in the labs; podcrates writes a shared core (PodUtils/podcrates@T-702).

## Approach

1. Check the free space of the prefix, the store and `TMPDIR` before each download and load, and name the knob that fixes it.
2. Read the proxy of the environment for each download, and write it into no file.
3. Test the installer in `dash`, `busybox sh` and `bash --posix`, with `shellcheck --shell=sh`, a second run that changes nothing, and a run cut in the middle, in each lab of T-244, with a limit of time.
4. Take the shared core of podcrates when it is out, and keep only the files and the setup steps of podrunner.

## Prove

`sh scripts/test-install.sh` runs each shell and the cut run; `user-gate.yml` (T-243) runs the installer in each lab with a limit of time.

# T-250: The state of each issue comes from the record by a tool

**Source:** the ruling "Issues" of the owner (2026-11-15, PodUtils/podcrates#183); COORD-065 (GitHub #184)
**Category:** chore
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** blocked

## Problem

The state column of `TODO/issues.md` and the bodies and comments of the issues are written by hand, and drift: the housekeeping of 2026-11-15 amended 12 issues, and a tester read a state that was old.

## Premise

podcrates writes a reusable workflow and the check in `pod-todo` (PodUtils/podcrates@T-703): an issue is open exactly when an entry of it is not done, and a block of its body holds its state, from the record.

## Approach

Take the workflow and the check of podcrates when they are out; give it the map of `TODO/issues.md` (COORD-065, GitHub #184); drop the state column that is written by hand.

## Prove

`cargo todo check` fails on an open issue whose entries are all done; the workflow writes the block of a test issue.

## Blocker

2026-11-15: podcrates: the workflow and the check of PodUtils/podcrates@T-703 (PodUtils/podcrates#183).

# T-260: An expression in the image of a service or a job container fails the job

**Source:** T-243 (the corpus of the user-path gate, 2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

GitHub evaluates `jobs.ID.services.NAME.image` and `jobs.ID.container.image` with the contexts `github`, `needs`, `strategy`, `matrix`, `vars` and `inputs`. Real workflows name the image of each cell so: rq/rq at 535172412469b78d4ac690a446284d8ec1e9f541 has in its workflow valkey.yml `image: valkey/valkey:${{ matrix.valkey-version }}`. The local mode does not evaluate the expression: the collect of the images refuses it, and the cell fails before it starts with `the image "valkey/valkey:${{ matrix.valkey-version }}" is not pinned by digest (NAME@sha256:HEX)`. The text names the wrong cause: the image has a tag after the evaluation, which the pull of T-130 pins.

## Premise

Measured on 2026-11-15 in this sandbox: `podrunner run .github/workflows/valkey.yml --checkout . --job valkey-test --matrix python-version=3.14 --matrix valkey-version=9.0 --matrix redis-py-version=8 --runner-dir DIR --dry-run` in a clone of that commit gives the line above. Code read: `crates/podrunner-local/src/message.rs` (`pinned`) and `crates/podrunner-local/src/images.rs` (an expression is refused). Not measured: the same on `--podvm`.

## Approach

Evaluate the image of each cell with the contexts that GitHub allows there, before the collect of the images, so the pull and the lock see `NAME:TAG`. A context that GitHub does not allow there fails the cell by name. The text of a refusal names the expression when one is there.

## Prove

A test of `crates/podrunner-local`: a service image `valkey/valkey:${{ matrix.v }}` gives `valkey/valkey:9.0` for the cell `v=9.0` in the collect of the images; `podrunner run ... --dry-run` of the corpus row `rq-valkey` (`scripts/corpus/user-gate.tsv`) plans the cell on `--podvm`.

State (2026-11-16): the third QA, agent A (finding A4): on `--podvm`, the image `ghcr.io/...:${{ matrix.postgres }}` of a service refuses the whole run before any job, exit 78, also with `--job lint`, and `--validate` says `valid` (brianc/node-postgres at `2a5850fb`). Each job with no such image must run, and the refusal must name its cell.

State (2026-11-16, partial): the run pulls only the images of the jobs that run (`--job` and their needs), and leaves out a job whose `container:` or service image is an expression, with a line; that job fails at its start, and each other job runs (`run_local/actions.rs`, `run_jobs`; test `a_job_with_an_image_of_an_expression_is_left_out`). Next: give such an image for each cell of the matrix (the values of `matrix`), pull each one, and give the runner the digest of its cell.

## Done

2026-11-16: an image of `container:` or of a service that takes values of the matrix of its job (`${{ matrix.KEY }}`) gives one image for each combination of the static values of the matrix (its vectors and `include`), each one pulled and pinned before the first job (`run_local/actions.rs`, `run_jobs`); the job message of each cell gives the runner the image of its own values, by digest (`message::image_with`, `pin_node`). An image with another expression fails its own job only, with a line. The run pulls only the images of the jobs that run. Unit tests: `a_job_with_an_image_of_an_expression_is_left_out`, `an_image_takes_the_values_of_the_matrix`.


# T-251: The gate and the host of an agent run shellcheck of other versions

**Source:** gate 43730995552 on `037cb35` (the request of `v0.1.0-beta.7`, GitHub #154, #179)
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

The gate ran the shellcheck of its runner image, which flagged SC2015 in `scripts/install.sh`; shellcheck 0.11.0 does not flag that line, and on the host of an agent `scripts/test-install.sh` skipped shellcheck with no word when it was not there. So the checks of the session passed, the gate failed, and the release of beta.7 did not start.

## Premise

The log of the job `lint` of gate 43730995552, read on 2026-11-15.

## Approach

One pinned version of ShellCheck, by its SHA-256, in CI and on the host of an agent (`scripts/shellcheck.sh`); `scripts/test-install.sh` never skips it. The three lines `A && B || C` of the installer and of its test become `if`.

## Prove

`sh scripts/test-install.sh`: case 13 runs the pinned shellcheck on each script; the gate runs the same script.

## Done

2026-11-15: `scripts/shellcheck.sh` (ShellCheck 0.11.0, SHA-256 pinned, into `.work/tools`); `scripts/test-install.sh` case 13; `gate.yml` takes no shellcheck of the image; `scripts/install.sh` and `scripts/test-install.sh` have no `A && B || C`.

# T-252: No workflow is linted before it runs

**Source:** the release run of `v0.1.0-beta.7` (2026-11-15): a lint of `runner-image.yml` with actionlint here gives findings that no check reads
**Category:** chore
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** open

## Problem

The gate lints the Rust code and the scripts, not the workflows. A fault of a workflow (a wrong expression, a wrong input, a shell fault of a `run` block) shows only when the workflow runs, on a release candidate. actionlint 1.7.7 with ShellCheck 0.11.0 gives findings of style and info in `runner-image.yml` today.

## Premise

Measured on 2026-11-15 in this sandbox: actionlint passes `user-gate.yml` and `release.yml`, and gives SC2129, SC2016 and SC2012 in `runner-image.yml`.

## Approach

A pinned actionlint (by its SHA-256, as `scripts/shellcheck.sh` pins ShellCheck) in the gate and in `scripts/agent/check.sh`, on each workflow; fix the findings, or name each one that stays with a reason.

## Prove

`sh scripts/agent/check.sh`: a step `actionlint` that fails on a planted fault of a workflow.

# T-253: The faults of the first run of the user-path gate

**Source:** the first run of the user-path gate, user-gate 43743490768 on `f0fd5f2` (2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

The first run of the gate passed the install and the quick start in each lane, and failed the corpus: the tar of the checkout had the link `HomebrewFormula` of ripgrep before its target, which podvm `0.2.0-beta.22` refuses (T-232); the installer made its directories writable by the group under the umask of the lab user, so `--microvm` refused its binary; the jobs of the corpus called the API of GitHub with no token and met the rate limit of a shared address; and a run refused before its first job showed an empty detail in the summary.

## Premise

The summary and the logs of the lanes of run 43743490768, read on 2026-11-15.

## Approach

Each symbolic link after the other files in the tarball of a tree (`tree::links_last`), on top of podvm `0.2.0-beta.24`; `umask 022` in the installer; the token of the gate run for the jobs of the corpus (`PODRUNNER_GATE_TOKEN`, contents: read); the last `podrunner:` lines of stderr as the detail of a refusal.

## Prove

`cargo test -p podrunner-local --lib a_link_comes_after_the_other_files`; the next run of `user-gate.yml`.

## Done

2026-11-15: `crates/podrunner-local/src/tree.rs` (`links_last`, also in `localrepo.rs`), `scripts/install.sh` (`umask 022`), `scripts/user-gate.py` (`token_flags`, the detail of a refusal), `scripts/lab/nonroot.sh` and `user-gate.yml` (the token). The proof of the gate is its next run.

# T-254: The made-up `github.token` fails each action that calls the API of a real forge

**Source:** the first run of the user-path gate, user-gate 43743490768 on `f0fd5f2` (2026-11-15): express lint, `setup-node` "Bad credentials"
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

The local mode gave each job `github.token = podrunner-local-made-up-token` (T-122). An action that calls the API of GitHub with it (`actions/setup-node` reads its manifest so) gets "Bad credentials" and fails the job, where GitHub gives a real token.

## Premise

The log of the job `lint` of expressjs/express in run 43743490768.

## Approach

The made-up token stays (T-122). When a failed job has "Bad credentials" in its log, the run says on stderr that the job gave the made-up token to a real API, and names `--github-token-env NAME` (`logs::token_hint`). The user-path gate gives the jobs of its corpus the token of its run (T-253).

## Decision

The call of 2026-11-15, after a first change: an empty `github.token` made `actions/github-script` refuse its required input before it ran ("Input required and not supplied: github-token": the parity suite in runner-image 43750369310); a required input is a more common case than an LTS alias of `setup-node`. So the token stays made-up, and the run names the fix.

## Prove

`cargo test -p podrunner --lib bad_credentials_name_the_token_flag`; the parity suite of `runner-image.yml` and the express row of the gate.

## Done

2026-11-15: `logs::token_hint` (`crates/podrunner/src/run_local/logs.rs`), the line on stderr after the logs of the failed jobs; `docs/cli.md`, `docs/local-mode.md`.

# T-255: On `--runner-dir`, the tests of express miss headers and hang

**Source:** the first run of the user-path gate, user-gate 43743490768 on `f0fd5f2` (2026-11-15)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The job `test` (Node.js 22) of expressjs/express at `43fca52d` passes on `--podvm` and on GitHub, and on `--runner-dir` its tests of `sendfile` fail ("expected Content-Disposition header field", "expected Accept-Ranges", "NotFoundError") and the run hangs until the timeout of the row.

## Premise

The log `express-node22-runner-dir/test.4/job.log` of the lane `plain` of run 43743490768.. Not reproduced here: in the sandbox of a session, `actions/setup-node` of the job fails at the TLS of the egress proxy (a certificate that the runner of the job does not take), before the tests.

## Approach

Run the job on a runner directory here; compare the environment of the job on `--runner-dir` and in a podvm unit (the user, `HOME`, `TMPDIR`, the umask, the proxy variables, the path of the work directory); fix the cause; the corpus row (`scripts/corpus/user-gate.tsv`, an xfail now) back to `ok`.

## Prove

The row `express-node22` on `runner-dir` of the gate passes.

State (2026-11-16): with the job environment of T-265 and T-266 (the `PATH`, the proxy and the CA of the user, a scratch `HOME`), the job `test` (Node.js 22) of express at `43fca52d` passes on `--runner-dir` on this sandbox as the user `tester` with no root: 1290 passing, exit 0. The row `express-node22` on `runner-dir` of the corpus is `ok` again; the proof is the next run of the user-path gate. Also fixed on the way: a lock file of the runner directory that another user made is opened read-only (`exec/host.rs`, `lock`).

State (2026-11-16, later): user-gate 43795123788 on `a4b41b9`: the row still times out (exit 124) in each lane in CI, so the row is an xfail again. The difference between CI and the sandbox is not known: read `logs/express-node22-runner-dir` of the artifact of a lane (the tests of sendfile and the hang).

State (2026-11-16, the cause): the log of the lane `proxy-only` of run 43795123788: 1230 passing, 60 failing (each a test of `res.download` and `res.sendFile`), then the run waits until the cancel at 2700 s. In the gate, the runner directory of the installer is `~/.local/share/podrunner/runner/2.338.2`, so the workspace is under `.local`. The `send` module of Node refuses an absolute path with a name that starts with `.` (404, `dotfiles: ignore`); measured on the sandbox with `send@1`: 404 for `a/.local/b/f.txt`, 200 for `c/b/f.txt`. The runner directory of the sandbox had no such name. The fix: when the path of the runner directory has such a name, the work folder of the runner is `BASE/podrunner-work/NAME-HASH` (`work::host_work`; `start_runner_in` writes it into `.runner`). Proven on the sandbox with the runner 2.338.2 in `h/.local/share/podrunner/runner/2.338.2`: `GITHUB_WORKSPACE` is `h/podrunner-work/2.338.2-9f2cc63a/workflow/workflow`. The corpus row is `ok` again; the proof is the next run of the user-path gate.

## Done

2026-11-16, `12a1300`: the work directory of `--runner-dir` has no name that starts with `.` (`work::host_work`). Proven in CI: user-gate 43816078517 on `12a1300`: `corpus:express-node22:runner-dir` passes in each of the five lanes (plain, small-tmp, userns-off, inherited-fds, proxy-only).


# T-256: The checks of a session do not run the suites that need the official runner

**Source:** level2 43750372759 on `92a7394` (2026-11-15): T-254 changed the default token, and `a_step_reads_a_secret_and_the_token_and_the_log_masks_the_secret` (`crates/podrunner-local/tests/runner.rs`) still expected the made-up token
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

`scripts/agent/check.sh` ran `cargo test --workspace`, which skips each test marked `#[ignore]`. The suites of `level2.yml` that need the official runner are such tests, so a change passed each check of the session and failed level2, the third time this day that a local check and CI disagreed (T-251).

## Premise

The log of the job `runner` of level2 43750372759, read on 2026-11-15.

## Approach

`check.sh` runs the suites of `level2.yml` that need the runner when `PODRUNNER_RUNNER_DIR` names an unpacked release of actions/runner, and says by name that they did not run when it does not. The test expects an empty token (T-254).

## Prove

`PODRUNNER_RUNNER_DIR=DIR sh scripts/agent/check.sh`: a line `runner 0`; with no variable, the line `runner skipped`.

## Done

2026-11-15: `scripts/agent/check.sh` (the step `runner`); `crates/podrunner-local/tests/runner.rs` (`test -z "$T"`). The three suites pass here with the runner v2.338.2.

# T-261: In a podvm unit, the persist of a temporary file gets EACCES

**Source:** the second run of the user-path gate, user-gate 43758947099 on `b5c9214` (2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** partial

## Problem

Two jobs that pass on github.com and on `--runner-dir` fail in a podvm unit (podvm `0.2.0-beta.24`, lane `plain`): the tests of pallets/click with tox and uv (`Could not persist temporary file /root/.cache/uv/.tmp...: Permission denied (os error 13)`), and the lint of rq/rq with ruff (`Failed to rename temporary cache file ... failed to persist temporary file: Permission denied`). Both use the Rust crate `tempfile`, whose persist is `rename(2)`, or `renameat2(2)` with `RENAME_NOREPLACE`, or `link(2)`.

## Premise

The step logs `click-py313-podvm/tests.5` and `rq-lint-podvm/lint.0` of the artifact of the lane `plain` of run 43758947099, read on 2026-11-16. Not reproduced here.

## Approach

podvm found the cause (PodUtils/podvm#439): a volume of the unit puts the run on the loader family of the userland rung, whose `LD_PRELOAD` maps the paths of libc calls only; the crate `rustix` of `tempfile` issues `renameat2` as a raw `syscall` instruction, so the kernel sees the path of the guest. podvm binds a volume on the namespace rung in a later release. No variable of the environment turns the raw calls off. Then the corpus rows `click-py313` and `rq-lint` on `podvm` (`scripts/corpus/user-gate.tsv`) go back to `ok`.

## Prove

The rows `click-py313` and `rq-lint` on `podvm` pass in the user-path gate.

In the code (2026-11-18): podvm `0.2.0-beta.32` holds the raw system calls of a run on the userland rung with `--podvm-tier=syscall` (PodUtils/podvm#470, COORD-185). The podvm kind adds the flag to `run -d` of a unit where the probe names the userland rung (`labels::tier_args`, since removed). The tier runs on podvm's native-static rung, which is x86_64 only: `runner-musl.yml` on aarch64 got exit 125 for the flag on `8ec7d87`, so an aarch64 host gets no flag, and uv and ruff keep the fault there. Open: the next run of `user-gate.yml`; when the rows `click-py313` and `rq-lint` on `podvm` pass, set them `ok` in `scripts/corpus/user-gate.tsv` and close GitHub #189.

2026-11-18, later: the flag broke the container hooks on the userland rung of GitHub's runner (`runner-image.yml` run 44292729694: the relay in the unit cannot connect to the control socket under its `-v`, `curl: (7)`); the flag is removed. Asked of podvm by COORD-091 (GitHub #301).

State (2026-11-18): podvm `0.2.0-beta.33` maps the path of a unix socket in the syscall tier (COORD-190, PodUtils/podvm#586). It is pinned, and the podvm kind gives `--podvm-tier=syscall` again on the userland rung of x86_64 with that podvm or later (`labels::tier_args`, test `the_syscall_tier_holds_the_userland_rung_of_x86_64_only`). Next: the container steps of `runner-image.yml` (the relay of the hooks under the tier), then the rows `click-py313` and `rq-lint` in `user-gate.yml`.

2026-11-18, later: with podvm `0.2.0-beta.33` the relay of the hooks connects under the tier (#301 closed), but `runner-image.yml` run 44331618064 failed `podrunner-podvm --test links`: under the tier an absolute symbolic link of the unit does not resolve in its rootfs (`cat: link: No such file or directory`). The flag is removed again; asked of podvm by COORD-093 (GitHub #303).

State (2026-11-18, later): podvm `0.2.0-beta.34` fixes COORD-093 (COORD-192, PodUtils/podvm#588). It is pinned, and the flag is back on the userland rung of x86_64 with that podvm or later (`labels::tier_args`, `SYSCALL_TIER`). Next: `links.rs` and the container steps of `runner-image.yml` with it; then the rows `click-py313` and `rq-lint` in `user-gate.yml`, and close #303 and #189.

State (2026-11-18, later): `runner-image.yml` run 44365652790 (`79fe9e1`): with the tier, `links.rs` and the plain conformance pass (COORD-093 holds), but the conformance under the confine layer fails: its self-test is not proven. The cause (podvm's answer COORD-194, PodUtils/podvm#590): `run -d` with `--podvm-confine` on the tier could not start its payload (Landlock denied the exec of podvm itself), so the held run failed; the tier hides no host process. Fixed on podvm `main` (PodUtils/podvm@T-1982). Call until the next beta: a kind with the confine layer takes no syscall tier (`PodvmUnit::no_tier`, set by `with_confine`), in its units and its self-test; asked of podvm by COORD-094 (GitHub #304). Next: the next run of `runner-image.yml`.

State (2026-11-18, later): run 44376274000 (`2041d27`) is green with the tier for an unconfined kind: `links.rs`, the conformance and the container steps; #303 is closed. Open: the tier for a confined kind (COORD-094, #304, the next beta of podvm), and the rows `click-py313` and `rq-lint` in `user-gate.yml`.

State (2026-11-18, evening): `user-gate.yml` run 44393864747, lane `proxy-only`: with the tier, `click-py313` and `rq-lint` pass on the podvm executor (the xfail rows fail the gate as "passes now"). The tier breaks a dynamic program in a volume: Node of `setup-node` in `_work/_tool` gives `native-static rung: ... ld-linux-x86-64.so.2: errno 11` each time (the express rows), and `/usr/bin/bash` gave it once in `runner-image.yml` run 44391387598. COORD-098 (GitHub #308, to podvm). The lanes `inherited-fds`, `proxy-only-dns` and `small-tmp` give the same five rows. So podrunner gives no tier now (`SYSCALL_TIER = None`, the call in `docs/decisions.md`); the rows stay as they are. podvm's answer (COORD-197, PodUtils/podvm#593): the EAGAIN of `openat2` is retried on its `main`, for the beta after `0.2.0-beta.35`; the hang does not reproduce. Next: with that beta, give the tier again (`SYSCALL_TIER`), run `user-gate.yml`, and set `ok` for click and rq when the express rows and `rg-docs` pass; if `rg-docs` hangs again, take `uname -r`, `ps -eo pid,ppid,stat,wchan:32,args` of the unit and the `wchan` and fds of the waiting process for podvm.


# T-262: In a podvm unit, Node sometimes reads an invalid package.json

**Source:** the second run of the user-path gate, user-gate 43758947099 on `b5c9214` (2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The job `lint` of expressjs/express fails in a podvm unit of the lane `plain` with `Error: Invalid package config .../node_modules/eslint/package.json`, and passes in the lane `inherited-fds` of the same run, on github.com and on `--runner-dir`. Node gives this error when the read of the file gives bytes that are not valid JSON.

## Premise

The step log `express-lint-podvm/lint.0` of the artifact of the lane `plain` of run 43758947099, and the summary of the run, read on 2026-11-16.

## Approach

podvm (PodUtils/podvm#439): the libuv of Node sends file calls through io_uring, which passes no libc call, so the interposer of the loader family does not map their paths (the cause of T-261). podrunner sets `UV_USE_IO_URING=0` in each unit that is not a guest (`crates/podrunner-local/src/exec.rs`), so each file call of libuv goes through libc. The corpus row `express-lint` on `podvm` is `ok`.

State: 2026-11-16: `UV_USE_IO_URING=0` in the environment of a unit that is not a guest; the corpus row `express-lint` on `podvm` back to `ok`. Not reproduced: the proof is two runs of the user-path gate.

## Prove

The row `express-lint` on `podvm` passes in each lane of two runs of the user-path gate.


The blocker of the time: 2026-11-16: user-gate 43787604846 on `12cb64e` (with `UV_USE_IO_URING=0`): the job fails in three lanes of four, at the first read of a `package.json` (of npm in the tool cache) by the module loader of Node 26; `UV_USE_IO_URING=0` is not the fix. The row is an xfail again; waits for podvm (COORD-068, GitHub #193). podvm (PodUtils/podvm#442): the cause is the loader family of #439; the fix (a volume as a bind mount on the namespace rung for a foreground `run`) is on podvm `main` (`1298b66a`) and comes in its next release; no variable closes it on beta.24. user-gate 43795123788 on `a4b41b9`: the row passed in the lane `inherited-fds` and failed in the other four, so an xfail of it can fail the gate as "passes now"; until podvm releases the fix, a run of the gate can need a second attempt.

State (2026-11-16): user-gate 43816078517 on `12a1300`: `corpus:express-lint:podvm` passed in the lanes inherited-fds and proxy-only and failed in plain, small-tmp and userns-off. The row is `flaky` now (`scripts/user-gate.py`: a flaky row takes either result, and its note names the entry); it goes to `ok` with the podvm release that carries the fix.

State (2026-11-16): T-309 found a likely cause: a write or a seek of podvm's memo on descriptor 17 after a program closed it (COORD-071, GitHub #197).

State (2026-11-16): podvm `0.2.0-beta.26` fixes the memo on descriptor 17 (T-309), the likely cause. The row `express-lint` on podvm stays `flaky` until a run of the user-path gate on beta.26 shows it.

## Done

2026-11-16: the cause was the memo of podvm on descriptor 17 (T-309), fixed in podvm `0.2.0-beta.26`. Proven in CI: user-gate 43870085390 on `43dce45` (podvm `0.2.0-beta.27`): `corpus:express-lint:podvm` passes in each of the five lanes; the row is `ok` again.


# T-263: The installer keeps a directory that the group can write

**Source:** the second run of the user-path gate, user-gate 43758947099 on `b5c9214` (2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

`--microvm` refused the podvm of the install (`[unit.microvm] binary: .../.local/share/podrunner/bin is writable by the group or the others`) after the `umask 022` of T-253: the umask applies to a new directory only, and the directory was there before the installer.

## Premise

The summary of run 43758947099 (the rows `hello:microvm` and `corpus:*:microvm`); the code of `check_driver` (`crates/podrunner-slots/src/config.rs`).

## Approach

The installer removes the write of the group and the others from `bin` and `share/podrunner/bin` after `mkdir -p`.

## Prove

`sh scripts/test-install.sh` (the step "the installer removes the write of the group from its directories"); the rows of `microvm` in the next run of the user-path gate.

## Done

2026-11-16: `scripts/install.sh` (`chmod go-w`), `scripts/test-install.sh` (step 4b). The proof of the gate is its next run. Proven in CI: user-gate 43787604846 on `12cb64e`: `--microvm` takes the podvm of the install (the refusal of the mode is gone); the run then meets T-291.

# T-264: Take podcrates `v0.55.0`: the sync of the issues and pod-lab

**Source:** PodUtils/podcrates#188 (COORD-113, `v0.54.0`), PodUtils/podcrates#194 (COORD-115, `v0.55.0`) PodUtils/podcrates#200 (COORD-116, `v0.58.0`: the installer core, `podlab` as a release asset), PodUtils/podcrates#204 (COORD-118, `v0.59.0`) and PodUtils/podcrates#206 (COORD-119, `v0.60.0`: the network labs of pod-lab, 20 host-class rows of pod-hostprobe) and PodUtils/podcrates#214 (COORD-124, `v0.64.0`: the roots of the host in the certificate file of a lab with `--way-out`, the SIGPIPE of the lab, the overlay root with `userxattr`), PodUtils/podcrates#219 (COORD-129, `v0.68.0`) and PodUtils/podcrates#220 (COORD-130, `v0.69.0`: two more hosts of GitHub Actions in `services/github-actions.toml`) (2026-11-16)
**Category:** chore
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** partial

## Problem

podcrates released `pod-lab` (34 lab profiles, `podlab check` and `podlab run`), `pod-manual` part 1 and a workflow that syncs each issue with the record (`issue-sync.yml`, the ruling "Issues" of the owner). podrunner pins podcrates `v0.52.0`, runs its lanes with `podvm lab run` and its own scripts, and has no `Kind` column and no `issue_rules` in its record.

## Premise

The bodies of PodUtils/podcrates#188 and #194, read on 2026-11-16. Not measured: the drift of the map of podrunner.

## Approach

Bump podcrates to the latest tag (`v0.59.0` or later: also the volumes of pod-enter and the chroot row of T-283); weigh the installer core of `v0.58.0` against `scripts/install.sh`. Add the column `Kind` to `TODO/issues.md` and repair the map; set `issue_rules = "on"` in `TODO/todo.toml`; add a workflow `issue-sync.yml` that calls the workflow of the tag, and read its dry run first. Move the lanes of `user-gate.yml` to `podlab run` where a profile holds with no root (T-244), and say on PodUtils/podcrates#180 which profiles podrunner runs.

## Prove

`cargo todo check` with `issue_rules = "on"`; a dry run of `issue-sync.yml` with no change that the record does not give; the lanes of the user-path gate green under `podlab run`.

State (2026-11-17): podcrates `v0.69.0` is pinned (each `Cargo.toml`); `pod-hostprobe` has the field `network` (the doctor sends no packet: `None`) and the kinds `Here` and `Network` (`Probe::measure`). fmt, clippy, the tests and `cargo todo check` pass here. `issue_rules = "on"` (`TODO/todo.toml`): it found 19 issues open whose entries were done; each is closed on GitHub, and #206 maps T-343 only. `issue-sync.yml` calls the workflow of the tag, as a dry run for a push and the schedule. The dry runs 44096256027 and 44099014996 were read: 183 status blocks, no change of state (the first also reopened #170 and #191, which the record had open then). The owner ruled Q9 (2026-11-17): the sync writes. Open: the lanes under `podlab run`; the mark of an unknown need is the line of the job, not of `needs:` (T-331 F9).

# T-265: A job on `--runner-dir` does not get the PATH, the proxy and the CA of the user

**Source:** the adversarial QA of `12cb64e`, agents A and B (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A job on `--runner-dir` gets `PATH=/usr/bin:/bin` and no `HTTPS_PROXY`, `NO_PROXY`, `SSL_CERT_FILE`, `NODE_EXTRA_CA_CERTS` or `GIT_SSL_CAINFO`. On a host with a proxy that intercepts TLS, `actions/setup-node` and `actions/setup-go` fail with "self-signed certificate in certificate chain"; tools in `/usr/local/bin` or `/opt` (`gh`, `node`) are not found. `doctor --run` gives each check `ok`, and no document tells how a local run gets a proxy or a CA.

## Premise

Findings A1, A4 and F21 of the reports of the QA (fatih/color, chalk/chalk, dtolnay/itoa, pallets/click on this sandbox; `--env NAME=VALUE` for each variable is the work-around that passed). The reports are in the scratch directory of the session, not in the repository.

## Approach

A job on `--runner-dir` takes the `PATH` of the user, and the proxy and CA variables of the user (the list of `docs/local-mode.md`). `doctor --run` names the proxy and the CA that a job gets. A section of `docs/local-mode.md` for a host with a proxy.

## Prove

`cargo test -p podrunner-local`: A test of `crates/podrunner-local`: the environment of a job on `--runner-dir` has the `PATH` and the proxy variables of the caller; the run of chalk/chalk on this sandbox with no `--env`.

## Done

2026-11-16: `crates/podrunner-local/src/exec/host.rs` (`PASSED`, `host_env`): a job on `--runner-dir` takes `PATH`, the proxy variables and the CA files of the caller; `NO_PROXY` holds the loopback, with no entry `::`, at which the official runner aborts (measured here: `Runner.Listener run` with `NO_PROXY=::` throws `ArgumentNullException` in `RunnerWebProxy`); `doctor --run` names the variables (`proxy`); `docs/local-mode.md` "A host with a proxy", `docs/cli.md`. Proven on this sandbox (a proxy that intercepts TLS, as root): `actions/setup-node@v4` and `curl https://github.com/` in a job pass with no `--env`. Test `a_job_on_a_runner_directory_takes_the_path_the_ca_and_the_proxy`.

# T-266: A job on `--runner-dir` writes into the home of the user

**Source:** the adversarial QA of `12cb64e`, agent A (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A job on `--runner-dir` gets the `HOME` of the user. rustup of a job added a line to `~/.profile` and `~/.bashrc`, and about 2 GiB of caches (`~/.rustup`, `~/.cargo`, `~/.npm`, `~/.cache/uv`) stay. `docs/local-mode.md` says "a fresh unit for each job", which a reader applies to the quick start too.

## Premise

Finding A10 of the report of the QA (dtolnay/itoa on `--runner-dir`). The reports are in the scratch directory of the session, not in the repository.

## Approach

Each job on `--runner-dir` gets a scratch `HOME` under its work directory, removed after the job (with `--keep-failed`, kept). The documents say what `--runner-dir` isolates and what it does not.

## Prove

`cargo test -p podrunner-local`: A test of `crates/podrunner-local`: a step that writes `$HOME/x` leaves no file in the home of the caller.

## Done

2026-11-16: `crates/podrunner-local/src/exec/host.rs`: `HOME` of a job on `--runner-dir` is `_work/_home`, which the next job removes with `_work`; `docs/local-mode.md`, `docs/cli.md`. Proven on this sandbox: a step that writes `$HOME/marker` leaves no file in the home of the caller.

# T-267: `--checkout .` gives no `.git`, so a step that runs `git` fails

**Source:** the adversarial QA of `12cb64e`, agent A (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`actions/checkout` on GitHub gives a `.git`; `--checkout .` of the quick start does not, so `git` in a step fails with "not a git repository". `--with-git` fixes it, but no document of the quick start names it.

## Premise

Finding A2 of the report of the QA (fatih/color). The reports are in the scratch directory of the session, not in the repository.

## Approach

`--with-git` is the default for a checkout of a git tree, with a flag to turn it off; or the failure of `git` names `--with-git`. The quick start names the flag when it is not the default.

## Prove

`cargo test -p podrunner-local`: A test: `--checkout` of a git tree gives a step `git rev-parse HEAD` equal to the commit of the run.

## Done

2026-11-16: `crates/podrunner/src/run_local.rs`: a checkout of a git tree has a `.git` by default; `--no-git` (also a key of a project file) leaves it out; `--with-git` and `--no-git` exclude each other (`run_local/args.rs`); `docs/cli.md`, `docs/local-mode.md`. Proven on this sandbox: with `--checkout .` only, a step `test "$(git rev-parse HEAD)" = "$GITHUB_SHA" && git status --porcelain` passes on `--runner-dir`.

# T-268: The lock of the actions goes into the next checkout as a new file

**Source:** the adversarial QA of `12cb64e`, agent A (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A run writes `podrunner-actions.lock` beside the workflow into the tree of the user, and the next `--checkout .` takes it as a new file, so a job that checks for a clean tree (`go mod tidy`, `git status`) fails.

## Premise

Finding A3 of the report of the QA (fatih/color). The reports are in the scratch directory of the session, not in the repository.

## Approach

Leave the lock file of podrunner out of the tarball of a checkout (`crates/podrunner-local/src/tree.rs`), as `.git` is.

## Prove

`cargo test -p podrunner-local`: A test of `crates/podrunner-local`: the tarball of a tree with `podrunner-actions.lock` has no such file.

## Done

2026-11-16: `crates/podrunner-local/src/tree.rs` (`left_out`): a lock file that is not committed stays out of the tarball; test `the_secrets_and_the_lock_stay_out_of_the_tree`.

# T-269: The made-up token fails `actions/setup-python` with a 404, and the run names no fix

**Source:** the adversarial QA of `12cb64e`, agent A (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`raw.githubusercontent.com` gives 404 to a request with the made-up `github.token`, so `actions/setup-python@v7.1.0` fails at the manifest of the versions. The hint of T-254 holds for "Bad credentials" only.

## Premise

Finding A5 of the report of the QA (rq/rq, job `mypy`; curl gives 200 with no `Authorization` and 404 with a made-up token). The reports are in the scratch directory of the session, not in the repository.

## Approach

The hint of `token_hint` (`crates/podrunner/src/run_local/logs.rs`) also for a failed fetch of `raw.githubusercontent.com` and an HTTP 401 or 404 of `api.github.com`; `docs/local-mode.md` says that an empty token variable gives no `Authorization`.

## Prove

`cargo test -p podrunner`: A test of `token_hint` with the log line of setup-python.

## Done

2026-11-16: `crates/podrunner/src/run_local/logs.rs` (`token_hint`): also for "Unable to get manifest from https://raw.githubusercontent.com"; the hint names an empty variable for public data; `docs/cli.md`; test `bad_credentials_name_the_token_flag`.

# T-270: A Dockerfile action on `--runner-dir` starts the job and calls the docker of the host

**Source:** the adversarial QA of `12cb64e`, agent A (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A job with an action of `runs.using: docker` starts on `--runner-dir`; the runner calls `docker build` of the host three times. `docs/cli.md` says that a job with containers fails before it starts (T-218), and `docs/local-mode.md` says that no Docker socket of the host reaches a job.

## Premise

Finding A6 of the report of the QA (rq/rq, supercharge/redis-github-action, docker/build-push-action). The reports are in the scratch directory of the session, not in the repository.

## Approach

The resolve of the actions marks an action of `runs.using: docker` (also a `docker://` step); on `--runner-dir` such a job fails before it starts, by name, also in `--dry-run`.

## Prove

`cargo test -p podrunner-local`: A test of `crates/podrunner-local`: a job with a `docker://` step and one with a Dockerfile action fail on `--runner-dir` in a dry run.

## Done

2026-11-16: `crates/podrunner-local/src/exec/docker.rs` (`docker_actions`, `refusal`): the `action.yml` of each action of the store at its pin, and of each composite action in it; `exec/host.rs` and the dry run (`run_local/inspect.rs`) refuse such a job on `--runner-dir`; `docs/cli.md`; test `a_docker_action_and_one_in_a_composite_are_found`. Proven on this sandbox: `--dry-run` of a job with `supercharge/redis-github-action@1.8.1` on `--runner-dir` gives the refusal. The same action on `--podvm` is T-285.

# T-271: `--job` does not select a need that calls a reusable workflow

**Source:** the adversarial QA of `12cb64e`, agent A (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`--job test` skips its need `pre_ci` (`uses:` of a reusable workflow) as "not selected", and `--job pre_ci` does not select it either, so no job that needs a called workflow can run alone.

## Premise

Finding A8 of the report of the QA (dtolnay/itoa at its pinned commit). The reports are in the scratch directory of the session, not in the repository.

## Approach

The selection of `--job` takes the jobs of a call as members of the calling job: `--job ID` of a call selects each job of the call, and a need of a selected job selects the call.

## Prove

`cargo test -p podrunner-local`: A test of the selection with a workflow whose job needs a call.

## Done

2026-11-16: `crates/podrunner-local/src/lib.rs` (`not_selected`): a cell of a called workflow (`CALLER/CALLEE`, `CALLER[N]/CALLEE`) is selected when its caller is; test `a_job_filter_selects_the_jobs_of_a_call` (`--job` of a job that needs the call, and of the call).

# T-272: `--dry-run` on `--podvm` plans a `services:` job and does not check the image

**Source:** the adversarial QA of `12cb64e`, agent A (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

With `--podvm` and `--dry-run`, a job with `services:` is `planned`, and an `--image` that is not in the store is not named; the real run then asks Docker Hub for `library/runner`.

## Premise

Finding A9 of the report of the QA. The reports are in the scratch directory of the session, not in the repository.

## Approach

The dry run on `--podvm` applies the refusal of `services:` of the real run and checks that `--image` is in `--images`; a missing image is a refusal that names `podvm load -i` with the store, with no pull from a registry.

## Prove

`cargo test -p podrunner-local`: A test of the dry run: a `services:` job fails, and an image that is not in the store is named.

## Done

2026-11-16: `crates/podrunner/src/run_local/inspect.rs` (`DryRun::podvm`) refuses a job with services; `slots.rs` (the dry run) and `routes.rs` (`in_podvm`, before `prepare`) check each runner image in the store with `podvm inspect` and name the fix with the store; `docs/cli.md`; test `a_dry_run_on_podvm_refuses_services`. Proven on this sandbox with podvm `0.2.0-beta.24`: a run and a dry run with an empty store fail with the fix, and ask no registry.

# T-273: A SIGHUP leaves the runner running and breaks the runner directory

**Source:** the adversarial QA of `12cb64e`, agent B (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A closed terminal (SIGHUP) leaves the runner and the processes of the job, and files in `_diag/pages`; each later job on that runner directory fails with "The file ... already exists". `doctor --run` gives `ok`. SIGINT and SIGTERM clean up.

## Premise

Finding F1 of the report of the QA. The reports are in the scratch directory of the session, not in the repository.

## Approach

SIGHUP does what SIGTERM does. A run on a runner directory clears `_diag/pages` before the start, and `doctor --run` names a runner process that holds the directory.

## Prove

`cargo test -p podrunner-local`: A test: a SIGHUP to a run on a runner directory ends each process of the job; the next run passes.

## Done

2026-11-16: `crates/podrunner-local/src/cancel.rs`: SIGHUP cancels as SIGTERM does, and sends the output to /dev/null; `exec/host.rs` clears `_diag/pages` before each job; `docs/cli.md`. Proven on this sandbox: a SIGHUP to a run during `sleep 60` on a runner directory ends podrunner and each runner process ("Canceled"), and the next run passes.

# T-274: Two runs on one runner directory at the same time break each other

**Source:** the adversarial QA of `12cb64e`, agent B (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A second run on a runner directory that a run uses fails with an internal error of the runner, and removes the `_work` of the first run during its job.

## Premise

Finding F2 of the report of the QA. The reports are in the scratch directory of the session, not in the repository.

## Approach

A run takes an exclusive `flock` on a file of the runner directory; a second run fails at once and names the run that holds it.

## Prove

`cargo test -p podrunner-local`: A test: a second run on the same runner directory fails with that text, and the first passes.

## Done

2026-11-16: `crates/podrunner-local/src/exec/host.rs` (`lock`): an exclusive `flock` on `.podrunner-run.lock` of the runner directory for the run; `docs/cli.md`; test `a_second_run_on_a_runner_directory_fails_at_once`. Proven on this sandbox: a second run during a job fails at once with the text, and the first ends as before.

# T-275: A project file reads a host file through a symbolic link

**Source:** the adversarial QA of `12cb64e`, agent B (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

The check that a path of `.podrunner/run.toml` stays in the project reads the text of the path only. A symbolic link in the project passes it, so a project puts each host file that the user can read into the environment of a job (`env-file`, `var-file`, `input-file`).

## Premise

Finding F3 of the report of the QA (`env-file = "link"`, the link to a file outside the project). The reports are in the scratch directory of the session, not in the repository.

## Approach

Resolve each path of a project file (canonicalize) and refuse a path whose real path is not under the real path of the project.

## Prove

`cargo test -p podrunner`: A test of the configuration: a link out of the project is refused, and a link in the project passes.

## Done

2026-11-16: `crates/podrunner/src/run_config.rs` (`path_value`) and `run_local/secrets.rs` (`.secrets`) resolve the real path and refuse one out of the project; test `a_link_out_of_the_project_is_refused`.

# T-276: `.secrets` of a checkout goes into the workspace of each job

**Source:** the adversarial QA of `12cb64e`, agent B (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

The documents tell the user to put the secrets in `.secrets` of the tree. `--checkout .` copies a file that git does not ignore, so `.secrets` is in the workspace: each step, action, artifact, cache and `--export` reads each secret as plain text.

## Premise

Finding F4 of the report of the QA. The reports are in the scratch directory of the session, not in the repository.

## Approach

Leave the files that podrunner reads as configuration (`.secrets`, `.vars`, `.env`, `.input`, the files of `.podrunner/run.toml`) out of the tarball of a checkout; the documents say so.

## Prove

`cargo test -p podrunner-local`: A test of `crates/podrunner-local`: the tarball of a tree with `.secrets` has no such file.

## Done

2026-11-16: `crates/podrunner-local/src/tree.rs` (`left_out`, `Options::exclude`) and `run_local/secrets.rs` (`run_files`): `.secrets` and the files of `--secret-file`, `--env-file` and `--var-file` in the tree stay out of the tarball, also out of a git tree; `docs/cli.md`, `docs/local-mode.md`; test `the_secrets_and_the_lock_stay_out_of_the_tree`.

# T-277: Some refusals name the wrong cause or a fix that does not work

**Source:** the adversarial QA of `12cb64e`, agents A and B (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** partial

## Problem

From the QA: a `services:` job on `--runner-dir` first says "not pinned by digest" with the expression of the matrix (A7); a detached clone defaults to `refs/heads/main` and the refusal names only `--force-event` (A11); `run.toml` refuses `jobs` and `web` but says each flag is a key (F10); a directory as `--secret-file` gets `chmod 600`, a used `--web` port gets the loopback fix, the image fix loads into the default store, `doctor` names the wrong parent, `--offline` and an HTTP 503 of `git ls-remote` give no fix (F11); `--repository bad`, `--sha zz`, `--remote nosuch`, `--approve nosuch`, an unused `--platform` label and a `--matrix` value that matches no cell pass with no word (F17).

## Premise

Findings A7, A11, F10, F11 and F17 of the reports of the QA. The reports are in the scratch directory of the session, not in the repository.

## Approach

Fix each text and each check; a test for each.

## Prove

`cargo test -p podrunner`: A test for each case of the list.

State (2026-11-16): done: a directory as `--secret-file` is "not a file"; a `--web` port in use names another port or 0; the fix of a missing image names the store (`PODVM_STORE=STORE podvm load -i`); `doctor` names the first parent from the root that cannot be read; a missing `TMPDIR` names its fix; a miss of `--offline` names a run with no `--offline` or the `--actions DIR` of a run that fetched it; a YAML error has no "the actions:"; `--checkout DIR` names its flag; `--repository` must be OWNER/REPO and `--sha` 40 or 64 hex digits; a `--remote` that the tree does not have fails; the refusal of the event names `--ref`; `run.toml` of the user takes `jobs`, `web` and `toolcache`, and the refusal names the flags that are not keys. Tests `the_values_of_the_github_context_are_checked`, `jobs_and_web_are_keys_of_the_user_file`. Open: a `--matrix` value or a `--platform` label that no cell or job uses, an `--approve` of no environment (the cells come during the run); a retry of `git ls-remote` after an HTTP 503; `run` names the parent that cannot be read, as `doctor` does; the line of the hold before the refusal of `--runner-dir`; on `--runner-dir`, the refusal of containers before the check of a digest (A7).

# T-278: The output, the exit codes and the help disagree with the documents

**Source:** the adversarial QA of `12cb64e`, agents A and B (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

From the QA: `-q` prints the console of the runner, and `--log-dir` is written at the end only (A12, F14); `podrunner version` prints `0.1.0` for a beta (A13, F19); an unreadable workflow exits 69 and a missing `--event-path` 64 against `docs/cli.md` (F12); a job that did not start before SIGINT says `skipped`, and `--watch` says "a signal cancelled the run" on a change (F13); the layout of `--log-dir` and the stderr of `--json` differ from the documents (F14); `help` and `run --help` give different usages, `run --help` names `docs/cli.md`, the layout of `doctor --help` is broken, `doctor --run` with no executor prints the usage (F19).

## Premise

Findings A12, A13, F12, F13, F14 and F19 of the reports of the QA. The reports are in the scratch directory of the session, not in the repository.

## Approach

Make the code or the document agree for each case; the version of a release build is the version of the release.

## Prove

`cargo test -p podrunner`: A test for each case.

## Done

2026-11-16: `-q` and `--json` turn the console of the runner of `--runner-dir` off (`podrunner_broker_fake::runner::start_runner_with`), and the errors of the runner stay on stderr; the documents give the layout of `--log-dir` (`JOB.N/step-UUID.log`, written at the end of the run) and say that each other line of stderr starts with `podrunner:`; a workflow that cannot be read exits 66, and `docs/cli.md` gives 64 for a missing file of an option; after a stop, a job that its needs would skip is `cancelled` (`drive.rs`, `cancel_the_rest`); `--watch` says "a change of the tree cancelled the run" (`cancel::by_change`); `podrunner help` gives the usage of `run` as `run --help` does, and `run --help` names the URL of `docs/cli.md`; `doctor --help` gives each usage on its line; `doctor --run` with no executor gives a FAIL line with the fix (the installer, or `--runner-dir`). The version: a release build has the version of the release (`PODRUNNER_VERSION`, `release.yml`); the QA took a local build. Proven on this sandbox: `-q` on `--runner-dir` prints the lines of the jobs only; a SIGINT during `a` gives `cancelled a` and `cancelled b`.

# T-279: `--rerun --failed`, `--validate` and `--schedule` disagree with GitHub or the documents

**Source:** the adversarial QA of `12cb64e`, agent B (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`--rerun --failed` runs again the cells of a matrix that passed (F5); `--validate` takes an invalid cron and `--list` a cycle of `needs` (F15); on `--schedule`, `github.event.schedule` is empty (F16).

## Premise

Findings F5, F15 and F16 of the report of the QA. The reports are in the scratch directory of the session, not in the repository.

## Approach

Keep each passed cell; check the cron and refuse a cycle; give the cron text in the event.

## Prove

`cargo test -p podrunner-local`: A test for each case.

## Done

2026-11-16: `crates/podrunner-local/src/rerun.rs` (`kept`): with `--failed`, each cell that passed of a job that runs again for its own failed cells stays `kept`, when no need of the job runs again; `run_local/inspect.rs`: a cycle of `needs` fails `--list` and `--graph` as `--validate`, and `--validate` checks each cron of `on.schedule`; `run_local/watch.rs` and `context.rs`: `github.event.schedule` is the cron of the run. Tests `a_rerun_runs_the_failed_jobs_and_the_jobs_after_them`, `validate_checks_the_crons_and_list_refuses_a_cycle`. Proven on this sandbox: `--rerun --failed` of a matrix with one failed cell gives `kept m (1) (success)` and runs `m (2)`. Open in F5: a cancelled cell is not in `state.json` (T-286).

# T-280: `--break` on `--runner-dir` speaks of a unit, and two flags pass together

**Source:** the adversarial QA of `12cb64e`, agent B (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`--break` on `--runner-dir` offers a shell of a "unit" that is not there, and each typed text continues (F8); `--break-on-failure` with `--keep-failed` is not refused (F9).

## Premise

Findings F8 and F9 of the report of the QA. The reports are in the scratch directory of the session, not in the repository.

## Approach

The text of `--break` on `--runner-dir` names the work directory; refuse the pair of flags or document it.

## Prove

`cargo test -p podrunner`: A test of the text and of the pair.

## Done

2026-11-16: `crates/podrunner/src/run_local/pause.rs` (`watch(unit)`): on `--runner-dir` the pause names the runner and offers no shell; Enter goes on, `s` opens a shell only in a unit, and each other text is refused and the job waits. `--keep-failed` is another name of `--break-on-failure`, as `docs/cli.md` now says, so the pair is one flag.

# T-281: A runner directory with a space, and the growth of `_diag`

**Source:** the adversarial QA of `12cb64e`, agent B (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

A runner directory with a space in its path passes `doctor --run`, and each `run:` step fails (F7); `_diag` grows with two files for each job, and a killed runner leaves files of dotnet in `/tmp` (F20).

## Premise

Findings F7 and F20 of the report of the QA. The reports are in the scratch directory of the session, not in the repository.

## Approach

`doctor --run` and `run` refuse a runner directory with a space, with the fix; the run removes the `_diag` files of its job.

## Prove

`cargo test -p podrunner`: A test of each.

## Done

2026-11-16: `crates/podrunner-local/src/exec/host.rs` (`space_refusal`, used by `RunnerDir::new` and by `doctor --run`; `prune_diag` keeps the newest 20 logs of `_diag`); `crates/podrunner-broker-fake/src/runner.rs` (`DOTNET_EnableDiagnostics=0`: no debug pipes in `/tmp`); `docs/cli.md`; test `a_runner_directory_with_a_space_is_refused`. Proven on this sandbox: a job on `--runner-dir` leaves no new `clr-debug-pipe` or `dotnet-diagnostic` file in `/tmp`.

# T-282: Each live log line comes twice

**Source:** the adversarial QA of `12cb64e`, agent B (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

Each live line of a job comes twice, the second with a byte order mark, on stderr, in `--json` and in the view of `--web`; the dump after a failure is out of order.

## Premise

Finding F6 of the report of the QA. The reports are in the scratch directory of the session, not in the repository.

## Approach

Find the second source of the lines (the console of the runner and the feed of the broker) and keep one.

## Prove

`cargo test -p podrunner-local`: A test: a step that prints one line gives one live line.

## Done

2026-11-16: the cause: the live lines read each blob of the broker, also the log of the whole job, which repeats each line of the step logs with a byte order mark. `crates/podrunner-broker-fake/src/lib.rs`: `take_new` reads the step logs only, with no byte order mark; `logs` gives the log of the whole job (in the order of the steps) and the step logs only when there is none; test `the_blobs_keep_their_kind_and_the_new_text_comes_once`. Proven on this sandbox: a failed job on `--runner-dir` gives each line once, no byte order mark on stderr, and the dump in the order of the steps.

# T-283: `doctor` reports mount and chroot as denied where a user namespace mounts

**Source:** the adversarial QA of `12cb64e`, agent B (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

The probe of `pod-hostprobe` clones `CLONE_NEWNS` with no `CLONE_NEWUSER`, so a host where a user namespace with a mount works shows the mount and the chroot as denied.

## Premise

Finding F18 of the report of the QA; the code of the probe is in podcrates. The reports are in the scratch directory of the session, not in the repository.

## Approach

COORD-067 (GitHub #191) asks podcrates for a mount and a chroot probe in a user namespace with an ID map. podrunner tried `run_probe(name, ns_flags | CLONE_NEWUSER)`: the rows stay EPERM, as an `execve` with no uid map drops the capabilities of the namespace (measured as a user with no root on this sandbox). podcrates answered (PodUtils/podcrates#201): the row `mount(tmpfs,/tmp) in unshare(NEWUSER+maps,NEWNS)` (`USERNS_LEG`) is in `v0.52.0`, and `doctor` shows it now (`crates/podrunner/src/doctor.rs`; as qa2 on this sandbox: `ok`). The chroot row comes in the next tag; then `doctor` shows it in place of `chroot(/tmp)`.

## Prove

`podrunner doctor`: `doctor` on this sandbox gives the mount as `ok`.

History of the blocker: 2026-11-16: the mount row is done; the chroot row `chroot(/tmp) in unshare(NEWUSER+maps)` is in podcrates `v0.59.0` (PodUtils/podcrates#204); it waits for the bump of T-264, after the release of `v0.1.0-beta.7`.

## Done

2026-11-17: with podcrates `v0.69.0` (T-264), `doctor` shows `chroot(/tmp) in unshare(NEWUSER+maps)` (`USERNS_CHROOT`) in place of the chroot of the caller, beside the mount row (`HOST_ROWS`). Proven as root on this sandbox (`doctor --json`: `ok`); not run as a user with no root. COORD-067 (#191) is closed.

# T-284: The user-path gate has no lane of a host with a proxy only

**Source:** the adversarial QA of `12cb64e`, agent A (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** partial

## Problem

The QA host has egress through a proxy that intercepts TLS only, and the jobs of the local mode failed there (T-265); the user-path gate has no such lane, so CI did not find it (`AGENTS.md`, section 12: each host class becomes a lane).

## Premise

The reports of the QA; `scripts/lab/` has no proxy lane; `pod-lab` does not hold `proxy-only` yet (PodUtils/podcrates#188). The reports are in the scratch directory of the session, not in the repository.

## Approach

A lane `proxy-only` in `user-gate.yml`: a local proxy with its own CA (for example mitmproxy or a small CONNECT proxy), direct egress refused for the lab user, and the corpus on `--runner-dir` and `--podvm`.

## Prove

`python3 -I scripts/user-gate.py`: The lane passes in a run of `user-gate.yml`.

State (2026-11-16): `scripts/lab/proxy-only.sh` (mitmproxy on the loopback as its own user, its CA in a bundle, iptables refuses each other way out of the lab user; `LAB_PROXY_UPSTREAM` chains to the proxy of a host whose egress is a proxy) and the lane `proxy-only` of `user-gate.yml` (podvm, runner-dir); `nonroot.sh` keeps the proxy and CA variables. A podvm unit with the network of the host takes the proxy variables, and each CA file as a credential file (`crates/podrunner-local/src/exec/proxy.rs`, `podrunner_podvm::CREDENTIAL_DIR`; test `a_unit_gets_the_proxy_and_each_ca_file_once`). Not run: here the chain to the proxy of the sandbox fails (mitmproxy sends an absolute GET, which that proxy refuses with 400). The proof is the next run of the gate.

# T-285: A Dockerfile action on `--podvm` gets no container hooks

**Source:** T-270 (2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** open

## Problem

`InUnit` sets up the container hooks of a podvm unit only for a job with `container:`, `services:` or a `docker://` step (`needs_containers`). A job with an action of `runs.using: docker` gets no hooks, so the runner in the unit finds no docker for the build or the pull of the image of the action. Not measured.

## Premise

Code read: `crates/podrunner-local/src/exec.rs` (`containers`), `exec/docker.rs`.

## Approach

Measure a job with `supercharge/redis-github-action@1.8.1` on `--podvm`. Set up the hooks also for a job with a Docker action (`docker::docker_actions`), and check that the hook server builds an image of a Dockerfile, or refuse the job by name until it does.

## Prove

`cargo test -p podrunner-local`: a job with a Docker action on a podvm unit gets the hooks; a row of the corpus of the user-path gate with a Docker action on `podvm`.

# T-286: A cell that a signal cancelled is not in `state.json`

**Source:** the adversarial QA of `12cb64e`, agent B, finding F5 (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

After a run with `--state DIR` that a signal or `fail-fast` cancelled, `state.json` has no end for the cancelled cells, so `--rerun DIR --failed` sees them as cells with no end, not as cancelled ones.

## Premise

Finding F5 of the report of the QA. Not measured here.

## Approach

Write the end `cancelled` of each cell that the drive cancels into the ends of the run (`LocalSource::cell_ends`), and so into `state.json`.

## Prove

`cargo test -p podrunner-local`: a drive that a stop cancels gives a `cancelled` end for each cell that did not end.

## Done

2026-11-16: `crates/podrunner-local/src/drive.rs` (`cancel_the_rest`) and `rerun.rs` (`note_end`): each cell that a stop cancelled before it ran is an end of the run, and so in `state.json`; test `a_signal_cancels_the_job_that_runs_and_each_job_after_it`.

# T-287: An empty token does not reach a job of `permissions: {}`, and the hint names a fix that fails

**Source:** the second adversarial QA of `f5d1de7`, agent A, finding F1 (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

python-humanize/humanize has `permissions: {}` at the top of its workflow. `--github-token-env NAME` with an empty NAME, the fix that the run named (T-269), did not reach such a job, so `actions/setup-python` still sent the made-up token and failed; and `actions/github-script` (in `codecov/codecov-action`) refuses an empty token as its required input.

## Premise

Finding F1 of the report of the second QA (a run as a user with no root on this sandbox).

## Approach

An empty token is no credential: a job of `permissions: {}` takes it in place of the made-up one. The hint says that a token with read access to public repositories is the fix, and that an empty one serves the manifests of `setup-*` only.

## Prove

`cargo test -p podrunner-local --lib permissions_none`

## Done

2026-11-16: `crates/podrunner-local/src/lib.rs` (`broker_job`): an empty token reaches a job of `permissions: {}`; `crates/podrunner/src/run_local/logs.rs` (`token_hint`): the text; test `the_secrets_and_the_token_are_secret_variables_and_permissions_none_gets_no_real_token`.

# T-288: A Docker action of the tree on `--runner-dir` starts its job

**Source:** the second adversarial QA of `f5d1de7`, agent A, finding F2 (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

T-270 refused a Docker action of a repository (`OWNER/REPO@REF`) on `--runner-dir`, but not a `./` action of the tree with `runs.using: docker`, nor a `./` composite action with a `docker://` step: the dry run planned the job, and the run called the docker of the host.

## Premise

Finding F2 of the report of the second QA.

## Approach

`exec/docker.rs` reads the `action.yml` of a `./` action from the tarball of the tree of `--checkout` (or from the current directory with no `--checkout`), and follows a composite action into `./` and remote actions.

## Prove

`cargo test -p podrunner-local --lib a_docker_action_of_the_tree_is_found`

## Done

2026-11-16: `crates/podrunner-local/src/exec/docker.rs` (`Sources`, `tree_action_text`), used by `exec/host.rs` and the dry run (`run_local/inspect.rs`, `slots.rs`); test `a_docker_action_of_the_tree_is_found`.

# T-289: The P3 findings of the second QA, agent A, part 1

**Source:** the second adversarial QA of `f5d1de7`, agent A, findings F4, F5, F8 and F9 (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

The dry run planned a macOS cell that the run refuses (F4); after a SIGHUP with the output in a file, the `cancelled` lines and the end of the live log were lost (F5); after a failed job, the log of each job came again on stderr, also with the live lines on (F8); `doctor --run` with a runner directory that is not there asked for a read of a parent that is not there (F9).

## Premise

The report of the second QA, agent A (a user with no root on this sandbox).

## Approach

The dry run selects each cell as the dispatch of the run does (`dispatch::foreign`); SIGHUP sends only a terminal to /dev/null; with the live lines on, the end of a failed run names the failure and does not repeat the logs; `runner-dir-parents` leaves a path that is not there to the check `runner-dir`.

## Prove

`cargo test -p podrunner`; on this sandbox: `--dry-run` of a matrix of `ubuntu-latest` and `macos-latest` on `--runner-dir` gives the refusal of the macOS cell.

## Done

2026-11-16: `crates/podrunner-local/src/dispatch.rs` (`foreign`), `run_local/inspect.rs` (`DryRun::select`), `cancel.rs` (`isatty`), `run_local.rs` (the end of a failed run), `run_local/doctor.rs` (`parents_readable`), `docs/cli.md`. Proven on this sandbox as root: the dry run refuses the macOS cell; `doctor --run` with `/nonexist/runner` gives one FAIL.

# T-290: The P3 findings of the second QA, agent A, part 2

**Source:** the second adversarial QA of `f5d1de7`, agent A, findings F3, F6, F7 and F10 (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** open

## Problem

On `--runner-dir`, a `services:` job with a tagged image is refused as "not pinned by digest" with the expression of the matrix (T-260, T-277 A7), and a pinned one is told to use `--podvm`, which refuses services too (F3); with `--job other`, the caller of a reusable workflow is skipped with the reason of `if:`, not "not selected", and a job inside a called workflow cannot be selected (F6); `--runner-dir` has no tool cache that a user can give (`--toolcache` is for units; `RUNNER_TOOL_CACHE` of `--env` has no effect) (F7); no document gives a user with no root a way to get a runner image before a release attaches it (F10).

## Premise

The report of the second QA, agent A.

## Approach

F3: on `--runner-dir`, the refusal of containers before the check of a digest, and the fix of `services:` names `--microvm` with `--engine`. F6: the line of a call that `--job` leaves out is "not selected"; `--job CALLER/CALLEE` selects one job of a call. F7: `--toolcache DIR` on `--runner-dir` sets `RUNNER_TOOL_CACHE` of the runner. F10: `docs/install.md` names the artifact of a green `runner-image.yml` run and `podvm load -i`, and says that a build needs docker buildx.

## Prove

`cargo test -p podrunner -p podrunner-local`: a test for F3, F6 and F7; `docs/install.md` for F10.

# T-291: As a user with no root, the machine tier ends before the exit code of the runner

**Source:** user-gate 43787604846 on `12cb64e` (2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`podrunner run --microvm PODVM` of a one-step job, as the user `labuser` with no root and the files of the installer, fails: "podvm run: machine tier: the guest ended before it gave the exit code of the payload", podvm exit 125. The same image passes on the machine tier in `runner-image.yml` as the user of the GitHub runner.

## Premise

The log of the lane `plain` of run 43787604846 (`hello-microvm.err`), read on 2026-11-16.

## Approach

podvm names what the machine tier needs from a user with no root (COORD-068, GitHub #193); then podrunner fixes its side (the installer, `doctor`) or takes the release of podvm. Until then `hello:microvm` and the row `rg-rustfmt` on `microvm` of the gate are an xfail (`scripts/user-gate.py`, `HELLO_XFAIL`).

## Prove

`python3 -I scripts/user-gate.py`: `hello:microvm` passes in the lane `plain` of the gate.


The blocker of the time: 2026-11-16: podvm (PodUtils/podvm#442) did not reproduce it as uid 1000 under TCG; its guess, a guest memory of 512 MiB, does not hold (podrunner gives `--podvm-mem 4G`). The gate now runs `podvm run --verbose` of the image on the machine tier when `hello:microvm` fails (`scripts/user-gate.py`, `microvm_probe`), for the last line of the emulator; podvm `main` prints that line with no `--verbose` in its next release. Waits for that line. The probe of user-gate 43795123788 gave the line: "podvm run: machine tier: a guest under KVM ..., base archive from the cache"; "the guest ended before it gave the exit code of the payload; qemu: qemu-system-x86_64: -chardev socket,id=podvmerr,path=/tmp/podvm-guest-ID.err,server=on,wait=on: info: QEMU waiting for connection on: disconnected:unix:...". The probe fails with no podrunner in it (`podvm run --rm --podvm-tier=machine IMAGE true`). COORD-069 (GitHub #195, 2026-11-16) sends podvm the line and the environment of the lab user. Waits for its answer.

State (2026-11-16): podvm (PodUtils/podvm#444, COORD-151) passes the machine tier as a new user made by `sudo -u` on a runner with KVM, so a user with no root is not the cause. podvm `v0.2.0-beta.25` names how the emulator ended (its exit code or signal, whether the guest reached the command, the last console lines). podrunner pins it; the probe of the next user-path gate gives that message. Next: send it to podvm in a new COORD issue, with the size of the root of the image (`podvm image inspect`).

State (2026-11-16): with podvm `0.2.0-beta.25`, the probe of user-gate 43834454578 says `the emulator was still running, and podvm killed it, before the guest reached the command`, with no console line. COORD-072 (GitHub #198) sends it to podvm.

State (2026-11-16): podvm (PodUtils/podvm#451, COORD-158) booted the machine tier as a user made by `sudo -u` and with an image of 300 MiB; the console of the emulator closed while QEMU ran. `microvm_probe` now removes the cached base archives of the store and probes the runner image and `alpine:3.20`, with podvm `0.2.0-beta.26`, whose message names the seconds and the size of the initrd. Next: send its lines to podvm.

State (2026-11-16, user-gate 43870085390, podvm `0.2.0-beta.27`): with no cached base, `alpine:3.20` passes the machine tier as the lab user; the runner image fails: `the console of the emulator closed 2.6 s after the start, with an initrd of 1348 MiB`. COORD-073 (GitHub #199) sends it to podvm.

State (2026-11-16): podvm (PodUtils/podvm#454, COORD-161) names the cause: the initrd of the runner image (1348 MiB) needs a guest of more than 4 GiB. The microvm kind and `--microvm` now give each guest 8 GiB by default (`[unit.microvm] mem`), and the probe of the gate uses 8G. The proof is the next user-path gate: `hello:microvm` then passes, and the gate asks to remove it from `HELLO_XFAIL`.

## Done

2026-11-16, `a98857a`: the cause was the memory of the guest: the initrd of the runner image (1348 MiB) needs more than 4 GiB (PodUtils/podvm#454); each guest of the microvm kind and of `--microvm` gets 8 GiB by default. Proven in CI: user-gate 43877528216 on `a98857a`, lane `plain` (a user with no root, KVM): `hello:microvm` and `corpus:rg-rustfmt:microvm` pass; both are `ok` rows now.


# T-292: A project file reads a file out of the project through `.actrc` or a link

**Source:** the second adversarial QA of `f5d1de7`, agent B, findings F3, F10 and F11 (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A project `.actrc` gave `--env-file`, `--var-file` and `--input-file` paths out of the project (`../`, absolute, links) to the run with no word, and a private value reached a job (F3); an unreadable `.podrunner/run.toml` was skipped with no word (F10); a project `run.toml` that is a link out of the project was read, and the parse error showed its first line (F11).

## Premise

The report of the second QA, agent B (a user with no root on this sandbox).

## Approach

The path keys of a project `.actrc` take the rule of `run.toml` (`path_value`); a project file is read through `project_file`, which refuses a link out of an untrusted project and fails on a file that cannot be read.

## Prove

`cargo test -p podrunner --lib run_config`

## Done

2026-11-16: `crates/podrunner/src/run_config.rs` (`actrc` with the project, `project_file`); test `actrc_of_a_project_keeps_its_files_in_the_project`.

# T-293: `github.api_url` of a job names the fake broker

**Source:** the second adversarial QA of `f5d1de7`, agent B, finding F2 (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A job had `GITHUB_API_URL` and `github.api_url` of the fake broker and no `GITHUB_GRAPHQL_URL`, so octokit (`actions/github-script`) called the broker ("no route: GET //repos/...") and not GitHub.

## Premise

Finding F2 of the report of the second QA.

## Approach

The `github` context of a job has `api_url` `https://api.github.com` and `graphql_url` `https://api.github.com/graphql`, as on GitHub; the scripts of podrunner (the stand-in of `actions/checkout`, the pause of `--break`) reach the broker by `github.podrunner_broker`.

## Prove

`cargo test -p podrunner-broker-fake`

## Done

2026-11-16: `crates/podrunner-broker-fake/src/message.rs`, `crates/podrunner-local/src/message.rs`, `crates/podrunner/src/run_local/pause.rs`. Proven on this sandbox: on `--runner-dir`, `actions/checkout@v4` passes and a step sees `GITHUB_API_URL=https://api.github.com` and `GITHUB_GRAPHQL_URL=https://api.github.com/graphql`.

# T-294: A runner that a SIGKILL of podrunner leaves breaks the runner directory

**Source:** the second adversarial QA of `f5d1de7`, agent B, finding F1 (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A SIGKILL of podrunner (or a third signal) leaves the runner and its processes; the lock of T-274 goes with podrunner, so the next run starts, and each job fails with "`_diag/blocks/...` already exists". `doctor --run` gives `ok`.

## Premise

Finding F1 of the report of the second QA.

## Approach

The runner gets `PR_SET_PDEATHSIG` (SIGKILL) at its start, so it ends with podrunner; a run clears `_diag/blocks` with `_diag/pages`; `doctor --run` names a runner process that holds the runner directory.

## Prove

`cargo test -p podrunner-local`

## Done

2026-11-16: `crates/podrunner-local/src/exec/host.rs`: a run writes the process group of its runner to `.podrunner-run.group`; the next run, with the lock, ends that group when its leader still works in the runner directory (`end_stale`), and clears `_diag/blocks` with `_diag/pages`. Proven on this sandbox: a SIGKILL of podrunner during `sleep 60` left 4 runner processes; the next run ended them by name and passed.

# T-295: A rerun with a matrix filter rewrites a failed cell as skipped

**Source:** the second adversarial QA of `f5d1de7`, agent B, finding F4 (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`--rerun DIR --failed --matrix K=V` with a filter that leaves out the failed cell writes that cell into `state.json` as `skipped`, and exits 0; the failure is lost for each later rerun.

## Premise

Finding F4 of the report of the second QA.

## Approach

A cell that the filter of a rerun leaves out keeps its end of the state (`kept`), and is not written as `skipped`.

## Prove

`cargo test -p podrunner-local`

## Done

2026-11-16: `crates/podrunner-local/src/rerun.rs` (`with_kept`), used when `run` writes `state.json`; test `a_rerun_keeps_the_end_of_a_cell_that_its_filter_leaves_out`. Proven on this sandbox: after `--rerun DIR --failed --matrix n=1`, `state.json` keeps `failure` for `m (2)`, and the next `--rerun DIR --failed` runs it.

# T-296: The lines of the results come at the end, and a SIGHUP loses them

**Source:** the second adversarial QA of `f5d1de7`, agent B, findings F5, F6 and F13 (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The result lines of stdout (also with `--json`) are written when the run ends; a SIGHUP with the output in a file or a pipe loses them and the `cancelled` lines (F5, F6). With `--json`, a failed run writes "--- log of" dumps that are not JSON on stderr (F13).

## Premise

Findings F5, F6 and F13 of the report of the second QA.

## Approach

Write each result line when its job ends, and flush it; with `--json`, the dump of a log is one JSON object for each line.

## Prove

`cargo test -p podrunner`

## Done

2026-11-16: `crates/podrunner/src/run_local.rs` (`STREAM`, `emit`, set by `main.rs`): each result line goes to stdout, flushed, when its job ends; `many.rs` writes the header of a workflow before its lines; with `--json` and `-q`, the dump of a failed run is one JSON object for each line. Proven on this sandbox: a SIGHUP with stdout and stderr in files gives `cancelled a`, `cancelled b` and "a signal cancelled the run".

# T-297: `--microvm` does not check its image and `/dev/kvm` before the run

**Source:** the second adversarial QA of `f5d1de7`, agent B, finding F7 (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`--microvm` skips the check that the image is in the store and that `/dev/kvm` opens, which `--podvm` makes; the unit then asks Docker Hub for the image (against "a unit never pulls"), and the error is `exit Some(Code(125))`.

## Premise

Finding F7 of the report of the second QA.

## Approach

The check of T-272 (`image_in_store`) for `--microvm` and `--incus` too, and a check of `/dev/kvm` for `--microvm`, with the fix; `doctor --run` the same.

## Prove

`cargo test -p podrunner`

## Done

2026-11-16: `crates/podrunner/src/run_local/guest.rs` (`kvm`, the check of the image), the dry run (`slots.rs`) and `doctor --run` (`kvm` opens `/dev/kvm`). Proven on this sandbox (no KVM): `run --microvm` fails at once with the fix of `/dev/kvm`.

# T-298: The P3 findings of the second QA, agent B

**Source:** the second adversarial QA of `f5d1de7`, agent B, findings F8, F9, F12, F14 to F18 (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** open

## Problem

A link to a runner directory with a space passes (F8); a workflow file is read with no size limit, and a FIFO hangs (F9); a `--log-dir` that cannot be written loses the logs with exit 0 (F12); with no `-q`, the console of the runner prints with no prefix (F14); exit codes and messages of a missing file of a flag (F15); several wrong causes or fixes (F16); each usage error prints the whole usage on one line (F17); small gaps: `--matrix` of a matrix of calls, `workflow_dispatch` with no `inputs:`, two runs on one `--state`, `_diag` keeps 22, the exit of `--watch` and `--schedule` at a signal, `~` in `trusted` (F18).

## Premise

The report of the second QA, agent B.

## Approach

Fix each one; the report of the QA lists the commands.

## Prove

`cargo test -p podrunner -p podrunner-local`

# T-299: In the lane proxy-only, the lint of rq on `--runner-dir` fails

**Source:** user-gate 43795123788 on `a4b41b9`, the first run of the lane `proxy-only` (2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

In the lane `proxy-only` (mitmproxy on the loopback, the lab user reaches the network only through it), the row `rq-lint` on `runner-dir` fails ("failure Lint"); it passes in the four other lanes. The other rows of the lane gave their results as in the other lanes.

## Premise

The log `logs/rq-lint-runner-dir` of the artifact `lane-proxy-only` of run 43795123788: the step `astral-sh/ruff-action@v3` (`using: node20`) fails with "connect ECONNREFUSED 20.29.134.17:443". The action asks the API of GitHub for the ruff version by `fetch`, and the `fetch` of Node ignores `HTTPS_PROXY`. Node 22.21 and 24.5 or later use the proxy variables with `NODE_USE_ENV_PROXY=1`; Node 20 does not. The runner v2.338.2 runs a `node20` action on Node 20.20.2 unless the job message has the flag `actions.runner.usenode24bydefault` (GitHub sends it); the fake broker did not send it. Reproduced on the sandbox with the action at `4919ec5` and Node 22.23: with the variable, the call goes through the proxy; without it, the call goes direct.

## Approach

Read the log of the step that failed (ruff, or the setup of Python or of uv through the proxy and its CA); fix the variable or the path that the job misses on a proxy host (T-265), or name the cause in an entry of another project.

## Prove

`python3 -I scripts/user-gate.py`: the row `rq-lint` on `runner-dir` passes in the lane `proxy-only`.

State (2026-11-16): the fake broker sends `actions.runner.usenode24bydefault` (`message.rs`), so a `node20` action runs on Node 24 as on GitHub. A job on a runner directory and a unit with the proxy of the host get `NODE_USE_ENV_PROXY=1` when a proxy variable is set and the user did not set it (`exec/host.rs`, `node_proxy`; `exec/proxy.rs`). Unit tests: `cargo test -p podrunner-local --lib`. On the sandbox with the runner 2.338.2, a local `node20` action prints `node=v24.22.1 envproxy=1` (the sandbox has a direct route too, so it does not show the refusal). The proof is the next run of the user-path gate.

## Done

2026-11-16, `f2d7b66`: `actions.runner.usenode24bydefault` in the job message and `NODE_USE_ENV_PROXY=1` with a proxy. Proven in CI: user-gate 43816078517 on `12a1300`: `corpus:rq-lint:runner-dir` passes in the lane `proxy-only` (and in the four other lanes).

# T-300: A job in a podvm unit writes into the home of the user on the host

**Source:** the third adversarial QA of the release files of `12a1300` (podvm `0.2.0-beta.24`), agent A, as the user `qa1` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

A static binary in a podvm unit (Go up to 1.20, and the toolchains of finding A5: uv, cross, the installer of uv) reaches the files of the host: with `GOCACHE=/home/qa1/ESCAPED-gocache`, the directory comes on the host with 257 directories; with `GOCACHE=/tmp/qa1-gocache`, in the `/tmp` of the host. In the unit: uid 30033 (qa1), `LD_PRELOAD=.../rootfs/.podvm/interpose.so`, `uid_map` `0 0 4294967295`: no user namespace. Go 1.23 stays in the unit. The same class gives `stat /root/go/pkg/mod: permission denied` (cobra), `Could not persist temporary file` (uv), `interpose.so cannot be preloaded` (cross), `Unable to find /proc/self/exe`, and the host path `/tmp/podrunner-run-units-PID/...` in the job log (A5).

## Premise

Finding A1 and A5 of agent A. Reproduced by the lead session as `qa1` with the workflow of the agent (`setup-go` 1.20, `go build` of a program): podvm `0.2.0-beta.24` makes `/home/qa1/ESCAPED-gocache` and `/tmp/qa1-gocache` on the host. With podvm `0.2.0-beta.25` neither comes, and `podvm inspect` of a `run -d` names the rung `namespace` (beta.24: `userland`). The unit of podrunner then fails to start (T-301).

## Approach

Take podvm `0.2.0-beta.25` or later, which runs `run -d` of a user with no root on the namespace rung (a mount namespace of its own), when T-301 holds. Add a row to the user-path gate: a static binary of a job writes an absolute path of the home of the user, and the gate checks that the host has no such file. Until then, the notes of the release name the class.

## Prove

The new row of `scripts/user-gate.py` passes in each lane; `cargo test -p podrunner-podvm`.

State (2026-11-16): podvm `0.2.0-beta.25` is pinned (`ede07ea`) and its unit starts again (T-301). `scripts/user-gate.py` has the check `confine:podvm`: a job builds a Go 1.20 program that writes into the home of the lab user, and the file must not come on the host. Proven on a constrained host as a user (the job of the agent, as `qa1`): no file with beta.25; the file with beta.24. The proof in CI is the next user-path gate.

State (2026-11-16, user-gate 43834454578 on `a5079e6`): in CI each lane ran the podvm units on the loader family (`go` finds no GOROOT; uv and ruff get EACCES), as a `run -d` with a volume stays off the namespace rung in podvm `0.2.0-beta.25` until pod-enter binds its volumes (PodUtils/podvm#442). There a static binary of a job can still reach the files of the user; the check `confine:podvm` now fails only on the file on the host. Waits for the podvm release with the binds of `run -d`; a host with no user namespace keeps the loader family, so the notes name the limit.

State (2026-11-16, later): podvm says `0.2.0-beta.25` binds the volumes of `run -d` too (PodUtils/podvm#442, closed), yet the CI lanes ran the units on the loader family. The likely cause is the host: Ubuntu 24.04 (GitHub's runners) restricts unprivileged user namespaces with AppArmor, so podvm cannot take the namespace rung there. `podrunner doctor --run` now prints the rung of podvm (`podvm system info`) and, off the namespace rung, warns that a static binary of a job can reach the files of the user and names `--microvm`, which works for a user with no root since T-291. Next: read the rung of each lane in the doctor logs of the next user-path gate; then the limit stays documented (the notes, `docs/local-mode.md`) or goes to podvm.

## Done

2026-11-16, `30b35ec`: on the namespace rung (podvm `0.2.0-beta.25` and later, a host with user namespaces that AppArmor does not restrict) a job cannot reach the files of the user: proven on a constrained host as a user (`qa1`, the job of the agent, no file on the host). Where podvm cannot take that rung, the limit holds by design of podvm's userland rungs: user-gate 43894112975 shows `rung: supervise` in each CI lane (GitHub's Ubuntu 24.04). The call of 2026-11-16 in `docs/decisions.md`: podrunner runs podvm units there, `doctor --run` warns with the rung, and `docs/local-mode.md` ("What a podvm unit holds") and the notes name `--microvm` for untrusted jobs. The check `confine:podvm` of the gate holds on the file of the host in each lane.


# T-301: With podvm 0.2.0-beta.25 the runner of a podvm unit ends with no result

**Source:** the third adversarial QA of the release files of `12a1300` (podvm `0.2.0-beta.24`), agent A, as the user `qa1` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

With podvm `0.2.0-beta.25`, `podrunner run hello.yml --podvm` as a user with no root fails: `u-1: unit-start-failed: podvm run: exit 125: podvm run: --hostname needs the handler of the native-static rung or the interposer of the loader family, and this entry has neither and no UTS namespace`, then the start again with no `--hostname`, then `failure hello: the runner in u-1 ended with no result (exit Some(Code(0)))`. No job log.

## Premise

Measured by the lead session as `qa1` on the QA host (user namespaces, no KVM) with the image `runner@sha256:0aae39da...` and podvm `0.2.0-beta.25`; podvm `0.2.0-beta.24` passes the same run. The pin of beta.25 is on `main` (`ede07ea`); runner-image run 43828189959 runs it.

## Approach

Read the output of the unit (`podvm logs`) after the start with no `--hostname`; compare the tree that `podvm extract` gives (where the driver writes the credential files) with the root of the `run -d` on the namespace rung. Fix the driver, or write a COORD issue to podvm. Then `--hostname` on the namespace rung: ask podvm for a UTS namespace there.

## Prove

`podrunner run hello.yml --podvm` passes as a user with no root with podvm `0.2.0-beta.25`; runner-image.yml green on the pin.

## Done

2026-11-16: the cause, from the logs of the unit: on the namespace rung the payload is uid 0, and the runner refuses root (`Must not run interactively with sudo`). A podvm unit sets `RUNNER_ALLOW_RUNASROOT=1`, and `TAR_OPTIONS=--no-same-owner` and `npm_config_unsafe_perm=true` for the same root (T-317 asks podvm for the user of the image). The start with no `--hostname` prints one plain line. Proven on a constrained host as a user: as `qa1` with podvm `0.2.0-beta.25`, `hello.yml` passes, and the job of T-300 passes with no file on the host. CI: the next runner-image run.


# T-302: The egress hold of the installer refuses each run where user namespaces exist

**Source:** the third adversarial QA of the release files of `12a1300` (podvm `0.2.0-beta.24`), agent A, as the user `qa1` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

With the `podnet` and `allow` lines of `run.toml` on, as its comment says, each run fails: `--podnet: the hold is Enforced; the local mode takes only the seccomp hold (advisory) now`, exit 80, and `doctor --run` says `ok podnet`. On a host with user namespaces the driver takes the `netns` backend, which the local mode refuses (its broker is on the loopback of the host).

## Premise

Finding A2 of agent A; the code: `crates/podrunner-podvm/src/unit/hold.rs` (`choose`), `crates/podrunner/src/run_local/routes.rs`.

## Approach

The local mode asks the driver for the `seccomp` backend also where `netns` holds (`with_podnet_seccomp`).

## Prove

`cargo test -p podrunner-podvm`; as a user with no root on a host with user namespaces, `podrunner run hello.yml` with `--podnet` passes and refuses each host that `--allow` does not name.

## Done

2026-11-16: the local mode asks the driver for the `seccomp` backend (`PodvmUnit::with_podnet_seccomp`; `choose` takes the ask). Unit test: `the_hold_comes_from_pod_doctor`. Proven on a constrained host as a user: on the QA host (user namespaces, a proxy only) as `qa1`, `podrunner run hello.yml --podvm --podnet --allow github.com` gives `ok hello`, and the unit's connects to `archive.ubuntu.com` and `security.ubuntu.com` fail at the policy.


# T-303: An executor flag does not replace the executor of run.toml

**Source:** the third adversarial QA of the release files of `12a1300` (podvm `0.2.0-beta.24`), agent A, as the user `qa1` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

With the `run.toml` of the installer (`podvm = ...`), `podrunner run ... --runner-dir DIR` fails: `--runner-dir and --podvm are two executors: give one, or map labels to each with --platform`, exit 64, and the whole usage. `--microvm` gives the same. The message names neither the file nor `--no-config`.

## Premise

Finding A3 of agent A.

## Approach

An executor on the command line replaces the executor of `run.toml`; the other keys of the file stay. A refusal that comes from the file names the file.

## Prove

`cargo test -p podrunner --test local`: a run with `run.toml` and `--runner-dir` uses the runner directory.

## Done

2026-11-16: `run_config::executor_of_command_line`: an executor flag of the command line drops the executor flags of the files, and `--runner-dir` also their `images`, `image`, `podnet` and `engine`; `docs/cli.md` says so. Unit test: `an_executor_of_the_command_line_replaces_the_one_of_the_files`.


# T-304: The checkout of the tree answers 404 to later jobs of a run

**Source:** the third adversarial QA of the release files of `12a1300` (podvm `0.2.0-beta.24`), agent A, as the user `qa1` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

Two runs on one tree, 80 s apart (`--podvm --jobs 2` and `--runner-dir`): from one time on, each later job of both runs got `curl: (22) ... 404` on `_checkout.tar.gz`, then `gzip: stdin: unexpected end of file` (15 cells). The result lines say only `failure`; no line of podrunner names the cause. Five more tries did not show it again.

## Premise

Finding A6 of agent A; the logs `cobra-podvm-full.txt` and `cobra-rd.txt` of the QA host. Not reproduced.

## Approach

Read the life of the tarball of the checkout (where it is written, when it is removed, whether two runs share a path); give each run its own; when the route answers 404, write a line of podrunner that names it.

## Prove

`cargo test -p podrunner --test local`: two runs on one tree at once; each job of both gets the tree.

State (2026-11-16): the broker answers 404 only when it cannot read the tarball of the tree (`actions.rs`), which each run keeps in its own `TMPDIR/podrunner-run-tree-PID`. Both runs of the agent lost their tree at the same second, so the likely cause is a removal from outside (the cleanup of the agent, or of `TMPDIR`). The broker now names the path and the error on stderr. The cleanup of dead runs (T-312) counts each live process as a live run, so it never removes the tree of a running podrunner, also one started through a link of another name. Next: a test with two runs on one tree at once (the Prove).

## Done

2026-11-16, `0af29ee`: not reproduced; a tree that goes during a run is named now. Proven on a constrained host as a user (`qa2`): two runs of one tree at once, `--runner-dir` and podvm, each of five jobs with `actions/checkout`, gave `ok` for each of the ten jobs.


# T-305: actions/setup-python v7 fails with the made-up token

**Source:** the third adversarial QA of the release files of `12a1300` (podvm `0.2.0-beta.24`), agent A, as the user `qa1` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`actions/setup-python` v7 fails at `Failed to fetch the Python versions manifest` on the default path: `raw.githubusercontent.com` answers 404 to a made-up token and 200 to no token. The fix (`--github-token-env NAME`) comes only after each job ends. With an empty variable the job passes on `--runner-dir`.

## Premise

Finding A7 of agent A; T-254 kept the made-up token.

## Approach

Do not give the made-up token where it changes an answer of a real host: for example, an empty `github.token` when the user gives none and the action takes an empty one, or name the fix in the job when a step fails with such a line.

## Prove

A test of `setup-python` v7 on `--runner-dir` with no flag passes.

## Done

2026-11-16: the live sink prints the fix (`--github-token-env NAME`; an empty NAME for the manifests of setup-*) once, at the first line of a job that shows the refusal of the made-up token, and the end of the run gives it again (`logs::refused`, `TOKEN_HINT`). The made-up token stays (T-254: `actions/github-script` refuses an empty one).


# T-306: On --runner-dir, a Python of setup-python cannot load libpython

**Source:** the third adversarial QA of the release files of `12a1300` (podvm `0.2.0-beta.24`), agent A, as the user `qa1` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

With `update-environment: false`, the Python of `actions/setup-python` on `--runner-dir` fails: `error while loading shared libraries: libpython3.14.so.1.0` (the tool cache is not at `/opt/hostedtoolcache`). `--env LD_LIBRARY_PATH=RUNNER/_work/_tool/Python/3.14.10/x64/lib` works.

## Premise

Finding A8 of agent A.

## Approach

Give `RUNNER_TOOL_CACHE` a path that the builds of setup-python take, or set the path of the libraries; else document the limit and the work-around in `docs/local-mode.md`.

## Prove

A job with `setup-python` and `update-environment: false` runs `python --version` on `--runner-dir`.

## Done

2026-11-16: the limit and its work-around are in `docs/local-mode.md`, "A host with a proxy": the Pythons of setup-python take `libpython` from `/opt/hostedtoolcache`, which a user with no root cannot make; `--env LD_LIBRARY_PATH=WORK/_tool/Python/VERSION/x64/lib` or `update-environment` on (the work-around that agent A measured).


# T-307: The documents load the larger image into the wrong store

**Source:** the third adversarial QA of the release files of `12a1300` (podvm `0.2.0-beta.24`), agent A, as the user `qa1` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`podvm load -i runner-full-x86_64.tar`, as the documents give it, loads into `~/.local/share/podvm`; then `--platform ubuntu-latest=runner@sha256:...` fails, and its fix (`PODVM_STORE=.../images podvm load -i ...`) works. `podvm load` prints no digest; `podvm images --digests` gives it.

## Premise

Finding A9 of agent A.

## Approach

`docs/local-mode.md` and the notes give the store (`PODVM_STORE` of the installer) and the command for the digest.

## Prove

`python3 -I scripts/user-gate.py`: the row `install:full` runs the commands of the documents as written and a job on the larger image.

## Done

2026-11-16: `docs/local-mode.md` gives `sh podrunner-install.sh --full`, and by hand the store of the installer (`PODVM_STORE=~/.local/share/podrunner/images`), the command of the digest (`podvm images --digests`) and the `--platform` line; `docs/install.md` names the store at the asset. The row `install:full` of the user-path gate runs `--full`.


# T-308: The P3 findings of the third QA, agent A

**Source:** the third adversarial QA of the release files of `12a1300` (podvm `0.2.0-beta.24`), agent A, as the user `qa1` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** open

## Problem

A10: setup-node@v4 cannot reach the cache service (actions/cache@v4 can). A11 and A12: live lines out of order (`Cleaning up orphan processes` first); with `--jobs 2` the steps print above their header. A13: refusals on `--runner-dir` say "not pinned by digest" for a job that needs containers. A14: `--platform windows-latest=host` gives a green job; "a unit is Linux" on `--runner-dir`. A15: `--dry-run` exits 76 at `fromJSON` of an output of a needed job. A16: a dry run writes the lock into the tree and pulls a service image that it refuses. A17: `github.workflow` is the file name, `github.workflow_ref` is empty. A18: a run that a concurrency group cancels does not name the group. A19: `_home` and `_work` stay after a run (2.7 GiB). A20: the runner credential files stay after a SIGHUP. A21: they have mode 0664 under umask 002. A22: a spurious `egress: job podrunner-run-fetch github.com:443 failed at connect`. A23: the line of the hold prints before a refusal. A24: the images lack tools of ubuntu-latest (make, python3, sudo, cargo, yarn), and the documents do not list them. A25: a cell that fail-fast cancels has no reason. A26: the notes say to run the installer in the bundle, which unpacks into a subdirectory. A27: a failed mirror install leaves files, and install.md does not list the files of a mirror. A28: `pins 1 references`; doctor says ok for a busy runner directory; step logs named by UUID; `if: success()` for a dependent of a job that `--job` leaves out. Also, the lead session: with `--podnet`, a line `egress: job u-1 null:null failed at proxy (usage)`.

## Premise

The report of agent A (the commands and outputs are in it).

## Approach

Fix each one; a finding that the code cannot change goes into the documents.

## Prove

`cargo test -p podrunner -p podrunner-local`

# T-309: A podvm unit reports ok for a failing test suite of npm 6

**Source:** the third adversarial QA of the release files of `12a1300` (podvm `0.2.0-beta.24`), agent B, as the user `qa2` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

`sindresorhus/escape-string-regexp` at `a7f7dabaf336bda98d2166848af15d9168b10bcf` (`main.yml`, `npm test` = `xo && ava && tsd`, Node 14 and 12, npm 6): on podvm each cell is `ok`, and nothing comes after `> xo && ava && tsd`. On `--runner-dir`: `3 tests passed`, 240 lines of tsd errors, `failure`. In the unit, `node node_modules/ava/cli.js --version` gives `3.15.0` from a shell and nothing (exit 0) under npm 6.

## Premise

Finding B-01 of agent B. The lead session reproduced it with podvm `0.2.0-beta.25` and the variables of T-301 (both cells `ok`, no test output). Measured in the unit: a script of npm 6 (`node -e "console.log(42); process.exit(3)"`) gives its output and exit 3; `pwd`, `pwd -P`, `process.cwd()`, `PWD` and `INIT_CWD` give one path; `ava --version` called directly works.

## Approach

Find what makes a node child of npm 6 end with no output in the unit: the environment of npm (`npm run env`) against the shell, `strace -f` of `npm test` in the unit, and the `import-local` and `resolve-cwd` of xo and ava. Fix the unit, or write a COORD issue to podvm.

## Prove

`podrunner run .github/workflows/main.yml --checkout .` of that project on podvm gives `failure` for each cell, as on `--runner-dir`.

State (2026-11-16): the cause, measured in a unit of podvm `0.2.0-beta.25`: podvm gives the payload descriptor 17 (`ownership.memo`); npm 6 closes it before a script; the first file that node opens then gets 17, and a read from its offset gives 0 bytes (a read at offset 0 gives the data). So ava, xo and tsd read empty files and end with no output and exit 0. Without npm: `bash -c 'exec 17<&-; exec 17</etc/passwd; head -c 20 <&17'` gives 0 bytes; with descriptor 18 it gives the data.

2026-11-16, the blocker of the time: COORD-071 (GitHub #197) asks podvm not to keep a fixed descriptor 17 for its memo.

## Done

2026-11-16: podvm `0.2.0-beta.26` (PodUtils/podvm#450, COORD-157) checks that descriptor 17 is still its memo (`PODVM_MEMO_ID=DEV:INO`) before each use; podrunner pins it (`images/podvm.toml`, the installer). Proven on a constrained host as a user (`qa2`): the job of `sindresorhus/escape-string-regexp` at `a7f7daba` on podvm gives `3 tests passed`, the 240 lines of tsd and `failure` for both cells, as on `--runner-dir`.


# T-310: The live log of a step comes only when the step ends

**Source:** the third adversarial QA of the release files of `12a1300` (podvm `0.2.0-beta.24`), agent B, as the user `qa2` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A step that prints `tick` six times at 10 s gives all six lines at its end, on both executors and with `--json`. `docs/cli.md` says that each line goes to stderr while the job runs. A long step looks hung.

## Premise

Finding B-02 of agent B; A11 and A12 of agent A (the order of the lines).

## Approach

Read how the broker gets the lines of a step (the web console feed of the runner against the log blobs at the end of a step); print each line when the broker gets it, in the order of the runner.

## Prove

`cargo test -p podrunner-local --test runner -- --ignored`: a step with a pause between two lines gives the first line before the step ends.

## Done

2026-11-16: the cause: in the flow of the results service, the runner sends the lines of a running step only over a websocket at `FeedStreamUrl` of the endpoint, which the fake broker did not give (`JobServerQueue`, `ResultsServer` of the runner 2.338.2). The broker now gives `FeedStreamUrl` when it is on the loopback and takes the websocket (`podrunner-broker-fake/src/feed.rs`); each message goes to the live output at once, and the blob of a step that the feed gave is not printed again. A guest of `--microvm` and `--incus` reaches the broker through its front, which takes no websocket, so its lines still come at the end of each step. Unit tests: `feed::tests`. Proven on a constrained host as a user (`qa2`): a step that prints `tick` four times at 8 s gives each line 8 s apart, on `--runner-dir` and on podvm; `check.sh` with the runner suites passes.


# T-311: container: and docker:// steps on podvm are planned and then refused

**Source:** the third adversarial QA of the release files of `12a1300` (podvm `0.2.0-beta.24`), agent B, as the user `qa2` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

On podvm, `--dry-run` gives `planned` and `doctor --run` gives `ok` for a job with `container:` or a `docker://` step; the run then fails: `the job needs containers, and this unit kind has no container hooks`, with no fix. `docs/cli.md` says such steps run through the container hooks of the unit; `doctor --config` with `[unit.podvm]` shows `container none`.

## Premise

Finding B-04 of agent B.

## Approach

The dry run and `doctor --run` refuse such a job as the run does, with the fix (`--microvm --engine`); the documents say which kind has container hooks.

## Prove

`cargo test -p podrunner --test local`: the dry run of a `container:` job on podvm is refused with the fix.

## Done

2026-11-16: the dry run on podvm asks podvm's probe for its container route and refuses a job with containers where it is not `hooks`, with the fix `--microvm and --engine` (`inspect::NO_HOOKS`; the run gives the same fix); `doctor --run` prints a line `containers` with the route (measured as `qa2`: `podvm gives no container hooks here ...`); `docs/cli.md` says so. Unit test: `a_dry_run_on_podvm_with_no_hooks_refuses_a_container_job`.


# T-312: A SIGKILL during a podvm job leaves the unit and its files

**Source:** the third adversarial QA of the release files of `12a1300` (podvm `0.2.0-beta.24`), agent B, as the user `qa2` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A SIGKILL (or a third SIGINT) during a podvm job leaves the unit running and 2.4 GB in `/tmp/podrunner-run-units-PID`; the next `run` and `doctor` neither name nor remove them. One Ctrl-C during the start of a unit takes 28 to 46 s to end the run, with no message.

## Premise

Finding B-05 of agent B; T-294 does it for `--runner-dir`.

## Approach

Each run writes its unit directory and process group where the next run finds it (as T-294); the next run ends a stale unit with the label `podrunner.unit` and removes its directory, and says so. At the first signal, print a line that the unit is being stopped.

## Prove

`cargo test -p podrunner --test local`: a stale unit directory of a dead run is removed by the next run.

## Done

2026-11-16: `run_local/stale.rs`: each run finds the directories `podrunner-run-units-PID` and `podrunner-run-tree-PID` of this user in `TMPDIR` whose PID is no live podrunner, ends each process of their units (by the tag of each unit) and removes them, with a line. The first SIGINT or SIGTERM prints that the job ends and its unit goes. Unit test: `the_directories_of_dead_runs_are_found`. Proven on a constrained host as a user (`qa2`, podvm `0.2.0-beta.24`): a SIGKILL of a run in a podvm job left its directory and 8 unit processes; the next run said `the files of a run that died are gone (1 units, 8 processes ended)`, passed `hello`, and no unit process stayed.


# T-313: A failed --export is silent

**Source:** the third adversarial QA of the release files of `12a1300` (podvm `0.2.0-beta.24`), agent B, as the user `qa2` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`--export DIR` to a directory of mode 500, to a file or to `/proc/nope` gives exit 0 and no message, on both executors. `docs/cli.md` says `DIR/JOB`; the path is `DIR/JOB.N`.

## Premise

Finding B-06 of agent B.

## Approach

A failed export fails the run with the path and the cause, before the jobs where it can (a check of `DIR`); the documents give `DIR/JOB.N`.

## Prove

`cargo test -p podrunner-local --lib`: an export to a path that cannot be written gives an error.

## Done

2026-11-16: `routes::export_dir` makes `--export DIR` and writes and removes a probe file before the first job; a failure stops the run with the path and the cause (measured as `qa2`: `--export /proc/nope` gives `podrunner: --export /proc/nope: No such file or directory`). A failed copy after a job is named on stderr too. `docs/cli.md` gives `DIR/JOB.N`.


# T-314: A relative trusted path is read from the current directory

**Source:** the third adversarial QA of the release files of `12a1300` (podvm `0.2.0-beta.24`), agent B, as the user `qa2` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

With `trusted = ["."]` in the `run.toml` of the user, a project's `.podrunner/run.toml` is taken when run from that project and refused from `~`. `docs/cli.md` says a relative path is read from the directory of the file. `"~/proj"` is not expanded and trusts nothing, with no message.

## Premise

Finding B-07 of agent B.

## Approach

Read a relative path of `trusted` from the directory of the file; expand `~/`; name an entry that trusts nothing.

## Prove

`cargo test -p podrunner --lib`: `trusted = ["."]` names the directory of the file.

## Done

2026-11-16: `run_config::trusted_path`: a relative path of `trusted` is read from the directory of the file of the user, and `~/DIR` from the home; `docs/cli.md` says so. Unit test: `a_trusted_path_is_read_from_the_directory_of_the_file`.


# T-315: The runner images have no sudo and lack tools of ubuntu-latest

**Source:** the third adversarial QA of the release files of `12a1300` (podvm `0.2.0-beta.24`), agent B, as the user `qa2` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`sudo apt-get install -y cowsay` fails with `sudo: command not found` in each image; the base image has no `make`, `python3`, `cargo`, `docker` or `yarn`, the larger one no `cargo`, `yarn`, `docker` or `sudo`. The unit user has no passwd entry (`I have no name!`) and `HOME=/root`. No document lists the gaps.

## Premise

Finding B-08 and B-37 of agent B, A24 of agent A.

## Approach

Add `sudo` to the images (a rule for the user of the unit), or document that a step of `sudo` fails; list the tools of each image against ubuntu-latest in `docs/local-mode.md`.

## Prove

`python3 -I scripts/user-gate.py`: a job that runs `sudo true` passes on podvm, or the documents list the gap.

State (2026-11-16): `images/runner/Dockerfile` installs `sudo` with a rule of no password for `runner`, and sets the sandbox user of apt to root (its `_apt` has no uid in a user namespace of one uid). `docs/local-mode.md`, "The runner images", lists the tools of each image and what neither has. The check `confine:podvm` of the user-path gate runs `sudo true`. The proof is the next `runner-image.yml` and user-path gate.

## Done

2026-11-16: the images have `sudo` with a rule of no password for `runner` and apt as root (runner-image 43862847276 green). `docs/local-mode.md`, "The runner images", lists the tools of each image and says where `sudo` works: in a guest of `--microvm` or `--incus`; not in a podvm unit, which maps one uid (on the namespace rung of podvm 0.2.0-beta.27 the user of the job, on the loader family none), so no process there becomes root. Not run: `sudo` in a guest (the guests of a user with no root wait for T-291).


# T-316: The P3 findings of the third QA, agent B

**Source:** the third adversarial QA of the release files of `12a1300` (podvm `0.2.0-beta.24`), agent B, as the user `qa2` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** open

## Problem

B-09 `help check` prints a garbled line. B-10 the usage output is too large. B-11 `run --help` and `docs/cli.md` differ. B-12 the live lines are not in time order. B-13 exit codes do not match `docs/cli.md` (78 for a missing file of a flag, 69 for `--microvm` with no `/dev/kvm`). B-14 refusals that do not name their flag or file, and values of `run.toml` not named as such. B-15 `--image runner:latest` is not refused. B-16 `--incus` prints the hold line of a microvm guest. B-17 `--dry-run` passes a missing `--runner-dir` or `--incus`. B-18 early refusals leave `/tmp/podrunner-run-tree-PID`. B-19 `doctor --run` misses known faults (proxy variables on podvm, no `bin/Runner.Worker`, an unreadable `run.toml` and a fix that loops). B-20 the intermittent `egress: job podrunner-run-fetch ... failed at connect`. B-21 the order of `--list` of a directory. B-22 `--validate` passes `uses:` with no `@REF` and an unknown job key. B-23 a missing action gives a git prompt error. B-24 the fix `--ref` for an event that `on:` does not take. B-25 `github.event.schedule` with `--event schedule`, the PR number of `--event-path`, a bad `--ref` and an empty `--actor`. B-26 the exit codes of `--watch` and `--schedule` at a signal. B-27 `--rerun` and artifacts; the line about `--log-dir DIR`. B-28 the web view lists no artifacts. B-29 `--web` with `--dry-run` waits; a wrong fix for a bind. B-30 `--break-on-failure` writes its pause step twice. B-31 container refusals on `--runner-dir` name the wrong cause and suggest `--podvm`. B-32 `--no-cache` makes `actions/cache` retry for 60 s. B-33 an unwritable `--log-dir` with exit 0. B-34 a corrupt store gives no fix. B-35 the installer takes a runner directory with a missing file. B-36 `--prefix` rewrites the one `run.toml`. B-38 `--github-token-env ghp_abc=1` echoes the value. B-39 control verbs (`stop --wait` with no serve, `drain` output, the mode of `journal.jsonl`, two texts of one error). B-40 `source-conformance -- podrunner fake-source` fails 7 of 12 tests. B-41 `--dry-run --json` gives `success`. B-42 `actions/checkout` with no `--checkout` (T-203). B-43 the lock file of a dry run in the tree. B-44 the cancel of a concurrency group and `if: success()` for a job that `--job` leaves out.

## Premise

The report of agent B (the commands and outputs are in it).

## Approach

Fix each one; a finding that the code cannot change goes into the documents.

Fixed: B-15 (`--image` needs a full digest). B-18 (each return of a run removes its directories in `TMPDIR`).

## Prove

`cargo test -p podrunner -p podrunner-local`

# T-317: On the namespace rung of podvm the job runs as root of a one-uid namespace

**Source:** the lead session, measured during T-301 (2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

With podvm `0.2.0-beta.25`, `run -d` of a user with no root takes the namespace rung: the payload is uid 0 of a user namespace that maps one uid. The runner refuses root (`Must not run interactively with sudo`); tar of `actions/setup-node` fails at each chown (`Cannot change ownership to uid 1001: Invalid argument`); npm 6 runs its scripts as `nobody`, which is not mapped; `--hostname` is refused (no UTS namespace), so the unit has the host name of the host (T-182).

## Premise

Measured by the lead session as `qa1` and `qa2` on the QA host with podvm `0.2.0-beta.25`. podrunner sets `RUNNER_ALLOW_RUNASROOT=1`, `TAR_OPTIONS=--no-same-owner` and `npm_config_unsafe_perm=true` in a podvm unit (T-301), and starts again with no `--hostname`.

## Approach

Ask podvm (a COORD issue) to map the uid of the caller to the user of the image (`runner`, 1001) on the namespace rung, and to give a UTS namespace for `--hostname`; then remove the three variables.

## Prove

`cargo test -p podrunner-podvm`; a job on podvm runs as `runner`, and `hostname` gives `podrunner-u-1`.

2026-11-16, the blocker of the time: COORD-070 (GitHub #196) asks podvm to map the uid of the caller to the user of the image on the namespace rung, and for a UTS namespace there.

State (2026-11-16): podvm answered (PodUtils/podvm#448, COORD-155): `--hostname` of `run -d` is fixed on podvm `main` (its next release); the user of the image waits for the binds of pod-enter.

## Done

2026-11-16: podvm `0.2.0-beta.26` keeps `--hostname` of `run -d` on the namespace rung (PodUtils/podvm@T-1926), and `0.2.0-beta.27` runs the payload as the `USER` of the image there (PodUtils/podvm@T-1927, PodUtils/podvm#448); podrunner pins beta.27. Proven on a constrained host as a user (`qa1`): a job prints `uid=1001(runner)` and the host name `podrunner-u-1`, with no line of the start with no name; the job of T-300 passes and writes no file on the host. The three variables of T-301 stay: `TAR_OPTIONS` for a program that closed descriptor 17 (PodUtils/podvm#450), and the two others do nothing for a user that is not root.

# T-318: A podvm unit shares the processes of the host: a job can kill processes of the user

**Source:** the fourth adversarial QA of the release files of `30b35ec` (podvm `0.2.0-beta.27`), agent B, as the user `qa4` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

On the namespace rung of podvm, a step lists each process of the host with its argv (`ps -eo user,pid,args`: the run of another user, root's processes) and kills a process of the user on the host (`kill -TERM` of a `sleep` started on the host; and `pkill -KILL` of its own podrunner, exit 137). `/proc/PID/root` and `/proc/PID/environ` of host processes cannot be read. `docs/local-mode.md` says the job sees only the image and its work directory.

## Premise

Finding F1 of agent B (round 4).

## Approach

Write a COORD issue to podvm: a PID namespace (and a `/proc` of it) on the namespace rung of `run -d`. Until then the documents say that a podvm unit shares the processes of the host with the user, and name `--microvm`.

## Prove

`python3 -I scripts/user-gate.py`: a check `pids:podvm`: a job does not see a process of the host.

State (2026-11-16): the documents (`docs/local-mode.md`, "What a podvm unit holds", the notes) say that a podvm unit has no process namespace and name `--microvm`. Next: a COORD issue to podvm for a PID namespace on the namespace rung.

History of the blocker:

2026-11-16: COORD-074 (GitHub #200) asks podvm for a PID namespace on the namespace rung. podvm answered (PodUtils/podvm#458, COORD-165): a PID namespace for `run -d` needs pod-enter first (podvm's COORD-164 to podcrates).

2026-11-17: podcrates `v0.65.0` has `Plan::pid_namespace` of pod-enter (PodUtils/podcrates#215, COORD-125). The entry waits for the podvm release that sets it for `run -d`. podvm measured it (a job sees only its unit) but `run -d` does not return with it on; podvm took `v0.65.0` with the option off and waits for COORD-167 (PodUtils/podvm#460). podvm names the beta on PodUtils/podvm#458. 2026-11-17: podcrates `v0.66.0` fixes the init (PodUtils/podcrates#216, COORD-126); podvm took it in `b1fa48f` (PodUtils/podvm#461): a namespace run of `podvm-supervise` has a PID namespace, `run -d` returns, a host pid is not there; PodUtils/podvm@T-1930 is done for its next tag (newest release: `v0.2.0-beta.29`, before the fix). Next: that tag, then the pin.

## Done

2026-11-17: podvm `v0.2.0-beta.30` is pinned (`images/podvm.toml`, the installer). On the namespace rung a unit of `run -d` has a PID namespace and a `/proc` of its own. `doctor --run` names a podvm below `0.2.0-beta.30` on that rung. The user-path gate has the check `pids:podvm` (an xfail off the namespace rung, with the rung). `docs/local-mode.md` and the notes say what each rung holds. Proven on a constrained host as a user (`qa5`, no root, a proxy that intercepts TLS): a step saw 9 processes (pid 1 the init of podvm) and `kill -0` of a `sleep` of the user on the host failed; with podvm `0.2.0-beta.27` the same step saw 98 processes and the signal reached the `sleep`.

# T-319: The egress hold of the local mode lets a job out through a proxy on the loopback

**Source:** the fourth adversarial QA of the release files of `30b35ec` (podvm `0.2.0-beta.27`), agent B, as the user `qa4` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

With `podnet` and `allow` of `run.toml`, a direct connect is refused, but a step reaches any host through the proxy of the host on `127.0.0.1` (`curl -x http://127.0.0.1:PORT ...`), also with a POST of a secret. `docs/local-mode.md` says that a secret cannot leave to another host; the installer says that the hold of podnet is enforced with user namespaces, while the run takes the seccomp hold (advisory, T-302), which does not hold loopback TCP.

## Premise

Finding F2 and F32 of agent B; `crates/podrunner-podvm/src/unit/hold.rs` (the seccomp hold does not hold loopback TCP).

## Approach

The documents and the installer say what the hold of the local mode is: advisory, with each loopback port of the host open (a proxy there is a way out). Then ask podnet for a seccomp hold that refuses loopback ports other than the broker (a COORD issue), or reach the broker of the local mode through the netns hold (T-220: the forward of podnet).

## Prove

`grep -n advisory docs/local-mode.md docs/install.md`; the installer names the hold of the local mode.

State (2026-11-16): the documents (`docs/local-mode.md`, "Where podrunner exceeds them", the notes) and the installer say that the hold of `--podnet` in the local mode is advisory, with each loopback port of the host open; the hold of a guest stays the enforced one. Next: ask podnet for a seccomp hold that refuses loopback ports other than the broker.


The blocker of the time: 2026-11-16: COORD-075 (GitHub #201) asks podnet for a seccomp hold that refuses loopback ports but the broker, or a forward for the netns hold.

## Done

2026-11-16: podnet answered (PodUtils/podnet#605): `--loopback-rules` since `0.4.0-beta.10`. podrunner pins podnet `0.4.0-beta.13`; a held unit of the local mode names the port of its broker (`PODRUNNER_LOOPBACK_ALLOW`, `exec/proxy.rs`), and the podvm driver starts `podnet pod --loopback-rules -r tcp:127.0.0.1:PORT=direct -r cidr:127.0.0.0/8=reject -r cidr:::1/128=reject` (`unit/hold.rs`). Unit tests: `the_rules_of_the_loopback`, `a_held_unit_names_the_port_of_its_broker`. Proven on a constrained host as a user (`qa5`, the proxy of the host on the loopback): `curl -x` of the proxy of the host is refused (exit 7), `github.com` of `--allow` gives 200, and the job ends `ok` (it reached its broker). The documents and the installer say so.


# T-320: Under the egress hold a Node action does not take the CA of the host

**Source:** the fourth adversarial QA of the release files of `30b35ec` (podvm `0.2.0-beta.27`), agent B, as the user `qa4` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

With `podnet` and `allow` on a host whose proxy intercepts TLS, `actions/setup-go` fails: `self-signed certificate in certificate chain`. curl in the same unit works; the job has no `NODE_EXTRA_CA_CERTS`; with no `--podnet` it has it.

## Premise

Finding F4 of agent B.

## Approach

Give a held unit the CA files and their variables (`exec/proxy.rs`, `unit_env`) as an unheld one gets them.

## Prove

`cargo test -p podrunner-local --lib`: a held unit gets the CA variables.

## Done

2026-11-16: a held unit takes the CA files and their variables of the caller, and no proxy variable (`exec/proxy.rs`, `unit_env` with `held`). Unit test: `a_unit_gets_the_proxy_and_each_ca_file_once` (the held case).


# T-321: doctor --run does not check the paths of many flags

**Source:** the fourth adversarial QA of the release files of `30b35ec` (podvm `0.2.0-beta.27`), agent B, as the user `qa4` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`doctor --run` gives exit 0 with no FAIL for `--checkout /nonexist`, `--secret-file /nonexist`, `--env-file /nonexist`, `--log-dir /proc/x`, `--export /proc/x`, `--toolcache /nonexist`, `--state /proc/x` and `--actions /proc/x`; the documents say that it checks each input with its fix.

## Premise

Finding F5 of agent B.

## Approach

Check each path of a flag in `doctor --run` (a directory that exists or can be made and written, a file that can be read), each with its fix.

## Prove

`cargo test -p podrunner`: doctor fails each of those paths by name.

## Done

2026-11-16: `doctor --run` checks `--secret-file`, `--env-file`, `--var-file` and `--input-file` (a file to read), `--checkout` and `--toolcache` (a directory that exists), and `--log-dir`, `--export`, `--state` and `--actions` (a directory that exists and takes files, or can be made), each with its fix. Measured as `nobody`: each bad path gives a FAIL line, and the doctor exits 1.


# T-322: An unwritable --log-dir is found after the run, with exit 0

**Source:** the fourth adversarial QA of the release files of `30b35ec` (podvm `0.2.0-beta.27`), agent B, as the user `qa4` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`--log-dir` in a directory of mode 555 gives `ok build`, then `podrunner: --log-dir: ...: Permission denied` and exit 0: the logs are lost.

## Premise

Finding F6 of agent B.

## Approach

Check `--log-dir` before the first job, as `--export` (T-313); a failed write after the run fails the run.

## Prove

`cargo test -p podrunner`: an unwritable `--log-dir` fails before the first job.

## Done

2026-11-16: `--log-dir` gets the check of `--export` before the first job: made, and a probe file written and removed (`routes::export_dir`); a failure stops the run with the flag, the path and the cause.


# T-323: The P3 findings of the fourth QA, agent B

**Source:** the fourth adversarial QA of the release files of `30b35ec` (podvm `0.2.0-beta.27`), agent B, as the user `qa4` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-16)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** open

## Problem

F7 `check --help` second line. F8 `help --help`. F9 the help links `blob/main`. F10 `--break-on-failure | --keep-failed` and the usage line of 1,500 characters. F11 exit codes of missing files of flags (78, no flag name). F12 Rust error kinds (`entity not found`) reach the user. F13 a file that is not YAML. F14 `TMPDIR` that does not exist or is read-only. F15 `--list` and `--graph` of a directory. F16 `--validate --json` prints text. F17 live lines twice in 1 of 4 runs of a called workflow on `--runner-dir`. F18 `-q` prints the last requests of the broker. F19 ANSI codes to a non-terminal and in the web view. F20 `--allow "*"`, `--toolcache /nonexist` and `--incus` with no incusd pass a dry run. F21 `--image runner@sha256:00`. F22 `run` does not check the parents of the runner directory. F23 `--approve nosuch`; the text of `--environment-secret-file`. F24 `--web` with `--dry-run` waits. F25 dry-run refusals on `--runner-dir` say "not pinned by digest". F26 the container refusal names `doctor --config`. F27 a `paths` refusal names `--ref`. F28 `--export` with `--microvm --runner-dir`. F29 `--action-server` on the loopback is refused (`fixed:loopback`); an unknown action gives a git prompt error. F30 a cancel of a concurrency group. F31 `--watch` exit code and stop line. F33 `--prefix` rewrites the one `run.toml`. F34 `doctor --run` on podvm does not name the proxy variables. F35 local-mode.md says a podvm job runs as root (it runs as `runner`, `HOME=/root`). F36 noise lines under the hold. F37 `ubuntu-22.04` runs on the 24.04 image with no line. F38 an early refusal leaves `podrunner-run-tree-PID`, and the next run says that a run died. F39 after a SIGKILL during `podvm extract`, the cleanup fails at `Directory not empty` (4.3 GB). F40 the shell of `--break-on-failure` does not see the `/tmp` of the job. F41 `skipped test: if: success()` with `--job`.

## Premise

The report of agent B, round 4 (the commands and outputs are in it).

## Approach

Fix each one; a finding that the code cannot change goes into the documents.

Fixed: F23, part 1 (a flag of an environment that no job names gets a line). F7 (`check --help` has its own lines only). F8 (`help --help`). F13 (a file that is not a workflow is named so, before the actions). F14 (a run refuses a TMPDIR that does not take a directory, by name). F16 (`--validate`, `--list` and `--graph` print JSON with `--json`). F21 (`--image` needs a full digest). F11 (a missing file or directory of a flag exits 64 with the flag). F9 (a release links to the documents of its tag). F35 (`docs/local-mode.md` names the user `runner`). F38 (each return of a run removes its directories in `TMPDIR`). Also fixed (2026-11-17): F12 (each error of a file gives the text of the system, not the Rust kind), F15 (a directory gives each workflow its own lines, T-339 item 7), F19 (no ANSI code of the runner on a stderr that is not a terminal, nor in the web view: `logs::plain`), F20 (`--allow "*"` is refused; `--toolcache` is checked by a dry run, T-345), F24 (`--web` and `--dry-run` exclude each other). Open: F10, F17, F18, F20 (`--incus` with no incusd), F22, F23 part 2.

## Prove

`cargo test -p podrunner -p podrunner-local`

# T-324: The base runner image has no make, libatomic1 or python3

**Source:** the fifth adversarial QA of the release files of `a238f8d` (user-gate 44025228142, podvm `0.2.0-beta.30`), agent 1, as the user `qa6` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

On the default (base) image, cobra's tests fail with `make: command not found`, Node 26 of `setup-node` fails at `libatomic.so.1`, and Python tooling has no `python3`. `sudo` does not work in a podvm unit, so a job cannot add them. The larger image (`--full`) passes both.

## Premise

The report of agent 1 of the fifth QA (the commands and outputs are in it).

## Approach

Add `make`, `libatomic1` and `python3` to the base image (`images/runner/Dockerfile`), and list them in `docs/local-mode.md`, "The runner images".

## Prove

A runner-image run; the corpus row of a project that needs `make`.

State (2026-11-17): `make`, `python3` and `libatomic1` are in the base image (`images/runner/Dockerfile`), the step "The tools of the base image in a podvm unit" of `runner-image.yml` checks them, and `docs/local-mode.md` lists them. Next: a green `runner-image.yml` run.

## Done

2026-11-17: `make`, `python3` and `libatomic1` are in the base image; the step "The tools of the base image in a podvm unit" of `runner-image.yml` passed (run 44043008133 on `291f702`, proven in CI on GitHub-hosted runners).

# T-325: A podvm unit needs more scratch disk than doctor says; --jobs 2 fills the disk

**Source:** the fifth adversarial QA of the release files of `a238f8d` (user-gate 44025228142, podvm `0.2.0-beta.30`), agent 1, as the user `qa6` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

The peak size of a unit in `TMPDIR` is about 2.4 GiB on the base image and 3.9 GiB on the full one; `doctor --run` says a run needs about 2 GiB and says `ok` with 7 GiB free for `--jobs 2` on the full image, which then fails with `No space left on device`. The unpack of the image takes 27 to 67 s with no line. The error carries an internal reference of the record of podvm.

## Premise

The report of agent 1 of the fifth QA (the commands and outputs are in it).

## Approach

`doctor --run` gives the need of the image of the run (its size in the store, unpacked) times `--jobs`, and fails below it with the fix. A line names the unpack of the image. The documents say how much disk a unit needs.

## Prove

`cargo test -p podrunner`; `doctor --run` on a small tmpfs as a user.

## Done

2026-11-17: `doctor --run` needs eight times the size of the image in the store (at least 2 GiB), times `--jobs`, and fails below it with the fix; an ENOSPC at the start of a unit names TMPDIR, the need and `--jobs`, with no reference into the record of podvm; a line `unit u-N starts (its image unpacks)` comes before the unpack (not with `-q`); `docs/local-mode.md`, "The disk of a unit". Proven on a constrained host as a user (`qa5`, podvm, 6 GiB free): `--jobs 1` gives `ok ... needs about 2.3 GiB`, `--jobs 4` gives `FAIL ... needs about 9.2 GiB` with the fix.

# T-326: A uv build in a podvm unit on the base image fails at utime of its cache

**Source:** the fifth adversarial QA of the release files of `a238f8d` (user-gate 44025228142, podvm `0.2.0-beta.30`), agent 1, as the user `qa6` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

pallets/markupsafe tests (3.13, uv) on the base image: with no gcc the C extension falls back, then `Cannot update time stamp of directory '/root/.cache/uv/builds-v0/.../MarkupSafe.egg-info'`. The job passes on `--full` and on `--runner-dir`.

## Premise

The report of agent 1 of the fifth QA (the commands and outputs are in it).

## Approach

Find the call that fails (strace in a unit). A path of a raw system call that podvm does not map (the T-261 family) goes to podvm by COORD; the notes list the case with its work-around.

## Prove

`podrunner run .github/workflows/tests.yaml --job tests --matrix python=3.13` of pallets/markupsafe on the base image as a user.

## Done

2026-11-17: the cause is not podvm. With no gcc, markupsafe's `setup.py` builds again without the C extension in the same process; distutils' cache of made directories (`mkpath`) skips the `egg-info` directory of the new temporary tree, so `os.utime` gets ENOENT. Measured as `qa5` with a wrapper of `os.utime` (`errno=2`, the directory is not there); the unit was on the namespace rung (9 mounts, no volume). The base image keeps no compiler (the call in `docs/decisions.md`); `docs/local-mode.md` and the notes name the case and `--full`, with which the job passes (the fifth QA).

# T-327: A run pulls the images of services: before it refuses the job

**Source:** the fifth adversarial QA of the release files of `a238f8d` (user-gate 44025228142, podvm `0.2.0-beta.30`), agent 1, as the user `qa6` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

On podvm with no container hooks, and on `--microvm` with no `/dev/kvm`, a real run first pulls `postgres:14` and `mysql:5.7` (290 MiB), then a 429 of the registry ends the run. The dry run already knows that the job cannot run there.

## Premise

The report of agent 1 of the fifth QA (the commands and outputs are in it).

## Approach

Refuse each job that the host cannot run (no hooks; no KVM) before the pull of its images, and check `/dev/kvm` before any pull. A failed pull fails only the jobs that need that image.

## Prove

`cargo test -p podrunner`; the run as a user with `services:` and no KVM.

## Done

2026-11-17: the refusals of T-330 come before the pin: a run whose executor refuses containers pulls no image, and podvm leaves the images of `services:` out of its pull; `--microvm` checks `/dev/kvm` before the pin (`guest::early`). Proven on a constrained host as a user (`qa5`): a job with `services: postgres:14` on podvm was refused with no pull. The rest (a failed pull fails only the jobs of that image) is in T-331.

# T-328: Under the egress hold, a refused host is not named with its fix

**Source:** the fifth adversarial QA of the release files of `a238f8d` (user-gate 44025228142, podvm `0.2.0-beta.30`), agent 1, as the user `qa6` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

With `allow = ["github.com"]`, a job fails at `ECONNREFUSED`; the refusal `nodejs.org:443 failed at proxy (policy)` is one of eleven egress lines, and the run ends with `a job failed` only.

## Premise

The report of agent 1 of the fifth QA (the commands and outputs are in it).

## Approach

At the end of the run, name each host that the hold refused for a failed job, with the fix (`--allow HOST`, or `allow` in run.toml).

## Prove

`cargo test -p podrunner`; the run as a user under `--podnet`.

## Done

2026-11-17: a failed job names the hosts that the egress hold refused, once, on stderr, with the fix (`--allow HOST`, or `allow` in run.toml); the job log keeps each denial (`exec/proxy.rs`, `denials`). Proven on a constrained host as a user (`qa5`, `--podnet`): a job that fetched `https://nodejs.org` failed with `podrunner: n.0: the egress hold refused ..., nodejs.org; if the job needs one, allow it: --allow HOST, or `allow` in run.toml`.

# T-329: Each unit under --podnet logs connects that the job did not make

**Source:** the fifth adversarial QA of the release files of `a238f8d` (user-gate 44025228142, podvm `0.2.0-beta.30`), agent 1, as the user `qa6` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

Before the first step of `hello.yml` under `--podnet`: `null:null failed at proxy (usage)`, and two refusals each of `help.ubuntu.com`, `archive.ubuntu.com` and `security.ubuntu.com`. A user reads them as a unit that calls home.

## Premise

The report of agent 1 of the fifth QA (the commands and outputs are in it).

## Approach

Find the process that makes the connects (the image, the runner or podnet). Stop it, or keep the line out of the output of a run.

## Prove

The hello run under `--podnet` as a user: no egress line.

## Done

2026-11-17: the local mode writes no line on stderr for each failed connection (`Egress::quiet`); T-328 names the refused hosts of a failed job once. The connects come from podvm itself: `podvm run` of the image with `/bin/true` under `podnet pod` makes them (COORD-076, GitHub #202); their hosts in the list of a failed job are T-332. Proven on a constrained host as a user (`qa5`, `--podnet`): `hello.yml` prints no egress line and ends `ok`.

# T-330: The refusals of services: send the user in a circle

**Source:** the fifth adversarial QA of the release files of `a238f8d` (user-gate 44025228142, podvm `0.2.0-beta.30`), agent 1, as the user `qa6` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

On `--runner-dir` a job with `services:` is first refused for a tag (`not pinned by digest`); with a digest, it needs a unit; on `--podvm` with no hooks, `--microvm --engine`, which the host cannot run. The refusal names `doctor --config`, the file of `serve`.

## Premise

The report of agent 1 of the fifth QA (the commands and outputs are in it).

## Approach

One refusal that says what the host needs for `services:` and names `podrunner doctor --run`; the kind of the job comes before the pin of its image.

## Prove

`cargo test -p podrunner`.

## Done

2026-11-17: one refusal for each executor that cannot run containers (`run_local/containers.rs`): a runner directory, podvm with no hooks, `services:` on podvm, a guest with no `--engine`, `--incus`. It comes before the pin of the image, names `podrunner doctor --run` and what the host needs; the dry run and the run give the same text. Proven on a constrained host as a user (`qa5`, podvm with no hooks, and a runner directory in a dry run): `failure db: the job needs containers, and podvm gives no container hooks on this host (...)`, and the other job ran.

# T-331: The P3 findings of the fifth QA, agent 1

**Source:** the fifth adversarial QA of the release files of `a238f8d` (user-gate 44025228142, podvm `0.2.0-beta.30`), agent 1, as the user `qa6` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** open

## Problem

F8 a `failed at connect (unavailable)` line where the fallback to the proxy works. F9 a missing `ci.yml` of the quick start gives `No such file or directory`, not the list of workflows; a folded `name: >` gives a blank line after `planned`; `--event nope` prints the usage; `--validate` points at the job, not at `needs:`; the runner console on `--runner-dir` prints `Connected to GitHub`. F10 `~/podrunner-work` stays after a run; a socket of podnet stayed after a `kill -9`. From T-327: a failed pull of an image (a 429 of the registry) ends the run, also for the jobs that need no image.

## Premise

The report of agent 1 of the fifth QA (the commands and outputs are in it).

## Approach

Fix each one; a finding that the code cannot change goes into the documents.

Fixed (2026-11-17): F9, a missing workflow file names the workflows of its directory (`inspect::missing`); F9, a folded `name: >` is one line (`line` of `run_local.rs`); F8, no `failed at connect` line where the fallback to the proxy works (T-329, `Egress::quiet`). Also fixed: F9, a wrong value of a flag (`--event nope`) names its flag with no usage; the usage comes with a wrong form of the line only (`args::with_usage`); F9, the console of `--runner-dir` says `Connected to the fake broker of podrunner (not GitHub)` (`runner::console_line`); F10, a work directory out of the runner directory (`BASE/podrunner-work`) goes with the run. Fixed (2026-11-18): F9, `--validate` points at the need that names no job (`jobs.b.needs (line 9, column 16)`), with podcrates `v0.70.0` (PodUtils/podcrates@T-859, GitHub #214); measured with the built binary. Open: F10, a socket of podnet after a `kill -9` of podrunner (the socket is podnet's); a failed pull ends the run, also for the jobs that need no image (T-327).

## Prove

`cargo test -p podrunner -p podrunner-local`

# T-332: podvm connects to the hosts of the image at the start of a unit

**Source:** T-329 (the fifth QA, agent 1), measured by the lead session as `qa5` with podvm `0.2.0-beta.30` (2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`podvm run` of the runner image with a payload of `/bin/true`, under `podnet pod`, makes an empty request and CONNECTs to `help.ubuntu.com:443`, `archive.ubuntu.com:443` and `security.ubuntu.com:443` (IPv4 and IPv6). Under `--podnet` the hold refuses them, and T-328 lists them among the refused hosts of a failed job, before the host of the job.

## Premise

The measurement of T-329 (the listener on the egress socket logs each CONNECT).

## Approach

podvm stops the connects or gives a flag that turns them off (COORD-076, GitHub #202); podrunner passes that flag.

## Prove

`podrunner run hello.yml --podvm PATH --podnet PATH` as a user: no refused host in the egress of the unit.

History of the blocker:

2026-11-17: COORD-076 (GitHub #202) asks podvm. 2026-11-17: podvm answered (PodUtils/podvm#464, COORD-171): the connects are the source fixup of podvm; `--no-source-fixup` turns it off.

## Done

2026-11-17: each `run -d` and `exec` of a podvm unit passes `--no-source-fixup` to a podvm of `0.2.0-beta.30` or later (`labels::fixup_args`; the version of each binary is probed once). Proven on a constrained host as a user (`qa5`, `--podnet`, podvm `0.2.0-beta.30`): `podnet pod -v` shows no connect through the egress socket for `hello.yml` (it showed six before), and the run ends `ok`.

# T-333: A dry run with --state writes success for each job; --dry-run --json says success

**Source:** the fifth adversarial QA of the release files of `a238f8d` (user-gate 44025228142, podvm `0.2.0-beta.30`), agent 2, as the user `qa7` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`podrunner run ci.yml --rerun DIR --dry-run` writes `success` for `test.1`, which was `failure`, into `DIR/state.json`; a later `--rerun --failed` keeps the job as `success`. `--state DIR --dry-run` writes `success` for each planned cell. `--dry-run --json` gives `"result":"success"` for a planned job.

## Premise

The report of agent 2 of the fifth QA (the commands and outputs are in it).

## Approach

A dry run writes no state; its JSON gives `planned`.

## Prove

`cargo test -p podrunner`

## Done

2026-11-17: a dry run writes no `state.json` (with `--state` or `--rerun`), and its JSON gives `"result":"planned"` for a job that would start. Proven on a constrained host as a user (`qa5`): `--dry-run --json --state DIR` printed `planned` and DIR holds no `state.json`; a test holds it.

# T-334: --artifacts DIR or --state DIR of an earlier run makes each upload of the same name fail with 409

**Source:** the fifth adversarial QA of the release files of `a238f8d` (user-gate 44025228142, podvm `0.2.0-beta.30`), agent 2, as the user `qa7` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The second `podrunner run art.yml --artifacts DIR` (or `--state DIR` with no `--rerun`) fails each `upload-artifact@v4` of a name of the first run: `(409) Conflict: an artifact with the name "dup" already exists on the workflow run`.

## Premise

The report of agent 2 of the fifth QA (the commands and outputs are in it).

## Approach

A new run (no `--rerun`) replaces the artifacts of a name that it uploads again, as a rerun does.

## Prove

`cargo test -p podrunner -p podrunner-local`

## Done

2026-11-17: a new run (no `--rerun`) removes the artifacts of an earlier run from its artifact directory first (`run_local/artifact_dir.rs`), by name on stderr; other files stay. Proven on a constrained host as a user (`qa5`, podvm): two runs of an upload of `dup` into one `--artifacts DIR` both end `ok`; the second says `the artifacts of an earlier run go: dup`.

# T-335: --state DIR with --artifacts DIR2 keeps no artifact, and --rerun fails

**Source:** the fifth adversarial QA of the release files of `a238f8d` (user-gate 44025228142, podvm `0.2.0-beta.30`), agent 2, as the user `qa7` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`podrunner run ci.yml --artifacts ~/arts --state ~/st1`: `st1` has only `state.json`; `--rerun ~/st1 --failed` fails at `Artifact not found for name: art1`.

## Premise

The report of agent 2 of the fifth QA (the commands and outputs are in it).

## Approach

The state names the artifact directory of its run, and a rerun reads it; or `--state` with `--artifacts` is refused with the fix.

## Prove

`cargo test -p podrunner`

## Done

2026-11-17: `--state DIR` with `--artifacts DIR2` writes `DIR/artifacts-dir`, and `--rerun DIR` reads DIR2 from it. Proven on a constrained host as a user (`qa5`, podvm): a run with `--artifacts` and `--state` whose second job failed, then `--rerun --failed`: `kept up`, `ok down` (it downloaded the artifact).

# T-336: actions/checkout outside a git tree with no --checkout fails with a curl 404

**Source:** the fifth adversarial QA of the release files of `a238f8d` (user-gate 44025228142, podvm `0.2.0-beta.30`), agent 2, as the user `qa7` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

In a directory with no `.git`, a workflow with `actions/checkout@v4` and no `--checkout` fails in the job: `curl: (22) The requested URL returned error: 404`, `tar: This does not look like a tar archive`. The dry run says `planned`.

## Premise

The report of agent 2 of the fifth QA (the commands and outputs are in it).

## Approach

With no `--checkout`, a workflow that uses `actions/checkout` takes the current directory also out of a git tree (each file), with the line of T-203.

## Prove

`cargo test -p podrunner`; the run as a user in a plain directory.

## Done

2026-11-17: out of a git tree the default checkout takes the project whose `.github/workflows` holds the file; with neither, the run is refused before the first job: `the workflow uses actions/checkout, and DIR is in no git tree: give --checkout DIR, the tree of the project` (exit 64). Proven on a constrained host as a user (`qa5`): a plain project gets its tree; a file in no project is refused.

# T-337: From a subdirectory of a git tree, the checkout holds only that subdirectory

**Source:** the fifth adversarial QA of the release files of `a238f8d` (user-gate 44025228142, podvm `0.2.0-beta.30`), agent 2, as the user `qa7` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`cd ~/proj/.github && podrunner run workflows/co.yml`: `it gets the tree of /home/qa7/proj/.github (--checkout .)`; in the job `git status` shows ` D README.md`.

## Premise

The report of agent 2 of the fifth QA (the commands and outputs are in it).

## Approach

The default checkout of T-203 takes the top of the git tree (`git rev-parse --show-toplevel`), as on GitHub.

## Prove

`cargo test -p podrunner`; the run as a user from a subdirectory.

## Done

2026-11-17: the default checkout takes `git rev-parse --show-toplevel` of the current directory (`args::default_checkout`). Proven on a constrained host as a user (`qa5`): from `PROJECT/.github`, `it gets the tree of PROJECT`.

# T-338: --no-cache makes each actions/cache step retry for about a minute

**Source:** the fifth adversarial QA of the release files of `a238f8d` (user-gate 44025228142, podvm `0.2.0-beta.30`), agent 2, as the user `qa7` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`podrunner run cache.yml --no-cache`: `Attempt 1 of 5 ... Unexpected end of JSON input` about 8 times, `Failed to restore`, `Failed to save`, ten lines of `no route of the fake broker: ...CacheService/...`; the job takes 95 s (30 s with the cache).

## Premise

The report of agent 2 of the fifth QA (the commands and outputs are in it).

## Approach

With `--no-cache`, the broker answers the CacheService as a miss (no entry; a save is refused at once), with no retry and no line of a missing route.

## Prove

`cargo test -p podrunner-broker-fake`; the run as a user.

## Done

2026-11-17: with `--no-cache` the broker answers the CacheService (`cache::off`): each lookup is a miss and each save is refused with `the cache of this run is off (--no-cache)`; no retry, no line of a missing route. Proven on a constrained host as a user (`qa5`, podvm): an `actions/cache@v4` job ends `ok` in 27 s with `Cache not found` and the refusal, and no `Attempt` line.

# T-339: The P3 findings of the fifth QA, agent 2

**Source:** the fifth adversarial QA of the release files of `a238f8d` (user-gate 44025228142, podvm `0.2.0-beta.30`), agent 2, as the user `qa7` with no root behind a proxy that intercepts TLS, no KVM (T-246, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** open

## Problem

1 `--jobs N` and the disk of a unit (T-325). 2 a cancel of queued cells has no matrix label; jobs that `--job` left out get `cancelled`. 3 a signal during the start of a unit gives `failure`. 4 a refusal of `on:` names `--ref` for an event that the workflow does not take; a refusal of `paths` or `types` does not name its filter. 5 errors with no flag or file: `--podvm /nonexist`, `--artifacts`, `--cache`, `--env =x`, `--var novalue`, `--event-path` of bad JSON, a bad state, `quiet = "yes"` of run.toml, `--incus /nonexist`, `--state /etc`. 6 the dry run and `doctor --run` miss checks of the run (`--log-dir`, `--export`, `--toolcache`, `--incus`, `.secrets`, no executor; `--cache`, `--artifacts`; the lock of `/nonexist.yml`; `--json`). 7 `--list` of a directory mixes the files; `--json` gets `--- NAME` lines. 8 `--validate --watch` waits; `--list --web`, `--list --rerun` are taken. 9 `.actrc` and a project `run.toml` treat a host flag in two ways; the warnings do not name `trusted` and `--platform`. 10 `.secrets` of the current directory with no checkout gets no line. 11 a committed lock with local edits is left out of the checkout (` D` in the job). 12 the egress lines (T-329, T-331). 13 `a job failed; its log is above` with `--json` or with no job; a cancel of a concurrency group says a signal; SIGINT while `--watch` waits says a job ends; the console of `--runner-dir`. 14 `--web localhost:0` is refused. 15 a missing repository of an action gives a git prompt error. 16 `--validate` passes `timeout-minutes: abc`. 17 `--podvm X --runner-dir Y` says `--podvm needs --images STORE`. 18 `version --x` exits 0; the usage of `run` is one line of 2 KB. 19 `print-unit --config /nonexist` exits 0; `serve` makes DIR before it fails; `audit verify --file /etc/hostname`; `entity not found` against `No such file or directory`. 20 `cli.md` and `run --help` differ (`--toolcache`, `--podnet`).

## Premise

The report of agent 2 of the fifth QA (the commands and outputs are in it).

## Approach

Fix each one; a finding that the code cannot change goes into the documents.

Fixed (2026-11-17): 5, `--env`, `--var` and `--input` name their flag in a bad pair; a missing binary of `--podvm`, `--microvm` and `--incus` is named with its flag at the build of its executor; `--artifacts`, `--cache`, `--event-path` (bad JSON) and `--rerun` (a bad state) name their flag; 7, a directory gives each workflow its own lines in order, and with `--json` each head is `{"event":"workflow","file":...}` (`many::head`); 16, `--validate` reads each number of minutes of a job and of a step (`inspect::minutes`); 13, with `--json` no text line ends a failed run, and a run whose jobs failed before they ran says so (`failed_end`); 4, a refusal of `on:` names `--event` for an event that `on:` does not name, and `--ref`, `--changed-since` and `--force-event` for one that a filter refuses (`context::refused_event`); 14, `--web localhost:PORT`; 18, `version` refuses an option (exit 64); 19, `print-unit` refuses a `--config` that is not there (exit 66); 12, the egress lines (T-328, T-329, T-332). Also fixed (2026-11-17): 6 (`dry::paths`, T-345; the lock of a missing file is item 6 of T-345 too), 8 (T-347), 10 (`.secrets` of the current directory, T-341), 17 (two executors are named before the store), 19 (`serve` reads its configuration before it makes DIR; `audit verify` says `not an audit file`; the errors of a file give the text of the system, not `entity not found`), 20 (`cli.md` names `--toolcache`; `--podnet` was there). Also fixed: 11 (a committed lock stays in the checkout, at its state on disk), 15 (a login prompt of git names a repository that is not there or is private). Also fixed: 2 (at a stop each open cell is cancelled by its name with its matrix; a job that `--job` keeps out is `not selected`). Also fixed: 3 (an error of a job after a signal is `cancelled`, not a failure; not run in a test, as the signal flag is global), 9 (a host flag of a project `.actrc` stays a note, not a failure, as act shares the file; the note names `trusted`, and `-P` names `--platform`: the call of the session). Open: 1 (T-325 gives the need of `--jobs N` in `doctor --run`; the run does not check it).

## Prove

`cargo test -p podrunner -p podrunner-local`

# T-340: A workflow file whose symbolic link leads out of the project is followed

**Source:** GitHub #203 (a user QA of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A workflow file under `.github/workflows` that is a symbolic link to a file out of the project is read, planned and run, in file mode and in directory mode. The default checkout then becomes the project of the target. The documents say that a project file whose link leads out of the project fails the run.

## Premise

The report of #203 (the commands and outputs are in it).

## Approach

Resolve each workflow file (given or found in a directory) with the project rule of `.podrunner/run.toml` and `.actrc`. Refuse a link out of the project with the path and its target. The default checkout comes from the given path, not from the target.

## Prove

`cargo test -p podrunner`; A test: a link to a valid workflow out of the project is refused in file mode and in directory mode (also `--validate`, `--list`).

## Done

2026-11-17: a workflow file (given, or of a directory) that is a symbolic link out of its project is refused with the link and its target (`args::link_out`); the project of a link is that of the link, not of its target. Proven in CI on GitHub-hosted runners by `qa6_tests::a_workflow_link_out_of_the_project_is_refused`.

# T-341: The `.secrets` file of the tree skips the checks of `--secret-file`

**Source:** GitHub #204 (a user QA of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The `.secrets` of the tree is read with no check of mode, kind, format or link at plan time. The same file as `--secret-file` is refused.

## Premise

The report of #204 (the commands and outputs are in it).

## Approach

Give `.secrets` the checks of `--secret-file` (a regular file, mode 0600 or narrower, `NAME=VALUE` or `NAME<<DELIM`, the project link rule), in the dry run too. The error names `.secrets` and the line.

## Prove

`cargo test -p podrunner`; Tests: mode 0644, a bad line, a directory and a link out of the project each fail a dry run with the name of `.secrets`.

## Done

2026-11-17: `.secrets` of the checkout, else of the current directory, is read with each check of `--secret-file` (a file, mode 0600, the format, the link rule), also in a dry run. Proven in CI on GitHub-hosted runners by `qa6_tests::the_secrets_of_the_tree_are_checked_in_a_dry_run`.

# T-342: `doctor --run --json` gives no JSON; `doctor --json --run` is an unknown option

**Source:** GitHub #205 (a user QA of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`doctor --run FILE --json` writes text rows only. With `--json` before `--run`, `--run` is an unknown option.

## Premise

The report of #205 (the commands and outputs are in it).

## Approach

Parse the options of `doctor` in any order. With `--run`, `--json` gives one object with the rows and the verdict.

## Prove

`cargo test -p podrunner`; Tests of both orders; the JSON form parses.

## Done

2026-11-17: `doctor --run` takes `--run` after another option, and with `--json` writes one object `{"ok","checks":[{"name","ok","text"}]}`, also for a refusal of its options (`doctor::view`). Proven in CI on GitHub-hosted runners by `tests/doctor.rs` (`doctor_run_checks_the_inputs_of_a_local_run`).

# T-343: `print-unit` writes values in `ExecStart` with no quotes, and takes a relative or empty `--binary`

**Source:** GitHub #206 (a user QA of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

A path with a space splits the command of systemd. A relative or empty `--binary` gives a bad `ExecStart`. (The missing `--config` is fixed by T-339 item 19.)

## Premise

The report of #206 (the commands and outputs are in it).

## Approach

Quote each value as systemd needs (double quotes, escape `\\`, `"`, `%` and `$`); refuse a value with a control character. Refuse a relative or empty `--binary`.

## Prove

`cargo test -p podrunner`; Tests: a path with a space is quoted; `--binary ./x` and `--binary ''` exit 64.

## Done

2026-11-17: `print-unit` writes each value as one word of `ExecStart` (double quotes for a space or a quote; `%%`, `$$`), refuses a control character, and refuses a relative or empty `--binary` (exit 64); a missing `--config` was fixed by T-339. Proven in CI on GitHub-hosted runners by `cli::tests::print_unit_quotes_each_value`.

# T-344: `image verify` refuses the runner image of the release, and the error does not name the file it takes

**Source:** GitHub #207 (a user QA of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`image verify --tarball runner-x86_64.tar` fails with a hash mismatch. The command checks the official runner tarball only, and neither the help nor the error says so.

## Premise

The report of #207 (the commands and outputs are in it).

## Approach

The help and the mismatch error name the file that the command takes (`actions-runner-linux-ARCH-VERSION.tar.gz`) and the pin. The error of an image tarball (an `index.json` in it) says that `sha256sum -c` with the `.sha256` file of the release checks the image.

## Prove

`cargo test -p podrunner`; A test of the error text for an OCI tarball and for a bad runner tarball.

## Done

2026-11-17: the error of `image verify` names the file of the pin (`actions-runner-linux-ARCH-VERSION.tar.gz`), the SHA-256 of the pin and of the file, and says when the file is a runner image (an OCI layout) with the way to check it; the help and `cli.md` name the file. Proven in CI on GitHub-hosted runners by `image::tests::a_tarball_with_another_digest_is_refused`.

# T-345: A dry run writes the artifact and cache directories, and skips the checks of some executor paths

**Source:** GitHub #208 (a user QA of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

A dry run with `--state DIR` makes `DIR/artifacts`, and the default cache makes its directories. `--runner-dir` of an empty directory or of a path with a space, `--actions /nope`, `--incus /nope` and `--podnet /nope` give `planned`, but the run fails.

## Premise

The report of #208 (the commands and outputs are in it).

## Approach

A dry run makes no directory. It checks each path that the run will use, as the start of the run does: the runner directory, `--actions`, `--incus`, `--podnet`.

## Prove

`cargo test -p podrunner`; Tests: a dry run leaves no new directory; each bad path fails the dry run with its flag.

## Done

2026-11-17: a dry run keeps its artifact store in the scratch and opens no cache, so `--state` and the cache get no directory; it checks the runner directory (its files, a space), the binaries of `--microvm`, `--incus`, `--podnet` and `--vnet`, `--toolcache`, and that `--actions`, `--artifacts`, `--cache`, `--log-dir` and `--export` take files, each by its flag (`dry::paths`). A dry run that resolves a new action still writes the lock and the store of actions, as it pins. Proven in CI on GitHub-hosted runners by `flags_tests::a_dry_run_writes_no_state`.

# T-346: Nested `uses:` of a local composite action are not pinned; the refusal says offline mode with no `--offline`

**Source:** GitHub #209 (a user QA of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

A dry run, also with `--offline`, plans a job whose local composite action uses an action that is not in the lock or the store. For a remote composite, the refusal says "the offline mode refuses a change" with no `--offline`, and it does not give the fix.

## Premise

The report of #209 (the commands and outputs are in it).

## Approach

Read each local composite action (`./PATH/action.yml`) of each job, and pin its `uses:` as a direct `uses:`, recursively, with a depth limit. Word the refusal from the real cause (`--offline`, `--frozen`, or no fetch), and name the fix (`--local-repository NAME@REF=DIR`, or a run with no `--offline`).

## Prove

`cargo test -p podrunner`; Tests: a nested reference of a local composite is in the lock after a dry run; with `--offline` and no pin, the dry run fails; the refusal text names the cause.

## Done

2026-11-17: each remote `uses:` of a local composite action (`./PATH` of the checkout, else of the current directory, to 10 levels) is pinned with the direct `uses:` (`composites::local_uses`). A run whose references are each a `--local-repository` resolves offline first, then in the mode of the run for a reference that a composite names; a refusal of `--offline` or `--frozen` names the fix (`actions::with_fix`). Proven in CI on GitHub-hosted runners by `composites::tests` and `qa6_tests::an_offline_dry_run_pins_the_uses_of_a_local_composite`; the fall-back to a fetch is not run.

# T-347: `--watch` and `--schedule` with a one-shot mode wait for ever; `--schedule` of a workflow with no `on.schedule` gives a bad message

**Source:** GitHub #210 (a user QA of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`--watch --list` and `--schedule --dry-run` wait. A workflow with no `on.schedule` gets `on.schedule has no cron time`. `--watch` writes no line while it waits.

## Premise

The report of #210 (the commands and outputs are in it).

## Approach

Refuse `--watch` and `--schedule` with `--list`, `--graph`, `--validate` and `--dry-run` (exit 64). Say `the workflow has no on.schedule trigger` when there is none. Write one line `watching FILES for a change` on stderr when a watch waits.

## Prove

`cargo test -p podrunner`; Tests of each pair and of the message.

## Done

2026-11-17: `--watch` and `--schedule` refuse `--list`, `--graph`, `--validate` and `--dry-run` (exit 64); `--schedule` of a workflow with no `on.schedule` says `the workflow has no on.schedule trigger`; an idle watch writes `watching TREE and FILE for a change`. Proven in CI on GitHub-hosted runners by `qa6_tests::a_loop_refuses_a_view_and_a_workflow_with_no_schedule`; the line of an idle watch is not run in a test.

# T-348: `--validate --json` gives no JSON for a workflow that is not valid

**Source:** GitHub #211 (a user QA of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

For a bad workflow, `--validate --json` writes nothing on stdout.

## Premise

The report of #211 (the commands and outputs are in it).

## Approach

Write `{"file":...,"valid":false,"warnings":[],"error":"..."}` on stdout, and keep the line on stderr.

## Prove

`cargo test -p podrunner`; A test: a bad YAML file gives the object and exit 78.

## Done

2026-11-17: `--validate --json` writes `{"file","valid":false,"warnings":[],"error"}` on stdout for a file that is not valid, and the line on stderr. Proven in CI on GitHub-hosted runners by `qa6_tests::validate_json_names_a_file_that_is_not_valid`.

# T-349: A failure before the start of a job names a log in `--log-dir` that is not there

**Source:** GitHub #212 (a user QA of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

When a job fails before the runner starts, no log is written, but the end line says `its log is above (and in --log-dir DIR)`.

## Premise

The report of #212 (the commands and outputs are in it).

## Approach

Write `DIR/JOB/job.log` with the failure line when a job fails before it starts. The end line names `--log-dir` only when a log file is there.

## Prove

`cargo test -p podrunner`; A test: a failure before the start writes `job.log` with the cause.

## Done

2026-11-17: a job that failed before it ran gets its log of the run (the cause) as `DIR/JOB/job.log` in `--log-dir` (or the logs of `--state`); the end line names the directory of the logs only when the run has one (`logs::write_dir`, `logs::failed_end`). Proven in CI on GitHub-hosted runners by `qa6_tests::a_failure_before_the_start_has_its_log_in_the_log_dir`.

# T-350: beta.7 pins podnet `0.4.0-beta.13`; the vnet forward of #170 is in `0.4.0-beta.14`

**Source:** GitHub #213 (a user QA of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** partial

## Problem

The forward of the broker into a unit (#170, COORD-089, the answer PodUtils/podnet#582: the IPv4 name of a mapped address and `PODNET_VNET_FORWARD`) needs podnet `0.4.0-beta.14` or later. The release pins beta.13. With beta.15 and `PODNET_VNET_FORWARD` to the inbound socket of the hold, the broker closes the connection: the forward needs a listener of the broker on a unix socket (#170 step 1).

## Premise

The report of #213 (the commands and outputs are in it).

## Approach

Pin podnet `0.4.0-beta.15` (`images/podnet.toml`) after the gate passes with it. Then do #170 step 1 (the broker listens on a unix socket too) and pass `PODNET_VNET_FORWARD` to the hold (T-220).

## Prove

`cargo test -p podrunner`; The gate and the user-gate with podnet beta.15; a podvm job in `podnet pod --backend seccomp` connects to the broker on a host that refuses a bind of the loopback.

Pinned (2026-11-17): podnet `0.4.0-beta.15` in `images/podnet.toml` and `scripts/install.sh`; its Breaking items (the ports of `netns-kernel`, the rules of `warp proxy`) do not touch the seccomp hold. Open: the gate and the user-gate with it; the listener of the broker on a unix socket and `PODNET_VNET_FORWARD` (#170 step 1, T-220).

# T-351: The egress module resolves each name itself, so a host with DNS only through its proxy gets 502 for each fetch and each allowed host

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding F1; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** partial

## Problem

On a host whose only resolver is the proxy, `git ls-remote` of each action gets `502 ... system resolver: failed to lookup address information: Try again; DNS over HTTPS: not asked for this name`. The 502 comes from podrunner's fetch relay, not from the proxy. The same `git ls-remote` from the shell through the same proxy works 3 of 3 times. Each workflow with an action outside the built-in set fails with exit 78. The error does not name `--local-repository`.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`. Other project: the root fix is in podcrates (`pod-broker`).

## Approach

(1) Root fix in podcrates (`pod-broker`): with `Settings::names_upstream`, when `resolve_once` fails with `NotFound`, `Unavailable` or `Timeout`, check the name against each deny, then send the name to the upstream. Write an audit line `unresolved`. Keep the hold `advisory` unless the canary says `refuses-non-public` (as `names.rs` says now). Send a COORD issue `(to podcrates)` and set the entry `blocked` on it. (2) Interim fix in podrunner, `crates/podrunner-local/src/fetch.rs`: when the environment names a proxy for the host and the resolver of the module has no address for it, give git the relay of a plain CONNECT forward to the proxy (one host, the same `GIT_CONFIG_*` rules), or give the module a resolver that answers with a placeholder address for the one allowed host so that the name goes up. (3) The error of a failed fetch (`run_local/actions.rs`) names the cause ("the resolver of this host has no address for HOST") and the workaround `--local-repository OWNER/REPO@REF=DIR`. (4) `doctor` reports "the system resolver has no address for github.com; names go through the proxy". The ask to podcrates is COORD-078 (GitHub #215).

## Prove

`cargo test -p podrunner-egress names_upstream_with_no_resolver` (a fake resolver that fails, a fake CONNECT proxy that takes names: the CONNECT gets 200). A new user-gate lane `proxy-only-dns` (see "Gaps of the gate"): the corpus fetches each action.

In the code (2026-11-17): step 3: a fetch whose way out fails (`CONNECT tunnel failed`) names the egress module, the proxy of the environment, `podrunner doctor` and `--local-repository OWNER/REPO@REF=DIR` (`actions::with_fix`); measured here with a dead proxy. Open: the root fix (COORD-078, #215), the lane of T-371, and the line of `doctor`.

In the code (2026-11-17): step 2, the fix in podrunner: an egress module with a proxy of the environment (the fetch of actions, `--podnet`, `--vnet`, a guest) takes `dns::resolver_via(proxy)`: the system resolver first, then DNS over HTTPS through that proxy, with the CA bundle of the environment added to the roots. Proven on a constrained host as a user: on this sandbox, a user with no resolver (port 53 refused by the firewall, `getent hosts github.com` fails), behind a proxy that intercepts TLS, gave the 502 before the fix and pins `actions/upload-artifact@v4` after it (`run --dry-run`). Not proven: a proxy that asks for a login (the lane `proxy-only-dns`, T-371), and a proxy that refuses a CONNECT to the address of a DoH server. The ask of #215 stays: a broker that sends a name it cannot resolve.

In the code (2026-11-18): step 1, the root fix: podcrates `v0.70.0` (PodUtils/podcrates@T-862) sends an allowed name with no local address to the upstream in the names mode, and podrunner takes the tag. Test `names_upstream_with_no_resolver` (`podrunner-egress`): a resolver with no address for each name, an upstream that takes names: the allowed name gets a tunnel, a denied name gets 403. Open: the lane of T-371 and the line of `doctor`.

# T-352: `image verify` cannot verify the runner image of the release

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding F2; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

`podrunner image verify --tarball runner-x86_64.tar --arch x86_64` exits 78 for the asset of the same release, whose bytes match the published `.sha256`. The command only checks the tarball of `actions/runner`. A user has no podrunner command that checks the image that the installer loads.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`.

## Approach

In `crates/podrunner/src/image.rs`, when `is_image` is true, read `index.json` of the layout and compare its manifest digest with the image digest that the release pins (the digest that the installer writes into `run.toml` as `image = "runner@sha256:..."`). Build that digest into the binary at release time (`scripts/release-assets.sh` gives it as an environment value of the build), or take `--image NAME@sha256:HEX`. Say `the runner image of release VERSION: the digest matches`.

## Prove

`image::tests::an_image_of_the_release_is_verified` (a small OCI layout with a known digest); a user-gate step runs `image verify` on the real asset.

# T-353: The runner image has no `less`

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding F3; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** partial

## Problem

`command -v less` is empty in a podvm unit. `ubuntu-latest` has `less`. Suites that test a pager (click `test_echo_via_pager.py`) fail.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`.

## Approach

Add `less` to the base package list in `images/runner/Dockerfile` (it is small). Add it to the tool check of `runner-image.yml` (the check of T-324).

## Prove

`cargo test -p podrunner`; the tool check of `runner-image.yml` in a podvm unit: `command -v less`.

In the code (2026-11-17): `less` is in the base package list of `images/runner/Dockerfile`, and the step "The tools of the base image in a podvm unit" of `runner-image.yml` checks `command -v less`. Open: that step green on a `runner-image.yml` run; the measure `rung:podvm` of the user gate.

# T-354: A podvm unit has no devpts: `openpty` fails

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding F3; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

In a podvm unit `/dev/pts` is empty, `pty.spawn` gives `out of pty devices`, and `script` fails. On `--runner-dir` each probe passes.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`. Other project: podvm.

## Approach

Ask podvm by a COORD issue `(to podvm)` to mount a new `devpts` instance (`newinstance,ptmxmode=0666`) and link `/dev/ptmx` in each unit of `run -d`, on each rung. Set the entry `blocked` on it. podrunner then adds a pty probe to the tool check.

## Prove

`cargo test -p podrunner`; a probe step in `runner-image.yml` (`python3 -c "import pty; pty.spawn(['/bin/true'])"`).

In the code (2026-11-18): podvm `0.2.0-beta.32` mounts a devpts instance of the unit and its `ptmx` on the namespace rung (COORD-185, PodUtils/podvm#572); podrunner pins it. `runner-image.yml` has the step "A pty in a podvm unit": `run -d` of the runner image, then `pty.spawn` of Python and `script` through `podvm exec`. Open: the green run of that step; then close GitHub #216.

## Done

2026-11-18, `f348c47` (the step) and the next commit (the check of the gate). `runner-image.yml` run 44302792725: the step "A pty in a podvm unit" passed on x86_64 and aarch64 (`pty.spawn` of Python and `script` through `podvm exec` in a unit of `run -d`, on the namespace rung of a GitHub-hosted runner). The user-path gate has the check `pty:podvm` (`scripts/user-gate.py`, `pty`): a step runs `script`; a failure on a rung other than the namespace rung is an xfail that names the rung, as `pids:podvm`. `sh scripts/test-install.sh`: 14 passed. Level: proven in CI on GitHub-hosted runners; the other rungs wait for the next run of `user-gate.yml`.


# T-355: A failed route of the egress module is reported as a host that the policy refused

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding F4; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** partial

## Problem

With `--allow github.com`, a job that cannot reach `github.com` gets `the egress hold refused github.com; if the job needs one, allow it: --allow HOST`. The host is allowed. The cause is no route (no resolver, or the upstream failed). The fix that the message gives does not work.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`.

## Approach

In `crates/podrunner-local/src/exec/proxy.rs` `denials`, read the rule of each `HOST:PORT (RULE)`. For the rules `resolve` and `upstream`, print `podrunner: JOB: the egress module found no way to HOST (RULE: the resolver of this host / the proxy); see doctor`. Keep the "allow it" text for policy rules only. Keep the stage text from the audit line in `podrunner-egress/src/denials.rs`. The route itself follows the fix of F1.

## Prove

`exec::proxy::tests::a_failed_route_is_not_a_refusal` (denials with rule `resolve` give no "allow it" line). The user-gate lane `proxy-only-dns` runs `hold:podvm`.

In the code (2026-11-17): a denial with the rule `resolve` or `upstream` is named `the egress module found no way to HOST (CAUSE); an allow does not fix this: see podrunner doctor`, and only the refusals of the policy keep the fix `--allow HOST` (`exec::proxy::split`). Open: the route itself follows T-351 (COORD-078); the measure of the user gate (T-372).

# T-356: A run does not check `--toolcache` before its first unit

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding F6; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

`podrunner run wf.yml --toolcache /tmp/toolcache` with no such directory fails job `g` with `unit-start-failed: podvm run: exit 125: ... the host path ...: No such file or directory`. The run exits 1. The message gives no fix.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`.

## Approach

In `crates/podrunner/src/run_local/routes.rs` (`export_dir`, or a new check next to it), refuse a `--toolcache` that is not a directory before the first unit: `--toolcache DIR: not a directory; fix: mkdir -p DIR`, exit 78. Better: call the path checks of `dry::paths` at the start of each run, not only of a dry run.

## Prove

`flags_tests::a_run_refuses_a_missing_toolcache_before_a_unit`.

## Done

2026-11-17: each run checks the paths of the run before its first unit, as a dry run does (`dry::paths` from `slots::build`); a missing `--toolcache` gives `--toolcache DIR: not a directory; fix: mkdir -p DIR`, exit 78. Proven in CI on GitHub-hosted runners by `qa6_tests::a_run_refuses_a_missing_toolcache_before_a_unit` (once the gate runs).

# T-357: `--state FILE` is named as `--artifacts`; a `--log-dir` with a missing parent does not name the parent

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding F7; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

`--state /tmp/statefile` (a file) gives an error on the internal path `/tmp/statefile/artifacts`. `--log-dir /nope/logs` gives `Permission denied` and does not say which directory cannot be made.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`.

## Approach

In `run_local.rs`, check `--state` (and `--rerun`) before the artifact store: a path that exists and is not a directory gives `--state PATH: not a directory`. In `routes.rs` `export_dir` and in `dry.rs`, on a failed `create_dir_all`, name the first parent that is missing and the parent that refuses it (`doctor` does this already: reuse it).

## Prove

`flags_tests::a_state_that_is_a_file_is_named_by_its_flag`.

# T-358: `check --config run.toml` gives a serve error with no hint

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding F8; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

`podrunner check --config run.toml` (the file of the installer) exits 78 with `unknown field image, expected one of host, ...`. The error does not say that `check` reads a configuration of `serve`, and that `run.toml` is for `run`.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`.

## Approach

In `serve::check`, when the parse fails and the file has a key of `run` (`podvm`, `images`, `image`, `runner-dir`; the list of `run_config.rs`), add `; this is a file of podrunner run (run.toml): check it with podrunner doctor --run FILE`. Also say it in `check --help`.

## Prove

`crates/podrunner/tests/check.rs`: `a_run_toml_names_doctor_run`.

# T-359: `podrunner --bogus` says "an unknown command"

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding F9; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

`podrunner --bogus` exits 64 with `an unknown command "--bogus"`. `--help` and `--version` are options at the top level, so the right noun is option.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`.

## Approach

In `cli.rs` dispatch, a first word that starts with `-` gives `an unknown option "--bogus"; the options are --help and --version`.

## Prove

`cli::tests::an_unknown_top_option_is_an_option`.

# T-360: `--no-git`, `--with-git` and `--checkout-head` do not apply to the default checkout

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding F11; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

In a git tree a run with no `--checkout` checks out the top of the tree. `--no-git` alone exits 64 with `--checkout-head, --with-git and --no-git need --checkout`. The same flag works with `--checkout .`.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`.

## Approach

Move the check from `args.rs:325` to the place where the default checkout is chosen (`run_local.rs`, the T-336 code). Refuse the modifiers only when the run has no checkout at all.

## Prove

`flags_tests::no_git_applies_to_the_default_checkout`.

# T-361: The line of a concurrency wait comes after the run, not when the wait starts

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding F13; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

A second run of a held group prints nothing for about 40 s. The line `the group ... waits for the run PID` comes at the end, after `ok`. A user cannot tell a wait from a hang.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`.

## Approach

In `run_local.rs:426` (and the job-level `Grouped` notes), print the note on stderr at once (with `--json`, as a `{"event":"wait"}` line). Keep the vector for the tests.

## Prove

`concurrency::tests::a_wait_is_named_before_it_blocks` (the note arrives before the lock is free).

# T-362: `print-unit` writes a relative `--config` and `--state` into `ExecStart`

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding F15; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

`print-unit --config example.toml --state DIR` writes `ExecStart=... --config example.toml`. systemd starts the unit in `/`, so the unit fails to start.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`.

## Approach

In `cli.rs`, make `--config` absolute with `std::fs::canonicalize` (the file must exist already), and `--state` absolute against the current directory.

## Prove

`cli::tests::print_unit_makes_paths_absolute`.

# T-363: The stop line of a signal names a running job when no job runs

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding F16; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

After the run ends, the web view waits for SIGINT. At SIGINT podrunner prints `podrunner: stopping: the running job ends and its unit goes`. No job runs. `--watch` between runs gives the same line.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`.

## Approach

Add an atomic `IDLE` to `cancel.rs`; `web::hold` and the wait of `--watch` set it. The handler then writes `podrunner: stopping` only. Add the open part to T-339 item 13 in the same commit.

## Prove

`cancel::tests::an_idle_stop_names_no_job`.

# T-364: A raw seed file gives `invalid data`, not the format

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding F18; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

A 32-byte raw seed file gives `podrunner: the seed file: invalid data`, exit 78. A file of 64 hex digits works. The error does not name the format.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`.

## Approach

In `pins.rs` `read_seed`, read the bytes; when they are not UTF-8 or not 64 hex digits, say `the seed file: not 64 hex digits (a raw seed of 32 bytes: xxd -p -c 64 FILE)`. Use the text of the system, not `e.kind()` (as T-323 F12).

## Prove

`pins::tests::a_raw_seed_names_the_format`.

# T-365: A `--var`, `--input` or `--env` of two config files is refused, not overridden

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding F20; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

`.actrc` with `--var MYVAR=a` and `.podrunner/run.toml` with `var = ["MYVAR=b"]` give `--var MYVAR: given two times`, exit 64. For each other flag a later file or the command line wins.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`.

## Approach

In `run_config.rs` `expand`, drop an earlier `NAME=` of `--var`, `--input`, `--env` and `--secret` when a later source sets the same name. Keep the refusal of a name given two times on the command line, or in one file.

## Prove

`run_config::tests::a_later_file_wins_for_a_var`.

# T-366: `serve` gives no ready line in the foreground

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding F21; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

`serve` with a valid configuration writes nothing at start. The control socket works. A user in the foreground cannot tell that it is ready, or where its socket is.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`.

## Approach

In `serve.rs`, next to `notify::send("READY=1")`, write `podrunner: serve: ready; the control socket is PATH; the sources: N; the kinds: K`.

## Prove

`cargo test -p podrunner`; a test in `crates/podrunner/tests/` that reads the first line of `serve` with the fake configuration.

# T-367: An abort of podrunner leaves no cause; two runs at once may abort

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding F22; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

One of two `podrunner run` processes with podvm units, started a few seconds apart, exited 134 after the start of `a.0`. No text was kept. A later pair passed.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`.

## Approach

(1) Install a panic hook in `crates/podrunner/src/main.rs` that writes the panic text, the thread and the version to stderr and to `$XDG_STATE_HOME/podrunner/crash-PID.txt`, then aborts. (2) Make the image tar cache of T-242 safe for two runs: write to a temporary name and `rename`, under a `flock` of the cache. (3) Add a stress step to the user gate: two runs of `hello.yml` on podvm at once, 5 times; each exits 0.

## Prove

`cargo test -p podrunner`; the user-gate step `two-runs`; `cargo test -p podrunner a_panic_writes_its_text`.

# T-368: The two executors give a different console

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding F23; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

On `--runner-dir`, stderr carries the console of the runner (`Listening for Jobs`, the host name, `Runner listener exit with 0 return code`). On podvm only the `[JOB.N]` lines come. The host name appears only on the executor with no isolation.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`.

## Approach

In `crates/podrunner-broker-fake/src/runner.rs` (the console relay at `:102`), put each console line in the log of the job (`--log-dir`) and show on stderr only a warning or an error line, as on podvm. Give the runner a machine name `podrunner-host` (`--name` of the config of the runner directory) so that the host name does not leak.

## Prove

`runner::tests::the_console_stays_in_the_log`.

# T-369: The installer names no command to add its binary directory to PATH

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding N1; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

The warning `DIR is not in PATH: add it, or use the full path` (`scripts/install.sh`) gives no command, and a new user does not know which file to change.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`.

## Approach

Add the line `export PATH="DIR:$PATH"` and the profile file of the shell (`~/.profile`) to the warning.

## Prove

`scripts/test-install.sh` checks the text.

# T-370: A run does not say which configuration files it read

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding N2; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

The installer writes `run.toml` under `XDG_CONFIG_HOME`, and a run does not say which configuration files it read; a user in another directory cannot tell which file applies.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`.

## Approach

`doctor --run` names each file that `run_config::expand` read.

## Prove

`cargo test -p podrunner` (a test of `doctor --run` with a `run.toml` of the user).

# T-371: The user gate has no lane of a host with DNS only through an authenticated proxy

**Source:** a user QA report of `v0.1.0-beta.7` (2026-11-17), finding gate; the triage of the report against `main` at `b752051`
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** open

## Problem

The lane `proxy-only` (`scripts/lab/proxy-only.sh`) lets the lab user reach the loopback resolver, and its mitmproxy takes no credential. The host of the QA report has DNS only through an authenticated proxy that intercepts TLS. F1 and F4 pass in the lane and fail on the host.

## Premise

The report and its logs (not in the repository), read on 2026-11-17; the triage read the code of `main`.

## Approach

Add a lane `proxy-only-dns` (or options of `proxy-only.sh`): no working resolver for the lab user (REJECT of port 53 to each address, the loopback too), and mitmproxy with `--proxyauth` and the credential in `HTTPS_PROXY`. Run the corpus (which fetches actions), `hold:podvm` and `image verify` of the real release asset in it.

## Prove

`cargo test -p podrunner`; The lane fails on `b752051` (F1) and passes after the fix.

# T-372: The user gate checks success paths only

**Source:** PodUtils/podnet#649 (COORD-102 of podnet, 2026-11-17): a gate that measured reach, not correctness; the user QA of beta.7 (F1, F4)
**Category:** release
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** partial

## Problem

Each check of the user gate runs one path that must pass. No check makes a way out fail: a dead proxy, a proxy that refuses a CONNECT, a host with no resolver, a refused connect of a unit, a podvm that exits 125 at the start of a unit. The users found the defects of beta.7 on these paths (F1, F4, #212): a wrong cause in the error, a fix that does not work, a claim of a log that is not there.

## Premise

The lanes and the checks of `scripts/user-gate.py` and `.github/workflows/user-gate.yml`, read on 2026-11-17.

## Approach

Add error-path checks to the user gate, each with three demands: the run ends within a limit, the exit code is the documented one, and stderr has one line that names the cause and a fix that works. The cases: a proxy that is dead (`HTTPS_PROXY` to a closed port), a proxy that answers 502 to each CONNECT, no resolver (T-371), a `--podnet --allow HOST` job with no route to HOST (T-355), a missing `--toolcache` (T-356), a podvm that refuses the unit start (a missing image in the store). Report each with the level of proof (`AGENTS.md`, section 12).

## Prove

The user gate has a check for each case, and each holds on `main`.

In the code (2026-11-17): the check `errors` of the user gate (`error_paths`) runs a dead proxy for an action fetch (each lane with `runner-dir`), a missing `--toolcache` and an image that is not in the store (each lane with podvm). Each case must end within 120 s, with exit 78, and a last line with its words. A case that does not hold is an xfail with its detail, until it holds. Open: a proxy that answers 502, no resolver (T-371), no route to an allowed host (T-355); then the xfail becomes a failure.

# T-373: `--runner-dir` writes the runner files, a private key with them, at mode 0644

**Source:** GitHub #218 (a user who read the code of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`start_runner_in` wrote `.runner`, `.credentials` and `.credentials_rsaparams` with `std::fs::write`, so at the umask (0644). A unit gets them at 0600, and the contract says 0600.

## Premise

The report of #218, with the lines of the code and a measure.

## Approach

Write each runner file with `OpenOptions::mode(0o600)` and set the mode of a file that was there (`runner::private_file`).

## Prove

`cargo test -p podrunner-broker-fake a_runner_file_is_written_at_mode_0600`

## Done

2026-11-17: each runner file of `--runner-dir` is written at 0600, also over a file at 0644 (`runner::private_file`). Proven in CI on GitHub-hosted runners by `runner::tests::a_runner_file_is_written_at_mode_0600` (once the gate runs).

# T-374: `--state` writes the outputs of the jobs into a file that each user can read

**Source:** GitHub #219 (a user who read the code of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`state.json` holds the outputs of each cell, which can hold a secret; it was 0644 in a 0755 directory.

## Premise

The report of #219, with the lines of the code and a measure.

## Approach

The state directory is 0700 and `state.json` 0600 (`rerun::write_state`); the outputs stay as they are, as `--rerun` gives them to the jobs after.

## Prove

`cargo test -p podrunner-local the_state_is_private`

## Done

2026-11-17: `--state DIR` is 0700 and `DIR/state.json` 0600 (`rerun::write_state`). Proven in CI on GitHub-hosted runners by `rerun::mode_tests::the_state_is_private` (once the gate runs).

# T-375: `--checkout` of a directory in a git tree gets a `.git` of the whole tree

**Source:** GitHub #220 (a user who read the code of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The `.git` of a checkout was a clone of the top of the tree, while the workspace held the files of the directory only: `git status` in the job showed each file as deleted or untracked.

## Premise

The report of #220, with the lines of the code and a measure.

## Approach

A `.git` only for the top of a git tree: a directory in it gets no `.git` by default, and `--with-git` there is refused with the fix (`tree::is_git_top`).

## Prove

`cargo test -p podrunner-local a_subdirectory_of_a_git_tree_gets_no_git`

## Done

2026-11-17: a checkout of a directory in a git tree has no `.git` by default, and `--with-git` there is refused with `give the top of the tree to --checkout` (`tree::is_git_top`). Proven in CI on GitHub-hosted runners by `tree::tests::a_subdirectory_of_a_git_tree_gets_no_git` (once the gate runs).

# T-376: Each job leaves the thread and the listener of its fake broker

**Source:** GitHub #221 (a user who read the code of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`Broker::start` and `start_unix` spawned an accept loop with no end; a dropped broker kept its thread and its descriptor, so a long `--watch` grows.

## Premise

The report of #221, with the lines of the code and a measure.

## Approach

At drop a broker sets a stop flag and connects once to its listener; the loop sees the flag and closes the listener (`listen.rs`).

## Prove

`cargo test -p podrunner-broker-fake a_broker_that_goes_closes_its_listener`

## Done

2026-11-17: a dropped broker closes its listener and ends its accept loop (`Broker` `Drop`, `listen::Wake`). Proven in CI on GitHub-hosted runners by `listen::tests::a_broker_that_goes_closes_its_listener` (once the gate runs). A host that refuses the connect of the wake keeps the loop until the process ends.

# T-377: Two concurrency groups can share one lock file

**Source:** GitHub #222 (a user who read the code of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The name of a lock file replaced each character that is not safe with `_` and cut it at 200 characters, so `a b` and `a_b`, and two long names, shared one lock.

## Premise

The report of #222, with the lines of the code and a measure.

## Approach

A readable prefix of 80 characters and a SHA-256 of the whole name, with no case, as GitHub compares the names of a group with no case.

## Prove

`cargo test -p podrunner concurrency`

## Done

2026-11-17: the lock file of a group is `PREFIX-HASH.lock`, HASH of the repository and the group with no case (GitHub: `prod` and `Prod` are one group); `a b` and `a_b`, and two long names, get two files. Proven in CI on GitHub-hosted runners by the test of `run_local::concurrency` (once the gate runs).

# T-378: `--break-on-failure` pauses after each step after a failure; `--break` before an `always()` step is skipped

**Source:** GitHub #223 (a user who read the code of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

Each pause after a step had `failure()`, so each step that ran after a failure paused again; a `--break` pause had `success()`, so it did not run before a step that runs after a failure.

## Premise

The report of #223, with the lines of the code and a measure.

## Approach

The pause after a failure holds once in a job (a marker in `RUNNER_TEMP`); a `--break` pause takes the condition of its step.

## Prove

`cargo test -p podrunner pause`

## Done

2026-11-17: the pause after a failure holds once in a job, and a `--break` pause has the condition of its step (`pause::insert`). Proven in CI on GitHub-hosted runners by `pause::tests` (once the gate runs); a run with a pause is not run in CI.

# T-379: `timeout-minutes` is held to 6 days

**Source:** GitHub #224 (a user who read the code of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

The limit of a job was `timeout-minutes`, at most 6 days. GitHub holds a job of a self-hosted runner to 5 days, and one of a hosted runner to 6 hours.

## Premise

The report of #224, with the lines of the code and a measure.

## Approach

Hold it to 5 days, the limit of a self-hosted runner, as podrunner is one (`drive::MAX_MINUTES`); the documents say it.

## Prove

`cargo test -p podrunner-local the_limit_of_a_job_is_at_most_five_days`

## Done

2026-11-17: the limit of a job is its `timeout-minutes`, at most 5 days (`drive::MAX_MINUTES`); `docs/cli.md` says it. Proven in CI on GitHub-hosted runners by `drive::tests::the_limit_of_a_job_is_at_most_five_days` (once the gate runs).

# T-380: The image of an excluded cell of a matrix is pulled

**Source:** GitHub #225 (a user who read the code of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The images of a job with a matrix were made from each value of each vector; `exclude` was left out, so a run pulled the image of a cell that does not run.

## Premise

The report of #225, with the lines of the code and a measure.

## Approach

Make the cells as GitHub does (the product, less `exclude`, then `include`), and the images from those cells (`run_local/matrix.rs`).

## Prove

`cargo test -p podrunner the_image_of_an_excluded_cell_is_not_pulled`

## Done

2026-11-17: the images of a matrix come from its cells, with `exclude` and `include` (`matrix::cells`). Proven in CI on GitHub-hosted runners by `matrix::tests::the_cells_of_a_matrix` and `the_image_of_an_excluded_cell_is_not_pulled` (once the gate runs).

# T-381: `run DIR --checkout DIR` replaces the value of `--checkout` with a workflow file

**Source:** GitHub #226 (a user who read the code of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

Directory mode found the file argument by its text, so a `--checkout` with the same text was replaced with the first workflow file.

## Premise

The report of #226, with the lines of the code and a measure.

## Approach

Find the argument that the parser takes as the file (`many::run_dir`).

## Prove

`cargo test -p podrunner a_checkout_with_the_text_of_the_directory_stays`

## Done

2026-11-17: directory mode replaces the argument that the parser takes as the file, not a value of a flag (`many::run_dir`). Proven in CI on GitHub-hosted runners by `qa6_tests::a_checkout_with_the_text_of_the_directory_stays` (once the gate runs).

# T-382: The test of the plants of the fake unit is not stable under load

**Source:** the full test suite of this session (2026-11-17): `tests::each_unit_plant_fails_its_test` of `podrunner-fake` failed once and passed three times alone
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

Under the load of the whole suite, one plant of the fake unit passed its test of the conformance once. A test that fails at random hides a real failure and makes a red gate that nobody reads.

## Premise

One run of `cargo test --workspace` here; the plant that passed is not named (the output was cut).

## Approach

Run the test 50 times under load (`cargo test -p podrunner-fake each_unit_plant -- --test-threads 1` beside a loaded suite), name the plant, and make its test wait for its event, not for a time.

## Prove

`cargo test -p podrunner-fake each_unit_plant_fails_its_test` passes 50 times of 50 under the load of the suite.

State (2026-11-17): six runs of the suite of `podrunner-fake` at one time on this sandbox: each passed. Not reproduced; the entry stays open for the next failure.

## Done

2026-11-17: the cause, from a second failure in the check of a push (`TagOnly passes test 25`): the plant `TagOnly` ends the tagged processes, but the shim ends its children when the payload ends; under load the shim ran before its own kill and ended the untagged orphan, so the plant passed the test that it must fail. The plant now ends the shim first (`podrunner-fake/src/unit.rs`), so the orphan stays. Proof: the suite of `check.sh` passes; the race is in the plant, not in a driver.

# T-383: A container mount can name the runner root or a file of it

**Source:** GitHub #227 (a user who read the code of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

The hook policy took a mount source of `<runner>/.` (or a file of the runner root, as `.credentials_rsaparams`): the private key of the runner went into a container.

## Premise

The report of #227, with the lines of the code and a measure.

## Approach

A mount source is under `_work` or `externals` of the runner, and a `.` part of a path is refused (`policy::MOUNT_DIRS`, `clean_abs`).

## Prove

`cargo test -p podrunner-hook the_runner_root_is_no_mount_source`

## Done

2026-11-17: the hook policy takes a mount source under `_work` or `externals` of the runner only, and refuses a `.` part of a path. Proven in CI on GitHub-hosted runners by `tests::the_runner_root_is_no_mount_source` (once the gate runs).

# T-384: A secret that starts a longer secret leaves the rest of the longer one in a log

**Source:** GitHub #228 (a user who read the code of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

The mask replaced the secrets in their order: with `abc` before `abcdef`, a log kept `***def`.

## Premise

The report of #228, with the lines of the code and a measure.

## Approach

Mask the longest secret first (`logs::mask`), for the live lines, `--log-dir` and the web view.

## Prove

`cargo test -p podrunner the_longest_secret_is_masked_first`

## Done

2026-11-17: `logs::mask` masks the longest secret first. Proven in CI on GitHub-hosted runners by `logs::mask_tests::the_longest_secret_is_masked_first` (once the gate runs).

# T-385: `--github-token-env ""` fails, though the documents say it sends no token

**Source:** GitHub #229 (a user who read the code of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

An empty NAME read the variable of the empty name and failed the run.

## Premise

The report of #229, with the lines of the code and a measure.

## Approach

An empty NAME sends no token (`secrets::secrets`).

## Prove

`cargo test -p podrunner an_empty_token_variable_sends_no_token`

## Done

2026-11-17: an empty NAME of `--github-token-env` sends no token. Proven in CI on GitHub-hosted runners by `qa6_tests::an_empty_token_variable_sends_no_token` (once the gate runs).

# T-386: A `pull_request` filter of branches reads the default branch, not the base branch

**Source:** GitHub #230 (a user who read the code of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The filter of `on.pull_request.branches` took the default branch of the tree; GitHub takes the base branch of the pull request.

## Premise

The report of #230, with the lines of the code and a measure.

## Approach

Take `pull_request.base.ref` of the payload, else the default branch, for the filter and for `github.base_ref` (`context::base_branch`).

## Prove

`cargo test -p podrunner a_pull_request_is_filtered_on_its_base_branch`

## Done

2026-11-17: a pull request is filtered on `pull_request.base.ref` of its payload, and `github.base_ref` is that branch (`context::base_branch`). Proven in CI on GitHub-hosted runners by `qa6_tests::a_pull_request_is_filtered_on_its_base_branch` (once the gate runs).

# T-387: A `workflow_dispatch` with no inputs takes each `--input`

**Source:** GitHub #231 (a user who read the code of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

With no `inputs:` table, each `--input` went to the job; the documents say an input that `on:` does not declare fails the run.

## Premise

The report of #231, with the lines of the code and a measure.

## Approach

A `workflow_dispatch` with no `inputs:` declares none: each `--input` is refused by name (`context::dispatch_inputs`).

## Prove

`cargo test -p podrunner the_inputs_of_workflow_dispatch_take_their_defaults_and_types`

## Done

2026-11-17: `on: workflow_dispatch` with no `inputs:` refuses each `--input` with `on.workflow_dispatch declares no inputs`. Proven in CI on GitHub-hosted runners by `tests::the_inputs_of_workflow_dispatch_take_their_defaults_and_types` (once the gate runs).

# T-388: `check` passes a `mem` of `[unit.podvm]` or `[unit.microvm]` that `serve` refuses

**Source:** GitHub #232 (a user who read the code of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`serve::plan` did not read the limits of the podvm kind, and `microvm_kind::plan` did not either: `check` called a file usable that `serve` then refused.

## Premise

The report of #232, with the lines of the code and a measure.

## Approach

`podvm_kind::plan` and `microvm_kind::plan` read the limits as the build does (`limits_of`).

## Prove

`cargo test -p podrunner --test check`

## Done

2026-11-17: `check` reads `mem`, `cpus` and `pids` of `[unit.podvm]` and `[unit.microvm]` as `serve` does (`podvm_kind::plan`, `microvm_kind::plan`). Proven in CI on GitHub-hosted runners by `tests/check.rs` (once the gate runs).

# T-389: The control socket takes a field that an op does not take

**Source:** GitHub #233 (a user who read the code of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

A request was the flattened body of an envelope, so `deny_unknown_fields` did not hold: `status` with `job_id` or a bogus field was a status.

## Premise

The report of #233, with the lines of the code and a measure.

## Approach

Drop `v` and hold the fields of each op by hand, then read the request itself (`control::parse`).

## Prove

`cargo test -p podrunner bad_lines_are_bad_requests`

## Done

2026-11-17: each field that an op of the control socket does not take gives `bad-request` (`control::parse`); the source wire already refused them (a test guards it). Proven in CI on GitHub-hosted runners by `control::tests::bad_lines_are_bad_requests` and `wire::field_tests` (once the gate runs).

# T-390: An error of a source whose text starts with "the " ends its sidecar

**Source:** GitHub #234 (a user who read the code of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The sidecar was ended when an `Unavailable` error began with `the `, the words of a broken wire; a source that says `the API is down` lost its process.

## Premise

The report of #234, with the lines of the code and a measure.

## Approach

The pipe marks a failure of the wire itself; only that ends the sidecar (`sidecar::Pipe::broken`).

## Prove

`cargo test -p podrunner-mint an_error_of_the_source_keeps_its_sidecar`

## Done

2026-11-17: only a failure of the wire (a write, a read, a deadline, an end) ends the sidecar; an error that the source sends goes to the caller (`Pipe::broken`). Proven in CI on GitHub-hosted runners by `sidecar::tests::an_error_of_the_source_keeps_its_sidecar` (once the gate runs).

# T-391: A stale first payload of an architecture hides a fresh one after it

**Source:** GitHub #235 (a user who read the code of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

Placement took the first payload of the architecture; when it was stale, each job was refused, though a fresh payload came after it.

## Premise

The report of #235, with the lines of the code and a measure.

## Approach

Take the first payload of the architecture that is not stale (`placement::pick_payload`).

## Prove

`cargo test -p podrunner-slots a_stale_image_and_a_digest_off_the_pin_list_are_refused`

## Done

2026-11-17: placement takes the first payload of the architecture that is not stale, and refuses `image-stale` only when each is stale. Proven in CI on GitHub-hosted runners by `tests::guards::a_stale_image_and_a_digest_off_the_pin_list_are_refused` (once the gate runs).

# T-392: The affinity of a shared principal compares a repository with case

**Source:** GitHub #236 (a user who read the code of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

With `same_principal_ok`, `Owner/Repo` and `owner/repo` were two repositories.

## Premise

The report of #236, with the lines of the code and a measure.

## Approach

Compare with no case, as GitHub does (`placement`).

## Prove

`cargo test -p podrunner-slots a_shared_principal_runs_one_unit_at_a_time`

## Done

2026-11-17: the affinity compares the repositories with no case. Proven in CI on GitHub-hosted runners by `tests::guards::a_shared_principal_runs_one_unit_at_a_time` (once the gate runs).

# T-393: A dry run ends the processes and removes the files of a dead run

**Source:** GitHub #237 (a user who read the code of `v0.1.0-beta.7`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`stale::end_dead_runs` ran before the dry run was known, and killed tagged processes.

## Premise

The report of #237, with the lines of the code and a measure.

## Approach

A dry run does not end or remove anything (`stale::end_dead_runs(dry)`).

## Prove

`cargo test -p podrunner a_dry_run_leaves_a_dead_run`

## Done

2026-11-17: a dry run leaves the processes and the files of a dead run. Proven in CI on GitHub-hosted runners by `stale::tests::a_dry_run_leaves_a_dead_run` (once the gate runs).

# T-394: The parity suite pulls from Docker Hub with no login and meets its limit

**Source:** runner-image 44100036389 (2026-11-17): `09-dockerfile.yml` failed with `toomanyrequests` in the engine of a guest
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The Dockerfile action of the parity suite starts `FROM busybox@sha256:...` of Docker Hub. A shared hosted runner meets the limit of an unnamed pull, and the check of `runner-image.yml` fails for a cause outside podrunner.

## Premise

The log of the job `build (x86_64)` of runner-image 44100036389.

## Approach

Take the same digest from `mirror.gcr.io` (it serves the manifest and the blobs on its own host), and allow that host to the guests of the step.

## Prove

The step "The parity suite in guests with an engine" of `runner-image.yml`.

## Done

2026-11-17: the Dockerfile starts `FROM mirror.gcr.io/library/busybox@sha256:a7dbc319...` (the same digest; checked: 200 for the index, the blobs on `mirror.gcr.io`), and the step allows `mirror.gcr.io`. Not run yet in CI.

# T-395: The actions and images of a called workflow of the tree are not pinned before the first job

**Source:** GitHub #238 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`actions::pin` read the top workflow only; a `jobs.ID.uses: ./PATH` was read later, so its actions and images were not in the lock or the store.

## Premise

The report of #238, with the lines of the code and a measure.

## Approach

Read each called workflow of the tree before the pin (to 10 levels, in the tree only) and pin its actions, its trees and its images (`composites::local_calls`).

## Prove

`cargo test -p podrunner the_actions_of_a_called_workflow_are_pinned`

## Done

2026-11-17: the called workflows of the tree are read before the pin, and their actions, trees and images are pinned (`composites::local_calls`); a called workflow out of the tree is refused, also in `calls::Texts`. Proven in CI on GitHub-hosted runners by `qa6_tests::the_actions_of_a_called_workflow_are_pinned` (once the gate runs).

# T-396: A later matrix `include` does not overwrite a value that an earlier one added

**Source:** GitHub #239 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`matrix::cells` kept the first value of an added key; GitHub says an added value can be overwritten.

## Premise

The report of #239, with the lines of the code and a measure.

## Approach

An include that fits a cell sets each key that is not an original key (`matrix::cells`).

## Prove

`cargo test -p podrunner a_later_include_overwrites_an_added_value`

## Done

2026-11-17: a later include overwrites an added value; an original value stays. Proven in CI on GitHub-hosted runners by `matrix::tests::a_later_include_overwrites_an_added_value` (once the gate runs). The defect came with T-380 of this session.

# T-397: A matrix `exclude` with an expression drops each cell

**Source:** GitHub #240 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

An entry with an expression became an empty entry, which matched each cell.

## Premise

The report of #240, with the lines of the code and a measure.

## Approach

An entry with a value that is not static is left out, and an empty entry is no entry (`matrix::entries`).

## Prove

`cargo test -p podrunner an_exclude_with_an_expression_drops_no_cell`

## Done

2026-11-17: an `exclude` or `include` entry with an expression is left out (`matrix::entries`). Proven in CI on GitHub-hosted runners by `matrix::tests::an_exclude_with_an_expression_drops_no_cell` (once the gate runs). The defect came with T-380 of this session.

# T-398: `--vnet` gives no `NODE_USE_ENV_PROXY`

**Source:** GitHub #241 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

The proxy variables of the vnet were set, but `fetch` of Node reads them only with `NODE_USE_ENV_PROXY`.

## Premise

The report of #241, with the lines of the code and a measure.

## Approach

Set `NODE_USE_ENV_PROXY=1` in `vnet::env`, as the host path does.

## Prove

`cargo test -p podrunner-local the_environment_of_a_runner_under_the_vnet_shim`

## Done

2026-11-17: `vnet::env` sets `NODE_USE_ENV_PROXY=1` when it is not set. Proven in CI on GitHub-hosted runners by `vnet::tests` (once the gate runs). The defect came with T-220 of this session.

# T-399: `--offline` with a store of actions that the user cannot write is refused

**Source:** GitHub #242 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`dry::paths` asked for a store that takes files also with `--offline`, which only reads it.

## Premise

The report of #242, with the lines of the code and a measure.

## Approach

With `--offline`, the store must be a directory only (`dry::paths`).

## Prove

`cargo test -p podrunner the_paths_of_a_run_by_their_use`

## Done

2026-11-17: with `--offline`, `--actions` must be a directory, not one that takes files. Proven in CI on GitHub-hosted runners by `qa6_tests::the_paths_of_a_run_by_their_use` (once the gate runs). `--frozen` still needs a store that takes files: it fetches a tarball that the lock pins. The defect came with T-345 of this session.

# T-400: A local composite action of `./../PATH` or a link out of the tree is read

**Source:** GitHub #243 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`composites::local_uses` read `base.join(path)` with no check: a workflow made podrunner read a file out of the checkout.

## Premise

The report of #243, with the lines of the code and a measure.

## Approach

Refuse a `..` part and a link that leads out of the tree, by name (`composites::local_uses`, `local_calls`, `calls::Texts`).

## Prove

`cargo test -p podrunner composites`

## Done

2026-11-17: a local action or a called workflow with `..` or a link out of the tree is refused by name. Proven in CI on GitHub-hosted runners by `composites::tests::the_uses_of_local_composite_actions` (once the gate runs). The defect came with T-346 of this session.

# T-401: A dry run with a runner directory that cannot take the lock says planned

**Source:** GitHub #244 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`dry::paths` checked the files of the runner directory, not that it takes the lock file of the run.

## Premise

The report of #244, with the lines of the code and a measure.

## Approach

Check that the runner directory takes files, and name the lock file (`dry::paths`).

## Prove

`cargo test -p podrunner the_paths_of_a_run_by_their_use`

## Done

2026-11-17: the runner directory must take files, with the lock file named in the error. Proven on a constrained host as a user (`labdns`, no root, a 0555 runner directory: exit 78 with `.podrunner-run.lock`); the unit test skips as root.

# T-402: The mint forgets a late credential that starts or goes back after its alert

**Source:** GitHub #245 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The count of late credentials of a host was the pending set: a credential that went back after its alert was not counted, so a host stayed under `revoke_after`.

## Premise

The report of #245, with the lines of the code and a measure.

## Approach

Count the late credentials of each host for the life of the mint, and count a credential that starts or goes back past the limit (`gate::late`, `gate::settle`).

## Prove

`cargo test -p podrunner-remote a_released_late_credential_still_counts`

## Done

2026-11-17: each late credential of a host counts, also one that starts or goes back after its alert (`late_of`, `settle`). Proven in CI on GitHub-hosted runners by `tests::a_released_late_credential_still_counts` (once the gate runs).

# T-403: A mount source of the hooks follows a symbolic link that the job made

**Source:** the security review of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** blocked

## Problem

The policy checks the text of a mount source only; the host resolves a link in it. A step makes `_work/x` a link to `/` and posts `run_container_step` with the source `_work/x`, or swaps `_work/_temp/_github_home` for a link: a container gets the root of the host, writable (`podrunner-hook/src/policy.rs`, `podrunner-hook/src/unit.rs`, `podrunner-podvm/src/unit/containers.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Resolve each source on the host under the runner directory with no link (`openat2` with `RESOLVE_BENEATH|RESOLVE_NO_SYMLINKS`, or a walk with `O_NOFOLLOW`), and refuse a link in the path. Ask podvm by COORD whether its `-v` refuses a linked source.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

In the code (2026-11-17): `HookUnit::spec` refuses a mount source with a symbolic link in its path under the root of the unit (`container-option-refused`). The test `tests_unit::a_linked_mount_source_is_refused` fails on the code before the fix and passes after it. Open: a link that the job makes after the check and before podvm reads the path (a race): podvm must refuse a linked `-v` source itself (a COORD issue to podvm).

In the code (2026-11-18): podvm `0.2.0-beta.32` gives `--podvm-volume-base DIR` (a `-v` source under DIR opens with no symbolic link and binds by its descriptor), on the namespace rung only; the userland rung and the guest tiers refuse it. The containers of a podvm unit run on the supervise and userland rungs (U19), so the flag cannot hold the race yet.

## Blocker

The containers of a podvm unit on the namespace rung with `--podvm-volume-base` (T-532), or the flag on the supervise and userland rungs (PodUtils/podvm@T-1972, COORD-090, GitHub #300).


# T-404: In the local mode the logs of a hook step are in the socket directory that the unit writes

**Source:** the security review of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

The job makes `$GUEST/logs/step-1.log` a link to a file of the user; the host writes the step output through it, and can read it back (`podrunner-local/src/exec/hooks.rs`, `podrunner-hook/src/unit.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Keep the logs out of the socket directory, as `serve` does (`dir.with_extension("logs")`), and open each log with `O_EXCL|O_NOFOLLOW`.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-17: the logs of the hook steps of the local mode are beside the socket directory (`hooks::logs_dir`, 0700), never in it, and go with the hooks. The test `the_logs_are_out_of_the_socket_directory` fails on the old path. Not run with a real unit.

# T-405: Secrets of a job and the credential of the proxy go into the argv of podvm

**Source:** the security review of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

Each step environment (secrets, `ACTIONS_RUNTIME_TOKEN`, a proxy URL with `user:pass@`) becomes `-e K=V` of a `podvm` process; each user of the host reads `/proc/PID/cmdline` (`podrunner-podvm/src/unit/containers.rs`, `unit.rs`, `podrunner-local/src/exec/proxy.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Give the environment to podvm in a file at 0600 or on a descriptor (a COORD issue to podvm when podvm has no form for it).

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

State (2026-11-17): podvm answered by PodUtils/podvm#509 (COORD-179): `exec --env-file PATH` is on podvm `main` for its next beta, and `run --env-file` is in `0.2.0-beta.21` and later; the file is the caller's to make at 0600 and to remove. podvm keeps its pin until the gate of `PodUtils/podvm@T-1949` (PodUtils/podvm#567), so podrunner takes `exec --env-file` with that beta.

In the code (2026-11-18): podvm `0.2.0-beta.32` takes `--env-file` on `run` and `exec` (COORD-185, PodUtils/podvm#572). The podvm kind writes the environment of a unit, of a job container and of a step in a container to a file of mode 0600 in the directory of the unit, which goes after the call (`crates/podrunner-podvm/src/unit/envfile.rs`); an older podvm keeps `-e`. Tests: `the_values_go_in_a_file_of_mode_0600`, `an_older_podvm_gets_the_values_in_the_argv`. Not run with a real podvm unit here. podvm answered COORD-090 with COORD-189 (PodUtils/podvm#585): `--env-file` holds on the machine and incus tiers, so the microvm and incus kinds write the file into the directory of the unit (`podrunner_podvm::write_env_file`; test `an_environment_file_keeps_the_values_out_of_the_argv`); it goes with the directory at `destroy`. Open: a value with a newline stays `-e` until `--env-file0` (PodUtils/podvm@T-1971) is in a tagged podvm.

## Done

2026-11-18, the commit that names T-405. podvm `0.2.0-beta.33` is pinned; with it each value of the environment of a unit, of a job container and of a step in a container goes in the file of mode 0600, each `KEY=VALUE` ended by a NUL (`--env-file0`), a value with a newline too: no value is in the argv. The microvm and incus kinds pass the same arguments (`write_env_file` gives them). `cargo test -p podrunner-podvm envfile` (3: `each_value_goes_in_a_file_of_nul_ended_entries` fails on the code before, where the newline value stays `-e`), `cargo test -p podrunner-microvm`, `cargo test -p podrunner-incus`. Level: proven here with a stand-in podvm; the real podvm of `runner-image.yml` takes the file in each unit there.


# T-406: The fake broker of the local mode gives the job message to each local process

**Source:** the security review of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

The broker on the loopback checks no credential: each uid can `POST /acquirejob` and get the secrets and the token of the job; the socket directory of a guest is 0755 (`podrunner-broker-fake`, `podrunner-local/src/front.rs`, `exec.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Give the broker a bearer token that only the runner files of the unit hold, and make the socket directory 0700.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

In the code (2026-11-17): the socket directory of a unit is new at 0700, and one that another user made, or a link, is refused (`work::private_dir`; test `a_private_directory_is_new_and_ours`). Open: the fake broker on the loopback checks no credential; it needs a token that only the runner files of the unit hold, and a check of the signed assertion of `oauth2/token`.

## Done

2026-11-17: the fake broker has random tokens for each broker (module `auth` of `podrunner-broker-fake`). `oauth2/token` gives the client token only for a client assertion (RS256) that a key of `Broker::runner_files` signed. The session, the long poll and `acquirejob` take only that token. `renewjob`, `completejob`, the id-token, the live feed and each twirp call (the logs, the cache, the artifacts) take it or the job token. A blob URL of the logs has a random signature in place of `sig=fake`. The tokens that the runner sends on each route were measured on the runner of the pin. The checkout script gives the job token (`github.podrunner_token`, masked) to curl on its standard input (`printf` is a builtin; `curl -K -`), never in an argv. A hold is `/_podrunner/hold/KEY/CELL` with a hold key of the broker (`github.podrunner_hold`): that key is in the argv of curl or wget and gives the holds only. The request line refuses control characters (no escape sequence reaches a log or a terminal), and the lists `seen` and `missing` have a bound. A second review found the token in argv; it is fixed in the same work.

Proof: the test `a_caller_with_no_token_gets_no_job` fails on the code before the change (and with the blob check or the twirp gate removed); `only_an_admitted_key_gets_the_token` and `each_route_takes_its_token` test the edges (no key, a key of another runner, a changed claim, a bad form, the case of the scheme). The suites of the runner of the pin pass (`podrunner-broker-fake --test runner`, `podrunner --test local`, `scripts/agent/check.sh` with `PODRUNNER_RUNNER_DIR`). As a user with no root (`podrunner run --runner-dir`), a step that scans the ports of the loopback finds the broker and gets 401 on the long poll, `acquirejob`, `oauth2/token` and the session, and the job passes. Level: proven on a constrained host as a user (the host mode); the guest kinds run in CI.

# T-407: Directories of a run with a fixed name in the shared `/tmp` take a directory that another user made

**Source:** the security review of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

Another user makes the directory first; `create_dir_all` takes it, and the writes of the relay and the vnet shim follow that user's links (`podrunner-local/src/exec.rs`, `exec/hooks.rs`, `exec/vnet.rs`, `podrunner-slots/src/run.rs`). Also GitHub #250.

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Make each such directory with `mkdir` at 0700 that fails when it exists (or a random name), check its owner, and open files with `O_EXCL|O_NOFOLLOW`.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

In the code (2026-11-17): the socket directory of a unit (`work::private_dir`). Open: the other fixed names (`podrunner-run-*`, the vnet shim copy, the run directories of `serve`); also GitHub #250.

## Done

2026-11-17: each directory of a run with a fixed name in the shared temporary directory is new at 0700 or refused by name when another user made it, or when it is a link: the tree and the units of `podrunner run` (`stale::run_dirs`, with `work::private_dir`, named `PID-N` so that two runs of one process get two; they are made before the mirror of `--engine`, which is in the units directory, as the vnet shim copy and the tarball), the logs of the hooks (`exec/hooks.rs`), the socket directory of a unit (before), and the root of the socket directories of `serve` (`own::own_root`: `STATE/egress` or its short fallback in the temporary directory). The state path of `doctor` is fixed but nothing writes it. The export directory at 0777 is T-408.

A regression of this work, found by runner-image 44170614365: with `--parallel` above 1, each slot builds its executor with `STATE/slot-N`, so the mirror of `--engine` (now in the units directory of the run) was not found, and the engine pulled from Docker Hub, which the egress refuses. The mirror path now comes from the run (`routes::Common::mirror`); the step of runner-image is the proof. Proof: `the_directories_of_a_run_are_private` (fails with `create_dir_all`) and `the_root_of_the_sockets_is_a_directory_of_this_user`. As a user with no root: a link that another user made at `/tmp/podrunner-run-units-PID-0` before the run with that PID: the run refuses it by name (exit 78) and writes nothing through it; a normal run passes.

# T-408: With `--export` the work directory of a job is 0777 in `/tmp`

**Source:** the security review of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

Other users read the workspace and change the checkout during the run (`podrunner-local/src/exec.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Make it with a random name at 0700 and give the unit access by its user map, not by mode 0777.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-17: the work directory of a job with `--export` is `work` at 0777 in a new parent at 0700 of this user (`work::export_dir`; a name of another user, or a link, is refused). The runner user of the unit writes it through its mount, as before; no other user of the host can reach it. The end of the job removes the parent. Proof: `the_export_directory_is_open_only_through_a_closed_parent`. Level: the unit test; the guest kinds (podvm, microvm) run `--export` in CI only, not on this host.

# T-409: The signed pin list trusts the signer key of the same file

**Source:** the security review of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

A step that rewrites the configuration makes a key, signs its own pin list and writes its own `signer`: `verify` passes (`podrunner-slots/src/pins.rs`, `config.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Pin the trusted signer key out of each file that the uid of a unit can write (a key of the binary or of a root file).

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-17: `check` and `serve` refuse a configuration file that the group or others can write, or that another user (not root) owns (`tiers::file_closed`): the signer of the pin list is in that file. The other way in is closed already: `trusted` at `redirect` needs `same_principal_ok = false` (DR-4), so the user of a unit is not the user of `serve` and cannot write a file of mode 0644 or 0600. A key out of the file (a key of the binary) would add nothing more for that case. Test `a_configuration_that_others_can_write_is_refused`. Level: a unit test.

# T-410: With no `[egress] default_policy`, an offer with `mode: open` opens the egress

**Source:** the security review of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

With no ceiling the offer policy is taken whole, also `mode: open`; `policy.rs` says an offer only takes away (`podrunner-slots/src/run.rs`). Not certain that a job can set the policy of an offer.

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

With no ceiling, narrow the offer by the policy that allows nothing, or refuse an offer policy.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-17: with no `[egress] default_policy`, the ceiling allows nothing: an offer with a policy (also `mode: open`) gets no way out, and the audit names why (`Slots::policy_of`, `run.rs`). An open ceiling (`mode = "open"`) takes the policy of the offer. The call is in `docs/decisions.md`; `docs/configuration.md` says it. Test `with_no_ceiling_an_offer_opens_nothing` fails on the old code (the open offer opened the egress).

# T-411: The `id` of a configured organization or repository is not checked

**Source:** the security review of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** open

## Problem

The sidecar sends no `owner_id` or `repo_id`, and `same_id` passes when the offer has none: an `id` in the configuration checks the name only (`podrunner-tier/src/lib.rs`, `tools/scaleset-sidecar/sidecar.go`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Require the id in the offer when the configuration gives one, and send it from the sidecar.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

# T-412: The front of a guest reads a request line with no limit

**Source:** the security review of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** open

## Problem

A job sends one long line with no newline, and the memory of the host process grows (`podrunner-local/src/front.rs`). Also GitHub #260.

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Read each line through `take(HEAD_MAX)` and answer 431, with a read timeout and a cap of connections.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

# T-413: Text that a job controls reaches stderr with no escape

**Source:** the security review of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** open

## Problem

A host name of a CONNECT with control bytes, or the last line of `podvm run`, can write lines of the log or change the terminal (`podrunner-egress/src/denials.rs`, `exec/proxy.rs`, `podrunner-hook/src/hooks.rs`, `podrunner-slots/src/run.rs`). Not certain for the host name.

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Give each such value through `text::display` before it is written.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

# T-414: The audit line of a failed start carries the text of the driver to the source

**Source:** the security review of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** open

## Problem

The line holds host paths and podvm output and goes to the source (`podrunner-slots/src/run.rs`); `audit.rs` says an audit line holds no path.

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Keep the detail in the log of `serve`, and put only a code in the audit line.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

# T-415: A matrix job keeps the outputs of its last cell only

**Source:** the review of the local mode against GitHub of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

GitHub combines the outputs of the cells, and an empty value does not replace a set one; `needs.a.outputs.o1` is empty in the example of the GitHub documents (`podrunner-local/src/lib.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Combine the outputs of the cells: a value that is set wins over an empty one, the last set one wins.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-17: The outputs of a matrix job combine its cells in the order that they ended: a value that is set wins over an empty one, and the last cell to end that sets it wins (`LocalSource::complete`). Test `the_outputs_of_a_matrix_job_combine_its_cells` (three cells that end out of order) fails on the old code.

# T-416: A job of a called workflow ignores the `permissions` of the caller

**Source:** the review of the local mode against GitHub of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

`id-token: write` on the caller gives no OIDC token when the called workflow has no `permissions:` (`podrunner-local/src/lib.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Take the permissions of the calling job as the default of each job of the called workflow, as GitHub does.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-17: A called job takes the `permissions` of each job that calls it: a level of `{}` gives no token, and the id-token needs the top caller to give it and each called level to give it or to name no permissions (`calls.rs` `Calls::permissions`, `oidc::of_chain`). The test of the calls checks eight cases, with a matrix of calls; the end-to-end test of the OIDC token (`tests/local.rs`, the official runner) gets the token in a called job.

# T-417: A run can hold one concurrency group two times

**Source:** the review of the local mode against GitHub of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

The workflow and a job with the same group, or cells with `--jobs 2`: with no `cancel-in-progress` the run waits for itself; with it, it sends SIGTERM to itself (`run_local/concurrency.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Let a run hold a group that it holds already (a count of holds in the run).

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-17: The groups that this process holds are in a registry, with whether the run or a job holds each: a job takes a group of its run at once, and a job that meets another job of its run waits for it; a run never signals itself, also with `cancel-in-progress` (`concurrency::hold`). Test `a_run_never_waits_for_or_cancels_itself`. On the old code the job waits for its own run for ever, or the run sends SIGTERM to itself.

# T-418: `github.run_id`, `run_number` and `run_attempt` are always 1

**Source:** the review of the local mode against GitHub of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

A cache key with `github.run_id` hits the first entry for ever; the fallback `head_ref || run_id` of a group makes runs cancel each other (`run_local/context.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Give each run a new `run_id` (time and process), a `run_number` that counts in the state, and `run_attempt` from `--rerun`.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-17: Each run has a new `run_id` (the time in milliseconds), a `run_number` that counts the runs of the repository and the workflow in the cache of the user, and `run_attempt` 1; a `--rerun` keeps the id and the number of its run (`run.json` beside the state) and counts its attempt. A dry run writes nothing (`run_local/ids.rs`). Test `each_run_has_its_own_id_and_number`.

# T-419: `--protected` misses jobs of called workflows, needs of `--job` and environment names of an expression

**Source:** the review of the local mode against GitHub of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

Those jobs run with no approval and get the secrets of the environment (`run_local/protect.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Check the environment of each cell that runs, after the expansion of calls and of `--job`, and refuse a name of an expression in a protected run.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-17: Each protected environment with no approval is held. The terminal asks for each environment of a job that runs, of each job that `--job` needs, and of each job of a called workflow, to 10 levels (`protect.rs` `used`, `walk`); each other protected one is refused with no question, so a name of an expression cannot pass. The unit test checks needs and two levels of calls; the end-to-end test (`tests/local.rs`, the official runner) holds a called job and a job of an expression, and runs both with `--approve`.

# T-420: An environment name of an expression is not evaluated

**Source:** the review of the local mode against GitHub of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

`environment: ${{ inputs.env }}` stays as text; the OIDC `sub` names it (`podrunner-local/src/lib.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Evaluate the name with the contexts of the job (`inputs`, `vars`, `matrix`, `needs`).

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-17: The name of an environment is evaluated with `github`, `inputs` (of the call for a called job), `vars`, `matrix`, `strategy` and `needs`; an empty name is no environment (`podrunner-local/src/environment.rs`). The concurrency group of a called job now reads the inputs of its call too. Tests: a name of `inputs`, of `matrix` and an empty one; a called job; the end-to-end test of T-419 with a name of `vars`.

# T-421: No checkout tree when only a called workflow uses `actions/checkout`

**Source:** the review of the local mode against GitHub of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

`default_checkout` reads the caller file only; the called workflows come from the current directory, not from the top of the git tree (`run_local/args.rs`, `run_local.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Look for `actions/checkout` in the called workflows too, and read them from the root of the tree.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-17: With no `--checkout`, the default checkout also looks in each called workflow of `./PATH` (to 10 levels), and a call of another repository takes the tree when there is one; a called workflow of `./PATH` comes from the tree of the project, not the current directory (`run_local/checkout_root.rs`). The end-to-end test (`tests/local.rs`) runs from a subdirectory of a git tree, and the checkout of the called workflow gets the tree. Its first CI run (level2 44203198207) failed: an earlier test of `tests/local.rs` held the lock of the runner directory in the test process, and the child process of this test could not take it. The test is now in `tests/local_subdir.rs`, a test binary of its own, which level2 and `check.sh` run.

# T-422: The stand-in of `actions/checkout` needs `curl` in a job container

**Source:** the review of the local mode against GitHub of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

It fails in `ubuntu:24.04`, `debian:*-slim`, `python:*-slim` and `node:*-slim` (`podrunner-local/src/message.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Fetch the tree with a fallback that needs no tool of the image (the Node of the runner in `/__e`, or `wget`, `python3`), as `actions/checkout` runs on Node.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-17: The stand-in of `actions/checkout` gets the tree with curl, else with node (of the runner in `/__e`, or of the image), else with python3, and else names what it needs in one line; the token goes to curl on its standard input and to node and python3 in the environment of that one process, never in an argv. The tree goes to a file first, so a failed fetch fails the step. Test `the_checkout_stand_in_needs_no_curl` runs the script under a PATH with no curl (python3, node, neither) against a broker that checks the token; it fails on the old script. Level: the curl path runs in the suites of the runner; the node and python3 paths in the unit test only.

# T-423: `--schedule` reads no `timezone` of `on.schedule`

**Source:** the review of the local mode against GitHub of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

GitHub takes `timezone`; podrunner runs at the UTC time, and `docs/cli.md` says UTC (`run_local/watch.rs`, `podrunner-local/src/cron.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Read `timezone` (an IANA name, through the zone files of the host) and say it in the documents.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-18: podcrates `v0.71.0` gives `pod-time` (PodUtils/podcrates@T-907). `--schedule` reads each cron of `on.schedule` with the zone of its `timezone` (`Zone::load`, `Cron::next_in`); an unknown zone fails with its line; the note "timezone is not read" is gone, and the cron of `podrunner-local` (removed by T-423) is removed for `pod_time::Cron`. Test `the_crons_of_on_schedule_and_a_change_of_the_tree`: 09:00 in Europe/Berlin on 1970-01-01 is 08:00 UTC; an unknown zone gives `line 3: timezone "Mars/Olympus"`.


# T-424: The minimal payload of `pull_request` has no `head.repo` and `base.repo`

**Source:** the review of the local mode against GitHub of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

The guard `head.repo.full_name == github.repository` skips the job locally, and runs it on GitHub (`run_local/context.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Give `head.repo` and `base.repo` with `full_name`, `name` and `owner.login` of the repository.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-17: The minimal payload of `pull_request` names the repository in `head.repo` and `base.repo` (a branch of the same repository, not a fork), with `user` and `draft`. The test of the event context checks both sides.

# T-425: A git submodule is an empty directory in the checkout

**Source:** the review of the local mode against GitHub of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

Also with `submodules: true` or `recursive` (`podrunner-local/src/tree.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Add the files of each submodule (git `ls-files --recurse-submodules`), or refuse `submodules:` by name.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-17: The tarball of a git tree holds the tracked files of each submodule with a work tree, to 10 levels (with `--checkout-head`, each at the commit that its parent records), and a list of the submodules (`tree/submodules.rs`). The stand-in keeps those of its `submodules:` and leaves each other one an empty directory; a kept submodule with no files fails the step with the fix; an expression in `submodules`, or `submodules` with a checkout of another repository, is refused by name (`message/checkout.rs`). The stamp of `--watch` takes the submodule files too. Tests: the tarball of a tree with two levels of submodules, at disk and at HEAD, and of a clone with no update; the two refusals; the end-to-end test (`tests/local.rs`, the official runner) with `false`, `true` and `recursive`, and a clone with no update that fails with the fix.

# T-426: `github.workflow` is the file name, not the `name:` of the workflow

**Source:** the review of the local mode against GitHub of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

GitHub gives the `name:`, else the path (`run_local/context.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Give `name:`, else the path of the file in the tree.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-17: `github.workflow` is the `name:` of the workflow, else its path (the path under .github/workflows for a file of that directory, else the name of the file); `github.workflow_ref` and `github.workflow_sha` are in the context, so the runner gives `GITHUB_WORKFLOW_REF` (`run_local/context.rs` `workflow_of`). Tests: `workflow_tests` (a name, no name, a file out of `.github/workflows`), and the context test.

# T-427: `--rerun DIR --job CALLER` reruns nothing of a call

**Source:** the review of the local mode against GitHub of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

Jobs that need a call always rerun, also after it passed (`podrunner-local/src/rerun.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Map a caller to the cells of its called jobs in the state.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-18: A rerun maps each cell of a called job (`CALLER__CALLEE.N`, `CALLER-N__CALLEE.N`) to the job that calls (`rerun.rs` `top_job`, read from the jobs of the workflow): a call that passed is kept, and `--job CALLER` runs the jobs of the call. Tests: the rerun test with a call and a matrix of calls; the end-to-end test (`tests/local.rs`, the official runner) reruns a failed job and keeps the call.

# T-428: A directory run with `--artifacts` or `--state` keeps the output of the last workflow only

**Source:** the review of the local mode against GitHub of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

Each workflow writes the same directory (`run_local/many.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Give each workflow a directory of its own under DIR, or refuse the pair.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-17: A directory run gives each workflow `DIR/NAME` for each flag of an output directory (`--artifacts`, `--state`, `--log-dir`, `--export`, `--rerun`; `many.rs` `outputs`), so no workflow writes over another. Tests: `outputs` in `many.rs`; the end-to-end test (`tests/local.rs`, the official runner) runs two workflows with `--state` and reads both states.

# T-429: A line of a secret, var or env file with `<<` in its value is refused

**Source:** the review of the local mode against GitHub of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

The runner reads `NAME=a<<b` as a plain value (`podrunner-local/src/secrets.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Take `NAME<<DELIM` only when `<<` comes right after the name, as the runner does.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-17: A line is `NAME<<DELIM` only when `<<` comes before the first `=`, as the file commands of the runner read it; `A=a<<b` is a plain value (`secrets.rs`). The test of the parser checks both forms; it fails on the code before the fix.

# T-430: `--changed-since` does not see new files that git does not track

**Source:** the review of the local mode against GitHub of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

The checkout has them; a `paths` filter does not (`run_local/context.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Add the new files that git does not ignore to the changed files.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-17: The changed files of `--changed-since` are the diff with the work tree and each new file that git does not ignore, from the top of the tree (`context.rs` `changed`). The test runs from a subdirectory with a changed file, a new file and an ignored file.

# T-431: The `secrets:` of a call see only the `secrets` context

**Source:** the review of the local mode against GitHub of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** open

## Problem

Another expression gives an empty secret with no message (`podrunner-local/src/calls.rs`). Not certain how often users meet it.

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Evaluate with the contexts that GitHub gives there, and name an empty result.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

# T-432: A concurrency group queues each waiter

**Source:** the review of the local mode against GitHub of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

GitHub keeps one pending run and cancels the others (`run_local/concurrency.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Keep one waiter; a newer waiter cancels the older one.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-18: A run that waits for a group of another run takes the waiting place of the group (a second lock file); a newer waiter sends SIGTERM to the older one and takes the place, as GitHub cancels a pending run. The older run ends with a line that names the cause (`concurrency.rs` `wait_place`). The test holds the group and the waiting place in two other processes; the older one ends by the signal, and the new waiter waits for the holder. On the code before the fix the older waiter stays.

# T-433: `--schedule` runs on the current branch and takes a cron under 5 minutes

**Source:** the review of the local mode against GitHub of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

GitHub runs a schedule on the default branch, every 5 minutes at most (`run_local/watch.rs`).

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Run on the default branch, and say that a cron under 5 minutes runs every 5 minutes on GitHub.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-17: A run of `--event schedule` (also of `--schedule`) has the ref of the default branch and the commit of its head, with the files of the tree (`context.rs` `facts`). `--schedule` and `--validate` name a cron more often than each 5 minutes, once for each loop (`watch.rs` `notes`). Tests: the ref of a schedule in the context test; four crons in the watch test.

# T-434: The OIDC claims of a called job name the caller

**Source:** the review of the local mode against GitHub of `65ee5e3` by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

`job_workflow_ref` names the caller; the `*_id` claims and `GITHUB_WORKFLOW_REF` are missing (`podrunner-broker-fake/src/oidc.rs`). Also GitHub #254.

## Premise

The report of the review agent, read from the code, not run. Each case is a measure to make first (`docs/development.md`, "The proof of a change").

## Approach

Give the claims of the called workflow, and the `*_id` claims.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix.

## Done

2026-11-17: The claims take `workflow_ref` from the context, and a called job gets `job_workflow_ref` and `job_workflow_sha` of its own workflow: a `./` workflow of the tree at the ref and commit of the run, a workflow of another repository at the ref of its call and its pinned commit, a `./` workflow of another repository in that repository (`Calls::job_workflow`, `Calls::origin`; the text of a `./` call through a `./` workflow of another repository comes from that repository too). `workflow_sha` and `runner_environment` (`self-hosted`) are added. podrunner knows no numeric ids of GitHub, so the `*_id` claims stay out. Test: the token of a local and a remote nested called job (`calls/tests.rs`). Closes #254.

# T-435: mint: one thread for each TCP connection with no cap, each waiting up to 90 s

**Source:** GitHub #246 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The remote mint spawns one thread for each TCP connection with no cap, and each thread can wait up to `READ_TIMEOUT` (90 s) for a TLS handshake and a request:

## Premise

The report of #246, with the lines of the code and a measure.

## Approach

Cap the connections served at one time (as `podrunner-hook` does with `CLIENTS`), refuse the excess with a short `busy` reply, and/or set a much shorter handshake deadline. Add a test that opens more than the cap and sees the excess refused.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: The remote mint serves at most 256 connections at one time (`transport::CONNECTIONS`) and at most 16 of one peer address (`PER_PEER`, so one peer cannot lock the hosts out: a review found it); a connection over a cap closes at once, and the refusals get one line on stderr each minute. A failed spawn of a thread gives its place back. The mint waits 15 s for the handshake and the request (the 90 s stay on the client, above the long poll). A guard lowers the count also when a call panics. Test `a_flood_of_connections_gets_no_more_threads` (16 idle connections of one peer, then one more is closed at once, and a connection from `127.0.0.2` is still served); on the old code the extra connection waits.

# T-436: guest: the self-test needs a loopback bind, so a host that denies loopback can never claim guest

**Source:** GitHub #247 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The guest self-test needs a listener on the host loopback, so a host that denies `bind(127.0.0.1:0)` can never claim `guest`. `guest::second`:

## Premise

The report of #247, with the lines of the code and a measure.

## Approach

Use a canary that does not need an AF_INET bind (a unix socket, a file, or a connect to a port that the host already has), or treat a bind failure as "the unit cannot reach a listener" and continue with the other checks. Add a test that runs the self-test on a host that denies the loopback bind.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: A host that refuses the bind of the loopback has no listener in the guest self-test: the unit tries a closed port, and the other lines still decide (another kernel, no network interface, no file or process of the host or of another unit). Level: the code only; no host here refuses the bind.

# T-437: placement: a required limit that the kind reports is refused, although the docs allow reported

**Source:** GitHub #248 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

Placement refuses a required limit that the kind reports, although the docs allow `reported`. `placement::place`:

## Premise

The report of #248, with the lines of the code and a measure.

## Approach

Accept `Enforced` or `Reported` for a required limit; for `wall`, also require `tree_kill: Verified`. The report can say that the limit is reported. Add a test with `wall: Reported` and `tree_kill: Verified`.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: A required limit holds on a kind that enforces or reports it (SPEC 9); `wall` also needs a verified tree kill, which `Capability::limit` does not say for `reported`. Cases in `a_required_limit_that_no_kind_enforces_is_refused` (reported wall with and without the tree kill, reported disk, no disk); they fail on the old code.

# T-438: policy: a rule with an unknown field is accepted and the field is dropped, broadening the policy

**Source:** GitHub #249 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A policy rule with an unknown field is accepted by `JobOffer::check`, and `policy::narrow` drops the field, so the resulting policy is broader than the rule. Measured with an offer policy:

## Premise

The report of #249, with the lines of the code and a measure.

## Approach

Validate each rule with the `broker-policy.v1` rule shape (or `pod_broker::Policy::from_value`) in `JobOffer::check`/`policy_fault`, and refuse an unknown rule field by name. `narrow` should preserve the known fields it keeps, not silently drop the rest. Add a test with an unknown rule field.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: An offer rule (or a rule of `default_policy`) with a field other than `host`, `path` and `ports`, or with a value of another type (`ports` as text, a port of 0 or above 65535, no `host`), is a rule that podrunner cannot hold: placement refuses the offer with `egress-policy-unsupported`, and `check` refuses the ceiling (`policy::unheld_rule`, which also holds the path rule). Tests `a_rule_that_podrunner_cannot_hold_is_found` and the `methods` case of `egress_refuses_an_unknown_key_and_a_policy_it_cannot_hold`.

# T-439: run: a checkout of another repository with an empty, full or SHA ref is not resolved

**Source:** GitHub #251 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A checkout of another repository passes `with: ref` through unchanged, so an empty ref, a full ref and a commit SHA are not resolved. `checkout_target`:

## Premise

The report of #251, with the lines of the code and a measure.

## Approach

Normalize the ref: empty or absent -> `HEAD`; `refs/heads/X` -> `X`; `refs/tags/X` -> `X`; a full 40-hex SHA -> the SHA (and `Git::resolve` should return it as the commit). Add tests for each form.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-18: podcrates `v0.71.0` (PodUtils/podcrates@T-906) resolves the ref of a checkout of another repository: an empty ref is `HEAD`, a full ref is looked up as written, a commit id needs no lookup. podrunner takes the tag. Test `the_ref_of_a_checkout_of_another_repository` (`podrunner-local`, GitHub #251) fails on `v0.70.0`, where an empty ref gives `o/empty@`.

# T-440: run: fetch::host_of cuts an IPv6 action-server address at the first colon

**Source:** GitHub #252 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`fetch::host_of` splits the authority at the first `:`, so an IPv6 literal is cut. Measured:

## Premise

The report of #252, with the lines of the code and a measure.

## Approach

Parse the authority with brackets: `[::1]:8443` -> `::1`; `[2001:db8::1]` -> `2001:db8::1`; a user part is removed first. Add the three cases above to the `host_of` test.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: `fetch::host_of` takes an IPv6 literal in brackets, after a user part and before a port, and gives `[ADDR]` in its one spelling, the host form of a rule of `pod-broker` (`on_local_network` takes the brackets off). The cases of #252 are in the test of `host_of`; they fail on the old code.

# T-441: run: a runs-on label of ppc64le, s390x, riscv64, i386, i686 or armv7 is not refused

**Source:** GitHub #253 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`dispatch::foreign` refuses only ARM and X64 labels. Measured:

## Premise

The report of #253, with the lines of the code and a measure.

## Approach

Recognize the other architecture labels (`ppc64le`, `s390x`, `riscv64`, `i386`, `i686`, `armv7`, and the like) and refuse them by name, or refuse any label that names an architecture the host is not. Add the cases above to the dispatch test.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: `dispatch::foreign` refuses a label (or the end `-ARCH` of one) of ppc64le, ppc64, s390x, riscv64, loongarch64, i386, i686, x86, armv7, armv7l, armv6, armhf and mips64, and the label `ARM` alone (a 32-bit ARM runner; `-arm` at the end of a GitHub image stays 64-bit): no unit runs them, and the line names `--platform`. Test `a_label_of_an_architecture_with_no_unit_is_refused` (a table) fails on the old code.

# T-442: serve: a malformed offer is refused as event-not-allowed with no detail, not bad-request

**Source:** GitHub #255 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

A malformed offer is refused as `event-not-allowed` with no detail, while the remote mint refuses the same offer as `bad-request` with the parser's text. `engine::intake`:

## Premise

The report of #255, with the lines of the code and a measure.

## Approach

Keep the parser/check error and refuse with a `bad-request` reason (the source wire's `withdraw` reason is a string) and an audit detail, as the remote mint does. Add a test with a bad schema, an unknown limit and a bad policy.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: A malformed offer (the parser, or `JobOffer::check`: the schema, a limit, the policy) is withdrawn with the reason `bad-request`, as the remote mint does, also when it is offered again, and the audit line has the fault (`bad-request: ...`). The report keeps `event-not-allowed`: pod-job has no code of a bad offer. Test `a_malformed_offer_is_a_bad_request_with_its_fault` (a bad schema, a bad policy, an unknown limit) fails on the old code. After `forget` (an hour after the end), a malformed offer offered again is `event-not-allowed` again: the job is gone.

# T-443: serve: a job-completed hook event is lost when the unit ends before the next tick

**Source:** GitHub #256 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A `job-completed` hook event is read only while the job is `Started` or `Running`. If the unit ends before the next tick, the job is `Ended`/`Destroyed`/`Reported` and `hook_events` skips it; the event stays in the hook port and no audit line is written. Measured with a unit that ends, then a queued `JobCompleted`:

## Premise

The report of #256, with the lines of the code and a measure.

## Approach

Read the hook events of a unit that is `Ended`/`Destroyed` before `finish` closes it, or drain the hook port in `finish`/`close_hooks` and write the `JobCompleted` line there. Add a test with a unit that ends before the event is read.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: `finish` reads the hooks of the unit (`read_hooks`) before it destroys the unit and closes them, so a `job-completed` that the last tick did not read gives its audit line. Test `a_job_completed_event_of_a_unit_that_ended_is_not_lost` (a port that gives nothing on the read of the tick) fails on the old code.

# T-444: podvm: a start that fails after run -d leaves the podvm unit, because destroy skips podvm rm

**Source:** GitHub #257 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

If a podvm unit starts (`podvm run -d` succeeds) and a later step of `start` fails before the unit is inserted into `self.units`, `destroy` does not call `podvm rm -f`. `PodvmUnit::destroy`:

## Premise

The report of #257, with the lines of the code and a measure.

## Approach

Call `podvm rm -f` for the unit even when `removed` is `None` (the unit directory's store is enough), or insert the `Running` record before the steps that can fail and remove it on the error path. Add a test with a driver whose `root_of_unit`/`waiter` fails after a successful `run -d`.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: `destroy` of a podvm unit removes its containers and the podvm unit by the store of the unit directory also when the unit has no record: a start that failed after `run -d`, or a replay. Test `a_destroy_with_no_record_removes_the_podvm_unit` (a fake podvm that logs its calls). The tests that write a fake podvm hold one lock (`FAKE_PODVM`; a fork of one test held the file of another: `Text file busy`). `verify_tree_kill` and `image_reference` moved to `unit/tree_kill.rs` (the size of `unit.rs`).

# T-445: source-conformance: a timed-out test leaves its thread and child process alive

**Source:** GitHub #258 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`source-conformance` runs each test in a thread with a 60 s timeout, but a timed-out test is not stopped: the thread stays in `Under::exchange`'s `read_line`, and the child process it spawned is not killed. `source_conformance`:

## Premise

The report of #258, with the lines of the code and a measure.

## Approach

Kill the child and join the thread on timeout (a `Child` handle shared with the timeout path, or a process group), or run each test in a child process that the timeout can kill. Add a test with a program that sleeps past a short limit.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: On the time limit of a test, `source-conformance` ends each child of the process that runs the program (an argument of its command line is the program, as given or resolved, so a script under its interpreter counts, and the line ends with its arguments) and each process below it, so the thread of the test ends; then it waits for the thread a short time. Test `a_program_that_never_answers_leaves_no_process` (a script that runs `sleep`, with a limit of 0.5 s) leaves processes on the old code.

# T-446: serve: two source names that map to the same spool file lose one refused report

**Source:** GitHub #259 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

The spool file name maps each character that is not alphanumeric or `-_` to `_`, so two source names can share one file. `spool`:

## Premise

The report of #259, with the lines of the code and a measure.

## Approach

Encode the source name reversibly (percent/hex for the bytes that are not safe) or add a hash of the full name, as the concurrency lock should. Add a test with `a b` and `a_b`.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: The spool file of a report has a readable form of the source and the job and a 64-bit FNV-1a hash of the exact names (`spool::file_name`): `a b` and `a_b` get two files. Test `two_source_names_get_two_spool_files`.

# T-447: run: a lock key or uses: with ../x@v1 is accepted and the fetch URL normalizes to another repository

**Source:** GitHub #261 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A lock key and a workflow `uses:` accept a repository part with `..`, so the fetch URL can normalize to another repository. Measured:

## Premise

The report of #261, with the lines of the code and a measure.

## Approach

Validate the owner and repository with the same name rule as `--repository` (alphanumeric, `-_.`), in `Lock::parse` and in the `uses` split, and refuse `..`, an empty part and a path separator. Add a test with `../x@v1` and `o/../x@v1`.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: `store::repo_ok` refuses a remote `uses:` whose owner, repository or path has a part that is empty, `.`, `..` or not a name, before any fetch (the pins of `run`: each action, each checkout of another repository, each repository of a called workflow) and in the job message. A lock key is used only for a key of these, so the check holds it too. Tests `the_repository_of_a_uses_is_a_name` (a table), the `../x@v1` case of the message test, and `a_repository_with_dot_dot_is_refused_before_a_fetch` (a checkout of `repository: ../x`, a call of `../x/...`, a step; it fails with the check of the step `uses:` only, which a review found).

# T-448: check: a podvm config with a bad upstream_proxy, an old podvm or a bad podnet passes check and stops serve

**Source:** GitHub #262 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`podrunner check` says a podvm configuration is usable, and `serve` refuses the same file at build. Three measured cases:

## Premise

The report of #262, with the lines of the code and a measure.

## Approach

In `serve::plan` for `[unit.podvm]`, parse `upstream_proxy` (or call the same `pod_proxy::Proxy::parse` as `build`), call `podvm.require(LOWEST)`, and `check_driver` the `podnet` binary. Add the three cases above to the `check` tests.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: `check` checks what `serve` checks at its build: `upstream_proxy` parses, podvm is `0.2.0-beta.5` or later (exit 80, as `serve`), and the `podnet` driver of `[unit.podvm]` passes `check_driver`. The three cases of #262 are in `check_and_serve_refuse_a_bad_file_with_one_text`, with a fake podvm that answers its version; they fail on the old code.

# T-449: check_driver: a non-executable driver and a writable ancestor are accepted

**Source:** GitHub #263 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`check_driver` accepts a driver file that is not executable, and it does not check the ancestors of the driver's directory. Measured:

## Premise

The report of #263, with the lines of the code and a measure.

## Approach

Require `m.is_file()` and `m.mode() & 0o111 != 0` for a driver, and check each ancestor up to `/` for ownership and the group/other write bits (as `parents_readable` does for read). Add the three cases above to the `check_driver` tests.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: `check_driver` needs a regular file that this user can run (`access(X_OK)`), and checks each real directory above it (`canonicalize`: a link hides none; a root directory with the sticky bit, as `/tmp`, may be open). Test `a_driver_comes_from_a_checked_place` has the cases of #263.

# T-450: run: a dry run with a --vnet podnet binary that cannot build the shim says planned

**Source:** GitHub #264 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

A dry run with a `--vnet` path that is a file but not a usable podnet binary says `planned`; the real run fails when it builds the shim. Measured with `--vnet /bin/false`:

## Premise

The report of #264, with the lines of the code and a measure.

## Approach

In `dry::paths`, call `vnet::shim` (into a scratch directory that the dry run removes) or at least check that the path is a `.so` or an executable podnet binary; name the build failure before the first job. Add a test with `/bin/false`.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: A dry run builds the shim of `--vnet` into a private scratch directory that it removes, so a program that is not podnet is named; a `.so` must start with the magic of ELF. Test `a_vnet_that_builds_no_shim_fails_the_dry_run` (`/bin/false`, and a `.so` that is text, which reaches the check of the ELF header) fails on the old code.

# T-451: audit: a failed write still appears in the in-memory lines and the next line reuses its seq

**Source:** GitHub #265 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

An audit line that cannot be written still goes into the in-memory `lines`, and the next line reuses its `seq`. `Audit::write`:

## Premise

The report of #265, with the lines of the code and a measure.

## Approach

Push the entry to `lines` only when `append` succeeds, or mark it as not written and keep the seq/prev unchanged. Add a test that a failed write leaves `lines` without the entry and the next line has the next seq.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: `Audit::write` gives the entry only when it is on the disk: a failed write is in no memory and no copy for the source, the next line takes its `seq`, and one line on stderr names it. Test `a_failed_write_moves_no_chain_and_memory_is_bounded` (with `/dev/full`) fails on the old code.

# T-452: guest: the self-test skips the first unit's file check when the driver reports no unit_root

**Source:** GitHub #266 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The guest self-test skips the first unit's file check when the driver reports no `unit_root`. `guest::observe`:

## Premise

The report of #266, with the lines of the code and a measure.

## Approach

Treat a missing `unit_root` as a failed self-test (or use a known path from the driver's `observe` contract), so the firstfile check always runs. Add a test with a driver that returns `unit_root: None`.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: A driver that names no root of the first unit fails the guest self-test by name (`guest::first_file`): the check of one unit by another must run. Test `a_driver_with_no_root_of_the_first_unit_fails` (the function; no driver here returns no root).

# T-453: status: no_tier_served ignores the egress hold, so it says a tier is served when every offer is refused

**Source:** GitHub #267 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`status.no_tier_served` checks only the boundary floor, not the egress hold that placement also requires. `Slots::no_tier_served`:

## Premise

The report of #267, with the lines of the code and a measure.

## Approach

Include `k.capability.egress_hold.effective() >= needed_hold(s.cfg.tier)` in `no_tier_served`, as `doctor` does. Add a test with a guest kind and no hold.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: `Slots::no_tier_served` also needs the egress hold of the tier, as placement and `tiers::served` (T-465). Test `a_kind_with_no_hold_serves_no_tier` fails on the old code.

# T-454: serve: a spooled report of a source that is no longer configured is skipped forever

**Source:** GitHub #268 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A spooled report whose source is no longer in the configuration is skipped forever. `retry_spool`:

## Premise

The report of #268, with the lines of the code and a measure.

## Approach

Name the orphan on stderr and remove it (or move it to a `spool/orphan/` directory) after a grace period, or refuse to start with an orphan and name the file. Add a test with a spooled report of a source that is not in the config.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: A spooled report of a source that the configuration no longer has goes to `STATE/spool/orphan/` with one line on stderr; it is kept, not sent and not counted. The retry time saturates (a test with the largest time found an overflow). Test `a_report_of_a_source_that_is_gone_goes_to_the_orphans`.

# T-455: serve: a bad journal line is counted in Replay.bad_lines and never reported

**Source:** GitHub #269 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`journal::replay` counts the lines it cannot read in `Replay.bad_lines`, but `Slots::start` never reads that field. A torn or corrupt journal line is silently dropped; the operator sees no line and no count.

## Premise

The report of #269, with the lines of the code and a measure.

## Approach

In `Slots::start`, print a line for `replay.bad_lines` (with the path and the count) and/or put the host in `degraded` until an operator looks. Add a test with a torn journal line.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: `Slots::start` keeps a copy of a journal with lines that could not be read (`journal.jsonl.bad-N`) before the compaction drops them, and names the count and the copy on stderr. Each start with bad lines makes one copy (at most 1000; then the line says `no copy`). Test `a_bad_journal_line_is_kept_and_named`.

# T-456: check: a pin payload with an invalid arch (riscv64, x86-64) is accepted and then refuses every job

**Source:** GitHub #270 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`Config::check` validates the `arch` of each source but not the `arch` of each `[[pin.payload]]`. Measured:

## Premise

The report of #270, with the lines of the code and a measure.

## Approach

In `Config::check`, require each `pin.payload.arch` to be `x86_64` or `aarch64` (and, if a second payload for one architecture is not a rotation, refuse it as #235 suggests). Add a test with `riscv64` and `x86-64`.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: `pins::check` refuses a payload `arch` other than `x86_64` and `aarch64`, the names that placement picks. The table of `a_bad_pin_payload_is_refused_by_name` has `riscv64`, `x86-64`, `amd64`, `X86_64` and an empty one.

# T-457: check: an empty [unit.podvm] command, a relative runner_dir and a bad registry host are accepted

**Source:** GitHub #271 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`Config::check` accepts an empty `[unit.podvm] command`, a relative `runner_dir` and a registry host that is not a host. Measured:

## Premise

The report of #271, with the lines of the code and a measure.

## Approach

In `serve::plan` for `[unit.podvm]`, require `command` non-empty, `runner_dir` absolute (as the runner names it), and each `registries` entry a host name (no space, no slash). Add the three cases above to the `check` tests.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: `podvm_kind::plan` (`check` and `serve`) refuses an empty `command`, a `runner_dir` that is not absolute, and a registry that is not `HOST`, `HOST:PORT` or `[IPv6]:PORT`. Test `the_fields_of_a_podvm_unit_are_checked` (a table).

# T-458: guest: the microvm and incus scripts set HTTP(S)_PROXY but not NODE_USE_ENV_PROXY

**Source:** GitHub #272 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

The guest scripts of the `microvm` and `incus` kinds set the proxy variables of the channel but not `NODE_USE_ENV_PROXY`:

## Premise

The report of #272, with the lines of the code and a measure.

## Approach

Add `NODE_USE_ENV_PROXY=1` to both guest scripts when the channel is set (unless the image or the job sets it), as `proxy::all_env` does. Add it to the guest script tests.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: The guest scripts of the microvm and incus kinds export `NODE_USE_ENV_PROXY=${NODE_USE_ENV_PROXY:-1}` with the proxy of the channel. Tests `the_guest_script_gives_node_the_proxy` (each kind; the proxy lines of the script run in `sh`) fail on the old scripts.

# T-459: incus: the instance name is a 32-bit hash, so two state directories can collide

**Source:** GitHub #273 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The incus instance name is a 32-bit FNV-1a hash of the state directory plus the unit id:

## Premise

The report of #273, with the lines of the code and a measure.

## Approach

Use a 64-bit hash (the name has room: `podrunner-{h:016x}-{unit}` is under 63 characters for a normal unit id) or a random suffix stored in the state directory. Add a test with the two paths above.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: The incus instance name has a 64-bit FNV-1a hash of the state directory. The two paths of #273 are in `the_name_is_one_incus_takes`; it fails on the old hash. A destroy of a replay of the journal also removes the instance of the old 32-bit name, so an upgrade during a job leaves no instance with its credential (a review found it; `a_replay_destroy_removes_the_instance_of_the_old_name`).

# T-460: microvm: destroy of a unit that is not in the map leaves its directory and reports a residue

**Source:** GitHub #274 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`MicrovmUnit::destroy` removes the unit directory only when the unit is in `self.units`. A second call, or a replay of the journal for a unit that is not in the map, takes the `else` branch and returns a residue without removing the directory:

## Premise

The report of #274, with the lines of the code and a measure.

## Approach

In the `else` branch, remove the unit directory (and the `machine/run-PID-*` dirs if the podvm process is known) and count what is left, as the `Some` branch does. Add a test with a unit directory and no `Running` entry.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: `destroy` of a microvm unit that this process does not know ends its tagged processes and removes its directory, as the other kinds. The run directory of its old podvm in the store (`machine/run-PID-*`) stays: its pid is not known after a restart. Test `a_destroy_of_an_unknown_unit_removes_its_directory` fails on the old code.

# T-461: run: a host path with a colon breaks the -v mount of a unit (toolcache, export, container mount)

**Source:** GitHub #275 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A host path with a colon is passed to `-v` without escaping, so the mount is misparsed. Measured with `microvm::run_args` and a work path `/tmp/a:b`:

## Premise

The report of #275, with the lines of the code and a measure.

## Approach

Refuse a host path with a colon before the first job, with the flag and the path, or use a mount syntax that escapes it. Add a test with `/tmp/a:b` for each mount.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: Each driver refuses a host path with a `:` (or not UTF-8) before it builds a `-v` or a `--podvm-channel` of podvm, with a line that names the path and the fix (`podrunner_unit::mounts`): podvm `start` (the socket directory and each mount: toolcache, export), its container mounts, and the unit directory, the engine and the egress of the microvm and incus kinds. Tests `a_host_path_with_a_colon_is_refused` and `a_start_with_a_colon_in_a_host_path_is_refused`. Level: unit tests; the guest kinds run in CI only.

# T-462: microvm: remove_runs_of uses the podvm PID, which can be reused before the run directory is removed

**Source:** GitHub #276 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`MicrovmUnit::destroy` removes the podvm run directories by the PID of the `podvm` child:

## Premise

The report of #276, with the lines of the code and a measure.

## Approach

Name the run directory by the unit (or by a random token) instead of the podvm PID, or remove it before `child.wait` while the PID is still the child's, or check the directory's owner/tag before removing. Add a test with a pre-created `run-{pid}-*` of another unit.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: The run directories of a podvm are listed while its pid is its own (before `kill` and `wait`), and only those are removed (`runs_of`, `remove_runs`). Test `the_runs_of_a_podvm_are_listed_before_its_pid_is_free` (a run of the same pid made after the list stays).

# T-463: hooks: a job container with no entry point is started with tail, which a distroless image does not have

**Source:** GitHub #278 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The hook starts a job container that has no entry point with `tail -f /dev/null`. A distroless or scratch image has no `tail`: the container stops at once, and the job fails before its first step.

## Premise

The report of #278, with the lines of the code.

## Approach

Keep the container alive with a binary that the hook gives (a static sleep in a mount), not with a binary of the image. Add a test with an image that has no `tail`.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: `prepare_job` of the hooks fails at once when the job container does not start or stops at its start, with a cause that names what a job container needs: `tail` and `sh`, as on GitHub (the runner and the container hooks of `actions` start it with `tail -f /dev/null`; a distroless job container fails on GitHub too). Test `a_job_container_that_stops_at_its_start_is_named` fails with the check removed. Not run with a real engine.

# T-464: podvm: a config without [unit.podvm] runner_dir gives the hook policy an empty work_root and refuses every container job

**Source:** GitHub #279 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`serve` sets `work_root` of the hook policy to `runner_dir` or to an empty path. A podvm kind with no `runner_dir` refuses each container job with "the unit has no runner directory", but the podvm driver uses `/home/runner`.

## Premise

The report of #279, with the lines of the code.

## Approach

Use the runner directory of the driver when the configuration has none. Add a test with a podvm kind with no `runner_dir`.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: A podvm kind with no `runner_dir` gives its hook policy the runner directory of its driver, `/home/runner` (`podvm_kind::work_root`). Test `a_podvm_kind_with_no_runner_dir_has_the_work_root_of_the_driver`. Not run with a real podvm unit.

# T-465: check: tiers::served ignores the egress hold, so a config with no hold passes check and serves nothing

**Source:** GitHub #280 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`tiers::served` checks the boundary floor of each source, but not the egress hold that placement needs. `check` passes and `serve` starts, and each offer is refused with `hold-not-available`.

## Premise

The report of #280, with the lines of the code.

## Approach

Also require the hold of the tier, as `doctor` does. Add a test with a trusted source at `redirect` and a podvm kind with no hold.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: `tiers::served` also needs the egress hold of the tier, as placement and `doctor`: a podvm kind has a hold only with `podnet`. The case of `tests/check.rs` (a trusted source, a podvm kind at `confined` with no podnet) fails on the old code. Level: a test of `check` through the binary.

# T-466: shim: the close_range fallback closes only 3..4096, so a higher descriptor leaks into the payload

**Source:** GitHub #281 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

On a kernel with no `close_range`, the shim closes only the descriptors 3 to 4095. A descriptor at 4096 or above goes into the payload.

## Premise

The report of #281, with the lines of the code.

## Approach

Close to the limit of `RLIMIT_NOFILE`, or read `/proc/self/fd`. Add a test of the fallback with a descriptor above 4096.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: With no `close_range`, the shim closes the descriptors up to the soft limit of open files (at least 4096, at most 2^20). The report is right on the bound, not on the effect: the shim closes its own descriptors, not those of the payload; a high pipe of the parent could stay open in the shim. Test `the_fallback_closes_to_the_limit` (the bound only): the host has `close_range`, so the fallback path did not run.

# T-467: mint: the revocation message calls the source name a credential

**Source:** GitHub #282 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

The message of a revoked source says "the credential NAME is revoked", but NAME is the name of the source.

## Premise

The report of #282, with the lines of the code.

## Approach

Say "the source NAME is revoked". Add a test.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: The message is "the source NAME is revoked: renew its credential, then start serve again". Test `a_revocation_is_final_and_names_no_secret` fails on the old text.

# T-468: check: a guest_kinds entry that is not a [unit.NAME] table passes check and drops the guest claim

**Source:** GitHub #283 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`Config::check` does not check that each entry of `guest_kinds` names a `[unit.NAME]` table. A typo passes `check`, and each untrusted job is refused with no line that names the typo.

## Premise

The report of #283, with the lines of the code.

## Approach

Refuse an entry that is not a unit kind, by name. Add a test with `microvmm`.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: `Config::check` refuses an entry of `guest_kinds` that names no `[unit.NAME]`, by name; the default `microvm` stays valid with no microvm kind. Test `a_guest_kind_with_no_unit_is_refused` fails on the old code.

# T-469: lock: an uppercase digest is accepted and then fails every action against the lowercase SHA-256

**Source:** GitHub #284 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`Lock::parse` accepts a digest in uppercase, and the compare with the lowercase SHA-256 of the tarball fails each action.

## Premise

The report of #284, with the lines of the code.

## Approach

Lowercase the digest in `Lock::parse`, as the commit. Add a test.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-18: podcrates `v0.70.0` (PodUtils/podcrates@T-902) keeps each digest of `Lock::parse` in lowercase. podrunner takes the tag. Test `lock::tests::an_uppercase_digest_matches_the_store` (`podrunner-local`) reproduces #284; it fails on `v0.69.0`, whose parser keeps the digest as written.

# T-470: status: trust_unknown counts a repository or owner literally named unknown

**Source:** GitHub #285 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`status.trust_unknown` counts a job when any value of its trust inputs is the text `unknown`, also an owner or a repository with that name.

## Premise

The report of #285, with the lines of the code.

## Approach

Count only the fields with an `unknown` value of their enum. Add a test with the owner `unknown`.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: `trust_unknown` counts only the four fields with an `unknown` value of their enum. The status test with a `ref` and a `workflow_ref` named `unknown` fails on the old code.

# T-471: policy: a host with a trailing dot is not covered by a wildcard ceiling

**Source:** GitHub #286 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`policy::covers("*.github.com", "api.github.com.")` is false: the compare is of text. `narrow` drops the rule.

## Premise

The report of #286, with the lines of the code.

## Approach

Remove one trailing dot of a host before the compare. Add a test.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: `policy::host` removes one trailing dot. Test `a_trailing_dot_names_the_same_host` fails on the old code.

# T-472: check: a pin payload with a duplicate reference, a future built_at_s, max_age_s 0 or a bad runner_version is accepted

**Source:** GitHub #287 (a user who read the code of `main`, 2026-11-17)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

`Config::check` accepts two payloads with one reference, a `built_at_s` in the future, `max_age_s = 0`, and a `runner_version` that is not a version.

## Premise

The report of #287, with the lines of the code.

## Approach

Refuse each of the four, by name. Add a test with each case.

## Prove

`cargo test --workspace`; a test that fails on the code before the fix (`docs/development.md`, "The proof of a change").

## Done

2026-11-17: `pins::check` refuses a duplicate reference, an age limit of 0 (of `[pin]` or of a payload) and a `runner_version` that is not `MAJOR.MINOR.PATCH`. One reference on two architectures is valid (an index digest). A payload built more than one hour after the clock is stale (`Payload::stale`), at placement, where the clock of the engine is; `check` and `serve` name it on stderr (`tiers::future_pins`), and the examples of the documents have a build time in the past. Tests `a_bad_pin_payload_is_refused_by_name` (fails on the old code) and `a_payload_built_after_the_clock_is_named`.

# T-473: Take `pod_core::clock` in place of the crate `podrunner-clock`

**Source:** PodUtils/podcrates#221 (COORD-131, the ruling "Shared code"), 2026-11-17
**Category:** chore
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

The crate podrunner-clock was a copy of the clock of `pod_core::clock`. The ruling "Shared code" asks each project to take the crate of podcrates.

## Premise

The survey of COORD-131, read from the code of `main`.

## Approach

Replace each use of `podrunner_clock` with `pod_core::clock`, keep the manual clock of the tests if podcrates has one (else ask by COORD), and remove the crate.

## Prove

`cargo test --workspace`; the tests of slots with the manual clock pass unchanged.

## Done

2026-11-18: the crate `podrunner-clock` is removed. The slots, the fake source, the signer of the mint, the remote gate and the tests take `pod_core::clock` (`Clock`, `SystemClock`, `ManualClock`). Each holder keeps the instant of its make, and its times in ms count from it, as before. The test bed of the slots takes the Unix time from the wall clock of `ManualClock::at`, so it moves on across a restart of the slots. `cargo test -p podrunner-slots -p podrunner-fake -p podrunner-mint -p podrunner-remote -p podrunner-broker-fake` passes with no change of an assertion, except the end of `the_script_follows_the_manual_clock` (the elapsed time in place of `now_ms`).

# T-474: Take `pod_proxy::env` for the proxy environment of a unit

**Source:** PodUtils/podcrates#221 (COORD-131, the ruling "Shared code"), 2026-11-17
**Category:** chore
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

`crates/podrunner-local/src/exec/proxy.rs` splits `NO_PROXY` by hand and lists the CA variables itself. podcrates has `pod_proxy::env::NoProxy` and `Environment`; the list of CA variables waits for `pod_tls::env_bundle` (`PodUtils/podcrates@T-881`), also for `podrunner-egress/src/dns.rs` (`resolver_via`).

## Premise

The survey of COORD-131, read from the code of `main`.

## Approach

Take `pod_proxy::env` for the proxy variables and `NO_PROXY`. When podcrates gives `pod_tls::env_bundle`, take it in `proxy.rs` and in `dns::resolver_via`. Keep the tests of the proxy-only lane of the user gate.

## Prove

`cargo test --workspace`; the lane `proxy-only-dns` of the user gate passes.

## Done

2026-11-18: the CA variables of a unit are `pod_tls::BUNDLE_VARS`; `dns::resolver_via` takes the bundle of `pod_tls::env_bundle` (the first variable that names a file which opens; each variable passed over gets one line on stderr). The names of `NO_PROXY` come from `pod_proxy::env::NO_PROXY_VARS_UPPER_FIRST`, and the two copies of the merge of `NO_PROXY` (the runner directory, a unit) are one function, `exec::host::no_proxy`. `pod_proxy::env::Environment` takes the first list only; a job gets both lists, so the merge stays. `cargo test -p podrunner-local -p podrunner-egress` passes. The lane `proxy-only-dns` of the user gate is T-371.

# T-475: A `uses:` with an empty part runs on GitHub and is refused here

**Source:** the review of the fix of #261 by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

The runner of GitHub splits `OWNER/REPO/PATH` with no empty parts, so `o//r@v1` and `o/r/dir/@v1` run there. `store::repo_ok` and the job message refuse them.

## Premise

The review read the split of the runner; not measured against GitHub.

## Approach

Measure one such `uses:` in the live workflow. If GitHub runs it, drop the empty parts before the check and in the message, as the runner does.

## Prove

`cargo test --workspace`; the live run of the measure.

# T-476: A `:` in TMPDIR or in an option path fails each job start, not `check`

**Source:** the review of the fix of #275 by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

A host path with a `:` is refused at the start of each job of a guest kind (#275). `check`, `doctor` and the start of `podrunner run` do not name it, so a run starts and each job fails.

## Premise

The review read the code of the drivers.

## Approach

Check the state directory, `TMPDIR`, `--toolcache`, `--export` and the `engine` of a guest kind in `check`, `doctor` and the checks of `podrunner run` (`dry.rs`), with the same line as the drivers.

## Prove

`cargo test --workspace`; a test with `TMPDIR=/tmp/a:b` and `--microvm`.

## Done

2026-11-18: The checks of `podrunner run` (also a dry run) and the check `host-paths` of `doctor --run` name a `:` in TMPDIR, `--toolcache`, `--export` or `--engine` of a unit kind (`dry.rs` `mounted`); `check` and `serve` name it in the `engine` of `[unit.microvm]`, and `serve` in a `--state` of a kind of podvm (`tiers.rs` `state_path`), each with the line of the drivers. Test: `mounted` with a podvm and with a runner directory. Measured: the binary with TMPDIR of a `:` fails the dry run with exit 78, and `doctor --run` gives `FAIL host-paths`.

# T-477: An audit line that fails half way leaves a fragment, and the next line joins it

**Source:** the review of the fix of #265 by a review agent of this session (read only), 2026-11-17
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

When `write_all` of an audit line fails part way (a full disk), part of the line stays in `audit.jsonl` with no end of line. The next line is appended to that fragment, so it cannot be read on the disk, while the source has its copy.

## Premise

The review read `Audit::append`; not measured.

## Approach

Remember the length of the file before a write; on a failure, truncate the file to it (or write a line end before the next line and mark the chain as torn, as `verify` reads it).

## Prove

`cargo test -p podrunner-slots`; a test with a file that takes only part of a line.

## Done

2026-11-18: A write of an audit line remembers the length of the file; when it fails part way, the file is cut back to it. When the cut fails too, the next write ends the part and writes a `resume` line first, so `verify` passes the torn part once (`audit.rs` `append_line`, `mend`). A write that adds nothing (a full device) marks nothing. The test makes a write fail after 12 bytes, with and without the cut; the chain holds each time, and the copy for the source has the `seq` of the disk. On the code before the fix the next line joins the part.

# T-478: The build of the runner image fails when public.ecr.aws refuses the pull of its base image

**Source:** runner-image 44192584888 (`402d7a5`, 2026-11-17): the job `build (x86_64)` failed at "Build the OCI archive" with `429 toomanyrequests: Data limit exceeded` five times
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`images/runner/Dockerfile` starts `FROM public.ecr.aws/docker/library/ubuntu:24.04@sha256:...`. public.ecr.aws can refuse each pull of a shared hosted runner for a time. The five tries of `scripts/retry.sh` (150 s) all failed, and each later step of the job failed for want of the image. `images/runner/Dockerfile.musl` takes alpine from the same registry.

## Premise

The log of the job `build (x86_64, linux/amd64, ubuntu-24.04)` of runner-image 44192584888.

## Approach

Give the registry of the base image as `ARG BASE_REGISTRY`; the digest pins the content. `scripts/build-image.sh` tries public.ecr.aws, mirror.gcr.io and Docker Hub, in three rounds.

## Prove

The steps "Build the OCI archive" and "The larger runner image by label" of `runner-image.yml`, and the build of `runner-musl.yml`.

## Done

2026-11-17: both Dockerfiles start `ARG BASE_REGISTRY=public.ecr.aws/docker` and `FROM ${BASE_REGISTRY}/library/...@sha256:...`; mirror.gcr.io serves both digests (checked: 200 for each index). `scripts/build-image.sh` builds from the first registry that serves the image and names it; a test with a fake `docker` gives mirror.gcr.io after a refusal of public.ecr.aws, and exit 1 with a line when each registry fails. The three builds use it. Not run yet in CI.

2026-11-18: the same class hit the pull of Alpine in `runner-musl.yml` (`toomanyrequests` on `8ec7d87`). `scripts/pull-base.sh` pulls a base image pinned by its digest from the first of public.ecr.aws, mirror.gcr.io and Docker Hub that serves it, and prints the reference; the Alpine step runs it. Proven with a fake `docker` that refuses public.ecr.aws (the digest comes from mirror.gcr.io), one that refuses each registry (exit 1), and a reference that is not pinned (exit 2).


# T-479: A JSON Schema of `podrunner.toml` for a program that writes the file

**Source:** GitHub #292 (cfb, P-1), 2026-11-18
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

cfb asked for a schema of `podrunner.toml`, as the schema of `status.v1` is for `status --json`, so that a renderer can check its output. By cfb's R-053 (GitHub #292), githome renders the file and bootstraps the host, not cfb: the schema serves githome. No schema exists: the contract is `docs/configuration.md` and the parser (`crates/podrunner-slots/src/config.rs`). `docs/non-goals.md` lists "A schema for `podrunner.toml`" with the trigger "a second reader of the file exists"; githome's renderer meets it.

## Premise

The issue of cfb, read on 2026-11-18. The parser refuses an unknown key (`deny_unknown_fields`); `podrunner check --config FILE` is the validator with no side effect.

## Approach

1. Derive `schemars::JsonSchema` on the types of the configuration (a dev-dependency or a feature, as `pod-job` does), and write the schema file `config.v1.json` into `schemas/` from them.
2. A test fails when the file differs from the types, and a test validates `docs/examples/podrunner.toml` and each block of `docs/configuration.md` against it.
3. The rules that a schema cannot hold (a floor, the pin list, the modes of files) stay in `check`; the schema names `check` as the last word.
4. Move the row out of `docs/non-goals.md` ("Triggers that are met").

## Prove

`cargo test -p podrunner-slots` with the new tests; a planted key that the parser refuses fails the schema test.

## Done

2026-11-18: `schemas/config.v1.json` (draft 2020-12) comes from the types of the parser: each type of `crates/podrunner-slots/src/config.rs`, `policy.rs` (`EgressConfig`) and `trust.rs` derives `schemars::JsonSchema` in the tests only, and `podrunner-tier` has a feature `schema` for its lists; the binaries carry no schemars. `cargo test -p podrunner-slots config_schema` fails when the file differs from the types (`PODRUNNER_WRITE_SCHEMAS=1` writes it). `scripts/check-config-schema.py --self-test` validates `docs/examples/podrunner.toml` and each `toml` block of `docs/configuration.md` (a block may leave out `host` and `pin`; the file of the mint is marked as another file), and a planted unknown key and a planted wrong type fail; the gate runs it in the venv of its design step, and `check.sh` where python3 has `jsonschema`. The floors, the pin list and the modes of the files stay in `check`. The row of `docs/non-goals.md` names it.

# T-480: A cache directory of the host, kept apart by trust (cfb and githome G-07)

**Source:** GitHub #292 (cfb, P-3), 2026-11-18
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** L
**Status:** open

## Problem

githome's G-20 (GitHub #292, 2026-11-18) asks the same, in this shape: an offer field `cache: { key, mode }` with `mode` `rw` (a push to the default branch), `ro` (a pull request) or `none`, set by the source; one directory for each source, key and tier at a fixed path; `npm_config_cache`, `PNPM_STORE_DIR`, `BUN_INSTALL_CACHE_DIR`, `PLAYWRIGHT_BROWSERS_PATH` and `RUNNER_TOOL_CACHE` from a `[cache]` table; a size cap for each key with the oldest evicted; the bytes read and written in the report (G-19). The gates of cfb's projects use bun, pnpm, npm and Playwright. Each job starts in a clean unit and fetches each package again. cfb asks for a cache directory on the host for `serve`: read-write for the jobs of the main branch, read-only for a pull request. Shared caches are a non-goal of v1, with the trigger "a measure shows that the cost of a start or a fetch matters" (`docs/non-goals.md`). githome's G-07 sets the rule: a unit of a lower trust never writes what a unit of a higher trust reads.

## Premise

The crate half is githome's G-32 on PodUtils/podcrates#223 (the fields of `pod-job`), which podcrates takes under COORD-134. The issue of cfb, read on 2026-11-18. Not measured.

## Approach

1. Take cfb's measure for each gate as the baseline: `claim_to_first_step_s` of the run report, the time of the install steps on a cold unit, and the time with a restore. The ruling of 2026-11-18 lifts the trigger of the non-goal, so the work does not wait for the measure.
2. A cache directory for each repository and trust class on the host, mounted read-write only into a trusted job of the default branch, read-only into each other trusted job, and into no untrusted job until a decision says otherwise. Each kind needs its own mount (a disk device of incus, a 9p mount of the guest).
3. A test: a file that an untrusted job writes is absent from the cache of the next trusted job.

The census of T-491 (2026-11-18): no gate of cfb needs bun (Q-13). The base line is the row of T-528.

## Decision

Each cache is scoped by source, forge, repository, tier and `key`: no cache is shared between two repositories. A job of the untrusted tier (a fork, an agent) gets no cache (`none` is forced). `rw` only for a push to the default branch, `ro` for each other trusted job. (the question Q-5 of podrunner on GitHub #292, 2026-11-18T07:25Z; githome went away at 07:22Z with no answer, so podrunner takes its recommendation)

## Prove

The test of G-07 above, on each kind in `runner-image.yml`, and cfb's measure again with the cache.

# T-481: The checkout of a job on the incus kind is proven by a test of the script only

**Source:** GitHub #292 (cfb, P-4), 2026-11-18
**Category:** measurement
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** partial

## Problem

T-178 fixed `HOME=/root` for the runner on the `incus` and `microvm` kinds. CI proves `actions/checkout` in `microvm` guests (the parity suite of `runner-image.yml`). On `incus`, only `tests::the_payload_has_the_home_of_the_runner` proves it, with stubs. cfb plans the `incus` kind for each trusted job, and githome has not run a checkout there.

## Premise

The Done section of T-178, read on 2026-11-18.

## Approach

Add a job with `actions/checkout` to the suite of the incus kind on real instances in `runner-image.yml`.

## Prove

The step of the incus suite in `runner-image.yml` passes, and it fails with the guest script of before T-178.

In the code (2026-11-19): the parity suite takes `PODRUNNER_PARITY_ROUTES=0` (a unit with no container route skips those workflows), and a step of `runner-image.yml` runs it on the incus kind with no engine, as a measure: `06-actions.yml` (the real `actions/checkout@v4`) must give PASS. Left: the green run, then the step blocks; the plant of the Prove (the guest script of before T-178).

State (2026-11-19, `runner-image.yml` run 44429314715, `b9128b2`): on the incus kind with no engine, `01`, `02`, `03`, `06-actions.yml` (the real `actions/checkout@v4` in an instance), `07` and `08` PASS, the three workflows of containers SKIP, and both plants fail. Proven in CI on GitHub-hosted runners. Left: the plant of the Prove (the guest script of before T-178), then the step blocks.

# T-482: The incus kind bounds no disk of the root of an instance

**Source:** GitHub #292 (cfb, P-5), 2026-11-18
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** partial

## Problem

The `disk` limit of an offer, else `[unit.KIND] disk_max_bytes`, bounds the bytes under the work directory of a unit (`crates/podrunner-slots/src/disk.rs`; `observe` gives the work directory as `unit_root`). An instance of the `incus` kind has its own root file system in the storage pool of incusd, and the driver sets no size of it (`crates/podrunner-incus/src/guest.rs` passes `--memory`, `--cpus` and `--pids-limit` only). A job that writes into `$HOME` or `/tmp` of the instance (a package cache) can fill the pool of the host, and no `limit` ends it. cfb plans 40 GB for each unit.

## Premise

Read from the code on 2026-11-18. Not measured on an instance.

## Approach

1. Measure: a job on the incus kind writes 1 GiB into `/tmp` of the instance with `disk_max_bytes` of 100 MiB; record whether the job ends.
2. Find whether podvm's incus tier sets a size of the root disk device (incus `size` of the root device). If yes, pass the lesser of `disk` of the offer and `disk_max_bytes`. If no, ask podvm by a COORD issue.

## Prove

The measure of step 1 on real instances in `runner-image.yml`: the job ends with the exit reason `limit`.

In the code (2026-11-18): podvm `0.2.0-beta.33` sizes the root disk of an instance (`run -d --podvm-tier=incus --storage-opt size=N`, COORD-187, PodUtils/podvm#583) and refuses the size on a pool with no quota (the `dir` driver, exit 125). `[unit.incus] root_quota = true` (the operator says that the pool holds a size) gives each instance a root disk of the lesser of `disk_max_bytes` and the `disk` of the offer (64 MiB at least), and the kind says `disk: enforced`; with no `root_quota` the bound stays the measure of `disk.rs` (`crates/podrunner-incus/src/guest.rs`, `crates/podrunner/src/incus_kind.rs`, `docs/configuration.md`; call of 2026-11-18 in `docs/decisions.md`). Test: `cargo test -p podrunner-incus a_pool_with_quotas_gives_each_instance_a_sized_root`. Open: step 1, the measure on a real instance in `runner-image.yml` (the cloud host of podvm has a `dir` pool only; a pool of `btrfs` on a loop file is a way to measure ENOSPC there).


# T-483: The incus kind reports `unknown` for the CPU time and the peak memory of a job

**Source:** GitHub #292 (cfb, P-7, and githome), 2026-11-18
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** partial

## Problem

`crates/podrunner-slots/src/usage.rs` samples the processes of a unit on the host. The processes of an `incus` instance belong to incusd, so its report gives `unknown` for `cpu_s` and `peak_rss_mib`. githome's G-06 asks for both on container and microVM units, and cfb shows the CI time of each project from them. cfb plans the `incus` kind for each trusted job.

## Premise

Read from the code and from `docs/HANDOFF.md` on 2026-11-18.

## Approach

Read the cgroup of the instance (the CPU time and the peak memory that incus gives in the state of an instance, or `cpu.stat` and `memory.peak` of its cgroup through podvm) at each tick and at the end. Where podvm gives no way, ask podvm by a COORD issue.

## Prove

A job on real instances in `runner-image.yml` that burns a known CPU time gives `cpu_s` within the tolerance of the schema, and a `peak_rss_mib` that is not `unknown`.

In the code (2026-11-18): podvm `0.2.0-beta.33` gives `CpuSeconds` and `PeakMemoryBytes` in `inspect` of an incus unit (COORD-187, PodUtils/podvm#583). The unit seam has `UnitDriver::usage` (none by default); the incus kind answers it from `inspect`; slots asks it before the destroy, and the report takes it over the samples of the host (`cpu_s` rounded up, `peak_rss_mib` in MiB rounded up; `crates/podrunner-slots/src/usage.rs`, `run.rs`). Tests: `cargo test -p podrunner-incus the_use_of_an_instance_comes_from_incusd`, `cargo test -p podrunner-slots the_use_of_the_kind_holds`. Open: the measure of the Prove on real instances.

# T-484: A release has no `SHA256SUMS` file with a cosign bundle, as the other projects of PodUtils have

**Source:** GitHub #292 (cfb, "Release files"; PodUtils/podcrates#223, C-2), 2026-11-18
**Category:** release
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** partial

## Problem

podssh, podnet, podvm and podcrates sign one `SHA256SUMS` file of each release with a cosign bundle. A release of podrunner has one `.sha256` file for each asset and a build provenance attestation of each binary, the installer, the bundles and the runner image archives (`.github/workflows/release.yml`), but no `SHA256SUMS` and no cosign bundle. cfb checks the sums file of each other project with one procedure, and cannot do so for podrunner.

## Premise

`.github/workflows/release.yml`, read on 2026-11-18, the note of cfb on GitHub #292, and the answer of podcrates to C-2 of PodUtils/podcrates#223: podssh, podnet, podvm and podcrates publish `SHA256SUMS` with a keyless cosign bundle `SHA256SUMS.sigstore.json`.

## Approach

1. In `release.yml`, write `SHA256SUMS` of each asset, and sign it with `cosign sign-blob --bundle SHA256SUMS.sigstore.json` (keyless, the identity of the workflow), with the file names of the other projects; keep the `.sha256` files and the attestations.
2. Name the check in `docs/install.md`: `cosign verify-blob --bundle SHA256SUMS.sigstore.json --certificate-identity-regexp '^https://github.com/PodUtils/podrunner/' --certificate-oidc-issuer https://token.actions.githubusercontent.com SHA256SUMS`, then `sha256sum -c SHA256SUMS`.
3. The installer can check the sums file when `cosign` exists.

## Prove

A dry run of `release.yml` makes both files, and the step of the check passes on them and fails on a changed `SHA256SUMS`.

In the code (2026-11-18): `sh scripts/release-assets.sh sums DIR` writes `DIR/SHA256SUMS` of each asset (not the `.sha256` files; case 12 of `sh scripts/test-install.sh` checks it). The job `release` of `release.yml` writes it over `dist`, signs it with `cosign sign-blob --bundle SHA256SUMS.sigstore.json` (the cosign installer and `v3.1.4` of podcrates), checks the signature with the identity of the workflow at the ref of the dispatch, and checks that a changed copy fails; both files go into the release. `docs/install.md` names the check. Left: the first release that runs it (the signing needs the token of a release run), and step 3 (the installer checks the sums file when `cosign` exists).

# T-485: The incus kind has no container route, though G-02 asks for an engine in the system container

**Source:** a question of the owner on the answer to GitHub #292 (P-4), 2026-11-18
**Category:** feature
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

githome's G-21 (GitHub #292, 2026-11-18) asks for it: `services:`, `container:`, Docker actions and `docker build` in an `incus` unit, for trusted jobs on a host with no `/dev/kvm`, with the same `[unit.KIND] engine` as the `microvm` kind and the images from the store. The `incus` kind says `container: none` (`crates/podrunner-incus/src/lib.rs`) and refuses `[unit.incus] engine` (`crates/podrunner/src/incus_kind.rs`). So a trusted job with `container:`, `services:` or a `docker://` step on a host whose trusted tier has only the incus kind is refused with `container-not-supported`. githome's G-02 asks for a system container "with its own init, file system, network and container engine", and `docs/container-engine.md` names a system container of incus as a place for the engine. Only the engine of the `microvm` kind exists (T-103). The record gives no reason for the refusal. This is a limit of podrunner, not of Incus.

## Premise

githome's owner (GitHub #292, 2026-11-18T06:15Z): githome has no interim rule for GitHub jobs; after T-493 and T-494 the trusted jobs of cfb's five repositories go to the `incus` kind, so a trusted job with containers needs this entry, or a `microvm` guest with `engine` on the same host. Read from the code and the documents on 2026-11-18. Not measured.

## Approach

1. Measure on real instances: `dockerd` in a system container of podvm's incus tier with no NIC and one channel (nesting, cgroups, overlay, the pull through the egress channel).
2. Decide what nesting does to the claim `confined`, and record the call in `docs/decisions.md`.
3. If the claim holds: `[unit.incus] engine = DIR`, the kind says `container: engine`, and the parity suite of the engine runs on incus. If podvm must change, ask podvm by a COORD issue.

## Decision

If an engine in an instance cannot keep `confined`: by default a trusted job with containers on a host with no `/dev/kvm` is refused by name (`container-not-supported`, with the reason); an opt-in of the operator (`[unit.incus] engine` with an explicit `boundary_with_engine`) runs it at the lower boundary, and the report names that boundary. (the question Q-6 of podrunner on GitHub #292, 2026-11-18T07:25Z; githome went away at 07:22Z with no answer, so podrunner takes its recommendation)

## Prove

The parity suite of the engine (`services:`, a job container, a Docker action, a Dockerfile action) passes on real incus instances in `runner-image.yml`.

In the code (2026-11-18): podvm `0.2.0-beta.32` runs an engine in an incus instance (`--podvm-engine`, COORD-187 ask 3, PodUtils/podvm#583; measured by podvm with `docker info` in an unprivileged instance with no NIC). `[unit.incus] engine = DIR` gives each instance `--podvm-engine` and the directory read-only; the script starts `dockerd` with no bridge and no iptables and gives its socket as `DOCKER_HOST`; the record says `container: engine`; `check` refuses a directory with no `dockerd` and a podvm before `0.2.0-beta.32` (`crates/podrunner-incus/src/guest.rs`, `lib.rs`, `crates/podrunner/src/incus_kind.rs`, `docs/configuration.md`). The call on `confined` is in `docs/decisions.md`. Tests: `cargo test -p podrunner-incus an_engine_directory_gives_each_instance_an_engine`, `cargo test -p podrunner --test check`; `the_runner_user_runs_a_container_on_the_engine_of_its_instance` (`cargo test -p podrunner-incus --test engine`, a step of `runner-image.yml`) runs a container on real instances. Open: its green run, the network of the engine (`services:` needs one), the parity suite on incus with an engine, and `--incus --engine` in the local mode (`--engine needs --microvm` now).

State (2026-11-18): `runner-image.yml` run 44349725647 (`e6beea0`): `the_runner_user_runs_a_container_on_the_engine_of_its_instance` passed on a real incus instance (the user `runner`, `DOCKER_HOST`, a container gave `engine-42`), and `docker network create` worked in the instance. Open: the parity suite on incus with an engine (`services:`), and `--incus --engine` in the local mode.

In the code (2026-11-19): `podrunner run --incus PODVM --engine DIR` gives each instance the engine (the local mode builds the unit as `serve` does; `doctor --run` needs podvm `0.2.0-beta.32` with an engine; a job with containers on `--incus` with no engine fails by that cause); the step "The parity suite on the incus kind" of `runner-image.yml` gives the engine when the engine directory is there, so `04-services.yml`, `05-containers.yml` and `09-dockerfile.yml` run in an instance. Left: the green run of that step.

State (2026-11-18, later): run 44376274000 (`2041d27`): the engine test passed again; it blocks from the next commit.

## Done

2026-11-19: `runner-image.yml` run 44431209673 (`b24f5a3`): the parity suite on the incus kind with the engine of the guest engine step gives PASS for each of the 9 workflows, also `04-services.yml` (`services:`), `05-containers.yml` (a job container) and `09-dockerfile.yml` (a Dockerfile action), and both plants fail. `podrunner run --incus PODVM --engine DIR` gives the engine to the local mode. The step blocks now. Level: proven in CI on GitHub-hosted runners.

# T-486: G-04: the egress verdicts of a unit do not reach the source of the job

**Source:** githome on GitHub #66 (the live run of 2026-11-15: "The audit lines of a unit do not reach the source yet") and on GitHub #151; the ruling of the owner of 2026-11-18 (each need of githome and cfb)
**Category:** feature
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

githome stores the audit lines of each job (`job_id`, `host`, `port`, `verdict`, `bytes`, `rule`, `at`), the first 1000 of a job, and shows them on the job page (G-04). slots sends its own audit lines to the source (`crates/podrunner-slots/src/audit.rs`), but the verdicts of the egress module of a unit (each connection, allowed or denied, with its host, port, bytes and rule) go only to `tracing` and to the stderr of `serve` (`crates/podrunner-egress/src/denials.rs`). The source sees no egress verdict.

## Premise

The crate half is githome's G-33 on PodUtils/podcrates#223 (the audit lines of `pod-broker` to the source), which podcrates takes under COORD-134. Read from the code and from the comments of githome on 2026-11-18. Not measured live.

## Approach

1. Take the audit lines of `pod-broker` for each job (the sink of the module), and give each one to `audit` of the source of the job, in the form that githome stores, with no data of a request beyond the host, the port, the verdict, the bytes and the rule.
2. A cap for each job (githome keeps 1000), and a count of the lines that the cap dropped.
3. The run report keeps its form.

## Decision

`{ job_id, host, port, verdict: "allow" | "deny" | "fail", rule, bytes_in, bytes_out, at }`, `at` in RFC 3339 UTC, one line when a connection ends; at most 1000 lines for each job, then one line `{ job_id, dropped: N }`. No path, header or body of a request. (the question Q-4 of podrunner on GitHub #292, 2026-11-18T07:25Z; githome went away at 07:22Z with no answer, so podrunner takes its recommendation)

## Prove

A test of slots with a fake source and a fake egress sink: an allowed and a denied connection of a job give two `audit` calls to its source, and no call to another source. Then githome's pair run shows the lines on the job page.

## Done

2026-11-18: the sink of the egress module keeps the line of each connection that ends (`crates/podrunner-egress/src/lines.rs`: host, port, verdict `allow`, `deny` or `fail`, rule, bytes in and out, the time; a queue of 10000 lines at most), `serve` gives them to the slots through `EgressPort::lines`, and the slots send each one to `audit` of the source of the job of the unit, with its `job_id`, at most 1000 for a job, then one line `{ job_id, dropped }` (`crates/podrunner-slots/src/egress_lines.rs`). `docs/contracts/source-driver.md` names the form. Tests: `each_connection_gives_one_line_with_no_path` (`podrunner-egress`), `the_egress_lines_of_a_unit_reach_the_source_of_its_job` and `a_job_gets_at_most_1000_lines_and_the_count_of_the_rest` (`podrunner-slots`; the other source gets none). Level: proven by tests here; githome's pair run (its job page) is not run, and T-512 holds it.


# T-487: G-11: warm units from a pinned golden image

**Source:** githome's G-11 (GitHub #66) and cfb's A-X-01 ("a warm golden image", "warm pool"; GitHub #65); the ruling of the owner of 2026-11-18 (each need of githome and cfb)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** L
**Status:** open

## Problem

githome's G-25 (GitHub #292, 2026-11-18) asks for a warm pool size for each kind in the configuration, and the start time of a warm and of a cold unit in the report (G-19). Each job waits for a fresh unit to start: podvm, a guest of the machine tier (about 30 s), or an incus instance. githome (G-11) and cfb (A-X-01) ask for units that start from a pinned golden image and wait warm, so that the time from claim to first step is short. `docs/non-goals.md` lists warm units as a non-goal of v1; the ruling of 2026-11-18 lifts it.

## Premise

Read from GitHub #65 and #66 on 2026-11-18. Not measured: the time of a cold start of each kind on cfb's host (`claim_to_first_step_s`).

## Approach

1. Measure `claim_to_first_step_s` of each kind in `runner-image.yml` and on cfb's first host.
2. Design: a warm unit is fresh (no job ran in it), from the pinned image, with no credential and no egress until a claim; each unit still runs one job and is destroyed. Decide the count of warm units for each kind and what a warm unit holds of the host (memory of a guest).
3. Build it behind `UnitDriver` (a `prepare` before the claim), with the unit tests of a fresh unit.

## Prove

The measure of step 1 before and after (`claim_to_first_step_s` of the run report); the conformance suite of `crates/podrunner-unit` on a warm unit (no file of an earlier job, no credential before the mint).

# T-488: G-12: the microvm kind enforces no limit of processes

**Source:** githome's G-12 (GitHub #66); the ruling of the owner of 2026-11-18 (each need of githome and cfb)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

G-12 asks for limits of CPU, memory and processes on container and microVM units. The `microvm` kind applies the memory and the vCPUs of its guest, but refuses `pids` (`docs/configuration.md`, "The limits of a unit"). The jobs of forks run on the `microvm` kind, and so do the trusted jobs with containers until T-485: none of them has a limit of processes.

## Premise

Read from `docs/configuration.md` on 2026-11-18.

## Approach

Apply `pids` in the guest: the guest script sets a cgroup `pids.max` (or `RLIMIT_NPROC` of `runner`) before the runner starts. If the guest of podvm gives no way, ask podvm by a COORD issue.

podvm's answer (COORD-187, PodUtils/podvm#583, 2026-11-18): the init of each guest mounts cgroup2 on `/sys/fs/cgroup` (its PodUtils/podvm@T-1780), so the guest script can write `+pids` to `cgroup.subtree_control`, make a child, write `pids.max` and move the runner into it now; podvm adds `--pids-limit N` on the machine tier later (its PodUtils/podvm@T-1967).

## Prove

A job on real guests in `runner-image.yml` that forks past `pids` fails, and the guest and the host stay up.

In the code (2026-11-18): podvm `0.2.0-beta.32` holds `--pids-limit N` on the machine tier (COORD-187 ask 4, PodUtils/podvm#583). `[unit.microvm] pids` gives each guest `--pids-limit` of the lesser of it and the `pids` of the offer, and the record says `pids: enforced`; `check` asks podvm `0.2.0-beta.32` or later for it (`crates/podrunner-microvm/src/guest.rs`, `lib.rs`, `crates/podrunner/src/microvm_kind.rs`, `docs/configuration.md`). Tests: `cargo test -p podrunner-microvm a_limit_of_processes_goes_to_the_guest`, `cargo test -p podrunner --test check`; `a_payload_past_the_pids_limit_of_a_guest_gets_an_error` (`cargo test -p podrunner-microvm --test pids`, a step of `runner-image.yml`). Open: its green run.

State (2026-11-18): `runner-image.yml` run 44349725647: the test failed before the limit: it sized the guest at 1 GiB, and the initrd of the runner image (892 MiB) needs about 3.3 GiB, so the control run ended with 125. The test now keeps the size of the kind. Next: its run.

## Done

2026-11-18, `9da2056` and `f66eef5` (the test keeps the size of the kind). `runner-image.yml` run 44376274000 (`2041d27`): `a_payload_past_the_pids_limit_of_a_guest_gets_an_error` passed on real guests: 300 forks with no limit end with 12 (each fork passed); under `pids = 64` they end with 2 (a fork failed), and the guest and the host stayed up. The step is blocking from the next commit. Level: proven in CI on GitHub-hosted runners.


# T-489: githome and cfb read `cpu_s` and `peak_rss_mib`: a change of a field of the run report needs a new version

**Source:** githome and cfb on GitHub #292 (P-7); the ruling of the owner of 2026-11-18 (each need of githome and cfb)
**Category:** chore
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** partial

## Problem

githome stores `cpu_s` and `peak_rss_mib` of each `run-report.v1`, and cfb shows the CI time of each project from them. Both ask that the fields stay in a versioned schema. The schema belongs to podcrates (`pod-job`), and the ruling of 2026-11-05 allows a change of a field in any version.

## Premise

GitHub #292, read on 2026-11-18. cfb pins a version and needs no promise; githome asks for one.

## Approach

Ask podcrates (COORD) that a change of a field of `run-report.v1` comes as a new schema id, with a COORD issue to all; name each change of the report in the notes of the podrunner release that takes it.

## Prove

The answer of podcrates, and a line in `docs/releases/` of the next release that names the rule.

State (2026-11-18): podcrates answered yes (COORD-134, PodUtils/podcrates#225; the README of `pod-job` of podcrates, "Versions of a record"): a field that changes its type, name, unit or meaning, a removed field and a new required field give a new schema id (`NAME.v2`) and a COORD issue `(to all)`; an added optional field keeps the id, and the hand-off COORD issue of its tag names it. Left: the line in the notes of the next release of podrunner.

# T-490: Settle each open unknown of `docs/unknowns.md`

**Source:** the ruling of the owner of 2026-11-18 ("researching/lifting all current unknowns+limits")
**Category:** research
**Milestone:** later
**Priority:** P1
**Effort:** L
**Status:** done

## Problem

`docs/unknowns.md` holds open unknowns: U6, U7, U8, U11, U12, U14, U17, U18, U20, U22 and U23 wait for a live job (T-040); U9 is not measured on the long poll of github.com; U16 is deferred; U26 (a job of another architecture in an emulated guest) is "no for now". githome and cfb build on the answers.

## Premise

`docs/unknowns.md`, read on 2026-11-18. The live runs that can settle them: LIVE-1 and LIVE-2 (GitHub #74, #75), githome's pair run (GitHub #151) and the acceptance run of A-X-01 on cfb's first host (GitHub #292, C1 to C5).

## Approach

1. For each open unknown, name the run that settles it (a live gate, githome's pair, cfb's host, or a measure in CI), and give that run a step that records it.
2. Measure in CI each one that needs no live forge.
3. U26: ask podvm for the state of the foreign guest (PodUtils/podvm#366).
4. Write each answer into `docs/unknowns.md`, and an entry for each defect that an answer shows.

podvm's answer for U26 (COORD-187, PodUtils/podvm#583, 2026-11-18): an aarch64 guest on an x86_64 host runs since podvm `0.2.0-beta.22` (`run --podvm-tier=machine --platform linux/arm64`, TCG, no KVM); the reverse is not measured; no measure of the cost of a CI job yet.

## Prove

`docs/unknowns.md` has no row in the state "Open" without a named run and date.

## Done

2026-11-18, the commit that names T-490. Each open row of `docs/unknowns.md` names its run and the date: U6, U7, U8, U11, U12, U14, U17, U18, U20, U22 and U23 the live gates LIVE-1 and LIVE-2 (GitHub #74, #75) on the next beta and githome's pair run (GitHub #151); U9 the long poll of github.com in LIVE-2 with podnet `v0.4.0-beta.17` (T-179). U26 is settled for a guest: `runner-image.yml` run 44302792725 (podvm `0.2.0-beta.32`) booted `busybox:1.37.0` for `linux/arm64` on x86_64 in 10 s, `uname -m` `aarch64`; the job in such a guest is T-533. U16 stays deferred (no shim of the Docker API). A defect that the read of that run found is fixed: the probe of the runner version of `scripts/measure-machine.sh` ran `run.sh --version`, which starts a run and aborts with no configuration (exit 134), and printed the exit of `sed`; it runs `Runner.Listener --version` now and prints its exit. `sh scripts/shellcheck.sh scripts/measure-machine.sh scripts/measure-foreign.sh`: clean.

# T-491: Lift each limit and non-goal that a need of githome or cfb names

**Source:** the ruling of the owner of 2026-11-18 ("accommodate every need & asks", "researching/lifting all current unknowns+limits")
**Category:** research
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

podrunner has limits that block a need of githome or cfb, and `docs/non-goals.md` lists items that their asks name. Examples: no container route on the incus kind (T-485); no disk bound of the root of an instance (T-482); no measure of CPU and memory on incus (T-483); no process limit on microvm (T-488); `cpus` on the podvm kind only with a delegated cgroup; no trusted tier on an emulated aarch64 host; shared caches (T-480); warm units (T-487); `metrics` (cfb A-X-01: "metrics as JSON"); the bridges of Forgejo and GitLab (cfb A-X-01: "later backends"); `podrunner image build` and a JSON API on a Unix socket (cfb A-X-01, A-X-15) against the current command line.

## Premise

`docs/non-goals.md`, `docs/configuration.md` and the full text of A-X-01 and of G-01 to G-14 (GitHub #65, #66), read on 2026-11-18.

## Approach

1. A census: each ask of A-X-01 and G-01 to G-14, clause by clause, against the code and the release (done, partial, missing), with the level of proof.
2. One entry for each gap that has none, in the order of the needs of githome and cfb.
3. Move each lifted row of `docs/non-goals.md` to "Triggers that are met", with its entry.

## Decision

cfb's answer to Q-12 (GitHub #292, 2026-11-18): of A-X-01, (a) metrics as JSON beyond `status --json` and the run report: no; (b) `podrunner image build`: yes, as T-500; (c) a JSON API on a Unix socket beyond the control socket: no; (d) the runner backends of Forgejo and GitLab: no for this milestone (a project on such a forge comes back later as an ask through githome, cfb's R-054). The rest of A-X-01 stands. githome's G-41 (T-512): githome's milestone closes when each ask G-01 to G-41 is done in a release.

## Prove

The census table in this entry; each gap has an entry; `docs/non-goals.md` names no item that a need of githome or cfb names without an entry.

## Done

2026-11-18: two read-only agents made the census of each clause of G-01 to G-41 and of cfb's A-X-01, A-V-11, P-1 to P-8, R-051 to R-054 and C1 to C5, against `main` at `d72b161` and `v0.1.0-beta.7`; the lead session checked each claimed defect in the code. The tables are in `docs/reports/census-2026-11-18.md`. No ask of G-01 to G-41 is done in full in a release. The gaps with no entry are T-514 to T-528; a defect that it found is fixed (T-529). Notes went to T-480, T-492, T-497, T-498, T-500, T-502 and T-510. The asks to other projects: podvm by COORD-089 (GitHub #296: T-482, T-483, T-485, T-488, T-490, T-496, T-515); podcrates by an amend of COORD-087 (GitHub #294: `arch`, T-514); the rest wait for work of podcrates that is in its order already (T-509, T-511, T-523, T-524). `docs/non-goals.md` moves each row that a need names to "Triggers met", with its entry. A claim of the census was wrong and is corrected in the report: a cancel never meets `Claimed` (T-502).

# T-492: The trackers of githome and cfb and the state pages name an old release

**Source:** the session start of 2026-11-18
**Category:** docs
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The bodies of GitHub #65 and #66 say "State read: 2026-11-15, at podrunner `v0.1.0-beta.6`"; the G-12 line of #66 calls T-162 partial, which is done, and the G-14 line names a run on beta.6. `docs/configuration.md` says that the microvm kind says `container: none` until podcrates gives `engine`; the code says `engine` with an engine. A reader of githome or cfb reads an old state.

## Premise

GitHub #65 and #66 and `docs/configuration.md`, read on 2026-11-18.

## Approach

Write the hand-written part of #65 and #66 from the record and the census of T-491 (the generated block stays), and fix the line of `docs/configuration.md`. Change the bodies at each change of state of an ask.

The census of T-491 (2026-11-18): `docs/container-engine.md` steps 3.3 and 4 and `docs/configuration.md` still say `container: none` for a kind that has an engine; the body of GitHub #65 says that A-X-01 waits for #74 and #75, where by R-053 it waits for githome's C1 to C5.

## Prove

Each line of #65 and #66 names the current release and the entry of its ask; the test of `docs/configuration.md` passes.

## Done

2026-11-18, the commit that names T-492. The hand-written parts of GitHub #65 and #66 are written from the record: the state read at `main` `a817f6f` with the last release `v0.1.0-beta.7` and podvm `v0.2.0-beta.33`, `CHANGES.md` for the changes since; each line names the entries of its ask, what is on `main` and what is open; G-07 and G-11 are open entries (T-480, T-487) by the owner's ruling of 2026-11-18; A-X-01 waits for githome's C1 to C5 (R-053). `docs/configuration.md` and `docs/container-engine.md` say `container: engine` for a kind with an engine. `cargo test -p podrunner --test check`: 4 passed. Rule: amend the bodies at each change of state of an ask.

# T-493: G-15: the job-started hook ends a job that GitHub gave to a runner of a program source, when its run is not the run of the offer

**Source:** githome's G-15 on GitHub #292 (2026-11-18); the ruling of the owner of 2026-11-18 (each need of githome and cfb)
**Category:** defect
**Milestone:** later
**Priority:** P0
**Effort:** M
**Status:** done

## Problem

githome's GitHub path (cfb's R-051) mints a GitHub just-in-time configuration through a `program` source. GitHub, not the source, chooses which queued job of the repository a runner takes: each queued job with the labels of the runner can go to it, also a job of another run. The job-started hook of the runner image (T-175) ends a job whose run id is not the run id of the offer, so a correct job of GitHub fails.

## Premise

githome's comment on GitHub #292, read on 2026-11-18. T-175 measured that GitHub binds no runner of a scale set to its job.

## Approach

1. The hook asks `serve` before it ends a job; `serve` asks the source with a new request `assigned` (the `job_id` of the offer and the facts of the job that the runner got: `run_id`, the job id and name, `event`, the head repository and its fork flag, the actor). The source answers `keep` (podrunner records the real job in the journal and the report) or `end` with a code.
2. Independent of the source, the hook ends a job whose trust facts are below the tier of its unit, with an annotation: a job of a fork never runs in a trusted unit.
3. The source wire (`docs/contracts/source-driver.md`) and `source-conformance` get the request.

## Decision

The job-started hook asks `serve`, and `serve` asks the source: `assigned { job_id (of the offer), real: { run_id, job_id, job_name, event, repository, head_repository, head_is_fork, actor } }`. The source answers `keep { trust_inputs }` (the trust facts of the real job, read by githome from GitHub) or `end { code }`. podrunner runs its tier rule on those facts against the tier of the unit and ends the job on a mismatch, with an annotation. No answer in 10 s: the job ends (fail closed) with the code `source-unavailable`. The report keeps the `job_id` of the offer and adds the ids of the real job; the job of the offer stays open for a later mint. (the question Q-1 of podrunner on GitHub #292, 2026-11-18T07:25Z; githome went away at 07:22Z with no answer, so podrunner takes its recommendation)

## Prove

`cargo test -p podrunner-broker-fake --test runner -- --ignored`: two runs of one repository with the same labels, in both orders of assignment, end `success` in units of the right tier; a fork job given to a trusted unit ends with its annotation and the source gets a report with its code.

In the code (2026-11-18): the source wire has `assigned` (`podrunner-source`: `Request::Assigned`, the replies `keep` and `end`, `JobSource::assigned` with a default of `bad-request`, the remote gate and the mint pass it on; conformance test 13 `assigned-needs-a-claim`, failed by the plant `AssignedUnclaimed` of the fake). The slots answer once for each unit (`crates/podrunner-slots/src/assigned.rs`): the run of the offer is kept with no call; else the source answers, and `keep` runs the job only when `podrunner_tier::intake` of the facts of the real job gives the tier of the unit; `bad-request` of a source ends the job as before; an audit line `assigned` names the run, the repository and the answer, and goes to the source. `serve` gives each unit a front on its egress socket (`crates/podrunner/src/serve/front.rs`): `POST http://control.podrunner.test/v1/assigned` is a question; each other request goes to the egress module; a head over 64 KiB is 431, a silent peer ends after 30 s, a 129th connection is 503; no answer within 10 s is `end source-unavailable`. The job-started hook (`images/runner/podrunner-job-started.sh`) asks through the proxy of the unit with the node of the runner and ends the job on each answer but `keep`. Tests: `cargo test -p podrunner-slots assigned` (5), `cargo test -p podrunner front::` (3), `cargo test -p podrunner-fake`, `cargo test -p podrunner-source`, and with the official runner `the_job_started_hook_asks_serve_for_a_job_of_another_run` (`cargo test -p podrunner-broker-fake --test runner`). Level: proven here with the official runner and fakes; not run on a real unit. A review agent read the diff; its findings are fixed in the same commit: an offer with no run gets the job file with run 0, so the hook always asks; the facts of `keep` must name the repository and the run of the real job (`facts-mismatch`); the front gives a place back only when both ways of a tunnel end, survives a failed accept and a failed thread, and reads a head as bytes; the queue of questions has 64 places; an answer that comes after the hook gave up records `end source-unavailable`; the hook runs with a fixed `PATH` and no `NODE_OPTIONS`. The podvm kind asks through `PODRUNNER_EGRESS` (T-530, done). Open: the ids of the real job in the run report (a field of `run-report.v1`, COORD-087); a run of `serve` with two runs on a real microvm or incus unit in `runner-image.yml`. With podcrates `v0.71.0` the run report has `assigned` (the run, the repository, the job name, the answer and its code), filled when the real job is not the run of the offer.

State (2026-11-18): the test `a_job_of_another_run_that_serve_keeps_runs_in_a_podvm_unit` (`podrunner-broker-fake --test podvm`, a step of `runner-image.yml`): the official runner in a real podvm unit gets a job of another run, its hook asks a stand-in of the front through the egress socket of the unit, and `keep` runs the step. Next: its green run; then a run of `serve` itself with two runs.

## Done

2026-11-18, the commit that names T-493. The question of the job-started hook through the front of the egress socket, the `assigned` request of the wire with `keep` and `end`, the tier rule on the facts of the real job, the audit line and the `assigned` field of the report (see "In the code" above). Proven with the official runner: `the_job_started_hook_asks_serve_for_a_job_of_another_run` and `the_job_started_hook_asks_through_the_egress_socket` (`podrunner-broker-fake --test runner`, `keep` runs the job, `end` ends it with the code in its annotation), and on a real podvm unit in `runner-image.yml` run 44349725647: `a_job_of_another_run_that_serve_keeps_runs_in_a_podvm_unit` passed, on the plain unit and under the confine layer. Level: proven in CI on GitHub-hosted runners; not run with githome's source.


# T-494: G-16: one `[trust]` list for each host: a login of githome can match a login of GitHub

**Source:** githome's G-16 on GitHub #292 (2026-11-18); the ruling of the owner of 2026-11-18 (each need of githome and cfb)
**Category:** defect
**Milestone:** later
**Priority:** P0
**Effort:** M
**Status:** done

## Problem

`[trust] users` and `organizations` hold for each source of the host. A login of githome and a login of GitHub are names in two namespaces: an entry `alice` meant for one forge trusts the owner `alice` of the other.

## Premise

The crate half is githome's G-32 on PodUtils/podcrates#223 (the fields of `pod-job`), which podcrates takes under COORD-134. githome's comment on GitHub #292, read on 2026-11-18. `crates/podrunner-tier/src/lib.rs` compares the owner by name, and by id only when both sides give one.

## Approach

1. `[[source]] trust = { users = [...], organizations = [...] }` replaces the global `[trust]` for that source.
2. `trust_inputs.forge` (`github.com`, or the host of the forge) in `job-offer.v1`, checked against the source: ask podcrates for the field (an added optional field keeps the id, COORD-134).
3. `check` refuses a global `[trust]` with two sources of two forges and no list of their own.

## Decision

One `program` source carries the jobs of two forges, so the trust lists are per source and per forge: `[[source]] trust = [{ forge = "github.com", users = [...], organizations = [...] }, { forge = "HOST", users = [...] }]`. `trust_inputs.forge` is the host name of the forge. An offer with no forge, or with a forge that the source does not list, is refused by name. (the question Q-2 of podrunner on GitHub #292, 2026-11-18T07:25Z; githome went away at 07:22Z with no answer, so podrunner takes its recommendation)

## Prove

`cargo test -p podrunner-tier`: a job whose owner is `alice` on githome is not trusted by an entry `alice` of the GitHub source, and the reverse.

In the code (2026-11-18): steps 1 and 3. `[[source]] trust` (the keys of `[trust]`) holds in place of `[trust]` for the offers of its source: the placement (`placement::place_for`), the answer to the job-started hook (T-493) and the remote mint read `Config::trust_of` (`crates/podrunner-slots/src/trust.rs`). `check` refuses a `[trust]` that names an owner when a `program` source shares it with another source. Tests: `cargo test -p podrunner-slots trust` (`a_login_of_one_source_is_not_trusted_on_another`, `a_global_list_shared_with_a_program_source_is_refused`). Level: proven by unit tests here; not run with githome. Open: step 2, the field `trust_inputs.forge` and one list for each forge inside a source; the field is in `pod-job` of podcrates (PodUtils/podcrates@T-923, asked by COORD-087, GitHub #294).

## Done

2026-11-18: steps 1 and 3 on `6d0770b`; step 2 with podcrates `v0.71.0` (`trust_inputs.forge`, PodUtils/podcrates@T-923): `[[source]] trust` is one table, or a list of tables each with its `forge`; an offer takes the table of its forge, and an offer with no forge or an unlisted forge is refused (`tier-mismatch`); a forge named twice is refused; a remote mint takes one table (`crates/podrunner-slots/src/trust.rs`). The placement and the answer to the job-started hook read `Config::trust_for`. Tests: `a_login_of_one_source_is_not_trusted_on_another`, `a_global_list_shared_with_a_program_source_is_refused`, `each_forge_of_a_source_has_its_own_list`. Level: proven by unit tests here; githome has not run it.


# T-495: G-17: the facts of a host for its source in `hello`

**Source:** githome's G-17 on GitHub #292 (2026-11-18); the ruling of the owner of 2026-11-18 (each need of githome and cfb)
**Category:** feature
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

A source does not know what a host can run, so it offers jobs that the host must refuse, and it cannot show its hosts to an operator.

## Premise

githome's comment on GitHub #292, read on 2026-11-18.

## Approach

The `hello` request carries `host`: the versions of podrunner, podvm and podnet, `arch`, `/dev/kvm`, and for each kind its boundary, its container route, and its largest `cpu`, `memory`, `disk` and `pids`. `serve` sends a new `hello` after a change of the configuration. The source wire and `source-conformance` get the field.

## Prove

`cargo test -p podrunner-source`: the `hello` of a host with two kinds holds both, with their limits.

In the code (2026-11-18): the `hello` request has an optional `host` (`podrunner_source::HostFacts`: the versions of podrunner and podvm, `arch`, `kvm`, and for each kind its boundary, its container route and its largest `cpu`, `memory`, `disk` and `pids`); `JobSource::hello_host` gives it (the default is `hello`); the wire, the mint and the remote gate pass it on. `serve` sends it to each source at its start, and logs one line for a source that refuses it (`crates/podrunner/src/serve/host.rs`, `Slots::hello_sources`). With no reload, a change of the configuration takes a new start, which sends the new facts. Tests: `the_facts_of_the_host_reach_a_source_in_hello` (`podrunner-source`), `the_facts_of_a_kind_hold_its_largest_values` (`podrunner-slots`). Open: the version of podnet (the field is empty), and a test of `source-conformance` for the field.

## Done

2026-11-18, `ca8c308` (the field) and the next commit. `serve` fills `podnet` with the version that `podnet --version` of the first kind with a `podnet` gives (`crates/podrunner/src/serve/host.rs`, `tools`). `source-conformance` has test 14 `hello-takes-the-facts-of-the-host`: `hello` with `host` names the same source as `hello` with none; the plant `RefuseHost` of the fake fails it. Tests: `cargo test -p podrunner serve::host` (2), `cargo test -p podrunner-source`, `cargo test -p podrunner-fake` (`each_source_plant_fails_its_test`), `cargo test -p podrunner --test program_source` (14 passes over the wire). Level: proven here with the fake source; not run with githome's source.


# T-496: G-18: the disk of a unit is a measured bound, not a size: a job does not get `ENOSPC`

**Source:** githome's G-18 on GitHub #292 (2026-11-18); the ruling of the owner of 2026-11-18 (each need of githome and cfb)
**Category:** feature
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** blocked

## Problem

githome asks for `limits.disk` as the size of the writable layer and the work directory of a unit, so that a job past it gets `ENOSPC` in its unit. podrunner measures the bytes under the work directory each 30 s and ends the job with `limit` (`crates/podrunner-slots/src/disk.rs`); the root of an incus instance has no bound (T-482). The `wall` of an offer already ends the job (`timeout`).

## Premise

The crate half is githome's G-32 on PodUtils/podcrates#223 (the fields of `pod-job`), which podcrates takes under COORD-134. githome's comment on GitHub #292 and the code, read on 2026-11-18.

## Approach

A size for each kind: a quota or a sized file system for the work directory and the writable layer (podvm, incus `size` of the root device, a sized disk of a guest). Ask podvm by a COORD issue where podvm must give it. Keep the measure as a second path.

## Prove

`cargo test -p podrunner-unit` (the conformance suite) and a job on real units in `runner-image.yml` that writes past `disk` fails with `ENOSPC` on each kind.

## Blocker

podvm must act for the other kinds: ask 5 of COORD-089 (GitHub #296), PodUtils/podvm@T-1968 (open): a sized disk of a machine guest, and on the namespace rung a sized work directory (podvm proposes a tmpfs with `size=`, memory and not disk). The incus kind has its size since podvm `0.2.0-beta.33` (T-482, `[unit.incus] root_quota`).
# T-497: G-19: more facts of a job in the run report

**Source:** githome's G-19 on GitHub #292 (2026-11-18); the ruling of the owner of 2026-11-18 (each need of githome and cfb)
**Category:** feature
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** partial

## Problem

githome and cfb meter the CI time of each project from the run report. githome asks for: `host`, `unit_kind`, `boundary`, the image digest, the runner version, the timings (`prepare_s`, `start_s`, `run_s`, `destroy_s`), `cpu_s` and `peak_rss_mib` on incus too (T-483), `disk_peak_bytes`, `net_rx_bytes`, `net_tx_bytes`, the bytes of the cache (G-20), and a scrubbed `log_tail` (64 KiB at most) for a unit that failed before the runner connected.

## Premise

The crate half is githome's G-32 on PodUtils/podcrates#223 (the fields of `pod-job`), which podcrates takes under COORD-134. githome's comment on GitHub #292, read on 2026-11-18. The rule of COORD-134: an added optional field keeps the schema id.

## Approach

1. Ask podcrates for the optional fields of `run-report.v1` that it does not have.
2. Fill each field on podvm, incus and microvm, or give `unknown` with its reason.
3. The `log_tail` goes through the scrubber of credentials.

The census of T-491 (2026-11-18): `host`, `unit.unit_kind`, `unit.boundary` and the image digest are in the report already; ask githome whether it wants them at the top level. `arch` is T-514.

## Decision

`host`, `unit_kind`, `boundary`, `image_digest` (`sha256:HEX`), `runner_version` (strings); `prepare_s`, `start_s`, `run_s`, `destroy_s` (integer seconds, rounded up); `disk_peak_bytes`, `net_rx_bytes`, `net_tx_bytes` (integer bytes; the network bytes through the egress module); `cache_read_bytes`, `cache_written_bytes` (integer bytes); `log_tail` (UTF-8, at most 65536 bytes, scrubbed of each credential, only for a unit that failed before the runner connected). Each optional; `unknown` with a reason where a kind cannot measure. cfb's milestone needs these fields (cfb's answer to Q-14): the CI time of each job reaches cfb's bill split through githome's usage feed. (the question Q-3 of podrunner on GitHub #292, 2026-11-18T07:25Z; githome went away at 07:22Z with no answer, so podrunner takes its recommendation)

## Prove

`cargo test -p podrunner-slots`: a report of each kind has each field or `unknown`; a canary in the log of a failed start is not in `log_tail`.

In the code (2026-11-18), with podcrates `v0.71.0`: the slots fill `image_digest` (the pinned digest of the payload), `runner_version` (the pin of the payload), `prepare_s` (claim to start), `start_s` (start to the first step of the runner), `run_s` (the first step, or else the start, to the end of the unit), `destroy_s` (the end to the destroy), each in whole seconds rounded up (`crates/podrunner-slots/src/report.rs`, `Times`). `disk_peak_bytes` is the peak of the disk checks (`disk.rs`), or `unknown` with the reason "no disk bound, so no disk check of the unit". `net_rx_bytes` and `net_tx_bytes` are the sums of the bytes of the egress lines of the job (`egress_lines.rs`), or `unknown` with the reason "the host has no egress module". A unit that ran with no first step gives no `start_s` and the reason "the runner reached no first step". `host`, `unit_kind` and `boundary` were in the report before. Test: `cargo test -p podrunner-slots tests::report`. Level: proven here with the fake unit; not run on a real unit. Open: `log_tail` with the scrubber; `cache_read_bytes` and `cache_written_bytes` (T-480); `cpu_s` and `peak_rss_mib` on incus (T-483).

State (2026-11-18): `log_tail` is filled for a unit that started and ended with no step and not by success or a cancel: the driver gives the log of the unit (`unit_logs`) before the destroy, and `crates/podrunner-slots/src/log_tail.rs` masks each word whose SHA-256 is the hash of a string value of a JSON credential file (or a long word of another file; `.runner` and `.podrunner-job` hold no secret), and each word of the shape of a token of GitHub, then keeps the last 65536 bytes. No credential stays in slots after the start: only the hashes. The fake unit keeps the output of its payload in its root (`.unit.log`). Tests: `cargo test -p podrunner-slots log_tail` (4), `tests::report::a_unit_that_fails_before_a_step_gives_a_scrubbed_log_tail` (a canary of the credential that the payload prints is not in the tail; it fails with no hashes). Open: the cache bytes (T-480); `cpu_s` and `peak_rss_mib` on incus (T-483, in the code; its measure on real instances).

State (2026-11-19): `gate.yml` run 44432274968 failed once in `a_report_has_its_times_its_image_and_its_measures_or_a_reason`: `run_s` was 31, not 30 (the bed ticks each second, and on a slow host the unit starts a tick late). The test takes 30 to 32 s now.

# T-498: G-22: a rotation of the key of a program source with no drain

**Source:** githome's G-22 on GitHub #292 (2026-11-18); the ruling of the owner of 2026-11-18 (each need of githome and cfb)
**Category:** feature
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

githome's host tokens get an end date. A new `key_file` needs a restart of `serve`, which drains the host; a source that answers `source-revoked` is started again in a loop.

## Premise

githome's comment on GitHub #292, read on 2026-11-18.

## Approach

`serve` passes the path of `key_file` each time it starts the program (it does); `podrunner source restart NAME` restarts one source and keeps its running jobs; a source that answers `source-revoked` is parked (its state in `status --json`), and starts again when its `key_file` changes.

The census of T-491 (2026-11-18): the code parks a revoked source (`revoked` in `engine.rs`) and never clears it; it does not restart in a loop. Measure this before the change.

## Prove

`cargo test -p podrunner --test serve_restart`: a change of the key and `source restart` keep each running job; a revoked key shows `revoked` in `status --json` with no loop of restarts.

## Done

2026-11-18, the commit that names T-498. Measured first: a revoked source stayed parked and was never started again (no loop; `revoked` in `engine.rs` was never cleared). Now `podrunner source restart --name NAME (--state DIR | --socket PATH)` (the control op `restart-source`) builds the source again from its table with its key file of now, sends it `hello` with the facts of the host, and puts it in the place of the old one (`Slots::replace_source`): each job of the source stays, and `revoked` clears. A source that does not build or refuses `hello` keeps the old one (exit 77). Each tick, a revoked source whose key file changed since it was built starts again by itself: one try for each change, so a bad key gives no loop (`crates/podrunner/src/serve/restart.rs`). `docs/cli.md` has the command. `cargo test -p podrunner --test serve_restart`: 2 passed (`source_restart_keeps_the_running_job`: a restart while the job runs, the job is reported, no `lost`, an unknown name exits 77; `a_revoked_source_waits_for_a_new_key_file`: `revoked` stays 3 s with the same key, then a new key file starts the source again, one line). The second fails with no watch of the key file. The process test uses the fake source with a key file: a program source needs a tier above `local`, which the fake unit does not serve; both kinds go through `source_of`. Level: proven here with fakes.

# T-499: G-23: the runner of a GitHub job stays inside GitHub's window of updates

**Source:** githome's G-23 on GitHub #292 (2026-11-18); the ruling of the owner of 2026-11-18 (each need of githome and cfb)
**Category:** feature
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** partial

## Problem

GitHub refuses a runner that is too old. A new release of `actions/runner` needs a new runner image, which comes only with a release of podrunner.

## Premise

githome's comment on GitHub #292, read on 2026-11-18. `images/runner/runner.toml` pins the runner.

## Approach

1. A runner image within 14 days of each release of `actions/runner`, as a release asset with its digest, with no new podrunner binary.
2. A manifest in each release that a machine reads (the runner version, the image digests, the pins of podvm and podnet).
3. `doctor` names the runner version and its age, and warns near the end of the window.

## Prove

`cargo test -p podrunner --test doctor` names the version and the age; a new image loads with `podvm load -i` on a running host.

In the code (2026-11-18): step 3: `podrunner doctor --config FILE` names each pinned payload with its runner version, its age, its window (the age limit of the pin) and `ok`, `near-end` (the last 7 days) or `stale`, and the runner that the build pins (`crates/podrunner/src/doctor.rs`, `pin_rows`; `docs/cli.md`). Step 2: each release attaches `podrunner-manifest.json` with its `.sha256` and an attestation: the versions of podrunner, the runner, podvm and podnet, each binary with its SHA-256, and each runner image with its file, SHA-256 and manifest digest (`scripts/install-pins.py --manifest`, `scripts/release-assets.sh local-mode`, `release.yml`; `docs/install.md`). Tests: `cargo test -p podrunner --test doctor doctor_names_the_runner_of_each_pin_and_its_age`, `cargo test -p podrunner doctor::tests::a_pin_names`, `sh scripts/test-install.sh` (the manifest of a release from the fixture). Open: step 1, a runner image as a release asset of its own within 14 days of a release of `actions/runner`, with no new binary (a workflow that builds the image of a new runner and publishes it with a manifest); the load of such an image with `podvm load -i` on a running host.

# T-500: G-24: a runner image with the software that cfb's gates need

**Source:** githome's G-24 on GitHub #292 (2026-11-18); the ruling of the owner of 2026-11-18 (each need of githome and cfb)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** open

## Problem

cfb's gates use Node, bun, pnpm and Playwright with its browser libraries. The larger image (`images/runner/full.txt`) has no published list against GitHub's `ubuntu-24.04` image and no browser libraries; a user cannot make an image from a recipe.

## Premise

cfb's answer to Q-13 (GitHub #292, 2026-11-18): the gates of the five private repositories (rbg first) use npm 12 or pnpm 10 to 12, Node 26 (ai also 22.12 and 24), TypeScript 7, wrangler 4.149 to 4.156 (dry runs only), Biome 2.5, vitest and `node --test`; rbg uses Playwright 1.64.1 with its browsers; no gate needs bun. The gates use about 19,300 to 24,600 build minutes in 30 days, rbg about half. A row of `scripts/corpus/user-gate.tsv` that copies rbg's gate measures this entry and T-480. githome's comment on GitHub #292, read on 2026-11-18.

## Approach

A `runner-full` image with a published list of its software and the libraries of Playwright, or `podrunner image build` from a recipe file, pinned by digest.

The census of T-491 (2026-11-18): cfb keeps `podrunner image build` (Q-12 b), so it is not optional; no gate of cfb needs bun (Q-13).

## Prove

A Playwright test of a gate passes in a unit with no download of the system libraries (`runner-image.yml`).

# T-501: G-26: a drain and a stop of one source

**Source:** githome's G-26 on GitHub #292 (2026-11-18); the ruling of the owner of 2026-11-18 (each need of githome and cfb)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`podrunner drain` and `stop` act on the whole host. An operator cannot stop one source and keep the others.

## Premise

githome's comment on GitHub #292, read on 2026-11-18.

## Approach

`podrunner drain --source NAME` and `podrunner stop --source NAME`, with the state of each source in `status --json`.

## Prove

`cargo test -p podrunner --test serve`: a drained source takes no offer, and the other sources go on.

In the code (2026-11-18): `podrunner drain --source NAME` and `podrunner resume --source NAME` (the control ops `drain-source` and `resume-source`): a drained source gets no capacity, so it takes no offer; its jobs in flight and each other source go on; `status --json` has `draining` for each source (the schema of `status.v1`). `Slots::drain_source` (`crates/podrunner-slots/src/host.rs`); `Source` moved to `sources.rs`. Test: `cargo test -p podrunner --test serve_drain` (a drained source takes no offer while the other runs its job; an unknown name exits 77; `status` names the states; after `resume` the offer runs). Open: `stop --source NAME`, which also ends the program of the source when its jobs end.

## Done

2026-11-18: `podrunner stop --source NAME` (the control op `stop-source`) drains the source; when its last job ends, `Slots::end_stopped` puts `sources::Stopped` in its place (it gives no event and refuses each call), so the old source and its program end (`Running` of the sidecar kills and reaps its child on drop), and `serve` writes one line. `resume --source NAME` builds a stopped source again from its `[[source]]` table (as `source restart`), then takes its offers. Test: `cargo test -p podrunner --test serve_drain` (`a_stopped_source_ends_and_resume_builds_it_again`: the line of the stop, the other source runs its job, the stopped one takes no offer, an unknown name exits 77, and after `resume` its offer runs). Level: proven in CI on GitHub-hosted runners when `gate` is green.

# T-502: G-27: a cancel ends its unit in a bounded time, with a conformance test

**Source:** githome's G-27 on GitHub #292 (2026-11-18); the ruling of the owner of 2026-11-18 (each need of githome and cfb)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

githome asks that a `cancel` destroys the unit within 60 s and reports `exit_reason: cancelled`, with a test in `source-conformance`. The check C3 on cfb's host (GitHub #292) measures it live.

## Premise

githome's comment on GitHub #292, read on 2026-11-18.

## Approach

Measure the time from a cancel to `destroyed` on each kind; bound it; add the test to `source-conformance`.

The census of T-491 (2026-11-18): no test of a cancel exists in the slots or in `serve`; with the defaults, `grace_s` 30 and `destroy_s` 60 allow 90 s, above the 60 s of the ask. A cancel never meets `Matched`, `Claimed` or `Minted`: `intake` runs the placement, the claim, the mint and the start in one call.

## Prove

`podrunner source-conformance` has the test and it passes against `podrunner fake-source`; `cargo test -p podrunner-source`.

In the code (2026-11-18): `cargo test -p podrunner-slots tests::cancel` (`a_cancel_destroys_its_unit_within_60_s`): a cancel of the source 5 s after the start ends the unit, which is destroyed with no residual process and reported `cancelled`, within 60 s of the cancel, for a payload that ends at SIGTERM and for one that ignores it (the kill comes at `grace_s`, 30 s). The defaults hold the bound when the destroy is fast; `destroy_s` (60) is the deadline of a slow destroy. Open: a test of `source-conformance`: the wire has no request with which the suite makes a source cancel a job, so the test needs a design (a scripted cancel of a conformance source, or a report of `cancelled` that the source must take); the measure on real kinds (C3 on cfb's host).

## Done

2026-11-18: test 15 of `source-conformance`, `a-cancelled-job-is-reported-and-released`: after `mint`, the report with `exit_reason: cancelled` and `release` are taken, and a claim after them is `unknown-job`; the plant `RefuseCancelled` of the fake source fails it (`each_source_plant_fails_its_test`). The wire has no request that makes a source cancel, so the time from a cancel to `destroyed` stays a test of the slots (`tests::cancel`, within 60 s) and the check C3 of `tests/accept.rs` on real kinds (T-525). `cargo test -p podrunner-source -p podrunner-fake -p podrunner-local`, `cargo test -p podrunner --test program_source` (15 lines) pass.

# T-503: G-28: the output of `source-conformance` for CI

**Source:** githome's G-28 on GitHub #292 (2026-11-18); the ruling of the owner of 2026-11-18 (each need of githome and cfb)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`source-conformance` writes text. githome's CI must parse it, and it does not say how many queued jobs a run needs.

## Premise

githome's comment on GitHub #292, read on 2026-11-18.

## Approach

`--json`: one line for each test; the count of queued jobs that the run needs, at its start.

## Prove

`cargo test -p podrunner --test program_source`: the JSON lines of a run against `podrunner fake-source`.

## Done

2026-11-18, the commit that names T-503. `podrunner source-conformance --json -- PROGRAM ARGS` writes one JSON object for each line: `{tests, offers_needed}` (14 and 2: each test takes a fresh source, whose first `next` gives two offers or more), then `{number, name, pass, why}` for each test, then `{passed, failed}` (`crates/podrunner/src/program_source.rs`, `docs/cli.md`). The Prove names the test target of the command, which is `podrunner`, not `podrunner-source`. `cargo test -p podrunner --test program_source source_conformance_json_is_a_line_for_each_test`: the fake source passes 14 of 14; `/bin/false` fails 14 and exits 1.

# T-504: G-29: a GitHub source for the repositories of a personal account

**Source:** githome's G-29 on GitHub #292 (2026-11-18); the ruling of the owner of 2026-11-18 (each need of githome and cfb)
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** open

## Problem

A GitHub source serves the repositories of an organization only (call C3, DR-9: a repository of a personal account needs "Administration: write"). githome asks for repository scale sets of a personal account, as a second path beside githome; R-051 does not need it.

## Premise

githome's comment on GitHub #292, read on 2026-11-18. DR-9 is a ruling of 2026-11-11; the ruling of 2026-11-18 accommodates each ask of githome.

## Approach

1. Measure the permission that a repository scale set needs.
2. An opt-in of the operator, with the permission named in `check` and `doctor`; the default stays DR-9. Record the call in `docs/decisions.md`; ask the owner if the opt-in changes DR-9.

## Prove

`cargo test -p podrunner --test scaleset`: a repository URL of a personal account with the opt-in serves a job of the fake scale set; without it, `repo-scope-refused`.

# T-505: No cap of units for the whole host across its kinds

**Source:** cfb's P-8 on GitHub #292 (2026-11-18); the ruling of the owner of 2026-11-18 (each need of githome and cfb)
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

A host bounds its units by the `slots` of each kind and of each source only. cfb's host has 32 GiB of RAM and units of 12 GiB on two kinds (`incus` for trusted jobs, `microvm` for the jobs of forks and the trusted jobs with containers): it must set `[unit.incus] slots = 1` and `[unit.microvm] slots = 1`, so two trusted jobs never run at one time, also when no job of a fork waits. A cap for the whole host would let each kind take up to the cap.

## Premise

`crates/podrunner-slots/src/config.rs` (no key of the host for a count or a memory of units), read on 2026-11-18.

## Approach

A key of the host, `max_units` (and, if a measure shows the need, a memory budget: the sum of the `mem` of the running units), checked by placement before a claim, with its value and use in `status --json`. `check` refuses a cap of 0.

## Prove

`cargo test -p podrunner-slots`: with `max_units = 2`, two kinds of 2 slots each never hold more than 2 units, and each kind can take both.

## Done

2026-11-18, the commit that names T-505. The key `max_units` of the host caps the units across its kinds: the free slots of each kind are the lesser of its own and of the cap less the units that run (`kind_views`, now in `crates/podrunner-slots/src/host.rs`), so placement and the capacity of each source hold it; `check` refuses 0; `status --json` has `max_units` and `units` (the schema of `status.v1`); `docs/configuration.md`; `CHANGES.md`. No memory budget: no measure shows the need yet. `cargo test -p podrunner-slots tests::host_cap`: with `max_units = 2`, two kinds of 2 slots and four offers hold 2 units for 5 more ticks, the first kind takes both, `status` says 2 of 2; with no cap the four run; `max_units = 0` is refused.

# T-506: G-36: a sandbox unit: a payload that the source names, for an agent task or a web terminal

**Source:** githome's G-36 on GitHub #292 (2026-11-18): githome's owner ruled that podrunner is the one way a machine runs work for githome; the ruling of the owner of 2026-11-18 (each need of githome and cfb)
**Category:** feature
**Milestone:** later
**Priority:** P1
**Effort:** L
**Status:** partial

## Problem

githome removes its Cloudflare Containers, its own job agent and its runner protocol: its coding-agent tasks and its web terminals move to podrunner. A unit of podrunner runs only the official runner (or an alternate payload of a CI job, T-161). githome asks for a sandbox unit: a payload that the source names (an image of the store by digest and a command), with the isolation, the egress hold, the limits and the report of a job unit; an attach of a terminal over an outbound WebSocket to a URL of the source, with no inbound port; an end at `idle_s` with no attach, and at `max_s`; `cancel` ends it; the credential of the payload as a file, as the files of `mint`.

## Premise

githome's comment on GitHub #292, read on 2026-11-18. `payload_from_offer` and the pin list (`[pin]`) take a payload of an offer only when it is pinned.

## Approach

1. The offer: `payload: { kind: "sandbox", image, command }`, `idle_s`, `max_s` (ask podcrates for the optional fields of `job-offer.v1`; COORD-134: an optional field keeps the id).
2. Placement and the tier rule as for a job; the image must be in the store and on the pin list of the operator, or the offer is refused by name.
3. The attach: the payload dials the URL of the source through the egress module of its unit (the policy of the offer allows that host); podrunner adds no inbound port.
4. The ends: `idle_s` with no attach, `max_s`, `cancel`; the report with the use, as for a job.

## Decision

githome's sandbox image carries its own attach client: podrunner gives the credential as a file and a way out to the URL of the source through the egress module, and knows no attach protocol. An agent task and a web terminal both run at `untrusted` (a guest of the `microvm` kind), also for a member. A new value `sandbox` of `Event` in `job-offer.v1` (asked of podcrates). Sandboxes come through a `program` source of their own, with its own `slots`. (the question Q-7 of podrunner on GitHub #292, 2026-11-18T07:25Z; githome went away at 07:22Z with no answer, so podrunner takes its recommendation)

## Prove

`cargo test -p podrunner-slots` with a fake source and a fake unit: a sandbox offer runs its command, ends at `idle_s` with no attach and at `max_s`, and gives a report; then a sandbox on real units in `runner-image.yml` that attaches over an outbound WebSocket.

In the code (2026-11-18): `pod-job` `v0.71.0` carries `sandbox { image, command, idle_s, max_s }` of an offer (no new value of `Event`: a sandbox comes through a source of its own). An offer with it runs at `untrusted` (a guest), also on a trusted set; its image must be a `[[pin.payload]]` with `sandbox = true` of its architecture and digest (else `payload-not-pinned`), and a runner job never takes such a pin; the unit runs `command`, not the command of the kind; the payload gets `PODRUNNER_IDLE_S`; the unit ends at the lesser of `max_s` and the wall; no job file of the hook (`crates/podrunner-slots/src/sandbox.rs`; `docs/configuration.md`; `CHANGES.md`). The idle end is the payload's: an attach is one long connection through the egress module, which shows no idle time to podrunner (the call in `sandbox.rs`). Tests: `cargo test -p podrunner-slots tests::sandbox` (3: the placement and the pin, the command, the idle time and the end, and a run through slots with the fake unit). Open: a sandbox on real units that attaches over an outbound WebSocket.

In the code (2026-11-18, later): `a_sandbox_ends_at_its_max_s`: a sandbox with `max_s = 2` whose command sleeps ends with `timeout`. The lane `serve-incus-kvm` of the user-path gate runs a sandbox offer on a real microvm unit (T-527).

# T-507: G-37: an install of `serve` that another program drives, with no prompt

**Source:** githome's G-37 on GitHub #292 (2026-11-18): `githome runner` becomes a wrapper of podrunner; the ruling of the owner of 2026-11-18 (each need of githome and cfb)
**Category:** feature
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** partial

## Problem

githome's CLI installs and configures podrunner on a machine of a user, with a host token that the user mints in githome. `scripts/install.sh` sets up the local mode only: it writes no `podrunner.toml` and no unit file. A program that drives the install must write the configuration of `serve` by hand from `docs/configuration.md`.

## Premise

githome's comment on GitHub #292 and `docs/install.md`, read on 2026-11-18.

## Approach

1. `podrunner-install.sh --serve`: the pinned files of the serve mode (podrunner, podvm, podnet, the runner images), each checked by its SHA-256.
2. `podrunner config init --source NAME --program 'CMD ARGS' --key-file FILE --kinds auto`: a `podrunner.toml` from the kinds that `doctor` finds, which `check` takes; then `print-unit`.
3. `--json` output and a documented exit code on each step; no root where the kinds allow it; a second run changes nothing.
5. The host keeps no inbound port (cfb's R-044; githome on GitHub #292, 2026-11-18T07:12Z): the install and `serve` open no TCP listener; githome's agent reaches the host through the podssh roost of the operator.
4. The schema of T-479 checks the file that `config init` writes.

## Decision

The operator does the steps with root once: Incus with a range for root in `/etc/subuid` and `/etc/subgid`, the user in the group of `/dev/kvm` and of incusd, and `loginctl enable-linger` for the user. `podrunner doctor --json` lists each one that is missing. Then githome's CLI runs, as the user, `podrunner-install.sh --serve` and `podrunner config init`, and installs a systemd user unit from `print-unit`. No TCP listener. (the question Q-8 of podrunner on GitHub #292, 2026-11-18T07:25Z; githome went away at 07:22Z with no answer, so podrunner takes its recommendation)

## Prove

`sh scripts/test-install.sh` with a lane of `--serve`: on a fresh user account, a run with no prompt gives a configuration that `podrunner check` takes and a unit from `print-unit`; a second run changes nothing; `ss -ltn` shows no new TCP listener after the install and the start of `serve`.

In the code (2026-11-18): step 2: `podrunner config init` writes a `podrunner.toml` with one `program` source, the kinds that this host can run and the pin of the image; `check` takes it; a second run changes nothing; 78 when no kind runs; 77 when the file holds another text (`crates/podrunner/src/config_init.rs`, `docs/cli.md`). The manifest of a release has the time of the build of each image (`built_at_s`, from the OCI config) for `--built-at-s`. Test: `cargo test -p podrunner --test config_init`. Open: the `print-unit` of a systemd user unit, the check of `ss -ltn`, and the schema of T-479.

In the code (2026-11-18, step 1): `podrunner-install.sh --serve` installs the same files with no `run.toml` and no `hello.yml`, writes `DATA/serve.json` (`podrunner`, `podvm`, `podnet`, `images`, `image`, `runner_version`) and prints the command of `config init`; a second run changes nothing (`sh scripts/test-install.sh`, case `--serve`). The lane `serve-incus-kvm` of the user-path gate (T-527) runs `config init`, `check`, `doctor` and `print-unit` as a user with no root.


# T-508: COORD-135: the schemas of `status` and of the source wire move to `pod-job`; podcrates needs their types, the stable fields, and the new fields of githome's asks

**Source:** COORD-135 of podcrates (PodUtils/podcrates#226, 2026-11-18): githome's G-31 and G-32 (PodUtils/podcrates#223); the ruling of the owner of 2026-11-18 (each need of githome and cfb) and the ruling "Shared code"
**Category:** chore
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

githome pins the schemas of podrunner from one podcrates tag (G-31). podrunner and githome both read the schema of `status.v1` and the source wire (`docs/contracts/source-driver.md`), so under the ruling "Shared code" they move to `pod-job` (PodUtils/podcrates@T-922). The new fields of the offer and the report for githome's G-16, G-18, G-19 and G-20 come as fields of `pod-job` too (PodUtils/podcrates@T-923, githome's G-32). podcrates needs from podrunner: the names of the types and the files at a commit of `main`; which fields are stable; and the names and units of the fields of G-16, G-18, G-19 and G-20.

## Premise

The direct message of the podcrates session and COORD-135, read on 2026-11-18.

## Approach

1. Name the types of `status.v1` (`crates/podrunner-slots/src/status.rs`, the schema of `status.v1`) and of the source wire (`crates/podrunner-source`), at a commit of `main`, with the stable fields.
2. With the designs of T-494 (`trust_inputs.forge`), T-496 (the disk as a size), T-497 (the facts of the report) and T-480 (`cache: { key, mode }`), give the name, the type and the unit of each new field.
3. Answer by a COORD issue `(to podcrates)`, and take the tag of podcrates that carries them; podrunner then reads the schemas from `pod-job`, and the schema of `status.v1` goes.

## Prove

The answer on a COORD issue, and `cargo test --workspace` on the take of the tag, with a test that the status of `serve` validates against the schema of `pod-job`.

State (2026-11-18): answered by COORD-087 (GitHub #294): the types and files at `3a0cbbb`, the stable fields (each field of `3a0cbbb` is the `v1` form), and the names and units of the new fields of G-15, G-16, G-17, G-18, G-19, G-20, G-36 and G-04, from the calls of Q-1 to Q-8. Left: take the tag of podcrates that carries them, and read the schemas from `pod-job`.

State (2026-11-18, later): podcrates took the answer (PodUtils/podcrates@T-922, PodUtils/podcrates@T-923, podcrates `main` `1da8fcc9`; COORD-135 closed). Its two calls: the source wire moves to `pod-job` after T-486, T-493 and T-495 land, so that each change lands once (the coordinator confirms the order at the start of the next session); `limits.disk` as a size where a kind can enforce it keeps the meaning "the disk limit of the job in bytes", the enforcement of each kind is in `unit-capability.v1`, so the id stays `v1` (podrunner agrees).

State (2026-11-18, v0.72.0): podrunner took podcrates `v0.72.0` (COORD-138, PodUtils/podcrates#229). The source wire is `pod_job::source`: `podrunner-source` re-exports `Request`, `Reply`, the events, the facts of the host and the codes; the credential is `pod_job::source::Minted`, and the check of a file name stays in podrunner (`PlainName`). Left: `status.v1`. The `Status` of `pod-job` refuses unknown fields and has no `draining` of a source (T-501), no `max_units` and no `units` of the host (T-505); podrunner keeps its own `Status` until a tag of podcrates has them (asked on the COORD issue of the take).

## Done

2026-11-18: podcrates `v0.73.0` (COORD-140, PodUtils/podcrates#231; PodUtils/podcrates@T-947) has `SourceStatus::draining`, `Status::max_units` and `Status::units`. `podrunner-schema` re-exports `pod_job::{Status, SourceStatus, KindStatus, PinStatus}` and `podrunner-slots` fills them; the schema of `status.v1` of podrunner and its test go. The check C4 of `runner-image.yml` (T-525) validates the samples against `PodUtils/podcrates:schemas/status.v1.schema.json` at the commit of the tag. With `v0.72.0`, the source wire and the credential are `pod_job::source`. `cargo test --workspace` passes.

# T-509: G-38: a test of githome's source in podrunner's CI that blocks a release

**Source:** githome's G-38 on GitHub #292 (2026-11-18T07:22Z, before githome's pause); the ruling of the owner of 2026-11-18 (each need of githome and cfb)
**Category:** feature
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** partial

## Problem

githome is away until podrunner is complete, and a change of podrunner must never break githome meanwhile. podrunner's CI tests the source wire against its own fake source only.

## Premise

githome's comment on GitHub #292, read on 2026-11-18. githome's wire is `docs/contracts/source-driver.md` (githome serves it at `/_runner/source/OP`, JSON).

## Approach

1. A job of CI runs `podrunner source-conformance` and one live job of each kind (`podvm`, `incus`, `microvm` with `engine`) against a githome core: the CLI of githome from its released bundle (`githome source`) with a core of githome's local stack, or a fake of githome's wire that podcrates' `pod-testkit` keeps (githome's G-30, G-40).
2. A red run of that job blocks `release.yml`, as the user-path gate does.

## Prove

The job in `runner-image.yml` (or its own workflow) passes on `main`, and `release.yml` refuses a version whose run of it is red.

State (2026-11-18): `PodUtils/githome` is not reachable from the sessions and the CI of podrunner (the REST API gives 404), so podrunner has no source of githome to pin; podcrates PodUtils/podcrates@T-921 is a fake GitHub, not githome's source.

State (2026-11-18, later): podcrates `v0.73.0` has the release asset `pod-fake-githome-TARGET` (PodUtils/podcrates@T-948, in the signed `SHA256SUMS`): `pod-fake-githome serve` keeps the state, `pod-fake-githome source` is the program source; a run of `source-conformance` takes one queued job. Next: pin the asset, run `source-conformance --json` and one live job of each kind against it in `runner-image.yml`, and gate `release.yml` on it.

In the code (2026-11-18): `images/fake-githome.toml` pins `pod-fake-githome` of podcrates `v0.73.0` (the SHA-256 of each architecture, from the signed `SHA256SUMS`). `scripts/githome-conformance.sh PODRUNNER` downloads and checks it, starts `serve`, queues one job for each test and two more, and runs `podrunner source-conformance --json -- pod-fake-githome source`: each of the 15 tests passes (also here). The job `test` of `gate.yml` runs it, and `release.yml` needs a green gate. Left: one live job of each kind against githome's source (it needs the broker of `pod-fake-github`, PodUtils/podcrates@T-921).


# T-510: G-39: one record of each change of a contract that githome reads

**Source:** githome's G-39 on GitHub #292 (2026-11-18T07:22Z); the ruling of the owner of 2026-11-18
**Category:** release
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

githome's next session must see what changed since `v0.1.0-beta.7` in each contract that it reads: `job-offer`, `run-report`, the source wire, the keys of `podrunner.toml` that `githome runner install` writes (T-507), and the exit codes. The notes of each release list changes in prose.

## Premise

githome's comment on GitHub #292, read on 2026-11-18.

## Approach

A file `CHANGES.md` (one line for each change of such a contract, with its release tag and its entry), written in the same commit as the change; `scripts/check-notes.sh` refuses a release whose contract changes have no line.

The census of T-491 (2026-11-18): the changes of a contract on `main` since `v0.1.0-beta.7` go into the first lines: no egress with no `[egress] default_policy` (T-410), T-409, `cpus` of the podvm kind (T-221), the disk limit of an offer as the lesser of two (T-529), `pod-job` from `v0.52.0` to `v0.70.0`, and the path of the schema of `status.v1` after T-508 (the check C4 on GitHub #292 names it).

## Prove

`sh scripts/check-notes.sh --latest` fails on a planted change of `docs/contracts/source-driver.md` with no line, and passes with it.

## Done

2026-11-18, the commit that names T-510. `CHANGES.md` has one row for each change of a contract since `v0.1.0-beta.7` (the wire, the report, the control socket, the keys, the command line, the assets, the pins), under `## next`, and the SHA-256 of each contract file (`docs/contracts/*.md`, `schemas/*.json`). `scripts/check-notes.sh` (in `check.sh`, `gate.yml` and `release.yml`) fails when a contract file differs from its stamp (`--stamp` writes the stamps after the row), and refuses the notes of a release after the base with no section of its tag or with rows left under `## next`. `docs/maintenance.md`, "A release", step 5. Measured: `sh scripts/check-notes.sh --latest` fails with a line planted in `docs/contracts/source-driver.md` and passes with the file restored; the notes of a planted `v0.1.0-beta.8` fail with `## next` and pass with a section `## v0.1.0-beta.8`. `sh scripts/shellcheck.sh scripts/check-notes.sh`: clean.

Note (2026-11-18): when the source wire moves to `pod-job` (T-508), the lines of `CHANGES.md` for the records of `pod-job` move to the change file of the schemas of podcrates (the call of podcrates on COORD-135).

# T-511: G-40: each fake and tool that githome's tests need, as a release asset

**Source:** githome's G-40 on GitHub #292 (2026-11-18T07:22Z); the ruling of the owner of 2026-11-18
**Category:** release
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** partial

## Problem

githome's CI must pin, with no build: a fake podrunner that drives the source wire (`podrunner-fake`), the fake GitHub of githome's G-30, and the runner images. A release attaches the images, but no fake.

## Premise

githome's comment on GitHub #292, read on 2026-11-18. podcrates moves `podrunner-fake` and `podrunner-broker-fake` into podcrates for G-30 (COORD-134).

## Approach

Each fake as a static binary of the release (or of the podcrates tag that holds it after the move), with its SHA-256 and an attestation, listed in `SHA256SUMS` (T-484).

## Prove

A dry run of `release.yml` attaches each fake, and `sha256sum -c` and `gh attestation verify` pass on each.

In the code (2026-11-18): the fakes that drive a source ship in the released `podrunner` binary, with its SHA-256 and its attestation: `podrunner fake-source --script FILE` (a fake source), `podrunner source-conformance -- PROGRAM` (14 tests of the wire), and `podrunner serve` with `[unit.fake]`. A program source needs `trusted` or `untrusted`, which the fake kind did not serve; with `allow_fake`, `[unit.fake] claim = "confined"` (or `"guest"`, with `guest_kinds` and the self-test) makes the fake kind claim that boundary and an enforced hold, with one line at the start that it holds nothing (`crates/podrunner/src/serve/fake.rs`, `docs/configuration.md`). Test: `cargo test -p podrunner --test serve_program` (no claim: `check` refuses; a bad claim: refused; with the claim a job of a trusted program source runs and is reported). The fake GitHub of G-30 is podcrates' (`pod-testkit`). Open: the release dry run of the Prove.

# T-512: G-41: the statement "githome: complete in vTAG"

**Source:** githome's G-41 on GitHub #292 (2026-11-18T07:22Z): githome's next session starts on it; the ruling of the owner of 2026-11-18
**Category:** release
**Milestone:** later
**Priority:** P0
**Effort:** S
**Status:** open

## Problem

githome's owner starts githome's next session only on a statement of podrunner that each ask of githome (G-01 to G-41) is done in a release. cfb's milestone closes with the checks C1 to C5 on githome's host and the gates of the five repositories through githome (cfb's Q-14). With no statement, githome does not come back.

## Premise

githome's and cfb's comments on GitHub #292, read on 2026-11-18.

## Approach

When each ask G-01 to G-41 is done in a release (each entry `done` with its tag, the gates of `AGENTS.md` section 12 held): one line `githome: complete in vTAG` on GitHub #292 and #151, with the table of each ask, its entry and its release, the steps of githome's pair run and of the bootstrap of the host at that tag (T-507), and the record of changes (T-510). Name each release tag on #292 when T-493, T-494, T-505 and T-507 ship (cfb's request).

## Prove

The statement on both issues, and `cargo todo check` with each entry of G-01 to G-41 `done`.

# T-513: Two file locks of the local mode that `pod_core::lock` cannot hold yet

**Source:** the take of podcrates `v0.70.0` (COORD-132, PodUtils/podcrates#222; COORD-131, the ruling "Shared code"), 2026-11-18
**Category:** chore
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

podrunner has two more copies of a `flock` lock, beside `pod_core::lock::FileLock`. `run_local/concurrency.rs` (the groups of `concurrency:`) waits with no bound for the holder, and a signal ends the wait (a newer run of the group cancels a waiting one, or Ctrl-C). `exec/host.rs` (the lock of a runner directory) opens a lock file of another user read-only when it cannot open it for write, as `flock` takes a read descriptor. `FileLock` has a wait with a bound only, and opens each file for write.

## Premise

The code of both files and `pod-core/src/lock.rs` at `v0.70.0`, read on 2026-11-18. `crates/podrunner/src/lock.rs` (the lock of `serve`) takes `FileLock` already.

## Approach

Ask podcrates by a COORD issue for two extensions of `FileLock`: a wait with no bound that a signal ends (`EINTR` gives a refusal of its own), and a read-only open for a file that the caller cannot write. Then replace both copies, and keep the tests of both (`concurrency`, `exec::tests` with `flock(1)`).

## Prove

`cargo test -p podrunner-local exec`, `cargo test -p podrunner concurrency`; no `libc::flock` in `crates/` (`grep -rn 'libc::flock' crates`).

## Done

2026-11-18: podcrates `v0.71.0` (PodUtils/podcrates@T-924) gives `FileLock::lock_until` (a wait with no bound that a stop flag ends), `FileLock::try_lock_existing` (a probe that makes no file) and a read-only open of a lock file that the caller cannot write. The groups of `concurrency:` wait with `lock_until` and the flag of `cancel` (a SIGTERM of a newer run, Ctrl-C), the lock of a runner directory takes `try_lock`, and `is_free` takes `try_lock_existing`. No `libc::flock` is left in `crates/`. `cargo test -p podrunner concurrency`, `-p podrunner lock::` and `-p podrunner-local exec` pass, the last with `flock(1)` of another process.

# T-514: G-05 and G-06: the run report names no architecture

**Source:** the census of T-491 (`docs/reports/census-2026-11-18.md`), 2026-11-18
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

githome asks that each report names the architecture of the job (G-05, G-06). `RunReport` and `PublicReport` of `pod-job` have no field `arch`, and the field list of T-497 leaves it out.

## Premise

The census read `pod-job` at `v0.70.0` and `crates/podrunner-slots/src/run.rs`. Not measured.

## Approach

Ask podcrates to add an optional `arch` to `run-report.v1` beside the fields of COORD-087 (an added optional field keeps the id). Fill it from `capability.arch` of the kind. Add it to the `CHANGES.md` of T-510.

## Prove

`cargo test -p podrunner-slots` with a test that reads `arch` in the report of a fake unit.

## Done

2026-11-18: podcrates `v0.71.0` (PodUtils/podcrates@T-923) has `arch` in `run-report.v1`; the report of a placed job names the architecture of its kind (`crates/podrunner-slots/src/report.rs`).

# T-515: G-12: a unit past its memory limit does not end with the exit reason `limit`

**Source:** the census of T-491 (`docs/reports/census-2026-11-18.md`), 2026-11-18
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** partial

## Problem

githome's G-12 asks that a unit past a limit ends with `limit`. Only the disk check sets `ExitReason::Limit` (`crates/podrunner-slots/src/disk.rs`). On the podvm kind one process gets `ENOMEM` and the job fails as `failure`; incus and microvm kill on OOM with no fact in the report.

## Premise

The census read the code of `main`. Not measured on a unit.

## Approach

At the end of a unit, read the OOM facts of its kind: `memory.events` `oom_kill` of the delegated cgroup (podvm), the state of the instance (incus), the exit of the guest (microvm). Report `limit` with an audit line `limit` and the detail `memory`. Ask podvm by a COORD issue for each fact that it does not give.

## Prove

`cargo test -p podrunner-slots` (a fake unit that reports an OOM ends `limit`); `runner-image.yml`: a payload past `mem` on the podvm and incus kinds ends `limit`.

In the code (2026-11-18): the unit seam has `UnitDriver::oom_kills` (none by default). The podvm kind reads `oom_kills` of its run report (podvm `0.2.0-beta.32`, a unit with `--memory` in a delegated cgroup2 subtree), the incus kind `OomKills` of `inspect` (podvm `0.2.0-beta.33`; COORD-187 ask 6, PodUtils/podvm#583). Before the destroy, slots turns a job that failed (`failure`, `unit-failed`) after a kill into `limit`, with an audit line `limit` and the detail `memory` (`crates/podrunner-slots/src/run.rs`). The fake unit says a count that its payload writes to `.oom_kills` (`crates/podrunner-fake/src/unit_files.rs`). Test: `cargo test -p podrunner-slots a_job_that_failed_after_an_oom_kill_ends_with_limit`. Open: the measure on the podvm and incus kinds in `runner-image.yml`, and a guest of the microvm kind on real guests. With podvm `0.2.0-beta.34` the microvm kind gives each guest `--report` in the directory of its unit and reads `oom_kills` of it (`crates/podrunner-microvm/src/lib.rs`; test `the_oom_kills_of_a_guest_come_from_the_report_of_podvm`).

# T-516: G-09: no CI proof of a `docker build` step and of Docker-in-Docker in a guest with an engine

**Source:** the census of T-491 (`docs/reports/census-2026-11-18.md`), 2026-11-18
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** open

## Problem

githome's G-09 keeps `docker build` and Docker-in-Docker. The parity workflows of the engine (01 to 09) run `services:`, `container:` and Docker actions; none runs a `docker build` step or a Docker-in-Docker step. `docs/container-engine.md` step 4 says "Open".

## Premise

The census read `crates/podrunner/tests/local/parity` and `runner-image.yml`. Not run.

## Approach

Add a parity workflow `10-docker-build.yml` (a `docker build` of a small Dockerfile and a `docker run` of the result) and one with a `docker:dind` service, to the engine run of `runner-image.yml`; compare with the result of github.com.

## Prove

`runner-image.yml`: the parity step of the engine passes with the two new workflows.

# T-517: G-09: the engine of a guest is a directory of the host, not a part of the payload image

**Source:** the census of T-491 (`docs/reports/census-2026-11-18.md`), 2026-11-18
**Category:** docs
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

githome's G-09 says that the engine is in the payload image, not set by the host. `[unit.microvm] engine = DIR` names a directory of the host.

## Premise

The census read `docs/container-engine.md` and `crates/podrunner-microvm`. Not measured.

## Approach

Decide: ship the engine in `runner-full` and keep `engine = DIR` as an override, or keep the directory. Record the call in `docs/decisions.md` and give it to githome on GitHub #151.

## Prove

The call in `docs/decisions.md`; `cargo todo check`.

# T-518: G-13: each event by name, a rule of trust that can be swapped, and a published table of cases

**Source:** the census of T-491 (`docs/reports/census-2026-11-18.md`), 2026-11-18
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** open

## Problem

githome's G-13 asks for each event by name (also `pull_request_target`), a module of the trust rule that can be swapped, and a table of cases that githome runs. The ruling Q5 allows four extra events, and never trusts `pull_request_target`; `podrunner-tier` has no trait; `crates/podrunner-tier/tests/cases.json` is not a release asset.

## Premise

The census read G-13 on GitHub #66 and the ruling Q5 (`docs/decisions.md`).

## Approach

Compare each clause of G-13 with Q5. A clause that needs a change of Q5 becomes a question for the owner with a recommendation (`TODO/PROGRESS.md`). Put the trust rule behind a trait (the ruling of DR-3: each seam stays swappable). Attach `cases.json` to each release with T-511.

## Prove

`cargo test -p podrunner-tier`; the release assets list `tier-cases.json`.

# T-519: G-03: no document says that a step of the job can read the credential of its runner

**Source:** the census of T-491 (`docs/reports/census-2026-11-18.md`), 2026-11-18
**Category:** docs
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

githome's G-03 asks that the report says so. Only the SPEC and `kinds[].probe.secret_delivery` of `status` say it.

## Premise

The census read G-03 and `docs/contracts/`. Not measured.

## Approach

Say it in `docs/security/residuals.md` and in the section of the run report of `docs/configuration.md`; or record that `secret_delivery` of `status` meets the clause, and tell githome.

## Prove

`cargo todo check`; the documents name it.

## Done

2026-11-18, the commit that names T-519. `docs/security/residuals.md` has a row: each step of a job can read the credential files of its runner on each kind (`secret_delivery: files` in `kinds[].probe` of `status --json`), for the life of one JIT configuration; it is the design of the official runner. `cargo todo check`: the record agrees.

# T-520: G-06: no test of `cpu_s` against a known load, and no tolerance in the contract

**Source:** the census of T-491 (`docs/reports/census-2026-11-18.md`), 2026-11-18
**Category:** measurement
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** partial

## Problem

githome's G-06 asks that the CPU time is within a tolerance. The tests check `cpu_s > 0` only, and the schema names no tolerance.

## Premise

The census read `crates/podrunner-slots/src/usage.rs` and its tests.

## Approach

Run a payload that burns a known CPU time on the podvm and microvm kinds in `runner-image.yml`; check `cpu_s` within 20 %; write the tolerance in the contract of the report.

## Prove

`runner-image.yml`: the step of the CPU measure passes.

In the code (2026-11-18): `docs/contracts/source-driver.md` names the tolerance of `cpu_s` (20 % or 1 s of the CPU time of the payload, the larger) and how each kind measures it. `cargo test -p podrunner-slots cpu_s_is_the_cpu_time_of_the_payload_within_its_tolerance`: a payload of the fake kind (its processes on the host, sampled as the podvm kind's) burns 2 s of CPU in one process, and `cpu_s` is 2 (three runs); the test takes 1 to 3. Open: the step of `runner-image.yml` on the podvm and microvm kinds of the Prove.

# T-521: G-12: the `pids` limit of the podvm kind counts each process of the user, not of one unit

**Source:** the census of T-491 (`docs/reports/census-2026-11-18.md`), 2026-11-18
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** open

## Problem

`[unit.podvm] pids` is `RLIMIT_NPROC`, which counts each process of the uid. Two units of one user share it.

## Premise

The census read `crates/podrunner-podvm`. Not measured.

## Approach

Where the cgroup of the unit is delegated (the path of T-221), set `pids.max` in the leaf of the unit; keep `RLIMIT_NPROC` as the fall back, and say which one holds in `status`.

## Prove

`runner-image.yml`: two units under one user, each with its own `pids`.

# T-522: G-31: the design copies of the schemas have the ids of `pod-job` and other fields

**Source:** the census of T-491 (`docs/reports/census-2026-11-18.md`), 2026-11-18
**Category:** docs
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`docs/design/schemas/*.v1.schema.json` have the same ids as the schemas of `pod-job` and other fields (the design copy of `run-report.v1` has `arch`, `bytes_in`, `bytes_out`, `image_digest`, `limits_applied`). A reader can take the wrong file.

## Premise

The census compared the files. The design copy is call C17: do not edit it.

## Approach

Do not edit the copy. Add a line to `docs/design-issues.md` and to the README of `docs/design/` that the schemas of record are those of `pod-job`, and name them.

## Prove

`cargo todo check`; `sh docs/design/verify.sh`.

## Done

2026-11-18, the commit that names T-522. `docs/design-issues.md` and the table of `docs/design/README.md` say that the schemas of record are those of `pod-job` (`job-offer.v1`, `unit-capability.v1`, `run-report.v1`, `public-report.v1`) and the schema of `status.v1`, and that the copies of `docs/design/schemas/` are the design; no copy is edited. `sh docs/design/verify.sh`: exit 0.

# T-523: G-34: take the signed release index of `pod-release`

**Source:** the census of T-491 (`docs/reports/census-2026-11-18.md`), 2026-11-18
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** blocked

## Problem

githome's G-34 asks for a signed index of each release. A release of podrunner has a `.sha256` for each file and attestations.

## Premise

The census read `release.yml` and `scripts/release-assets.sh`. T-484 holds the interim.

## Approach

When `pod-release` gives the signed index, write it in `release.yml` and check it in the installer.

## Prove

`release.yml` dry run: the index is signed and lists each asset.

## Blocker

`pod-release` of podcrates: PodUtils/podcrates@T-104 and PodUtils/podcrates@T-880.

# T-524: G-04 and G-35: the half of podrunner of the placeholders of a policy

**Source:** the census of T-491 (`docs/reports/census-2026-11-18.md`), 2026-11-18
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** blocked

## Problem

githome's G-04 (its P2 part) and G-35 ask for placeholders and path rules in a policy. podrunner refuses a path rule (`egress-policy-unsupported`).

## Premise

The census read G-04 and G-35 and `docs/non-goals.md` (the TLS end of the broker).

## Approach

When `pod-broker` gives placeholders, pass the placeholders of the offer's policy to the egress module and lift the refusal of path rules.

## Prove

`cargo test -p podrunner-egress` with a policy that has a placeholder.

## Blocker

The TLS end of `pod-broker`: PodUtils/podcrates@T-100.

# T-525: C1 to C5: the acceptance of A-X-01 through `serve` on real units in CI, and the results of githome's host

**Source:** the census of T-491 (`docs/reports/census-2026-11-18.md`), 2026-11-18
**Category:** measurement
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** partial

## Problem

No test runs `serve` on real incus and microvm units for 20 jobs, a burst of 10, a cancel, `status` samples against the schema and a canary beside `key_file` (cfb's checks C1 to C5, GitHub #292). The incus suite skips the credential cases 6, 7, 21 and 24. No entry takes githome's results of C1 to C5.

## Premise

The census read the scripts of #292 and `runner-image.yml`.

## Approach

Add a step to `runner-image.yml` that runs the scripts of C1 to C5 against `serve` with a `program` script source, on each kind. Record the results of githome's host here, and file each defect as an entry.

## Prove

`runner-image.yml`: the step of C1 to C5 passes on the incus and the microvm kinds.

State (2026-11-18): `crates/podrunner-broker-fake/tests/accept.rs` runs C1 to C5 through the slots on the kind of `PODRUNNER_ACCEPT_KIND` (`microvm` at `untrusted`, `incus` at `trusted`): 20 offers in a burst at 2 slots, each job checks that no earlier job left its marker (C1) and finds no canary of the host (an environment variable of the host process, the content and the path of a `key_file`) in `/proc/*/environ`, `/proc/*/cmdline`, `/etc` and `/root` (C5); each tick checks `busy <= slots` (C2) and writes a sample of `status` that `scripts/check-status-samples.py` validates against the schema of `status.v1` (C4); a long job is cancelled 60 s after its mint, and its report must come within 60 s, with `busy` 0 and `degraded` null after (C3). Two steps of `runner-image.yml` run it, as measures until they are green. Left: the green runs, then the steps block; the results of githome's host when githome comes back.

State (2026-11-18, the first run, `runner-image.yml` 44396030197): microvm: two jobs passed, then each runner listened for 600 s with no job. The cause: a runner polls `GET /message` again while it runs its job (`status=Busy`, for a cancel), and the fake broker gave each poll the next queued job, which was then lost (18 of 20). The fake broker now gives one job to each session (`sessionId` of the poll; `a_session_that_got_a_job_takes_no_other`). incus: each offer was `event-not-allowed`: the trusted offer of the test named a public repository, which needs `trusted_public`; it is private now. Next: the next run of both steps.

State (2026-11-19, run 44417916405): on incus each runner was "placed for another job": the broker gives its jobs to any runner, each with a run of its own, so the job-started hook asks `assigned`, which the source of the test did not answer. It now hands `assigned` to the fake source, which keeps a job of the owner of the offer.

State (2026-11-19, `runner-image.yml` run 44428126855, `2d3d73b`): on the microvm kind and on the incus kind each of the 20 jobs of C1 ended `success`, each in a fresh unit with no residue, and each step found no canary (C5). The test then failed on its own count of `podrunner-accept-ok` in `broker.logs()`, which holds the last job only (2 lines, not 20); it counts the jobs that succeeded on the broker now. C2, C3 and C4 come after that assertion: the next run gives them.

# T-526: The incus suite of `runner-image.yml` does not block a release

**Source:** the census of T-491 (`docs/reports/census-2026-11-18.md`), 2026-11-18
**Category:** chore
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

`runner-image.yml` runs the incus step with `continue-on-error: true`. `release.yml` needs a green `runner-image.yml` run, so a red incus suite does not stop a release, and incus is the kind of cfb's trusted jobs.

## Premise

The census read `.github/workflows/runner-image.yml`.

## Approach

Keep `scripts/measure-incus.sh` as a measure. Make the conformance, egress, limits and local suites of incus blocking, with a retry of the install of incus, as the microvm step does.

## Prove

`runner-image.yml` is red when a planted defect fails the incus conformance.

In the code (2026-11-18): the incus step of `runner-image.yml` has no `continue-on-error`: its conformance, egress, limits and local suites block (they passed in run 44302792725); `scripts/measure-incus.sh` and the engine test of T-485 stay measures (a line in the summary); the apt installs go through `scripts/retry.sh`. The new `pids` test of T-488 in the microvm step is a measure too until it is green once. Open: the run with a planted defect of the Prove.

In the code (2026-11-18, the plant): `runner-image.yml` takes the input `plant` (`none` or `incus-conformance`) of a dispatch; with `incus-conformance` the fixture of `crates/podrunner-incus/tests/conformance.rs` gives each exit payload a wrong exit code (`PODRUNNER_PLANT`). Next: a dispatch with `plant: incus-conformance` must end red in the incus step.

## Done

2026-11-19: the dispatch of `runner-image.yml` with `plant: incus-conformance` (run 44417916405, `9fa28c2`) ended red, in the incus step only: `unit  8 wait-gives-the-exit-code: FAIL: wait gives Some(Code(8)), not exit code 7`. The incus suites block a release (`release.yml` needs a green `runner-image.yml` run of its commit).

# T-527: No lane of the user-path gate for the host class of githome's pool

**Source:** the census of T-491 (`docs/reports/census-2026-11-18.md`), 2026-11-18
**Category:** chore
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** partial

## Problem

Each lane of `user-gate.yml` runs `podrunner run`. No lane runs `serve` with a `program` source, and no lane has incus. `AGENTS.md` section 12: each host class becomes a lane.

## Premise

The census read `user-gate.yml` and `scripts/user-gate.py`.

## Approach

Add a lane `serve-incus-kvm`: install as a user with no root, then `check`, `doctor --json` (incus `confined`, microvm `guest`, hold `enforced`), `print-unit`, and `serve` with one job on each kind. After T-507 the lane uses the install of `serve`.

## Prove

`user-gate.yml`: the lane passes.

State (2026-11-18): `user-gate.yml` has the lane `serve-incus-kvm`: incusd from apt, KVM, and the user of the lane in the group `incus-admin` with no root. After the install, `scripts/user-gate.py` runs for each kind (`--serve-kinds incus,microvm`) the check `serve:check:KIND` (`config init` with the released fake source as its program source, `check`, `doctor --config --json` with the boundary `confined` for incus and `guest` for microvm and each source served, `print-unit`) and `serve:run:KIND` (`serve` starts and stops on SIGTERM; on microvm it runs one sandbox offer of the image to a report of `success` in the journal). A job of a runner through `serve` needs a broker: the released `pod-fake-github` of podcrates (PodUtils/podcrates@T-921, its next tag) gives one. Left: the green run of the lane, then one runner job on each kind with `pod-fake-github`.

State (2026-11-18, run 44416704184): `config init` exited 78 for both kinds: the driver check refused `~/.local/share` (T-537). Next: the run after the fix of the installer.

A runner job through `serve` needs a source whose `mint` reaches a broker: `pod-fake-githome` mints made-up files, `pod-fake-github` has the JIT route and the broker. Asked: COORD-101 (GitHub #311), `pod-fake-githome serve --github URL`.

# T-528: No row of the user-path gate copies the gate of rbg

**Source:** the census of T-491 (`docs/reports/census-2026-11-18.md`), 2026-11-18
**Category:** measurement
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** partial

## Problem

cfb's answer to Q-13 (GitHub #292) asks for the gate of rbg (npm 12, Node 26 or later, Playwright 1.64.1, TypeScript 7) as the base line of T-480 and T-500. `scripts/corpus/user-gate.tsv` has no such row.

## Premise

The census read the corpus and cfb's answer.

## Approach

Add a pinned stand-in row on the microvm and incus kinds that records `claim_to_first_step_s` and the cold install time; make it the Prove of T-480 and T-500.

## Prove

`user-gate.yml`: the row passes and the summary gives both times.

State (2026-11-18): `.github/workflows/rbg-stand-in.yml` is the stand-in: Node 26 (setup-node v7.1.0), npm 12.2.3, TypeScript 7.0.2 and `@playwright/test` 1.64.1, a cold install, `tsc --strict`, and a page in headless Chromium. On github.com (run 44400362506) the job took 33 s and the cold install 12 s. The rows `rbg-gate` of `scripts/corpus/user-gate.tsv` run it at `14bc880` on the podvm, runner-dir and microvm executors in the lane `plain`; `scripts/user-gate.py` gives each row the seconds from the start of `run` to the first step and the time of the install (`podrunner-gate-NAME=N` lines). It is the base line of T-480 and T-500. Left: the green run of the rows, and a row on the incus kind (the user-path gate has no incus executor of `run`; the lane `serve-incus-kvm` of T-527 is the place).

State (2026-11-18, run 44416704184): each row `rbg-gate` exited 78: the workflow takes `workflow_dispatch` only, and `run` gives `push` by default; the rows give `--event workflow_dispatch` now. Next: the next run.


# T-529: The disk limit of an offer lifts the bound of the operator

**Source:** the census of T-491 (`docs/reports/census-2026-11-18.md`), 2026-11-18
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`crates/podrunner-slots/src/disk.rs` took the `disk` of the offer first and `disk_max_bytes` only when the offer had none. An offer with a larger value lifted the bound of the operator. Each other limit takes the lesser of the two (T-198), and an offer only narrows (T-107).

## Premise

The census of T-491 read the code; the test below reproduces it.

## Approach

Take the lesser of the two bounds; correct `docs/configuration.md`.

## Prove

`cargo test -p podrunner-slots the_disk_of_an_offer_does_not_lift_the_bound_of_the_kind`.

## Done

2026-11-18: the disk check takes the lesser of the offer and `disk_max_bytes`. Test `the_disk_of_an_offer_does_not_lift_the_bound_of_the_kind` fails on the code before the fix (the unit writes 300000 bytes under a bound of 100000 and an offer of 10000000, and runs past 120 ticks). `docs/configuration.md` says the rule.

# T-530: The job-started hook of a podvm unit cannot ask serve

**Source:** the work of T-493 (githome's G-15), 2026-11-18
**Category:** feature
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The hook asks `serve` through the proxy variables of its unit (T-493). The guests of the microvm and incus kinds set them to the channel of the egress socket. A unit of the podvm kind gets `PODRUNNER_EGRESS` (`http://unix:PATH`) and no proxy variable, so its hook ends each job of another run, as before T-493.

## Premise

`crates/podrunner-slots/src/run.rs`, the guest scripts of `crates/podrunner-microvm` and `crates/podrunner-incus`, read on 2026-11-18. githome sends its jobs to the incus and microvm kinds; the podvm kind is not on its path now.

## Approach

Let the hook take `PODRUNNER_EGRESS` when it names a unix socket (node `http.request` with `socketPath`), or ask through the control socket of a unit with `container: hooks` (`PODRUNNER_CTL`, a route `/v1/assigned` of `podrunner-hook`). Test it with a podvm unit in `runner-image.yml`.

## Prove

`cargo test -p podrunner-broker-fake --test runner -- --ignored` with `PODRUNNER_EGRESS` of a unix socket and no proxy variable: the hook asks and `keep` runs the job.

## Done

2026-11-18, the commit that names T-530. With no proxy variable, the hook asks through the unix socket that `PODRUNNER_EGRESS` names (`http://unix:PATH`; node `http.request` with `socketPath`), at the same front (`images/runner/podrunner-job-started.sh`). `PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner-broker-fake --test runner -- --ignored the_job_started_hook`: 2 passed; the new test `the_job_started_hook_asks_through_the_egress_socket` fails on the hook before (the job ends `failed`). Level: proven here with the official runner and a stand-in of the front; not run in a real podvm unit (the run of T-493 on a real unit).

# T-531: Take rsa 0.10 in the fake broker

**Source:** Dependabot GitHub #299 (rsa 0.9.10 to 0.10.0), 2026-11-18
**Category:** chore
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

Dependabot proposes rsa 0.10.0 (#299). The gate of its branch fails: rsa 0.10 changes the API that the fake broker uses to build the key of a runner and to sign a token (`BigUint`, `RsaPrivateKey::from_components`, `Pkcs1v15Sign`, the traits `PrivateKeyParts` and `PublicKeyParts`).

## Premise

The diff of #299 and the red gate run of its branch, read on 2026-11-18; the uses are in `crates/podrunner-broker-fake/src/auth.rs` and `crates/podrunner-broker-fake/src/runner.rs`. rsa is a dependency of the fake broker only: the released binary signs with `ring`.

## Approach

Move the two files to the API of rsa 0.10 (its `BoxedUint`, the new constructors and the signing traits), raise the version in `Cargo.toml`, and apply it as a commit on `main`; close #299 with a link to the commit.

## Prove

`cargo test -p podrunner-broker-fake`; with the official runner, `cargo test -p podrunner-broker-fake --test runner -- --ignored` (the runner accepts the signed token).

## Done

2026-11-18: the fake broker takes rsa `0.10.0`: `rsa::PrivateKey::new` with `getrandom::SysRng` (getrandom `0.4`, feature `sys_rng`; rsa no longer takes the `OsRng` of rand_core `0.6`), the numbers as `BoxedUint` (`to_be_bytes_trimmed_vartime`, `from_be_slice_vartime`), and `Pkcs1v15Sign` under the feature `pkcs1v15`. The workspace drops `rand_core`; the dev profile optimises `crypto-bigint` and `crypto-primes` in place of `num-bigint-dig`. `cargo test -p podrunner-broker-fake`, and `cargo test -p podrunner-broker-fake --test runner -- --ignored` with the official runner (it accepts the signed token), pass. Dependabot #299 closes with a link to the commit.

# T-532: The containers of a podvm unit on the namespace rung, with a volume base

**Source:** podvm's answer to COORD-090 (COORD-189, PodUtils/podvm#585), 2026-11-18
**Category:** feature
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** open

## Problem

The podvm kind gives the route of the container hooks only on the supervise and userland rungs (`crates/podrunner-podvm/src/probe.rs`: "podvm gives volumes on the supervise rung only", U19, T-034). podvm says that `run -d -v` binds on the namespace rung since `0.2.0-beta.25` (PodUtils/podvm@T-1920), and that `--podvm-volume-base` holds there since `0.2.0-beta.32`. So on a host with user namespaces the containers of a job can run on the namespace rung, where the flag closes the race of T-403 (a step makes a mount source a link after the check).

## Premise

COORD-189, read on 2026-11-18. Not measured: the container suite of the hooks on the namespace rung.

## Approach

1. Measure in `runner-image.yml`: the container suite of the hooks (`podrunner-broker-fake --test container`) in a podvm unit on the namespace rung, with `--podvm-volume-base` of the runner root on each `run -d` of a container.
2. If it passes, the probe says `container: hooks` on the namespace rung too, and each `-v` of a container under the runner root goes with the flag; correct U19 in `docs/unknowns.md`.
3. Then T-403 is done on that rung; the supervise and userland rungs wait for PodUtils/podvm@T-1972.

## Prove

`runner-image.yml`: the container suite passes on the namespace rung; a test of the hooks where a step swaps a mount source for a link after the check gets exit 125 of podvm and `container-option-refused`.

# T-533: A job of another architecture in a guest of the machine tier

**Source:** podvm's answer for U26 (COORD-187, PodUtils/podvm#583, 2026-11-18); the measure of T-490 (`runner-image.yml` run 44302792725)
**Category:** measurement
**Milestone:** later
**Priority:** P2
**Effort:** M
**Status:** open

## Problem

podvm runs a `linux/arm64` guest on an x86_64 host in the machine tier (TCG, no KVM) since `0.2.0-beta.22`: the measure step of `runner-image.yml` booted `busybox:1.37.0` for `linux/arm64` in 10 s and read `uname -m` `aarch64`. podrunner refuses a label of another architecture unless a `--platform` route names it, and no run measured the official runner of arm64 in such a guest: the start of .NET under TCG and the time of a small job are not known (U26).

## Premise

`scripts/measure-foreign.sh`, the step "A guest of another architecture (a measure)" of `runner-image.yml`, and its output of run 44302792725, read on 2026-11-18. `crates/podrunner-local/src/dispatch.rs` (`other_arch`). podvm's comment on PodUtils/podvm#583.

## Approach

1. In `runner-image.yml`, give the x86_64 job the arm64 runner image archive of the aarch64 job (an artifact), load it into a store of its own, and run `Runner.Listener --version` and a canned job of the fake broker in a `linux/arm64` guest; print the seconds of each.
2. Write the result into `docs/unknowns.md` (U26).
3. If a job ends in a usable time, let a `--platform` route of the machine tier take a label of the other architecture, and name the cost in `docs/local-mode.md`.

## Prove

The measure step prints `foreign runner ok` and the seconds of a canned job; `docs/unknowns.md` U26 names the run.

# T-534: The probe of the incus kind claims the kind on a host with no incusd

**Source:** the work of T-507, 2026-11-18
**Category:** defect
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`IncusUnit::probe` gives a fixed capability record, and `incus_kind::plan` checks only the version of podvm. So `doctor --config` named the incus kind on a host with no incusd, `config init --kinds auto` would take it, and `serve` starts with the kind and each job on it fails at its start.

## Premise

`crates/podrunner-incus/src/lib.rs` (`probe`), `crates/podrunner/src/incus_kind.rs` (`plan`, `build`) and `crates/podrunner/src/doctor.rs` (`capability`), read on 2026-11-18.

## Approach

1. `doctor` and `config init`: the kind needs a socket of incusd (`PODVM_INCUS_SOCKET`, else `unix.socket` of `INCUS_DIR`, else of `/var/lib/incus`) that takes a connection of this user.
2. `serve`: the same check at the build of the kind, so that a host with no incusd starts with no incus kind and one line that names the socket; `check` stays free of side effects and does not connect.

## Prove

`cargo test -p podrunner --test config_init` (no incusd: no incus kind); a test of `serve` with an `[unit.incus]` and no incusd names the socket.


## Done

2026-11-18, `1fff2fe` (step 1) and the commit that names T-534 (step 2). `doctor::capability` (used by `doctor` and `config init`) needs a connection to the socket of incusd, found as podvm finds it (`PODVM_INCUS_SOCKET`, else `unix.socket` of `INCUS_DIR`, else of `/var/lib/incus`) for the incus kind; `serve` builds no incus kind with no incusd, runs each other kind, and writes one line `[unit.incus]: not here: incusd: PATH: ...`; `check` connects to nothing. Tests: `cargo test -p podrunner doctor::tests::doctor_knows_the_microvm_and_incus_kinds` (no socket: refused; a listener: the record), `cargo test -p podrunner --test config_init`, `cargo test -p podrunner --test serve_incus` (status names the fake kind only, and stderr names the socket). Level: proven here with stand-ins.

# T-535: The tests leave their temporary directories in /tmp

**Source:** the work of T-515, 2026-11-18 (a full `check.sh` found `/tmp` full)
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** partial

## Problem

After a day of `sh scripts/agent/check.sh` runs on one host, `/tmp` held 3172 directories `podrunner-*` of the tests (2190 of `podrunner-remote`, 164 of `podrunner-front`, 122 of `podrunner-envfile`, 56 of each `podrunner-qa6-runner-*`, 56 of `podrunner-egress-*`): a test that fails, or one that does not remove its directory, leaves it. They held 34 MB; the disk was full for other reasons, but a host of CI that runs the tests often fills.

## Premise

`ls /tmp/podrunner-*` on the host of the session, 2026-11-18.

## Approach

Give each test a directory that goes when the test ends, also on a panic (a guard with `Drop`, or `tempfile`), in the crates of the prefixes above; a test of `check.sh` counts the directories `podrunner-*` of `/tmp` before and after a run.

## Prove

Two runs of `cargo test --workspace` leave no new directory `/tmp/podrunner-*`.

State (2026-11-18): the tests of `podrunner-remote` (one directory for each gate of the conformance fixture, about 700 of each run of `check.sh`) take a guard `tests::Dir` that removes its directory on drop, also on a panic; the fixture holds one base directory for its gates. `cargo test -p podrunner-remote` leaves no directory `/tmp/podrunner-remote-*`. Left: the directories of one process that tests share while they run (`podrunner-qa6-runner-*`, `podrunner-program-*`, `podrunner-tier-*`, `podrunner-podvm-plan-*`, `podrunner-front-*`: one each for each run), and the count in `check.sh`.

# T-536: Take `pod_core::http1` in place of the reader of an HTTP/1.1 head of podrunner-hook

**Source:** podcrates `v0.72.0` (COORD-138, PodUtils/podcrates#229; PodUtils/podcrates@T-882; COORD-131, the ruling "Shared code"), 2026-11-18
**Category:** chore
**Milestone:** later
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`crates/podrunner-hook/src/http.rs` reads the head of a request by its own lines and checks. The control socket of a unit, the fake broker, the remote gate and the web front of the local mode use it. Two readers that read one head two ways can let a request past one of them.

## Premise

`pod-core/src/http1` at `v0.72.0`, read on 2026-11-18.

## Approach

Read the head to its empty line with `pod_core::http1::find_request` (the body stays in the reader), parse it with `parse_request`, and keep the checks of podrunner: the form of the path, no `Transfer-Encoding`, one `Content-Length`, the bound of the body. The reason phrases come from `http1::reason`.

## Prove

`cargo test -p podrunner-hook`, `cargo test -p podrunner-broker-fake`, `cargo test -p podrunner-remote`.

## Done

2026-11-18: `http.rs` reads the head with `pod_core::http1` (at most 16 KiB and 65 lines). Changes of the answers: another version of HTTP gets 505 (was 400), and a head over 16 KiB gets 431 (a line over 8 KiB got 400). `the_reader_holds_its_limits` holds the new statuses. `cargo test -p podrunner-hook` passes; the full `check.sh` passes.

# T-537: The installer leaves the data directory writable by the group, and the microvm kind refuses its podvm

**Source:** `user-gate.yml` run 44393864747, lane `plain` (job 135128949293), 2026-11-18
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

In the lane `plain`, `hello:microvm` and the microvm row of the corpus exit 78: `[unit.microvm] binary: ~/.local/share/podrunner is writable by the group or the others`. Since T-263 (`check_driver`) a driver needs each directory above it closed to the group and the others. The installer removes that write from `bin` and `data/bin` only; a data directory that was there before (or made with a umask of 002) stays open.

## Premise

The log of the lane, read on 2026-11-18. Which step of the lane made the data directory open is not known.

## Approach

`chmod go-w` the data directory too, beside `bin` and `data/bin`; the case 4b of `scripts/test-install.sh` plants an open data directory.

## Prove

`sh scripts/test-install.sh` (case 5: each of the three directories is closed after the install); the next run of `user-gate.yml`: `hello:microvm` passes in the lane `plain`.

## Done

2026-11-18: `scripts/install.sh` removes the write of the group and the others from `$data` as well; `sh scripts/test-install.sh` passes with an open data directory planted. The run of `user-gate.yml` that proves it in the lane `plain` is the next one.

State (2026-11-18, later): `user-gate.yml` run 44416704184: the lane `plain` then refused `~/.local/share`, the next directory up (a umask of 002 leaves it open; a desktop makes it so too). The installer now closes each directory of the user from `bin` and `data/bin` up to `$HOME` (not `$HOME` itself), with one line for each; case 5 of `sh scripts/test-install.sh` plants each of them open.

State (2026-11-19, `user-gate.yml` run 44428133678): the check then refused the home of the lane itself, which `scripts/user-gate.py` made under the umask 002 of the lab user. The installer leaves `$HOME` as it is (a home is the user's); the harness now makes the home of the lane and its work directory with mode 0755, as a home is.

