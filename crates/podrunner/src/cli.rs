//! The command line. Exit 64 is a usage error (the exit codes of
//! `pod-core`).

/// The version line: the version of the crate and the commit of the tree.
pub fn version() -> String {
    format!("podrunner {} ({})\n", env!("CARGO_PKG_VERSION"), env!("PODRUNNER_COMMIT"))
}

const USAGE: &str = "usage: podrunner [--version | --help]\n";

/// Run the command line `args` (without the program name). Return the exit
/// code and the text for stdout (code 0) or stderr.
pub fn run(args: &[String]) -> (u8, String) {
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["--version"] | ["-V"] => (0, version()),
        ["--help"] | ["-h"] => (0, USAGE.into()),
        _ => (64, USAGE.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn version_names_the_version_and_a_commit() {
        let (code, out) = run(&args(&["--version"]));
        assert_eq!(code, 0);
        assert!(out.starts_with(&format!("podrunner {} (", env!("CARGO_PKG_VERSION"))), "{out}");
        assert!(out.trim_end().ends_with(')'), "{out}");
    }

    #[test]
    fn no_argument_is_a_usage_error() {
        assert_eq!(run(&[]).0, 64);
        assert_eq!(run(&args(&["serve"])).0, 64);
    }

    #[test]
    fn help_exits_0() {
        assert_eq!(run(&args(&["--help"])), (0, USAGE.to_string()));
    }
}
