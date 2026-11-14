//! The OIDC id-tokens of a local run (T-140). A job with
//! `permissions: id-token: write` gets `GenerateIdTokenUrl` in the data of
//! its system endpoint; the runner gives it to the steps as
//! `ACTIONS_ID_TOKEN_REQUEST_URL`, and `core.getIDToken()` asks
//! `URL&audience=AUD` for `{"value": JWT}`. The JWT has the claims of
//! GitHub and an ES256 signature by a key of the run: one P-256 key for
//! each run, in memory only. The broker serves the discovery document and
//! the keys for a relying party on the loopback.

use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair};
use serde_json::{Map, Value, json};

/// The path of the token route on the broker. The URL has a query, so that
/// the toolkit can add `&audience=`.
pub const TOKEN_PATH: &str = "/_oidc/token";

/// How long a token holds, in seconds.
const LIFE: u64 = 600;

/// The issuer of the run: its name and its key.
pub struct Issuer {
    pub issuer: String,
    key: EcdsaKeyPair,
    rng: SystemRandom,
    kid: String,
}

impl Issuer {
    /// A new key for the issuer `issuer` (a URL).
    pub fn new(issuer: &str) -> Result<Issuer, String> {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng)
            .map_err(|_| "the OIDC key: no key".to_string())?;
        let key = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng)
            .map_err(|_| "the OIDC key: a bad key".to_string())?;
        let d = ring::digest::digest(&ring::digest::SHA256, key.public_key().as_ref());
        let kid = URL_SAFE_NO_PAD.encode(&d.as_ref()[..16]);
        Ok(Issuer { issuer: issuer.trim_end_matches('/').to_string(), key, rng, kid })
    }

    /// The public keys, as a JWK set.
    pub fn jwks(&self) -> Value {
        // An uncompressed point: 0x04, X (32 bytes), Y (32 bytes).
        let p = self.key.public_key().as_ref();
        json!({ "keys": [{
            "kty": "EC", "crv": "P-256", "alg": "ES256", "use": "sig", "kid": self.kid,
            "x": URL_SAFE_NO_PAD.encode(&p[1..33]), "y": URL_SAFE_NO_PAD.encode(&p[33..65]),
        }]})
    }

    /// The discovery document.
    pub fn discovery(&self) -> Value {
        json!({
            "issuer": self.issuer,
            "jwks_uri": format!("{}/.well-known/jwks", self.issuer),
            "response_types_supported": ["id_token"],
            "subject_types_supported": ["public", "pairwise"],
            "id_token_signing_alg_values_supported": ["ES256"],
            "claims_supported": ["sub", "aud", "exp", "iat", "iss", "jti", "nbf", "ref", "repository", "sha",
                "actor", "event_name", "job_workflow_ref", "run_id", "environment"],
        })
    }

    /// A signed token with `claims` (the claims of the job) and `aud`.
    pub fn token(&self, claims: &Map<String, Value>, aud: &str) -> Result<String, String> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let mut c = claims.clone();
        let jti: [u8; 16] = rand_bytes(&self.rng)?;
        c.insert("iss".into(), json!(self.issuer));
        c.insert("aud".into(), json!(aud));
        c.insert("iat".into(), json!(now));
        c.insert("nbf".into(), json!(now));
        c.insert("exp".into(), json!(now + LIFE));
        c.insert("jti".into(), json!(URL_SAFE_NO_PAD.encode(jti)));
        let head = json!({ "typ": "JWT", "alg": "ES256", "kid": self.kid });
        let input = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(head.to_string()),
            URL_SAFE_NO_PAD.encode(Value::Object(c).to_string())
        );
        let sig = self.key.sign(&self.rng, input.as_bytes()).map_err(|_| "the OIDC signature failed".to_string())?;
        Ok(format!("{input}.{}", URL_SAFE_NO_PAD.encode(sig.as_ref())))
    }
}

fn rand_bytes(rng: &SystemRandom) -> Result<[u8; 16], String> {
    let mut b = [0u8; 16];
    ring::rand::SecureRandom::fill(rng, &mut b).map_err(|_| "no random bytes".to_string())?;
    Ok(b)
}

/// The value of `name` in the query of `path`, decoded.
fn query(path: &str, name: &str) -> Option<String> {
    let q = path.split_once('?')?.1;
    let v = q.split('&').find_map(|kv| kv.strip_prefix(name)?.strip_prefix('='))?;
    let mut out = Vec::new();
    let b = v.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// The answer of a route of OIDC, or `None` for another route. `claims` are
/// those of the job of this broker, when it may have a token.
pub fn route(method: &str, path: &str, issuer: &Issuer, claims: Option<&Map<String, Value>>) -> Option<(u16, Vec<u8>)> {
    let bare = path.split('?').next().unwrap_or("");
    let json = |v: Value| Some((200, v.to_string().into_bytes()));
    match (method, bare) {
        ("GET", "/.well-known/openid-configuration") => json(issuer.discovery()),
        ("GET", "/.well-known/jwks") => json(issuer.jwks()),
        ("GET", TOKEN_PATH) => {
            let Some(claims) = claims else {
                return Some((403, b"the job has no permissions: id-token: write".to_vec()));
            };
            // GitHub gives the URL of the owner when the request names no audience.
            let owner = claims.get("repository_owner").and_then(Value::as_str).unwrap_or("local");
            let aud = query(path, "audience").unwrap_or_else(|| format!("https://github.com/{owner}"));
            match issuer.token(claims, &aud) {
                Ok(t) => json(json!({ "value": t })),
                Err(e) => Some((500, e.into_bytes())),
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
