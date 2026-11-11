# Appendix: the asks of #35 and #36, and acceptance detail

`SPEC.md` §12 and §13 summarise this file. "Met" means the acceptance as written can pass with this design. "Restated" means the acceptance changes and the ask's owner must agree (DR-6). "Open" means a dependency outside this spec has not landed, or the mechanism is specified and not built. "Deferred" means cut from v1 with a trigger (`SPEC.md` §15). Asks not listed are out of scope for the runner path: A-X-02, A-X-03, A-X-12, A-X-16, A-X-24, A-V-05 and A-S-02. A-N-13, A-V-03 and A-S-11 merge into A-X-05.

| Ask | Status | Where | Note |
| --- | --- | --- | --- |
| G-01 job-source contract | Met in design | §4.2, §6, §11.7 | The `[[source]]` table gives each source its slots. The contract is written before any code (R6 B6) |
| G-02 unit kind, no fallback | **Restated** | §6.3, §6.5 | The ask says the source states the unit kind that each label needs. This spec computes it from `trust_inputs` and the tier, never from the label. No VM on the host means the tier is not polled |
| G-03 credential unreadable | **Restated** | §8.4 | The official runner cannot meet "not from the file" |
| G-04 policy per job | Met for v1 on `enforced` holds only. Open for audit forwarding and placeholders | §9, `contracts/source-driver.md` | `audit` is specified and unbuilt (WP-03). Placeholders need T-100 (§15) |
| G-05 placement by architecture | Met | §4.2, §6.5 | The driver refuses a foreign platform unless emulation is named (R6 B18) |
| G-06 report | Met, fields depend on drivers | §11.4 | CPU, RSS and bytes need T-1695 for podvm |
| G-07 caches by trust level | Met for v1 (no shared cache). Deferred for the shared form | §11.8, §15 | The userland rung does not enforce `:ro` (Documented, R6 A20) |
| G-08 capacity and drain | Met | §6.2, §11.5 | `capacity` in `next`, `wait_deadline_s`, `withdraw` |
| G-09 services, container jobs, Docker actions | **Restated** | §7 | The ask names a container engine inside a unit and no host engine socket. This spec: hooks for the official runner, `services:` refused on `shared-host`, the Docker API deferred. `microvm` has `container: none` until a guest runner image exists, so the ask's `microvm` acceptance is open. Hooks meet it only if G-H1 holds (U6) |
| G-10 fake forge source | Met | §13 | `fake` source, in the `podrunner` repository (T-106) |
| G-11 warm units by digest | Deferred | §15 | A warm unit would be a prepared root: the runner cannot start before its credential is staged |
| G-12 limits and 60 s end | Open | §10.4 | Needs a verified tree kill |
| A-X-01 `podrunner` | Met in part | §4 to §11 | The Incus backend returns as a driver, not in v1 (§15) |
| A-X-05 one egress proxy | Open | §9 | Three hosts are named in the records (R6 B13). DR-11 |
| A-X-13 signed releases | Open | | `pod-release` (T-104) |
| A-X-14 acceptance registry | Open | §3.3, §13 | T-105. The "Proves" column of the work packages is a small form of it |
| A-X-15 control API | Met by a module that T-103 replaces | §11.5 | |
| A-X-17 conventions | Met | §11.5 | `pod-core`'s table and one JSON log schema (T-101) |
| A-X-18 test doubles | Met in the project | §13 | Fakes live in `podrunner` |
| A-X-20 MCP servers | Open | | Not designed. `podssh mcp` exists (Documented, R2:3) |
| A-X-21 fuzz, deny, SBOM | Open | §3.3 | Each new crate needs a fuzz target (R3 section 7). The control socket server and the hook parser get one |
| A-X-22 host matrix as proof set | Met as the host table, CI matrix open | §10.2 | |
| A-X-25 host qualification | Open | §11.5 | Three owners named in R6 section 1.2. DR-15 |
| A-C-07 revoked credential | Met | §8.1 | One exit code, the name, never the value, no restart loop |
| A-V-01 confine layer | Open | | T-108. Not named in #47 (R6 B14). Owner: podcrates |
| A-V-02 egress hold, refuse unheld | Met as a matching rule | §9.3 | |
| A-V-04 Node job on a no-root host | Partly measured | §10.5 | |
| A-V-06 `-v` and `:ro` | Open | | T-1653 partial |
| A-V-07 limits, hard end | Open, and unsafe for .NET as written | §10.4 | |
| A-V-08 microVM from an OCI image | Open | §10.1 | |
| A-V-09 report and labels | Open | | T-1695 |
| A-V-10 probe schema | Open | §10.1 | The podvm driver reads `probe --json`. Publication is podvm's and podcrates' |
| A-V-11 `--secret` | Open | | T-1696, with the three-file reality of §8.2 |

## Level 1 guards, each with a planted defect

The method is that of the archive's `AGENTS.md`: plant the defect, run, read the exit code unpiped, then prove correct input passes.

- An `unknown` trust input never gives `trusted`: the job is refused with `event-not-allowed` unless it is a fork `pull_request`.
- A fork offer on the `trusted` set is `tier-mismatch` and not acquired.
- A `pull_request_target`, `release` or `other` offer, a same-repository pull request by an external actor and a member push to a public repository that `trusted_public` does not list are `event-not-allowed`.
- `untrusted` never gets a boundary below `guest`, and there is no weaker fallback.
- A stale image is refused. An offer naming a digest outside the pin list is `payload-not-pinned`, except from a `payload_from_offer` source, and never for a `host` unit.
- An unenforceable required limit is refused. An offer cannot lower a tier minimum.
- A `fake` driver claiming `guest` fails the self-test.
- Two co-tenant jobs on a `shared` principal do not run at once.
- A `container_start` posted straight to the control socket with a host mount, a privileged flag, `ports` on a shared-host unit or an image of a registry outside `registries` is refused, and a valid runner request passes. A second `prepare_job` and a container operation before the first are refused.
- A step kills the unit's shim: `destroy` does not report 0 residual processes, and the host is `degraded`. A grandchild forked inside a container that the socket started is killed by `destroy`.
- A `started` event with a foreign `runner_name` destroys the unit (`job-mismatch`).
- A `mint` for a job never offered to the host token is refused. A second `mint` for one job is refused.
- Killing `slots` leaves no residue (`SPEC.md` §11.6).
- A planted credential canary appears in no output of any module.
- `job_id` `../x` is refused.

## Level 2 detail

The archive's seven-workflow conformance contract (`ghactions-local`, `conformance.sh`, `check.sh`, R5:5.1) runs against `runner.server` and its patched listener inside a unit. The archive's script runs `docker rm -f $(docker ps -aq)` between cases (`conformance.sh:64`), which removes every container on the host: inside a unit the script must be changed or the unit isolated from the host's engine. The archive's last recorded state: w1 to w5 pass, w6 (403 on action fetch) and w7 (artifacts, 403 and ECONNRESET) fail, and the cause of w7 is not explained, because `ArtifactControllerV2.cs` exists in the vendored tree (R4:4.1, R5 F1). The `runner.server` flow is V1: the official runner's `session`, `acquirejob` and JIT path are not exercised (R4:4.1).

The canned-message fake broker (WP-12) serves `session`, `message`, `acquirejob`, `renewjob`, `completejob` and the results routes from recorded or hand-written job messages. M1's fake server shows the first half (`APPENDIX-MEASUREMENTS.md`). The corpus also includes the `ubuntu-24.04` jobs of the 13 podcrates workflows (Source, R3:5.3). podvm's `nightly.yml` needs `sudo apt-get install qemu-user-static` and cannot run on H0.

## Asks that are not on this path

The cfb tunnel set (A-N-01 to A-N-03, A-C-01, A-C-02) is not on this path (Documented, `inputs/i47.md`; R6 lists A-N-01 as not on the runner path). `podrelay-worker` is named by A-X-04, A-X-23 and A-X-26 of #35 and not by #47. Inferred: nothing in `podrunner` depends on either. The disposition of every other name that #47 mentions is in `APPENDIX-DISPOSITION.md`.
