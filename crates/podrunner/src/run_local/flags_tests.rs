//! The checks of the flags of `run` that a user agent found open
//! (GitHub #155 to #162).

use super::*;

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

/// T-206: `--platform` does not turn off the other checks.
#[test]
fn a_platform_keeps_each_check_of_the_flags() {
    let host = ["--platform", "ubuntu-latest=host"];
    for (bad, why) in [
        (vec!["w.yml", "--jobs", "2", "--runner-dir", "/r"], "--jobs N needs a unit kind"),
        (vec!["w.yml", "--toolcache", "/t", "--runner-dir", "/r"], "--toolcache needs --podvm"),
        (vec!["w.yml", "--allow", "example.com", "--runner-dir", "/r"], "--allow needs --podnet"),
        (vec!["w.yml", "--environment-allow", "e=example.com", "--runner-dir", "/r"], "--environment-allow needs"),
        (vec!["w.yml", "--engine", "/e", "--runner-dir", "/r"], "--engine needs --microvm"),
    ] {
        let mut with = bad.clone();
        with.extend(host);
        for a in [bad, with] {
            let e = parse(&args(&a)).unwrap_err();
            assert!(e.contains(why), "{a:?}: {e}");
        }
    }
}

/// T-207: the refusal names the flag that clashes or is missing.
#[test]
fn the_refusal_of_an_executor_names_its_flag() {
    let pin = format!("r@sha256:{}", "a".repeat(64));
    for (bad, why) in [
        (vec!["w.yml", "--runner-dir", "/r", "--podvm", "/p", "--images", "/s", "--image", &pin], "two executors"),
        (vec!["w.yml", "--runner-dir", "/r", "--image", &pin], "--images and --image need a unit kind"),
        (vec!["w.yml", "--runner-dir", "/r", "--images", "/s"], "--images and --image need a unit kind"),
        (vec!["w.yml", "--incus", "/p", "--image", &pin], "--incus needs --images STORE"),
        (vec!["w.yml", "--podvm", "/p", "--images", "/s"], "--podvm needs --image"),
        (vec!["w.yml"], "is necessary"),
    ] {
        let e = parse(&args(&bad)).unwrap_err();
        assert!(e.contains(why), "{bad:?}: {e}");
    }
    // A short option that run does not have is not a file (T-208).
    assert!(parse(&args(&["w.yml", "-x", "--runner-dir", "/r"])).unwrap_err().contains("an unknown option \"-x\""));
}

fn workflow(tag: &str, text: &str) -> (PathBuf, String) {
    let dir = std::env::temp_dir().join(format!("podrunner-flags-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("w.yml");
    std::fs::write(&file, text).unwrap();
    let w = file.display().to_string();
    (dir, w)
}

/// T-209: `--validate` refuses the needs that a plan refuses.
#[test]
fn validate_refuses_a_dangling_need_and_a_cycle() {
    for (tag, needs, why) in [("dangling", "nope", "not a job"), ("cycle", "b", "cycle")] {
        let text = format!(
            "on: push\njobs:\n  a:\n    needs: {needs}\n    runs-on: x\n    steps: [{{run: a}}]\n  b:\n    needs: a\n    runs-on: x\n    steps: [{{run: b}}]\n"
        );
        let (dir, w) = workflow(tag, &text);
        let out = run(&args(&[&w, "--validate"]));
        assert_eq!(out.code, 78, "{tag}: {}{}", out.stdout, out.stderr);
        assert!(out.stderr.contains(why), "{tag}: {}", out.stderr);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// T-210: `--break` names a job and a step of the workflow.
#[test]
fn a_break_names_a_job_and_a_step_of_the_file() {
    let (dir, w) =
        workflow("break", "on: push\njobs:\n  build:\n    runs-on: x\n    steps: [{run: a}, {id: two, run: b}]\n");
    for (b, ok) in [
        ("nosuchjob:1", false),
        ("build:99", false),
        ("build:nosuchstep", false),
        ("build:2", true),
        ("build:two", true),
    ] {
        let out = run(&args(&[&w, "--runner-dir", "/nonexistent", "--dry-run", "--no-cache", "--break", b]));
        assert_eq!(out.code == 64, !ok, "{b}: {}{}", out.stdout, out.stderr);
        if !ok {
            assert!(out.stderr.contains(&format!("--break {b}")), "{b}: {}", out.stderr);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-212: a missing podvm binary needs a path, not a version.
#[test]
fn doctor_names_a_missing_podvm_as_a_path() {
    let (dir, w) = workflow("doctor", "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n");
    for flag in ["--podvm", "--incus"] {
        let out = doctor(&args(&[
            &w,
            flag,
            "/nonexistent/podvm",
            "--images",
            "/s",
            "--image",
            "r@sha256:0000000000000000000000000000000000000000000000000000000000000000",
        ]));
        let line = out.stdout.lines().find(|l| l.contains(" podvm: ")).unwrap_or_default().to_string();
        assert!(line.contains(&format!("{flag} /nonexistent/podvm: not a file")), "{flag}: {}", out.stdout);
        assert!(!line.contains("take podvm"), "{flag}: {line}");
    }
    // T-213: doctor tries the bind of the loopback.
    let out = doctor(&args(&[&w, "--runner-dir", "/nonexistent"]));
    assert!(out.stdout.contains(" loopback: "), "{}", out.stdout);
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-211: when `--local-repository` serves each reference, no forge is
/// asked and no relay opens, also with no `--offline`. A forge with no
/// host cannot open a relay: the run must not try.
#[test]
fn a_local_repository_opens_no_relay() {
    let (dir, w) = workflow("relay", "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{uses: me/tool@main}]\n");
    std::fs::create_dir_all(dir.join("tool")).unwrap();
    std::fs::write(dir.join("tool/action.yml"), "runs:\n  using: composite\n  steps: [{run: hi, shell: sh}]\n")
        .unwrap();
    let local = format!("me/tool@main={}", dir.join("tool").display());
    let store = dir.join("store").display().to_string();
    let base = [w.as_str(), "--dry-run", "--no-cache", "--actions", &store, "--action-server", "http://"];
    let out = run(&args(&[&base[..], &["--local-repository", &local]].concat()));
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.contains("planned a\n"), "{}", out.stdout);
    // The control: with no local repository the forge is asked.
    assert_ne!(run(&args(&base)).code, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-218: the views exclude each other, a name of `--input` or `--var` comes
/// once, and a dry run on a runner directory fails a job with containers.
#[test]
fn the_nits_of_the_user_run() {
    for (bad, why) in [
        (vec!["w.yml", "--list", "--graph"], "exclude each other"),
        (vec!["w.yml", "--runner-dir", "/r", "--input", "a=1", "--input", "a=2"], "--input a: given two times"),
        (vec!["w.yml", "--runner-dir", "/r", "--var", "A=1", "--var", "A=2"], "--var A: given two times"),
    ] {
        let e = parse(&args(&bad)).unwrap_err();
        assert!(e.contains(why), "{bad:?}: {e}");
    }
    let pin = format!("redis@sha256:{}", "a".repeat(64));
    let (dir, w) = workflow(
        "svc",
        &format!(
            "on: push\njobs:\n  db:\n    runs-on: x\n    services:\n      r:\n        image: {pin}\n    steps: [{{run: a}}]\n"
        ),
    );
    let runner = super::qa6_tests::runner_dir("nits");
    let r = runner.text().to_string();
    let out = run(&args(&[&w, "--runner-dir", &r, "--dry-run", "--no-cache"]));
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.contains("failure db:") && out.stdout.contains("needs containers"), "{}", out.stdout);
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-219: an event that GitHub does not have, and a missing payload file,
/// each fail with their own text, before the filter of `on:`.
#[test]
fn an_unknown_event_and_a_missing_payload_fail_first() {
    let e = parse(&args(&["w.yml", "--runner-dir", "/r", "--event", "notanevent"])).unwrap_err();
    assert!(e.contains("--event notanevent: GitHub has no such event"), "{e}");
    let e = parse(&args(&["w.yml", "--runner-dir", "/r", "--event-path", "/nonexistent/e.json"])).unwrap_err();
    assert!(e.contains("--event-path /nonexistent/e.json: not a file"), "{e}");
    assert!(parse(&args(&["w.yml", "--runner-dir", "/r", "--event", "workflow_dispatch"])).is_ok());
}

/// T-323 F11: a missing file or directory of a flag names its flag.
#[test]
fn a_missing_file_of_a_flag_names_the_flag() {
    for (flag, value, text) in [
        ("--var-file", "/nonexistent/v", "--var-file /nonexistent/v: not a file"),
        ("--secret-file", "/", "--secret-file /: not a file"),
        ("--environment-secret-file", "e=/nonexistent/s", "--environment-secret-file /nonexistent/s: not a file"),
        ("--local-repository", "a/b@v1=/nonexistent/d", "--local-repository /nonexistent/d: not a directory"),
    ] {
        let e = parse(&args(&["w.yml", "--runner-dir", "/r", flag, value])).unwrap_err();
        assert!(e.contains(text), "{e}");
    }
}

/// T-223, T-226: `doctor --run` checks that the store holds `--image`, and
/// names the egress hold of the run.
#[test]
fn doctor_checks_the_image_of_the_store_and_names_the_hold() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, w) = workflow("image", "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n");
    let podvm = dir.join("podvm");
    std::fs::write(
        &podvm,
        "#!/bin/sh\ncase \"$1\" in probe) echo '{\"podvm\":\"0.2.0-beta.22\",\"rung\":\"userland\"}';; inspect) [ \"$2\" = have@sha256:0000000000000000000000000000000000000000000000000000000000000000 ];; esac\n",
    )
    .unwrap();
    std::fs::set_permissions(&podvm, std::fs::Permissions::from_mode(0o755)).unwrap();
    let store = dir.join("store");
    std::fs::create_dir_all(&store).unwrap();
    let (p, s) = (podvm.display().to_string(), store.display().to_string());
    let out = doctor(&args(&[
        &w,
        "--podvm",
        &p,
        "--images",
        &s,
        "--image",
        "none@sha256:0000000000000000000000000000000000000000000000000000000000000000",
    ]));
    assert!(
        out.stdout.contains("FAIL image:")
            && out
                .stdout
                .contains("does not hold none@sha256:0000000000000000000000000000000000000000000000000000000000000000"),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("ok   hold: none: the unit has the network of the host"), "{}", out.stdout);
    let out = doctor(&args(&[
        &w,
        "--podvm",
        &p,
        "--images",
        &s,
        "--image",
        "have@sha256:0000000000000000000000000000000000000000000000000000000000000000",
    ]));
    assert!(
        out.stdout
            .contains("ok   image: have@sha256:0000000000000000000000000000000000000000000000000000000000000000 is in"),
        "{}",
        out.stdout
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-230: with no `--platform`, a job of Windows, of macOS or of another
/// architecture fails by name, not `ok` on the host.
#[test]
fn a_foreign_label_fails_with_no_platform() {
    let other = if std::env::consts::ARCH == "aarch64" { "X64" } else { "ARM64" };
    for (tag, label) in [("win", "windows-latest"), ("mac", "macos-14"), ("arch", other)] {
        let (dir, w) = workflow(
            tag,
            &format!("on: push\njobs:\n  j:\n    runs-on: [self-hosted, {label}]\n    steps: [{{run: x}}]\n"),
        );
        std::fs::create_dir_all(dir.join("runner/bin")).unwrap();
        std::fs::write(dir.join("runner/run.sh"), "exit 9").unwrap();
        std::fs::write(dir.join("runner/bin/Runner.Listener"), "").unwrap();
        let r = dir.join("runner").display().to_string();
        let out = run(&args(&[&w, "--runner-dir", &r, "--no-cache"]));
        assert_eq!(out.code, 1, "{label}: {}{}", out.stdout, out.stderr);
        assert!(out.stdout.contains(&format!("failure j: runs-on \"{label}\"")), "{label}: {}", out.stdout);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// T-277: `--repository` and `--sha` have the forms of GitHub.
#[test]
fn the_values_of_the_github_context_are_checked() {
    let base = ["w.yml", "--runner-dir", "/r"];
    let with = |more: &[&str]| parse(&args(&[&base[..], more].concat()));
    assert!(with(&["--repository", "bad"]).unwrap_err().contains("not OWNER/REPO"));
    assert!(with(&["--repository", "o/r/x"]).is_err());
    assert!(with(&["--repository", "o-1/r.x"]).is_ok());
    assert!(with(&["--sha", "zz"]).unwrap_err().contains("not a commit"));
    assert!(with(&["--sha", &"a".repeat(40)]).is_ok());
}

/// The dry run on `--podvm` refuses a job with services, as its run does
/// (T-272).
#[test]
fn a_dry_run_on_podvm_refuses_services() {
    use podrunner_local::drive::Executor;
    let mut dry = inspect::DryRun {
        host: false,
        podvm: true,
        no_hooks: false,
        actions: None,
        pins: Default::default(),
        tree: None,
        platforms: vec![],
    };
    let mut job = podrunner_local::Job::scripts("a", &["true"]);
    let id = podrunner_schema::JobId::new("a.0").unwrap();
    assert!(dry.run(&id, job.clone(), std::time::Duration::from_secs(1)).is_ok());
    job.overrides.insert("jobServiceContainers".into(), serde_json::json!({ "type": 2, "map": [] }));
    let e = dry.run(&id, job, std::time::Duration::from_secs(1)).unwrap_err();
    assert!(e.contains("services") && e.contains("--microvm"), "{e}");
}

/// T-279: `--validate` checks each cron, `--list` refuses a cycle, and the
/// payload of a run of the schedule has its cron.
#[test]
fn validate_checks_the_crons_and_list_refuses_a_cycle() {
    let (dir, w) = workflow(
        "cron",
        "on:\n  schedule:\n    - cron: '99 * * * *'\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n",
    );
    let out = run(&args(&[&w, "--validate"]));
    assert_eq!(out.code, 78, "{}{}", out.stdout, out.stderr);
    let _ = std::fs::remove_dir_all(&dir);
    let cycle = "on: push\njobs:\n  a:\n    needs: b\n    runs-on: x\n    steps: [{run: a}]\n  b:\n    needs: a\n    runs-on: x\n    steps: [{run: b}]\n";
    let (dir, w) = workflow("listcycle", cycle);
    assert_eq!(run(&args(&[&w, "--list"])).code, 78);
    let _ = std::fs::remove_dir_all(&dir);
    context::set_schedule("30 5 * * 1");
    let f = context::Facts {
        repository: "o/r".into(),
        git_ref: "refs/heads/main".into(),
        sha: "0".repeat(40),
        actor: "a".into(),
        default_branch: "main".into(),
        inputs: Default::default(),
    };
    assert_eq!(context::payload("schedule", &f)["schedule"], "30 5 * * 1");
}

/// The dry run on podvm with no container hooks refuses a job with
/// containers, as its run does (T-311).
#[test]
fn a_dry_run_on_podvm_with_no_hooks_refuses_a_container_job() {
    use podrunner_local::drive::Executor;
    let mut dry = inspect::DryRun {
        host: false,
        podvm: true,
        no_hooks: true,
        actions: None,
        pins: Default::default(),
        tree: None,
        platforms: vec![],
    };
    let mut job = podrunner_local::Job::scripts("a", &["true"]);
    let id = podrunner_schema::JobId::new("a.0").unwrap();
    assert!(dry.run(&id, job.clone(), std::time::Duration::from_secs(1)).is_ok());
    job.overrides.insert("jobContainer".into(), serde_json::json!({ "type": 2, "map": [] }));
    let e = dry.run(&id, job, std::time::Duration::from_secs(1)).unwrap_err();
    assert!(e.contains("--microvm and --engine"), "{e}");
}

/// T-323 F21, T-316 B-15: `--image` needs a full digest.
#[test]
fn the_image_needs_a_full_digest() {
    for image in ["runner:latest", "runner@sha256:00"] {
        let e = parse(&args(&["w.yml", "--podvm", "/p", "--images", "/s", "--image", image])).unwrap_err();
        assert!(e.contains("--image: the image") && e.contains("not pinned by digest"), "{e}");
    }
}

/// T-323 F16: with `--json`, each view is JSON.
#[test]
fn the_views_print_json() {
    let text = "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n  b:\n    needs: a\n    runs-on: x\n    foo: 1\n    steps: [{run: b}]\n";
    let (dir, w) = workflow("views-json", text);
    let first = |flag: &str| {
        let out = run(&args(&[&w, flag, "--json"]));
        serde_json::from_str::<serde_json::Value>(out.stdout.lines().next().unwrap_or_default()).unwrap()
    };
    let v = first("--validate");
    assert_eq!((v["valid"].as_bool(), v["warnings"].as_array().map(Vec::len)), (Some(true), Some(1)), "{v}");
    assert_eq!(first("--list")["job"], "a");
    assert_eq!(first("--graph")["edges"], serde_json::json!([["a", "b"]]));
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-323 F13: a file that is not a workflow is named so.
#[test]
fn a_file_that_is_not_a_workflow_is_named() {
    let (dir, w) = workflow("not-workflow", "just text\n");
    let out = run(&args(&[&w, "--runner-dir", "/nonexistent", "--dry-run"]));
    assert_eq!(out.code, 78, "{}", out.stderr);
    assert!(out.stderr.contains(": not a workflow: ") && !out.stderr.contains("the actions"), "{}", out.stderr);
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-333: a dry run writes no state, and its JSON says `planned`.
#[test]
fn a_dry_run_writes_no_state() {
    let (dir, w) = workflow("dry-state", "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n");
    let st = dir.join("st").display().to_string();
    let runner = super::qa6_tests::runner_dir("dry-state");
    let r = runner.text().to_string();
    let out = run(&args(&[&w, "--runner-dir", &r, "--dry-run", "--json", "--no-cache", "--state", &st]));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stdout.contains(r#""result":"planned""#), "{}", out.stdout);
    assert!(!dir.join("st").exists(), "a dry run made the state or its artifacts (T-345)");
    // T-345: each path of the run is checked, by its flag.
    let empty = dir.join("empty").display().to_string();
    std::fs::create_dir_all(&empty).unwrap();
    for (flags, why) in [
        (vec!["--runner-dir", empty.as_str()], "--runner-dir"),
        (vec!["--runner-dir", r.as_str(), "--log-dir", "/proc/podrunner-nope"], "--log-dir"),
    ] {
        let mut x = vec![w.as_str(), "--dry-run", "--no-cache"];
        x.extend(flags);
        let out = run(&args(&x));
        assert_eq!(out.code, 78, "{why}: {}{}", out.stdout, out.stderr);
        assert!(out.stderr.contains(why), "{}", out.stderr);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-339: `--validate` reads each number of minutes as a run does.
#[test]
fn validate_reads_the_minutes() {
    let bad = "on: push\njobs:\n  a:\n    runs-on: x\n    timeout-minutes: abc\n    steps: [{run: a}]\n";
    let (dir, w) = workflow("minutes-bad", bad);
    let out = run(&args(&[&w, "--validate"]));
    assert_eq!(out.code, 78, "{}", out.stdout);
    assert!(out.stderr.contains("jobs.a.timeout-minutes") && out.stderr.contains("not a number"), "{}", out.stderr);
    let _ = std::fs::remove_dir_all(&dir);
    let good = "on: push\njobs:\n  a:\n    runs-on: x\n    timeout-minutes: ${{ inputs.t }}\n    steps: [{run: a, timeout-minutes: 5}]\n";
    let (dir, w) = workflow("minutes-good", good);
    assert_eq!(run(&args(&[&w, "--validate"])).code, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-631: a value of `--changed-since` or `--remote` that starts with `-`
/// is an option of git, not a ref: the parse refuses it, and a dry run
/// writes no file.
#[test]
fn a_git_value_that_starts_with_a_dash_is_refused() {
    for flag in ["--changed-since", "--remote"] {
        for value in ["--output=/nonexistent/f", "-p", "-"] {
            let e = parse(&args(&["w.yml", "--runner-dir", "/r", flag, value])).unwrap_err();
            assert!(e.contains(&format!("{flag} {value}: starts with '-'")), "{e}");
        }
        assert!(parse(&args(&["w.yml", "--runner-dir", "/r", flag, "main"])).is_ok());
    }
    let dir = crate::scratch::Scratch::new("t631");
    let w = dir.join("w.yml");
    std::fs::write(&w, "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n").unwrap();
    let file = dir.join("INJECT");
    let (w, out) = (w.display().to_string(), format!("--output={}", file.display()));
    let r = run(&args(&[&w, "--runner-dir", "/r", "--changed-since", &out, "--dry-run"]));
    assert_eq!(r.code, 64, "{}{}", r.stdout, r.stderr);
    assert!(!file.exists(), "git wrote the file of --output");
}
