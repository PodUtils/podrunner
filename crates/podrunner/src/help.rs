//! The help of each command (T-208): `podrunner help [COMMAND]`,
//! `COMMAND --help` (or `-h`) and `podrunner version`. Each answer is exit 0
//! and needs no other argument. The text comes from the model of
//! `crate::manual` (T-627): no help is written here.

use pod_core::failure::Class;
use pod_manual::help::Help;

use crate::cli::{Out, err, ok};
use crate::text::display;

/// The answer to `help`, `version` or a `--help` of a command; `None` for
/// each other command line.
pub(crate) fn answer(cmd: &str, rest: &[String]) -> Option<Out> {
    let manual = crate::manual::manual();
    // The arguments of a program after `--` are not options of podrunner.
    let words: Vec<&str> = rest.iter().map(String::as_str).take_while(|a| *a != "--").collect();
    let asks = words.iter().any(|a| matches!(*a, "--help" | "-h"));
    let json = words.contains(&"--json");
    match cmd {
        // `version` takes no option but its help (T-339).
        "version" if !asks => {
            return Some(match rest.first() {
                None => ok(crate::cli::version()),
                Some(x) => err(
                    Class::Usage,
                    format!("version takes no option: {:?}\n{}", display(x), crate::manual::usage("version")),
                ),
            });
        }
        "--help" | "-h" => return Some(show(&Help { path: Vec::new(), json })),
        "help" => {
            // The longest command of the words: `help config init`.
            let names: Vec<&str> = words.iter().copied().filter(|w| !w.starts_with('-')).collect();
            let found = (1..=names.len()).rev().find_map(|n| crate::manual::find(&names[..n].join(" ")));
            return Some(match (names.first(), found) {
                (Some(c), None) => {
                    err(Class::Usage, format!("an unknown command {:?}\n{}", display(c), crate::cli::usage()))
                }
                (_, f) => show(&Help { path: f.map(|c| words_of(c.path)).unwrap_or_default(), json }),
            });
        }
        _ => {}
    }
    // An unknown command is a usage error, also with --help.
    crate::manual::find(cmd)?;
    let mut line = vec![cmd.to_string()];
    line.extend(rest.iter().cloned());
    pod_manual::help::request(manual, &line).filter(|_| asks).map(|h| show(&h))
}

fn words_of(path: &str) -> Vec<String> {
    path.split_whitespace().map(str::to_string).collect()
}

/// The help of one command, as text or JSON.
fn show(h: &Help) -> Out {
    match pod_manual::help::answer(crate::manual::manual(), h) {
        Ok(text) => ok(text),
        Err(why) => err(Class::Usage, why),
    }
}

#[cfg(test)]
mod tests {
    use crate::cli::run;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    /// T-208: each command has its help, with no other argument.
    #[test]
    fn each_command_has_its_help() {
        for (a, want) in [
            (&["run", "--help"][..], "--runner-dir DIR"),
            (&["run", "-h"], "--runner-dir DIR"),
            (&["run", "w.yml", "-h"], "--runner-dir DIR"),
            (&["doctor", "--help"], "Usage: podrunner doctor"),
            (&["status", "-h"], "Usage: podrunner status"),
            (&["help", "stop"], "--wait --state DIR"),
            (&["help", "--help"], "Usage: podrunner COMMAND"),
            (&["help", "config", "init"], "--key-file FILE"),
            (&["version", "--help"], "Usage: podrunner version"),
            (&["version"], "podrunner "),
        ] {
            let o = run(&args(a));
            assert_eq!(o.code, 0, "{a:?}: {}", o.stderr);
            assert!(o.stdout.contains(want), "{a:?}: {}", o.stdout);
        }
        assert_eq!(run(&args(&["help", "nope"])).code, 64);
        assert_eq!(run(&args(&["version", "--x"])).code, 64, "T-339");
        // A next line of another command is not the help of check (T-323 F7).
        let o = run(&args(&["check", "--help"]));
        assert!(!o.stdout.contains("audit.jsonl"), "{}", o.stdout);
    }
}

#[cfg(test)]
mod t627_tests {
    use crate::cli::run;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    /// The flags of the row of `cmd` in `docs/cli.md`.
    fn row_flags(cmd: &str) -> Vec<String> {
        let cli = include_str!("../../../docs/cli.md");
        let row = cli.lines().find(|l| l.starts_with(&format!("| `podrunner {cmd} "))).expect("a row");
        let cell = row.split("` |").next().unwrap_or(row);
        cell.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .filter(|w| w.starts_with("--") && w.len() > 2)
            .map(str::to_string)
            .collect()
    }

    /// T-627: `man` answers, and the help of each command names each flag
    /// of its row in `docs/cli.md`.
    #[test]
    fn the_help_names_each_flag_of_the_documents() {
        let o = run(&args(&["man", "--no-pager"]));
        let man = o.code;
        for (cmd, path) in [("config init", &["config", "init"][..]), ("run (FILE", &["run"])] {
            let mut a = args(path);
            a.push("--help".into());
            let help = run(&a).stdout;
            for f in row_flags(cmd) {
                assert!(help.contains(&f), "{cmd}: {f} is not in the help");
            }
        }
        let top = run(&args(&["--help"])).stdout;
        assert!(!top.contains("podrunner run (FILE") || top.contains("--vnet"), "{top}");
        assert_eq!(man, 0, "{}", o.stderr);
    }
}
