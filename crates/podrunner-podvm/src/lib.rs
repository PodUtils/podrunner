//! The podvm unit driver (GitHub #23). podvm is the only unit kind for a
//! host with no root. This crate is the only code that knows the argv of
//! podvm: a flag rename in a beta of podvm breaks this crate and its test,
//! not podrunner at run time.
//!
//! The driver uses the released podvm binary as it is. Its plan, from the
//! measured facts of GitHub #23: one store for the units (`PODVM_STORE`)
//! that loads and extracts each pinned image once from a saved tar, and a
//! container with a tree of its own for each unit (T-545); a tag in the
//! environment of each process of the unit, and a scan of `/proc` for the
//! tree kill, because a grandchild outlives `podvm stop` and `rm -f`;
//! `DOTNET_EnableWriteXorExecute=0` and no `fsize` limit; the rung read
//! from the run banner, not from a request; no `--unsafe-host-paths`.
//! `probe` maps `podvm probe --json` ([`probe`]); [`PodvmUnit`] is the
//! driver.

pub mod probe;
mod unit;

pub use unit::envfile::write_env_file;
pub use unit::{CREDENTIAL_DIR, PodvmUnit};

use std::path::PathBuf;
use std::process::Command;

use podrunner_schema::Capability;
use podrunner_unit::{UnitCode, UnitError, UnitLog};

/// A podvm binary on this host.
#[derive(Debug, Clone)]
pub struct Podvm {
    pub binary: PathBuf,
}

/// podvm removes the tree of a run in a background process after its
/// exit, unless this is `0`. That process races the removal of the unit
/// directory at `destroy` (thousands of `chmod` errors) and outlives the
/// unit. With `0` the removal is part of the run (PodUtils/podvm#391).
pub const SPARE_TREE: (&str, &str) = ("PODVM_SPARE_TREE", "0");

impl Podvm {
    /// A command of this podvm, with no background removal of a tree.
    pub fn command(&self) -> std::process::Command {
        let mut c = std::process::Command::new(&self.binary);
        c.env(SPARE_TREE.0, SPARE_TREE.1);
        c
    }

    /// Run `podvm probe --json` and map it.
    pub fn probe(&self) -> Result<Capability, UnitError> {
        probe::capability(&self.probe_json()?, std::env::consts::ARCH)
    }

    /// The version of this podvm, refused below `lowest` or at
    /// [`probe::BEYOND`] and later: a kind that needs a newer podvm fails
    /// by name at `serve`, not each job at its start (T-110).
    pub fn require(&self, lowest: &str) -> Result<String, UnitError> {
        let v = self.probe_json()?;
        let unsupported = |why: String| UnitError::new(UnitCode::Unsupported, why);
        let version = v["podvm"].as_str().ok_or_else(|| unsupported("the probe names no podvm version".into()))?;
        let have =
            probe::Version::parse(version).ok_or_else(|| unsupported(format!("podvm {version:?} is not a version")))?;
        let low = probe::Version::parse(lowest).ok_or_else(|| unsupported(format!("{lowest:?} is not a version")))?;
        let beyond = probe::Version::parse(probe::BEYOND).expect("BEYOND");
        if have < low || !probe::before(&have, &beyond) {
            return Err(unsupported(format!(
                "podvm {version}: this kind needs {lowest} to {} (exclusive)",
                probe::BEYOND
            )));
        }
        Ok(version.to_string())
    }

    /// The version text that `podvm probe` gives, or `None`.
    pub fn version_text(&self) -> Option<String> {
        Some(self.probe_json().ok()?["podvm"].as_str()?.to_string())
    }

    /// The rung that `podvm probe` gives, or `None`.
    pub fn rung(&self) -> Option<String> {
        Some(self.probe_json().ok()?["rung"].as_str()?.to_string())
    }

    /// The version that `podvm probe` gives, or `None`.
    pub fn version(&self) -> Option<probe::Version> {
        probe::Version::parse(self.probe_json().ok()?["podvm"].as_str()?)
    }

    /// Whether this podvm is `version` or later. The answer of each binary
    /// for each version is kept.
    pub fn at_least(&self, version: &str) -> bool {
        type Seen = std::collections::BTreeMap<(PathBuf, String), bool>;
        static SEEN: std::sync::Mutex<Seen> = std::sync::Mutex::new(std::collections::BTreeMap::new());
        let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
        *seen.entry((self.binary.clone(), version.to_string())).or_insert_with(|| {
            let low = probe::Version::parse(version).expect("a version");
            self.version().is_some_and(|v| v >= low)
        })
    }

    /// `podvm logs FLAGS NAME` as a stream (T-570), in the environment
    /// `envs`. A bounded read adds `--limit-bytes N` with its bound when
    /// this podvm takes it ([`probe::LOWEST_LOG_LIMIT`]): podvm then stops
    /// by itself and holds no more of the log.
    pub fn logs(&self, flags: &[&str], name: &str, envs: Vec<(&'static str, std::ffi::OsString)>) -> UnitLog {
        let limit = self.at_least(probe::LOWEST_LOG_LIMIT);
        let mut cmd = self.command();
        cmd.arg("logs").args(flags).envs(envs);
        let name = name.to_string();
        UnitLog::limited(move |bound| {
            if let (true, Some(n)) = (limit, bound) {
                cmd.arg("--limit-bytes").arg(n.to_string());
            }
            cmd.arg(name);
            cmd
        })
    }

    fn probe_json(&self) -> Result<serde_json::Value, UnitError> {
        let out = output_of(Command::new(&self.binary).args(["probe", "--json"]))
            .map_err(|e| UnitError::new(UnitCode::Host, format!("podvm probe: {}", e.kind())))?;
        if !out.status.success() {
            return Err(UnitError::new(
                UnitCode::Host,
                format!("podvm probe: exit {}", out.status.code().unwrap_or(-1)),
            ));
        }
        serde_json::from_slice(&out.stdout)
            .map_err(|_| UnitError::new(UnitCode::Unsupported, "podvm probe --json gives no JSON"))
    }
}

/// The output of `cmd`. A binary that a process still holds open for a
/// write (`ETXTBSY`: a copy in place, or a fork of another thread in the
/// short time after a write) is tried again a few times.
fn output_of(cmd: &mut Command) -> std::io::Result<std::process::Output> {
    for _ in 0..5 {
        match cmd.output() {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            r => return r,
        }
    }
    cmd.output()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With the released podvm on this host (PODRUNNER_PODVM names it), the
    /// probe maps. The test is ignored by default: CI has no podvm.
    #[test]
    #[ignore]
    fn the_probe_of_a_real_podvm_maps() {
        let binary = PathBuf::from(std::env::var_os("PODRUNNER_PODVM").expect("PODRUNNER_PODVM"));
        let c = Podvm { binary }.probe().unwrap();
        assert_eq!(c.kind, "podvm");
    }

    #[test]
    fn each_command_of_podvm_removes_its_tree_in_the_run() {
        let c = Podvm { binary: "podvm".into() }.command();
        let envs: Vec<_> = c.get_envs().collect();
        assert!(
            envs.contains(&(std::ffi::OsStr::new("PODVM_SPARE_TREE"), Some(std::ffi::OsStr::new("0")))),
            "{envs:?}"
        );
    }

    /// A fake podvm whose probe names `version`.
    fn fake(version: &str) -> (Podvm, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let d = std::env::temp_dir().join(format!("podrunner-podvm-require-{version}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("podvm");
        std::fs::write(&f, format!("#!/bin/sh\necho '{{\"podvm\":\"{version}\",\"rung\":\"userland\"}}'\n")).unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        (Podvm { binary: f }, d)
    }

    #[test]
    fn a_kind_refuses_a_podvm_below_its_lowest_version() {
        let _one = crate::unit::FAKE_PODVM.lock().unwrap_or_else(|e| e.into_inner());
        for (have, ok) in [("0.2.0-beta.9", false), ("0.2.0-beta.10", true), ("0.2.0", true), ("0.3.0-beta.1", false)] {
            let (p, d) = fake(have);
            let got = p.require("0.2.0-beta.10");
            let _ = std::fs::remove_dir_all(&d);
            assert_eq!(got.is_ok(), ok, "{have}: {got:?}");
            if let Err(e) = got {
                assert_eq!(e.code, UnitCode::Unsupported);
                assert!(e.to_string().contains(have) && e.to_string().contains("0.2.0-beta.10"), "{e}");
            }
        }
    }

    /// T-570: from podvm `0.2.0-beta.37` a bounded read of the log passes
    /// its bound (plus one byte) as `--limit-bytes`, before the name; an
    /// older podvm, and a read with no bound, get no flag.
    #[test]
    fn a_bounded_read_of_the_log_passes_its_bound_to_podvm() {
        use std::os::unix::fs::PermissionsExt;
        let _one = crate::unit::FAKE_PODVM.lock().unwrap_or_else(|e| e.into_inner());
        for (version, flag) in [("0.2.0-beta.37", true), ("0.2.0-beta.36", false)] {
            let d = std::env::temp_dir().join(format!("podrunner-podvm-logs-{version}-{}", std::process::id()));
            std::fs::create_dir_all(&d).unwrap();
            let f = d.join("podvm");
            let script = format!(
                "#!/bin/sh\ncase $1 in probe) echo '{{\"podvm\":\"{version}\",\"rung\":\"userland\"}}' ;;\n\
                 logs) echo \"$*\" >> \"$ARGS\"; n=3000; [ \"$3\" = --limit-bytes ] && n=$4; yes x | head -c $n ;; esac\n"
            );
            std::fs::write(&f, script).unwrap();
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
            let p = Podvm { binary: f };
            let args = d.join("args");
            let envs = || vec![("ARGS", args.clone().into_os_string())];
            let read = p.logs(&["--podvm-tier=incus"], "u-1", envs()).read_bounded(
                1000,
                std::time::Duration::from_secs(5),
                &mut |_| {},
            );
            assert_eq!(read.unwrap().cut, Some(podrunner_unit::Cut::Bytes), "{version}");
            assert_eq!(p.logs(&[], "u-1", envs()).read_all().unwrap().len(), 3000);
            let seen = std::fs::read_to_string(&args).unwrap();
            let _ = std::fs::remove_dir_all(&d);
            let want = if flag {
                "logs --podvm-tier=incus --limit-bytes 1001 u-1\nlogs u-1\n"
            } else {
                "logs --podvm-tier=incus u-1\nlogs u-1\n"
            };
            assert_eq!(seen, want, "{version}");
        }
    }

    #[test]
    fn a_missing_binary_is_a_host_error() {
        let e = Podvm { binary: "/nonexistent/podvm".into() }.probe().unwrap_err();
        assert_eq!(e.code, UnitCode::Host);
    }
}
