# Threat model

podrunner runs code of CI jobs, which can be hostile. This page gives each
actor, what it can do, the controls, and the verdict. `residuals.md` lists
the risks that the design accepts; `leak-runbook.md` gives the steps after
a leak of a credential. Each control names the entry that builds it.

## Credentials

| Credential | Where it lives | Control |
| --- | --- | --- |
| The source credential (the private key of the GitHub App) | The process of the sidecar, or of `mint` | A `sign` module signs one JWT shape, with a rate limit (T-027). No process that parses bytes of a job holds it (below). |
| The session and host tokens | Memory only | Never in a file, a log, argv or the environment. |
| The runner credential of one job | Three files in the runner root of the unit, mode 0600 | Unit test 6 and 7 (`docs/contracts/unit-driver.md`); removed by `destroy` (test 11). Never in argv, the environment, the journal, the audit or a report (T-021, a canary test). |

**The process rule.** No process that parses bytes that a job controls
holds a source credential. In v1 the modules of podrunner share one
process, so the rule holds only for the scale-set sidecar (T-029); the
split of `mint` into its own process is deferred (T-010).

## Actors

| # | Actor | Capability | Controls | Verdict |
| --- | --- | --- | --- | --- |
| 1 | The author of a pull request from a fork | Any code in the job; any `runs-on` label | The tier comes from job metadata, never from a label (T-018); `untrusted` needs `guest` and an `enforced` hold (T-021); a fork offer on a trusted set is `tier-mismatch` | Held, when a `guest` kind exists (T-037); else refused (`tier-needs-guest`). |
| 2 | A compromised dependency in a trusted job | Any code in the unit of a trusted job | `trusted` needs `confined` unless the operator sets `redirect` (DR-4, T-045); the egress broker allows the Actions hosts only (T-024); a fresh root for each unit (unit test 5); destroy removes each process and file (tests 3, 10, 11) | Accepted at `redirect` (DR-4, yes), with the mitigations of T-045 (`residuals.md`, R1). |
| 3 | A co-tenant job on one uid | Signals, `/proc`, the files of the uid | A shared principal runs one unit at a time, of one repository (T-021) | Accepted for `trusted` (`residuals.md`, R2). |
| 4 | A holder of a leaked source credential | Registers runners, edits runner groups | `leak-runbook.md`; the App is installed on named repositories | Detected by the runbook, not prevented. |
| 5 | A hostile container request | Options, mounts, images | The hook contract refuses a build, an image off the pin list and options that the kind refuses (`docs/contracts/hook.md`, T-035) | Held. |
| 6 | Hostile hook input | Bytes on the hook path and the control socket | Parsers with a line limit and fuzz targets (`fuzz/`); a refusal writes `hook-refuse` (T-035) | Held. |
| 7 | A hostile proxy or DNS | Answers of the network | TLS to each host of the Actions service; pins of the runner payload by digest (T-030) | Held. |
| 8 | A holder of a stolen host token | Calls `mint` as the host | Mutual TLS with pinned keys for a remote `mint` (T-031, PodUtils/podcrates@T-165); revocation at `mint` | DR-4 is yes: T-031 waits for PodUtils/podcrates@T-165. |
| 9 | A compromised `mint` host or sidecar | Each credential of its source | The process rule; the runbook | Accepted (`residuals.md`, R4). |
| 10 | A job with secrets, or an author that nobody vetted | Secrets of the repository in a trusted unit | Trust comes from the organization and the event (T-018), not from the author; secrets reach the unit as the runner gets them | Accepted (`residuals.md`, R3). |
| 11 | An observer on the path to a remote `mint` | Reads and changes traffic | Mutual TLS (T-031) | DR-4 is yes: T-031 waits for PodUtils/podcrates@T-165. |
| 12 | A trusted job that kills its shim | Ends the subreaper of its unit | A dead shim gives `residual_processes: unknown`, which makes the host `degraded` (T-021, T-023) | Held: the host stops taking jobs. |

## Text from a job

Each string that a job or a forge controls prints only after each control
character becomes `\xNN`, cut at 200 bytes (`crates/podrunner/src/text.rs`,
T-022).
