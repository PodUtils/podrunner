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
    let want = pin.sha256.get(arch).ok_or_else(|| format!("no pin for the architecture {arch:?}"))?;
    let have = sha256_file(tarball).map_err(|e| format!("the tarball: {}", e.kind()))?;
    if &have != want {
        return Err(format!(
            "the tarball is not the runner {} of {arch}: its SHA-256 differs from the pin",
            pin.version
        ));
    }
    Ok(format!("actions/runner {} {arch}: the SHA-256 matches the pin\n", pin.version))
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
        assert!(verify(&f, "x86_64").unwrap_err().contains("differs"));
        assert!(verify(&f, "riscv64").unwrap_err().contains("no pin"));
        let _ = std::fs::remove_file(&f);
    }
}
