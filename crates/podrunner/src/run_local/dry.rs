//! The checks of the paths that a run uses, before its first unit, also
//! in a dry run (T-345, T-356): a `planned` line is a promise that the run
//! can keep. A dry run writes no directory of the run.

use super::Args;

/// The first path of the run that the run could not use, by its flag.
pub(super) fn paths(a: &Args) -> Result<(), String> {
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
    let dirs = [
        ("--actions", a.actions.as_ref().filter(|_| !a.offline)),
        ("--artifacts", a.artifacts.as_ref()),
        ("--cache", a.cache.as_ref().filter(|_| !a.no_cache)),
        ("--log-dir", a.log_dir.as_ref()),
        ("--export", a.export.as_ref()),
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
