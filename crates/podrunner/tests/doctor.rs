//! `podrunner doctor --json` runs the rows of `pod-hostprobe` in children
//! of the binary (T-070): each row has a verdict, and no child fails to
//! find its probe.

use std::process::Command;

use scratch::Scratch;

mod scratch;

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
    let dir = Scratch::new("doctor-run");
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
    let dir = Scratch::new("doctor-toml");
    std::fs::create_dir_all(dir.join("runner/bin")).unwrap();
    std::fs::create_dir_all(dir.join("config/podrunner")).unwrap();
    std::fs::write(dir.join("runner/run.sh"), "").unwrap();
    std::fs::write(dir.join("runner/bin/Runner.Listener"), "").unwrap();
    std::fs::write(dir.join("w.yml"), "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n").unwrap();
    std::fs::write(dir.join("config/podrunner/run.toml"), "runner-dir = \"../../runner\"\n").unwrap();
    // The free space of the scratch disk is a fact of the host, not of
    // this test (T-589): a TMPDIR that is not there fails that row as a
    // small `/tmp` does.
    for tmp in [dir.to_path_buf(), dir.join("missing")] {
        let out = Command::new(env!("CARGO_BIN_EXE_podrunner"))
            .args(["doctor", "--run", "w.yml"])
            .current_dir(&dir)
            .env("XDG_CONFIG_HOME", dir.join("config"))
            .env("HOME", &*dir)
            .env("TMPDIR", &tmp)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("ok   runner-dir"), "{text}");
        let failed: Vec<&str> = text.lines().filter(|l| l.starts_with("FAIL")).collect();
        assert!(failed.iter().all(|l| l.starts_with("FAIL scratch-disk:")), "{text}");
        let code = if failed.is_empty() { 0 } else { 1 };
        assert_eq!(out.status.code(), Some(code), "{text}{}", String::from_utf8_lossy(&out.stderr));
        if tmp.ends_with("missing") {
            assert_eq!(failed.len(), 1, "{text}");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-499: `doctor --config` names the runner of each pin, its age and its
/// window, and the runner of this build.
#[test]
fn doctor_names_the_runner_of_each_pin_and_its_age() {
    let dir = Scratch::new("doctor-pins");
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

/// T-566: doctor runs the self-test of podvm's confine layer that serve
/// runs, so it says what serve will do: a stand-in podvm that holds no
/// image proves nothing, the kind is missing with that cause, and the
/// trusted source is not served.
#[test]
fn doctor_runs_the_self_test_of_the_confine_layer() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new("doctor-confine");
    std::fs::create_dir_all(dir.join("store")).unwrap();
    let podvm = dir.join("podvm");
    std::fs::write(&podvm, "#!/bin/sh\necho '{\"podvm\": \"0.2.0-beta.36\", \"rung\": \"namespace\"}'\n").unwrap();
    std::fs::set_permissions(&podvm, std::fs::Permissions::from_mode(0o755)).unwrap();
    let config = format!(
        "host = \"h\"\nallow_fake = true\n[[source]]\nname = \"s\"\nkind = \"fake\"\ntier = \"trusted\"\narch = \"{arch}\"\n\
         [unit.podvm]\nslots = 1\nbinary = \"{podvm}\"\nimages = \"{store}\"\nconfine = true\n\
         [pin]\n[[pin.payload]]\narch = \"{arch}\"\nreference = \"runner@sha256:{sha}\"\nbuilt_at_s = 1795000000\n",
        arch = std::env::consts::ARCH,
        podvm = podvm.display(),
        store = dir.join("store").display(),
        sha = "1".repeat(64),
    );
    std::fs::write(dir.join("c.toml"), config).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_podrunner"))
        .args(["doctor", "--json", "--config"])
        .arg(dir.join("c.toml"))
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let kind = v["kinds"].as_array().unwrap().iter().find(|k| k["kind"] == "podvm").unwrap().clone();
    let why = kind["missing"].as_str().unwrap_or_default();
    assert!(why.contains("the confine layer that a trusted source needs is not proven here"), "{v}");
    assert_eq!(v["sources"][0]["served"], false, "{v}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-632: a small limit of file size ends the runner and a podvm load by
/// SIGXFSZ. `run` (and its dry run) refuses it before the first unit, with
/// the fix that `doctor --run` gives; with no limit the dry run plans.
#[test]
fn run_refuses_a_small_limit_of_file_size() {
    use std::os::unix::process::CommandExt;
    let dir = Scratch::new("t632-fsize");
    std::fs::create_dir_all(dir.join("runner/bin")).unwrap();
    std::fs::write(dir.join("runner/run.sh"), "").unwrap();
    std::fs::write(dir.join("runner/bin/Runner.Listener"), "").unwrap();
    std::fs::write(dir.join("w.yml"), "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n").unwrap();
    let run = |limit: Option<u64>| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_podrunner"));
        c.args(["run", "w.yml", "--runner-dir", "runner", "--dry-run", "--no-config"]).current_dir(&*dir);
        if let Some(bytes) = limit {
            // SAFETY: setrlimit is safe to call between fork and exec.
            unsafe {
                c.pre_exec(move || {
                    let mut r = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
                    libc::getrlimit(libc::RLIMIT_FSIZE, &mut r);
                    r.rlim_cur = bytes;
                    match libc::setrlimit(libc::RLIMIT_FSIZE, &r) {
                        0 => Ok(()),
                        _ => Err(std::io::Error::last_os_error()),
                    }
                });
            }
        }
        c.output().unwrap()
    };
    let out = run(Some(100_000 * 1024));
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(78), "{err}");
    assert!(err.contains("ulimit -f") && err.contains("SIGXFSZ") && err.contains("fix: ulimit -f unlimited"), "{err}");
    let out = run(None);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
}
