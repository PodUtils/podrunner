# The configuration: podrunner.toml

`crates/podrunner-slots/src/config.rs` reads this file. An unknown key is
refused. The `microvm` and `incus` kinds also refuse a key of the podvm
kind that they do not read (`registries`, `runner_dir`, `podnet`,
`confine`). A file that lowers the floor of a tier, or that names a `fake`
source or unit with no `allow_fake = true`, is refused. A file that the
group or others can write, or that another user (not root) owns, is
refused: it names the signer of the pin list and the paths of the keys
(T-409).

`docs/examples/podrunner.toml` is a working example (a fake source and fake units); `podrunner check --config FILE` checks a file with no side effect. A test reads each block below and refuses a key that does not exist.

`schemas/config.v1.json` is the JSON Schema of the file (draft 2020-12, T-479), generated from the types of the parser: a program that writes the file checks its output with it (read the TOML as JSON). `scripts/check-config-schema.py` validates the example and each block below against it. A floor, the pin list and the modes of the files are not in the schema: `podrunner check` is the last word.

```toml
host = "host-a"              # the name of the host in the full report
allow_fake = false           # a fake source or unit only with true
terminal_suppress_s = 3600   # a terminal refusal suppresses its job id
same_principal_ok = false    # more than one unit at a time on a shared principal
payload_from_offer = false   # take the payload of an offer when it is pinned
repository = "owner/repo"    # the one repository of the host (DR-4); other offers: repo-scope-refused
max_units = 2                # the most units of the host across its kinds (T-505); absent: the slots of each kind; 0 is refused
upstream_proxy = "http://proxy:3128"  # the way out of the egress module (absent: direct)
upstream_names_only = false  # the proxy takes names only; at the start a canary checks that it refuses
                             # a non-public name, else each hold is advisory and no untrusted job runs
guest_kinds = ["microvm"]    # the kinds whose claim of guest may hold, after the self-test
driver_dir = "/opt/podrunner/drivers"  # each binary and sidecar is in it; absolute paths only

[[source]]                   # one set: one tier, one architecture, this host
name = "org-trusted"
kind = "github-scaleset"     # or "fake", "remote" (the remote mint, below), or "program" (below)
tier = "trusted"             # "untrusted", "trusted" or "local"
arch = "x86_64"              # or "aarch64"
operator_controlled = true   # the full report; else the public report
slots = 4                    # the most units of this source at one time
sidecar = "scaleset-sidecar" # github-scaleset: the sidecar in driver_dir
config_url = "https://github.com/example-org"
scale_set = "podrunner"
app_id = "12345"             # the App: its id, its installation and its key file (mode 0400 or 0600) ...
installation_id = 678
key_file = "/etc/podrunner/app.pem"
# token_file = "/etc/podrunner/org.token"  # ... or a fine-grained token of the organization (mode 0400
                             # or 0600) in place of the three; the sidecar gets it on its descriptor 3

[unit.podvm]                 # one table for each unit kind: fake, podvm, microvm, incus
slots = 2
command = ["./run.sh"]       # the top process of a unit
binary = "/usr/local/bin/podvm"       # podvm: the released podvm binary
images = "/var/lib/podrunner/images"  # podvm: a podvm store with the pinned images
registries = ["ghcr.io"]     # container: hooks: the registry hosts of job containers; none: each is refused
runner_dir = "/home/runner"  # the runner directory in the unit; absent: the path that the driver observes
podnet = "/usr/local/bin/podnet"      # podvm: hold the egress of each unit with podnet pod (netns: enforced, else seccomp: advisory); absent: unknown
disk_max_bytes = 10737418240          # the bytes that a unit may write under its root, measured each 30 s; over it the
                                      # unit ends with the exit reason limit; the lesser of it and a disk limit of the offer holds; absent: the limit of the offer, or no measure
# init = "image"                    # incus and microvm only: the init of the image (systemd) is PID 1 of each unit, with the services
                                      # and the engine of the image (the hosted snapshot, T-543, T-549); not with engine; podvm
                                      # 0.2.0-beta.36 (incus) or 0.2.0-beta.37 (microvm) or later; the kind takes only a payload with
                                      # init = "image" ("Placement by label and by init")
# work_folder = "/home/runner/work"  # podvm, incus and microvm only: the work folder of the runner in each unit, an absolute
                                      # path; podrunner writes it as workFolder into .runner before the start, so GITHUB_WORKSPACE
                                      # is FOLDER/REPO/REPO as on a hosted runner (T-541, T-590); a .runner that is not a JSON
                                      # object refuses the unit; absent: /home/runner/work (podvm: work in its runner_dir); podvm:
                                      # a folder in runner_dir (not bin, externals, hooks or a dot name), as the container hooks
                                      # mount only there; "/home/runner/_work" gives the folder of the mint
# root_quota = true                  # incus only: the pool of incusd holds a size of a root disk (btrfs, zfs, lvm; not dir): each
                                      # instance gets a root disk of the lesser of disk_max_bytes and the disk limit of the offer (64 MiB at
                                      # least), a job past it gets ENOSPC, and the kind says `disk: enforced` (T-482); podvm refuses it on dir;
                                      # check refuses a pin whose unpacked_bytes is over disk_max_bytes (T-551)
# claim = "confined"                 # fake only, with allow_fake: the fake kind claims confined or guest and an enforced hold, so that
                                      # a test drives a trusted or untrusted forge source through serve (T-511); it holds nothing
confine = false                       # podvm: each unit and container under the confine layer of podvm (--podvm-confine); the kind says confined after its self-test

[tier.trusted]               # a floor at or above the lowest floor
min_boundary = "confined"

[trust]                      # the tier rule (crates/podrunner-tier)
organizations = [{ login = "example-org", id = 11 }]
users = [{ login = "example-user" }]   # repositories of a person, for a forge that has them
                             # (a `program` source); the GitHub scale set takes organizations only
trusted_public = [{ owner = "example-org", repo = "open-tool" }]
extra_events = ["release"]   # more events of a trusted job: only workflow_run, issue_comment,
                             # release, repository_dispatch (the owner, Q5) and pull_request_target
                             # (Q12); default none. Each needs the facts of a trusted job: a configured
                             # owner, a head that is not a fork, an actor member. A pull_request_target
                             # job runs the workflow of the base branch of its pull request and gets
                             # the secrets of the repository: check and serve print one line that
                             # names this opt-in (T-518)

[deadlines]                  # seconds; these are the defaults
prepare_s = 600
claim_s = 60
mint_s = 60
start_s = 120
                             # an emulated host (an aarch64 VM under TCG) starts the runner in about
                             # 3 minutes: give it start_s = 900 (T-197)
running_s = 21600
grace_s = 30
destroy_s = 60
report_s = 60

[pin]
max_age_s = 2592000          # an older payload is image-stale
signer = "HEX64"             # the Ed25519 key of the pin list (podrunner pin-sign)
signature = "HEX128"         # its signature; a changed pin list is refused
[[pin.payload]]
arch = "x86_64"
reference = "runner@sha256:HEX"
built_at_s = 1763337600      # the time of the build, Unix seconds; a time after the clock is stale
runner_version = "2.338.2"   # the official runner in the image
max_age_s = 2419200          # the age limit of this pin; the smaller of it and [pin] max_age_s holds
# sandbox = true             # the image of a sandbox unit (T-506): only an offer with sandbox.image of this digest
                             # takes it, at untrusted (a guest); a runner job never does
unpacked_bytes = 59000000000 # the bytes of the image unpacked (podrunner-manifest.json of a release, T-572; or du -sxb / in a
                             # unit of it); check and doctor refuse a pin above the root disk of [unit.incus] (root_quota,
                             # disk_max_bytes, T-551) or above (mem - 512 MiB) x 4 / 13 of [unit.microvm] (T-573); absent:
                             # not checked
# labels = ["ubuntu-latest"] # the runs-on labels that this payload serves, no case; absent: each job (T-549)
# init = "image"             # the payload boots the init of its image (systemd): only a kind with init = "image" takes it

[egress]                     # the egress policy of the operator (T-024, T-107)
# broker-policy.v1 of pod-broker: the policy of each job. The policy of an offer
# can only narrow it. Absent: no way out at all, also for an offer with a policy
# (T-410); to take the policies of a source, set { schema = 1, mode = "open" }.
default_policy = { schema = 1, mode = "allow", allow = [{ host = "*.github.com", ports = [443] }] }

[egress.secrets.github]      # a secret that a credential of a policy names (T-563)
file = "/etc/podrunner/secrets/github"   # the value, mode 0600 or 0400; read at the start of each unit
hosts = ["api.github.com"]   # the hosts that may get the value
sources = ["githome"]        # the sources whose jobs may get it (also from default_policy)
places = ["header", "basic"] # the places of the value in a request (the default); also query, body

[dns]                        # the resolver of the egress and of a remote mint (pod-dns, T-076)
doh = true                   # ask DNS over HTTPS (resolvers by their address) when the system and the cache fail
pins = ["mint.example.org=192.0.2.10"]  # HOST=IP: a pinned name is not asked of the system
```

The resolver asks in this order: an IP literal, the pins, the system
resolver, the addresses that carried a session, DNS over HTTPS to
resolvers by their address, then the old addresses of that cache. A host
whose own resolver fails keeps its egress and its mint.

## The microvm kind

A guest of the machine tier of podvm for each job (T-037): its own kernel,
under KVM where `/dev/kvm` opens. It is the unit of the `untrusted` tier:
it claims `guest`, and `serve` takes the claim only for a kind on
`guest_kinds` and after the self-test of the guest (GitHub #36). Each
entry of `guest_kinds` other than the default `microvm` must name a
`[unit.NAME]` table: `check` refuses another name.

`serve` runs the self-test at its start, then each day and on a change
of the driver file. A test with no verdict (a unit did not start, did
not end in 60 s, or wrote no result) runs once more at once. If it again
has no verdict, the next test is in 2 minutes, then 4, 8, 16, and 30
minutes at most (T-574). A failed verdict waits one day. The line `the
claim of guest drops to none` gives the cause: for a test unit with no
result, its exit code and the end of its log. The scrubbed end of that
log stays in `STATE/guest-test-KIND.log`. Before
each job the guest script checks that each host mount of the guest is in
the table of the unit (`MOUNT_CHECK`); a guest with another mount ends with
exit 96. It has no route of the container hooks. The host needs
`qemu-system-x86_64` (or `-aarch64`) and podvm `v0.2.0-beta.10` or later
(`LOWEST_GUEST`).

The guest has no NIC. Its one way out is a channel of podvm from
`127.0.0.1:3128` in the guest to the egress socket of the unit. The
driver gives the guest the port in `PODRUNNER_CHANNEL_PORT`, and the
script of the guest sets `http_proxy`, `https_proxy` and their upper-case
forms to `http://127.0.0.1:PORT`. So the egress hold is `enforced` with
no `podnet`: a program that does not use the proxy reaches nothing.

```toml
guest_kinds = ["microvm"]
[unit.microvm]
slots = 1
command = ["/home/runner/run.sh"]        # an absolute path: the job runs in a work directory
binary = "/opt/podrunner/drivers/podvm"  # the released podvm binary
images = "/var/lib/podrunner/images"     # a podvm store with the pinned images
mem = "8G"                               # the memory of each guest (default 8G: the initrd of a runner image is about 1.4 GiB);
                                         # podrunner always passes it to podvm. The image is an uncompressed initramfs: podvm needs
                                         # 13/4 x unpacked_bytes + 512 MiB (COORD-219). check refuses a pin whose unpacked_bytes is
                                         # over (mem - 512 MiB) x 4 / 13 (T-573), with the fix: mem of at least 13/4 x N + 512 MiB,
                                         # in whole GiB
cpus = 2                                 # its vCPUs (default 2)
engine = "/opt/podrunner/docker"         # optional: the static Docker binaries; each guest runs an engine
```

With `engine`, each guest loads the modules of an engine
(`--podvm-engine`, podvm `v0.2.0-beta.14` or later), mounts the directory
read-only at `/podrunner-engine`, and starts `dockerd` as root before the
runner. `serve`, `check` and `doctor` refuse an older podvm for `engine`
(`LOWEST_ENGINE`, T-112, T-116). The runner gets its
socket as `DOCKER_HOST`; the socket stays in the guest, and the engine
pulls through the egress channel. The runner image needs `iptables`
(`images/runner/Dockerfile` has it). With `engine` the kind says
`container: engine`, and placement gives a job that needs an engine
(`services:`, a Dockerfile action) only to such a kind (T-103, GitHub #91).
The incus kind takes `engine` too (T-485).

## The incus kind

A system container of the incus tier of podvm for each job (T-105): its
own init, root file system and network namespace, and the kernel of the
host. It claims `confined`, the floor of `trusted`, so it never serves an
`untrusted` job. With no `engine` it has no container route. With
`engine` (podvm `v0.2.0-beta.32` or later, `LOWEST_ENGINE_INCUS`), each
instance runs an engine of its own, as a guest of the microvm kind does
(T-485): podvm sets `security.nesting` and the intercepts of `mknod` and
`setxattr` (`--podvm-engine`), the directory is read-only at
`/podrunner-engine`, `dockerd` starts as root with no bridge and no
iptables (the instance has no NIC), and the runner gets its socket as
`DOCKER_HOST`; the record says `container: engine`. The instance stays
unprivileged, so the kind keeps `confined` (call of 2026-11-18,
`docs/decisions.md`). The host needs an
incusd that the user of podrunner can reach (`PODVM_INCUS_SOCKET` or
`INCUS_DIR` in the environment of `serve`) and podvm `v0.2.0-beta.13` or
later (`LOWEST_INCUS`): the credential files go into the instance on
stdin, and the instance gets its channel with no NIC.

The instance has no NIC (`--podvm-net none`). Its one way out is a
channel of podvm (`--podvm-channel`, no incus `proxy` device) from
`127.0.0.1:3128` in the instance to the egress socket of the unit: podvm
mounts the socket file into the instance and relays to it. The driver sets
the socket to mode 0666, so that the mapped root of the instance can
connect; the socket directory of the unit (mode 0700) keeps each other
user of the host out. The job gets `http_proxy` and `https_proxy` of that
address, as for the microvm kind. So the egress hold is `enforced`, and
the kind serves `trusted` jobs.

```toml
[unit.incus]
slots = 2
command = ["/home/runner/run.sh"]        # an absolute path: the job runs in a work directory
binary = "/opt/podrunner/drivers/podvm"  # the released podvm binary
images = "/var/lib/podrunner/images"     # a podvm store with the pinned images
mem = "8G"                               # the memory of each instance (limits.memory)
cpus = 2                                 # its CPUs (limits.cpu)
pids = 4096                              # its processes (limits.processes)
```

## The limits of a unit (G-12)

Each unit gets the lesser of the value of the offer (`memory` in bytes,
`cpu` in milli-cores, rounded up to whole CPUs, `pids`) and the value of
its kind (the keys below); a value that one side has alone holds (the
owner, 2026-11-15, T-198). The `podvm` kind enforces `memory` and `pids` (and `cpu` where the host delegates a cgroup to the user),
the `incus` kind `memory`, `cpu` and `pids`, and the `microvm` kind the
`memory` and `cpu` of its guest, and `pids` with `[unit.microvm] pids`; placement refuses an offer that needs a
limit that no kind enforces (`limit-not-enforceable`).
On the `incus` kind with `root_quota`, the root disk of a unit is the
lesser of the `disk` limit of the offer and `disk_max_bytes`. Placement
refuses an offer whose root disk is below `unpacked_bytes` of its
payload (`limit-not-enforceable`); the audit line names both sizes
(T-571).

| Kind | `mem` | `cpus` | `pids` |
| --- | --- | --- | --- |
| `podvm` | `RLIMIT_AS` of the payload (`--ulimit as=`); .NET gets a heap limit and a region range of three quarters | `--cpus` in a leaf of a cgroup v2 subtree that the host delegates to the user (a systemd user session; podvm `0.2.0-beta.23` or later, T-221); refused elsewhere, with the reason | `RLIMIT_NPROC` (`--ulimit nproc=`); it counts each process of the user, also of other units of that user |
| `incus` | `limits.memory` (podvm `0.2.0-beta.20` or later) | `limits.cpu` | `limits.processes` |
| `microvm` | the memory of the guest | its vCPUs | `pids.max` of the cgroup of the payload in the guest (`--pids-limit`, podvm `0.2.0-beta.32` or later, T-488); with no `pids` of the kind, none |

A size is whole bytes, or a number with `K`, `M`, `G` or `T`. Set
`[unit.podvm] mem` large: `RLIMIT_AS` counts the address space, which is
more than the memory in use.

## Placement by label and by init

The `runs-on` labels of a job choose its payload (T-549). A
`[[pin.payload]]` with `labels` serves only a job with one of them;
GitHub compares a label with no case. When a payload of the arch names a
label of the job, placement takes the first such payload that is not
stale. Else it takes the first payload with no `labels`. With none, the
job is `payload-not-pinned`.

A payload with `init = "image"` boots the init of its image (systemd of
the hosted snapshot). Placement sends it only to a kind with
`init = "image"`, and a payload with no `init` only to a kind with no
`init`: the unit would fail at its PID 1. When no kind that meets the job
has the `init` of its payload, the job is `boundary-not-available`, and
the audit line names the payload and the `init` that it needs.

| Kind | Boots the init of the image | podvm |
| --- | --- | --- |
| `incus` | with `init = "image"`: `--podvm-init=image --podvm-engine` | `0.2.0-beta.36` or later |
| `microvm` | with `init = "image"`: `--podvm-init=image` on a foreground `run --rm` of the machine tier | `0.2.0-beta.37` or later |
| `podvm` | no: `check` refuses `init` | podvm refuses an init with `--ulimit` and `--podvm-confine` (exit 125), refuses `run -d` with an init on the runsc and machine tiers, and gives a caller of uid 0 no namespace rung; the namespace rung maps one uid |
| `fake` | no: `check` refuses `init` | |

`check` refuses a payload with `init = "image"` when no `[unit.KIND]`
has `init = "image"`, and a label that is empty or has white space. The
signature of the pin list covers `labels` and `init`. The size checks of
T-551 and T-573 read only the payloads of the `init` of the kind, and the
self-tests of a kind use the first payload of its `init`.

```toml
[unit.incus]
slots = 2
binary = "/opt/podrunner/drivers/podvm"
images = "/var/lib/podrunner/images"
init = "image"
command = ["/home/runner/actions-runner/run.sh"]   # the runner of the runner image of the snapshot (T-541)
work_folder = "/home/runner/work"                   # the hosted work folder

[unit.podvm]
slots = 2
binary = "/opt/podrunner/drivers/podvm"
images = "/var/lib/podrunner/images"

[pin]
[[pin.payload]]                          # the runner image of the hosted snapshot: jobs of these labels, on incus only
arch = "x86_64"
reference = "ghcr.io/podutils/podrunner-hosted@sha256:HEX"   # a manifest of hosted-runner.yml
built_at_s = 1763337600
labels = ["ubuntu-latest", "ubuntu-24.04"]
init = "image"
[[pin.payload]]                          # the runner image: each other job, on podvm only
arch = "x86_64"
reference = "runner@sha256:HEX"
built_at_s = 1763337600
```

## The floors of the tiers

| Tier | Lowest floor | Why |
| --- | --- | --- |
| `untrusted` | `guest` | A fork job is hostile code. |
| `trusted` | `redirect` (default `confined`) | DR-4: `redirect` is a choice of the operator. The file must then have a signed pin list (`podrunner pin-sign`), set `repository`, keep `same_principal_ok = false` and set `slots` on each `trusted` source (T-045 has the other mitigations). |
| `local` | `none` | Only the configuration marks a set `local`; its jobs run at `local` with no tier rule, and only a `fake` source can be `local` (call C14). |

## The case table of the tier rule

The release asset `tier-cases.json` is the case table of the tier rule
(T-518): each row gives the trust lists, the tier of the set, the facts of
a job and the result. Its source is `crates/podrunner-tier/tier-cases.json`,
made from `crates/podrunner-tier/tests/cases.json`. A forge with a trust
rule of its own runs each row against it.

`head_is_fork` of a job is `yes` when the head is another repository than
the repository of the job, not the `fork` flag of the head
(`docs/contracts/source-driver.md`).

`docs/design/examples/tier-cases.json` has the same file name and another
format. It is the design copy (call C17): it predates the rulings Q5 and
Q12 and refuses each `pull_request_target`. Do not use it as the table.

## A program source

A forge other than GitHub gives jobs through a program of its own that
speaks the source wire (`docs/contracts/source-driver.md`) on its stdio,
as the scale-set sidecar does for GitHub (T-101). `serve` starts it, and
starts it again with a backoff when it ends.

```toml
[[source]]
name = "githome-trusted"
kind = "program"
tier = "trusted"
arch = "x86_64"
slots = 2
program = "/opt/podrunner/drivers/githome-source"  # absolute, in driver_dir
program_args = ["--deployment", "staging"]
key_file = "/etc/podrunner/githome.token"         # optional, mode 0400 or 0600
trust = { users = [{ login = "alice" }], organizations = [{ login = "acme" }] }  # in place of [trust]
```

The program gets the path of `key_file` in `PODRUNNER_SOURCE_KEY_FILE`,
never its content. A program source is never `local`.

A login is a name of one forge: `alice` of githome is not `alice` of
GitHub (T-494, githome's G-16). `trust` of a `[[source]]` has the keys of
`[trust]` and holds in place of it for the offers of that source: the
placement, the question of the job-started hook (T-493) and a remote mint
read it. `check` refuses a `[trust]` that names an owner when a `program`
source shares it with another source: give each one its own `trust`.

One source can carry the jobs of two forges. Then `trust` is a list, one
table for each forge, and an offer takes the table of its
`trust_inputs.forge` (the host name of the forge, in any case); an offer
with no forge, or with a forge that the list does not name, is refused
(`tier-mismatch`). A remote mint takes one table only.

```toml
[[source]]
name = "forges"
kind = "program"
tier = "trusted"
arch = "x86_64"
program = "forge-source"
trust = [
  { forge = "github.com", organizations = [{ login = "acme" }] },
  { forge = "githome.example", users = [{ login = "alice" }] },
]
```
`podrunner fake-source --script FILE` is a program of this kind over the
fake source: a forge checks its own program against it.

## The attach of a sandbox

An offer with `sandbox { image, command, idle_s, max_s }` runs `command`
in a guest of `image`, a `[[pin.payload]]` with `sandbox = true` (T-506).
The unit has no inbound port. The payload attaches to its source (a
terminal, an agent task) over an outbound connection, for example a
WebSocket, through the egress module of the unit. The payload gets the
module as `http_proxy` and `https_proxy`.

* The client opens a tunnel with `CONNECT HOST:PORT` and then sends the
  upgrade of the WebSocket (`ws://`) or the TLS handshake (`wss://`) in
  the tunnel. A plain request through the proxy loses its `Upgrade` and
  `Connection` headers, so it gets no upgrade.
* The `policy` of the offer names the host and the port of the URL, for
  example `{ "schema": 1, "mode": "allow", "allow": [{ "host":
  "attach.githome.example", "ports": [443] }] }`. The `[egress]
  default_policy` of the operator must also allow them: `mode = "open"`
  takes the policy of the offer. A rule holds a host and ports only: an
  offer with a path rule is refused (`egress-policy-unsupported`).
* The egress module refuses a private or a loopback address, whatever
  the policy.
* The attach is one long tunnel: podrunner sees no idle time in it. The
  payload ends itself at `idle_s` (`PODRUNNER_IDLE_S`); podrunner ends
  the unit at `max_s`.

The user-path gate tests this on a microvm unit (`serve:run:ws:microvm`
of `scripts/user-gate.py`): the client of `scripts/ws-client.py` holds a
WebSocket to `scripts/ws-echo.py` and gets a 403 for a port that the
policy of the offer does not name.

## Placeholder credentials

A policy can name `credentials` (`broker-policy.v1` of `pod-broker`): each
has a `placeholder`, the name of a `secret`, its `hosts` and its places
(`in`: `header`, `basic`, `query`, `body`). The unit holds the placeholder
only. The broker of the unit ends the TLS of each tunnel to a host of a
credential, puts the value of the secret in place of the placeholder, and
puts the placeholder back in each answer. The value comes from the file of
`[egress.secrets.NAME]`; it is not in the unit, in the policy, in a line of
the audit or in the text of an error.

* An offer whose credential names a secret that this host does not hold,
  a secret that is not for its source, or a host that is not one of the
  `hosts` of the secret is refused: `egress-policy-unsupported`, and the
  audit line names the secret and the host.
* A credential of `default_policy` names a secret of `[egress.secrets]`;
  `check` refuses another one. It holds for a job of the `sources` of its
  secret only: a job of another source is refused
  (`egress-policy-unsupported`), and the audit names the secret.
* The `in` of a credential (none: `header` and `basic`) is in the
  `places` of its secret, else the offer is refused. A value in a `body`
  or a `query` can reach a host that keeps the text and shows it again.
* A job with a credential goes only to a kind whose unit is apart from the
  user of `serve`: a boundary of `confined` or more, or a distinct user.
  That user reads the files of the secrets and the memory of the broker.
* The broker scrubs the value, its percent form, its JSON form and its
  base64 in each answer. A host that gives back a changed form of the
  value (a part of it, or another encoding) is not scrubbed: keep `hosts`
  narrow.
* A file of a secret is a regular file of the user of `serve`, mode 0600
  or 0400, and not a symbolic link.
* A rule can have a `path` (an absolute path, on whole segments): the
  broker ends the TLS of each tunnel to the host of that rule and checks
  the path of each request. The path of an offer holds under the path of
  `default_policy`; the narrower one holds.
* The broker makes a CA for each job with a credential or a path rule.
  The unit gets the bundle (that CA, then the CA file of the environment
  of `serve` and a system bundle of the host) as the credential file
  `podrunner-broker-ca.pem`, and the ten variables of `pod_broker::ca::env`
  name it (`SSL_CERT_FILE`, `CURL_CA_BUNDLE`, `REQUESTS_CA_BUNDLE`,
  `NODE_EXTRA_CA_CERTS`, `GIT_SSL_CAINFO`, `AWS_CA_BUNDLE`, `DENO_CERT`,
  `HEX_CACERTS_PATH`, `CLOUDSDK_CORE_CUSTOM_CA_CERTS_FILE`,
  `CARGO_HTTP_CAINFO`). A host with no CA file refuses such a job. A JVM
  reads none of them: the unit also gets the trust store of the bundle
  (PKCS #12, the credential file `podrunner-broker-ca.p12`), and
  `JAVA_TOOL_OPTIONS` names it (T-577). Each container of the job (a `container:`
  job, a service, a Docker action) of a kind with `container: hooks`
  gets the bundle read-only at `/__podrunner/podrunner-broker-ca.pem`
  and the same ten variables (T-578, `docs/contracts/hook.md`).

## The remote mint

On a host with no root, a step can read each file of the uid of podrunner
(DR-4). The source and its credentials then live on another machine:
`podrunner mint-serve --config mint.toml` (GitHub #31). The unit host
names it with a source of kind `remote`:

```toml
[[source]]
name = "org-trusted"
kind = "remote"
tier = "trusted"
arch = "x86_64"
slots = 2
mint = "mint.example.org:8443"     # host:port of the mint
mint_pin = "sha256/BASE64"          # the pin of the key of the mint
cert_file = "/etc/podrunner/host.pem"
key_file = "/etc/podrunner/host.key" # a P-256 key, mode 0600
```

The key of a host is its token at a mint, and each mint serves one tier.
Give each tier its own key: the configuration refuses one `key_file` for
two remote sources of two tiers.

The file of the mint:

<!-- not podrunner.toml -->
```toml
listen = "0.0.0.0:8443"
cert_file = "/etc/podrunner-mint/mint.pem"
key_file = "/etc/podrunner-mint/mint.key"   # a P-256 key, mode 0600
state = "/var/lib/podrunner-mint"           # ledger.jsonl and audit.jsonl
reconcile_after_s = 300      # a credential with no start after this time is an alert
revoke_after = 3             # this many alerts of a host revoke it (source-revoked)

[source]                     # the source that the mint holds: github-scaleset, or fake
name = "org-trusted"
kind = "github-scaleset"
tier = "trusted"
arch = "x86_64"
# ... the keys of a github-scaleset source

[trust]                      # the tier rule of the mint: it withdraws an offer that the set may not run
organizations = [{ login = "example-org" }]

[[host]]                     # one for each unit host
name = "host-a"
pin = "sha256/BASE64"        # the pin of the key of the host
slots = 2                    # its offers are capped at this count
not_after_s = 1800000000     # the end of the life of this key (optional); a rotation adds a
                             # second [[host]] of the same name with the new pin, then removes this one
```

The pin of a key is the SHA-256 of its SubjectPublicKeyInfo, in base64,
as curl's `--pinnedpubkey` writes it:
`openssl x509 -in host.pem -pubkey -noout | openssl pkey -pubin -outform der | openssl dgst -sha256 -binary | base64`.
