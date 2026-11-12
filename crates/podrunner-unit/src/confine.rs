//! The `confined` boundary (T-042): a unit whose processes run under
//! Landlock and a seccomp deny filter with `no_new_privs`, from
//! `pod-confine`. The payload reads and executes the system directories,
//! reads and writes its own root, and reaches no other path: not the root
//! of another unit, not the state of podrunner, not the credential of
//! another job. Landlock also keeps a process from reading the memory, the
//! argv and the environment of a process outside its domain.
//!
//! The rules compile before the fork; the shim applies them in the payload
//! between the fork and the exec ([`crate::shim::under_shim_confined`]).

use std::path::Path;
use std::sync::Arc;

pub use pod_confine::Compiled;
use pod_confine::{Access, Spec};

/// The directories of the system that a payload reads and executes, when
/// they exist.
const SYSTEM: [&str; 7] = ["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc", "/opt"];

/// The devices that a payload reads and writes.
const DEVICES_RW: [&str; 2] = ["/dev/null", "/dev/tty"];

/// The devices that a payload reads.
const DEVICES_RO: [&str; 3] = ["/dev/zero", "/dev/urandom", "/dev/random"];

/// The Landlock ABI where a confinement loads on this host, or why not.
pub fn check() -> Result<u32, String> {
    pod_confine::check().map_err(|f| f.to_string())
}

/// The proof of the boundary for the capability record.
pub fn proof(abi: u32) -> String {
    format!("pod-confine: landlock abi {abi}, seccomp deny filter, no_new_privs")
}

/// The rules of a unit whose root is `root`, compiled for the ABI `abi`.
/// `root` must exist. `sockets`: the socket directory of the unit (its
/// egress and control sockets). From Landlock ABI 9 a connect to a Unix
/// socket that was made outside the domain needs a write rule on its path.
pub fn for_unit(root: &Path, sockets: Option<&Path>, abi: u32) -> Result<Arc<Compiled>, String> {
    let mut spec = Spec::new();
    for dir in SYSTEM.iter().filter(|d| Path::new(d).exists()) {
        spec = spec.allow(*dir, Access::READ | Access::EXECUTE);
    }
    // The processes of the unit; Landlock keeps the others out of reach.
    spec = spec.allow("/proc", Access::READ);
    for dev in DEVICES_RW.iter().filter(|d| Path::new(d).exists()) {
        spec = spec.allow(*dev, Access::READ | Access::WRITE);
    }
    for dev in DEVICES_RO.iter().filter(|d| Path::new(d).exists()) {
        spec = spec.allow(*dev, Access::READ);
    }
    let root = root.to_str().ok_or("the root of a unit is not UTF-8")?;
    spec = spec.allow(root, Access::READ | Access::WRITE | Access::EXECUTE);
    if let Some(dir) = sockets {
        let dir = dir.to_str().ok_or("the socket directory of a unit is not UTF-8")?;
        spec = spec.allow(dir, Access::READ | Access::WRITE);
    }
    let mut compiled = spec.compile(abi).map_err(|f| f.to_string())?;
    // The child duplicates its standard streams onto 0, 1 and 2.
    compiled.keep_clear_of(&[0, 1, 2]).map_err(|f| f.to_string())?;
    Ok(Arc::new(compiled))
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;
    use crate::shim::{Mode, under_shim_confined};

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("podrunner-confined-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn run(root: &Path, script: &str) -> (Option<i32>, String) {
        run_with(root, None, script)
    }

    fn run_with(root: &Path, sockets: Option<&Path>, script: &str) -> (Option<i32>, String) {
        let abi = check().expect("Landlock on this host");
        let mut c = Command::new("sh");
        c.args(["-c", script]).current_dir(root);
        under_shim_confined(&mut c, Mode::EndWithPayload, Some(for_unit(root, sockets, abi).unwrap()));
        let out = c.output().unwrap();
        (out.status.code(), String::from_utf8_lossy(&out.stdout).into_owned())
    }

    #[test]
    fn a_confined_payload_writes_its_root_and_reads_no_other_path() {
        if check().is_err() {
            return; // No Landlock on this host: `podrunner doctor` names it.
        }
        let (mine, other) = (dir("mine"), dir("other"));
        std::fs::write(other.join("secret"), "podrunner-canary-of-another-unit").unwrap();
        // A positive control: the payload writes and reads its own root.
        let (code, out) = run(&mine, "echo own > own && cat own");
        assert_eq!((code, out.trim()), (Some(0), "own"));
        let (code, out) = run(&mine, &format!("cat {}/secret", other.display()));
        assert_ne!(code, Some(0));
        assert!(!out.contains("canary"), "{out}");
        // No write outside the root, also in /tmp.
        let (code, _) = run(&mine, &format!("echo x > {}/written", other.display()));
        assert_ne!(code, Some(0));
        assert!(!other.join("written").exists());
        // The environment of a process outside the domain: this test. Also
        // as root: `apply` drops CAP_SYS_ADMIN and CAP_PERFMON, which pass
        // Landlock for an ancestor.
        let (_, out) = run(&mine, &format!("cat /proc/{}/environ", std::process::id()));
        assert!(out.is_empty(), "the environment of a process outside the domain is readable");
        let _ = std::fs::remove_dir_all(&mine);
        let _ = std::fs::remove_dir_all(&other);
    }

    /// The socket directory of a unit is outside its root: a confined
    /// payload connects to a socket there (Landlock ABI 9 needs the write
    /// rule of `for_unit`).
    #[test]
    fn a_confined_payload_reaches_the_sockets_of_its_unit() {
        if check().is_err() || !Path::new("/usr/bin/python3").exists() {
            return; // No Landlock or no python3 for the connect.
        }
        let (mine, sockets) = (dir("sock-root"), dir("sock-dir"));
        let path = sockets.join("egress.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let accept = std::thread::spawn(move || listener.accept().is_ok());
        let connect = format!(
            "/usr/bin/python3 -I -c 'import socket; s=socket.socket(socket.AF_UNIX); s.connect(\"{}\"); print(\"connected\")'",
            path.display()
        );
        let (code, out) = run_with(&mine, Some(&sockets), &connect);
        assert_eq!((code, out.trim()), (Some(0), "connected"));
        assert!(accept.join().unwrap());
        let _ = std::fs::remove_dir_all(&mine);
        let _ = std::fs::remove_dir_all(&sockets);
    }
}
