# Handoff

The state of podrunner, for a session with no memory of the last one. Read
this page first, then `AGENTS.md`, `TODO/PROGRESS.md` (the state, the work
order, the questions), `docs/maintenance.md`, `docs/development.md` and
`docs/decisions.md`. Keep this page current; remove a line when its work
is done.

## Where podrunner stands

- The frame of podcrates is copied and adapted (T-011, T-012, T-013):
  the workspace, the gate, the record, the hooks, the watcher.
- The plan is GitHub #1. Each issue has one entry: GitHub #N is T-00N up
  to #43.
- DR-1 and DR-2 are yes (`docs/decisions.md`). DR-4, DR-5 and DR-6 are
  questions Q1 to Q3. DR-7 waits for the ruling "Test accounts". The
  defaults of GitHub #8 are calls C1 to C12.

## In flight

- Milestone M-A (GitHub #25), in the order of `TODO/PROGRESS.md`.
- The spec of the design review (T-014): the owner holds it as a zip. The
  podcrates session asks the owner for it. Until it comes, work from the
  texts of the issues (each entry keeps the text in its Problem).

## Channels

- podcrates: direct messages (`send_message`) to the session of podcrates
  (`list_sessions`, title with `(podcrates)`). Each ask for a crate goes
  there; podcrates answers with the id of its entry and releases a tag.
  A short report goes there after each milestone: the commit, the gate
  run, what is next.
- podnet: a COORD issue of this repository titled
  `COORD-NNN: subject (to podnet)`; podnet answers in its repository with
  `(to podrunner)`, and the watcher finds it. PodUtils/podvm#228 holds
  podnet's asks for podrunner; podnet's answer is PodUtils/podnet#422.
- podvm, podssh: on break. A COORD issue `(to podvm)` can go; expect no
  answer.
