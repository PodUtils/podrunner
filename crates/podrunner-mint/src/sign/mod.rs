//! `sign`: the one holder of the private key of the GitHub App. It signs
//! one JWT shape (RS256; the claims `iat`, `exp` and `iss`, with a fixed
//! `iss` and `exp` at most ten minutes after `iat`), at most once a minute,
//! and writes an audit line for each request (the caller, `iss`, `exp`).
//! Another shape is refused and audited. The signature comes from `ring`,
//! not from the private-key path of `rsa` 0.9 (RUSTSEC-2023-0071).

use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use podrunner_clock::Clock;
use ring::rand::SystemRandom;
use ring::signature::{RSA_PKCS1_SHA256, RsaKeyPair};
use serde::Serialize;
use serde_json::{Map, Value};

/// The longest life of a token: GitHub refuses more than ten minutes.
pub const MAX_LIFE_S: u64 = 600;
/// The least time between two signatures.
pub const MIN_GAP_MS: u64 = 60_000;

/// Why `sign` refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// A claim other than `iat`, `exp` and `iss`, another `iss`, or an
    /// `exp` past ten minutes.
    Shape(String),
    /// A second request within a minute.
    Rate,
    /// The key cannot sign.
    Key(String),
}

/// One audit line of `sign`. It holds no claim value but `iss` and `exp`,
/// and no byte of the key or of a token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Audit {
    pub t_ms: u64,
    pub caller: String,
    pub iss: String,
    pub exp: u64,
    pub outcome: &'static str,
}

/// The signer.
pub struct Signer {
    key: RsaKeyPair,
    iss: String,
    clock: Arc<dyn Clock>,
    /// The time of the Unix epoch in seconds.
    now_unix: Box<dyn Fn() -> u64 + Send + Sync>,
    last_ms: Option<u64>,
    pub audit: Vec<Audit>,
}

impl std::fmt::Debug for Signer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Signer {{ iss: {:?}, key: <hidden> }}", self.iss)
    }
}

/// Read a PKCS#1 PEM key (`-----BEGIN RSA PRIVATE KEY-----`), the form that
/// GitHub gives for an App. The error names no byte of the key.
pub fn key_from_pem(pem: &str) -> Result<RsaKeyPair, Refusal> {
    let body: String = pem
        .lines()
        .map(str::trim)
        .skip_while(|l| *l != "-----BEGIN RSA PRIVATE KEY-----")
        .skip(1)
        .take_while(|l| *l != "-----END RSA PRIVATE KEY-----")
        .collect();
    if body.is_empty() {
        return Err(Refusal::Key("no PKCS#1 PEM block (BEGIN RSA PRIVATE KEY)".into()));
    }
    let der = STANDARD.decode(body).map_err(|_| Refusal::Key("the PEM body is not base64".into()))?;
    RsaKeyPair::from_der(&der).map_err(|e| Refusal::Key(format!("the key is refused: {e}")))
}

impl Signer {
    pub fn new(
        key: RsaKeyPair,
        iss: &str,
        clock: Arc<dyn Clock>,
        now_unix: Box<dyn Fn() -> u64 + Send + Sync>,
    ) -> Signer {
        Signer { key, iss: iss.into(), clock, now_unix, last_ms: None, audit: Vec::new() }
    }

    /// The public key, for a verifier.
    pub fn public_key(&self) -> Vec<u8> {
        use ring::signature::KeyPair;
        self.key.public_key().as_ref().to_vec()
    }

    /// Sign the claims of `caller`. They must be exactly `iat`, `exp` and
    /// `iss`, with `iss` the App id of this signer and `exp` after `iat` by
    /// ten minutes or less; `iat` within a minute of now.
    pub fn sign(&mut self, caller: &str, claims: &Map<String, Value>) -> Result<String, Refusal> {
        let now_ms = self.clock.now_ms();
        let iss = claims.get("iss").and_then(Value::as_str).unwrap_or("").to_string();
        let exp = claims.get("exp").and_then(Value::as_u64).unwrap_or(0);
        let result = self.check(claims, now_ms).and_then(|()| self.jwt(claims));
        let outcome = match &result {
            Ok(_) => "signed",
            Err(Refusal::Shape(_)) => "refused-shape",
            Err(Refusal::Rate) => "refused-rate",
            Err(Refusal::Key(_)) => "refused-key",
        };
        self.audit.push(Audit { t_ms: now_ms, caller: caller.chars().take(64).collect(), iss, exp, outcome });
        if result.is_ok() {
            self.last_ms = Some(now_ms);
        }
        result
    }

    fn check(&self, claims: &Map<String, Value>, now_ms: u64) -> Result<(), Refusal> {
        let mut keys: Vec<&str> = claims.keys().map(String::as_str).collect();
        keys.sort_unstable();
        if keys != ["exp", "iat", "iss"] {
            return Err(Refusal::Shape(format!("the claims are {keys:?}, not exp, iat, iss")));
        }
        if claims["iss"].as_str() != Some(self.iss.as_str()) {
            return Err(Refusal::Shape("another iss".into()));
        }
        let (Some(iat), Some(exp)) = (claims["iat"].as_u64(), claims["exp"].as_u64()) else {
            return Err(Refusal::Shape("iat and exp are not whole seconds".into()));
        };
        let now = (self.now_unix)();
        if exp <= iat || exp - iat > MAX_LIFE_S || iat.abs_diff(now) > 60 {
            return Err(Refusal::Shape("exp is not within ten minutes of iat, or iat is not now".into()));
        }
        if self.last_ms.is_some_and(|t| now_ms.saturating_sub(t) < MIN_GAP_MS) {
            return Err(Refusal::Rate);
        }
        Ok(())
    }

    fn jwt(&self, claims: &Map<String, Value>) -> Result<String, Refusal> {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
        let body = URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims).expect("claims are JSON"));
        let input = format!("{header}.{body}");
        let mut sig = vec![0; self.key.public().modulus_len()];
        self.key
            .sign(&RSA_PKCS1_SHA256, &SystemRandom::new(), input.as_bytes(), &mut sig)
            .map_err(|_| Refusal::Key("the signature failed".into()))?;
        Ok(format!("{input}.{}", URL_SAFE_NO_PAD.encode(sig)))
    }
}

#[cfg(test)]
mod tests;
