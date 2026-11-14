//! The concurrency groups of a local run (T-137): the `concurrency` of the
//! workflow and of each job, evaluated with the contexts of the run (and the
//! `matrix` of a cell). The caller holds each group across runs.

use pod_actions_expr::{Contexts, Template, Value as XValue};
use pod_actions_workflow::{Kind, Node};
use podrunner_schema::JobId;

use crate::LocalSource;

/// A group and whether a new run cancels the run that holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub name: String,
    pub cancel_in_progress: bool,
}

fn text(t: &str, c: &Contexts) -> Result<String, String> {
    let v = Template::parse(t).and_then(|t| t.evaluate(c)).map_err(|e| format!("{t:?}: {e}"))?;
    Ok(v.to_text())
}

/// The group of a `concurrency` node: a string, or `group:` and
/// `cancel-in-progress:` of a mapping.
fn group(node: &Node, c: &Contexts) -> Result<Option<Group>, String> {
    let at = |e: String| format!("line {}: concurrency: {e}", node.mark.line);
    match &node.kind {
        Kind::String(s) => Ok(Some(Group { name: text(s, c).map_err(at)?, cancel_in_progress: false })),
        Kind::Map(e) => {
            let get = |k: &str| e.iter().find(|(key, _)| key.name == k).map(|(_, v)| v);
            let Some(Node { kind: Kind::String(g), .. }) = get("group") else { return Err(at("no group".into())) };
            let cancel = match get("cancel-in-progress").map(|n| &n.kind) {
                None => false,
                Some(Kind::Bool(b)) => *b,
                Some(Kind::String(s)) => text(s, c).map_err(at)? == "true",
                Some(_) => return Err(at("cancel-in-progress is a boolean".into())),
            };
            Ok(Some(Group { name: text(g, c).map_err(at)?, cancel_in_progress: cancel }))
        }
        _ => Ok(None),
    }
}

impl LocalSource {
    fn contexts(&self, matrix: Option<&XValue>) -> Contexts {
        let mut c = Contexts::new();
        c.insert("github", self.github.clone());
        c.insert("inputs", self.inputs.clone());
        c.insert("vars", self.vars.clone());
        if let Some(m) = matrix {
            c.insert("matrix", m.clone());
        }
        c
    }

    /// The group of the run: `concurrency` of the workflow.
    pub fn run_group(&self) -> Result<Option<Group>, String> {
        match &self.run.workflow().concurrency {
            Some(n) => group(n, &self.contexts(None)),
            None => Ok(None),
        }
    }

    /// The group of the job of a cell: its `concurrency`.
    pub fn job_group(&self, job: &JobId) -> Result<Option<Group>, String> {
        let Some(cell) = self.cells.get(job) else { return Ok(None) };
        match self.run.workflow().job(&cell.job).and_then(|j| j.concurrency.as_ref()) {
            Some(n) => group(n, &self.contexts(Some(&cell.instance.matrix))),
            None => Ok(None),
        }
    }
}
