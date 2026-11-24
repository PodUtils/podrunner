//! The one model of the command line (the ruling "Discipline"): each
//! command with its usage, its flags, its rules, the facts of the
//! environment and the exit codes. The parser of each command but `run`
//! takes its flags and its rules from here, `run` is checked against it,
//! and `pod-manual` makes the help, `podrunner man` and `docs/podrunner.1`
//! from it. No help text is written in a second place.

use std::sync::OnceLock;

use pod_manual::{Arg, Command, Flag, Group, Manual, Rule};

use crate::text::display;

mod commands;
mod facts;
mod run_flags;
#[cfg(test)]
mod tests;

pub(crate) use run_flags::{Key, key as run_key, not_keys, takes_value as run_key_takes_value};

/// The rules between the flags of `run`.
pub(crate) fn run_rules() -> &'static [R] {
    run_flags::RULES
}

/// One command of the table.
pub(crate) struct Cmd {
    /// The words after `podrunner`; empty for the root.
    pub(crate) path: &'static str,
    pub(crate) about: &'static str,
    /// The words of the usage after the path.
    pub(crate) usage: &'static str,
    pub(crate) notes: &'static [&'static str],
    pub(crate) args: &'static [A],
    pub(crate) flags: &'static [F],
    pub(crate) rules: &'static [R],
    /// The commands under this one by purpose: a title and the last words.
    pub(crate) groups: &'static [(&'static str, &'static [&'static str])],
}

/// One positional argument.
pub(crate) struct A {
    pub(crate) name: &'static str,
    pub(crate) help: &'static str,
    pub(crate) required: bool,
    pub(crate) many: bool,
}

/// One flag: its long name without the dashes, its value name, one line.
#[derive(Clone, Copy)]
pub(crate) struct F {
    pub(crate) long: &'static str,
    pub(crate) short: Option<char>,
    pub(crate) value: Option<&'static str>,
    pub(crate) help: &'static str,
    pub(crate) many: bool,
    /// The key of `run.toml` that gives the flag (`run` only).
    pub(crate) key: Key,
}

/// A rule between the flags of one command; the parser of the command
/// holds it.
pub(crate) enum R {
    /// At most one of the flags.
    Excludes(&'static [&'static str]),
    /// The flag only with each of the others.
    Needs(&'static str, &'static [&'static str]),
    /// The flag only with one of the others or more.
    NeedsOne(&'static str, &'static [&'static str]),
    /// Exactly one of the flags.
    OneOf(&'static [&'static str]),
}

/// A flag that takes no value.
pub(crate) const fn flag(long: &'static str, help: &'static str) -> F {
    F { long, short: None, value: None, help, many: false, key: Key::No }
}

/// A flag that takes the value `value`.
pub(crate) const fn val(long: &'static str, value: &'static str, help: &'static str) -> F {
    F { value: Some(value), ..flag(long, help) }
}

impl F {
    /// The flag can come more than one time.
    pub(crate) const fn many(self) -> F {
        F { many: true, ..self }
    }

    pub(crate) const fn short(self, c: char) -> F {
        F { short: Some(c), ..self }
    }

    pub(crate) const fn key(self, key: Key) -> F {
        F { key, ..self }
    }

    /// `--long`.
    pub(crate) fn spelling(&self) -> String {
        format!("--{}", self.long)
    }

    /// Whether `word` is a spelling of the flag.
    pub(crate) fn is(&self, word: &str) -> bool {
        word.strip_prefix("--") == Some(self.long) || self.short.is_some_and(|c| word == format!("-{c}"))
    }
}

/// The command at `path` (`config init`), or `None`.
pub(crate) fn find(path: &str) -> Option<&'static Cmd> {
    commands::COMMANDS.iter().find(|c| c.path == path)
}

/// Each command of the table.
pub(crate) fn all() -> &'static [Cmd] {
    commands::COMMANDS
}

/// The usage of one command, for an error of its command line.
pub(crate) fn usage(path: &str) -> String {
    match find(path) {
        Some(c) => {
            format!("usage: podrunner {}\nEach option: podrunner {path} --help", format!("{path} {}", c.usage).trim())
        }
        None => root_help(),
    }
}

/// The text of `podrunner --help`.
pub(crate) fn root_help() -> String {
    pod_manual::help::render(manual(), &[]).unwrap_or_default()
}

/// The model of `pod-manual`, made once.
pub(crate) fn manual() -> &'static Manual {
    static M: OnceLock<Manual> = OnceLock::new();
    M.get_or_init(|| build().with_help_flags())
}

fn rule(r: &R) -> Rule {
    let names = |l: &[&str]| l.iter().map(|s| s.to_string()).collect();
    match r {
        R::Excludes(l) => Rule::Excludes { flags: names(l) },
        R::Needs(f, l) => Rule::Needs { flag: f.to_string(), value: None, needs: names(l), one: false },
        R::NeedsOne(f, l) => Rule::Needs { flag: f.to_string(), value: None, needs: names(l), one: true },
        R::OneOf(l) => Rule::OneOf { flags: names(l), required: true },
    }
}

fn command(c: &Cmd) -> Command {
    let words = |s: &str| s.split_whitespace().map(str::to_string).collect::<Vec<_>>();
    let flags = c.flags.iter().map(|f| Flag {
        short: f.short,
        repeatable: f.many,
        value: f.value.map(str::to_string),
        ..Flag::long(f.long, f.help)
    });
    let args = c.args.iter().map(|a| Arg {
        name: a.name.into(),
        help: a.help.into(),
        required: a.required,
        many: a.many,
        ..Arg::default()
    });
    let groups = c
        .groups
        .iter()
        .map(|(t, l)| Group { title: t.to_string(), commands: l.iter().map(|s| s.to_string()).collect() });
    Command {
        path: words(c.path),
        about: c.about.into(),
        notes: c.notes.iter().map(|n| n.replace("{CLI_DOC}", CLI_DOC)).collect(),
        usage: c.usage.into(),
        args: args.collect(),
        flags: flags.collect(),
        rules: c.rules.iter().map(rule).collect(),
        groups: groups.collect(),
        ..Command::default()
    }
}

/// The page of each option, for a user who has no tree.
pub(crate) const CLI_DOC: &str =
    concat!("https://github.com/PodUtils/podrunner/blob/", env!("PODRUNNER_DOC_REF"), "/docs/cli.md");

fn build() -> Manual {
    Manual {
        name: "podrunner".into(),
        version: env!("PODRUNNER_VERSION").into(),
        about: find("").map_or("", |c| c.about).into(),
        description: commands::DESCRIPTION.iter().map(|d| d.to_string()).collect(),
        commands: all().iter().map(command).collect(),
        facts: facts::facts(),
        see_also: vec![CLI_DOC.into()],
        ..Manual::default()
    }
}

/// `podrunner man [SECTION...] [--no-pager | --json | --roff]`: the manual,
/// through a pager on a terminal.
pub(crate) fn man(rest: &[String]) -> crate::cli::Out {
    use pod_core::failure::Class;
    let c = find("man").expect("the command man");
    let mut req = pod_manual::Request::default();
    let mut words = Vec::new();
    for a in rest {
        match c.flags.iter().find(|f| f.is(a)).map(|f| f.long) {
            Some("no-pager") => req.no_pager = true,
            Some("json") => req.json = true,
            Some("roff") => req.roff = true,
            _ if a.starts_with('-') => {
                return crate::cli::err(Class::Usage, format!("an unknown option {}\n{}", display(a), usage("man")));
            }
            // A control character is in no name: the refusal shows it escaped.
            _ if a.chars().any(char::is_control) => {
                return crate::cli::err(Class::Usage, format!("{}: no such section\n{}", display(a), usage("man")));
            }
            // Each name of a section is lowercase: `man RUN` is `man run`.
            _ => words.push(a.to_ascii_lowercase()),
        }
    }
    req.section = (!words.is_empty()).then(|| words.join(" "));
    let tty = pod_manual::pager::Tty::probed();
    let keys = &mut std::io::stdin().lock();
    // A pager writes to the terminal itself; else the text is the answer.
    if tty.can_page() && !(req.no_pager || req.json || req.roff) {
        let said = pod_manual::run(manual(), &req, tty, keys, &mut std::io::stdout(), &mut std::io::stderr());
        return said.map_or_else(|why| crate::cli::err(Class::Usage, why), |()| crate::cli::ok(String::new()));
    }
    let (mut out, mut errs) = (Vec::new(), Vec::new());
    match pod_manual::run(manual(), &req, tty, keys, &mut out, &mut errs) {
        Ok(()) => crate::cli::Out {
            code: 0,
            stdout: String::from_utf8_lossy(&out).into(),
            stderr: String::from_utf8_lossy(&errs).into(),
        },
        Err(why) => crate::cli::err(Class::Usage, why),
    }
}
