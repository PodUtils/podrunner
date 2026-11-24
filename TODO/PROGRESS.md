# Progress

`TODO/INDEX.md` holds 641 entries: 79 open, 60 partial, 14 blocked, 488 done.

## State

- The plan is GitHub #1; one entry for each issue. The owner answered DR-1
  to DR-7 and GitHub #8 (`docs/decisions.md`). DR-7 is yes: the live gates
  run on GitHub #74 and #75.
- Pins: podcrates `v0.82.1`, podvm `v0.2.0-beta.39`, podnet `v0.4.0-beta.17`,
  runner v2.338.2. Last release: `v0.1.0-beta.7`. Candidate:
  `v0.1.0-beta.8`. Its release commit is made and not pushed. Its notes
  (`docs/releases/v0.1.0-beta.8.md`) cite the runs on `dd88c33`: the gate
  45413638800 (on `c5c6b12`), `level2.yml` 45413571282, `runner-image.yml`
  45413234684 and `user-gate.yml` 45413256290.
- githome and cfb come first (the owner, 2026-11-18, `docs/decisions.md`,
  "githome and cfb first"): podrunner accommodates each need and ask of
  theirs. The coordinator is the session of podcrates (COORD-086, GitHub
  #293). The one channel to the other projects is COORD issues.
- Milestones: M-A, M-B and L2 hold. M-C and M-D wait for the live gates
  (T-032, T-033; GitHub #74, #75).
- The `microvm` kind and the `incus` kind (no NIC, one channel) pass on real
  guests and instances. A guest with an engine runs `services:`, a job
  container and Docker actions (T-103, T-124, T-157).
- The sidecar of the scale set, with the opt-ins of Q12 (T-518) and Q13
  (T-504), has unit and fake tests only. It did not run against GitHub.
  Open: T-621 (a live lane), T-622, T-624.
- The state of each issue comes from the record by a tool; the step of the
  hand-written State column is open (T-250, partial).
- Keep this page short. Write the current state only; git keeps the
  history.

## Work order

Finish each entry, close it in place, change this page in the same commit,
then take the next one.

1. Release `v0.1.0-beta.8` (`docs/HANDOFF.md`, "The release of
   `v0.1.0-beta.8`"). Next: push `main`; on the final commit dispatch
   `runner-image.yml` and `user-gate.yml` (`version=0.1.0-beta.8`,
   `images_run=45390017489`); then the dry run of `release.yml`; then
   `release.yml` with `version=0.1.0-beta.8`. Give each `release.yml` run
   the same `images_run`. The gates are T-243, T-244 and
   T-246 (partial) and the checklist of `docs/maintenance.md`, "A release".
2. T-512 (P0): after the release, write `githome: complete in vTAG` on #292
   and #151 only when each ask G-01 to G-41 is in a release. It is not:
   `docs/HANDOFF.md`, "The work order", gives the counts that are left.
   Until then, name the tag of each release on #292 and #151.
3. The open asks of githome and cfb: T-480 (cache directory by trust),
   T-487 (warm units), T-516 (CI proof of `docker build` and
   Docker-in-Docker), T-500 (runner image for cfb; partial). Then T-517,
   T-521 and the partial entries T-497, T-499, T-509, T-511, T-515, T-520,
   T-523.
4. The local P3 entries: T-598, T-597, T-616, T-603 (step 1), T-610,
   T-609. The follow-ups of QA round 6: T-638, T-643, T-644, T-645. The P3
   groups of the QA rounds: T-308, T-316, T-323, T-331, T-339, T-633 to
   T-637, T-642.
5. The live reports (GitHub #74, #75): T-192 (P1, partial), T-181, T-186,
   T-175 (partial; its step on the test organization is next).
6. The local mode: T-205 (partial: a `runner-image.yml` run and a dry run of
   `release.yml` prove the assets), T-204, T-229, T-242 (partial), T-161
   (partial; next, the measure of a third-party runner).
7. The hosted snapshot and the signed release: T-539, T-542, T-555, T-540,
   T-553 (ghcr.io, COORD-142), T-557, T-556, T-562 (after T-484), T-484,
   T-579, T-547, T-546, T-550. T-613 (partial) waits for podvm.
8. The incus kind: T-482, T-483 (COORD-187), T-481. T-261 (partial): the
   syscall tier stays off until podvm fixes it (#308, #304, #189). T-403
   (partial) waits for COORD-090 (#300).

Waits:

- podvm `0.2.0-beta.40` (not tagged): T-611 (COORD-128, #350), T-613
  (COORD-127, #349; COORD-125, #347 ask 1).
- The live gates (GitHub #74, #75): T-032, T-033, T-040.
- Other projects: T-543 and T-544 (COORD-103, #313; COORD-106, #316),
  T-558 to T-560 (COORD-106, COORD-107, #317), T-496 (COORD-089, #296 ask
  5), T-545 (a COORD issue to podvm, not written), T-082 (a runner release
  with WebUtilities 8.0.34), T-043 (crates of podcrates), T-199 (a report
  of a user on GitHub #153).
- Partial, closed by their gates: T-001 (closes with GitHub #1), T-009 (the
  live rows wait for T-040).

## Questions for the owner

None is open. `docs/decisions.md` has each ruling of the owner, Q1 to Q13.
