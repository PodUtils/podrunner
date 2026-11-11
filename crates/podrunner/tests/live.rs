//! The gates G-H2 to G-H4 (T-071, SPEC §5 and §6.2) in their offline form,
//! on the fake scale-set service (`tools/fake-scaleset`) with the real
//! sidecar: the same calls as a live run, with a record of each request.
//! A live run against github.com needs DR-7 and the live workflow of T-032.
//!
//! - G-H2: the sidecar creates its scale set with one label, its name.
//! - G-H3: the sidecar calls only the routes of the minimum permissions of
//!   SPEC §6.2 (a check with a planted route).
//! - G-H4: a fork job on the trusted set is withdrawn and never acquired:
//!   `tests/scaleset.rs` (`a_fork_is_withdrawn_and_a_member_job_runs`).
//!
//! Ignored by default: it needs the Go binaries.
//!
//!   PODRUNNER_FAKE_SCALESET=PATH PODRUNNER_SCALESET_SIDECAR=PATH \
//!   cargo test -p podrunner --test live -- --ignored

use std::io::BufRead;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use podrunner_clock::ManualClock;
use podrunner_mint::SidecarSource;
use podrunner_mint::sign::{Signer, key_from_pem};
use podrunner_source::JobSource;
use serde_json::Value;

const SCRIPT: &str = r#"{"scaleSetName":"podrunner","jobs":[
 {"requestId":11,"jobId":"a","owner":"example-org","repo":"app","workflowRunId":101,"event":"push","actor":"alice","actorIsMember":true,"visibility":"private","labels":["podrunner"]}]}"#;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

/// The routes that the minimum permissions of SPEC §6.2 cover: the token
/// of the installation (the App JWT), the runner registration and the
/// scale-set service with its message queue ("Self-hosted runners: read and
/// write"), the run
/// object ("Actions: read"), the member check ("Members: read"), and the
/// organization record (public metadata).
fn route_allowed(path: &str) -> bool {
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    match parts.as_slice() {
        ["app", "installations", _, "access_tokens"] => true,
        [.., "actions", "runner-registration"] | [.., "runners", "registration-token"] => true,
        // The scale-set service and its message queue (the token of its
        // session).
        ["tenant", ..] | ["queue", ..] => true,
        ["repos", _, _, "actions", "runs", _] => true,
        ["orgs", _, "members", _] => true,
        ["orgs", _] => true,
        _ => false,
    }
}

/// The paths of the record that no minimum permission covers.
fn beyond(record: &[Value]) -> Vec<String> {
    let path = |l: &Value| {
        let p = l["path"].as_str().unwrap_or("");
        p.strip_prefix("/api/v3").unwrap_or(p).to_string()
    };
    record.iter().map(path).filter(|p| !route_allowed(p)).collect()
}

#[test]
fn g_h3_the_check_names_a_route_beyond_the_minimum() {
    let ok = serde_json::json!({ "path": "/api/v3/repos/o/r/actions/runs/1" });
    let planted = serde_json::json!({ "path": "/api/v3/repos/o/r/contents/README.md" });
    assert!(beyond(std::slice::from_ref(&ok)).is_empty());
    assert_eq!(beyond(&[ok, planted]), ["/repos/o/r/contents/README.md"]);
}

#[test]
#[ignore]
fn gates_g_h2_and_g_h3_offline() {
    let dir = std::env::temp_dir().join(format!("podrunner-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("script.json"), SCRIPT).unwrap();
    let record = dir.join("record.jsonl");
    let mut fake = Command::new(env("PODRUNNER_FAKE_SCALESET"))
        .args(["-listen", "127.0.0.1:0", "-script"])
        .arg(dir.join("script.json"))
        .arg("-record")
        .arg(&record)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut url = String::new();
    std::io::BufReader::new(fake.stdout.take().unwrap()).read_line(&mut url).unwrap();
    let url = url.trim().to_string();

    let pem = Command::new("openssl").args(["genrsa", "-traditional", "2048"]).output().unwrap().stdout;
    let key = key_from_pem(&String::from_utf8(pem).unwrap()).unwrap();
    let unix = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let signer = Arc::new(Mutex::new(Signer::new(key, "4242", Arc::new(ManualClock::default()), Box::new(unix))));
    let args = [
        "-config-url",
        &format!("{url}/example-org"),
        "-api-url",
        &format!("{url}/api/v3"),
        "-installation-id",
        "7",
        "-scale-set",
        "podrunner",
        "-source",
        "org-trusted",
        "-arch",
        "x86_64",
        "-app-id",
        "4242",
    ]
    .map(String::from)
    .to_vec();
    let mut sidecar = SidecarSource::new(&env("PODRUNNER_SCALESET_SIDECAR"), args, signer, "sidecar:org-trusted");
    sidecar.hello("podrunner", "0").unwrap();
    let mut offers = 0;
    for _ in 0..10 {
        offers += sidecar.next(1, Duration::from_secs(2)).unwrap().len();
        if offers > 0 {
            break;
        }
    }
    drop(sidecar);
    let _ = fake.kill();
    let _ = fake.wait();
    assert!(offers > 0, "no offer came");

    let lines: Vec<Value> =
        std::fs::read_to_string(&record).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    // G-H2: the scale set has one label, its name.
    let created: Vec<&Value> = lines
        .iter()
        .filter(|l| l["method"] == "POST" && l["path"].as_str().is_some_and(|p| p.ends_with("/runnerscalesets")))
        .collect();
    assert_eq!(created.len(), 1, "one creation of the scale set");
    let labels: Vec<&str> =
        created[0]["body"]["labels"].as_array().unwrap().iter().filter_map(|l| l["name"].as_str()).collect();
    assert_eq!(labels, ["podrunner"]);
    // G-H3: each call is a route of the minimum permissions.
    assert_eq!(beyond(&lines), Vec::<String>::new());
    let _ = std::fs::remove_dir_all(&dir);
}
