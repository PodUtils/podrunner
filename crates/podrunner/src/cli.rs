//! The command line (GitHub #22). The exit codes are the table of
//! `pod-core` (64 to 80): 64 usage, 69 a socket that does not answer, 78 a
//! configuration that cannot be used, 80 a kind that this build has not.

use std::path::PathBuf;

use pod_core::failure::Class;

use crate::control::{self, Request};
use crate::text::display;
use crate::verbs::{audit_verify, human, stop_wait};

mod unit;
pub use unit::{print_system_unit, print_unit};

/// The version line: the version of the crate and the commit of the tree.
pub fn version() -> String {
    format!("podrunner {} ({})\n", env!("PODRUNNER_VERSION"), env!("PODRUNNER_COMMIT"))
}

pub const USAGE: &str = "usage: podrunner COMMAND [OPTION...]

  serve --config FILE --state DIR [--socket PATH]   run the slots
  check --config FILE                               check a configuration; start nothing
  status [--json] (--state DIR | --socket PATH)     the state of the slots
  drain (--state DIR | --socket PATH)               take no new job
  drain --source NAME (--state DIR | --socket PATH) one source takes no new job
  resume --source NAME (--state DIR | --socket PATH)
                                                    the source takes jobs again
  stop (--state DIR | --socket PATH)                drain, then end
  stop --wait --state DIR [--timeout SECONDS]       ... and wait until serve ends
  clear-degraded (--state DIR | --socket PATH)      start jobs again after a residue
  config init --source NAME --program PATH --podvm PATH --images STORE --image REF --built-at-s N --out FILE ...
                                                    a podrunner.toml from the kinds of this host
  source restart --name NAME (--state DIR | --socket PATH)
                                                    build one source again (a new key), keep its jobs
  doctor [--config FILE] [--json]                   the inputs of a host check
  doctor [--json] --run FILE [the options of run]   the checks of a local run
  audit verify (--state DIR | --file FILE) [--anchor SEQ:HASH]
                                                    check the chain of audit.jsonl
  print-unit --config FILE --state DIR [--binary PATH] [--watchdog SECONDS]
                                                    a user unit of systemd, with no credential
  print-unit --system [--user NAME] --config FILE --state DIR [--binary PATH] [--watchdog SECONDS]
                                                    a unit of the system that runs as NAME (podrunner)
  install-unit --system [--user NAME] --config FILE --state DIR [--binary PATH] [--watchdog SECONDS]
                                                    install that unit and start it (as root)
  remove-unit --system                              stop and remove the unit of the system (as root)
  pin-sign --config FILE --seed-file FILE           the signer and the signature of [pin]
  mint-serve --config FILE                          the remote mint (on a machine with no unit)
  fake-source [--script FILE]                       a fake source on stdio (for a test of a program source)
  source-conformance [--json] -- PROGRAM ARGS...    the source tests against a source program
  image verify --tarball FILE --arch ARCH           check actions-runner-linux-*.tar.gz against its pin
  run (FILE | DIR) (--runner-dir DIR | (--podvm PATH [--podnet PATH] | --microvm PATH [--engine DIR] | --incus PATH)
      --images STORE --image NAME@sha256:HEX) [OPTION...]
                                                    run the jobs of a workflow; each option: podrunner run --help
  version | --version                               the version
  help [COMMAND] | COMMAND --help | --help          this text, or the options of one command
";

/// The result of a command: the exit code, stdout, stderr.
pub struct Out {
    pub code: u8,
    pub stdout: String,
    pub stderr: String,
}

pub(crate) fn ok(stdout: String) -> Out {
    Out { code: 0, stdout, stderr: String::new() }
}

pub(crate) fn err(class: Class, msg: impl Into<String>) -> Out {
    Out { code: class.exit_code(), stdout: String::new(), stderr: format!("podrunner: {}\n", msg.into()) }
}

/// The options of a command: `--name VALUE` pairs and `--flag` words.
pub(crate) struct Opts {
    pub(crate) values: Vec<(String, String)>,
    pub(crate) flags: Vec<String>,
}

const FLAGS: [&str; 3] = ["--json", "--wait", "--system"];

pub(crate) fn opts(args: &[String]) -> Result<Opts, Out> {
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
    pub(crate) fn get(&self, name: &str) -> Option<PathBuf> {
        self.values.iter().find(|(k, _)| k == name).map(|(_, v)| PathBuf::from(v))
    }

    pub(crate) fn need(&self, name: &str) -> Result<PathBuf, Out> {
        self.get(name).ok_or_else(|| err(Class::Usage, format!("{name} is necessary\n{USAGE}")))
    }

    /// Each option and each flag is one of `allowed`: a flag of another
    /// command is refused, not ignored (`serve --system`).
    pub(crate) fn only(&self, allowed: &[&str]) -> Result<(), Out> {
        let names = self.values.iter().map(|(k, _)| k).chain(&self.flags);
        match names.into_iter().find(|k| !allowed.contains(&k.as_str())) {
            Some(k) => Err(err(Class::Usage, format!("an unknown option {}\n{USAGE}", display(k)))),
            None => Ok(()),
        }
    }

    pub(crate) fn socket(&self) -> Result<PathBuf, Out> {
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
    if let Some(o) = crate::help::answer(cmd, rest) {
        return Ok(o);
    }
    match cmd.as_str() {
        "--version" | "-V" => Ok(ok(version())),
        "--help" | "-h" => Ok(ok(USAGE.into())),
        "serve" => {
            let o = opts(rest)?;
            o.only(&["--config", "--state", "--socket"])?;
            let state = o.need("--state")?;
            let socket = o.get("--socket").unwrap_or_else(|| crate::serve::socket_of(&state));
            // As PID 1 of a container, a parent reaps the orphans (T-180).
            crate::init::reap_as_pid1();
            crate::serve::run(&o.need("--config")?, &state, &socket).map_err(|f| err(f.class, f.detail))?;
            Ok(ok(String::new()))
        }
        "check" => {
            let o = opts(rest)?;
            o.only(&["--config"])?;
            let said = crate::mint_serve::check_any(&o.need("--config")?).map_err(|f| err(f.class, f.detail))?;
            Ok(ok(said.into()))
        }
        "stop" if rest.iter().any(|a| a == "--wait") => stop_wait(rest),
        "status" | "drain" | "stop" | "clear-degraded" | "resume" => {
            let o = opts(rest)?;
            o.only(&["--state", "--socket", "--source", "--json"])?;
            let source = o.get("--source").map(|s| s.display().to_string());
            let req = match (cmd.as_str(), source) {
                ("drain", Some(source)) => Request::DrainSource { source },
                ("resume", Some(source)) => Request::ResumeSource { source },
                ("stop", Some(source)) => Request::StopSource { source },
                ("resume", None) => return Err(err(Class::Usage, format!("resume needs --source\n{USAGE}"))),
                (_, Some(_)) => return Err(err(Class::Usage, format!("{cmd} takes no --source\n{USAGE}"))),
                ("status", None) => Request::Status,
                ("drain", None) => Request::Drain,
                ("stop", None) => Request::Stop,
                _ => Request::ClearDegraded,
            };
            let value = control::call(&o.socket()?, &req).map_err(|e| crate::verbs::control_failure(&o, e))?;
            if req == Request::Status && !o.flags.contains(&"--json".into()) {
                Ok(ok(human(&value)))
            } else {
                Ok(ok(format!("{value}\n")))
            }
        }
        "doctor" if rest.iter().any(|x| x == "--run") => {
            // The checks see the flags that `run` sees: those of the files
            // first. `--run` can come after another option (T-342).
            let at = rest.iter().position(|x| x == "--run").unwrap_or(0);
            let args: Vec<String> = rest.iter().enumerate().filter(|(i, _)| *i != at).map(|(_, x)| x.clone()).collect();
            let rest = crate::run_config::expand(&args).map_err(|e| err(Class::Config, e))?;
            Ok(crate::run_local::doctor(&rest))
        }
        "doctor" => {
            let o = opts(rest)?;
            o.only(&["--config", "--json"])?;
            let v = crate::doctor::report(o.get("--config").as_deref()).map_err(|f| err(f.class, f.detail))?;
            if o.flags.contains(&"--json".into()) { Ok(ok(format!("{v}\n"))) } else { Ok(ok(crate::doctor::text(&v))) }
        }
        "print-unit" => unit::print(rest),
        "install-unit" => unit::install(rest),
        "remove-unit" => unit::remove(rest),
        "config" => match rest.split_first() {
            Some((sub, rest)) if sub == "init" => crate::config_init::init(rest),
            _ => Err(err(Class::Usage, format!("config needs init\n{USAGE}"))),
        },
        "source" => match rest.split_first() {
            Some((sub, rest)) if sub == "restart" => crate::verbs::source_restart(rest),
            _ => Err(err(Class::Usage, format!("source needs restart\n{USAGE}"))),
        },
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
        "run" => {
            // The flags of the configuration files go first (T-143).
            let rest = crate::run_config::expand(rest).map_err(|e| err(Class::Config, e))?;
            match rest.iter().any(|a| a == "--watch" || a == "--schedule") {
                true => Ok(crate::run_local::repeat(&rest)),
                false => Ok(crate::run_local::run(&rest)),
            }
        }
        "source-conformance" => {
            let (text, passed) = crate::program_source::source_conformance(rest).map_err(|f| err(f.class, f.detail))?;
            Ok(Out { code: if passed { 0 } else { 1 }, stdout: text, stderr: String::new() })
        }
        "fake-source" => {
            crate::program_source::fake_source(rest).map_err(|f| err(f.class, f.detail))?;
            Ok(ok(String::new()))
        }
        "mint-serve" => {
            let o = opts(rest)?;
            o.only(&["--config"])?;
            crate::mint_serve::run(&o.need("--config")?).map_err(|f| err(f.class, f.detail))?;
            Ok(ok(String::new()))
        }
        "pin-sign" => {
            let o = opts(rest)?;
            o.only(&["--config", "--seed-file"])?;
            let config = o.need("--config")?;
            let text = std::fs::read_to_string(&config)
                .map_err(|e| err(Class::Config, format!("the configuration {}: {e}", config.display())))?;
            let pins = podrunner_slots::pins::of_file(&text).map_err(|e| err(Class::Config, e))?;
            let seed = podrunner_slots::pins::read_seed(&o.need("--seed-file")?).map_err(|e| err(Class::Config, e))?;
            let (signer, signature) = podrunner_slots::pins::sign(&pins, &seed);
            Ok(ok(format!("signer = \"{signer}\"\nsignature = \"{signature}\"\n")))
        }
        other => Err(err(Class::Usage, format!("an unknown command {:?}\n{USAGE}", display(other)))),
    }
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
        assert!(o.stdout.starts_with(&format!("podrunner {} (", env!("PODRUNNER_VERSION"))), "{}", o.stdout);
        assert!(env!("PODRUNNER_VERSION").starts_with(env!("CARGO_PKG_VERSION")));
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
        let a = |c: &std::path::Path, s: &std::path::Path| {
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
        assert_eq!(run(&args(&["print-unit", "--config", "/nonexistent/c", "--state", "s"])).code, 66, "T-339");
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
