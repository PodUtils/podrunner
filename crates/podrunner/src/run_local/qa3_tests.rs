//! The checks of the P3 findings of the third QA, agent A (T-308).

use super::qa6_tests::runner_dir;
use super::*;
use crate::scratch::Scratch;

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

/// An image source that gives one digest for each tag, with no pull.
struct Fake;

impl podrunner_local::images::ImageSource for Fake {
    fn pull(&self, _image: &str) -> Result<String, String> {
        Ok(format!("sha256:{}", "a".repeat(64)))
    }
}

/// A16: a dry run pins the images, and writes no lock file into the tree;
/// a run writes it. A28: one reference is "1 reference".
#[test]
fn a_dry_run_writes_no_lock() {
    let dir = Scratch::new("qa3-lock");
    let text = "on: push\njobs:\n  a:\n    runs-on: x\n    container: alpine:3.20\n    steps: [{run: a}]\n";
    std::fs::write(dir.join("w.yml"), text).unwrap();
    let lock = dir.join(podrunner_local::lock::LOCK_FILE);
    let pin = |dry_run: bool| {
        let a = Args { file: dir.join("w.yml"), checkout: Some(dir.to_path_buf()), dry_run, ..Args::default() };
        let mode = podrunner_local::store::Mode::Online;
        actions::pin(&a, text, mode, "o/r", &"b".repeat(40), Some(&Fake), false).unwrap().expect("pins")
    };
    let p = pin(true);
    assert!(!lock.exists(), "a dry run wrote {}", lock.display());
    assert!(p.note.contains("--dry-run:") && p.note.contains("would pin 1 reference;"), "{}", p.note);
    assert_eq!(
        p.commits.get("docker://alpine:3.20").map(String::as_str),
        Some(&*format!("alpine@sha256:{}", "a".repeat(64)))
    );
    let p = pin(false);
    assert!(lock.is_file(), "the run writes the lock");
    assert!(p.note.ends_with("pins 1 reference\n"), "{}", p.note);
}

/// A15: an expression that a dry run cannot evaluate (no outputs of a
/// needed job) names why; a run with no dry run does not.
#[test]
fn a_dry_run_names_its_empty_outputs() {
    let dir = Scratch::new("qa3-dry-outputs");
    let r = runner_dir("qa3-dry-outputs");
    let flow = "on: push\njobs:\n  a:\n    runs-on: x\n    outputs: {list: '${{ steps.s.outputs.list }}'}\n    \
                steps: [{id: s, run: x}]\n  b:\n    needs: a\n    runs-on: x\n    strategy:\n      matrix:\n        \
                v: ${{ fromJSON(needs.a.outputs.list) }}\n    steps: [{run: x}]\n";
    std::fs::write(dir.join("w.yml"), flow).unwrap();
    let w = dir.join("w.yml").display().to_string();
    let rd = r.display().to_string();
    let out =
        run(&args(&[&w, "--runner-dir", &rd, "--no-cache", "--dry-run", "--checkout", &dir.display().to_string()]));
    assert!(out.stderr.contains("fromJSON"), "{}", out.stderr);
    assert!(out.stderr.contains("can fail in a dry run only"), "{}", out.stderr);
}

/// A25: the line of a cell that `fail-fast` cancelled names the cell that
/// failed, as text and as JSON.
#[test]
fn a_cancelled_cell_names_its_cause() {
    let id = podrunner_schema::JobId::new("m.1").unwrap();
    let cut = Line::Cancelled {
        job: id.clone(),
        name: "m (2)".into(),
        reason: "fail-fast: the cell \"m (1)\" failed".into(),
    };
    assert_eq!(line(&cut), "cancelled m (2): fail-fast: the cell \"m (1)\" failed\n");
    let v: serde_json::Value = serde_json::from_str(&logs::line_json(&cut, false)).unwrap();
    assert_eq!(
        (v["result"].as_str(), v["reason"].as_str()),
        (Some("cancelled"), Some("fail-fast: the cell \"m (1)\" failed"))
    );
}
