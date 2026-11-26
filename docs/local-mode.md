# The local mode against act and runner.server

## Quick start

`docs/install.md` gives the installer: podrunner, podvm, podnet, the
official runner, the runner image in a podvm store, and the file of the
user `run.toml`, each checked against its pin. Take it from a release that
attaches `podrunner-install.sh`:

```sh
curl -fsSLO "https://github.com/PodUtils/podrunner/releases/download/vVERSION/podrunner-install.sh"
sh podrunner-install.sh
cd YOUR-PROJECT && podrunner run .github/workflows/ci.yml --checkout .
```

## A host with a proxy

A job on `--runner-dir` takes `PATH` and these variables of the
environment of podrunner: `HTTPS_PROXY`, `HTTP_PROXY` (and their lower
case forms), `SSL_CERT_DIR`, and the ten CA variables of
`pod_broker::ca::env` of podcrates: `SSL_CERT_FILE`, `CURL_CA_BUNDLE`,
`REQUESTS_CA_BUNDLE`, `NODE_EXTRA_CA_CERTS`, `GIT_SSL_CAINFO`,
`AWS_CA_BUNDLE`, `DENO_CERT`, `HEX_CACERTS_PATH`,
`CLOUDSDK_CORE_CUSTOM_CA_CERTS_FILE` and `CARGO_HTTP_CAINFO`. A JVM
reads none of them: the job also gets the trust store of each CA file
(PKCS #12, `_work/_home/podrunner-ca.p12`), and `JAVA_TOOL_OPTIONS` names
it. A runner directory with a space in its path gets no store (the JVM
splits the variable at each space), and one line on stderr names the
cause (T-598). Its
`NO_PROXY` always holds the loopback, where the broker of the run is.
It does not hold an entry of colons only (`::`, `:`): such an entry has no
host, and the official runner stops at its start on it. One line on stderr
names each such entry once for the run (T-593).
Node reads `NODE_EXTRA_CA_CERTS` and never `SSL_CERT_FILE`, so an action
of Node (`actions/setup-go`, `actions/setup-python`) fails with
"self-signed certificate in certificate chain" behind a proxy that has its
own CA. With `SSL_CERT_FILE` and no `NODE_EXTRA_CA_CERTS` in the
environment of podrunner, a job gets `NODE_EXTRA_CA_CERTS` with the file of
`SSL_CERT_FILE`; a value that you set has priority (T-642).
`podrunner doctor --run` names the variables that a job takes, and the CA
file that Node gets (T-695). Set each
one before the run; give another variable with `--env NAME=VALUE`. With a
proxy variable, a job also gets `NODE_USE_ENV_PROXY=1`, so that `fetch` of
Node uses the proxy, and a `node20` action runs on Node 24, as on GitHub
(T-299).

A job in a podvm unit with no `--podnet` (the unit has the network of the
host) takes the same proxy variables; each CA file goes into the unit as a
file of the runner, and its variable names that file. A JVM (Maven,
Gradle, sbt) reads none of them: the unit also gets the trust store of
each CA file (PKCS #12, `podrunner-ca.p12` beside them), and
`JAVA_TOOL_OPTIONS` names it
(`-Djavax.net.ssl.trustStore=/podrunner/.runner/podrunner-ca.p12
-Djavax.net.ssl.trustStoreType=PKCS12`). The job does not get the
`JAVA_TOOL_OPTIONS` of the environment of podrunner: it is not one of the
variables above, and it can hold a password (`-Dhttp.proxyPassword=`). A
job, a step or `--env` that sets `JAVA_TOOL_OPTIONS` replaces the value:
add the two options to it (T-577).

A container of such a unit (a `container:` job, a service, a Docker
action) gets one bundle of the CA files, read-only at
`/__podrunner/podrunner-ca.pem`, and each of the ten CA variables names
it, unless the job sets the variable for the container (T-596). The
bundle holds each CA file once. A CA file with no certificate stays out
of the bundle and of the trust store, and one line on stderr names it.
Each container also gets the trust store of the bundle at
`/__podrunner/podrunner-ca.p12`, and `JAVA_TOOL_OPTIONS` names it. A
value of `JAVA_TOOL_OPTIONS` that the job sets for the container comes
after the two options: the JVM takes the last value of a property, so a
store that the job names keeps its priority (T-616).

A job on `--runner-dir` gets a scratch `HOME` (`_work/_home` of the runner
directory), which the next job removes. It runs as the user of podrunner,
with the files and the network of that user: it is not a unit.

The work directory of the runner is `_work` of the runner directory. When
the path of the runner directory has a name that starts with `.` (the
installer puts it in `~/.local/share/podrunner`), the work directory is
`podrunner-work/NAME-HASH` in the directory before that name (for example
`~/podrunner-work/2.338.2-1a2b3c4d`). Some tools refuse a path with such a
name: the `send` module of Node gives 404 for it (T-255).

In a unit (`--podvm`, `--microvm`, `--incus`), the work directory is
`/home/runner/work`, the path of a hosted runner: `GITHUB_WORKSPACE` is
`/home/runner/work/REPO/REPO` (T-590). `--export`, `--toolcache` and the
container hooks use the same directory.

On `--runner-dir`, the Pythons of `actions/setup-python` find
`libpython` at the path of the tool cache of GitHub
(`/opt/hostedtoolcache`), which a user with no root cannot make. A step
that runs the Python of the tool cache with `update-environment: false`
then fails with `error while loading shared libraries: libpython3.X.so`.
Give the path of the libraries:
`--env LD_LIBRARY_PATH=WORK/_tool/Python/VERSION/x64/lib` (WORK is the
work directory above), or keep `update-environment` on (T-306).

## The runner images

The runner images are Ubuntu 24.04 with the official runner, not the
image of `ubuntu-latest`. A job that needs a tool that they do not have
installs it in a step, or runs on `--runner-dir` with the tools of the
host (T-315). The images have `sudo` with no password, as ubuntu-latest,
and it works in a guest of `--microvm` or `--incus`. In a podvm unit it
fails: a unit of a user with no root maps one uid (the user of the job,
or no user namespace at all), so no process becomes root; install a tool
into a directory of the job, or use a guest.

| Image | Tools |
| --- | --- |
| base (`runner-ARCH.tar`) | bash, curl, git, jq, make, python3, libatomic1, sudo (no password; a guest only), tar, gzip, unzip, zstd, iptables |
| larger (`runner-full-ARCH.tar`, `--full`) | the base, and `images/runner/full.txt`: build-essential, cmake, pkg-config, libssl-dev, python3 with pip and venv, nodejs, npm, zip, xz-utils, bzip2, rsync, openssh-client, wget, file, patch |

The base image has no C compiler: a project that builds a C extension
(a Python package such as pallets/markupsafe) needs the larger image
(`--full`) (T-326). The larger image has the static Docker Engine, and a
guest of `--microvm` or `--incus` starts it (T-672,
`docs/container-engine.md`); the base image has no Docker. Neither image
has Rust (`cargo`, `rustup`), Go, Java, .NET or yarn: the `setup-*`
actions install a toolchain into the tool cache of the job. In a podvm unit on the namespace rung of podvm (a user with no
root on a host with user namespaces), a job runs as the user `runner` of
the image with `HOME=/root`, in a user namespace of one uid; `sudo` of the
image does not work there (T-317).

## The disk of a unit

A podvm unit unpacks its image under `TMPDIR` at its start: about 2.5 GiB
for the base image and 4 GiB for the larger one, for each job at a time
(`--jobs`). The unpack takes half a minute or more; the line
`unit u-N starts` names it. `podrunner doctor --run` gives the need of the
image of the run (T-325).

Each run keeps its files in two directories of `TMPDIR`,
`podrunner-run-tree-ID` and `podrunner-run-units-ID`, at mode 0700, and
removes them when it ends. `ID` is `NS-PID-N`: a key of the host and of
the pid namespace of the run, its pid, and a count. The key is FNV-1a of
the machine id (`/etc/machine-id`) and the inode of the pid namespace
(`/proc/self/ns/pid`). When there is no machine id (no file, or not 32
hex digits), the key uses the boot id of the kernel
(`/proc/sys/kernel/random/boot_id`). With neither, the name is
`xNANOS-PID-N`, and the run ends no other run.

When a run was killed, the next run of the same user with the same key
ends the units of the dead run, removes its directories, and writes one
line for each. The machine id stays the same after a reboot, so a run
that was killed before a reboot of this host is also found and removed.
A run of another pid namespace (another container that shares `TMPDIR`)
or of another host (a `TMPDIR` on NFS or in a shared home) is never
ended. A limit: two hosts with one copied machine id (a cloned image)
that share `TMPDIR` are not told apart. Give each such host its own
`TMPDIR`. A directory of an older release stays: remove it by hand
(T-312, T-607, T-615).

### The disk of a guest

A guest of `--microvm` (and of the `microvm` kind of `serve`) has no disk:
its root is the image, unpacked into the memory of the guest (an
initramfs on a tmpfs). The work folder `/home/runner/work`, the tool
cache and `/tmp` are on that root, so the space of a job comes from the
memory of the guest (`mem`, 8G by default). The kernel caps the root at
half of the memory less the initrd. The guest script raises the cap to
75 % of the memory of the guest, so the processes of the job keep at
least a quarter (T-688).

Measured in a guest of 8G (podvm `0.2.0-beta.40`, TCG, on a development
host):

| Image | Root used | Free, kernel cap | Free, 75 % cap |
| --- | --- | --- | --- |
| larger, before the engine (1537 MiB unpacked) | 1685 MiB | 1512 MiB | 4280 MiB |
| larger, with the engine (1762 MiB unpacked, T-672) | 1909 MiB | 1175 MiB | 4055 MiB |

A job that writes more than that gets `ENOSPC`. Use the base image when
the job does not need the tools of the larger one, or give the guest more
memory: `--mem SIZE` of `run --microvm` (a size as `8G`; K, M, G or T),
or `[unit.microvm] mem` on `serve` (T-690). `run` refuses a `--mem` that
keeps less than 2 GiB of root for a job with no image (below about
2731M). podvm gives the stored size of an image, not its unpacked size.
When `--microvm` is `DATA/bin/podvm` of an install and `DATA/serve.json`
(each install writes it) names the digest of `--image`,
`run` takes `unpacked_bytes` of that file: it refuses a `--mem` that does
not boot the image or keeps less than 2 GiB of root for its job, with the
fix (T-698). For each
other image the size is not known and not checked: a larger image leaves
its job less. `doctor --run` names the memory and the root of a guest,
and says whether it knows the unpacked size of the image. `check` and `doctor` refuse a pin of `serve` that leaves less
than 2 GiB of root for its job (T-689). The estimate, 75 % of `mem` less
the unpacked image, is about 330 MiB above the measure: the root holds
more than the unpacked bytes.

### The image cache

The first podvm run of an image saves it from the store of `--images` as
`~/.cache/podrunner/image-tars/DIGEST.tar` (`$XDG_CACHE_HOME` when it is
set), about 300 MB for the base image. Each later run loads that tar and
saves nothing. The cache keeps the four tars that runs used last, and
removes the others at the start of a run. A save writes
`DIGEST.part-PID` and renames it at its end; a part file older than an
hour is of a save that stopped, and a run removes it (T-242, T-630).

A tar that does not load and is not whole (a cut save, a full disk, a
change on the disk) is removed, and the run saves and loads the image
again, once. Runs that share the cache repair a tar one at a time (a
lock on `DIGEST.lock`), and a run loads a tar that another run saved
again; it does not remove it. When that fails too, the error names the
tar and the fix `rm PATH`. `podrunner doctor --run` checks the tar of `--image` (the
SHA-256 of each blob, and each blob that the manifest names) and says
`FAIL image-cache: PATH is not whole (...); fix: rm PATH`; an `--image`
with no `@sha256:` digest has no tar in the cache, and the check says so. To clear the
cache, remove the directory: the next run saves the image again.

## A host that refuses each AF_INET bind

On a host whose sandbox refuses `bind(2)` (and `connect(2)`) of AF_INET,
also on the loopback, the fake broker of `--podvm` cannot listen, and a
unit cannot reach it. Give `--vnet PATH` with `--podvm` (T-220):

* The broker of each job listens on a unix socket only. The front of the
  socket directory of the unit gives the broker, and the hosts of
  `--allow` through the egress module; each other host gets 403.
* The runner of the unit runs under the vnet shim of podnet
  (`libpodnet-vnet.so`, podnet `0.4.0-beta.14` or later). The shim sends
  a connect to `127.0.0.1:3128` to the front
  (`PODNET_VNET_FORWARD`), and the proxy variables of the unit name that
  address.
* PATH is the shim, or a podnet binary that builds it with
  `podnet net shim` (a C compiler on the host). The shim loads in the
  runner image: build it for glibc.
* The shim reaches the unit in its socket directory: podvm must give the
  socket volume (its supervise, userland or namespace rung; `podrunner
  doctor --run` names the rung). The unit mounts that directory at its
  host path, and `LD_PRELOAD` and the forward name that path: on the
  userland rung of podvm the loader of the image runs on the host, before
  a path of the unit exists (T-561). `--vnet` and `--podnet` exclude each
  other.

Proof level: in CI on GitHub-hosted runners (the lane `bind-denied` of
`user-gate.yml`: `hello.yml` passes with `--vnet` in `podvm lab run
bind-denied,getsockname-denied` as a user with no root), and on a
constrained host as a user (T-561), also on the namespace rung of podvm
(T-564; the lane `userns-on` measures it in CI).

## What a podvm unit holds

A podvm unit is a boundary for files only on the namespace rung of podvm:
a mount namespace of its own, where the job sees only the image and its
work directory. On that rung a unit also has a PID namespace and a
`/proc` of its own (podvm `0.2.0-beta.30` or later): a job sees only the
processes of its unit (T-318). podrunner turns off the source fixup of podvm
(`--no-source-fixup`, podvm `0.2.0-beta.30` or later): a unit asks no
host of the package sources of the image before the job (T-332). Where podvm cannot take that rung (a host
with no user namespace, or with user namespaces that AppArmor restricts,
as on Ubuntu 24.04), podvm runs the unit on a userland rung: a static
binary of a job (Go up to 1.20, for example) can reach the files of your
user, and a job sees the processes of the host and can send a signal to
each process of your user.
`podrunner doctor --run` names the rung of the host. Run jobs that you do
not trust with `--microvm` (a guest under KVM, no root needed), or on a
host where the rung is `namespace` (T-300).

## The comparison

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
| Isolation | a container of the Docker daemon of the host | a container of the host's Docker, or the host | a fresh unit for each job (podvm, microvm, incus), no root, no daemon; the isolation is that of the kind: a guest of `microvm` or an incus instance isolates, and the userland rung of podvm (a host with no user namespace) gives a fresh root but no isolation |
| Network of a job | the Docker network, open | open | the egress hold: only the hosts of `--allow` |
| Binary | static Go | .NET, not static | static musl |

## The features

The column "podrunner" gives the state of `main`. The last column names the
entry that did the work or that closes the gap.

| Feature | act | runner.server | podrunner | Entry |
| --- | --- | --- | --- | --- |
| `needs`, outputs, `if:`, matrix with `include` and `exclude` | yes | yes | yes | T-087, T-088 |
| `fail-fast`, job `continue-on-error` | yes | yes | yes | T-117 |
| The `github` context in the steps | yes | yes | yes, the context of the run over the canned one | T-114 |
| `run:` steps, `defaults.run`, `shell`, `working-directory` | yes | yes | yes | T-088 |
| Composite and JavaScript actions by commit | yes | yes | yes, from `--actions DIR` | T-091 |
| Actions by tag or branch, fetched | yes, git clone | yes, tarball of the forge | yes, pinned by a lock file with a SHA-256; a moved tag fails by name | T-118 |
| Offline mode, action cache | `--action-offline-mode`, `--action-cache-path` | no | `--offline`, `--frozen`, the store `--actions DIR` | T-118 |
| `--local-repository`, another forge | yes | yes | yes: a made-up commit of the directory, never in the lock; `--action-server URL` for any git server | T-139 |
| `actions/checkout` of the local tree | copy of the working tree | local checkout of the changes | the working tree with its changes, not the ignored files; `--checkout-head`; a `.git` in a git tree (`--no-git` leaves it out) | T-092, T-133, T-267 |
| Checkout of another repository | real action | `--remote-checkout` | from the store at the commit of the lock | T-133 |
| `actions/checkout` of the same repository with `ref:` | real action | local checkout | the commit of the ref in the local git of `--checkout` (a branch, a tag, a remote branch such as `origin/main`, a commit id), with a `.git` at that commit; the local tree when the ref is the commit of HEAD; a ref with no commit fails its step with the name and the fix (`git fetch origin REF`); a ref that starts with `-` fails it too; an expression (`${{ }}`) gives the local tree with one line on stderr; `--dry-run` names each ref; no submodules for another commit | T-640 |
| Keys of `with:` of `actions/checkout`: `fetch-depth`, `filter`, `sparse-checkout`, `sparse-checkout-cone-mode`, `clean`, `lfs`, `token`, `ssh-key`, `persist-credentials` | real action | local checkout | ignored: the stand-in gives the local tree (or the commit of `ref:`) and fetches nothing; the run names each key once on stderr, also with `--dry-run`, and the display of the step lists them; the tree has the whole `.git` with `--with-git`, else no `.git` | T-643 |
| `./` local actions | yes | yes | yes | T-092 |
| Docker actions by image | yes | yes | yes, pinned by digest, through the hooks | T-093, T-098 |
| Dockerfile actions (`docker build`) | yes | yes | yes, in a `microvm` guest with an engine (`--engine DIR`); refused on the other unit kinds; a Dockerfile action on `--podvm` fails at its step, not before the job (T-237) | T-124 |
| `container:` | yes | yes | yes, pinned by digest | T-093 |
| `services:` with health checks | yes | yes | yes, in a `microvm` guest with an engine: a service answers by `localhost` and by its name; refused on the other executors | T-123, T-124 |
| Images by tag, pull | yes, `--pull` | yes | yes: `podvm pull` into the store, pinned by the lock; the units never pull | T-130 |
| Images by `runs-on` label | `-P LABEL=IMAGE` | `-P LABEL=IMAGE` or the host | `--platform LABEL=IMAGE` (pinned) or `host`; no fallback to an image the user did not name. The larger image (`images/runner/full.txt`: C and C++, CMake, Python, Node.js) is the release asset `runner-full-ARCH.tar`: `sh podrunner-install.sh --full` loads it and uses it for each job; by hand, load it into the store of the installer (`PODVM_STORE=~/.local/share/podrunner/images podvm load -i runner-full-ARCH.tar`), read its digest with `PODVM_STORE=~/.local/share/podrunner/images podvm images --digests`, then `--platform ubuntu-latest=runner@DIGEST` (T-307) | T-129 |
| Another architecture | `--container-architecture` | yes | host only | T-151 |
| Reusable workflows | local and remote | yes, `secrets: inherit` | yes: local and remote (pinned by the lock), inputs, passed or inherited secrets, outputs, matrix and nested calls | T-131 |
| Event name | argument, `--detect-event` | `--event` | `--event` | T-089 |
| Event payload | `-e FILE` | `--payload` | `--event-path FILE`, or a minimal payload of the event | T-125 |
| `github` facts from the git tree | yes | yes | yes, and flags over each fact | T-125 |
| Filters of `on:`, a directory of workflows | event names, `-W DIR` | yes | yes: the filters of `on:` (`pod-actions-workflow`), `--changed-since`, a directory of workflows | T-125 |
| Secrets | `-s`, `.secrets`, mask | `--secret`, `--secret-file`, mask | `--secret NAME` from the environment, `--secret-file` at mode 0600, `.secrets` of the checkout, else of the current directory (never copied into the workspace; checked as a secret file, T-341), mask; no value in argv | T-121, T-276, T-341 |
| `GITHUB_TOKEN` | the token of the GitHub CLI of the user | configured | a made-up token; an action that gives it to a real API fails with "Bad credentials", and the run names `--github-token-env` (T-254); a real one only by `--github-token-env NAME`, never to `permissions: {}` | T-122 |
| Vars | `--var`, `.vars` | `--var`, `--var-file` | `--var`, `--var-file` | T-089, T-126 |
| Inputs, defaults of `workflow_dispatch` | `--input`, `.input` | `--input`, `--input-file` | `--input`, `--input-file`; the defaults, types, `required` and `choice` of GitHub | T-126 |
| Extra environment | `--env`, `.env` | `--env`, `--env-file` | `--env`, `--env-file` | T-126 |
| Environments, their secrets and vars | name only | yes | secrets, vars and an egress policy for each environment; `--protected NAME` with `--approve NAME` or a prompt on the terminal | T-136 |
| Concurrency groups | no | yes | held across runs by a lock file; `cancel-in-progress` cancels the older run | T-137 |
| Artifacts v4 | yes | yes | yes, one store for each run, size and SHA-256 checked at finalize; v3 refused by name | T-119 |
| Cache of `actions/cache` | v1 server | v1 and v2 | v2, a store that stays across runs, Range downloads, a limit of 10 GB | T-120 |
| OIDC id-token | no | yes | yes: ES256 with one key for each run, in memory only; `--oidc-issuer`, `--oidc-keys` | T-140 |
| One job, a matrix filter | `-j`, `--matrix` | `--job`, `--matrix` | `--job` with its needs, `--matrix`; each other cell `skipped` with its reason | T-127 |
| List, graph, dry run, validate | yes | `--list` | `--list`, `--graph`, `--validate`; `--dry-run` builds each job message and checks the pins | T-128 |
| Parallel jobs | `--concurrent-jobs` | `--parallel N` | `--jobs N` with a unit kind, `max-parallel`, `fail-fast` stops the running cells | T-132 |
| Live logs, log files, JSON | yes | web console, `--log-output-dir` | live lines on stderr, `--json`, `--log-dir` with each step log and `summary.md`; secrets masked again. A step in a `container:` job gives its lines while it runs: the hook of the job container sends the log of the step as it grows, at most 50 ms late (T-694). A container action (`run_container_step`) gives its lines at its end | T-134 |
| Web view | no | yes | yes: `--web IP:PORT`, the loopback only, a random token in the path, the page in the binary | T-149 |
| Reruns | no | `rerun --failed`, keep artifacts | `--state DIR`, `--rerun DIR [--failed \| --job ID]`, kept results, outputs and artifacts; a changed workflow fails by name | T-135 |
| Watch, schedules | `--watch` | `--watch`, schedules | `--watch` (a change cancels the run in progress), `--schedule` (the crons of `on.schedule`, UTC) | T-142 |
| Configuration file | `.actrc`, trust of the project file | `.actrc` | `run.toml` of the user and of the project, with `trusted`; the flags of `.actrc` that podrunner knows | T-143 |
| Keep a container, bind the tree, tool cache | `--reuse`, `--bind` | `--keep-container`, shared tool cache | `--export DIR` on `--runner-dir` and `--podvm`; `--toolcache DIR` on `--podvm` (a tree of links into `/home/runner/work/_tool`; the directory is mounted read-only where the rung of podvm has mounts); `--keep-failed` keeps a failed job at its failed step | T-138 |
| Pause before a step | no | no | `--break JOB:STEP`, `--break-on-failure`: the job waits for Enter; `s` opens a shell in a podvm unit or an incus instance | T-150 |
| Host check, bug report | `--bug-report` | no | `doctor --run FILE [the options of run]`: each input checked, with its fix | T-153 |
| Interrupt | cancels and removes | cancels | cancels: each unit is destroyed with its containers (also a `container:` job in a long step: the unit goes before the hooks stop, and the run waits at most 10 s for a hook call), the scratch directories go, exit 130; a second signal prints `stopping: already stopping`, a third ends podrunner at once | T-141, T-639 |
| `env:` of one expression | not read | yes | yes: the workflow, the job and the step, as the schema of the runner allows | T-152 |
| Privileged containers, host binds, cap add | flags, with a policy | yes | refused by the hook policy; privileged work runs in the engine of the unit | T-124 |
| Azure Pipelines YAML | no | a subset | no: it needs the Azure Pipelines agent (`docs/azure-pipelines.md`, Q6) | T-148 |

## Faults that the study found

| Fault | In podrunner | Entry |
| --- | --- | --- |
| .NET dies with SIGXFSZ under a small file size limit, with W^X on | each executor sets `DOTNET_EnableWriteXorExecute=0`; a runner that SIGXFSZ ends fails its job with the limit named | T-144 |
| The artifact step fails with ECONNRESET, then 403: no route of the results service | the ArtifactService is served (T-119); a route that the fake broker does not serve is named in the job log and on stderr | T-145 |
| A rerun fails with EACCES on files that a container wrote as another uid | `_work` gets its write bits back before the removal, and a failure fails the job with the path | T-146 |

The study's conformance suite has defects (T-046) and no licence. The
parity suite of T-147 (`crates/podrunner/tests/local/parity`) is its
replacement: each row passes in `microvm` guests with an engine and on a
runner directory in `runner-image.yml`.

## Gaps

- The egress hold of the local mode is the seccomp hold (`advisory`); the
  netns hold waits for a grant of the loopback port of the fake broker
  (T-187).
- `services:` and Dockerfile actions need the `microvm` kind with an
  engine (KVM and QEMU, or podvm's own QEMU).
- A job of another architecture fails by name (T-151).
- A user agent has not run the local mode on the real workflows of other
  projects (GitHub #153, T-199, blocked).

## Where podrunner exceeds them

- **A fresh unit for each job.** act shares one Docker daemon and its
  containers; runner.server can run on the host. Each job of podrunner
  starts from a clean unit and ends with `destroy`; residue fails the job.
  A reuse never carries state from one job into the next (T-138).
- **No root and no daemon.** No Docker socket of the host reaches a job.
  `services:` and `docker build` run in an engine inside the guest of the
  job (T-124, `docs/container-engine.md`).
- **The egress hold.** With a guest of `--microvm` or `--incus` (no NIC,
  one channel), a step reaches only the hosts of `--allow`, through the
  egress module, so a secret or a real token cannot leave to another host
  (T-121, T-122). With `--podvm` and `--podnet`, the hold is the seccomp
  hold of podnet: a direct connect is refused, and so is each port on the
  loopback of the host but the broker of the job, also a proxy there
  (podnet `0.4.0-beta.10` and later, `--loopback-rules`, T-319); it holds
  TCP, not unix sockets (podnet calls it advisory). An environment can have
  its own egress policy (T-136).
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
- **Guest kinds.** With `--microvm` a job runs in a guest with its own
  kernel and no NIC; with `--incus`, in a system container of incus.
- **Integrity.** Artifacts and cache entries carry their digest; the cache
  keeps the scope of a ref (T-119, T-120).
