# The unknowns of the design

The design rests on 26 unknowns (GitHub #9, T-009). Each row has the
action that settles it, its blocker and its state. A statement of a
document that depends on an open row says "Unverified". A live row runs in
the live acceptance suite (GitHub #40, T-040). DR-7 is yes (2026-11-13): the
live rows wait for the live gates (GitHub #74 and #75).

| Id | Unknown | Settles it | Blocker | State |
| --- | --- | --- | --- | --- |
| U1 | The official runner runs on the podvm userland rung | podvm PodUtils/podvm#229 to PodUtils/podvm#232 | none | Settled: no on the userland rung of the design review (exit 154, then 137); yes on the supervise rung of podvm `v0.2.0-beta.3` and later, with no root and `DOTNET_EnableWriteXorExecute=0`: the runner takes a canned job and a `container:` job (T-026, T-035; `runner-image.yml`). On the namespace rung CoreCLR fails at start (no `/proc`). |
| U2 | `RLIMIT_FSIZE` kills the runner at start | A measure | none | Settled: yes, with the default W^X. The podvm driver sets `DOTNET_EnableWriteXorExecute=0` and applies no `fsize` limit (T-023). |
| U3 | The runner starts from pre-staged files, with nothing in argv or the environment | A measure | none | Settled: yes. The environment route leaks for the whole life of the job. Unit test 7 holds it (`docs/contracts/unit-driver.md`). |
| U4 | Grandchildren survive `podvm stop` | A measure | none | Settled: yes (6 of 6 in the design review; podvm `v0.2.0-beta.2`, T-023). The driver kills the tree under each tagged process. From podvm `v0.2.0-beta.3` the supervisor is a subreaper: an orphan that drops the tag (`setsid env -i`) has it as its parent, and `podvm rm -f` ends it. Unit tests 25 and 26 hold it (T-063). |
| U5 | Two containers of one image share a root | A measure | none | Settled: yes. The driver gives each unit a store of its own (T-023). |
| U6 | github.com sends `AllowRunnerContainerHooks` | A live job | the live gates | Open (T-040). |
| U7 | Several labels for each scale set | A live job | the live gates | Open (T-040). |
| U8 | `actions/scaleset` works with the minimum App permissions | A live job | the live gates | Open (T-040). |
| U9 | `podnet pod` holds the .NET runner, Node `fetch`, `checkout`, `setup-node` | PodUtils/podnet@T-229 | none | Settled for the seccomp backend (T-044): each of six payloads passes, each CONNECT has a flow. The official runner in a held podvm unit takes a job from the fake broker through the egress module (T-065, `crates/podrunner-broker-fake/tests/held.rs`). Not measured: the long poll of github.com (T-040). |
| U10 | The broker fails on a proxy that resets CONNECT to IP literals | A loopback test of the egress module | none | Settled: yes with addresses, no in the names mode. Through an upstream that resets a CONNECT to an IP literal, a unit gets 502; with `upstream_names_only = true` the broker sends the checked name, and the tunnel works (`ip_literal_reset_and_the_names_mode`, `crates/podrunner-egress/src/tests.rs`; T-072). The canary then says `takes-non-public` for such an upstream: the hold stays `advisory`. |
| U11 | A stolen credential of one job affects only its job | A live job | the live gates | Open (T-040). |
| U12 | The run object needs only "Actions: read"; the member check needs "Members: read"; a runner group can refuse public repositories | A live job | the live gates | Open (T-040). |
| U13 | `DOTNET_EnableWriteXorExecute=0` has a measurable cost | A bench on a unit | none | Settled for the start: no measurable cost. `Runner.Listener --version` of runner 2.338.2, 20 runs for each value, three times: 124 to 140 ms with W^X, 125 to 131 ms with no W^X (the spread of the host is larger than the difference). Not measured: a long job. |
| U14 | The runner with the hooks variable set and the job variable false | A live job | the live gates | Open (T-040). |
| U15 | The treatment of a third-party runner version string by github.com | The owner reads the terms (call C10, DR-16) | the owner | Open. No alternate payload ships before it. |
| U16 | `dookoid` on a host that denies ptrace | A run on such a host | none | Deferred: no shim of the Docker API is planned. T-103 runs a real engine in a guest (`docs/container-engine.md`), and no `dookoid` runs there. |
| U17 | Two hosts share one scale set | A live job | the live gates | Open (T-040). |
| U18 | A job can be failed by name through the scale-set API | A live job | the live gates | Open (T-040). |
| U19 | A job container of another image and a Docker action run on a podvm unit with a shared host network | T-034 | podvm PodUtils/podvm#236 | Settled (T-034, podvm `v0.2.0-beta.3`): yes on the supervise rung (a job container of another image with the seven volumes, `exec -e -w` at 47 ms, an action with the five volumes); `:ro` holds for `run` and, from podvm `v0.2.0-beta.4`, for `exec` (T-062, GitHub #62). On the namespace rung `-v` is refused. |
| U20 | A real JIT blob differs from the synthetic one; the files survive a token refresh over hours | A live job | the live gates | Open (T-040). |
| U21 | `curl` runs under the podvm interposer; a host socket is visible in the unit; bash decodes framed output | T-034 | podvm PodUtils/podvm#235 | Settled (T-034, podvm `v0.2.0-beta.3`): curl and bash run on both rungs. A socket that the host binds in the rootfs of the unit is reachable on both rungs; a socket through `-v` on the supervise rung too (GitHub #51, fixed in beta.3); `-v` is refused on the namespace rung. The relay uses the plain mode (no frames). |
| U22 | A JIT runner is bound to the job it was minted for | A live job | the live gates | Open (T-040). |
| U23 | The trust inputs behave as the tier rule says for re-runs, merge queues and reusable workflows | A live job | the live gates | Open (T-040). |
| U24 | `pod-broker` v1 serves one listener for each unit | A read of the crate | none | Settled: yes (podcrates `v0.16.0` and later): `Broker::serve_unix_job` binds one listener to one job, with no token in the unit, and a token of another job gets 407 (PodUtils/podcrates@T-160; T-024: `crates/podrunner-egress` and its tests). No wrapper. |
| U25 | `podvm run -v` accepts `/proc/self/fd/N` as a mount source | A run on the userland rung | an H0 host | Settled (T-023): yes on the supervise rung of podvm `v0.2.0-beta.2` (the payload reads the open file). On the namespace rung `-v` is refused by name. The driver writes the credential files into the rootfs and needs no `-v`. |
| U26 | A job of another architecture (`linux/arm64` on an x86_64 host) runs in a guest of the machine tier, emulated by QEMU | T-151 (`scripts/measure-foreign.sh` in `runner-image.yml`) | podvm PodUtils/podvm#366 (PodUtils/podvm@T-1890) | Settled for now (T-151, podvm `v0.2.0-beta.14`): no. The machine tier starts an x86_64 guest only; `--platform linux/arm64` exits 125. podrunner refuses a label of another architecture (`ARM64`, `ubuntu-24.04-arm`) by name unless a `--platform` route names it. podvm plans the guest (QEMU of that architecture under TCG); podrunner measures it when podvm amends PodUtils/podvm#366 with its release. |
