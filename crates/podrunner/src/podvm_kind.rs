//! The podvm unit kind of `serve`: the driver of `[unit.podvm]`, its hold,
//! and the self-tests of its tree kill and of its confine layer on this
//! host, before the record claims them.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pod_core::failure::Class;
use podrunner_schema::Capability;
use podrunner_slots::config::{Config, UnitConfig};
use podrunner_unit::UnitDriver;

use crate::serve::{Failure, fail};

/// The runner directory of the image when `[unit.podvm]` names none.
const RUNNER_DIR: &str = "/home/runner";

/// The work root of the hook policy of the kind `name`: the runner
/// directory that its driver uses (#279). Empty for a kind whose driver
/// observes it.
pub(crate) fn work_root(name: &str, u: &UnitConfig) -> String {
    match (&u.runner_dir, name) {
        (Some(d), _) => d.clone(),
        (None, "podvm") => RUNNER_DIR.into(),
        (None, _) => String::new(),
    }
}

/// The driver and the record of `[unit.podvm]`.
/// The checks of `[unit.podvm]` before a build: the binary, the store and
/// the limits, as the build reads them (`check` gives the same text, #232).
pub fn plan(u: &UnitConfig) -> Result<(), Failure> {
    let binary = u.binary.clone().ok_or_else(|| fail(Class::Config, "[unit.podvm] needs binary"))?;
    podrunner_slots::config::check_driver(Path::new(&binary))
        .map_err(|e| fail(Class::Config, format!("[unit.podvm] binary: {e}")))?;
    if !Path::new(&binary).is_file() {
        return Err(fail(Class::Config, "[unit.podvm] binary: not a file"));
    }
    let images = u.images.clone().ok_or_else(|| fail(Class::Config, "[unit.podvm] needs images"))?;
    if !Path::new(&images).is_dir() {
        return Err(fail(Class::Config, "[unit.podvm] images: not a directory"));
    }
    podrunner_slots::limits::limits_of(u).map_err(|e| fail(Class::Config, format!("[unit.podvm] {e}")))?;
    // The fields that the unit and its hook policy read at the start of a
    // job: each one is checked here, not at that start (#271).
    if u.command.is_empty() || u.command[0].is_empty() {
        return Err(fail(Class::Config, "[unit.podvm] command is empty: name the command of the unit"));
    }
    if let Some(d) = u.runner_dir.as_deref().filter(|d| !d.starts_with('/')) {
        return Err(fail(Class::Config, format!("[unit.podvm] runner_dir {d:?} is not an absolute path")));
    }
    if let Some(r) = u.registries.iter().find(|r| !registry_ok(r)) {
        return Err(fail(Class::Config, format!("[unit.podvm] registries: {r:?} is not a HOST or HOST:PORT")));
    }
    // Then the host, as `build` checks it (#262): the version of podvm and
    // the podnet driver.
    podrunner_podvm::Podvm { binary: binary.clone().into() }
        .require(podrunner_podvm::probe::LOWEST)
        .map_err(|e| fail(Class::Unsupported, format!("[unit.podvm] binary: {e}")))?;
    if let Some(podnet) = &u.podnet {
        podrunner_slots::config::check_driver(Path::new(podnet))
            .map_err(|e| fail(Class::Config, format!("[unit.podvm] podnet: {e}")))?;
    }
    Ok(())
}

/// A registry of the hook policy: a host name or `[IPv6]`, with an
/// optional port.
fn registry_ok(r: &str) -> bool {
    if let Some(rest) = r.strip_prefix('[') {
        let Some((ip, port)) = rest.split_once(']') else { return false };
        let port_ok =
            port.is_empty() || port.strip_prefix(':').and_then(|p| p.parse::<u16>().ok()).is_some_and(|n| n > 0);
        return ip.parse::<std::net::Ipv6Addr>().is_ok() && port_ok;
    }
    let (host, port) = r.rsplit_once(':').map_or((r, None), |(h, p)| (h, Some(p)));
    let host_ok = !host.is_empty()
        && host.split('.').all(|l| !l.is_empty() && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'));
    host_ok && port.is_none_or(|p| p.parse::<u16>().is_ok_and(|n| n > 0))
}

pub fn build(u: &UnitConfig, cfg: &Config, state: &Path) -> Result<(Arc<dyn UnitDriver>, Capability), Failure> {
    let need = |v: &Option<String>, key: &str| {
        v.clone().ok_or_else(|| fail(Class::Config, format!("[unit.podvm] needs {key}")))
    };
    let binary = need(&u.binary, "binary")?;
    podrunner_slots::config::check_driver(Path::new(&binary))
        .map_err(|e| fail(Class::Config, format!("[unit.podvm] binary: {e}")))?;
    let podvm = podrunner_podvm::Podvm { binary: binary.into() };
    let images = PathBuf::from(need(&u.images, "images")?);
    let mut unit = podrunner_podvm::PodvmUnit::new(podvm, &state.join("podvm"), &images)
        .map_err(|e| fail(Class::Unsupported, format!("[unit.podvm]: {e}")))?
        .with_runner_dir(u.runner_dir.as_deref().unwrap_or(RUNNER_DIR));
    if let Some(podnet) = &u.podnet {
        podrunner_slots::config::check_driver(Path::new(podnet))
            .map_err(|e| fail(Class::Config, format!("[unit.podvm] podnet: {e}")))?;
        unit = unit
            .with_podnet(Path::new(podnet))
            .map_err(|e| fail(Class::Unsupported, format!("[unit.podvm] podnet: {e}")))?;
    }
    if u.confine {
        unit = unit.with_confine();
    }
    let limits = podrunner_slots::limits::limits_of(u).map_err(|e| fail(Class::Config, format!("[unit.podvm] {e}")))?;
    unit = unit.with_limits(limits).map_err(|e| fail(Class::Config, format!("[unit.podvm] cpus: {}", e.detail)))?;
    // The tree kill is verified on this host, with the pinned
    // payload, before a wall limit counts as enforced; the
    // confine layer before the kind says `confined`.
    let arch = std::env::consts::ARCH;
    if let Some(pin) = cfg.pin.payload.iter().find(|p| p.arch == arch) {
        if u.confine {
            let id = podrunner_schema::UnitId::parse("u-999999998").expect("a unit id");
            match unit.verify_confine(&pin.reference, id) {
                Ok(true) => {}
                Ok(false) => {
                    eprintln!("podrunner: [unit.podvm]: the confine layer is not proven: the boundary stays redirect")
                }
                Err(e) => eprintln!("podrunner: [unit.podvm]: the self-test of the confine layer: {e}"),
            }
        }
        let id = podrunner_schema::UnitId::parse("u-999999999").expect("a unit id");
        match unit.verify_tree_kill(&pin.reference, id) {
            Ok(true) => {}
            Ok(false) => eprintln!("podrunner: [unit.podvm]: the tree kill is not proven: tree_kill stays unverified"),
            Err(e) => eprintln!("podrunner: [unit.podvm]: the self-test of the tree kill: {e}"),
        }
    }
    let cap = unit.probe().map_err(|e| fail(Class::Unsupported, e.to_string()))?;
    Ok((Arc::new(unit), cap))
}

#[cfg(test)]
mod tests {
    /// #279: a podvm kind with no `runner_dir` has the work root of its
    /// driver, not an empty one that refuses each container job.
    #[test]
    fn a_podvm_kind_with_no_runner_dir_has_the_work_root_of_the_driver() {
        let unit = |extra: &str| {
            let cfg = podrunner_slots::config::Config::parse(&format!(
                "host = \"h\"\n[unit.podvm]\nslots = 1\ncommand = [\"x\"]\n{extra}[pin]\n"
            ))
            .unwrap();
            cfg.unit["podvm"].clone()
        };
        assert_eq!(super::work_root("podvm", &unit("")), "/home/runner");
        assert_eq!(super::work_root("podvm", &unit("runner_dir = \"/r\"\n")), "/r");
        assert_eq!(super::work_root("docker", &unit("")), "");
    }

    /// #271: an empty command, a relative runner_dir and a registry that is
    /// not a host fail `check`, not the start of a job.
    #[test]
    fn the_fields_of_a_podvm_unit_are_checked() {
        use std::os::unix::fs::PermissionsExt;
        // A podvm that answers its version (`plan` asks it, #262).
        let dir = std::env::temp_dir().join(format!("podrunner-podvm-plan-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("podvm");
        std::fs::write(&fake, "#!/bin/sh\necho '{\"podvm\":\"0.2.0-beta.21\",\"rung\":\"userland\"}'\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let plan = |extra: &str| {
            let cfg = podrunner_slots::config::Config::parse(&format!(
                "host = \"h\"\n[unit.podvm]\nslots = 1\nbinary = \"{}\"\nimages = \"/\"\n{extra}\n[pin]\n",
                fake.display()
            ))
            .unwrap();
            super::plan(&cfg.unit["podvm"]).map_err(|f| f.detail)
        };
        plan("command = [\"/home/runner/run.sh\"]\nrunner_dir = \"/home/runner\"\nregistries = [\"docker.io\", \"ghcr.io:443\", \"localhost:5000\", \"[::1]:5000\", \"[fd00::1]\"]")
            .unwrap();
        for (extra, says) in [
            ("command = []", "command is empty"),
            ("command = [\"\"]", "command is empty"),
            ("command = [\"x\"]\nrunner_dir = \"relative/runner\"", "not an absolute path"),
            ("command = [\"x\"]\nregistries = [\"not a host\"]", "not a HOST"),
            ("command = [\"x\"]\nregistries = [\"ghcr.io:0\"]", "not a HOST"),
            ("command = [\"x\"]\nregistries = [\"https://ghcr.io\"]", "not a HOST"),
            ("command = [\"x\"]\nregistries = [\"\"]", "not a HOST"),
            ("command = [\"x\"]\nregistries = [\"[::1\"]", "not a HOST"),
            ("command = [\"x\"]\nregistries = [\"[nope]:5000\"]", "not a HOST"),
        ] {
            let e = plan(extra).unwrap_err();
            assert!(e.contains(says), "{extra}: {e}");
        }
    }
}
