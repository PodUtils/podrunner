# Accepted residuals

The risks that the design accepts. Each one names the decision that
accepts it and what would remove it. GitHub #4 (DR-4, Q1 of
`TODO/PROGRESS.md`) links this list.

| # | Residual | Accepted by | What removes it |
| --- | --- | --- | --- |
| R1 | A trusted job on a no-root host runs at boundary `redirect`: path rewriting, no isolation against hostile code. A compromised dependency can read the files of the uid and reach the host. | DR-4, yes (2026-11-11), for all trusted repositories, only when the operator sets `[tier.trusted] min_boundary = "redirect"`, with the mitigations of T-045. pmask can later cut the same-uid reads of other jobs (DR-6). | The `confined` boundary (T-042, `pod-confine`). |
| R2 | On a shared principal, a unit can signal and read the other processes of the uid, also podrunner and the control socket. One unit at a time limits it to the job that runs and the next one. | The design of `slots` (T-021) | A distinct principal for each unit. On a podvm unit of the supervise rung, `[unit.podvm] confine = true` denies `/proc/PID/environ`, `root` and `cwd` of the other processes of the uid (T-079). |
| R3 | A trusted job of a member gets the secrets of its repository, also when a dependency in it is hostile. | The tier rule (T-018) | Nothing in podrunner: the forge owns secrets. |
| R4 | A compromised `mint` host or sidecar has each credential of its source. | The process rule (`threat-model.md`) | The runbook (`leak-runbook.md`) limits the time. |
| R5 | The `advisory` hold (`podnet pod --backend seccomp`) does not hold unix, netlink and vsock sockets, loopback TCP and inherited descriptors (T-044). A trusted payload can go around it. | `trusted` needs `advisory` only | The netns hold (`enforced`, T-041) on a host with user namespaces and `/dev/net/tun`. On a host with neither, R5 stays. |
| R6 | The lock of a state directory is a `flock`. On a network filesystem it may give no protection, so two hosts that share one state directory can both run `serve` and destroy the units of the other. | GitHub #54: one host for each state directory | A lease with a clock, if a shared state directory is ever needed. |
