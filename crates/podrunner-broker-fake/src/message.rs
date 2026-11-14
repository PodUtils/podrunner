//! The canned messages of the fake broker: the job request that the listener
//! long-polls, and the job message of `acquirejob` that the Worker runs. The
//! shapes are the ones that runner v2.338.2 reads (`AgentJobRequestMessage`,
//! camel case); a field that the runner does not need is left out.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use pod_actions_expr::Condition;
use serde_json::{Map, Value, json};

/// One `run:` step of a job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub name: String,
    pub script: String,
    /// The `if:` of the step, or empty. The runner evaluates it.
    pub condition: String,
    /// The `env:` of the step.
    pub env: Vec<(String, String)>,
}

/// A job: its name, its environment and its `run:` steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub name: String,
    /// The `env:` of the workflow and of the job.
    pub env: Vec<(String, String)>,
    pub steps: Vec<Step>,
    /// The image of `container:`, or none: the runner then runs the job
    /// through its container hooks.
    pub container: Option<String>,
    /// The `credentials` of `container:` (a user name and a password).
    pub registry: Option<(String, String)>,
    /// Keys of the message that replace the canned ones (`steps`,
    /// `environmentVariables`, `jobOutputs`), and the entries of
    /// `contextData` that join the canned `github` (T-088).
    pub overrides: Map<String, Value>,
    /// The secrets of the job: secret variables of the message, which the
    /// runner gives as `secrets` and masks (T-121).
    pub secrets: Vec<(String, String)>,
    /// `github.token` and `secrets.GITHUB_TOKEN`; [`TOKEN`] when none
    /// (T-122).
    pub token: Option<String>,
}

/// The made-up token of `github.token` (T-122): a call with it to a real
/// forge fails.
pub const TOKEN: &str = "podrunner-local-made-up-token";

impl Job {
    /// A job of plain scripts, one step for each.
    pub fn scripts(name: &str, scripts: &[&str]) -> Job {
        let steps = scripts.iter().enumerate().map(|(i, s)| Step {
            name: format!("Run step {}", i + 1),
            script: s.to_string(),
            condition: String::new(),
            env: Vec::new(),
        });
        Job {
            name: name.into(),
            env: Vec::new(),
            steps: steps.collect(),
            container: None,
            registry: None,
            overrides: Map::new(),
            secrets: Vec::new(),
            token: None,
        }
    }
}

/// The `jobContainer` token: the image alone, or a mapping with the
/// credentials of the registry.
fn container(job: &Job) -> Value {
    let Some(image) = &job.container else { return Value::Null };
    let Some((user, password)) = &job.registry else { return json!(image) };
    let credentials = mapping(&[("username".into(), user.clone()), ("password".into(), password.clone())]);
    json!({ "type": 2, "map": [
        { "Key": "image", "Value": image },
        { "Key": "credentials", "Value": credentials },
    ] })
}

/// A mapping token of the runner (`TemplateToken` type 2).
fn mapping(pairs: &[(String, String)]) -> Value {
    let map: Vec<Value> = pairs.iter().map(|(k, v)| json!({ "Key": k, "Value": v })).collect();
    json!({ "type": 2, "map": map })
}

fn b64(v: &Value) -> String {
    URL_SAFE_NO_PAD.encode(v.to_string())
}

/// A made-up job token: the runner reads its claims and checks no signature.
pub fn job_token() -> String {
    let head = json!({ "typ": "JWT", "alg": "RS256" });
    let claims = json!({
        "nameid": "dddddddd-dddd-dddd-dddd-dddddddddddd",
        "scp": "Actions.GenericRead:00000000-0000-0000-0000-000000000000 Actions.Results:1:2",
        "iat": 1_700_000_000u64, "nbf": 1_700_000_000u64, "exp": 4_102_444_800u64,
        "iss": "podrunner-broker-fake", "aud": "podrunner-broker-fake",
    });
    format!("{}.{}.c2lnbmF0dXJl", b64(&head), b64(&claims))
}

fn guid(n: u64) -> String {
    format!("00000000-0000-0000-0000-{n:012x}")
}

/// The body of a broker message: a reference to job `request` on the run
/// service at `base`.
pub fn job_request(base: &str, request: u64) -> Value {
    let reference = json!({
        "runner_request_id": request.to_string(), "run_service_url": base,
        "billing_owner_id": "podrunner", "should_acknowledge": false,
    });
    json!({ "messageId": request, "messageType": "RunnerJobRequest", "body": reference.to_string() })
}

/// The condition of a step as the runner evaluates it: `success()` when
/// none; GitHub adds `success() &&` only to a condition with no status
/// function (`always()`, `failure()`, `cancelled()`, `success()`).
pub fn step_condition(condition: &str) -> String {
    if condition.trim().is_empty() {
        return "success()".into();
    }
    match Condition::parse(condition) {
        Ok(c) => c.text().to_string(),
        Err(_) => format!("success() && ({condition})"),
    }
}

/// The job message of job `request`.
pub fn job_message(base: &str, request: u64, job: &Job) -> Value {
    let steps: Vec<Value> = job
        .steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let condition = step_condition(&step.condition);
            let mut v = json!({
                "type": "action", "reference": { "type": "script" },
                "id": guid(0x100 + i as u64), "name": format!("__run_{i}"), "contextName": format!("__run_{i}"),
                "displayName": step.name, "condition": condition,
                "inputs": mapping(&[("script".into(), step.script.clone())]),
            });
            if !step.env.is_empty() {
                v["environment"] = mapping(&step.env);
            }
            v
        })
        .collect();
    let s = |k: &str, v: &str| json!({ "k": k, "v": v });
    let mut m = json!({
        "messageType": "RunnerJobRequest",
        "plan": { "scopeIdentifier": guid(1), "planType": "actions", "version": 1, "planId": guid(2 + 0x10 * request) },
        "timeline": { "id": guid(3 + 0x10 * request), "changeId": 1 },
        "jobId": guid(4 + 0x10 * request),
        "jobDisplayName": job.name, "jobName": job.name, "requestId": request,
        "lockedUntil": "2099-01-01T00:00:00Z",
        "resources": {
            "endpoints": [{
                "name": "SystemVssConnection", "url": base, "isReady": true,
                "authorization": { "scheme": "OAuth", "parameters": { "AccessToken": job_token() } },
                // ResultsServiceUrl gives ACTIONS_RESULTS_URL to the steps (T-119).
                "data": { "ServerId": guid(5), "ServerName": "podrunner-broker-fake", "ResultsServiceUrl": base },
            }],
            "repositories": [],
        },
        "workspace": {},
        "jobContainer": container(job),
        "environmentVariables": if job.env.is_empty() { json!([]) } else { json!([mapping(&job.env)]) },
        // The results service of the Worker (logs, step updates): this broker.
        "variables": {
            "system.github.job": { "value": job.name },
            "system.github.results_endpoint": { "value": base },
            // The launch service that resolves the actions of the job (T-091).
            "system.github.launch_endpoint": { "value": base },
            // The runner takes the container hooks only with this flag
            // (`FeatureManager.IsContainerHooksEnabled`), and the variable
            // `ACTIONS_RUNNER_CONTAINER_HOOKS`.
            "DistributedTask.AllowRunnerContainerHooks": { "value": "true" },
            // The runner gives ACTIONS_CACHE_SERVICE_V2 to the steps: the
            // cache client calls the CacheService of this broker (T-120).
            "actions_uses_cache_service_v2": { "value": "true" },
            "system.github.token": { "value": job.token.as_deref().unwrap_or(TOKEN), "isSecret": true },
            // The runner does not derive `secrets.GITHUB_TOKEN` from the
            // token above: it is a secret variable of its own.
            "GITHUB_TOKEN": { "value": job.token.as_deref().unwrap_or(TOKEN), "isSecret": true },
        },
        "contextData": { "github": { "t": 2, "d": [
            s("repository", "example-org/app"), s("workflow", "podrunner"), s("job", &job.name),
            s("run_id", &request.to_string()), s("event_name", "push"),
            s("server_url", "https://github.com"), s("api_url", base),
        ] } },
        "steps": steps,
        // GitHub masks each secret of a job; the password of the registry
        // comes from a secret.
        "fileTable": [], "mask": job.registry.iter().map(|(_, p)| json!({ "type": "regex", "value": p })).collect::<Vec<_>>(),
    });
    for (key, value) in &job.overrides {
        match (key.as_str(), value) {
            ("contextData", Value::Object(extra)) => {
                let data = m["contextData"].as_object_mut().expect("the context data");
                for (name, v) in extra {
                    match (name.as_str(), data.get_mut(name)) {
                        // The entries of `github` join the canned ones, one
                        // key at a time: `api_url` and `job` of this broker
                        // stay (T-114).
                        ("github", Some(canned)) => merge_dictionary(canned, v),
                        _ => {
                            data.insert(name.clone(), v.clone());
                        }
                    }
                }
            }
            _ => {
                m[key] = value.clone();
            }
        }
    }
    for (name, value) in &job.secrets {
        m["variables"][name] = json!({ "value": value, "isSecret": true });
    }
    m
}

/// Put each entry of the context dictionary `extra` (`{"t":2,"d":[{"k","v"}]}`)
/// into `into`, over an entry of the same key.
fn merge_dictionary(into: &mut Value, extra: &Value) {
    let (Some(have), Some(new)) = (into["d"].as_array_mut(), extra["d"].as_array()) else {
        *into = extra.clone();
        return;
    };
    for e in new {
        match have.iter_mut().find(|h| h["k"] == e["k"]) {
            Some(h) => *h = e.clone(),
            None => have.push(e.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_messages_have_the_shapes_of_the_runner() {
        let r = job_request("http://127.0.0.1:1/", 3);
        assert_eq!(r["messageType"], "RunnerJobRequest");
        let body: Value = serde_json::from_str(r["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["runner_request_id"], "3");
        let mut job = Job::scripts("j", &["true", "false"]);
        job.env.push(("A".into(), "1".into()));
        job.steps[1].condition = "github.event_name == 'push'".into();
        let m = job_message("http://127.0.0.1:1/", 3, &job);
        // The run service path of the runner needs this type (MessageUtil).
        assert_eq!(m["messageType"], "RunnerJobRequest");
        assert_eq!(m["steps"].as_array().unwrap().len(), 2);
        assert_eq!(m["steps"][1]["inputs"]["map"][0]["Value"], "false");
        assert_eq!(m["steps"][0]["condition"], "success()");
        assert_eq!(m["steps"][1]["condition"], "success() && (github.event_name == 'push')");
        assert_eq!(m["environmentVariables"][0]["map"][0]["Key"], "A");
        assert!(m["jobContainer"].is_null());
        // The job token is a JWT with three parts and an expiry.
        let token = job_token();
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        let claims: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert!(claims["exp"].as_u64().unwrap() > 4_000_000_000);
    }

    /// A status function keeps its condition: `always()` runs after a
    /// failure, as on GitHub.
    #[test]
    fn a_status_function_gets_no_implicit_success() {
        assert_eq!(step_condition(""), "success()");
        assert_eq!(step_condition("github.ref == 'x'"), "success() && (github.ref == 'x')");
        assert_eq!(step_condition("always()"), "always()");
        assert_eq!(step_condition("failure() && env.X"), "failure() && env.X");
    }

    /// The overrides replace a key of the message and join `contextData`.
    #[test]
    fn the_overrides_replace_keys_and_join_the_context_data() {
        let mut job = Job::scripts("j", &["true"]);
        job.overrides.insert("steps".into(), json!([]));
        job.overrides.insert("contextData".into(), json!({ "matrix": { "t": 2, "d": [] } }));
        let m = job_message("http://127.0.0.1:1/", 1, &job);
        assert_eq!(m["steps"], json!([]));
        assert_eq!(m["contextData"]["matrix"]["t"], 2);
        assert_eq!(m["contextData"]["github"]["t"], 2, "the canned github stays");
        let mut j = Job::scripts("j", &["true"]);
        j.overrides.insert(
            "contextData".into(),
            json!({ "github": { "t": 2, "d": [ { "k": "event_name", "v": "pull_request" } ] } }),
        );
        let m = job_message("http://b/", 1, &j);
        let d = m["contextData"]["github"]["d"].as_array().unwrap();
        let get = |k: &str| d.iter().find(|e| e["k"] == k).map(|e| e["v"].clone());
        assert_eq!(get("event_name"), Some(json!("pull_request")), "the run gives its event (T-114)");
        assert_eq!(get("api_url"), Some(json!("http://b/")), "the api_url of the broker stays");
    }
}
