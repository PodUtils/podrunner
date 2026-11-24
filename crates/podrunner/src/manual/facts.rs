//! The facts of the manual: the variables of the environment, the exit
//! codes, the files and the keys of `run.toml`. A test compares each table
//! with the code, both ways.

use pod_manual::{Exit, Facts, FileFact, Table, Use, Var, VarGroup};

use super::Key;

/// The exit codes of the failure classes that podrunner gives.
pub(crate) const CLASS_CODES: &[i32] = &[64, 66, 69, 75, 76, 77, 78, 80];

fn var(names: &[&str], used_by: &str, usage: Use, meaning: &str) -> Var {
    Var { usage, used_by: used_by.into(), ..Var::reads(names, meaning) }
}

fn variables() -> Vec<VarGroup> {
    let run = vec![
        var(
            &["HOME"],
            "run",
            Use::Reads,
            "The home: the place of the user files and stores of run when XDG is not set.",
        ),
        var(&["XDG_CONFIG_HOME"], "run", Use::Reads, "The directory of the user file podrunner/run.toml."),
        var(
            &["XDG_CACHE_HOME"],
            "run",
            Use::Reads,
            "The directory of the default stores of run: actions, cache and concurrency.",
        ),
        var(&["GIT_TERMINAL_PROMPT"], "run", Use::Sets, "Set to 0 for each call of git: git asks for no password."),
    ];
    let serve = vec![
        var(
            &["NOTIFY_SOCKET"],
            "serve",
            Use::Reads,
            "The socket of systemd: serve sends READY=1 and STOPPING=1 to it.",
        ),
        var(&["WATCHDOG_USEC"], "serve", Use::Reads, "The watchdog of systemd: serve sends WATCHDOG=1 at half of it."),
        var(&["WATCHDOG_PID"], "serve", Use::Reads, "The process that the watchdog of systemd watches."),
        var(
            &["GITHUB_ACTIONS_FORCE_GHES"],
            "serve",
            Use::Reads,
            "When it is set, a source takes its forge as GitHub Enterprise Server, as actions/scaleset does.",
        ),
    ];
    let units = vec![
        var(&["PODVM_STORE"], "", Use::Sets, "The image store of podvm, set for each call of podvm."),
        var(&["PODVM_INCUS_SOCKET"], "doctor", Use::Reads, "The socket of incusd, as podvm finds it."),
        var(&["INCUS_DIR"], "doctor", Use::Reads, "With no PODVM_INCUS_SOCKET: the directory of the socket of incusd."),
        var(
            &["PODRUNNER_UNIT_TAG"],
            "",
            Use::Sets,
            "The tag of a unit of the host kinds: it finds each process of the unit.",
        ),
        var(
            &["PODRUNNER_CONFINE_CANARY"],
            "serve",
            Use::Sets,
            "The mark of the process that the self-test of the confine layer starts.",
        ),
    ];
    vec![
        VarGroup { name: "The local mode".into(), vars: run },
        VarGroup { name: "serve and its sources".into(), vars: serve },
        VarGroup { name: "The unit kinds".into(), vars: units },
        pod_manual::pager::variables(),
    ]
}

fn exits() -> Vec<Exit> {
    let mut out = vec![
        Exit::new(0, "success", "The command did its work."),
        Exit::new(
            1,
            "failed",
            "A job of run failed, a check of doctor --run failed, or a test of source-conformance failed.",
        ),
    ];
    out.extend(
        pod_manual::facts::failure_classes()
            .into_iter()
            .filter(|e| matches!(e.code, pod_manual::Code::Value(n) if CLASS_CODES.contains(&n))),
    );
    out.push(Exit::new(130, "interrupted", "A signal ended a local run."));
    out
}

fn files() -> Vec<FileFact> {
    let f = |path: &str, meaning: &str| FileFact { path: path.into(), meaning: meaning.into() };
    vec![
        f("DIR/ctl.sock", "The control socket of serve, at mode 0600, in its state directory."),
        f("DIR/serve.lock", "The lock of the state directory: one serve at a time."),
        f("DIR/audit.jsonl", "The audit file of serve: a chain of lines."),
        f("podrunner-actions.lock", "The pins of the actions and images of a workflow, beside the workflow file."),
        f(".secrets", "The secrets of a local run, in the checkout tree or the current directory."),
    ]
}

/// The keys of `run.toml`: each flag of `run` with a key. A table, not a
/// configuration of `pod-manual`: its key `config` would name the command.
fn run_toml() -> Table {
    let rows = super::run_flags::RUN.iter().filter(|f| f.key != Key::No).map(|f| {
        let who = match f.key {
            Key::Safe | Key::SafePath => "each file",
            _ => "the user file, or the file of a trusted project",
        };
        vec![f.long.to_string(), f.value.unwrap_or("true").to_string(), f.spelling(), who.to_string()]
    });
    Table {
        key: "run-toml".into(),
        heading: "THE KEYS OF RUN.TOML".into(),
        about: "XDG_CONFIG_HOME/podrunner/run.toml of the user, then .podrunner/run.toml of the project. Each key is \
                a flag of run with no dashes. A flag of the command line wins."
            .into(),
        columns: vec!["Key".into(), "Value".into(), "Flag".into(), "Set by".into()],
        rows: rows.collect(),
    }
}

pub(super) fn facts() -> Facts {
    Facts { variables: variables(), exits: exits(), files: files(), config: Vec::new(), tables: vec![run_toml()] }
}
