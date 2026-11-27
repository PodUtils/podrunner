//! A `TMPDIR` of 100 bytes (T-738). A run whose units put sockets there (a
//! hold, the vnet, a guest) ends before its first unit with exit 78, the
//! socket path and the fix, and `doctor --run` names the same path; a run
//! with no such socket is not refused for it.

mod scratch;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use scratch::Scratch;

const BIN: &str = env!("CARGO_BIN_EXE_podrunner");

/// A `TMPDIR` of 100 bytes, `/tmp/pr-long-PID-N-tt...`, at mode 0700; it
/// goes at the drop.
struct LongTmp(PathBuf);

impl LongTmp {
    fn new() -> LongTmp {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let base = format!("/tmp/pr-long-{}-{n}-", std::process::id());
        let p = PathBuf::from(format!("{base}{}", "t".repeat(100 - base.len())));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        assert_eq!(p.as_os_str().len(), 100);
        LongTmp(p)
    }
}

impl Drop for LongTmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A stand-in podvm and podnet, a store with no image, and a workflow.
fn bed(dir: &Path) -> Vec<String> {
    std::fs::create_dir_all(dir.join("store")).unwrap();
    std::fs::write(dir.join("w.yml"), "on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{run: a}]\n").unwrap();
    let podvm = "#!/bin/sh\necho '{\"podvm\": \"0.2.0-beta.32\", \"rung\": \"namespace\"}'\n";
    podrunner_unit::program_file::write(dir.join("podvm"), podvm).unwrap();
    let podnet = "#!/bin/sh\necho '{\"netns\":{\"level\":\"none\"},\"seccomp\":{\"level\":\"advisory\"}}'\n";
    podrunner_unit::program_file::write(dir.join("podnet"), podnet).unwrap();
    std::fs::write(dir.join("vnet"), "").unwrap();
    let image = format!("runner@sha256:{}", "1".repeat(64));
    let p = |n: &str| dir.join(n).display().to_string();
    vec![p("w.yml"), "--no-config".into(), "--images".into(), p("store"), "--image".into(), image]
}

/// `text` with the key of the run process (7 characters after the prefix
/// of a file of a unit) as `KEY`: two processes have two keys (T-715).
fn keyless(text: &str) -> String {
    let mut out = text.to_string();
    for prefix in ["podrunner-local-unit-", "podrunner-local-broker-"] {
        if let Some(at) = out.find(prefix).map(|i| i + prefix.len()) {
            out.replace_range(at..(at + 7).min(out.len()), "KEY");
        }
    }
    out
}

fn podrunner(verb: &[&str], args: &[String], tmp: &Path, home: &Path) -> Output {
    Command::new(BIN).args(verb).args(args).env("TMPDIR", tmp).env("HOME", home).output().unwrap()
}

/// T-738, cases 1 and 2: `--podnet`, `--vnet` (with and with no
/// `--allow`) and a guest (`--microvm`, `--incus`) put the egress socket
/// and the broker of a unit in TMPDIR. Under a TMPDIR of 100 bytes each
/// run exits 78 before its first unit with the path and the fix, and
/// `doctor --run` fails the row `socket-paths` with the same text. A guest
/// run checks its host first (`/dev/kvm`, incusd): with no such host, only
/// the row of doctor is checked. With no hold, no row and no refusal for
/// it.
#[test]
fn a_long_tmpdir_refuses_a_run_whose_units_bind_sockets_there() {
    let dir = Scratch::new("long-tmp");
    let tmp = LongTmp::new();
    let base = bed(&dir);
    let p = |n: &str| dir.join(n).display().to_string();
    let podvm = ["--podvm".to_string(), p("podvm")];
    let allow = ["--allow".to_string(), "github.com".into()];
    let cases: [(&str, Vec<String>); 5] = [
        ("--podnet", [&podvm[..], &["--podnet".into(), p("podnet")], &allow[..]].concat()),
        ("--vnet", [&podvm[..], &["--vnet".into(), p("vnet")]].concat()),
        ("--vnet --allow", [&podvm[..], &["--vnet".into(), p("vnet")], &allow[..]].concat()),
        ("--microvm", vec!["--microvm".into(), p("podvm")]),
        ("--incus --allow", [&["--incus".to_string(), p("podvm")][..], &allow[..]].concat()),
    ];
    for (name, flags) in cases {
        let args = [&base[..], &flags[..]].concat();
        let doctor = podrunner(&["doctor", "--run"], &args, &tmp.0, &dir);
        let out = String::from_utf8_lossy(&doctor.stdout);
        let row = out.lines().find(|l| l.contains(" socket-paths:")).unwrap_or_default();
        let why = keyless(row.strip_prefix("FAIL socket-paths: ").unwrap_or_else(|| panic!("{name}: {out}")));
        assert!(why.starts_with(&format!("the socket path {}/podrunner-local-", tmp.0.display())), "{name}: {why}");
        assert!(why.ends_with("fix: set TMPDIR to a shorter path"), "{name}: {why}");
        let run = podrunner(&["run"], &args, &tmp.0, &dir);
        let err = String::from_utf8_lossy(&run.stderr);
        let host = ["/dev/kvm", "incusd"].iter().any(|h| err.contains(&format!("{}: {h}", flags[0])));
        if host && run.status.code() == Some(69) {
            continue;
        }
        let line = keyless(err.lines().find(|l| l.contains("socket path")).unwrap_or_default());
        assert_eq!(run.status.code(), Some(78), "{name}: {err}");
        assert!(line.ends_with(&why), "{name}: doctor and run name one path: {row}\n{line}");
    }
    // No hold: the unit binds no socket in TMPDIR, and the run fails at the
    // missing image, not at the length of TMPDIR.
    let args = [&base[..], &podvm[..]].concat();
    let run = podrunner(&["run"], &args, &tmp.0, &dir);
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(!err.contains("socket path") && !err.contains("set TMPDIR"), "{err}");
    let doctor = podrunner(&["doctor", "--run"], &args, &tmp.0, &dir);
    assert!(!String::from_utf8_lossy(&doctor.stdout).contains("socket-paths"));
    let left: Vec<_> = std::fs::read_dir(&tmp.0).unwrap().flatten().map(|e| e.file_name()).collect();
    assert!(left.is_empty(), "the runs leave nothing in TMPDIR: {left:?}");
}
