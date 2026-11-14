# The fakes

`crates/podrunner-fake` holds the `fake` source and the `fake` unit
(GitHub #20). Level 1 runs on them, offline, on a manual clock
(`crates/podrunner-clock`). Each fake can carry one planted defect, which
fails one conformance test (`docs/contracts/unit-driver.md`,
`docs/contracts/source-driver.md`). The configuration refuses a `fake`
source or unit in production unless `allow_fake = true`, and each report
of a fake shows the kind `fake`.

## The script of the fake source

One JSON object on each line. An empty line, and a line that starts with
`#`, is skipped. `at_s` is the time of the step on the clock of the
source, in seconds. A step is due when the clock reaches it; `next` gives
the due events, and the due offers up to its capacity.

| Step | Example |
| --- | --- |
| An offer (`job-offer.v1`); a second offer of one job id is a redelivery | `{"at_s":0,"offer":{"job_id":"j-1","source":"fake-1","labels":["x"],"arch":"x86_64","wait_deadline_s":60}}` |
| A cancel | `{"at_s":30,"cancel":"j-1"}` |
| A `started` event with a runner name | `{"at_s":5,"started":{"job_id":"j-1","runner_name":"r-u-1"}}` |
| A `completed` event | `{"at_s":90,"completed":{"job_id":"j-1","result":"success"}}` |
| An error of the next `mint` of a job | `{"at_s":0,"mint_error":{"job_id":"j-1","code":"credential-unavailable"}}` |

`mint` gives a canned credential: the files `.runner`, `.credentials` and
`.credentials_rsaparams`, with made-up text.

## The fake unit

The fake unit declares the capability record that it is given, and runs
the payload as real processes on the host, in a directory of its own
under the state directory. Each process carries `PODRUNNER_UNIT` and the
tag of its unit (`PODRUNNER_UNIT_TAG`) in its environment; `signal`
finds the processes of a unit by a scan of `/proc`. The top process runs
under a shim (`podrunner_unit::shim`): a subreaper that holds each orphan
of the unit, also one that drops the tag (`setsid env -i`). `destroy`
kills the tree under the shim and each tagged process. A shim that a
signal ended makes the count `unknown`. With `container: hooks`, a
container is a process tree under a shim of its own, in a directory of
the unit. `FakeUnit::confined` gives a `confined` fake (T-042): each process
of a unit runs under the Landlock rules and the seccomp filter of
`podrunner_unit::confine` (`pod-confine`), and reads and writes only its
root and reads the system directories. The fake applies no
mount and no create option; it writes them to the file `spec` of the
container directory, one on each line, for the tests of the hooks.

The behaviours of a payload are argv (`payload` of the crate): exit with a
code, sleep, a `setsid` child with a grandchild, a marker file. A test
gives other behaviours as argv of its own.

## Planted defects

| Unit test | Plant |
| --- | --- |
| 1 | each probe gives another record |
| 2 | a tag is accepted |
| 3 | destroy kills the top process only |
| 4 | a foreign platform is accepted |
| 5 | each unit gets the same root |
| 6 | the credential files have mode 0644 |
| 7 | the credential goes into the environment |
| 8 | wait gives exit code 0 |
| 9 | signal does nothing |
| 10 | destroy kills nothing |
| 11 | destroy keeps the files |
| 12 | destroy counts nothing (`unknown`) |
| 13 | a container starts with no route |
| 14 | exec accepts stdin |
| 15 | a second destroy fails |
| 16 | a unit id in use starts again |
| 17 | probe claims a verified tree kill, and destroy kills the top process only |
| 18 | probe writes a file in the state directory |
| 19 | each unit gets one shared `/etc/hosts` |
| 20 | the processes of a container do not carry the tag, and have no shim |
| 21 | start keeps a copy of the credential outside the unit root |
| 22 | probe gives the credential of the last unit in `boundary_proof` |
| 23 | probe leaves a process (`tests/alone.rs`) |
| 24 | the credential goes into the environment of each container (`tests/alone.rs`) |
| 25 | no shim: destroy finds the processes by their tag only |
| 26 | destroy counts 0 processes after its shim was killed |
| 27 | the record claims `confined`, and no rule applies |

| Source test | Plant |
| --- | --- |
| 1 | `hello` gives no name |
| 2 | an offer with capacity 0 |
| 3 | each due offer, past the capacity |
| 4 | an ack of a job that was not offered |
| 5 | a second claim |
| 6 | a mint with no claim |
| 7 | a refused report |
| 8 | a refused audit line |
| 9 | withdraw keeps the offer |
| 10 | release keeps the claim |
| 11, 12 | a wire that answers each line with `done` |

## The fake broker (level 2)

`crates/podrunner-broker-fake` serves the routes that the official
runner v2.338.2 calls with the JIT files of a scale set
(`files::runner_files`): the token of its OAuth client, the broker
session and messages, and the run service (`acquirejob`, `renewjob`,
`completejob`), and the results service of the Worker (the logs and the
step updates). A test queues a `Job` (a name, an environment, the
`run:` steps with their `if:` and `env:`, and overrides of the message:
the steps, the context data and the containers of the local mode) and
reads each `Completion` (the conclusion and the job outputs) and the
logs. For the local mode it also serves the launch route and the tarballs
of the actions of a directory (`serve_actions`, T-091) and the tarball of
the local tree (`serve_checkout`, T-092). It checks no credential and listens on the loopback only.
`.github/workflows/level2.yml` runs the official runner against it.

The corpus `corpus/podcrates.json` holds the `ubuntu-24.04` jobs of the
podcrates workflows that the fake can run, at one commit of podcrates.
`scripts/corpus.py` writes it from a checkout and lists each other job
with the reason (an expression, an action other than the checkout,
`sudo`, `needs`, a matrix). The checkout step becomes a clone of the
local source that `PODRUNNER_CORPUS_SRC` names. To move the corpus to a
newer commit, run the script on a checkout of that commit. The corpus
job of `level2.yml` checks that the file agrees with its workflows.

