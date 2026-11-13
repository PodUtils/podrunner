//! The `program` source kind (T-101, githome's G-01): any program that
//! speaks the source wire (`docs/contracts/source-driver.md`) on its stdio,
//! as the scale-set sidecar does for GitHub. podrunner adds no code for one
//! forge: the forge ships its own program.
//!
//! `podrunner fake-source --script FILE` is such a program over the fake
//! source of the tests, and `podrunner source-conformance -- PROGRAM ARGS`
//! runs the 12 tests of the source wire against a program (T-104): a forge
//! runs them in its own CI.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;

use pod_core::failure::Class;
use podrunner_clock::{Clock, SystemClock};
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
        let meta = std::fs::metadata(key)
            .map_err(|e| fail(Class::Config, format!("{}: the key file: {}", s.name, e.kind())))?;
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
        [flag, file] if flag == "--script" => std::fs::read_to_string(file)
            .map_err(|e| fail(Class::NotFound, format!("the script {file}: {}", e.kind())))?,
        [] => String::new(),
        _ => return Err(fail(Class::Usage, "usage: podrunner fake-source [--script FILE]")),
    };
    let script = podrunner_fake::parse_script(&script).map_err(|e| fail(Class::Config, format!("the script: {e}")))?;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut source = podrunner_fake::FakeSource::new("fake-program", clock, script);
    let stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.split(b'\n') {
        let line = line.map_err(|e| fail(Class::Unavailable, format!("stdin: {}", e.kind())))?;
        let reply = podrunner_source::serve_line(&mut source, &line);
        stdout
            .write_all(reply.as_bytes())
            .and_then(|()| stdout.flush())
            .map_err(|e| fail(Class::Unavailable, format!("stdout: {}", e.kind())))?;
    }
    Ok(())
}

/// The time limit of one test of `source-conformance`.
const TEST_LIMIT: std::time::Duration = std::time::Duration::from_secs(60);

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

/// `podrunner source-conformance -- PROGRAM ARGS...`: the 12 tests of the
/// source wire against PROGRAM, one line each; the text and true when each
/// passes. The program must give two offers or more on its first `next`.
pub fn source_conformance(args: &[String]) -> Result<(String, bool), Failure> {
    let rest = match args.split_first() {
        Some((dash, rest)) if dash == "--" && !rest.is_empty() => rest,
        _ => return Err(fail(Class::Usage, "usage: podrunner source-conformance -- PROGRAM ARGS...")),
    };
    let under = Arc::new(Under { program: rest[0].clone(), args: rest[1..].to_vec() });
    let (mut text, mut passed) = (String::new(), true);
    // Each test in a time limit: a program that never answers fails that
    // test by name, and the run goes on.
    for (number, name) in podrunner_source::conformance::tests() {
        let (tx, rx) = std::sync::mpsc::channel();
        let u = under.clone();
        std::thread::spawn(move || {
            let _ = tx.send(podrunner_source::conformance::run(&*u, &[number]).remove(0));
        });
        let line = match rx.recv_timeout(TEST_LIMIT) {
            Ok(r) => {
                passed &= r.failure.is_none();
                r.to_string()
            }
            Err(_) => {
                passed = false;
                format!("source {number:2} {name}: FAIL: no end in {} s", TEST_LIMIT.as_secs())
            }
        };
        text.push_str(&line);
        text.push('\n');
    }
    Ok((text, passed))
}
