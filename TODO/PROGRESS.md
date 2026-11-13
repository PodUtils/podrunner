# Progress

`TODO/INDEX.md` holds 111 entries: 2 open, 7 partial, 8 blocked, 94 done.

## State

- The plan is GitHub #1; one entry for each issue. The owner answered
  DR-1 to DR-7 and GitHub #8 (`docs/decisions.md`); DR-7 is yes
  (2026-11-13): the live gates run on GitHub #74 and #75.
- Pins: podcrates `v0.39.0`, podvm `v0.2.0-beta.11`, podnet
  `v0.4.0-beta.8`, runner v2.338.2. Last beta: `v0.1.0-beta.3` (release
  run 43461852923, `e8795f2`). Last green `runner-image.yml`: 43454398354 (`e8795f2`).
- M-A and M-B hold. M-C: T-027 to T-030 done; the gate T-032 waits for
  the live gates (GitHub #74, #75). M-D: T-031 and T-045 done; the gate
  T-033 waits for the live gates. L2: T-034, T-035, T-038 done; the gate
  T-039 waits for T-036 and T-037, which pass on real guests.
- Each project gets COORD issues only (no direct message, the owner,
  2026-11-12); podnet and podvm answer; podssh is on break. An entry that
  needs a change of another project is `blocked`, and the session takes
  the next entry.
- Keep this section short. Write the current state only; git keeps the
  history.

## Work order

Finish each entry, close it in place, change this page in the same commit,
then take the next one. No entry of the queue waits for a person, a
release or a live account.

The queue, most valuable first:

1. T-037 (M, partial): the microvm kind passes on real guests with no NIC (run 43454398354). Next: the acceptance of GitHub #37 on the fake source.
2. T-036 (S, partial): the check of the mounts on each `guest` start. With T-037 it closes T-039 (L2).
3. T-105 (M, partial): the incus suite passes on real instances (run 43454398354); GitHub #85 is closed. It closes with T-109.
4. T-110 (S, open): the microvm and incus kinds check the version of podvm in `probe`.
5. T-103 (L, partial): steps 1 and 2 are done. Next: the runner image with an engine in a guest, and `engine` of `ContainerRoute` (a COORD issue to podcrates).
6. T-111 (S, open): refuse the keys of `[egress]` and of `[unit.KIND]` that nothing reads.
7. T-101 (M, partial): the `program` source kind is in; its pair with GitHub runs on the host of LIVE-2 (GitHub #75) when githome gives its program.

Partial, closed by their gates: T-001 (closes with GitHub #1), T-009 (the live rows wait for T-040; U15 for the owner).

Blocked: T-109 (podvm: no NIC and one channel on the incus tier, GitHub #88), T-032, T-033, T-040 (the live gates: GitHub #74 and #75), T-039 (after T-036 and T-037), T-108 (the tag `v0.40.0` of podcrates, GitHub #86), T-082 (a runner release with WebUtilities 8.0.34), T-043 (crates of podcrates).

## Questions for the owner

None open.

The owner answered Q1 to Q3 (DR-4, DR-5, DR-6) on 2026-11-11, and Q4, Q5 and DR-7 on 2026-11-13 (`docs/decisions.md`).
