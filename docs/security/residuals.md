# Accepted residuals

The risks that the design accepts. Each one names the decision that
accepts it and what would remove it. GitHub #4 (DR-4, Q1 of
`TODO/PROGRESS.md`) links this list.

| # | Residual | Accepted by | What removes it |
| --- | --- | --- | --- |
| R1 | A trusted job on a no-root host runs at boundary `redirect`: path rewriting, no isolation against hostile code. A compromised dependency can read the files of the uid and reach the host. | DR-4, open: until a yes, `trusted` needs `confined` (call C13) | The `confined` boundary (T-042, `pod-confine`). |
| R2 | On a shared principal, a unit can signal and read the other processes of the uid, also podrunner and the control socket. One unit at a time limits it to the job that runs and the next one. | The design of `slots` (T-021) | A distinct principal for each unit. |
| R3 | A trusted job of a member gets the secrets of its repository, also when a dependency in it is hostile. | The tier rule (T-018) | Nothing in podrunner: the forge owns secrets. |
| R4 | A compromised `mint` host or sidecar has each credential of its source. | The process rule (`threat-model.md`) | The runbook (`leak-runbook.md`) limits the time. |
| R5 | The `advisory` hold (`podnet pod --backend seccomp`) does not hold unix, netlink and vsock sockets, loopback TCP and inherited descriptors (T-044). A trusted payload can go around it. | `trusted` needs `advisory` only | An `enforced` hold on podvm units (T-041). |
