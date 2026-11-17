//! The identity of a local run in the `github` context (T-418): GitHub
//! gives each run a new `run_id`, counts `run_number` for each workflow,
//! and counts `run_attempt` for each rerun. A cache key or a concurrency
//! group with one of them must differ from run to run.

use std::path::{Path, PathBuf};

use crate::run_local::Args;

/// The file beside the state of a run (`--state`, `--rerun`) that a rerun
/// reads.
const FILE: &str = "run.json";

/// `run_id`, `run_number`, `run_attempt` of this run. A rerun keeps the
/// id and the number of its run and adds one attempt; another run takes a
/// new id (the time in milliseconds) and the next number of the workflow
/// (`counter`). A dry run writes nothing.
pub(super) fn ids(a: &Args, counter: Option<&Path>) -> (String, String, String) {
    let before = a.rerun.as_ref().and_then(|d| std::fs::read_to_string(d.join(FILE)).ok());
    let before: Option<serde_json::Value> = before.and_then(|t| serde_json::from_str(&t).ok());
    let field = |k: &str| before.as_ref().and_then(|v| v[k].as_u64());
    let (id, number, attempt) = match (field("run_id"), field("run_number"), field("run_attempt")) {
        (Some(id), Some(n), Some(at)) => (id, n, at + 1),
        _ => {
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_millis());
            let last = counter.and_then(|c| std::fs::read_to_string(c).ok()).and_then(|t| t.trim().parse::<u64>().ok());
            let n = last.unwrap_or(0) + 1;
            if let (false, Some(c)) = (a.dry_run, counter) {
                let _ = c.parent().map(std::fs::create_dir_all);
                let _ = std::fs::write(c, format!("{n}\n"));
            }
            (now as u64, n, 1)
        }
    };
    if let (false, Some(dir)) = (a.dry_run, a.state.clone().or_else(|| a.rerun.clone())) {
        let text = serde_json::json!({ "run_id": id, "run_number": number, "run_attempt": attempt }).to_string();
        let _ = podrunner_local::rerun::write_file(&dir, FILE, &text);
    }
    (id.to_string(), number.to_string(), attempt.to_string())
}

/// The counter of the runs of the workflow `file` of `repository`, in the
/// cache of the user.
pub(super) fn counter(repository: &str, file: &Path) -> Option<PathBuf> {
    let key = format!("{repository}\n{}", file.display());
    let name = podrunner_local::store::digest(key.as_bytes());
    crate::run_local::actions::user_cache("runs").ok().map(|d| d.join(&name[name.len().saturating_sub(16)..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T-418: two runs have two ids and two numbers; a rerun keeps both
    /// and counts its attempt; a dry run counts nothing.
    #[test]
    fn each_run_has_its_own_id_and_number() {
        let dir = std::env::temp_dir().join(format!("podrunner-ids-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let counter = dir.join("counter");
        let state = dir.join("state").display().to_string();
        let parse = |extra: &[&str]| {
            let mut v = vec!["w.yml", "--runner-dir", "/r"];
            v.extend_from_slice(extra);
            crate::run_local::parse(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
        };
        let first = ids(&parse(&["--state", &state]), Some(&counter));
        std::thread::sleep(std::time::Duration::from_millis(5));
        let second = ids(&parse(&["--state", &state]), Some(&counter));
        assert_ne!(first.0, second.0, "a new id");
        assert_eq!((first.1.as_str(), second.1.as_str()), ("1", "2"));
        assert_eq!((first.2.as_str(), second.2.as_str()), ("1", "1"));
        let again = ids(&parse(&["--rerun", &state]), Some(&counter));
        assert_eq!(again, (second.0.clone(), "2".to_string(), "2".to_string()), "a rerun keeps the run");
        let dry = ids(&parse(&["--dry-run"]), Some(&counter));
        assert_eq!(dry.1, "3");
        assert_eq!(ids(&parse(&["--dry-run"]), Some(&counter)).1, "3", "a dry run counts nothing");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
