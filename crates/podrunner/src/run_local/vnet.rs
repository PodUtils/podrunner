//! `--vnet PATH` (T-220): the vnet shim of podnet for the runner of a
//! podvm unit. PATH is the shim (`libpodnet-vnet.so`), or a podnet binary
//! that builds it with `podnet net shim` (a C compiler of the host). The
//! shim loads in the runner image, so it must be built for its C library
//! (glibc).

use std::path::{Path, PathBuf};

/// The file name of the shim.
const SHIM: &str = "libpodnet-vnet.so";

/// The shim of `path`: the library itself, or the one that podnet builds
/// into `dir`.
pub(super) fn shim(path: &Path, dir: &Path) -> Result<PathBuf, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!("{}: not a file; give the vnet shim ({SHIM}) or a podnet binary", path.display()));
    }
    if path.extension().is_some_and(|x| x == "so") {
        // A shared object starts with the magic of ELF (#264).
        let mut magic = [0u8; 4];
        let elf = std::fs::File::open(path).and_then(|mut f| std::io::Read::read_exact(&mut f, &mut magic)).is_ok()
            && magic == *b"\x7fELF";
        if !elf {
            return Err(format!(
                "{}: not a shared object (no ELF header); give the vnet shim of podnet",
                path.display()
            ));
        }
        return Ok(path.to_path_buf());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let out = std::process::Command::new(path)
        .args(["net", "shim", "--json", "--out"])
        .arg(dir)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "{} net shim: {}; the shim needs a C compiler (CC, cc, gcc or clang), or give the built {SHIM}",
            path.display(),
            why.trim()
        ));
    }
    found(&String::from_utf8_lossy(&out.stdout), dir)
        .ok_or_else(|| format!("{} net shim gave no {SHIM}", path.display()))
}

/// The path of the shim in the output of `podnet net shim --json`, else
/// in `dir`.
fn found(stdout: &str, dir: &Path) -> Option<PathBuf> {
    let named = stdout
        .split(|c: char| c == '"' || c == ':' || c == ' ' || c == '=' || c.is_whitespace())
        .find(|t| t.starts_with('/') && t.ends_with(SHIM))
        .map(PathBuf::from)
        .filter(|p| p.is_file());
    named.or_else(|| Some(dir.join(SHIM)).filter(|p| p.is_file()))
}

#[cfg(test)]
mod tests {
    /// T-220: the shim is the library, or the one that the output of podnet
    /// names; a directory is refused.
    #[test]
    fn the_shim_of_the_vnet() {
        let dir = std::env::temp_dir().join(format!("podrunner-vnet-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let lib = dir.join(super::SHIM);
        std::fs::write(&lib, b"\x7fELF\x02\x01\x01").unwrap();
        assert_eq!(super::shim(&lib, &dir.join("out")).unwrap(), lib);
        let json = format!("{{\"LD_PRELOAD\":\"{}:/other.so\"}}", lib.display());
        assert_eq!(super::found(&json, &dir.join("none")), Some(lib.clone()));
        assert_eq!(super::found("{}", &dir), Some(lib), "the file in the directory");
        assert!(super::shim(&dir, &dir).unwrap_err().contains("not a file"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
