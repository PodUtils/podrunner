//! The facts of this host that `serve` gives each source in `hello` at its
//! start (T-495, githome's G-17).

use std::path::Path;
use std::process::{Command, Stdio};

use podrunner_slots::Slots;
use podrunner_slots::config::Config;
use podrunner_source::HostFacts;

/// The versions of the tools of the kinds of this host.
#[derive(Debug, Default)]
pub(crate) struct Tools {
    pub podvm: Option<String>,
    pub podnet: Option<String>,
}

/// The version of podvm of the first kind that names a podvm binary, and
/// the version of podnet of the first kind that names a podnet binary.
pub(crate) fn tools(cfg: &Config) -> Tools {
    let podvm = cfg.unit.values().find_map(|u| u.binary.clone());
    let podnet = cfg.unit.values().find_map(|u| u.podnet.clone());
    Tools {
        podvm: podvm.and_then(|b| podrunner_podvm::Podvm { binary: b.into() }.version_text()),
        podnet: podnet.and_then(|b| podnet_version(Path::new(&b))),
    }
}

/// The version that `podnet --version` gives (`podnet X (COMMIT)`), or
/// `None`.
fn podnet_version(binary: &Path) -> Option<String> {
    let out = Command::new(binary).arg("--version").stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    if !out.status.success() {
        return None;
    }
    version_word(&String::from_utf8_lossy(&out.stdout))
}

fn version_word(text: &str) -> Option<String> {
    let mut words = text.lines().next()?.split_whitespace();
    (words.next()? == "podnet").then_some(())?;
    let v = words.next()?;
    v.chars().next()?.is_ascii_digit().then(|| v.to_string())
}

/// The facts of this host, with the kinds of `slots`.
pub(crate) fn facts(slots: &Slots, tools: Tools) -> HostFacts {
    HostFacts {
        podrunner: env!("CARGO_PKG_VERSION").to_string(),
        podvm: tools.podvm,
        podnet: tools.podnet,
        arch: std::env::consts::ARCH.to_string(),
        kvm: std::path::Path::new("/dev/kvm").exists(),
        kinds: slots.kind_facts(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_of_podnet_is_the_second_word() {
        assert_eq!(version_word("podnet 0.4.0-beta.17 (cfc0924)\n").as_deref(), Some("0.4.0-beta.17"));
        assert_eq!(version_word("podnet 0.4.0\n").as_deref(), Some("0.4.0"));
        for bad in ["", "other 1.0", "podnet", "podnet beta"] {
            assert_eq!(version_word(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_kind_with_a_podnet_gives_its_version() {
        let dir = std::env::temp_dir().join(format!("podrunner-host-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("podnet");
        std::fs::write(&bin, "#!/bin/sh\necho 'podnet 9.8.7 (abc)'\n").unwrap();
        std::fs::set_permissions(&bin, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        assert_eq!(podnet_version(&bin).as_deref(), Some("9.8.7"));
        assert_eq!(podnet_version(&dir.join("none")), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
