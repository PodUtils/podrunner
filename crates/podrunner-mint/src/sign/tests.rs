//! The signer with a test key that `openssl` makes for each run: no key
//! is in the repository.

use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use pod_core::clock::{Clock, ManualClock};
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

fn signer(clock: &Arc<ManualClock>) -> Signer {
    let key = key_from_pem(&test_key()).unwrap();
    Signer::new(key, "12345", clock.clone(), Box::new(|| NOW))
}

fn claims(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

#[test]
fn a_jwt_of_the_one_shape_verifies() {
    let clock = Arc::new(ManualClock::new());
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
    let clock = Arc::new(ManualClock::new());
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

/// A signer whose Unix time follows `clock`.
fn moving(clock: &Arc<ManualClock>) -> Signer {
    let key = key_from_pem(&test_key()).unwrap();
    let (c, start) = (clock.clone(), clock.now());
    Signer::new(key, "12345", clock.clone(), Box::new(move || NOW + (c.now() - start).as_secs()))
}

#[test]
fn one_signature_a_minute() {
    let clock = Arc::new(ManualClock::new());
    let mut s = moving(&clock);
    // A token with a short life is not given again: the rate holds.
    let short = claims(json!({ "iat": NOW, "exp": NOW + 100, "iss": "12345" }));
    s.sign("a", &short).unwrap();
    clock.advance(Duration::from_secs(59));
    let c = claims(json!({ "iat": NOW + 59, "exp": NOW + 600, "iss": "12345" }));
    assert_eq!(s.sign("a", &c), Err(Refusal::Rate));
    clock.advance(Duration::from_secs(1));
    assert!(s.sign("a", &c).is_ok());
    assert_eq!(s.audit.iter().map(|a| a.outcome).collect::<Vec<_>>(), ["signed", "refused-rate", "signed"]);
}

/// T-165: a restarted sidecar asks again inside the minute; it gets the
/// last token, which is valid for long enough, and no new signature.
#[test]
fn a_second_request_in_the_minute_gets_the_last_token() {
    let clock = Arc::new(ManualClock::new());
    let mut s = moving(&clock);
    let first = s.sign("a", &claims(json!({ "iat": NOW - 30, "exp": NOW + 540, "iss": "12345" }))).unwrap();
    clock.advance(Duration::from_secs(5));
    let again = s.sign("a", &claims(json!({ "iat": NOW - 25, "exp": NOW + 545, "iss": "12345" }))).unwrap();
    assert_eq!(again, first);
    assert_eq!(s.last_exp(), Some(NOW + 540));
    assert_eq!(s.audit.last().unwrap().outcome, "reused");
    // A request of another shape is still refused inside the minute.
    let bad = claims(json!({ "iat": NOW, "exp": NOW + 60, "iss": "999" }));
    assert!(matches!(s.sign("a", &bad), Err(Refusal::Shape(_))));
}

#[test]
fn the_key_and_the_token_reach_no_output() {
    let pem = test_key();
    let body_line = pem.lines().nth(1).unwrap().to_string();
    let clock = Arc::new(ManualClock::new());
    let mut s = Signer::new(key_from_pem(&pem).unwrap(), "12345", clock.clone(), Box::new(|| NOW));
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

/// T-172: each audit line goes to the sink as one JSON line with no token,
/// and the memory keeps the last `AUDIT_KEEP` lines.
#[test]
fn each_audit_line_goes_to_the_sink_and_the_memory_is_bounded() {
    #[derive(Clone, Default)]
    struct Sink(Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for Sink {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let sink = Sink::default();
    let clock = Arc::new(ManualClock::new());
    let mut s = signer(&clock).with_sink(Box::new(sink.clone()));
    let token = s.sign("a", &claims(json!({ "iat": NOW, "exp": NOW + 600, "iss": "12345" }))).unwrap();
    let bad = claims(json!({ "iat": NOW, "exp": NOW + 60, "iss": "999" }));
    for _ in 0..AUDIT_KEEP + 10 {
        let _ = s.sign("a", &bad);
    }
    assert_eq!(s.audit.len(), AUDIT_KEEP);
    let text = String::from_utf8(sink.0.lock().unwrap().clone()).unwrap();
    assert_eq!(text.lines().count(), AUDIT_KEEP + 11);
    assert!(text.lines().next().unwrap().contains("\"outcome\":\"signed\""), "{text}");
    assert!(!text.contains(&token[token.len() - 40..]), "the sink holds the token");
}
