//! The input of a hook call: the JSON object that the official runner
//! writes on the stdin of the hook script (`HookInput.cs` of
//! `actions/runner` v2.338.2, camel case). The field set is closed: an
//! unknown field is refused, so a new field of the runner is a change of
//! this file and its test.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

/// A list or a map that the runner writes as `null` when it is empty.
fn or_null<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// The longest body of a hook call.
pub const BODY_MAX: usize = 1 << 20;

/// The four commands of the hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    PrepareJob,
    CleanupJob,
    RunScriptStep,
    RunContainerStep,
}

impl Command {
    /// The command of a path segment (`/v1/hook/COMMAND`).
    pub fn parse(s: &str) -> Option<Command> {
        serde_json::from_value(Value::String(s.into())).ok()
    }
}

/// A mount of the runner (`MountVolume` of `ContainerInfo.cs`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MountVolume {
    #[serde(default)]
    pub user_provided_value: Option<String>,
    #[serde(default)]
    pub source_volume_path: Option<String>,
    #[serde(default)]
    pub target_volume_path: Option<String>,
    #[serde(default)]
    pub read_only: bool,
}

/// The credential of a registry. `Debug` hides the password.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Registry {
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub server_url: Option<String>,
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry").field("server_url", &self.server_url).finish_non_exhaustive()
    }
}

/// A container of a job or of a container action (`HookContainer`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Container {
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub dockerfile: Option<String>,
    #[serde(default, deserialize_with = "or_null")]
    pub entry_point_args: Vec<String>,
    #[serde(default)]
    pub entry_point: Option<String>,
    #[serde(default)]
    pub working_directory: Option<String>,
    #[serde(default)]
    pub create_options: Option<String>,
    #[serde(default)]
    pub registry: Option<Registry>,
    #[serde(default, deserialize_with = "or_null")]
    pub environment_variables: BTreeMap<String, String>,
    #[serde(default, deserialize_with = "or_null")]
    pub port_mappings: Vec<String>,
    #[serde(default, deserialize_with = "or_null")]
    pub system_mount_volumes: Vec<MountVolume>,
    #[serde(default, deserialize_with = "or_null")]
    pub user_mount_volumes: Vec<MountVolume>,
}

/// The arguments of `prepare_job`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareJob {
    #[serde(default)]
    pub container: Option<Container>,
    #[serde(default, deserialize_with = "or_null")]
    pub services: Vec<Container>,
}

/// The arguments of `run_script_step`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptStep {
    #[serde(default, deserialize_with = "or_null")]
    pub entry_point_args: Vec<String>,
    #[serde(default)]
    pub entry_point: Option<String>,
    #[serde(default, deserialize_with = "or_null")]
    pub environment_variables: BTreeMap<String, String>,
    #[serde(default, deserialize_with = "or_null")]
    pub prepend_path: Vec<String>,
    #[serde(default)]
    pub working_directory: Option<String>,
}

/// The arguments of a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Args {
    PrepareJob(PrepareJob),
    CleanupJob,
    RunScriptStep(ScriptStep),
    RunContainerStep(Container),
}

/// One hook call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookInput {
    pub command: Command,
    pub response_file: Option<String>,
    pub args: Args,
    pub state: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Raw {
    command: Command,
    #[serde(default)]
    response_file: Option<String>,
    #[serde(default)]
    args: Option<Value>,
    #[serde(default)]
    state: Option<Value>,
}

/// Why an input is refused: a short text with no byte of the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadInput(pub &'static str);

/// Read one hook call. `command` is the command of the path; the body must
/// name the same one.
pub fn parse(command: Command, body: &[u8]) -> Result<HookInput, BadInput> {
    if body.len() > BODY_MAX {
        return Err(BadInput("the body is over 1 MiB"));
    }
    let raw: Raw =
        serde_json::from_slice(body).map_err(|_| BadInput("the body is not a hook input of runner v2.338.2"))?;
    if raw.command != command {
        return Err(BadInput("the command of the body is not the command of the path"));
    }
    let args = raw.args.unwrap_or(Value::Object(Default::default()));
    let bad = |_| BadInput("the args are not the args of the command");
    let args = match command {
        Command::PrepareJob => Args::PrepareJob(serde_json::from_value(args).map_err(bad)?),
        Command::CleanupJob => {
            if args.as_object().is_none_or(|m| !m.is_empty()) {
                return Err(BadInput("cleanup_job has no args"));
            }
            Args::CleanupJob
        }
        Command::RunScriptStep => Args::RunScriptStep(serde_json::from_value(args).map_err(bad)?),
        Command::RunContainerStep => Args::RunContainerStep(serde_json::from_value(args).map_err(bad)?),
    };
    Ok(HookInput { command, response_file: raw.response_file, args, state: raw.state })
}
