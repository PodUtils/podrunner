//! The `program` source kind (T-101, githome's G-01): any program that
//! speaks the source wire (`docs/contracts/source-driver.md`) on its stdio,
//! as the scale-set sidecar does for GitHub. podrunner adds no code for one
//! forge: the forge ships its own program.
//!
//! `podrunner fake-source --script FILE` is such a program over the fake
//! source of the tests, and `podrunner source-conformance -- PROGRAM ARGS`
//! runs the 15 tests of the source wire against a program (T-104): a forge
//! runs them in its own CI.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;

use pod_core::clock::{Clock, SystemClock};
use pod_core::failure::Class;
use podrunner_slots::config::SourceConfig;

use crate::serve::{Failure, fail};

/// The environment variable that gives a program the path of its
/// credential file.
pub const KEY_FILE_VAR: &str = "PODRUNNER_SOURCE_KEY_FILE";

/// The source of a `[[source]]` table of the kind `program`.
pub(crate) fn program(s: &SourceConfig) -> Result<podrunner_mint::SidecarSource, Failure> {
    let path = s.program.clone().unwrap_or_default();
    podrunner_slots::config::check_driver(Path::new(&path))
        .map_err(|e| fail(Class::Config, format!("[[source]] {} program: {e}", s.name)))?;
    let mut env = Vec::new();
    if let Some(key) = &s.key_file {
        let meta =
            std::fs::metadata(key).map_err(|e| fail(Class::Config, format!("{}: the key file: {}", s.name, e)))?;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err(fail(
                Class::Config,
                format!("{}: the key file is open to the group or to others: set mode 0400", s.name),
            ));
        }
        env.push((KEY_FILE_VAR.to_string(), key.clone()));
    }
    let caller = format!("program:{}", s.name);
    Ok(podrunner_mint::SidecarSource::program(&path, s.program_args.clone(), env, &caller))
}

/// `podrunner fake-source --script FILE`: serve the fake source of FILE on
/// stdio, one request line at a time, until stdin ends.
pub fn fake_source(args: &[String]) -> Result<(), Failure> {
    let script = match args {
        [flag, file] if flag == "--script" => {
            std::fs::read_to_string(file).map_err(|e| fail(Class::NotFound, format!("the script {file}: {}", e)))?
        }
        [] => String::new(),
        _ => return Err(fail(Class::Usage, crate::manual::usage("fake-source"))),
    };
    let script = podrunner_fake::parse_script(&script).map_err(|e| fail(Class::Config, format!("the script: {e}")))?;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let mut source = podrunner_fake::FakeSource::new("fake-program", clock, script);
    let stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.split(b'\n') {
        let line = line.map_err(|e| fail(Class::Unavailable, format!("stdin: {}", e)))?;
        let reply = podrunner_source::serve_line(&mut source, &line);
        stdout
            .write_all(reply.as_bytes())
            .and_then(|()| stdout.flush())
            .map_err(|e| fail(Class::Unavailable, format!("stdout: {}", e)))?;
    }
    Ok(())
}

/// The time limit of one test of `source-conformance`.
const TEST_LIMIT: std::time::Duration = std::time::Duration::from_secs(60);

/// End each child of this process that runs `program` with `args`, and
/// each process below it (#258). A child runs it when an argument of its
/// command line is the program (as given, or the file it names: a script
/// runs under its interpreter) and its command line ends with `args`.
fn end_children_of(program: &str, args: &[String]) {
    let me = std::process::id();
    let real = std::fs::canonicalize(program).ok().map(|p| p.display().to_string());
    let mut procs: Vec<(u32, u32, Vec<String>)> = Vec::new();
    for e in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        let stat = std::fs::read_to_string(e.path().join("stat")).unwrap_or_default();
        // The second field after the name in parentheses is the parent.
        let Some(ppid) = stat.rsplit_once(')').and_then(|(_, r)| r.split_whitespace().nth(1)?.parse().ok()) else {
            continue;
        };
        let raw = std::fs::read(e.path().join("cmdline")).unwrap_or_default();
        let argv = raw.split(|b| *b == 0).filter(|a| !a.is_empty()).map(|a| String::from_utf8_lossy(a).into_owned());
        procs.push((pid, ppid, argv.collect()));
    }
    let runs = |argv: &[String]| argv.iter().any(|a| a == program || Some(a) == real.as_ref()) && argv.ends_with(args);
    let mut doomed: Vec<u32> = procs.iter().filter(|(_, pp, argv)| *pp == me && runs(argv)).map(|p| p.0).collect();
    let mut i = 0;
    while i < doomed.len() {
        let parent = doomed[i];
        doomed.extend(procs.iter().filter(|(_, pp, _)| *pp == parent).map(|p| p.0));
        i += 1;
    }
    for pid in doomed {
        // SAFETY: a signal to a process below this one.
        unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    }
}

/// The program under test of `source-conformance`.
struct Under {
    program: String,
    args: Vec<String>,
}

impl podrunner_source::conformance::Fixture for Under {
    fn fresh(&self) -> Box<dyn podrunner_source::JobSource> {
        Box::new(podrunner_mint::SidecarSource::program(&self.program, self.args.clone(), vec![], "program:test"))
    }

    fn exchange(&self, line: &[u8]) -> String {
        let spawned = std::process::Command::new(&self.program)
            .args(&self.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn();
        let Ok(mut child) = spawned else { return String::new() };
        let mut reply = String::new();
        if let (Some(mut i), Some(o)) = (child.stdin.take(), child.stdout.take()) {
            let _ = i.write_all(line).and_then(|()| i.write_all(b"\n"));
            let _ = BufReader::new(o).read_line(&mut reply);
        }
        let _ = child.kill();
        let _ = child.wait();
        reply
    }
}

/// The offers that a fresh source of each test gives on its first `next`:
/// test 3 needs two to be strict.
pub const OFFERS_NEEDED: u32 = 2;

/// `podrunner source-conformance [--json] -- PROGRAM ARGS...`: the 15 tests
/// of the source wire against PROGRAM, one line each; the text and true
/// when each passes. The program must give two offers or more on its first
/// `next`. With `--json` (T-503, githome's G-28), one JSON object for each
/// line: first `{tests, offers_needed}`, then `{number, name, pass, why}`
/// for each test, then `{passed, failed}`.
pub fn source_conformance(args: &[String]) -> Result<(String, bool), Failure> {
    match args.split_first() {
        Some((flag, rest)) if flag == "--json" => source_conformance_in(rest, TEST_LIMIT, true),
        _ => source_conformance_in(args, TEST_LIMIT, false),
    }
}

/// [`source_conformance`] with the time limit `limit` of each test.
fn source_conformance_in(args: &[String], limit: std::time::Duration, json: bool) -> Result<(String, bool), Failure> {
    let rest = match args.split_first() {
        Some((dash, rest)) if dash == "--" && !rest.is_empty() => rest,
        _ => return Err(fail(Class::Usage, crate::manual::usage("source-conformance"))),
    };
    let tests = podrunner_source::conformance::tests();
    let mut lines: Vec<String> = Vec::new();
    if json {
        lines.push(serde_json::json!({ "tests": tests.len(), "offers_needed": OFFERS_NEEDED }).to_string());
    }
    let mut failed = 0;
    let under = Arc::new(Under { program: rest[0].clone(), args: rest[1..].to_vec() });
    // Each test in a time limit: a program that never answers fails that
    // test by name, and the run goes on.
    for (number, name) in tests.iter().copied() {
        let (tx, rx) = std::sync::mpsc::channel();
        let u = under.clone();
        std::thread::spawn(move || {
            let _ = tx.send(podrunner_source::conformance::run(&*u, &[number]).remove(0));
        });
        let report = match rx.recv_timeout(limit) {
            Ok(r) => r,
            Err(_) => {
                // The program of the test ends, so that its thread ends too
                // and leaves no process (#258).
                end_children_of(&under.program, &under.args);
                let _ = rx.recv_timeout(std::time::Duration::from_secs(5));
                let why = format!("no end in {} s", limit.as_secs());
                podrunner_source::conformance::Report { number, name, failure: Some(why) }
            }
        };
        failed += usize::from(report.failure.is_some());
        lines.push(if json {
            serde_json::json!({
                "number": report.number, "name": report.name, "pass": report.failure.is_none(), "why": report.failure,
            })
            .to_string()
        } else {
            report.to_string()
        });
    }
    if json {
        lines.push(serde_json::json!({ "passed": tests.len() - failed, "failed": failed }).to_string());
    }
    Ok((lines.iter().map(|l| format!("{l}\n")).collect(), failed == 0))
}

#[cfg(test)]
mod tests {
    /// #258: a program that never answers fails each test by its time
    /// limit, and leaves no process and no thread that waits on it.
    #[test]
    fn a_program_that_never_answers_leaves_no_process() {
        // A script, as a program of a user often is: its process is the
        // interpreter, and it has a child (#258, the review).
        let dir = std::env::temp_dir().join(format!("podrunner-conformance-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("never");
        podrunner_unit::program_file::write(&script, "#!/bin/sh\nsleep 307\n").unwrap();
        let args = ["--".to_string(), script.display().to_string()];
        let (text, passed) = super::source_conformance_in(&args, std::time::Duration::from_millis(500), false).unwrap();
        assert!(!passed && text.contains("FAIL"), "{text}");
        std::thread::sleep(std::time::Duration::from_millis(300));
        let name = script.display().to_string();
        let left: Vec<String> = std::fs::read_dir("/proc")
            .unwrap()
            .flatten()
            .filter(|e| {
                std::fs::read_to_string(e.path().join("cmdline"))
                    .is_ok_and(|c| c.contains(name.as_str()) || c == "sleep\u{0}307\u{0}")
            })
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(left.is_empty(), "processes left: {left:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
