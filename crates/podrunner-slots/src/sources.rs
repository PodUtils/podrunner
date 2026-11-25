//! The keys of a `github-scaleset` source: the App (its id, its
//! installation and its key file), or a fine-grained token of the
//! organization (`token_file`, T-174), never both.

use podrunner_schema::JobId;
use podrunner_source::{JobSource, SourceCode, SourceError};
use serde_json::{Map, Value};

use crate::config::{SourceConfig, SourceKind};

/// One source instance.
pub struct Source {
    pub cfg: SourceConfig,
    pub source: Box<dyn JobSource>,
    /// The source revoked podrunner: it gets no capacity.
    pub revoked: bool,
}

/// The place of a source that an operator stopped (T-501): the old source
/// (and its program) went with its last job. It gives no event, and refuses
/// each other call; `resume --source` builds the source again.
pub struct Stopped;

impl JobSource for Stopped {
    fn hello(&mut self, _: &str, _: &str) -> Result<podrunner_source::Hello, SourceError> {
        Err(stopped())
    }
    fn next(&mut self, _: u32, _: std::time::Duration) -> Result<Vec<podrunner_source::Event>, SourceError> {
        Ok(Vec::new())
    }
    fn ack(&mut self, _: &JobId) -> Result<(), SourceError> {
        Err(stopped())
    }
    fn claim(&mut self, _: &JobId, _: &str) -> Result<(), SourceError> {
        Err(stopped())
    }
    fn mint(&mut self, _: &JobId, _: &str) -> Result<podrunner_schema::Credential, SourceError> {
        Err(stopped())
    }
    fn report(&mut self, _: &JobId, _: &Map<String, Value>) -> Result<(), SourceError> {
        Err(stopped())
    }
    fn audit(&mut self, _: &Map<String, Value>) -> Result<(), SourceError> {
        Ok(())
    }
    fn withdraw(&mut self, _: &JobId, _: &str) -> Result<(), SourceError> {
        Err(stopped())
    }
    fn release(&mut self, _: &JobId) -> Result<(), SourceError> {
        Err(stopped())
    }
}

fn stopped() -> SourceError {
    SourceError::new(SourceCode::Unavailable, "the operator stopped this source")
}

/// The keys of a `github-scaleset` source, and its `config_url` with its
/// `api_url` (`crate::config_url`).
pub fn check_scaleset(s: &SourceConfig) -> Result<(), String> {
    let app = [
        ("installation_id", s.installation_id.is_some()),
        ("app_id", s.app_id.is_some()),
        ("key_file", s.key_file.is_some()),
    ];
    if s.token_file.is_some()
        && let Some((key, _)) = app.iter().find(|(_, set)| *set)
    {
        return Err(format!("[[source]] {:?}: token_file and {key}: the App or a token, not both", s.name));
    }
    let mut need = vec![
        ("sidecar", s.sidecar.is_some()),
        ("config_url", s.config_url.is_some()),
        ("scale_set", s.scale_set.is_some()),
    ];
    if s.token_file.is_none() {
        need.extend(app);
    }
    match need.iter().find(|(_, set)| !*set) {
        Some((key, _)) => Err(format!("[[source]] {:?}: a github-scaleset source needs {key}", s.name)),
        None => crate::config_url::check(s),
    }
}

/// A source name is a key of the journal and of the audit lines: it is
/// not empty and has no control character, and `@` starts the names of
/// the host (`@warm`, T-487).
pub fn check_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.starts_with('@') || name.chars().any(char::is_control) {
        return Err(format!("[[source]] {name:?}: the name is empty, starts with @ or has a control character"));
    }
    Ok(())
}

/// The opt-in of a personal account (T-504) is a key of a scale-set source
/// only: another kind has no scale set.
pub fn check_personal(s: &SourceConfig) -> Result<(), String> {
    if s.allow_personal_account && s.kind != SourceKind::GithubScaleset {
        return Err(format!("[[source]] {:?}: allow_personal_account is for a github-scaleset source", s.name));
    }
    Ok(())
}

/// The extra permission of the App that `allow_personal_account` needs.
pub const PERSONAL_PERMISSION: &str = "Administration: write";

/// The line that `check`, `serve` and `mint-serve` print for a source with
/// `allow_personal_account` (T-504, the owner's Q13), after the name of the
/// table.
pub fn personal_note(s: &SourceConfig) -> Option<String> {
    s.allow_personal_account.then(|| {
        format!(
            "[[source]] {}: allow_personal_account: a repository of a personal account may register a runner \
             scale set. The GitHub App (or the token) needs the extra repository permission \"{PERSONAL_PERMISSION}\": \
             GitHub gives the registration token of a repository scale set only with it, and it also lets a leaked \
             key change the settings, the collaborators and the deploy keys of the repository. The account is the \
             only member: a run that a collaborator triggers or re-runs is not trusted, but a run that the \
             account triggers on the code of a collaborator is (residual R11)",
            s.name
        )
    })
}

#[cfg(test)]
mod tests {
    use crate::config::Config;

    fn file(keys: &str) -> Result<Config, String> {
        Config::parse(&format!(
            "host = \"h\"\n[trust]\norganizations = [{{ login = \"o\" }}]\n[[source]]\nname = \"s\"\n\
             kind = \"github-scaleset\"\ntier = \"trusted\"\narch = \"x86_64\"\nsidecar = \"/bin/true\"\n\
             config_url = \"https://github.com/o/r\"\nscale_set = \"s\"\n{keys}[pin]\n"
        ))
    }

    /// T-504 (the owner's Q13): a personal account is an opt-in of a
    /// `github-scaleset` source, off by default; `check` names the extra
    /// permission of the App when it is on.
    #[test]
    fn a_personal_account_is_an_opt_in_of_a_scaleset_source() {
        let app = "installation_id = 1\napp_id = \"1\"\nkey_file = \"/k\"\n";
        for keys in [app.to_string(), format!("{app}allow_personal_account = false\n")] {
            let c = file(&keys).unwrap();
            assert!(!c.source[0].allow_personal_account, "{keys}");
            assert_eq!(super::personal_note(&c.source[0]), None, "{keys}");
        }
        let c = file(&format!("{app}allow_personal_account = true\n")).unwrap();
        let note = super::personal_note(&c.source[0]).unwrap();
        assert!(note.starts_with("[[source]] s: allow_personal_account"), "{note}");
        assert!(
            note.contains("\"Administration: write\"") && note.contains("re-runs") && note.contains("R11"),
            "{note}"
        );
        assert!(file(&format!("{app}allow_personal_account = \"yes\"\n")).is_err());
        // Another kind of source has no scale set: the key is refused.
        let fake = "host = \"h\"\nallow_fake = true\n[[source]]\nname = \"f\"\nkind = \"fake\"\ntier = \"trusted\"\n\
                    arch = \"x86_64\"\nallow_personal_account = true\n[pin]\n";
        let err = Config::parse(fake).unwrap_err();
        assert!(err.contains("allow_personal_account is for a github-scaleset source"), "{err}");
        assert!(
            Config::parse(&fake.replace("allow_personal_account = true", "allow_personal_account = false")).is_ok()
        );
    }

    #[test]
    fn a_scaleset_source_has_the_app_or_a_token() {
        assert!(file("installation_id = 1\napp_id = \"1\"\nkey_file = \"/k\"\n").is_ok());
        assert!(file("token_file = \"/t\"\n").is_ok());
        assert!(file("").unwrap_err().contains("needs installation_id"));
        let both = file("token_file = \"/t\"\nkey_file = \"/k\"\n").unwrap_err();
        assert!(both.contains("not both"), "{both}");
    }
}
