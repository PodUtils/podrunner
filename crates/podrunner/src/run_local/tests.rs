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
    let held = parse(&args(&[
        "w.yml",
        "--podvm",
        "/p",
        "--images",
        "/s",
        "--image",
        "r@sha256:00",
        "--podnet",
        "/n",
        "--allow",
        "a.example",
        "--allow",
        "b.example",
    ]))
    .unwrap();
    assert_eq!((held.podnet.as_deref(), held.allow.len()), (Some(std::path::Path::new("/n")), 2));
    assert!(parse(&args(&["w.yml", "--runner-dir", "/r", "--podnet", "/n"])).unwrap_err().contains("--podvm"));
    assert!(
        parse(&args(&["w.yml", "--podvm", "/p", "--images", "/s", "--image", "r@sha256:00", "--allow", "a"]))
            .unwrap_err()
            .contains("--podnet")
    );
    assert_eq!(policy(&["a.example".into()]), r#"{"allow":[{"host":"a.example"}],"mode":"allow","schema":1}"#);
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
        &["w", "--runner-dir", "/r", "--cache", "/c", "--no-cache"],
        &["w", "--runner-dir", "/r", "--offline", "--frozen"],
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
fn the_github_context_comes_from_the_tree_and_the_flags() {
    for (url, want) in [
        ("https://github.com/o/r.git", "o/r"),
        ("git@github.com:o/r.git", "o/r"),
        ("ssh://git@host.example/o/r", "o/r"),
        ("https://host.example/group/o/r/", "o/r"),
    ] {
        assert_eq!(context::repository_of(url).as_deref(), Some(want), "{url}");
    }
    let dir = std::env::temp_dir().join(format!("podrunner-facts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let g = |a: &[&str]| {
        assert!(
            std::process::Command::new("git").arg("-C").arg(&dir).args(a).output().unwrap().status.success(),
            "{a:?}"
        )
    };
    g(&["init", "-q", "-b", "feature"]);
    g(&["-c", "user.name=Tester", "-c", "user.email=t@example.invalid", "commit", "-q", "--allow-empty", "-m", "x"]);
    g(&["config", "user.name", "Tester"]);
    g(&["remote", "add", "origin", "https://github.com/acme/app.git"]);
    let tree = dir.display().to_string();
    let a = parse(&args(&["w.yml", "--runner-dir", "/r", "--checkout", &tree])).unwrap();
    let f = context::facts(&a);
    assert_eq!(
        (f.repository.as_str(), f.git_ref.as_str(), f.actor.as_str()),
        ("acme/app", "refs/heads/feature", "Tester")
    );
    assert_eq!(f.sha.len(), 40);
    let a = parse(&args(&[
        "w.yml",
        "--runner-dir",
        "/r",
        "--checkout",
        &tree,
        "--ref",
        "refs/tags/v1",
        "--repository",
        "x/y",
    ]))
    .unwrap();
    let f = context::facts(&a);
    assert_eq!((f.repository.as_str(), f.git_ref.as_str()), ("x/y", "refs/tags/v1"), "the flags replace the tree");
    let gh = context::github(&a, &f).unwrap();
    assert_eq!(gh.get("ref_type").and_then(Value::as_str), Some("tag"));
    assert_eq!(gh.get("workflow").and_then(Value::as_str), Some("w.yml"));
    let push = context::payload("push", &f);
    assert_eq!((push["ref"].as_str(), push["after"].as_str()), (Some("refs/tags/v1"), Some(f.sha.as_str())));
    let a = parse(&args(&["w.yml", "--runner-dir", "/r", "--checkout", &tree, "--event", "pull_request"])).unwrap();
    let f = context::facts(&a);
    let gh = context::github(&a, &f).unwrap();
    assert_eq!(gh.get("head_ref").and_then(Value::as_str), Some("feature"));
    assert_eq!(gh.get("ref").and_then(Value::as_str), Some("refs/pull/1/merge"));
    let event = gh.get("event").unwrap();
    assert_eq!(event.get("pull_request").and_then(|p| p.get("number")), Some(&Value::Number(1.0)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_workflow_whose_on_does_not_take_the_event_is_refused() {
    let dir = std::env::temp_dir().join(format!("podrunner-on-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("w.yml");
    std::fs::write(&file, "on: [push, workflow_dispatch]\njobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n")
        .unwrap();
    let w = file.display().to_string();
    let out = run(&args(&["run", &w, "--runner-dir", "/nonexistent", "--event", "pull_request"])[1..]);
    assert!(
        out.stderr.contains("not the event \"pull_request\"") && out.stderr.contains("--force-event"),
        "{}",
        out.stderr
    );
    let on: pod_actions_workflow::Node = pod_actions_workflow::parse(
        "on:\n  push:\n    branches: [main]\n  schedule: []\njobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n",
    )
    .unwrap()
    .workflow
    .on;
    assert_eq!(context::events(&on), ["push", "schedule"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_tree_of_the_checkout_is_a_tarball_of_its_files() {
    let dir = std::env::temp_dir().join(format!("podrunner-tree-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("plain/sub")).unwrap();
    std::fs::write(dir.join("plain/sub/f.txt"), "x").unwrap();
    let t = podrunner_local::tree::tarball(&dir.join("plain"), &dir.join("scratch"), Default::default()).unwrap();
    let list = std::process::Command::new("tar").arg("-tzf").arg(&t).output().unwrap();
    assert!(String::from_utf8_lossy(&list.stdout).contains("sub/f.txt"));
    // A git tree gives its committed files.
    let repo = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let t =
        podrunner_local::tree::tarball(std::path::Path::new(repo), &dir.join("scratch2"), Default::default()).unwrap();
    let list = std::process::Command::new("tar").arg("-tzf").arg(&t).output().unwrap();
    assert!(String::from_utf8_lossy(&list.stdout).lines().any(|l| l == "Cargo.toml"));
    assert!(podrunner_local::tree::tarball(&dir.join("missing"), &dir.join("scratch3"), Default::default()).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The containers suite (T-093) gives a message for each job once CI
/// writes a pinned image into it; with no pin it is refused.
#[test]
fn the_containers_suite_gives_its_messages() {
    use podrunner_source::{Event, JobSource};
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/local/containers.yml")).unwrap();
    let image = format!("runner@sha256:{}", "b".repeat(64));
    let pinned = text.replace("IMAGE", &image);
    let mut s = LocalSource::new("l", &pinned, Base::default(), "x86_64", Box::new(|_, _| Default::default())).unwrap();
    let offers: Vec<JobId> = s
        .next(10, std::time::Duration::ZERO)
        .unwrap()
        .into_iter()
        .filter_map(|e| match e {
            Event::Offer { offer } => Some(offer.job_id),
            _ => None,
        })
        .collect();
    assert_eq!(offers.len(), 2);
    for job in &offers {
        let m = s.broker_job(job).unwrap();
        assert!(podrunner_local::exec::needs_containers(&m), "{job}");
    }
    let mut raw = LocalSource::new("l", &text, Base::default(), "x86_64", Box::new(|_, _| Default::default())).unwrap();
    raw.next(10, std::time::Duration::ZERO).unwrap();
    assert!(raw.broker_job(&JobId::new("in-container.0").unwrap()).unwrap_err().contains("not pinned"));
}

/// The egress workflow (T-095) gives one script job, with no container.
#[test]
fn the_egress_workflow_gives_one_job() {
    use podrunner_source::{Event, JobSource};
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/local/egress.yml")).unwrap();
    let mut s = LocalSource::new("l", &text, Base::default(), "x86_64", Box::new(|_, _| Default::default())).unwrap();
    let offers: Vec<JobId> = s
        .next(10, std::time::Duration::ZERO)
        .unwrap()
        .into_iter()
        .filter_map(|e| match e {
            Event::Offer { offer } => Some(offer.job_id),
            _ => None,
        })
        .collect();
    assert_eq!(offers.len(), 1);
    assert!(!podrunner_local::exec::needs_containers(&s.broker_job(&offers[0]).unwrap()));
}

#[test]
fn the_guest_kinds_of_run() {
    let unit = ["--images", "/s", "--image", "r@sha256:00"];
    let with = |extra: &[&str]| {
        let mut v = vec!["w.yml"];
        v.extend_from_slice(extra);
        v.extend_from_slice(&unit);
        parse(&args(&v))
    };
    let m = with(&["--microvm", "/p", "--engine", "/e", "--allow", "github.com"]).unwrap();
    assert_eq!(m.guest, Some((true, PathBuf::from("/p"))));
    assert_eq!((m.engine, m.allow.len()), (Some(PathBuf::from("/e")), 1), "a guest takes --allow with no podnet");
    let i = with(&["--incus", "/p"]).unwrap();
    assert_eq!(i.guest, Some((false, PathBuf::from("/p"))));
    assert!(with(&["--incus", "/p", "--engine", "/e"]).unwrap_err().contains("--engine needs --microvm"));
    assert!(with(&["--microvm", "/p", "--podnet", "/n"]).unwrap_err().contains("--podnet needs --podvm"));
    assert!(with(&["--microvm", "/p", "--podvm", "/q"]).unwrap_err().contains("one of --podvm"));
    assert!(parse(&args(&["w.yml", "--microvm", "/p"])).unwrap_err().contains("--images"));
}

#[test]
fn the_platforms_of_run() {
    let pin = format!("big@sha256:{}", "a".repeat(64));
    let p = parse(&args(&["w.yml", "--runner-dir", "/r", "--platform", "gpu=host"])).unwrap();
    assert_eq!(p.platforms, [("gpu".to_string(), "host".to_string())]);
    let p = parse(&args(&["w.yml", "--podvm", "/p", "--images", "/s", "--platform", &format!("big={pin}")])).unwrap();
    assert_eq!((p.platforms.len(), p.image), (1, None), "a platform image with no default --image");
    for (bad, why) in [
        (vec!["w.yml", "--platform", "gpu=host"], "--runner-dir"),
        (vec!["w.yml", "--runner-dir", "/r", "--platform", &format!("big={pin}")], "unit kind"),
        (vec!["w.yml", "--runner-dir", "/r", "--platform", "big=ubuntu:22.04"], "digest"),
        (vec!["w.yml", "--runner-dir", "/r", "--platform", "nolabel"], "LABEL=TARGET"),
    ] {
        let e = parse(&args(&bad)).unwrap_err();
        assert!(e.contains(why), "{bad:?}: {e}");
    }
}

/// A job of Windows fails by name before any runner starts (T-129); a job
/// of a label that a platform names would take that route.
#[test]
fn a_job_of_windows_fails_by_name() {
    let dir = std::env::temp_dir().join(format!("podrunner-platform-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("runner/bin")).unwrap();
    std::fs::write(dir.join("runner/run.sh"), "exit 9").unwrap();
    std::fs::write(dir.join("runner/bin/Runner.Listener"), "").unwrap();
    std::fs::write(dir.join("w.yml"), "on: push\njobs:\n  win:\n    runs-on: windows-latest\n    steps: [{run: x}]\n")
        .unwrap();
    let (w, r) = (dir.join("w.yml").display().to_string(), dir.join("runner").display().to_string());
    let out = run(&args(&[&w, "--runner-dir", &r, "--platform", "gpu=host", "--no-cache"]));
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.contains("failure win: runs-on \"windows-latest\": a unit is Linux"), "{}", out.stdout);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_inputs_of_workflow_dispatch_take_their_defaults_and_types() {
    let text = "on:\n  workflow_dispatch:\n    inputs:\n      debug:\n        type: boolean\n        default: false\n      level:\n        type: choice\n        options: [info, warn]\n        default: info\n      count:\n        type: number\n        default: 3\n      who:\n        required: true\njobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n";
    let on = pod_actions_workflow::parse(text).unwrap().workflow.on;
    let given = |p: &[(&str, &str)]| p.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<Vec<_>>();
    let (typed, event) = context::dispatch_inputs(&on, &given(&[("who", "me"), ("debug", "true")])).unwrap();
    let t: std::collections::BTreeMap<_, _> = typed.into_iter().collect();
    assert_eq!(t["debug"], Value::Bool(true));
    assert_eq!(t["count"], Value::Number(3.0));
    assert_eq!(t["level"], Value::String("info".into()));
    assert_eq!(event["debug"], serde_json::json!("true"), "github.event.inputs has strings");
    for (bad, why) in [
        (given(&[]), "required"),
        (given(&[("who", "me"), ("level", "debug")]), "one of"),
        (given(&[("who", "me"), ("count", "x")]), "number"),
        (given(&[("who", "me"), ("debug", "yes")]), "boolean"),
        (given(&[("who", "me"), ("other", "1")]), "no such input"),
    ] {
        let e = context::dispatch_inputs(&on, &bad).unwrap_err();
        assert!(e.contains(why), "{bad:?}: {e}");
    }
}

#[test]
fn the_files_of_vars_inputs_and_env_are_under_the_flags() {
    let dir = std::env::temp_dir().join(format!("podrunner-files-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("vars"), "A=file\nB=file\nMULTI<<END\none\ntwo\nEND\n").unwrap();
    let v = dir.join("vars").display().to_string();
    let a =
        parse(&args(&["w.yml", "--runner-dir", "/r", "--var-file", &v, "--var", "B=flag", "--env", "E=1"])).unwrap();
    let (vars, _, env) = files(&a).unwrap();
    let m: std::collections::BTreeMap<_, _> = vars.into_iter().collect();
    assert_eq!((m["A"].as_str(), m["B"].as_str(), m["MULTI"].as_str()), ("file", "flag", "one\ntwo"));
    assert_eq!(env, [("E".to_string(), "1".to_string())]);
    let _ = std::fs::remove_dir_all(&dir);
}
