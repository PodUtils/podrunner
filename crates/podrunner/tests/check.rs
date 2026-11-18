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
    // A podvm that answers its version, as `check` and `serve` ask it (#262).
    let fake = |name: &str, version: &str| {
        let f = d.join(name);
        std::fs::write(&f, format!("#!/bin/sh\necho '{{\"podvm\":\"{version}\",\"rung\":\"userland\"}}'\n")).unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        f.display().to_string()
    };
    let (podvm, old) = (fake("podvm", "0.2.0-beta.21"), fake("podvm-old", "0.1.0-beta.12"));
    let open = d.join("podnet-open");
    std::fs::write(&open, "").unwrap();
    std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o777)).unwrap();
    let unit =
        |binary: &str, extra: &str| format!("[unit.podvm]\nslots = 1\nbinary = \"{binary}\"\nimages = \"/\"\n{extra}");
    let cases = [
        // #262: what `serve` refuses at its build, `check` refuses too.
        (format!("host = \"h\"\nupstream_proxy = \"not a url\"\n{pin}"), "upstream_proxy"),
        (format!("{head}{}{pin}", unit(&old, "")), "0.2.0-beta.5"),
        (format!("{head}{}{pin}", unit(&podvm, &format!("podnet = \"{}\"\n", open.display()))), "podnet"),
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
        // The incus kind (T-105): its podvm binary; a limit is a size (T-162).
        (format!("{head}[unit.incus]\nslots = 1\nbinary = \"/\"\nimages = \"/\"\n{pin}"), "[unit.incus] binary"),
        (
            format!("{head}[unit.incus]\nslots = 1\nbinary = \"/bin/true\"\nimages = \"/\"\nmem = \"4X\"\n{pin}"),
            "[unit.incus] mem",
        ),
        // #280: a podvm kind at `confined` with no podnet has no egress
        // hold, and a trusted job needs one: no kind serves it.
        (
            format!(
                "host = \"h\"\nallow_fake = true\n[trust]\norganizations = [{{ login = \"o\" }}]\n\
                 [[source]]\nname = \"s\"\nkind = \"fake\"\ntier = \"trusted\"\narch = \"{}\"\n\
                 [unit.podvm]\nslots = 1\nbinary = \"{podvm}\"\nimages = \"/\"\nconfine = true\n{pin}",
                std::env::consts::ARCH
            ),
            "no-tier-served",
        ),
        // T-185: a trusted source (floor `confined`) and a podvm kind with no
        // confine layer (at best `redirect`): no kind serves it.
        (
            format!(
                "host = \"h\"\nallow_fake = true\n[trust]\norganizations = [{{ login = \"o\" }}]\n\
                 [[source]]\nname = \"s\"\nkind = \"fake\"\ntier = \"trusted\"\narch = \"{}\"\n\
                 [unit.podvm]\nslots = 1\nbinary = \"{podvm}\"\nimages = \"/\"\n{pin}",
                std::env::consts::ARCH
            ),
            "no-tier-served",
        ),
        // #232: a `mem` that is not a size fails `check`, as it fails `serve`.
        (
            format!("{head}[unit.podvm]\nslots = 1\nbinary = \"{podvm}\"\nimages = \"/\"\nmem = \"nonsense\"\n{pin}"),
            "mem",
        ),
        (
            format!("{head}[unit.microvm]\nslots = 1\nbinary = \"/bin/true\"\nimages = \"/\"\nmem = \"2GB\"\n{pin}"),
            "mem",
        ),
        // T-162: a guest holds no limit of processes.
        (format!("{head}[unit.microvm]\nslots = 1\nbinary = \"/bin/true\"\nimages = \"/\"\npids = 9\n{pin}"), "pids"),
        // T-111: a key of the podvm kind that the incus kind does not read.
        (
            format!(
                "{head}[unit.incus]\nslots = 1\nbinary = \"/bin/true\"\nimages = \"/\"\npodnet = \"/bin/true\"\n{pin}"
            ),
            "[unit.incus] podnet",
        ),
        // T-103, T-485: an engine directory with no dockerd.
        (
            format!("{head}[unit.microvm]\nslots = 1\nbinary = \"/bin/true\"\nimages = \"/\"\nengine = \"/\"\n{pin}"),
            "no dockerd",
        ),
        (
            format!("{head}[unit.incus]\nslots = 1\nbinary = \"/bin/true\"\nimages = \"/\"\nengine = \"/\"\n{pin}"),
            "[unit.incus] engine: no dockerd",
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
        // 78: the file; 80: the host (an old podvm, #262).
        assert!(matches!(check.status.code(), Some(78 | 80)), "{i}: {text}");
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

/// T-171: `check` reads a file of `mint-serve` as the mint does: a good one
/// exits 0, and a key file of the mint that others can read exits 78.
#[test]
fn check_reads_a_mint_file_as_the_mint_does() {
    use std::os::unix::fs::PermissionsExt;
    let d = dir("mint");
    std::fs::write(d.join("cert.pem"), "made-up certificate\n").unwrap();
    std::fs::write(d.join("key.pem"), "made-up key\n").unwrap();
    std::fs::set_permissions(d.join("key.pem"), std::fs::Permissions::from_mode(0o600)).unwrap();
    let text = format!(
        "listen = \"127.0.0.1:0\"\ncert_file = \"{0}/cert.pem\"\nkey_file = \"{0}/key.pem\"\nstate = \"{0}/state\"\nallow_fake = true\n\
         [source]\nname = \"src\"\nkind = \"fake\"\ntier = \"trusted\"\narch = \"x86_64\"\n\
         [trust]\norganizations = [{{ login = \"example-org\" }}]\n\
         [[host]]\nname = \"h1\"\npin = \"sha256/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\"\nslots = 2\n",
        d.display()
    );
    std::fs::write(d.join("mint.toml"), text).unwrap();
    let check = || Command::new(BIN).args(["check", "--config"]).arg(d.join("mint.toml")).output().unwrap();
    let out = check();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8_lossy(&out.stdout).contains("mint configuration is usable"));
    std::fs::set_permissions(d.join("key.pem"), std::fs::Permissions::from_mode(0o644)).unwrap();
    let out = check();
    assert_eq!(out.status.code(), Some(78), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("open to the group"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!d.join("state").exists(), "check made the state directory");
    let _ = std::fs::remove_dir_all(&d);
}
