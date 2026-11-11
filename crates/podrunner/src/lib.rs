//! podrunner: one static program that runs a CI job in one fresh unit for
//! each job (PodUtils/podrunner#1). The binary `podrunner` calls
//! [`cli::run`].

pub mod cli;
pub mod control;
pub mod doctor;
pub mod hooks;
pub mod mint_serve;
pub mod serve;
pub mod text;
