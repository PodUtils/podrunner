//! The stand-in of `actions/checkout` (T-697, T-708) with the node of the
//! runner and `tar` only: no curl, no python, no shell in its PATH, as in a
//! job container where the runner mounts its node. It takes the tree from
//! the fake broker on the loopback. Ignored by default: it needs an
//! unpacked release of `actions/runner` for its node.
//!
//!   PODRUNNER_RUNNER_DIR=DIR cargo test -p podrunner-broker-fake --lib stand_in -- --ignored

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::{LIST, tarball};
use crate::Broker;

/// Unpack `tgz` into `dir` with the tar of the host.
fn unpack(tgz: &[u8], dir: &Path, strip: bool) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("a.tgz"), tgz).unwrap();
    let mut c = Command::new("tar");
    c.args(["-xzf", "a.tgz"]).current_dir(dir);
    if strip {
        c.arg("--strip-components=1");
    }
    assert!(c.status().unwrap().success());
    std::fs::remove_file(dir.join("a.tgz")).unwrap();
}

struct Case {
    action: PathBuf,
    node: PathBuf,
    base: String,
    token: String,
}

impl Case {
    /// Run the stand-in in `ws` with `inputs`; PATH holds only `bin`.
    fn run(&self, ws: &Path, bin: &Path, inputs: &[(&str, &str)]) -> Output {
        std::fs::create_dir_all(ws).unwrap();
        let mut c = Command::new(&self.node);
        c.arg(self.action.join("index.js")).env_clear().env("PATH", bin).env("GITHUB_WORKSPACE", ws);
        c.env("GITHUB_REPOSITORY", "example-org/app").env("GITHUB_REF", "refs/heads/main");
        c.env("INPUT_BROKER", &self.base).env("ACTIONS_RUNTIME_TOKEN", &self.token);
        for (k, v) in inputs {
            c.env(format!("INPUT_{}", k.to_uppercase()), v);
        }
        c.output().unwrap()
    }
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

#[test]
#[ignore]
fn the_stand_in_needs_only_node_and_tar() {
    let runner = PathBuf::from(std::env::var("PODRUNNER_RUNNER_DIR").expect("PODRUNNER_RUNNER_DIR"));
    let node = runner.join("externals/node20/bin/node");
    assert!(node.is_file(), "{}", node.display());
    let dir = std::env::temp_dir().join(format!("podrunner-standin-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // The tree: a file, a submodule with files, and the list of it.
    let tree = dir.join("tree");
    std::fs::create_dir_all(tree.join("sm")).unwrap();
    std::fs::write(tree.join("hello.txt"), "local-tree\n").unwrap();
    std::fs::write(tree.join("sm/f.txt"), "sub\n").unwrap();
    std::fs::write(tree.join(LIST), "1 1 sm\n").unwrap();
    let tgz = dir.join("tree.tar.gz");
    let ok = Command::new("tar").arg("-czf").arg(&tgz).arg("-C").arg(&tree).arg(".").status().unwrap();
    assert!(ok.success());
    let broker = Broker::start().unwrap();
    broker.serve_checkout(&tgz);
    let action = dir.join("action");
    unpack(&tarball(), &action, true);
    // PATH: tar only. A second PATH has nothing.
    let (bin, empty) = (dir.join("bin"), dir.join("empty"));
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&empty).unwrap();
    let tar = ["/usr/bin/tar", "/bin/tar"].into_iter().find(|t| Path::new(t).exists()).expect("tar");
    std::os::unix::fs::symlink(tar, bin.join("tar")).unwrap();
    let token = broker.state.lock().unwrap().auth.job.clone();
    let mut case = Case { action, node, base: broker.base.clone(), token };

    // The inputs of the job message: a route of the broker.
    let ws = dir.join("ws1");
    let o = case.run(&ws, &bin, &[("route", "_checkout.tar.gz"), ("list", LIST), ("level", "0")]);
    assert!(o.status.success(), "{}", text(&o));
    assert_eq!(std::fs::read_to_string(ws.join("hello.txt")).unwrap(), "local-tree\n");
    assert!(!ws.join("sm/f.txt").exists() && ws.join("sm").is_dir() && !ws.join(LIST).exists());

    // The inputs of `actions/checkout` in an action (T-708): `path`,
    // `submodules`, the ref of the run, and an ignored key (no value in the
    // log).
    let ws = dir.join("ws2");
    let o = case.run(
        &ws,
        &bin,
        &[("path", "sub/dir"), ("submodules", "true"), ("ref", "refs/heads/main"), ("token", "made-up-pat")],
    );
    assert!(o.status.success(), "{}", text(&o));
    assert_eq!(std::fs::read_to_string(ws.join("sub/dir/sm/f.txt")).unwrap(), "sub\n");
    assert!(text(&o).contains("ignores token") && !text(&o).contains("made-up-pat"), "{}", text(&o));
    let ws = dir.join("ws3");
    let o = case.run(&ws, &bin, &[("repository", "Example-Org/App")]);
    assert!(o.status.success() && ws.join("hello.txt").is_file(), "{}", text(&o));

    // T-713: the trees that the run pinned for the static values of a
    // local action: a commit of the local git, the local tree, and another
    // repository from the store.
    let (at, other) = ("a".repeat(40), "b".repeat(40));
    let at_dir = dir.join("at");
    std::fs::create_dir_all(&at_dir).unwrap();
    std::fs::write(at_dir.join("hello.txt"), "tag-v1\n").unwrap();
    let ok = Command::new("tar")
        .arg("-czf")
        .arg(dir.join(crate::actions::checkout_file(&at)))
        .arg("-C")
        .arg(&at_dir)
        .arg(".")
        .status()
        .unwrap();
    assert!(ok.success());
    let store = dir.join("store");
    let top = dir.join("top/o-second-local");
    std::fs::create_dir_all(&top).unwrap();
    std::fs::write(top.join("b.txt"), "second\n").unwrap();
    let file = store.join(crate::actions::archive_name("o/second", &other));
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let ok = Command::new("tar")
        .arg("-czf")
        .arg(&file)
        .arg("-C")
        .arg(dir.join("top"))
        .arg("o-second-local")
        .status()
        .unwrap();
    assert!(ok.success());
    let pin = |k: &str, v: &str| (k.to_string(), v.to_string());
    broker.serve_actions(
        Some(&store),
        [
            pin("checkout-ref:v1", &at),
            pin("checkout-ref:main2", ""),
            pin("checkout-ref:gone", "!`ref: gone` is not a commit of the local git"),
            pin("o/second@v2", &other),
        ]
        .into(),
    );
    let ws = dir.join("ws7");
    let o = case.run(&ws, &bin, &[("ref", " v1 "), ("path", "same")]);
    assert!(o.status.success(), "{}", text(&o));
    assert_eq!(std::fs::read_to_string(ws.join("same/hello.txt")).unwrap(), "tag-v1\n");
    let o = case.run(&ws, &bin, &[("repository", "o/second"), ("ref", "v2"), ("path", "second")]);
    assert!(o.status.success(), "{}", text(&o));
    assert_eq!(std::fs::read_to_string(ws.join("second/b.txt")).unwrap(), "second\n");
    let o = case.run(&ws, &bin, &[("ref", "main2"), ("path", "local"), ("submodules", "true")]);
    assert!(o.status.success(), "{}", text(&o));
    assert_eq!(std::fs::read_to_string(ws.join("local/sm/f.txt")).unwrap(), "sub\n");

    // Each refusal fails the step with one line, and writes no tree.
    for (inputs, want) in [
        (
            &[("repository", "other/repo")][..],
            "the checkout of other/repo in an action: the run pinned no tree for it; fix:",
        ),
        (&[("ref", "v1x")][..], "the checkout of the ref v1x in an action: the run pinned no tree"),
        (&[("repository", "O/second"), ("ref", "v2")][..], "the checkout of O/second at v2 in an action"),
        (&[("ref", "gone")][..], "the checkout of the ref gone: `ref: gone` is not a commit"),
        (
            &[("repository", "o/second"), ("ref", "v2"), ("submodules", "true")][..],
            "the checkout of o/second at v2 with submodules",
        ),
        (&[("ref", "v1"), ("submodules", "recursive")][..], "the checkout of the ref v1 with submodules"),
        (&[("submodules", "yes")][..], "must be true, false or recursive, not yes"),
        (&[("path", "../out")][..], "the path ../out is not in the workspace"),
        (&[("path", "/tmp")][..], "the path /tmp is not in the workspace"),
    ] {
        let ws = dir.join("ws4");
        let o = case.run(&ws, &bin, inputs);
        assert!(!o.status.success() && text(&o).contains(want), "{inputs:?}: {}", text(&o));
        assert!(!ws.join("hello.txt").exists(), "{inputs:?}");
    }
    // With no tar, the step names it; with no job token, the broker
    // gives no tree.
    let o = case.run(&dir.join("ws5"), &empty, &[]);
    assert!(text(&o).contains("needs tar in the job container"), "{}", text(&o));
    case.token = "made-up-runtime-token".into();
    let o = case.run(&dir.join("ws6"), &bin, &[]);
    assert!(text(&o).contains("the tree: HTTP 401"), "{}", text(&o));
    let _ = std::fs::remove_dir_all(&dir);
}
#[test]
fn the_tarball_is_a_gzip_of_a_tar_with_the_action() {
    assert_eq!(super::crc32(b"123456789"), 0xcbf4_3926);
    let gz = tarball();
    let dir = std::env::temp_dir().join(format!("podrunner-standin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.tgz"), &gz).unwrap();
    let st = std::process::Command::new("tar")
        .args(["-xzf", "a.tgz", "--strip-components=1"])
        .current_dir(&dir)
        .status()
        .unwrap();
    assert!(st.success(), "tar reads the tarball");
    assert_eq!(std::fs::read_to_string(dir.join("action.yml")).unwrap(), super::action_yml());
    let js = std::fs::read_to_string(dir.join("index.js")).unwrap();
    assert_eq!(js, super::index_js());
    assert!(!js.contains('@'), "each mark is replaced: {js}");
    let _ = std::fs::remove_dir_all(&dir);
}
