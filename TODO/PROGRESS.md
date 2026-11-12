# Progress

`TODO/INDEX.md` holds 80 entries: 0 open, 2 partial, 12 blocked, 66 done.

## State

- The plan is GitHub #1; one entry for each issue. The owner answered
  DR-1 to DR-7 and GitHub #8 (`docs/decisions.md`); DR-7 is "not now".
- Pins: podcrates `v0.28.0`, podvm `v0.2.0-beta.5`, podnet
  `v0.4.0-beta.3`, runner v2.338.2.
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

The queue for tonight, most valuable first:

(empty: each entry that is left waits for another project or for the owner)

Partial, closed by their gates: T-001 (closes with GitHub #1), T-009 (the live rows wait for T-040; U15 for the owner).

Blocked: T-077 (podnet, GitHub #64), T-007, T-032, T-033, T-040 (DR-7), T-030 and T-051 (podvm aarch64), T-036, T-037, T-039 (the podvm machine tier), T-043, T-046 (crates of podcrates).

## Questions for the owner

None is open. The owner answered Q1 to Q3 (DR-4, DR-5, DR-6) on
2026-11-11 (`docs/decisions.md`).
