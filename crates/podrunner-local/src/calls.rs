//! The calls of reusable workflows (T-131): `jobs.ID.uses` names a workflow
//! of the tree (`./.github/workflows/x.yml`) or of another repository
//! (`OWNER/REPO/.github/workflows/x.yml@REF`, pinned by the lock and kept
//! in the store as an action, T-118). `pod-actions-workflow` expands each
//! call into jobs of the run (`CALLER/CALLEE`); this module gives it the
//! text of each called workflow, keeps the workflow of each called job for
//! its job message, and gives each job the secrets that its call passes.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pod_actions_expr::{Contexts, Template, Value as XValue};
use pod_actions_workflow::{Call, Called, Secrets, Workflow};
use podrunner_schema::JobId;

/// The texts of the called workflows: the tree, and the store of the
/// pinned repositories.
pub struct Texts {
    /// The root of the tree (`--checkout`, or the current directory).
    pub root: PathBuf,
    /// The store of the tarballs (`DIR/OWNER_REPO/SHA.tar.gz`).
    pub store: Option<PathBuf>,
    /// The commit of each `OWNER/REPO@REF`.
    pub pins: BTreeMap<String, String>,
    cache: Mutex<BTreeMap<String, String>>,
}

impl Texts {
    pub fn new(root: PathBuf, store: Option<PathBuf>, pins: BTreeMap<String, String>) -> Texts {
        Texts { root, store, pins, cache: Mutex::default() }
    }

    /// The file `path` of `repo` at the commit of `r`, from the store.
    fn remote(&self, repo: &str, r: &str, path: &str) -> Result<String, String> {
        let full = r.len() == 40 && r.bytes().all(|b| b.is_ascii_hexdigit());
        let commit = match self.pins.get(&format!("{repo}@{r}")) {
            Some(c) => c.clone(),
            None if full => r.to_string(),
            None => return Err(format!("`{repo}@{r}` is not pinned: run once with no --offline")),
        };
        let store = self.store.as_ref().ok_or("no store of the actions")?;
        let file = store.join(podrunner_broker_fake::actions::archive_name(repo, &commit));
        let gz = std::fs::File::open(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(gz));
        for entry in archive.entries().map_err(|e| e.to_string())? {
            let mut entry = entry.map_err(|e| e.to_string())?;
            let p = entry.path().map_err(|e| e.to_string())?.into_owned();
            // One top directory, as the tarballs of GitHub.
            if p.components().skip(1).collect::<PathBuf>() == std::path::Path::new(path) {
                let mut text = String::new();
                entry.read_to_string(&mut text).map_err(|e| e.to_string())?;
                return Ok(text);
            }
        }
        Err(format!("`{repo}@{r}` has no file `{path}`"))
    }
}

/// `OWNER/REPO`, the path and the ref of `OWNER/REPO/PATH@REF`.
fn split(uses: &str) -> Option<(String, String, String)> {
    let (left, r) = uses.rsplit_once('@')?;
    let mut parts = left.splitn(3, '/');
    let (owner, repo, path) = (parts.next()?, parts.next()?, parts.next()?);
    Some((format!("{owner}/{repo}"), path.to_string(), r.to_string()))
}

impl Called for Texts {
    fn text(&self, uses: &str, from: Option<&str>) -> Result<String, String> {
        let key = format!("{uses} {}", from.unwrap_or(""));
        if let Some(t) = self.cache.lock().expect("the texts").get(&key) {
            return Ok(t.clone());
        }
        let text = match (uses.strip_prefix("./"), from.filter(|f| !f.starts_with("./"))) {
            // A `./` call in a workflow of another repository: that repository.
            (Some(rel), Some(remote)) => {
                let (repo, _, r) = split(remote).ok_or_else(|| format!("`{remote}` is not OWNER/REPO/PATH@REF"))?;
                self.remote(&repo, &r, rel)?
            }
            (Some(rel), None) => {
                let file = self.root.join(rel);
                std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?
            }
            (None, _) => {
                let (repo, path, r) = split(uses).ok_or_else(|| format!("`{uses}` is not OWNER/REPO/PATH@REF"))?;
                self.remote(&repo, &r, &path)?
            }
        };
        self.cache.lock().expect("the texts").insert(key, text.clone());
        Ok(text)
    }
}

/// The workflow of each called job: its workflow, its id in that workflow,
/// and the `uses` of that workflow.
#[derive(Default)]
pub struct Calls {
    jobs: BTreeMap<String, (Arc<Workflow>, String, String)>,
}

impl Calls {
    /// The `uses` of the workflow that holds the job `node`; `None` for a
    /// job of the top workflow.
    pub fn uses_of(&self, node: &str) -> Option<&str> {
        self.jobs.get(node).map(|(_, _, u)| u.as_str())
    }

    /// Keep the called jobs of `call`.
    pub fn add(&mut self, call: &Call, texts: &dyn Called) -> Result<(), String> {
        let from = self.uses_of(&call.job).map(str::to_string);
        let text = texts.text(&call.uses, from.as_deref())?;
        let workflow = Arc::new(pod_actions_workflow::parse(&text).map_err(|e| e.to_string())?.workflow);
        for node in &call.jobs {
            let callee = node.rsplit('/').next().unwrap_or(node).to_string();
            self.jobs.insert(node.clone(), (workflow.clone(), callee, call.uses.clone()));
        }
        Ok(())
    }

    /// The workflow and the id in it of the called job `node`.
    pub fn get(&self, node: &str) -> Option<(&Workflow, &str)> {
        self.jobs.get(node).map(|(w, j, _)| (w.as_ref(), j.as_str()))
    }
}

/// The offer id of cell `index` of the job `node`: a called job
/// `CALLER/CALLEE` (or `CALLER[N]/CALLEE`) takes `CALLER__CALLEE`
/// (`CALLER-N__CALLEE`), as an id takes no `/`, `[` or `]`.
pub fn cell_id(node: &str, index: usize) -> Result<JobId, String> {
    let id = node.replace('/', "__").replace('[', "-").replace(']', "");
    JobId::new(&format!("{id}.{index}")).map_err(|e| format!("the job id of {node}: {e:?}"))
}

/// The secrets of a job: each secret of the run, or the ones that its call
/// passes, each template evaluated with the secrets of the caller.
pub fn values(secrets: &Secrets, run: &[(String, String)]) -> Vec<(String, String)> {
    match secrets {
        Secrets::Run => run.to_vec(),
        Secrets::Passed { from, items } => {
            let parent = values(from, run);
            let mut contexts = Contexts::new();
            let obj = XValue::object(parent.iter().map(|(k, v)| (k.clone(), XValue::String(v.clone()))));
            contexts.insert("secrets", obj);
            items
                .iter()
                .map(|(name, template)| {
                    let v = Template::parse(template)
                        .and_then(|t| t.evaluate(&contexts))
                        .map(|v| v.as_str().map(str::to_string).unwrap_or_default())
                        .unwrap_or_default();
                    (name.clone(), v)
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests;
