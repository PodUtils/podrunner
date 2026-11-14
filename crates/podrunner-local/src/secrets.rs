//! The secrets of a local run (T-121). A secret comes from the environment
//! of podrunner (`--secret NAME`) or from a secret file (`--secret-file`):
//! one `NAME=VALUE` on each line, or `NAME<<DELIM`, the lines of the value,
//! and `DELIM`, as `GITHUB_ENV`. A value never comes from argv. A secret
//! file that another user can read is refused.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// A name that GitHub takes for a secret: a name of an expression, not
/// `GITHUB_` at the start.
fn check_name(name: &str) -> Result<(), String> {
    if !pod_actions_expr::is_name(name) || name.contains('-') {
        return Err(format!("{name:?} is not a name of a secret"));
    }
    if name.to_ascii_uppercase().starts_with("GITHUB_") {
        return Err(format!("{name:?}: a secret name cannot start with GITHUB_"));
    }
    Ok(())
}

/// The secrets of the text of a secret file.
pub fn parse(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut lines = text.lines().enumerate();
    while let Some((n, line)) = lines.next() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let (name, value) = if let Some((name, delim)) = line.split_once("<<") {
            let mut value = Vec::new();
            loop {
                let Some((_, l)) = lines.next() else {
                    return Err(format!("line {}: no {delim:?} ends the value of {name}", n + 1));
                };
                if l == delim {
                    break;
                }
                value.push(l);
            }
            (name, value.join("\n"))
        } else if let Some((name, value)) = line.split_once('=') {
            (name, value.to_string())
        } else {
            return Err(format!("line {}: not NAME=VALUE or NAME<<DELIM", n + 1));
        };
        check_name(name).map_err(|e| format!("line {}: {e}", n + 1))?;
        out.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
        out.push((name.to_string(), value));
    }
    Ok(out)
}

/// The secrets of a secret file: its mode must be 0600 or less.
pub fn read_file(path: &Path) -> Result<Vec<(String, String)>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(format!("{}: mode {mode:04o}: a secret file needs mode 0600 (chmod 600)", path.display()));
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The secret NAME from the environment.
pub fn from_env(name: &str) -> Result<(String, String), String> {
    check_name(name)?;
    let value = std::env::var(name).map_err(|_| format!("--secret {name}: the environment has no {name}"))?;
    Ok((name.to_string(), value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_file_takes_lines_and_blocks() {
        let s = parse("# c\nA=1\nB=x=y\n\nCERT<<END\nline 1\nline 2\nEND\nA=2\n").unwrap();
        let want = [("B", "x=y"), ("CERT", "line 1\nline 2"), ("A", "2")];
        assert_eq!(s, want.map(|(k, v)| (k.to_string(), v.to_string())));
        for bad in ["A", "1A=x", "GITHUB_TOKEN=x", "A-B=x", "C<<END\nno end"] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_secret_file_that_others_read_is_refused() {
        let path = std::env::temp_dir().join(format!("podrunner-secrets-{}", std::process::id()));
        std::fs::write(&path, "A=1\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_file(&path).unwrap_err().contains("0600"));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(read_file(&path).unwrap(), [("A".to_string(), "1".to_string())]);
        let _ = std::fs::remove_file(&path);
    }
}
