//! Run the official runner once against the fake broker: a new RSA key and
//! the start of `run.sh` (T-026, T-088).

use std::path::Path;
use std::process::Command;

use crate::{Broker, files::runner_files};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

/// A new RSA key, in the form of `.credentials_rsaparams`. .NET takes each
/// number big-endian with no sign byte, `d` as long as the modulus, and the
/// five numbers of the CRT half as long: each is padded with zero bytes.
pub fn rsaparams() -> String {
    use rsa::BigUint;
    use rsa::traits::{PrivateKeyParts, PublicKeyParts};
    let key = rsa::RsaPrivateKey::new(&mut rand_core::OsRng, 2048).expect("an RSA key");
    let k = key.size();
    let b = |n: &BigUint, len: usize| {
        let raw = n.to_bytes_be();
        let mut v = vec![0u8; len.saturating_sub(raw.len())];
        v.extend_from_slice(&raw);
        STANDARD.encode(v)
    };
    let primes = key.primes();
    let qinv = key.crt_coefficient().expect("the CRT coefficient");
    serde_json::json!({
        "d": b(key.d(), k), "dp": b(key.dp().expect("dp"), k / 2), "dq": b(key.dq().expect("dq"), k / 2),
        "exponent": STANDARD.encode(key.e().to_bytes_be()), "inverseQ": b(&qinv, k / 2), "modulus": b(key.n(), k),
        "p": b(&primes[0], k / 2), "q": b(&primes[1], k / 2),
    })
    .to_string()
}

/// Run the runner in `dir` once against `broker`, with the extra
/// environment `env`.
pub fn run_once(dir: &Path, broker: &Broker, env: &[(&str, String)]) -> std::process::ExitStatus {
    for (name, content) in runner_files(&broker.base, "runner-u-1", &rsaparams()) {
        std::fs::write(dir.join(name), content).unwrap();
    }
    // A clean environment: the runner refuses some proxy variables.
    Command::new("./run.sh")
        .current_dir(dir)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
        .env("RUNNER_ALLOW_RUNASROOT", "1")
        .envs(env.iter().map(|(k, v)| (k, v)))
        .status()
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

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
