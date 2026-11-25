# Progress

`TODO/INDEX.md` holds 660 entries: 87 open, 59 partial, 13 blocked, 501 done.

## State

- The plan is GitHub #1; one entry for each issue. The owner answered DR-1
  to DR-7 and GitHub #8 (`docs/decisions.md`). DR-7 is yes: the live gates
  run on GitHub #74 and #75.
- Pins: podcrates `v0.84.0`, podvm `v0.2.0-beta.40`, podnet `v0.4.0-beta.17`,
  runner v2.338.2. Last release: `v0.1.0-beta.8` (tag on `93e1710`;
  `release.yml` 45459310537 and 45466841655; `docs/HANDOFF.md`, "The
  release `v0.1.0-beta.8`"). The QA of the release as a user found no P1
  and no P2.
- githome and cfb come first (the owner, 2026-11-18, `docs/decisions.md`,
  "githome and cfb first"): podrunner accommodates each need and ask of
  theirs. The coordinator is the session of podcrates (COORD-086, GitHub
  #293). The one channel to the other projects is COORD issues.
- Milestones: M-A, M-B and L2 hold. M-C and M-D wait for the live gates
  (T-032, T-033; GitHub #74, #75).
- The `microvm` kind and the `incus` kind (no NIC, one channel) pass on real
  guests and instances. A guest with an engine runs `services:`, a job
  container and Docker actions (T-103, T-124, T-157).
- A `docker build` step and a `docker:dind` service pass in a guest and in
  an incus instance, proven in CI on GitHub-hosted runners (T-516,
  `runner-image.yml` 45564676018; `level2.yml` 45564676057 on github.com
  twins). The G-09 line of GitHub #66 names T-516 as open; T-517 (the
  engine as a part of the payload image) and the live check (T-040) stay.
- Warm units (T-487, partial): the seam, the fake, microvm and incus kinds,
  the pool and the report and status fields (T-657) are in the code. A
  release build refuses a `warm` key (exit 78, "not in this build"); only a
  CI build with the feature `warm-ci` takes it. The lift waits for the rows
  `serve:warm:incus` and `serve:warm:microvm` of `user-gate.yml`.
- The first step of a job on a kind with no container hook comes from the
  job-started hook of the runner image (`/v1/started`, T-660, partial until
  a run of `user-gate.yml` shows it on incus and microvm). The unit
  destroy waits for each killed process (T-661, done; live proof in the
  next `runner-image.yml`). T-662 (the split steps) is partial until a run
  shows them.
- A job of another run with a `rw` cache ends with `cache-mismatch`: the hook
  sends the ref and the repository id (T-653, done), but the rule stays off
  (`PAYLOAD_HOOK_KNOWN = false`) until T-663 (P2, open) gives the fact that
  a payload carries the hook.
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

1. The proof of the push of `72cd0ab` to `f2716a1`: `runner-image.yml` on
   the new head (cancel an older run that covers the same images); then
   `user-gate.yml` with `images_run` set to that green run: the rows
   `serve:warm:incus` and `serve:warm:microvm` must pass (T-660, T-487).
   Then the lift of T-487 (its State section has the steps). When `gate`,
   `level2.yml` and `runner-image.yml` on the new head are green, close
   #352, #353 and #356 (the record first).
2. T-663 (P2). Then the `rw` restore of a job of another run turns on
   (`PAYLOAD_HOOK_KNOWN`), and T-480 (partial; T-654) goes on.
3. The open asks of githome and cfb: T-512 (P0): write
   `githome: complete in vTAG` on #292 and #151 only when each ask G-01 to
   G-41 is in a release. It is not: githome has 41 asks, 25 in a release, 10
   partial, 6 open; cfb has 38 clauses, 26 in a release, 6 partial, 6 open.
   The comments of podrunner on #292 and #151 name `v0.1.0-beta.8`. Then
   T-500 (runner image for cfb; partial), T-517, T-521 and the partial
   entries T-497, T-499, T-509, T-511, T-515 and T-520.
4. Dependabot: #357 (trufflehog 3.100.0, `images/trufflehog`) and #358
   (toml 1.2.2): each as its own commit on `main` with green checks, then
   close the pull request with a link (`docs/maintenance.md`, "Dependabot").
5. The statement of G-09 on #66 (the docker build and Docker-in-Docker
   proof is in CI; T-517 is open).
6. The local P3 entries: T-665, T-655, T-656, T-658, T-659, T-598, T-597,
   T-616, T-603 (step 1), T-610, T-609. The follow-ups of QA round 6: T-638,
   T-643, T-644, T-645. The P3 groups of the QA rounds: T-308, T-316, T-323,
   T-331, T-339, T-633 to T-637, T-642.
7. The live reports (GitHub #74, #75): T-192 (P1, partial), T-181, T-186,
   T-175 (partial; its step on the test organization is next).
8. The local mode: T-204, T-229, T-242 (partial), T-161
   (partial; next, the measure of a third-party runner).
9. The hosted snapshot: T-539, T-542, T-555, T-540, T-557, T-556, T-579,
   T-547, T-546, T-550. T-613 (partial): the line of COORD-124 (#346).
10. The incus kind: T-482, T-483 (COORD-187), T-481. T-261 (partial): the
    syscall tier stays off until podvm fixes it (#308, #304, #189). T-403
    (partial) waits for COORD-090 (#300).

Waits:

- A run of `runner-image.yml` and `user-gate.yml` on the new head (T-660,
  T-662, T-487; item 1).
- Podvm: T-613 (COORD-124, #346): a run of `hosted-measure.yml` that gives
  the line of the free space and the need. T-658 (COORD-132, #354) and
  T-659 (COORD-133, #355): a podvm release.
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
