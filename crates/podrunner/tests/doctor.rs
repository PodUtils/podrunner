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
    let dir = Scratch::new("doctor-confine");
    std::fs::create_dir_all(dir.join("store")).unwrap();
    let podvm = dir.join("podvm");
    podrunner_unit::program_file::write(
        &podvm,
        "#!/bin/sh\necho '{\"podvm\": \"0.2.0-beta.36\", \"rung\": \"namespace\"}'\n",
    )
    .unwrap();
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

/// A child of the binary in `dir` with the limit of file size `limit`
/// (soft, hard); None keeps the limit of this process.
fn limited(dir: &std::path::Path, args: &[&str], limit: Option<(u64, u64)>) -> std::process::Output {
    use std::os::unix::process::CommandExt;
    let mut c = Command::new(env!("CARGO_BIN_EXE_podrunner"));
    c.args(args).current_dir(dir).env("XDG_CACHE_HOME", dir.join("cache"));
    if let Some((soft, hard)) = limit {
        // SAFETY: setrlimit is safe to call between fork and exec.
        unsafe {
            c.pre_exec(move || {
                let r = libc::rlimit { rlim_cur: soft, rlim_max: hard };
                match libc::setrlimit(libc::RLIMIT_FSIZE, &r) {
                    0 => Ok(()),
                    _ => Err(std::io::Error::last_os_error()),
                }
            });
        }
    }
    c.output().unwrap()
}

/// The hard limit of file size of this process: a child cannot raise it.
fn hard_limit() -> u64 {
    let mut r = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    // SAFETY: getrlimit writes into the struct that this function owns.
    unsafe { libc::getrlimit(libc::RLIMIT_FSIZE, &mut r) };
    r.rlim_max
}

const GIB: u64 = 1 << 30;

/// T-632, T-144: the runner of `--runner-dir` runs with W^X off, so a
/// limit of file size does not stop it. `ulimit -f 1048576` (1 GiB) does
/// not refuse its dry run, and a smaller limit only warns.
#[test]
fn the_host_route_is_not_refused_under_a_limit_of_file_size() {
    let dir = Scratch::new("t632-fsize-host");
    std::fs::create_dir_all(dir.join("runner/bin")).unwrap();
    std::fs::write(dir.join("runner/run.sh"), "").unwrap();
    std::fs::write(dir.join("runner/bin/Runner.Listener"), "").unwrap();
    std::fs::write(dir.join("w.yml"), "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n").unwrap();
    let args = ["run", "w.yml", "--runner-dir", "runner", "--dry-run", "--no-config"];
    let hard = hard_limit();
    for soft in [GIB, 100_000 * 1024] {
        let out = limited(&dir, &args, Some((soft.min(hard), hard)));
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(0), "{soft}: {err}");
        assert!(err.contains("warning: the limit of file size (ulimit -f) is"), "{soft}: {err}");
    }
    let out = limited(&dir, &args, None);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
}

/// A stand-in podvm in `dir` whose store holds IMAGE of `size` bytes.
fn podvm_of_size(dir: &std::path::Path, size: u64) -> String {
    let p = dir.join("podvm");
    podrunner_unit::program_file::write(
        &p,
        format!(
            "#!/bin/sh\ncase \"$1\" in probe) echo '{{\"podvm\":\"0.2.0-beta.39\",\"rung\":\"userland\"}}';; \
             inspect) [ \"$2\" = --format ] && echo {size}; exit 0;; esac\n"
        ),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("store")).unwrap();
    p.display().to_string()
}

const IMAGE: &str = "runner@sha256:1111111111111111111111111111111111111111111111111111111111111111";

/// T-632: a podvm unit saves and loads the tar of its image, so a limit
/// below the size of the image (plus a margin) refuses the run, with the
/// size and the fix; a limit above it does not refuse.
#[test]
fn a_podvm_route_is_refused_under_a_limit_below_its_image() {
    let dir = Scratch::new("t632-fsize-podvm");
    std::fs::write(dir.join("w.yml"), "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n").unwrap();
    let p = podvm_of_size(&dir, 300_000_000);
    let args = ["run", "w.yml", "--podvm", &p, "--images", "store", "--image", IMAGE, "--dry-run", "--no-config"];
    let hard = hard_limit();
    if hard < GIB {
        eprintln!("skip: the hard limit of file size of this process is {hard} bytes");
        return;
    }
    let out = limited(&dir, &args, Some((200_000_000, hard)));
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(78), "{err}");
    assert!(
        err.contains("the limit of file size (ulimit -f) is 200000000 bytes")
            && err.contains("the image runner@sha256:1111")
            && err.contains("300000000 bytes")
            && err.contains("SIGXFSZ")
            && err.contains("fix: ulimit -f"),
        "{err}"
    );
    assert!(!err.contains("root"), "{err}");
    // 1 GiB holds an image of 300 MB: the threshold is from the image.
    let out = limited(&dir, &args, Some((GIB, hard)));
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    // The tar of the image cache gives the size when it is there.
    let tars = dir.join("cache/podrunner/image-tars");
    std::fs::create_dir_all(&tars).unwrap();
    let tar = std::fs::File::create(tars.join(&IMAGE[IMAGE.len() - 64..]).with_extension("tar")).unwrap();
    tar.set_len(2 * GIB).unwrap();
    let out = limited(&dir, &args, Some((GIB, hard)));
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(78), "{err}");
    assert!(err.contains(&format!("{} bytes", 2 * GIB)), "{err}");
}

/// T-632: when the hard limit of file size is below what the route needs,
/// only root can raise it; the text says so and names `--runner-dir`.
#[test]
fn a_hard_limit_below_the_image_names_root_and_the_host_route() {
    let dir = Scratch::new("t632-fsize-hard");
    std::fs::write(dir.join("w.yml"), "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n").unwrap();
    let p = podvm_of_size(&dir, 300_000_000);
    let args = ["run", "w.yml", "--podvm", &p, "--images", "store", "--image", IMAGE, "--dry-run", "--no-config"];
    let out = limited(&dir, &args, Some((200_000_000, 200_000_000)));
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(78), "{err}");
    assert!(
        err.contains("the hard limit is 200000000 bytes")
            && err.contains("only root can raise")
            && err.contains("--runner-dir"),
        "{err}"
    );
    assert!(!err.contains("fix: ulimit -f unlimited"), "{err}");
}

/// T-680: `doctor --run` takes the seccomp hold of `--podnet` by the same
/// probe as `run`: a podnet with no usable seccomp hold fails the row with
/// the text of `run`, and a usable one passes both.
#[test]
fn doctor_run_and_run_agree_on_the_podnet_hold() {
    let dir = Scratch::new("doctor-podnet");
    std::fs::create_dir_all(dir.join("store")).unwrap();
    std::fs::write(dir.join("w.yml"), "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n").unwrap();
    let podvm = dir.join("podvm");
    let script = "#!/bin/sh\necho '{\"podvm\": \"0.2.0-beta.32\", \"rung\": \"namespace\"}'\n";
    podrunner_unit::program_file::write(&podvm, script).unwrap();
    let image = format!("runner@sha256:{}", "1".repeat(64));
    let (w, p, s) = (dir.join("w.yml"), podvm.display().to_string(), dir.join("store").display().to_string());
    let both = |podnet: &std::path::Path| {
        let n = podnet.display().to_string();
        let common = |verb: &str| {
            let mut a = vec![verb.to_string()];
            if verb == "doctor" {
                a.push("--run".into());
            }
            for x in [w.to_str().unwrap(), "--podvm", &p, "--images", &s, "--image", &image, "--podnet", &n] {
                a.push(x.to_string());
            }
            a
        };
        let doctor = podrunner::cli::run(&common("doctor"));
        let run = podrunner::cli::run(&common("run"));
        let row = doctor.stdout.lines().find(|l| l.contains(" podnet:")).unwrap_or("").to_string();
        (row, run.stderr)
    };
    // netns available, seccomp unavailable: both refuse, with one text.
    let held = r#"{"netns":{"level":"enforced"},"seccomp":{"level":"unavailable"}}"#;
    let bad = dir.join("podnet-bad");
    podrunner_unit::program_file::write(&bad, format!("#!/bin/sh\necho '{held}'\n")).unwrap();
    let (row, run) = both(&bad);
    assert!(row.starts_with("FAIL podnet"), "{row}");
    let why = row.strip_prefix("FAIL podnet: ").unwrap();
    assert!(why.contains("--podnet: ") && why.contains("the seccomp hold is unavailable"), "{row}");
    assert!(run.contains(why), "doctor: {row}\nrun: {run}");
    // A usable seccomp hold: doctor passes the row, and run does not stop at it.
    let good = dir.join("podnet-good");
    let ok = r#"{"netns":{"level":"none"},"seccomp":{"level":"advisory"}}"#;
    podrunner_unit::program_file::write(&good, format!("#!/bin/sh\necho '{ok}'\n")).unwrap();
    let (row, run) = both(&good);
    assert!(row.starts_with("ok   podnet") && row.contains("seccomp hold"), "{row}");
    assert!(!run.contains("--podnet:"), "{run}");
}

/// T-695: the `proxy` row of `doctor --run` names the CA file that Node
/// gets: `SSL_CERT_FILE` (T-642), or the `NODE_EXTRA_CA_CERTS` of the user,
/// which wins; with neither it names no Node.
#[test]
fn doctor_run_names_the_ca_file_that_node_gets() {
    let dir = Scratch::new("doctor-node-ca");
    std::fs::create_dir_all(dir.join("runner/bin")).unwrap();
    std::fs::write(dir.join("runner/run.sh"), "").unwrap();
    std::fs::write(dir.join("runner/bin/Runner.Listener"), "").unwrap();
    std::fs::write(dir.join("w.yml"), "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n").unwrap();
    let row = |vars: &[(&str, &str)]| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_podrunner"));
        c.args(["doctor", "--run"]).arg(dir.join("w.yml")).arg("--runner-dir").arg(dir.join("runner"));
        // Only the variables of the case: each CA and proxy variable of
        // the host would change the row.
        c.env_clear();
        for k in ["PATH", "TMPDIR"] {
            if let Some(v) = std::env::var_os(k) {
                c.env(k, v);
            }
        }
        let out = c.envs(vars.iter().copied()).output().unwrap();
        let text = String::from_utf8(out.stdout).unwrap();
        text.lines().find(|l| l.contains(" proxy:")).unwrap_or_else(|| panic!("no proxy row: {text}")).to_string()
    };
    let none = row(&[]);
    assert!(none.contains("direct egress") && !none.contains("Node"), "{none}");
    let ssl = row(&[("SSL_CERT_FILE", "/etc/made-up-ca.pem")]);
    assert!(ssl.contains("Node gets /etc/made-up-ca.pem (NODE_EXTRA_CA_CERTS from SSL_CERT_FILE)"), "{ssl}");
    let both = row(&[("SSL_CERT_FILE", "/etc/made-up-ca.pem"), ("NODE_EXTRA_CA_CERTS", "/etc/node-ca.pem")]);
    assert!(both.contains("Node gets /etc/node-ca.pem (NODE_EXTRA_CA_CERTS of the user wins)"), "{both}");
    // An empty value of the user does not win: `run` takes SSL_CERT_FILE.
    let empty = row(&[("SSL_CERT_FILE", "/etc/made-up-ca.pem"), ("NODE_EXTRA_CA_CERTS", "")]);
    assert!(empty.contains("Node gets /etc/made-up-ca.pem"), "{empty}");
    // The user sets only the variable of Node: no line of a copy.
    let node = row(&[("NODE_EXTRA_CA_CERTS", "/etc/node-ca.pem")]);
    assert!(node.contains("takes NODE_EXTRA_CA_CERTS") && !node.contains("Node gets"), "{node}");
}

/// T-635: `doctor --run` checks the proxy and the CA files of a job as
/// `run` takes them, on the podvm form too: a CA file that is not there
/// fails a row (the run would fail), a `NO_PROXY` entry that the runner
/// cannot read is named (the run drops it), and a held unit takes no proxy.
/// A workflow file that is not there gives no lock path.
#[test]
fn doctor_run_checks_the_proxy_and_the_ca_files_of_each_form() {
    let dir = Scratch::new("doctor-proxy-forms");
    std::fs::create_dir_all(dir.join("runner/bin")).unwrap();
    std::fs::create_dir_all(dir.join("store")).unwrap();
    std::fs::write(dir.join("runner/run.sh"), "").unwrap();
    std::fs::write(dir.join("runner/bin/Runner.Listener"), "").unwrap();
    std::fs::write(dir.join("ca.pem"), "made-up CA\n").unwrap();
    std::fs::write(dir.join("w.yml"), "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n").unwrap();
    let podvm = dir.join("podvm");
    let script = "#!/bin/sh\necho '{\"podvm\": \"0.2.0-beta.32\", \"rung\": \"namespace\"}'\n";
    podrunner_unit::program_file::write(&podvm, script).unwrap();
    let podnet = dir.join("podnet");
    let held = r#"{"netns":{"level":"none"},"seccomp":{"level":"advisory"}}"#;
    podrunner_unit::program_file::write(&podnet, format!("#!/bin/sh\necho '{held}'\n")).unwrap();
    let image = format!("runner@sha256:{}", "1".repeat(64));
    let runner_dir = ["--runner-dir".to_string(), dir.join("runner").display().to_string()];
    let unit = [
        "--podvm".to_string(),
        podvm.display().to_string(),
        "--images".into(),
        dir.join("store").display().to_string(),
        "--image".into(),
        image,
    ];
    let mut with_podnet = unit.to_vec();
    with_podnet.extend(["--podnet".to_string(), podnet.display().to_string()]);
    let rows = |file: &std::path::Path, form: &[String], vars: &[(&str, String)]| {
        let mut c = Command::new(env!("CARGO_BIN_EXE_podrunner"));
        c.args(["doctor", "--run"]).arg(file).args(form);
        // Only the variables of the case.
        c.env_clear().env("HOME", &*dir).env("XDG_CONFIG_HOME", dir.join("config"));
        for k in ["PATH", "TMPDIR"] {
            if let Some(v) = std::env::var_os(k) {
                c.env(k, v);
            }
        }
        let out = c.envs(vars.iter().map(|(k, v)| (*k, v))).output().unwrap();
        String::from_utf8(out.stdout).unwrap()
    };
    let row = |text: &str, name: &str| {
        let at = |l: &&str| l.get(4..).is_some_and(|r| r.starts_with(&format!(" {name}:")));
        text.lines().find(at).unwrap_or("").to_string()
    };
    let w = dir.join("w.yml");
    let ca = dir.join("ca.pem").display().to_string();
    let gone = dir.join("gone.pem").display().to_string();
    let proxy = ("HTTPS_PROXY", "http://127.0.0.1:3128".to_string());
    for form in [&runner_dir[..], &unit[..]] {
        let text = rows(&w, form, &[proxy.clone(), ("SSL_CERT_FILE", ca.clone())]);
        let p = row(&text, "proxy");
        assert!(
            p.starts_with("ok   proxy") && p.contains("HTTPS_PROXY") && p.contains("SSL_CERT_FILE"),
            "{form:?}: {text}"
        );
        assert!(row(&text, "ca-files").starts_with("ok   ca-files"), "{form:?}: {text}");
        let text = rows(&w, form, &[proxy.clone(), ("SSL_CERT_FILE", gone.clone())]);
        let c = row(&text, "ca-files");
        assert!(c.starts_with("FAIL ca-files") && c.contains(&format!("SSL_CERT_FILE={gone}")), "{form:?}: {text}");
        let text = rows(&w, form, &[proxy.clone(), ("NO_PROXY", "::,intra.example".into())]);
        let p = row(&text, "proxy");
        assert!(p.contains("NO_PROXY: the job does not get the entry \"::\""), "{form:?}: {text}");
    }
    // A held unit takes the CA files, not the proxy: its hold is its way out.
    let text = rows(&w, &with_podnet, &[proxy.clone(), ("SSL_CERT_FILE", ca.clone()), ("NO_PROXY", "::".into())]);
    let p = row(&text, "proxy");
    assert!(p.contains("takes SSL_CERT_FILE of the environment") && !p.contains("HTTPS_PROXY"), "{text}");
    assert!(!p.contains("NO_PROXY"), "{text}");
    // No workflow file: the lock row names no path beside it.
    let text = rows(&dir.join("none.yml"), &runner_dir, &[]);
    let lock = row(&text, "lock");
    assert!(lock.contains("not checked") && !lock.contains("podrunner-actions.lock"), "{text}");
    assert!(row(&text, "workflow").starts_with("FAIL workflow"), "{text}");
}
