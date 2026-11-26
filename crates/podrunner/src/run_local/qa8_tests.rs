//! The checks of QA round 6, agent A: the flags of `podrunner run` (T-634).

use super::qa6_tests::runner_dir;
use super::*;
use crate::scratch::Scratch;

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

const MATRIX: &str = "on: push\njobs:\n  m:\n    runs-on: x\n    strategy:\n      matrix:\n        os: [a, b]\n        \
                      include: [{os: a, extra: e}]\n    steps: [{run: x}]\n  plain:\n    runs-on: x\n    steps: [{run: x}]\n";

/// An empty value of a flag is refused by its name; an empty NAME of
/// `--github-token-env` still sends no token (T-385).
#[test]
fn an_empty_value_is_refused_by_its_flag() {
    for flag in ["--ref", "--actor", "--checkout", "--artifacts", "--sha", "--state", "--runner-dir", "--secret-file"] {
        let e = parse(&args(&["w.yml", "--runner-dir", "/r", flag, ""])).unwrap_err();
        assert_eq!(e, format!("{flag}: the value is empty"), "{flag}");
        let out = run(&args(&["w.yml", "--runner-dir", "/r", flag, ""]));
        assert_eq!(out.code, 64, "{flag}: {}", out.stderr);
        assert!(!out.stderr.contains("usage:"), "a wrong value, not a wrong form: {}", out.stderr);
    }
    let a = parse(&args(&["w.yml", "--runner-dir", "/r", "--github-token-env", ""])).unwrap();
    assert_eq!(a.token_env.as_deref(), Some(""));
    assert!(parse(&args(&["w.yml", "--runner-dir", "/r", "--ref"])).unwrap_err().ends_with("needs a value"));
}

/// A `--job` or a `--matrix` axis that the workflow does not have is 64,
/// also for `--list`, `--graph` and `--validate`; an axis of `include`
/// counts, and a matrix of an expression can have each axis.
#[test]
fn a_job_or_an_axis_that_the_workflow_has_not_is_refused() {
    let dir = Scratch::new("qa8-filter");
    std::fs::write(dir.join("w.yml"), MATRIX).unwrap();
    let expr = "on: push\njobs:\n  m:\n    runs-on: x\n    strategy:\n      matrix: ${{ fromJSON('{}') }}\n    \
                steps: [{run: x}]\n";
    std::fs::write(dir.join("expr.yml"), expr).unwrap();
    let w = dir.join("w.yml").display().to_string();
    let e = dir.join("expr.yml").display().to_string();
    let runner = runner_dir("qa8-filter");
    let r = runner.text();
    let dry = ["--runner-dir", r, "--dry-run", "--no-cache"];
    for (filter, view, want) in [
        (["--matrix", "nosuch=a"], &["--list"][..], "--matrix nosuch=a: no matrix of the workflow has the axis nosuch"),
        (["--matrix", "nosuch=a"], &["--validate"], "--matrix nosuch=a"),
        (["--matrix", "nosuch=a"], &dry, "--matrix nosuch=a"),
        (["--job", "nosuch"], &["--list"], "--job nosuch: the workflow has no such job"),
        (["--job", "nosuch"], &["--graph"], "--job nosuch"),
    ] {
        let out = run(&args(&[&[w.as_str()][..], &filter, view].concat()));
        assert_eq!(out.code, 64, "{filter:?} {view:?}: {}{}", out.stdout, out.stderr);
        assert!(out.stderr.contains(want), "{filter:?} {view:?}: {}", out.stderr);
    }
    for (file, filter, views) in [
        (&w, ["--matrix", "os=a"], &[&["--list"][..], &dry][..]),
        (&w, ["--matrix", "extra=e"], &[&["--list"][..], &dry]),
        (&w, ["--job", "plain"], &[&["--list"][..], &dry]),
        (&e, ["--matrix", "any=1"], &[&["--list"][..]]),
    ] {
        for view in views {
            let out = run(&args(&[&[file.as_str()][..], &filter, view].concat()));
            assert_eq!(out.code, 0, "{file} {filter:?} {view:?}: {}{}", out.stdout, out.stderr);
        }
    }
}

/// A dry run makes the checks of the run: a `--state` that the run cannot
/// write, a `--podvm` that is not podvm, an `--incus` with no incusd.
#[test]
fn a_dry_run_checks_the_state_and_the_unit_kind() {
    let dir = Scratch::new("qa8-dry");
    std::fs::write(dir.join("w.yml"), "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n").unwrap();
    std::fs::write(dir.join("file"), "").unwrap();
    std::fs::create_dir_all(dir.join("store")).unwrap();
    let (w, file, store) = (dir.join("w.yml"), dir.join("file"), dir.join("store"));
    let (w, file, store) = (w.to_str().unwrap(), file.to_str().unwrap(), store.to_str().unwrap());
    let runner = runner_dir("qa8-dry");
    let out = run(&args(&[w, "--runner-dir", runner.text(), "--dry-run", "--no-cache", "--state", file]));
    assert_eq!(out.code, 78, "{}{}", out.stdout, out.stderr);
    assert!(out.stderr.contains("--state ") && out.stdout.is_empty(), "{}{}", out.stdout, out.stderr);
    let image = format!("runner@sha256:{}", "0".repeat(64));
    let unit = |flag: &str| {
        run(&args(&[w, flag, "/bin/true", "--images", store, "--image", &image, "--dry-run", "--no-cache"]))
    };
    let out = unit("--podvm");
    assert_eq!(out.code, 78, "{}{}", out.stdout, out.stderr);
    assert!(out.stderr.contains("podvm: ") && !out.stdout.contains("planned"), "{}{}", out.stdout, out.stderr);
    if crate::doctor::incusd().is_ok() {
        eprintln!("skip: this host has an incusd");
        return;
    }
    let out = unit("--incus");
    assert_eq!(out.code, 69, "{}{}", out.stdout, out.stderr);
    assert!(out.stderr.contains("--incus: ") && !out.stdout.contains("planned"), "{}{}", out.stdout, out.stderr);
}

/// `timeout-minutes` above the 5 days at which the run stops a job gets a
/// warning of `--validate`, in the text and in the JSON.
#[test]
fn a_timeout_above_five_days_gets_a_warning() {
    let dir = Scratch::new("qa8-timeout");
    let flow = |minutes: &str| {
        format!("on: push\njobs:\n  a:\n    runs-on: x\n    timeout-minutes: {minutes}\n    steps: [{{run: a}}]\n")
    };
    let w = dir.join("w.yml");
    let ws = w.to_str().unwrap();
    std::fs::write(&w, flow("99999999")).unwrap();
    let out = run(&args(&[ws, "--validate"]));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stdout.contains("warning: jobs.a.timeout-minutes (line 5): 99999999 is more than 7200"),
        "{}",
        out.stdout
    );
    let out = run(&args(&[ws, "--validate", "--json"]));
    let v: serde_json::Value = serde_json::from_str(out.stdout.trim()).unwrap();
    assert!(v["warnings"][0].as_str().unwrap().contains("the run stops the job at 5 days"), "{v}");
    for ok in ["7200", "'${{ inputs.t }}'"] {
        std::fs::write(&w, flow(ok)).unwrap();
        let out = run(&args(&[ws, "--validate"]));
        assert_eq!(out.code, 0, "{ok}: {}", out.stderr);
        assert!(!out.stdout.contains("timeout-minutes"), "{ok}: {}", out.stdout);
    }
}

/// A file of an option that exists and cannot be read is 66, as a workflow
/// that cannot be read; a missing one stays 64. Root reads each file: the
/// test then checks the missing file only.
#[test]
fn an_option_file_that_cannot_be_read_is_66() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Scratch::new("qa8-unreadable");
    std::fs::write(dir.join("w.yml"), "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n").unwrap();
    std::fs::write(dir.join("vars"), "A=1\n").unwrap();
    let (w, vars) = (dir.join("w.yml"), dir.join("vars"));
    let (w, vars) = (w.to_str().unwrap(), vars.to_str().unwrap());
    let runner = runner_dir("qa8-unreadable");
    let r = runner.text();
    let out = run(&args(&[w, "--runner-dir", r, "--dry-run", "--no-cache", "--var-file", "/nonexistent/vars"]));
    assert_eq!(out.code, 64, "{}", out.stderr);
    std::fs::set_permissions(dir.join("vars"), std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::File::open(dir.join("vars")).is_ok() {
        eprintln!("skip: this user reads a file of mode 0000 (root)");
        return;
    }
    let out = run(&args(&[w, "--runner-dir", r, "--dry-run", "--no-cache", "--var-file", vars]));
    assert_eq!(out.code, 66, "{}", out.stderr);
    assert!(out.stderr.contains(&format!("--var-file {vars}: ")), "{}", out.stderr);
}

/// A link out of the project is 64 for a file and for a directory, and
/// the text names the link, the project and the target.
#[test]
fn a_link_out_of_the_project_is_64_and_names_both_ends() {
    let dir = Scratch::new("qa8-link");
    let flows = dir.join("p/.github/workflows");
    std::fs::create_dir_all(&flows).unwrap();
    std::fs::create_dir_all(dir.join("other")).unwrap();
    std::fs::write(dir.join("other/w.yml"), MATRIX).unwrap();
    std::os::unix::fs::symlink(dir.join("other/w.yml"), flows.join("out.yml")).unwrap();
    let project = std::fs::canonicalize(dir.join("p")).unwrap();
    let target = std::fs::canonicalize(dir.join("other/w.yml")).unwrap();
    for arg in [flows.join("out.yml"), flows.clone()] {
        let out = run(&args(&[arg.to_str().unwrap(), "--list"]));
        assert_eq!(out.code, 64, "{}: {}", arg.display(), out.stderr);
        let want = format!(
            "{}: the symbolic link leads out of the project {}, to {}",
            flows.join("out.yml").display(),
            project.display(),
            target.display()
        );
        assert!(out.stderr.contains(&want), "{}: {}", arg.display(), out.stderr);
    }
}
