//! The probe of `--export` and `--log-dir` (T-614), and the paths of a unit (T-609).

use super::probe;
use crate::scratch::Scratch;

/// The names that a probe takes, one after the other.
fn names(list: &'static [&'static str]) -> impl FnMut() -> String {
    let mut it = list.iter();
    move || it.next().expect("a probe asked for more names than the test gives").to_string()
}

/// T-614: two runs of one pid (in two containers) share the directory: the
/// probe of one run leaves the probe file of the other in place.
#[test]
fn a_probe_leaves_the_probe_of_another_run() {
    let dir = Scratch::new("probe-other");
    let other = dir.join(".podrunner-probe-4026532266-1-0");
    std::fs::write(&other, b"").unwrap();
    probe(&dir, names(&["4026531836-1-0"])).unwrap();
    assert!(other.is_file(), "the probe removed the probe file of another run");
    assert!(!dir.join(".podrunner-probe-4026531836-1-0").exists(), "the probe stayed");
}

/// T-614: a link at the name of the probe is not followed: its target is
/// not written, and the link stays. The probe takes the next name.
#[test]
fn a_probe_follows_no_link() {
    let dir = Scratch::new("probe-link");
    let out = Scratch::new("probe-link-out");
    let target = out.join("target");
    let link = dir.join(".podrunner-probe-a");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    probe(&dir, names(&["a", "b"])).unwrap();
    assert!(!target.exists(), "the probe wrote through a link");
    assert!(link.symlink_metadata().unwrap().file_type().is_symlink(), "the link went");
    assert!(!dir.join(".podrunner-probe-b").exists(), "the probe stayed");
}

/// T-614: when each name is taken, the probe fails and writes nothing. The
/// text names the last probe path and says which files to remove.
#[test]
fn a_probe_with_no_free_name_fails() {
    let dir = Scratch::new("probe-full");
    let out = Scratch::new("probe-full-out");
    let target = out.join("target");
    for n in ["a", "b", "c", "d"] {
        std::os::unix::fs::symlink(&target, dir.join(format!(".podrunner-probe-{n}"))).unwrap();
    }
    let e = probe(&dir, names(&["a", "b", "c", "d"])).unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists, "{e}");
    let last = dir.join(".podrunner-probe-d");
    assert!(e.to_string().starts_with(&format!("{}: ", last.display())), "{e}");
    assert!(e.to_string().contains("remove the .podrunner-probe-* files"), "{e}");
    assert!(!target.exists(), "the probe wrote through a link");
}

/// T-614: a probe of an empty directory leaves it empty.
#[test]
fn a_probe_leaves_nothing() {
    let dir = Scratch::new("probe-empty");
    probe(&dir, names(&["a"])).unwrap();
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
}

/// T-609: `--run-sh PATH` gives the runner of each unit; with no
/// `--run-sh`, the default of the image of podrunner stays.
#[test]
fn the_runner_of_the_command_line_goes_to_each_unit() {
    use podrunner_unit::UnitDriver;
    let dir = Scratch::new("run-sh");
    let cap = serde_json::from_value(serde_json::json!({
        "schema": 1, "kind": "fake", "arch": "x86_64", "boundary": "confined", "principal": "distinct",
        "container": "hooks", "net": "broker", "secret_delivery": ["files"], "credential_store": "private-dir",
        "mount_modes": ["rw"], "rootfs_fresh": "yes", "tree_kill": "verified", "limits": {},
        "egress_hold": "advisory", "kvm": "no"
    }))
    .unwrap();
    let driver = std::sync::Arc::new(podrunner_fake::FakeUnit::new(&dir.join("units"), cap, "linux/amd64"));
    let image = driver.prepare(&format!("img@sha256:{}", "a".repeat(64)), "linux/amd64", false).unwrap();
    let digest = format!("r@sha256:{}", "0".repeat(64));
    let line = |extra: &[&str]| {
        let base = ["w.yml", "--podvm", "/p", "--images", "/i", "--image", digest.as_str()];
        base.iter().chain(extra).map(|s| s.to_string()).collect::<Vec<_>>()
    };
    let hosted = "/home/runner/actions-runner/run.sh";
    let mut e = podrunner_local::exec::InUnit::new(driver.clone(), image.clone());
    super::paths(&super::super::args::parse(&line(&["--run-sh", hosted])).unwrap(), &mut e);
    assert_eq!(e.run_sh, hosted);
    assert_eq!(podrunner_local::exec::workdir::work_folder(&e.run_sh), "/home/runner/work");
    let mut e = podrunner_local::exec::InUnit::new(driver, image);
    super::paths(&super::super::args::parse(&line(&[])).unwrap(), &mut e);
    assert_eq!(e.run_sh, "/home/runner/run.sh");
}

/// T-308 A14: a `--platform` of a Windows or macOS label, in any case, warns
/// that its jobs run on Linux; another label does not.
#[test]
fn a_route_of_windows_or_macos_warns() {
    let platforms = [("Windows-Latest", "host"), ("macos-14", "host"), ("gpu", "host"), ("ubuntu-latest", "host")];
    let a = super::Args {
        platforms: platforms.iter().map(|(l, t)| (l.to_string(), t.to_string())).collect(),
        ..super::Args::default()
    };
    let w = super::foreign_routes(&a);
    assert_eq!(w.len(), 2, "{w:?}");
    assert!(w[0].starts_with("--platform Windows-Latest=host: each job of runs-on \"Windows-Latest\" runs on Linux"));
    assert!(w[1].contains("\"macos-14\""), "{w:?}");
    assert!(super::foreign_routes(&super::Args::default()).is_empty());
}
