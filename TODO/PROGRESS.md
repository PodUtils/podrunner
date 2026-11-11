# Progress

`TODO/INDEX.md` holds 44 entries: 6 open, 6 partial, 14 blocked, 18 done.

## State

- The rulings of 2026-11-11 ("podrunner online", "The sessions of
  2026-11-11") give podrunner a lead session; DR-1 and DR-2 are yes
  (T-002, T-003). The plan is GitHub #1; one entry for each issue.
- Last beta: `v0.1.0-beta.1` at `6df0826` (release run 42942338772).
- M-A: done T-015, T-016, T-018 to T-022, T-024; partial T-017
  (PodUtils/podcrates@T-163 gives the authoritative schemas) and T-023
  (an H0 run is open). M-C: done T-027, T-028, T-029. L2: partial T-036.
- podcrates is on direct messages; podnet answers COORD issues; podvm and
  podssh are on break. An entry that needs a change of podvm or podssh is
  `blocked`, and the session takes the next entry.
- Keep this section short. Write the current state only; git keeps the
  history.

## Work order

Finish each entry, close it in place, change this page in the same commit,
then take the next one. Continue a `partial` entry first.

1. **The plan:** T-001 and T-008 close when their issues close.
2. **M-A** (GitHub #25): T-017 (partial: the authoritative schemas come
   from podcrates, PodUtils/podcrates@T-163), T-023 (podvm driver, on the released podvm
   binary), then T-025.
3. **The registers:** T-009 (partial: the open rows wait for their entries).
4. **M-C, L2:** T-036 (partial), T-035, T-030; T-034, T-038.
5. **Blocked:** T-004, T-005, T-006 (questions Q1 to Q3), T-007, T-014
   (the spec), T-026 (podvm), T-031 to T-033, T-037, T-040 to T-043.

## Questions for the owner

- **Q1 (DR-4, GitHub #4, T-004).** Allow `trusted` jobs at boundary
  `redirect` on no-root hosts? Recommendation of the issue: yes, for
  member jobs of configured organizations only, with the residuals of
  T-019 written down. Until the answer, `trusted` needs `confined` or
  more.
- **Q2 (DR-5, GitHub #5, T-005).** The microVM unit for `untrusted` jobs:
  an own driver now, or podvm's microVM wiring
  (PodUtils/podvm@T-1656)? Recommendation of the issue: as GitHub #5
  states; T-037 waits.
- **Q3 (DR-6, GitHub #6, T-006).** Do githome and cfb accept the restated
  asks G-02, G-03 and G-09? Recommendation of the issue: yes.
