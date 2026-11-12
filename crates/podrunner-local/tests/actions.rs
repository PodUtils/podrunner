//! Actions in the local mode (T-091): the official runner resolves each
//! `uses: OWNER/REPO@SHA` through the fake broker and takes its tarball
//! from the archive directory of the run. A composite action and a
//! JavaScript action (node of the runner) give outputs. Ignored by default:
//! it needs an unpacked release of `actions/runner` and `tar`.
//!
//!   PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner-local --test actions -- --ignored

use std::path::{Path, PathBuf};

use pod_actions_workflow::Base;
use podrunner_broker_fake::actions::archive_name;
use podrunner_local::LocalSource;
use podrunner_local::drive::{Line, drive};
use podrunner_local::exec::RunnerDir;

const HELLO: &str = "1111111111111111111111111111111111111111";
const JS: &str = "2222222222222222222222222222222222222222";

/// Write the archive of `name@sha` with `files` under one top directory.
fn archive(dir: &Path, name: &str, sha: &str, files: &[(&str, &str)]) {
    let work = dir.join("work");
    let top = work.join(format!("{}-{}", name.replace('/', "-"), &sha[..7]));
    std::fs::create_dir_all(&top).unwrap();
    for (f, text) in files {
        std::fs::write(top.join(f), text).unwrap();
    }
    let out = dir.join(archive_name(name, sha));
    std::fs::create_dir_all(out.parent().unwrap()).unwrap();
    let ok = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&out)
        .arg("-C")
        .arg(&work)
        .arg(top.file_name().unwrap())
        .status()
        .unwrap();
    assert!(ok.success());
    std::fs::remove_dir_all(&work).unwrap();
}

const COMPOSITE: &str = r#"name: hello
inputs:
  who:
    required: true
outputs:
  said:
    value: ${{ steps.s.outputs.said }}
runs:
  using: composite
  steps:
    - id: s
      shell: bash
      run: echo "said=hi-${{ inputs.who }}-$((5*5))" >> "$GITHUB_OUTPUT"
"#;

const NODE: &str = r#"name: js
inputs:
  n:
    default: "6"
runs:
  using: node24
  main: index.js
"#;

const INDEX: &str = r#"const fs = require("fs");
const n = Number(process.env["INPUT_N"]);
fs.appendFileSync(process.env.GITHUB_OUTPUT, `js=node-${n * 7}\n`);
"#;

#[test]
#[ignore]
fn the_runner_takes_actions_from_the_archive_directory() {
    let runner = PathBuf::from(std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR"));
    let dir = std::env::temp_dir().join(format!("podrunner-actions-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    archive(&dir, "o/hello", HELLO, &[("action.yml", COMPOSITE)]);
    archive(&dir, "o/js", JS, &[("action.yml", NODE), ("index.js", INDEX)]);
    let flow = format!(
        "on: push\njobs:\n  act:\n    runs-on: x\n    steps:\n      - id: h\n        uses: o/hello@{HELLO}\n        \
         with:\n          who: me\n      - id: j\n        uses: o/js@{JS}\n      \
         - run: test \"${{{{ steps.h.outputs.said }}}}\" = hi-me-25 && test \"${{{{ steps.j.outputs.js }}}}\" = node-42\n"
    );
    let mut s = LocalSource::new("local", &flow, Base::default(), "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    let mut exec = RunnerDir::new(runner).unwrap();
    exec.actions = Some(dir.clone());
    let mut lines = Vec::new();
    let ok = drive(&mut s, &mut exec, &mut |l| lines.push(l.clone())).unwrap();
    let logs: String = exec.logs.iter().map(|(_, l)| l.as_str()).collect();
    assert!(ok, "{lines:?}\n{logs}");
    assert!(matches!(&lines[..], [Line::Ran { .. }]), "{lines:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

const LOCAL: &str = r#"name: local
outputs:
  n:
    value: ${{ steps.n.outputs.n }}
runs:
  using: composite
  steps:
    - id: n
      shell: bash
      run: echo "n=local-$((8*8))" >> "$GITHUB_OUTPUT"
"#;

/// T-092: `actions/checkout` takes the tarball of the local tree from the
/// broker, also into a `path`, and a `./` action of the tree runs.
#[test]
#[ignore]
fn the_checkout_stand_in_gives_the_local_tree_and_its_actions() {
    let runner = PathBuf::from(std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR"));
    let dir = std::env::temp_dir().join(format!("podrunner-checkout-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let tree = dir.join("tree");
    std::fs::create_dir_all(tree.join("act")).unwrap();
    std::fs::write(tree.join("hello.txt"), "local-tree\n").unwrap();
    std::fs::write(tree.join("act/action.yml"), LOCAL).unwrap();
    let tarball = dir.join("tree.tar.gz");
    let ok =
        std::process::Command::new("tar").arg("-czf").arg(&tarball).arg("-C").arg(&tree).arg(".").status().unwrap();
    assert!(ok.success());
    let flow = "on: push\njobs:\n  co:\n    runs-on: x\n    steps:\n      - uses: actions/checkout@v4\n      \
                - run: test \"$(cat hello.txt)\" = local-tree\n      - id: l\n        uses: ./act\n      \
                - run: test \"${{ steps.l.outputs.n }}\" = local-64\n      - uses: actions/checkout@v4\n        \
                with:\n          path: sub\n      - run: test -f sub/hello.txt && test -f sub/act/action.yml\n";
    let mut s = LocalSource::new("local", flow, Base::default(), "x86_64", Box::new(|_, _| unreachable!())).unwrap();
    let mut exec = RunnerDir::new(runner).unwrap();
    exec.checkout = Some(tarball);
    let mut lines = Vec::new();
    let ok = drive(&mut s, &mut exec, &mut |l| lines.push(l.clone())).unwrap();
    let logs: String = exec.logs.iter().map(|(_, l)| l.as_str()).collect();
    assert!(ok, "{lines:?}\n{logs}");
    let _ = std::fs::remove_dir_all(&dir);
}
