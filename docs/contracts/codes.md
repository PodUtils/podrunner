# The refusal codes, version 1

The table of the refusal codes, with the kind (`wait` or `terminal`) and
the meaning of each, is in `pod-job` of podcrates:
PodUtils/podcrates:crates/pod-job/README.md, "The refusal codes"
(`pod_job::RefusalCode`; a test of `pod-job` compares the table and the
enum). podrunner takes it at the tag of `Cargo.toml`.

podrunner refuses a job with one of these codes (GitHub #21). The run
report of a refused job has `refusal_code` and no unit fields. A `wait`
code holds the offer until a slot comes; at `wait_deadline_s` podrunner
sends `withdraw`. A `terminal` code refuses the job at once, and podrunner
suppresses the job id for `terminal_suppress_s` (3600 by default).

## Uses of podrunner beyond the table

| Code | Also when |
| --- | --- |
| `repo-scope-refused` | The offer is of a repository other than the `repository` of the host (DR-4, T-045). |
| `container-not-supported` | A hook call comes before `prepare_job`, or `services:` on a unit of the host network (`docs/contracts/hook.md`). |
| `container-option-refused` | A hook call breaks a rule of `docs/contracts/hook.md` (a mount, a create option, a registry). |
| `payload-not-pinned` | A container image of a hook call is not `NAME@sha256:HEX`. |
