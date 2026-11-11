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
tag of its unit (`PODRUNNER_UNIT_TAG`) in its environment; `signal` and
`destroy` find the processes of a unit by a scan of `/proc`, also a
`setsid` grandchild whose parent is gone. With `container: hooks`, a
container is a process in a directory of the unit. The fake applies no
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
