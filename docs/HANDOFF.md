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
- The owner answered DR-1 to DR-7 and GitHub #8 (`docs/decisions.md`):
  `trusted` at `redirect` is a choice of the operator with the mitigations
  of T-045; `microvm` is an adapter over podvm's machine tier (T-037);
  DR-7 is not now. C1 to C12 are rulings; C13 to C16 are calls.
- podrunner takes the crates of podcrates at `v0.17.0` (`pod-core`,
  `pod-todo`, `pod-broker`, `pod-proxy`, `pod-identity`, `pod-job`).
  `pod-supervise` can replace the tag scan of the podvm driver (T-023);
  it is not on the M-A path.
- Milestone M-A: done: T-015 (`v0.1.0-beta.1`), T-016 to T-022, T-024.
  Partial: T-023 (an H0 run is open). T-025 closes when it is done.
- M-C: done T-027, T-028, T-029. T-030 is partial: the pin fields, the
  age check at the start, runner exit 7, `images/runner/` (the reviewed
  pin, the Dockerfile), `.github/workflows/runner-image.yml` and
  `podrunner image verify` are in; the first green image build and its
  pin are open.
- M-D: T-045 is done (the mitigations of DR-4). T-031 (the remote mint,
  `crates/podrunner-remote`, `podrunner mint-serve`, a source kind
  `remote`) is partial: the removal of the runners of a revoked host and
  the source tests over HTTPS in CI are open.
- L2: T-035 (the container hooks, `crates/podrunner-hook`) is partial:
  the checks, the socket, the relay, the mapping onto the driver and the
  seam `HookPort` of slots are in; the tests with a real runner wait for
  T-038 and T-026. T-036 is blocked: the rest needs a real guest (T-037,
  PodUtils/podvm@T-1656). T-034, T-038 need the runner on podvm (T-026,
  blocked).
- podnet answered COORD-001 (PodUtils/podnet#425): the runner pin
  `actions/runner` v2.338.2 and the object `hold` of `pod doctor --json`
  (T-044).

## In flight

- The work order of `TODO/PROGRESS.md`: T-031, T-030 and T-035 are
  partial; their entries name the open steps.
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
