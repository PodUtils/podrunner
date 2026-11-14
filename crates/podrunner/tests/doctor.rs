//! `podrunner doctor --json` runs the rows of `pod-hostprobe` in children
//! of the binary (T-070): each row has a verdict, and no child fails to
//! find its probe.

use std::process::Command;

#[test]
fn doctor_runs_the_probe_rows_in_children_of_the_binary() {
    let out = Command::new(env!("CARGO_BIN_EXE_podrunner")).args(["doctor", "--json"]).output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = v["host"].as_array().unwrap();
    assert_eq!(rows.len(), podrunner::doctor::HOST_ROWS.len(), "{v}");
    for r in rows {
        let verdict = r["verdict"].as_str().unwrap();
        assert!(["ok", "denied", "skip"].contains(&verdict), "{r}");
        // A child that did not run the probe gives a skip with this reason.
        assert!(!r["reason"].as_str().unwrap_or("").contains("no probe named"), "{r}");
    }
    let text = Command::new(env!("CARGO_BIN_EXE_podrunner")).arg("doctor").output().unwrap();
    let text = String::from_utf8(text.stdout).unwrap();
    assert!(text.contains("probe chroot(/tmp): "), "{text}");
}

/// T-153: `doctor --run` checks the inputs of a local run; a check that
/// fails names its fix and the exit is 1.
#[test]
fn doctor_run_checks_the_inputs_of_a_local_run() {
    let dir = std::env::temp_dir().join(format!("podrunner-doctor-run-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("runner/bin")).unwrap();
    std::fs::write(dir.join("runner/run.sh"), "").unwrap();
    std::fs::write(dir.join("runner/bin/Runner.Listener"), "").unwrap();
    std::fs::write(dir.join("w.yml"), "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n").unwrap();
    let w = dir.join("w.yml").display().to_string();
    let r = dir.join("runner").display().to_string();
    let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<String>>();
    let out = podrunner::cli::run(&args(&["doctor", "--run", &w, "--runner-dir", &r]));
    assert!(out.stdout.contains("ok   runner-dir") && out.stdout.contains("ok   workflow"), "{}", out.stdout);
    let missing = dir.join("none").display().to_string();
    let out = podrunner::cli::run(&args(&["doctor", "--run", &w, "--runner-dir", &missing]));
    assert_eq!(out.code, 1, "{}", out.stdout);
    assert!(out.stdout.contains("FAIL runner-dir") && out.stdout.contains("fix:"), "{}", out.stdout);
    let _ = std::fs::remove_dir_all(&dir);
}
