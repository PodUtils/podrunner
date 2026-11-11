# The non-goals of v1

v1 cuts these items (GitHub #10, T-010). Nothing of v1 depends on them.
Each has a trigger that brings it back; a request that meets a trigger
opens its own issue.

| Item | Trigger |
| --- | --- |
| A `docker` unit kind | The owner names a Docker or Podman host with no KVM (call C7, DR-13). |
| The `incus` and `host` kinds, and a `local` tier for forge jobs | The owner names a dedicated machine, or an ask names Incus. Now `local` serves only a `fake` source (call C14). |
| Bridges for Gitea, Forgejo, GitLab Runner | An ask names one of those forges as a target. |
| The `forge-contract` source (githome) | githome has code. |
| `credential_delivery: socket` (the strong form of G-03) | A forge names a runner that takes its credential from a socket. |
| Port claims and a lock of a service for the whole host | A workflow of the owner needs `services:` on H0. |
| `pod-dockerapi` and the census of verbs | Gate G-H1 fails (T-040), or an ask names a client that the hooks do not reach (calls C2, C11). |
| The TLS end of the broker: placeholders, path rules | A policy needs path rules or placeholders. |
| A schema for `podrunner.toml` | A second reader of the file exists. |
| Shared caches, warm units, a mirror of actions (G-07 shared form, G-11) | A measure shows that the cost of a start or a fetch matters. |
| `metrics`, `reload`, the audit lines shipped to a second host | A consumer is named. |
| `wall_s_max` and `per_repo_max` for each tier | A measured starvation of the trusted tier. |
| Alternate payloads and a native runner | An ask names a target that no official runner serves (riscv64, i686, loongarch64, ppc64le). |
| The split of `sign`, `egress` and `ctl` into processes of their own uids | A host with distinct uids is the first target. |
| A hosted runner provider with an airlock | The owner uses a hosted provider and no own hosts. |
| A wire of JSON lines for the unit seam | The first driver outside the tree exists. |
| A local mode `podrunner run workflow.yml` | An ask names it. |
