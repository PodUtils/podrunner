//! The credentials of the broker (T-406). A local process can connect to
//! the broker, so the broker gives a job only to the runner that it made:
//!
//! * The token of the OAuth client goes only to a client assertion that a
//!   key of [`crate::Broker::runner_files`] signed (RS256, as the runner
//!   signs it with `.credentials_rsaparams`).
//! * The routes that give a job (the session, the long poll, `acquirejob`)
//!   take only that token.
//! * The lock and the end of a job, the id-token, the live feed and the
//!   twirp services (the logs, the cache, the artifacts) take that token or
//!   the job token, which the job message gives to the Worker (the runner
//!   sends the job token on each of these: measured).
//! * A blob URL of the logs has a random signature (the module `results`);
//!   the stores of the cache and the artifacts sign their own blob URLs.
//!
//! * The tree of the job (`_checkout.tar.gz`) takes the job token, which the
//!   checkout script reads from `github.podrunner_token` and gives to curl
//!   on its standard input, never in its argv. The runner masks it.
//! * A hold is `/_podrunner/hold/KEY/CELL` with the hold key of the broker
//!   (`github.podrunner_hold`): the URL is in the argv of curl or wget, so
//!   its key gives the holds only.
//!
//! Each token is random for each broker.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use ring::rand::SecureRandom;

/// The credentials of one broker.
pub(crate) struct Auth {
    /// The access token of the OAuth client.
    pub runner: String,
    /// The job token of the job messages.
    pub job: String,
    /// The signature of each blob URL of the logs.
    pub blob: String,
    /// The key of the holds in their path.
    pub hold: String,
    /// The public key (modulus, exponent) of each admitted runner.
    keys: Vec<(Vec<u8>, Vec<u8>)>,
}

impl Default for Auth {
    fn default() -> Auth {
        Auth {
            runner: crate::message::job_token_with(&secret()),
            job: crate::message::job_token_with(&secret()),
            blob: secret(),
            hold: secret(),
            keys: Vec::new(),
        }
    }
}

/// 32 random bytes, in base64url: the signature part of a token.
fn secret() -> String {
    let mut b = [0u8; 32];
    ring::rand::SystemRandom::new().fill(&mut b).expect("the random source of the system");
    URL_SAFE_NO_PAD.encode(b)
}

/// What a route needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Need {
    /// The token of the OAuth client.
    Runner,
    /// The token of the OAuth client or the job token.
    Either,
}

/// What `method` and `path` (with no query) need; none for an open route.
pub(crate) fn need(method: &str, path: &str) -> Option<Need> {
    match (method, path) {
        ("POST" | "DELETE", "/session") | ("GET", "/message") | ("POST", "/acquirejob") => Some(Need::Runner),
        ("POST", "/renewjob" | "/completejob") | (_, crate::oidc::TOKEN_PATH) | ("GET", crate::feed::PATH) => {
            Some(Need::Either)
        }
        // The tree of the job: the checkout script sends the job token.
        ("GET", p) if p.strip_prefix('/') == Some(crate::actions::CHECKOUT) => Some(Need::Either),
        // The results, the cache and the artifacts: the Worker and the
        // clients of the actions send the job token.
        ("POST", p) if p.starts_with("/twirp/") => Some(Need::Either),
        _ => None,
    }
}

/// Bytes equal in a time that does not depend on where they differ.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

impl Auth {
    /// Admit the runner of `rsaparams` (the JSON of its key).
    pub fn admit(&mut self, rsaparams: &str) -> Result<(), String> {
        let v: serde_json::Value = serde_json::from_str(rsaparams).map_err(|e| format!("the runner key: {e}"))?;
        let part = |k: &str| -> Result<Vec<u8>, String> {
            let raw =
                STANDARD.decode(v[k].as_str().unwrap_or_default()).map_err(|e| format!("the runner key {k}: {e}"))?;
            // The key of ring takes no leading zero byte.
            Ok(raw.iter().position(|b| *b != 0).map_or_else(Vec::new, |i| raw[i..].to_vec()))
        };
        let (n, e) = (part("modulus")?, part("exponent")?);
        if n.is_empty() || e.is_empty() {
            return Err("the runner key has no modulus or no exponent".into());
        }
        self.keys.push((n, e));
        Ok(())
    }

    /// The path of a hold with no key, when `path` is a hold with the key of
    /// this broker; `Err` for a hold with another key or none.
    pub fn hold_path(&self, path: &str) -> Option<Result<String, ()>> {
        let rest = path.strip_prefix(crate::hold::PATH)?;
        Some(match rest.split_once('/') {
            Some((key, cell)) if same(key.as_bytes(), self.hold.as_bytes()) => {
                Ok(format!("{}{cell}", crate::hold::PATH))
            }
            _ => Err(()),
        })
    }

    /// Whether `sig` is the signature of the blob URLs.
    pub fn blob_sig(&self, sig: &str) -> bool {
        same(sig.as_bytes(), self.blob.as_bytes())
    }

    /// Whether `authorization` (the value of the header) holds what `need`
    /// asks for.
    pub fn allows(&self, need: Need, authorization: Option<&str>) -> bool {
        // The scheme has no case (RFC 9110, 11.1): `@actions/core` sends
        // `bearer`.
        let Some(token) = authorization
            .and_then(|a| a.trim().split_once(' '))
            .and_then(|(scheme, t)| scheme.eq_ignore_ascii_case("bearer").then_some(t))
        else {
            return false;
        };
        let token = token.trim().as_bytes();
        same(token, self.runner.as_bytes()) || (need == Need::Either && same(token, self.job.as_bytes()))
    }

    /// The access token for the form `body` of `oauth2/token`, when its
    /// `client_assertion` has the signature of an admitted key.
    pub fn grant(&self, body: &[u8]) -> Option<&str> {
        let body = std::str::from_utf8(body).ok()?;
        let assertion = body.split('&').find_map(|kv| kv.strip_prefix("client_assertion="))?;
        let assertion = percent_decode(assertion)?;
        let (signed, sig) = assertion.rsplit_once('.')?;
        let sig = URL_SAFE_NO_PAD.decode(sig).ok()?;
        let ok = self.keys.iter().any(|(n, e)| {
            ring::signature::RsaPublicKeyComponents { n, e }
                .verify(&ring::signature::RSA_PKCS1_2048_8192_SHA256, signed.as_bytes(), &sig)
                .is_ok()
        });
        ok.then_some(self.runner.as_str())
    }
}

/// The text of a form value: `%XX` and `+`.
fn percent_decode(s: &str) -> Option<String> {
    let (mut out, b) = (Vec::with_capacity(s.len()), s.as_bytes());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                out.push(u8::from_str_radix(std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?, 16).ok()?);
                i += 3;
            }
            b'+' => (out.push(b' '), i += 1).1,
            c => (out.push(c), i += 1).1,
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A client assertion signed with the key `rsaparams`, as the runner
    /// makes it.
    pub(crate) fn assertion(rsaparams: &str) -> String {
        use rsa::BigUint;
        let v: serde_json::Value = serde_json::from_str(rsaparams).unwrap();
        let int = |k: &str| BigUint::from_bytes_be(&STANDARD.decode(v[k].as_str().unwrap()).unwrap());
        let key =
            rsa::RsaPrivateKey::from_components(int("modulus"), int("exponent"), int("d"), vec![int("p"), int("q")])
                .unwrap();
        let head = URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256","typ":"JWT"}"#);
        let claims = URL_SAFE_NO_PAD.encode(r#"{"sub":"00000000-0000-0000-0000-000000000007"}"#);
        let signed = format!("{head}.{claims}");
        // The DigestInfo prefix of SHA-256 (RFC 8017, 9.2).
        let mut hashed = vec![
            0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05, 0x00, 0x04,
            0x20,
        ];
        hashed.extend_from_slice(ring::digest::digest(&ring::digest::SHA256, signed.as_bytes()).as_ref());
        let sig = key.sign(rsa::Pkcs1v15Sign::new_unprefixed(), &hashed).unwrap();
        format!("{signed}.{}", URL_SAFE_NO_PAD.encode(sig))
    }

    fn form(assertion: &str) -> Vec<u8> {
        format!(
            "grant_type=client_credentials&client_assertion_type=urn%3Aietf%3Aparams%3Aoauth%3Aclient-assertion-type%3Ajwt-bearer&client_assertion={assertion}"
        )
        .into_bytes()
    }

    #[test]
    fn only_an_admitted_key_gets_the_token() {
        let (mine, other) = (crate::runner::rsaparams(), crate::runner::rsaparams());
        let mut auth = Auth::default();
        assert_eq!(auth.grant(&form(&assertion(&mine))), None, "no key is admitted");
        auth.admit(&mine).unwrap();
        assert_eq!(auth.grant(&form(&assertion(&mine))), Some(auth.runner.as_str()));
        assert_eq!(auth.grant(&form(&assertion(&other))), None, "a key of another runner");
        // A changed claim breaks the signature.
        let good = assertion(&mine);
        let (h, rest) = good.split_once('.').unwrap();
        let (_, sig) = rest.rsplit_once('.').unwrap();
        let forged = format!("{h}.{}.{sig}", URL_SAFE_NO_PAD.encode(r#"{"sub":"x"}"#));
        assert_eq!(auth.grant(&form(&forged)), None);
        // The edges of the form.
        for bad in ["", "client_assertion=", "client_assertion=a.b", "client_assertion=%zz", "x=1"] {
            assert_eq!(auth.grant(bad.as_bytes()), None, "{bad}");
        }
        assert!(auth.admit("{}").is_err() && auth.admit("not json").is_err());
    }

    #[test]
    fn each_route_takes_its_token() {
        let auth = Auth::default();
        let (runner, job) = (format!("Bearer {}", auth.runner), format!("Bearer {}", auth.job));
        assert_ne!(auth.runner, auth.job);
        assert_ne!(auth.runner, Auth::default().runner, "each broker has its own token");
        assert!(auth.allows(Need::Runner, Some(&runner)));
        assert!(!auth.allows(Need::Runner, Some(&job)), "a step has the job token: it gets no other job");
        assert!(auth.allows(Need::Either, Some(&job)) && auth.allows(Need::Either, Some(&runner)));
        assert!(auth.allows(Need::Either, Some(&format!("bearer {}", auth.job))), "the scheme has no case");
        assert!(auth.allows(Need::Runner, Some(&format!("BEARER  {} ", auth.runner))));
        assert!(!auth.allows(Need::Runner, Some(&format!("Basic {}", auth.runner))));
        let made_up = format!("Bearer {}", crate::message::job_token());
        for bad in [None, Some(""), Some("Bearer "), Some(auth.runner.as_str()), Some(made_up.as_str())] {
            assert!(!auth.allows(Need::Either, bad), "{bad:?}");
        }
        assert_eq!(need("GET", "/message"), Some(Need::Runner));
        assert_eq!(need("POST", "/acquirejob"), Some(Need::Runner));
        assert_eq!(need("POST", "/renewjob"), Some(Need::Either));
        assert_eq!(need("GET", crate::oidc::TOKEN_PATH), Some(Need::Either));
        assert_eq!(need("PUT", "/blobs/step-1"), None, "a blob URL has its own signature");
        assert_eq!(need("GET", crate::feed::PATH), Some(Need::Either));
        assert_eq!(need("GET", "/_checkout.tar.gz"), Some(Need::Either));
        assert_eq!(need("GET", "/_podrunner/hold/build.0"), None, "a hold takes its key in the path");
        let key = format!("/_podrunner/hold/{}/build.0", auth.hold);
        assert_eq!(auth.hold_path(&key), Some(Ok("/_podrunner/hold/build.0".to_string())));
        assert_eq!(auth.hold_path("/_podrunner/hold/build.0"), Some(Err(())));
        assert_eq!(auth.hold_path("/_podrunner/hold/wrong/build.0"), Some(Err(())));
        assert_eq!(auth.hold_path("/message"), None);
        assert_eq!(
            need("POST", "/twirp/github.actions.results.api.v1.CacheService/GetCacheEntryDownloadURL"),
            Some(Need::Either)
        );
        assert!(auth.blob_sig(&auth.blob) && !auth.blob_sig("fake") && !auth.blob_sig(""));
    }
}
