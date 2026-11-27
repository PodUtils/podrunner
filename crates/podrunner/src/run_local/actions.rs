//! The actions and the images of `podrunner run` by tag (T-118, T-130): each
//! remote `uses:` of the workflow, and of each composite action that it
//! names, and each image that a tag names (for a unit kind), is pinned by
//! the lock file beside the workflow before the first job. Each tarball is
//! kept in the store (`--actions DIR`, by default the cache directory of the
//! user); each image is pulled into the store of the units.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use podrunner_local::images::{ImageSource, workflow_images};
use podrunner_local::localrepo::Local;
use podrunner_local::lock::{LOCK_FILE, Lock};
use podrunner_local::store::{Git, Mode, Store, add_trees, resolve, workflow_trees, workflow_uses};

use super::Args;

/// The cells of a static matrix (#225).
#[path = "matrix.rs"]
mod matrix;

/// The forge of the actions, unless `--action-server URL` (T-139).
const FORGE: &str = "https://github.com/";

/// A directory of the cache of the user: `$XDG_CACHE_HOME/podrunner/NAME`,
/// or `$HOME/.cache/podrunner/NAME`.
pub(super) fn user_cache(name: &str) -> Result<PathBuf, String> {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").filter(|v| !v.is_empty()).map(|h| PathBuf::from(h).join(".cache")))
        .ok_or_else(|| format!("no XDG_CACHE_HOME and no HOME: give the directory with --{name} DIR"))?;
    Ok(cache.join("podrunner").join(name))
}

/// The actions and the images of a run.
pub(super) struct Pinned {
    /// The store of the tarballs, when the run has an action or a tree.
    pub dir: Option<PathBuf>,
    /// The commit of each `OWNER/REPO@REF`, and `NAME@sha256:HEX` of each
    /// `docker://NAME:TAG`.
    pub commits: BTreeMap<String, String>,
    /// A line for stderr when the lock changed.
    pub note: String,
    /// The checkout steps of the composite actions of the store (T-718).
    pub checkouts: Vec<podrunner_local::store::checkouts::Checkout>,
}

/// The actions of the run, or `None` when the workflow names no action of a
/// repository.
pub(super) fn pin(
    a: &Args,
    text: &str,
    mode: Mode,
    self_repo: &str,
    images: Option<&dyn ImageSource>,
    no_services: bool,
) -> Result<Option<Pinned>, String> {
    let (file, store) = (a.file.as_path(), a.actions.as_deref());
    // The errors of `pod-actions-store` as the text of a run.
    let s = |e: podrunner_local::store::Error| e.to_string();
    let workflow = pod_actions_workflow::parse(text).map_err(|e| e.to_string())?.workflow;
    let mut uses = workflow_uses(&workflow);
    // The local composite actions of the tree (T-346), and the workflows of
    // the tree that a job calls, with their own (#238).
    let tree = a.checkout.clone().or_else(|| std::env::current_dir().ok()).unwrap_or_default();
    let called = super::composites::local_calls(&workflow, &tree)?;
    for w in std::iter::once(&workflow).chain(&called) {
        let more = workflow_uses(w).into_iter().chain(super::composites::local_uses(w, &tree)?);
        for u in more {
            if !uses.contains(&u) {
                uses.push(u);
            }
        }
    }
    let mut trees = workflow_trees(&workflow, self_repo).map_err(s)?;
    for w in &called {
        for t in workflow_trees(w, self_repo).map_err(s)? {
            if !trees.contains(&t) {
                trees.push(t);
            }
        }
    }
    // The checkouts of other repositories in the local actions (T-713).
    for w in std::iter::once(&workflow).chain(&called) {
        for t in super::composites::local_trees(w, &tree, self_repo)? {
            if !trees.contains(&t) {
                trees.push(t);
            }
        }
    }
    // The repository of each remote called workflow (T-131): its tree goes
    // into the store at the commit of the lock, as an action.
    for job in &workflow.jobs {
        if let Some(u) = job.uses.as_deref().filter(|u| !u.starts_with("./"))
            && let Some((left, r)) = u.rsplit_once('@')
        {
            let repo: Vec<&str> = left.splitn(3, '/').take(2).collect();
            let key = format!("{}@{r}", repo.join("/"));
            if !trees.contains(&key) {
                trees.push(key);
            }
        }
    }
    // No fetch of a name with `..` (#261): the actions, the checkouts of
    // other repositories and the repositories of called workflows.
    uses.iter().chain(&trees).try_for_each(|u| podrunner_local::store::repo_ok(u))?;
    let tags = match images {
        Some(_) => {
            // The executor refuses services: their images stay out (T-327).
            let mut w = workflow.clone();
            w.jobs.iter_mut().filter(|_| no_services).for_each(|j| j.services = None);
            let (w, more) = run_jobs(&w, &a.jobs);
            let mut t = workflow_images(&w).map_err(s)?;
            t.extend(more);
            // The images of the called workflows of the tree (#238).
            for c in &called {
                let (c, more) = run_jobs(c, &[]);
                t.extend(workflow_images(&c).map_err(s)?);
                t.extend(more);
            }
            t.sort();
            t.dedup();
            t
        }
        None => Vec::new(),
    };
    if uses.is_empty() && trees.is_empty() && tags.is_empty() {
        return Ok(None);
    }
    let lock_path = file.parent().unwrap_or(Path::new(".")).join(LOCK_FILE);
    let lock = Lock::read(&lock_path).map_err(s)?;
    let name = lock_path.display().to_string();
    let mut checkouts = Vec::new();
    let (mut r, dir) = if uses.is_empty() && trees.is_empty() {
        (podrunner_local::store::Resolved { lock, ..Default::default() }, None)
    } else {
        let dir = match store {
            Some(d) => d.to_path_buf(),
            None => user_cache("actions")?,
        };
        // Any git server serves `git ls-remote` and a fetch: GitHub
        // Enterprise Server, Gitea, Forgejo (T-139).
        let base = a.action_server.as_deref().map_or(FORGE.to_string(), |s| format!("{}/", s.trim_end_matches('/')));
        // When `--local-repository` serves each reference of the file, no
        // forge is asked: the run opens no relay, and a reference that a
        // local action names comes from the lock and the store (T-211).
        let local = |u: &String| {
            u.rsplit_once('@').is_some_and(|(left, r)| {
                let repo: Vec<&str> = left.splitn(3, '/').take(2).collect();
                a.local_repos.iter().any(|(k, _)| *k == format!("{}@{r}", repo.join("/")))
            })
        };
        // A run whose references are each local asks no forge first; a
        // reference that a local composite names then takes the mode of
        // the run (T-346).
        let local_only = !matches!(mode, Mode::Offline) && uses.iter().chain(&trees).all(local);
        let store = Store { dir: dir.clone() };
        type Attempt = (podrunner_local::store::Resolved, Vec<podrunner_local::store::checkouts::Checkout>);
        let attempt = |mode: Mode| -> Result<Attempt, podrunner_local::store::Error> {
            // The fetch goes through one egress grant of the forge host
            // only (T-154); `--offline` fetches nothing and opens none.
            let fetch = match mode {
                Mode::Offline => None,
                _ => open_fetch(a, &base)
                    .map_err(|reason| podrunner_local::store::Error::Forge { repo: String::new(), reason })?,
            };
            let mut git = Git::new(base.clone(), std::env::temp_dir());
            git.proxy = fetch.as_ref().map(|f| f.proxy.clone());
            let forge = Local::new(a.local_repos.iter().cloned().collect(), &git);
            // The checkouts of other repositories in the composite actions
            // of the store (T-718): the actions are in the store after
            // `resolve`. A local action that one names (T-725) can name more
            // actions: resolve again until no new one comes.
            let mut uses = uses.clone();
            let (r, found) = loop {
                let r = resolve(&uses, &lock, &store, &forge, mode, &name)?;
                let mut found = podrunner_local::store::checkouts::action_checkouts(&uses, &store.dir, &r.commits);
                let (more, local) = super::composites::store_locals(&found.locals, &tree).map_err(|e| {
                    let reason = format!("a local action that an action of the store names: {e}");
                    podrunner_local::store::Error::Forge { repo: String::new(), reason }
                })?;
                found.checkouts.extend(local);
                let more: Vec<String> = more.into_iter().filter(|u| !uses.contains(u)).collect();
                if more.is_empty() {
                    break (r, found.checkouts);
                }
                // No fetch of a name with `..` (#261).
                for u in &more {
                    podrunner_local::store::repo_ok(u)
                        .map_err(|_| podrunner_local::store::Error::Uses { uses: u.clone() })?;
                }
                uses.extend(more);
            };
            let mut all = trees.clone();
            // A name with `..` gets no tree (#261): the stand-in fails its step.
            for t in super::composites::trees_of(&found, self_repo) {
                if !all.contains(&t) && podrunner_local::store::repo_ok(&t).is_ok() {
                    all.push(t);
                }
            }
            Ok((add_trees(&all, r, &store, &forge, mode, &name)?, found))
        };
        let r = match local_only {
            true => match attempt(Mode::Offline) {
                Err(
                    podrunner_local::store::Error::NotLocked { .. } | podrunner_local::store::Error::NotStored { .. },
                ) => attempt(mode),
                r => r,
            },
            false => attempt(mode),
        };
        let (mut r, found) = r.map_err(|e| with_fix(&e, mode))?;
        // The broker gives the stand-in for each ref of `actions/checkout`.
        r.commits.retain(|k, _| !podrunner_local::store::is_checkout(k));
        checkouts = found;
        (r, Some(dir))
    };
    if let (Some(source), false) = (images, tags.is_empty()) {
        let (pins, changed) = podrunner_local::images::resolve(&tags, &mut r.lock, source, mode, &name).map_err(s)?;
        r.commits.extend(pins);
        r.changed |= changed;
    }
    let mut note = String::new();
    if r.changed {
        r.lock.write(&lock_path).map_err(s)?;
        note = format!("podrunner: {name} pins {} references\n", r.lock.pins.len() + r.lock.images.len());
    }
    Ok(Some(Pinned { dir, commits: r.commits, note, checkouts }))
}

/// The grant of the forge for a fetch, or `None` for a forge on a local
/// network.
fn open_fetch(a: &Args, base: &str) -> Result<Option<podrunner_local::fetch::Fetch>, String> {
    let host = podrunner_local::fetch::host_of(base).ok_or_else(|| format!("{base}: no host"))?;
    match a.action_server.is_some() && on_local_network(&host) {
        // The egress module refuses the loopback and a private network; the
        // user named this forge (T-239).
        true => {
            eprintln!(
                "podrunner: --action-server {host} is on the loopback or a private network: git reaches it directly, with no egress grant"
            );
            Ok(None)
        }
        false => podrunner_local::fetch::Fetch::open(&host).map(Some),
    }
}

/// The text of a failed pin, with its fix: a reference that the lock or
/// the store does not have (T-346).
fn with_fix(e: &podrunner_local::store::Error, mode: Mode) -> String {
    use podrunner_local::store::Error;
    let fix = match (e, mode) {
        (Error::NotLocked { .. }, Mode::Frozen) => {
            "; run with no --frozen to pin it, or serve it with --local-repository OWNER/REPO@REF=DIR"
        }
        (Error::NotLocked { .. } | Error::NotStored { .. }, Mode::Offline) => {
            "; run with no --offline to fetch it, or serve it with --local-repository OWNER/REPO@REF=DIR"
        }
        _ => "",
    };
    // git asks for a login where the forge has no such public repository
    // (T-339 item 15).
    let text = e.to_string();
    let login = match text.contains("terminal prompts disabled") || text.contains("could not read Username") {
        true => "; the forge asked for a login: the repository of the action is not there, or it is private",
        false => "",
    };
    // A fetch that the way out refused: the proxy, or the resolver of the
    // egress module (T-351, F1 of the user QA of beta.7).
    let way = match text.contains("CONNECT tunnel failed") || text.contains("Could not resolve proxy") {
        true => {
            "; the fetch goes through the egress module of podrunner and the proxy of the environment (HTTPS_PROXY), and that way failed: check the proxy with `podrunner doctor`, or serve the action with --local-repository OWNER/REPO@REF=DIR"
        }
        false => "",
    };
    format!("{text}{fix}{login}{way}")
}

/// The workflow with the jobs whose images the run pulls (T-260): the jobs
/// of `--job` and each job that they need (each job with no `--job`). An
/// image that takes values of the matrix of its job gives one image for
/// each combination of those values (the second part), and leaves the
/// workflow; an image with another expression leaves it too, is named
/// here, and its job fails at its start, as no store holds it.
fn run_jobs(w: &pod_actions_workflow::Workflow, selected: &[String]) -> (pod_actions_workflow::Workflow, Vec<String>) {
    use pod_actions_workflow::yaml::{Kind, Node};
    let mut keep: std::collections::BTreeSet<String> = match selected.is_empty() {
        true => w.jobs.iter().map(|j| j.id.clone()).collect(),
        false => selected.iter().cloned().collect(),
    };
    loop {
        let more: Vec<String> = w
            .jobs
            .iter()
            .filter(|j| keep.contains(&j.id))
            .flat_map(|j| j.needs.clone())
            .filter(|n| !keep.contains(n))
            .collect();
        if more.is_empty() {
            break;
        }
        keep.extend(more);
    }
    let mut out = w.clone();
    out.jobs.retain(|j| keep.contains(&j.id));
    let mut more = Vec::new();
    for j in &mut out.jobs {
        // The cells that run: `exclude` and `include` hold (#225).
        let cells = matrix::cells(j);
        // The images of an image with values of the matrix, else `None`.
        let expand = |image: &str| -> Option<Vec<String>> {
            let mut out: Vec<String> = Vec::new();
            for c in &cells {
                let one = podrunner_local::message::image_with(image, |key| {
                    c.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.to_string())
                })?;
                if !out.contains(&one) {
                    out.push(one);
                }
            }
            Some(out)
        };
        // Each image of a node: the node stays when it has no expression.
        let mut take = |n: &Node| -> bool {
            let image = match &n.kind {
                Kind::String(s) => Some(s.clone()),
                Kind::Map(_) => n.get("image").and_then(|i| match &i.kind {
                    Kind::String(s) => Some(s.clone()),
                    _ => None,
                }),
                _ => None,
            };
            let Some(image) = image.filter(|i| i.contains("${{")) else { return true };
            match expand(&image) {
                Some(list) => more.extend(list.iter().filter_map(|i| podrunner_local::images::by_tag(i))),
                None => eprintln!(
                    "podrunner: job {}: the image {image:?} is an expression that is not a value of its matrix, which no store holds: the job fails at its start (T-260)",
                    j.id
                ),
            }
            false
        };
        if j.container.as_ref().is_some_and(|c| !take(c)) {
            j.container = None;
        }
        if let Some(Node { kind: Kind::Map(m), .. }) = &mut j.services {
            m.retain(|(_, s)| take(s));
        }
    }
    (out, more)
}

/// `localhost`, or an address of the loopback, of a private network or a
/// link-local one (a name that resolves to one is not looked up).
fn on_local_network(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        Ok(std::net::IpAddr::V6(ip)) => ip.is_loopback() || (ip.segments()[0] & 0xfe00) == 0xfc00,
        Err(_) => host.eq_ignore_ascii_case("localhost"),
    }
}

#[cfg(test)]
mod fix_tests {
    /// T-339 item 15: a login prompt of git names a missing repository.
    #[test]
    fn a_login_prompt_names_a_missing_repository() {
        let e = podrunner_local::store::Error::Forge {
            repo: "me/none".into(),
            reason: "fatal: could not read Username for 'https://github.com': terminal prompts disabled".into(),
        };
        let t = super::with_fix(&e, podrunner_local::store::Mode::Online);
        assert!(t.contains("is not there, or it is private"), "{t}");
        // T-351: a refused way out names the proxy and the fix.
        let e = podrunner_local::store::Error::Forge {
            repo: "actions/upload-artifact".into(),
            reason: "git ls-remote: fatal: unable to access 'https://github.com/actions/upload-artifact/': CONNECT tunnel failed, response 502".into(),
        };
        let t = super::with_fix(&e, podrunner_local::store::Mode::Online);
        assert!(t.contains("proxy") && t.contains("--local-repository"), "{t}");
    }
}

#[cfg(test)]
mod local_network_tests {
    /// T-239: a forge on the loopback or a private network.
    #[test]
    fn a_forge_on_a_local_network() {
        for h in ["127.0.0.1", "localhost", "10.1.2.3", "192.168.0.5", "[::1]", "fd00::5"] {
            assert!(super::on_local_network(h), "{h}");
        }
        for h in ["github.com", "8.8.8.8", "gitea.example"] {
            assert!(!super::on_local_network(h), "{h}");
        }
    }
}

#[cfg(test)]
mod run_jobs_tests {
    /// T-260: a job with an image of an expression is left out of the pull,
    /// the others stay; an expression in `env:` of a service is no image.
    #[test]
    fn a_job_with_an_image_of_an_expression_is_left_out() {
        let text = "on: push\njobs:\n  lint:\n    runs-on: ubuntu-latest\n    steps: [{run: 'true'}]\n  \
            db:\n    runs-on: ubuntu-latest\n    needs: lint\n    strategy: {matrix: {v: [15, '16']}}\n    \
            services:\n      pg: {image: 'postgres:${{ matrix.v }}'}\n    steps: [{run: 'true'}]\n  \
            other:\n    runs-on: ubuntu-latest\n    services:\n      pg:\n        image: postgres:17\n        \
            env: {P: '${{ secrets.P }}'}\n      x: {image: 'x:${{ github.sha }}'}\n    steps: [{run: 'true'}]\n";
        let w = pod_actions_workflow::parse(text).unwrap().workflow;
        let ids = |w: &pod_actions_workflow::Workflow| w.jobs.iter().map(|j| j.id.clone()).collect::<Vec<_>>();
        let (all, more) = super::run_jobs(&w, &[]);
        assert_eq!(ids(&all), ["lint", "db", "other"]);
        assert_eq!(more, ["postgres:15", "postgres:16"], "one image for each value of the matrix");
        let tags = podrunner_local::images::workflow_images(&all).unwrap();
        assert_eq!(tags, ["postgres:17"], "the image of another expression leaves; an env expression is no image");
        assert_eq!(ids(&super::run_jobs(&w, &["lint".to_string()]).0), ["lint"]);
        assert_eq!(ids(&super::run_jobs(&w, &["db".to_string()]).0), ["lint", "db"], "db needs lint");
    }

    /// #225: the image of an excluded cell is not pulled.
    #[test]
    fn the_image_of_an_excluded_cell_is_not_pulled() {
        let text = "on: push\njobs:\n  db:\n    runs-on: x\n    strategy:\n      matrix:\n        v: [15, 16]\n        \
            exclude: [{v: 16}]\n    container: 'postgres:${{ matrix.v }}'\n    steps: [{run: 'true'}]\n";
        let w = pod_actions_workflow::parse(text).unwrap().workflow;
        assert_eq!(super::run_jobs(&w, &[]).1, ["postgres:15"]);
    }
}
