//! The environment of a job (T-136). Its name is a text, or `name:` of a
//! mapping; an expression in it reads the contexts of the cell, as on
//! GitHub: `github`, `inputs`, `vars`, `matrix`, `strategy` and `needs`
//! (T-420).

use pod_actions_expr::{Contexts, Template};
use pod_actions_workflow::{Called, Kind};
use podrunner_schema::JobId;

use crate::{Cell, LocalSource};

impl LocalSource {
    /// The contexts of an expression of the job of `cell`, or of the run.
    /// A called job reads the inputs of its call.
    pub(crate) fn contexts_of(&self, cell: Option<&Cell>) -> Contexts {
        let mut c = Contexts::new();
        c.insert("github", self.github.clone());
        c.insert("vars", self.vars.clone());
        match cell {
            Some(cell) => {
                let i = &cell.instance;
                c.insert("inputs", i.inputs.clone());
                c.insert("matrix", i.matrix.clone());
                c.insert("strategy", i.strategy.clone());
                c.insert("needs", i.needs.clone());
            }
            None => {
                c.insert("inputs", self.inputs.clone());
            }
        }
        c
    }

    /// The name of the environment of the job `job`, with its expressions
    /// evaluated. An empty name is no environment.
    pub fn environment_of(&self, job: &JobId) -> Result<Option<String>, String> {
        let Some(cell) = self.cells.get(job) else { return Ok(None) };
        let (w, id) = self.job_of(cell);
        let Some(node) = w.job(id).and_then(|j| j.environment.as_ref()) else {
            return Ok(None);
        };
        let Some(name) = name_of(&node.kind) else { return Ok(None) };
        if !name.contains("${{") {
            return Ok(Some(name));
        }
        let text = Template::parse(&name)
            .and_then(|t| t.evaluate(&self.contexts_of(Some(cell))))
            .map_err(|e| format!("line {}: the environment {name:?}: {e}", node.mark.line))?
            .to_text();
        Ok((!text.is_empty()).then_some(text))
    }

    /// The texts of the called workflows of the run.
    pub fn called(&self) -> Option<&dyn Called> {
        self.texts.as_deref().map(|t| t as &dyn Called)
    }
}

/// The name of an `environment:` node, as written: a text, or `name:` of a
/// mapping.
pub fn name_of(kind: &Kind) -> Option<String> {
    match kind {
        Kind::String(s) => Some(s.clone()),
        Kind::Map(e) => e.iter().find(|(k, _)| k.name == "name").and_then(|(_, v)| match &v.kind {
            Kind::String(s) => Some(s.clone()),
            _ => None,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::Duration;

    use pod_actions_expr::Value as XValue;
    use pod_actions_workflow::Base;
    use podrunner_source::{Event, JobSource};

    use super::*;
    use crate::calls::Texts;

    fn offers(s: &mut LocalSource) -> Vec<JobId> {
        let events = s.next(10, Duration::ZERO).unwrap();
        events
            .into_iter()
            .filter_map(|e| if let Event::Offer { offer } = e { Some(offer.job_id) } else { None })
            .collect()
    }

    /// T-420: the name of an environment is evaluated with the contexts of
    /// its cell; an empty name is no environment.
    #[test]
    fn the_name_of_an_environment_is_evaluated() {
        let text = "on: push\njobs:\n  a:\n    runs-on: x\n    environment: ${{ inputs.target }}\n    steps: [{run: x}]\n  \
                    m:\n    runs-on: x\n    strategy: {matrix: {env: [stage, prod]}}\n    environment:\n      \
                    name: deploy-${{ matrix.env }}\n    steps: [{run: x}]\n  \
                    e:\n    runs-on: x\n    environment: ${{ vars.NONE }}\n    steps: [{run: x}]\n";
        let inputs = XValue::object([("target", XValue::String("prod".into()))]);
        let mut s =
            LocalSource::new("l", text, Base { inputs, ..Base::default() }, "x86_64", Box::new(|_, _| unreachable!()))
                .unwrap();
        let mut got: Vec<(String, Option<String>)> =
            offers(&mut s).iter().map(|j| (j.as_str().to_string(), s.environment_of(j).unwrap())).collect();
        got.sort();
        let want = [("a.0", Some("prod")), ("e.0", None), ("m.0", Some("deploy-stage")), ("m.1", Some("deploy-prod"))];
        assert_eq!(got, want.map(|(j, e)| (j.to_string(), e.map(str::to_string))));
    }

    /// T-420: a called job reads the inputs of its call, in the name of its
    /// environment and in its concurrency group.
    #[test]
    fn a_called_job_reads_the_inputs_of_its_call() {
        let root = std::env::temp_dir().join(format!("podrunner-env-call-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".github/workflows")).unwrap();
        std::fs::write(
            root.join(".github/workflows/deploy.yml"),
            "on:\n  workflow_call:\n    inputs:\n      env: {type: string, required: true}\njobs:\n  d:\n    \
             runs-on: x\n    environment: ${{ inputs.env }}\n    concurrency: deploy-${{ inputs.env }}\n    steps: [{run: x}]\n",
        )
        .unwrap();
        let text = "on: push\njobs:\n  call:\n    uses: ./.github/workflows/deploy.yml\n    with: {env: prod}\n";
        let texts = Arc::new(Texts::new(root.clone(), None, BTreeMap::new()));
        let mut s =
            LocalSource::new_with("l", text, Base::default(), "x86_64", Box::new(|_, _| unreachable!()), Some(texts))
                .unwrap();
        let jobs = offers(&mut s);
        assert_eq!(jobs.iter().map(JobId::as_str).collect::<Vec<_>>(), ["call__d.0"]);
        assert_eq!(s.environment_of(&jobs[0]).unwrap().as_deref(), Some("prod"));
        assert_eq!(s.job_group(&jobs[0]).unwrap().unwrap().name, "deploy-prod");
        let _ = std::fs::remove_dir_all(&root);
    }
}
