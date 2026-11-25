//! The checks of the model (T-627, T-248): the text rules, the parser of
//! each command, the help, the generated man page, `docs/cli.md`, the
//! variables and the exit codes. Each fails when the model and the code
//! or a file differ.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pod_manual::check::{ParserCommand, ParserFlag, assert_none, exit_codes, flags, generated, help, model, texts};
use pod_manual::{roff, scan};

use super::{R, all, find, manual, usage};
use crate::cli::run;
use crate::run_local::qa6_tests::contract::{BEFORE_THE_PARSER, flags as long_flags, parser_flags};

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

fn root() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

const MAN_PAGE: &str = "docs/podrunner.1";
const WRITE: &str = "run PODRUNNER_WRITE_SCHEMAS=1 cargo test -p podrunner --lib manual";

#[test]
fn each_text_of_the_manual_keeps_the_rules() {
    assert_none(texts(manual()));
    assert_none(model(manual()));
    assert!(manual().commands.len() >= 25, "{}", manual().commands.len());
}

#[test]
fn the_help_of_each_command_comes_from_the_model() {
    assert_none(help(manual()));
    // A change of the model changes the help: the help shows its words.
    for c in all() {
        let mut a = args(&c.path.split_whitespace().collect::<Vec<_>>());
        a.push("--help".into());
        let o = run(&a);
        assert_eq!(o.code, 0, "{}: {}", c.path, o.stderr);
        assert!(o.stdout.contains(c.about), "{}: {}", c.path, o.stdout);
        let flat = |t: &str| t.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(flat(&o.stdout).contains(&flat(c.usage)), "{}: {}", c.path, o.stdout);
    }
}

/// The arms of the dispatch of `cli.rs` and `help.rs`: the commands of
/// the parser.
fn dispatched() -> BTreeSet<String> {
    let text = format!("{}{}", include_str!("../cli.rs"), include_str!("../help.rs"));
    let mut out = BTreeSet::new();
    for line in text.lines().map(str::trim).filter(|l| l.starts_with('"') && l.contains("=>")) {
        let head = line.split("=>").next().unwrap_or("").split(" if ").next().unwrap_or("");
        out.extend(head.split('|').map(|w| w.trim().trim_matches('"').to_string()));
    }
    // `help` is the word of a help request, not a command of the model.
    out.retain(|w| !w.starts_with('-') && w != "help");
    out
}

/// What the parser of `path` takes: the run parser by its arms, each other
/// command by a probe of its options.
fn parser(path: &[String]) -> Option<ParserCommand> {
    let help = ParserFlag { spellings: args(&["-h", "--help"]), takes_value: false };
    let cli = include_str!("../cli.rs");
    let name = path.join(" ");
    let known = match path {
        [] => true,
        [a] => dispatched().contains(a),
        [a, b] => dispatched().contains(a) && cli.contains(&format!("sub == \"{b}\"")),
        _ => false,
    };
    if !known {
        return None;
    }
    let mut c = ParserCommand { flags: vec![help], subcommands: Vec::new() };
    if path.is_empty() {
        c.flags.push(ParserFlag { spellings: args(&["-V", "--version"]), takes_value: false });
        c.subcommands = dispatched().into_iter().collect();
        return Some(c);
    }
    let model = find(&name)?;
    if name == "run" {
        for f in parser_flags() {
            let e = crate::run_local::refusal(&["w.yml".to_string(), f.clone()]).unwrap_or_default();
            let mut spellings = vec![f.clone()];
            spellings.extend(model.flags.iter().filter(|m| m.is(&f)).filter_map(|m| m.short.map(|s| format!("-{s}"))));
            // The flags before the parser take no value.
            let value = e.ends_with("needs a value") && !BEFORE_THE_PARSER.contains(&f.as_str());
            c.flags.push(ParserFlag { spellings, takes_value: value });
        }
        return Some(c);
    }
    for f in model.flags {
        let s = f.spelling();
        let said = match name.as_str() {
            "man" => crate::manual::man(std::slice::from_ref(&s)).stderr,
            _ => crate::cli::opts(model.path, std::slice::from_ref(&s)).err().map(|o| o.stderr).unwrap_or_default(),
        };
        if !said.contains("an unknown option") {
            let value = said.contains("needs a value");
            c.flags.push(ParserFlag {
                spellings: f.short.map(|s| format!("-{s}")).into_iter().chain([s]).collect(),
                takes_value: value,
            });
        }
    }
    Some(c)
}

#[test]
fn the_model_and_the_parser_agree() {
    assert_none(flags(manual(), parser));
}

/// Each `--NAME` that the code of a command reads is a flag of the model.
#[test]
fn each_flag_that_the_code_reads_is_in_the_model() {
    let mut model: BTreeSet<String> = all().iter().flat_map(|c| c.flags.iter().map(|f| f.spelling())).collect();
    // `pod-manual` gives each command its `--help`.
    model.insert("--help".into());
    for (file, text) in [
        ("cli.rs", include_str!("../cli.rs")),
        ("verbs.rs", include_str!("../verbs.rs")),
        ("cli/unit.rs", include_str!("../cli/unit.rs")),
        ("config_init.rs", include_str!("../config_init.rs")),
    ] {
        for f in long_flags(&scan::non_test(text)) {
            assert!(model.contains(&f), "{file}: {f} is no flag of the model");
        }
    }
}

/// The value of a flag in a test of a rule.
fn sample(value: Option<&str>) -> Option<&'static str> {
    value.map(|v| match v {
        "IP:PORT" => "127.0.0.1:0",
        "ID" | "NAME" => "a",
        _ => "/nonexistent-podrunner-t627",
    })
}

/// The command line of `run` with `names`, each with a value of its kind.
fn run_line(names: &[&str]) -> Vec<String> {
    let run_flags = find("run").expect("run").flags;
    let mut a = args(&["run", "--no-config", "w.yml"]);
    for n in names {
        a.push(n.to_string());
        let f = run_flags.iter().find(|f| f.is(n)).expect("a flag of run");
        a.extend(sample(f.value).map(str::to_string));
    }
    a
}

/// The flags that `flag` needs by the rules of `run`.
fn needs(flag: &str) -> Vec<&'static str> {
    let rules = find("run").expect("run").rules.iter();
    rules
        .filter_map(|r| match r {
            R::Needs(f, l) if *f == flag => Some(l.to_vec()),
            R::NeedsOne(f, l) if *f == flag => Some(l[..1].to_vec()),
            _ => None,
        })
        .flatten()
        .collect()
}

/// The parser of `run` holds each rule of the model, and names its flags.
#[test]
fn the_parser_of_run_holds_each_rule() {
    for r in find("run").expect("run").rules {
        let lines: Vec<(Vec<&str>, Vec<&str>)> = match r {
            // Each of a pair with its needs, so that the clash is the error.
            R::Excludes(l) => {
                let pairs = l.iter().enumerate().flat_map(|(i, a)| l[i + 1..].iter().map(move |b| (*a, *b)));
                pairs.map(|(a, b)| ([vec![a, b], needs(a), needs(b)].concat(), vec![a, b])).collect()
            }
            R::Needs(f, l) | R::NeedsOne(f, l) => {
                vec![(vec![*f], std::iter::once(*f).chain(l.iter().copied()).collect())]
            }
            R::OneOf(l) => vec![(Vec::new(), l.to_vec())],
        };
        for (given, named) in lines {
            let o = run(&run_line(&given));
            assert_eq!(o.code, 64, "{given:?}: {}", o.stderr);
            for n in named {
                assert!(o.stderr.contains(n), "{given:?}: the error does not name {n}: {}", o.stderr);
            }
        }
    }
}

/// The parser of each other command holds the rules of the model.
#[test]
fn the_rules_of_the_model_hold() {
    for (a, why) in [
        (&["status", "--state", "d", "--socket", "s"][..], "give one of --state, --socket"),
        (&["stop", "--timeout", "5", "--state", "d"], "--timeout needs --wait"),
        (&["stop", "--wait", "--socket", "s"], "--wait needs --state"),
        (&["stop", "--wait", "--state", "d", "--source", "x"], "--wait, --source exclude each other"),
        (&["print-unit", "--user", "u", "--config", "/c", "--state", "/s"], "--user needs --system"),
        (&["audit", "verify", "--state", "d", "--file", "f"], "give one of --state, --file"),
        (&["man", "--json", "--roff"], "--json, --roff exclude each other"),
        (&["status", "--source", "x", "--state", "d"], "an unknown option --source"),
    ] {
        let o = run(&args(a));
        assert_eq!(o.code, 64, "{a:?}: {}", o.stderr);
        assert!(o.stderr.contains(why), "{a:?}: {}", o.stderr);
    }
}

/// T-627: the usage of each source program is the one of the model, not a
/// second text.
#[test]
fn the_source_programs_print_the_usage_of_the_model() {
    for (a, path) in [(&["fake-source", "--bogus"][..], "fake-source"), (&["source-conformance"], "source-conformance")]
    {
        let o = run(&args(a));
        assert_eq!(o.code, 64, "{a:?}: {}", o.stderr);
        assert!(o.stderr.contains(&usage(path)), "{a:?}: {}", o.stderr);
    }
}

/// T-627: the text of `PAGER` tells how the value is run.
#[test]
fn the_pager_variable_says_how_it_is_run() {
    let text = serde_json::to_string(&manual().facts).unwrap();
    assert!(text.contains("split at white space and run with no shell"), "{text}");
}

/// Each flag that a usage names is a flag of its command.
#[test]
fn each_usage_names_flags_of_its_command() {
    for c in all() {
        for f in long_flags(c.usage) {
            assert!(c.flags.iter().any(|m| m.is(&f)), "{}: the usage names {f}", c.path);
        }
    }
}

/// The man page of the repository is the page of the model now.
#[test]
fn the_man_page_equals_the_model() {
    let want = roff::render(&manual().clone().for_repository());
    let path = root().join(MAN_PAGE);
    if std::env::var_os("PODRUNNER_WRITE_SCHEMAS").is_some() {
        std::fs::write(&path, &want).unwrap();
    }
    assert_none(generated(&path, &want, WRITE));
}

/// The rows of the table of commands in `docs/cli.md`: the command and
/// the flags of each.
fn rows() -> Vec<(String, BTreeSet<String>)> {
    let cli = std::fs::read_to_string(root().join("docs/cli.md")).unwrap();
    let mut out = Vec::new();
    for line in cli.lines().filter(|l| l.starts_with("| `podrunner ")) {
        let cell = line.trim_start_matches("| `podrunner ").split("` |").next().unwrap_or("");
        let words: Vec<&str> =
            cell.split_whitespace().take_while(|w| w.chars().all(|c| c.is_ascii_lowercase() || c == '-')).collect();
        let path =
            (1..=words.len().min(2)).rev().map(|n| words[..n].join(" ")).find(|p| find(p).is_some() || p == "help");
        let path = path.unwrap_or_else(|| panic!("docs/cli.md: a row of no command: {cell}"));
        let mut flags = long_flags(cell);
        // A short flag, such as `-q`, by the long name of its flag.
        for w in cell.split([' ', '[', ']', '|']).filter(|w| w.len() == 2 && w.starts_with('-') && *w != "--") {
            let c = find(&path).and_then(|c| c.flags.iter().find(|f| f.is(w)));
            flags.insert(c.map_or_else(|| w.to_string(), |f| f.spelling()));
        }
        out.push((path, flags));
    }
    out
}

/// Each command of the model has its rows in `docs/cli.md`, and the rows
/// of a command name exactly its flags.
#[test]
fn the_documents_name_the_flags_of_the_model() {
    let rows = rows();
    for c in all()
        .iter()
        .filter(|c| !c.path.is_empty() && !all().iter().any(|k| k.path.starts_with(&format!("{} ", c.path))))
    {
        let documented: BTreeSet<String> =
            rows.iter().filter(|(p, _)| *p == c.path).flat_map(|(_, f)| f.iter().cloned()).collect();
        let model: BTreeSet<String> = c.flags.iter().map(|f| f.spelling()).collect();
        assert!(rows.iter().any(|(p, _)| *p == c.path), "docs/cli.md has no row of {}", c.path);
        assert_eq!(documented, model, "docs/cli.md and the model differ for {}", c.path);
    }
}

/// The source directories of the crates in the binary.
fn sources() -> Vec<(PathBuf, String)> {
    let crates = root().join("crates");
    let mut dirs = vec![crates.join("podrunner/src")];
    let manifest = include_str!("../../Cargo.toml");
    for line in manifest.lines().filter(|l| l.starts_with("podrunner-")) {
        dirs.push(crates.join(line.split('.').next().unwrap_or("").trim()).join("src"));
    }
    assert!(dirs.len() >= 12, "{dirs:?}");
    // The model names each fact: it is no source that reads one.
    let mut s = scan::rust_sources(&dirs);
    s.retain(|(p, _)| !p.ends_with("manual.rs") && !p.parent().is_some_and(|d| d.ends_with("manual")));
    s.push((PathBuf::from("pod-manual/src/pager.rs"), scan::non_test(pod_manual::pager::SOURCE)));
    s
}

/// Each variable that the code reads or sets is a fact, and each fact is
/// in the code.
#[test]
fn each_variable_of_the_code_is_a_fact() {
    assert_none(pod_manual::check::variables(manual(), &sources(), &["PODRUNNER_ASSIGNMENT_HOOK"]));
    // A planted read of a variable with no fact is a finding.
    let mut planted = sources();
    planted.push((PathBuf::from("planted.rs"), "fn f() { std::env::var(\"PODRUNNER_PLANTED\"); }".into()));
    let found = pod_manual::check::variables(manual(), &planted, &[]);
    assert!(found.iter().any(|f| f.what.contains("PODRUNNER_PLANTED")), "{found:?}");
}

/// Each exit code that the code gives has a fact, and each fact of one code
/// is a code that the code gives: the failure classes that the code names,
/// and the literal codes of an answer.
#[test]
fn each_exit_code_has_a_fact() {
    use pod_core::failure::Class;
    let mut given: BTreeSet<i32> = BTreeSet::from([0]);
    for (_, code) in sources() {
        for c in Class::ALL {
            if code.contains(&format!("Class::{c:?}")) {
                given.insert(i32::from(c.exit_code()));
            }
        }
        for (at, _) in code.match_indices("Out { code: ") {
            let n: String = code[at + 12..].chars().take_while(char::is_ascii_digit).collect();
            given.extend(n.parse::<i32>());
        }
    }
    assert_none(exit_codes(manual(), &given.into_iter().collect::<Vec<_>>()));
}

/// T-627: `man` answers as text and as JSON, and the help of `config
/// init` and `run` names each flag of their rows in `docs/cli.md`.
#[test]
fn man_answers_as_text_and_json() {
    let o = run(&args(&["man", "--no-pager"]));
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(o.stdout.contains("EXIT STATUS") && o.stdout.contains("--vnet PATH"), "{}", o.stdout);
    let o = run(&args(&["man", "--json"]));
    let v: serde_json::Value = serde_json::from_str(&o.stdout).expect("JSON");
    assert_eq!(v["name"], "podrunner");
    let o = run(&args(&["man", "config", "init", "--no-pager"]));
    assert!(o.code == 0 && o.stdout.contains("--key-file FILE"), "{}", o.stdout);
    let o = run(&args(&["man", "RUN", "--no-pager"]));
    assert!(o.code == 0 && o.stdout.contains("--runner-dir DIR"), "{}", o.stderr);
    for bad in ["nope", "", "..", "${{ x }}", "run\u{1b}[2J"] {
        let o = run(&args(&["man", bad, "--no-pager"]));
        assert!(o.code == 64 && o.stderr.contains("no such section"), "{bad:?}: {}", o.stderr);
        assert!(!o.stderr.contains('\u{1b}'), "{bad:?}: a control character is shown escaped");
    }
    assert_eq!(run(&args(&["man", "--bogus"])).code, 64);
}
