# Progress

`TODO/INDEX.md` holds 44 entries: 15 open, 5 partial, 14 blocked, 10 done.

## State

- The rulings of 2026-11-11 ("podrunner online", "The sessions of
  2026-11-11") give podrunner a lead session; DR-1 and DR-2 are yes
  (T-002, T-003). The plan is GitHub #1; one entry for each issue.
- The frame is on `main` at `9d2e4dd` (gate run 42934750114, secrets run
  42934749863, both green).
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
   from podcrates, PodUtils/podcrates@T-163), T-021 (slots core), T-022
   (CLI and control socket), T-023 (podvm driver, on the released podvm
   binary), T-024 (egress, after PodUtils/podcrates@T-160 and
   PodUtils/podcrates@T-161), T-015 (beta release), then T-025.
3. **The registers:** T-009, T-010.
4. **M-C, L2:** T-027, T-028, T-029, T-030; T-034, T-035, T-036, T-038.
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
