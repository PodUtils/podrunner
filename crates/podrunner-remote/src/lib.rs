//! The remote mint of podrunner (GitHub #31, DR-4). On a host with no root,
//! a step of a job can read each file of the uid of podrunner, so the
//! source credentials and `mint` live on another machine. The unit host
//! speaks the source wire to it, one request in each HTTPS POST, over
//! mutual TLS with pinned keys.

pub mod gate;
pub mod ledger;

#[cfg(test)]
mod tests;
