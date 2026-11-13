//! The checks of the host on each hook call (GitHub #35). A step of the
//! job runs as the uid of the runner and can skip the hook script, so this
//! side holds each rule: the order of the calls, the two mount tables, no
//! host path, no device, no privilege, no network mode, registries by
//! host, and no build.

use std::collections::BTreeMap;

use podrunner_schema::{ContainerRoute, RefusalCode};

use crate::input::{Args, Container, HookInput, MountVolume, Registry, ScriptStep};

/// The mounts of a job container, by target. `/__e` must be read-only.
pub const JOB_MOUNTS: [&str; 7] =
    ["/__w", "/__w/_temp", "/__w/_actions", "/__w/_tool", "/__e", "/github/home", "/github/workflow"];

/// The mounts of a container action, by target.
pub const ACTION_MOUNTS: [&str; 5] =
    ["/github/runner_temp", "/github/home", "/github/workflow", "/github/file_commands", "/github/workspace"];

/// The socket that the runner always mounts: accepted, and dropped.
const DOCKER_SOCK: &str = "/var/run/docker.sock";

/// The options of `createOptions` that a unit applies: each takes one
/// value, except `--init`.
const OPTIONS: [&str; 6] = ["--user", "--cpus", "--memory", "--shm-size", "--hostname", "--workdir"];

/// What a unit kind allows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    pub container: ContainerRoute,
    /// The unit shares the network of the host (`net: shared-host`).
    pub shared_host_net: bool,
    /// The registry hosts that images can come from (`docker.io`,
    /// `ghcr.io`).
    pub registries: Vec<String>,
    /// The directory of the runner in the unit, as the runner names it:
    /// each mount source is under it.
    pub work_root: String,
}

/// A refusal: a code, and a short reason with no byte of the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub code: RefusalCode,
    pub why: &'static str,
}

fn refuse<T>(code: RefusalCode, why: &'static str) -> Result<T, Refusal> {
    Err(Refusal { code, why })
}

/// A mount that the unit applies: the source is relative to `work_root`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub source: String,
    pub target: String,
    pub read_only: bool,
}

/// A container to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub image: String,
    pub argv: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub workdir: Option<String>,
    pub mounts: Vec<Mount>,
    pub options: Vec<(String, String)>,
    pub registry: Option<Registry>,
}

/// What the unit does for a call that passed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    Prepare { job: Option<Planned>, services: Vec<Planned> },
    Cleanup,
    Script(ScriptStep),
    ContainerStep(Planned),
}

/// The state of the hooks of one unit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Gate {
    prepared: bool,
}

impl Gate {
    /// Check one call, in the order of the calls of the unit.
    pub fn check(&mut self, input: &HookInput, policy: &Policy) -> Result<Plan, Refusal> {
        if policy.container != ContainerRoute::Hooks {
            return refuse(RefusalCode::ContainerNotSupported, "the unit kind has no container route");
        }
        match &input.args {
            Args::PrepareJob(p) => {
                if self.prepared {
                    return refuse(RefusalCode::ContainerOptionRefused, "prepare_job comes once");
                }
                if !p.services.is_empty() && policy.shared_host_net {
                    return refuse(RefusalCode::ContainerNotSupported, "services on a unit of the host network");
                }
                let job = p.container.as_ref().map(|c| plan(c, &JOB_MOUNTS, policy)).transpose()?;
                let services = p.services.iter().map(|c| plan(c, &[], policy)).collect::<Result<_, _>>()?;
                self.prepared = true;
                Ok(Plan::Prepare { job, services })
            }
            // A Docker action in a job with no `container:` and no
            // `services:` gets no `prepare_job` from the runner; it runs as
            // on GitHub, with the checks of the action mounts (Q4, T-098).
            Args::RunContainerStep(c) => Ok(Plan::ContainerStep(plan(c, &ACTION_MOUNTS, policy)?)),
            _ if !self.prepared => refuse(RefusalCode::ContainerNotSupported, "a container call before prepare_job"),
            Args::CleanupJob => Ok(Plan::Cleanup),
            Args::RunScriptStep(s) => {
                if s.entry_point.as_deref().is_none_or(str::is_empty) {
                    return refuse(RefusalCode::ContainerOptionRefused, "a script step with no entry point");
                }
                env(&s.environment_variables)?;
                if s.prepend_path.iter().chain(&s.working_directory).any(|p| !clean_abs(p)) {
                    return refuse(RefusalCode::ContainerOptionRefused, "a path that is not absolute and clean");
                }
                Ok(Plan::Script(s.clone()))
            }
        }
    }
}

/// An absolute path with no `..`, no empty part and no control byte.
fn clean_abs(p: &str) -> bool {
    (p.starts_with('/') && !p.bytes().any(|b| b < 0x20) && p[1..].split('/').all(|s| !s.is_empty() && s != ".."))
        || p == "/"
}

fn env(vars: &BTreeMap<String, String>) -> Result<(), Refusal> {
    if vars.iter().any(|(k, v)| k.is_empty() || k.contains(['=', '\0']) || v.contains('\0')) {
        return refuse(RefusalCode::ContainerOptionRefused, "a bad environment variable");
    }
    Ok(())
}

/// The registry host of an image: the first part when it names a host, else
/// `docker.io`.
pub fn registry_host(image: &str) -> &str {
    match image.split_once('/') {
        Some((first, _)) if first.contains(['.', ':']) || first == "localhost" => first,
        _ => "docker.io",
    }
}

fn plan(c: &Container, table: &[&str], policy: &Policy) -> Result<Planned, Refusal> {
    if c.dockerfile.is_some() {
        return refuse(RefusalCode::BuildNotSupported, "a Dockerfile action");
    }
    let image = c.image.clone().unwrap_or_default();
    if podrunner_unit::parse_pinned(&image).is_err() {
        return refuse(RefusalCode::PayloadNotPinned, "an image that is not NAME@sha256:HEX");
    }
    let host = registry_host(&image);
    if !policy.registries.iter().any(|r| r == host) {
        return refuse(RefusalCode::ContainerOptionRefused, "an image of a registry that the kind does not allow");
    }
    if let Some(r) = &c.registry {
        // Runner v2.338.2 sends an empty server for a Docker Hub image
        // (`DockerUtil.ParseRegistryHostnameFromImageName`).
        let server = r.server_url.as_deref().filter(|s| !s.is_empty()).unwrap_or("docker.io");
        let server = server.trim_start_matches("https://").trim_end_matches('/');
        if server != host {
            return refuse(RefusalCode::ContainerOptionRefused, "a registry credential for another host");
        }
    }
    if !c.port_mappings.is_empty() && policy.shared_host_net {
        return refuse(RefusalCode::ContainerOptionRefused, "ports on a unit of the host network");
    }
    if !c.user_mount_volumes.is_empty() {
        return refuse(RefusalCode::ContainerOptionRefused, "a volume of the workflow: a host path");
    }
    env(&c.environment_variables)?;
    if c.working_directory.as_deref().is_some_and(|w| !clean_abs(w)) {
        return refuse(RefusalCode::ContainerOptionRefused, "a working directory that is not absolute and clean");
    }
    let mut mounts = Vec::new();
    for m in &c.system_mount_volumes {
        if let Some(m) = mount(m, table, policy)? {
            mounts.push(m);
        }
    }
    let mut argv: Vec<String> = c.entry_point.iter().filter(|e| !e.is_empty()).cloned().collect();
    argv.extend(c.entry_point_args.iter().cloned());
    Ok(Planned {
        image,
        argv,
        env: c.environment_variables.clone(),
        workdir: c.working_directory.clone(),
        mounts,
        options: options(c.create_options.as_deref().unwrap_or(""))?,
        registry: c.registry.clone(),
    })
}

fn mount(m: &MountVolume, table: &[&str], policy: &Policy) -> Result<Option<Mount>, Refusal> {
    let (source, target) = (m.source_volume_path.as_deref(), m.target_volume_path.as_deref().unwrap_or(""));
    if source == Some(DOCKER_SOCK) && target == DOCKER_SOCK {
        return Ok(None);
    }
    if !table.contains(&target) {
        return refuse(RefusalCode::ContainerOptionRefused, "a mount target outside the table");
    }
    let source = source.unwrap_or("");
    let root = policy.work_root.trim_end_matches('/');
    let rel = source.strip_prefix(root).and_then(|r| r.strip_prefix('/')).filter(|_| clean_abs(source));
    let Some(rel) = rel.filter(|r| !r.ends_with(".sock")) else {
        return refuse(RefusalCode::ContainerOptionRefused, "a mount source outside the unit");
    };
    // The externals of the runner (node, the hook relay) are code that each
    // step runs: a writable `/__e` would let one step change the next.
    if target == "/__e" && !m.read_only {
        return refuse(RefusalCode::ContainerOptionRefused, "/__e is read-only");
    }
    Ok(Some(Mount { source: rel.into(), target: target.into(), read_only: m.read_only }))
}

/// The options of `createOptions` that the unit applies; any other is
/// refused (a device, a privilege, a network mode, a host path).
pub fn options(text: &str) -> Result<Vec<(String, String)>, Refusal> {
    let mut out = Vec::new();
    let mut words = text.split_whitespace();
    while let Some(w) = words.next() {
        let (name, inline) = match w.split_once('=') {
            Some((n, v)) => (n, Some(v)),
            None => (w, None),
        };
        if name == "--init" && inline.is_none() {
            out.push(("--init".into(), String::new()));
            continue;
        }
        if !OPTIONS.contains(&name) {
            return refuse(RefusalCode::ContainerOptionRefused, "a create option that the kind does not apply");
        }
        let value = match inline {
            Some(v) => v.to_string(),
            None => words
                .next()
                .map(str::to_string)
                .ok_or(Refusal { code: RefusalCode::ContainerOptionRefused, why: "a create option with no value" })?,
        };
        out.push((name.into(), value));
    }
    Ok(out)
}
