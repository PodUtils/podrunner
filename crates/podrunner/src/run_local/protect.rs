//! The protection of an environment (T-136). GitHub keeps the rules of an
//! environment in the settings of the repository, so `--protected NAME`
//! gives them here. A job of a protected environment runs when
//! `--approve NAME` gives the approval, or when the user approves it on the
//! terminal before the first job. With no terminal, its jobs are refused by
//! name; the other jobs run.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, IsTerminal, Write};

use pod_actions_workflow::{Called, Kind, Workflow};
use podrunner_local::Environment;
use podrunner_local::environment::name_of;

use super::Args;

/// The environments that the jobs of `w` name, and the jobs of each
/// workflow that they call (T-419). `--job` selects jobs and each job that
/// they need. A name that is an expression is not known before the run.
fn used(w: &Workflow, jobs: &[String], called: Option<&dyn Called>) -> BTreeSet<String> {
    let mut selected: BTreeSet<&str> = jobs.iter().map(String::as_str).collect();
    loop {
        let more: Vec<&str> = w
            .jobs
            .iter()
            .filter(|j| selected.contains(j.id.as_str()))
            .flat_map(|j| j.needs.iter().map(String::as_str))
            .filter(|n| !selected.contains(n))
            .collect();
        if more.is_empty() {
            break;
        }
        selected.extend(more);
    }
    let mut out = BTreeSet::new();
    walk(w, &|id| jobs.is_empty() || selected.contains(id), None, called, 0, &mut out);
    out
}

/// The names of the jobs of `w` that `pick` selects, and of the workflows
/// that they call, to the 10 levels of GitHub.
fn walk(
    w: &Workflow,
    pick: &dyn Fn(&str) -> bool,
    from: Option<&str>,
    called: Option<&dyn Called>,
    depth: usize,
    out: &mut BTreeSet<String>,
) {
    for j in w.jobs.iter().filter(|j| pick(&j.id)) {
        if let Some(n) = j.environment.as_ref().and_then(|n| name_of(&n.kind)).filter(|n| !n.contains("${{")) {
            out.insert(n);
        }
        let (Some(uses), Some(texts)) = (&j.uses, called) else { continue };
        let Ok(text) = texts.text(uses, from) else { continue };
        if let (Ok(p), true) = (pod_actions_workflow::parse(&text), depth < 10) {
            walk(&p.workflow, &|_| true, Some(uses), called, depth + 1, out);
        }
    }
}

/// A line for each environment of a flag that no job names: a typo of
/// `--approve` or `--protected` would else pass with no sign (T-323 F23).
/// A job whose environment is an expression can name any of them.
pub(super) fn unknown(a: &Args, w: &Workflow, called: Option<&dyn Called>) -> Vec<String> {
    let expr = w.jobs.iter().filter_map(|j| j.environment.as_ref()).any(|n| match &n.kind {
        Kind::String(s) => s.contains("${{"),
        _ => true,
    });
    if expr {
        return Vec::new();
    }
    let used = used(w, &[], called);
    let names = a.approved.iter().map(|n| ("--approve", n)).chain(a.protected.iter().map(|n| ("--protected", n)));
    let names = names
        .chain(a.environment_secrets.iter().map(|(n, _)| ("--environment-secret-file", n)))
        .chain(a.environment_vars.iter().map(|(n, _)| ("--environment-var-file", n)))
        .chain(a.environment_allow.iter().map(|(n, _)| ("--environment-allow", n)));
    let mut seen = BTreeSet::new();
    names
        .filter(|(f, n)| !used.contains(*n) && seen.insert((*f, (*n).clone())))
        .map(|(f, n)| format!("podrunner: {f} {n}: no job of the workflow names the environment {n}"))
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

/// Mark each protected environment that has no approval: `ask` gives the
/// answer of the user for each one that a job names, or `None` with no
/// terminal. Each other one is refused with no question: a job can still
/// reach it by an expression (T-419).
pub(super) fn hold(
    a: &Args,
    w: &Workflow,
    called: Option<&dyn Called>,
    envs: &mut BTreeMap<String, Environment>,
    ask: &mut dyn FnMut(&str) -> Option<bool>,
) {
    let used = used(w, &a.jobs, called);
    let protected: BTreeSet<&String> = a.protected.iter().collect();
    for name in protected {
        if a.approved.contains(name) {
            continue;
        }
        let answer = if used.contains(name) { ask(name) } else { None };
        let refused = match answer {
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
        let text = "on: push\njobs:\n  deploy:\n    runs-on: x\n    environment: prod\n    steps: [{run: x}]\n  after:\n    needs: deploy\n    runs-on: x\n    steps: [{run: x}]\n  call:\n    uses: ./.github/workflows/rel.yml\n  stage:\n    runs-on: x\n    environment:\n      name: stage\n    steps: [{run: x}]\n  test:\n    runs-on: x\n    steps: [{run: x}]\n";
        let w = pod_actions_workflow::parse(text).unwrap().workflow;
        let args = |protected: &[&str], approved: &[&str]| Args {
            protected: protected.iter().map(|s| s.to_string()).collect(),
            approved: approved.iter().map(|s| s.to_string()).collect(),
            ..Args::default()
        };
        // The called workflow names its environment, and calls one more.
        struct Texts;
        impl Called for Texts {
            fn text(&self, uses: &str, from: Option<&str>) -> Result<String, String> {
                Ok(match (uses, from) {
                    ("./.github/workflows/rel.yml", None) => "on: workflow_call\njobs:\n  r:\n    runs-on: x\n    environment: release\n    steps: [{run: x}]\n  n:\n    uses: ./.github/workflows/pub.yml\n",
                    ("./.github/workflows/pub.yml", Some("./.github/workflows/rel.yml")) => "on: workflow_call\njobs:\n  p:\n    runs-on: x\n    environment: {name: publish}\n    steps: [{run: x}]\n",
                    _ => return Err(format!("no {uses} from {from:?}")),
                }
                .to_string())
            }
        }
        let run = |a: &Args, answer: Option<bool>| {
            let mut envs = BTreeMap::new();
            let mut asked = Vec::new();
            hold(a, &w, Some(&Texts), &mut envs, &mut |n| {
                asked.push(n.to_string());
                answer
            });
            (envs.into_iter().map(|(k, e)| (k, e.refused.unwrap_or_default())).collect::<Vec<_>>(), asked)
        };
        let (held, asked) = run(&args(&["prod", "stage", "unused"], &["stage"]), None);
        assert_eq!(asked, ["prod"], "an approved or unused environment asks nothing");
        // T-419: an environment that no job names is held too, as an
        // expression can name it.
        assert_eq!(held.iter().map(|h| h.0.as_str()).collect::<Vec<_>>(), ["prod", "unused"]);
        assert!(held.iter().all(|h| h.1.contains(&format!("--approve {}", h.0))), "{held:?}");
        let (held, _) = run(&args(&["prod"], &[]), Some(false));
        assert!(held[0].1.contains("refused"), "{held:?}");
        assert!(run(&args(&["prod"], &[]), Some(true)).0.is_empty());
        let only_test = Args { jobs: vec!["test".into()], ..args(&["prod"], &[]) };
        assert!(run(&only_test, None).1.is_empty(), "--job test runs no job of prod");
        // T-419: `--job` asks for the environment of each job that it needs,
        // and the jobs of a called workflow ask for theirs, to each level.
        let after = Args { jobs: vec!["after".into()], ..args(&["prod"], &[]) };
        assert_eq!(run(&after, None).1, ["prod"], "--job after needs deploy");
        let called = args(&["release", "publish"], &[]);
        assert_eq!(run(&called, None).1, ["publish", "release"]);
    }

    /// T-323 F23: a flag that names an environment of no job gets a line.
    #[test]
    fn an_environment_of_no_job_is_named() {
        let w = pod_actions_workflow::parse(
            "on: push\njobs:\n  a:\n    runs-on: x\n    environment: prod\n    steps: [{run: x}]\n",
        )
        .unwrap()
        .workflow;
        let a =
            Args { approved: vec!["prod".into(), "nosuch".into()], protected: vec!["prod".into()], ..Args::default() };
        assert_eq!(
            unknown(&a, &w, None),
            ["podrunner: --approve nosuch: no job of the workflow names the environment nosuch"]
        );
        let w = pod_actions_workflow::parse(
            "on: push\njobs:\n  a:\n    runs-on: x\n    environment: ${{ inputs.e }}\n    steps: [{run: x}]\n",
        )
        .unwrap()
        .workflow;
        assert!(unknown(&a, &w, None).is_empty(), "an expression can name each");
    }
}
