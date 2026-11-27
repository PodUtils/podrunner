//! The checks of the paths that a run uses, before its first unit, also
//! in a dry run (T-345, T-356): a `planned` line is a promise that the run
//! can keep. A dry run writes no directory of the run.

use super::Args;

/// The first path of the run that the run could not use, by its flag.
pub(super) fn paths(a: &Args) -> Result<(), String> {
    // A unit writes the tar of its image: a smaller limit of file size ends
    // it by SIGXFSZ, so refuse before the first unit, as doctor does
    // (T-632). The host runner runs with W^X off (T-144): only a warning.
    match file_size_limit(a) {
        Err(e) => return Err(format!("the limit of file size (ulimit -f) is {e}")),
        Ok(w) if a.podvm.is_none() && a.guest.is_none() && fsize().0.is_some() => {
            eprintln!("podrunner: warning: the limit of file size (ulimit -f) is {w}");
        }
        Ok(_) => {}
    }
    if let Some(dir) = &a.runner_dir {
        if !dir.join("run.sh").is_file() || !dir.join("bin/Runner.Listener").is_file() {
            return Err(format!("--runner-dir {}: no unpacked runner (run.sh, bin/Runner.Listener)", dir.display()));
        }
        if let Some(e) = podrunner_local::exec::space_refusal(dir) {
            return Err(format!("--runner-dir {e}"));
        }
        // The run takes the directory with a lock file in it (#244).
        super::doctor::writable(dir)
            .map_err(|e| format!("--runner-dir {e}; the run writes its lock file .podrunner-run.lock there"))?;
    }
    let binaries = [
        a.guest.as_ref().map(|(microvm, b)| (if *microvm { "--microvm" } else { "--incus" }, b)),
        a.podnet.as_ref().map(|b| ("--podnet", b)),
        a.vnet.as_ref().map(|b| ("--vnet", b)),
    ];
    for (flag, b) in binaries.into_iter().flatten() {
        if !b.is_file() {
            return Err(format!("{flag} {}: not a file", b.display()));
        }
    }
    // A dry run builds the shim of `--vnet` too (#264), into a private
    // scratch directory that it removes; a real run builds it in its routes.
    if let (true, Some(v)) = (a.dry_run, &a.vnet) {
        let scratch = std::env::temp_dir().join(format!("podrunner-dry-vnet-{}", std::process::id()));
        let built = podrunner_local::work::private_dir(&scratch).and_then(|()| super::vnet::shim(v, &scratch));
        let _ = std::fs::remove_dir_all(&scratch);
        built.map_err(|e| format!("--vnet {e}"))?;
    }
    if let Some(t) = a.toolcache.as_ref().filter(|t| !t.is_dir()) {
        return Err(format!("--toolcache {}: not a directory; fix: mkdir -p {0}", t.display()));
    }
    // With `--offline` the store of actions is read only (#242).
    if let Some(d) = a.actions.as_ref().filter(|_| a.offline)
        && !d.is_dir()
    {
        return Err(format!("--actions {}: not a directory; --offline reads the actions from it", d.display()));
    }
    mounted(a)?;
    sockets(a)?;
    let dirs = [
        ("--actions", a.actions.as_ref().filter(|_| !a.offline)),
        ("--artifacts", a.artifacts.as_ref()),
        ("--cache", a.cache.as_ref().filter(|_| !a.no_cache)),
        ("--log-dir", a.log_dir.as_ref()),
        ("--export", a.export.as_ref()),
        // The run writes its state at the end: a dry run checks it too (T-634).
        ("--state", a.state.as_ref()),
    ];
    for (flag, d) in dirs {
        if let Some(d) = d {
            super::doctor::writable(d).map_err(|e| format!("{flag} {e}"))?;
        }
    }
    Ok(())
}

/// The host paths that a unit kind mounts with a `-v` of podvm, which
/// takes no ':' (T-476): the scratch of the run in TMPDIR, `--toolcache`,
/// `--export` and `--engine`. A run names one before its first job.
pub(super) fn mounted(a: &Args) -> Result<(), String> {
    if a.podvm.is_none() && a.guest.is_none() {
        return Ok(());
    }
    let tmp = std::env::temp_dir();
    let paths = [
        ("TMPDIR", Some(&tmp)),
        ("--toolcache", a.toolcache.as_ref()),
        ("--export", a.export.as_ref()),
        ("--engine", a.engine.as_ref()),
    ];
    for (what, p) in paths {
        if let Some(p) = p {
            podrunner_unit::mounts::host_path(p, podrunner_unit::UnitCode::UnitStartFailed)
                .map_err(|e| format!("{what}: {}", e.detail))?;
        }
    }
    Ok(())
}

/// The sockets of a unit in TMPDIR (T-738): a hold or a guest puts them
/// there, and a driver, podnet, the vnet shim or a guest reaches each one
/// by its path. A unit up to `u-999` is checked here, before the first
/// unit; each later one at its start.
pub(super) fn sockets(a: &Args) -> Result<(), String> {
    let fronted = a.vnet.is_some() || a.guest.is_some();
    let egress = a.podnet.is_some() || !a.allow.is_empty() || !a.environment_allow.is_empty();
    podrunner_local::runkey::sockets_fit(999, fronted, egress)
}

#[cfg(test)]
mod tests {
    /// T-476: a host path with a ':' that a unit kind mounts fails the
    /// checks of the run; with `--runner-dir` it mounts nothing.
    #[test]
    fn a_mounted_path_with_a_colon_is_named() {
        let a =
            super::Args { podvm: Some("/bin/true".into()), export: Some("/tmp/a:b".into()), ..super::Args::default() };
        let e = super::mounted(&a).unwrap_err();
        assert!(e.starts_with("--export: the host path /tmp/a:b has a ':'"), "{e}");
        let host =
            super::Args { runner_dir: Some("/r".into()), export: Some("/tmp/a:b".into()), ..super::Args::default() };
        assert!(super::mounted(&host).is_ok());
    }

    /// #264: a dry run names a `--vnet` that builds no shim (a program that
    /// is not podnet, a `.so` that is not a library), not `planned`.
    #[test]
    fn a_vnet_that_builds_no_shim_fails_the_dry_run() {
        let img = format!("r@sha256:{}", "0".repeat(64));
        let dir = std::env::temp_dir().join(format!("podrunner-dry-vnet-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let not_elf = dir.join("libpodnet-vnet.so");
        std::fs::write(&not_elf, "not a library").unwrap();
        for vnet in ["/bin/false".to_string(), not_elf.display().to_string()] {
            let a = super::super::parse(
                &["w.yml", "--podvm", "/bin/true", "--images", "/", "--image", &img, "--vnet", &vnet, "--dry-run"]
                    .map(String::from),
            )
            .unwrap();
            let e = super::paths(&a).unwrap_err();
            assert!(e.starts_with("--vnet "), "{vnet}: {e}");
            assert!(!vnet.ends_with(".so") || e.contains("no ELF header"), "{vnet}: {e}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The soft and the hard limit of file size; None is no limit.
fn fsize() -> (Option<u64>, Option<u64>) {
    let mut r = libc::rlimit { rlim_cur: libc::RLIM_INFINITY, rlim_max: libc::RLIM_INFINITY };
    // SAFETY: getrlimit writes into the struct that this function owns.
    unsafe { libc::getrlimit(libc::RLIMIT_FSIZE, &mut r) };
    let some = |v: libc::rlim_t| (v != libc::RLIM_INFINITY).then_some(v);
    (some(r.rlim_cur), some(r.rlim_max))
}

/// The bytes of `image`: its tar in the image cache of the user, else the
/// size that podvm gives for it in the store of `--images`.
pub(super) fn image_size(a: &Args, image: &str) -> Option<u64> {
    let digest = image.rsplit_once('@').map(|(_, d)| d).filter(|d| d.starts_with("sha256:"));
    let tar =
        digest.and_then(|d| Some(super::actions::user_cache("image-tars").ok()?.join(podrunner_podvm::tar_name(d))));
    if let Some(len) = tar.and_then(|t| std::fs::metadata(t).ok()).filter(|m| m.is_file()).map(|m| m.len()) {
        return Some(len);
    }
    let podvm = a.podvm.as_ref().or(a.guest.as_ref().map(|(_, b)| b))?;
    let out = std::process::Command::new(podvm)
        .args(["inspect", "--format", "{{.Size}}", image])
        .env("PODVM_STORE", a.images.as_ref()?)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    String::from_utf8(out.stdout).ok()?.trim().parse::<u64>().ok().filter(|s| *s > 0)
}

/// The largest file that a unit of the route of `a` writes on the host:
/// the tar of its image (the save of the cache, the load) and a margin,
/// with the image. None: the host route, whose runner writes no such file.
fn fsize_need(a: &Args) -> Option<(u64, String)> {
    if a.podvm.is_none() && a.guest.is_none() {
        return None;
    }
    let images = a.image.iter().chain(a.platforms.iter().map(|(_, t)| t).filter(|t| *t != "host"));
    // An image of an unknown size counts as 1 GiB, about three times the
    // base image.
    let (size, image) = images.map(|i| (image_size(a, i).unwrap_or(1 << 30), i.clone())).max()?;
    // The microvm tier writes an initrd of the unpacked image: about 1.4
    // GiB for a base image of 300 MB.
    let need = match a.guest {
        Some((true, _)) => size * 5,
        _ => size + size / 4,
    };
    Some((need + (64 << 20), format!("the image {image} is {size} bytes")))
}

/// The limit of file size of this process against the route of `a`
/// (T-144, T-632).
pub(super) fn file_size_limit(a: &Args) -> Result<String, String> {
    let (soft, hard) = fsize();
    fsize_verdict(soft, hard, fsize_need(a))
}

fn fsize_verdict(soft: Option<u64>, hard: Option<u64>, need: Option<(u64, String)>) -> Result<String, String> {
    let Some(soft) = soft else { return Ok("no limit".into()) };
    let Some((need, image)) = need else {
        return Ok(format!(
            "{soft} bytes: the runner of --runner-dir runs with W^X off, but a step that writes a larger file ends by SIGXFSZ"
        ));
    };
    if soft >= need {
        return Ok(format!("{soft} bytes; a unit writes files up to about {need} bytes ({image})"));
    }
    let why = format!(
        "{soft} bytes: a unit writes files of its image (its tar, an initrd; {image}) up to about {need} bytes, and ends by SIGXFSZ at a larger one"
    );
    match hard {
        Some(hard) if hard < need => Err(format!(
            "{why}; the hard limit is {hard} bytes, and only root can raise it: ask the administrator, or run the jobs on the host with --runner-dir DIR (an unpacked actions/runner), which writes no such file"
        )),
        Some(_) => Err(format!("{why}; fix: ulimit -f $(ulimit -H -f)")),
        None => Err(format!("{why}; fix: ulimit -f unlimited")),
    }
}

#[cfg(test)]
mod fsize_tests {
    use super::fsize_verdict;

    /// T-632: the verdict of each limit against each route.
    #[test]
    fn the_verdict_of_a_limit_of_file_size() {
        let need = || Some((1000, "the image I is 700 bytes".to_string()));
        assert_eq!(fsize_verdict(None, None, need()).unwrap(), "no limit");
        assert!(fsize_verdict(Some(10), Some(10), None).unwrap().contains("W^X off"));
        assert!(fsize_verdict(Some(1000), None, need()).is_ok());
        let e = fsize_verdict(Some(999), None, need()).unwrap_err();
        assert!(e.contains("the image I is 700 bytes") && e.ends_with("fix: ulimit -f unlimited"), "{e}");
        let e = fsize_verdict(Some(999), Some(1000), need()).unwrap_err();
        assert!(e.ends_with("fix: ulimit -f $(ulimit -H -f)"), "{e}");
        let e = fsize_verdict(Some(999), Some(999), need()).unwrap_err();
        assert!(
            e.contains("the hard limit is 999 bytes, and only root can raise it") && e.contains("--runner-dir"),
            "{e}"
        );
    }
}
