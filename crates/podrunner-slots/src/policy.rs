//! The egress policy of a job (T-107, githome's G-04): the policy of the
//! offer narrowed by the ceiling of the operator (`[egress]
//! default_policy`). An offer can take away, never add (SPEC §9).
//!
//! The policies are `broker-policy.v1` of `pod-broker`: `schema`, `mode`
//! (`open` or `allow`), `allow` rules (`host`, `path`, `ports`) and extra
//! `deny` entries. The fixed denies of `pod-broker` hold in each mode.

use serde_json::{Value, json};

/// Whether a policy has a rule with a path: podrunner has no TLS end, so it
/// cannot hold one (`egress-policy-unsupported`).
pub fn has_path_rule(policy: &serde_json::Map<String, Value>) -> bool {
    rules(&Value::Object(policy.clone())).iter().any(|r| r.get("path").is_some_and(|p| !p.is_null()))
}

fn rules(p: &Value) -> Vec<Value> {
    p.get("allow").and_then(Value::as_array).cloned().unwrap_or_default()
}

fn open(p: &Value) -> bool {
    p.get("mode").and_then(Value::as_str) == Some("open")
}

fn host(r: &Value) -> String {
    r.get("host").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase()
}

fn ports(r: &Value) -> Vec<u64> {
    r.get("ports").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default()
}

/// Whether the host rule `ceiling` holds each host of the host rule `offer`:
/// the same host, or a `*.NAME` of the ceiling over a name (or a `*.NAME`)
/// below NAME.
fn covers(ceiling: &str, offer: &str) -> bool {
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
    let mut r = json!({ "host": host(o) });
    if !kept.is_empty() {
        r["ports"] = json!(kept);
    }
    Some(r)
}

/// The policy of the offer narrowed by the ceiling.
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
    p
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

    #[test]
    fn a_path_rule_is_found() {
        let p = json!({ "schema": 1, "mode": "allow", "allow": [{ "host": "a", "path": "/x" }] });
        assert!(has_path_rule(p.as_object().unwrap()));
        assert!(!has_path_rule(allow(json!([{ "host": "a" }])).as_object().unwrap()));
    }
}
