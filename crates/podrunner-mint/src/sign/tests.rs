//! The signer with a test key that `openssl` makes for each run: no key
//! is in the repository.

use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use podrunner_clock::ManualClock;
use ring::signature::{RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};
use serde_json::{Map, Value, json};

use super::*;

const NOW: u64 = 1_800_000_000;

/// A made-up App key in PKCS#1 PEM form, the form that GitHub gives.
fn test_key() -> String {
    let out = Command::new("openssl").args(["genrsa", "-traditional", "2048"]).output().expect("openssl");
    assert!(out.status.success(), "openssl genrsa");
    String::from_utf8(out.stdout).unwrap()
}

fn signer(clock: &ManualClock) -> Signer {
    let key = key_from_pem(&test_key()).unwrap();
    Signer::new(key, "12345", Arc::new(clock.clone()), Box::new(|| NOW))
}

fn claims(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

#[test]
fn a_jwt_of_the_one_shape_verifies() {
    let clock = ManualClock::default();
    let mut s = signer(&clock);
    let token = s.sign("sidecar", &claims(json!({ "iat": NOW - 30, "exp": NOW + 540, "iss": "12345" }))).unwrap();
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3);
    assert_eq!(URL_SAFE_NO_PAD.decode(parts[0]).unwrap(), br#"{"alg":"RS256","typ":"JWT"}"#);
    let sig = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
    let input = format!("{}.{}", parts[0], parts[1]);
    UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, s.public_key())
        .verify(input.as_bytes(), &sig)
        .expect("a verifier accepts it");
    assert_eq!(s.audit.last().unwrap().outcome, "signed");
}

#[test]
fn another_shape_is_refused_and_audited() {
    let clock = ManualClock::default();
    let mut s = signer(&clock);
    let bad = [
        json!({ "iat": NOW, "exp": NOW + 60, "iss": "12345", "sub": "x" }),
        json!({ "iat": NOW, "exp": NOW + 60, "iss": "999" }),
        json!({ "iat": NOW, "exp": NOW + 601, "iss": "12345" }),
        json!({ "iat": NOW - 3600, "exp": NOW - 3000, "iss": "12345" }),
        json!({ "iat": NOW, "exp": NOW, "iss": "12345" }),
        json!({ "iat": "now", "exp": NOW + 60, "iss": "12345" }),
    ];
    for b in bad {
        assert!(matches!(s.sign("x", &claims(b.clone())), Err(Refusal::Shape(_))), "{b}");
    }
    assert_eq!(s.audit.len(), 6);
    assert!(s.audit.iter().all(|a| a.outcome == "refused-shape"));
}

#[test]
fn one_signature_a_minute() {
    let clock = ManualClock::default();
    let mut s = signer(&clock);
    let c = claims(json!({ "iat": NOW, "exp": NOW + 600, "iss": "12345" }));
    s.sign("a", &c).unwrap();
    clock.advance(Duration::from_secs(59));
    assert_eq!(s.sign("a", &c), Err(Refusal::Rate));
    clock.advance(Duration::from_secs(1));
    assert!(s.sign("a", &c).is_ok());
}

#[test]
fn the_key_and_the_token_reach_no_output() {
    let pem = test_key();
    let body_line = pem.lines().nth(1).unwrap().to_string();
    let clock = ManualClock::default();
    let mut s = Signer::new(key_from_pem(&pem).unwrap(), "12345", Arc::new(clock.clone()), Box::new(|| NOW));
    let token = s.sign("a", &claims(json!({ "iat": NOW, "exp": NOW + 600, "iss": "12345" }))).unwrap();
    // A positive control: the token holds the claims.
    assert!(token.len() > 300);
    let shown = format!("{s:?} {:?}", s.audit);
    assert!(!shown.contains(&body_line) && !shown.contains(&token[token.len() - 40..]), "{shown}");
    assert!(matches!(
        key_from_pem("-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----"),
        Err(Refusal::Key(_))
    ));
    let Err(Refusal::Key(why)) =
        key_from_pem("-----BEGIN RSA PRIVATE KEY-----\nnot*base64\n-----END RSA PRIVATE KEY-----")
    else {
        panic!()
    };
    assert!(!why.contains("not*base64"));
}
