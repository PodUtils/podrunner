# Progress

`TODO/INDEX.md` holds 76 entries: 4 open, 4 partial, 16 blocked, 52 done.

## State

- The plan is GitHub #1; one entry for each issue. The owner answered
  DR-1 to DR-7 and GitHub #8 (`docs/decisions.md`); DR-7 is "not now".
- Pins: podcrates `v0.24.0`, podvm `v0.2.0-beta.3`, runner v2.338.2.
  Last beta: `v0.1.0-beta.1` (release run 42942338772).
- M-A and M-B hold. M-C: T-027 to T-029 done; T-030 waits for T-051
  (podvm aarch64); the gate T-032 waits for DR-7. M-D: T-031 and T-045
  done; the gate T-033 waits for DR-7. L2: T-034, T-035, T-038 done; the
  gate T-039 waits for a real guest (T-036, T-037).
- podcrates is on direct messages; podnet answers COORD issues; podvm and
  podssh are on break. An entry that needs a change of podvm or podssh is
  `blocked`, and the session takes the next entry.
- Keep this section short. Write the current state only; git keeps the
  history.

## Work order

Finish each entry, close it in place, change this page in the same commit,
then take the next one. No entry of the queue waits for a person, a
release or a live account.

The queue for tonight, most valuable first:

1. T-065 (M): the podvm unit runs under `podnet pod`; without a hold, a podvm unit gets no `trusted` job, so M-D cannot pass. Start the measure in `runner-image.yml`.
2. T-070 (S): `doctor` reads the configuration, the driver records, `pod-hostprobe` and the Landlock ABI; the operator sees what placement sees.
3. T-071 (M): the live gate harness on the fake scale set; when DR-7 is yes, only the secrets are missing.
4. T-074 (M): the disk limit of a unit (SPEC §10.4); a job can fill the disk of the host now.
5. T-041 (S): the `enforced` hold with `podnet pod --backend netns` on a host with user namespaces; start when T-065 is done.
6. T-075 (S): the report and status fields of SPEC §11.4 and §11.5; needs a small ask to podcrates.

Partial, closed by their gates: T-001 (closes with GitHub #1), T-009 (the live rows wait for T-040; U15 for the owner).

Blocked: T-064 (the next podvm beta: only the pin), T-073 (the podcrates tag after `v0.24.0`), T-007, T-032, T-033, T-040 (DR-7), T-030 and T-051 (podvm aarch64), T-036, T-037, T-039 (the podvm machine tier), T-042 (PodUtils/podvm#259), T-061, T-062 (the next podvm beta), T-043, T-046 (crates of podcrates).

## Questions for the owner

None is open. The owner answered Q1 to Q3 (DR-4, DR-5, DR-6) on
2026-11-11 (`docs/decisions.md`).
