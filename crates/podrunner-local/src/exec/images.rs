//! The images of a job message and their registries.

use crate::Job;

/// The images of a job: `container:`, each service and each Docker step.
pub(super) fn images(message: &Job) -> Vec<String> {
    use serde_json::Value;
    // A container token: the image itself, or the `image` of a mapping.
    fn of(v: &Value) -> Option<String> {
        match v {
            Value::String(s) => Some(s.clone()),
            Value::Object(_) => {
                v["map"].as_array()?.iter().find(|e| e["Key"] == "image")?["Value"].as_str().map(str::to_string)
            }
            _ => None,
        }
    }
    let o = &message.overrides;
    let mut all: Vec<String> = o.get("jobContainer").and_then(of).into_iter().collect();
    let services = o.get("jobServiceContainers").and_then(|s| s["map"].as_array().cloned()).unwrap_or_default();
    all.extend(services.iter().filter_map(|e| of(&e["Value"])));
    let steps = o.get("steps").and_then(|s| s.as_array().cloned()).unwrap_or_default();
    all.extend(
        steps
            .iter()
            .filter(|s| s["reference"]["type"] == "containerRegistry")
            .filter_map(|s| s["reference"]["image"].as_str().map(str::to_string)),
    );
    all
}

/// The registry of each image of the job: the first part of its name when
/// it names a host, else `docker.io`.
pub(super) fn registries(message: &Job) -> Vec<String> {
    let mut hosts: Vec<String> = images(message)
        .iter()
        .map(|i| match i.split_once('/') {
            Some((h, _)) if h.contains('.') || h.contains(':') || h == "localhost" => h.to_string(),
            _ => "docker.io".to_string(),
        })
        .collect();
    hosts.sort();
    hosts.dedup();
    hosts
}
