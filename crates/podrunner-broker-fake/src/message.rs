//! The canned messages of the fake broker: the job request that the listener
//! long-polls, and the job message of `acquirejob` that the Worker runs. The
//! shapes are the ones that runner v2.338.2 reads (`AgentJobRequestMessage`,
//! camel case); a field that the runner does not need is left out.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};

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
}

impl Job {
    /// A job of plain scripts, one step for each.
    pub fn scripts(name: &str, scripts: &[&str]) -> Job {
        let steps = scripts.iter().enumerate().map(|(i, s)| Step {
            name: format!("Run step {}", i + 1),
            script: s.to_string(),
            condition: String::new(),
            env: Vec::new(),
        });
        Job { name: name.into(), env: Vec::new(), steps: steps.collect() }
    }
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

/// The job message of job `request`.
pub fn job_message(base: &str, request: u64, job: &Job) -> Value {
    let steps: Vec<Value> = job
        .steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let condition = match step.condition.as_str() {
                "" => "success()".to_string(),
                c => format!("success() && ({c})"),
            };
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
    json!({
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
                "data": { "ServerId": guid(5), "ServerName": "podrunner-broker-fake" },
            }],
            "repositories": [],
        },
        "workspace": {},
        "environmentVariables": if job.env.is_empty() { json!([]) } else { json!([mapping(&job.env)]) },
        // The results service of the Worker (logs, step updates): this broker.
        "variables": {
            "system.github.job": { "value": job.name },
            "system.github.results_endpoint": { "value": base },
        },
        "contextData": { "github": { "t": 2, "d": [
            s("repository", "example-org/app"), s("workflow", "podrunner"), s("job", &job.name),
            s("run_id", &request.to_string()), s("event_name", "push"),
            s("server_url", "https://github.com"), s("api_url", base),
        ] } },
        "steps": steps,
        "fileTable": [], "mask": [],
    })
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
        // The job token is a JWT with three parts and an expiry.
        let token = job_token();
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        let claims: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert!(claims["exp"].as_u64().unwrap() > 4_000_000_000);
    }
}
