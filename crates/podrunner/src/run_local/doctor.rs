//! `podrunner doctor --run FILE [the options of run]` (T-153): the checks of
//! the inputs of a local run before it starts. Each check that fails names
//! its fix; nothing changes.

use std::path::Path;

use podrunner_podvm::Podvm;
use podrunner_podvm::probe::{LOWEST, LOWEST_ENGINE, LOWEST_GUEST, LOWEST_INCUS};

use super::Args;

/// One check: its name, whether it holds, and the detail or the fix.
pub(super) struct Check {
    pub name: &'static str,
    pub ok: bool,
    pub text: String,
}

fn check(name: &'static str, r: Result<String, String>) -> Check {
    match r {
        Ok(text) => Check { name, ok: true, text },
        Err(text) => Check { name, ok: false, text },
    }
}

/// The limit of file size: .NET maps its code through a large memfd, so a
/// small limit ends the runner by SIGXFSZ (T-144).
fn file_size_limit() -> Result<String, String> {
    let mut r = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    // SAFETY: getrlimit writes into the struct that this function owns.
    if unsafe { libc::getrlimit(libc::RLIMIT_FSIZE, &mut r) } != 0 {
        return Err("getrlimit failed".into());
    }
    if r.rlim_cur == libc::RLIM_INFINITY || r.rlim_cur >= 4 << 30 {
        return Ok("no small limit".into());
    }
    Err(format!("{} bytes: the runner dies by SIGXFSZ; fix: ulimit -f unlimited", r.rlim_cur))
}

/// The free bytes of the file system of `dir`.
fn free_disk(dir: &Path) -> Result<String, String> {
    let c = std::ffi::CString::new(dir.as_os_str().as_encoded_bytes()).map_err(|e| e.to_string())?;
    // SAFETY: statvfs writes into a zeroed struct that this function owns.
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return Err(format!("{}: statvfs failed", dir.display()));
    }
    let free = s.f_bavail as u64 * s.f_frsize as u64;
    let gib = free >> 30;
    if gib < 2 {
        return Err(format!(
            "{}: {gib} GiB free; a run needs about 2 GiB; fix: free the disk or set TMPDIR",
            dir.display()
        ));
    }
    Ok(format!("{}: {gib} GiB free", dir.display()))
}

/// The checks of a run with the options `a`.
pub(super) fn checks(a: &Args) -> Vec<Check> {
    let mut out =
        vec![check("file-size-limit", file_size_limit()), check("scratch-disk", free_disk(&std::env::temp_dir()))];
    out.push(check(
        "workflow",
        std::fs::read_to_string(&a.file)
            .map_err(|e| format!("{}: {e}", a.file.display()))
            .and_then(|t| pod_actions_workflow::parse(&t).map(|_| "it parses".to_string()).map_err(|e| e.to_string())),
    ));
    let lock = a.file.parent().unwrap_or(Path::new(".")).join(podrunner_local::lock::LOCK_FILE);
    out.push(check(
        "lock",
        if lock.exists() {
            podrunner_local::lock::Lock::read(&lock)
                .map(|l| format!("{}: {} references", lock.display(), l.pins.len() + l.images.len()))
                .map_err(|e| e.to_string())
        } else {
            Ok(format!("{}: none yet; a run with no --frozen writes it", lock.display()))
        },
    ));
    if let Some(dir) = &a.runner_dir {
        let ok = dir.join("run.sh").is_file() && dir.join("bin/Runner.Listener").is_file();
        out.push(check(
            "runner-dir",
            if ok {
                Ok(dir.display().to_string())
            } else {
                Err(format!("{}: no unpacked runner; fix: unpack a release of actions/runner there", dir.display()))
            },
        ));
    }
    let (binary, lowest) = match (&a.podvm, &a.guest) {
        (Some(b), _) => (Some(b.clone()), LOWEST),
        (None, Some((true, b))) => (Some(b.clone()), if a.engine.is_some() { LOWEST_ENGINE } else { LOWEST_GUEST }),
        (None, Some((false, b))) => (Some(b.clone()), LOWEST_INCUS),
        _ => (None, LOWEST),
    };
    if let Some(b) = binary {
        let podvm = Podvm { binary: b.clone() };
        out.push(check(
            "podvm",
            podvm
                .require(lowest)
                .map(|v| format!("{} is {v} ({lowest} or later)", b.display()))
                .map_err(|e| format!("{e}; fix: take podvm {lowest} or later")),
        ));
        if let Some(store) = &a.images {
            out.push(check(
                "store",
                if store.is_dir() {
                    Ok(store.display().to_string())
                } else {
                    Err(format!("{}: not a directory; fix: give the podvm store with --images", store.display()))
                },
            ));
        }
    }
    if matches!(a.guest, Some((true, _))) {
        out.push(check(
            "kvm",
            if Path::new("/dev/kvm").exists() {
                Ok("/dev/kvm".into())
            } else {
                Err("no /dev/kvm: the microvm kind needs KVM; fix: run on a host with KVM".into())
            },
        ));
        if let Some(e) = &a.engine {
            out.push(check(
                "engine",
                if e.join("dockerd").is_file() {
                    Ok(e.display().to_string())
                } else {
                    Err(format!("{}: no dockerd; fix: unpack the static Docker binaries there", e.display()))
                },
            ));
        }
    }
    if let Some(p) = &a.podnet {
        out.push(check(
            "podnet",
            if p.is_file() { Ok(p.display().to_string()) } else { Err(format!("{}: not a file", p.display())) },
        ));
    }
    out
}

/// The text of the checks: one line for each.
pub(super) fn text(checks: &[Check]) -> String {
    checks.iter().map(|c| format!("{} {}: {}\n", if c.ok { "ok  " } else { "FAIL" }, c.name, c.text)).collect()
}
