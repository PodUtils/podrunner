//! `podrunner run DIR` (T-125): each workflow of the directory (`*.yml`,
//! `*.yaml`, in the order of their names) whose `on:` takes the event
//! after its filters runs, one after the other, as GitHub starts each
//! workflow that an event triggers. A workflow that does not take the event
//! gets one line; `--force-event` runs each one. The code is the worst code
//! of the runs.

use std::path::PathBuf;

use pod_core::failure::Class;

use super::{Args, context, fail, run};
use crate::cli::Out;

/// The workflows of `dir`, by name.
fn workflows(dir: &std::path::Path) -> Result<Vec<PathBuf>, String> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && matches!(p.extension().and_then(|x| x.to_str()), Some("yml" | "yaml")))
        .collect();
    out.sort();
    Ok(out)
}

/// Run each workflow of the directory `a.file` that takes the event.
pub(super) fn run_dir(args: &[String], a: &Args) -> Out {
    let files = match workflows(&a.file) {
        Ok(f) if f.is_empty() => {
            return fail(Class::NotFound, format!("{}: no workflow (*.yml, *.yaml)", a.file.display()));
        }
        Ok(f) => f,
        Err(e) => return fail(Class::NotFound, e),
    };
    let dir = a.file.to_string_lossy().into_owned();
    let Some(at) = args.iter().position(|x| *x == dir) else {
        return fail(Class::Usage, format!("run: {dir} is not an argument"));
    };
    let facts = context::facts(a);
    let mut out = Out { code: 0, stdout: String::new(), stderr: String::new() };
    for file in files {
        let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let parsed = std::fs::read_to_string(&file)
            .map_err(|e| e.to_string())
            .and_then(|t| pod_actions_workflow::parse(&t).map_err(|e| e.to_string()));
        let w = match parsed {
            Ok(p) => p.workflow,
            Err(e) => {
                out.stderr.push_str(&format!("podrunner: {}: {e}\n", file.display()));
                out.code = out.code.max(Class::Config.exit_code());
                continue;
            }
        };
        if !a.force_event {
            match context::takes(a, &facts, &w) {
                Ok(true) => {}
                Ok(false) => {
                    out.stdout.push_str(&format!("--- {name}: `on:` does not take the event {:?}\n", a.event));
                    continue;
                }
                Err(e) => {
                    out.stderr.push_str(&format!("podrunner: {}: {e}\n", file.display()));
                    out.code = out.code.max(Class::Config.exit_code());
                    continue;
                }
            }
        }
        let mut one = args.to_vec();
        one[at] = file.display().to_string();
        let o = run(&one);
        out.stdout.push_str(&format!("--- {name}\n{}", o.stdout));
        out.stderr.push_str(&o.stderr);
        out.code = out.code.max(o.code);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_runs_each_workflow_that_takes_the_event() {
        let dir = std::env::temp_dir().join(format!("podrunner-many-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let job = "jobs:\n  a:\n    runs-on: x\n    steps: [{run: x}]\n";
        std::fs::write(dir.join("a.yml"), format!("on: push\n{job}")).unwrap();
        std::fs::write(dir.join("b.yaml"), format!("on: pull_request\n{job}")).unwrap();
        std::fs::write(dir.join("c.yml"), format!("on:\n  push:\n    tags: [v*]\n{job}")).unwrap();
        std::fs::write(dir.join("notes.txt"), "not a workflow").unwrap();
        let d = dir.display().to_string();
        let args: Vec<String> = [&d, "--dry-run", "--no-cache", "--repository", "o/r", "--ref", "refs/heads/main"]
            .map(String::from)
            .to_vec();
        let out = run(&args);
        assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
        assert!(out.stdout.contains("--- a.yml\nplanned a\n"), "{}", out.stdout);
        assert!(out.stdout.contains("--- b.yaml: `on:` does not take"), "{}", out.stdout);
        assert!(out.stdout.contains("--- c.yml: `on:` does not take"), "a tag filter on a branch: {}", out.stdout);
        let empty = dir.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let out = run(&[empty.display().to_string(), "--dry-run".into()]);
        assert!(out.stderr.contains("no workflow"), "{}", out.stderr);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
