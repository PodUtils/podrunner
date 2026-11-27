//! T-541: `[unit.KIND] work_folder` puts the work folder of the runner at
//! the hosted path: the unit gets `.runner` with it.

use podrunner_schema::{Boundary, EgressHold, Principal, RefusalCode};
use serde_json::json;

use super::*;

/// The `.runner` that the payload of a unit of a kind with `folder` sees.
fn seen_runner(folder: Option<&str>) -> (Bed, Option<Value>) {
    seen_runner_of("fake", folder)
}

/// The same, on the kind `name`.
fn seen_runner_of(name: &'static str, folder: Option<&str>) -> (Bed, Option<Value>) {
    let cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    let out = TempDir::new();
    let copy = out.0.join("runner.json");
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))])],
        vec![KindSpec {
            name,
            slots: 1,
            capability: cap,
            command: sh(&format!("cp .runner-root/.runner '{}'", copy.display())),
        }],
    );
    bed.slots.kinds[0].cfg.work_folder = folder.map(str::to_string);
    bed.until(50, |s| s.jobs().all(|j| j.state.is_final()) && s.jobs().count() == 1);
    let text = std::fs::read_to_string(&copy).ok();
    (bed, text.map(|t| serde_json::from_str(&t).unwrap()))
}

#[test]
fn a_kind_with_a_work_folder_gives_the_runner_the_hosted_path() {
    let (_, runner) = seen_runner(Some("/home/runner/work"));
    let runner = runner.expect("the payload saw .runner");
    assert_eq!(runner["workFolder"], "/home/runner/work", "{runner}");
    assert!(runner["agentName"].as_str().is_some_and(|n| !n.is_empty()), "the rest of the file stays: {runner}");
    // With no key, the file of the mint stays as it is.
    let (_, runner) = seen_runner(None);
    assert_eq!(runner.expect("the payload saw .runner").get("workFolder"), None);
}

/// T-590: a kind with a runner home gets the hosted path with no key; the
/// key comes first.
#[test]
fn a_kind_with_a_runner_home_has_the_hosted_work_folder_by_default() {
    for kind in ["podvm", "incus", "microvm"] {
        let (_, runner) = seen_runner_of(kind, None);
        assert_eq!(runner.expect("the payload saw .runner")["workFolder"], "/home/runner/work", "{kind}");
    }
    let (_, runner) = seen_runner_of("podvm", Some("/home/runner/_work"));
    assert_eq!(runner.expect("the payload saw .runner")["workFolder"], "/home/runner/_work");
    let u = |extra: &str| -> crate::config::UnitConfig { toml::from_str(&format!("slots = 1\n{extra}")).unwrap() };
    assert_eq!(u("runner_dir = \"/opt/r/\"").work_folder_of("podvm").as_deref(), Some("/opt/r/work"));
    for kind in ["fake", "docker", "host"] {
        assert_eq!(u("").work_folder_of(kind), None, "{kind}");
    }
}

#[test]
fn a_runner_file_that_is_not_json_refuses_the_unit() {
    let cap = capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory);
    let mut bed = Bed::new(
        config(""),
        vec![("a", "trusted", None, vec![offer(0, "j-1", json!({}))])],
        vec![KindSpec { name: "fake", slots: 1, capability: cap, command: sh("true") }],
    );
    bed.slots.kinds[0].cfg.work_folder = Some("/home/runner/work".into());
    // The fake source gives a JSON `.runner`: a wrapper changes it.
    let placeholder = FakeSource::new("a", bed.clock.clone(), parse_script("").unwrap());
    let inner = std::mem::replace(&mut bed.slots.sources[0].source, Box::new(placeholder));
    bed.slots.sources[0].source = Box::new(NoRunner(inner));
    bed.until(50, |s| s.jobs().all(|j| j.state.is_final()) && s.jobs().count() == 1);
    let r = bed.slots.reports.iter().find(|r| r.job_id.as_str() == "j-1").expect("a report");
    assert_eq!(r.refusal_code, Some(RefusalCode::UnitStartFailed), "{r:?}");
    let audit = std::fs::read_to_string(bed.slots.state_dir.join("audit.jsonl")).unwrap();
    assert!(audit.contains("work_folder: .runner is not JSON"), "{audit}");
}

/// A source whose `.runner` is not JSON.
struct NoRunner(Box<dyn podrunner_source::JobSource>);

impl podrunner_source::JobSource for NoRunner {
    fn hello(&mut self, c: &str, v: &str) -> Result<podrunner_source::Hello, podrunner_source::SourceError> {
        self.0.hello(c, v)
    }
    fn next(
        &mut self,
        n: u32,
        w: std::time::Duration,
    ) -> Result<Vec<podrunner_source::Event>, podrunner_source::SourceError> {
        self.0.next(n, w)
    }
    fn ack(&mut self, j: &podrunner_schema::JobId) -> Result<(), podrunner_source::SourceError> {
        self.0.ack(j)
    }
    fn claim(&mut self, j: &podrunner_schema::JobId, r: &str) -> Result<(), podrunner_source::SourceError> {
        self.0.claim(j, r)
    }
    fn mint(
        &mut self,
        j: &podrunner_schema::JobId,
        r: &str,
    ) -> Result<podrunner_schema::Credential, podrunner_source::SourceError> {
        let mut c = self.0.mint(j, r)?;
        for f in c.files.iter_mut().filter(|f| f.name == ".runner") {
            f.content = podrunner_schema::Secret::new("agentName=r");
        }
        Ok(c)
    }
    fn report(
        &mut self,
        j: &podrunner_schema::JobId,
        r: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<(), podrunner_source::SourceError> {
        self.0.report(j, r)
    }
    fn audit(&mut self, l: &serde_json::Map<String, serde_json::Value>) -> Result<(), podrunner_source::SourceError> {
        self.0.audit(l)
    }
    fn withdraw(&mut self, j: &podrunner_schema::JobId, w: &str) -> Result<(), podrunner_source::SourceError> {
        self.0.withdraw(j, w)
    }
    fn release(&mut self, j: &podrunner_schema::JobId) -> Result<(), podrunner_source::SourceError> {
        self.0.release(j)
    }
}

/// The file: an absolute path on the podvm, the incus or the microvm kind;
/// a podvm folder in its runner directory (T-590).
#[test]
fn the_file_holds_the_work_folder_of_a_kind() {
    let check = |unit: &str| {
        Config::parse(&format!(
            "host = \"h\"\nallow_fake = true\n[unit.{unit}\n[pin]\n[[pin.payload]]\narch = \"x86_64\"\n\
             reference = \"{PIN}\"\nbuilt_at_s = 1\n"
        ))
    };
    for kind in ["podvm", "incus", "microvm"] {
        assert!(check(&format!("{kind}]\nslots = 1\nwork_folder = \"/home/runner/work\"")).is_ok(), "{kind}");
    }
    for kind in ["docker", "fake"] {
        let e = check(&format!("{kind}]\nslots = 1\nwork_folder = \"/home/runner/work\"")).unwrap_err();
        assert!(e.contains(&format!("[unit.{kind}] work_folder: only the podvm, the incus and the microvm")), "{e}");
    }
    for bad in ["work", "/", "/home//work", "/home/runner/../work", "/home/./work", "/home/runner/work\\n"] {
        let e = check(&format!("incus]\nslots = 1\nwork_folder = \"{bad}\"")).unwrap_err();
        assert!(e.contains("[unit.incus] work_folder") && e.contains("give an absolute path"), "{bad}: {e}");
    }
    // podvm: a folder in its runner directory, not one of the runner, or
    // beside it (the hosted layout, T-734).
    for good in [
        "runner_dir = \"/opt/r\"\nwork_folder = \"/opt/r/w/x\"",
        "work_folder = \"/home/runner/_work\"",
        "runner_dir = \"/home/runner/actions-runner\"\nwork_folder = \"/home/runner/work\"",
        "runner_dir = \"/home/runner/actions-runner\"\nwork_folder = \"/home/runner/a/b\"",
        "work_folder = \"/home/runnerx/work\"",
    ] {
        assert!(check(&format!("podvm]\nslots = 1\n{good}")).is_ok(), "{good}");
    }
    for bad in [
        "/home/runner",
        "/home",
        "/work",
        "/home/runner/bin",
        "/home/runner/externals/x",
        "/home/runner/hooks",
        "/home/runner/.runner",
    ] {
        let e = check(&format!("podvm]\nslots = 1\nwork_folder = \"{bad}\"")).unwrap_err();
        assert!(e.contains("give a folder in the runner directory \"/home/runner\" or beside it"), "{bad}: {e}");
    }
    let e = check("podvm]\nslots = 1\nrunner_dir = \"/opt/r\"\nwork_folder = \"/home/runner/work\"").unwrap_err();
    assert!(e.contains("in the runner directory \"/opt/r\" or beside it, such as \"/opt/r/work\""), "{e}");
    // A runner directory in `/` has no folder beside it, and the hosted
    // layout keeps the code of the runner out.
    let e = check("podvm]\nslots = 1\nrunner_dir = \"/r\"\nwork_folder = \"/w\"").unwrap_err();
    assert!(e.contains("work_folder \"/w\""), "{e}");
    for bad in ["/home/runner/actions-runner/externals", "/home/runner/actions-runner", "/home/runner"] {
        let e =
            check(&format!("podvm]\nslots = 1\nrunner_dir = \"/home/runner/actions-runner\"\nwork_folder = \"{bad}\""))
                .unwrap_err();
        assert!(e.contains("[unit.podvm] work_folder"), "{bad}: {e}");
    }
    // T-737: `/` as the runner directory gives an empty base to the hook
    // policy; refuse it by the name of the key, with or without a folder.
    for extra in ["", "work_folder = \"/work\"", "work_folder = \"/home/runner/work\""] {
        for dir in ["/", "//"] {
            let e = check(&format!("podvm]\nslots = 1\nrunner_dir = \"{dir}\"\n{extra}")).unwrap_err();
            assert!(e.contains("[unit.podvm] runner_dir") && e.contains("give a folder"), "{dir} {extra}: {e}");
        }
    }
    for good in ["/home/runner", "/home/runner/actions-runner", "/r"] {
        assert!(check(&format!("podvm]\nslots = 1\nrunner_dir = \"{good}\"")).is_ok(), "{good}");
    }
    assert!(
        check("podvm]\nslots = 1\nrunner_dir = \"/home/runner/actions-runner\"\nwork_folder = \"/home/runner/work\"")
            .is_ok()
    );
    // Another kind keeps its host path.
    assert!(check("docker]\nslots = 1\nrunner_dir = \"/\"").is_ok());
}
