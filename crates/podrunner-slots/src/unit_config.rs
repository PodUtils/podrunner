//! `[unit.KIND]` of `podrunner.toml`: the slots and the keys of one unit
//! kind.

use serde::Deserialize;

/// The PID 1 of a unit: `[unit.KIND] init` of a kind (what it boots) and
/// `[[pin.payload]] init` of a payload (what it needs). Absent: podvm's
/// own init. Placement sends a payload only to a kind with the same value
/// (T-549).
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnitInit {
    /// The init of the image (`--podvm-init=image`): systemd boots the
    /// services of the image.
    Image,
}

/// The name of an init as an audit line and a refusal give it.
pub fn init_name(init: Option<UnitInit>) -> &'static str {
    match init {
        Some(UnitInit::Image) => "init = \"image\"",
        None => "no init (podvm's own)",
    }
}

/// The slots of one unit kind.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitConfig {
    pub slots: u32,
    /// The top process of a unit: the runner in the payload.
    #[serde(default = "run_sh")]
    pub command: Vec<String>,
    /// The tool of the kind (`podvm`: the path of the podvm binary).
    #[serde(default)]
    pub binary: Option<String>,
    /// The store of the pinned images (`podvm`: a podvm store).
    #[serde(default)]
    pub images: Option<String>,
    /// The registry hosts of the container images of a job (`ghcr.io`),
    /// for `container: hooks`. None: each container is refused.
    #[serde(default)]
    pub registries: Vec<String>,
    /// The runner directory in a unit, as the runner names it; absent: the
    /// path that the driver observes (the fake unit).
    #[serde(default)]
    pub runner_dir: Option<String>,
    /// `podvm`: the `podnet` binary that holds the egress of each unit
    /// (`podnet pod`, T-065); absent: no hold (`egress_hold: unknown`).
    #[serde(default)]
    pub podnet: Option<String>,
    /// The bytes that a unit may write under its root, measured every
    /// 30 s (T-074); a `disk` limit of the offer comes first. Absent: no
    /// measure.
    #[serde(default)]
    pub disk_max_bytes: Option<u64>,
    /// `incus`: the pool holds a size of a root disk (not `dir`): each root
    /// is the lesser of the `disk` of the offer and `disk_max_bytes` (T-482).
    #[serde(default)]
    pub root_quota: bool,
    /// `incus`, `microvm`: the PID 1 of each unit. `image`: the init of the
    /// image (systemd of the hosted snapshot) with its own services and
    /// engine: `--podvm-init=image --podvm-engine` on the incus tier
    /// (T-543), `--podvm-init=image` on the machine tier (T-549). Absent:
    /// podvm's own. The other kinds refuse it (T-549).
    #[serde(default)]
    pub init: Option<UnitInit>,
    /// `fake`, with `allow_fake`: the boundary that the fake kind claims and
    /// an enforced hold, for a test of a forge source (T-511). A test only.
    #[serde(default)]
    pub claim: Option<podrunner_schema::Boundary>,
    /// `podvm`: run each unit and container under the confine layer of
    /// podvm (`--podvm-confine`, T-079). The kind says `confined` only
    /// after its self-test on this host.
    #[serde(default)]
    pub confine: bool,
    /// `microvm`: the most memory of each guest (`--podvm-mem`, default
    /// `8G`) and its vCPUs (default 2). The image is an uncompressed
    /// initramfs: podvm needs 13/4 times the image plus 512 MiB, and `check`
    /// refuses a pin whose `unpacked_bytes` is over `(mem - 512 MiB) × 4 /
    /// 13` (T-573). An offer lowers the memory, not below the boot floor,
    /// the image, and the 2 GiB of root that a job keeps (T-699): the
    /// `memory` of the offer is beyond the image (T-655).
    /// `podvm`: the memory of each unit (`RLIMIT_AS`);
    /// `incus`: the memory and the CPUs of each instance (T-162).
    #[serde(default)]
    pub mem: Option<String>,
    #[serde(default)]
    pub cpus: Option<u32>,
    /// `podvm`, `incus`: the most processes of each unit (T-162). `mem`
    /// and, on `incus`, `cpus` are limits there too.
    #[serde(default)]
    pub pids: Option<u32>,
    /// `microvm`: the directory of the static Docker binaries; each guest
    /// runs an engine of its own (T-103).
    #[serde(default)]
    pub engine: Option<String>,
    /// `podvm`, `incus`, `microvm`: the work folder of the runner in each
    /// unit, an absolute path (T-541, T-590). podrunner writes it into
    /// `.runner` of the credential, so `GITHUB_WORKSPACE` is
    /// `FOLDER/REPO/REPO`. Absent: the hosted path `/home/runner/work`
    /// (`podvm`: `work` in its `runner_dir`). A podvm folder is in its
    /// `runner_dir`: the container hooks mount only there.
    #[serde(default)]
    pub work_folder: Option<String>,
    /// The warm units of the kind (T-487): a count for an offer with no
    /// label of a payload, or a table of `runs-on` label to count (`*`: no
    /// such label). Each holds a slot of the kind and a unit of
    /// `max_units`.
    #[serde(default)]
    pub warm: Option<WarmCount>,
    /// A warm unit older than this ends and is made again (default 3600).
    #[serde(default)]
    pub warm_max_age_s: Option<u64>,
}

/// `[unit.KIND] warm`: one count, or a count for each label.
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum WarmCount {
    Count(u32),
    ByLabel(std::collections::BTreeMap<String, u32>),
}

/// The label of a warm row for an offer with no label of a payload.
pub const ANY_LABEL: &str = "*";

/// The home of the runner in the runner image and in the hosted snapshot.
pub const RUNNER_HOME: &str = "/home/runner";

/// The work folder of a hosted runner.
pub const HOSTED_WORK_FOLDER: &str = "/home/runner/work";

impl UnitConfig {
    /// The warm rows of the kind: each label (`*` for none) and its count
    /// above 0.
    pub fn warm_rows(&self) -> Vec<(String, u32)> {
        match &self.warm {
            None => Vec::new(),
            Some(WarmCount::Count(n)) => vec![(ANY_LABEL.to_string(), *n)],
            Some(WarmCount::ByLabel(m)) => m.iter().map(|(l, n)| (l.clone(), *n)).collect(),
        }
        .into_iter()
        .filter(|(_, n)| *n > 0)
        .collect()
    }

    /// The work folder of the runner in a unit of the kind `name` (T-590):
    /// `work_folder`, else the hosted path on a kind with a runner home,
    /// else none (the folder of the mint, `_work` under the runner root).
    pub fn work_folder_of(&self, name: &str) -> Option<String> {
        let home = self.runner_dir.as_deref().unwrap_or(RUNNER_HOME).trim_end_matches('/');
        match name {
            _ if self.work_folder.is_some() => self.work_folder.clone(),
            "podvm" => Some(format!("{home}/work")),
            "incus" | "microvm" => Some(HOSTED_WORK_FOLDER.into()),
            _ => None,
        }
    }
}

/// The kinds that take `work_folder`: the runner and its work folder are in
/// the unit (the fake kind for the tests).
const WORK_FOLDER_KINDS: [&str; 4] = ["podvm", "incus", "microvm", "fake"];

/// The parts of a podvm runner directory that are no work folder: the code
/// of the runner, which a writable mount would give to each container.
const RUNNER_PARTS: [&str; 3] = ["bin", "externals", "hooks"];

/// `work_folder` of each kind: an absolute path with no empty, `.`, `..`
/// or control part, on a kind that takes it. A podvm folder is a folder in
/// `runner_dir` (default `/home/runner`): the hook policy mounts only the
/// work folder and the externals of that directory (T-590).
pub(crate) fn check_work_folders<'a>(
    units: impl IntoIterator<Item = (&'a String, &'a UnitConfig)>,
) -> Result<(), String> {
    for (name, u) in units {
        let Some(f) = &u.work_folder else { continue };
        if !WORK_FOLDER_KINDS.contains(&name.as_str()) {
            return Err(format!("[unit.{name}] work_folder: only the podvm, the incus and the microvm kinds take it"));
        }
        let bad = |p: &str| p.is_empty() || p == "." || p == ".." || p.chars().any(char::is_control);
        if f.strip_prefix('/').is_none_or(|rest| rest.split('/').any(bad)) {
            return Err(format!(
                "[unit.{name}] work_folder {f:?}: give an absolute path, such as \"/home/runner/work\""
            ));
        }
        if name == "podvm" {
            let home = u.runner_dir.as_deref().unwrap_or(RUNNER_HOME).trim_end_matches('/');
            let first = f.strip_prefix(home).and_then(|r| r.strip_prefix('/')).and_then(|r| r.split('/').next());
            if first.is_none_or(|p| p.starts_with('.') || RUNNER_PARTS.contains(&p)) {
                return Err(format!(
                    "[unit.podvm] work_folder {f:?}: give a folder in the runner directory {home:?}, such as \
                     \"{home}/work\" (not bin, externals, hooks or a name with a leading dot): the container hooks \
                     mount only there"
                ));
            }
        }
    }
    Ok(())
}

fn run_sh() -> Vec<String> {
    vec!["./run.sh".into()]
}
