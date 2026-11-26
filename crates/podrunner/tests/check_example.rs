//! T-642: the example of `docs/examples/podrunner.toml` gives a job when
//! its `script` key is on.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_podrunner");

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("podrunner-check-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// T-642: the example gives a job when its `script` key is on: the file
/// of jobs of the example reads, and `check` builds the source with it.
#[test]
fn the_example_gives_a_job_with_its_script_key_on() {
    let d = dir("example-script");
    let text = std::fs::read_to_string(root().join("docs/examples/podrunner.toml")).unwrap();
    let script = root().join("docs/examples/fake-jobs.jsonl");
    let on =
        text.replace("# script = \"docs/examples/fake-jobs.jsonl\"", &format!("script = \"{}\"", script.display()));
    assert_ne!(on, text, "the example has the key as a comment");
    let steps = podrunner_fake::parse_script(&std::fs::read_to_string(&script).unwrap()).unwrap();
    assert_eq!(steps.len(), 1, "one job");
    let file = d.join("podrunner.toml");
    std::fs::write(&file, &on).unwrap();
    let run = |f: &Path| {
        Command::new(BIN).args(["check", "--config"]).arg(f).env("HTTPS_PROXY", "http://127.0.0.1:1").output().unwrap()
    };
    let out = run(&file);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    // A planted defect: a script that does not read fails the same check.
    let bad = d.join("bad.jsonl");
    std::fs::write(&bad, "{\"at_s\":0,\"offer\":\n").unwrap();
    std::fs::write(&file, on.replace(&script.display().to_string(), &bad.display().to_string())).unwrap();
    let out = run(&file);
    assert_eq!(out.status.code(), Some(78), "{}", String::from_utf8_lossy(&out.stderr));
    let _ = std::fs::remove_dir_all(&d);
}
