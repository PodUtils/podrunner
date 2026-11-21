//! The labels of podvm (T-078, podvm `v0.2.0-beta.5`). Each `create` of a
//! unit, and each `run -d` of its containers, carries the label
//! `podrunner.unit=ID`: `destroy` removes each container with it, also one
//! whose processes dropped the tag. `create` takes no `--report` (T-545):
//! the unit has no run report of podvm; `inspect` gives the image and the
//! labels.

use std::path::Path;
use std::process::Stdio;

use super::PodvmUnit;

/// The key of the label of a unit.
pub const LABEL: &str = "podrunner.unit";

/// The arguments of `run` that label a container of `unit`.
pub(super) fn label_args(unit: &str) -> [String; 2] {
    ["--label".into(), format!("{LABEL}={unit}")]
}

/// The arguments of `run -d` of `unit`: its label and `fixup_args`.
pub(super) fn run_args(podvm: &crate::Podvm, unit: &str) -> Vec<String> {
    label_args(unit).into_iter().chain(fixup_args(podvm).iter().map(|a| a.to_string())).collect()
}

/// The first podvm that is known to take `--no-source-fixup`.
const NO_SOURCE_FIXUP: &str = "0.2.0-beta.30";

/// `--no-source-fixup` for `run` and `exec` of `podvm`: with the fixup,
/// podvm asks each host of the package sources of the image before the
/// payload, also under the egress hold of a unit (T-332,
/// PodUtils/podvm#464). The answer of each binary is kept.
pub(super) fn fixup_args(podvm: &crate::Podvm) -> &'static [&'static str] {
    if at_least(podvm, NO_SOURCE_FIXUP) { &["--no-source-fixup"] } else { &[] }
}

/// The first podvm whose syscall tier podrunner gives: none now. Beta.34
/// holds the raw system calls of uv and ruff (PodUtils/podvm#470), the
/// unix socket under a `-v` mount (PodUtils/podvm#586) and the absolute
/// links (PodUtils/podvm#588), but its loader fails a dynamic program in a
/// volume with `EAGAIN` and hangs a cargo build (COORD-098, GitHub #308):
/// Node of `setup-node` and Rust builds are the more common jobs (T-261).
const SYSCALL_TIER: Option<&str> = None;

/// `--podvm-tier=syscall` for `run -d` of a unit where podvm takes the
/// userland rung: each raw system call (`renameat2` of `rustix`, as uv and
/// ruff issue it) goes to podvm's handler, and `exec` enters the same tier
/// (T-261, PodUtils/podrunner#189). The tier runs on podvm's native-static
/// rung, which is x86_64 only: on another host podvm refuses the flag
/// (runner-musl.yml on aarch64). The rung of each binary is kept.
pub(super) fn tier_args(podvm: &crate::Podvm) -> &'static [&'static str] {
    tier_args_on(podvm, std::env::consts::ARCH, SYSCALL_TIER)
}

fn tier_args_on(podvm: &crate::Podvm, arch: &str, from: Option<&str>) -> &'static [&'static str] {
    let Some(from) = from else { return &[] };
    if arch != "x86_64" {
        return &[];
    }
    static SEEN: std::sync::Mutex<std::collections::BTreeMap<std::path::PathBuf, bool>> =
        std::sync::Mutex::new(std::collections::BTreeMap::new());
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    let userland = *seen.entry(podvm.binary.clone()).or_insert_with(|| podvm.rung().as_deref() == Some("userland"));
    if userland && at_least(podvm, from) { &["--podvm-tier=syscall"] } else { &[] }
}

/// Whether `podvm` is `version` or later. The answer of each binary for
/// each version is kept.
pub(super) fn at_least(podvm: &crate::Podvm, version: &str) -> bool {
    podvm.at_least(version)
}

impl PodvmUnit {
    /// Remove each container of `store` with the label of `unit`. It gives
    /// the count that `ps` listed.
    pub(super) fn remove_labeled(&self, store: &Path, unit: &str) -> usize {
        let Ok(out) = self
            .podvm
            .command()
            .args(["ps", "-a", "-q", "--filter", &format!("label={LABEL}={unit}")])
            .env("PODVM_STORE", store)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
        else {
            return 0;
        };
        let ids: Vec<String> = String::from_utf8_lossy(&out.stdout).split_whitespace().map(String::from).collect();
        for id in &ids {
            let _ = self
                .podvm
                .command()
                .args(["rm", "-f", id])
                .env("PODVM_STORE", store)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        ids.len()
    }
}

/// `argv` with no `--hostname NAME`.
pub(super) fn no_hostname<'a>(argv: &[&'a str], name: &str) -> Vec<&'a str> {
    let mut out = Vec::with_capacity(argv.len());
    let mut it = argv.iter().peekable();
    while let Some(a) = it.next() {
        if *a == "--hostname" && it.peek().is_some_and(|n| **n == name) {
            it.next();
            continue;
        }
        out.push(*a);
    }
    out
}

/// The names of the processes `pids` but `waiter`, each read once from
/// `/proc`: a process that ended after the list has no name, and is gone,
/// not an orphan (T-159).
pub(super) fn live_names(pids: &[u32], waiter: Option<u32>) -> Vec<String> {
    pids.iter()
        .filter(|p| Some(**p) != waiter)
        .filter_map(|p| std::fs::read_to_string(format!("/proc/{p}/comm")).ok())
        .map(|c| c.trim().to_string())
        .collect()
}

/// With no live supervisor of podvm, but processes of the unit, the orphans
/// went to init: no count is true then (design test 15). The `podnet pod` of
/// the hold carries the tag and can outlive the payload: it is no process of
/// the payload.
pub(super) fn orphaned(live: &[String]) -> bool {
    !live.iter().any(|c| c.starts_with("podvm")) && live.iter().any(|c| !c.starts_with("podnet"))
}

#[cfg(test)]
mod tests {
    /// A stand-in podvm whose probe gives `version` and `rung`.
    fn stand_in(version: &str, rung: &str) -> crate::Podvm {
        use std::os::unix::fs::PermissionsExt;
        let d = std::env::temp_dir().join(format!("podrunner-tier-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let bin = d.join(format!("podvm-{version}-{rung}"));
        let probe = format!("{{\"podvm\":\"{version}\",\"rung\":\"{rung}\"}}");
        std::fs::write(&bin, format!("#!/bin/sh\necho '{probe}'\n")).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        crate::Podvm { binary: bin }
    }

    /// T-564: a unit on the namespace rung gets its socket directory at
    /// the socket mount, as on the rungs with containers; one on a rung
    /// with no volume reaches the host path.
    #[test]
    fn the_socket_mount_follows_the_rungs_with_volumes() {
        use podrunner_unit::UnitDriver;
        let state = std::env::temp_dir().join(format!("podrunner-sockmount-{}", std::process::id()));
        for (rung, mount) in [("namespace", true), ("userland", true), ("supervise", true), ("chroot", false)] {
            let mut unit = crate::PodvmUnit::new(stand_in("0.2.0-beta.36", rung), &state, &state).unwrap();
            assert_eq!(unit.socket_mount().is_some(), mount, "{rung}");
            // The hold and a verified confine add to the proof (the review
            // of T-564): the socket mount stays.
            let proof = unit.capability.boundary_proof.take().unwrap_or_default();
            unit.capability.boundary_proof =
                Some(format!("{proof}; hold: podnet pod seccomp advisory; podvm --podvm-confine, verified"));
            assert_eq!(unit.socket_mount().is_some(), mount, "{rung}, held and confined");
        }
        let _ = std::fs::remove_dir_all(&state);
    }

    /// T-261: the syscall tier on the userland rung of podvm beta.34 or
    /// later on x86_64, and nowhere else (podvm refuses it on aarch64); none
    /// while `SYSCALL_TIER` is `None` (COORD-098).
    #[test]
    fn the_syscall_tier_holds_the_userland_rung_of_x86_64_only() {
        assert!(super::tier_args_on(&stand_in("0.2.0-beta.34", "userland"), "x86_64", super::SYSCALL_TIER).is_empty());
        let on = |p: &crate::Podvm, arch: &str| super::tier_args_on(p, arch, Some("0.2.0-beta.34"));
        assert_eq!(on(&stand_in("0.2.0-beta.34", "userland"), "x86_64"), ["--podvm-tier=syscall"]);
        assert!(on(&stand_in("0.2.0-beta.34", "userland"), "aarch64").is_empty());
        assert!(on(&stand_in("0.2.0-beta.34", "namespace"), "x86_64").is_empty());
        // beta.33 broke the absolute links of a unit under the tier.
        assert!(on(&stand_in("0.2.0-beta.33", "userland"), "x86_64").is_empty());
    }

    /// T-159: a process that ended after the list is no orphan.
    #[test]
    fn a_process_that_ended_is_no_orphan() {
        let gone = u32::MAX - 7;
        assert!(super::live_names(&[gone], None).is_empty());
        assert!(!super::orphaned(&super::live_names(&[gone], None)));
        let me = std::process::id();
        assert_eq!(super::live_names(&[me, gone], None).len(), 1);
        assert!(super::live_names(&[me], Some(me)).is_empty());
        assert!(super::orphaned(&["sh".into()]));
        assert!(!super::orphaned(&["podvm".into(), "sh".into()]));
        assert!(!super::orphaned(&["podnet".into()]));
    }

    /// T-182: the start with no name drops `--hostname NAME` only.
    #[test]
    fn the_start_with_no_name_drops_the_name_only() {
        let argv = ["run", "-d", "--hostname", "podrunner-u-1", "-e", "A=--hostname", "img"];
        assert_eq!(super::no_hostname(&argv, "podrunner-u-1"), ["run", "-d", "-e", "A=--hostname", "img"]);
    }
}
