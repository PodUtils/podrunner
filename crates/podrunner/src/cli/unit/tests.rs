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
    ran: RefCell<Vec<String>>,
}

impl Fake {
    fn new() -> Fake {
        Fake { admin: true, uid: "990", start_fails: false, ran: RefCell::new(Vec::new()) }
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
        "Type=notify\n",
        "KillMode=mixed\n",
        "TimeoutStopSec=infinity\n",
        "Restart=on-failure\n",
        "RestartPreventExitStatus=64 78\n",
        "NoNewPrivileges=yes\n",
        "WantedBy=default.target\n",
    ] {
        assert!(unit.contains(line), "{line}: {unit}");
    }
    assert!(!unit.contains("\nMemoryDenyWriteExecute=") && !unit.contains("\nRestrictNamespaces="), "{unit}");
    assert!(!unit.contains("User=") && !unit.contains("Delegate"), "{unit}");
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
        "User=podrunner\n",
        "Type=notify\n",
        "KillMode=mixed\n",
        "Restart=on-failure\n",
        "NoNewPrivileges=yes\n",
        "AmbientCapabilities=\n",
        "ProtectKernelModules=yes\n",
        "RemoveIPC=yes\n",
        "Delegate=yes\n",
        "DelegateSubgroup=serve\n",
        "StateDirectory=podrunner\n",
        "StateDirectoryMode=0700\n",
        "WantedBy=multi-user.target\n",
    ] {
        assert!(unit.contains(line), "{line}: {unit}");
    }
    for key in ["PrivateDevices", "ProtectControlGroups", "ProtectClock", "PrivateTmp", "RestrictNamespaces"] {
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
    ] {
        assert_eq!(state_name(Path::new(state)).as_deref(), name, "{state}");
    }
    let unit = print_system_unit(Path::new("/b"), Path::new("/c"), Path::new("/srv/state"), None, "ci").unwrap();
    assert!(!unit.contains("StateDirectory"), "{unit}");
    // A name that systemd cannot hold is no state directory of systemd.
    let unit = print_system_unit(Path::new("/b"), Path::new("/c"), Path::new("/var/lib/-x"), None, "ci");
    assert!(unit.is_err(), "{unit:?}");
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
            "systemctl restart podrunner.service"
        ]
    );
    let host = Fake::new();
    let out = remove_on(&args(&["--system"]), &d.places(), &host).unwrap_or_else(|o| panic!("{}", o.stderr));
    assert!(out.stdout.contains("removed"), "{}", out.stdout);
    assert!(!d.unit_file().exists());
    assert_eq!(host.ran()[0], "systemctl disable --now podrunner.service");
    let out = remove_on(&args(&["--system"]), &d.places(), &Fake::new()).unwrap_or_else(|o| panic!("{}", o.stderr));
    assert!(out.stdout.contains("nothing to remove"), "{}", out.stdout);
}

/// T-588: a caller with no root is refused (77) before any write or run.
#[test]
fn install_unit_refuses_a_caller_with_no_privilege() {
    let d = Dir::new("noroot");
    let host = Fake { admin: false, ..Fake::new() };
    let out = install_on(&system_args(&d.config(), &[]), &d.places(), &host).err().expect("a refusal");
    assert_eq!(out.code, 77, "{}", out.stderr);
    assert!(out.stderr.contains("as root"), "{}", out.stderr);
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

/// T-588: the help of each command of a unit comes from the usage.
#[test]
fn each_command_of_a_unit_has_its_help() {
    for (cmd, want) in [
        ("print-unit", "usage: podrunner print-unit --system [--user NAME]"),
        ("install-unit", "usage: podrunner install-unit --system [--user NAME]"),
        ("remove-unit", "usage: podrunner remove-unit --system"),
    ] {
        let out = run(&args(&["help", cmd]));
        assert_eq!(out.code, 0, "{cmd}: {}", out.stderr);
        assert!(out.stdout.contains(want), "{cmd}: {}", out.stdout);
        let out = run(&args(&[cmd, "--help"]));
        assert!(out.stdout.contains(want), "{cmd}: {}", out.stdout);
    }
}
