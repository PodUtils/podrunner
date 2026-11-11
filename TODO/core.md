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

2026-11-11, commit `229d29f` (and the gate run 42936703374 of `51ffb40`,
green, which holds it). `crates/podrunner-unit` (the trait, the tests 1
to 17, the stub) and `crates/podrunner-source` (the wire, the trait,
`WireSource`, `serve_line`, the tests 1 to 12). On the stub and on a
refuser, each test fails and is reported by its number. The wire refuses a
line over 1 MiB, an unknown `op` and a `job_id` of `../x`
(`bad-request`); the fuzz target `source_wire` builds and survives its
seeds and 5000 mutations. `docs/contracts/` holds `unit-driver.md`,
`source-driver.md` and `hook.md`, version 1; two tests compare the
operations and the tests with them. `cargo test -p podrunner-unit -p
podrunner-source`: 12 passed. The fakes of T-020 are the correct inputs
and the plants of each test.


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

Rust types with serde for the four records, the four JSON Schemas, 11 valid and 19 invalid examples with the expected reason, and a test that validates each. Ruling Q-A8: the schemas live in podcrates; ask podcrates for them (PodUtils/podcrates@T-163) and use the released types when they come.

podrunner drafted the types and the schemas; podcrates released them as `pod-job` in v0.17.0 (PodUtils/podcrates#58). `crates/podrunner-schema` now re-exports `pod_job` and keeps the credential (`secret.rs`) only; the four schemas, the examples and their test are in podcrates. `schemas/status.v1.json` stays here: it embeds the capability record of `pod-job`. `docs/contracts/codes.md` points to the table of `pod-job` and lists the uses of podrunner beyond it.

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

2026-11-11. `docs/security/threat-model.md` (the credentials, the
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

2026-11-11, commit `e8f4c0d`; the gate run 42938706745 of `f72f679` is green. `crates/podrunner-fake` and `crates/podrunner-clock`, `docs/fakes.md`. `cargo test -p podrunner-fake`: 7 passed: the fake unit passes the unit tests 1 to 17 (13 or 14 skip by the container route), the fake source passes the source tests 1 to 12, each of 17 unit plants and 12 source plants fails the test of its number, the script follows the manual clock. `fake` is refused unless `allow_fake = true` (`crates/podrunner-slots/src/config.rs`), and a report of a fake unit names the kind `fake`. A fake that claims `guest` fails the guest self-test once T-036 exists.


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

The state machine with a deadline for each transition, the placement steps, the 19 refusal codes, the journal and the reconcile on start, the `degraded` state, `podrunner.toml`, the audit lines and the exit codes of `pod-core`. The level 1 tests G-01, G-02, G-05, G-08 and G-12 run on the fakes of T-020 with a manual clock.

Done so far (2026-11-11): `crates/podrunner-slots` holds the configuration (`docs/configuration.md`), placement, the states, the journal with the reconcile at start, the audit lines (forwarded to the source), the reports (full or public), the engine, and 14 tests: G-01, G-02, G-05, G-08, G-12, and the guards (untrusted below guest, a fork on the trusted set, a stale image, a digest off the pin list, an unenforceable limit, a shared principal, a foreign runner name, a restart that ends the units of the journal, no credential in the journal, the audit or a report). The fuzz targets `slots_config` and `slots_journal` exist. With T-022 (2026-11-11): `status` (`crates/podrunner-slots/src/status.rs`, `schemas/status.v1.json`), `no-tier-served` (a source of a tier that no kind meets gets no capacity), the clear of `degraded` by the control socket, and the exit codes of `pod-core` in the CLI.

## Prove

`cargo test -p podrunner-slots`, and `cd fuzz && cargo test --lib`

## Done

2026-11-11, commits `f72f679` and `644bab7`; the gate run 42940434305 of `223c009` is green. `cargo test -p podrunner-slots`: 17 passed (10 loops of the first 14, each green): G-01 (no source holds more than its 2 slots; 20 jobs end `success`), G-02 (a `guest` job waits while its slot is busy and runs when it frees), G-05 (an arm64 job is not claimed), G-08 (one withdraw for each job past `wait_deadline_s`; no start after a withdraw; no claim after `drain`), G-12 (a job past its wall time ends `timeout`, its unit gone with 0 processes); the guards: `untrusted` below `guest`, a fork on the trusted set, a stale image, a digest off the pin list, an unenforceable required limit, a shared principal, a foreign runner name, a restart that ends the units of the journal (with no replay, the processes outlive podrunner), no credential canary in the journal, the audit or a report; the status. `cd fuzz && cargo test --lib`: the targets `slots_config` and `slots_journal` survive their seeds and 5000 mutations.


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

The control socket (JSON lines on a unix socket, mode 0600, peer uid checked, `"v": 1` first) behind a trait that `pod-control` can replace; `status --json` with a schema in `docs/`; `doctor` as a thin reader; `print-unit` with no credential.

Done so far (2026-11-11): `crates/podrunner/src/control.rs` (the socket of mode 0600, the peer uid, `"v": 1`, the trait `Handler`), `cli.rs` (`serve`, `status [--json]`, `drain`, `stop`, `clear-degraded`, `doctor`, `print-unit`; the exit codes of `pod-core`), `text.rs` (an escape prints as `\x1b`, a value is cut at 200 bytes), `doctor.rs` (a thin reader that names each missing input), `serve.rs` (the `fake` source and unit; another kind is refused with code 80 until its entry). `docs/cli.md`. The fuzz target `control_line`. A run by hand: `serve` with a fake script, `status`, `status --json`, `stop`.

## Prove

`cargo test -p podrunner`

## Done

2026-11-11, commit `644bab7`; the gate run 42940434305 of `223c009` is green. `cargo test -p podrunner`: 11 passed: a display name with an ANSI escape prints as `\x1b[2J` and a value is cut at 200 bytes; the socket has mode 0600 and answers one request; bad lines are `bad-request`; usage errors exit 64, a socket that does not answer 69, a bad configuration 78; `print-unit` holds no credential canary. `status --json` follows `schemas/status.v1.json` (generated from the type; a test fails on a difference). The fuzz target `control_line`. The module sits behind the trait `Handler`. A run by hand: `serve`, `status`, `status --json`, `stop` (exit 0), on the fake kind and on the podvm kind.


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

A driver over the released podvm binary: a store for each unit, a subreaper shim for the tree kill, the probe record from `podvm probe`. It runs the unit conformance tests with a stand-in payload. A fix that needs podvm goes to a COORD issue `(to podvm)`, and its work waits (podvm is on break).

Done so far (2026-11-11): `crates/podrunner-podvm`: the version range of
podvm (`0.2.0-beta.2` up to `0.3.0`, each pre-release of `0.3.0`
outside; a podvm outside it fails `probe` by name) and the map of
`podvm probe --json` to the capability record (each rung `redirect`,
`boundary_proof` names the rung, `tree_kill: unverified`, `egress_hold:
unknown`), with three recorded probes in `crates/podrunner-podvm/tests/probe/`.
Measured on the session host with the released podvm `v0.2.0-beta.2`
(SHA-256 of `podvm-x86_64`
`0af41a96233c7f9f3ed48b80ebb3271fe9eaa1295616ea0375625f8035baf6a9`): the
rung is `namespace` (the host has root: it is not H0); `podvm run` gives
the exit code of the payload; a `setsid` grandchild outlives `podvm stop`
and `rm -f`, as GitHub #23 says. The driver `PodvmUnit`
(`crates/podrunner-podvm/src/unit.rs`): `prepare` saves the pinned image
from the store of the operator into a tar; `start` loads it into a store
of the unit, writes the credential files (mode 0600) into the rootfs that
`podvm extract` names, and runs the payload detached with the tag of the
unit, `PODRUNNER_UNIT` and `DOTNET_EnableWriteXorExecute=0`; `destroy`
kills each tagged process, removes the container and the store, and
counts. `serve` builds it from `[unit.podvm]` (`binary`, `images`). The
unit tests 1 to 17 on this host with busybox as the stand-in payload: 16
pass, test 14 skips (no container route); 5 runs, each green, no process
left. `verify_tree_kill` runs test 3 with the pinned
payload at the start of `serve`; only then does the record say
`tree_kill: verified` and `wall: enforced` (3 runs, each green). A job of
a local fake source ran through `serve` on a podvm unit: offered,
claimed, minted, started, ended `success` (the payload found
`.runner/.credentials`), destroyed, reported. The H0 run (2026-11-11): as `nobody`, with a
seccomp filter that refuses user namespaces, on this host with no KVM
(`podvm probe`: rung `supervise`, `CLONE_NEWNS`, `CLONE_NEWUSER`, mount
and chroot denied; `crates/podrunner-podvm/tests/probe/supervise.json`).
busybox does not run there: the loader of the rung needs glibc, libdl
and libgcc_s in the image, so `scripts/stand-in-image.py` builds a
stand-in image from host binaries. The unit tests pass 16 of 17 (test 14
skips), two runs; a job of a local fake source through `serve` ends
`success` 10 of 10, and a payload `exit 3` reports 3. On the way: `podvm
wait` there gives no exit code for a payload that ends at once (T-047,
GitHub #49); the driver runs the payload under `/bin/sh` of the image,
which writes the code to `.podrunner-exit`, reads it when `podvm wait`
gives none, and waits again while a process of the unit lives. U25: on
the supervise rung `podvm run -v /proc/self/fd/N:/cred` mounts the open
file (the payload reads it); the driver still writes the files into the
rootfs. The journal line `ended` now carries the exit code.

## Prove

`cargo test -p podrunner-podvm`, and `cargo test -p podrunner-podvm --test conformance -- --ignored` with `PODRUNNER_PODVM`, `PODRUNNER_PODVM_IMAGES` and `PODRUNNER_PODVM_IMAGE` (as root: the namespace rung, busybox; as a uid with no user namespace: the supervise rung, the image of `scripts/stand-in-image.py`)

## Done

2026-11-11: the podvm driver passes the unit tests 1 to 17 (14 skips: no container route) on the namespace rung and on the supervise rung of an H0 profile; a job through `serve` ends `success` 10 of 10 there; U25 is settled. `cargo test -p podrunner-podvm`: each test passes.

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

A module over `pod-broker` with one listener for each unit, bound to one job, and no token in the unit. It needs PodUtils/podcrates@T-160 and PodUtils/podcrates@T-161; use the tag that podcrates releases. The hold of a podvm unit is `podnet pod` (T-044, Done): `advisory` with the seccomp backend; read the level from `hold.seccomp` of `pod doctor --json`. podcrates `v0.16.0` (PodUtils/podcrates#57, COORD-032) holds
`pod-broker` with `Broker::serve_unix_job(job, listener)` (a listener of
one job, no token in the unit; a `Proxy-Authorization` of another job
gets 407), `Broker::load(Load { job, token, ttl, policy })`,
`Broker::drop_job`, `Policy::from_json`, and an async API on tokio: the
module runs the broker on a runtime of its own and needs a resolver
(`Resolve` of `pod-core`, for example the resolver of `pod-dns`) and a
dialer (`pod_broker::upstream`).

Done so far (2026-11-11): `crates/podrunner-egress` (`pod-broker` and
`pod-proxy` of `v0.16.0` on a tokio runtime of their own; the system
resolver; `open` binds a unix socket of mode 0600 to one job, `close`
drops the job and its listener, `kill` ends the module). Its tests: two
units get two listeners with two policies, a request with no token is the
job of its listener, a token of unit A on the listener of unit B gets
407, a close and a kill end the listeners, a bad policy is named without
its text. slots (`EgressPort`): each unit gets a listener of its job with
the policy of its offer (else `[egress] default_policy`, else no way
out) and the variable `PODRUNNER_EGRESS=http://unix:PATH`; the end of the
unit closes it; when egress ends, no source gets capacity and each running
unit ends (`unit-failed`); a kind with hold `none` gets no `trusted` job.
U24 is settled (`docs/unknowns.md`). No host of the Actions service is in
the code: the default policy comes from the configuration. A finding of
the run by hand: under a long state directory the socket path passed the
limit of 108 bytes and the job ended `unit-start-failed` with no reason;
the egress directory is now short when it must be, and the audit line
names the reason. Open: the unit of podvm reaches a host socket only
after PodUtils/podvm#235 (U21).

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

2026-11-11: T-015 to T-024 are done; the level 1 suite and the unit tests pass in the gate; on an H0 profile (no root, no user namespace, no mounts, no KVM; the probe is `crates/podrunner-podvm/tests/probe/supervise.json`) the podvm driver passes its unit tests with a stand-in payload, the credential files are at 0600 and in no argv or environment (tests 6, 7), and a job of a local source runs through `serve`. No trusted job ran.

# T-047: COORD-002: podvm wait gives no exit code for a short payload on the supervise rung (GitHub #49)

**Source:** GitHub #49
**Category:** defect
**Milestone:** M-A
**Priority:** P2
**Effort:** S
**Status:** blocked

## Problem

With the released podvm `v0.2.0-beta.2` on the supervise rung (a uid with no root, no user namespace, no mounts, no KVM), `podvm wait NAME` of a detached payload that ends at once often exits 125: "NAME has no exit code podvm measured: it is Up (podvm TODO/supervise.md, PodUtils/podvm@T-0604)". The podvm driver then read no exit code and reported -1: 7 of 10 jobs of an H0 run ended `failure`. GitHub #49 (COORD-002, to podvm) records it; podvm is on break.

## Premise

Measured on 2026-11-11 as `nobody` with a seccomp filter that refuses `CLONE_NEWUSER`: `podvm wait` exited 125 in 4 of 5 runs of `sh -c 'exit 0'`.

## Approach

The workaround is in the driver (T-023): the payload runs under `/bin/sh` of the image, which writes the exit code to `.podrunner-exit` in the working directory; when `podvm wait` gives no code, the driver reads that file, and starts `podvm wait` again while a process of the unit lives. When podvm answers, remove the workaround if `podvm wait` gives each code.

## Prove

`cargo test -p podrunner-podvm --test conformance -- --ignored`, as a uid with no user namespace, with the image of `scripts/stand-in-image.py`

## Blocker

podvm (on break) fixes `podvm wait` on the supervise rung (PodUtils/podvm@T-0604). Until then the workaround of T-023 holds.

