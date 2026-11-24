//! T-639: a second SIGINT during the stop of `podrunner run` is not lost:
//! it prints a line, and the run still ends with 130.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use scratch::Scratch;

mod scratch;

#[test]
fn a_second_signal_says_that_the_stop_goes_on() {
    let dir = Scratch::new("run-signal");
    // A runner that never reaches its broker: the job runs until the cancel.
    std::fs::create_dir_all(dir.join("r/bin")).unwrap();
    podrunner_unit::program_file::write(dir.join("r/run.sh"), "#!/bin/sh\nexec sleep 30\n").unwrap();
    std::fs::write(dir.join("r/bin/Runner.Listener"), "").unwrap();
    std::fs::write(dir.join("w.yml"), "on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n      - run: sleep 30\n")
        .unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_podrunner"))
        .args(["run", "w.yml", "--runner-dir", "r"])
        .current_dir(&*dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_secs(2));
    let t = Instant::now();
    for _ in 0..2 {
        // SAFETY: kill(2) sends SIGINT to the child that this test started.
        unsafe { libc::kill(run.id() as libc::pid_t, libc::SIGINT) };
        std::thread::sleep(Duration::from_millis(300));
    }
    let out = run.wait_with_output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(130), "{err}");
    assert!(t.elapsed() < Duration::from_secs(20), "{:?}", t.elapsed());
    assert!(err.contains("podrunner: stopping: the running job ends"), "{err}");
    assert!(err.contains("podrunner: stopping: already stopping; a third signal ends podrunner at once"), "{err}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("cancelled a"));
}
