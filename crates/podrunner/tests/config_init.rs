//! `podrunner config init` (T-507, githome's G-37): a `podrunner.toml`
//! from the kinds of this host that `check` takes; a second run with the
//! same input changes nothing; no kind gives 78.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_podrunner");

/// A stand-in of podvm: `probe --json` names a version and a rung.
fn podvm(dir: &Path) -> PathBuf {
    let p = dir.join("podvm");
    std::fs::write(&p, "#!/bin/sh\necho '{\"podvm\": \"0.2.0-beta.32\", \"rung\": \"namespace\"}'\n").unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

fn init(dir: &Path, kinds: &str, out: &Path, extra: &[&str]) -> Output {
    std::fs::create_dir_all(dir.join("store")).unwrap();
    std::fs::write(dir.join("key"), "made-up-key\n").unwrap();
    std::fs::set_permissions(dir.join("key"), std::fs::Permissions::from_mode(0o600)).unwrap();
    let image = format!("runner@sha256:{}", "1".repeat(64));
    let p = podvm(dir);
    let (p, store, key, out) = (p.to_str().unwrap(), dir.join("store"), dir.join("key"), out.to_str().unwrap());
    Command::new(BIN)
        .args(["config", "init", "--host", "host-a", "--source", "githome", "--program", BIN, "--arg", "serve-source"])
        .args(["--key-file", key.to_str().unwrap(), "--org", "example-org", "--podvm", p, "--images"])
        .arg(&store)
        .args(["--image", &image, "--built-at-s", "1793491200", "--runner-version", "2.338.2"])
        .args(["--kinds", kinds, "--out", out, "--json"])
        .args(extra)
        .env("PODVM_INCUS_SOCKET", dir.join("incus.socket"))
        .output()
        .unwrap()
}

#[test]
fn config_init_writes_a_file_that_check_takes_once() {
    let dir = std::env::temp_dir().join(format!("podrunner-config-init-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = dir.join("podrunner.toml");
    // No incusd: the incus kind is not here, so no kind serves the source.
    let o = init(&dir, "incus", &out, &[]);
    assert_eq!(o.status.code(), Some(78), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stderr).contains("no unit kind runs on this host"), "{o:?}");
    // podvm with no podnet holds no egress: check refuses the file, and
    // none is written.
    let o = init(&dir, "podvm", &out, &[]);
    assert_eq!(o.status.code(), Some(78), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stderr).contains("no-tier-served"), "{o:?}");
    assert!(!out.exists());
    // An incusd that takes a connection: the file is written at 0600, and
    // `check` takes it.
    let _incusd = std::os::unix::net::UnixListener::bind(dir.join("incus.socket")).unwrap();
    let o = init(&dir, "auto", &out, &[]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["changed"], true, "{v}");
    assert!(v["kinds"].as_array().unwrap().contains(&"incus".into()), "{v}");
    assert_eq!(std::fs::metadata(&out).unwrap().permissions().mode() & 0o777, 0o600);
    let check = Command::new(BIN).args(["check", "--config"]).arg(&out).output().unwrap();
    assert!(check.status.success(), "{}", String::from_utf8_lossy(&check.stderr));
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(!text.contains("made-up-key") && !text.contains("listen"), "{text}");
    // The runner of the image by its absolute path: a podvm unit runs in
    // /podrunner, where `./run.sh` (the default) is not (T-565).
    assert!(text.contains("command = [\"/home/runner/run.sh\"]"), "{text}");
    // The hosted work folder of the runner image (T-590).
    assert!(text.contains("\nwork_folder = \"/home/runner/work\"\n"), "{text}");
    // The same input again changes nothing; another input is refused.
    let o = init(&dir, "auto", &out, &[]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(serde_json::from_slice::<serde_json::Value>(&o.stdout).unwrap()["changed"], false);
    let o = init(&dir, "auto", &out, &["--slots", "2"]);
    assert_eq!(o.status.code(), Some(77), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(std::fs::read_to_string(&out).unwrap(), text);
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-565: a trusted source on the podvm kind with the hold of podnet needs
/// the confine layer (the trusted floor is `confined`). `config init`
/// runs the self-test of the layer that `serve` runs: here podvm is a
/// stand-in that holds no image and starts no unit, so the layer is not
/// proven, and the kind is refused with that cause, with no file.
#[test]
fn a_trusted_podvm_kind_needs_a_proven_confine_layer() {
    let dir = std::env::temp_dir().join(format!("podrunner-config-init-confine-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let podnet = dir.join("podnet");
    let doctor = r#"{"netns":{"level":"enforced"},"seccomp":{"level":"advisory"}}"#;
    std::fs::write(&podnet, format!("#!/bin/sh\necho '{doctor}'\n")).unwrap();
    std::fs::set_permissions(&podnet, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = dir.join("podrunner.toml");
    let o = init(&dir, "podvm", &out, &["--podnet", podnet.to_str().unwrap()]);
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(78), "{err}");
    assert!(err.contains("the confine layer that a trusted source needs is not proven here"), "{err}");
    assert!(err.contains("podvm:"), "the refusal names the kind: {err}");
    assert!(!out.exists());
    // An untrusted source runs no self-test: its floor is a guest, which
    // podvm is not, so `check` refuses the file as before.
    let o = init(&dir, "podvm", &out, &["--podnet", podnet.to_str().unwrap(), "--tier", "untrusted"]);
    let err = String::from_utf8_lossy(&o.stderr);
    assert_eq!(o.status.code(), Some(78), "{err}");
    assert!(err.contains("no-tier-served") && !err.contains("confine"), "{err}");
    assert!(!out.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-572: `--unpacked-bytes` (the size of the image in
/// `podrunner-manifest.json`) lands in the pin, and `check` takes the file;
/// a value that is not a number exits 64.
#[test]
fn the_unpacked_bytes_of_the_image_land_in_the_pin() {
    let dir = std::env::temp_dir().join(format!("podrunner-config-init-size-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let _incusd = std::os::unix::net::UnixListener::bind(dir.join("incus.socket")).unwrap();
    let out = dir.join("podrunner.toml");
    let o = init(&dir, "incus", &out, &["--unpacked-bytes", "many"]);
    assert_eq!(o.status.code(), Some(64), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(!out.exists());
    let o = init(&dir, "incus", &out, &["--unpacked-bytes", "1500000000"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains("\nunpacked_bytes = 1500000000\n"), "{text}");
    let check = Command::new(BIN).args(["check", "--config"]).arg(&out).output().unwrap();
    assert!(check.status.success(), "{}", String::from_utf8_lossy(&check.stderr));
    let _ = std::fs::remove_dir_all(&dir);
}
