# The non-goals of v1

v1 cuts these items (GitHub #10, T-010). Nothing of v1 depends on them.
Each has a trigger that brings it back; a request that meets a trigger
opens its own issue.

A row that a need of githome or cfb names is lifted (`docs/decisions.md`,
"githome and cfb first"). The census of T-491
(`docs/reports/census-2026-11-18.md`) lists each such row under "Triggers
that are met", with its entry.

| Item | Trigger |
| --- | --- |
| Bridges for Gitea, Forgejo, GitLab Runner | An ask names one of those forges as a target. cfb's A-X-01 names Forgejo and GitLab; cfb defers them past this milestone (Q-12 d, R-054 on GitHub #292): a project on such a forge comes back as an ask through githome. |
| A `docker` unit kind | The owner names a Docker or Podman host with no KVM (call C7, DR-13). |
| The `host` kind, and a `local` tier for forge jobs | The owner names a dedicated machine. `local` serves only a `fake` source (call C14). |
| `credential_delivery: socket` (the strong form of G-03) | A forge names a runner that takes its credential from a socket. |
| Port claims and a lock of a service for the whole host | A workflow of the owner needs `services:` on H0. |
| `pod-dockerapi` | Its trigger is met: githome's G-09 names `services:`, `docker build` and Docker-in-Docker (the owner, 2026-11-13). The census (`docs/container-engine.md`) finds that Docker-in-Docker needs the whole Engine API, so T-103 runs a real engine inside a guest, not a shim and not a socket of the host. |
| `wall_s_max` and `per_repo_max` for each tier | A measured starvation of the trusted tier. |
| The split of `sign`, `egress` and `ctl` into processes of their own uids | A host with distinct uids is the first target. |
| A hosted runner provider with an airlock | The owner uses a hosted provider and no own hosts. |
| `metrics` | A consumer is named. cfb drops metrics as JSON beyond `status --json` and the run report (Q-12 a). |
| A native runner for an architecture that no official runner serves (riscv64, i686, loongarch64, ppc64le) | An ask names such a target. |
| A wire of JSON lines for the unit seam | The first driver outside the tree exists. |
| A trusted or untrusted tier on an emulated host (an aarch64 container under QEMU user mode on x86_64) | podvm or podnet gives a confine layer or an egress hold under emulation. Such a host has no Landlock (ENOSYS) and no hold, so no kind meets a floor of a forge tier, and `doctor` names both (T-194). The ways that remain: a real aarch64 host, or a guest of the machine tier when podvm runs an aarch64 guest. |

Triggers that are met: the `forge-contract` source (githome has code: T-101, the `program` source kind), the `incus` kind (T-105), `pod-dockerapi` (T-103), and the local mode (done: T-046).

Triggers met by the needs of githome and cfb (the census of T-491):

| Item | The need | Entries |
| --- | --- | --- |
| The TLS end of the broker: placeholders, path rules | githome's G-04 (its P2 part) and G-35 | T-524, T-563 (done: `[egress.secrets]`, `docs/configuration.md`, "Placeholder credentials") |
| A schema for `podrunner.toml` | githome renders the file (G-37, cfb's R-053): a second reader exists | T-479 (done: `schemas/config.v1.json`), T-507 |
| Shared caches, a mirror of actions | G-07, G-20, cfb's A-X-01 | T-480, T-528 |
| Warm units | G-11, G-25, cfb's A-X-01 | T-487 |
| `reload` | G-17, G-22 | T-495, T-498 |
| The audit lines shipped to a second host (the source) | G-04, G-33 | T-486 |
| Alternate payloads: a sandbox unit | G-36 | T-506, T-161 |
