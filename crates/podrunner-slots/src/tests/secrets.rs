//! T-563, T-524: a unit holds the placeholder of a credential, never the
//! value; the egress module gets the value from the file of the host and
//! gives the CA bundle of the broker, which the unit trusts through the
//! variables of `ca::env`. An offer that names a secret that is not for
//! its source or its hosts is refused.

use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};

use podrunner_schema::{Boundary, EgressHold, ExitReason, Principal, RefusalCode, Secret};
use serde_json::{Value, json};

use super::*;
use crate::{EgressPort, Opened};

/// A made-up value of a secret.
const VALUE: &str = "made-up-secret-value-T563-slots-c9d0";
const PH: &str = "PODCRED_TEST_TOKEN_0001";
const BUNDLE: &str = "-----BEGIN CERTIFICATE-----\nmade-up-ca-of-the-job\n-----END CERTIFICATE-----\n";

/// The policy and the secrets (name, value) of each unit.
type Seen = Arc<Mutex<Vec<(String, Vec<(String, String)>)>>>;

/// A module with a TLS end: it keeps the policy and the secrets of each
/// unit, and gives a made-up bundle to a policy with a credential.
#[derive(Clone, Default)]
struct Tls {
    opened: Seen,
}

impl EgressPort for Tls {
    fn open(&self, _: &str, _: &str, _: &std::path::Path) -> Result<usize, String> {
        Err("open_with only".into())
    }
    fn open_with(
        &self,
        _: &str,
        policy: &str,
        secrets: Vec<(String, Secret)>,
        _: &std::path::Path,
    ) -> Result<Opened, String> {
        let shown = secrets.iter().map(|(n, v)| (n.clone(), v.expose().to_string())).collect();
        self.opened.lock().unwrap().push((policy.into(), shown));
        let ca = policy.contains("credentials").then(|| Secret::new(BUNDLE));
        Ok(Opened { flagged: 0, ca_bundle: ca, ca_vars: vec!["SSL_CERT_FILE".into(), "CARGO_HTTP_CAINFO".into()] })
    }
    fn close(&self, _: &str) {}
    fn alive(&self) -> bool {
        true
    }
}

fn credential(secret: &str, host: &str) -> Value {
    json!({ "policy": { "schema": 1, "mode": "allow", "allow": [{ "host": host }],
        "credentials": [{ "placeholder": PH, "secret": secret, "hosts": [host] }] } })
}

/// The configuration with an open ceiling and the secret `token` for the
/// source `a` and the hosts under `example.com`, in a file of `dir`.
fn with_secret(dir: &std::path::Path) -> crate::config::Config {
    let file = dir.join("token");
    std::fs::write(&file, format!("{VALUE}\n")).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    config(&format!(
        "[egress]\ndefault_policy = {{ schema = 1, mode = \"open\" }}\n\
         [egress.secrets.token]\nfile = \"{}\"\nhosts = [\"*.example.com\"]\nsources = [\"a\"]",
        file.display()
    ))
}

fn run(offer_with: Value, check: &str) -> (Bed, Tls) {
    let dir = TempDir::new();
    let tls = Tls::default();
    let cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    let kind = KindSpec { name: "fake", slots: 1, capability: cap, command: sh(check) };
    let bed =
        Bed::new(with_secret(&dir.0), vec![("a", "trusted", None, vec![offer(0, "j-1", offer_with)])], vec![kind]);
    let mut bed = Bed { slots: bed.slots.with_egress(Box::new(tls.clone())), ..bed };
    bed.until(30, |s| s.jobs().count() == 1 && Bed::all_final(s));
    drop(dir);
    (bed, tls)
}

#[test]
fn the_unit_holds_the_placeholder_and_trusts_the_ca_of_its_broker() {
    // The unit: the bundle in its credential files, named by each variable,
    // and no value of the secret in its environment or its files.
    let check = format!(
        "grep -q made-up-ca-of-the-job \"$SSL_CERT_FILE\" && test \"$SSL_CERT_FILE\" = \"$CARGO_HTTP_CAINFO\" \
         && ! env | grep -q {VALUE} && ! grep -rq {VALUE} \"$(dirname \"$SSL_CERT_FILE\")\""
    );
    let (bed, tls) = run(credential("token", "api.example.com"), &check);
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::Success, "{:?}", bed.slots.reports[0]);
    let opened = tls.opened.lock().unwrap().clone();
    assert_eq!(opened.len(), 1);
    let (policy, secrets) = &opened[0];
    let p: Value = serde_json::from_str(policy).unwrap();
    assert_eq!(p["credentials"][0]["placeholder"], PH, "the credential reaches the broker: {p}");
    assert!(!policy.contains(VALUE), "the policy names a secret, never its value");
    assert_eq!(*secrets, [("token".to_string(), VALUE.to_string())], "the value of the file, with no end of line");
    let audit = std::fs::read_to_string(bed.dir.0.join("state/audit.jsonl")).unwrap_or_default();
    assert!(!audit.contains(VALUE), "{audit}");
}

#[test]
fn a_job_with_no_credential_gets_no_bundle() {
    let plain = json!({ "policy": { "schema": 1, "mode": "allow", "allow": [{ "host": "api.example.com" }] } });
    let (bed, tls) = run(plain, "test -z \"${SSL_CERT_FILE:-}\"");
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::Success);
    assert!(tls.opened.lock().unwrap()[0].1.is_empty(), "no secret");
}

/// A secret that the host does not hold, that is not for the source, or
/// that does not go to a host of the credential: the offer is refused,
/// and the detail names the secret and never a value.
#[test]
fn an_offer_that_names_a_secret_it_may_not_use_is_refused() {
    for (with, why) in [
        (credential("other", "api.example.com"), "does not hold"),
        (credential("token", "evil.example"), "does not go to evil.example"),
    ] {
        let (bed, tls) = run(with, "exit 0");
        assert_eq!(bed.slots.reports[0].refusal_code, Some(RefusalCode::EgressPolicyUnsupported), "{why}");
        assert!(tls.opened.lock().unwrap().is_empty(), "no unit: {why}");
        let audit = std::fs::read_to_string(bed.dir.0.join("state/audit.jsonl")).unwrap_or_default();
        assert!(audit.contains(why), "{audit}");
        assert!(!audit.contains(VALUE));
    }
    let dir = TempDir::new();
    let store = with_secret(&dir.0).egress.secrets;
    let p = credential("token", "api.example.com")["policy"].as_object().unwrap().clone();
    let err = crate::secrets::check_credentials(&p, &store, Some("b")).unwrap_err();
    assert!(err.contains("not for the source b"), "{err}");
}

/// A file of a secret that the group can read is refused at the start of
/// the unit.
#[test]
fn a_secret_file_open_to_the_group_is_refused() {
    let dir = TempDir::new();
    let store = with_secret(&dir.0).egress.secrets;
    std::fs::set_permissions(dir.0.join("token"), std::fs::Permissions::from_mode(0o640)).unwrap();
    let policy = credential("token", "api.example.com")["policy"].to_string();
    let err = crate::secrets::values(&policy, &store).unwrap_err();
    assert!(err.contains("mode 0600"), "{err}");
}

/// `[egress.secrets]` refuses a relative file, no host and no source; a
/// credential of `default_policy` names a secret of the host.
#[test]
fn the_secrets_of_egress_are_checked() {
    let parse = |extra: &str| {
        crate::config::Config::parse(&format!(
            "host = \"h\"\n[egress]\n{extra}\n[trust]\norganizations = [{{ login = \"o\" }}]\n[pin]\n"
        ))
    };
    let secret = |body: &str| format!("[egress.secrets.s]\n{body}");
    assert!(parse(&secret("file = \"rel\"\nhosts = [\"a\"]\nsources = [\"x\"]")).unwrap_err().contains("absolute"));
    assert!(parse(&secret("file = \"/f\"\nhosts = []\nsources = [\"x\"]")).unwrap_err().contains("hosts"));
    assert!(parse(&secret("file = \"/f\"\nhosts = [\"a\"]\nsources = []")).unwrap_err().contains("sources"));
    assert!(parse(&secret("file = \"/f\"\nhosts = [\"a\"]\nsources = [\"x\"]\nvalue = \"v\"")).is_err(), "no value");
    let ceiling = format!(
        "default_policy = {{ schema = 1, mode = \"open\", credentials = [{{ placeholder = \"{PH}\", secret = \"t\", hosts = [\"a\"] }}] }}"
    );
    assert!(parse(&ceiling).unwrap_err().contains("does not hold"));
}
