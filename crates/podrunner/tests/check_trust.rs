//! T-518 (the owner, Q12): `check` takes a trust table that lists
//! `pull_request_target`, in a file of `serve` and in a file of
//! `mint-serve`, and names the opt-in on stderr; `other` and `unknown` stay
//! refused in both files.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_podrunner");

/// A temporary directory that goes at the drop, also on a panic.
struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir(name: &str) -> Dir {
    let d = std::env::temp_dir().join(format!("podrunner-check-trust-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    Dir(d)
}

fn check(file: &Path, text: &str) -> Output {
    std::fs::write(file, text).unwrap();
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600)).unwrap();
    Command::new(BIN)
        .args(["check", "--config"])
        .arg(file)
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("NO_PROXY", "")
        .output()
        .unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// The line of the opt-in names the event, the facts and the risk.
fn names_the_opt_in(text: &str, table: &str) -> bool {
    text.lines().any(|l| {
        l.starts_with("podrunner: ")
            && l.contains(table)
            && l.contains("pull_request_target")
            && l.contains("not a fork")
            && l.contains("member")
            && l.contains("secrets")
    })
}

fn serve_file(trust: &str, source_trust: &str) -> String {
    let example =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples/podrunner.toml"))
            .unwrap();
    let example =
        example.replace("operator_controlled = true\n", &format!("operator_controlled = true\n{source_trust}"));
    example.replace("[trust]\norganizations = [{ login = \"example-org\" }]\n", &format!("[trust]\n{trust}"))
}

#[test]
fn check_names_the_opt_in_of_pull_request_target_in_a_file_of_serve() {
    let d = dir("serve");
    let f = d.0.join("podrunner.toml");
    let orgs = "organizations = [{ login = \"example-org\" }]\n";
    // No key, an empty list, another event: no line.
    for extra in ["", "extra_events = []\n", "extra_events = [\"release\"]\n"] {
        let o = check(&f, &serve_file(&format!("{orgs}{extra}"), ""));
        assert_eq!(o.status.code(), Some(0), "{extra}: {}", stderr(&o));
        assert!(!stderr(&o).contains("pull_request_target"), "{extra}: {}", stderr(&o));
    }
    let o = check(&f, &serve_file(&format!("{orgs}extra_events = [\"release\", \"pull_request_target\"]\n"), ""));
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert!(names_the_opt_in(&stderr(&o), "[trust]"), "{}", stderr(&o));
    assert!(String::from_utf8_lossy(&o.stdout).contains("the configuration is usable"));
    // The table of a source names its source.
    let own = "trust = { organizations = [{ login = \"example-org\" }], extra_events = [\"pull_request_target\"] }\n";
    let o = check(&f, &serve_file(orgs, own));
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert!(names_the_opt_in(&stderr(&o), "[[source]] local"), "{}", stderr(&o));
    // `other` and `unknown` stay refused, also beside the opt-in.
    for bad in ["other", "unknown"] {
        let o = check(&f, &serve_file(&format!("{orgs}extra_events = [\"pull_request_target\", \"{bad}\"]\n"), ""));
        assert_eq!(o.status.code(), Some(78), "{bad}: {}", stderr(&o));
        assert!(stderr(&o).contains("can never be trusted"), "{bad}: {}", stderr(&o));
    }
}

fn mint_file(d: &Path, extra: &str) -> String {
    format!(
        "listen = \"127.0.0.1:0\"\ncert_file = \"{0}/cert.pem\"\nkey_file = \"{0}/key.pem\"\nstate = \"{0}/state\"\n\
         allow_fake = true\n[source]\nname = \"src\"\nkind = \"fake\"\ntier = \"trusted\"\narch = \"x86_64\"\n\
         [trust]\norganizations = [{{ login = \"example-org\" }}]\n{extra}\
         [[host]]\nname = \"h1\"\npin = \"sha256/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\"\nslots = 2\n",
        d.display()
    )
}

#[test]
fn check_names_the_opt_in_of_pull_request_target_in_a_file_of_the_mint() {
    let d = dir("mint");
    std::fs::write(d.0.join("cert.pem"), "made-up certificate\n").unwrap();
    std::fs::write(d.0.join("key.pem"), "made-up key\n").unwrap();
    std::fs::set_permissions(d.0.join("key.pem"), std::fs::Permissions::from_mode(0o600)).unwrap();
    let f = d.0.join("mint.toml");
    let o = check(&f, &mint_file(&d.0, ""));
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert!(!stderr(&o).contains("pull_request_target"), "{}", stderr(&o));
    let o = check(&f, &mint_file(&d.0, "extra_events = [\"pull_request_target\"]\n"));
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert!(names_the_opt_in(&stderr(&o), "[trust]"), "{}", stderr(&o));
    assert!(String::from_utf8_lossy(&o.stdout).contains("mint configuration is usable"));
    // The mint refuses an event that can never be trusted, as serve does.
    for bad in ["other", "unknown"] {
        let o = check(&f, &mint_file(&d.0, &format!("extra_events = [\"{bad}\"]\n")));
        assert_eq!(o.status.code(), Some(78), "{bad}: {}", stderr(&o));
        assert!(stderr(&o).contains("can never be trusted"), "{bad}: {}", stderr(&o));
    }
    assert!(!d.0.join("state").exists(), "check made the state directory");
}
