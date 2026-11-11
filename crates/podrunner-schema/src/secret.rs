//! A credential: the files that the runner reads at start (the JIT runner
//! configuration). The bytes go on the source wire and into files of mode
//! 0600; they never go into argv, the environment, a log or a report.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A secret text. `Debug` and `Display` print no byte of it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "schema"), derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(s: impl Into<String>) -> Secret {
        Secret(s.into())
    }

    /// The text, for the one writer of the credential file.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Secret(<{} bytes>)", self.0.len())
    }
}

/// One file of a credential. The name is a plain file name: no `/`, no
/// `..`, and not empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "schema"), derive(schemars::JsonSchema))]
pub struct CredentialFile {
    pub name: String,
    pub content: Secret,
}

impl CredentialFile {
    /// True when the name is a plain file name.
    pub fn name_is_plain(&self) -> bool {
        let n = self.name.as_str();
        !n.is_empty() && n.len() <= 64 && n != "." && n != ".." && !n.contains(['/', '\\', '\0'])
    }
}

/// The files that the runner needs for one job.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "schema"), derive(schemars::JsonSchema))]
pub struct Credential {
    pub files: Vec<CredentialFile>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_prints_no_secret() {
        let c =
            Credential { files: vec![CredentialFile { name: ".runner".into(), content: Secret::new("CANARY-77") }] };
        let shown = format!("{c:?}");
        assert!(!shown.contains("CANARY-77"), "{shown}");
        assert!(shown.contains("9 bytes"), "{shown}");
    }

    #[test]
    fn a_credential_file_name_is_plain() {
        let f = |n: &str| CredentialFile { name: n.into(), content: Secret::new("") }.name_is_plain();
        assert!(f(".credentials") && f(".runner"));
        assert!(!f("") && !f("..") && !f("a/b") && !f("../x"));
    }
}
