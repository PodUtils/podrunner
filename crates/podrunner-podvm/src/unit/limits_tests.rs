//! The limits of a podvm unit: the arguments of `run -d`, the check of
//! the delegated cgroup, and the record.

use super::{Hold, Holds};
use podrunner_unit::limits::Limits;

/// Both limits as rlimits, and both in the leaf of the unit.
const RLIMITS: Holds = Holds { memory: Hold::Rlimit, pids: Hold::Rlimit };
const LEAF: Holds = Holds { memory: Hold::Leaf, pids: Hold::Leaf };

#[test]
fn the_limits_become_rlimits_of_the_payload() {
    let mut env = std::collections::BTreeMap::new();
    let l = Limits { memory_bytes: Some(8 << 30), cpus: None, pids: Some(512), disk_bytes: None };
    let a = super::ulimit_args(&l, RLIMITS, &mut env);
    assert_eq!(a, ["--ulimit", "as=8589934592", "--ulimit", "nproc=512"]);
    assert_eq!(env["DOTNET_GCHeapHardLimit"], format!("{:x}", 6u64 << 30));
    assert_eq!(env["DOTNET_GCRegionRange"], format!("{:x}", 6u64 << 30));
    let mut none = std::collections::BTreeMap::new();
    for hold in [RLIMITS, LEAF] {
        assert!(super::ulimit_args(&Limits::default(), hold, &mut none).is_empty() && none.is_empty());
    }
    let cpus = Limits { cpus: Some(2), ..Limits::default() };
    assert_eq!(super::ulimit_args(&cpus, RLIMITS, &mut none), ["--cpus", "2"], "T-221");
}

/// T-515: where the parent is delegated with the `memory` controller, a
/// unit gets `--memory` (podvm writes `memory.max` in the leaf of the run
/// and counts each OOM kill there) and no RLIMIT_AS, which would give
/// ENOMEM first; elsewhere RLIMIT_AS holds. The .NET heap limit stays.
#[test]
fn the_memory_of_a_unit_goes_to_its_leaf_where_the_cgroup_is_delegated() {
    let t = Tree::new("memory");
    let run = "0::/user.slice/app/run\n";
    let args = |m: u64, memory: Hold| {
        let mut env = std::collections::BTreeMap::new();
        let l = Limits { memory_bytes: Some(m), ..Limits::default() };
        let a = super::ulimit_args(&l, Holds { memory, pids: Hold::Rlimit }, &mut env);
        assert_eq!(env["DOTNET_GCHeapHardLimit"], format!("{:x}", m / 4 * 3), "{a:?}");
        a
    };
    let leaf = super::hold_of(t.check(run, "cpu memory pids\n", "", "memory"));
    assert_eq!(leaf, Hold::Leaf);
    assert_eq!(args(256 << 20, leaf), ["--memory", "268435456"]);
    // The edges: the least `--memory` of podvm; below it, RLIMIT_AS; the
    // largest value.
    assert_eq!(args(6 << 20, leaf), ["--memory", "6291456"]);
    assert_eq!(args((6 << 20) - 1, leaf), ["--ulimit", "as=6291455"]);
    assert_eq!(args(u64::MAX, leaf), ["--memory".to_string(), u64::MAX.to_string()]);
    for (why, controllers, procs, proc_text) in [
        ("no memory controller", "cpu pids\n", "", run),
        ("a process in the parent", "memory\n", "7\n", run),
        ("cgroup v1", "memory\n", "", "3:memory:/x\n"),
        ("the root", "memory\n", "", "0::/top\n"),
    ] {
        let hold = super::hold_of(t.check(proc_text, controllers, procs, "memory"));
        assert_eq!(hold, Hold::Rlimit, "{why}");
        assert_eq!(args(256 << 20, hold), ["--ulimit", "as=268435456"], "{why}");
    }
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
    let args = |pids| super::ulimit_args(&l, Holds { memory: Hold::Rlimit, pids }, &mut Default::default());
    let leaf = super::hold_of(t.check(run, "memory pids\n", "", "pids"));
    assert_eq!(leaf, Hold::Leaf);
    assert_eq!(args(leaf), ["--pids-limit", "512"]);
    for (why, controllers, procs, proc_text) in [
        ("no pids controller", "cpu memory\n", "", run),
        ("a process in the parent", "pids\n", "7\n", run),
        ("cgroup v1", "pids\n", "", "3:pids:/x\n"),
        ("the root", "pids\n", "", "0::/top\n"),
    ] {
        let hold = super::hold_of(t.check(proc_text, controllers, procs, "pids"));
        assert_eq!(hold, Hold::Rlimit, "{why}");
        assert_eq!(args(hold), ["--ulimit", "nproc=512"], "{why}");
    }
}

/// The record says where `memory` and `pids` hold; each is `enforced`
/// both ways.
#[test]
fn the_record_names_where_the_memory_and_the_pids_hold() {
    use podrunner_schema::LimitMode;
    let text = r#"{"schema":1,"kind":"podvm","arch":"x86_64","boundary":"confined","principal":"distinct",
        "container":"hooks","net":"broker","secret_delivery":["files"],"credential_store":"private-dir",
        "mount_modes":["rw"],"rootfs_fresh":"yes","tree_kill":"verified","egress_hold":"advisory","kvm":"no"}"#;
    let mode = |h| if h == Hold::Leaf { LimitMode::Enforced } else { LimitMode::None };
    for (cpu, memory, pids) in
        [(false, Hold::Rlimit, Hold::Rlimit), (true, Hold::Leaf, Hold::Leaf), (false, Hold::Leaf, Hold::Rlimit)]
    {
        let mut cap: podrunner_schema::Capability = serde_json::from_str(text).unwrap();
        super::record(&mut cap, cpu, Holds { memory, pids });
        assert_eq!(cap.limit("pids"), LimitMode::Enforced);
        assert_eq!(cap.limit("memory"), LimitMode::Enforced);
        assert_eq!(cap.limit("cpu") == LimitMode::Enforced, cpu);
        assert_eq!(cap.limits.get(super::PIDS_PER_UNIT), Some(&mode(pids)), "{pids:?}");
        assert_eq!(cap.limits.get(super::MEMORY_PER_UNIT), Some(&mode(memory)), "{memory:?}");
    }
}
