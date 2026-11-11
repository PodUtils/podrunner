//! The container hooks of podrunner (GitHub #35, `docs/contracts/hook.md`).
//! The official runner calls a hook script for each container operation;
//! the script relays the call to the control socket of its unit, and this
//! crate checks it and maps it to the operations of the unit driver.

pub mod input;
pub mod policy;

#[cfg(test)]
mod tests;
