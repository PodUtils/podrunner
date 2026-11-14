# The parity suite of the local mode (T-147)

Nine workflows of rising scope. `crates/podrunner/tests/parity.rs` runs
each one with `podrunner run` and gives one verdict for each:

* PASS: each job of the workflow is `ok`.
* PARTIAL: a job failed, and the workflow needs a feature of an open entry
  (the table of the test names it).
* FAIL: a job failed, and no open entry explains it.
* SKIP: the workflow needs a container route, and the run is with
  `--runner-dir`. `PODRUNNER_PARITY_UNIT` gives the options of a unit kind
  (`--microvm PODVM --images STORE --image REF --engine DIR --allow HOST`).

Each check computes its value in the shell and prints it on a line of its
own, so the echo of a script cannot match it. A check that fails exits
non-zero. Images are pinned by digest. `podrunner-actions.lock` pins each
action by commit and by the digest of its tar (T-118); the test runs with
`--frozen`, so a change of an action changes the lock in a commit.

| File | Scope | Route |
| --- | --- | --- |
| `01-exit.yml` | exit codes, `continue-on-error`, `if:` status functions | |
| `02-expressions.yml` | functions, `fromJSON`, `toJSON`, `needs` outputs | |
| `03-matrix.yml` | `exclude` and `include`: exactly four cells | |
| `04-services.yml` | a service with a health check, by its name and by `localhost` | engine |
| `05-containers.yml` | a job container and a Docker step | engine or hooks |
| `06-actions.yml` | a marketplace action by tag, a local composite action with outputs | |
| `07-artifacts.yml` | an artifact round trip with a check of its content | |
| `08-cache.yml` | a cache round trip: the exact key and a restore key | |
| `09-dockerfile.yml` | a Dockerfile action that builds and gives an output | engine |
