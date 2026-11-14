//! The claims of the OIDC id-token of a job (T-140). A job may ask for a
//! token when its `permissions` (or those of the workflow) give
//! `id-token: write`, or are `write-all`; as on GitHub, no `permissions`
//! gives no token. The claims are those that GitHub puts in a token, from
//! the `github` context of the run.

use pod_actions_expr::Value as XValue;
use pod_actions_workflow::{Kind, Node};
use serde_json::{Map, Value, json};

/// The issuer of the id-tokens of a run: its key signs each token.
pub use podrunner_broker_fake::oidc::Issuer;

/// The permissions let the job ask for an id-token.
pub fn may_ask(p: Option<&Node>) -> bool {
    match p.map(|n| &n.kind) {
        Some(Kind::String(s)) => s == "write-all",
        Some(Kind::Map(e)) => {
            e.iter().any(|(k, v)| k.name == "id-token" && matches!(&v.kind, Kind::String(s) if s == "write"))
        }
        _ => false,
    }
}

/// The claims of the job `job` of the environment `environment`.
pub fn claims(github: &XValue, job: &str, environment: Option<&str>) -> Map<String, Value> {
    let g = |k: &str| github.as_object().and_then(|o| o.get(k)).and_then(XValue::as_str).unwrap_or("").to_string();
    let (repo, git_ref, event) = (g("repository"), g("ref"), g("event_name"));
    let sub = match (environment, event.as_str()) {
        (Some(env), _) => format!("repo:{repo}:environment:{env}"),
        (None, "pull_request" | "pull_request_target") => format!("repo:{repo}:pull_request"),
        (None, _) => format!("repo:{repo}:ref:{git_ref}"),
    };
    let workflow_ref = format!("{repo}/.github/workflows/{}@{git_ref}", g("workflow"));
    let mut m = Map::new();
    for (k, v) in [
        ("sub", sub),
        ("repository", repo.clone()),
        ("repository_owner", g("repository_owner")),
        ("ref", git_ref),
        ("ref_type", g("ref_type")),
        ("sha", g("sha")),
        ("actor", g("actor")),
        ("event_name", event),
        ("workflow", g("workflow")),
        ("workflow_ref", workflow_ref.clone()),
        ("job_workflow_ref", workflow_ref),
        ("run_id", g("run_id")),
        ("run_number", g("run_number")),
        ("run_attempt", g("run_attempt")),
        ("head_ref", g("head_ref")),
        ("base_ref", g("base_ref")),
        ("job", job.to_string()),
    ] {
        m.insert(k.into(), json!(v));
    }
    if let Some(env) = environment {
        m.insert("environment".into(), json!(env));
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_permissions_and_the_claims_of_an_id_token() {
        let perm = |text: &str| {
            let w = pod_actions_workflow::parse(&format!(
                "on: push\npermissions: {text}\njobs:\n  a:\n    runs-on: x\n    steps: [{{run: x}}]\n"
            ))
            .unwrap()
            .workflow;
            may_ask(w.permissions.as_ref())
        };
        assert!(perm("{id-token: write}") && perm("write-all"));
        assert!(!perm("{id-token: read}") && !perm("read-all") && !perm("{}") && !perm("{contents: write}"));
        assert!(!may_ask(None), "no permissions give no token, as on GitHub");
        let s = |v: &str| XValue::String(v.into());
        let github = XValue::object([
            ("repository", s("me/app")),
            ("repository_owner", s("me")),
            ("ref", s("refs/heads/main")),
            ("event_name", s("push")),
            ("workflow", s("ci.yml")),
        ]);
        let c = claims(&github, "deploy", None);
        assert_eq!(c["sub"], "repo:me/app:ref:refs/heads/main");
        assert_eq!(c["job_workflow_ref"], "me/app/.github/workflows/ci.yml@refs/heads/main");
        assert_eq!(claims(&github, "deploy", Some("prod"))["sub"], "repo:me/app:environment:prod");
    }
}
