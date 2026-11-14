//! The configuration files of `podrunner run` (T-143). Each key of a file
//! is a flag of `run` with no `--`; the flags of the files go before those
//! of the command line, so a flag of the command line wins. The order: the
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

/// The keys that a project file may set: they reach no resource of the
/// host. `var-file`, `input-file` and `env-file` must stay in the project.
const SAFE: &[&str] = &[
    "event",
    "var",
    "input",
    "env",
    "matrix",
    "job",
    "protected",
    "remote",
    "actor",
    "repository",
    "quiet",
    "json",
    "no-cache",
    "offline",
    "frozen",
    "checkout-head",
    "with-git",
    "force-event",
    "var-file",
    "input-file",
    "env-file",
];

/// The keys of the file of the user, or of a trusted project: they name
/// binaries, stores, hosts, secrets or approvals of the host.
const HOST: &[&str] = &[
    "runner-dir",
    "podvm",
    "microvm",
    "incus",
    "engine",
    "images",
    "image",
    "podnet",
    "allow",
    "actions",
    "artifacts",
    "cache",
    "secret",
    "secret-file",
    "github-token-env",
    "log-dir",
    "state",
    "export",
    "local-repository",
    "action-server",
    "oidc-issuer",
    "oidc-keys",
    "environment-secret-file",
    "environment-var-file",
    "environment-allow",
    "approve",
    "platform",
    "checkout",
    "ref",
    "sha",
    "event-path",
];

/// The keys whose value is a path: a relative path is read from the
/// directory of the project (the project file) or of the file (the user
/// file).
const PATHS: &[&str] = &[
    "runner-dir",
    "podvm",
    "microvm",
    "incus",
    "engine",
    "images",
    "actions",
    "artifacts",
    "cache",
    "secret-file",
    "log-dir",
    "state",
    "export",
    "oidc-keys",
    "event-path",
    "var-file",
    "input-file",
    "env-file",
    "checkout",
];

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
        let safe = SAFE.contains(&key.as_str());
        if !safe && !HOST.contains(&key.as_str()) {
            return Err(format!("{name}: `{key}` is not a key of run (each key is a flag of run with no --)"));
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
            match PATHS.contains(&key.as_str()) {
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
fn actrc(text: &str, name: &str, project: bool, trusted: bool, notes: &mut Vec<String>) -> Vec<String> {
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
        let host = HOST.contains(&key.as_str());
        match (ACTRC.contains(&key.as_str()), value) {
            // A secret of `.actrc` is NAME=VALUE or NAME; podrunner takes NAME only.
            (true, Some(v)) if key == "secret" && v.contains('=') => {
                notes.push(format!("{name}: -s NAME=VALUE: podrunner takes no value of a secret from a file of flags"));
            }
            (true, Some(_)) if project && host && !trusted => {
                notes.push(format!("{name}: --{key} reaches the host; it is not used for an untrusted project"));
            }
            (true, Some(v)) => out.extend([format!("--{key}"), v]),
            _ => notes.push(format!("{name}: {w} is not a flag of podrunner run; it is not used")),
        }
    }
    out
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
        flags.extend(actrc(&text, "~/.actrc", false, true, &mut notes));
    }
    if let Some(dir) = user_dir().map(|d| d.join("podrunner")) {
        let file = dir.join("run.toml");
        if let Ok(text) = std::fs::read_to_string(&file) {
            let name = file.display().to_string();
            let table: toml::Table = toml::from_str(&text).map_err(|e| format!("{name}: {e}"))?;
            let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
            trusted = match table.get("trusted") {
                Some(toml::Value::Array(list)) => {
                    list.iter().filter_map(toml::Value::as_str).any(|p| canon(Path::new(p)) == canon(&project))
                }
                Some(_) => return Err(format!("{name}: `trusted` is a list of directories")),
                None => false,
            };
            flags.extend(flags_of(&text, &name, &dir, false, true)?);
        }
    }
    if let Ok(text) = std::fs::read_to_string(project.join(".actrc")) {
        flags.extend(actrc(&text, ".actrc", true, trusted, &mut notes));
    }
    let file = project.join(".podrunner/run.toml");
    if let Ok(text) = std::fs::read_to_string(&file) {
        flags.extend(flags_of(&text, &file.display().to_string(), &project, true, trusted)?);
    }
    for n in notes {
        eprintln!("podrunner: {n}");
    }
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

#[cfg(test)]
mod tests;
