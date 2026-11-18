//! The environment of a unit or of a container, to podvm (T-405). Each
//! user of the host reads the argv of a process (`/proc/PID/cmdline`), and
//! a step environment holds secrets and the runtime token. With podvm
//! 0.2.0-beta.32 or later the values go in a file of mode 0600 in the
//! directory of the unit (`--env-file`), which goes after the call. podvm
//! reads a line as `KEY=VALUE`, so a value with a newline stays an `-e`
//! argument until podvm has a form for it.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use podrunner_unit::{UnitCode, UnitError};

/// The first podvm that takes `--env-file` on `run` and `exec`.
const ENV_FILE: &str = "0.2.0-beta.32";

/// The file of one call; it goes at the drop.
pub(super) struct EnvFile(Option<PathBuf>);

impl Drop for EnvFile {
    fn drop(&mut self) {
        if let Some(p) = &self.0 {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// The arguments that give `env` to a call of podvm, and the file that
/// holds the values (in `dir`, a directory of the unit).
pub(super) fn env_args<'a>(
    podvm: &crate::Podvm,
    dir: &Path,
    env: impl IntoIterator<Item = (&'a String, &'a String)>,
) -> Result<(Vec<String>, EnvFile), UnitError> {
    let env: std::collections::BTreeMap<String, String> =
        env.into_iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let (path, rest) = write_env_file(podvm, dir, &env)?;
    let mut args = Vec::new();
    if let Some(p) = &path {
        args.extend(["--env-file".into(), p.display().to_string()]);
    }
    args.extend(rest.iter().flat_map(|(k, v)| ["-e".into(), format!("{k}={v}")]));
    Ok((args, EnvFile(path)))
}

/// Write each value of `env` with no newline to a new file of mode 0600 in
/// `dir`, for a podvm that takes `--env-file`. The path of the file (none
/// when no value went in it), and the pairs that stay `-e`. The caller
/// removes the file; a guest kind leaves it to the removal of the
/// directory of its unit.
pub fn write_env_file(
    podvm: &crate::Podvm,
    dir: &Path,
    env: &std::collections::BTreeMap<String, String>,
) -> Result<(Option<PathBuf>, std::collections::BTreeMap<String, String>), UnitError> {
    if env.is_empty() || !super::labels::at_least(podvm, ENV_FILE) {
        return Ok((None, env.clone()));
    }
    let (lines, rest): (Vec<_>, Vec<_>) = env.iter().partition(|(_, v)| !v.contains('\n'));
    let rest = rest.into_iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    if lines.is_empty() {
        return Ok((None, rest));
    }
    // A pid repeats in each PID namespace: the name has a nonce too, and
    // the open refuses a file that is there.
    static N: AtomicU64 = AtomicU64::new(0);
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    let path = dir.join(format!("env-{}-{}-{nonce}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let text: String = lines.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
    let io = |e: std::io::Error| UnitError::new(UnitCode::UnitStartFailed, format!("the environment file: {e}"));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .map_err(io)?;
    if let Err(e) = f.write_all(text.as_bytes()) {
        let _ = std::fs::remove_file(&path);
        return Err(io(e));
    }
    Ok((Some(path), rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "podrunner-envfile-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// T-405: with a podvm that takes the file, no value is in the argv
    /// but one with a newline; the file is 0600 and goes at the drop.
    #[test]
    fn the_values_go_in_a_file_of_mode_0600() {
        use std::os::unix::fs::PermissionsExt;
        let d = dir();
        let env = [("TOKEN".to_string(), "secret-value-41".to_string()), ("KEY".to_string(), "a\nb".to_string())];
        let (args, file) = write_for_test(&d, &env);
        assert!(!args.iter().any(|a| a.contains("secret-value-41")), "{args:?}");
        assert_eq!(args[0], "--env-file");
        assert!(args.contains(&"KEY=a\nb".to_string()), "a value with a newline stays: {args:?}");
        let path = PathBuf::from(&args[1]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "TOKEN=secret-value-41\n");
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        drop(file);
        assert!(!path.exists(), "the file goes after the call");
    }

    /// `env_args` with a stand-in podvm of `version` (its `probe --json`).
    fn args_with(d: &Path, version: &str, env: &[(String, String)]) -> (Vec<String>, EnvFile) {
        use std::os::unix::fs::PermissionsExt;
        let bin = d.join(format!("podvm-{version}"));
        std::fs::write(&bin, format!("#!/bin/sh\necho '{{\"podvm\":\"{version}\"}}'\n")).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let podvm = crate::Podvm { binary: bin };
        env_args(&podvm, d, env.iter().map(|(k, v)| (k, v))).unwrap()
    }

    fn write_for_test(d: &Path, env: &[(String, String)]) -> (Vec<String>, EnvFile) {
        args_with(d, "0.2.0-beta.32", env)
    }

    /// An older podvm has no `--env-file`: the values stay in the argv.
    #[test]
    fn an_older_podvm_gets_the_values_in_the_argv() {
        let d = dir();
        let env = [("A".to_string(), "1".to_string())];
        let (args, _f) = args_with(&d, "0.2.0-beta.30", &env);
        assert_eq!(args, ["-e", "A=1"]);
    }
}
