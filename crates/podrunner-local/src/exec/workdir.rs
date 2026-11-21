//! The work folder of the runner in a unit of the local mode (T-590): the
//! hosted path, `work` in the runner directory of the image
//! (`/home/runner/work`), not `_work` of the fake broker. Then
//! `GITHUB_WORKSPACE` is `/home/runner/work/REPO/REPO`, as on a hosted
//! runner. The export, the tool cache and the hook policy use the same
//! folder.

use podrunner_schema::{CredentialFile, Secret};

/// The work folder in the runner directory, as the hook policy names it.
pub const WORK_DIR: &str = "work";

/// The runner directory of `run_sh`: `/home/runner` for
/// `/home/runner/run.sh`.
pub fn run_root(run_sh: &str) -> &str {
    run_sh.rsplit_once('/').map_or("/home/runner", |x| x.0)
}

/// The work folder of the runner of `run_sh`.
pub fn work_folder(run_sh: &str) -> String {
    format!("{}/{WORK_DIR}", run_root(run_sh))
}

/// Set `workFolder` of `.runner` in `files` to `folder`. An error names
/// the cause, never the content of the file.
pub(super) fn set(files: &mut [CredentialFile], folder: &str) -> Result<(), String> {
    for f in files.iter_mut().filter(|f| f.name == ".runner") {
        let mut v: serde_json::Value =
            serde_json::from_str(f.content.expose()).map_err(|_| "the .runner of the fake broker is not JSON")?;
        let map = v.as_object_mut().ok_or("the .runner of the fake broker is not a JSON object")?;
        map.insert("workFolder".into(), folder.into());
        f.content = Secret::new(v.to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use podrunner_broker_fake::Job;
    use podrunner_fake::FakeUnit;
    use podrunner_schema::JobId;
    use podrunner_unit::UnitDriver;

    use super::super::tests::{Mounted, capability};
    use super::super::{InUnit, vnet};
    use crate::drive::Executor;

    #[test]
    fn the_work_folder_is_in_the_runner_directory() {
        assert_eq!(super::work_folder("/home/runner/run.sh"), "/home/runner/work");
        assert_eq!(super::work_folder("/opt/r/run.sh"), "/opt/r/work");
        assert_eq!(super::work_folder("run.sh"), "/home/runner/work");
        let file = |t: &str| podrunner_schema::CredentialFile {
            name: ".runner".into(),
            content: podrunner_schema::Secret::new(t),
        };
        let mut files = [file(r#"{"agentName":"r","workFolder":"_work"}"#)];
        super::set(&mut files, "/home/runner/work").unwrap();
        let v: serde_json::Value = serde_json::from_str(files[0].content.expose()).unwrap();
        assert_eq!((v["workFolder"].as_str(), v["agentName"].as_str()), (Some("/home/runner/work"), Some("r")));
        let e = super::set(&mut [file("[\"SECRET\"]")], "/w").unwrap_err();
        assert!(e.contains("not a JSON object") && !e.contains("SECRET"), "{e}");
    }

    /// The unit of a job gets `.runner` with the hosted work folder; the
    /// export mounts its host directory there, and the tool cache links
    /// into its `_tool` (T-590).
    #[test]
    fn a_unit_of_the_local_mode_has_the_hosted_work_folder() {
        let dir = std::env::temp_dir().join(format!("podrunner-local-workdir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("cache")).unwrap();
        std::fs::write(dir.join(vnet::SHIM), "").unwrap();
        let fake = FakeUnit::new(&dir.join("units"), capability(), "linux/amd64");
        let driver = Arc::new(Mounted(fake, std::sync::Mutex::new(None)));
        let image = driver.prepare(&format!("img@sha256:{}", "a".repeat(64)), "linux/amd64", false).unwrap();
        let mut exec = InUnit::new(driver.clone(), image);
        exec.vnet = Some(dir.join(vnet::SHIM));
        exec.export = Some(dir.join("out"));
        exec.toolcache = Some(dir.join("cache"));
        let e = exec.run(&JobId::new("a.0").unwrap(), Job::scripts("a", &["true"]), Duration::from_secs(60));
        assert!(e.unwrap_err().contains("kept"));
        let spec = driver.1.lock().unwrap().take().expect("a start");
        let runner = spec.credential.files.iter().find(|f| f.name == ".runner").expect("a .runner");
        let v: serde_json::Value = serde_json::from_str(runner.content.expose()).unwrap();
        assert_eq!(v["workFolder"], "/home/runner/work", "{v}");
        let targets: Vec<&str> = spec.mounts.iter().map(|(_, t)| t.as_str()).collect();
        assert!(targets.contains(&"/home/runner/work") && !targets.iter().any(|t| t.contains("_work")), "{targets:?}");
        assert_eq!(&spec.argv[3..], ["podrunner", "/home/runner/work/_tool", "/home/runner/run.sh"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
