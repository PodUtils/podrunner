# Progress

`TODO/INDEX.md` holds 153 entries: 34 open, 3 partial, 6 blocked, 110 done.

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
  guest; its capability waits for podcrates (GitHub #91).
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

2. T-118 (M): actions by tag, fetched through the egress module into a store with a lock file.
3. T-119 (M): artifacts v4, one store for each run.
4. T-120 (M): the cache service of `actions/cache`.
5. T-121 (S): secrets from the environment and a secret file, with the mask.
6. T-122 (S): `github.token` and `secrets.GITHUB_TOKEN`.
7. T-123 (M): the local mode on the microvm and incus kinds.
8. T-124 (M): `services:`, `docker build` and Docker-in-Docker through the engine of the unit.
9. T-145 (S), T-146 (S), T-141 (S): the defects of a missing route, of files that a job leaves, and of Ctrl-C.
10. T-125 (M): the event payload, the `github` context of the tree, the filters of `on:`.
11. T-133 (M): the checkout with the changes of the tree, `.git`, other repositories.
12. T-130 (M), T-129 (M): images by tag into the store; runner images by `runs-on`.
13. T-131 (L): reusable workflows.
14. T-127 (S), T-134 (M), T-126 (S), T-128 (S): selection, logs, inputs and files, list and dry run.
15. T-132 (M), T-135 (M), T-136 (S), T-137 (S): parallel jobs, reruns, environments, concurrency.
16. T-138, T-139, T-140, T-142, T-143, T-152, T-153 (S each), T-150 (M), T-149 (M), T-151 (S), T-148 (M).

Partial, closed by their gates: T-001 (closes with GitHub #1), T-009 (the live rows wait for T-040; U15 for the owner).

Blocked: T-103 (podcrates: `ContainerRoute::Engine` in the tag after `v0.41.0`, GitHub #91, PodUtils/podcrates@T-553, on the `main` of podcrates; steps 1 to 3b are done, `docs/container-engine.md`), T-032, T-033, T-040 (the live gates: GitHub #74 and #75), T-082 (a runner release with WebUtilities 8.0.34), T-043 (crates of podcrates).

## Questions for the owner

None open.

The owner answered Q1 to Q3 (DR-4, DR-5, DR-6) on 2026-11-11, and Q4, Q5 and DR-7 on 2026-11-13 (`docs/decisions.md`).
