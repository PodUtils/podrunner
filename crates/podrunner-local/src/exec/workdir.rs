//! The work folder of the runner in a unit of the local mode (T-590): the
//! hosted path `/home/runner/work`, not `_work` of the fake broker. Then
//! `GITHUB_WORKSPACE` is `/home/runner/work/REPO/REPO`, as on a hosted
//! runner. The export, the tool cache and the hook policy use the same
//! folder.

use podrunner_schema::{CredentialFile, Secret};

/// The work folder in the runner directory, as the hook policy names it.
pub const WORK_DIR: &str = "work";

/// The runner of a unit when `--run-sh` names no other.
pub const RUN_SH: &str = "/home/runner/run.sh";

/// The runner directory of `run_sh`: `/home/runner` for
/// `/home/runner/run.sh`.
pub fn run_root(run_sh: &str) -> &str {
    run_sh.rsplit_once('/').map_or("/home/runner", |x| x.0)
}

/// The home of the runner user of a hosted runner.
const RUNNER_HOME: &str = "/home/runner";

/// The work folder of the runner of `run_sh`: `/home/runner/work` for a
/// runner in `/home/runner` or in a directory under it (the runner image of
/// the hosted snapshot has `/home/runner/actions-runner`, T-541, T-609),
/// else `work` in the runner directory.
pub fn work_folder(run_sh: &str) -> String {
    let root = run_root(run_sh);
    let hosted = root.strip_prefix(RUNNER_HOME).is_some_and(|r| r.is_empty() || r.starts_with('/'));
    format!("{}/{WORK_DIR}", if hosted { RUNNER_HOME } else { root })
}

/// The roots of the hook policy of the runner of `run_sh`: its work folder
/// and its externals, under the directory that holds both (T-734). The
/// podvm driver takes each relative mount source under that directory.
pub fn hook_roots(run_sh: &str) -> podrunner_hook::policy::Roots {
    let externals = format!("{}/{}", run_root(run_sh), podrunner_hook::policy::EXTERNALS);
    podrunner_hook::policy::Roots::of(&work_folder(run_sh), &externals)
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
        // T-609: a runner root under the home of the runner user (the runner
        // layer of the hosted snapshot) takes the hosted folder.
        assert_eq!(super::work_folder("/home/runner/actions-runner/run.sh"), "/home/runner/work");
        assert_eq!(super::work_folder("/home/runner/a/b/run.sh"), "/home/runner/work");
        assert_eq!(super::work_folder("/home/runnerx/run.sh"), "/home/runnerx/work");
        assert_eq!(super::work_folder("/home/run.sh"), "/home/work");
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

    /// T-734: the hook policy holds the work folder and the externals of the
    /// runner under one base, the runner directory of the podvm driver.
    #[test]
    fn the_hook_roots_hold_the_work_folder_and_the_externals() {
        let roots = |run_sh: &str| {
            let r = super::hook_roots(run_sh);
            (r.base, r.work, r.externals)
        };
        let t = |b: &str, w: &str, e: &str| (b.to_string(), w.to_string(), e.to_string());
        assert_eq!(roots("/home/runner/run.sh"), t("/home/runner", "work", "externals"));
        assert_eq!(roots("run.sh"), t("/home/runner", "work", "externals"));
        assert_eq!(roots("/home/runner/actions-runner/run.sh"), t("/home/runner", "work", "actions-runner/externals"));
        assert_eq!(roots("/home/runner/a/b/run.sh"), t("/home/runner", "work", "a/b/externals"));
        assert_eq!(roots("/opt/r/run.sh"), t("/opt/r", "work", "externals"));
        // A base of `/` is no base: the unit refuses each container.
        assert_eq!(roots("/run.sh"), t("", "", ""));
    }

    /// The unit of a job gets `.runner` with the hosted work folder; the
    /// export mounts its host directory there, and the tool cache links
    /// into its `_tool` (T-590).
    #[test]
    fn a_unit_of_the_local_mode_has_the_hosted_work_folder() {
        if !crate::tests::here_or_in_short_tmp(
            "exec::workdir::tests::a_unit_of_the_local_mode_has_the_hosted_work_folder",
        ) {
            return;
        }
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

    /// T-609: the runner of the hosted runner image
    /// (`/home/runner/actions-runner`, T-541) runs with the hosted work
    /// folder, and the export mounts its host directory there.
    #[test]
    fn the_runner_of_the_hosted_image_has_the_hosted_work_folder() {
        if !crate::tests::here_or_in_short_tmp(
            "exec::workdir::tests::the_runner_of_the_hosted_image_has_the_hosted_work_folder",
        ) {
            return;
        }
        let dir = std::env::temp_dir().join(format!("podrunner-local-workdir-hosted-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(vnet::SHIM), "").unwrap();
        let fake = FakeUnit::new(&dir.join("units"), capability(), "linux/amd64");
        let driver = Arc::new(Mounted(fake, std::sync::Mutex::new(None)));
        let image = driver.prepare(&format!("img@sha256:{}", "a".repeat(64)), "linux/amd64", false).unwrap();
        let mut exec = InUnit::new(driver.clone(), image);
        exec.run_sh = "/home/runner/actions-runner/run.sh".into();
        exec.vnet = Some(dir.join(vnet::SHIM));
        exec.export = Some(dir.join("out"));
        let e = exec.run(&JobId::new("a.0").unwrap(), Job::scripts("a", &["true"]), Duration::from_secs(60));
        assert!(e.unwrap_err().contains("kept"));
        let spec = driver.1.lock().unwrap().take().expect("a start");
        let runner = spec.credential.files.iter().find(|f| f.name == ".runner").expect("a .runner");
        let v: serde_json::Value = serde_json::from_str(runner.content.expose()).unwrap();
        assert_eq!(v["workFolder"], "/home/runner/work", "{v}");
        let targets: Vec<&str> = spec.mounts.iter().map(|(_, t)| t.as_str()).collect();
        assert_eq!(targets[0], "/home/runner/work", "{targets:?}");
        assert_eq!(spec.argv, ["/home/runner/actions-runner/run.sh"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
