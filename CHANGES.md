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
| `pod-job` | podcrates `v0.52.0` to `v0.74.0`: the optional fields of githome's asks (`trust_inputs.forge`, `cache`, `sandbox` of an offer; `arch`, `assigned`, the times, the measures and `log_tail` of a report). An optional field keeps the schema id. | T-508, T-514 |
| source wire | The types of the wire are `pod_job::source` of podcrates `v0.72.0` (one form for podrunner and each source); the lines do not change. | T-508 |
| source wire | The gate runs the tests of `source-conformance` against githome's source (`pod-fake-githome` of podcrates `v0.74.0`, `images/fake-githome.toml`); a red run blocks a release. | T-509 |
| source wire | `hello` has an optional `host`: the versions of podrunner, podvm and podnet, `arch`, `kvm`, and for each kind its boundary, its container route and its largest `cpu`, `memory`, `disk` and `pids`. | T-495 |
| source wire | A new request `assigned { job_id, real }`, with the replies `keep { trust_inputs }` and `end { code, detail }`; a source with no `assigned` answers `bad-request`. | T-493 |
| source wire | `audit` carries an egress line `{ job_id, host, port, verdict, rule, bytes_in, bytes_out, at }` for each connection of a unit, at most 1000 for each job, then `{ job_id, dropped }`. | T-486 |
| source wire | `source-conformance` has 15 tests: test 13 `assigned-needs-a-claim`, test 14 `hello-takes-the-facts-of-the-host`, test 15 `a-cancelled-job-is-reported-and-released`. | T-493, T-495, T-502 |
| `run-report.v1` | The report fills `arch`, `assigned`, `image_digest`, `runner_version`, `prepare_s`, `start_s`, `run_s`, `destroy_s`, `disk_peak_bytes`, `net_rx_bytes`, `net_tx_bytes` (or `unknown` with a reason), and `log_tail` (scrubbed, 64 KiB at most) for a unit that failed before a step. | T-497, T-514 |
| `run-report.v1` | The incus kind fills `cpu_s` and `peak_rss_mib` from incusd; a job that failed after a kill of the OOM killer ends with `limit`. | T-483, T-515 |
| `run-report.v1` | The disk bound of a unit is the lesser of the `disk` of the offer and `disk_max_bytes`. | T-529 |
| `run-report.v1` | The contract names the tolerance of `cpu_s`: 20 % or 1 s of the CPU time of the payload, the larger. | T-520 |
| `run-report.v1` | A job that the job-started hook of the runner image ended before its first step (a job of another run with no `keep` of the source) ends with `job-mismatch`, not `success`, and its report has `log_tail`. | T-552 |
| control socket | The HTTP head of each socket of a unit is read by `pod_core::http1`: another version of HTTP gets 505, a head over 16 KiB gets 431. | T-536 |
| control socket | A new op `restart-source` (field `source`). | T-498 |
| control socket | New ops `drain-source`, `resume-source` and `stop-source` (field `source`); `resume-source` builds a stopped source again. | T-501 |
| `status.v1` | The type and the schema are `pod_job::Status` of podcrates `v0.73.0` (`schemas/status.v1.schema.json` of podcrates); `schemas/status.v1.json` of podrunner goes. | T-508 |
| `status.v1` | Each source has `draining` (an operator drained it). | T-501 |
| `status.v1` | `max_units` and `units` of the host. | T-505 |
| `podrunner.toml` | `max_units`: the most units of the host across its kinds. | T-505 |
| `podrunner.toml` | `schemas/config.v1.json`: the JSON Schema of the file, generated from the types of the parser; `check` stays the last word. | T-479 |
| `podrunner.toml` | `[[source]] trust` (one trust list for each source, or for each forge of a source). | T-494 |
| `podrunner.toml` | With no `[egress] default_policy`, a unit has no way out; a configuration that the group or others can write is refused. | T-410, T-409 |
| `podrunner.toml` | `[unit.podvm] cpus` where the host delegates a cgroup v2 subtree to the user. | T-221 |
| `podrunner.toml` | `[unit.incus] root_quota`, `[unit.incus] engine`, `[unit.microvm] pids`. | T-482, T-485, T-488 |
| `podrunner.toml` | `serve` runs no incus kind on a host with no incusd that takes a connection (one line names the socket). | T-534 |
| command line | `podrunner config init` (exit 0, 64, 77, 78) and `podrunner source restart --name NAME` (exit 77 for a source that does not build or refuses `hello`). | T-507, T-498 |
| command line | `doctor --config` names the runner of each pin, its age, its window and `near-end` or `stale`; the incus kind needs a reachable incusd. | T-499, T-534 |
| command line | `run --schedule` reads the `timezone` of each item of `on.schedule`. | T-423 |
| command line | `drain --source NAME`, `stop --source NAME` and `resume --source NAME`. | T-501 |
| command line | `run --incus PATH --engine DIR`: an engine in each incus instance of the local mode. | T-485 |
| command line | `source-conformance --json`: one JSON object for each line (the count and the offers needed, each test, the totals). | T-503 |
| release assets | `SHA256SUMS` of each asset with its keyless cosign bundle `SHA256SUMS.sigstore.json` (the identity of `release.yml`). | T-484 |
| release assets | `podrunner-install.sh --serve` writes `serve.json` and no file of the local mode. | T-507 |
| release assets | `podrunner-manifest.json` with its `.sha256` and an attestation: the versions, each binary and each runner image with its digest and `built_at_s`. | T-499 |
| `job-offer.v1` | An offer with `sandbox { image, command, idle_s, max_s }` runs `command` in a unit of `image` at `untrusted`; the image must be a `[[pin.payload]]` with `sandbox = true` (else `payload-not-pinned`); the payload gets `PODRUNNER_IDLE_S`; the unit ends at the lesser of `max_s` and the wall. | T-506 |
| `podrunner.toml` | `[[pin.payload]] sandbox = true`. | T-506 |
| `podrunner.toml` | `[unit.fake] claim = "confined" \| "guest"` (with `allow_fake`): a forge tests its program source through `serve` with the fake kind. | T-511 |
| pins | podvm `0.2.0-beta.34`, podnet `0.4.0-beta.17`. No `--podvm-tier=syscall` (it breaks Node of `setup-node` and a cargo build, #308). | T-405, T-179, T-261 |

<!-- contract docs/contracts/codes.md 50ade2b7f60a98cde60ea927082fa7d5de71c003ed6ab2753bb9d12fa3b4331c -->
<!-- contract docs/contracts/hook.md 893e04a8c4625f20b931f9bf6117f867bdb9cf5759b2f94c89191c94aa64bcd3 -->
<!-- contract docs/contracts/source-driver.md 5700b24d3839fe92012962770699804c9473e901c8bec3dc7d6aa260b3b899d1 -->
<!-- contract docs/contracts/unit-driver.md 07162598a8a264928373602ba954e00d94f973583ce4e6592494f758b8746401 -->
<!-- contract schemas/config.v1.json 81d02c7cd73381f795db86183a3c81c28a3935b67df3a63c2b8f03be443176d2 -->
