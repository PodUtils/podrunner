//! The secrets of a local run, and the files of the run that the checkout
//! leaves out.

use super::Args;

/// The files of the run under the tree `dir`, relative: the tarball of the
/// checkout leaves them out (T-276).
pub(super) fn run_files(a: &Args, dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(root) = dir.canonicalize() else { return Vec::new() };
    [&a.secret_file, &a.env_file, &a.var_file]
        .into_iter()
        .flatten()
        .filter_map(|f| f.canonicalize().ok()?.strip_prefix(&root).ok().map(|r| r.to_path_buf()))
        .collect()
}

/// The secrets of a run and its token.
pub(super) type Secrets = (Vec<(String, String)>, Option<String>);

pub(super) fn secrets(a: &Args) -> Result<Secrets, String> {
    use podrunner_local::secrets::{from_env, read_file};
    let mut out: Vec<(String, String)> = Vec::new();
    let mut add = |more: Vec<(String, String)>| {
        for (k, v) in more {
            out.retain(|(o, _)| !o.eq_ignore_ascii_case(&k));
            out.push((k, v));
        }
    };
    // `.secrets` of the checkout, else of the current directory, gets each
    // check of `--secret-file`, also in a dry run (T-341).
    let dir = a.checkout.clone().or_else(|| std::env::current_dir().ok());
    if let Some(dir) = dir.filter(|d| std::fs::symlink_metadata(d.join(".secrets")).is_ok()) {
        // A link of the project must not read a file out of it (T-275).
        let file = dir.join(".secrets");
        match (file.canonicalize(), dir.canonicalize()) {
            (Ok(real), Ok(root)) if real.starts_with(&root) => add(read_file(&file)?),
            _ => return Err(format!("{}: a symbolic link leads out of the tree", file.display())),
        }
    }
    if let Some(file) = &a.secret_file {
        add(read_file(file)?);
    }
    for name in &a.secrets {
        add(vec![from_env(name)?]);
    }
    let token = match &a.token_env {
        // An empty NAME sends no token, as the documents say (#229).
        Some(name) if name.is_empty() => None,
        Some(name) => {
            Some(std::env::var(name).map_err(|_| format!("--github-token-env {name}: the environment has no {name}"))?)
        }
        None => None,
    };
    Ok((out, token))
}
