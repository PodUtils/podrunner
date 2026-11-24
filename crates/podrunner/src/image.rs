//! The unit image of the official runner (GitHub #30). The version and the
//! digest of each release tarball are in `images/runner/runner.toml`, a
//! reviewed file that this binary holds; `podrunner image verify` checks a
//! tarball against it before a build or an import.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use serde::Deserialize;

/// The pin file of the runner, as built in.
pub const RUNNER_TOML: &str = include_str!("../../../images/runner/runner.toml");

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerPin {
    pub version: String,
    /// The SHA-256 of the tarball of each architecture, in hex.
    pub sha256: BTreeMap<String, String>,
}

/// The built-in pin of the runner.
pub fn runner_pin() -> RunnerPin {
    toml::from_str(RUNNER_TOML).expect("images/runner/runner.toml is a pin file")
}

/// The SHA-256 of a file, in hex.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        ctx.update(&buf[..n]);
    }
    Ok(ctx.finish().as_ref().iter().map(|b| format!("{b:02x}")).collect())
}

/// Check the tarball of `arch` against the pin: the version line on
/// success, the reason on a refusal.
pub fn verify(tarball: &Path, arch: &str) -> Result<String, String> {
    let pin = runner_pin();
    let arch = arch_of(arch)?;
    let want = pin.sha256.get(arch).ok_or_else(|| format!("no pin for the architecture {arch:?}"))?;
    let have = sha256_file(tarball).map_err(|e| format!("the tarball: {}", e))?;
    if &have != want {
        // The file that the pin names, and the way to check an image of a
        // release (T-344).
        let name = runner_file(arch, &pin.version);
        let image = match is_image(tarball) {
            true => {
                "; this file is a runner image of a release (runner-ARCH.tar), not the tarball of actions/runner: check it with `sha256sum -c runner-ARCH.tar.sha256`, and give `podrunner image verify` only actions-runner-linux-*.tar.gz"
            }
            false => "",
        };
        return Err(format!(
            "the tarball is not the runner {} of {arch} ({name}, SHA-256 {want}): its SHA-256 is {have}{image}",
            pin.version
        ));
    }
    Ok(format!("actions/runner {} {arch}: the SHA-256 matches the pin\n", pin.version))
}

/// The architecture of the pin for a name of `--arch`: actions/runner
/// names its files `x64` and `arm64`, the kernel `x86_64` and `aarch64`
/// (T-628).
pub fn arch_of(name: &str) -> Result<&'static str, String> {
    match name {
        "x64" | "x86_64" => Ok("x86_64"),
        "arm64" | "aarch64" => Ok("aarch64"),
        _ => Err(format!("no pin for the architecture {name:?}: use x64 or arm64 (or x86_64, aarch64)")),
    }
}

/// The file name of the official runner tarball that the pin names.
pub fn runner_file(arch: &str, version: &str) -> String {
    let a = match arch {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => other,
    };
    format!("actions-runner-linux-{a}-{version}.tar.gz")
}

/// Whether `path` is a runner image of a release: its name is
/// `runner-*.tar`, or it is a tar with an entry of an OCI layout
/// (`oci-layout`, `index.json`, `blobs/sha256/`) or of `docker save`
/// (`manifest.json`). The release sorts its entries, so the blobs come
/// first: the walk seeks over the data of each entry.
fn is_image(path: &Path) -> bool {
    let named =
        path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("runner-") && n.ends_with(".tar"));
    named || std::fs::File::open(path).is_ok_and(|f| tar_of_image(f).unwrap_or(false))
}

fn tar_of_image(mut f: std::fs::File) -> std::io::Result<bool> {
    use std::io::{Seek, SeekFrom};
    let mut h = [0u8; 512];
    for _ in 0..4096 {
        f.read_exact(&mut h)?;
        if &h[257..262] != b"ustar" {
            return Ok(false);
        }
        let end = h[..100].iter().position(|b| *b == 0).unwrap_or(100);
        let name = String::from_utf8_lossy(&h[..end]);
        let name = name.trim_start_matches("./");
        if matches!(name, "oci-layout" | "index.json" | "manifest.json") || name.starts_with("blobs/sha256/") {
            return Ok(true);
        }
        let size = std::str::from_utf8(&h[124..136]).ok().map(|s| s.trim_matches(|c: char| c == '\0' || c == ' '));
        let size = size.and_then(|s| u64::from_str_radix(s, 8).ok()).unwrap_or(0);
        f.seek(SeekFrom::Current(size.div_ceil(512) as i64 * 512))?;
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pin_is_the_runner_of_pod_services() {
        let pin = runner_pin();
        let service = pod_services::get("github-actions-runner").expect("the service of the runner pin");
        assert_eq!(service.text("runner-tag"), Some(format!("v{}", pin.version).as_str()));
        assert_eq!(pin.sha256.keys().collect::<Vec<_>>(), ["aarch64", "x86_64"]);
        assert!(pin.sha256.values().all(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit())));
    }

    #[test]
    fn a_tarball_with_another_digest_is_refused() {
        let f = std::env::temp_dir().join(format!("podrunner-image-{}", std::process::id()));
        std::fs::write(&f, b"abc").unwrap();
        assert_eq!(sha256_file(&f).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert!(verify(&f, "x86_64").unwrap_err().contains("its SHA-256 is ba7816bf"));
        assert!(verify(&f, "riscv64").unwrap_err().contains("no pin"));
        // T-344: the error names the file of the pin, and an OCI image.
        let e = verify(&f, "x86_64").unwrap_err();
        assert!(e.contains(&runner_file("x86_64", &runner_pin().version)) && !e.contains("runner image"), "{e}");
        let mut tar = vec![0u8; 1024];
        tar[..11].copy_from_slice(b"index.json\0");
        tar[257..262].copy_from_slice(b"ustar");
        std::fs::write(&f, &tar).unwrap();
        assert!(verify(&f, "x86_64").unwrap_err().contains("this file is a runner image of a release"));
        let _ = std::fs::remove_file(&f);
    }

    /// A tar entry of `name` with `len` bytes of data.
    fn entry(name: &str, len: usize) -> Vec<u8> {
        let mut h = vec![0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[124..135].copy_from_slice(format!("{len:011o}").as_bytes());
        h[257..262].copy_from_slice(b"ustar");
        h.resize(512 + len.div_ceil(512) * 512, 0);
        h
    }

    /// T-628: `--arch` takes the names of actions/runner (`x64`, `arm64`)
    /// and of the kernel (`x86_64`, `aarch64`); another value names them.
    #[test]
    fn each_name_of_an_architecture_reaches_the_check() {
        let dir = crate::scratch::Scratch::new("t628-arch");
        let f = dir.join("t.tar.gz");
        std::fs::write(&f, b"abc").unwrap();
        for (arch, pinned) in [("x64", "x86_64"), ("x86_64", "x86_64"), ("arm64", "aarch64"), ("aarch64", "aarch64")] {
            let e = verify(&f, arch).unwrap_err();
            assert!(
                e.contains("its SHA-256 is ba7816bf") && e.contains(&runner_file(pinned, &runner_pin().version)),
                "{arch}: {e}"
            );
        }
        for bad in ["riscv64", "X64", "", "amd64 "] {
            let e = verify(&f, bad).unwrap_err();
            assert!(e.contains("use x64 or arm64 (or x86_64, aarch64)"), "{bad:?}: {e}");
        }
    }

    /// T-628: the runner image of a release (`runner-ARCH.tar`: its blobs
    /// first, `index.json` after them) is named as an image, by its name
    /// and by its entries; a gzip file is not.
    #[test]
    fn the_runner_image_of_a_release_is_named() {
        let dir = crate::scratch::Scratch::new("t628-image");
        let mut tar = entry("blobs/sha256/aa", 3 << 20);
        tar.extend(entry("index.json", 10));
        tar.extend(entry("oci-layout", 10));
        tar.extend([0u8; 1024]);
        for name in ["runner-x86_64.tar", "plain.tar"] {
            std::fs::write(dir.join(name), &tar).unwrap();
            let e = verify(&dir.join(name), "x64").unwrap_err();
            assert!(e.contains("this file is a runner image"), "{name}: {e}");
        }
        let mut docker = entry("./manifest.json", 10);
        docker.extend([0u8; 1024]);
        std::fs::write(dir.join("d.tar"), &docker).unwrap();
        assert!(verify(&dir.join("d.tar"), "x64").unwrap_err().contains("this file is a runner image"));
        std::fs::write(dir.join("runner.tgz"), [0x1f, 0x8b, 8, 0]).unwrap();
        assert!(!verify(&dir.join("runner.tgz"), "x64").unwrap_err().contains("runner image"));
    }
}
