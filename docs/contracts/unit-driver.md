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
  `container_exec` refuses `stdin` (`container-option-refused`). A driver
  applies each mount and each create option of `container_start`, or
  refuses the start (`container-option-refused`); the hook checks of
  `docs/contracts/hook.md` come before it.

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
| `container_start` | the unit id, a pinned image, argv, environment, workdir, mounts (a path of the unit root, a target, read-only), checked create options | a container id |
| `container_exec` | the unit id, the container id, argv, environment, workdir, a host file for the output | the exit |
| `container_logs` | the unit id, the container id | the output so far |
| `container_inspect` | the unit id, the container id | running, the exit |
| `container_stop` | the unit id, the container id | none |

The socket directory. Each unit has a directory of its own on the host
with its egress socket (`egress.sock`), its control socket (`ctl.sock`)
and the hook relay. A driver whose unit has a root of its own names, in
`socket_mount`, where the unit sees that directory; `start` gets it as
`sockets` and mounts it there, and the environment of the unit names the
guest paths. The podvm driver mounts it at `/run/podrunner` on the
supervise rung, the rung that has volumes and so the container route.
A driver with no `socket_mount` shares the paths of the host (`fake`).

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
| 18 | `probe-writes-no-file` | Two probes change no file of the state directory of the driver (the fixture names it; else `skip`). |
| 19 | `etc-hosts-of-one-unit` | A name written to `/etc/hosts` of one unit is not in `/etc/hosts` of another. |
| 20 | `destroy-reaches-a-grandchild-in-a-container` | With `container: hooks`: after `destroy`, no process of a container with a `setsid` grandchild lives, the count is 0, and `destroy` ends within the time limit. |
| 21 | `a-host-scan-finds-no-credential` | After `destroy`, no file of the state directory holds the credential (the scan finds it before `destroy`: a positive control). |
| 22 | `no-credential-in-the-outputs` | The probe record (with `boundary_proof`), the observation and the residue hold no credential. |

The numbers of the tests follow this page. The spec of the design review
(T-014) can give other numbers; T-014 makes them agree.

## The tests of the design contract

`docs/design/contracts/unit-driver.md` numbers its suite in another
order, and the issues name those numbers (GitHub #35). This table gives
each design test, the tests here that cover it, and what is open (T-053).
`UC` is `crates/podrunner-unit/src/conformance/` (`cases.rs`, `design.rs`), `HK`, `HU` and
`HS` are the tests of `crates/podrunner-hook/src/`, `SG` is
`crates/podrunner-slots/src/tests/`.

| Design | Covered by | Open |
| --- | --- | --- |
| 1 probe | UC `probe-is-idempotent`, `probe-writes-no-file` | no process appears |
| 2 two roots | UC `a-fresh-root-for-each-unit`, `etc-hosts-of-one-unit` | none |
| 3 tree kill | UC `destroy-reaches-a-setsid-grandchild`, `a-verified-tree-kill-is-proven`, `destroy-reaches-a-grandchild-in-a-container` (also the time limit) | none |
| 4 limits | SG `a_required_limit_that_no_kind_enforces_is_refused` (placement refuses before `start`), `a_limit_refusal_names_the_limit` (the audit line names the limit) | none |
| 5 tag, platform | UC `prepare-refuses-a-tag`, `prepare-refuses-a-foreign-platform` | none |
| 6 credential gone | UC `destroy-removes-each-file`, `a-host-scan-finds-no-credential` | none |
| 7 /proc dump | UC `no-credential-in-argv-or-environment` | a dump of `/proc/*` from a step |
| 8 exec env, workdir | UC `container-lifecycle` (an exec with an environment and `/tmp`) | none |
| 9 another unit's container | HU `the_socket_of_one_unit_reaches_no_container_of_another`: no call names a container id | none |
| 10 proof canary | UC `no-credential-in-the-outputs` | none |
| 11 guest self-test | SG `a_fake_that_claims_guest_fails_the_self_test` | a real guest passes (T-036, T-037) |
| 12 `../x` | `crates/podrunner-schema/src/lib.rs` `an_id_that_climbs_never_parses` (pod-job ids) | none |
| 13 container options | HK `each_rule_refuses_its_input`, `a_job_container_of_the_runner_passes_with_its_mounts` (the seven entries), `the_mount_tables_and_the_order_of_the_design` | none on the fake path |
| 14 job_state | HU `each_command_reaches_the_fake_unit` (a second `job-started` is ignored and goes to the audit), SG `job_started_in_running_is_ignored_and_job_completed_moves_no_state` | none |
| 15 shim killed | none | the whole test (podvm driver) |
| 16 the order | HK `each_call_before_prepare_job_is_refused`, `the_mount_tables_and_the_order_of_the_design` | none |
| 17 action table | HK `the_mount_tables_and_the_order_of_the_design` | none |
