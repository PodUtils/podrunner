//! `podrunner check` (GitHub #58): the checks of `serve` with no side
//! effect, the same text as `serve` for a bad file, the shipped example,
//! and the keys of the configuration documents.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_podrunner");

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("podrunner-check-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn listing(d: &Path) -> Vec<String> {
    let mut v: Vec<String> =
        std::fs::read_dir(d).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    v.sort();
    v
}

#[test]
fn the_example_passes_and_check_touches_nothing() {
    let d = dir("example");
    let before = listing(&d);
    let out = Command::new(BIN)
        .args(["check", "--config"])
        .arg(root().join("docs/examples/podrunner.toml"))
        .current_dir(&d)
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("NO_PROXY", "")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(listing(&d), before);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn check_and_serve_refuse_a_bad_file_with_one_text() {
    let d = dir("bad");
    let key = d.join("app.pem");
    std::fs::write(&key, "not a key").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o640)).unwrap();
    let head = "host = \"h\"\n[trust]\norganizations = [{ login = \"o\" }]\n";
    let pin = "[pin]\n";
    let cases = [
        // A misspelled table and a value of the wrong type name the key.
        (format!("{head}[[sources]]\nname = \"x\"\n{pin}"), "sources"),
        (format!("{head}[deadlines]\nclaim_s = \"60\"\n{pin}"), "claim_s"),
        // A key file open to the group.
        (
            format!(
                "{head}[[source]]\nname = \"s\"\nkind = \"github-scaleset\"\ntier = \"trusted\"\narch = \"x86_64\"\n\
                 sidecar = \"/bin/true\"\nconfig_url = \"https://github.com/o\"\ninstallation_id = 1\n\
                 app_id = \"1\"\nscale_set = \"s\"\nkey_file = \"{}\"\n{pin}",
                key.display()
            ),
            "open to the group",
        ),
        // A podvm binary that is a directory.
        (format!("{head}[unit.podvm]\nslots = 1\ncommand = [\"x\"]\nbinary = \"/\"\nimages = \"/\"\n{pin}"), "binary"),
        // The microvm kind (T-037): its podvm binary, and its vCPUs.
        (format!("{head}[unit.microvm]\nslots = 1\nbinary = \"/\"\nimages = \"/\"\n{pin}"), "[unit.microvm] binary"),
        (format!("{head}[unit.microvm]\nslots = 1\nbinary = \"/bin/true\"\nimages = \"/\"\ncpus = 0\n{pin}"), "cpus"),
        // The incus kind (T-105): its podvm binary, and no size of a guest.
        (format!("{head}[unit.incus]\nslots = 1\nbinary = \"/\"\nimages = \"/\"\n{pin}"), "[unit.incus] binary"),
        (
            format!("{head}[unit.incus]\nslots = 1\nbinary = \"/bin/true\"\nimages = \"/\"\nmem = \"4G\"\n{pin}"),
            "microvm kind only",
        ),
    ];
    for (i, (text, word)) in cases.iter().enumerate() {
        let file = d.join(format!("{i}.toml"));
        std::fs::write(&file, text).unwrap();
        let check = Command::new(BIN).args(["check", "--config"]).arg(&file).output().unwrap();
        let state = d.join(format!("state-{i}"));
        let serve =
            Command::new(BIN).args(["serve", "--config"]).arg(&file).arg("--state").arg(&state).output().unwrap();
        let text = String::from_utf8_lossy(&check.stderr);
        assert_eq!(check.status.code(), Some(78), "{i}: {text}");
        assert!(text.contains(word), "{i}: {text}");
        assert_eq!(serve.status.code(), check.status.code(), "{i}");
        assert_eq!(serve.stderr, check.stderr, "{i}");
        // serve refused before the replay: no journal.
        assert!(!state.join("journal.jsonl").exists(), "{i}");
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// The TOML blocks of the documents use only keys that exist: a block may
/// leave out a key or hold an example value, but it names no key that the
/// type refuses as unknown.
#[test]
fn the_keys_of_the_documents_exist() {
    fn keys<T: serde::de::DeserializeOwned>(t: &str) -> Result<(), String> {
        match toml::from_str::<T>(t) {
            Err(e) if e.to_string().contains("unknown field") => Err(e.to_string()),
            _ => Ok(()),
        }
    }
    let check = |b: &str| -> Result<(), String> {
        if b.contains("listen =") {
            keys::<podrunner::mint_serve::MintConfig>(b)
        } else {
            keys::<podrunner_slots::config::Config>(b)
        }
    };
    let doc = std::fs::read_to_string(root().join("docs/configuration.md")).unwrap();
    let blocks: Vec<&str> = doc.split("```toml\n").skip(1).map(|b| b.split("```").next().unwrap()).collect();
    assert!(blocks.len() >= 3, "{}", blocks.len());
    for b in &blocks {
        assert_eq!(check(b), Ok(()), "{b}");
        // A misspelled key fails.
        let bad = b.replacen("\nname = ", "\nnmae = ", 1);
        if bad != *b {
            assert!(check(&bad).is_err(), "{bad}");
        }
    }
}
