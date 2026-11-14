//! The protection of an environment (T-136). GitHub keeps the rules of an
//! environment in the settings of the repository, so `--protected NAME`
//! gives them here. A job of a protected environment runs when
//! `--approve NAME` gives the approval, or when the user approves it on the
//! terminal before the first job. With no terminal, its jobs are refused by
//! name; the other jobs run.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, IsTerminal, Write};

use pod_actions_workflow::{Kind, Workflow};
use podrunner_local::Environment;

use super::Args;

/// The environments that the jobs of `w` name (`--job` selects the jobs).
/// A name that is an expression is not known before the run.
fn used(w: &Workflow, jobs: &[String]) -> BTreeSet<String> {
    let selected = |id: &str| jobs.is_empty() || jobs.iter().any(|j| j == id);
    w.jobs
        .iter()
        .filter(|j| selected(&j.id))
        .filter_map(|j| j.environment.as_ref())
        .filter_map(|n| match &n.kind {
            Kind::String(s) => Some(s.clone()),
            Kind::Map(e) => e.iter().find(|(k, _)| k.name == "name").and_then(|(_, v)| match &v.kind {
                Kind::String(s) => Some(s.clone()),
                _ => None,
            }),
            _ => None,
        })
        .filter(|n| !n.contains("${{"))
        .collect()
}

/// The answer of the terminal for the environment NAME: `None` when stdin
/// is not a terminal.
pub(super) fn terminal(name: &str) -> Option<bool> {
    if !std::io::stdin().is_terminal() {
        return None;
    }
    eprint!("podrunner: the environment {name} is protected; run its jobs? [y/N] ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).ok()?;
    Some(matches!(line.trim(), "y" | "Y" | "yes"))
}

/// Mark each protected environment that a job uses and that has no
/// approval: `ask` gives the answer of the user, or `None` with no
/// terminal.
pub(super) fn hold(
    a: &Args,
    w: &Workflow,
    envs: &mut BTreeMap<String, Environment>,
    ask: &mut dyn FnMut(&str) -> Option<bool>,
) {
    let used = used(w, &a.jobs);
    let protected: BTreeSet<&String> = a.protected.iter().collect();
    for name in protected {
        if !used.contains(name) || a.approved.contains(name) {
            continue;
        }
        let refused = match ask(name) {
            Some(true) => continue,
            Some(false) => format!("the environment {name} is protected, and its approval was refused"),
            None => format!("the environment {name} is protected: give --approve {name} to run its jobs"),
        };
        envs.entry(name.clone()).or_default().refused = Some(refused);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_protected_environment_needs_an_approval() {
        let text = "on: push\njobs:\n  deploy:\n    runs-on: x\n    environment: prod\n    steps: [{run: x}]\n  stage:\n    runs-on: x\n    environment:\n      name: stage\n    steps: [{run: x}]\n  test:\n    runs-on: x\n    steps: [{run: x}]\n";
        let w = pod_actions_workflow::parse(text).unwrap().workflow;
        let args = |protected: &[&str], approved: &[&str]| Args {
            protected: protected.iter().map(|s| s.to_string()).collect(),
            approved: approved.iter().map(|s| s.to_string()).collect(),
            ..Args::default()
        };
        let run = |a: &Args, answer: Option<bool>| {
            let mut envs = BTreeMap::new();
            let mut asked = Vec::new();
            hold(a, &w, &mut envs, &mut |n| {
                asked.push(n.to_string());
                answer
            });
            (envs.into_iter().map(|(k, e)| (k, e.refused.unwrap_or_default())).collect::<Vec<_>>(), asked)
        };
        let (held, asked) = run(&args(&["prod", "stage", "unused"], &["stage"]), None);
        assert_eq!(asked, ["prod"], "an approved or unused environment asks nothing");
        assert_eq!(held.len(), 1);
        assert!(held[0].1.contains("--approve prod"), "{held:?}");
        let (held, _) = run(&args(&["prod"], &[]), Some(false));
        assert!(held[0].1.contains("refused"), "{held:?}");
        assert!(run(&args(&["prod"], &[]), Some(true)).0.is_empty());
        let only_test = Args { jobs: vec!["test".into()], ..args(&["prod"], &[]) };
        assert!(run(&only_test, None).0.is_empty(), "--job test runs no job of prod");
    }
}
