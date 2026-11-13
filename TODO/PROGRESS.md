# Progress

`TODO/INDEX.md` holds 106 entries: 3 open, 4 partial, 9 blocked, 90 done.

## State

- The plan is GitHub #1; one entry for each issue. The owner answered
  DR-1 to DR-7 and GitHub #8 (`docs/decisions.md`); DR-7 is yes (2026-11-13): the live gates run on GitHub #74 and #75.
- Pins: podcrates `v0.37.0`, podvm `v0.2.0-beta.7`, podnet
  `v0.4.0-beta.8`, runner v2.338.2.
  Last beta: `v0.1.0-beta.2` (release run 43359637417, 2026-11-13).
- M-A and M-B hold. M-C: T-027 to T-030 done; the gate T-032 waits for the live gates (GitHub #74, #75). M-D: T-031 and T-045
  done; the gate T-033 waits for the live gates. L2: T-034, T-035, T-038 done; the
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

1. T-099 (S, partial): the confined conformance test 27 hangs at times (2 of the last 3 runs); each test has a time limit now; find the call that waits.
2. T-037 (M, open): the `microvm` kind; steps 1 and 2 are done (the 11 unit tests of the acceptance pass on real guests); next step 3, a canned job in a guest.

Partial, closed by their gates: T-001 (closes with GitHub #1), T-009 (the live rows wait for T-040; U15 for the owner).


Blocked: T-082 (a runner release with WebUtilities 8.0.34), T-032, T-033, T-040 (the live gates: GitHub #74 and #75), T-036, T-039 (after T-037), T-043 (crates of podcrates), T-102 (Q5 of the owner), T-106 (podcrates: `ActorClass` agent and bot, GitHub #80).

## Questions for the owner

- Q5 (T-102, githome's G-13, not urgent): may the operator add events to the set that can run as a trusted job? Now only `push`, `pull_request`, `workflow_dispatch`, `schedule` and `merge_group` can; `workflow_run`, `issue_comment`, `release` and `repository_dispatch` are refused, and githome fires them. On GitHub these four run the code of the default branch with the secrets of the repository, not the code of a pull request. Recommendation: yes, with limits. A key `[trust] extra_events` takes only those four; each needs the same facts as the others (a configured owner, no fork, an actor `member`); `pull_request_target`, `other` and `unknown` stay refused for good. Until a ruling, githome's live test uses the five events.

The owner answered Q1 to Q3 (DR-4, DR-5, DR-6) on 2026-11-11, and Q4 and DR-7 on 2026-11-13 (`docs/decisions.md`).
