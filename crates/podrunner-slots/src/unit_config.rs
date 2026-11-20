//! `[unit.KIND]` of `podrunner.toml`: the slots and the keys of one unit
//! kind.

use serde::Deserialize;

/// The PID 1 of an incus instance (`[unit.incus] init`).
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IncusInit {
    /// The init of the image: systemd boots the services of the image.
    Image,
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
    /// `incus`: the PID 1 of each instance. `image`: the init of the image
    /// (systemd of the hosted snapshot) with its own services and engine
    /// (`--podvm-init=image --podvm-engine`, T-543); absent: podvm's own.
    #[serde(default)]
    pub init: Option<IncusInit>,
    /// `fake`, with `allow_fake`: the boundary that the fake kind claims and
    /// an enforced hold, for a test of a forge source (T-511). A test only.
    #[serde(default)]
    pub claim: Option<podrunner_schema::Boundary>,
    /// `podvm`: run each unit and container under the confine layer of
    /// podvm (`--podvm-confine`, T-079). The kind says `confined` only
    /// after its self-test on this host.
    #[serde(default)]
    pub confine: bool,
    /// `microvm`: the memory of each guest (`--podvm-mem`, default `8G`)
    /// and its vCPUs (default 2). The image is an uncompressed initramfs:
    /// podvm needs 3 times the image plus 512 MiB, and `check` refuses a pin
    /// whose `unpacked_bytes` is over `(mem - 512 MiB) / 3` (T-573).
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
}

fn run_sh() -> Vec<String> {
    vec!["./run.sh".into()]
}
