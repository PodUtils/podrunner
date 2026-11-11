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
| `hello` | `client`, `version` | `hello`: `source`, `operator_controlled` |
| `next` | `capacity`, `wait_s` (a long poll) | `events`: a list of events |
| `ack` | `job_id` | `done` |
| `claim` | `job_id`, `runner_name` | `done` |
| `mint` | `job_id`, `runner_name` | `credential`: `files` (`name`, `content`) |
| `report` | `job_id`, `report` | `done` |
| `audit` | `line` | `done` |
| `withdraw` | `job_id`, `reason` | `done` |
| `release` | `job_id` | `done` |

The order of one job: `offer` (an event of `next`), `ack`, `claim`,
`mint`, then `report` and `release`. `withdraw` ends an offer that
podrunner does not claim (a `wait` refusal at `wait_deadline_s`).

podrunner writes and syncs the `claimed` line of its journal before it
sends `claim` (GitHub #55). After a death between the two, the restart
sends `release` for a job that it may not hold. `release` names no
runner, so a source must not free the claim of another runner on it: the
scale-set sidecar releases only a runner that it registered itself.

A source in a child process (the scale-set sidecar) answers each request
within 15 s after the long poll of `next` (`REPLY_LIMIT`,
`crates/podrunner-mint/src/sidecar.rs`). A child that passes it is killed
and starts again after the backoff (GitHub #57): on a lock-step wire, a
late reply would be read as the answer to the next request.

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

The numbers of the tests follow this page. The spec of the design review
(T-014) can give other numbers; T-014 makes them agree.
