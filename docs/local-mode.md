# The local mode against act and runner.server

`podrunner run FILE` runs the jobs of one workflow file with the official
runner, against a fake broker on the loopback, one fresh unit for each job
(T-046, `docs/cli.md`). This note compares it with nektos/act and with
ChristopherHX/runner.server, as the owner's ghactions-local study shows
them (act 0.2.91, runner.server v3.14.0). Both are MIT. No code or text of
them is in this repository. An entry that plans to take third-party code
says so, and the take goes through `THIRD_PARTY.md`.

## How each one runs a job

| | act | runner.server | podrunner |
| --- | --- | --- | --- |
| Engine of the steps | its own Go engine | the official runner (a fork), with its own server | the official runner, unchanged, pinned |
| Isolation | a container of the Docker daemon of the host | a container of the host's Docker, or the host | a fresh unit for each job (podvm; microvm and incus with T-123), no root, no daemon |
| Network of a job | the Docker network, open | open | the egress hold: only the hosts of `--allow` |
| Binary | static Go | .NET, not static | static musl |

## The features

The column "podrunner now" gives the state of `main`. The last column names
the entry that closes the gap, or the entry that did the work.

| Feature | act | runner.server | podrunner now | Entry |
| --- | --- | --- | --- | --- |
| `needs`, outputs, `if:`, matrix with `include` and `exclude` | yes | yes | yes | T-087, T-088 |
| `fail-fast`, job `continue-on-error` | yes | yes | no | T-117 |
| The `github` context in the steps | yes | yes | yes, the context of the run over the canned one | T-114 |
| `run:` steps, `defaults.run`, `shell`, `working-directory` | yes | yes | yes | T-088 |
| Composite and JavaScript actions by commit | yes | yes | yes, from `--actions DIR` | T-091 |
| Actions by tag or branch, fetched | yes, git clone | yes, tarball of the forge | yes, pinned by a lock file with a SHA-256; a moved tag fails by name | T-118 |
| Offline mode, action cache | `--action-offline-mode`, `--action-cache-path` | no | `--offline`, `--frozen`, the store `--actions DIR` | T-118 |
| `--local-repository`, another forge | yes | yes | no | T-139 |
| `actions/checkout` of the local tree | copy of the working tree | local checkout of the changes | `git archive` of HEAD in a git tree, no `.git` | T-092, T-133 |
| Checkout of another repository | real action | `--remote-checkout` | no | T-133 |
| `./` local actions | yes | yes | yes | T-092 |
| Docker actions by image | yes | yes | yes, pinned by digest, through the hooks | T-093, T-098 |
| Dockerfile actions (`docker build`) | yes | yes | refused (`build-not-supported`) | T-124 |
| `container:` | yes | yes | yes, pinned by digest | T-093 |
| `services:` with health checks | yes | yes | refused on the host network | T-123, T-124 |
| Images by tag, pull | yes, `--pull` | yes | no: digest in the store only | T-130 |
| Images by `runs-on` label | `-P LABEL=IMAGE` | `-P LABEL=IMAGE` or the host | one `--image` | T-129 |
| Another architecture | `--container-architecture` | yes | host only | T-151 |
| Reusable workflows | local and remote | yes, `secrets: inherit` | refused | T-131 |
| Event name | argument, `--detect-event` | `--event` | `--event` | T-089 |
| Event payload | `-e FILE` | `--payload` | no `github.event` | T-125 |
| `github` facts from the git tree | yes | yes | stand-ins | T-125 |
| Filters of `on:`, a directory of workflows | event names, `-W DIR` | yes | one file, no filter | T-125 |
| Secrets | `-s`, `.secrets`, mask | `--secret`, `--secret-file`, mask | none | T-121 |
| `GITHUB_TOKEN` | the token of the GitHub CLI of the user | configured | none | T-122 |
| Vars | `--var`, `.vars` | `--var`, `--var-file` | `--var` | T-089, T-126 |
| Inputs, defaults of `workflow_dispatch` | `--input`, `.input` | `--input`, `--input-file` | `--input`, no default, no type | T-126 |
| Extra environment | `--env`, `.env` | `--env`, `--env-file` | no | T-126 |
| Environments, their secrets and vars | name only | yes | ignored | T-136 |
| Concurrency groups | no | yes | ignored | T-137 |
| Artifacts v4 | yes | yes | 404 | T-119 |
| Cache of `actions/cache` | v1 server | v1 and v2 | none | T-120 |
| OIDC id-token | no | yes | none | T-140 |
| One job, a matrix filter | `-j`, `--matrix` | `--job`, `--matrix` | no | T-127 |
| List, graph, dry run, validate | yes | `--list` | no | T-128 |
| Parallel jobs | `--concurrent-jobs` | `--parallel N` | one at a time | T-132 |
| Live logs, log files, JSON | yes | web console, `--log-output-dir` | one line for each job; logs of failed jobs at the end | T-134 |
| Web view | no | yes | no | T-149 |
| Reruns | no | `rerun --failed`, keep artifacts | no | T-135 |
| Watch, schedules | `--watch` | `--watch`, schedules | no | T-142 |
| Configuration file | `.actrc`, trust of the project file | `.actrc` | no | T-143 |
| Keep a container, bind the tree, tool cache | `--reuse`, `--bind` | `--keep-container`, shared tool cache | no | T-138 |
| Pause before a step | no | no | no | T-150 |
| Host check, bug report | `--bug-report` | no | `doctor` for `serve` only | T-153 |
| Interrupt | cancels and removes | cancels | leaves units and scratch files | T-141 |
| `env:` of one expression | not read | yes | refused | T-152 |
| Privileged containers, host binds, cap add | flags, with a policy | yes | refused by the hook policy; privileged work runs in the engine of the unit | T-124 |
| Azure Pipelines YAML | no | a subset | no | T-148 |

## The faults that the study reproduced

| Fault | In podrunner | Entry |
| --- | --- | --- |
| .NET dies with SIGXFSZ under a small file size limit, with W^X on | the podvm driver sets `DOTNET_EnableWriteXorExecute=0`; the `--runner-dir` executor does not | T-144 |
| The artifact step fails with ECONNRESET, then 403: no route of the results service | the fake broker gives 404 in silence for each route that it does not serve | T-145, T-119 |
| A rerun fails with EACCES on files that a container wrote as another uid | the `--runner-dir` executor ignores a failed removal of `_work`; the residue of a unit after a container job is not measured | T-146 |

The study's conformance suite has defects (T-046) and no licence. T-147
writes a new suite of the same scope.

## Where podrunner exceeds them

- **A fresh unit for each job.** act shares one Docker daemon and its
  containers; runner.server can run on the host. Each job of podrunner
  starts from a clean unit and ends with `destroy`; residue fails the job.
  A reuse never carries state from one job into the next (T-138).
- **No root and no daemon.** No Docker socket of the host reaches a job.
  `services:` and `docker build` run in an engine inside the guest of the
  job (T-124, `docs/container-engine.md`).
- **The egress hold.** A step reaches only the hosts of `--allow`, through
  the egress module. A secret or a real token cannot leave to another host
  (T-121, T-122). An environment can have its own egress policy (T-136).
- **Pins.** Actions are pinned by commit and image by digest, recorded in a
  lock file with a SHA-256 (T-118, T-130). A moved tag fails by name. The
  host fetches; a unit never fetches.
- **No listener on the network.** The broker, the artifact store and the
  cache listen on the loopback only. act binds its cache server to the
  outbound address of the host by default.
- **Secrets.** No secret in argv (act takes `-s NAME=VALUE`). A secret file
  needs mode 0600. No real token by default (act reads the token of the
  GitHub CLI of the user).
- **The official runner.** The steps run in the runner of GitHub, pinned
  by version and digest; act reimplements the engine.
- **Static binaries.** One static `podrunner`; runner.server needs .NET.
- **Guest kinds.** With T-123 a job runs in a microvm guest with its own
  kernel and no NIC, or in a system container of incus.
- **Integrity.** Artifacts and cache entries carry their digest; the cache
  keeps the scope of a ref (T-119, T-120).

## Generic work for podcrates

Each ask goes to podcrates as a COORD issue when its entry starts:

- the resolve of an action reference and a store of tarballs by digest
  (T-118), for example `pod-actions-store`;
- the artifact and cache services of the results API (T-119, T-120), for
  example `pod-actions-results`;
- the filters of `on:` and the expansion of a called workflow, in
  `pod-actions-workflow` (T-125, T-131);
- the registry client `pod-oci` for images by tag (T-130).
