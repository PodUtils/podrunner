//! The `program` source kind (T-101): the source tests over a program
//! that speaks the source wire, here `podrunner fake-source`.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use podrunner_source::conformance::{self, Fixture};
use podrunner_source::{JobSource, WireSource};

/// The podrunner binary of this build: `fake-source` is a command of it.
fn binary() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_podrunner"))
}

/// A script of three offers that are due at once: the source tests need
/// two or more on the first `next`.
fn script() -> String {
    // One file for each call: the tests run at the same time, and a write
    // of a shared file gives a source a script cut short.
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("podrunner-program-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("script-{n}.jsonl"));
    let line = |job: &str| {
        format!(
            r#"{{"at_s":0,"offer":{{"job_id":"{job}","source":"fake-program","labels":["x"],"arch":"x86_64","wait_deadline_s":60}}}}"#
        )
    };
    std::fs::write(&path, [line("j-1"), line("j-2"), line("j-3")].join("\n")).unwrap();
    path.to_str().unwrap().to_string()
}

fn spawn() -> (Child, ChildStdin, ChildStdout) {
    let mut c = Command::new(binary())
        .args(["fake-source", "--script", &script()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let (i, o) = (c.stdin.take().unwrap(), c.stdout.take().unwrap());
    (c, i, o)
}

/// A program source behind `SidecarSource::program`, as `serve` builds it.
struct Program;

impl Fixture for Program {
    fn fresh(&self) -> Box<dyn JobSource> {
        Box::new(podrunner_mint::SidecarSource::program(
            binary().to_str().unwrap(),
            vec!["fake-source".into(), "--script".into(), script()],
            vec![],
            "program:test",
        ))
    }
    fn exchange(&self, line: &[u8]) -> String {
        let (mut c, mut i, o) = spawn();
        i.write_all(line).unwrap();
        i.write_all(b"\n").unwrap();
        let mut reply = String::new();
        BufReader::new(o).read_line(&mut reply).unwrap();
        let _ = c.kill();
        let _ = c.wait();
        reply
    }
}

#[test]
fn a_program_source_passes_the_source_tests() {
    let results = conformance::run(&Program, &[]);
    for r in &results {
        println!("{r}");
    }
    assert!(conformance::not_passed(&results).is_empty(), "{results:?}");
}

#[test]
fn the_wire_reaches_the_fake_source_of_a_program() {
    let (mut c, i, o) = spawn();
    let mut s = WireSource::new(BufReader::new(o), i);
    let h = s.hello("podrunner", "0").unwrap();
    assert_eq!(h.source, "fake-program");
    let _ = c.kill();
    let _ = c.wait();
}

/// `podrunner source-conformance` passes on the fake source, and fails on a
/// program that ends with no answer (T-104).
#[test]
fn source_conformance_passes_a_source_and_fails_an_echo() {
    let script = script();
    let run = |args: &[&str]| Command::new(binary()).arg("source-conformance").arg("--").args(args).output().unwrap();
    let ok = run(&[binary().to_str().unwrap(), "fake-source", "--script", &script]);
    let text = String::from_utf8_lossy(&ok.stdout);
    assert_eq!(ok.status.code(), Some(0), "{text}");
    assert_eq!(text.lines().filter(|l| l.contains(": pass")).count(), 12, "{text}");
    let none = run(&["/bin/false"]);
    assert_eq!(none.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&none.stdout).contains("FAIL"));
    let usage = Command::new(binary()).arg("source-conformance").output().unwrap();
    assert_eq!(usage.status.code(), Some(64));
}
