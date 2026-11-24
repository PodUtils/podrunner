//! The configuration files of `podrunner run` (T-143). Each key of a file
//! is a flag of `run` with no `--`; the flags of the files go before those
//! of the command line, so a flag of the command line wins; an executor of
//! the command line replaces the executor of the files and drops their keys
//! of another executor, and a flag of the command line drops the key of the
//! files that it excludes (T-303, T-627). The order: the
//! file of the user (`$XDG_CONFIG_HOME/podrunner/run.toml`, or
//! `~/.config/podrunner/run.toml`), then the file of the project
//! (`.podrunner/run.toml` in `--checkout DIR`, or in the current
//! directory). An unknown key fails the run. A project file may only set
//! the keys that reach no resource of the host, unless the user file names
//! the project in `trusted`. `--no-config` reads no file.
//!
//! The flags of `.actrc` (the home and the project, as act reads them) that
//! podrunner knows are taken too; each other flag, and a flag of a project
//! `.actrc` that reaches the host, is named on stderr and not used.

use std::path::{Component, Path, PathBuf};

use crate::manual::Key;

/// The keys of a file come from the flags of `run` in the model
/// (`crate::manual`): each flag with a key is a key of the same name.
fn class(key: &str) -> Key {
    crate::manual::run_key(key).unwrap_or(Key::No)
}

/// A project file may set it: it reaches no resource of the host.
/// `var-file`, `input-file` and `env-file` must stay in the project.
fn safe(key: &str) -> bool {
    matches!(class(key), Key::Safe | Key::SafePath)
}

/// Only the file of the user, or of a trusted project, sets it: it names a
/// binary, a store, a host, a secret or an approval of the host.
fn host(key: &str) -> bool {
    matches!(class(key), Key::Host | Key::HostPath)
}

/// A relative path is read from the directory of the project (the project
/// file) or of the file (the user file).
fn path_key(key: &str) -> bool {
    matches!(class(key), Key::SafePath | Key::HostPath)
}

/// The flags of `.actrc` that podrunner takes: those of the same name.
const ACTRC: &[&str] = &["secret-file", "var", "env", "input", "var-file", "env-file", "input-file", "secret"];

fn user_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").filter(|v| !v.is_empty()).map(|h| PathBuf::from(h).join(".config")))
}

/// The value of a path key: relative to `base`; a project path must stay in
/// the project.
fn path_value(key: &str, v: &str, base: &Path, in_project: bool) -> Result<String, String> {
    let p = Path::new(v);
    if in_project && (p.is_absolute() || p.components().any(|c| c == Component::ParentDir)) {
        return Err(format!("{key} = {v:?}: a path of a project file must stay in the project"));
    }
    // A symbolic link in the project must not lead out of it (T-275).
    if in_project {
        let inside = match (base.join(p).canonicalize(), base.canonicalize()) {
            (Ok(real), Ok(root)) => real.starts_with(root),
            (Err(e), _) => e.kind() == std::io::ErrorKind::NotFound && std::fs::symlink_metadata(base.join(p)).is_err(),
            (_, Err(_)) => false,
        };
        if !inside {
            return Err(format!(
                "{key} = {v:?}: a path of a project file must stay in the project (a symbolic link leads out of it)"
            ));
        }
    }
    Ok(if p.is_absolute() { v.to_string() } else { base.join(p).display().to_string() })
}

/// The flags of one TOML file. `project` is the root of the project for a
/// project file, and `trusted` lets it set the keys of the host.
fn flags_of(text: &str, name: &str, base: &Path, project: bool, trusted: bool) -> Result<Vec<String>, String> {
    let table: toml::Table = toml::from_str(text).map_err(|e| format!("{name}: {e}"))?;
    let mut out = Vec::new();
    for (key, value) in &table {
        if key == "trusted" && !project {
            continue;
        }
        let safe = safe(key);
        if !safe && !host(key) {
            return Err(format!(
                "{name}: `{key}` is not a key of run (each key is a flag of run with no --, but for {})",
                crate::manual::not_keys()
            ));
        }
        if project && !safe && !trusted {
            return Err(format!(
                "{name}: `{key}` reaches the host; set it in the file of the user, or name the project in `trusted` there"
            ));
        }
        let flag = format!("--{key}");
        let one = |v: &toml::Value| -> Result<String, String> {
            let s = match v {
                toml::Value::String(s) => s.clone(),
                toml::Value::Integer(i) => i.to_string(),
                _ => return Err(format!("{name}: `{key}` takes a string, a list or a table")),
            };
            match path_key(key) {
                true => path_value(key, &s, base, project && !trusted),
                false => Ok(s),
            }
        };
        match value {
            toml::Value::Boolean(true) => out.push(flag),
            toml::Value::Boolean(false) => {}
            toml::Value::Array(items) => {
                for v in items {
                    out.extend([flag.clone(), one(v)?]);
                }
            }
            toml::Value::Table(t) => {
                for (k, v) in t {
                    out.extend([flag.clone(), format!("{k}={}", one(v)?)]);
                }
            }
            v => out.extend([flag, one(v)?]),
        }
    }
    Ok(out)
}

/// The flags of an `.actrc`: those that podrunner takes, and a note for each
/// other one.
fn actrc(
    text: &str,
    name: &str,
    project: Option<&Path>,
    trusted: bool,
    notes: &mut Vec<String>,
) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let words: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .flat_map(|l| match l.split_once([' ', '=']) {
            Some((f, v)) => vec![f.to_string(), v.trim().to_string()],
            None => vec![l.to_string()],
        })
        .collect();
    let mut it = words.into_iter().peekable();
    while let Some(w) = it.next() {
        let key = match w.as_str() {
            "-s" => "secret".to_string(),
            f => f.trim_start_matches('-').to_string(),
        };
        let value = it.next_if(|v| !v.starts_with('-'));
        let host = host(&key);
        match (ACTRC.contains(&key.as_str()), value) {
            // A secret of `.actrc` is NAME=VALUE or NAME; podrunner takes NAME only.
            (true, Some(v)) if key == "secret" && v.contains('=') => {
                notes.push(format!("{name}: -s NAME=VALUE: podrunner takes no value of a secret from a file of flags"));
            }
            (true, Some(_)) if project.is_some() && host && !trusted => {
                notes.push(format!(
                    "{name}: --{key} reaches the host; it is not used for an untrusted project: name the project in `trusted` of the file of the user"
                ));
            }
            // A file of an untrusted project stays in the project, as in
            // run.toml (T-292).
            (true, Some(v)) if path_key(&key) && !trusted => match project {
                Some(base) => out.extend([
                    format!("--{key}"),
                    path_value(&key, &v, base, true).map_err(|e| format!("{name}: {e}"))?,
                ]),
                None => out.extend([format!("--{key}"), v]),
            },
            (true, Some(v)) => out.extend([format!("--{key}"), v]),
            // The platforms of act are `--platform LABEL=TARGET` of podrunner
            // (T-339 item 9).
            _ if matches!(w.as_str(), "-P" | "--platform") => notes.push(format!(
                "{name}: {w} of act is not used; podrunner maps a label with --platform LABEL=NAME@sha256:HEX|host"
            )),
            _ => notes.push(format!("{name}: {w} is not a flag of podrunner run; it is not used")),
        }
    }
    Ok(out)
}

/// The text of the file `rel` of the project, if it is there. A file that
/// cannot be read fails the run, and an untrusted project cannot name a
/// file out of it through a link (T-292).
fn project_file(project: &Path, rel: &str, trusted: bool) -> Result<Option<String>, String> {
    let file = project.join(rel);
    if std::fs::symlink_metadata(&file).is_err() {
        return Ok(None);
    }
    let real = file.canonicalize().map_err(|e| format!("{}: {e}", file.display()))?;
    if !trusted && !project.canonicalize().is_ok_and(|root| real.starts_with(root)) {
        return Err(format!("{}: a symbolic link leads out of the project", file.display()));
    }
    std::fs::read_to_string(&file).map(Some).map_err(|e| format!("{}: {e}", file.display()))
}

/// The flags of the files, then `args`; `--no-config` reads no file.
pub fn expand(args: &[String]) -> Result<Vec<String>, String> {
    if args.iter().any(|a| a == "--no-config") {
        return Ok(args.iter().filter(|a| *a != "--no-config").cloned().collect());
    }
    let project = args
        .iter()
        .position(|a| a == "--checkout")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| ".".into());
    let mut flags = Vec::new();
    let mut notes = Vec::new();
    let mut trusted = false;
    let home = std::env::var_os("HOME").filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(home) = &home
        && let Ok(text) = std::fs::read_to_string(home.join(".actrc"))
    {
        flags.extend(actrc(&text, "~/.actrc", None, true, &mut notes)?);
    }
    if let Some(dir) = user_dir().map(|d| d.join("podrunner")) {
        let file = dir.join("run.toml");
        if let Ok(text) = std::fs::read_to_string(&file) {
            let name = file.display().to_string();
            let table: toml::Table = toml::from_str(&text).map_err(|e| format!("{name}: {e}"))?;
            let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
            let place = |p: &str| trusted_path(p, &dir, home.as_deref());
            trusted = match table.get("trusted") {
                Some(toml::Value::Array(list)) => {
                    list.iter().filter_map(toml::Value::as_str).any(|p| canon(&place(p)) == canon(&project))
                }
                Some(_) => return Err(format!("{name}: `trusted` is a list of directories")),
                None => false,
            };
            flags.extend(flags_of(&text, &name, &dir, false, true)?);
        }
    }
    if let Some(text) = project_file(&project, ".actrc", trusted)? {
        flags.extend(actrc(&text, ".actrc", Some(&project), trusted, &mut notes)?);
    }
    let file = project.join(".podrunner/run.toml");
    if let Some(text) = project_file(&project, ".podrunner/run.toml", trusted)? {
        flags.extend(flags_of(&text, &file.display().to_string(), &project, true, trusted)?);
    }
    for n in notes {
        eprintln!("podrunner: {n}");
    }
    let flags = command_line_wins(flags, args);
    // The workflow FILE stays first.
    let mut out = Vec::with_capacity(flags.len() + args.len());
    let (file, rest) = match args.split_first() {
        Some((f, rest)) if !f.starts_with('-') => (Some(f.clone()), rest),
        _ => (None, args),
    };
    out.extend(file);
    out.extend(flags);
    out.extend(rest.iter().cloned());
    Ok(out)
}

/// A directory of `trusted`: a relative path is read from the directory of
/// the file, and `~/` is the home (T-314).
fn trusted_path(p: &str, dir: &Path, home: Option<&Path>) -> PathBuf {
    match (p.strip_prefix("~/"), home) {
        (Some(rest), Some(h)) => h.join(rest),
        _ => dir.join(p),
    }
}

/// The executor flags; each one takes a value.
const EXECUTOR: &[&str] = &["--runner-dir", "--podvm", "--microvm", "--incus"];

/// The command line wins over the files (T-303, T-627). An executor of the
/// command line replaces the executor of the files and drops each key of
/// the files that belongs to another executor. For the other flags, a flag
/// of the command line drops the key of the files that it excludes; a
/// key of the files with the same name stays, and the parser takes the last.
fn command_line_wins(flags: Vec<String>, args: &[String]) -> Vec<String> {
    let given: Vec<&str> = args.iter().map(String::as_str).filter(|a| a.starts_with("--")).collect();
    let executors: Vec<&str> = EXECUTOR.iter().copied().filter(|f| given.contains(f)).collect();
    let mut out = Vec::with_capacity(flags.len());
    let mut it = flags.into_iter();
    while let Some(f) = it.next() {
        let drop = (!executors.is_empty() && (EXECUTOR.contains(&f.as_str()) || of_other_executor(&f, &executors)))
            || excluded_by(&f, &given);
        let value = crate::manual::run_key_takes_value(f.trim_start_matches("--"));
        if drop {
            if value {
                it.next();
            }
            continue;
        }
        out.push(f);
        if value {
            out.extend(it.next());
        }
    }
    out
}

/// The keys that only a unit kind can have, and no key of `--runner-dir`
/// has: the image keys. The other keys of a unit kind come from the rules
/// of the model (`Needs`, `NeedsOne`).
const UNIT_IMAGE_KEYS: &[&str] = &["--images", "--image"];

/// Whether the key `flag` of a file belongs to no executor in `given`: a
/// key that needs an executor needs one of `given`, and an image key needs
/// a unit kind.
fn of_other_executor(flag: &str, given: &[&str]) -> bool {
    use crate::manual::R;
    for r in crate::manual::run_rules() {
        if let R::Needs(f, needs) | R::NeedsOne(f, needs) = r
            && *f == flag
            && needs.iter().all(|n| EXECUTOR.contains(n))
        {
            return !needs.iter().any(|n| given.contains(n));
        }
    }
    UNIT_IMAGE_KEYS.contains(&flag) && given == ["--runner-dir"]
}

/// Whether a flag of the command line `given` is in a rule of exclusion
/// with `flag`: the flag of the file loses.
fn excluded_by(flag: &str, given: &[&str]) -> bool {
    use crate::manual::R;
    crate::manual::run_rules().iter().any(|r| match r {
        R::Excludes(group) => group.contains(&flag) && group.iter().any(|g| *g != flag && given.contains(g)),
        _ => false,
    })
}

#[cfg(test)]
mod tests;
