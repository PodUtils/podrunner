//! The binary `podrunner`: the command line of the crate.

use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // A probe child of `doctor` starts this binary again with the private
    // flag first; it runs one probe and nothing else.
    if let [flag, name, ..] = args.as_slice()
        && *flag == podrunner::doctor::probe_settings().child_flag
    {
        let code = pod_hostprobe::run_child(name, &mut std::io::stdout());
        return ExitCode::from(u8::try_from(code).unwrap_or(2));
    }
    let out = podrunner::cli::run(&args);
    let _ = std::io::stdout().write_all(out.stdout.as_bytes());
    let _ = std::io::stderr().write_all(out.stderr.as_bytes());
    ExitCode::from(out.code)
}
