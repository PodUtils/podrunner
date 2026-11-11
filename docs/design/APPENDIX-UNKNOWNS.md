# Appendix: unknowns, and what settles each

`SPEC.md` §14 summarises this table. A statement in the SPEC that depends on an open row is labelled Unverified or Inferred there. "Settled" rows are measurements of this directory (`measurements/`).

| Id | Unknown | Settled by | Blocker |
| --- | --- | --- | --- |
| U1 | The official runner runs under the podvm userland rung | Settled, Observed (M2 `03-runner-payload.log`): **no**, exit 154 then 137. Runs only under `--unsafe-host-paths` with W^X off | WP-V1 |
| U2 | `RLIMIT_FSIZE` kills the official runner at start | Settled, Observed (M1 Q2, M2 `04-fsize.log`): yes, with the default W^X | none |
| U3 | The runner starts from pre-staged credential files and exposes nothing in argv or environ | Settled, Observed (M1 Q3, Q7): it starts. The env route leaks for the whole life | none |
| U4 | Grandchildren survive a podvm stop | Settled, Observed (M2 `05-lifecycle.log`): yes, 6 of 6 | none |
| U5 | Two containers of one image collide | Settled, Observed (M2 `06-shared-rootfs.log`): they share the root | none |
| U6 | github.com sends `AllowRunnerContainerHooks` | A live job | DR-7 |
| U7 | github.com honours several labels per scale set | A live job | DR-7 |
| U8 | `actions/scaleset` works with the App permissions of §6.2 | A live job | DR-7 |
| U9 | `podnet pod` with the .NET runner, Node `fetch` and the JavaScript actions `checkout` and `setup-node` run by `node` directly with their inputs. `cache` and `upload-artifact` need the results service of a live job, so they stay at level 3 | WP-N1 | none |
| U10 | The broker fails against a proxy that resets CONNECT to IP literals | WP-C1 test | none |
| U11 | A step with the stolen credential affects only its own job | A live job | DR-7 |
| U12 | The run object needs only "Actions: read", the `member` check needs "Members: read", a runner group can refuse public repositories | A live job | DR-7 |
| U13 | `DOTNET_EnableWriteXorExecute=0` has a measurable cost | A bench on a unit | none |
| U14 | Runner behaviour with the hooks variable set and the job variable false | A live job (source only, M1 Q5) | DR-7 |
| U15 | github.com's treatment of a third-party runner version string | The GitHub terms, read by the owner | DR-16 |
| U16 | `dookoid` on a host that denies ptrace | A run on such a host | none, low priority |
| U17 | Two hosts can share one scale set | A live job | DR-7 |
| U18 | An unacquired or acquired job can be failed by name through the scale-set API | A live job | DR-7 |
| U19 | A job container of another image, and the container of a Docker action (the five mounts of the container action table), can be honoured on `net: shared-host` podvm, and the image's own environment sets no tool cache variable | WP-V1, WP-13 | none |
| U20 | A real JIT blob differs from M1's synthetic one, and the files survive a token refresh over hours | A live job | DR-7 |
| U21 | `curl` runs under the podvm interposer, a host socket is visible inside the unit, and bash decodes framed exec output | WP-13 | none |
| U22 | A JIT runner is bound to the job it was minted for. Until known, egress policy is set-wide, a credential minted for one job may run another job of the same set (§5.3), and `JobStarted` is checked against the claim (§11.1) | A live job | DR-7 |
| U23 | The trust inputs behave as §6.3 lists for re-runs, merge queue and reusable workflows | A live job | DR-7 |
| U24 | `pod-broker` v1 serves one listener for each unit | WP-08 | none |
| U25 | `podvm run -v` accepts a path of an inherited descriptor (`/proc/self/fd/N`), so the control socket can bind a mount source it has resolved (`contracts/unit-driver.md`) | WP-04 | none |
