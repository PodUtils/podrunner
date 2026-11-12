use pod_actions_workflow::JobResult;
use podrunner_local::drive::Line;
use podrunner_schema::JobId;

use super::*;

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

#[test]
fn the_options_of_run() {
    let a =
        parse(&args(&["w.yml", "--runner-dir", "/r", "--var", "A=1", "--input", "x=y=z", "--event", "pull_request"]))
            .unwrap();
    assert_eq!(a.file, PathBuf::from("w.yml"));
    assert_eq!(a.runner_dir, Some(PathBuf::from("/r")));
    assert_eq!(a.vars, [("A".into(), "1".into())]);
    assert_eq!(a.inputs, [("x".into(), "y=z".into())]);
    assert_eq!(a.event, "pull_request");
    assert_eq!(parse(&args(&["w.yml", "--runner-dir", "/r"])).unwrap().event, "push");
    let with = parse(&args(&["w.yml", "--runner-dir", "/r", "--actions", "/a"])).unwrap();
    assert_eq!(with.actions, Some(PathBuf::from("/a")));
    let p = parse(&args(&["w.yml", "--podvm", "/p", "--images", "/s", "--image", "r@sha256:00"])).unwrap();
    assert_eq!((p.podvm.as_deref(), p.image.as_deref()), (Some(std::path::Path::new("/p")), Some("r@sha256:00")));
    // One executor: a runner directory, or the three options of podvm.
    for bad in [
        &["w.yml"][..],
        &["w.yml", "--podvm", "/p", "--images", "/s"],
        &["w.yml", "--runner-dir", "/r", "--podvm", "/p", "--images", "/s", "--image", "r@sha256:00"],
    ] {
        assert!(parse(&args(bad)).unwrap_err().contains("--runner-dir"), "{bad:?}");
    }
    for bad in [
        &["--runner-dir"][..],
        &["a", "b"],
        &["w", "--var", "1x=2"],
        &["w", "--var", "novalue"],
        &["w", "--x", "y"],
        &[],
    ] {
        assert!(parse(&args(bad)).is_err(), "{bad:?}");
    }
}

#[test]
fn run_refuses_a_bad_call_with_the_class_of_its_failure() {
    assert_eq!(run(&args(&["w.yml"])).code, Class::Usage.exit_code(), "no --runner-dir");
    let dir = std::env::temp_dir().join(format!("podrunner-run-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let missing = dir.join("missing.yml");
    let o = run(&args(&[missing.to_str().unwrap(), "--runner-dir", "/nonexistent"]));
    assert_eq!(o.code, Class::NotFound.exit_code(), "{}", o.stderr);
    let bad = dir.join("bad.yml");
    std::fs::write(&bad, "on: push\n").unwrap();
    let o = run(&args(&[bad.to_str().unwrap(), "--runner-dir", "/nonexistent"]));
    assert_eq!(o.code, Class::Config.exit_code(), "{}", o.stderr);
    let good = dir.join("good.yml");
    std::fs::write(&good, "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n").unwrap();
    let o = run(&args(&[good.to_str().unwrap(), "--runner-dir", dir.to_str().unwrap()]));
    assert_eq!(o.code, Class::Config.exit_code(), "no runner: {}", o.stderr);
    assert!(o.stderr.contains("no unpacked runner"), "{}", o.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn each_line_names_the_job_and_its_result() {
    let id = JobId::new("a.0").unwrap();
    let ran = |result| Line::Ran { job: id.clone(), name: "build (x)".into(), result };
    assert_eq!(line(&ran(JobResult::Success)), "ok      build (x)\n");
    assert_eq!(line(&ran(JobResult::Failure)), "failure build (x)\n");
    let refused = Line::Refused { job: id.clone(), name: "b".into(), reason: "an action".into() };
    assert_eq!(line(&refused), "failure b: an action\n");
    assert_eq!(line(&Line::Skipped { job: "c".into(), condition: "failure()".into() }), "skipped c: if: failure()\n");
}

#[test]
fn the_github_context_names_the_event_and_the_file() {
    let g = github(std::path::Path::new("/x/ci.yml"), "push");
    assert_eq!(g.get("event_name").and_then(Value::as_str), Some("push"));
    assert_eq!(g.get("workflow").and_then(Value::as_str), Some("ci.yml"));
}

#[test]
fn the_tree_of_the_checkout_is_a_tarball_of_its_files() {
    let dir = std::env::temp_dir().join(format!("podrunner-tree-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("plain/sub")).unwrap();
    std::fs::write(dir.join("plain/sub/f.txt"), "x").unwrap();
    let t = tree_tarball(&dir.join("plain"), &dir.join("scratch")).unwrap();
    let list = std::process::Command::new("tar").arg("-tzf").arg(&t).output().unwrap();
    assert!(String::from_utf8_lossy(&list.stdout).contains("sub/f.txt"));
    // A git tree gives its committed files.
    let repo = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let t = tree_tarball(std::path::Path::new(repo), &dir.join("scratch2")).unwrap();
    let list = std::process::Command::new("tar").arg("-tzf").arg(&t).output().unwrap();
    assert!(String::from_utf8_lossy(&list.stdout).lines().any(|l| l == "Cargo.toml"));
    assert!(tree_tarball(&dir.join("missing"), &dir.join("scratch3")).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
