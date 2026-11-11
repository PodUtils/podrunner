# The unit driver contract, version 1

A unit is one isolated environment for one job. A driver makes units of
one kind (`fake`, `podvm`, `microvm`, `docker`, `host`). In v1 the seam is
the Rust trait `UnitDriver` of `crates/podrunner-unit/src/lib.rs`. A wire
of JSON lines for drivers outside the tree is deferred (GitHub #10).

The core knows no tool by name. The driver is the only code that knows the
argv and the files of its tool: a change of the tool breaks one module and
its test, not podrunner at run time.

## Rules

- `probe` gives the capability record (`unit-capability.v1`,
  `crates/podrunner-schema/src/capability.rs`). Two calls give the same
  record. A field that the driver cannot prove is `unknown`, and placement
  reads `unknown` as absent.
- `prepare` takes a digest (`NAME@sha256:HEX`), never a tag
  (`payload-not-pinned`). It refuses a platform that is not the host's
  (`arch-mismatch`) unless emulation is named and the driver can emulate.
- `start` gives a fresh root and a fresh store for each unit. It writes the
  credential files into the runner root at mode 0600. No credential goes
  into argv or into the environment. A unit id that is in use is refused
  (`unit-in-use`). The core makes each unit id (`u-` and a counter); a unit
  id never derives from a job id.
- `signal` reaches each process of the unit (the whole tree).
- `wait` waits up to a time limit for the end of the top process.
- `destroy` ends each process and removes each file of the unit, then
  counts what is left (`processes`, `files`; `unknown` when it cannot
  count). A count above 0 or `unknown` makes podrunner `degraded`. A
  second `destroy` succeeds.
- `observe` gives the paths and the processes of a running unit, for the
  conformance tests and for `status`.
- The container operations exist when the record says `container: hooks`.
  With `container: none`, each is refused (`container-not-supported`).
  `container_exec` refuses `stdin` (`container-option-refused`).

## Operations

| Operation | Input | Output |
| --- | --- | --- |
| `probe` | none | the capability record |
| `prepare` | a pinned reference, a platform, emulation yes or no | the digest and the platform |
| `start` | the unit id, the prepared image, argv, environment, credential | none |
| `signal` | the unit id, `term` or `kill` | none |
| `wait` | the unit id, a time limit | the exit, or none |
| `destroy` | the unit id | the residue counts |
| `observe` | the unit id | the unit root, the runner root, the process ids |
| `container_start` | the unit id, a pinned image, argv, environment | a container id |
| `container_exec` | the unit id, the container id, argv, environment, workdir | the exit |
| `container_logs` | the unit id, the container id | the output so far |
| `container_inspect` | the unit id, the container id | running, the exit |
| `container_stop` | the unit id, the container id | none |

## Errors

Each error has a code and a detail with no credential:
`payload-not-pinned`, `arch-mismatch`, `container-not-supported`,
`container-option-refused`, `unit-start-failed`, `unit-in-use`,
`unknown-unit`, `unsupported`, `host`.

## Tests

`crates/podrunner-unit/src/conformance/` runs these tests against a
driver. Each test has a planted defect that fails it and a correct input
that passes it. A test that does not apply to the kind (no container
route) reports `skip` with the reason. The payloads of the tests are given
by the fixture of the driver: exit with a code, sleep, a `setsid` child
with a grandchild, a marker file in the working directory.

| Number | Test | What passes |
| --- | --- | --- |
| 1 | `probe-is-idempotent` | Two probes give the same record, with schema 1 and the kind of the driver. |
| 2 | `prepare-refuses-a-tag` | A digest is accepted and echoed; a tag is `payload-not-pinned`. |
| 3 | `destroy-reaches-a-setsid-grandchild` | After `destroy`, no process of a tree with a `setsid` grandchild lives, and the count is 0. |
| 4 | `prepare-refuses-a-foreign-platform` | A foreign platform with no emulation is `arch-mismatch`. |
| 5 | `a-fresh-root-for-each-unit` | Two units have two roots; a file of one is not in the other. |
| 6 | `credential-files-at-mode-0600` | Each credential file is in the runner root, mode 0600, with its content. |
| 7 | `no-credential-in-argv-or-environment` | No process of the unit holds the credential in argv or environment (the environment is readable: a positive control). |
| 8 | `wait-gives-the-exit-code` | `wait` gives the exit code of the top process. |
| 9 | `term-ends-the-unit` | After `signal term`, `wait` gives an exit. |
| 10 | `destroy-ends-each-process` | After `destroy`, no process of the unit lives. |
| 11 | `destroy-removes-each-file` | After `destroy`, the unit root and the runner root are gone. |
| 12 | `destroy-counts-the-residue` | `destroy` counts 0 processes and 0 files, not `unknown`. |
| 13 | `container-refused-with-no-route` | With `container: none`, `container_start` is `container-not-supported`. |
| 14 | `container-lifecycle` | With `container: hooks`: start, inspect, exec, exec with stdin refused, logs, stop. |
| 15 | `destroy-is-idempotent` | A second `destroy` succeeds and counts 0. |
| 16 | `a-unit-id-in-use-is-refused` | A second `start` of one unit id is `unit-in-use`. |
| 17 | `a-verified-tree-kill-is-proven` | A record that claims `tree_kill: verified` passes test 3 on this host. |

The numbers of the tests follow this page. The spec of the design review
(T-014) can give other numbers; T-014 makes them agree.
