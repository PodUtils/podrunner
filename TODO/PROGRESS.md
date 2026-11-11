# Progress

`TODO/INDEX.md` holds 60 entries: 7 open, 2 partial, 19 blocked, 32 done.

## State

- The rulings of 2026-11-11 ("podrunner online", "The sessions of
  2026-11-11") give podrunner a lead session; DR-1 and DR-2 are yes
  (T-002, T-003). The plan is GitHub #1; one entry for each issue.
- Last beta: `v0.1.0-beta.1` at `6df0826` (release run 42942338772).
- The owner answered DR-4 to DR-7, GitHub #8, and podcrates Q10 and Q11
  on 2026-11-11 (`docs/decisions.md`). `trusted` can run at `redirect`
  as a choice of the operator (T-045).
- M-A is done (T-015 to T-025; the H0 run of T-023 on a profile with no
  root, no user namespace, no mounts and no KVM). T-047 waits for podvm.
  M-C: done T-027 to T-029; T-030 waits for T-051 (podvm). M-D: done T-031, T-045; T-033 waits for DR-7 and M-B.
  L2: on the fake path, the official runner runs canned jobs, the corpus
  and a `container:` job through the hooks (T-035, T-038); the podvm unit
  waits for T-026;
  T-036 blocked (the rest needs a real guest, T-037).
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
3. **M-D, from podnet (PodUtils/podnet#439, PodUtils/podnet#440):** T-055, T-057 (defects), then T-058, T-059.
4. **M0, L2:** T-014 (the spec is in: the owner attached the zip); T-053 (the design numbers of the conformance tests).
5. **Blocked:** T-007 (DR-7: not now), T-030 (aarch64: T-051), T-026 (podvm),
   T-034 (U19: podvm), T-035 (the podvm driver: T-034, T-026), T-036 (a real guest), T-038 (the run in a podvm unit: T-026), T-047, T-050, T-051, T-052 (podvm),
   T-032, T-033, T-037 (podvm), T-040
   to T-043, T-046.

## Questions for the owner

None is open. The owner answered Q1 to Q3 (DR-4, DR-5, DR-6) on
2026-11-11 (`docs/decisions.md`).
