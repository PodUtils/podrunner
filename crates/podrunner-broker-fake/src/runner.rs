//! Run the official runner once against the fake broker: a new RSA key and
//! the start of `run.sh` (T-026, T-088).

use std::path::Path;
use std::process::Command;

use crate::Broker;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

/// A new RSA key, in the form of `.credentials_rsaparams`. .NET takes each
/// number big-endian with no sign byte, `d` as long as the modulus, and the
/// five numbers of the CRT half as long: each is padded with zero bytes.
pub fn rsaparams() -> String {
    use rsa::BoxedUint;
    use rsa::traits::{PrivateKeyParts, PublicKeyParts};
    let key = rsa::PrivateKey::new(&mut getrandom::SysRng, 2048).expect("an RSA key");
    let k = key.size();
    let b = |n: &BoxedUint, len: usize| {
        let raw = n.to_be_bytes_trimmed_vartime();
        let mut v = vec![0u8; len.saturating_sub(raw.len())];
        v.extend_from_slice(&raw);
        STANDARD.encode(v)
    };
    let primes = key.primes();
    let qinv = key.crt_coefficient().expect("the CRT coefficient");
    serde_json::json!({
        "d": b(key.d(), k), "dp": b(key.dp().expect("dp"), k / 2), "dq": b(key.dq().expect("dq"), k / 2),
        "exponent": STANDARD.encode(key.e().to_be_bytes_trimmed_vartime()), "inverseQ": b(&qinv, k / 2),
        "modulus": b(key.n().as_ref(), k),
        "p": b(&primes[0], k / 2), "q": b(&primes[1], k / 2),
    })
    .to_string()
}

/// Write the runner files of `broker` into `dir` and start `run.sh` there,
/// with the extra environment `env`, as the leader of a process group of
/// its own: a caller can end the whole runner with one signal.
pub fn start_runner(dir: &Path, broker: &Broker, env: &[(&str, String)]) -> Result<std::process::Child, String> {
    start_runner_with(dir, broker, env, true)
}

/// [`start_runner`]; with no `console`, the console of the runner goes to
/// /dev/null (`-q` and `--json`, T-278).
pub fn start_runner_with(
    dir: &Path,
    broker: &Broker,
    env: &[(&str, String)],
    console: bool,
) -> Result<std::process::Child, String> {
    start_runner_in(dir, &dir.join("_work"), broker, env, console)
}

/// [`start_runner_with`], with the work folder `work` of the runner (an
/// absolute path, or a path relative to `dir`).
pub fn start_runner_in(
    dir: &Path,
    work: &Path,
    broker: &Broker,
    env: &[(&str, String)],
    console: bool,
) -> Result<std::process::Child, String> {
    spawn(dir, work, broker, env, console, std::process::Stdio::inherit())
}

fn spawn(
    dir: &Path,
    work: &Path,
    broker: &Broker,
    env: &[(&str, String)],
    console: bool,
    stderr: std::process::Stdio,
) -> Result<std::process::Child, String> {
    use std::os::unix::process::CommandExt;
    // A killed run leaves its pages and blocks, and the fake broker gives
    // each run the same ids: the runner refuses a file of them that exists.
    for d in ["pages", "blocks"] {
        let _ = std::fs::remove_dir_all(dir.join("_diag").join(d));
    }
    for (name, mut content) in broker.runner_files("runner-u-1") {
        if name == ".runner" {
            let mut v: serde_json::Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
            v["workFolder"] = work.to_string_lossy().into_owned().into();
            content = v.to_string();
        }
        private_file(&dir.join(&name), content.as_bytes())
            .map_err(|e| format!("{}: the runner file {name}: {}", dir.display(), e.kind()))?;
    }
    // A clean environment: the runner refuses some proxy variables.
    Command::new("./run.sh")
        .current_dir(dir)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
        .env("RUNNER_ALLOW_RUNASROOT", "1")
        // With W^X, .NET maps its code through a memfd that it truncates to
        // a large size: under a limit of file size it dies by SIGXFSZ at the
        // start (U2, T-144). The podvm driver sets the same.
        .env("DOTNET_EnableWriteXorExecute", "0")
        // No debug pipes of .NET in /tmp, which a killed runner leaves
        // (T-281).
        .env("DOTNET_EnableDiagnostics", "0")
        .envs(env.iter().map(|(k, v)| (k, v)))
        .process_group(0)
        // The console of the runner goes to stderr: stdout has the lines of
        // the jobs, and their JSON with `--json` (T-134).
        .stdout(match console {
            true => std::process::Stdio::piped(),
            false => std::process::Stdio::null(),
        })
        // The errors of the runner stay on stderr: a crash names its cause.
        .stderr(stderr)
        .spawn()
        .map(|mut child| {
            if let Some(out) = child.stdout.take() {
                std::thread::spawn(move || {
                    use std::io::BufRead;
                    for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
                        eprintln!("{}", console_line(&line));
                    }
                });
            }
            child
        })
        .map_err(|e| format!("{}: run.sh: {}", dir.display(), e.kind()))
}

/// Write `bytes` to `path` at mode 0600, as a unit gets its runner files
/// (`.credentials_rsaparams` is a private key; #218).
pub fn private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    // A file that was there keeps its mode at open: set it.
    f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    f.write_all(bytes)
}

/// A line of the console of the runner: the runner names GitHub where it
/// reached the fake broker of the run (T-331).
pub fn console_line(line: &str) -> String {
    line.replace("Connected to GitHub", "Connected to the fake broker of podrunner (not GitHub)")
}

/// Run the runner in `dir` once against `broker`, with the extra
/// environment `env` (a helper of the tests).
pub fn run_once(dir: &Path, broker: &Broker, env: &[(&str, String)]) -> std::process::ExitStatus {
    start_runner(dir, broker, env).expect("the runner starts").wait().expect("the runner ends")
}

/// [`run_once`], with the stderr of the runner kept in the file `err` (the
/// log of a unit, where the job-started hook writes its line, T-552).
pub fn run_once_into(dir: &Path, broker: &Broker, env: &[(&str, String)], err: &Path) -> std::process::ExitStatus {
    let file = std::fs::File::create(err).expect("the stderr file");
    spawn(dir, &dir.join("_work"), broker, env, false, file.into())
        .expect("the runner starts")
        .wait()
        .expect("the runner ends")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #218: a runner file is private, also one that was there at 0644.
    #[test]
    fn a_runner_file_is_written_at_mode_0600() {
        use std::os::unix::fs::PermissionsExt;
        let f = std::env::temp_dir().join(format!("podrunner-private-{}", std::process::id()));
        std::fs::write(&f, "old").unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).unwrap();
        private_file(&f, b"new").unwrap();
        assert_eq!(std::fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::read_to_string(&f).unwrap(), "new");
        let _ = std::fs::remove_file(&f);
    }

    /// T-331: the console does not claim GitHub.
    #[test]
    fn the_console_names_the_fake_broker() {
        assert_eq!(console_line("√ Connected to GitHub"), "√ Connected to the fake broker of podrunner (not GitHub)");
        assert_eq!(console_line("other"), "other");
    }

    /// Each number has the length that .NET takes, with no sign byte.
    #[test]
    fn the_key_numbers_have_the_lengths_of_dotnet() {
        let v: serde_json::Value = serde_json::from_str(&rsaparams()).unwrap();
        let len = |k: &str| STANDARD.decode(v[k].as_str().unwrap()).unwrap().len();
        assert_eq!(len("modulus"), 256);
        assert_eq!(len("d"), 256);
        for k in ["p", "q", "dp", "dq", "inverseQ"] {
            assert_eq!(len(k), 128, "{k}");
        }
        assert_eq!(STANDARD.decode(v["exponent"].as_str().unwrap()).unwrap(), [1, 0, 1]);
    }
}
