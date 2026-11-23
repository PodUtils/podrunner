//! podrunner: one static program that runs a CI job in one fresh unit for
//! each job (PodUtils/podrunner#1). The binary `podrunner` calls
//! [`cli::run`].

pub mod cli;
mod config_init;
pub mod control;
pub mod doctor;
pub mod help;
pub mod hooks;
pub mod image;
pub mod incus_kind;
pub mod init;
pub mod lock;
pub mod microvm_kind;
pub mod mint_serve;
pub mod notify;
pub mod pat;
pub mod pin_size;
pub mod podvm_kind;
pub mod program_source;
pub mod run_config;
pub mod run_local;
#[cfg(test)]
mod scratch;
pub mod serve;
pub mod text;
pub mod tiers;
pub mod verbs;
