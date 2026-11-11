//! The input of runner v2.338.2 and the checks of the host, each with an
//! input that a check must refuse.

use podrunner_schema::{ContainerRoute, RefusalCode};
use serde_json::{Value, json};

use crate::input::{Command, parse};
use crate::policy::{Gate, Plan, Policy, Refusal};

const IMAGE: &str = "node@sha256:0000000000000000000000000000000000000000000000000000000000000000";
const CANARY: &str = "podrunner-canary-registry-5e1f";

fn policy() -> Policy {
    Policy {
        container: ContainerRoute::Hooks,
        shared_host_net: false,
        registries: vec!["docker.io".into(), "ghcr.io".into()],
        work_root: "/home/runner/_work".into(),
    }
}

fn mount(src: &str, dst: &str) -> Value {
    json!({ "userProvidedValue": null, "sourceVolumePath": src, "targetVolumePath": dst, "readOnly": false })
}

/// A job container as the runner writes it (null for unset fields).
fn container() -> Value {
    json!({
        "image": IMAGE, "dockerfile": null, "entryPointArgs": ["-f", "/dev/null"], "entryPoint": "tail",
        "workingDirectory": "/__w/app/app", "createOptions": "--cpus 2",
        "registry": { "username": "u", "password": CANARY, "serverUrl": "docker.io" },
        "environmentVariables": { "CI": "true" }, "portMappings": [],
        "systemMountVolumes": [
            mount("/var/run/docker.sock", "/var/run/docker.sock"),
            mount("/home/runner/_work", "/__w"),
            mount("/home/runner/externals", "/__e"),
        ],
        "userMountVolumes": [],
    })
}

fn call(command: &str, args: Value) -> Vec<u8> {
    json!({ "command": command, "responseFile": "/home/runner/_work/_temp/r.json", "args": args, "state": {} })
        .to_string()
        .into_bytes()
}

fn prepare(c: Value) -> Vec<u8> {
    call("prepare_job", json!({ "container": c, "services": [] }))
}

fn check(gate: &mut Gate, p: &Policy, cmd: Command, body: &[u8]) -> Result<Plan, Refusal> {
    gate.check(&parse(cmd, body).unwrap(), p)
}

fn code(r: Result<Plan, Refusal>) -> RefusalCode {
    r.unwrap_err().code
}

#[test]
fn a_job_container_of_the_runner_passes_with_its_mounts() {
    let mut externals = policy();
    externals.work_root = "/home/runner".into();
    let plan = check(&mut Gate::default(), &externals, Command::PrepareJob, &prepare(container())).unwrap();
    let Plan::Prepare { job: Some(job), services } = plan else { panic!("{plan:?}") };
    assert!(services.is_empty());
    assert_eq!(job.argv, ["tail", "-f", "/dev/null"]);
    assert_eq!(job.options, [("--cpus".to_string(), "2".to_string())]);
    // The docker socket is dropped; /__e is read-only.
    assert_eq!(job.mounts.len(), 2);
    assert!(job.mounts.iter().any(|m| m.target == "/__e" && m.read_only && m.source == "externals"));
    // The password is in no Debug text.
    assert!(!format!("{job:?}").contains(CANARY));
}

#[test]
fn each_call_before_prepare_job_is_refused() {
    let mut g = Gate::default();
    let step = call("run_script_step", json!({ "entryPoint": "sh", "entryPointArgs": ["-e", "x.sh"] }));
    assert_eq!(code(check(&mut g, &policy(), Command::RunScriptStep, &step)), RefusalCode::ContainerNotSupported);
    assert_eq!(
        code(check(&mut g, &policy(), Command::CleanupJob, &call("cleanup_job", json!({})))),
        RefusalCode::ContainerNotSupported
    );
    let mut p = policy();
    p.work_root = "/home/runner".into();
    check(&mut g, &p, Command::PrepareJob, &prepare(container())).unwrap();
    assert!(matches!(check(&mut g, &policy(), Command::RunScriptStep, &step), Ok(Plan::Script(_))));
    // prepare_job comes once.
    assert_eq!(
        code(check(&mut g, &p, Command::PrepareJob, &prepare(container()))),
        RefusalCode::ContainerOptionRefused
    );
}

#[test]
fn each_rule_refuses_its_input() {
    let with = |k: &str, v: Value| {
        let mut c = container();
        c[k] = v;
        c
    };
    let mut p = policy();
    p.work_root = "/home/runner".into();
    let cases: Vec<(Value, RefusalCode)> = vec![
        (with("dockerfile", json!("Dockerfile")), RefusalCode::BuildNotSupported),
        (with("image", json!("node:20")), RefusalCode::PayloadNotPinned),
        (with("image", json!(format!("quay.io/x/{IMAGE}"))), RefusalCode::ContainerOptionRefused),
        (
            with("registry", json!({ "username": "u", "password": CANARY, "serverUrl": "ghcr.io" })),
            RefusalCode::ContainerOptionRefused,
        ),
        (with("userMountVolumes", json!([mount("/etc", "/etc")])), RefusalCode::ContainerOptionRefused),
        (with("systemMountVolumes", json!([mount("/home/runner/_work", "/__t")])), RefusalCode::ContainerOptionRefused),
        (with("systemMountVolumes", json!([mount("/etc", "/__w")])), RefusalCode::ContainerOptionRefused),
        (
            with("systemMountVolumes", json!([mount("/home/runner/../etc", "/__w")])),
            RefusalCode::ContainerOptionRefused,
        ),
        (
            with("systemMountVolumes", json!([mount("/home/runner/x.sock", "/__w")])),
            RefusalCode::ContainerOptionRefused,
        ),
        (
            with("systemMountVolumes", json!([mount("/run/podman.sock", "/var/run/docker.sock")])),
            RefusalCode::ContainerOptionRefused,
        ),
        (with("createOptions", json!("--privileged")), RefusalCode::ContainerOptionRefused),
        (with("createOptions", json!("--network host")), RefusalCode::ContainerOptionRefused),
        (with("createOptions", json!("--device /dev/kvm")), RefusalCode::ContainerOptionRefused),
        (with("createOptions", json!("-v /:/host")), RefusalCode::ContainerOptionRefused),
        (with("createOptions", json!("--user")), RefusalCode::ContainerOptionRefused),
        (with("environmentVariables", json!({ "A=B": "x" })), RefusalCode::ContainerOptionRefused),
        (with("workingDirectory", json!("../x")), RefusalCode::ContainerOptionRefused),
    ];
    for (c, want) in cases {
        let r = check(&mut Gate::default(), &p, Command::PrepareJob, &prepare(c.clone()));
        let refusal = r.clone().unwrap_err();
        assert_eq!(refusal.code, want, "{c}");
        assert!(!refusal.why.contains(CANARY));
    }
    // The runner sends an empty server for a Docker Hub image: it is
    // docker.io, the host of the image.
    let hub = with("registry", json!({ "username": "u", "password": CANARY, "serverUrl": "" }));
    assert!(check(&mut Gate::default(), &p, Command::PrepareJob, &prepare(hub)).is_ok());
    // Ports and services on a unit of the host network.
    p.shared_host_net = true;
    let ports = prepare(with("portMappings", json!(["8080:80"])));
    assert_eq!(code(check(&mut Gate::default(), &p, Command::PrepareJob, &ports)), RefusalCode::ContainerOptionRefused);
    let services = call("prepare_job", json!({ "container": null, "services": [container()] }));
    assert_eq!(
        code(check(&mut Gate::default(), &p, Command::PrepareJob, &services)),
        RefusalCode::ContainerNotSupported
    );
    // No container route: each call is refused.
    p.container = ContainerRoute::None;
    assert_eq!(
        code(check(&mut Gate::default(), &p, Command::PrepareJob, &prepare(container()))),
        RefusalCode::ContainerNotSupported
    );
}

#[test]
fn the_input_is_a_closed_set() {
    assert!(parse(Command::PrepareJob, &prepare(container())).is_ok());
    // An unknown field, at the top and in the args.
    let mut v: Value = serde_json::from_slice(&prepare(container())).unwrap();
    v["extra"] = json!(1);
    assert!(parse(Command::PrepareJob, v.to_string().as_bytes()).is_err());
    assert!(parse(Command::PrepareJob, &prepare(with_field("hostPath", json!("/")))).is_err());
    // The command of the path and of the body agree.
    assert!(parse(Command::CleanupJob, &prepare(container())).is_err());
    // Null lists and maps are empty.
    let nulls = call(
        "run_script_step",
        json!({ "entryPoint": "sh", "entryPointArgs": null, "environmentVariables": null, "prependPath": null, "workingDirectory": null }),
    );
    assert!(parse(Command::RunScriptStep, &nulls).is_ok());
    // A body over 1 MiB, and bytes that are not JSON.
    assert!(parse(Command::PrepareJob, &vec![b' '; crate::input::BODY_MAX + 1]).is_err());
    assert!(parse(Command::PrepareJob, b"\xff").is_err());
    assert_eq!(Command::parse("run_container_step"), Some(Command::RunContainerStep));
    assert_eq!(Command::parse("build"), None);
}

fn with_field(k: &str, v: Value) -> Value {
    let mut c = container();
    c[k] = v;
    c
}
