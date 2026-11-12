# The command line

`crates/podrunner/src/cli.rs`. The exit codes are the table of `pod-core`
of podcrates: 0 success, 64 a usage error, 69 a control socket that does
not answer or a state directory that another `serve` holds, 75 a
`stop --wait` that timed out, 76 an audit file whose chain breaks, 77 a refusal of the supervisor, 78 a
configuration that cannot be used, 80 a kind or a source that this build
has not.

| Command | What it does |
| --- | --- |
| `podrunner serve --config FILE --state DIR [--socket PATH]` | Run the slots: one tick each second, and the control socket (`DIR/ctl.sock` by default, mode 0600). It first takes the lock `DIR/serve.lock` (a `flock`; the file stays): a second `serve` on one state directory exits 69 before it reads the journal. A live socket at the path is never replaced. SIGTERM and SIGINT start the drain of `stop`. With `NOTIFY_SOCKET` it sends `READY=1` when the socket answers, `STOPPING=1` at the drain, and `WATCHDOG=1` at half of `WATCHDOG_USEC`. |
| `podrunner check --config FILE` | Check a configuration as `serve` does before it starts, with no side effect: each table and key (an unknown key or a wrong type exits 78 and names it), each source built and not called (key files, certificates, the sidecar file), each unit kind with its driver file. No state directory, no unit, no network. `serve` refuses a bad file with the same text. `docs/examples/podrunner.toml` passes it in the gate. |
| `podrunner status [--json] (--state DIR \| --socket PATH)` | The status (`schemas/status.v1.json`): each source with its jobs and refusals, each kind with its record, each pin with its age, `degraded`, `no-tier-served`. Text from a file or a forge prints with each control character as an escape, cut at 200 bytes. |
| `podrunner drain ...` | Take no new job; the jobs in flight go on. |
| `podrunner stop ...` | Drain, then end when each job is final. |
| `podrunner stop --wait --state DIR [--timeout SECONDS]` | Also wait until `serve` has ended (the lock of `DIR` is free). A timeout exits 75. |
| `podrunner clear-degraded ...` | Start jobs again after the operator looked at a residue. |
| `podrunner audit verify (--state DIR \| --file FILE) [--anchor SEQ:HASH]` | Walk the chain of `audit.jsonl`: each line has `seq` and `prev` (the SHA-256 of the line before). A lost, edited or torn line exits 76 and names the `seq`. A cut tail needs the anchor: the `seq` of a line and the `prev` of the next line in the copy at the source. |
| `podrunner doctor [--config FILE] [--json]` | With the file: the capability record of each unit kind through its driver binary, the hold of `[unit.podvm] podnet`, and for each source whether a kind meets the floor and the hold of its tier. Always: the Landlock ABI, `/dev/kvm`, and the rows of `pod-hostprobe` that the drivers need (user namespaces, mount, chroot, KVM, Landlock), each in a disposable child of the binary. It names each input that is missing and changes nothing (call C9). |
| `podrunner print-unit --config FILE --state DIR [--binary PATH] [--watchdog SECONDS]` | A systemd unit for `serve` (`Type=notify`, a stop that waits for the drain), with no credential. `WatchdogSec` only with `--watchdog`. |
| `podrunner image verify --tarball FILE --arch ARCH` | Check a release tarball of the official runner against `images/runner/runner.toml` (the version and the SHA-256 of each architecture). Exit 78 when it differs. |
| `podrunner run FILE (--runner-dir DIR \| --podvm PATH --images STORE --image NAME@sha256:HEX) [--event NAME] [--var NAME=VALUE]... [--input NAME=VALUE]...` | The local mode (T-046): run the jobs of one workflow file, with no forge, against a fake broker on the loopback, one job at a time. With `--runner-dir`, DIR is an unpacked release of the official runner that the run may change; each job runs in it as the user of podrunner. With `--podvm`, each job runs in a fresh podvm unit of the runner image IMAGE of the store STORE (T-090), and a unit with residue fails its job. `needs`, a matrix, `if:` and the outputs of jobs hold as on GitHub; `--event` (default `push`), `--var` and `--input` give `github.event_name`, `vars` and `inputs`. `--actions DIR` gives the actions: each `uses: OWNER/REPO@SHA` (a full commit id; a tag is refused) comes from `DIR/OWNER_REPO/SHA.tar.gz`, an archive with one top directory, as GitHub's tarballs. `--checkout DIR` gives the tree DIR (`git archive` of HEAD in a git tree) to each `uses: actions/checkout@REF`, which unpacks it into the workspace or into a plain relative `with: path:`; a `./` action reads the checked-out tree. A Docker action, `container:` and `services:` are not supported yet (T-093). One line for each job: `ok`, `failure` (with the reason when the job could not start), or `skipped` with its condition. Exit 0, or 1 when a job failed. |
| `podrunner mint-serve --config FILE` | The remote mint (GitHub #31), on a machine that runs no unit: the source wire over mutual TLS to the pinned keys of the unit hosts (`docs/configuration.md`, "The remote mint"). |
| `podrunner pin-sign --config FILE --seed-file FILE` | The `signer` and `signature` lines of `[pin]`. The seed file holds 64 hex digits (`openssl rand -hex 32`), mode 0600; keep it off the unit host. |

## The control socket

JSON lines on a unix socket of mode 0600. A peer with another uid is
refused. The socket has threads of its own: at most 8 clients at a time
(one more gets `busy`), and 5 s for each exchange, so a silent client
delays no tick of the supervisor. A request is one line, `{"v": 1, "op": ...}`, and the reply is one
line, `{"v": 1, "reply": "ok", "value": ...}` or
`{"v": 1, "reply": "error", "code": ..., "detail": ...}`. The operations:
`version`, `status`, `drain`, `stop`, `capacity`, `clear-degraded`,
`withdraw`. `crates/podrunner/src/control.rs` sits behind the trait
`Handler`, so that `pod-control` of podcrates can replace it with no change
of the wire.

Where units run as the uid of podrunner (a shared principal), the peer
check does not protect the socket from a unit.
