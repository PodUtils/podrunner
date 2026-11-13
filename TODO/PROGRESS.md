# Progress

`TODO/INDEX.md` holds 100 entries: 1 open, 4 partial, 7 blocked, 88 done.

## State

- The plan is GitHub #1; one entry for each issue. The owner answered
  DR-1 to DR-7 and GitHub #8 (`docs/decisions.md`); DR-7 is "not now".
- Pins: podcrates `v0.36.0`, podvm `v0.2.0-beta.7`, podnet
  `v0.4.0-beta.8`, runner v2.338.2.
  Last beta: `v0.1.0-beta.1` (release run 42942338772).
- M-A and M-B hold. M-C: T-027 to T-030 done; the gate T-032 waits for DR-7. M-D: T-031 and T-045
  done; the gate T-033 waits for DR-7. L2: T-034, T-035, T-038 done; the
  gate T-039 waits for a real guest (T-036, T-037).
- Each project gets COORD issues only (no direct message, the owner,
  2026-11-12); podnet and podvm answer; podssh is on break. An entry that needs a
  change of another project is `blocked`, and the session takes the next
  entry.
- Keep this section short. Write the current state only; git keeps the
  history.

## Work order

Finish each entry, close it in place, change this page in the same commit,
then take the next one. No entry of the queue waits for a person, a
release or a live account.

The queue, most valuable first:

1. Release `v0.1.0-beta.2` (the owner, 2026-11-13) when the gate and `runner-image.yml` of `main` are green.
2. T-098 (S, partial): a Docker action in a job with no container (Q4); the next green `runner-image.yml` run closes it.
3. T-099 (S, partial): the confined conformance test 27 hangs in 2 of 2 runs since `0a400b5` (podvm beta.6 and beta.7); each test has a time limit now; find the call that waits. It holds the release of beta.2 while `runner-image.yml` is red.
4. T-037 (M, open): the `microvm` kind over the machine tier of podvm; step 1 is the measure of `scripts/measure-machine.sh` in `runner-image.yml`. Then T-036 (the guest self-test on it) and T-039 (milestone L2).

Partial, closed by their gates: T-001 (closes with GitHub #1), T-009 (the live rows wait for T-040; U15 for the owner).


Blocked: T-082 (a runner release with WebUtilities 8.0.34), T-032, T-033, T-040 (the live gates: GitHub #74 and #75), T-036, T-039 (after T-037), T-043 (crates of podcrates).

## Questions for the owner

None open.

The owner answered Q1 to Q3 (DR-4, DR-5, DR-6) on 2026-11-11, and Q4 and DR-7 on 2026-11-13 (`docs/decisions.md`).
