# Progress

`TODO/INDEX.md` holds 160 entries: 0 open, 5 partial, 6 blocked, 149 done.

## State

- The plan is GitHub #1; one entry for each issue. The owner answered
  DR-1 to DR-7 and GitHub #8 (`docs/decisions.md`); DR-7 is yes
  (2026-11-13): the live gates run on GitHub #74 and #75.
- Pins: podcrates `v0.41.0`, podvm `v0.2.0-beta.14`, podnet
  `v0.4.0-beta.8`, runner v2.338.2. Last beta: `v0.1.0-beta.4` (release
  run 43506335609, `d796a94`). Last green `runner-image.yml`: 43541714659 (`9f15901`).
- M-A and M-B hold. M-C: T-027 to T-030 done; the gate T-032 waits for
  the live gates (GitHub #74, #75). M-D: T-031 and T-045 done; the gate
  T-033 waits for the live gates. L2 holds: T-034 to T-039 done
  (GitHub #39 is closed).
- The `microvm` kind (no NIC, one channel, a mount check before each job,
  an optional engine) and the `incus` kind (no NIC, one channel, hold
  `enforced`) pass on real guests and instances. T-103 has the engine in a
  guest, and its capability `container: engine` (podcrates `v0.42.0`).
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

1. T-101 (M, partial): the `program` source kind is in; its pair with GitHub runs on the host of LIVE-2 (GitHub #75) when githome gives its program.

The local mode against act and runner.server (`docs/local-mode.md`), the features that most workflows need first:

2. T-159 (S, partial): an intermittent unknown count of processes at the destroy of a podvm unit; read the cause in the next failed log.
3. T-160 (S, partial): a shell in a paused job of the incus kind; close it on the green run of `egress.rs` of the incus kind.

Partial, closed by their gates: T-001 (closes with GitHub #1), T-009 (the live rows wait for T-040; U15 for the owner).

Blocked: T-158 (podvm: a detached run of a static image, GitHub #100), T-032, T-033, T-040 (the live gates: GitHub #74 and #75), T-082 (a runner release with WebUtilities 8.0.34), T-043 (crates of podcrates).

## Questions for the owner

Q6 (T-148, 2026-11-14): Azure Pipelines YAML in the local mode? runner.server runs it on the Azure Pipelines agent, not on the GitHub runner: podrunner would need a second payload, a second fake broker, a store of tasks and a template expansion of about 4,000 lines (`docs/azure-pipelines.md`). Recommendation: no, not now; if yes, the first step is a measure of the agent in a podvm unit with one canned message. No work waits on the answer.

The owner answered Q1 to Q3 (DR-4, DR-5, DR-6) on 2026-11-11, and Q4, Q5 and DR-7 on 2026-11-13 (`docs/decisions.md`).
