# Handoff

The state of podrunner, for a session with no memory of the last one. Read
this page first, then `AGENTS.md`, `TODO/PROGRESS.md` (the state, the work
order, the questions), `docs/maintenance.md`, `docs/development.md` and
`docs/decisions.md`. Keep this page current; remove a line when its work
is done.

## Where podrunner stands

- The frame of podcrates is copied and adapted (T-011, T-012, T-013,
  done). The gate (`.github/workflows/gate.yml`) and `secrets.yml` run on
  each push; a newer push cancels the queued gate of an older one, so a
  Done section names the green run of a later commit that holds the work.
- The plan is GitHub #1. Each issue has one entry: GitHub #N is T-00N up
  to #43; later issues take the next free id (#46 is T-044).
- DR-1 and DR-2 are yes (`docs/decisions.md`). DR-4, DR-5 and DR-6 are
  questions Q1 to Q3. DR-7 waits for the ruling "Test accounts". The
  defaults of GitHub #8 are calls C1 to C12; C13 and C14 are calls of
  this session.
- Milestone M-A: done: T-015 (betas; `v0.1.0-beta.1`), T-016, T-018 to
  T-022. Partial: T-017 (the authoritative schemas come from
  PodUtils/podcrates@T-163), T-023 (the podvm driver passes the unit tests
  on this host with podvm `v0.2.0-beta.2`; a run on an H0 host is open).
  Open: T-024 (waits for `pod-broker`, PodUtils/podcrates@T-160 and
  PodUtils/podcrates@T-161), then T-025.
- M-C: T-027 is partial (`sign` and `Mint`; the restart of the sidecar
  comes with T-029). T-028 and T-029 are next: `actions/scaleset` v0.5.0
  needs Go 1.27.3 (the Go tool fetches it); the facts are in T-028.
- podnet answered COORD-001 (PodUtils/podnet#425): the runner pin
  `actions/runner` v2.338.2 and the object `hold` of `pod doctor --json`
  (T-044).

## In flight

- T-028 (the fake scale-set service, Go) and T-029 (the sidecar).
- The gate queues for many minutes; a newer push cancels the queued run.
  Push less often, and close an entry on the green run of a later commit.
- The spec of the design review (T-014): the owner holds it as a zip. The
  podcrates session asks the owner for it. Until it comes, work from the
  texts of the issues (each entry keeps the text in its Problem).
- The test numbers of the contracts follow `docs/contracts/`; T-014 makes
  them agree with the spec.

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
