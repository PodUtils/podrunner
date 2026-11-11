# The configuration: podrunner.toml

`crates/podrunner-slots/src/config.rs` reads this file. An unknown key is
refused. A file that lowers the floor of a tier, or that names a `fake`
source or unit with no `allow_fake = true`, is refused.

```toml
host = "host-a"              # the name of the host in the full report
allow_fake = false           # a fake source or unit only with true
terminal_suppress_s = 3600   # a terminal refusal suppresses its job id
same_principal_ok = false    # more than one unit at a time on a shared principal
payload_from_offer = false   # take the payload of an offer when it is pinned

[[source]]                   # one set: one tier, one architecture, this host
name = "org-trusted"
kind = "github-scaleset"     # or "fake"
tier = "trusted"             # "untrusted", "trusted" or "local"
arch = "x86_64"              # or "aarch64"
operator_controlled = true   # the full report; else the public report
slots = 4                    # the most units of this source at one time

[unit.podvm]                 # one table for each unit kind: fake, podvm
slots = 2
command = ["./run.sh"]       # the top process of a unit
binary = "/usr/local/bin/podvm"       # podvm: the released podvm binary
images = "/var/lib/podrunner/images"  # podvm: a podvm store with the pinned images

[tier.trusted]               # a floor at or above the lowest floor
min_boundary = "confined"

[trust]                      # the tier rule (crates/podrunner-tier)
organizations = [{ login = "example-org", id = 11 }]
trusted_public = [{ owner = "example-org", repo = "open-tool" }]

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
[[pin.payload]]
arch = "x86_64"
reference = "runner@sha256:HEX"
built_at_s = 1800000000      # the time of the build, Unix seconds

[egress]                     # the egress ceiling of each tier (T-024)
```

## The floors of the tiers

| Tier | Lowest floor | Why |
| --- | --- | --- |
| `untrusted` | `guest` | A fork job is hostile code. |
| `trusted` | `confined` | DR-4 (Q1) is open: `redirect` for `trusted` needs a ruling of the owner. |
| `local` | `none` | Only the configuration marks a set `local`; its jobs run at `local` with no tier rule, and only a `fake` source can be `local` (call C14). |
