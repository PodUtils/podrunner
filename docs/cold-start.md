# Cold start

Run these commands from the checkout root on Linux. The Claude Code
session hook runs the first command. In Codex, run it from the shell.

```sh
sh scripts/session-start.sh
sh scripts/cloud-setup.sh --runner
. ./scripts/cloud-env.sh
cargo todo check
```

The setup installs tools and the Actions runner. Load
`scripts/cloud-env.sh` again in each new shell. Use
`cargo build --release -p podrunner` to build the product.

Read [the handoff](HANDOFF.md), [the rules](../AGENTS.md) and
[the work order](../TODO/PROGRESS.md). Read the last CI runs and open issues as
shown in [Maintenance](maintenance.md). Read the watcher logs
`.work/watch.log` and `.work/runs.log`.
If the Actions mirror refuses a read, use
`gh run list --repo PodUtils/podrunner --branch main`.

Run `sh scripts/agent/check.sh` before each push. Run it as an
unprivileged user on the Linux host. The driver tests require a view of
`/` and `/tmp` with their normal owners and a writable user cache.
Check the owners with `stat -c '%n %u' / /tmp` if the gate reports a
driver path owned by another user.
On a host where PID 1 leaves exited children in the process table, use
`docker-init -sg -- sh scripts/agent/check.sh` if `docker-init` is present.

For an outside test, use `sh scripts/qa-bundle.sh BINARY OUTPUT_DIR`.
