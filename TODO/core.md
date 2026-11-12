Milestone M-A: the seams, the schemas, the tier rule, the security documents, the fakes, the slots core, the CLI, the podvm driver, the egress module and the gate of M-A (GitHub #16 to #25).

# T-016: WP-01: the seam traits, the contracts v1 and the conformance harness (GitHub #16)

**Source:** GitHub #16
**Category:** feature
**Milestone:** M-A
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

The text of GitHub #16 ("WP-01: seam traits, contracts v1 and the conformance harness"), cleaned:

**What and why.** podrunner has two seams. Each seam has two implementations in v1, so each is a protocol with a conformance suite.

- **Unit seam.** A Rust trait in v1. Operations: `probe`, `prepare`, `start`, `signal`, `wait`, `destroy`, and the container operations `container_start`, `container_exec`, `container_logs`, `container_inspect`, `container_stop`. Postconditions: `probe` is idempotent and `unknown` means absent; `prepare` takes a digest, never a tag, and refuses a foreign platform unless emulation is named; `start` gives a fresh root and store, copies the credential files into the runner root at mode 0600, and puts no credential in argv or the environment; `signal` reaches the whole tree; after `destroy` no process and no file of the unit remain, and the driver counts them. A JSON-lines wire for out-of-tree drivers is deferred (#10).
- **Source seam.** JSON lines from day one, because the scale-set sidecar is Go. Operations: `hello`, `next` (long poll with `capacity`; events `offer`, `cancel`, `completed`, `started`), `ack`, `claim`, `mint`, `report`, `audit`, `withdraw`, `release`. Every object starts with `"schema": 1`. A `job_id` matches `[A-Za-z0-9._-]{1,128}` and is never a path component. Unit ids are `u-` plus a counter and never derive from a `job_id`.
- **Hook contract.** The interface of the runner's container hooks (`prepare_job`, `cleanup_job`, `run_script_step`, `run_container_step`; the response shape with `isAlpine` and a `services` array), as read from the runner's `HookInput.cs` and `HookResponse.cs`.

**Content.** crates/podrunner-unit and crates/podrunner-source (traits and types), docs/contracts/unit-driver.md, `source-driver.md`, `hook.md`, and a harness that runs the unit conformance tests (1 to 17) and the source conformance tests (1 to 12) against any implementation, with a planted defect and a correct input for each.

**Acceptance.**

- The harness runs both suites against a stub driver that fails every test, and reports each test by number.
- The source wire parser has a fuzz target and refuses a line over 1 MiB, an unknown `op` (`bad-request`) and a `job_id` of `../x`.
- The contracts in docs/contracts/ state version 1 and match the trait (a test compares the operation names).

Dependencies: #11, #12. Estimate (spec): S.

## Premise

Read the text of GitHub #16 on 2026-11-11. Nothing measured.

## Approach

The crates `podrunner-unit` and `podrunner-source` with the traits and the types; docs/contracts/ with the unit, source and hook contracts (version 1); a harness that runs the unit tests 1 to 17 and the source tests 1 to 12 against an implementation, each test with a planted defect and a correct input. The wire parser of the source refuses a line over 1 MiB, an unknown `op` and a bad `job_id`.

## Prove

`cargo test -p podrunner-unit -p podrunner-source`

## Done

2026-11-11, `229d29f`; gate run 42936703374 of `51ffb40` is green. `crates/podrunner-unit` (the trait, tests 1 to 17, the stub) and `crates/podrunner-source` (the wire, the trait, `WireSource`, `serve_line`, tests 1 to 12). On the stub and on a refuser, each test fails and is reported by its number. The wire refuses a line over 1 MiB, an unknown `op` and a `job_id` of `../x` (`bad-request`). The fuzz target `source_wire` survives its seeds and 5000 mutations. Two tests compare `docs/contracts/` with the operations and the tests. `cargo test -p podrunner-unit -p podrunner-source`: 12 passed.

# T-017: Schemas v1: job offer, unit capability, run report, public report (GitHub #17)

**Source:** GitHub #17
**Category:** feature
**Milestone:** M-A
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

The text of GitHub #17 ("Schemas v1: job offer, unit capability, run report, public report"), cleaned:

**What and why.** Four records cross the seams. githome and cfb pin one schema (S14 of the spec). Ruling Q-A8 (2026-11-09) puts JSON Schemas in podcrates `schemas/`, generated from crate types. So podrunner drafts the types and the schemas here, and the authoritative copy moves to podcrates.

- `job-offer.v1`: `trust_inputs` (`event`, `head_is_fork`, `actor_class`, `repo_visibility`, `owner`, `repo`, `run_id`, `ref`, `workflow_ref`, optional `owner_id`, `repo_id`), `labels`, `arch`, `job_id`, `source`, `limits`, `limits_required`, `policy` (the `pod-broker` policy schema), `wait_deadline_s`, optional `payload` and `unit_need`. No `tier` field. No `payload` from a `github-scaleset` source.
- `unit-capability.v1`: `kind`, `arch`, `boundary`, `boundary_proof`, `principal`, `container`, `net`, `secret_delivery` (`files`, `socket`; no `env`), `credential_store`, `mount_modes`, `rootfs_fresh`, `tree_kill`, `limits`, `egress_hold`, `kvm`. No `isolated` value.
- `run-report.v1` and `run-report-public.v1` (the subset for a source that is not `operator_controlled`: `job_id`, `source`, `tier`, `unit_kind`, `boundary`, `wall_s`, `exit_reason`, refusal codes; never `host`).

**Content.** Rust types with serde, the four schemas, 11 valid and 19 invalid examples with an expected-reason map, and a test that validates each.

**Crate dependencies (crate item R-C4 of the study, merged here).** podcrates ask: PodUtils/podvm#227, section 2.4 (WP-C3). podcrates `schemas/` holds only `log.v1` and `broker-policy.v1` today. M-A keeps these schemas in podrunner. When podcrates releases the types and schemas, podrunner switches to them and removes its copies.

**Acceptance.**

- Each valid example passes and each invalid example fails for its named reason.
- A report for a refused job has no unit fields; a refused report needs `refusal_code`.
- The refusal code enum of the report schemas equals the code table of #21 (a test).
- A COORD issue `(to podcrates)` hands the types and schemas to WP-C3, or PodUtils/podvm#227 carries them.
- After the podcrates release: podrunner uses the podcrates types, and no second copy of a schema stays here.

Dependencies: #16. Cross-repo: PodUtils/podvm#227 section 2.4; the `pod-broker` policy schema. Estimate (mine): S.

## Premise

Read the text of GitHub #17 on 2026-11-11. Nothing measured.

## Approach

Draft the four records and their schemas, then use the released types of podcrates (ruling Q-A8, PodUtils/podcrates@T-163). podcrates released them as `pod-job` in v0.17.0 (PodUtils/podcrates#58). `crates/podrunner-schema` re-exports `pod_job` and keeps only the credential (`secret.rs`). The four schemas, the examples and their test are in podcrates. `schemas/status.v1.json` stays here: it embeds the capability record of `pod-job`. `docs/contracts/codes.md` points to the code table of `pod-job` and lists the uses of podrunner beyond it.

## Prove

`cargo test -p podrunner-slots status_schema`, `cargo test --workspace` (the schemas and their examples: the tests of `pod-job` in podcrates, at the tag)

## Done

2026-11-11: podrunner takes `pod-job` at podcrates v0.17.0; the draft types, the four schemas and the examples are removed. `cargo test --workspace --all-features`: each test passes; `schemas/status.v1.json` is generated again from the types (`PODRUNNER_WRITE_SCHEMAS=1`). Gate run 42953651407 (`0457ed0`) is green.

# T-018: The tier rule: Rust implementation and the case table (GitHub #18)

**Source:** GitHub #18
**Category:** feature
**Milestone:** M-A
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

The text of GitHub #18 ("Tier rule: Rust implementation and the case table"), cleaned:

**What and why.** Trust comes from job metadata, never from a label. The workflow author chooses `runs-on`, so a fork job can name the trusted set. The rule is default deny:

- `trusted` iff all of: the repository is private or internal, or listed in `trusted_public`; `owner` is a configured organization; `head_is_fork` is false; `actor_class` is `member`; `event` is one of `push`, `pull_request`, `workflow_dispatch`, `schedule`, `merge_group`.
- `untrusted` iff the job is not trusted, `event` is `pull_request` and `head_is_fork` is true.
- Else refused with `event-not-allowed`. That covers `pull_request_target`, `workflow_run`, `issue_comment`, `release`, `repository_dispatch`, `other`, every `unknown` value, a same-repository pull request by a non-member, and an event of an unlisted public repository.
- `local` is never computed; only configuration marks a set `local`.
- Intake check: a computed tier less trusted than the set's tier is `tier-mismatch`. Both refusals happen before the claim. A job more trusted than its set runs under the set's stricter tier. When the offer carries `owner_id` and `repo_id`, compare ids too.

**Content.** A pure function in crates/podrunner-tier and the case table (`tier-cases.json` of the spec, carried by #14) as a test vector file.

**Acceptance.**

- Every row of the case table passes.
- Planted defects fail: an `unknown` input gives `trusted`; a fork offer on the trusted set is accepted; `pull_request_target` from a member is accepted.
- A property test: no input with an `unknown` field computes to `trusted`.

Dependencies: #17. Estimate (mine): S.

## Premise

Read the text of GitHub #18 on 2026-11-11. Nothing measured.

## Approach

A pure function from the trust inputs of an offer and the configuration to a tier, with the case table of the issue as a table test; each case has a planted defect.

## Prove

`cargo test -p podrunner-tier`

## Done

2026-11-11, commit `33e6779`; the gate run 42938706745 of `f72f679` is green. `crates/podrunner-tier`: `compute` and `intake`. `cargo test -p podrunner-tier`: 4 passed: each of the 32 rows of `crates/podrunner-tier/tests/cases.json` passes; over 1296 inputs, no input with an `unknown` field is `trusted` and none is `local`; the three planted rules (an `unknown` read as good, no intake check, `pull_request_target` from a member) each fail rows. A `local` set runs its jobs at `local` with no rule (call C14).


# T-019: Security documents: threat model, accepted residuals, leak runbook (GitHub #19)

**Source:** GitHub #19
**Category:** docs
**Milestone:** M-A
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The text of GitHub #19 ("Security documents: threat model, accepted residuals, leak runbook"), cleaned:

**What and why.** The design accepts stated residuals (DR-4, #4) and needs an operator runbook for a leaked key. These must be in the repository before the first live job.

**Content.** docs/security/:

- Threat model: twelve actors (fork-PR author; compromised dependency in a trusted job; co-tenant job on one uid; leaked source credential; hostile container request; hostile hook input; hostile proxy or DNS; holder of a stolen host token; compromised `mint` host or sidecar; job with secrets or an unvetted author; observer on the path to a remote `mint`; trusted job that kills its shim). Each row: capability, controls, verdict.
- Process rule: no process that parses job-controlled bytes holds a source credential. In v1 the modules share one process, so this holds only for the sidecar; the split is deferred (#10).
- Credentials: source credential (App key, behind a `sign` module that signs one JWT shape, rate limited); session and host tokens (memory); per-job runner credential (three files).
- The App key also edits runner groups. Leak runbook: rotate the key; delete runners and scale sets that `status` does not list; install the App on named repositories only; check that each trusted runner group refuses public repositories; alert on a registration whose name is not in a host-written list; revoke a host token or key at `mint`.
- Every job-controlled string prints only after control characters become `\xNN` and the string is cut at 200 bytes.

**Acceptance.**

- The documents exist and each control names the issue that builds it.
- #4 links the residual list.

Dependencies: #14. Estimate (mine): S.

## Premise

Read the text of GitHub #19 on 2026-11-11. Nothing measured.

## Approach

Write docs/security/threat-model.md, docs/security/residuals.md and docs/security/leak-runbook.md from the issue, and link them from `SECURITY.md`.

## Prove

`cargo todo check`

## Done

2026-11-11, `fe35f58`. `docs/security/threat-model.md` (the credentials, the
process rule, the twelve actors, each with its capability, its controls
with the entry that builds each, and its verdict; the rule for text of a
job), `docs/security/residuals.md` (R1 to R5, each with the decision that
accepts it and what removes it) and `docs/security/leak-runbook.md`;
`SECURITY.md` links them. GitHub #4 links the residual list (a comment).
`cargo todo check` agrees.

# T-020: WP-02: the fake source and the fake unit, scripted, with a manual clock (GitHub #20)

**Source:** GitHub #20
**Category:** feature
**Milestone:** M-A
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

The text of GitHub #20 ("WP-02: fake source and fake unit, scripted, with a manual clock"), cleaned:

**What and why.** Level 1 acceptance runs offline in CI everywhere. `pod-testkit` holds no stand-in for GitHub, and new projects own their fakes (PodUtils/podcrates@T-106). G-10 asks for a fake forge source.

**Content.**

- `fake` source: reads a script of timed events (`offer`, `cancel`, redelivery, `mint` error, `started` with a given runner name), gives a canned credential.
- `fake` unit: declares any capability record; reads a script of behaviours for each unit (`exit N after T`, `fork-setsid-grandchild`, `fail start`, `residue N`) and spawns real processes where a tree kill needs them.
- A clock trait. Level 1 uses a manual clock, so no test waits real time.
- The script formats are documented.
- `fake` is refused in production unless `allow_fake = true`, and the report shows it.

**Fakes stay here (crate item R-C10 of the study, partly).** PodUtils/podcrates@T-106 keeps the fakes of GitHub in each project; no ask to podcrates (PodUtils/podvm#227, section 2.7, note 2).

**Acceptance.**

- The fake unit passes unit conformance tests 1 to 7, 10, 12 and 15 through the #16 harness.
- The fake source passes source conformance tests 1 to 5, 10 and 11.
- A fake that claims `guest` fails the guest self-test once #36 exists.

Dependencies: #16. Estimate (spec): S to M.

## Premise

Read the text of GitHub #20 on 2026-11-11. Nothing measured.

## Approach

A `fake` source and a `fake` unit that read scripts of timed events and behaviours; a clock trait with a manual clock; `fake` refused in production unless `allow_fake = true`. Each passes its conformance tests through the harness of T-016. The fakes are also the correct inputs and the planted defects of the harness of T-016: for each unit test 1 to 17 and each source test 1 to 12, a fake with one planted defect fails that test, and the correct fake passes it (a test of the crate podrunner-fake).

## Prove

`cargo test -p podrunner-fake`

## Done

2026-11-11, `e8f4c0d`; gate run 42938706745 of `f72f679` is green. `crates/podrunner-fake`, `crates/podrunner-clock`, `docs/fakes.md`. `cargo test -p podrunner-fake`: 7 passed: the fake unit passes unit tests 1 to 17 (13 or 14 skip by the container route), the fake source passes source tests 1 to 12, each of 17 unit plants and 12 source plants fails the test of its number, the script follows the manual clock. `fake` is refused unless `allow_fake = true` (`crates/podrunner-slots/src/config.rs`); a report of a fake unit names the kind `fake`. A fake that claims `guest` fails the guest self-test (T-036, `a_fake_that_claims_guest_fails_the_self_test`).

# T-021: WP-03: the slots core: states, journal, placement, status, drain, codes, config, audit (GitHub #21)

**Source:** GitHub #21
**Category:** feature
**Milestone:** M-A
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

The text of GitHub #21 ("WP-03: `slots` core: states, journal, placement, status, drain, codes, config, audit"), cleaned:

**What and why.** `slots` is the supervisor: state machine, placement, journal and status. It proves the githome asks G-01, G-02, G-05, G-08 and level 1.

**Content.**

- States: `offered`, `matched`, `claimed`, `minted`, `started`, `running`, `ended`, `destroyed`, `reported`; others `refused`, `released`, `failed`, `lost`. Each transition has a deadline key (`prepare_s`, `claim_s`, `mint_s`, `start_s`, `running_s`, `grace_s`, `destroy_s`, `report_s`).
- `started` to `running` only when the job-started event and the source's `started` event agree on runner name and job id; a mismatch destroys the unit (`job-mismatch`).
- Placement steps: architecture; tier and intake check (#18); effective boundary = max of the tier minimum and `unit_need.min_boundary`; candidates by boundary, limits (`enforced` or `reported`; `wall` needs `tree_kill: verified`), hold (`enforced` for `untrusted`, at least `advisory` for `trusted`), credential delivery, container need, mount modes; policy intersection with the tier ceiling; payload from the pin list (offer payload ignored and audited unless `payload_from_offer`); first free slot; on a shared principal at most one concurrent unit and one repository (`affinity`) unless `same_principal_ok`.
- Refusal codes (19): `no-slot-of-kind`, `port-claimed` (reserved), `image-stale`, `event-not-allowed`, `tier-mismatch`, `tier-needs-guest`, `boundary-not-available`, `limit-not-enforceable`, `hold-not-available`, `egress-policy-unsupported`, `arch-mismatch`, `payload-not-pinned`, `repo-scope-refused`, `container-not-supported`, `container-option-refused`, `build-not-supported`, `source-revoked`, `credential-unavailable`, `unit-start-failed`. `wait` codes end as `withdraw` at `wait_deadline_s`. A terminal refusal suppresses the `job_id` for `terminal_suppress_s` (default 3600).
- Journal: one JSON line for each transition, no credential. Reconcile on start: destroy every unit not `reported`; release and report `lost` from `claimed` on; stay `degraded` unless no residual process is counted.
- `degraded`: a residual count above 0 or `unknown` sets capacity 0, withdraws unclaimed offers, starts nothing until an operator clears it.
- Configuration `podrunner.toml` (`[[source]]`, `[unit.*]`, `[tier.*]`, `[deadlines]`, `[pin]`, `[egress]`). A file that lowers `tier.untrusted.min_boundary` is refused. `redirect` for `trusted` only if #4 is yes.
- Audit lines (`offer`, `tier`, `refuse`, `claim`, `mint`, `sign`, `control`, `hook-refuse`, `job-mismatch`, `payload-ignored`, `destroy`); no path, query, header or credential. `audit` forwarding to a source (G-04).
- Exit codes: the `pod-core` table (64 to 80).
- A host whose kinds meet no tier minimum starts no source instance; `status` says `no-tier-served`.

**Acceptance (level 1 tests, manual clock).**

- G-01: two sources with 2 slots each, 10 jobs each; none holds more than its slots.
- G-02: a job that needs `microvm` stays queued on a host with other slots, and runs when one appears.
- G-05: an arm64 job stays queued on x86_64-only hosts.
- G-08: one signal for each job past `wait_deadline_s`; a withdrawn job never starts; after `drain` none is claimed.
- G-12: a job past its wall time ends `timeout` and its unit is gone within 60 s (fake unit with a verified tree kill).
- Guards with planted defects: `untrusted` never below `guest`; stale image refused; digest outside the pin list refused; unenforceable required limit refused; an offer cannot lower a tier minimum; two co-tenant jobs on a shared principal do not run at once; a foreign `runner_name` destroys the unit; killing `slots` leaves no residue (skip the replay = defect); a planted credential canary appears in no output.
- The config parser and the journal reader have fuzz targets.

Dependencies: #16, #17, #18, #20. Cross-repo: podcrates `pod-core` exit table (done, taken in #11). Estimate (spec): M.

## Premise

Read the text of GitHub #21 on 2026-11-11. Nothing measured.

## Approach

The state machine with a deadline for each transition, the placement steps, the 19 refusal codes, the journal and the reconcile at start, the `degraded` state, `podrunner.toml` (`docs/configuration.md`), the audit lines (forwarded to the source), the full and the public reports, `status` (`crates/podrunner-slots/src/status.rs`, `schemas/status.v1.json`), `no-tier-served`, and the exit codes of `pod-core`. The level 1 tests G-01, G-02, G-05, G-08 and G-12 and the guards run on the fakes of T-020 with a manual clock. The fuzz targets `slots_config` and `slots_journal`.

## Prove

`cargo test -p podrunner-slots`, and `cd fuzz && cargo test --lib`

## Done

2026-11-11, `f72f679` and `644bab7`; gate run 42940434305 of `223c009` is green. `cargo test -p podrunner-slots`: 17 passed (10 loops of the first 14, each green): G-01, G-02, G-05, G-08, G-12, the guards (`untrusted` below `guest`, a fork on the trusted set, a stale image, a digest off the pin list, an unenforceable required limit, a shared principal, a foreign runner name, a restart that ends the units of the journal, no credential canary in the journal, the audit or a report), and the status. A planted skip of the replay leaves the processes alive. `cd fuzz && cargo test --lib`: `slots_config` and `slots_journal` survive their seeds and 5000 mutations.

# T-022: CLI and control socket: serve, status, drain, stop, doctor, print-unit (GitHub #22)

**Source:** GitHub #22
**Category:** feature
**Milestone:** M-A
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

The text of GitHub #22 ("CLI and control socket: `serve`, `status`, `drain`, `stop`, `doctor`, `print-unit`"), cleaned:

**What and why.** The operator surface (githome asks G-05, G-08, G-09; cfb ask A-X-15).

**Content.**

- Control socket: JSON lines on a unix socket, mode 0600, peer uid checked, `"v": 1` first. Operations `version`, `status`, `drain`, `stop`, `capacity`, `withdraw`. The peer check protects nothing where units share the uid; the documents say so.
- `podrunner status --json`: by source, then tier; each unit kind with its `probe` record, `principal`, `degraded`; refused jobs with code; `trust-lookup-failed` and `trust-lookup-throttled` counts; pins and ages; credential ages; App id and key age; `ended_without_completion`; `no-tier-served`.
- `podrunner doctor`: a thin reader of `podvm probe`, `pod-hostprobe` and the guest self-test (DR-15).
- `podrunner print-unit`: a service unit with no token in it.

**Crate dependencies (crate items R-C7 and R-C16 of the study, merged here).**

- Control socket (R-C7): podrunner builds `status` and `drain` as a module behind a trait. When podcrates releases `pod-control` (PodUtils/podcrates@T-103), the module goes and the wire does not change. One protocol for each process. podcrates ask: PodUtils/podvm#227, section 2.7 (PodUtils/podcrates@T-103: `drain` with jobs in flight, `status` that names each slot, its boundary, its hold and its principal).
- Host qualification (R-C16): `podrunner doctor` reads the host probe JSON of `pod doctor` (DR-15). It needs PodUtils/podcrates@T-107 (PodUtils/podvm#227, section 2.7) and a published `podvm probe --json` schema (podvm PodUtils/podvm#239 (N11)). Not on the path of M-A: until then `doctor` reads what exists and names each missing input.

**Acceptance.**

- A display name with an ANSI escape prints as `\x1b...` and is cut at 200 bytes.
- `status --json` validates against a schema in docs/.
- `print-unit` output holds no credential (canary test).
- The control socket parser has a fuzz target.
- The control module sits behind a trait, so that `pod-control` can replace it with no wire change.

Dependencies: #21. Cross-repo: PodUtils/podcrates@T-103 and PodUtils/podcrates@T-107 (PodUtils/podvm#227, section 2.7), podvm PodUtils/podvm#239 (N11), later. Estimate (mine): S to M.

## Premise

Read the text of GitHub #22 on 2026-11-11. Nothing measured.

## Approach

The control socket (JSON lines on a unix socket, mode 0600, peer uid checked, `"v": 1` first) behind the trait `Handler` that `pod-control` can replace (`crates/podrunner/src/control.rs`); the commands of `cli.rs` with the exit codes of `pod-core`; `text.rs` (an escape prints as `\x1b`, a value is cut at 200 bytes); `doctor.rs` (a thin reader that names each missing input); `print-unit` with no credential; `docs/cli.md`; the fuzz target `control_line`.

## Prove

`cargo test -p podrunner`

## Done

2026-11-11, `644bab7`; gate run 42940434305 of `223c009` is green. `cargo test -p podrunner`: 11 passed: an ANSI escape prints as `\x1b[2J` and a value is cut at 200 bytes; the socket has mode 0600 and answers one request; bad lines are `bad-request`; a usage error exits 64, a silent socket 69, a bad configuration 78; `print-unit` holds no credential canary. `status --json` follows `schemas/status.v1.json` (generated from the type; a test fails on a difference). A run by hand of `serve`, `status`, `status --json` and `stop` exits 0 on the fake kind and on the podvm kind.

# T-023: WP-04: the podvm unit driver with a store for each unit and a subreaper shim (GitHub #23)

**Source:** GitHub #23
**Category:** feature
**Milestone:** M-A
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

The text of GitHub #23 ("WP-04: podvm unit driver with a per-unit store and a subreaper shim"), cleaned:

**What and why.** podvm is the only unit kind for hosts with no root. The driver is the only code that knows podvm's argv (rule S6 of the spec: a flag rename in a beta CLI must break one module and its test, not podrunner at run time).

**Measured facts it must handle.**

- Every podvm rung is `redirect`. The driver reports `boundary: redirect`, `mount_modes: ["rw"]`, and `unknown` for anything it cannot map from `podvm probe --json`. It never passes `--unsafe-host-paths`.
- All containers of one image in one store share one mutable rootfs. So each unit gets its own `PODVM_STORE` under the state directory; the driver loads the image into it (`podvm load` of a saved tar); `destroy` removes it. Cost: about 5 s of extraction for the runner image, one image copy of disk for each slot. podvm PodUtils/podvm#234 (N6) (a rootfs for each container) removes this cost later.
- Grandchildren survive `podvm stop` and `rm -f` (6 of 6 runs). So the driver starts one subreaper shim for each unit (`PR_SET_CHILD_SUBREAPER`) as the parent of the top process. Every process carries `PODRUNNER_UNIT`. `destroy` walks the shim's descendants by `/proc/PID/stat`, sends SIGKILL, waits and counts. A dead shim gives `residual_processes: "unknown"`.
- A static payload is refused on a chroot-denied host. The stand-in payload is a dynamic shell or Node script.
- `start.env` always sets `DOTNET_EnableWriteXorExecute=0` (the .NET runner dies at start under any `RLIMIT_FSIZE` below 2 TiB with the default) and never applies an `fsize` limit. It sets no tool cache variable.
- The credential directory is a plain mode 0700 directory (`credential_store: private-dir`); H0 has no mount for a tmpfs. podvm PodUtils/podvm#240 (N12) fixes the modes of its own container table and store root.
- `podvm --podvm-tier=chroot` enters userland with exit 0, and `.EnteredRung` can name `chroot` while `run` enters userland. The driver does not trust a rung request or `.EnteredRung` until podvm PodUtils/podvm#237 (N9) and PodUtils/podvm#238 (N10) land; it reads the rung from the run banner and records both.
- `container_exec` maps to `podvm exec -e K=V -w DIR`, refuses `stdin: true` and a workdir inside a volume.
- Settle U25: does `podvm run -v` accept `/proc/self/fd/N` as a mount source.

**Crate dependencies (crate item R-C3 of the study, merged here).** The subreaper shim is a stopgap. podcrates ask: PodUtils/podvm#227, section 2.3 (WP-C2: `pod-supervise` kills the whole tree, has a wall timeout, gives rusage and a count of residual processes). podvm takes the mechanism in PodUtils/podvm@T-1655 and PodUtils/podvm#233 (N5). When podvm's `stop`, `rm -f` and wall bound end every process, the driver retires the shim. Until test 3 passes, `wall` in `limits_required` is `limit-not-enforceable`.

**Acceptance.**

- Unit conformance tests 1 to 7, 10, 12 and 15 pass with a stand-in dynamic payload on a host where `podvm probe` shows chroot and mount denied (record the probe output; a GitHub-hosted runner with sudo is not H0).
- Test 3 (setsid grandchild): `destroy` reports 0 residual processes. Until it passes, `tree_kill` is `unverified` and a job that requires `wall` is `limit-not-enforceable`.
- Credential files are mode 0600, absent from every `/proc/*/cmdline` and `/proc/*/environ`, and absent from disk after `destroy` (positive controls included).
- The driver pins a podvm version range and fails `probe` with a named reason outside it.
- U25 is answered in #9.

Dependencies: #16, #20. Cross-repo: podvm `probe --json` (A-V-10, PodUtils/podvm#239 (N11)), podvm `load`, podvm PodUtils/podvm#233 (N5), PodUtils/podvm#234 (N6), PodUtils/podvm#237 (N9), PodUtils/podvm#238 (N10), PodUtils/podvm#240 (N12), PodUtils/podvm@T-1655 and PodUtils/podvm#227 section 2.3 (tree kill inside podvm's launcher, later). Estimate (spec): M.

## Premise

Read the text of GitHub #23 on 2026-11-11. Nothing measured.

## Approach

A driver over the released podvm binary (`crates/podrunner-podvm`), the only code that knows the argv of podvm. A fix that needs podvm goes to a COORD issue `(to podvm)`.

- The version range of podvm (`probe::LOWEST` up to `probe::BEYOND`, exclusive): a podvm outside it fails `probe` by name.
- The map of `podvm probe --json` to the capability record: each rung `redirect`, `boundary_proof` names the rung, `egress_hold: unknown`. Recorded probes: `crates/podrunner-podvm/tests/probe/`.
- `prepare` saves the pinned image from the store of the operator into a tar. `start` loads it into a store of the unit, writes the credential files (mode 0600) into the rootfs that `podvm extract` names, and runs the payload detached with the tag of the unit, `PODRUNNER_UNIT` and `DOTNET_EnableWriteXorExecute=0`. `destroy` kills each tagged process and its tree, removes the container and the store, and counts.
- `verify_tree_kill` runs test 3 with the pinned payload at the start of `serve`; only then does the record say `tree_kill: verified` and `wall: enforced`.
- The H0 profile: a uid with no root and a seccomp filter that refuses user namespaces, no KVM: the supervise rung (`crates/podrunner-podvm/tests/probe/supervise.json`). The loader of that rung needs glibc in the image, so `scripts/stand-in-image.py` builds a stand-in image from host binaries.
- U25: on the supervise rung `podvm run -v /proc/self/fd/N:/cred` mounts the open file. The driver writes the files into the rootfs and needs no `-v`.

## Prove

`cargo test -p podrunner-podvm`, and `cargo test -p podrunner-podvm --test conformance -- --ignored` with `PODRUNNER_PODVM`, `PODRUNNER_PODVM_IMAGES` and `PODRUNNER_PODVM_IMAGE` (as root: the namespace rung, busybox; as a uid with no user namespace: the supervise rung, the image of `scripts/stand-in-image.py`)

## Done

2026-11-11, `b5ff350`; gate run 42957378324 is green. The podvm driver passes unit tests 1 to 17 (14 skips: no container route) on the namespace rung (root, busybox) and on the supervise rung of the H0 profile (the stand-in image); 5 runs each, no process left. A job of a local fake source through `serve` ends `success` 10 of 10 on the H0 profile, and a payload `exit 3` reports 3. U25 is settled. `cargo test -p podrunner-podvm`: each test passes.

# T-024: WP-08: the egress module over pod-broker v1, one listener for each unit (GitHub #24)

**Source:** GitHub #24
**Category:** feature
**Milestone:** M-A
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

The text of GitHub #24 ("WP-08: egress module over `pod-broker` v1, one listener for each unit"), cleaned:

**What and why.** githome ask G-04 (a policy for each job) and cfb ask A-V-02 ("refuse the run where no hold works, never run unheld"). `pod-broker` v1 (PodUtils/podcrates@T-099, done) gives per-job `load`, `drop`, tokens as SHA-256, TTL, modes `allow` and `open`, fixed denies, one JSON audit line for each connection.

**Content.**

- `egress.load` binds the job policy to one unix listener `egress/UNIT.sock`. The socket is the identity: no token reaches the unit on the advisory path.
- The hold is a capability of the unit record. `untrusted` needs `enforced`; `trusted` with an `allow` or `open` policy needs at least `advisory`; `none` only for `local`.
- `slots` appends the `control_hosts` of the source's `hello` to every `allow` list. The default policy is `allow` with the Actions control hosts, port 443.
- Path rules while the broker has no TLS end: `egress-policy-unsupported`.
- The fixed denies are enforced only where the hold is `enforced`; `status` and every report carry `egress_hold`.
- If `egress` dies, units fail closed and nothing is offered until it is back.
- Settle U24: does `pod-broker` v1 serve one listener for each unit, or does it need a thin wrapper.
- The unit reaches `egress/UNIT.sock` by a host path. podvm PodUtils/podvm#235 (N7) (unix socket paths under the interposer) decides if the socket path works inside a podvm unit.

**Crate dependencies (crate items R-C1, R-C2, R-C5 and R-C11 of the study, merged here).**

- One listener for each unit (R-C1): read in the crate, `pod-broker` takes the job from `Proxy-Authorization` on each connection, and no call binds one listener to one job. So U24 needs a crate change or a wrapper. podcrates ask: PodUtils/podvm#227, section 2.1. Until it lands, this module meets "no token in the unit" only with a wrapper. The redirect is `podnet pod -x http://unix:/PATH` (the form podnet parses), not `socks5://`: `pod-broker` speaks HTTP CONNECT and absolute-form HTTP only.
- Name-forwarding upstream (R-C2): the broker sends the name to a CONNECT upstream that takes names only (U10). podcrates ask: PodUtils/podvm#227, section 2.2 (WP-C1). On a proxy-only host, the hold stays `advisory` and no `untrusted` job runs until the canary check passes. It is the gate of phase 2 on proxy-only hosts, not of M-A.
- Actions hosts (R-C5): `hello.control_hosts` and the default policies read a `pod-services` data file, not a host list in podrunner code. podcrates ask: PodUtils/podvm#227, section 2.5 (WP-C4).
- `podnet pod` with the runner (R-C11): the `advisory` hold for the .NET runner and Node is proven by podnet. podnet ask: PodUtils/podvm#228, section 2.8 (WP-N1, U9). It gates M-D (#33), not M-A.

**Acceptance.**

- A fake unit at hold `none` gets no `trusted` job (`hold-not-available`).
- Two units get two listeners; a token of unit A is refused on the listener of unit B.
- Killing `egress` stops new offers and signals running units.
- U24 is answered in #9, with the crate change or the wrapper named.
- No Actions host name is written in podrunner code after PodUtils/podvm#227 section 2.5 lands.

Dependencies: #21, #23. Cross-repo: podcrates `pod-broker` tag and PodUtils/podvm#227 sections 2.1, 2.2, 2.5; podnet PodUtils/podvm#228 section 2.8; podvm PodUtils/podvm#235 (N7) and PodUtils/podvm@T-1665. Estimate (spec): S.

## Premise

Read the text of GitHub #24 on 2026-11-11. Nothing measured.

## Approach

`crates/podrunner-egress`: `pod-broker` and `pod-proxy` on a tokio runtime of their own, with the system resolver. `open` binds a unix socket of mode 0600 to one job (`Broker::serve_unix_job`: no token in the unit; a `Proxy-Authorization` of another job gets 407), `close` drops the job and its listener, `kill` ends the module. In slots (`EgressPort`), each unit gets a listener of its job with the policy of its offer (else `[egress] default_policy`, else no way out) and the variable `PODRUNNER_EGRESS=http://unix:PATH` (the form of `podnet pod -x`). The end of a unit closes it. When egress ends, no source gets capacity and each running unit ends (`unit-failed`). A kind with hold `none` gets no `trusted` job. No host of the Actions service is in the code: the default policy comes from the configuration. A socket path over the 108 bytes of `sun_path` gets a short directory, and the audit line names the reason of a failed open.

## Prove

`cargo test -p podrunner-egress`

## Done

2026-11-11, commit `d69e791`; the gate run 42946099969 is green. `cargo test -p podrunner-egress`: 3 passed (two units, two listeners; a token of unit A gets 407 on the listener of unit B; a close and a kill end the listeners). `cargo test -p podrunner-slots`: 21 passed, with a kind at hold `none` that gets no `trusted` job (`hold-not-available`) and a dead egress that stops the offers and ends the running units. U24 is settled in `docs/unknowns.md`: `serve_unix_job` of podcrates `v0.16.0`, no wrapper. No host of the Actions service is in the code.


# T-025: Milestone M-A: level 1 green, podvm unit conformance with a stand-in payload (GitHub #25)

**Source:** GitHub #25
**Category:** chore
**Milestone:** M-A
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

The text of GitHub #25 ("Milestone M-A: level 1 green, podvm unit conformance with a stand-in payload"), cleaned:

**What and why.** The first milestone needs no GitHub account, no second machine and no official runner. It proves the core and the podvm driver. No crate prerequisite blocks M-A: it uses the existing `pod-core` and keeps the schemas and the control socket inside podrunner until podcrates releases them.

**Acceptance.**

- #15, #16, #17, #18, #19, #20, #21, #22, #23, #24 are closed.
- The level 1 suite is green in the gate on `main`; the run link is recorded.
- On an H0-class host (probe output recorded): the podvm driver passes its conformance tests with a stand-in payload; credential files at 0600 and absent from argv and environ; the placement refusals by name; `limit-not-enforceable` for `wall` until test 3 passes; `status`, `drain` and journal replay work.
- No trusted job runs in M-A.

Dependencies: the issues above. Estimate: the sum of its issues.

Note: #15 (beta release) depends on this gate for its first tag; close the gate first, then cut the first beta.

## Premise

Read the text of GitHub #25 on 2026-11-11. Nothing measured.

## Approach

Close M-A when T-016 to T-024 are done, the level 1 suite is green in the gate, and the podvm driver passes the unit conformance with a stand-in payload. No account, no second machine, no official runner.

## Prove

`cargo test --workspace`, and the gate run of the closing commit.

## Done

2026-11-11, `b5ff350`: T-015 to T-024 are done; the level 1 suite and the unit tests pass in the gate; on an H0 profile (no root, no user namespace, no mounts, no KVM; the probe is `crates/podrunner-podvm/tests/probe/supervise.json`) the podvm driver passes its unit tests with a stand-in payload, the credential files are at 0600 and in no argv or environment (tests 6, 7), and a job of a local source runs through `serve`. No trusted job ran.

# T-047: COORD-002: podvm wait gives no exit code for a short payload on the supervise rung (GitHub #49)

**Source:** GitHub #49
**Category:** defect
**Milestone:** M-A
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

With the released podvm `v0.2.0-beta.2` on the supervise rung (a uid with no root, no user namespace, no mounts, no KVM), `podvm wait NAME` of a detached payload that ends at once often exits 125: "NAME has no exit code podvm measured: it is Up (podvm TODO/supervise.md, PodUtils/podvm@T-0604)". The podvm driver then read no exit code and reported -1: 7 of 10 jobs of an H0 run ended `failure`. GitHub #49 (COORD-002, to podvm) records it; podvm is on break.

## Premise

Measured on 2026-11-11 as `nobody` with a seccomp filter that refuses `CLONE_NEWUSER`: `podvm wait` exited 125 in 4 of 5 runs of `sh -c 'exit 0'`.

## Approach

The workaround is in the driver (T-023): the payload runs under `/bin/sh` of the image, which writes the exit code to `.podrunner-exit` in the working directory; when `podvm wait` gives no code, the driver reads that file, and starts `podvm wait` again while a process of the unit lives. When podvm answers, remove the workaround if `podvm wait` gives each code.

## Prove

`cargo test -p podrunner-podvm --test conformance -- --ignored`, as a uid with no user namespace, with the image of `scripts/stand-in-image.py`

## Done

2026-11-11, `016a78f`, podvm `v0.2.0-beta.3` (PodUtils/podvm#254, PodUtils/podvm@T-1720). On the H0 profile, `podvm wait` of a detached `sh -c 'exit N'` gives the exit code in 10 of 10 runs (beta.2: 1 of 5). The workaround is gone: the payload runs with no shell around it and no `.podrunner-exit` (`crates/podrunner-podvm/src/unit.rs`); the waiter starts again while a process of the unit lives. The driver needs beta.3 or later (`probe::LOWEST`). The unit suite passes on both rungs with the image of `scripts/stand-in-image.py`; test 14 skips (no container route).

# T-048: Egress: check the upstream of the broker; the names mode of pod-broker

**Source:** PodUtils/podcrates#60 (COORD-035)
**Category:** feature
**Milestone:** M-D
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

podcrates v0.18.0 gives `pod-broker` two parts for the egress module of GitHub #24 (PodUtils/podcrates@T-161, its call C51): `Broker::check_upstream()` sends the canary `localhost` through the way out and gives `UpstreamCheck::{RefusesNonPublic, TakesNonPublic, Unknown}`; `Settings::names_upstream` sends the checked name, once, to an upstream CONNECT proxy that takes names only (a host with a proxy only). The podcrates call: hold a job `advisory` and serve no `untrusted` job when the check is not `RefusesNonPublic`. The load also flags an allow rule under an open suffix (`JobInfo::flagged`).

## Premise

Read PodUtils/podcrates#60 on 2026-11-11. podrunner takes v0.18.0; nothing measured.

## Approach

At the start of `serve`, run `check_upstream` on the egress module; when it is not `RefusesNonPublic`, the egress hold of each kind is at most `advisory`, so placement serves no `untrusted` job (`hold-not-available`). A configuration key turns on `names_upstream` for a host whose proxy takes names only. Write a flagged allow rule of a job as an audit line.

## Prove

`cargo test -p podrunner-egress upstream`, `cargo test -p podrunner-egress open_suffix`, `cargo test -p podrunner-slots flagged`, `cargo test -p podrunner upstream`

## Done

2026-11-11, `6f24681`: `upstream_proxy` and `upstream_names_only` in `podrunner.toml` (names only needs a proxy); `Egress::host` passes `names_upstream` to `pod-broker`, and `Egress::check_upstream` sends the canary. `serve` runs the check when the proxy takes names only; any answer but `refuses-non-public` caps each hold at `advisory`, so placement serves no `untrusted` job. `Egress::open` counts the allow rules under an open suffix, and slots writes an audit line `egress-flagged` with the count only. The upstream check is tested against two fake CONNECT proxies (200 and 403). Each command of the Prove passes.

# T-051: COORD-005: podvm-aarch64 declines an arm64 payload on the supervise rung (GitHub #52)

**Source:** GitHub #52
**Category:** defect
**Milestone:** M-C
**Priority:** P2
**Effort:** S
**Status:** blocked

## Problem

On a GitHub-hosted `ubuntu-24.04-arm` runner (the supervise rung), `podvm-aarch64` of `v0.2.0-beta.2` loads the runner image, but `run` refuses each payload: "/bin/sh is ELF machine 0xb7 and the glibc object is 0x3e, so the tier cannot serve it". The glibc object of its loader is x86_64. So no unit of podrunner runs on an arm64 host with no user namespace. GitHub #52 (COORD-005, to podvm) records it; podvm is on break.

## Premise

Measured on 2026-11-11 in `runner-image.yml` run 42973869342 (`9c03931`), job aarch64, with the binary checked against its SHA-256.

## Approach

No workaround in podrunner: the loader is part of podvm. The aarch64 run of `runner-image.yml` is a measure until a podvm release; then pin that release in `images/podvm.toml` and make the aarch64 start a check again.

## Prove

The aarch64 job of `.github/workflows/runner-image.yml` starts `/bin/sh` of the image as a check, and it is green.

## Blocker

podvm answered in PodUtils/podvm#252 (COORD-033): the interposer of the supervise rung is x86_64 only by design; `main` now embeds no object of another machine, so `podvm-aarch64` names the missing object. The aarch64 port is PodUtils/podvm@T-1721 (P2, after the current work order of podvm). Until it is released, an arm64 host of podrunner needs the namespace rung (a user namespace) or another unit kind. The aarch64 job of `runner-image.yml` stays a measure; when the release carries the port, pin it and make the start a check.

# T-052: COORD-006: a podvm beta from main for podrunner (GitHub #53)

**Source:** GitHub #53
**Category:** chore
**Milestone:** M-C
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

The owner ruled "Ask podvm for a beta" (2026-11-11, `docs/decisions.md`). T-026, T-047, T-050 and T-030 wait only for fixes on podvm `main` (PodUtils/podvm@T-1706, PodUtils/podvm@T-1710, PodUtils/podvm@T-1720 and others); podrunner pins podvm by release (`images/podvm.toml`, `v0.2.0-beta.2`). GitHub #53 (COORD-006, to podvm) asks for a beta.

## Premise

The answers of podvm in PodUtils/podvm#250 and PodUtils/podvm#252 name these fixes on `main` and say that a release follows the release order of podvm.

## Approach

When the beta comes: pin it in `images/podvm.toml` (the version and the SHA-256 of each binary); run `scripts/measure-hook-path.sh` on both rungs; run the H0 profile again (T-047); run the runner image in a unit up to the long poll against the fake broker (T-026); report each result on its issue, and close GitHub #53.

## Prove

`runner-image.yml` and `level2.yml` are green with the new pin, and `scripts/measure-hook-path.sh` gives `volume-socket yes` on both rungs.

## Done

2026-11-11. podvm released `v0.2.0-beta.3` from `c62643d` (PodUtils/podvm#254). `images/podvm.toml` pins it (the SHA-256 of each binary equals its `.sha256`). T-047 and T-050 are done on it, the unit suite of the podvm driver passes on both rungs, and T-026 is done on it. The aarch64 interposer is not in the beta (T-051).

# T-053: The conformance suites follow the numbers of the design contracts

**Source:** T-014
**Category:** defect
**Milestone:** L2
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

The conformance suites of podrunner (`docs/contracts/unit-driver.md` tests 1 to 17, `docs/contracts/source-driver.md` tests 1 to 12) have their own numbers and names. The design contracts (`docs/design/contracts/unit-driver.md`, `docs/design/contracts/source-driver.md`) number other tests, and the issues name those numbers: GitHub #35 asks for the design tests 3, 8, 9, 13, 14, 16 and 17 on the fake path. Some design tests have no test here (for example unit test 9, the socket of unit A refuses a container id of unit B; unit test 8, `container_exec` sees its environment and working directory).

## Premise

Read on 2026-11-11: no number of `docs/contracts/` agrees with a number of `docs/design/contracts/` for the same test, apart from a few by chance.

## Approach

A table in each of `docs/contracts/unit-driver.md` and `docs/contracts/source-driver.md` ("The tests of the design contract"): each design test and the tests here that cover it. Keep the names of the suites; the table carries the design numbers. Each missing test is written with a planted defect and a correct input, on the fake unit or the fake source:

- Unit design tests 8, 9, 13, 14, 16, 17: `crates/podrunner-unit/src/conformance/cases.rs`, `crates/podrunner-hook/src/tests.rs`, `crates/podrunner-hook/src/tests_unit.rs`, `crates/podrunner-slots/src/tests/hooks.rs`.
- Unit design tests 1, 2, 3, 6, 10, 12: suite tests 18 to 22 (`crates/podrunner-unit/src/conformance/design.rs`), and a schema test for `../x`. Tests 23 and 24 (no process at probe; a dump of `/proc/*` from a step) see each process of the uid, so the fake runs them in a test binary of its own (`crates/podrunner-fake/tests/alone.rs`). Tests 25 and 26 (an orphan with no tag, a killed shim) come from T-063.
- Unit design test 4: `a_limit_refusal_names_the_limit`.
- Source design tests 2, 4, 8, 10, 11, 12 (8 and 12 by call C18); test 3 for each role (`crates/podrunner/tests/serve.rs`, `crates/podrunner/tests/mint.rs`, `crates/podrunner/tests/scaleset.rs`); test 6 (`one_key_for_two_tiers_is_refused`).

The one open row, unit design test 11 (a real guest passes the self-test), is the acceptance of T-036 and T-037.

## Prove

`cargo test -p podrunner-fake`, `cargo test -p podrunner-hook`, and the two tables have no row "missing".

## Done

2026-11-11, `30b9366` (tests 25, 26) and `d764700` (test 27); gate run 43018641175, `runner-image.yml` run 43018641300. Both tables in `docs/contracts/` have no row "missing": each design test of the unit and the source contracts has a test here with a plant, on the fake path, and the unit suite runs on the podvm driver in CI. Unit design test 11 on a real guest is the acceptance of T-036. A `started` with another job id now ends the unit; the sidecar ends with 78 after a revocation; the record of the `fake` kind says `net: shared-host`.

# T-054: serve: an exclusive lock on the state directory before the journal replay; stop --wait (GitHub #54)

**Source:** GitHub #54
**Category:** defect
**Milestone:** M-D
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

`podrunner serve` takes no lock. A second `serve` on the same `--state` directory replays the journal, destroys the units of the first, live instance, and takes over the control socket path (observed by podnet at `9d48ab8`). GitHub #54, from PodUtils/podnet#439 (block "airhand 1").

## Premise

podnet measured it at `9d48ab8` (PodUtils/podnet#439, Evidence). Not measured here yet.

## Approach

A non-blocking `flock` on `STATE/serve.lock` (mode 0600, `O_CLOEXEC`, never removed) before the configuration is built and the journal is read; a held lock gives exit 69, another lock error refuses the start. `control::bind` removes only a stale socket. `podrunner stop --wait [--timeout S]` polls the lock; a timeout gives exit 75 (add 75 to `docs/cli.md`). The lock of `pod-supervise` (podcrates) can replace the few lines of `flock`.

## Prove

`cargo test -p podrunner serve_lock`, and the acceptance tests of GitHub #54

## Done

2026-11-11, `1f1d389`. `crates/podrunner/src/lock.rs`: `serve` takes `flock` on `STATE/serve.lock` (mode 0600, `O_NOFOLLOW`, close on exec, never removed) before it reads the configuration and the journal; a held lock and each other lock error give exit 69. `control::bind` replaces only a stale socket; a live socket is 69, a path that is not a socket is 64. `podrunner stop --wait --state DIR [--timeout S]` polls the lock; a timeout is 75. Tests: `crates/podrunner/tests/serve.rs` (a second `serve` exits 69 with the journal, the socket inode and the job of the first unchanged; after SIGKILL of the holder the next `serve` starts; `stop --wait` returns after `serve` ends, `--timeout 1` gives 75); a planted removal of the lock fails the test. `docs/cli.md`; residual R6 (a network filesystem). The lock of `pod-supervise` can replace `lock.rs` later.


# T-055: journal: write and sync the claimed line before source.claim (GitHub #55)

**Source:** GitHub #55
**Category:** defect
**Milestone:** M-D
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

`crates/podrunner-slots/src/run.rs` calls `source.claim` before it writes the `claimed` journal line, and `Journal::write` never syncs. A death in that window leaves a job that the source holds and the journal does not list as claimed; the restart neither releases nor reports it. GitHub #55, from PodUtils/podnet#439 (block "airhand 2").

## Premise

podnet measured it with a planted abort at `9d48ab8` (PodUtils/podnet#439, Evidence).

## Approach

Write `claimed`, `sync_data`, then call `source.claim`; on an error, write `released`. Sync only the lines before an external effect that a restart must undo. A sync failure makes the host `degraded`. Record the open question of the source contract: `release` names no runner.

## Prove

`cargo test -p podrunner-slots claim_is_journaled_first`

## Done

2026-11-11, `a464cea`. `crates/podrunner-slots/src/run.rs`: `claim` writes the `claimed` line and syncs the journal (`Journal::sync`, `sync_data`) before it calls `source.claim`; a line that cannot be written or synced makes the host `degraded`, releases the placement and claims nothing. Tests `crates/podrunner-slots/src/tests/journal.rs`: a source that reads the journal during `claim` finds the `claimed` line (a planted old order fails it); a journal on `/dev/full` gives `degraded` and no claim at the source; a replayed `claimed` line is reported `lost` (and released), a replayed `matched` line writes `lost` with no release, because the claim never went out. `docs/contracts/source-driver.md` names the open point: `release` names no runner, so a source frees only a runner that it registered (the scale-set sidecar does).


# T-056: control socket: an idle client stalls the tick of the supervisor (GitHub #56)

**Source:** GitHub #56
**Category:** defect
**Milestone:** M-D
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

`serve` answers its control socket in the thread that ticks the supervisor. A silent same-uid client holds that thread for the read timeout of 5 s, and each further idle client adds 5 s. GitHub #56, from PodUtils/podnet#439 (block "airhand 14").

## Premise

podnet read it at `9d48ab8` (PodUtils/podnet#439, Evidence).

## Approach

The socket I/O in its own threads, at most 8 clients at a time (a ninth gets `busy`), an absolute deadline for each exchange; whole requests reach the supervisor through a channel.

## Prove

`cargo test -p podrunner control`

## Done

2026-11-11, `1f1d389`. `control::spawn`: the socket has an accept thread and one thread for each client, at most 8 (a ninth gets `busy`), an absolute deadline of 5 s for each exchange; the supervisor gets whole requests through a channel (`answer_waiting`). Tests in `crates/podrunner/src/control.rs`: with seven silent clients a `status` answers in under 1 s and a ninth client is `busy`; a client that sends one byte each 500 ms is cut at the deadline.


# T-057: source wire: a reply deadline for each call; restart a silent sidecar (GitHub #57)

**Source:** GitHub #57
**Category:** defect
**Milestone:** M-D
**Priority:** P1
**Effort:** S
**Status:** done

## Problem

`serve` calls the scale-set sidecar over a lock-step JSON-line wire with a blocking read and no deadline. A sidecar that lives and says nothing blocks the supervisor thread. GitHub #57, from PodUtils/podnet#439 (block "agentanycast 1").

## Premise

podnet read it at `9d48ab8` (PodUtils/podnet#439, Evidence).

## Approach

A reply deadline for each call of the wire (`REPLY_LIMIT`). When it passes, kill the child and use the backoff restart, so that no late reply is read as the answer to the next request. A child that exits during a call gives `Unavailable`.

## Prove

`cargo test -p podrunner-source reply_deadline`

## Done

2026-11-11, `1ddd4c4`. `crates/podrunner-mint/src/sidecar.rs`: the stdio of the sidecar is a `Pipe` that polls before each read; each call has a deadline of `REPLY_LIMIT` (15 s) plus the long poll of `next`. A timeout is a broken wire: the child is killed, and it starts again after the backoff. Tests: a sidecar that answers `hello` and then sleeps fails the next call in under 3 s (limit 0.5 s) and starts again; a reply that comes after the deadline never answers a later call; a child that ends during a call gives `unavailable`. `docs/contracts/source-driver.md`.


# T-058: podrunner check: validate a configuration and start nothing (GitHub #58)

**Source:** GitHub #58
**Category:** feature
**Milestone:** M-D
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

The only way to know whether a `podrunner.toml` works is `podrunner serve`, which replays the journal, destroys open units and runs a self-test before it binds the socket. GitHub #58, from PodUtils/podnet#440 (block "tailsplit 8").

## Premise

Read on 2026-11-11.

## Approach

`podrunner check --config FILE`: each part that has no side effect, with the text and the exit of `serve` for the same file; no `--state`, no file of a state directory, no network. A shipped example configuration that CI checks; unknown keys and wrong types exit 78 and name the key.

## Prove

`cargo test -p podrunner check`

## Done

2026-11-11, `4f09320`. `podrunner check --config FILE` (`serve::check`, `serve::plan`): the TOML with each unknown key and type refused, each source built and not called, each unit kind with its driver file (`podvm`: a file binary and a directory of images); no state directory, no unit, no network. `serve` runs the same `check` after its lock and before `build`, so both refuse a bad file with one text. `docs/examples/podrunner.toml`. Tests `crates/podrunner/tests/check.rs`: the example passes with an unusable `HTTPS_PROXY` and leaves the working directory as it was; `[[sources]]`, a string for `claim_s`, a key file at 0640 and a podvm binary that is a directory each exit 78 with the same text from `check` and `serve`, and `serve` writes no journal; each TOML block of `docs/configuration.md` names only known keys, and a misspelled key fails.


# T-059: audit.jsonl: chain each line to the one before; podrunner audit verify (GitHub #59)

**Source:** GitHub #59
**Category:** feature
**Milestone:** M-D
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`serve` appends one line for each decision to `STATE/audit.jsonl`. Nothing shows that the file lost a line or that a line was edited. GitHub #59, from PodUtils/podnet#440 (block "tunnet 3").

## Premise

Read on 2026-11-11.

## Approach

`seq` and `prev` (the SHA-256 of the bytes of the line before) on each line; `podrunner audit verify [--anchor SEQ:HASH]` (exit 76 on a break or a torn line); a `resume` line at start after a torn line; the file at mode 0600; at most 1000 lines in memory.

## Prove

`cargo test -p podrunner-slots audit_chain`

## Done

2026-11-11, `1773096`. `crates/podrunner-slots/src/audit.rs`: each line has `seq` (from 1) and `prev` (the SHA-256 of the exact bytes of the line before; 64 zeros for the first); a failed write moves no `seq`; the file is at mode 0600; a torn last line gets a newline and a `resume` line at open; at most 1000 lines stay in memory. The line is written first and its copy with `seq` and `prev` goes to the source (the anchor). `audit::verify` and `podrunner audit verify (--state DIR | --file FILE) [--anchor SEQ:HASH]` (exit 76 on a break). Tests: five lines verify; a lost third line is "broken at seq 4"; an edited detail of the second line is "broken at seq 3"; a cut tail passes with no anchor and fails with the anchor of the source; a torn end fails until the `resume` line; a write to `/dev/full` leaves no gap; `x|y` and `x||y` give two `prev` values; the CLI exits 0, 76 and 64.


# T-060: serve: SIGTERM and SIGINT start a drain; sd_notify (GitHub #60)

**Source:** GitHub #60
**Category:** feature
**Milestone:** M-D
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`serve` installs no signal handler. SIGTERM ends it at once, and the next start destroys each open unit, so each running job is lost. The unit of `print-unit` has no readiness, stop or watchdog contract. GitHub #60, from PodUtils/podnet#440 (block "tunnet 5").

## Premise

Read on 2026-11-11.

## Approach

SIGTERM and SIGINT start the drain of `podrunner stop`. With `NOTIFY_SOCKET` (path and `@` forms): `READY=1` after the control socket answers, `STOPPING=1` at the signal, `WATCHDOG=1` when enabled. `print-unit` writes `Type=notify`, and `WatchdogSec` only with a flag.

## Prove

`cargo test -p podrunner signal`

## Done

2026-11-11, `1f1d389`. `crates/podrunner/src/notify.rs`: SIGTERM and SIGINT set a flag that starts the drain of `stop`; with `NOTIFY_SOCKET` (a path or `@NAME`) `serve` sends `READY=1` after the socket answers, `STOPPING=1` once at the drain, and `WATCHDOG=1` at half of `WATCHDOG_USEC`. `print-unit` writes `Type=notify`, `KillMode=mixed`, `TimeoutStopSec=infinity`, and `WatchdogSec` only with `--watchdog`. Test `sigterm_drains_and_notify_says_ready_and_stopping`: SIGTERM ends `serve` with exit 0 only when the job is reported, with no `lost` line; a planted removal of the handler fails it. `systemd-analyze verify` is not run here (not on this host).

# T-061: COORD-007: on the supervise rung an absolute symlink in the rootfs resolves on the host (GitHub #61)

**Source:** GitHub #61
**Category:** defect
**Milestone:** L2
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

On the supervise rung of podvm `v0.2.0-beta.3`, an absolute symlink inside the rootfs of an image resolves on the host: the official runner could not read `/home/runner/.credentials -> /podrunner/.runner/.credentials`, and a relative link works. On another host the target can be a host file. Many images hold absolute links. GitHub #61 (COORD-007, to podvm) records it.

## Premise

Measured on 2026-11-11 in T-026 (`crates/podrunner-broker-fake/tests/podvm.rs`, `1db8bc3`), as `nobody` with a seccomp filter that refuses `CLONE_NEWUSER`.

## Approach

No fix in podrunner: the path view is the interposer of podvm. The runner image uses relative links (`images/runner/Dockerfile`). When podvm answers, record the rule of the rung, and add a case of an absolute link to the unit tests of the podvm driver if podvm fixes it.

## Prove

`cargo test -p podrunner-podvm --test conformance -- --ignored` as a uid with no user namespace, with a case where a unit reads a file through an absolute link inside its rootfs.

## Done

2026-11-11, `baab86e`; runner-image run 43100514216 and gate run 43100514057 of `baab86e` are green. `crates/podrunner-podvm/tests/links.rs`: a payload reads a file through an absolute link in its rootfs, as a uid with no user namespace and as root.

# T-062: COORD-008: on the supervise rung a volume given as :ro is writable (GitHub #62)

**Source:** GitHub #62
**Category:** defect
**Milestone:** L2
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

On the supervise rung of podvm `v0.2.0-beta.3`, `-v SRC:/__e:ro` stays writable from the container: a step can change the externals of the runner (node, the hook relay), which the next step runs. GitHub #62 (COORD-008, to podvm) records it.

## Premise

Measured on 2026-11-11 with `scripts/measure-container-verbs.sh`, as `nobody` with a seccomp filter that refuses `CLONE_NEWUSER`.

## Approach

The hook checks refuse a request for a writable `/__e`, and the podvm driver passes `/__e` with `:ro`. `run` applies `:ro`; `exec` does not, so a step can write `/__e` of its own unit, and a later step of the same job runs it. The unit is destroyed after its one job, so no other job sees the change. When podvm releases PodUtils/podvm@T-1726: pin it, run `scripts/measure-container-verbs.sh` on the supervise rung, and add a check to the unit tests of the podvm driver that a step cannot write `/__e`.

## Prove

`sh scripts/measure-container-verbs.sh PODVM DIR` as a uid with no user namespace prints `externals-read-only ok`.

## Done

2026-11-11, `baab86e`; runner-image run 43100514216 and gate run 43100514057 of `baab86e` are green. `scripts/measure-container-verbs.sh` gives `externals-read-only ok` on the supervise rung, and `runner-image.yml` checks it.

# T-063: The tree kill finds a unit by an environment tag, and a process can drop it

**Source:** T-053
**Category:** defect
**Milestone:** L2
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

The `fake` and `podvm` drivers find the processes of a unit by the variable `PODRUNNER_UNIT_TAG` in `/proc/PID/environ` (`crates/podrunner-unit/src/procs.rs`). A step that runs `setsid env -i sleep 1d &` starts a process with an empty environment: no scan finds it, `destroy` does not kill it and counts 0 processes, and the probe claims `tree_kill: verified`. A job can leave a process on the host after its unit is gone.

## Premise

Read on 2026-11-11: `sh -c 'setsid env -i sleep 30 &'` with the tag set gives a `sleep` whose `environ` is empty. The test 3 of the suite uses a grandchild that keeps its environment, so it passes.

## Approach

A shim for each unit with no root (`crates/podrunner-unit/src/shim.rs`): it sets `PR_SET_CHILD_SUBREAPER`, starts the payload, and keeps it as its child. Each orphan of the unit then has the shim as its parent, also after `setsid` and a double fork. `destroy` kills each descendant of the shim (a walk of the parent links of `/proc`), then the shim. The tag stays as a second check. The fake unit runs its top process and each container under a shim; the count is `unknown` when a signal ended the shim. The podvm driver needs no shim: the supervisor of podvm `v0.2.0-beta.3` is a subreaper, an orphan with `setsid env -i` has it as its parent, and `podvm rm -f` ends it. The driver kills the tree under each tagged process, and gives `unknown` when no supervisor lives but processes of the unit do. New conformance tests 25 (an orphan with no tag) and 26 (a killed shim), each with a plant.

## Prove

`cargo test -p podrunner-fake`, and the unit conformance suite on the podvm driver in `.github/workflows/runner-image.yml`: tests 25 and 26 pass on both drivers, and their plants fail them.

## Done

2026-11-11, `30b9366`; gate run 43018641175, `runner-image.yml` run 43018641300. The unit conformance suite passes on the podvm driver in CI (no root, the runner image as the payload): 25 tests pass and test 13 skips (container route); tests 25 and 26 included. They pass on the fake unit, and each plant fails its test. The host scan of test 21 reads regular files only (a named pipe of the podvm store blocked it; a test holds it).

# T-068: serve keeps each job, report and journal line forever

**Source:** T-033
**Category:** defect
**Milestone:** M-D
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

A `serve` that runs for weeks grows with no bound. In `crates/podrunner-slots/src/engine.rs` the maps `jobs` and `suppressed` and the list `reports` only grow: no code removes a final job, an expired suppression or a sent report. `progress` walks each job at each tick. The journal (`crates/podrunner-slots/src/journal.rs`) only appends, and `replay` reads the whole file at each start. The ledger of the remote mint (`crates/podrunner-remote/src/ledger.rs`) reads each line at open, too. Memory, the time of a tick and the time of a start grow with the count of jobs since the first start.

## Premise

Read the code on 2026-11-11: `grep -rn "jobs.remove\|jobs.retain\|reports" crates/podrunner-slots/src` finds no removal. Not measured: the memory of one job, and the time of a replay of a long journal.

## Approach

- Forget a job when it is `reported` and its report went out (T-069): remove it from `jobs`; keep a bounded ring of the last reports for `status` (for example 256).
- Remove a suppression when its time passes.
- Compact the journal at start, after the reconcile: write the open jobs only to a new file, sync it, and rename it over the old one (the lock of `serve` is held). The same for the ledger of the mint: keep the lines that a later call can need (the jobs whose credential still lives, the alert counts of each host).
- A level 1 test: 10 000 jobs on the fake source and the manual clock; the count of `jobs` and the size of the journal after a restart stay under a bound. Plant: no removal.

## Prove

`cargo test -p podrunner-slots bounded`, `cargo test -p podrunner-remote ledger_compact`

## Done

2026-11-11, `94d831d`; gate run 43071088850 of `9fb9d17` is green. A final job leaves memory after one hour, with a suppression of one more hour; the reports keep the last 256; the journal and the ledger of the mint are compacted at start. The tests `bounded_*` and `ledger_compact_*` pass.

# T-069: A report that the source refuses is retried, then spooled

**Source:** T-014
**Category:** feature
**Milestone:** M-D
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

SPEC §11.1: from `destroyed` to `reported`, on a failure: "Retry, then spool in the state directory". `crates/podrunner-slots/src/run.rs` (`report`) sends the report once and drops the error (`let _ = ... .report(...)`). A sidecar that restarts or a remote mint that a host cannot reach loses the report of each job that ends in that time.

## Premise

Read `crates/podrunner-slots/src/run.rs` and SPEC §11.1 on 2026-11-11. Nothing measured.

## Approach

A failed `report` keeps the job in a `reporting` set with a backoff, up to `report_s`. After `report_s`, write the report to `STATE/spool/JOB.json` (mode 0600, no credential) and an audit line `report-spooled`. At start and when the source answers again, send each spooled report and remove its file. `status` counts the spooled reports. Tests on the fake source with a planted refusal of `report`: the report reaches the source after the refusal ends, and after a restart.

## Prove

`cargo test -p podrunner-slots report_spool`

## Done

2026-11-11, `5d2ca12`; gate run 43071088850 of `9fb9d17` is green. A refused report goes to `STATE/spool/` (mode 0600) with an audit line `report-spooled`, and goes again every 30 s; `status` shows `spooled_reports`. `tests/spool.rs` passes.

# T-070: podrunner doctor reads the configuration and the probes of each kind

**Source:** GitHub #22
**Category:** feature
**Milestone:** M-D
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

`podrunner doctor` (`crates/podrunner/src/doctor.rs`) runs `pod` and `podvm` from `PATH`, prints the count of the keys of `podvm probe --json`, and always prints "missing: the guest self-test". It reads no configuration, so it does not use the binaries of `[unit.podvm]` and `driver_dir` (T-036 refuses a driver from `PATH`), does not show the capability record that placement reads, and gives no JSON. SPEC §11.5 and call C9: `doctor` is a thin reader of `podvm probe`, `pod-hostprobe` and the self-test. `pod-hostprobe` is in podcrates `v0.24.0`, and `pod-confine` gives the Landlock ABI.

## Premise

Read `crates/podrunner/src/doctor.rs`, `docs/cli.md` and the README of `pod-hostprobe` at `v0.24.0` on 2026-11-11. Nothing measured.

## Approach

`podrunner doctor [--config FILE] [--json]`. With a configuration: for each unit kind, the driver binary of the file, the capability record of its `probe` through the driver (the record that placement reads), and the tiers that the host can serve with it (`no-tier-served`). Always: the findings of `pod-hostprobe` that the drivers need (user namespaces, mount, chroot, KVM), the Landlock ABI of `pod-confine`, and the hold of `pod doctor --json` when the `pod` binary is given. Each missing input is named; `doctor` decides nothing and changes no file. A test with a fake configuration and a stand-in `podvm` checks the text and the JSON; `docs/cli.md`.

Done: `--config` and `--json`; the capability record of each kind through its driver, the hold of `[unit.podvm] podnet`, the tiers that the host serves (floor and hold), the Landlock ABI and `/dev/kvm` (`crates/podrunner/src/doctor.rs`). The rows of `pod-hostprobe` (`HOST_ROWS`) run in children of the binary; `main` reads the child flag first (`crates/podrunner/tests/doctor.rs`).

## Prove

`cargo test -p podrunner doctor`

## Done

2026-11-11, `5ef2e2a` and `fdcfe51`; gate run 43073532205 of `296bfd0` is green. `podrunner doctor [--config FILE] [--json]` shows the capability record of each kind through its driver, the hold, the tiers that the host serves, the Landlock ABI, `/dev/kvm` and the rows of `pod-hostprobe` (run in children of the binary). `cargo test -p podrunner doctor` passes.

# T-072: U10: the egress module against a proxy that resets CONNECT to an IP literal

**Source:** GitHub #9
**Category:** measurement
**Milestone:** M-D
**Priority:** P2
**Effort:** S
**Status:** done

## Problem

U10 of `docs/unknowns.md`: "The broker fails on a proxy that resets CONNECT to IP literals". `pod-broker` has a names mode (`upstream_names_only`, T-048), but no test sends a job through the egress module to an upstream that resets a CONNECT to an IP literal and accepts a name. U10 stays open, and T-009 with it.

## Premise

Read `docs/unknowns.md` and T-048 on 2026-11-11. T-048 tests the canary against two fake proxies (200 and 403). Nothing else measured.

## Approach

A loopback test of `crates/podrunner-egress`: a fake upstream CONNECT proxy that resets each CONNECT to an IP literal and tunnels a CONNECT to a name to a loopback server; a fake resolver. With `upstream_names_only = false` the request of the unit fails; with `true` it reaches the server, and the canary check answers as the proxy says. Write the result into U10.

## Prove

`cargo test -p podrunner-egress ip_literal`

## Done

2026-11-11, `9fb9d17`; gate run 43071088850 of `9fb9d17` is green. `ip_literal_reset_and_the_names_mode`: an upstream that resets a CONNECT to an IP literal fails a unit with addresses and passes it in the names mode. U10 is settled in `docs/unknowns.md`.

# T-074: The disk limit of a unit

**Source:** T-014
**Category:** feature
**Milestone:** M-D
**Priority:** P2
**Effort:** M
**Status:** done

## Problem

SPEC §10.4: H0 has no mount, so no sized tmpfs and no quota. "`slots` watches the unit's workspace with `du` on an interval and ends the job with `limit`. `limits.disk` is `reported`. A job whose `limits_required` names `disk` on a unit that reports nothing for it is `limit-not-enforceable`." No code of slots or of a driver watches the disk of a unit: a job can fill the disk of the host.

## Premise

Read SPEC §10.4 and `crates/podrunner-slots/src` on 2026-11-11: `disk` appears only in the journal code. `limits` of the capability record of `pod-job` names `disk`. Nothing measured.

## Approach

`observe` gives the unit root. slots sums the bytes under it (no symlink followed, one file system) every `disk_check_s` (default 30), and ends a job over its limit (the `limits` of the offer, else `[unit.X] disk_max_bytes`) with the reason `limit` and an audit line. The fake and podvm kinds report `disk: reported` only when the operator sets a bound. Test on the fake unit with a payload that writes past the bound; plant: no check.

## Prove

`cargo test -p podrunner-slots disk_limit`

## Done

2026-11-11, `2f58ecb`; gate run 43073532205 of `296bfd0` is green. Every 30 s slots sums the bytes under the root of each running unit with a bound (the `disk` limit of the offer, else `[unit.X] disk_max_bytes`) and ends a unit over it with an audit line `limit` (`disk`). `disk_limit_ends_a_unit_over_its_bound` passes.

# T-075: The report and status fields of SPEC §11.4 and §11.5

**Source:** T-014
**Category:** feature
**Milestone:** M-D
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

SPEC §11.4 names `claim_to_first_step_s` (from the claim to the job-started hook) and `job_images` (the digests of the job images) in the run report; SPEC §11.5 names credential ages and the App id and key age in `status`. `RunReport` of `pod-job` at `v0.24.0` has neither report field, nor the exit reason `limit` of SPEC §10.4 (a unit over its disk bound ends as `failure` with an audit line `limit`, T-074), and `crates/podrunner-slots/src/status.rs` shows no age of a credential or of the App key.

## Premise

Read SPEC §11.4, §11.5, `pod-job` (`report.rs`) at `v0.24.0` and `crates/podrunner-slots/src/status.rs` on 2026-11-11. Nothing measured.

## Approach

Ask podcrates (a direct message) for the two optional fields in `RunReport` of `pod-job`; fill them in slots from the `started` time and the images of the hook calls. Add to `status` the age of each credential of a running job and the age of the key file of each `github-scaleset` source (from its time of change), with no value of the key. Regenerate `schemas/status.v1.json`.

Done: `status` shows `credential_ages_s` (the age of the credential of each job that is not final, from its mint) and `key_age_s` (the age of the key file of a source, from its time of change) for each source; `schemas/status.v1.json` is regenerated (`the_status_shows_the_age_of_each_credential_and_of_the_key`).

## Prove

`cargo test -p podrunner-slots status_schema`, `cargo test -p podrunner-slots report_fields`

## Done

2026-11-12, `4284967`; gate run 43130712544, level2 run 43130712280 and runner-image run 43130712347 are green. podrunner takes podcrates `v0.26.0` (PodUtils/podcrates#77, PodUtils/podcrates@T-226). The report names `claim_to_first_step_s` (from the claim to the `job-started` hook) and `job_images` (the digest of each image of a container of the hooks, once); the public report leaves both out. A unit over its disk bound ends with the exit reason `limit`. `report_fields_name_the_first_step_and_the_job_images`, `each_command_reaches_the_fake_unit` and `disk_limit_ends_a_unit_over_its_bound` pass.

# T-076: podrunner resolves names with pod-dns: no single resolver of the host

**Source:** the owner, 2026-11-11
**Category:** feature
**Milestone:** M-D
**Priority:** P1
**Effort:** M
**Status:** done

## Problem

podrunner uses only the system resolver of the host. The egress module resolves each CONNECT target with `tokio::net::lookup_host` (`crates/podrunner-egress/src/lib.rs`), and the client of a remote mint dials its `host:port` the same way (`crates/podrunner-remote/src/transport.rs`). On a host whose resolver fails, each job loses its egress and each unit host loses its mint. A runner of GitHub Actions showed such a failure: the build container of `runner-image.yml` resolved no Ubuntu mirror (fixed in the workflow: the host network and retries).

## Premise

Read on 2026-11-11: `pod-dns` of podcrates `v0.24.0` resolves in the order IP literal, pins, system resolver, the cache of addresses that carried a session, DNS over HTTPS to resolvers by their address, the old cache. It plugs into `pod_proxy::Dialer` as `Arc<dyn pod_core::Resolve>`. No crate ask is needed.

## Approach

One `pod_dns::Resolver` for each process, from a `[dns]` table of the configuration (pins, DNS over HTTPS on or off, the resolvers; the default list of `pod-services`). The egress module dials through `pod_proxy::Dialer` with it, and the transport of the remote mint resolves its host with it. A refusal names each source that was tried (`Stage::Dns`). `podrunner doctor` (T-070) prints the source that answered for the hosts of the configuration. The audit line of a failed dial names the source.

## Prove

`cargo test -p podrunner-egress dns` (a name that the system cannot resolve answers from its pin) and `cargo test -p podrunner-remote a_pinned_host` (the mint dial by a name that only the lookup knows). DNS over HTTPS itself is a test of `pod-dns`.

## Done

2026-11-11, `03d3aac`; gate run 43071088850 of `9fb9d17` is green. The egress and the HTTPS transport of the remote mint resolve through `pod-dns` (system resolver, cache, DNS over HTTPS by address) with the pins of `[dns]`. The tests of `podrunner-egress` and `podrunner-remote` pass.

# T-077: COORD-009: the seccomp hold of podnet pod does not take ::ffff:127.0.0.1 as loopback (GitHub #64)

**Source:** GitHub #64
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** blocked

## Problem

Under `podnet pod --backend seccomp`, a connect to `[::ffff:127.0.0.1]:PORT` goes to the `-x` chain (rule `default`); `127.0.0.1` and `[::1]` go direct (rule `loopback`). .NET connects with dual-stack sockets, so the official runner in a held unit sends a loopback connect to the egress module, which refuses a non-public address. Production does not hit it (the broker of GitHub is public), but the held test of the fake broker does.

## Premise

Measured on 2026-11-11 on H0 with podnet `v0.4.0-beta.3`, podvm `v0.2.0-beta.4` and `--flow-log` (GitHub #64 has the table).

## Approach

podnet takes an IPv4-mapped address by its IPv4 address in the rules of the seccomp backend. Until then, `crates/podrunner-broker-fake/tests/held.rs` gives the runner `DOTNET_SYSTEM_NET_DISABLEIPV6=1`. With the fix: pin the podnet release, remove the variable, and run the held test.

## Prove

`cargo test -p podrunner-broker-fake --test held -- --ignored` with no `DOTNET_SYSTEM_NET_DISABLEIPV6`, in `runner-image.yml`.

## Blocker

A podnet release that takes `::ffff:127.0.0.0/104` as loopback (GitHub #64, to podnet).

# T-078: The podvm driver labels each unit and reads the run report of podvm

**Source:** PodUtils/podvm#268
**Category:** feature
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

The podvm driver finds the processes of a unit by a tag in their environment, which a payload can drop (`env -i`, design test 3), and it learns nothing of a run from podvm but the exit code. podvm on `main` after `v0.2.0-beta.4` keeps `run --label KEY=VALUE` (`ps --filter label=`, `inspect --format '{{.Labels}}'`) and writes `run --report PATH`: one JSON document for each run with `image_digest`, `exit_code`, `signal`, `wall_ms`, `max_rss_kib`, `timed_out`, `refused` and `degradations`.

## Premise

Read PodUtils/podvm#268 (COORD-049) on 2026-11-11. Not in a podvm release yet. Nothing measured.

## Approach

With the next beta: give `run -d` and each container `--label podrunner.unit=ID` and `--report` in the unit directory; `destroy` also lists `ps -a -q --filter label=podrunner.unit=ID` and removes each. The image digests of the reports fill `job_images` of the run report (T-075); a `refused` or `timed_out` value goes to the audit. Pin the beta and raise `probe::LOWEST`.

## Prove

`cargo test -p podrunner-podvm --test conformance -- --ignored` on the new beta, and a test that the run report of a unit names the digest of its image.

## Done

2026-11-12, `e42d60c`; gate run 43143954799 and runner-image run 43143954813 are green. podvm `v0.2.0-beta.5` is pinned (`images/podvm.toml`, `probe::LOWEST`; PodUtils/podvm#273). The `run -d` of a unit and of each container carries `--label podrunner.unit=ID`, and `destroy` removes each container with the label (`crates/podrunner-podvm/src/unit/labels.rs`). The `run -d` of a unit writes `--report` to its directory; `PodvmUnit::run_report` reads it. Measured: for `run -d` the report ends with the start (`detached: true`, `exit_code: null`), so the exit stays with `podvm wait`, and a refusal of podvm fails the start already. `job_images` comes from the hooks (T-075), so the report adds no field to the run report of podrunner. `crates/podrunner-podvm/tests/report.rs` passes as root and as a uid with no user namespace; the conformance, hold, links, held-runner and canned-job tests pass on beta.5 on H0.

# T-079: Measure podvm --podvm-confine on the supervise rung, for a confined podvm unit

**Source:** T-042
**Category:** measurement
**Milestone:** later
**Priority:** P3
**Effort:** M
**Status:** done

## Problem

T-042 waits for podvm to apply `pod-confine` in its units. podvm `v0.2.0-beta.5` has `run --podvm-confine read=P;write=P;exec=P;connect=PORT`. Its help says that only a rung with a chroot and the loader family of the userland rung take it, and each other rung refuses by name. On H0 the supervise rung takes it with no refusal, so the help and the binary do not agree, and no proof shows that the rules hold there. On the namespace rung (root) a spec path is a guest path, and `write=/tmp` is denied over the private tmpfs of `/tmp`.

## Premise

Measured on 2026-11-12 on H0 with podvm `v0.2.0-beta.5` and the image `glibc-mini:1`:

- As uid 65534 with no user namespace (supervise rung), `--podvm-confine 'read=/;write=/tmp'` runs. A write to `/tmp` passes, a write to `/etc` fails (EACCES), and a read of `/proc/1/environ` fails. An unconfined run already shows `NoNewPrivs: 1` and `Seccomp: 2` (the supervisor of podvm), so the status lines prove nothing.
- A spec path is checked on the host: `write=/podrunner` fails with ENOENT for `<store>/trees/<id>/rootfs/podrunner` when the image has no such directory.
- As root (namespace rung), `write=/tmp` is denied, and the unconfined payload has no `/proc`.

## Approach

1. Write a measure script (`scripts/measure-podvm-confine.sh`): a positive control (write in the allowed path), a read of a sibling unit's tree by its host path, a read of a host file by its host path, with and without `--podvm-confine`, on the supervise rung and as root. A path that the redirect of the interposer hides already proves nothing: each check must pass unconfined and fail confined.
2. If a check passes unconfined and fails confined, give the podvm driver an option `[unit.podvm] confine = true`: `run -d` and each container `run -d` take `--podvm-confine` with read and execute on the system directories of the image and read and write on the working directory and the socket directory, and the record says `boundary: confined` with the proof `podvm --podvm-confine`. Run unit conformance test 27 on it, and its plant (the record claims `confined`, no spec is given) must fail.
3. If the help is right and the supervise rung must refuse, write a COORD issue `(to podvm)`.

Step 1 measured on 2026-11-12 (`scripts/measure-podvm-confine.sh`, spec `read=/;write=/tmp`):

| Check | supervise, uid 65534: unconfined / confined | namespace, root: unconfined / confined |
| --- | --- | --- |
| control (write and read `/tmp`) | reached / reached | reached / denied |
| host file by its host path | denied / denied | denied / denied |
| `/proc/PID/environ` of a host process of the uid | reached / denied | denied / denied |
| `/proc/PID/root/PATH` of a host process | reached / denied | denied / denied |
| `/proc/PID/cwd/FILE` of a host process | reached / denied | denied / denied |

On the supervise rung the confine layer closes the three paths of R2 (`docs/security/residuals.md`) and keeps the control. On the namespace rung the payload has no `/proc`, and the spec denies its own `/tmp`. Open: step 2.

## Prove

`sh scripts/measure-podvm-confine.sh PODVM DIR` as uid 65534 with no user namespace and as root; `cargo test -p podrunner-podvm --test conformance -- --ignored` with `confine = true`.

## Done

2026-11-12, `bd07a93` and `660bdf1`; gate run 43155326553, level2 run 43155326441 and runner-image run 43155326439 of `660bdf1` are green. Steps 1 and 2: `[unit.podvm] confine = true` runs each unit and container under `--podvm-confine` (`crates/podrunner-podvm/src/unit/confine.rs`), and `verify_confine` proves it on the host before the kind says `confined`; `confine_is_proven_only_by_a_check_that_sees_and_a_layer_that_holds` holds the plants. On the supervise rung of the GitHub host the measure gives environ and root reached unconfined and denied confined (cwd is denied both ways there); test 27, the canned job and the container job pass under the layer. As root (namespace rung) the self-test refuses the claim: the open run sees no `/proc`. Step 3 is not needed: the supervise rung takes the spec. Tests 25 and 26 skip under the layer: T-080.

# T-080: Tests 25 and 26 skip on a confined podvm unit: the orphan is not seen on the host

**Source:** T-079
**Category:** defect
**Milestone:** later
**Priority:** P3
**Effort:** S
**Status:** done

## Problem

With `PODRUNNER_PODVM_CONFINE=1`, unit conformance tests 25 (an orphan with no tag) and 26 (a killed shim) skip with "the orphan is not seen on the host (a pid namespace)", on H0 and on the GitHub host. Unconfined they pass. So the tree kill of a confined unit has no proof of its own: `tree_kill: verified` comes from a self-test of an unconfined unit.

## Premise

Seen on 2026-11-12 in runner-image run 43155326439 and on H0 (podvm `v0.2.0-beta.5`, supervise rung). The skip comes from `an_orphan_with_no_tag` of `crates/podrunner-unit/src/conformance/design.rs`: it needs the pid of the orphan alive with an empty `/proc/PID/environ` on the host. Not measured: whether the pid file holds a host pid, or whether the host test cannot read the environ of a process in the Landlock domain.

## Approach

Run the escape payload under `--podvm-confine` by hand and compare the pid of the file with the host pids of the unit (`observe`), and read `/proc/PID/status` of the orphan. If the environ read is the cause, make the check of the test use another proof of an empty environment (the `status` line, or the payload). Then run `verify_tree_kill` on a confined unit too.

## Prove

`PODRUNNER_PODVM_CONFINE=1 cargo test -p podrunner-podvm --test conformance -- --ignored`: tests 25 and 26 pass.

## Done

2026-11-12: the cause is the layer, not the test. Under `--podvm-confine`, `env -i PROGRAM` fails with EACCES: with no environment the exec leaves the interposer of podvm, and Landlock denies the exec of the host path. Unconfined the same exec runs the host binary (an escape of the redirect). So a confined payload cannot start the orphan of tests 25 and 26; the measure script has the check `escape-env-i` (unconfined reached, confined denied on H0). The skip message names both causes, and test 25 also takes an environment that the host cannot read (a confined process is not dumpable) as no tag. The tree kill stays proven by `verify_tree_kill`.
