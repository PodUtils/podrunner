//! The mounts of a fake container. A fake container is a host process with
//! no mount namespace, so the fake maps each mount target to its host
//! source in the argv, the environment and the working directory of each
//! process of the container. A real driver applies the mounts instead.

use std::path::{Path, PathBuf};

use podrunner_unit::ContainerMount;

/// The mounts of one container, the longest target first.
#[derive(Debug, Clone, Default)]
pub struct Mounts(Vec<(String, String)>);

impl Mounts {
    /// The mounts of `spec`; a relative source is under `runner_root`.
    pub fn of(mounts: &[ContainerMount], runner_root: &Path) -> Mounts {
        let mut v: Vec<(String, String)> = mounts
            .iter()
            .map(|m| {
                let src = if m.source.is_absolute() { m.source.clone() } else { runner_root.join(&m.source) };
                (m.target.trim_end_matches('/').to_string(), src.display().to_string())
            })
            .filter(|(t, _)| t.starts_with('/') && t.len() > 1)
            .collect();
        v.sort_by_key(|(t, _)| std::cmp::Reverse(t.len()));
        Mounts(v)
    }

    /// `s` with each path under a target moved to its source. A target
    /// matches at the start of `s` or after `=`, `:` or a space, and only
    /// before `/` or the end of a path.
    pub fn map(&self, s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut i = 0;
        'scan: while i < s.len() {
            let at_start = i == 0 || matches!(s.as_bytes()[i - 1], b'=' | b':' | b' ' | b'\t' | b'\n' | b'"' | b'\'');
            if at_start {
                for (target, source) in &self.0 {
                    let rest = &s[i..];
                    if rest.starts_with(target.as_str()) {
                        let end = rest.as_bytes().get(target.len());
                        if matches!(end, None | Some(b'/' | b':' | b' ' | b'"' | b'\'' | b'\n')) {
                            out.push_str(source);
                            i += target.len();
                            continue 'scan;
                        }
                    }
                }
            }
            let c = s[i..].chars().next().unwrap_or_default();
            out.push(c);
            i += c.len_utf8();
        }
        out
    }

    /// The working directory: the mapped `workdir` when it exists, else
    /// `default`.
    pub fn workdir(&self, workdir: Option<&str>, default: &Path) -> PathBuf {
        match workdir.map(|w| PathBuf::from(self.map(w))) {
            Some(p) if p.is_dir() => p,
            _ => default.to_path_buf(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mounts() -> Mounts {
        let m = |s: &str, t: &str| ContainerMount { source: s.into(), target: t.into(), read_only: false };
        Mounts::of(&[m("/r/_work", "/__w"), m("/r/_work/_temp", "/__w/_temp/"), m("ext", "/__e")], Path::new("/r"))
    }

    #[test]
    fn a_target_maps_to_its_source() {
        let m = mounts();
        assert_eq!(m.map("/__w/app/app"), "/r/_work/app/app");
        assert_eq!(m.map("/__w"), "/r/_work");
        // The longest target wins.
        assert_eq!(m.map("/__w/_temp/x.sh"), "/r/_work/_temp/x.sh");
        // A relative source is under the runner root.
        assert_eq!(m.map("/__e/node20/bin"), "/r/ext/node20/bin");
        // In a list of paths and after `=`.
        assert_eq!(m.map("/__e/a:/usr/bin"), "/r/ext/a:/usr/bin");
        assert_eq!(m.map("X=/__w/a"), "X=/r/_work/a");
        assert_eq!(m.map("sh -e '/__w/_temp/a.sh'"), "sh -e '/r/_work/_temp/a.sh'");
    }

    #[test]
    fn a_longer_name_or_an_inner_part_is_kept() {
        let m = mounts();
        assert_eq!(m.map("/__work"), "/__work");
        assert_eq!(m.map("/x/__w/a"), "/x/__w/a");
        assert_eq!(m.map("ÿ/__w"), "ÿ/__w");
        assert_eq!(m.workdir(Some("/__w/none"), Path::new("/d")), PathBuf::from("/d"));
    }
}
