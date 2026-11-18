# Progress

`TODO/INDEX.md` holds 489 entries: 54 open, 28 partial, 14 blocked, 393 done.

## State

- The plan is GitHub #1; one entry for each issue. The owner answered
  DR-1 to DR-7 and GitHub #8 (`docs/decisions.md`); DR-7 is yes
  (2026-11-13): the live gates run on GitHub #74 and #75.
- Pins: podcrates `v0.52.0`, podvm `v0.2.0-beta.27`, podnet
  `v0.4.0-beta.13`, runner v2.338.2. Last beta: `v0.1.0-beta.6` (release run 43708786078, `88cc4d7`; its runner image is the archive of runner-image run 43704601731). Last green
  `runner-image.yml`: 43704601731 (`88cc4d7`, the tag `v0.1.0-beta.6`). `main` has T-169 (the cancel of a refused run) and T-198 (the limits of an offer) after the tag: the next beta carries them.
- M-A and M-B hold. M-C: T-027 to T-030 done; the gate T-032 waits for
  the live gates (GitHub #74, #75). M-D: T-031 and T-045 done; the gate
  T-033 waits for the live gates. L2 holds: T-034 to T-039 done
  (GitHub #39 is closed).
- The `microvm` kind (no NIC, one channel, a mount check before each job,
  an optional engine) and the `incus` kind (no NIC, one channel, hold
  `enforced`) pass on real guests and instances. A guest with an engine runs
  `services:`, a job container and Docker actions (T-103, T-124, T-157).
- Each project gets COORD issues only (no direct message, the owner,
  2026-11-12); podnet and podvm answer; podssh is on break. An entry that
  needs a change of another project is `blocked`, and the session takes
  the next entry.
- Keep this section short. Write the current state only; git keeps the
  history.

## Work order

Finish each entry, close it in place, change this page in the same commit,
then take the next one.

The queue, most valuable first:

0. The release gate (the owner, 2026-11-15): T-243 and T-244 (P1: the user-path gate on the files of a release, with a lane for each host class; the labs are shared, COORD-061 #180, and podrunner's scripts go first), T-246 (P1: an adversarial QA before each release), T-245 (the command line as a contract), T-249 (P1: the installer in each lab). No release request after beta.7 before T-243, T-244 and T-246 hold (`AGENTS.md`, section 12). Each P1 and P2 of the third and fourth QA is done. `v0.1.0-beta.7` is released (2026-11-17, release run 44066276664 on `129310e`). The owner (2026-11-17): podvm `v0.2.0-beta.30` is pinned (the PID namespace of a unit); the fifth QA (T-246) ran; agent 1 found no P1 and seven P2 (each done; P3: T-331); agent 2 found no P1 and six P2 (each done; P3: T-339). Next: the P3 groups. Meanwhile the P3 groups T-308, T-316 and T-323.

1. The live reports (GitHub #74, #75, and the issues #116 to #147 of the owner): T-192 (P1: `serve` stops after a unit ends `unit-failed`), T-179 (P1: H0, the seccomp hold and the egress module time out), then T-181 (P2), T-186 (P3). T-175 waits for its step on the test organization. `v0.1.0-beta.6` is out for LIVE-1 and LIVE-2 (#74, #75) and githome (#151).

2. The onboarding of the local mode: T-205 (P1, partial: a `runner-image.yml` run and a dry run of `release.yml` prove the assets; then the next release), T-204 (P3: the packages of the image archives).

3. The second local-mode user run (#171 to #176): T-229 (M: the run picks its transport and stores; the fetch relay on the transport of the broker; with T-220).
4. The second QA report (its open entries): T-242 (partial: the load and the extract of each unit), then the P3 ones.
5. T-220 (M, blocked on podnet, COORD-058 #170): the local mode on a host that refuses AF_INET bind and connect (GitHub #167); steps 1 and 2 (the broker on a unix socket, the front of the guest kinds) can go first.
6. T-161 (M, partial): an alternate payload: the census and the call are in `docs/alternate-payload.md`; next, the measure of a third-party runner.

The local mode against act and runner.server (`docs/local-mode.md`) has no open entry.

Partial, closed by their gates: T-001 (closes with GitHub #1), T-009 (the live rows wait for T-040).

Blocked: T-032, T-033, T-040 (the live gates: GitHub #74 and #75), T-082 (a runner release with WebUtilities 8.0.34), T-043 (crates of podcrates).

## Questions for the owner

None is open. The owner ruled Q9 on 2026-11-17 (`docs/decisions.md`).

Before Q9: the owner ruled Q7 and the size of a job, and Q8 and `limits_required` settled with githome, on 2026-11-15 (`docs/decisions.md`).

Before Q7: The owner answered Q6 and U15 on 2026-11-14 (`docs/decisions.md`).

The owner answered Q1 to Q3 (DR-4, DR-5, DR-6) on 2026-11-11, and Q4, Q5 and DR-7 on 2026-11-13 (`docs/decisions.md`).
