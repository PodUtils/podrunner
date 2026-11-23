//! The environment of a unit or of a container, to podvm (T-405). Each
//! user of the host reads the argv of a process (`/proc/PID/cmdline`), and
//! a step environment holds secrets and the runtime token. With podvm
//! 0.2.0-beta.32 or later the values go in a file of mode 0600 in the
//! directory of the unit, which goes after the call. podvm 0.2.0-beta.33
//! and later read each `KEY=VALUE` to a NUL (`--env-file0`), so each value
//! goes in the file; with 0.2.0-beta.32 (`--env-file`, one line each) a
//! value with a newline stays an `-e` argument.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use podrunner_unit::{UnitCode, UnitError};

/// The first podvm that takes `--env-file` on `run` and `exec`.
const ENV_FILE: &str = "0.2.0-beta.32";

/// The first podvm that takes `--env-file0` (PodUtils/podvm@T-1971).
const ENV_FILE0: &str = "0.2.0-beta.33";

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
    let (mut args, rest) = write_env_file(podvm, dir, &env)?;
    let path = args.get(1).map(PathBuf::from);
    args.extend(rest.iter().flat_map(|(k, v)| ["-e".into(), format!("{k}={v}")]));
    Ok((args, EnvFile(path)))
}

/// Write the values of `env` to a new file of mode 0600 in `dir`, for a
/// podvm that takes it: each one with `--env-file0`, each one with no
/// newline with `--env-file`. The arguments that name the file (none when
/// no value went in it: the flag and the path), and the pairs that stay
/// `-e`. The caller removes the file; a guest kind leaves it to the
/// removal of the directory of its unit.
pub fn write_env_file(
    podvm: &crate::Podvm,
    dir: &Path,
    env: &std::collections::BTreeMap<String, String>,
) -> Result<(Vec<String>, std::collections::BTreeMap<String, String>), UnitError> {
    if env.is_empty() || !super::labels::at_least(podvm, ENV_FILE) {
        return Ok((Vec::new(), env.clone()));
    }
    let nul = super::labels::at_least(podvm, ENV_FILE0);
    let (lines, rest): (Vec<_>, Vec<_>) = env.iter().partition(|(_, v)| nul || !v.contains('\n'));
    let rest = rest.into_iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    if lines.is_empty() {
        return Ok((Vec::new(), rest));
    }
    // A pid repeats in each PID namespace: the name has a nonce too, and
    // the open refuses a file that is there.
    static N: AtomicU64 = AtomicU64::new(0);
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
    let path = dir.join(format!("env-{}-{}-{nonce}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    let end = if nul { '\0' } else { '\n' };
    let text: String = lines.iter().map(|(k, v)| format!("{k}={v}{end}")).collect();
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
    let flag = if nul { "--env-file0" } else { "--env-file" };
    Ok((vec![flag.into(), path.display().to_string()], rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> crate::scratch::Scratch {
        crate::scratch::Scratch::new("envfile")
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

    /// With podvm 0.2.0-beta.33 each value goes in the file, a value with a
    /// newline too, each one ended by a NUL.
    #[test]
    fn each_value_goes_in_a_file_of_nul_ended_entries() {
        let d = dir();
        let env = [("TOKEN".to_string(), "secret-value-41".to_string()), ("KEY".to_string(), "a\nb".to_string())];
        let (args, file) = args_with(&d, "0.2.0-beta.33", &env);
        assert_eq!(args.len(), 2, "no -e: {args:?}");
        assert_eq!(args[0], "--env-file0");
        assert_eq!(std::fs::read_to_string(&args[1]).unwrap(), "KEY=a\nb\0TOKEN=secret-value-41\0");
        drop(file);
        assert!(!PathBuf::from(&args[1]).exists());
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
