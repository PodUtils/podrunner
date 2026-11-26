//! The signals of `podrunner run`. T-639: a second SIGINT during the stop
//! is not lost: it prints a line, and the run still ends with 130. T-636:
//! SIGHUP prints the same line as SIGINT, and a third signal leaves no
//! process of the job and no file of the run in `TMPDIR`.
//!
//! Each test waits for an event (a file of the runner, a line of stderr,
//! the end of a process), never for a fixed time.

use std::io::BufRead;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use scratch::Scratch;

mod scratch;

/// The limit of each wait for an event.
const LIMIT: Duration = Duration::from_secs(30);

/// A runner directory in `dir/r` whose runner never reaches its broker: the
/// job runs until the cancel. The runner writes its pid to `dir/started`.
/// With `stubborn`, it ignores SIGTERM, so the stop of its group takes 5 s
/// and a third signal comes while the run still stops; it ends by itself
/// after about 20 s, so an old binary leaves no process for long.
fn runner(dir: &Path, stubborn: bool) {
    std::fs::create_dir_all(dir.join("r/bin")).unwrap();
    let body = match stubborn {
        true => "trap '' TERM\necho $$ > ../started.tmp && mv ../started.tmp ../started\n\
                 i=0; while [ $i -lt 200 ]; do sleep 0.1; i=$((i+1)); done\n",
        false => "echo $$ > ../started.tmp && mv ../started.tmp ../started\nexec sleep 30\n",
    };
    podrunner_unit::program_file::write(dir.join("r/run.sh"), &format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::write(dir.join("r/bin/Runner.Listener"), "").unwrap();
    std::fs::write(dir.join("w.yml"), "on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n      - run: sleep 30\n")
        .unwrap();
}

/// Start `podrunner run` in `dir` with its own `TMPDIR` (`dir/tmp`), and a
/// reader of its stderr that sends each line.
fn start(dir: &Path) -> (Child, Receiver<String>, std::thread::JoinHandle<String>) {
    std::fs::create_dir_all(dir.join("tmp")).unwrap();
    let mut run = Command::new(env!("CARGO_BIN_EXE_podrunner"))
        .args(["run", "w.yml", "--runner-dir", "r", "--no-cache"])
        .current_dir(dir)
        .env("TMPDIR", dir.join("tmp"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (tx, rx) = channel();
    let err = run.stderr.take().unwrap();
    // The reader ends at the end of the pipe: when each process that holds
    // it ended.
    let reader = std::thread::spawn(move || {
        let mut all = String::new();
        for line in std::io::BufReader::new(err).lines().map_while(Result::ok) {
            all.push_str(&line);
            all.push('\n');
            let _ = tx.send(line);
        }
        all
    });
    (run, rx, reader)
}

/// Wait until `ok` holds, at most [`LIMIT`].
fn wait_until(what: &str, mut ok: impl FnMut() -> bool) {
    let t = Instant::now();
    while !ok() {
        assert!(t.elapsed() < LIMIT, "no {what} in {LIMIT:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Wait for a line of stderr that contains `text`.
fn line(rx: &Receiver<String>, text: &str) {
    let end = Instant::now() + LIMIT;
    loop {
        let left = end.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(l) if l.contains(text) => return,
            Ok(_) => continue,
            Err(e) => panic!("no line {text:?}: {e}"),
        }
    }
}

fn signal(run: &Child, sig: libc::c_int) {
    // SAFETY: kill(2) sends a signal to the child that this test started.
    unsafe { libc::kill(run.id() as libc::pid_t, sig) };
}

/// The pid that the runner wrote, once it runs.
fn started(dir: &Path) -> u32 {
    wait_until("start of the runner", || dir.join("started").exists());
    std::fs::read_to_string(dir.join("started")).unwrap().trim().parse().unwrap()
}

/// Whether the process `pid` is gone or a zombie.
fn gone(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(_) => true,
        Ok(s) => s.rsplit_once(')').is_some_and(|(_, r)| r.trim_start().starts_with(['Z', 'X'])),
    }
}

#[test]
fn a_second_signal_says_that_the_stop_goes_on() {
    let dir = Scratch::new("run-signal");
    runner(&dir, false);
    let (run, rx, reader) = start(&dir);
    started(&dir);
    let t = Instant::now();
    signal(&run, libc::SIGINT);
    line(&rx, "podrunner: stopping: the running job ends");
    signal(&run, libc::SIGINT);
    let out = run.wait_with_output().unwrap();
    let err = reader.join().unwrap();
    assert_eq!(out.status.code(), Some(130), "{err}");
    assert!(t.elapsed() < Duration::from_secs(20), "{:?}", t.elapsed());
    assert!(err.contains("podrunner: stopping: already stopping; a third signal ends podrunner at once"), "{err}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("cancelled a"));
}

/// T-636: SIGHUP (a closed terminal) says that the run stops, as SIGINT
/// and SIGTERM do; stderr is a pipe here, so the line stays.
#[test]
fn a_hangup_says_that_the_run_stops() {
    let dir = Scratch::new("run-hangup");
    runner(&dir, false);
    let (run, _rx, reader) = start(&dir);
    started(&dir);
    signal(&run, libc::SIGHUP);
    let out = run.wait_with_output().unwrap();
    let err = reader.join().unwrap();
    assert_eq!(out.status.code(), Some(130), "{err}");
    assert!(err.contains("podrunner: stopping: the running job ends"), "{err}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("cancelled a"));
}

/// T-636: a third signal ends podrunner at once with 130, ends the process
/// group of the runner, and a helper removes the files of the run: no
/// entry of podrunner stays in `TMPDIR`.
#[test]
fn a_third_signal_leaves_no_process_and_no_file() {
    let dir = Scratch::new("run-third");
    runner(&dir, true);
    let (run, rx, reader) = start(&dir);
    let pid = started(&dir);
    signal(&run, libc::SIGINT);
    line(&rx, "podrunner: stopping: the running job ends");
    signal(&run, libc::SIGINT);
    line(&rx, "podrunner: stopping: already stopping");
    signal(&run, libc::SIGINT);
    let out = run.wait_with_output().unwrap();
    // The end of stderr: the helper ended too.
    let err = reader.join().unwrap();
    assert_eq!(out.status.code(), Some(130), "{err}");
    assert!(err.contains("podrunner: ended at once"), "{err}");
    wait_until("end of the runner", || gone(pid));
    let left: Vec<String> = std::fs::read_dir(dir.join("tmp"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("podrunner-"))
        .collect();
    assert!(left.is_empty(), "the run left {left:?}\n{err}");
}
