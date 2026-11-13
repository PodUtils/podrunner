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

State (2026-11-13): step 1 is done: the census of the verbs and the design (`docs/container-engine.md`): the runner runs 15 verbs of the `docker` command line, and Docker-in-Docker needs the whole Engine API, so a real engine runs inside a guest with its socket there, and no shim (`pod-dockerapi`) is needed. Step 2 is done: podvm `v0.2.0-beta.10` gives a guest with no NIC and one host channel (GitHub #83 is closed; the microvm kind uses it, T-037). Next: step 3, the runner image with an engine and the value `engine` of `ContainerRoute` (a COORD issue to podcrates).

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
**Status:** partial

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
**Status:** blocked

## Problem

githome takes `cpu_s` and `peak_rss_mib` from `run-report.v1` for the record of a job and for its quotas. `RunReport` of `pod-job` (podcrates `v0.39.0`) has neither field, and slots measures neither.

## Premise

Read the comment of githome on GitHub #66 (2026-11-13) and the report module of `pod-job` of podcrates `v0.39.0`.

## Approach

Ask podcrates for `cpu_s` and `peak_rss_mib` in `RunReport` and `PublicReport`, each a number or `unknown` (GitHub #86, COORD-026). Then slots samples each unit at each tick: the sum of `utime` and `stime` of each process with the tag of the unit (`/proc/PID/stat`), and the highest sum of their RSS. A kind whose processes are not on the host (`incus`) reports `unknown`.

## Prove

`cargo test -p podrunner-slots report`: a unit that spins reports a `cpu_s` above 0 and a `peak_rss_mib`; the `incus` kind reports `unknown`.

## Blocker

podcrates: the two fields in `pod-job` (GitHub #86, COORD-026). podcrates did the work on its `main` (PodUtils/podcrates@T-527, `ed9b2d9`); the entry waits for the tag `v0.40.0`.

# T-109: The incus kind has no egress hold, so placement gives it no trusted job

**Source:** T-105 (a read of `crates/podrunner-incus/src/lib.rs` and `crates/podrunner-slots/src/placement.rs`, 2026-11-13)
**Category:** defect
**Milestone:** later
**Priority:** P1
**Effort:** M
**Status:** blocked

## Problem

The `incus` kind claims `confined`, the floor of `trusted`, and its `probe` gives `net: nat` and `egress_hold: none`: an instance uses the default bridge of incusd with NAT. Placement asks the hold `advisory` or more for a `trusted` job (`needed_hold`), so it refuses each `trusted` offer on the kind with `hold-not-available`. The kind then serves no job of a forge, and G-02 of githome (GitHub #66) is not met. An instance with NAT also has a way out that does not pass the egress module.

## Premise

Read `crates/podrunner-incus/src/lib.rs` (`probe`), `crates/podrunner/src/incus_kind.rs` and `crates/podrunner-slots/src/placement.rs` (`needed_hold`) on 2026-11-13. Not measured: a `trusted` offer under `serve` with only `[unit.incus]`.

## Approach

- Write a slots test first: a `trusted` offer and only a kind with the record of `incus` is refused with `hold-not-available`.
- Give the instance no NIC and one way out to the egress socket of the unit, as the `microvm` kind has: an incus `proxy` device from a port on the loopback of the instance to the socket, and the proxy variables set by the script of the instance. Then the kind claims `enforced`.
- If the incus tier of podvm cannot give that (no `--podvm-net none` and no proxy device), ask podvm by a COORD issue `(to podvm)` and set this entry `blocked`.

## Prove

`cargo test -p podrunner-slots placement`, then `cargo test -p podrunner-incus --test conformance -- --ignored` on a host with incus: a payload reaches an allowed host through the egress socket and nothing else.

## Blocker

podvm: `--podvm-net none` and `--podvm-channel` on the incus tier (GitHub #88, COORD-028). podvm answered in PodUtils/podvm#316 (COORD-082): both flags on the incus tier (PodUtils/podvm@T-1779), in `v0.2.0-beta.12`. The code is in: the kind passes `--podvm-net none`, and with a socket directory `--podvm-channel SOCKETS/egress.sock:127.0.0.1:3128` and `PODRUNNER_CHANNEL_PORT`; the script sets the proxy variables; `probe` says `net: none` and `enforced`; `serve` needs podvm `LOWEST_INCUS` (`0.2.0-beta.12`). `the_instance_has_no_nic_and_one_channel_to_the_egress_socket` holds the argv. Next: pin beta.12, then the proof on a hosted runner (a payload reaches the socket and nothing else). The slots side holds already: `a_kind_with_no_hold_gets_no_trusted_job` refuses a `trusted` offer on a kind with no hold, as the record of `incus` has.

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

