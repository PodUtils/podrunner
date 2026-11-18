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
    assert!(text.contains("probe chroot(/tmp) in unshare(NEWUSER+maps): "), "{text}");
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
    // T-342: `--json` gives one object, before or after `--run`.
    for a in [
        vec!["doctor", "--run", &w, "--runner-dir", &missing, "--json"],
        vec!["doctor", "--json", "--run", &w, "--runner-dir", &missing],
    ] {
        let out = podrunner::cli::run(&args(&a));
        assert_eq!(out.code, 1, "{a:?}: {}{}", out.stdout, out.stderr);
        let v: serde_json::Value =
            serde_json::from_str(out.stdout.trim()).unwrap_or_else(|e| panic!("{e}: {}", out.stdout));
        assert_eq!(v["ok"], false);
        assert!(v["checks"].as_array().unwrap().iter().any(|c| c["name"] == "runner-dir" && c["ok"] == false), "{v}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-202: `doctor --run` reads the file of the user `run.toml` as `run`
/// does, so the installer can check the run that it configured.
#[test]
fn doctor_run_reads_the_run_toml_of_the_user() {
    let dir = std::env::temp_dir().join(format!("podrunner-doctor-toml-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("runner/bin")).unwrap();
    std::fs::create_dir_all(dir.join("config/podrunner")).unwrap();
    std::fs::write(dir.join("runner/run.sh"), "").unwrap();
    std::fs::write(dir.join("runner/bin/Runner.Listener"), "").unwrap();
    std::fs::write(dir.join("w.yml"), "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n").unwrap();
    std::fs::write(dir.join("config/podrunner/run.toml"), "runner-dir = \"../../runner\"\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_podrunner"))
        .args(["doctor", "--run", "w.yml"])
        .current_dir(&dir)
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env("HOME", &dir)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{text}{}", String::from_utf8_lossy(&out.stderr));
    assert!(text.contains("ok   runner-dir"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-499: `doctor --config` names the runner of each pin, its age and its
/// window, and the runner of this build.
#[test]
fn doctor_names_the_runner_of_each_pin_and_its_age() {
    let dir = std::env::temp_dir().join(format!("podrunner-doctor-pins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let config = format!(
        "host = \"h\"\n[pin]\n[[pin.payload]]\narch = \"x86_64\"\nreference = \"runner@sha256:{}\"\n\
         built_at_s = {}\nrunner_version = \"2.338.2\"\n",
        "1".repeat(64),
        now - 25 * 24 * 3600
    );
    std::fs::write(dir.join("p.toml"), config).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_podrunner"))
        .args(["doctor", "--json", "--config"])
        .arg(dir.join("p.toml"))
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let pin = &v["pins"][0];
    assert_eq!((pin["runner_version"].as_str(), pin["state"].as_str()), (Some("2.338.2"), Some("near-end")), "{v}");
    assert!(pin["age_s"].as_u64().unwrap() >= 25 * 24 * 3600, "{v}");
    assert!(v["runner_of_this_build"].as_str().is_some_and(|r| r.starts_with("2.")), "{v}");
    let _ = std::fs::remove_dir_all(&dir);
}
