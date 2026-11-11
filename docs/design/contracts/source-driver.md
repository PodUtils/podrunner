# Source driver contract, version 1

A source driver finds jobs on a forge, resolves the facts that decide trust, and mints the credential that lets one runner take one job. `mint` is the supervisor of the driver and the only module that holds the credentials of the source. `slots` is the only caller of `mint`. The topology is in SPEC §5.2.

Transports. The same operations and messages are carried three ways.

1. stdio JSON lines between `mint` and a driver it spawns (the Go scale-set sidecar).
2. JSON lines between `slots` and `mint` (in process in v1, a unix socket where they are separate). `mint` forwards all nine operations (`hello`, `next`, `ack`, `claim`, `mint`, `report`, `audit`, `withdraw`, `release`) to the driver and holds the credentials the driver is given through its environment at spawn.
3. HTTPS, for the remote mint (SPEC §5.3): one `POST` for each operation to `/v1/OP`, the object as the body, the answer as the response body, over mutual TLS. The rules of the remote mint:
   - A host token names one host and one source instance (`kind/instance`), not a tier. It is bound to a host key that the host proves on each request. The host key has a rotation interval set in the configuration of `mint`, and a token's life is shorter than the key's. A token is refreshed with the key, so whoever holds the key holds tokens until `mint` revokes the key (SPEC §5.3).
   - `mint` keeps a ledger of the offers it delivered to that token. `next` returns only offers that `mint` assigned to that host. `claim`, `mint`, `ack`, `release`, `withdraw` and `report` are accepted only for a `job_id` in the ledger, and `ack` only for a `message_id` that this token was sent.
   - `mint` reads the run object and computes the tier itself with the rule of SPEC §6.3. The unit host never states a tier. A `claim` whose computed tier is less trusted than the instance is refused with `tier-mismatch`.
   - The ceiling of outstanding offers and concurrent claims for a host is its slot count in the configuration of `mint`. The `capacity` of a `next` request is clamped to it and never raises it. `mint` also bounds claims per token per hour and per repository. `mint` issues the credential for one `job_id` once. A second `mint` for the same job is refused with `credential-already-issued`, an `error` value of this wire and not a refusal code of the job vocabulary.
   - The ledger, including the flag that a credential was issued, is written to disk before the answer of `mint` is sent, so a restart of `mint` does not reopen a second issue. `mint` writes an audit line for every call with the token id, the peer address and the job id. A credential with no start reported within `start_s` plus a margin raises an alert and, past a threshold, revokes the token. A job that `slots` released in state `minted` is not counted as unreconciled.
   - The listener has a 1 MiB request limit and a rate limit for each token.

Until U22 is settled, a credential that `mint` issued for one job may run another job of the same set, so the ledger bounds the credentials a host receives and not the jobs it runs.

A forge that implements the contract itself (G-01, githome) would use the same transport. It is deferred (SPEC §15).

Conventions are those of `unit-driver.md`: JSON lines, `"schema": 1` first, `"op"` and `"id"`, the error object, `unknown` is absent, `job_id` is data of the form `[A-Za-z0-9._-]{1,128}` and never a path component.

## Operations

### hello

```
{"schema":1,"op":"hello","id":"1"}
{"schema":1,"id":"1","ok":true,"kind":"github-scaleset","instance":"org-a-trusted-x86_64","tier":"trusted","arch":"x86_64","outbound_only":true,"acquires_on_next":false,"control_hosts":["github.com","api.github.com","*.actions.githubusercontent.com"]}
```

One instance serves one tier and one architecture (SPEC §6.2). `control_hosts` are the hosts the runner needs to reach its own control plane. `slots` appends them to every `allow` policy of the instance (SPEC §9.6). `source` in an offer is `kind/instance`. A source that cannot avoid acquiring a job when it polls says `"acquires_on_next":true`, and `slots` then refuses to poll that source on a host with no slot of the required kind.

### next

```
{"schema":1,"op":"next","id":"2","wait_s":50,"capacity":2}
{"schema":1,"id":"2","ok":true,"message_id":"m-77","events":[{"type":"offer","offer":{ ...job-offer.v1... }},{"type":"cancel","job_id":"1001"},{"type":"completed","job_id":"1000"},{"type":"started","job_id":"1002","runner_name":"pr-4f2a-u-0001"}]}
```

Long poll. `capacity` is the number of free slots for this instance and goes to the forge as the set's capacity (G-08). An offer has the fields of `schemas/job-offer.v1.schema.json`: `trust_inputs` (`event`, `head_is_fork`, `actor_class`, `repo_visibility`, `owner`, `repo`, `run_id`, `ref`, `workflow_ref`, and optionally `owner_id`, `repo_id`), `labels`, `arch`, `job_id`, `source`, `limits` (`cpu_s`, `memory_mib`, `disk_mib`, `pids`, `wall_s`), `limits_required`, `policy` (`version`, `mode` of `allow` or `open`, and the `allow` rules), `wait_deadline_s`, and optionally `payload` (`image`, `image_max_age_days`, `argv`, `credential_delivery`) and `unit_need` (`min_boundary`, `container`, `ports`). `payload` is absent unless the source is marked `payload_from_offer`. Otherwise `slots` takes it from the pin list, and a `payload` in the offer is ignored and audited. The schema refuses a `payload` in an offer of a `github-scaleset` source. The answer holds no events after `wait_s`. One poll can return many messages, and the driver buffers them and returns them in order. An event is `offer`, `cancel` (the forge cancelled a job: `slots` signals the unit, and the report says `cancelled`), `completed` (`{"type":"completed","job_id":"1000"}`, the forge reports the job finished) or `started`. A `started` event is `{"type":"started","job_id":"1002","runner_name":"pr-4f2a-u-0001"}` (the `JobStarted` message: the job id and the name of the runner that took the job). The source sends it when the forge reports that a runner took the job. `slots` compares `runner_name` with the one it passed to `mint` and `job_id` with its claim, and applies the `job-mismatch` row of SPEC §11.1 on a difference. The driver never acquires the job at this step.

Deduplication is by `job_id`. A job that is offered again while it is offered, claimed, running or refused is dropped. A withdrawn `job_id` is suppressed for `wait_deadline_s` after the withdraw.

### ack

```
{"schema":1,"op":"ack","id":"3","message_id":"m-77"}
{"schema":1,"id":"3","ok":true}
```

`slots` sends it after it has recorded every event of the message. Scale sets redeliver an unacknowledged message (Documented, R4:3.1).

### claim

```
{"schema":1,"op":"claim","id":"4","job_id":"1001"}
{"schema":1,"id":"4","ok":true}
```

The source acquires the job. `slots` calls it only when a slot of the right kind is free. Calling it for an acquired job is a no-op.

### mint

```
{"schema":1,"op":"mint","id":"5","job_id":"1001","runner_name":"pr-4f2a-u-0001","work_folder":"_work"}
{"schema":1,"id":"5","ok":true,"credential":{"delivery":"files","files":{".runner":"<base64>",".credentials":"<base64>",".credentials_rsaparams":"<base64>"}}}
```

`runner_name` and `work_folder` come from `slots`, which owns the unit layout. `runner_name` holds a short random token that `slots` draws at each start, because unit ids restart at `u-0001` with each boot. `delivery` is `files` or `socket`. The source driver decodes the JIT blob: for `files` the map holds the decoded files, each as base64 bytes, and `slots` writes them with mode 0600 (SPEC §8.2). For `socket` the answer is `{"delivery":"socket","path":"..."}`. There is no `env` form (SPEC §8.2). The credential is handed to `slots` and the driver forgets it. It never appears in a log line, an audit line, a report or `status`.

### report

```
{"schema":1,"op":"report","id":"6","job_id":"1001","report":{ ...run-report.v1... }}
```

A report has the fields of `schemas/run-report.v1.schema.json`: `job_id`, `source`, `tier`, `unit_kind`, `boundary`, `boundary_proof`, `same_principal_ok`, `credential_store`, `host`, `arch`, `image_digest`, `image_age_days`, `job_images`, `claim_to_first_step_s`, `wall_s`, `cpu_s`, `peak_rss_mib`, `bytes_in`, `bytes_out`, `egress_hold`, `limits_applied`, `exit_reason`, `exit_code`, `refusal_code` and `hook_refusals`. The full report goes to a source marked `operator_controlled`. Any other source receives the public subset, validated by `schemas/run-report-public.v1.schema.json` (SPEC §11.4):

```
{"schema":1,"op":"report","id":"6b","job_id":"1001","report":{ ...run-report-public.v1... }}
```

### audit

```
{"schema":1,"op":"audit","id":"7","lines":[{"ts":"2026-11-10T09:00:01Z","job_id":"1001","tier":"trusted","event":"broker.connection","verdict":"allow"}]}
```

Batched audit lines for a source that wants them (G-04). A line carries the job id and the tier and never a path, a query or a header. The mechanism is specified and has no implementation. G-04 audit forwarding stays open until WP-03 builds it.

### withdraw

```
{"schema":1,"op":"withdraw","id":"8","job_id":"1001","reason":"wait-deadline"}
```

Tells the source that this host will not run a job it has not claimed. `reason` is `wait-deadline` or any refusal code (SPEC §11.1). A withdrawn job never starts here. A job withdrawn with a terminal code is suppressed for `terminal_suppress_s` (default 3600), so a source that redelivers cannot loop it. A source that can fail a job by name does so on a terminal reason (Unverified, U18).

### release

```
{"schema":1,"op":"release","id":"9","job_id":"1001","reason":"unit-start-failed"}
```

For a job that was claimed and will not run (a job that was never claimed is `withdraw`): `mint` failed, `prepare` or `start` failed, or `destroy` left residue. The source fails the job by name or lets the forge re-queue it. Which of the two github.com allows is Unverified. `slots` also asks the source to delete a runner registration that was minted and never used.

## Trust

A source copies facts into `trust_inputs` and never into a tier. It never decides the tier and never sends one. `unknown` is a legal value for every trust input. An offer with an `unknown` input is never `trusted`: it is `untrusted` only if it is a fork `pull_request`, and `slots` refuses it with `event-not-allowed` otherwise. For GitHub the scale-set message carries event name, repository, workflow ref and run id. The fork flag, the actor and the visibility come from the workflow run object, which needs repository "Actions: read" (SPEC §6.3). `member` means the actor is a member of the organization named by `owner`, and the membership check is the source's duty.

`slots` compares the tier it computes with the tier of the set that delivered the job. A computed tier that is less trusted than the set's is the refusal `tier-mismatch`, and a job that computes to no tier is `event-not-allowed` (SPEC §6.3). Both are refused before the claim.

## A forge that names the official runner

A forge that names the official runner as its payload must also serve the Results and cache services the runner calls for artifacts and cache. Reference implementations are `ArtifactControllerV2.cs` and `CacheControllerV2.cs` of runner.server (R5 F1). This is the forge's work, assigned to githome with G-01. It is not part of this protocol and the offer has no field for it.

## Conformance suite

1. An offer with `unknown` trust inputs is never placed as `trusted` (the test is in `slots`; the source test checks that it never sends a tier).
2. `claim` twice is one claim. `next` with a redelivered `job_id` gives one offer.
3. A credential never appears in the driver's stderr, in any log file, in an audit line or in the report. Method: a planted canary in the credential, a scan of all outputs, and a positive control (the scan finds a planted canary in a log). The scan covers every role.
4. A revoked credential ends the driver with the `Revoked` class exit code, names the credential and never prints its value, and the driver does not restart itself.
5. `job_id` `../x` is refused.
6. Over the HTTPS transport, a host token for the `untrusted` tier is refused on a `mint` for a `trusted` job.
7. A `mint` or `claim` for a `job_id` that this token never received from `next` is refused. A second `mint` for one job is refused with `credential-already-issued`.
8. A `claim` for a job whose tier `mint` computes as less trusted than the instance is refused with `tier-mismatch`, whatever the unit host says.
9. A `next` with a `capacity` above the host's configured slot count returns no more offers than that count.
10. A `started` event with a `runner_name` that `slots` did not pass to `mint`, or with a `job_id` other than the claim, destroys the unit and the report says `failed` (SPEC §11.1, `job-mismatch`).
11. An offer of a `github-scaleset` source with a `payload` is refused by the schema. A `payload` from a source that is not `payload_from_offer` is ignored and audited.
12. A restart of `mint` between an issued credential and a second `mint` for the same job: the second is refused with `credential-already-issued`.
