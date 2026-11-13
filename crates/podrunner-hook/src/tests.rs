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

fn read_only(src: &str, dst: &str) -> Value {
    let mut m = mount(src, dst);
    m["readOnly"] = json!(true);
    m
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
            mount("/home/runner/_work/_temp", "/__w/_temp"),
            mount("/home/runner/_work/_actions", "/__w/_actions"),
            mount("/home/runner/_work/_tool", "/__w/_tool"),
            read_only("/home/runner/externals", "/__e"),
            mount("/home/runner/_work/_temp/_github_home", "/github/home"),
            mount("/home/runner/_work/_temp/_github_workflow", "/github/workflow"),
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
    // The seven entries of the table pass; the docker socket is dropped.
    assert_eq!(job.mounts.len(), 7);
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

/// The design tests 13, 16 and 17 of the unit contract (T-053): the two
/// mount tables, the docker socket, `/__e`, and the order of the calls.
#[test]
fn the_mount_tables_and_the_order_of_the_design() {
    let mut p = policy();
    p.work_root = "/home/runner".into();
    let with_mounts = |mounts: Vec<Value>| {
        let mut c = container();
        c["systemMountVolumes"] = json!(mounts);
        c
    };
    let refused = |r: Result<Plan, Refusal>| r.map(drop).map_err(|e| e.code);
    let job = |c: Value| refused(check(&mut Gate::default(), &p, Command::PrepareJob, &prepare(c)));
    let w = mount("/home/runner/_work", "/__w");
    // 13: the docker socket path as source only, a writable /__e, /__t.
    assert_eq!(job(with_mounts(vec![mount("/var/run/docker.sock", "/__w")])), Err(RefusalCode::ContainerOptionRefused));
    assert_eq!(
        job(with_mounts(vec![mount("/home/runner/externals", "/__e")])),
        Err(RefusalCode::ContainerOptionRefused)
    );
    assert_eq!(
        job(with_mounts(vec![mount("/home/runner/_work/_tool", "/__t")])),
        Err(RefusalCode::ContainerOptionRefused)
    );
    assert_eq!(job(with_mounts(vec![w.clone(), mount("/home/runner/_work/_tool", "/__w/_tool")])), Ok(()));
    // 17: an action target in prepare_job; a mount on a service.
    assert_eq!(
        job(with_mounts(vec![mount("/home/runner/_work/app", "/github/workspace")])),
        Err(RefusalCode::ContainerOptionRefused)
    );
    let mut service = container();
    service["systemMountVolumes"] = json!([w.clone()]);
    let services = call("prepare_job", json!({ "container": null, "services": [service] }));
    assert_eq!(
        refused(check(&mut Gate::default(), &p, Command::PrepareJob, &services)),
        Err(RefusalCode::ContainerOptionRefused)
    );
    // 16: a container step before prepare_job (a Docker action in a job
    // with no container) runs, with the checks of the action mounts (Q4).
    let action = |mounts: Vec<Value>| {
        let mut c = container();
        c["systemMountVolumes"] = json!(mounts);
        call("run_container_step", c)
    };
    let table = vec![
        mount("/var/run/docker.sock", "/var/run/docker.sock"),
        mount("/home/runner/_work/_temp", "/github/runner_temp"),
        mount("/home/runner/_work/_temp/_github_home", "/github/home"),
        mount("/home/runner/_work/_temp/_github_workflow", "/github/workflow"),
        mount("/home/runner/_work/_temp/_runner_file_commands", "/github/file_commands"),
        mount("/home/runner/_work/app/app", "/github/workspace"),
    ];
    let mut g = Gate::default();
    let Ok(Plan::ContainerStep(early)) = check(&mut g, &p, Command::RunContainerStep, &action(table.clone())) else {
        panic!()
    };
    assert_eq!(early.mounts.len(), 5);
    assert_eq!(
        refused(check(&mut g, &p, Command::RunContainerStep, &action(vec![mount("/etc", "/github/workspace")]))),
        Err(RefusalCode::ContainerOptionRefused)
    );
    // 17: the five entries and the docker socket pass after prepare_job;
    // a job target, or a source outside the unit, in a step is refused.
    check(&mut g, &p, Command::PrepareJob, &prepare(container())).unwrap();
    let Ok(Plan::ContainerStep(step)) = check(&mut g, &p, Command::RunContainerStep, &action(table)) else { panic!() };
    assert_eq!(step.mounts.len(), 5);
    assert_eq!(
        refused(check(&mut g, &p, Command::RunContainerStep, &action(vec![w]))),
        Err(RefusalCode::ContainerOptionRefused)
    );
    let outside = mount("/etc", "/github/workspace");
    assert_eq!(
        refused(check(&mut g, &p, Command::RunContainerStep, &action(vec![outside]))),
        Err(RefusalCode::ContainerOptionRefused)
    );
    // 16: a second prepare_job, with a new image, is refused.
    let mut other = container();
    other["image"] = json!(format!("other@sha256:{}", "2".repeat(64)));
    assert_eq!(
        refused(check(&mut g, &p, Command::PrepareJob, &prepare(other))),
        Err(RefusalCode::ContainerOptionRefused)
    );
}
