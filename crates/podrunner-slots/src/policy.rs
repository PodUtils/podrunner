//! The egress policy of a job (T-107, githome's G-04): the policy of the
//! offer narrowed by the ceiling of the operator (`[egress]
//! default_policy`). An offer can take away, never add (SPEC §9).
//!
//! The policies are `broker-policy.v1` of `pod-broker`: `schema`, `mode`
//! (`open` or `allow`), `allow` rules (`host`, `path`, `ports`) and extra
//! `deny` entries. The fixed denies of `pod-broker` hold in each mode.
//! The broker of `serve` has its TLS end, so a rule can hold a path, and a
//! policy can name `credentials` (T-524, T-563, `crate::secrets`).

use serde::Deserialize;
use serde_json::{Map, Value, json};

/// Whether a policy has a rule that podrunner cannot hold
/// (`egress-policy-unsupported`): a field other than `host`, `path` and
/// `ports`, or a value of another type (`ports` as text, a `path` that is
/// not absolute), which `narrow` would drop, so the policy would be wider
/// than its rule (#249). The rule of `pod-broker` has these three.
pub fn unheld_rule(policy: &serde_json::Map<String, Value>) -> bool {
    let port = |p: &Value| p.as_u64().is_some_and(|n| (1..=65535).contains(&n));
    rules(&Value::Object(policy.clone())).iter().any(|r| {
        let Some(o) = r.as_object() else { return true };
        o.keys().any(|k| !matches!(k.as_str(), "host" | "path" | "ports"))
            || !o.get("host").is_some_and(Value::is_string)
            || o.get("path").is_some_and(|p| !p.is_null() && !p.as_str().is_some_and(|p| p.starts_with('/')))
            || o.get("ports").is_some_and(|p| !p.is_null() && !p.as_array().is_some_and(|a| a.iter().all(port)))
    })
}

fn rules(p: &Value) -> Vec<Value> {
    p.get("allow").and_then(Value::as_array).cloned().unwrap_or_default()
}

fn open(p: &Value) -> bool {
    p.get("mode").and_then(Value::as_str) == Some("open")
}

/// The host of a rule, in lowercase, with no trailing dot: `name.` and
/// `name` are one host (#286).
fn host(r: &Value) -> String {
    let h = r.get("host").and_then(Value::as_str).unwrap_or("");
    h.strip_suffix('.').unwrap_or(h).to_ascii_lowercase()
}

fn path(r: &Value) -> Option<&str> {
    r.get("path").and_then(Value::as_str)
}

/// Whether the path `p` is under `base` on whole segments, as a path rule
/// of `pod-broker` holds it: `/v1` holds `/v1` and `/v1/x`, not `/v1x`.
fn under(p: &str, base: &str) -> bool {
    let base = base.trim_end_matches('/');
    p == base || p.starts_with(&format!("{base}/"))
}

fn ports(r: &Value) -> Vec<u64> {
    r.get("ports").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default()
}

/// Whether the host rule `ceiling` holds each host of the host rule `offer`:
/// the same host, or a `*.NAME` of the ceiling over a name (or a `*.NAME`)
/// below NAME.
pub(crate) fn covers(ceiling: &str, offer: &str) -> bool {
    if ceiling == offer {
        return true;
    }
    match ceiling.strip_prefix("*.") {
        Some(suffix) => {
            let o = offer.strip_prefix("*.").unwrap_or(offer);
            o.ends_with(&format!(".{suffix}")) || (offer.starts_with("*.") && o == suffix)
        }
        None => false,
    }
}

/// The part of the offer rule `o` that the ceiling rule `c` allows, if any.
fn meet(c: &Value, o: &Value) -> Option<Value> {
    if !covers(&host(c), &host(o)) {
        return None;
    }
    let (cp, op) = (ports(c), ports(o));
    let any = cp.is_empty() && op.is_empty();
    let kept: Vec<u64> = match (cp.is_empty(), op.is_empty()) {
        (true, _) => op,
        (false, true) => cp,
        (false, false) => op.into_iter().filter(|p| cp.contains(p)).collect(),
    };
    if kept.is_empty() && !any {
        return None;
    }
    // The narrower path of the two, when one holds the other.
    let path = match (path(c), path(o)) {
        (None, p) | (p, None) => p,
        (Some(c), Some(o)) if under(o, c) => Some(o),
        (Some(c), Some(o)) if under(c, o) => Some(c),
        _ => return None,
    };
    let mut r = json!({ "host": host(o) });
    if !kept.is_empty() {
        r["ports"] = json!(kept);
    }
    if let Some(p) = path {
        r["path"] = json!(p);
    }
    Some(r)
}

/// The policy of the offer narrowed by the ceiling. The credentials of
/// both hold: placement checked each of the offer against the secrets of
/// the host (`crate::secrets::check_offer`).
pub fn narrow(ceiling: &Value, offer: &Value) -> Value {
    let mut deny: Vec<Value> = Vec::new();
    for d in [ceiling, offer].iter().filter_map(|p| p.get("deny").and_then(Value::as_array)).flatten() {
        if !deny.contains(d) {
            deny.push(d.clone());
        }
    }
    let (mode, allow) = match (open(ceiling), open(offer)) {
        (true, true) => ("open", Vec::new()),
        (true, false) => ("allow", rules(offer)),
        (false, true) => ("allow", rules(ceiling)),
        (false, false) => {
            let mut kept = Vec::new();
            for o in rules(offer) {
                if let Some(r) = rules(ceiling).iter().find_map(|c| meet(c, &o)) {
                    kept.push(r);
                }
            }
            ("allow", kept)
        }
    };
    let mut p = json!({ "schema": 1, "mode": mode });
    if !allow.is_empty() {
        p["allow"] = json!(allow);
    }
    if !deny.is_empty() {
        p["deny"] = json!(deny);
    }
    let mut credentials: Vec<Value> = Vec::new();
    for c in [ceiling, offer].iter().filter_map(|p| p.get("credentials").and_then(Value::as_array)).flatten() {
        if !credentials.contains(c) {
            credentials.push(c.clone());
        }
    }
    if !credentials.is_empty() {
        p["credentials"] = json!(credentials);
    }
    p
}

/// The egress policy of a job: the policy of its offer narrowed by the
/// ceiling `default_policy` of `[egress]` (T-107), else the ceiling; with
/// no ceiling, no way out at all (T-410).
pub fn of_job(offer: Option<&Map<String, Value>>, ceiling: Option<&Map<String, Value>>) -> String {
    let offer = offer.filter(|p| !p.is_empty()).map(|p| Value::Object(p.clone()));
    match (offer, ceiling.map(|p| Value::Object(p.clone()))) {
        (Some(o), Some(d)) => narrow(&d, &o).to_string(),
        (None, Some(p)) => p.to_string(),
        (_, None) => r#"{"schema":1,"mode":"allow","allow":[]}"#.into(),
    }
}

/// `[egress]` (T-111): an unknown key is refused, so a misspelled
/// `default_policy` does not leave the operator with no ceiling.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EgressConfig {
    /// The `broker-policy.v1` of the operator: the policy of a job with no
    /// policy of its own, and the ceiling of the policy of an offer (T-024,
    /// T-107).
    #[serde(default)]
    pub default_policy: Option<Map<String, Value>>,
    /// The secrets of the host that a credential of a policy names, by
    /// name (T-563).
    #[serde(default)]
    pub secrets: std::collections::BTreeMap<String, crate::secrets::SecretConfig>,
}

impl EgressConfig {
    /// The parts of `default_policy` that a host can hold: version 1, a
    /// known mode, rules of a host, a path and ports, and credentials that
    /// name a secret of `secrets`.
    pub(crate) fn check(&self) -> Result<(), String> {
        crate::secrets::check_store(&self.secrets)?;
        let Some(p) = &self.default_policy else { return Ok(()) };
        if p.get("schema").and_then(Value::as_u64) != Some(1) {
            return Err("[egress] default_policy: schema must be 1".into());
        }
        if !matches!(p.get("mode").and_then(Value::as_str), Some("open" | "allow")) {
            return Err("[egress] default_policy: mode must be \"open\" or \"allow\"".into());
        }
        if unheld_rule(p) {
            return Err("[egress] default_policy: a rule holds a host, an absolute path and ports only".into());
        }
        crate::secrets::check_credentials(p, &self.secrets, None).map_err(|e| format!("[egress] default_policy: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allow(rules: Value) -> Value {
        json!({ "schema": 1, "mode": "allow", "allow": rules })
    }

    #[test]
    fn an_offer_takes_away_and_never_adds() {
        let ceiling = allow(json!([{ "host": "*.github.com" }, { "host": "registry.npmjs.org", "ports": [443] }]));
        let offer = allow(json!([
            { "host": "api.github.com" },
            { "host": "evil.example" },
            { "host": "registry.npmjs.org" },
            { "host": "*.github.com", "ports": [443, 22] }
        ]));
        let p = narrow(&ceiling, &offer);
        assert_eq!(
            p["allow"],
            json!([
                { "host": "api.github.com" },
                { "host": "registry.npmjs.org", "ports": [443] },
                { "host": "*.github.com", "ports": [443, 22] }
            ])
        );
        assert_eq!(p["mode"], "allow");
    }

    #[test]
    fn an_open_side_gives_the_rules_of_the_other() {
        let open = json!({ "schema": 1, "mode": "open", "deny": ["10.0.0.0/8"] });
        let ceiling = allow(json!([{ "host": "github.com" }]));
        assert_eq!(narrow(&ceiling, &open)["allow"], json!([{ "host": "github.com" }]));
        assert_eq!(narrow(&ceiling, &open)["deny"], json!(["10.0.0.0/8"]));
        assert_eq!(narrow(&open, &ceiling)["allow"], json!([{ "host": "github.com" }]));
        assert_eq!(narrow(&open, &open)["mode"], "open");
    }

    #[test]
    fn the_denies_of_both_hold_and_no_port_outside_the_ceiling() {
        let ceiling =
            json!({ "schema": 1, "mode": "allow", "allow": [{ "host": "a.example", "ports": [443] }], "deny": ["x"] });
        let offer = json!({ "schema": 1, "mode": "allow", "allow": [{ "host": "a.example", "ports": [80] }], "deny": ["y", "x"] });
        let p = narrow(&ceiling, &offer);
        assert_eq!(p.get("allow"), None, "port 80 is outside the ceiling: {p}");
        assert_eq!(p["deny"], json!(["x", "y"]));
    }

    #[test]
    fn a_wildcard_covers_only_names_below_it() {
        assert!(covers("*.github.com", "api.github.com"));
        assert!(covers("*.github.com", "*.api.github.com"));
        assert!(covers("*.github.com", "*.github.com"));
        assert!(!covers("*.github.com", "github.com"), "whole labels: the name itself is not below it");
        assert!(!covers("*.github.com", "evilgithub.com"));
        assert!(!covers("api.github.com", "*.github.com"));
    }

    /// #286: a host with a trailing dot (an absolute name) is the same host.
    #[test]
    fn a_trailing_dot_names_the_same_host() {
        let ceiling = allow(json!([{ "host": "*.github.com" }, { "host": "example.org." }]));
        let offer = allow(json!([{ "host": "api.github.com." }, { "host": "EXAMPLE.org" }, { "host": "." }]));
        let got = narrow(&ceiling, &offer);
        assert_eq!(got["allow"], json!([{ "host": "api.github.com" }, { "host": "example.org" }]), "{got}");
        // One dot only: `a..` is not `a`.
        let two = narrow(&ceiling, &allow(json!([{ "host": "api.github.com.." }])));
        assert!(two.get("allow").is_none_or(|a| a == &json!([])), "{two}");
    }

    /// A field that `narrow` would drop (#249), or a path that is not
    /// absolute, is a rule that podrunner cannot hold.
    #[test]
    fn a_rule_that_podrunner_cannot_hold_is_found() {
        let unheld = |rule: Value| unheld_rule(allow(json!([rule])).as_object().unwrap());
        for r in [
            json!({ "host": "a", "path": "x" }),
            json!({ "host": "a", "path": ["/x"] }),
            json!({ "host": "a.example", "methods": ["GET"] }),
            json!({ "host": "a", "Ports": [443] }),
            json!("a.example"),
            json!({ "host": "a", "ports": ["443"] }),
            json!({ "host": "a", "ports": 443 }),
            json!({ "host": "a", "ports": [0] }),
            json!({ "host": "a", "ports": [70000] }),
            json!({ "host": 1 }),
            json!({ "ports": [443] }),
        ] {
            assert!(unheld(r.clone()), "{r}");
        }
        for r in [
            json!({ "host": "a" }),
            json!({ "host": "a", "ports": [443] }),
            json!({ "host": "a", "path": null }),
            json!({ "host": "a", "path": "/x" }),
        ] {
            assert!(!unheld(r.clone()), "{r}");
        }
    }

    /// T-524: a path of the offer holds under a path of the ceiling; the
    /// narrower path of the two holds; two paths with no common part give
    /// no rule.
    #[test]
    fn the_narrower_path_holds() {
        let ceiling = allow(json!([{ "host": "a.example", "path": "/v1" }, { "host": "b.example" }]));
        let offer = allow(json!([
            { "host": "a.example", "path": "/v1/repos" },
            { "host": "a.example" },
            { "host": "a.example", "path": "/v1x" },
            { "host": "b.example", "path": "/x" }
        ]));
        let got = narrow(&ceiling, &offer);
        assert_eq!(
            got["allow"],
            json!([
                { "host": "a.example", "path": "/v1/repos" },
                { "host": "a.example", "path": "/v1" },
                { "host": "b.example", "path": "/x" }
            ]),
            "{got}"
        );
    }

    /// T-563: the credentials of the ceiling and of the offer reach the
    /// policy of the job, each once; with no ceiling, none.
    #[test]
    fn the_credentials_of_both_hold() {
        let c = |s: &str| json!({ "placeholder": "PODCRED_TEST_TOKEN_0001", "secret": s, "hosts": ["a.example"] });
        let mut ceiling = json!({ "schema": 1, "mode": "open", "credentials": [c("x")] });
        let offer = json!({ "schema": 1, "mode": "open", "credentials": [c("x"), c("y")] });
        assert_eq!(narrow(&ceiling, &offer)["credentials"], json!([c("x"), c("y")]));
        ceiling.as_object_mut().unwrap().remove("credentials");
        assert_eq!(narrow(&ceiling, &offer)["credentials"], json!([c("x"), c("y")]));
        let none: Value = serde_json::from_str(&of_job(offer.as_object(), None)).unwrap();
        assert_eq!(none.get("credentials"), None, "no ceiling, no way out: {none}");
    }
}
