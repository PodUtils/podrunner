# Progress

`TODO/INDEX.md` holds 107 entries: 0 open, 5 partial, 8 blocked, 94 done.

## State

- The plan is GitHub #1; one entry for each issue. The owner answered
  DR-1 to DR-7 and GitHub #8 (`docs/decisions.md`); DR-7 is yes (2026-11-13): the live gates run on GitHub #74 and #75.
- Pins: podcrates `v0.39.0`, podvm `v0.2.0-beta.10`, podnet
  `v0.4.0-beta.8`, runner v2.338.2.
  Last beta: `v0.1.0-beta.2` (release run 43359637417, 2026-11-13).
- M-A and M-B hold. M-C: T-027 to T-030 done; the gate T-032 waits for the live gates (GitHub #74, #75). M-D: T-031 and T-045
  done; the gate T-033 waits for the live gates. L2: T-034, T-035, T-038 done; the
  gate T-039 waits for a real guest (T-036, T-037).
- Each project gets COORD issues only (no direct message, the owner,
  2026-11-12); podnet and podvm answer; podssh is on break. An entry that needs a
  change of another project is `blocked`, and the session takes the next
  entry.
- Keep this section short. Write the current state only; git keeps the
  history.

## Work order

Finish each entry, close it in place, change this page in the same commit,
then take the next one. No entry of the queue waits for a person, a
release or a live account.

The queue, most valuable first:

1. T-037 (M, partial): the microvm kind on podvm `v0.2.0-beta.10` (no NIC, one channel to the egress socket). Next: the proof in a runner-image run, the self-test of T-036 on a real guest, then the acceptance of GitHub #37.
2. T-105 (M, partial): the `incus` kind (`crates/podrunner-incus`) on podvm `v0.2.0-beta.10`. Next: read its conformance run in `runner-image.yml`, then close GitHub #85.
3. T-101 (M, partial): the `program` source kind is in; its pair with GitHub runs on the host of LIVE-2 (GitHub #75) when githome gives its program.

Partial, closed by their gates: T-001 (closes with GitHub #1), T-009 (the live rows wait for T-040; U15 for the owner).


Blocked: T-082 (a runner release with WebUtilities 8.0.34), T-032, T-033, T-040 (the live gates: GitHub #74 and #75), T-103 (the engine in a guest, after T-037), T-036, T-039 (after T-037), T-043 (crates of podcrates).

## Questions for the owner

None open.

The owner answered Q1 to Q3 (DR-4, DR-5, DR-6) on 2026-11-11, and Q4, Q5 and DR-7 on 2026-11-13 (`docs/decisions.md`).
