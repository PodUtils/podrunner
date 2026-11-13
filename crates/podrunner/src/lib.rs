//! podrunner: one static program that runs a CI job in one fresh unit for
//! each job (PodUtils/podrunner#1). The binary `podrunner` calls
//! [`cli::run`].

pub mod cli;
pub mod control;
pub mod doctor;
pub mod hooks;
pub mod image;
pub mod lock;
pub mod mint_serve;
pub mod notify;
pub mod podvm_kind;
pub mod program_source;
pub mod run_local;
pub mod serve;
pub mod text;
