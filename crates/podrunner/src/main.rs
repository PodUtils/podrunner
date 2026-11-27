//! The binary `podrunner`: the command line of the crate.

use std::io::Write;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // A test of `run` sets a short bound on the wait for a unit at a
    // cancel with a private first argument (T-716).
    if let [flag, ms, ..] = args.as_slice()
        && flag == podrunner_local::cancel::BOUND_FLAG
        && let Ok(ms) = ms.parse::<u64>()
    {
        podrunner_local::cancel::set_unit_bound_ms(ms);
        args.drain(..2);
    }
    // A probe child of `doctor` starts this binary again with the private
    // flag first; it runs one probe and nothing else.
    if let [flag, name, ..] = args.as_slice()
        && *flag == podrunner::doctor::probe_settings().child_flag
    {
        let code = pod_hostprobe::run_child(name, &mut std::io::stdout());
        return ExitCode::from(u8::try_from(code).unwrap_or(2));
    }
    // The helper that a third signal of `run` starts (T-636).
    if let [flag, rest @ ..] = args.as_slice()
        && flag == podrunner::run_local::reap::FLAG
    {
        return ExitCode::from(podrunner::run_local::reap::helper(rest));
    }
    podrunner::run_local::reap::prepare();
    // The binary writes each line of a local run when its job ends (T-296).
    podrunner::run_local::STREAM.store(true, std::sync::atomic::Ordering::SeqCst);
    let out = podrunner::cli::run(&args);
    let _ = std::io::stdout().write_all(out.stdout.as_bytes());
    let _ = std::io::stderr().write_all(out.stderr.as_bytes());
    // A unit whose end goes on in the background (T-720): a helper ends it
    // when this process is gone.
    let going = podrunner_local::exec::going();
    if going > 0 {
        let _ = std::io::stdout().flush();
        let why =
            format!("podrunner: {going} units did not go at the cancel; podrunner ends now, and a helper ends them");
        podrunner_local::cancel::hand_over(&why, out.code);
    }
    ExitCode::from(out.code)
}
