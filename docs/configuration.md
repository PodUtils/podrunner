# The configuration: podrunner.toml

`crates/podrunner-slots/src/config.rs` reads this file. An unknown key is
refused. A file that lowers the floor of a tier, or that names a `fake`
source or unit with no `allow_fake = true`, is refused.

`docs/examples/podrunner.toml` is a working example (a fake source and fake units); `podrunner check --config FILE` checks a file with no side effect. A test reads each block below and refuses a key that does not exist.

```toml
host = "host-a"              # the name of the host in the full report
allow_fake = false           # a fake source or unit only with true
terminal_suppress_s = 3600   # a terminal refusal suppresses its job id
same_principal_ok = false    # more than one unit at a time on a shared principal
payload_from_offer = false   # take the payload of an offer when it is pinned
repository = "owner/repo"    # the one repository of the host (DR-4); other offers: repo-scope-refused
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

[unit.podvm]                 # one table for each unit kind: fake, podvm
slots = 2
command = ["./run.sh"]       # the top process of a unit
binary = "/usr/local/bin/podvm"       # podvm: the released podvm binary
images = "/var/lib/podrunner/images"  # podvm: a podvm store with the pinned images
registries = ["ghcr.io"]     # container: hooks: the registry hosts of job containers; none: each is refused
runner_dir = "/home/runner"  # the runner directory in the unit; absent: the path that the driver observes
podnet = "/usr/local/bin/podnet"      # podvm: hold the egress of each unit with podnet pod (netns: enforced, else seccomp: advisory); absent: unknown
disk_max_bytes = 10737418240          # the bytes that a unit may write under its root, measured each 30 s; over it the
                                      # unit ends with the exit reason limit; a disk limit of the offer comes first; absent: no measure
confine = false                       # podvm: each unit and container under the confine layer of podvm (--podvm-confine); the kind says confined after its self-test

[tier.trusted]               # a floor at or above the lowest floor
min_boundary = "confined"

[trust]                      # the tier rule (crates/podrunner-tier)
organizations = [{ login = "example-org", id = 11 }]
trusted_public = [{ owner = "example-org", repo = "open-tool" }]
extra_events = ["release"]   # more events of a trusted job: only workflow_run, issue_comment,
                             # release and repository_dispatch (the owner, Q5); default none

[deadlines]                  # seconds; these are the defaults
prepare_s = 600
claim_s = 60
mint_s = 60
start_s = 120
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
built_at_s = 1800000000      # the time of the build, Unix seconds
runner_version = "2.338.2"   # the official runner in the image
max_age_s = 2419200          # the age limit of this pin; the smaller of it and [pin] max_age_s holds

[egress]                     # the egress ceiling of each tier (T-024)

[dns]                        # the resolver of the egress and of a remote mint (pod-dns, T-076)
doh = true                   # ask DNS over HTTPS (resolvers by their address) when the system and the cache fail
pins = ["mint.example.org=192.0.2.10"]  # HOST=IP: a pinned name is not asked of the system
```

The resolver asks in this order: an IP literal, the pins, the system
resolver, the addresses that carried a session, DNS over HTTPS to
resolvers by their address, then the old addresses of that cache. A host
whose own resolver fails keeps its egress and its mint.

## The floors of the tiers

| Tier | Lowest floor | Why |
| --- | --- | --- |
| `untrusted` | `guest` | A fork job is hostile code. |
| `trusted` | `redirect` (default `confined`) | DR-4: `redirect` is a choice of the operator. The file must then have a signed pin list (`podrunner pin-sign`), set `repository`, keep `same_principal_ok = false` and set `slots` on each `trusted` source (T-045 has the other mitigations). |
| `local` | `none` | Only the configuration marks a set `local`; its jobs run at `local` with no tier rule, and only a `fake` source can be `local` (call C14). |

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
```

The program gets the path of `key_file` in `PODRUNNER_SOURCE_KEY_FILE`,
never its content. A program source is never `local`.
`podrunner fake-source --script FILE` is a program of this kind over the
fake source: a forge checks its own program against it.

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
