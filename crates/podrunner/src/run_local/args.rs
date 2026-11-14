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
    pub(super) podnet: Option<PathBuf>,
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
            "--env" => a.env.push(pair(&value()?)?),
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
            "--image" => a.image = Some(value()?),
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
            "--podnet" => a.podnet = Some(PathBuf::from(value()?)),
            "--allow" => a.allow.push(value()?),
            "--event" => a.event = value()?,
            "--event-path" => a.event_path = Some(PathBuf::from(value()?)),
            "--force-event" => a.force_event = true,
            "--remote" => a.remote = Some(value()?),
            "--ref" => a.git_ref = Some(value()?),
            "--sha" => a.sha = Some(value()?),
            "--repository" => a.repository = Some(value()?),
            "--actor" => a.actor = Some(value()?),
            "--var" => a.vars.push(pair(&value()?)?),
            "--input" => a.inputs.push(pair(&value()?)?),
            o if o.starts_with("--") => return Err(format!("an unknown option {o:?}")),
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
    if (a.checkout_head || a.with_git) && a.checkout.is_none() {
        return Err("--checkout-head and --with-git need --checkout".into());
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
    if !a.platforms.is_empty() {
        let host = a.platforms.iter().any(|(_, t)| t == "host");
        let images = a.platforms.iter().any(|(_, t)| t != "host");
        if host && a.runner_dir.is_none() {
            return Err("--platform LABEL=host needs --runner-dir DIR".into());
        }
        if images && (!kind || a.images.is_none()) {
            return Err("--platform LABEL=IMAGE needs a unit kind (--podvm, --microvm, --incus) and --images".into());
        }
        if a.image.is_some() && !kind {
            return Err("--image needs a unit kind".into());
        }
        return Ok(a);
    }
    let unit = [kind, a.images.is_some(), a.image.is_some()];
    // One runner directory runs one job at a time.
    if a.parallel > 1 && (a.runner_dir.is_some() || !kind) {
        return Err("--jobs N needs a unit kind (--podvm, --microvm, --incus) and no --runner-dir".into());
    }
    if a.export.is_some() && a.runner_dir.is_none() {
        return Err("--export needs --runner-dir: a unit kind does not export its workspace yet".into());
    }
    if !a.environment_allow.is_empty() && a.podnet.is_none() && a.guest.is_none() {
        return Err("--environment-allow needs an egress hold: --podnet with --podvm, or --microvm or --incus".into());
    }
    if a.podnet.is_some() && a.podvm.is_none() {
        return Err("--podnet needs --podvm".into());
    }
    // A guest has no NIC: the egress module holds --allow with no podnet.
    if !a.allow.is_empty() && a.podnet.is_none() && a.guest.is_none() {
        return Err("--allow needs --podnet with --podvm, or --microvm or --incus".into());
    }
    if a.engine.is_some() && !matches!(a.guest, Some((true, _))) {
        return Err("--engine needs --microvm".into());
    }
    match (a.runner_dir.is_some(), unit) {
        (true, [false, false, false]) | (false, [true, true, true]) => Ok(a),
        _ => Err("one of --runner-dir DIR, or a unit kind (--podvm, --microvm, --incus) with --images and --image, is necessary".into()),
    }
}
