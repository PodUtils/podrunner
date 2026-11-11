# The unknowns of the design

The design rests on 25 unknowns (GitHub #9, T-009). Each row has the
action that settles it, its blocker and its state. A statement of a
document that depends on an open row says "Unverified". A live row runs in
the live acceptance suite (GitHub #40, T-040), which waits for the ruling
"Test accounts" (DR-7, T-007).

| Id | Unknown | Settles it | Blocker | State |
| --- | --- | --- | --- | --- |
| U1 | The official runner runs on the podvm userland rung | podvm PodUtils/podvm#229 to PodUtils/podvm#232 | podvm (on break; T-026) | Settled: no (exit 154, then 137), in the design review. |
| U2 | `RLIMIT_FSIZE` kills the runner at start | A measure | none | Settled: yes, with the default W^X. The podvm driver sets `DOTNET_EnableWriteXorExecute=0` and applies no `fsize` limit (T-023). |
| U3 | The runner starts from pre-staged files, with nothing in argv or the environment | A measure | none | Settled: yes. The environment route leaks for the whole life of the job. Unit test 7 holds it (`docs/contracts/unit-driver.md`). |
| U4 | Grandchildren survive `podvm stop` | A measure | none | Settled: yes, 6 of 6 in the design review; again with podvm `v0.2.0-beta.2` on the session host (T-023). The driver kills by the tag of the unit. |
| U5 | Two containers of one image share a root | A measure | none | Settled: yes. The driver gives each unit a store of its own (T-023). |
| U6 | github.com sends `AllowRunnerContainerHooks` | A live job | DR-7 | Open (T-040). |
| U7 | Several labels for each scale set | A live job | DR-7 | Open (T-040). |
| U8 | `actions/scaleset` works with the minimum App permissions | A live job | DR-7 | Open (T-040). |
| U9 | `podnet pod` holds the .NET runner, Node `fetch`, `checkout`, `setup-node` | PodUtils/podnet@T-229 | none | Settled for the seccomp backend (T-044): each of six payloads passes, each CONNECT has a flow; not measured: `run.sh` to a long poll. |
| U10 | The broker fails on a proxy that resets CONNECT to IP literals | A test of `pod-broker` | podcrates | Open (T-024). |
| U11 | A stolen credential of one job affects only its job | A live job | DR-7 | Open (T-040). |
| U12 | The run object needs only "Actions: read"; the member check needs "Members: read"; a runner group can refuse public repositories | A live job | DR-7 | Open (T-040). |
| U13 | `DOTNET_EnableWriteXorExecute=0` has a measurable cost | A bench on a unit | the runner on podvm (T-026) | Open. |
| U14 | The runner with the hooks variable set and the job variable false | A live job | DR-7 | Open (T-040). |
| U15 | The treatment of a third-party runner version string by github.com | The owner reads the terms (call C10, DR-16) | the owner | Open. No alternate payload ships before it. |
| U16 | `dookoid` on a host that denies ptrace | A run on such a host | none | Deferred with the Docker API (`docs/non-goals.md`). |
| U17 | Two hosts share one scale set | A live job | DR-7 | Open (T-040). |
| U18 | A job can be failed by name through the scale-set API | A live job | DR-7 | Open (T-040). |
| U19 | A job container of another image and a Docker action run on a podvm unit with a shared host network | T-034 | podvm PodUtils/podvm#236 | Open. |
| U20 | A real JIT blob differs from the synthetic one; the files survive a token refresh over hours | A live job | DR-7 | Open (T-040). |
| U21 | `curl` runs under the podvm interposer; a host socket is visible in the unit; bash decodes framed output | T-034 | podvm PodUtils/podvm#235 | Settled (T-034, 2026-11-11, podvm `v0.2.0-beta.2`): curl and bash run on both rungs. The host socket is reachable on the namespace rung when the host binds it in the rootfs of the unit, and on the supervise rung by its host path only (`connect` is not mapped: GitHub #51, T-050). The relay uses the plain mode (no frames). |
| U22 | A JIT runner is bound to the job it was minted for | A live job | DR-7 | Open (T-040). |
| U23 | The trust inputs behave as the tier rule says for re-runs, merge queues and reusable workflows | A live job | DR-7 | Open (T-040). |
| U24 | `pod-broker` v1 serves one listener for each unit | A read of the crate | none | Settled: yes with podcrates `v0.16.0`: `Broker::serve_unix_job` binds one listener to one job, with no token in the unit, and a token of another job gets 407 (PodUtils/podcrates@T-160; T-024: `crates/podrunner-egress` and its tests). No wrapper. |
| U25 | `podvm run -v` accepts `/proc/self/fd/N` as a mount source | A run on the userland rung | an H0 host | Settled (T-023, 2026-11-11): yes on the supervise rung of podvm `v0.2.0-beta.2` (the payload reads the open file). On the namespace rung `-v` is refused by name. The driver writes the credential files into the rootfs and needs no `-v`. |
