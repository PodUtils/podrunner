//! The service unit of `serve` (`pod_unit`): `print-unit` prints a user unit
//! or a unit of the system; `install-unit --system` and `remove-unit
//! --system` install and remove the unit of the system with
//! `pod_unit::install` (T-576, T-588). No unit holds a credential.

use std::path::{Component, Path, PathBuf};

use pod_core::failure::Class;
use pod_unit::{Form, Host, Removed, SystemPlaces, Unit};

use super::{Opts, Out, USAGE, err, ok, opts};

/// The name of the unit: `podrunner.service`.
const NAME: &str = "podrunner";

/// The mark of each unit: a removal removes only a file with it.
const MARK: &str = "podrunner print-unit";

/// The account of a unit of the system when `--user` is absent.
const ACCOUNT: &str = "podrunner";

/// The lines of the hardening of `pod-unit` that a unit of `serve` cannot
/// take, each with its reason: `serve` starts the drivers and the jobs, and
/// a job runs what its build needs.
const RELAXED: &[(&str, &str)] = &[
    ("RestrictNamespaces", "podvm and the confine layer of a unit make namespaces"),
    ("MemoryDenyWriteExecute", "the runner is .NET and a job runs Node.js, which compile code at run time"),
    ("SystemCallFilter", "a job and a driver (podvm with KVM) make the system calls that they need"),
    ("SystemCallArchitectures", "a job can run a binary of another architecture of the host (i386 on x86_64)"),
    ("RestrictAddressFamilies", "a job and podnet use netlink and other families"),
    ("UMask", "the files of a job keep the modes of a runner of GitHub (umask 0022)"),
];

/// The lines that only a unit of the system has, which `serve` cannot take.
/// A user unit has none of them, and `serve` runs in such a unit. The unit
/// keeps `NoNewPrivileges`, `AmbientCapabilities`, `RestrictSUIDSGID`,
/// `ProtectKernelModules`, `RestrictRealtime`, `LockPersonality` and
/// `RemoveIPC`.
const RELAXED_SYSTEM: &[(&str, &str)] = &[
    (
        "CapabilityBoundingSet",
        "podvm and the confine layer use the capabilities of their own user namespace, which the bounding set limits",
    ),
    ("ProtectSystem", "a driver and a job write the paths of the configuration and of the build, as the account can"),
    ("ProtectHome", "podvm, the image store and the runner can be under a home, which incusd sees as it is"),
    ("PrivateTmp", "the units of serve are under /tmp, and incusd and podvm must see the same /tmp"),
    ("PrivateDevices", "podvm and the microvm kind open /dev/kvm and /dev/net/tun"),
    ("ProtectClock", "it limits the devices to a list with no /dev/kvm"),
    (
        "ProtectKernelTunables",
        "a unit mounts a new /proc in its user namespace, which the kernel refuses under a covered /proc/sys",
    ),
    (
        "ProtectKernelLogs",
        "a unit mounts a new /proc in its user namespace, which the kernel refuses under a covered /proc/kmsg",
    ),
    ("ProtectControlGroups", "podvm makes the cgroup of a unit in the subtree that Delegate= gives to the account"),
    ("ProtectHostname", "podvm sets the host name of a unit in its own UTS namespace"),
    (
        "ProtectProc",
        "the shim finds the processes of a unit in /proc, and a process of a unit can run as a subordinate uid",
    ),
];

/// The options of a unit, as the command line gives them.
struct Spec {
    binary: PathBuf,
    config: PathBuf,
    state: PathBuf,
    watchdog: Option<u32>,
    /// The account of a unit of the system; None: a user unit.
    account: Option<String>,
}

/// `print-unit [--system [--user NAME]] ...`.
pub(super) fn print(rest: &[String]) -> Result<Out, Out> {
    let spec = spec(&opts(rest)?, "print-unit", false)?;
    let text = match &spec.account {
        None => print_unit(&spec.binary, &spec.config, &spec.state, spec.watchdog),
        Some(user) => print_system_unit(&spec.binary, &spec.config, &spec.state, spec.watchdog, user),
    };
    text.map(ok).map_err(|e| err(Class::Usage, e))
}

/// `install-unit --system ...` on this host, as root.
pub(super) fn install(rest: &[String]) -> Result<Out, Out> {
    install_on(rest, &SystemPlaces::default(), &pod_unit::System::default())
}

/// `remove-unit --system` on this host, as root.
pub(super) fn remove(rest: &[String]) -> Result<Out, Out> {
    remove_on(rest, &SystemPlaces::default(), &pod_unit::System::default())
}

/// `install-unit` with the places and the host of a test. `pod-unit`
/// checks the privilege, systemd, and the uid of the account (root is
/// refused) before it writes; a unit that does not start is removed.
fn install_on(rest: &[String], places: &SystemPlaces, host: &dyn Host) -> Result<Out, Out> {
    let spec = spec(&opts(rest)?, "install-unit", true)?;
    let user = spec.account.as_deref().unwrap_or(ACCOUNT);
    let unit =
        system_unit(&spec.binary, &spec.config, &spec.state, spec.watchdog, user).map_err(|e| err(Class::Usage, e))?;
    let said = pod_unit::install::install_system(&unit, Form::Systemd, places, host).map_err(failed)?;
    Ok(ok(said.join("\n") + "\n"))
}

/// `remove-unit` with the places and the host of a test: only a file with
/// the mark of podrunner is removed. The state directory stays.
fn remove_on(rest: &[String], places: &SystemPlaces, host: &dyn Host) -> Result<Out, Out> {
    let o = opts(rest)?;
    o.only(&["--system"])?;
    if !o.flags.iter().any(|f| f == "--system") {
        return Err(err(Class::Usage, format!("remove-unit needs --system\n{USAGE}")));
    }
    // The name and the mark find the file; the program is no part of it.
    let unit = Unit::new(NAME, Path::new("/usr/local/bin/podrunner"), MARK).system(Some(ACCOUNT));
    match pod_unit::install::remove_system(&unit, Form::Systemd, places, host).map_err(failed)? {
        Removed::Nothing => Ok(ok(format!("no unit of the system {NAME}.service of podrunner: nothing to remove\n"))),
        Removed::Done(lines) => Ok(ok(lines.join("\n") + "\n")),
    }
}

fn failed(f: pod_core::Failure) -> Out {
    err(f.class, f.to_string())
}

/// The options of `cmd`. `need_system`: `--system` is necessary.
fn spec(o: &Opts, cmd: &str, need_system: bool) -> Result<Spec, Out> {
    o.only(&["--config", "--state", "--binary", "--watchdog", "--user", "--system"])?;
    let system = o.flags.iter().any(|f| f == "--system");
    if need_system && !system {
        return Err(err(
            Class::Usage,
            format!("{cmd} needs --system; for a user unit, give the text of print-unit to systemctl --user\n{USAGE}"),
        ));
    }
    let account = match (system, o.get("--user")) {
        (false, Some(_)) => return Err(err(Class::Usage, format!("--user needs --system\n{USAGE}"))),
        (false, None) => None,
        (true, None) => Some(ACCOUNT.to_string()),
        (true, Some(u)) => Some(account(&u.display().to_string()).map_err(|e| err(Class::Usage, e))?),
    };
    let binary = o.get("--binary").unwrap_or_else(|| PathBuf::from("/usr/local/bin/podrunner"));
    let watchdog = match o.get("--watchdog") {
        None => None,
        Some(w) => Some(
            w.to_str()
                .and_then(|w| w.parse::<u32>().ok())
                .filter(|w| *w > 0)
                .ok_or_else(|| err(Class::Usage, "--watchdog needs a number of seconds"))?,
        ),
    };
    // A unit file of a configuration that is not there would fail at its
    // start, not here (T-339).
    let config = o.need("--config")?;
    if !config.is_file() {
        return Err(err(Class::NotFound, format!("--config {}: no such file", config.display())));
    }
    // `ExecStart` runs an absolute path (T-343).
    if !binary.is_absolute() {
        return Err(err(Class::Usage, format!("--binary {:?}: not an absolute path", binary.display().to_string())));
    }
    Ok(Spec { binary, config, state: o.need("--state")?, watchdog, account })
}

/// The account of a unit of the system: a name with no privilege. root and
/// a number (a uid, such as 0) are refused by name; `install-unit` also
/// refuses an account whose uid is 0. `.` and `..` are no names.
fn account(user: &str) -> Result<String, String> {
    let first = user.bytes().next().is_some_and(|b| b.is_ascii_alphabetic() || b == b'_');
    let rest = user.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b));
    if !(first && rest && user.len() <= 32) {
        return Err(format!(
            "--user {user:?}: give an account name of 1 to 32 of A-Z a-z 0-9 _ . -, first a letter or _"
        ));
    }
    if user == "root" {
        return Err("--user root: give an account with no privilege".into());
    }
    Ok(user.to_string())
}

/// The user unit of systemd for `serve` (`pod_unit::systemd::render`): it
/// names paths only, no token and no key. `serve` says READY and STOPPING
/// (`Type=notify`), drains at a stop with no limit of time, and starts
/// again after a failure, not after 64 or 78. Each value is one word of
/// `ExecStart` (T-343); a control character or a path that is not UTF-8 is
/// refused.
pub fn print_unit(binary: &Path, config: &Path, state: &Path, watchdog: Option<u32>) -> Result<String, String> {
    pod_unit::systemd::render(&unit(binary, config, state, watchdog)?).map_err(|f| f.to_string())
}

/// The unit of the system for `serve`, which runs as the account `user`
/// (T-588): the unit of [`print_unit`], `User=`, the lines of the hardening
/// of a unit of the system that `serve` can take, and a delegated cgroup.
pub fn print_system_unit(
    binary: &Path,
    config: &Path,
    state: &Path,
    watchdog: Option<u32>,
    user: &str,
) -> Result<String, String> {
    let unit = system_unit(binary, config, state, watchdog, &account(user)?)?;
    pod_unit::systemd::render(&unit).map_err(|f| f.to_string())
}

fn system_unit(binary: &Path, config: &Path, state: &Path, watchdog: Option<u32>, user: &str) -> Result<Unit, String> {
    let mut unit = unit(binary, config, state, watchdog)?.system(Some(user));
    for (key, why) in RELAXED_SYSTEM {
        unit.hardening.relax(key, why).map_err(|f| f.to_string())?;
    }
    // podvm makes the cgroup of a unit beside the cgroup of its caller: the
    // account owns the cgroup of the service, and serve runs in a child.
    for (key, value) in [("Delegate", "yes"), ("DelegateSubgroup", "serve")] {
        unit.hardening.add(key, value).map_err(|f| f.to_string())?;
    }
    // systemd makes `/var/lib/NAME` for the account, mode 0700.
    unit.state_dir = state_name(state);
    Ok(unit)
}

/// NAME of a state directory `/var/lib/NAME`; None for each other path,
/// which the operator makes.
fn state_name(state: &Path) -> Option<String> {
    let mut parts = state.strip_prefix("/var/lib").ok()?.components();
    match (parts.next(), parts.next()) {
        (Some(Component::Normal(name)), None) => name.to_str().map(str::to_string),
        _ => None,
    }
}

fn unit(binary: &Path, config: &Path, state: &Path, watchdog: Option<u32>) -> Result<Unit, String> {
    let utf8 = |flag: &str, p: &Path| -> Result<String, String> {
        let s = p.to_str().ok_or_else(|| format!("{flag} {}: not UTF-8", p.display()))?;
        match s.is_empty() || s.chars().any(char::is_control) {
            true => Err(format!("{flag} {s:?}: empty, or with a control character")),
            false => Ok(s.to_string()),
        }
    };
    let (program, config, state) = (utf8("--binary", binary)?, utf8("--config", config)?, utf8("--state", state)?);
    let mut unit = Unit::new(NAME, Path::new(&program), MARK).args(["serve", "--config", &config, "--state", &state]);
    unit.description = "podrunner: CI jobs in one fresh unit for each job".into();
    unit.start = pod_unit::Start::Notify { watchdog_s: watchdog };
    // SIGTERM starts a drain: serve ends when each job is final.
    unit.stop = pod_unit::Stop { drain: true, timeout_s: None };
    // `podrunner stop` ends serve with 0: it stays down.
    unit.restart.always = false;
    for (key, why) in RELAXED {
        unit.hardening.relax(key, why).map_err(|f| f.to_string())?;
    }
    Ok(unit)
}

#[cfg(test)]
mod tests;
