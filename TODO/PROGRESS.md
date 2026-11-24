# Progress

`TODO/INDEX.md` holds 641 entries: 80 open, 59 partial, 15 blocked, 487 done.

## State

- The plan is GitHub #1; one entry for each issue. The owner answered
  DR-1 to DR-7 and GitHub #8 (`docs/decisions.md`); DR-7 is yes
  (2026-11-13): the live gates run on GitHub #74 and #75.
- Pins: podcrates `v0.82.1`, podvm `v0.2.0-beta.39`, podnet
  `v0.4.0-beta.17`, runner v2.338.2. Last beta: `v0.1.0-beta.7`
  (release run 44066276664, `129310e`).
- The owner, 2026-11-18 (`docs/decisions.md`, "githome and cfb first"):
  podrunner accommodates each need and ask of githome and cfb, and their
  CI work comes first. From the next session, all of PodUtils works to
  make podrunner complete and stable (COORD-086, GitHub #293).
- M-A and M-B hold. M-C: T-027 to T-030 done; the gate T-032 waits for
  the live gates (GitHub #74, #75). M-D: T-031 and T-045 done; the gate
  T-033 waits for the live gates. L2 holds: T-034 to T-039 done
  (GitHub #39 is closed).
- The `microvm` kind (no NIC, one channel, a mount check before each job,
  an optional engine) and the `incus` kind (no NIC, one channel, hold
  `enforced`) pass on real guests and instances. A guest with an engine runs
  `services:`, a job container and Docker actions (T-103, T-124, T-157).
- Each project gets COORD issues; a direct message to podcrates is also
  allowed (the owner, 2026-11-18); podnet and podvm answer; podssh is on
  break. An entry that
  needs a change of another project is `blocked`, and the session takes
  the next entry.
- The sidecar changes of T-504 and T-518 (the opt-ins of Q12 and Q13) are
  proven by unit and fake tests: not run against GitHub. Open from them:
  T-621 (a live lane), T-622, T-624. T-623 (the retry of a failed cancel) and T-625 (the attempt of a cancel of a job offered past the limits) are done. T-613 is partial: it waits
  for podvm (COORD-125 #347 ask 1, COORD-127 #349) and its next beta after
  beta.39. T-611 is blocked: on podvm beta.39 a stopped incus instance gave
  no peak and 0 OOM kills (runner-image 45359834173, COORD-128 #350); the
  live test takes that reason by name meanwhile.
- Keep this section short. Write the current state only; git keeps the
  history.

## Work order

Finish each entry, close it in place, change this page in the same commit,
then take the next one.

The queue, most valuable first:

0. Next (`docs/HANDOFF.md`, the session of 2026-11-24): read the CI on the newest commit; then the `v0.1.0-beta.8` path (the gates T-243, T-244 and T-246 with a new round of adversarial QA on the current `main`, then the checklist of `docs/maintenance.md`, "A release"), which moves 19 asks of githome and 11 clauses of cfb to "in a release"; then T-480, T-487, T-516, T-500; then the local P3 entries T-598, T-597, T-616, T-603 step 1, T-610, T-609. The rest of this item is the earlier order. githome and cfb first (the owner, 2026-11-18), in the order of the coordinator (COORD-136, PodUtils/podcrates#227; the one channel is COORD issues). githome and cfb are away until podrunner is complete; the recommendations of Q-1 to Q-8 are the Decision sections of their entries, and githome comes back on the statement of T-512. T-497 is partial (open: the cache bytes, T-480; the measure of incus on a real instance, T-483). The census is done (`docs/reports/census-2026-11-18.md`): no ask of G-01 to G-41 is done in full in a release; each of its gaps has an entry (T-515 and the 13 after it); the asks went to podvm (COORD-089, GitHub #296) and podcrates (COORD-087 amended, GitHub #294). P0: the trust lists of G-16 and the question of G-15 are done (proven on a real podvm unit); next a beta (name the tag on #292). Then P1 (the egress lines of G-04, the facts of G-17 and the question of G-15 are done: the schemas of `status.v1` and the source wire are `pod-job`'s from podcrates `v0.73.0`): T-496, T-497, T-499, T-509 (pod-fake-githome of podcrates `v0.73.0`), T-511, T-576 (the take of podcrates `v0.80.0`); then the GitHub-hosted parity (the rulings of 2026-11-19, T-538; `docs/reports/hosted-parity-2026-11-19.md` and `docs/reports/hosted-snapshot-2026-11-19.md`: a 1:1 snapshot of the hosted runner, no build): T-539, T-542, T-555, T-540, T-553 (ghcr.io, COORD-142), T-557, T-545, then on the answers of podvm and podcrates T-556, T-547, T-546, T-550, T-579 (after the first publish of T-555; T-543 and T-544 wait on COORD-103, #313, and COORD-106, #316; T-558, T-559 and T-560 wait on COORD-106 and COORD-107, #317); then T-482, T-483, T-481, T-487, T-480, T-484, T-489, T-515 to T-523, T-533, T-500, T-562 (after T-484), T-578 and T-577. The environment file is done; T-261 partial: the syscall tier is off until its run; T-403 waits for COORD-090, #300; COORD-187 unblocks T-482 and T-483; the guest pids limit is done. Last, T-512 (P0): `githome: complete in vTAG` on #292 and #151 when each ask G-01 to G-41 is done in a release. The items below come after it, except where an item is a need of githome or cfb.

1. The release gate (the owner, 2026-11-15): T-243 and T-244 (P1: the user-path gate on the files of a release, with a lane for each host class; the labs are shared, COORD-061 #180, and podrunner's scripts go first), T-246 (P1: an adversarial QA before each release), T-245 (the command line as a contract), T-249 (P1: the installer in each lab). No release request after beta.7 before T-243, T-244 and T-246 hold (`AGENTS.md`, section 12). Each P1 and P2 of the third and fourth QA is done. `v0.1.0-beta.7` is released (2026-11-17, release run 44066276664 on `129310e`). The owner (2026-11-17): podvm `v0.2.0-beta.30` is pinned (the PID namespace of a unit); the fifth QA (T-246) ran; agent 1 found no P1 and seven P2 (each done; P3: T-331); agent 2 found no P1 and six P2 (each done; P3: T-339). The path of `v0.1.0-beta.8` (`docs/decisions.md`, "beta.8"): (1) a user-gate run on `main` for the kit of the QA (the artifact `user-gate-assets`; the last run, 45276522625 on `f694571`, is green in all nine lanes); (2) QA round 6 (T-246) with two blind agents on that kit (`docs/qa-brief.md`, "A round before the release URL exists"), and each P1 and P2 of it fixed; (3) the runs of the gate, `level2.yml`, `runner-image.yml` and `user-gate.yml` on the release commit (T-626: `release.yml` needs the `runner-image.yml` run of that exact commit); (4) the checklist of `docs/maintenance.md`, "A release". The P3 groups T-308, T-316 and T-323 stay open meanwhile. QA round 6 runs on the candidate `0.1.0-beta.8` (user-gate run 45354505741 on `21aa8c4`). Agent A (user `qa8`) found no P1 and seven P2, and each blocks beta.8. Agent B (user `qa9`) found no P1 and three new P2 (Ctrl-C of a `container:` job on `--podvm`, `actions/checkout` `ref:` on the same repository, an organization `config_url` with `.git`) and a P2 that agent A also found (`config init` offers the microvm kind with no KVM). Round 6 is complete, and both agents cleaned their users. The beta.8 blockers of QA round 6 are all done: the last one (`podrunner man` and the help from one model of the commands, with `pod-manual` of podcrates `v0.82.1`, which also closes the entry of the help and the manual; follow-up T-645) is done at `fc1e52f` (not pushed). Next: the release commit (the notes docs/releases/v0.1.0-beta.8.md, `CHANGES.md` `## next` to `## v0.1.0-beta.8`) and the runs of the gate, `level2.yml`, `runner-image.yml` and `user-gate.yml` on it. T-611 waits for podvm `0.2.0-beta.40` (COORD-244, PodUtils/podvm#660). Done at `612088e` (batch 2, not pushed): the four P2 entries of agent B and of the microvm kind with no KVM; their follow-ups are T-643 (the other keys of the checkout) and T-644 (a lane of the cancel of a `container:` job on podvm). Done and pushed at `a6f1718` (batch 1): the other four P2 entries of agent A (the option of git in `--changed-since`, `image verify --arch x64`, a corrupt tar in the image cache, SIGXFSZ on a podvm unit), the P2 part of T-249 (the `.unpack.PID` directories of an interrupted install; it stays partial for its other steps) and the lane of T-638 (not run in CI; T-638 stays open). The P3 findings of agent A are T-633 to T-636, those of agent B are T-642 and lines in T-633, T-635 and T-636, and the untested areas of both are T-637.

2. The live reports (GitHub #74, #75, and the issues #116 to #147 of the owner): T-192 (P1: `serve` stops after a unit ends `unit-failed`), then T-181 (P2), T-186 (P3). T-175 waits for its step on the test organization. `v0.1.0-beta.7` is out for LIVE-1 and LIVE-2 (#74, #75) and githome (#151).

3. The onboarding of the local mode: T-205 (P1, partial: a `runner-image.yml` run and a dry run of `release.yml` prove the assets; then the next release), T-204 (P3: the packages of the image archives).

4. The second local-mode user run (#171 to #176): T-229 (M: the run picks its transport and stores; the fetch relay on the transport of the broker; `--vnet` is in).
5. The second QA report (its open entries): T-242 (partial: the load and the extract of each unit), then the P3 ones.
6. T-161 (M, partial): an alternate payload: the census and the call are in `docs/alternate-payload.md`; next, the measure of a third-party runner.

The local mode against act and runner.server (`docs/local-mode.md`) has no open entry.

Partial, closed by their gates: T-001 (closes with GitHub #1), T-009 (the live rows wait for T-040).

Blocked: T-032, T-033, T-040 (the live gates: GitHub #74 and #75), T-082 (a runner release with WebUtilities 8.0.34), T-043 (crates of podcrates).

## Questions for the owner

None is open.

### Answered

The owner ruled Q12 (an opt-in to trust `pull_request_target`, T-518) and Q13 (an opt-in to a GitHub source for a personal account, T-504) on 2026-11-24 (`docs/decisions.md`).

Before Q12: the owner ruled Q10 (retention on ghcr.io: keep each version) and Q11 (the snapshot keeps the Docker Hub login; the per-machine values are made fresh) on 2026-11-19 (`docs/decisions.md`).

Before Q10: the owner ruled Q9 on 2026-11-17 (`docs/decisions.md`).

Before Q9: the owner ruled Q7 and the size of a job, and Q8 and `limits_required` settled with githome, on 2026-11-15 (`docs/decisions.md`).

Before Q7: The owner answered Q6 and U15 on 2026-11-14 (`docs/decisions.md`).

The owner answered Q1 to Q3 (DR-4, DR-5, DR-6) on 2026-11-11, and Q4, Q5 and DR-7 on 2026-11-13 (`docs/decisions.md`).
