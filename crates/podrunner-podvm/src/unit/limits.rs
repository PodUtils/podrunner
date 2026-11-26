//! The limits of a podvm unit (G-12, T-162): rlimits of the payload, which
//! each child keeps. A CPU limit, and a limit of memory and of processes
//! for one unit, need a cgroup v2 subtree that the host delegates to the
//! user, and podvm `0.2.0-beta.23` or later (T-221, T-515, T-521).

use podrunner_unit::{UnitCode, UnitError};

use super::PodvmUnit;

impl PodvmUnit {
    /// The limits of each unit (`[unit.podvm] mem`, `pids`, `cpus`; T-162,
    /// T-221): rlimits of the payload that each child keeps, and `--cpus`
    /// where the host delegates a cgroup to the user.
    pub fn with_limits(mut self, limits: podrunner_unit::limits::Limits) -> Result<PodvmUnit, UnitError> {
        if limits.cpus.is_some()
            && let Err(why) = cgroup_held(&self.podvm, "cpu")
        {
            return Err(UnitError::new(
                UnitCode::UnitStartFailed,
                format!(
                    "podvm holds a CPU limit only in a cgroup v2 subtree that the host delegates to this user (a systemd user session), with podvm {CGROUP_FROM} or later: {why}"
                ),
            ));
        }
        limits.record(&mut self.capability);
        self.limits = limits;
        Ok(self)
    }
}

/// The first podvm that holds `--cpus` and `--pids-limit` on `run -d` in a
/// delegated cgroup (PodUtils/podvm@T-1747).
const CGROUP_FROM: &str = "0.2.0-beta.23";

/// The key of the capability record that says where `pids` holds:
/// `enforced` for `pids.max` of the leaf of each unit, `none` for
/// RLIMIT_NPROC, which each process of the user counts against.
pub const PIDS_PER_UNIT: &str = "pids-per-unit";

/// The key of the capability record that says where `memory` holds:
/// `enforced` for `memory.max` of the leaf of each unit, where the kernel
/// ends a process past it and podvm counts the kill (T-515); `none` for
/// RLIMIT_AS, which holds for each process alone and gives ENOMEM.
pub const MEMORY_PER_UNIT: &str = "memory-per-unit";

/// The least `--memory` that podvm takes (as docker): a smaller limit
/// stays RLIMIT_AS.
const LEAF_MEMORY_MIN: u64 = 6 << 20;

type Seen = std::collections::BTreeMap<(std::path::PathBuf, &'static str), Result<(), String>>;
/// The answer of [`cgroup_held`] for each binary and controller.
static SEEN: std::sync::Mutex<Seen> = std::sync::Mutex::new(std::collections::BTreeMap::new());

/// Whether a unit can hold a limit of `controller` (`cpu`, `memory`, `pids`) in a
/// cgroup here: the cgroup of this process is a cgroup v2 directory that
/// this user owns, with that controller, and podvm is new enough. The
/// answer of each binary is kept.
pub(super) fn cgroup_held(podvm: &crate::Podvm, controller: &'static str) -> Result<(), String> {
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    seen.entry((podvm.binary.clone(), controller))
        .or_insert_with(|| {
            let low = crate::probe::Version::parse(CGROUP_FROM).expect("a version");
            match podvm.version() {
                Some(v) if v >= low => delegated(
                    std::path::Path::new("/proc/self/cgroup"),
                    std::path::Path::new("/sys/fs/cgroup"),
                    controller,
                ),
                _ => Err(format!("podvm is older than {CGROUP_FROM}")),
            }
        })
        .clone()
}

/// The parent of the cgroup of `proc_cgroup` under `root` is a cgroup v2
/// directory of this user with `controller` and with no process: podvm
/// makes the leaf of a run beside the cgroup of its caller (a systemd user
/// session: the parent is `app.slice` of `user@UID.service`), and cgroup v2
/// gives no controller to the child of a cgroup with a process (podvm
/// refuses such a parent).
fn delegated(proc_cgroup: &std::path::Path, root: &std::path::Path, controller: &str) -> Result<(), String> {
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
    if !controllers.split_whitespace().any(|c| c == controller) {
        return Err(format!("{} has no {controller} controller", dir.display()));
    }
    match std::fs::read_to_string(dir.join("cgroup.procs")) {
        Ok(p) if !p.trim().is_empty() => Err(format!("{} holds a process", dir.display())),
        _ => Ok(()),
    }
}

/// Keep `verdict` as the answer of [`cgroup_held`] for `podvm` and
/// `controller`: a test of a fake podvm chooses the host.
#[cfg(test)]
pub(super) fn assume(podvm: &crate::Podvm, controller: &'static str, verdict: Result<(), String>) {
    SEEN.lock().unwrap_or_else(|e| e.into_inner()).insert((podvm.binary.clone(), controller), verdict);
}

/// Where a unit holds a limit of memory or of processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Hold {
    /// A file of the cgroup leaf of the unit: `memory.max` (`--memory`),
    /// `pids.max` (`--pids-limit`).
    Leaf,
    /// An rlimit: RLIMIT_AS (`--ulimit as=`) holds for each process alone,
    /// RLIMIT_NPROC (`--ulimit nproc=`) counts each process of the user.
    Rlimit,
}

/// Where a unit holds its `memory` and its `pids`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Holds {
    pub memory: Hold,
    pub pids: Hold,
}

/// Where a unit of `podvm` holds its `memory` and `pids` on this host.
pub(super) fn holds(podvm: &crate::Podvm) -> Holds {
    Holds { memory: hold_of(cgroup_held(podvm, "memory")), pids: hold_of(cgroup_held(podvm, "pids")) }
}

fn hold_of(delegation: Result<(), String>) -> Hold {
    if delegation.is_ok() { Hold::Leaf } else { Hold::Rlimit }
}

/// Record the limits that each unit holds: memory and processes, the CPU
/// where a cgroup holds it, and whether `memory` and `pids` hold for each
/// unit alone ([`MEMORY_PER_UNIT`], [`PIDS_PER_UNIT`]).
pub(super) fn record_modes(podvm: &crate::Podvm, cap: &mut podrunner_schema::Capability) {
    record(cap, cgroup_held(podvm, "cpu").is_ok(), holds(podvm));
}

fn record(cap: &mut podrunner_schema::Capability, cpu: bool, holds: Holds) {
    use podrunner_schema::LimitMode;
    for name in ["memory", "pids"] {
        cap.limits.insert(name.into(), LimitMode::Enforced);
    }
    if cpu {
        cap.limits.insert("cpu".into(), LimitMode::Enforced);
    }
    for (key, hold) in [(MEMORY_PER_UNIT, holds.memory), (PIDS_PER_UNIT, holds.pids)] {
        let per_unit = if hold == Hold::Leaf { LimitMode::Enforced } else { LimitMode::None };
        cap.limits.insert(key.into(), per_unit);
    }
}

/// The limits of a unit: the lesser of the kind and the offer; the CPU
/// only where a cgroup holds it.
pub(super) fn held(
    podvm: &crate::Podvm,
    kind: &podrunner_unit::limits::Limits,
    spec: &podrunner_unit::limits::Limits,
) -> podrunner_unit::limits::Limits {
    let l = kind.lesser(spec);
    podrunner_unit::limits::Limits { cpus: l.cpus.filter(|_| cgroup_held(podvm, "cpu").is_ok()), ..l }
}

/// The limits of `limits` for `podvm run -d` (T-162). `memory` goes to
/// `memory.max` of the leaf of the unit where `holds` says so (T-515): the
/// kernel then ends a process past it, and podvm counts the kill in the
/// run report, so the job ends with `limit`. Elsewhere RLIMIT_AS bounds
/// the address space of each process, and an allocation past it gets
/// ENOMEM. .NET reserves its heap at start: the GC gets a hard limit of
/// three quarters of the limit. `pids` goes to the leaf of the unit where
/// `holds` says so (T-521): RLIMIT_NPROC counts each process of the user,
/// also of another unit of the same user.
pub(super) fn ulimit_args(
    limits: &podrunner_unit::limits::Limits,
    holds: Holds,
    env: &mut std::collections::BTreeMap<String, String>,
) -> Vec<String> {
    let mut a = Vec::new();
    if let Some(m) = limits.memory_bytes {
        // Not both: RLIMIT_AS would give ENOMEM before the OOM killer.
        if holds.memory == Hold::Leaf && m >= LEAF_MEMORY_MIN {
            a.extend(["--memory".into(), m.to_string()]);
        } else {
            a.extend(["--ulimit".into(), format!("as={m}")]);
        }
        // The GC of .NET reserves its regions (256 GiB by default) at start,
        // and fails under RLIMIT_AS (runner-image run 43690754546): its range
        // is the heap limit too.
        let heap = format!("{:x}", m / 4 * 3);
        env.insert("DOTNET_GCHeapHardLimit".into(), heap.clone());
        env.insert("DOTNET_GCRegionRange".into(), heap);
    }
    // Not both: RLIMIT_NPROC would still join the units of the user.
    match (limits.pids, holds.pids) {
        (Some(n), Hold::Leaf) => a.extend(["--pids-limit".into(), n.to_string()]),
        (Some(n), Hold::Rlimit) => a.extend(["--ulimit".into(), format!("nproc={n}")]),
        (None, _) => {}
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
#[path = "limits_tests.rs"]
mod tests;
