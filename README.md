# podrunner

One static program that runs a CI job in one fresh unit for each job. It
finds a job, picks the safest isolation that the host offers, runs the
official `actions/runner` once inside it, reports, and destroys the unit.
It needs no root, no namespaces and no docker daemon; a proxy is the only
way out of a unit.

Status: beta, not for production. No live job ran on github.com
(DR-7, T-033). The plan and its milestones are in
[GitHub #1](https://github.com/PodUtils/podrunner/issues/1); the work is in
`TODO/` (`TODO/PROGRESS.md`). `docs/cli.md` gives the commands and
`docs/configuration.md` the configuration file.

## Build

```sh
cargo build --release
sh scripts/check-static.sh target/x86_64-unknown-linux-musl/release/podrunner
```

The binary is static musl. `docs/development.md` gives the rules of the
code; `AGENTS.md` gives the rules of each session.

## License

0BSD (`LICENSE`). `THIRD_PARTY.md` lists the components with other
licences.
