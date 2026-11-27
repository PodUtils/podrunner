//! The flags of `podrunner run`, with the key of `run.toml` that gives
//! each one. `run_config` reads its keys from here.

use super::{F, R, flag, val};

/// What a key of `run.toml` can set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Key {
    /// No key: the flag asks for one run of its own.
    No,
    /// A project file may set it: it reaches no resource of the host.
    Safe,
    /// As `Safe`, and the value is a path in the project.
    SafePath,
    /// Only the file of the user, or of a trusted project, sets it.
    Host,
    /// As `Host`, and the value is a path.
    HostPath,
}

use Key::{Host, HostPath, Safe, SafePath};

pub(crate) const RUN: &[F] = &[
    val(
        "runner-dir",
        "DIR",
        "An unpacked release of the official runner. Each job runs in it as the user of podrunner.",
    )
    .key(HostPath),
    val("podvm", "PATH", "The podvm binary. Each job runs in a fresh podvm unit.").key(HostPath),
    val("podnet", "PATH", "With --podvm: hold each unit under podnet pod.").key(HostPath),
    val("vnet", "PATH", "With --podvm: run the runner of each unit under the vnet shim of this podnet.").key(HostPath),
    val("microvm", "PATH", "The podvm binary. Each job runs in a fresh guest of the microvm kind.").key(HostPath),
    val("incus", "PATH", "The podvm binary. Each job runs in a fresh incus instance.").key(HostPath),
    val("engine", "DIR", "With --microvm or --incus: the container engine of each guest.").key(HostPath),
    val("mem", "SIZE", "With --microvm: the memory of each guest, as 8G (K, M, G or T). Default: 8G.").key(Host),
    val("init", "INIT", "With --microvm or --incus: image boots the init of the runner image in each guest.").key(Host),
    val("run-sh", "PATH", "With a unit kind: run.sh of the runner in the image. Default: /home/runner/run.sh.")
        .key(Host),
    val("images", "STORE", "The image store of podvm.").key(HostPath),
    val("image", "NAME@sha256:HEX", "The runner image of each unit, pinned by digest.").key(Host),
    val("allow", "HOST", "A host that a step can reach through the egress module.").many().key(Host),
    val("platform", "LABEL=TARGET", "Map a label of runs-on to a runner image NAME@sha256:HEX, or to host.")
        .many()
        .key(Host),
    val("actions", "DIR", "The store of the actions. Default: XDG_CACHE_HOME/podrunner/actions.").key(HostPath),
    flag("offline", "Use the lock file and the store only, and fetch nothing.").key(Safe),
    flag("frozen", "Refuse a change of the lock file.").key(Safe),
    val("artifacts", "DIR", "Keep the artifacts of the run in DIR.").key(HostPath),
    val("cache", "DIR", "The store of the cache service. Default: XDG_CACHE_HOME/podrunner/cache.").key(HostPath),
    flag("no-cache", "No cache: each lookup is a miss, and each save is refused.").key(Safe),
    val("secret", "NAME", "A secret from the variable NAME of the environment. A value in argv is refused.")
        .many()
        .key(Host),
    val("secret-file", "FILE", "A file of secrets, one NAME=VALUE on each line, at mode 0600 or less.").key(HostPath),
    val("github-token-env", "NAME", "Give the token of the variable NAME to each job whose permissions are not empty.")
        .key(Host),
    val("job", "ID", "Run the job ID and each job that it needs. With --rerun: run that job again.").many().key(Safe),
    val("matrix", "KEY=VALUE", "Keep only the matrix cells with this value.").many().key(Safe),
    flag("list", "Print each job with its name, runs-on and needs."),
    flag("graph", "Print the stages of needs and each edge."),
    flag("validate", "Parse the file and print each warning."),
    flag("dry-run", "Resolve the pins and build the job message of each cell, with no unit."),
    flag("quiet", "Do not write the new lines of each job log to stderr.").short('q').key(Safe),
    flag("json", "Write one JSON object for each log line and for the end of each job.").key(Safe),
    val("log-dir", "DIR", "Keep the logs of each job in DIR.").key(HostPath),
    val("environment-secret-file", "NAME=FILE", "The secrets of the environment NAME, from FILE at mode 0600.")
        .many()
        .key(Host),
    val("environment-var-file", "NAME=FILE", "The vars of the environment NAME, from FILE.").many().key(Host),
    val("environment-allow", "NAME=HOST", "A host that the jobs of the environment NAME can reach.").many().key(Host),
    val("protected", "NAME", "Mark the environment NAME as protected: its jobs need an approval.").many().key(Safe),
    val("approve", "NAME", "Approve the protected environment NAME.").many().key(Host),
    val("export", "DIR", "Copy the workspace of each job to DIR/JOB.N after the job.").key(HostPath),
    val("toolcache", "DIR", "With --podvm: a tool cache, read-only in each unit.").key(HostPath),
    val("local-repository", "OWNER/REPO@REF=DIR", "Serve the files of DIR as that reference of an action.")
        .many()
        .key(Host),
    val("action-server", "URL", "The forge of the actions in place of https://github.com.").key(Host),
    val("oidc-issuer", "URL", "The issuer of the OIDC tokens. Default: https://oidc.podrunner.invalid.").key(Host),
    val("oidc-keys", "FILE", "Write the public keys of the OIDC tokens to FILE.").key(HostPath),
    flag("watch", "Run again after each change of the tree or of the workflow file."),
    flag("schedule", "Run at each time of the crons of on.schedule, again and again."),
    flag("no-config", "Read no configuration file."),
    val("changed-since", "REF", "The changed files for the paths filters: the files that differ from REF."),
    val("web", "IP:PORT", "Serve a web view of the run on an address of the loopback.").key(Host),
    val("jobs", "N", "With a unit kind: run up to N jobs at a time, each in its own unit. Default: 1.").key(Host),
    val("break", "JOB:STEP", "Pause before the step STEP of the job JOB.").many(),
    flag("break-on-failure", "Pause after the first failed step of each job."),
    flag("keep-failed", "The same as --break-on-failure."),
    val("checkout", "DIR", "The tree of actions/checkout. Default: the git tree of the current directory.")
        .key(HostPath),
    flag("checkout-head", "With --checkout: the committed files of HEAD only.").key(Safe),
    flag("with-git", "With --checkout: give the checkout a .git, and fail out of a git tree.").key(Safe),
    flag("no-git", "With --checkout: give the checkout no .git.").key(Safe),
    val("event", "NAME", "The event of the run. Default: push.").key(Safe),
    val("event-path", "FILE", "The JSON payload of the event.").key(HostPath),
    flag("force-event", "Run the workflow also when its on: does not take the event.").key(Safe),
    val("remote", "NAME", "The git remote that gives github.repository. Default: origin.").key(Safe),
    val("ref", "REF", "The value of github.ref.").key(Host),
    val("sha", "SHA", "The value of github.sha: 40 or 64 hex digits.").key(Host),
    val("repository", "OWNER/REPO", "The value of github.repository.").key(Safe),
    val("actor", "NAME", "The value of github.actor.").key(Safe),
    val("var", "NAME=VALUE", "A value of vars.").many().key(Safe),
    val("input", "NAME=VALUE", "A value of inputs.").many().key(Safe),
    val("var-file", "FILE", "A file of vars, in the syntax of GITHUB_ENV.").key(SafePath),
    val("input-file", "FILE", "A file of inputs, in the syntax of GITHUB_ENV.").key(SafePath),
    val("env", "NAME=VALUE", "A variable of the environment of each job.").many().key(Safe),
    val("env-file", "FILE", "A file of variables of each job, in the syntax of GITHUB_ENV.").key(SafePath),
    val("state", "DIR", "Keep the run in DIR: its state, its logs and its artifacts.").key(HostPath),
    val("rerun", "DIR", "Run the failed cells of the run in DIR again, and each job after them."),
    flag("failed", "With --rerun: run the failed cells, not the jobs of --job."),
    flag("refresh", "With --rerun: accept a workflow that changed since the run."),
];

/// The rules between the flags of `run`. The parser of `run` holds each
/// one, and a test proves it.
pub(crate) const RULES: &[R] = &[
    R::Excludes(&["--podvm", "--microvm", "--incus"]),
    R::Excludes(&["--podnet", "--vnet"]),
    R::Needs("--podnet", &["--podvm"]),
    R::Needs("--vnet", &["--podvm"]),
    R::Needs("--toolcache", &["--podvm"]),
    R::NeedsOne("--engine", &["--microvm", "--incus"]),
    R::Needs("--mem", &["--microvm"]),
    R::NeedsOne("--init", &["--microvm", "--incus"]),
    R::Excludes(&["--init", "--engine"]),
    R::NeedsOne("--run-sh", &["--podvm", "--microvm", "--incus"]),
    R::Excludes(&["--list", "--graph", "--validate", "--dry-run"]),
    R::Excludes(&["--watch", "--schedule"]),
    R::Excludes(&["--web", "--dry-run"]),
    R::Excludes(&["--offline", "--frozen"]),
    R::Excludes(&["--cache", "--no-cache"]),
    R::Needs("--checkout-head", &["--checkout"]),
    R::Needs("--with-git", &["--checkout"]),
    R::Needs("--no-git", &["--checkout"]),
    R::Excludes(&["--with-git", "--no-git"]),
    R::Needs("--failed", &["--rerun"]),
    R::Needs("--refresh", &["--rerun"]),
    R::Excludes(&["--failed", "--job"]),
];

/// The key of `run.toml` named `key`, or `None` when no flag has that name.
pub(crate) fn key(name: &str) -> Option<Key> {
    RUN.iter().find(|f| f.long == name).map(|f| f.key)
}

/// The names of the flags of `run` that are no key, for an error.
pub(crate) fn not_keys() -> String {
    RUN.iter().filter(|f| f.key == Key::No).map(|f| f.long).collect::<Vec<_>>().join(", ")
}

/// Whether the flag `name` (no dashes) takes a value; a name that no flag
/// has takes one.
pub(crate) fn takes_value(name: &str) -> bool {
    RUN.iter().find(|f| f.long == name).is_none_or(|f| f.value.is_some())
}
