//! podrunner: one static program that runs a CI job in one fresh unit for
//! each job (PodUtils/podrunner#1). This binary has only `--version` and
//! `--help`; the commands come with milestone M-A.

use std::process::ExitCode;

mod cli;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (code, out) = cli::run(&args);
    if code == 0 {
        print!("{out}");
    } else {
        eprint!("{out}");
    }
    ExitCode::from(code)
}
