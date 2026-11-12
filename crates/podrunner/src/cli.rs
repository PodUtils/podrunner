//! The command line (GitHub #22). The exit codes are the table of
//! `pod-core` (64 to 80): 64 usage, 69 a socket that does not answer, 78 a
//! configuration that cannot be used, 80 a kind that this build has not.

use std::path::{Path, PathBuf};

use pod_core::failure::Class;
use serde_json::Value;

use crate::control::{self, Reply, Request};
use crate::text::display;

/// The version line: the version of the crate and the commit of the tree.
pub fn version() -> String {
    format!("podrunner {} ({})\n", env!("CARGO_PKG_VERSION"), env!("PODRUNNER_COMMIT"))
}

pub const USAGE: &str = "usage: podrunner COMMAND [OPTION...]

  serve --config FILE --state DIR [--socket PATH]   run the slots
  check --config FILE                               check a configuration; start nothing
  status [--json] (--state DIR | --socket PATH)     the state of the slots
  drain (--state DIR | --socket PATH)               take no new job
  stop (--state DIR | --socket PATH)                drain, then end
  stop --wait --state DIR [--timeout SECONDS]       ... and wait until serve ends
  clear-degraded (--state DIR | --socket PATH)      start jobs again after a residue
  doctor [--config FILE] [--json]                   the inputs of a host check
  audit verify (--state DIR | --file FILE) [--anchor SEQ:HASH]
                                                    check the chain of audit.jsonl
  print-unit --config FILE --state DIR [--binary PATH] [--watchdog SECONDS]
                                                    a service unit, with no credential
  pin-sign --config FILE --seed-file FILE           the signer and the signature of [pin]
  mint-serve --config FILE                          the remote mint (on a machine with no unit)
  image verify --tarball FILE --arch ARCH           check a runner tarball against its pin
  run FILE (--runner-dir DIR | --podvm PATH --images STORE --image REF) [--actions DIR] [--checkout DIR]
      [--event NAME] [--var NAME=VALUE]... [--input NAME=VALUE]...
                                                    run the jobs of a workflow, here or in podvm units
  --version | --help
";

/// The result of a command: the exit code, stdout, stderr.
pub struct Out {
    pub code: u8,
    pub stdout: String,
    pub stderr: String,
}

fn ok(stdout: String) -> Out {
    Out { code: 0, stdout, stderr: String::new() }
}

fn err(class: Class, msg: impl Into<String>) -> Out {
    Out { code: class.exit_code(), stdout: String::new(), stderr: format!("podrunner: {}\n", msg.into()) }
}

/// The options of a command: `--name VALUE` pairs and `--flag` words.
struct Opts {
    values: Vec<(String, String)>,
    flags: Vec<String>,
}

const FLAGS: [&str; 2] = ["--json", "--wait"];

fn opts(args: &[String]) -> Result<Opts, Out> {
    let mut o = Opts { values: Vec::new(), flags: Vec::new() };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if FLAGS.contains(&a.as_str()) {
            o.flags.push(a.clone());
        } else if a.starts_with("--") {
            let v = it.next().ok_or_else(|| err(Class::Usage, format!("{a} needs a value\n{USAGE}")))?;
            o.values.push((a.clone(), v.clone()));
        } else {
            return Err(err(Class::Usage, format!("an unknown argument {:?}\n{USAGE}", display(a))));
        }
    }
    Ok(o)
}

impl Opts {
    fn get(&self, name: &str) -> Option<PathBuf> {
        self.values.iter().find(|(k, _)| k == name).map(|(_, v)| PathBuf::from(v))
    }

    fn need(&self, name: &str) -> Result<PathBuf, Out> {
        self.get(name).ok_or_else(|| err(Class::Usage, format!("{name} is necessary\n{USAGE}")))
    }

    fn only(&self, allowed: &[&str]) -> Result<(), Out> {
        match self.values.iter().find(|(k, _)| !allowed.contains(&k.as_str())) {
            Some((k, _)) => Err(err(Class::Usage, format!("an unknown option {}\n{USAGE}", display(k)))),
            None => Ok(()),
        }
    }

    fn socket(&self) -> Result<PathBuf, Out> {
        match (self.get("--socket"), self.get("--state")) {
            (Some(s), _) => Ok(s),
            (None, Some(d)) => Ok(crate::serve::socket_of(&d)),
            (None, None) => Err(err(Class::Usage, format!("--state or --socket is necessary\n{USAGE}"))),
        }
    }
}

/// Run the command line `args` (without the program name).
pub fn run(args: &[String]) -> Out {
    match run_inner(args) {
        Ok(o) | Err(o) => o,
    }
}

fn run_inner(args: &[String]) -> Result<Out, Out> {
    let Some((cmd, rest)) = args.split_first() else { return Err(err(Class::Usage, USAGE)) };
    match cmd.as_str() {
        "--version" | "-V" => Ok(ok(version())),
        "--help" | "-h" => Ok(ok(USAGE.into())),
        "serve" => {
            let o = opts(rest)?;
            o.only(&["--config", "--state", "--socket"])?;
            let state = o.need("--state")?;
            let socket = o.get("--socket").unwrap_or_else(|| crate::serve::socket_of(&state));
            crate::serve::run(&o.need("--config")?, &state, &socket).map_err(|f| err(f.class, f.detail))?;
            Ok(ok(String::new()))
        }
        "check" => {
            let o = opts(rest)?;
            o.only(&["--config"])?;
            crate::serve::check(&o.need("--config")?).map_err(|f| err(f.class, f.detail))?;
            Ok(ok("the configuration is usable\n".into()))
        }
        "stop" if rest.iter().any(|a| a == "--wait") => stop_wait(rest),
        "status" | "drain" | "stop" | "clear-degraded" => {
            let o = opts(rest)?;
            o.only(&["--state", "--socket"])?;
            let req = match cmd.as_str() {
                "status" => Request::Status,
                "drain" => Request::Drain,
                "stop" => Request::Stop,
                _ => Request::ClearDegraded,
            };
            let reply = control::call(&o.socket()?, &req)
                .map_err(|e| err(Class::Unavailable, format!("the control socket: {}", e.kind())))?;
            match reply {
                Reply::Ok { value } if req == Request::Status && !o.flags.contains(&"--json".into()) => {
                    Ok(ok(human(&value)))
                }
                Reply::Ok { value } => Ok(ok(format!("{value}\n"))),
                Reply::Error { code, detail } => {
                    Err(err(Class::Refused, format!("{}: {}", display(&code), display(&detail))))
                }
            }
        }
        "doctor" => {
            let o = opts(rest)?;
            o.only(&["--config"])?;
            let v = crate::doctor::report(o.get("--config").as_deref()).map_err(|f| err(f.class, f.detail))?;
            if o.flags.contains(&"--json".into()) { Ok(ok(format!("{v}\n"))) } else { Ok(ok(crate::doctor::text(&v))) }
        }
        "print-unit" => {
            let o = opts(rest)?;
            o.only(&["--config", "--state", "--binary", "--watchdog"])?;
            let binary = o.get("--binary").unwrap_or_else(|| PathBuf::from("/usr/local/bin/podrunner"));
            let watchdog = match o.get("--watchdog") {
                None => None,
                Some(w) => Some(
                    w.to_str()
                        .and_then(|w| w.parse::<u32>().ok())
                        .filter(|w| *w > 0)
                        .ok_or_else(|| err(Class::Usage, "--watchdog needs a number of seconds"))?,
                ),
            };
            Ok(ok(print_unit(&binary, &o.need("--config")?, &o.need("--state")?, watchdog)))
        }
        "audit" => match rest.split_first() {
            Some((sub, rest)) if sub == "verify" => audit_verify(rest),
            _ => Err(err(Class::Usage, format!("audit needs verify\n{USAGE}"))),
        },
        "image" => match rest.split_first() {
            Some((sub, rest)) if sub == "verify" => {
                let o = opts(rest)?;
                o.only(&["--tarball", "--arch"])?;
                let arch = o.need("--arch")?.display().to_string();
                crate::image::verify(&o.need("--tarball")?, &arch).map(ok).map_err(|e| err(Class::Config, e))
            }
            _ => Err(err(Class::Usage, format!("image needs verify\n{USAGE}"))),
        },
        "run" => Ok(crate::run_local::run(rest)),
        "mint-serve" => {
            let o = opts(rest)?;
            o.only(&["--config"])?;
            crate::mint_serve::run(&o.need("--config")?).map_err(|f| err(f.class, f.detail))?;
            Ok(ok(String::new()))
        }
        "pin-sign" => {
            let o = opts(rest)?;
            o.only(&["--config", "--seed-file"])?;
            let text = std::fs::read_to_string(o.need("--config")?)
                .map_err(|e| err(Class::Config, format!("the configuration: {}", e.kind())))?;
            let pins = podrunner_slots::pins::of_file(&text).map_err(|e| err(Class::Config, e))?;
            let seed = podrunner_slots::pins::read_seed(&o.need("--seed-file")?).map_err(|e| err(Class::Config, e))?;
            let (signer, signature) = podrunner_slots::pins::sign(&pins, &seed);
            Ok(ok(format!("signer = \"{signer}\"\nsignature = \"{signature}\"\n")))
        }
        other => Err(err(Class::Usage, format!("an unknown command {:?}\n{USAGE}", display(other)))),
    }
}

/// `stop --wait`: ask for the stop, then wait until no `serve` holds the
/// lock of the state directory (GitHub #54). A timeout is exit 75.
fn stop_wait(rest: &[String]) -> Result<Out, Out> {
    let o = opts(rest)?;
    o.only(&["--state", "--timeout"])?;
    let state = o.get("--state").ok_or_else(|| err(Class::Usage, format!("stop --wait needs --state\n{USAGE}")))?;
    let timeout = match o.get("--timeout") {
        None => None,
        Some(t) => Some(std::time::Duration::from_secs(
            t.to_str()
                .and_then(|t| t.parse::<u64>().ok())
                .ok_or_else(|| err(Class::Usage, "--timeout needs a number of seconds"))?,
        )),
    };
    match control::call(&crate::serve::socket_of(&state), &Request::Stop) {
        Ok(Reply::Ok { .. }) => {}
        Ok(Reply::Error { code, detail }) => {
            return Err(err(Class::Refused, format!("{}: {}", display(&code), display(&detail))));
        }
        // No socket: serve may be gone already; the lock says.
        Err(_) => {}
    }
    let started = std::time::Instant::now();
    loop {
        match crate::lock::is_free(&state) {
            Ok(true) => return Ok(ok("stopped\n".into())),
            Ok(false) => {}
            Err(e) => return Err(err(Class::Unavailable, format!("the lock of the state directory: {}", e.kind()))),
        }
        if timeout.is_some_and(|t| started.elapsed() >= t) {
            return Err(err(Class::Timeout, "serve did not end in time: a job is not final"));
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// `audit verify`: walk the chain of an audit file (GitHub #59). A break
/// is exit 76; the anchor is `SEQ:HASH` from the copy at the source.
fn audit_verify(rest: &[String]) -> Result<Out, Out> {
    let o = opts(rest)?;
    o.only(&["--state", "--file", "--anchor"])?;
    let file = match (o.get("--file"), o.get("--state")) {
        (Some(f), _) => f,
        (None, Some(d)) => d.join("audit.jsonl"),
        (None, None) => return Err(err(Class::Usage, format!("--state or --file is necessary\n{USAGE}"))),
    };
    let anchor = o.get("--anchor").map(|a| a.display().to_string());
    let anchor = match anchor.as_deref().map(|a| a.split_once(':')) {
        None => None,
        Some(Some((seq, hash))) if seq.parse::<u64>().is_ok() && hash.len() == 64 => {
            Some((seq.parse::<u64>().expect("a number"), hash.to_string()))
        }
        Some(_) => return Err(err(Class::Usage, "--anchor is SEQ:HASH, the hash in 64 hex digits")),
    };
    let text = std::fs::read(&file).map_err(|e| err(Class::NotFound, format!("the audit file: {}", e.kind())))?;
    match podrunner_slots::audit::verify(&text, anchor.as_ref().map(|(s, h)| (*s, h.as_str()))) {
        Ok(seq) => Ok(ok(format!("the chain holds: {seq} lines\n"))),
        Err(why) => Err(err(Class::Protocol, why)),
    }
}

/// The status for a person. Each value from a file or a forge goes through
/// `display`.
fn human(v: &Value) -> String {
    let s = |v: &Value| display(&v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()));
    let mut out = format!("host {}  draining {}  degraded {}\n", s(&v["host"]), v["draining"], s(&v["degraded"]));
    if v["no_tier_served"] == Value::Bool(true) {
        out.push_str("no-tier-served: no unit kind meets the floor of a source\n");
    }
    for src in v["sources"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "source {} tier {} arch {}  jobs {}  refused {}\n",
            s(&src["name"]),
            s(&src["tier"]),
            s(&src["arch"]),
            src["jobs"],
            src["refused"]
        ));
    }
    for k in v["kinds"].as_array().into_iter().flatten() {
        let p = &k["probe"];
        out.push_str(&format!(
            "kind {} slots {} busy {}  boundary {} principal {} hold {}\n",
            s(&k["name"]),
            k["slots"],
            k["busy"],
            s(&p["boundary"]),
            s(&p["principal"]),
            s(&p["egress_hold"])
        ));
    }
    for p in v["pins"].as_array().into_iter().flatten() {
        out.push_str(&format!("pin {} {} age {} s\n", s(&p["arch"]), s(&p["reference"]), p["age_s"]));
    }
    out
}

/// A systemd unit for `serve`. It names paths only: no token, no key.
pub fn print_unit(binary: &Path, config: &Path, state: &Path, watchdog: Option<u32>) -> String {
    let watchdog = watchdog.map(|w| format!("WatchdogSec={w}\n")).unwrap_or_default();
    format!(
        "[Unit]
Description=podrunner: CI jobs in one fresh unit for each job
After=network-online.target

[Service]
# serve says READY after its control socket answers, and STOPPING when a
# stop signal starts the drain (sd_notify).
Type=notify
ExecStart={} serve --config {} --state {}
# SIGTERM starts a drain: serve ends when each job is final.
KillMode=mixed
TimeoutStopSec=infinity
{watchdog}Restart=on-failure
# 78: a configuration or a credential that cannot be used; a restart does
# not help (pod-core).
RestartPreventExitStatus=78
NoNewPrivileges=yes

[Install]
WantedBy=default.target
",
        binary.display(),
        config.display(),
        state.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn version_names_the_version_and_a_commit() {
        let o = run(&args(&["--version"]));
        assert_eq!(o.code, 0);
        assert!(o.stdout.starts_with(&format!("podrunner {} (", env!("CARGO_PKG_VERSION"))), "{}", o.stdout);
    }

    #[test]
    fn usage_errors_exit_64() {
        for a in [&[][..], &["nope"], &["serve"], &["status"], &["status", "--bogus", "x"], &["print-unit", "--config"]]
        {
            assert_eq!(run(&args(a)).code, 64, "{a:?}");
        }
        assert_eq!(run(&args(&["--help"])).code, 0);
    }

    #[test]
    fn a_socket_that_does_not_answer_is_69() {
        let o = run(&args(&["status", "--socket", "/nonexistent/podrunner.sock"]));
        assert_eq!(o.code, 69, "{}", o.stderr);
    }

    #[test]
    fn a_bad_configuration_is_78() {
        let dir = std::env::temp_dir().join(format!("podrunner-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = dir.join("podrunner.toml");
        std::fs::write(&cfg, "host = \"h\"\n[tier.untrusted]\nmin_boundary = \"none\"\n[pin]\n").unwrap();
        let o = run(&args(&["serve", "--config", cfg.to_str().unwrap(), "--state", dir.join("s").to_str().unwrap()]));
        assert_eq!(o.code, 78, "{}", o.stderr);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The lines of `pin-sign` make the file pass the check; a seed file
    /// that the group can read is refused.
    #[test]
    fn pin_sign_signs_the_pin_list_of_a_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("podrunner-pin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (cfg, seed) = (dir.join("podrunner.toml"), dir.join("seed"));
        let body = format!(
            "[[pin.payload]]\narch = \"x86_64\"\nreference = \"r@sha256:{}\"\nbuilt_at_s = 1\n",
            "0".repeat(64)
        );
        std::fs::write(&cfg, format!("host = \"h\"\n[pin]\n{body}")).unwrap();
        std::fs::write(&seed, "07".repeat(32)).unwrap();
        std::fs::set_permissions(&seed, std::fs::Permissions::from_mode(0o640)).unwrap();
        let a = |c: &Path, s: &Path| {
            args(&["pin-sign", "--config", c.to_str().unwrap(), "--seed-file", s.to_str().unwrap()])
        };
        assert_eq!(run(&a(&cfg, &seed)).code, 78);
        std::fs::set_permissions(&seed, std::fs::Permissions::from_mode(0o600)).unwrap();
        let o = run(&a(&cfg, &seed));
        assert_eq!(o.code, 0, "{}", o.stderr);
        let signed = format!("host = \"h\"\n[pin]\n{}{body}", o.stdout);
        podrunner_slots::config::Config::parse(&signed).unwrap();
        assert!(podrunner_slots::config::Config::parse(&signed.replace("built_at_s = 1", "built_at_s = 2")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn print_unit_holds_no_credential() {
        let canary = "podrunner-canary-0f9c2d71";
        let unit = print_unit(
            Path::new("/usr/local/bin/podrunner"),
            Path::new("/etc/podrunner.toml"),
            Path::new("/var/lib/podrunner"),
            None,
        );
        assert!(unit.contains("Type=notify") && !unit.contains("WatchdogSec"));
        assert!(unit.contains(
            "ExecStart=/usr/local/bin/podrunner serve --config /etc/podrunner.toml --state /var/lib/podrunner"
        ));
        assert!(!unit.contains(canary) && !unit.to_lowercase().contains("token"));
        let unit = print_unit(Path::new("/b"), Path::new("/c"), Path::new("/s"), Some(30));
        assert!(unit.contains("WatchdogSec=30\n"), "{unit}");
    }

    #[test]
    fn audit_verify_exits_0_on_a_chain_and_76_on_a_break() {
        use podrunner_slots::audit::{Audit, Entry, Kind};
        let dir = std::env::temp_dir().join(format!("podrunner-cli-audit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut a = Audit::open(&dir).unwrap();
        for n in 1..=3 {
            a.write(Entry {
                seq: 0,
                prev: String::new(),
                t_ms: n,
                audit: Kind::Offer,
                source: "s".into(),
                job_id: podrunner_schema::JobId::new("j-1").unwrap(),
                code: None,
                detail: None,
            });
        }
        let state = dir.display().to_string();
        assert_eq!(run(&args(&["audit", "verify", "--state", &state])).code, 0);
        let file = dir.join("audit.jsonl");
        let text = std::fs::read_to_string(&file).unwrap().replacen("\"t_ms\":2", "\"t_ms\":9", 1);
        std::fs::write(&file, text).unwrap();
        let o = run(&args(&["audit", "verify", "--state", &state]));
        assert_eq!(o.code, 76, "{}", o.stderr);
        assert!(o.stderr.contains("broken at seq 3"), "{}", o.stderr);
        assert_eq!(run(&args(&["audit", "verify", "--state", &state, "--anchor", "x"])).code, 64);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stop_wait_needs_the_state_and_a_number() {
        assert_eq!(run(&args(&["stop", "--wait", "--socket", "/x"])).code, 64);
        assert_eq!(run(&args(&["stop", "--wait", "--state", "/x", "--timeout", "soon"])).code, 64);
        assert_eq!(run(&args(&["print-unit", "--config", "c", "--state", "s", "--watchdog", "0"])).code, 64);
    }

    #[test]
    fn status_for_a_person_escapes_forge_text() {
        let v = serde_json::json!({ "host": "h", "draining": false, "degraded": null, "no_tier_served": false,
            "sources": [{ "name": "a\u{1b}[2J", "tier": "trusted", "arch": "x86_64", "jobs": {}, "refused": {} }],
            "kinds": [], "pins": [] });
        let text = human(&v);
        assert!(text.contains("a\\x1b[2J") && !text.contains('\u{1b}'), "{text}");
    }
}
