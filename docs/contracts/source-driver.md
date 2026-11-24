# The source driver contract, version 1

A source finds jobs: a forge, a GitHub scale set (the Go sidecar of
GitHub #29), or the `fake` source. The seam is JSON lines from the first
day, because the scale-set sidecar is Go. In the process of podrunner, a
source is the trait `JobSource` of `crates/podrunner-source/src/lib.rs`;
`WireSource` speaks the wire to a source in another process, and
`serve_line` answers one line of the wire from a `JobSource`.

## Wire

- One request on one line, one reply on one line, UTF-8 JSON, each line
  with `\n` at its end.
- Each object starts with `"schema": 1`. Another value, or no value, is
  `bad-request`.
- A request has `"op"`; a reply has `"reply"`: `hello`, `events`,
  `credential`, `done` or `error` (with `code` and `detail`).
- A line over 1 MiB (1048576 bytes) is `bad-request`. An unknown `op`, an
  unknown field and a bad `job_id` are `bad-request`.
- A `job_id` matches `[A-Za-z0-9._-]{1,128}` and is not `.` or `..`. It is
  never a path component. A unit id is `u-` and a counter, and never
  derives from a `job_id`.
- The `detail` of an error holds no value of the request: a value can be
  a credential.

## Operations

| Operation | Request fields | Reply |
| --- | --- | --- |
| `hello` | `client`, `version`; optional `host`: `podrunner`, `podvm`, `podnet` (versions), `arch`, `kvm`, `kinds` (each `name`, `boundary`, `container`, and the largest `cpu` in milli-cores, `memory` and `disk` in bytes, `pids`) | `hello`: `source`, `operator_controlled` |
| `next` | `capacity`, `wait_s` (the longest wait of the reply, in seconds; a source with a slower poll keeps it in the background) | `events`: a list of events |
| `ack` | `job_id` | `done` |
| `claim` | `job_id`, `runner_name` | `done` |
| `mint` | `job_id`, `runner_name` | `credential`: `files` (`name`, `content`) |
| `report` | `job_id`, `report` | `done` |
| `audit` | `line` | `done` |
| `withdraw` | `job_id`, `reason` | `done` |
| `release` | `job_id` | `done` |
| `assigned` | `job_id` (of the offer), `real`: `run_id`, `job_name`, `event`, `repository`, `actor`; optional `head_repository` (`OWNER/REPO` of the head of a pull request), `head_is_fork` (true when the head is another repository than `repository`, as `trust_inputs.head_is_fork` below), `job_id` (the number of the job at the forge) | `keep`: `trust_inputs` (of `job-offer.v1`); or `end`: `code`, `detail` |

The order of one job: `offer` (an event of `next`), `ack`, `claim`,
`mint`, then `report` and `release`. `withdraw` ends an offer that
podrunner does not claim (a `wait` refusal at `wait_deadline_s`).

How a job ends at the source (T-200):

* `withdraw` with a reason ends an offer that podrunner does not claim.
  The reason is a refusal code (`tier-mismatch`, for example), or
  `wait-deadline` (no slot came free in time), `cancelled` (the forge
  cancelled the job) or `conformance` (a test of `source-conformance`
  gives the job back). Only a refusal code says that this host must not
  take the job; a source can offer the job of another reason again, also
  to the same host.
* `release` carries no code, and follows the `report` of the job (a
  report that the source refuses goes to the spool and comes later). The
  report says how the job ended: a `refusal_code` says that podrunner
  refused the claimed job (`unit-start-failed`, for example), so this
  host must not take it again; `exit_reason` `lost` with no
  `refusal_code` says that a restart lost the job, and the source can
  offer it again; else the job ran.
* A `release` with no `report` gives the job back: only a test of
  `source-conformance` sends it. A source can offer the job again, also
  to the same host.
* The report is `run-report.v1` of `pod-job`. `cpu_s` is the CPU time
  of the processes of the unit, rounded up to whole seconds: within 20 %
  or 1 s of the CPU time of the payload, the larger (T-520). The kinds
  whose processes are on the host sample them at each tick, so a process
  that ends between two ticks can count up to one tick less. The incus
  kind reads the time of incusd at each tick while the instance runs
  (incusd gives none of a stopped instance), so it can count up to one
  tick of each busy CPU less. `peak_rss_mib` is the highest sum of the
  resident memory of one sample (or the highest peak of incusd), in MiB
  rounded up. A kind that cannot measure a field gives `unknown` with a reason.

podrunner writes and syncs the `claimed` line of its journal before it
sends `claim` (GitHub #55). After a death between the two, the restart
sends `release` for a job that it may not hold. `release` names no
runner, so a source must not free the claim of another runner on it: the
scale-set sidecar releases only a runner that it registered itself.

The forge, not the source, picks the queued job that a runner takes: a
runner of a just-in-time configuration can get a job of another run of
the same repository (T-175, githome's G-15). The job-started hook of the
runner image compares the run of the job with the run of the offer. When
they differ, `serve` sends `assigned` with the facts of the real job
(T-493). A source answers `keep` with the trust facts of the real job, as
it reads them from the forge; podrunner computes the tier of those facts
and runs the job only when it is the tier of the unit (a job of a fork
never runs in a trusted unit). Else, and on `end`, the job ends before
its first step with an annotation that names the code. A source with no
`assigned` answers `bad-request`, and the job ends as before. No answer
within 10 s ends the job with the code `source-unavailable`.

`serve` sends `hello` with `host` to each source at its start (T-495,
githome's G-17), so that a source offers no job that the host must refuse.
A change of the configuration needs a new start of `serve`, which sends
the new facts. A source whose wire refuses the field logs one line, and
the start goes on.

`audit` carries the audit lines of the slots, and one line for each
connection of a unit through its egress module when the connection ends
(T-486, githome's G-04): `{ job_id, host, port, verdict, rule, bytes_in,
bytes_out, at }`, with `verdict` `allow`, `deny` or `fail` and `at` in
RFC 3339 UTC. No path, header or body of a request is in it. A job gets
at most 1000 of them; then, when the job is final, one line
`{ job_id, dropped }` with the count of the rest.

A source in a child process (the scale-set sidecar) answers each request
within 15 s after the long poll of `next` (`REPLY_LIMIT`,
`crates/podrunner-mint/src/sidecar.rs`). A child that passes it is killed
and starts again after the backoff (GitHub #57): on a lock-step wire, a
late reply would be read as the answer to the next request.

The scale-set sidecar gives two kinds of job id. A job of a
`JobAvailable` message has the id of its runner request (`41`); its
`claim` calls `AcquireJobs`. A job that GitHub assigns to the scale set
with no `JobAvailable` has runner request 0 and the id `a-JOBID`, with
the `jobId` of GitHub; it is acquired already, so its `claim` calls
nothing (T-166). Each later message of a job (`JobAssigned`,
`JobStarted`, `JobCompleted`) maps to the id of its offer through its
`jobId`. GitHub binds no runner of the scale set to one job: the
`started` check above is the only proof that a runner took its own job
(T-175).

## The form of an offer

The parser of `job-offer.v1` is `pod_job::JobOffer` of podcrates. The
design copy (`docs/design/schemas`) stays as written (call C17); where it
differs, the parser is the contract (T-188):

| Field | The parser takes |
| --- | --- |
| `trust_inputs.head_is_fork` | `yes`, `no` or `unknown` (not a boolean). `yes` when the repository of the head is another repository than the repository of the job (another id; with no id, another `owner/repo` in any case); `no` when it is the same; `unknown` when one of the two is not known. For an event that is not a pull request, the head is the repository of the run, so the value is `no` (`workflow_run`, `issue_comment`, `repository_dispatch`, `release`; a push to a fork gives the fork as the repository of the job). Do not send the `fork` flag of the head: a pull request can come from the root of a fork network (`fork` false), and a push to a fork has the fork as its head (`fork` true) |
| `trust_inputs.actor_class` | `member`, `outside` or `unknown` (not `external`) |
| `trust_inputs.run_id` | a number |
| `limits`, `limits_required` | the names `wall`, `cpu`, `memory`, `pids`, `disk` |
| `policy` | `{}`, or a policy of `broker-policy.v1` with `schema: 1` |

The mint withdraws an offer that does not parse with the reason
`bad-request` and writes the text of the parser to its audit.

## Events

| Event | Fields | Meaning |
| --- | --- | --- |
| `offer` | `offer` (`job-offer.v1`, with `job_id`) | A job that podrunner can take. |
| `cancel` | `job_id` | The forge cancelled the job. |
| `completed` | `job_id`, `result` | The forge says that the job ended. |
| `started` | `job_id`, `runner_name` | The forge says that a runner took the job. |

`next` with `capacity` N gives N offers or fewer. podrunner moves a unit
from `started` to `running` only when the `started` event names the
runner name of the unit (`job-mismatch` else).

## Errors

`bad-request`, `unknown-job` (never offered, withdrawn or released),
`already-claimed`, `not-claimed`, `source-revoked`,
`credential-unavailable`, `credential-already-issued` (the remote mint
gives one credential for each job id, T-031), `unavailable`.

## Tests

`crates/podrunner-source/src/conformance.rs` runs these tests against a
source. Each test takes a fresh source, whose script holds two offers or
more for its first `next`. Each test has a planted defect that fails it
and a correct input that passes it.

| Number | Test | What passes |
| --- | --- | --- |
| 1 | `hello-names-the-source` | `hello` gives a source name. |
| 2 | `no-capacity-gives-no-offer` | `next` with capacity 0 gives no offer. |
| 3 | `next-gives-at-most-capacity-offers` | `next` with capacity 1 gives one offer. |
| 4 | `ack-of-an-offer` | An offered job is acked; a job that was not offered is `unknown-job`. |
| 5 | `a-job-is-claimed-once` | A second claim is `already-claimed`. |
| 6 | `mint-needs-a-claim` | `mint` before `claim` is `not-claimed`; after it, the credential has files with plain names. |
| 7 | `report-of-a-claimed-job` | A report of a claimed job is accepted. |
| 8 | `audit-is-accepted` | An audit line is accepted. |
| 9 | `withdraw-ends-an-offer` | After `withdraw`, a claim is `unknown-job`. |
| 10 | `release-ends-a-claim` | After `release`, a claim is `unknown-job`. |
| 11 | `an-unknown-op-is-bad-request` | An unknown `op` is `bad-request`. |
| 12 | `the-wire-refuses-a-long-line-a-path-and-a-schema` | A line over 1 MiB, a `job_id` of `../x`, schema 2 and no schema are each `bad-request`. |
| 13 | `assigned-needs-a-claim` | `assigned` of an offer that is not claimed is refused; of a claimed job it gives `keep` or `end` (or `bad-request` from a source with no `assigned`). |
| 14 | `hello-takes-the-facts-of-the-host` | `hello` with `host` names the same source as `hello` with none. |
| 15 | `a-cancelled-job-is-reported-and-released` | After `mint`, a report with `exit_reason: cancelled` and `release` are taken, and a claim after them is `unknown-job` (T-502). |

The numbers of the tests follow this page. The design contract numbers its
tests in another order; the next table maps them (T-053).

## The tests of the design contract

`docs/design/contracts/source-driver.md` numbers its suite in another
order. This table gives each design test, the tests here that cover it,
and what is open (T-053). `SC` is `crates/podrunner-source/src/conformance.rs`,
`RT` the tests of `crates/podrunner-remote/src/`, `SG` the tests of
`crates/podrunner-slots/src/tests/`, `MI` `crates/podrunner-mint/src/mint.rs`.

| Design | Covered by | Open |
| --- | --- | --- |
| 1 unknown trust | the tier rule (`crates/podrunner-tier`), SG `an_offer_with_a_tier_is_refused` | none |
| 2 one claim | SC `a-job-is-claimed-once`, SG `a_redelivered_offer_is_one_job` | none |
| 3 credential canary | SG `no_credential_reaches_the_journal_the_audit_or_a_report`, the sign tests; the host role as a process: `crates/podrunner/tests/serve.rs` `no_credential_reaches_stderr_or_a_file_of_the_state` (stderr, each file of the state, `status`; a positive control); the mint role as a process: `crates/podrunner/tests/mint.rs` (stderr and each file of the mint state; a positive control); the sidecar: `crates/podrunner/tests/scaleset.rs` (its stderr, the journal, the audit and the reports; the fake service plants a canary that the unit must see) | none |
| 4 revoked | MI `a_revocation_is_final_and_names_no_secret`, `tools/scaleset-sidecar` `TestARevocationEndsTheSidecarWith78` | none |
| 5 `../x` | SC `the-wire-refuses-a-long-line-a-path-and-a-schema` | none |
| 6 untrusted host token | RT (TLS) `a_pinned_host_reaches_the_gate_and_another_key_does_not`: a mint serves one source of one tier and pins its hosts, so the key of a host of another tier is not pinned there; SG `one_key_for_two_tiers_is_refused`: a host has one key for each tier | none |
| 7 not received, issued twice | RT `each_host_holds_its_own_offers_up_to_its_slots` | none |
| 8 tier-mismatch | RT `the_mint_computes_the_tier_and_withdraws_a_fork_on_the_trusted_set`: the mint withdraws the offer, so a later claim is `unknown-job` (call C18) | none |
| 9 capacity | RT `each_host_holds_its_own_offers_up_to_its_slots` | none |
| 10 started mismatch | SG `a_foreign_runner_name_ends_the_unit`, `a_started_with_another_job_id_ends_the_unit` (the report is `job-mismatch`) | none |
| 11 payload | SG `a_stale_image_and_a_digest_off_the_pin_list_are_refused`, `a_payload_of_a_scaleset_is_refused_and_an_ignored_one_is_audited` | none |
| 12 restart of mint | RT `each_host_holds_its_own_offers_up_to_its_slots`: after a restart the ledger refuses the second mint, and the gate answers `unknown-job` (call C18) | none |
