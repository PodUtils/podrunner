# Progress

`TODO/INDEX.md` holds 64 entries: 0 open, 2 partial, 16 blocked, 46 done.

## State

- The plan is GitHub #1; one entry for each issue. The owner answered
  DR-1 to DR-7 and GitHub #8 (`docs/decisions.md`); DR-7 is "not now".
- Pins: podcrates `v0.24.0`, podvm `v0.2.0-beta.3`, runner v2.338.2.
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
then take the next one. Continue a `partial` entry first.

1. **The plan:** T-001 closes when its issue closes.
2. **The registers:** T-009 (partial: the open rows wait for their entries).
3. **Blocked:** T-064 (the next podvm beta: the tree of each container; do it before that beta is pinned), T-039 (milestone L2: T-036, T-037), T-007 (DR-7: not now), T-030 (aarch64: T-051), T-036 (a real guest), T-051 (podvm: the aarch64 interposer), T-061 (podvm: absolute links), T-062 (podvm: :ro),
   T-032, T-033, T-037 (podvm), T-040
   to T-043, T-046.

## Questions for the owner

None is open. The owner answered Q1 to Q3 (DR-4, DR-5, DR-6) on
2026-11-11 (`docs/decisions.md`).
