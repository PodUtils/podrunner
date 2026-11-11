# Appendix: threat model

`SPEC.md` §5.1 summarises this table. Labels follow the SPEC.

| Actor | Capability | Controls | Verdict |
| --- | --- | --- | --- |
| T1 Fork-PR author | Workflow YAML, `runs-on`, code, branch names | Tier from `trust_inputs`, intake check, `guest` floor with a self-test, allow-list egress (§6.3, §6.6) | Holds on a guest host |
| T2 Compromised dependency in a trusted job | Code as the unit's uid, real secrets, open egress | `redirect` boundary, advisory egress denies | Not contained on H0: it reaches co-tenant jobs, the host network and the host key of the remote mint (§5.3, §9.1) |
| T3 Co-tenant job, concurrent or earlier, on one principal | Same tier, other repository. A job can rewrite what later jobs rely on: the pin list and its verification key file, `podrunner.toml`, saved image archives, driver binaries | `max_concurrent` 1, `affinity = "repository"` on a shared principal, signed pin list with a `mint` key, `image_id` check at `start` (§5.3) | Contained only with distinct principals. A verified tree kill removes processes, not files. On one principal the configuration, the key file and the driver binaries stay writable by the job: a stated residual of DR-4 |
| T4 Leaked source credential | App key or PAT | Signer, org-level App, alerts, leak runbook | Persistence bounded, use not (§8.1, §8.5) |
| T5 Hostile container request | Any step that reaches the control socket, including a forged `prepare_job` | Mount table, port rule, no host path, device or privileged flag, `prepare_job` once, registry allow list and egress policy for images (§7.2). The image rule is not a boundary: the other rules are | Specified, unbuilt. A step can fetch an image of an allowed registry through allowed egress anyway |
| T6 Hostile hook input | `createOptions`, mounts, registry credentials | The same enforcement. The hook only relays | Specified, unbuilt |
| T7 Hostile proxy or DNS | A name the job may resolve, the upstream proxy | Direct mode: one resolve, checked addresses. Name-forwarding: upstream must refuse private ranges (§9.4) | Direct sound. Name-forwarding advisory unless the requirement is met |
| T8 Holder of a stolen host token or key | The token and, on H0, the host key of the remote mint | Offer ledger capped at the host's configured slot count, host key with a rotation interval, tokens shorter than the key, start reconciliation, audit with token id and peer address (§5.3) | Bounded to the offers assigned to that host, and a credential for one job may run another until U22 is settled. On H0 a compromised job holds the key and obtains tokens until the operator revokes it. Short token life protects only a host whose key is outside the step principal |
| T9 Compromised `mint` host or sidecar | Job-controlled strings parsed next to session tokens | The sidecar is a child with no App key (§8.1) | Reach of the session tokens while it runs |
| T10 Job with secrets or an unvetted author | `pull_request_target`, `workflow_run`, `issue_comment`, `release`, `other`, a same-repository pull request by an external actor, an event of an unlisted public repository | Only a fork `pull_request` is `untrusted`. Every other job that is not `trusted` is refused with `event-not-allowed` (§6.3) | Specified |
| T11 Network observer or resolver on the path to the remote `mint` | Redirect or read the token | Mutual TLS with pinned keys (§5.3) | Specified |
| T12 Trusted job that causes `degraded` or kills its shim | A step as the unit's uid | `destroy` reports `unknown` when the shim is dead, the host is `degraded`, `capacity` 0, an operator restarts `slots` (§10.4) | Denial of service is accepted and stated. Processes that left the shim are killed by marker on a best effort basis |

## Notes on `SPEC.md` §6.3

Not covered by the inputs (Inferred or Unverified, U23): `actor` is the input, not the triggering actor of a re-run (the lookup should pin `run_attempt`). If the actor of a `merge_group` run is the queue's bot, every such job has no tier and is refused with `event-not-allowed`. If it is the member who enqueued, fork code runs `trusted` after one click. A reusable workflow runs under the caller's event, so a workflow called at a ref the owner does not control is the supply-chain residual of any action. Automation holding a member's token is `member`.

Why `guest` for untrusted: the plan says the interposer "is not a security boundary" and that fork jobs must not run in the chroot tier (plan §8 risk 2), and the owner's rule for githome is a microVM for forks (G-02). GitHub says self-hosted runners "should almost never be used for public repositories" (Documented, `runner-trust/src/gh-secure-use.txt` line 111). On a host with no VM the `untrusted` tier is unavailable and its set is not polled (§10.2).

## Why each event is refused (notes on SPEC.md §6.3)

`pull_request_target`, `workflow_run` and `issue_comment` carry secrets in the job message, and a same-repository pull request by an actor who is not a member runs with them. The events `release`, `repository_dispatch`, `other` and `unknown` have no vetted author, and neither has an event of a public repository that `trusted_public` does not list. A member's `pull_request_target`, `workflow_run` or `issue_comment` is refused too, because it has the shape of the attack. Labels: Inferred from GitHub's event model, not run.
