//! The limits of a podvm unit (G-12, T-162): rlimits of the payload, which
//! each child keeps. A CPU limit needs a cgroup v2 subtree that the host
//! delegates to the user, and podvm `0.2.0-beta.23` or later (T-221).

use podrunner_unit::{UnitCode, UnitError};

use super::PodvmUnit;

impl PodvmUnit {
    /// The limits of each unit (`[unit.podvm] mem`, `pids`, `cpus`; T-162,
    /// T-221): rlimits of the payload that each child keeps, and `--cpus`
    /// where the host delegates a cgroup to the user.
    pub fn with_limits(mut self, limits: podrunner_unit::limits::Limits) -> Result<PodvmUnit, UnitError> {
        if limits.cpus.is_some()
            && let Err(why) = cpu_held(&self.podvm)
        {
            return Err(UnitError::new(
                UnitCode::UnitStartFailed,
                format!(
                    "podvm holds a CPU limit only in a cgroup v2 subtree that the host delegates to this user (a systemd user session), with podvm {CPUS_FROM} or later: {why}"
                ),
            ));
        }
        limits.record(&mut self.capability);
        self.limits = limits;
        Ok(self)
    }
}

/// The first podvm that holds `--cpus` on `run -d` in a delegated cgroup
/// (PodUtils/podvm@T-1747).
const CPUS_FROM: &str = "0.2.0-beta.23";

/// Whether a unit can hold a CPU limit here: the cgroup of this process is
/// a cgroup v2 directory that this user owns, with the `cpu` controller,
/// and podvm is new enough. The answer of each binary is kept.
pub(super) fn cpu_held(podvm: &crate::Podvm) -> Result<(), String> {
    static SEEN: std::sync::Mutex<std::collections::BTreeMap<std::path::PathBuf, Result<(), String>>> =
        std::sync::Mutex::new(std::collections::BTreeMap::new());
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    seen.entry(podvm.binary.clone())
        .or_insert_with(|| {
            let low = crate::probe::Version::parse(CPUS_FROM).expect("a version");
            match podvm.version() {
                Some(v) if v >= low => {
                    delegated(std::path::Path::new("/proc/self/cgroup"), std::path::Path::new("/sys/fs/cgroup"))
                }
                _ => Err(format!("podvm is older than {CPUS_FROM}")),
            }
        })
        .clone()
}

/// The parent of the cgroup of `proc_cgroup` under `root` is a cgroup v2
/// directory of this user with the `cpu` controller: podvm makes the leaf
/// of a run beside the cgroup of its caller (a systemd user session: the
/// parent is `app.slice` of `user@UID.service`).
fn delegated(proc_cgroup: &std::path::Path, root: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let text = std::fs::read_to_string(proc_cgroup).map_err(|e| format!("{}: {e}", proc_cgroup.display()))?;
    let path = text.lines().find_map(|l| l.strip_prefix("0::")).ok_or("no cgroup v2 (the host has cgroup v1)")?;
    if text.lines().any(|l| !l.starts_with("0::") && !l.trim().is_empty()) {
        return Err("a host with cgroup v1 controllers".into());
    }
    let own = root.join(path.trim().trim_start_matches('/'));
    let dir =
        own.parent().filter(|p| p.starts_with(root) && *p != root).ok_or("the cgroup of this process is the root")?;
    let meta = std::fs::metadata(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    // SAFETY: geteuid has no failure.
    if meta.uid() != unsafe { libc::geteuid() } {
        return Err(format!("{} is not delegated to this user", dir.display()));
    }
    let controllers = std::fs::read_to_string(dir.join("cgroup.controllers")).unwrap_or_default();
    match controllers.split_whitespace().any(|c| c == "cpu") {
        true => Ok(()),
        false => Err(format!("{} has no cpu controller", dir.display())),
    }
}

/// The names of the limits that each unit holds: memory and processes as
/// rlimits, and the CPU where a cgroup holds it.
pub(super) fn held_names(podvm: &crate::Podvm) -> Vec<&'static str> {
    let mut names = vec!["memory", "pids"];
    if cpu_held(podvm).is_ok() {
        names.push("cpu");
    }
    names
}

/// The limits of a unit: the lesser of the kind and the offer; the CPU
/// only where a cgroup holds it.
pub(super) fn held(
    podvm: &crate::Podvm,
    kind: &podrunner_unit::limits::Limits,
    spec: &podrunner_unit::limits::Limits,
) -> podrunner_unit::limits::Limits {
    let l = kind.lesser(spec);
    podrunner_unit::limits::Limits { cpus: l.cpus.filter(|_| cpu_held(podvm).is_ok()), ..l }
}

/// The rlimits of `limits` for `podvm run -d` (T-162). RLIMIT_AS bounds the
/// address space, and .NET reserves its heap at start: the GC gets a hard
/// limit of three quarters of it. RLIMIT_NPROC counts each process of the
/// user, also of another unit of the same user.
pub(super) fn ulimit_args(
    limits: &podrunner_unit::limits::Limits,
    env: &mut std::collections::BTreeMap<String, String>,
) -> Vec<String> {
    let mut a = Vec::new();
    if let Some(m) = limits.memory_bytes {
        a.extend(["--ulimit".into(), format!("as={m}")]);
        // The GC of .NET reserves its regions (256 GiB by default) at start,
        // and fails under RLIMIT_AS (runner-image run 43690754546): its range
        // is the heap limit too.
        let heap = format!("{:x}", m / 4 * 3);
        env.insert("DOTNET_GCHeapHardLimit".into(), heap.clone());
        env.insert("DOTNET_GCRegionRange".into(), heap);
    }
    if let Some(n) = limits.pids {
        a.extend(["--ulimit".into(), format!("nproc={n}")]);
    }
    // In a leaf of the delegated cgroup of the user (T-221).
    if let Some(c) = limits.cpus {
        a.extend(["--cpus".into(), c.to_string()]);
    }
    a
}

/// The variables of the payload that the runner needs in a unit.
pub(super) fn runner_env(env: &mut std::collections::BTreeMap<String, String>) {
    // The .NET runner dies at start under a small RLIMIT_FSIZE with W^X on.
    env.insert("DOTNET_EnableWriteXorExecute".into(), "0".into());
    // On the namespace rung of a user with no root, the payload is uid 0 of a
    // user namespace, and the runner refuses root without this (podvm
    // 0.2.0-beta.25, T-301).
    env.insert("RUNNER_ALLOW_RUNASROOT".into(), "1".into());
    // Root there maps one uid only: a chown of tar fails with EINVAL, and npm
    // 6 runs its scripts as the unmapped `nobody` (T-301, T-317).
    env.insert("TAR_OPTIONS".into(), "--no-same-owner".into());
    env.insert("npm_config_unsafe_perm".into(), "true".into());
}

#[cfg(test)]
mod tests {
    use podrunner_unit::limits::Limits;

    #[test]
    fn the_limits_become_rlimits_of_the_payload() {
        let mut env = std::collections::BTreeMap::new();
        let l = Limits { memory_bytes: Some(8 << 30), cpus: None, pids: Some(512), disk_bytes: None };
        let a = super::ulimit_args(&l, &mut env);
        assert_eq!(a, ["--ulimit", "as=8589934592", "--ulimit", "nproc=512"]);
        assert_eq!(env["DOTNET_GCHeapHardLimit"], format!("{:x}", 6u64 << 30));
        assert_eq!(env["DOTNET_GCRegionRange"], format!("{:x}", 6u64 << 30));
        let mut none = std::collections::BTreeMap::new();
        assert!(super::ulimit_args(&Limits::default(), &mut none).is_empty() && none.is_empty());
        let cpus = Limits { cpus: Some(2), ..Limits::default() };
        assert_eq!(super::ulimit_args(&cpus, &mut none), ["--cpus", "2"], "T-221");
    }

    /// T-221: a CPU limit needs a cgroup v2 directory of this user with the
    /// `cpu` controller.
    #[test]
    fn a_cpu_limit_needs_a_delegated_cgroup() {
        let t = std::env::temp_dir().join(format!("podrunner-cgroup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        // The process is in `app/run`; podvm makes its leaf beside it, in `app`.
        let leaf = t.join("root/user.slice/app");
        std::fs::create_dir_all(leaf.join("run")).unwrap();
        let proc = t.join("cgroup");
        let check = |proc_text: &str, controllers: &str| {
            std::fs::write(&proc, proc_text).unwrap();
            std::fs::write(leaf.join("cgroup.controllers"), controllers).unwrap();
            super::delegated(&proc, &t.join("root"))
        };
        assert_eq!(check("0::/user.slice/app/run\n", "cpu memory pids\n"), Ok(()));
        assert!(check("0::/user.slice/app/run\n", "memory pids\n").unwrap_err().contains("no cpu controller"));
        assert!(check("4:memory:/x\n0::/user.slice/app/run\n", "cpu\n").unwrap_err().contains("cgroup v1"));
        assert!(check("0::/top\n", "cpu\n").is_err(), "the parent is the root");
        assert!(check("1:cpu:/\n", "cpu\n").is_err(), "cgroup v1 only");
        let _ = std::fs::remove_dir_all(&t);
    }
}
