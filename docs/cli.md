# The command line

`crates/podrunner/src/cli.rs`. The exit codes are the table of `pod-core`
of podcrates: 0 success, 64 a usage error, 69 a control socket that does
not answer, 77 a refusal of the supervisor, 78 a configuration that cannot
be used, 80 a kind or a source that this build has not.

| Command | What it does |
| --- | --- |
| `podrunner serve --config FILE --state DIR [--socket PATH]` | Run the slots: one tick each second, and the control socket (`DIR/ctl.sock` by default, mode 0600). |
| `podrunner status [--json] (--state DIR \| --socket PATH)` | The status (`schemas/status.v1.json`): each source with its jobs and refusals, each kind with its record, each pin with its age, `degraded`, `no-tier-served`. Text from a file or a forge prints with each control character as an escape, cut at 200 bytes. |
| `podrunner drain ...` | Take no new job; the jobs in flight go on. |
| `podrunner stop ...` | Drain, then end when each job is final. |
| `podrunner clear-degraded ...` | Start jobs again after the operator looked at a residue. |
| `podrunner doctor` | Read `pod doctor --json` (the hold) and `podvm probe --json`, and name each input that is missing (call C9). |
| `podrunner print-unit --config FILE --state DIR [--binary PATH]` | A systemd unit for `serve`, with no credential. |
| `podrunner pin-sign --config FILE --seed-file FILE` | The `signer` and `signature` lines of `[pin]`. The seed file holds 64 hex digits (`openssl rand -hex 32`), mode 0600; keep it off the unit host. |

## The control socket

JSON lines on a unix socket of mode 0600. A peer with another uid is
refused. A request is one line, `{"v": 1, "op": ...}`, and the reply is one
line, `{"v": 1, "reply": "ok", "value": ...}` or
`{"v": 1, "reply": "error", "code": ..., "detail": ...}`. The operations:
`version`, `status`, `drain`, `stop`, `capacity`, `clear-degraded`,
`withdraw`. `crates/podrunner/src/control.rs` sits behind the trait
`Handler`, so that `pod-control` of podcrates can replace it with no change
of the wire.

Where units run as the uid of podrunner (a shared principal), the peer
check does not protect the socket from a unit.
