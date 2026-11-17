//! The options of `podrunner run` and their checks.

use std::path::PathBuf;

/// The options of `run`.
#[derive(Debug, Default, PartialEq, Clone)]
pub(super) struct Args {
    pub(super) file: PathBuf,
    pub(super) runner_dir: Option<PathBuf>,
    pub(super) podvm: Option<PathBuf>,
    /// `--microvm PATH` (true) or `--incus PATH` (false): a guest kind of
    /// the podvm binary PATH (T-123).
    pub(super) guest: Option<(bool, PathBuf)>,
    pub(super) engine: Option<PathBuf>,
    /// `--platform LABEL=TARGET`: a runner image, or `host`, for a label of
    /// `runs-on` (T-129).
    pub(super) platforms: Vec<(String, String)>,
    /// `--job ID` and `--matrix KEY=VALUE` (T-127).
    pub(super) jobs: Vec<String>,
    pub(super) matrix: Vec<(String, String)>,
    /// `--var-file`, `--input-file`, `--env-file` and `--env` (T-126).
    pub(super) var_file: Option<PathBuf>,
    pub(super) input_file: Option<PathBuf>,
    pub(super) env_file: Option<PathBuf>,
    pub(super) env: Vec<(String, String)>,
    /// `--list`, `--graph`, `--validate`, `--dry-run` (T-128).
    /// The secret files, var files and hosts of each environment (T-136).
    pub(super) environment_secrets: Vec<(String, PathBuf)>,
    pub(super) environment_vars: Vec<(String, PathBuf)>,
    pub(super) environment_allow: Vec<(String, String)>,
    /// `--protected NAME` and `--approve NAME`: the protection of an
    /// environment (T-136).
    pub(super) protected: Vec<String>,
    pub(super) approved: Vec<String>,
    /// `--export DIR`: the workspaces of each job (T-138).
    pub(super) export: Option<PathBuf>,
    /// `--toolcache DIR`: read-only in each unit (T-138).
    pub(super) toolcache: Option<PathBuf>,
    /// `--local-repository OWNER/REPO@REF=DIR` and `--action-server URL`
    /// (T-139).
    pub(super) local_repos: Vec<(String, PathBuf)>,
    pub(super) action_server: Option<String>,
    /// `--oidc-issuer URL` and `--oidc-keys FILE` (T-140).
    pub(super) oidc_issuer: Option<String>,
    pub(super) oidc_keys: Option<PathBuf>,
    /// `--web ADDR`: the web view of the run on the loopback (T-149).
    pub(super) web: Option<std::net::SocketAddr>,
    /// `--jobs N`: N jobs at a time, each in its own unit (T-132).
    pub(super) parallel: usize,
    /// `--break JOB:STEP` and `--break-on-failure` (T-150).
    pub(super) breaks: super::pause::Breaks,
    /// `--changed-since REF`: the changed files for the `paths` filters of
    /// `on:` (T-125).
    pub(super) changed_since: Option<String>,
    /// `--state DIR`, `--rerun DIR`, `--failed`, `--refresh` (T-135).
    pub(super) state: Option<PathBuf>,
    pub(super) rerun: Option<PathBuf>,
    pub(super) failed: bool,
    pub(super) refresh: bool,
    /// `-q`, `--json`, `--log-dir` (T-134).
    pub(super) quiet: bool,
    pub(super) json: bool,
    pub(super) log_dir: Option<PathBuf>,
    pub(super) list: bool,
    pub(super) graph: bool,
    pub(super) validate: bool,
    pub(super) dry_run: bool,
    pub(super) images: Option<PathBuf>,
    pub(super) image: Option<String>,
    pub(super) actions: Option<PathBuf>,
    pub(super) offline: bool,
    pub(super) frozen: bool,
    pub(super) artifacts: Option<PathBuf>,
    pub(super) cache: Option<PathBuf>,
    pub(super) no_cache: bool,
    pub(super) secrets: Vec<String>,
    pub(super) secret_file: Option<PathBuf>,
    pub(super) token_env: Option<String>,
    pub(super) checkout: Option<PathBuf>,
    pub(super) checkout_head: bool,
    pub(super) with_git: bool,
    /// `--no-git`: no `.git` in the checkout of a git tree (T-267).
    pub(super) no_git: bool,
    pub(super) podnet: Option<PathBuf>,
    /// `--vnet PODNET`: the podvm kind on the vnet shim of podnet (T-220).
    pub(super) vnet: Option<PathBuf>,
    pub(super) allow: Vec<String>,
    pub(super) event: String,
    pub(super) event_path: Option<PathBuf>,
    pub(super) force_event: bool,
    pub(super) remote: Option<String>,
    pub(super) git_ref: Option<String>,
    pub(super) sha: Option<String>,
    pub(super) repository: Option<String>,
    pub(super) actor: Option<String>,
    pub(super) vars: Vec<(String, String)>,
    pub(super) inputs: Vec<(String, String)>,
}

pub(super) fn pair(v: &str) -> Result<(String, String), String> {
    let (k, v) = v.split_once('=').ok_or_else(|| format!("{v:?} is not NAME=VALUE"))?;
    if !pod_actions_expr::is_name(k) {
        return Err(format!("{k:?} is not a name"));
    }
    Ok((k.to_string(), v.to_string()))
}

#[path = "checkout_root.rs"]
mod checkout_root;
pub(super) use checkout_root::{calls_root, default_checkout};

/// The top of the git tree that holds `dir`.
fn toplevel(dir: &std::path::Path) -> Option<PathBuf> {
    let out =
        std::process::Command::new("git").arg("-C").arg(dir).args(["rev-parse", "--show-toplevel"]).output().ok()?;
    let top = String::from_utf8(out.stdout).ok()?;
    (out.status.success() && !top.trim().is_empty()).then(|| PathBuf::from(top.trim()))
}

/// The refusal of a workflow file whose symbolic link leads out of its
/// project: the run would read, and check out, another project (T-340).
pub(super) fn link_out(file: &std::path::Path) -> Option<String> {
    if !std::fs::symlink_metadata(file).is_ok_and(|m| m.file_type().is_symlink()) {
        return None;
    }
    let dir =
        std::fs::canonicalize(file.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(".".as_ref())).ok()?;
    let root = project_of(file).or_else(|| toplevel(&dir)).unwrap_or(dir);
    let real = std::fs::canonicalize(file).unwrap_or_else(|_| std::fs::read_link(file).unwrap_or_default());
    (!real.starts_with(&root))
        .then(|| format!("{}: the symbolic link leads out of the project ({})", file.display(), real.display()))
}

/// The project of a workflow file at `PROJECT/.github/workflows/FILE`; the
/// file itself is not followed.
fn project_of(file: &std::path::Path) -> Option<PathBuf> {
    let dir = file.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(".".as_ref());
    let file = std::fs::canonicalize(dir).ok()?.join(file.file_name()?);
    let workflows = file.parent().filter(|d| d.file_name().is_some_and(|n| n == "workflows"))?;
    let github = workflows.parent().filter(|d| d.file_name().is_some_and(|n| n == ".github"))?;
    github.parent().map(std::path::Path::to_path_buf)
}

/// A refusal of the command line, with the usage when the form of the line
/// is wrong; a wrong value names its flag only (T-331).
pub(super) fn with_usage(e: String) -> String {
    let form = e.starts_with("an unknown option")
        || e.ends_with("needs a value")
        || e.starts_with("a second file")
        || e == "the workflow file is necessary";
    match form {
        true => format!("{e}\n{}", super::USAGE),
        false => e,
    }
}

pub(super) fn parse(args: &[String]) -> Result<Args, String> {
    let mut a = Args { event: "push".into(), ..Args::default() };
    let mut it = args.iter();
    let mut file = None;
    while let Some(x) = it.next() {
        let mut value = || it.next().cloned().ok_or_else(|| format!("{x} needs a value"));
        match x.as_str() {
            "--runner-dir" => a.runner_dir = Some(PathBuf::from(value()?)),
            "--podvm" => a.podvm = Some(PathBuf::from(value()?)),
            "--microvm" => a.guest = Some((true, PathBuf::from(value()?))),
            "--incus" => a.guest = Some((false, PathBuf::from(value()?))),
            "--engine" => a.engine = Some(PathBuf::from(value()?)),
            "--job" => a.jobs.push(value()?),
            "--protected" => a.protected.push(value()?),
            "--export" => a.export = Some(PathBuf::from(value()?)),
            "--toolcache" => a.toolcache = Some(PathBuf::from(value()?)),
            "--local-repository" => a.local_repos.push(podrunner_local::localrepo::parse(&value()?)?),
            "--action-server" | "--oidc-issuer" => {
                let url = value()?;
                if !url.starts_with("https://") && !url.starts_with("http://") {
                    return Err(format!("{x} {url:?} is not a URL"));
                }
                match x.as_str() {
                    "--action-server" => a.action_server = Some(url),
                    _ => a.oidc_issuer = Some(url),
                }
            }
            "--oidc-keys" => a.oidc_keys = Some(PathBuf::from(value()?)),
            "--jobs" => {
                let v = value()?;
                a.parallel = v
                    .parse()
                    .ok()
                    .filter(|n| *n >= 1)
                    .ok_or_else(|| format!("--jobs {v:?} is not a number of 1 or more"))?;
            }
            "--break" => a.breaks.at.push(super::pause::parse(&value()?)?),
            // A failed job keeps its unit: the pause after the failed step,
            // with a shell in the unit (T-138, T-150).
            "--break-on-failure" | "--keep-failed" => a.breaks.on_failure = true,
            "--changed-since" => a.changed_since = Some(value()?),
            "--web" => a.web = Some(super::web::parse_addr(&value()?)?),
            "--approve" => a.approved.push(value()?),
            "--environment-secret-file" | "--environment-var-file" | "--environment-allow" => {
                let v = value()?;
                let (name, rest) = v
                    .split_once('=')
                    .filter(|(n, r)| !n.is_empty() && !r.is_empty())
                    .ok_or_else(|| format!("{x} {v:?} is not NAME=VALUE"))?;
                match x.as_str() {
                    "--environment-secret-file" => a.environment_secrets.push((name.into(), PathBuf::from(rest))),
                    "--environment-var-file" => a.environment_vars.push((name.into(), PathBuf::from(rest))),
                    _ => a.environment_allow.push((name.into(), rest.into())),
                }
            }
            "--state" => a.state = Some(PathBuf::from(value()?)),
            "--rerun" => a.rerun = Some(PathBuf::from(value()?)),
            "--failed" => a.failed = true,
            "--refresh" => a.refresh = true,
            "-q" | "--quiet" => a.quiet = true,
            "--json" => a.json = true,
            "--log-dir" => a.log_dir = Some(PathBuf::from(value()?)),
            "--list" => a.list = true,
            "--graph" => a.graph = true,
            "--validate" => a.validate = true,
            "--dry-run" => a.dry_run = true,
            "--var-file" => a.var_file = Some(PathBuf::from(value()?)),
            "--input-file" => a.input_file = Some(PathBuf::from(value()?)),
            "--env-file" => a.env_file = Some(PathBuf::from(value()?)),
            "--env" => a.env.push(pair(&value()?).map_err(|e| format!("--env {e}"))?),
            "--matrix" => {
                let v = value()?;
                let (k, val) = v
                    .split_once('=')
                    .filter(|(k, _)| !k.is_empty())
                    .ok_or_else(|| format!("--matrix {v:?} is not KEY=VALUE"))?;
                a.matrix.push((k.to_string(), val.to_string()));
            }
            "--platform" => {
                let v = value()?;
                let (label, target) =
                    v.split_once('=').ok_or_else(|| format!("--platform {v:?} is not LABEL=TARGET"))?;
                if label.is_empty() || label.contains(char::is_whitespace) {
                    return Err(format!("--platform {v:?}: not a label"));
                }
                if target != "host" {
                    podrunner_local::message::pinned(target).map_err(|e| format!("--platform {v:?}: {e}"))?;
                }
                a.platforms.push((label.to_string(), target.to_string()));
            }
            "--images" => a.images = Some(PathBuf::from(value()?)),
            "--image" => {
                // A tag or a short digest is refused, as for --platform (T-323 F21).
                let v = value()?;
                podrunner_local::message::pinned(&v).map_err(|e| format!("--image: {e}"))?;
                a.image = Some(v);
            }
            "--actions" => a.actions = Some(PathBuf::from(value()?)),
            "--artifacts" => a.artifacts = Some(PathBuf::from(value()?)),
            "--secret" => {
                let name = value()?;
                if name.contains('=') {
                    return Err("--secret takes a NAME of the environment, never a value in argv".into());
                }
                a.secrets.push(name);
            }
            "--secret-file" => a.secret_file = Some(PathBuf::from(value()?)),
            "--github-token-env" => a.token_env = Some(value()?),
            "--cache" => a.cache = Some(PathBuf::from(value()?)),
            "--no-cache" => a.no_cache = true,
            "--offline" => a.offline = true,
            "--frozen" => a.frozen = true,
            "--checkout" => a.checkout = Some(PathBuf::from(value()?)),
            "--checkout-head" => a.checkout_head = true,
            "--with-git" => a.with_git = true,
            "--no-git" => a.no_git = true,
            "--podnet" => a.podnet = Some(PathBuf::from(value()?)),
            "--vnet" => a.vnet = Some(PathBuf::from(value()?)),
            "--allow" => a.allow.push(value()?),
            "--event" => a.event = value()?,
            "--event-path" => a.event_path = Some(PathBuf::from(value()?)),
            "--force-event" => a.force_event = true,
            "--remote" => a.remote = Some(value()?),
            "--ref" => a.git_ref = Some(value()?),
            "--sha" => a.sha = Some(value()?),
            "--repository" => a.repository = Some(value()?),
            "--actor" => a.actor = Some(value()?),
            "--var" => a.vars.push(pair(&value()?).map_err(|e| format!("--var {e}"))?),
            "--input" => a.inputs.push(pair(&value()?).map_err(|e| format!("--input {e}"))?),
            // A short option that run does not have is not a file (T-208).
            o if o.starts_with('-') && o.len() > 1 => return Err(format!("an unknown option {o:?}")),
            f if file.is_none() => file = Some(PathBuf::from(f)),
            f => return Err(format!("a second file {f:?}")),
        }
    }
    a.file = file.ok_or("the workflow file is necessary")?;
    if (a.failed || a.refresh) && a.rerun.is_none() {
        return Err("--failed and --refresh need --rerun DIR".into());
    }
    if a.failed && !a.jobs.is_empty() {
        return Err("--failed and --job exclude each other".into());
    }
    if a.cache.is_some() && a.no_cache {
        return Err("--cache and --no-cache exclude each other".into());
    }
    if (a.checkout_head || a.with_git || a.no_git) && a.checkout.is_none() {
        return Err("--checkout-head, --with-git and --no-git need --checkout".into());
    }
    if a.with_git && a.no_git {
        return Err("--with-git and --no-git exclude each other".into());
    }
    // The values of the `github` context have the forms of GitHub (T-277).
    let name = |p: &str| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    if let Some(r) = &a.repository
        && !r.split_once('/').is_some_and(|(o, n)| name(o) && name(n))
    {
        return Err(format!("--repository {r}: not OWNER/REPO"));
    }
    if let Some(s) = &a.sha
        && !((s.len() == 40 || s.len() == 64) && s.chars().all(|c| c.is_ascii_hexdigit()))
    {
        return Err(format!("--sha {s}: not a commit (40 or 64 hex digits)"));
    }
    // Each prints its own view: one at a time, as the usage gives (T-218).
    if [a.list, a.graph, a.validate, a.dry_run].iter().filter(|x| **x).count() > 1 {
        return Err("--list, --graph, --validate and --dry-run exclude each other".into());
    }
    for (flag, pairs) in [("--var", &a.vars), ("--input", &a.inputs)] {
        if let Some((n, _)) =
            pairs.iter().enumerate().find_map(|(i, (n, _))| pairs[..i].iter().any(|(m, _)| m == n).then_some((n, ())))
        {
            return Err(format!("{flag} {n}: given two times"));
        }
    }
    // An event that GitHub has, and its payload file, before the filter of
    // `on:` reads them (T-219).
    if pod_actions_workflow::event_keys(&a.event).is_none() {
        return Err(format!("--event {}: GitHub has no such event", a.event));
    }
    if let Some(p) = a.event_path.as_ref().filter(|p| !p.is_file()) {
        return Err(format!("--event-path {}: not a file", p.display()));
    }
    // A file or a directory of a flag, by the name of its flag (T-323 F11).
    let files = [("--var-file", &a.var_file), ("--input-file", &a.input_file), ("--env-file", &a.env_file)]
        .into_iter()
        .chain([("--secret-file", &a.secret_file)])
        .filter_map(|(f, p)| p.as_ref().map(|p| (f, p)));
    let named = a.environment_secrets.iter().map(|(_, p)| ("--environment-secret-file", p));
    let named = named.chain(a.environment_vars.iter().map(|(_, p)| ("--environment-var-file", p)));
    if let Some((f, p)) = files.chain(named).find(|(_, p)| !p.is_file()) {
        return Err(format!("{f} {}: not a file", p.display()));
    }
    if let Some((_, p)) = a.local_repos.iter().find(|(_, p)| !p.is_dir()) {
        return Err(format!("--local-repository {}: not a directory", p.display()));
    }
    if a.offline && a.frozen {
        return Err("--offline and --frozen exclude each other".into());
    }
    if a.podvm.is_some() && a.guest.is_some() {
        return Err("one of --podvm, --microvm and --incus".into());
    }
    let kind = a.podvm.is_some() || a.guest.is_some();
    // These run no job: they need no executor.
    if a.list || a.graph || a.validate || (a.dry_run && a.runner_dir.is_none() && !kind) {
        return Ok(a);
    }
    let kind_flag = match &a.guest {
        Some((true, _)) => "--microvm",
        Some((false, _)) => "--incus",
        None => "--podvm",
    };
    // --platform maps labels: it does not turn off the checks below (T-206).
    if !a.platforms.is_empty() {
        let host = a.platforms.iter().any(|(_, t)| t == "host");
        let images = a.platforms.iter().any(|(_, t)| t != "host");
        if host && a.runner_dir.is_none() {
            return Err("--platform LABEL=host needs --runner-dir DIR".into());
        }
        if images && (!kind || a.images.is_none()) {
            return Err("--platform LABEL=IMAGE needs a unit kind (--podvm, --microvm, --incus) and --images".into());
        }
    }
    // One runner directory runs one job at a time.
    if a.parallel > 1 && (a.runner_dir.is_some() || !kind) {
        return Err("--jobs N needs a unit kind (--podvm, --microvm, --incus) and no --runner-dir".into());
    }
    // A guest kind checks its mounts and takes no host directory (T-138).
    if a.export.is_some() && a.guest.is_some() {
        return Err(
            "--export needs --runner-dir or --podvm: a guest of --microvm or --incus mounts no host directory".into()
        );
    }
    if a.toolcache.is_some() && a.podvm.is_none() {
        return Err(
            "--toolcache needs --podvm: a guest mounts no host directory, and a runner directory has its own cache"
                .into(),
        );
    }
    if !a.environment_allow.is_empty() && a.podnet.is_none() && a.vnet.is_none() && a.guest.is_none() {
        return Err("--environment-allow needs an egress hold: --podnet with --podvm, or --microvm or --incus".into());
    }
    if a.podnet.is_some() && a.podvm.is_none() {
        return Err("--podnet needs --podvm".into());
    }
    // A dry run runs no job to view, and `*` is no hold (T-323 F20, F24).
    if a.web.is_some() && a.dry_run {
        return Err("--web and --dry-run exclude each other: a dry run runs no job to view".into());
    }
    if let Some(h) = a.allow.iter().find(|h| h.trim() == "*" || h.trim().is_empty()) {
        return Err(format!("--allow {h:?}: not a host; a hold of each host is no hold: give each host"));
    }
    // The vnet holds the egress in the front: the hold of `podnet pod`
    // is not taken with it (T-220).
    if a.vnet.is_some() && a.podvm.is_none() {
        return Err("--vnet needs --podvm".into());
    }
    if a.vnet.is_some() && a.podnet.is_some() {
        return Err("--vnet and --podnet exclude each other: the vnet holds the egress of the unit".into());
    }
    // A guest has no NIC: the egress module holds --allow with no podnet.
    if !a.allow.is_empty() && a.podnet.is_none() && a.vnet.is_none() && a.guest.is_none() {
        return Err("--allow needs --podnet or --vnet with --podvm, or --microvm or --incus".into());
    }
    if a.engine.is_some() && !matches!(a.guest, Some((true, _))) {
        return Err("--engine needs --microvm".into());
    }
    // The executor: name the flag that is missing or that clashes (T-207).
    let store = a.images.is_some();
    // Two executors and no route: that clash first (T-339 item 17).
    if kind && a.runner_dir.is_some() && a.platforms.is_empty() {
        return Err(format!(
            "--runner-dir and {kind_flag} are two executors: give one, or map labels to each with --platform"
        ));
    }
    if kind && !store {
        return Err(format!("{kind_flag} needs --images STORE"));
    }
    if (store || a.image.is_some()) && !kind {
        return Err("--images and --image need a unit kind (--podvm, --microvm, --incus)".into());
    }
    if !a.platforms.is_empty() {
        return Ok(a);
    }
    match (a.runner_dir.is_some(), kind, a.image.is_some()) {
        (true, true, _) => Err(format!(
            "--runner-dir and {kind_flag} are two executors: give one, or map labels to each with --platform"
        )),
        (true, false, _) | (false, true, true) => Ok(a),
        (false, true, false) => Err(format!("{kind_flag} needs --image NAME@sha256:HEX")),
        (false, false, _) => Err(
            "one of --runner-dir DIR, or a unit kind (--podvm, --microvm, --incus) with --images and --image, is necessary"
                .into(),
        ),
    }
}

#[cfg(test)]
mod checkout_tests {
    /// T-336: a workflow file of `.github/workflows` names its project.
    #[test]
    fn the_project_of_a_workflow_file() {
        let dir = std::env::temp_dir().join(format!("podrunner-project-of-{}", std::process::id()));
        let wf = dir.join("p/.github/workflows/c.yml");
        std::fs::create_dir_all(wf.parent().unwrap()).unwrap();
        std::fs::write(&wf, "on: push\n").unwrap();
        std::fs::write(dir.join("c.yml"), "on: push\n").unwrap();
        let canon = std::fs::canonicalize(dir.join("p")).unwrap();
        assert_eq!(super::project_of(&wf), Some(canon));
        assert_eq!(super::project_of(&dir.join("c.yml")), None, "not in .github/workflows");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
