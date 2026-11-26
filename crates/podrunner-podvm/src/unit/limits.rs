//! The limits of a podvm unit (G-12, T-162): rlimits of the payload, which
//! each child keeps. A CPU limit, and a limit of processes for one unit,
//! need a cgroup v2 subtree that the host delegates to the user, and podvm
//! `0.2.0-beta.23` or later (T-221, T-521).

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

/// Whether a unit can hold a limit of `controller` (`cpu`, `pids`) in a
/// cgroup here: the cgroup of this process is a cgroup v2 directory that
/// this user owns, with that controller, and podvm is new enough. The
/// answer of each binary is kept.
pub(super) fn cgroup_held(podvm: &crate::Podvm, controller: &'static str) -> Result<(), String> {
    type Seen = std::collections::BTreeMap<(std::path::PathBuf, &'static str), Result<(), String>>;
    static SEEN: std::sync::Mutex<Seen> = std::sync::Mutex::new(std::collections::BTreeMap::new());
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

/// Where a unit holds its `pids`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PidsHold {
    /// `pids.max` of the cgroup leaf of the unit (`--pids-limit`).
    Leaf,
    /// RLIMIT_NPROC (`--ulimit nproc=`): each process of the user counts.
    Uid,
}

/// Where a unit of `podvm` holds its `pids` on this host.
pub(super) fn pids_hold(podvm: &crate::Podvm) -> PidsHold {
    hold_of(cgroup_held(podvm, "pids"))
}

fn hold_of(delegation: Result<(), String>) -> PidsHold {
    if delegation.is_ok() { PidsHold::Leaf } else { PidsHold::Uid }
}

/// Record the limits that each unit holds: memory and processes, the CPU
/// where a cgroup holds it, and whether `pids` holds for each unit alone
/// ([`PIDS_PER_UNIT`]).
pub(super) fn record_modes(podvm: &crate::Podvm, cap: &mut podrunner_schema::Capability) {
    record(cap, cgroup_held(podvm, "cpu").is_ok(), pids_hold(podvm));
}

fn record(cap: &mut podrunner_schema::Capability, cpu: bool, pids: PidsHold) {
    use podrunner_schema::LimitMode;
    for name in ["memory", "pids"] {
        cap.limits.insert(name.into(), LimitMode::Enforced);
    }
    if cpu {
        cap.limits.insert("cpu".into(), LimitMode::Enforced);
    }
    let per_unit = if pids == PidsHold::Leaf { LimitMode::Enforced } else { LimitMode::None };
    cap.limits.insert(PIDS_PER_UNIT.into(), per_unit);
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

/// The rlimits of `limits` for `podvm run -d` (T-162). RLIMIT_AS bounds the
/// address space, and .NET reserves its heap at start: the GC gets a hard
/// limit of three quarters of it. `pids` goes to the leaf of the unit
/// where `pids` says so (T-521): RLIMIT_NPROC counts each process of the
/// user, also of another unit of the same user.
pub(super) fn ulimit_args(
    limits: &podrunner_unit::limits::Limits,
    pids: PidsHold,
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
    // Not both: RLIMIT_NPROC would still join the units of the user.
    match (limits.pids, pids) {
        (Some(n), PidsHold::Leaf) => a.extend(["--pids-limit".into(), n.to_string()]),
        (Some(n), PidsHold::Uid) => a.extend(["--ulimit".into(), format!("nproc={n}")]),
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
mod tests {
    use super::PidsHold;
    use podrunner_unit::limits::Limits;

    #[test]
    fn the_limits_become_rlimits_of_the_payload() {
        let mut env = std::collections::BTreeMap::new();
        let l = Limits { memory_bytes: Some(8 << 30), cpus: None, pids: Some(512), disk_bytes: None };
        let a = super::ulimit_args(&l, PidsHold::Uid, &mut env);
        assert_eq!(a, ["--ulimit", "as=8589934592", "--ulimit", "nproc=512"]);
        assert_eq!(env["DOTNET_GCHeapHardLimit"], format!("{:x}", 6u64 << 30));
        assert_eq!(env["DOTNET_GCRegionRange"], format!("{:x}", 6u64 << 30));
        let mut none = std::collections::BTreeMap::new();
        for hold in [PidsHold::Uid, PidsHold::Leaf] {
            assert!(super::ulimit_args(&Limits::default(), hold, &mut none).is_empty() && none.is_empty());
        }
        let cpus = Limits { cpus: Some(2), ..Limits::default() };
        assert_eq!(super::ulimit_args(&cpus, PidsHold::Uid, &mut none), ["--cpus", "2"], "T-221");
    }

    /// A made-up cgroup tree: this process is in `user.slice/app/run`, and
    /// podvm makes its leaf beside it, in `app`. Remove it with the guard.
    struct Tree(std::path::PathBuf);

    impl Tree {
        fn new(name: &str) -> Tree {
            let t = std::env::temp_dir().join(format!("podrunner-cgroup-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&t);
            std::fs::create_dir_all(t.join("root/user.slice/app/run")).unwrap();
            Tree(t)
        }

        /// `delegated` with `proc_text` as `/proc/self/cgroup`, and the
        /// controllers and the processes of `app`.
        fn check(&self, proc_text: &str, controllers: &str, procs: &str, of: &str) -> Result<(), String> {
            let app = self.0.join("root/user.slice/app");
            std::fs::write(self.0.join("cgroup"), proc_text).unwrap();
            std::fs::write(app.join("cgroup.controllers"), controllers).unwrap();
            std::fs::write(app.join("cgroup.procs"), procs).unwrap();
            super::delegated(&self.0.join("cgroup"), &self.0.join("root"), of)
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// T-221: a CPU limit needs a cgroup v2 directory of this user with the
    /// `cpu` controller.
    #[test]
    fn a_cpu_limit_needs_a_delegated_cgroup() {
        let t = Tree::new("cpu");
        let run = "0::/user.slice/app/run\n";
        assert_eq!(t.check(run, "cpu memory pids\n", "", "cpu"), Ok(()));
        assert!(t.check(run, "memory pids\n", "", "cpu").unwrap_err().contains("no cpu controller"));
        assert!(
            t.check("4:memory:/x\n0::/user.slice/app/run\n", "cpu\n", "", "cpu").unwrap_err().contains("cgroup v1")
        );
        assert!(t.check("0::/top\n", "cpu\n", "", "cpu").is_err(), "the parent is the root");
        assert!(t.check("1:cpu:/\n", "cpu\n", "", "cpu").is_err(), "cgroup v1 only");
        // podvm gives no controller to a child of a cgroup with a process.
        assert!(t.check(run, "cpu\n", "4242\n", "cpu").unwrap_err().contains("holds a process"));
    }

    /// T-521: where the parent is delegated with the `pids` controller, a
    /// unit gets `--pids-limit` (podvm writes `pids.max` in the leaf of the
    /// run) and no RLIMIT_NPROC; elsewhere RLIMIT_NPROC holds.
    #[test]
    fn the_pids_of_a_unit_go_to_its_leaf_where_the_cgroup_is_delegated() {
        let t = Tree::new("pids");
        let run = "0::/user.slice/app/run\n";
        let l = Limits { pids: Some(512), ..Limits::default() };
        let args = |hold| super::ulimit_args(&l, hold, &mut std::collections::BTreeMap::new());
        let leaf = super::hold_of(t.check(run, "memory pids\n", "", "pids"));
        assert_eq!(leaf, PidsHold::Leaf);
        assert_eq!(args(leaf), ["--pids-limit", "512"]);
        for (why, controllers, procs, proc_text) in [
            ("no pids controller", "cpu memory\n", "", run),
            ("a process in the parent", "pids\n", "7\n", run),
            ("cgroup v1", "pids\n", "", "3:pids:/x\n"),
            ("the root", "pids\n", "", "0::/top\n"),
        ] {
            let hold = super::hold_of(t.check(proc_text, controllers, procs, "pids"));
            assert_eq!(hold, PidsHold::Uid, "{why}");
            assert_eq!(args(hold), ["--ulimit", "nproc=512"], "{why}");
        }
    }

    /// The record says where `pids` holds; `pids` is `enforced` both ways.
    #[test]
    fn the_record_names_where_the_pids_hold() {
        use podrunner_schema::LimitMode;
        let text = r#"{"schema":1,"kind":"podvm","arch":"x86_64","boundary":"confined","principal":"distinct",
            "container":"hooks","net":"broker","secret_delivery":["files"],"credential_store":"private-dir",
            "mount_modes":["rw"],"rootfs_fresh":"yes","tree_kill":"verified","egress_hold":"advisory","kvm":"no"}"#;
        for (cpu, hold, per_unit) in
            [(false, PidsHold::Uid, LimitMode::None), (true, PidsHold::Leaf, LimitMode::Enforced)]
        {
            let mut cap: podrunner_schema::Capability = serde_json::from_str(text).unwrap();
            super::record(&mut cap, cpu, hold);
            assert_eq!(cap.limit("pids"), LimitMode::Enforced);
            assert_eq!(cap.limit("memory"), LimitMode::Enforced);
            assert_eq!(cap.limit("cpu") == LimitMode::Enforced, cpu);
            assert_eq!(cap.limit(super::PIDS_PER_UNIT), per_unit, "{hold:?}");
            assert_eq!(cap.limits.get(super::PIDS_PER_UNIT), Some(&per_unit), "the record says it");
        }
    }
}
