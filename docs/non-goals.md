# The non-goals of v1

v1 cuts these items (GitHub #10, T-010). Nothing of v1 depends on them.
Each has a trigger that brings it back; a request that meets a trigger
opens its own issue.

The owner ruled on 2026-11-18 that podrunner accommodates each need and
ask of githome and cfb (`docs/decisions.md`, "githome and cfb first"): a
row that one of their needs names is lifted. T-491 finds each such row;
T-480 (shared caches), T-487 (warm units) and T-479 (a schema of
`podrunner.toml`) hold three of them.

| Item | Trigger |
| --- | --- |
| A `docker` unit kind | The owner names a Docker or Podman host with no KVM (call C7, DR-13). |
| The `host` kind, and a `local` tier for forge jobs | The owner names a dedicated machine. Now `local` serves only a `fake` source (call C14). The `incus` kind: its trigger is met (G-02 names Incus): T-105. |
| Bridges for Gitea, Forgejo, GitLab Runner | An ask names one of those forges as a target. |
| `credential_delivery: socket` (the strong form of G-03) | A forge names a runner that takes its credential from a socket. |
| Port claims and a lock of a service for the whole host | A workflow of the owner needs `services:` on H0. |
| `pod-dockerapi` | Its trigger is met: githome's G-09 names `services:`, `docker build` and Docker-in-Docker (the owner, 2026-11-13). The census (`docs/container-engine.md`) finds that Docker-in-Docker needs the whole Engine API, so T-103 runs a real engine inside a guest, not a shim and not a socket of the host. |
| The TLS end of the broker: placeholders, path rules | A policy needs path rules or placeholders. |
| A schema for `podrunner.toml` | A second reader of the file exists. |
| Shared caches, warm units, a mirror of actions (G-07 shared form, G-11) | A measure shows that the cost of a start or a fetch matters. |
| `metrics`, `reload`, the audit lines shipped to a second host | A consumer is named. |
| `wall_s_max` and `per_repo_max` for each tier | A measured starvation of the trusted tier. |
| Alternate payloads and a native runner | An ask names a target that no official runner serves (riscv64, i686, loongarch64, ppc64le). |
| The split of `sign`, `egress` and `ctl` into processes of their own uids | A host with distinct uids is the first target. |
| A hosted runner provider with an airlock | The owner uses a hosted provider and no own hosts. |
| A wire of JSON lines for the unit seam | The first driver outside the tree exists. |
| A trusted or untrusted tier on an emulated host (an aarch64 container under QEMU user mode on x86_64) | podvm or podnet gives a confine layer or an egress hold under emulation. Such a host has no Landlock (ENOSYS) and no hold, so no kind meets a floor of a forge tier, and `doctor` names both (T-194). The ways that remain: a real aarch64 host, or a guest of the machine tier when podvm runs an aarch64 guest. |

Triggers that are met: the `forge-contract` source (githome has code: T-101, the `program` source kind), the `incus` kind (T-105), `pod-dockerapi` (T-103), and the local mode (done: T-046).
