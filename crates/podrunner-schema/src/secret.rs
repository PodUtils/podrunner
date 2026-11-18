//! A credential: the files that the runner reads at start (the JIT runner
//! configuration). The form of the wire is `pod_job::source::Minted`
//! (podcrates `v0.72.0`); the check of a file name and the writer of the
//! files stay in podrunner.

pub use pod_job::source::{Hidden as Secret, Minted as Credential, MintedFile as CredentialFile};

/// The check of the name of a credential file.
pub trait PlainName {
    /// True when the name is a plain file name: no `/`, no `..`, and not
    /// empty.
    fn name_is_plain(&self) -> bool;
}

impl PlainName for CredentialFile {
    fn name_is_plain(&self) -> bool {
        let n = self.name.as_str();
        !n.is_empty() && n.len() <= 64 && n != "." && n != ".." && !n.contains(['/', '\\', '\0'])
    }
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
