//! The command line of `run` as a contract (T-245, T-627): each flag of the
//! model (`crate::manual`) is a flag of the parser, with or without a
//! value as the model says, and each flag of the parser is in the model.

use std::collections::BTreeSet;

use super::*;

/// The flags of a text: each `--NAME`.
pub(crate) fn flags(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let b = text.as_bytes();
    let mut i = 0;
    while i + 2 < b.len() {
        if &b[i..i + 2] == b"--" && b[i + 2].is_ascii_lowercase() && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) {
            let end = (i + 2..b.len()).find(|&j| !(b[j].is_ascii_lowercase() || b[j] == b'-')).unwrap_or(b.len());
            out.insert(text[i..end].trim_end_matches('-').to_string());
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

/// The flags that `run_config::expand` and `watch::repeat` take before
/// the parser.
pub(crate) const BEFORE_THE_PARSER: [&str; 3] = ["--no-config", "--watch", "--schedule"];

/// The flags of `run` in the model, with whether each takes a value.
fn model() -> Vec<(String, bool)> {
    crate::manual::find("run").expect("run").flags.iter().map(|f| (f.spelling(), f.value.is_some())).collect()
}

/// The flags of the parser: each arm `"--NAME" =>` or `"-x" | "--NAME" =>`.
pub(crate) fn parser_flags() -> BTreeSet<String> {
    let source = include_str!("args.rs");
    let mut out: BTreeSet<String> = source.lines().filter(|l| l.contains("\" =>")).flat_map(flags).collect();
    out.extend(BEFORE_THE_PARSER.iter().map(|s| s.to_string()));
    out
}

#[test]
fn each_flag_of_the_model_is_a_flag_of_the_parser() {
    for (f, value) in model() {
        if BEFORE_THE_PARSER.contains(&f.as_str()) {
            continue;
        }
        let e = parse(&["w.yml".to_string(), f.clone()]).err().unwrap_or_default();
        assert!(!e.starts_with("an unknown option"), "{f}: {e}");
        assert_eq!(e.ends_with("needs a value"), value, "{f}: {e}");
    }
}

#[test]
fn each_flag_of_the_parser_is_in_the_model() {
    let model: BTreeSet<String> = model().into_iter().map(|(f, _)| f).collect();
    assert_eq!(parser_flags(), model, "the parser of run and the model differ");
}
