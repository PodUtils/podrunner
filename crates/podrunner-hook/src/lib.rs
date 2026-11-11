//! The container hooks of podrunner (GitHub #35, `docs/contracts/hook.md`).
//! The official runner calls a hook script for each container operation;
//! the script relays the call to the control socket of its unit, and this
//! crate checks it and maps it to the operations of the unit driver.

pub mod http;
pub mod input;
pub mod policy;
pub mod server;

/// The relay script of the unit image: `ACTIONS_RUNNER_CONTAINER_HOOKS`
/// names it.
pub const RELAY: &str = include_str!("../relay/podrunner-hook.sh");

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_server;
