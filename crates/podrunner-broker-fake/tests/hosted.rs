//! The runner layer of the hosted snapshot (T-541): a canned job in a
//! unit of the incus kind that boots the init of the runner image of
//! `hosted-runner.yml` (`[unit.incus] init = "image"`, the runner at
//! `/home/runner/actions-runner`, `work_folder = "/home/runner/work"`),
//! through the slots of `serve`, as a job of `ubuntu-latest` goes there.
//! The job sees the hosted work folder, the tool cache and the `.env` of
//! the layer (a JIT runner runs no `config.sh`), and `actions/setup-python`
//! takes the Python of the tool cache with no download. Ignored by default:
//! it needs podvm, incusd and a store with a slice of the runner image that
//! holds the tool cache of one Python (`hosted-boot.yml`).
//!
//!   PODRUNNER_PODVM=PATH PODRUNNER_PODVM_IMAGES=STORE \
//!   PODRUNNER_HOSTED_IMAGE=NAME@sha256:HEX PODRUNNER_HOSTED_PYTHON=3.12 \
//!   PODRUNNER_HOSTED_ACTIONS=DIR PODRUNNER_HOSTED_SETUP_PYTHON=SHA \
//!   [PODRUNNER_HOSTED_IMAGE_OS=ubuntu24] [PODRUNNER_PLANT=work-folder] \
//!   cargo test -p podrunner-broker-fake --test hosted -- --ignored --nocapture
//!
//! DIR holds `actions_setup-python/SHA.tar.gz`, the tarball of
//! `actions/setup-python` at the commit SHA. `PODRUNNER_PLANT=work-folder`
//! gives the kind the old folder `/home/runner/_work`: the test must fail.

mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pod_core::clock::{Clock, SystemClock};
use podrunner_broker_fake::{Broker, Job};
use podrunner_fake::{FakeSource, parse_script};
use podrunner_podvm::Podvm;
use podrunner_schema::{Credential, CredentialFile, ExitReason, JobId, Secret};
use podrunner_slots::config::{Config, SourceConfig};
use podrunner_slots::{Slots, Source, UnitKind};
use podrunner_source::{Assigned, Event, Hello, JobSource, RealJob, SourceError};
use podrunner_unit::UnitDriver;
use serde_json::{Map, Value, json};

/// The name of the broker in a unit.
const NAME: &str = "broker.podrunner.test";
/// The runner of the layer.
const RUN_SH: &str = "/home/runner/actions-runner/run.sh";
/// The tool cache of a hosted runner.
const TOOL_CACHE: &str = "/opt/hostedtoolcache";
/// The job of the test.
const JOB: &str = "hosted-1";

/// The fake source, with the files of the fake broker as its credential.
struct BrokerSource {
    inner: FakeSource,
    broker: Arc<Broker>,
}

impl JobSource for BrokerSource {
    fn hello(&mut self, client: &str, version: &str) -> Result<Hello, SourceError> {
        self.inner.hello(client, version)
    }
    fn next(&mut self, capacity: u32, wait: Duration) -> Result<Vec<Event>, SourceError> {
        self.inner.next(capacity, wait)
    }
    fn ack(&mut self, job: &JobId) -> Result<(), SourceError> {
        self.inner.ack(job)
    }
    fn claim(&mut self, job: &JobId, runner_name: &str) -> Result<(), SourceError> {
        self.inner.claim(job, runner_name)
    }
    fn mint(&mut self, job: &JobId, runner_name: &str) -> Result<Credential, SourceError> {
        self.inner.mint(job, runner_name)?;
        let files = self
            .broker
            .runner_files(runner_name)
            .into_iter()
            .map(|(name, text)| CredentialFile { name, content: Secret::new(text) })
            .collect();
        Ok(Credential { files })
    }
    fn report(&mut self, job: &JobId, report: &Map<String, Value>) -> Result<(), SourceError> {
        self.inner.report(job, report)
    }
    fn audit(&mut self, line: &Map<String, Value>) -> Result<(), SourceError> {
        self.inner.audit(line)
    }
    fn withdraw(&mut self, job: &JobId, reason: &str) -> Result<(), SourceError> {
        self.inner.withdraw(job, reason)
    }
    fn release(&mut self, job: &JobId) -> Result<(), SourceError> {
        self.inner.release(job)
    }
    fn assigned(&mut self, job: &JobId, real: &RealJob) -> Result<Assigned, SourceError> {
        self.inner.assigned(job, real)
    }
}

/// The offer of a job of `ubuntu-latest`: a push of a member to a private
/// repository (trusted), run 1 of `example-org/app`, the run of the jobs of
/// the fake broker, so that the job-started hook of the layer keeps it.
fn offer() -> String {
    let o = json!({
        "schema": 1, "job_id": JOB, "source": "hosted", "labels": ["ubuntu-latest"], "arch": std::env::consts::ARCH,
        "wait_deadline_s": 3600,
        "trust_inputs": {
            "event": "push", "head_is_fork": "no", "actor_class": "member",
            "repo_visibility": "private", "owner": "example-org", "repo": "app", "run_id": 1,
            "ref": "refs/heads/main", "workflow_ref": "w"
        }
    });
    json!({ "at_s": 0, "offer": o }).to_string()
}

/// A step of the job message: `reference` and its `inputs`.
fn step(i: u64, context: &str, reference: Value, inputs: &[(&str, &str)]) -> Value {
    let map: Vec<Value> = inputs.iter().map(|(k, v)| json!({ "Key": k, "Value": v })).collect();
    json!({
        "type": "action", "reference": reference, "id": format!("00000000-0000-0000-0000-{:012x}", 0x100 + i),
        "name": context, "contextName": context, "displayName": context, "condition": "success()",
        "inputs": { "type": 2, "map": map },
    })
}

/// The canned job. Each check prints its value; a marker that the script
/// text does not hold (`$((6*7))`) tells that the step ran to its end.
fn job(python: &str, setup_python: &str) -> Job {
    let script = |i: u64, text: &str| step(i, &format!("__run_{i}"), json!({ "type": "script" }), &[("script", text)]);
    let first = format!(
        "set -eu\n\
         r=${{GITHUB_REPOSITORY#*/}}\n\
         echo \"hosted-workspace=$GITHUB_WORKSPACE\"\n\
         echo \"hosted-pwd=$PWD\"\n\
         echo \"hosted-temp=$RUNNER_TEMP\"\n\
         echo \"hosted-tool-cache=${{RUNNER_TOOL_CACHE:-}}\"\n\
         echo \"hosted-imageos=${{ImageOS:-}}\"\n\
         echo \"hosted-imageversion=${{ImageVersion:-}}\"\n\
         test \"$GITHUB_WORKSPACE\" = \"/home/runner/work/$r/$r\"\n\
         test \"$PWD\" = \"$GITHUB_WORKSPACE\"\n\
         test \"$RUNNER_TEMP\" = /home/runner/work/_temp\n\
         test \"${{RUNNER_TOOL_CACHE:-}}\" = {TOOL_CACHE}\n\
         test -n \"${{ImageOS:-}}\"\n\
         grep -qx \"ImageOS=$ImageOS\" /home/runner/actions-runner/.env\n\
         echo \"hosted-env-ok-$((6*7))\""
    );
    let last = "set -eu\n\
         echo \"hosted-python-location=${pythonLocation:-}\"\n\
         echo \"hosted-python=$(command -v python)\"\n\
         python --version\n\
         case \"${pythonLocation:-}\" in \"$RUNNER_TOOL_CACHE\"/Python/*/x64) ;; *) exit 1 ;; esac\n\
         test \"$(command -v python)\" = \"$pythonLocation/bin/python\"\n\
         echo \"hosted-python-ok-$((6*7))\"";
    let setup = json!({
        "type": "repository", "repositoryType": "GitHub", "name": "actions/setup-python", "ref": setup_python
    });
    let mut j = Job::scripts(JOB, &[]);
    j.run_id = Some(1);
    j.overrides.insert(
        "steps".into(),
        json!([
            script(0, &first),
            step(1, "__actions_setup-python", setup, &[("python-version", python)]),
            script(2, last),
        ]),
    );
    j
}

/// The value of the line `KEY=VALUE` that a step printed, after the time
/// of the log line.
fn printed(logs: &str, key: &str) -> Option<String> {
    let at = format!("{key}=");
    logs.lines().rev().find_map(|l| l.trim_end().split_once(&at).map(|(_, v)| v.to_string()))
}

/// The keys of a host of the runner image of the hosted snapshot
/// (`docs/hosted-snapshot.md`): the incus kind boots the init of the image
/// and runs the runner of the layer; the payload serves `ubuntu-latest`.
fn config(podvm: &str, images: &str, image: &str, work_folder: &str, now: u64) -> Result<Config, String> {
    Config::parse(&format!(
        r#"
host = "host-a"
allow_fake = true
[deadlines]
start_s = 900
[trust]
organizations = [{{ login = "example-org" }}]
[unit.incus]
slots = 1
binary = "{podvm}"
images = "{images}"
init = "image"
command = ["{RUN_SH}"]
work_folder = "{work_folder}"
[pin]
[[pin.payload]]
arch = "{arch}"
reference = "{image}"
built_at_s = {now}
init = "image"
labels = ["ubuntu-latest"]
"#,
        arch = std::env::consts::ARCH,
    ))
}

#[test]
#[ignore]
fn a_job_in_the_runner_layer_sees_the_hosted_paths_and_takes_the_tool_cache() {
    let image = common::env("PODRUNNER_HOSTED_IMAGE");
    let python = common::env("PODRUNNER_HOSTED_PYTHON");
    let setup_python = common::env("PODRUNNER_HOSTED_SETUP_PYTHON");
    let actions = PathBuf::from(common::env("PODRUNNER_HOSTED_ACTIONS"));
    assert!(
        actions.join(format!("actions_setup-python/{setup_python}.tar.gz")).is_file(),
        "no tarball of actions/setup-python at {setup_python} in {}",
        actions.display()
    );
    let image_os = std::env::var("PODRUNNER_HOSTED_IMAGE_OS").ok().filter(|v| !v.is_empty());
    let plant = std::env::var("PODRUNNER_PLANT").unwrap_or_default();
    let work_folder = match plant.as_str() {
        "" | "none" => "/home/runner/work",
        "work-folder" => "/home/runner/_work",
        other => panic!("PODRUNNER_PLANT is none or work-folder, not {other:?}"),
    };
    let state = std::env::temp_dir().join(format!("podrunner-hosted-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    std::fs::create_dir_all(&state).unwrap();
    let now_unix = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();

    let broker = Arc::new(Broker::start_seen_at(NAME).unwrap());
    let port: u16 = broker.base.trim_end_matches('/').rsplit(':').next().unwrap().parse().unwrap();
    broker.serve_actions(&actions, Default::default());
    broker.queue(job(&python, &setup_python));

    let podvm_bin = common::env("PODRUNNER_PODVM");
    let images = common::env("PODRUNNER_PODVM_IMAGES");
    let mut cfg = config(&podvm_bin, &images, &image, work_folder, now_unix()).unwrap();
    let sc: SourceConfig = serde_json::from_value(json!({
        "name": "hosted", "kind": "fake", "tier": "trusted", "arch": std::env::consts::ARCH,
        "operator_controlled": true
    }))
    .unwrap();
    cfg.source.push(sc.clone());
    let unit_cfg = cfg.unit["incus"].clone();

    // The driver as `serve` builds it for `[unit.incus] init = "image"`.
    let podvm = Podvm { binary: PathBuf::from(&podvm_bin) };
    let unit =
        podrunner_incus::IncusUnit::new(podvm, &state.join("incus"), &PathBuf::from(&images)).with_image_init(true);
    let capability = unit.probe().unwrap();
    let driver: Arc<dyn UnitDriver> = Arc::new(unit);
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let source = BrokerSource {
        inner: FakeSource::new("hosted", clock.clone(), parse_script(&offer()).unwrap()),
        broker: broker.clone(),
    };
    let kinds = vec![UnitKind { name: "incus".into(), cfg: unit_cfg, driver, capability }];
    let sources = vec![Source { cfg: sc, source: Box::new(source), revoked: false }];
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut slots = Slots::start(cfg, &state.join("state"), clock, Box::new(now_unix), sources, kinds)
        .unwrap()
        .with_egress(Box::new(common::ToBroker { port, seen: seen.clone() }));
    let end = Instant::now() + Duration::from_secs(1500);
    while Instant::now() < end && slots.reports.is_empty() {
        slots.tick();
        std::thread::sleep(Duration::from_millis(500));
    }
    let reports: Vec<_> = slots.reports.iter().cloned().collect();
    let logs = broker.logs();
    println!("{reports:#?}\nthe broker saw: {:?}\nthe egress saw: {:?}", broker.seen(), seen.lock().unwrap());
    println!("--- the log of the job\n{logs}\n--- the requests with no route: {:?}", broker.missing());
    let _ = std::fs::remove_dir_all(&state);

    for (key, want) in [
        ("hosted-workspace", "/home/runner/work/app/app".to_string()),
        ("hosted-pwd", "/home/runner/work/app/app".into()),
        ("hosted-temp", "/home/runner/work/_temp".into()),
        ("hosted-tool-cache", TOOL_CACHE.into()),
    ] {
        assert_eq!(printed(&logs, key).as_deref(), Some(want.as_str()), "{key}");
    }
    let os = printed(&logs, "hosted-imageos").unwrap_or_default();
    assert!(!os.is_empty(), "the job sees no ImageOS: the .env of the layer did not reach it");
    if let Some(want) = &image_os {
        assert_eq!(&os, want, "ImageOS");
    }
    assert!(logs.contains("hosted-env-ok-42"), "the checks of the first step");
    let location = printed(&logs, "hosted-python-location").unwrap_or_default();
    let prefix = format!("{TOOL_CACHE}/Python/{python}.");
    assert!(location.starts_with(&prefix) && location.ends_with("/x64"), "setup-python: {location:?}, not {prefix}*");
    assert!(logs.contains("hosted-python-ok-42"), "the checks of the last step");
    assert_eq!(reports.len(), 1, "{reports:#?}");
    assert_eq!(reports[0].exit_reason, ExitReason::Success, "{:#?}", reports[0]);
    let done = broker.completions();
    assert_eq!(done.len(), 1, "{done:?}");
    assert_eq!(done[0].conclusion, "succeeded", "{done:?}");
}

/// The file of the test parses: the incus kind boots the init of the image
/// and gives the runner the hosted work folder; the payload needs the init.
#[test]
fn the_file_of_the_hosted_kind_parses() {
    use podrunner_slots::config::UnitInit;
    let image = "hosted@sha256:1111111111111111111111111111111111111111111111111111111111111111";
    let cfg =
        config("/opt/podrunner/drivers/podvm", "/var/lib/podrunner/images", image, "/home/runner/work", 1).unwrap();
    let u = &cfg.unit["incus"];
    assert_eq!(u.init, Some(UnitInit::Image));
    assert_eq!(u.command, [RUN_SH]);
    assert_eq!(u.work_folder_of("incus").as_deref(), Some("/home/runner/work"));
    assert_eq!(cfg.pin.payload[0].init, Some(UnitInit::Image));
    let planted = config("/p", "/i", image, "/home/runner/_work", 1).unwrap();
    assert_eq!(planted.unit["incus"].work_folder_of("incus").as_deref(), Some("/home/runner/_work"));
}

/// The job message of the canned job: two scripts, and setup-python at its
/// commit with the version of the slice.
#[test]
fn the_canned_job_has_its_three_steps() {
    let sha = "6d4fd22b20465ccc0f7354476588e8df7c9ca0e0";
    let m = podrunner_broker_fake::message::job_message("http://127.0.0.1:1/", 1, &job("3.12", sha));
    let steps = m["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 3, "{m:#}");
    assert_eq!(steps[1]["reference"]["name"], "actions/setup-python");
    assert_eq!(steps[1]["reference"]["ref"], sha);
    assert_eq!(steps[1]["inputs"]["map"][0], json!({ "Key": "python-version", "Value": "3.12" }));
    assert_eq!(steps[0]["reference"]["type"], "script");
    assert!(steps[0]["inputs"]["map"][0]["Value"].as_str().unwrap().contains("/home/runner/work/$r/$r"));
    let ids: std::collections::BTreeSet<_> = steps.iter().map(|s| s["id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), 3);
    let github = m["contextData"]["github"]["d"].as_array().unwrap();
    assert!(github.contains(&json!({ "k": "run_id", "v": "1" })), "{github:?}");
}

/// A value that a step printed is its output, not the text of the script
/// that the log shows first.
#[test]
fn printed_takes_the_output_of_a_step() {
    let logs = "2026-01-01T00:00:00Z ##[group]Run echo \"hosted-pwd=$PWD\"\n\
                2026-01-01T00:00:01Z hosted-pwd=/home/runner/work/app/app\r\n";
    assert_eq!(printed(logs, "hosted-pwd").as_deref(), Some("/home/runner/work/app/app"));
    assert_eq!(printed(logs, "hosted-imageos"), None);
}
