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
- podrunner takes the crates of podcrates at `v0.21.0` (`pod-core`,
  `pod-todo`, `pod-broker`, `pod-proxy`, `pod-identity`, `pod-job`).
  `pod-supervise` can replace the tag scan of the podvm driver (T-023);
  it is not on the M-A path.
- Milestone M-A is done (T-015 to T-025). The H0 run of T-023: as
  `nobody` with a seccomp filter that refuses user namespaces, on the
  supervise rung of podvm, with the image of `scripts/stand-in-image.py`.
  T-047 (GitHub #49) records a defect of `podvm wait` there and the
  workaround of the driver.
- M-C: done T-027, T-028, T-029. T-030 is blocked on T-051: the image
  builds for both architectures and imports into a podvm store with no
  root (`runner-image.yml`, podvm of `images/podvm.toml`); on aarch64
  podvm has no aarch64 interposer yet (GitHub #52, PodUtils/podvm@T-1721).
- M-D: T-045 (the mitigations of DR-4) and T-031 (the remote mint,
  `crates/podrunner-remote`, `podrunner mint-serve`, a source kind
  `remote`) are done. T-033 waits for DR-7 and M-B.
- L2, on the fake path: `crates/podrunner-broker-fake` runs the official
  runner offline (`level2.yml`): canned jobs, the results routes, the
  corpus of podcrates jobs (`corpus/podcrates.json`, `scripts/corpus.py`)
  and a `container:` job through the hooks of T-035 (the relay, the
  socket, the checks, the fake unit), with the canary of a registry
  password. T-034 measured U21 (`scripts/measure-hook-path.sh`); U19
  needs podvm. T-034, T-035, T-036 and T-038 are blocked on podvm: the
  runner on podvm (T-026), the container verbs, the mapping of unix
  socket paths (T-050, fixed on podvm `main`).
- podnet answered COORD-001 (PodUtils/podnet#425): the runner pin
  `actions/runner` v2.338.2 and the object `hold` of `pod doctor --json`
  (T-044).

## In flight

- podvm `v0.2.0-beta.3` is pinned (`images/podvm.toml`); the driver needs
  it (`probe::LOWEST`). Milestone M-B holds: the official runner in a podvm
  unit runs a canned job on a host with no root (T-026, `runner-image.yml`).
  The container route of the podvm driver works on the supervise rung
  (T-035): a `container:` job runs through the hooks of a podvm unit.
- The gate queues for many minutes; a newer push cancels the queued run.
  Push less often, and close an entry on the green run of a later commit.
- The design copy is in `docs/design/` (T-014): the spec, the appendices,
  the contracts, the schemas, the examples, the recon and the
  measurements of the design review, cleaned. `docs/design-issues.md`
  maps each issue of the plan to its sections. The gate runs its checks
  (`docs/design/verify.sh`) and `scripts/check-design.sh`.
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
