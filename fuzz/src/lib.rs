//! The fuzz targets of podrunner. Each target is a function here, so that
//! a stable test runs it on its seeds and their mutations
//! (`src/tests.rs`), and libFuzzer runs it with nightly Rust.

/// A fuzz target.
pub type Target = fn(&[u8]);

/// Each target, by name.
pub const TARGETS: [(&str, Target); 1] = [("source_wire", targets::source_wire)];

pub mod targets {
    use podrunner_source::wire;

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
