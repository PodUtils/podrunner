//! T-682: the job-started hook finds `Runner.Worker` and `Runner.Listener`
//! among its ancestors also when the dynamic loader of the image runs each
//! program (the rungs of podvm below the namespace rung): the comm is then
//! the name of the loader. The test runs the function `named` of the hook
//! script against a process that the loader of the host runs.

use std::process::{Command, Stdio};

const SCRIPT: &str = include_str!("../../../images/runner/podrunner-job-started.sh");

/// The function `named` of the hook script, as text.
fn named() -> &'static str {
    let start = SCRIPT.find("named() {").expect("the hook defines named");
    let end = start + SCRIPT[start..].find("\n}\n").expect("the end of named") + 3;
    &SCRIPT[start..end]
}

/// Whether `named NAME PID` of the hook holds.
fn is(name: &str, pid: u32) -> bool {
    let script = format!("{}\nnamed \"$1\" \"$2\"", named());
    Command::new("sh").args(["-c", &script, "sh", name, &pid.to_string()]).status().unwrap().success()
}

#[test]
fn the_hook_names_a_process_that_the_loader_runs() {
    let loader = ["/lib64/ld-linux-x86-64.so.2", "/lib/ld-linux-aarch64.so.1"]
        .into_iter()
        .find(|l| std::path::Path::new(l).exists());
    let dir = std::env::temp_dir().join(format!("podrunner-hook-named-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let worker = dir.join("Runner.Worker");
    std::fs::copy("/bin/sleep", &worker).unwrap();
    // Directly: the comm is the name.
    let mut direct = Command::new(&worker).arg("30").stdout(Stdio::null()).spawn().unwrap();
    assert!(is("Runner.Worker", direct.id()));
    assert!(!is("Runner.Listener", direct.id()));
    let _ = direct.kill();
    let _ = direct.wait();
    // Through a loader of glibc, where the host has one: the comm is the
    // name of the loader, and an argument names the program.
    if let Some(loader) = loader {
        let mut child = Command::new(loader).arg(&worker).arg("30").stdout(Stdio::null()).spawn().unwrap();
        let comm = std::fs::read_to_string(format!("/proc/{}/comm", child.id())).unwrap_or_default();
        let named = is("Runner.Worker", child.id());
        let other = is("Runner.Listener", child.id());
        let _ = child.kill();
        let _ = child.wait();
        assert_ne!(comm.trim(), "Runner.Worker", "the loader names the process");
        assert!(named, "the hook does not find a worker that the loader runs");
        assert!(!other);
    }
    // A process that is gone is no name.
    assert!(!is("Runner.Worker", u32::MAX));
    let _ = std::fs::remove_dir_all(&dir);
}
