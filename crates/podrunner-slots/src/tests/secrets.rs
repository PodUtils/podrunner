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

/// The bytes of a made-up trust store, with a NUL and a byte that is not
/// UTF-8 (T-577).
const STORE: [u8; 4] = [0x30, 0x82, 0x00, 0xff];

/// A made-up value of `JAVA_TOOL_OPTIONS`: the real one is
/// `pod_broker::ca::java_options`, behind the seam.
fn java(store: &std::path::Path, _: Option<&str>) -> Result<String, String> {
    Ok(format!("-Dstore={}", store.display()))
}

/// The policy and the secrets (name, value) of each unit.
type Seen = Arc<Mutex<Vec<(String, Vec<(String, String)>)>>>;

/// A module with a TLS end: it keeps the policy and the secrets of each
/// unit, and gives a made-up bundle and trust store to a policy with a
/// credential.
#[derive(Clone, Default)]
struct Tls {
    opened: Seen,
    /// The value of `JAVA_TOOL_OPTIONS` fails: a path that a JVM cannot take.
    no_java: bool,
}

fn no_java(_: &std::path::Path, _: Option<&str>) -> Result<String, String> {
    Err("a made-up refusal".into())
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
        let options: crate::JavaOptions = if self.no_java { no_java } else { java };
        let trust_store = ca.as_ref().map(|_| (STORE.to_vec(), options));
        let ca_vars = vec!["SSL_CERT_FILE".into(), "CARGO_HTTP_CAINFO".into()];
        Ok(Opened { flagged: 0, ca_bundle: ca, ca_vars, trust_store })
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
    with_secret_as(dir, r#"{ schema = 1, mode = "open" }"#, "sources = [\"a\"]")
}

/// [`with_secret`] with the ceiling `ceiling` and the keys `keys` of the
/// secret after its file and its hosts.
fn with_secret_as(dir: &std::path::Path, ceiling: &str, keys: &str) -> crate::config::Config {
    let file = dir.join("token");
    std::fs::write(&file, format!("{VALUE}\n")).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    config(&format!(
        "[egress]\ndefault_policy = {ceiling}\n\
         [egress.secrets.token]\nfile = \"{}\"\nhosts = [\"*.example.com\"]\n{keys}",
        file.display()
    ))
}

fn run(offer_with: Value, check: &str) -> (Bed, Tls) {
    let dir = TempDir::new();
    let cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    let (bed, tls) = run_in(with_secret(&dir.0), cap, offer_with, check);
    drop(dir);
    (bed, tls)
}

fn run_in(cfg: crate::config::Config, cap: podrunner_schema::Capability, offer_with: Value, check: &str) -> (Bed, Tls) {
    run_with(cfg, cap, offer_with, check, Tls::default())
}

fn run_with(
    cfg: crate::config::Config,
    cap: podrunner_schema::Capability,
    offer_with: Value,
    check: &str,
    tls: Tls,
) -> (Bed, Tls) {
    let dir = TempDir::new();
    let kind = KindSpec { name: "fake", slots: 1, capability: cap, command: sh(check) };
    let bed = Bed::new(cfg, vec![("a", "trusted", None, vec![offer(0, "j-1", offer_with)])], vec![kind]);
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

/// T-577: the unit gets the trust store of the bundle as a binary
/// credential file beside it, and `JAVA_TOOL_OPTIONS` names it. A path that
/// a JVM cannot take gives no store and no variable; the bundle stays.
#[test]
fn the_unit_gets_the_trust_store_of_its_broker_for_a_jvm() {
    let check = "d=$(dirname \"$SSL_CERT_FILE\"); s=\"$d/podrunner-broker-ca.p12\"; \
         test \"$JAVA_TOOL_OPTIONS\" = \"-Dstore=$s\" && test \"$(od -An -tx1 \"$s\" | tr -d ' \\n')\" = 308200ff \
         && test \"$(stat -c %a \"$s\")\" = 600";
    let (bed, _) = run(credential("token", "api.example.com"), check);
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::Success, "{:?}", bed.slots.reports[0]);
    let check = "grep -q made-up-ca-of-the-job \"$SSL_CERT_FILE\" && test -z \"${JAVA_TOOL_OPTIONS-}\" \
         && ! test -e \"$(dirname \"$SSL_CERT_FILE\")/podrunner-broker-ca.p12\"";
    let dir = TempDir::new();
    let cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    let tls = Tls { no_java: true, ..Tls::default() };
    let (bed, _) = run_with(with_secret(&dir.0), cap, credential("token", "api.example.com"), check, tls);
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::Success, "{:?}", bed.slots.reports[0]);
    // No credential: no store.
    let plain = json!({ "policy": { "schema": 1, "mode": "allow", "allow": [{ "host": "api.example.com" }] } });
    let (bed, _) = run(plain, "test -z \"${JAVA_TOOL_OPTIONS-}\"");
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::Success);
}

/// The CA bundle and the variables that each open of the hooks got.
type HookCa = Arc<Mutex<Vec<Option<(String, String, Vec<String>)>>>>;

/// Hooks that keep what each unit gives its containers (T-578).
#[derive(Clone, Default)]
struct Hooks(HookCa);

impl crate::hooks::HookPort for Hooks {
    fn open(
        &self,
        _: &str,
        _: Arc<dyn podrunner_unit::UnitDriver>,
        _: &podrunner_schema::UnitId,
        _: &std::path::Path,
        _: Option<&str>,
        ca: Option<crate::hooks::ContainerCa<'_>>,
    ) -> Result<std::collections::BTreeMap<String, String>, String> {
        self.0.lock().unwrap().push(ca.map(|c| (c.file.to_string(), c.pem.to_string(), c.vars.to_vec())));
        Ok(Default::default())
    }
    fn close(&self, _: &podrunner_schema::UnitId) {}
    fn take(&self, _: &podrunner_schema::UnitId) -> (Vec<RefusalCode>, Vec<crate::hooks::HookEvent>) {
        Default::default()
    }
}

/// T-578: the containers of a unit whose broker ends TLS get the bundle of
/// that broker and the variables that name it; a unit with no bundle
/// gives its containers neither.
#[test]
fn the_containers_of_a_unit_get_the_bundle_of_its_broker() {
    let plain = json!({ "policy": { "schema": 1, "mode": "allow", "allow": [{ "host": "api.example.com" }] } });
    let vars = ["SSL_CERT_FILE", "CARGO_HTTP_CAINFO"].map(String::from).to_vec();
    for (with, want) in [
        (credential("token", "api.example.com"), Some((crate::secrets::CA_FILE.to_string(), BUNDLE.to_string(), vars))),
        (plain, None),
    ] {
        let dir = TempDir::new();
        let mut cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
        cap.container = podrunner_schema::ContainerRoute::Hooks;
        let kind = KindSpec { name: "fake", slots: 1, capability: cap, command: sh("exit 0") };
        let bed = Bed::new(with_secret(&dir.0), vec![("a", "trusted", None, vec![offer(0, "j-1", with)])], vec![kind]);
        let mut bed = Bed { slots: bed.slots.with_egress(Box::new(Tls::default())), ..bed };
        let hooks = Hooks::default();
        bed.slots.set_hooks(Box::new(hooks.clone()));
        bed.until(30, |s| s.jobs().count() == 1 && Bed::all_final(s));
        assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::Success, "{:?}", bed.slots.reports[0]);
        assert_eq!(*hooks.0.lock().unwrap(), [want]);
    }
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

/// The review of T-563 (P2-1): a credential of `default_policy` holds for
/// a job of the sources of its secret only; a job of another source is
/// refused, and the audit names the secret.
#[test]
fn a_credential_of_the_ceiling_is_for_the_sources_of_its_secret_only() {
    let dir = TempDir::new();
    let ceiling = format!(
        r#"{{ schema = 1, mode = "open", credentials = [{{ placeholder = "{PH}", secret = "token", hosts = ["api.example.com"] }}] }}"#
    );
    let cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    let other = with_secret_as(&dir.0, &ceiling, "sources = [\"other\"]");
    let (bed, tls) = run_in(other, cap.clone(), json!({}), "exit 0");
    assert_eq!(bed.slots.reports[0].refusal_code, Some(RefusalCode::EgressPolicyUnsupported));
    assert!(tls.opened.lock().unwrap().is_empty(), "no unit");
    let audit = std::fs::read_to_string(bed.dir.0.join("state/audit.jsonl")).unwrap_or_default();
    assert!(audit.contains("the secret token is not for the source a"), "{audit}");
    let mine = with_secret_as(&dir.0, &ceiling, "sources = [\"a\"]");
    let (bed, tls) = run_in(mine, cap, json!({}), "exit 0");
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::Success);
    assert_eq!(tls.opened.lock().unwrap()[0].1.len(), 1, "the job of the source gets the value");
}

/// The review of T-563 (P2-2): the operator limits the places of a value;
/// an offer that puts it in a body is refused unless `places` names it.
#[test]
fn the_places_of_a_credential_are_the_places_of_its_secret() {
    let dir = TempDir::new();
    let open = r#"{ schema = 1, mode = "open" }"#;
    let mut body = credential("token", "api.example.com");
    body["policy"]["credentials"][0]["in"] = json!(["header", "body"]);
    let cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    let (bed, _) = run_in(with_secret_as(&dir.0, open, "sources = [\"a\"]"), cap.clone(), body.clone(), "exit 0");
    assert_eq!(bed.slots.reports[0].refusal_code, Some(RefusalCode::EgressPolicyUnsupported));
    let audit = std::fs::read_to_string(bed.dir.0.join("state/audit.jsonl")).unwrap_or_default();
    assert!(audit.contains("does not go in the place body"), "{audit}");
    let wide = with_secret_as(&dir.0, open, "sources = [\"a\"]\nplaces = [\"header\", \"body\"]");
    let (bed, _) = run_in(wide, cap, body, "exit 0");
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::Success);
}

/// The review of T-563 (P2-3): a kind whose unit runs as the user of
/// podrunner (a boundary below `confined` and a shared user) gets no job
/// with a credential; a distinct user or a confined unit does.
#[test]
fn a_credential_needs_a_unit_apart_from_the_user_of_podrunner() {
    use crate::placement::{Kind, place};
    let dir = TempDir::new();
    let cfg = with_secret(&dir.0);
    let line: Value = serde_json::from_str(&offer(0, "j-1", credential("token", "api.example.com"))).unwrap();
    let o: podrunner_schema::JobOffer = serde_json::from_value(line["offer"].clone()).unwrap();
    let plain: Value =
        serde_json::from_str(&offer(0, "j-2", json!({ "policy": { "schema": 1, "mode": "open" } }))).unwrap();
    let p: podrunner_schema::JobOffer = serde_json::from_value(plain["offer"].clone()).unwrap();
    for (boundary, principal, placed) in [
        (Boundary::Redirect, Principal::Shared, false),
        (Boundary::Redirect, Principal::Unknown, false),
        (Boundary::Redirect, Principal::Distinct, true),
        (Boundary::Confined, Principal::Shared, true),
    ] {
        let kind = || Kind {
            capability: capability(boundary, principal, EgressHold::Advisory),
            free: 1,
            running: 0,
            repos: vec![],
            root_disk: None,
            init: None,
        };
        let got = place(&o, podrunner_schema::Tier::Local, &[kind()], &cfg, NOW_UNIX);
        assert_eq!(got.is_ok(), placed, "{boundary:?} {principal:?}: {got:?}");
        if !placed {
            assert_eq!(got.err(), Some(RefusalCode::EgressPolicyUnsupported));
        }
        let plain = place(&p, podrunner_schema::Tier::Local, &[kind()], &cfg, NOW_UNIX);
        assert!(plain.is_ok(), "no credential: each kind: {plain:?}");
    }
}

/// The review of T-563 (P3): a file of a secret that is a symbolic link is
/// refused; two placeholders where one holds the other are refused before a
/// unit starts.
#[test]
fn a_linked_secret_file_and_placeholders_that_hold_each_other_are_refused() {
    let dir = TempDir::new();
    let store = with_secret(&dir.0).egress.secrets;
    let link = dir.0.join("link");
    std::os::unix::fs::symlink(dir.0.join("token"), &link).unwrap();
    let mut linked = store.clone();
    linked.get_mut("token").unwrap().file = link;
    let policy = credential("token", "api.example.com")["policy"].to_string();
    let err = crate::secrets::values(&policy, &linked).unwrap_err();
    assert!(!err.contains(VALUE) && err.contains("token"), "{err}");
    assert_eq!(crate::secrets::values(&policy, &store).unwrap().len(), 1);
    let mut two = credential("token", "api.example.com")["policy"].as_object().unwrap().clone();
    let longer = json!({ "placeholder": format!("{PH}_MORE"), "secret": "token", "hosts": ["api.example.com"] });
    two["credentials"].as_array_mut().unwrap().push(longer);
    let err = crate::secrets::check_credentials(&two, &store, Some("a")).unwrap_err();
    assert!(err.contains("hold each other"), "{err}");
}

/// T-480: a variable of `[cache] env` does not take the place of a variable
/// of the CA bundle of the broker: the table takes only cache variables.
#[test]
fn a_variable_of_the_cache_does_not_replace_the_ca_bundle() {
    let dir = TempDir::new();
    let mut cfg = with_secret(&dir.0);
    let table = |env: &str| format!("dir = \"{}\"\nenv = {{ {env} = \"x\" }}", dir.0.join("c").display());
    assert!(toml::from_str::<crate::cache::CacheConfig>(&table("SSL_CERT_FILE")).is_err());
    cfg.cache = Some(toml::from_str(&table("npm_config_cache")).unwrap());
    let cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    let check =
        "grep -q made-up-ca-of-the-job \"$SSL_CERT_FILE\" && test \"$npm_config_cache\" = \"$PODRUNNER_CACHE/x\"";
    let (bed, _) = run_in(cfg, cap, credential("token", "api.example.com"), check);
    assert_eq!(bed.slots.reports[0].exit_reason, ExitReason::Success, "{:?}", bed.slots.reports[0]);
}
