use std::cell::RefCell;
use std::path::{Path, PathBuf};

use pod_core::Failure;
use pod_core::process::Ran;
use pod_unit::{FileMeta, Host, SystemPlaces};

use super::*;
use crate::cli::run;

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

/// A directory of one test, removed at its end, also after a panic.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("podrunner-unit-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("c.toml"), "").unwrap();
        Dir(d)
    }

    fn config(&self) -> String {
        self.0.join("c.toml").display().to_string()
    }

    fn places(&self) -> SystemPlaces {
        SystemPlaces { systemd: self.0.join("system"), launch_daemons: self.0.join("daemons") }
    }

    fn unit_file(&self) -> PathBuf {
        self.0.join("system/podrunner.service")
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A host of a test: root or not, the uid that `id -u` gives, and a
/// `systemctl restart` that fails or not. It records each program it runs.
struct Fake {
    admin: bool,
    uid: &'static str,
    start_fails: bool,
    /// The first line of `systemctl --version`.
    version: &'static str,
    ran: RefCell<Vec<String>>,
}

impl Fake {
    fn new() -> Fake {
        Fake {
            admin: true,
            uid: "990",
            start_fails: false,
            version: "systemd 255 (255.4-1)",
            ran: RefCell::new(Vec::new()),
        }
    }

    fn ran(&self) -> Vec<String> {
        self.ran.borrow().clone()
    }
}

impl Host for Fake {
    fn run(&self, program: &str, args: &[&str], _: Option<&[u8]>) -> Result<Ran, Failure> {
        let line = format!("{program} {}", args.join(" "));
        self.ran.borrow_mut().push(line.clone());
        let (code, stdout) = match (program, args.first().copied()) {
            // An empty uid: an account that the host does not know.
            ("id", _) if self.uid.is_empty() => (1, String::new()),
            ("id", _) => (0, format!("{}\n", self.uid)),
            ("systemctl", Some("restart")) if self.start_fails => (1, String::new()),
            ("systemctl", Some("--version")) => (0, format!("{}\n+PAM -SELINUX\n", self.version)),
            _ => (0, String::new()),
        };
        Ok(Ran { code: Some(code), stdout, stderr: if code == 0 { String::new() } else { format!("{line}: failed") } })
    }

    fn admin(&self) -> bool {
        self.admin
    }

    fn systemd_booted(&self) -> bool {
        true
    }

    fn file_meta(&self, _: &Path) -> std::io::Result<FileMeta> {
        Err(std::io::ErrorKind::NotFound.into())
    }
}

fn system_args<'a>(d: &'a str, more: &[&'a str]) -> Vec<String> {
    let mut a = vec!["--system", "--config", d, "--state", "/var/lib/podrunner"];
    a.extend_from_slice(more);
    args(&a)
}

#[test]
fn print_unit_holds_no_credential() {
    let canary = "podrunner-canary-0f9c2d71";
    let unit = print_unit(
        Path::new("/usr/local/bin/podrunner"),
        Path::new("/etc/podrunner.toml"),
        Path::new("/var/lib/podrunner"),
        None,
    )
    .unwrap();
    assert!(unit.contains("Type=notify") && !unit.contains("WatchdogSec"));
    assert!(
        unit.contains(
            "ExecStart=/usr/local/bin/podrunner serve --config /etc/podrunner.toml --state /var/lib/podrunner"
        )
    );
    assert!(!unit.contains(canary) && !unit.to_lowercase().contains("token"));
    let unit = print_unit(Path::new("/b"), Path::new("/c"), Path::new("/s"), Some(30)).unwrap();
    assert!(unit.contains("WatchdogSec=30\n"), "{unit}");
}

/// T-576: the unit of `pod-unit`: a drain with no limit of time, a start
/// again after a failure only, and each line of the hardening in force or
/// relaxed with its reason. A word that looks like a secret is refused.
#[test]
fn print_unit_is_the_unit_of_pod_unit_with_its_hardening() {
    let unit = print_unit(Path::new("/b"), Path::new("/c"), Path::new("/s"), None).unwrap();
    for line in [
        "\nType=notify\n",
        "\nKillMode=mixed\n",
        "\nTimeoutStopSec=infinity\n",
        "\nRestart=on-failure\n",
        "\nRestartPreventExitStatus=64 78\n",
        "\nNoNewPrivileges=yes\n",
        "\nWantedBy=default.target\n",
    ] {
        assert!(unit.contains(line), "{line}: {unit}");
    }
    assert!(!unit.contains("\nMemoryDenyWriteExecute=") && !unit.contains("\nRestrictNamespaces="), "{unit}");
    assert!(!unit.contains("User=") && !unit.contains("Delegate"), "{unit}");
    assert!(!unit.contains("\nLockPersonality=") && unit.contains(": LockPersonality=yes\n"), "{unit}");
    assert_eq!(pod_unit::systemd::audit(&unit, false), Vec::<String>::new(), "{unit}");
    let secret = format!("/c/ghp_{}", "a1B2c3D4e5F6g7H8i9J0".repeat(2));
    let e = print_unit(Path::new("/b"), Path::new(&secret), Path::new("/s"), None).unwrap_err();
    assert!(!e.contains("ghp_"), "{e}");
}

/// T-343: each value is one word of `ExecStart`; a relative or empty binary
/// and a control character are refused.
#[test]
fn print_unit_quotes_each_value() {
    let unit = print_unit(Path::new("/b"), Path::new("/c d/r%.toml"), Path::new("/s\"q$"), None).unwrap();
    assert!(unit.contains("ExecStart=/b serve --config \"/c d/r%%.toml\" --state \"/s\\\"q$$\"\n"), "{unit}");
    assert!(print_unit(Path::new("/b"), Path::new("/c\nx"), Path::new("/s"), None).is_err());
    let d = Dir::new("quote");
    let c = d.config();
    for b in ["./x", ""] {
        let out = run(&["print-unit", "--config", &c, "--state", "/s", "--binary", b].map(String::from));
        assert_eq!(out.code, 64, "{b:?}: {}", out.stderr);
    }
}

/// T-588: `print-unit --system --user podrunner` is a unit of the system
/// that `pod_unit::systemd::audit` passes: each line of the hardening of a
/// unit of the system is in force or relaxed with its reason.
#[test]
fn print_unit_system_passes_the_audit_of_a_unit_of_the_system() {
    let d = Dir::new("system");
    let out = run(&[vec!["print-unit".to_string()], system_args(&d.config(), &["--user", "podrunner"])].concat());
    assert_eq!(out.code, 0, "{}", out.stderr);
    let unit = out.stdout;
    assert_eq!(pod_unit::systemd::audit(&unit, true), Vec::<String>::new(), "{unit}");
    for line in [
        "\nUser=podrunner\n",
        "\nType=notify\n",
        "\nKillMode=mixed\n",
        "\nRestart=on-failure\n",
        "\nNoNewPrivileges=yes\n",
        "\nAmbientCapabilities=\n",
        "\nProtectKernelModules=yes\n",
        "\nRemoveIPC=yes\n",
        "\nDelegate=yes\n",
        "\nDelegateSubgroup=serve\n",
        "\nStateDirectory=podrunner\n",
        "\nStateDirectoryMode=0700\n",
        "\nWantedBy=multi-user.target\n",
    ] {
        // A line in force, not the text of a relaxed comment.
        assert!(unit.contains(line), "{line}: {unit}");
    }
    for key in [
        "PrivateDevices",
        "ProtectControlGroups",
        "ProtectClock",
        "PrivateTmp",
        "RestrictNamespaces",
        "LockPersonality",
    ] {
        assert!(!unit.contains(&format!("\n{key}=")), "{key}: {unit}");
        assert!(unit.contains(&format!(": {key}=")), "{key} has no reason: {unit}");
    }
    assert!(!unit.contains("DynamicUser"), "{unit}");
    // The default account is podrunner.
    let out = run(&[vec!["print-unit".to_string()], system_args(&d.config(), &[])].concat());
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, unit);
}

/// T-588: the account of a unit of the system: root, a uid, an empty name,
/// `.`, `..`, a path and a name past 32 are refused; a name of 32 and a name
/// in another case (another account) are taken. `--user` needs `--system`.
#[test]
fn print_unit_system_refuses_root_and_a_bad_account() {
    let d = Dir::new("account");
    let c = d.config();
    let print = |more: &[&str]| run(&[vec!["print-unit".to_string()], system_args(&c, more)].concat());
    let long = "a".repeat(32);
    for good in ["podrunner", "_ci", "ci-runner.1", "ROOT", long.as_str()] {
        let out = print(&["--user", good]);
        assert_eq!(out.code, 0, "{good}: {}", out.stderr);
        assert!(out.stdout.contains(&format!("User={good}\n")), "{good}: {}", out.stdout);
    }
    let longer = "a".repeat(33);
    for bad in
        ["root", "0", "1000", "", ".", "..", "../x", "a/b", "-x", ".x", "a b", "a%n", "${{ x }}", longer.as_str()]
    {
        let out = print(&["--user", bad]);
        assert_eq!(out.code, 64, "{bad:?}: {}", out.stdout);
        assert!(out.stdout.is_empty(), "{bad:?}");
    }
    let out = run(&args(&["print-unit", "--user", "podrunner", "--config", &c, "--state", "/s"]));
    assert_eq!(out.code, 64, "--user with no --system");
    assert!(print_system_unit(Path::new("/b"), Path::new("/c"), Path::new("/s"), None, "root").is_err());
    // A flag of print-unit is not a flag of another command.
    assert_eq!(run(&args(&["serve", "--config", &c, "--state", "/s", "--system"])).code, 64);
    assert_eq!(run(&args(&["check", "--config", &c, "--json"])).code, 64);
}

/// T-588: systemd makes the state directory only for `/var/lib/NAME`.
#[test]
fn the_state_directory_of_systemd_is_one_name_under_var_lib() {
    for (state, name) in [
        ("/var/lib/podrunner", Some("podrunner")),
        ("/var/lib/podrunner/", Some("podrunner")),
        ("/var/lib/ci.runner", Some("ci.runner")),
        ("/var/lib", None),
        ("/var/lib/", None),
        ("/var/lib/a/b", None),
        ("/var/lib/..", None),
        ("/var/lib/../etc", None),
        ("/var/libx/podrunner", None),
        ("/srv/podrunner", None),
        ("var/lib/podrunner", None),
        // A name that pod-unit refuses: the operator makes the directory.
        ("/var/lib/-x", None),
        ("/var/lib/.x", None),
        ("/var/lib/a b", None),
        ("/var/lib/a@b", None),
    ] {
        assert_eq!(state_name(Path::new(state)).as_deref(), name, "{state}");
    }
    let unit = print_system_unit(Path::new("/b"), Path::new("/c"), Path::new("/srv/state"), None, "ci").unwrap();
    assert!(!unit.contains("StateDirectory"), "{unit}");
    // A name that systemd cannot hold is no state directory of systemd.
    let unit = print_system_unit(Path::new("/b"), Path::new("/c"), Path::new("/var/lib/-x"), None, "ci").unwrap();
    assert!(!unit.contains("StateDirectory"), "{unit}");
    let long = format!("/var/lib/{}", "a".repeat(64));
    let unit = print_system_unit(Path::new("/b"), Path::new("/c"), Path::new(&long), None, "ci").unwrap();
    assert!(unit.contains(&format!("StateDirectory={}\n", "a".repeat(64))), "{unit}");
    let longer = format!("/var/lib/{}", "a".repeat(65));
    assert_eq!(state_name(Path::new(&longer)), None);
}

/// T-588: `install-unit --system` writes the unit of `print-unit --system`
/// and starts it; `remove-unit --system` stops and removes it, and a second
/// removal finds nothing.
#[test]
fn install_unit_writes_and_starts_the_unit_of_the_system() {
    let d = Dir::new("install");
    let host = Fake::new();
    let out = install_on(&system_args(&d.config(), &[]), &d.places(), &host);
    let out = out.unwrap_or_else(|o| panic!("{}", o.stderr));
    assert!(out.stdout.contains("enabled and started"), "{}", out.stdout);
    let printed = run(&[vec!["print-unit".to_string()], system_args(&d.config(), &[])].concat());
    assert_eq!(std::fs::read_to_string(d.unit_file()).unwrap(), printed.stdout);
    assert_eq!(
        host.ran(),
        [
            "id -u podrunner",
            "systemctl daemon-reload",
            "systemctl enable podrunner.service",
            "systemctl restart podrunner.service",
            "systemctl --version"
        ]
    );
    assert!(out.stderr.is_empty(), "{}", out.stderr);
    let host = Fake::new();
    let out = remove_on(&args(&["--system"]), &d.places(), &host).unwrap_or_else(|o| panic!("{}", o.stderr));
    assert!(out.stdout.contains("removed"), "{}", out.stdout);
    assert!(!d.unit_file().exists());
    assert_eq!(host.ran()[0], "systemctl disable --now podrunner.service");
    let out = remove_on(&args(&["--system"]), &d.places(), &Fake::new()).unwrap_or_else(|o| panic!("{}", o.stderr));
    assert!(out.stdout.contains("nothing to remove"), "{}", out.stdout);
}

#[test]
fn install_and_remove_report_the_unit_read_error() {
    let d = Dir::new("read-error");
    let path = d.unit_file();
    std::fs::create_dir_all(&path).unwrap();
    let cause = std::fs::read_to_string(&path).unwrap_err();
    let host = Fake::new();
    let places = d.places();

    let install = install_on(&system_args(&d.config(), &[]), &places, &host);
    let remove = remove_on(&args(&["--system"]), &places, &host);
    for result in [install, remove] {
        let out = result.err().expect("a read error");
        assert_eq!(out.code, Class::of_io(cause.kind()).exit_code(), "{}", out.stderr);
        assert_eq!(out.stderr, format!("podrunner: reading {}: {cause}\n", path.display()));
    }
}

/// T-588: a caller with no root is refused (77) before any write or run.
#[test]
fn install_unit_refuses_a_caller_with_no_privilege() {
    let d = Dir::new("noroot");
    let host = Fake { admin: false, ..Fake::new() };
    let out = install_on(&system_args(&d.config(), &[]), &d.places(), &host).err().expect("a refusal");
    assert_eq!(out.code, 77, "{}", out.stderr);
    assert!(out.stderr.contains("as root"), "{}", out.stderr);
    // T-702: a Linux line, no word of Windows.
    assert!(!out.stderr.contains("Windows") && out.stderr.contains("systemctl --user"), "{}", out.stderr);
    assert!(host.ran().is_empty() && !d.unit_file().exists(), "{:?}", host.ran());
    let out = remove_on(&args(&["--system"]), &d.places(), &host).err().expect("a refusal");
    assert_eq!(out.code, 77, "{}", out.stderr);
}

/// T-588: an account whose uid is 0 is refused (64) before any write; root
/// by name is refused before the host is asked.
#[test]
fn install_unit_refuses_an_account_of_root() {
    let d = Dir::new("root");
    let host = Fake { uid: "0", ..Fake::new() };
    let out = install_on(&system_args(&d.config(), &["--user", "toor"]), &d.places(), &host).err().expect("refused");
    assert_eq!(out.code, 64, "{}", out.stderr);
    assert!(out.stderr.contains("is root"), "{}", out.stderr);
    assert_eq!(host.ran(), ["id -u toor"]);
    assert!(!d.unit_file().exists());
    let host = Fake::new();
    let out = install_on(&system_args(&d.config(), &["--user", "root"]), &d.places(), &host).err().expect("refused");
    assert_eq!(out.code, 64, "{}", out.stderr);
    assert!(host.ran().is_empty(), "{:?}", host.ran());
    let host = Fake { uid: "", ..Fake::new() };
    let out = install_on(&system_args(&d.config(), &["--user", "ci"]), &d.places(), &host).err().expect("refused");
    assert_eq!(out.code, 66, "{}", out.stderr);
    assert!(out.stderr.contains("make it first") && !d.unit_file().exists(), "{}", out.stderr);
}

/// T-588: a unit that does not start is disabled and removed.
#[test]
fn install_unit_removes_a_unit_that_does_not_start() {
    let d = Dir::new("nostart");
    let host = Fake { start_fails: true, ..Fake::new() };
    let out = install_on(&system_args(&d.config(), &[]), &d.places(), &host).err().expect("a failure");
    assert_eq!(out.code, 69, "{}", out.stderr);
    assert!(!d.unit_file().exists());
    assert!(host.ran().contains(&"systemctl disable podrunner.service".to_string()), "{:?}", host.ran());
}

/// T-588: `install-unit` and `remove-unit` need `--system`; a file of
/// another program stays.
#[test]
fn install_unit_needs_system_and_keeps_a_file_of_another_program() {
    let d = Dir::new("other");
    let host = Fake::new();
    let mut user = system_args(&d.config(), &[]);
    user.retain(|a| a != "--system");
    assert_eq!(install_on(&user, &d.places(), &host).err().expect("refused").code, 64);
    assert_eq!(remove_on(&[], &d.places(), &host).err().expect("refused").code, 64);
    assert_eq!(remove_on(&args(&["--system", "--json"]), &d.places(), &host).err().expect("refused").code, 64);
    assert!(host.ran().is_empty(), "{:?}", host.ran());
    std::fs::create_dir_all(d.unit_file().parent().unwrap()).unwrap();
    std::fs::write(d.unit_file(), "[Service]\nExecStart=/bin/other\n").unwrap();
    let out = install_on(&system_args(&d.config(), &[]), &d.places(), &host).err().expect("refused");
    assert_eq!(out.code, 78, "{}", out.stderr);
    assert_eq!(remove_on(&args(&["--system"]), &d.places(), &host).err().expect("refused").code, 78);
    assert_eq!(std::fs::read_to_string(d.unit_file()).unwrap(), "[Service]\nExecStart=/bin/other\n");
}

/// T-588: the help of each command of a unit comes from the model.
#[test]
fn each_command_of_a_unit_has_its_help() {
    for (cmd, want) in [
        ("print-unit", "Usage: podrunner print-unit [--system [--user NAME]]"),
        ("install-unit", "Usage: podrunner install-unit --system [--user NAME]"),
        ("remove-unit", "Usage: podrunner remove-unit --system"),
    ] {
        let out = run(&args(&["help", cmd]));
        assert_eq!(out.code, 0, "{cmd}: {}", out.stderr);
        assert!(out.stdout.contains(want), "{cmd}: {}", out.stdout);
        let out = run(&args(&[cmd, "--help"]));
        assert!(out.stdout.contains(want), "{cmd}: {}", out.stdout);
    }
}

/// T-588: `ExecStart` runs in `/` (a unit of the system) or in the home (a
/// user unit), so a relative `--config` or `--state` is refused (64).
#[test]
fn a_relative_config_or_state_is_refused() {
    let d = Dir::new("relative");
    let c = d.config();
    // A relative file that is there: the directory of the test.
    let rel = "Cargo.toml";
    assert!(Path::new(rel).is_file());
    for system in [false, true] {
        for (config, state) in
            [(rel, "/var/lib/podrunner"), (c.as_str(), "state"), (c.as_str(), "./s"), (c.as_str(), "")]
        {
            let mut a = args(&["print-unit", "--config", config, "--state", state]);
            if system {
                a.push("--system".into());
            }
            let out = run(&a);
            assert_eq!(out.code, 64, "{a:?}: {}", out.stdout);
            assert!(out.stderr.contains("absolute"), "{a:?}: {}", out.stderr);
        }
    }
    let host = Fake::new();
    let out = install_on(&args(&["--system", "--config", rel, "--state", "/var/lib/p"]), &d.places(), &host);
    assert_eq!(out.err().expect("refused").code, 64);
    let out = install_on(&args(&["--system", "--config", &c, "--state", "p"]), &d.places(), &host);
    assert_eq!(out.err().expect("refused").code, 64);
    assert!(host.ran().is_empty() && !d.unit_file().exists(), "{:?}", host.ran());
    assert!(print_unit(Path::new("/b"), Path::new("c"), Path::new("/s"), None).is_err());
    assert!(print_system_unit(Path::new("/b"), Path::new("/c"), Path::new("s"), None, "ci").is_err());
}

/// T-588: `DelegateSubgroup=` needs systemd 254: an install on an older
/// systemd says that podvm then holds no CPU limit.
#[test]
fn install_unit_names_a_systemd_older_than_254() {
    for (version, warns) in [
        ("systemd 253 (253.5-1)", true),
        ("systemd 249 (249.11-0ubuntu3)", true),
        ("systemd 254 (254)", false),
        ("systemd 257 (257.9-1)", false),
        ("no version", true),
    ] {
        let d = Dir::new("version");
        let host = Fake { version, ..Fake::new() };
        let out =
            install_on(&system_args(&d.config(), &[]), &d.places(), &host).unwrap_or_else(|o| panic!("{}", o.stderr));
        assert_eq!(out.code, 0);
        assert_eq!(out.stderr.contains("DelegateSubgroup"), warns, "{version}: {}", out.stderr);
    }
}

/// T-602 (COORD-121, #343): a path under a directory of `mktemp -d`
/// (`tmp.XXXXXXXXXX`) or under a dot directory with a random name is no
/// secret: `print-unit` takes it (exit 0), also with `--system`.
#[test]
fn print_unit_takes_a_config_under_a_directory_of_mktemp() {
    // The command of the report: print_unit does not read the file.
    let config = Path::new("/tmp/tmp.Ab3dEf9kQ2/serve-config/podrunner.toml");
    let unit = print_unit(Path::new("/usr/local/bin/podrunner"), config, Path::new("/tmp/state"), None);
    assert!(
        unit.as_ref().is_ok_and(|u| u.contains("--config /tmp/tmp.Ab3dEf9kQ2/serve-config/podrunner.toml")),
        "{unit:?}"
    );
    let d = Dir::new("mktemp");
    for dir in ["tmp.Ab3dEf9kQ2/serve-config", ".Xy7Qp2Lm9R/podrunner-configs"] {
        let file = d.0.join(dir).join("podrunner.toml");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "").unwrap();
        let (c, s) = (file.display().to_string(), d.0.join(dir).join("state").display().to_string());
        let out = run(&args(&["print-unit", "--config", &c, "--state", &s]));
        assert_eq!(out.code, 0, "{c}: {}", out.stderr);
        assert!(out.stdout.contains(&format!("--config {c} --state {s}\n")), "{}", out.stdout);
        let out = run(&args(&["print-unit", "--system", "--config", &c, "--state", "/var/lib/podrunner"]));
        assert_eq!(out.code, 0, "--system {c}: {}", out.stderr);
    }
}
