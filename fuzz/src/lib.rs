//! The fuzz targets of podrunner. Each target is a function here, so that
//! a stable test runs it on its seeds and their mutations
//! (`src/tests.rs`), and libFuzzer runs it with nightly Rust.

/// A fuzz target.
pub type Target = fn(&[u8]);

/// Each target, by name.
pub const TARGETS: [(&str, Target); 5] = [
    ("control_line", targets::control_line),
    ("hook_input", targets::hook_input),
    ("slots_config", targets::slots_config),
    ("slots_journal", targets::slots_journal),
    ("source_wire", targets::source_wire),
];

pub mod targets {
    use podrunner::control;
    use podrunner_hook::input::{Command, parse};
    use podrunner_hook::policy::{Gate, Policy};

    /// The reader of a hook call and the checks of the host: no panic on any
    /// body, for each command; a gate refuses or plans.
    pub fn hook_input(data: &[u8]) {
        let policy = Policy {
            container: podrunner_schema::ContainerRoute::Hooks,
            shared_host_net: data.len() % 2 == 1,
            registries: vec!["docker.io".into()],
            work_root: "/home/runner".into(),
        };
        let mut gate = Gate::default();
        for command in [Command::PrepareJob, Command::RunScriptStep, Command::RunContainerStep, Command::CleanupJob] {
            if let Ok(input) = parse(command, data) {
                let _ = gate.check(&input, &policy);
            }
        }
    }
    use podrunner_slots::config::Config;

    /// The reader of a line of the control socket: no panic on any line.
    pub fn control_line(data: &[u8]) {
        let _ = control::parse(data);
    }

    use podrunner_slots::state::Line;
    use podrunner_source::wire;

    /// The reader of `podrunner.toml`: no panic on any text.
    pub fn slots_config(data: &[u8]) {
        if let Ok(text) = std::str::from_utf8(data) {
            let _ = Config::parse(text);
        }
    }

    /// The reader of a journal line: no panic; a line that it reads writes
    /// back to a line that reads the same.
    pub fn slots_journal(data: &[u8]) {
        if let Ok(line) = serde_json::from_slice::<Line>(data) {
            let text = serde_json::to_string(&line).expect("a line is JSON");
            assert_eq!(serde_json::from_str::<Line>(&text).ok(), Some(line));
        }
    }

    /// The parser of the source wire: no panic on any line; a line that it
    /// accepts writes back to a line that reads the same.
    pub fn source_wire(data: &[u8]) {
        if let Ok(req) = wire::parse_request(data) {
            let line = wire::request_line(&req);
            assert_eq!(wire::parse_request(line.as_bytes()).ok(), Some(req));
        }
        if let Ok(reply) = wire::parse_reply(data) {
            let line = wire::reply_line(&reply);
            assert_eq!(wire::parse_reply(line.as_bytes()).ok(), Some(reply));
        }
    }
}

#[cfg(test)]
mod tests;
