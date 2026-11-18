# The changes of the contracts

One row for each change of a contract that a forge, an operator or a
program reads: `job-offer.v1` and `run-report.v1` of `pod-job`, the source
wire (`docs/contracts/source-driver.md`), `status.v1`, the control socket,
the keys of `podrunner.toml`, the command line and its exit codes, and the
assets of a release. Each row names its entry. The rows of a change that is
not released go under `## next`; a release renames that section to its tag
(`scripts/check-notes.sh` refuses the notes of a release with no section of
its tag, or with rows left under `## next`). The base is `v0.1.0-beta.7`.
The end of the file holds the SHA-256 of each contract file
(`docs/contracts/*.md`, `schemas/*.json`): a change of one fails the check
until its row is here and `sh scripts/check-notes.sh --stamp` ran.

## next

| Contract | Change | Entry |
| --- | --- | --- |
| `pod-job` | podcrates `v0.52.0` to `v0.71.0`: the optional fields of githome's asks (`trust_inputs.forge`, `cache`, `sandbox` of an offer; `arch`, `assigned`, the times, the measures and `log_tail` of a report). An optional field keeps the schema id. | T-508, T-514 |
| source wire | `hello` has an optional `host`: the versions of podrunner, podvm and podnet, `arch`, `kvm`, and for each kind its boundary, its container route and its largest `cpu`, `memory`, `disk` and `pids`. | T-495 |
| source wire | A new request `assigned { job_id, real }`, with the replies `keep { trust_inputs }` and `end { code, detail }`; a source with no `assigned` answers `bad-request`. | T-493 |
| source wire | `audit` carries an egress line `{ job_id, host, port, verdict, rule, bytes_in, bytes_out, at }` for each connection of a unit, at most 1000 for each job, then `{ job_id, dropped }`. | T-486 |
| source wire | `source-conformance` has 14 tests: test 13 `assigned-needs-a-claim`, test 14 `hello-takes-the-facts-of-the-host`. | T-493, T-495 |
| `run-report.v1` | The report fills `arch`, `assigned`, `image_digest`, `runner_version`, `prepare_s`, `start_s`, `run_s`, `destroy_s`, `disk_peak_bytes`, `net_rx_bytes`, `net_tx_bytes` (or `unknown` with a reason), and `log_tail` (scrubbed, 64 KiB at most) for a unit that failed before a step. | T-497, T-514 |
| `run-report.v1` | The incus kind fills `cpu_s` and `peak_rss_mib` from incusd; a job that failed after a kill of the OOM killer ends with `limit`. | T-483, T-515 |
| `run-report.v1` | The disk bound of a unit is the lesser of the `disk` of the offer and `disk_max_bytes`. | T-529 |
| control socket | A new op `restart-source` (field `source`). | T-498 |
| control socket | New ops `drain-source` and `resume-source` (field `source`). | T-501 |
| `status.v1` | Each source has `draining` (an operator drained it). | T-501 |
| `podrunner.toml` | `[[source]] trust` (one trust list for each source, or for each forge of a source). | T-494 |
| `podrunner.toml` | With no `[egress] default_policy`, a unit has no way out; a configuration that the group or others can write is refused. | T-410, T-409 |
| `podrunner.toml` | `[unit.podvm] cpus` where the host delegates a cgroup v2 subtree to the user. | T-221 |
| `podrunner.toml` | `[unit.incus] root_quota`, `[unit.incus] engine`, `[unit.microvm] pids`. | T-482, T-485, T-488 |
| `podrunner.toml` | `serve` runs no incus kind on a host with no incusd that takes a connection (one line names the socket). | T-534 |
| command line | `podrunner config init` (exit 0, 64, 77, 78) and `podrunner source restart --name NAME` (exit 77 for a source that does not build or refuses `hello`). | T-507, T-498 |
| command line | `doctor --config` names the runner of each pin, its age, its window and `near-end` or `stale`; the incus kind needs a reachable incusd. | T-499, T-534 |
| command line | `run --schedule` reads the `timezone` of each item of `on.schedule`. | T-423 |
| command line | `drain --source NAME` and `resume --source NAME`. | T-501 |
| command line | `source-conformance --json`: one JSON object for each line (the count and the offers needed, each test, the totals). | T-503 |
| release assets | `podrunner-manifest.json` with its `.sha256` and an attestation: the versions, each binary and each runner image with its digest and `built_at_s`. | T-499 |
| `job-offer.v1` | An offer with `sandbox { image, command, idle_s, max_s }` runs `command` in a unit of `image` at `untrusted`; the image must be a `[[pin.payload]]` with `sandbox = true` (else `payload-not-pinned`); the payload gets `PODRUNNER_IDLE_S`; the unit ends at the lesser of `max_s` and the wall. | T-506 |
| `podrunner.toml` | `[[pin.payload]] sandbox = true`. | T-506 |
| `podrunner.toml` | `[unit.fake] claim = "confined" \| "guest"` (with `allow_fake`): a forge tests its program source through `serve` with the fake kind. | T-511 |
| pins | podvm `0.2.0-beta.33`, podnet `0.4.0-beta.17`. | T-405, T-179 |

<!-- contract docs/contracts/codes.md 50ade2b7f60a98cde60ea927082fa7d5de71c003ed6ab2753bb9d12fa3b4331c -->
<!-- contract docs/contracts/hook.md 893e04a8c4625f20b931f9bf6117f867bdb9cf5759b2f94c89191c94aa64bcd3 -->
<!-- contract docs/contracts/source-driver.md de94dbd537bf29edcb6450354e0b686be95757cb46c9dff287fd4417d31db972 -->
<!-- contract docs/contracts/unit-driver.md 07162598a8a264928373602ba954e00d94f973583ce4e6592494f758b8746401 -->
<!-- contract schemas/status.v1.json 30dd44ce233c91853167ecb96cd0a21a5f7aeee60314840983a75bc511684b23 -->
