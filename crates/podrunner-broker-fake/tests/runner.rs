//! The official runner runs canned jobs against the fake broker, offline.
//! Ignored by default: it needs an unpacked release of `actions/runner`
//! (v2.338.2, `images/runner/runner.toml`) in a directory it may change,
//! and openssl for the key of the test.
//!
//!   PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner-broker-fake --test runner -- --ignored

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use podrunner_broker_fake::{Broker, Job, files::runner_files};

/// An RSA key of the test, in the form of `.credentials_rsaparams`.
fn rsaparams() -> String {
    let pem = Command::new("openssl").args(["genrsa", "-traditional", "2048"]).output().unwrap().stdout;
    let mut child = Command::new("openssl")
        .args(["rsa", "-text", "-noout"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(&pem).unwrap();
    let text = String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap();
    // Each field is hex bytes with colons over many lines; the exponent is
    // one decimal line.
    let mut fields = std::collections::BTreeMap::new();
    let mut name = String::new();
    for line in text.lines() {
        if let Some(e) = line.strip_prefix("publicExponent: ") {
            let n: u64 = e.split_whitespace().next().unwrap().parse().unwrap();
            let bytes: Vec<u8> = n.to_be_bytes().into_iter().skip_while(|b| *b == 0).collect();
            fields.insert("publicExponent".to_string(), bytes);
        } else if !line.starts_with(' ') && line.ends_with(':') {
            name = line.trim_end_matches(':').to_string();
            fields.insert(name.clone(), Vec::new());
        } else if line.starts_with(' ') {
            let bytes = line.trim().trim_end_matches(':').split(':').map(|h| u8::from_str_radix(h, 16).unwrap());
            fields.get_mut(&name).unwrap().extend(bytes);
        }
    }
    let b = |k: &str| {
        let v = &fields[k];
        // DER puts a 0 byte before a high bit; .NET wants the plain number.
        let v: Vec<u8> = v.iter().copied().skip_while(|x| *x == 0).collect();
        STANDARD.encode(v)
    };
    serde_json::json!({
        "d": b("privateExponent"), "dp": b("exponent1"), "dq": b("exponent2"), "exponent": b("publicExponent"),
        "inverseQ": b("coefficient"), "modulus": b("modulus"), "p": b("prime1"), "q": b("prime2"),
    })
    .to_string()
}

fn run_once(dir: &Path, broker: &Broker) -> std::process::ExitStatus {
    for (name, content) in runner_files(&broker.base, "runner-u-1", &rsaparams()) {
        std::fs::write(dir.join(name), content).unwrap();
    }
    // A clean environment: the runner refuses some proxy variables.
    Command::new("./run.sh")
        .current_dir(dir)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
        .env("RUNNER_ALLOW_RUNASROOT", "1")
        .status()
        .unwrap()
}

#[test]
#[ignore]
fn the_official_runner_runs_a_canned_job_offline() {
    let dir = std::path::PathBuf::from(std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR"));
    let broker = Broker::start().unwrap();
    // A computed marker: the echo of the script cannot match it.
    let ok = Job { name: "hello".into(), scripts: vec!["echo podrunner-step-$((6*7)) | tee marker.txt".into()] };
    broker.queue(ok);
    let _ = std::fs::remove_dir_all(dir.join("_work"));
    assert!(run_once(&dir, &broker).success());
    let done = broker.completions();
    assert_eq!(done.len(), 1, "{:?}", broker.seen());
    assert_eq!(done[0].conclusion, "succeeded");
    let marker = std::fs::read_to_string(dir.join("_work/app/app/marker.txt")).unwrap();
    assert_eq!(marker.trim(), "podrunner-step-42");
    // The output of the step reaches the results service as a log.
    assert!(broker.logs().contains("podrunner-step-42"), "{:?}", broker.seen());
    assert!(!broker.step_updates().is_empty());

    // A step that fails fails the job, and a later step does not run.
    let broker = Broker::start().unwrap();
    broker.queue(Job { name: "fails".into(), scripts: vec!["exit 3".into(), "echo no > second.txt".into()] });
    let _ = std::fs::remove_dir_all(dir.join("_work"));
    run_once(&dir, &broker);
    std::thread::sleep(Duration::from_millis(100));
    let done = broker.completions();
    assert_eq!(done[0].conclusion, "failed", "{done:?}");
    assert!(!dir.join("_work/app/app/second.txt").exists());
}
