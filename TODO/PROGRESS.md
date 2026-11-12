# Progress

`TODO/INDEX.md` holds 98 entries: 0 open, 6 partial, 12 blocked, 80 done.

## State

- The plan is GitHub #1; one entry for each issue. The owner answered
  DR-1 to DR-7 and GitHub #8 (`docs/decisions.md`); DR-7 is "not now".
- Pins: podcrates `v0.32.0`, podvm `v0.2.0-beta.6`, podnet
  `v0.4.0-beta.6`, runner v2.338.2.
  Last beta: `v0.1.0-beta.1` (release run 42942338772).
- M-A and M-B hold. M-C: T-027 to T-029 done; T-030 waits for T-051
  (podvm aarch64); the gate T-032 waits for DR-7. M-D: T-031 and T-045
  done; the gate T-033 waits for DR-7. L2: T-034, T-035, T-038 done; the
  gate T-039 waits for a real guest (T-036, T-037).
- podcrates is on direct messages; podnet answers COORD issues; podvm and
  podssh are on break. An entry that needs a change of podvm or podssh is
  `blocked`, and the session takes the next entry.
- Keep this section short. Write the current state only; git keeps the
  history.

## Work order

Finish each entry, close it in place, change this page in the same commit,
then take the next one. No entry of the queue waits for a person, a
release or a live account.

The queue, most valuable first:

1. T-095 (M, partial): `podrunner run --podvm --podnet`, the egress of each unit through the egress module; `ok denied` held and `failure denied` unheld in run 43242367198; the local suite now runs held too; a green run of it closes T-095 and T-046 with the seccomp hold.
2. T-096 (S, partial): an absolute `-C` fails in a podvm unit (run 43242367198), sent to podcrates by GitHub #70 (COORD-012); read the `-C` with no `-z` in the next run.
3. T-051 (S, partial): podvm `v0.2.0-beta.6` is pinned; a green aarch64 job of `runner-image.yml` closes it and GitHub #52.

Partial, closed by their gates: T-001 (closes with GitHub #1), T-009 (the live rows wait for T-040; U15 for the owner).


Blocked: T-098 (Q4 of the owner), T-082 (a runner release with WebUtilities 8.0.34), T-007, T-032, T-033, T-040 (DR-7), T-030 (podvm aarch64, after T-051), T-036, T-037, T-039 (the podvm machine tier), T-043 (crates of podcrates), T-077 (podnet, GitHub #69).

## Questions for the owner

- Q4 (T-098, not urgent): may a Docker action run in a job with no `container:` and no `services:`? The contract of the design (`docs/design/contracts/unit-driver.md`, "Order") refuses each container operation before `prepare_job`, and the runner calls no `prepare_job` for such a job, so a Docker action there fails; on GitHub it runs. Recommendation: yes. Accept `run_container_step` before `prepare_job`, with its mounts from the container action table and its image from the registries of the unit; keep the refusal of `run_script_step` and `cleanup_job` before it. The contract itself names the mount table and the refusal of host paths, devices and privileged flags as the real controls, and those still apply. Until a ruling, the local mode runs a Docker step only in a job with `container:` or `services:`.

The owner answered Q1 to Q3 (DR-4, DR-5, DR-6) on 2026-11-11 (`docs/decisions.md`).
