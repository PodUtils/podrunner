//! A credential: the files that the runner reads at start (the JIT runner
//! configuration). The form of the wire is `pod_job::source::Minted`
//! (podcrates `v0.72.0`); the check of a file name and the writer of the
//! files stay in podrunner.

use std::borrow::Cow;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

pub use pod_job::source::{Hidden as Secret, Minted as Credential, MintedFile as CredentialFile};

/// The start of the content of a binary credential file (T-577): the mark,
/// then the bytes in base64. The wire of a credential holds text only, and
/// no text file of a credential starts with a NUL.
pub const BINARY_MARK: &str = "\0base64:";

/// A credential file that holds `bytes` (a PKCS #12 trust store, T-577).
/// A binary file holds no secret: the bytes that a driver decodes are not
/// wiped.
pub fn binary_file(name: &str, bytes: &[u8]) -> CredentialFile {
    CredentialFile { name: name.into(), content: Secret::new(format!("{BINARY_MARK}{}", STANDARD.encode(bytes))) }
}

/// The bytes of a credential file.
pub trait FileBytes {
    /// The bytes that a driver writes: the text, or the bytes of a binary
    /// file ([`binary_file`]). A binary file that is not base64 is an
    /// error that names the file only.
    fn bytes(&self) -> Result<Cow<'_, [u8]>, String>;
}

impl FileBytes for CredentialFile {
    fn bytes(&self) -> Result<Cow<'_, [u8]>, String> {
        let text = self.content.expose();
        match text.strip_prefix(BINARY_MARK) {
            None => Ok(Cow::Borrowed(text.as_bytes())),
            Some(b64) => {
                STANDARD.decode(b64).map(Cow::Owned).map_err(|_| format!("the binary file {} is not base64", self.name))
            }
        }
    }
}

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

    /// T-577: a binary file comes back byte for byte; a text file is its
    /// text; a binary file that is not base64 names the file only.
    #[test]
    fn a_binary_file_gives_its_bytes() {
        for bytes in [&b""[..], &[0x30, 0x82, 0x00, 0xff, b'\n'][..], &[0u8; 300][..]] {
            let f = binary_file("store.p12", bytes);
            assert!(f.name_is_plain());
            assert_eq!(f.bytes().unwrap().as_ref(), bytes);
        }
        let text = CredentialFile { name: ".runner".into(), content: Secret::new("{\"a\":1}") };
        assert_eq!(text.bytes().unwrap().as_ref(), b"{\"a\":1}");
        assert!(matches!(text.bytes().unwrap(), Cow::Borrowed(_)), "a text file is not copied");
        // A text that only looks like the mark is not binary: no NUL.
        let look = CredentialFile { name: "a".into(), content: Secret::new("base64:AAAA") };
        assert_eq!(look.bytes().unwrap().as_ref(), b"base64:AAAA");
        let bad = CredentialFile { name: "x.p12".into(), content: Secret::new(format!("{BINARY_MARK}*made-up*")) };
        let e = bad.bytes().unwrap_err();
        assert!(e.contains("x.p12") && !e.contains("made-up"), "{e}");
    }

    #[test]
    fn a_credential_file_name_is_plain() {
        let f = |n: &str| CredentialFile { name: n.into(), content: Secret::new("") }.name_is_plain();
        assert!(f(".credentials") && f(".runner"));
        assert!(!f("") && !f("..") && !f("a/b") && !f("../x"));
    }
}
