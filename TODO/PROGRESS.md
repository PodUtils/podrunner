# Progress

`TODO/INDEX.md` holds 723 entries: 67 open, 52 partial, 24 blocked, 580 done.

## State

- The plan is GitHub #1; one entry for each issue. The owner answered DR-1
  to DR-7 and GitHub #8 (`docs/decisions.md`). DR-7 is yes: the live gates
  run on GitHub #74 and #75.
- Pins: podcrates `v0.86.0`, podvm `v0.2.0-beta.40`, podnet `v0.4.0-beta.20`,
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
- The hosted Incus snapshot ran rootless Podman offline as `runner`,
  including FUSE overlay copy-up, in hosted-boot run 45623615056 (T-674).
- Incus CPU and peak memory passed the strict limits test (T-483).
  Podvm proved the Btrfs quota in experiment 474. The Podrunner job
  report for a root quota failure remains T-482.
- A `docker build` step and a `docker:dind` service pass in a guest and in
  an incus instance, proven in CI on GitHub-hosted runners (T-516,
  `runner-image.yml` 45564676018; `level2.yml` 45564676057 on github.com
  twins). The Docker 29.9.2 of `runner-full` (T-672) passed both on
  microvm and on Incus 6.0.7 (runner-image 45773152397). Incus before
  6.0.6 and 6.19 refuses its runc; podrunner warns of it (T-683). The live
  check (T-040) remains.
- Warm units (T-487) are enabled for fake, Incus and microvm. The
  installed binary passed both warm rows in user-gate run 45626716848.
  Incus first steps were 14 and 14 s cold, then 2 and 2 s warm. Microvm
  first steps were 6 and 6 s cold, then 2 and 3 s warm.
- The `assignment_hook` payload flag enabled the runner's assignment
  verdict and first-step marks on both guests (T-660, T-663). A job of
  another run can keep a `rw` cache when its verified ref and repository
  match the cache grant.
- The signed release assets of podrunner `v0.1.0-beta.8` and podcrates
  `v0.84.0` supply the source test tools and fakes on both architectures
  (T-511). The podcrates OCI provenance covers matching executables.
- The image recipe command built a full OCI image with an extra package.
  The imported digest ran a workflow on the test host as a user (T-500).
- The sidecar of the scale set, with the opt-ins of Q12 (T-518) and Q13
  (T-504), has unit and fake tests only. T-654 covers a single retry of
  a first-attempt job that ended with `cache-mismatch`. The live lane
  is T-621. T-622 and T-624 also remain open.
- The state of each issue comes from the record by a tool; the step of the
  hand-written State column is open (T-250, partial).
- Runner-image run 45736054587 on `8801e939` imported both full images.
  The bundled Docker ran the probe, `docker build` and Docker-in-Docker on
  the microvm kind. On the incus kind (Incus 6.0.0 of apt) each container
  start failed: the AppArmor profile of Incus before 6.0.6 and 6.19
  refuses runc 1.3.3 and later (T-683). podrunner now warns of it, and the
  image run proves the engine on Incus 6.0 LTS of Zabbly.
- Keep this page short. Write the current state only; git keeps the
  history.

## Work order

Finish each entry, close it in place, change this page in the same commit,
then take the next one.

1. Read the runs of #377: `user-gate.yml` proves T-717 and the check
   `serve:job:engine:microvm` of T-509.
2. The rest of the cache work in T-480 waits for podvm: T-648 (COORD-143,
   #371), T-650 (COORD-141, #368).
3. The open asks of githome and cfb: T-512 (P0): write
   `githome: complete in vTAG` on #292 and #151 only when each ask G-01 to
   G-41 is in a release. It is not: githome has 41 asks, 25 in a release, 10
   partial, 6 open; cfb has 38 clauses, 26 in a release, 6 partial, 6 open.
   The comments of podrunner on #292 and #151 name `v0.1.0-beta.8`. Then
   the partial
   entries T-497, T-499, T-509 and T-515.
4. Keep the G-09 line on #66 current with the CI proof of Docker build
   and Docker-in-Docker, the bundled engine (done), and the live check
   (T-040).
5. The local P3 entries: T-656, T-658, T-659, T-609, T-719, T-725 to T-728. The P3 groups of the QA rounds: T-308, T-316, T-323,
   T-331, T-339, T-637, T-701.
6. The live reports (GitHub #74, #75): T-192 (P1, partial), T-181, T-186,
   T-175 (partial; its step on the test organization is next).
7. The local mode: T-204, T-229, T-242 (partial), T-161
   (partial; next, the measure of a third-party runner).
8. The hosted snapshot: T-539, T-542, T-555, T-540, T-556, T-579,
   T-547, T-546, T-550.
9. The incus kind: T-482 (COORD-187), T-481. T-261 (partial): the
    syscall tier stays off until podvm fixes it (#308, #304, #189). T-403
    (partial) waits for COORD-090 (#300).

Waits:

- Podvm: T-685 (COORD-138, #364), T-650 (COORD-141, #368), T-706 (COORD-144, #372), T-603 (COORD-147, #378).
- Podcrates: T-707 (COORD-145, #373), T-714 (COORD-146, #375), T-724
  (COORD-148, #379). The copy of
  the cgroup rule of podvm goes after COORD-137 (#363). T-658 (COORD-132, #354) and
  T-659 (COORD-133, #355): a podvm release. T-681 waits for the next
  Incus image import path for the low-space control. COORD-254
  (PodUtils/podvm#671) asks to retest both runner image architectures
  on that release. T-649 (COORD-135, #359) waits for a podvm release with
  `--podvm-count-reads` (route A of podvm#672).
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
