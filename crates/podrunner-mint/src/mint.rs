//! `mint`: the holder of the source credentials. It forwards the nine
//! operations of the source seam to its source (in v1 the `fake` source or
//! the scale-set sidecar), gives a credential as files only, and stops for
//! good after a revocation: each later call is `source-revoked` and reaches
//! the source no more, so that `slots` drains and nothing restarts in a
//! loop (the class `Revoked` of `pod-core`, exit code 78).

use std::time::Duration;

use podrunner_schema::{Credential, JobId};
use podrunner_source::{Event, Hello, JobSource, SourceCode, SourceError};
use serde_json::{Map, Value};

/// `mint` over one source.
pub struct Mint {
    source: Box<dyn JobSource>,
    /// The name of the revoked credential, once the source revoked it.
    revoked: Option<String>,
    name: String,
}

impl Mint {
    /// `name` names the credential in messages: never its value.
    pub fn new(name: &str, source: Box<dyn JobSource>) -> Mint {
        Mint { source, revoked: None, name: name.into() }
    }

    pub fn revoked(&self) -> bool {
        self.revoked.is_some()
    }

    fn call<T>(&mut self, f: impl FnOnce(&mut dyn JobSource) -> Result<T, SourceError>) -> Result<T, SourceError> {
        if let Some(name) = &self.revoked {
            return Err(SourceError::new(SourceCode::SourceRevoked, format!("the credential {name} is revoked")));
        }
        let got = f(self.source.as_mut());
        if let Err(e) = &got
            && e.code == SourceCode::SourceRevoked
        {
            self.revoked = Some(self.name.clone());
            return Err(SourceError::new(
                SourceCode::SourceRevoked,
                format!("the credential {} is revoked", self.name),
            ));
        }
        got
    }
}

impl JobSource for Mint {
    fn hello(&mut self, client: &str, version: &str) -> Result<Hello, SourceError> {
        self.call(|s| s.hello(client, version))
    }
    fn next(&mut self, capacity: u32, wait: Duration) -> Result<Vec<Event>, SourceError> {
        self.call(|s| s.next(capacity, wait))
    }
    fn ack(&mut self, job: &JobId) -> Result<(), SourceError> {
        self.call(|s| s.ack(job))
    }
    fn claim(&mut self, job: &JobId, runner: &str) -> Result<(), SourceError> {
        self.call(|s| s.claim(job, runner))
    }
    fn mint(&mut self, job: &JobId, runner: &str) -> Result<Credential, SourceError> {
        let c = self.call(|s| s.mint(job, runner))?;
        if c.files.is_empty() || !c.files.iter().all(|f| f.name_is_plain()) {
            return Err(SourceError::new(
                SourceCode::CredentialUnavailable,
                "the source gave no files, or a name that is not plain",
            ));
        }
        Ok(c)
    }
    fn report(&mut self, job: &JobId, report: &Map<String, Value>) -> Result<(), SourceError> {
        self.call(|s| s.report(job, report))
    }
    fn audit(&mut self, line: &Map<String, Value>) -> Result<(), SourceError> {
        self.call(|s| s.audit(line))
    }
    fn withdraw(&mut self, job: &JobId, reason: &str) -> Result<(), SourceError> {
        self.call(|s| s.withdraw(job, reason))
    }
    fn release(&mut self, job: &JobId) -> Result<(), SourceError> {
        self.call(|s| s.release(job))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use podrunner_clock::ManualClock;
    use podrunner_fake::{FakeSource, parse_script};
    use podrunner_source::conformance::{self, Fixture};

    use super::*;

    fn script() -> String {
        (1..=3)
            .map(|i| format!(r#"{{"at_s":0,"offer":{{"job_id":"j-{i}","arch":"x86_64"}}}}"#))
            .collect::<Vec<_>>()
            .join("\n")
    }

    struct Fx;

    impl Fixture for Fx {
        fn fresh(&self) -> Box<dyn JobSource> {
            let fake = FakeSource::new("fake-1", Arc::new(ManualClock::default()), parse_script(&script()).unwrap());
            Box::new(Mint::new("fake-1", Box::new(fake)))
        }
    }

    #[test]
    fn mint_passes_the_source_tests() {
        let results = conformance::run(&Fx, &[1, 2, 3, 4, 5, 10, 11]);
        assert!(conformance::not_passed(&results).is_empty(), "{results:#?}");
    }

    /// A source that revokes at its second call.
    struct Revoking(u32);

    impl JobSource for Revoking {
        fn hello(&mut self, _: &str, _: &str) -> Result<Hello, SourceError> {
            self.0 += 1;
            if self.0 > 1 {
                return Err(SourceError::new(SourceCode::SourceRevoked, "401 secret-value-77"));
            }
            Ok(Hello { source: "s".into(), operator_controlled: true })
        }
        fn next(&mut self, _: u32, _: Duration) -> Result<Vec<Event>, SourceError> {
            self.0 += 1;
            Ok(vec![])
        }
        fn ack(&mut self, _: &JobId) -> Result<(), SourceError> {
            Ok(())
        }
        fn claim(&mut self, _: &JobId, _: &str) -> Result<(), SourceError> {
            Ok(())
        }
        fn mint(&mut self, _: &JobId, _: &str) -> Result<Credential, SourceError> {
            Ok(Credential::default())
        }
        fn report(&mut self, _: &JobId, _: &Map<String, Value>) -> Result<(), SourceError> {
            Ok(())
        }
        fn audit(&mut self, _: &Map<String, Value>) -> Result<(), SourceError> {
            Ok(())
        }
        fn withdraw(&mut self, _: &JobId, _: &str) -> Result<(), SourceError> {
            Ok(())
        }
        fn release(&mut self, _: &JobId) -> Result<(), SourceError> {
            Ok(())
        }
    }

    #[test]
    fn a_revocation_is_final_and_names_no_secret() {
        let mut m = Mint::new("app-key-of-org-a", Box::new(Revoking(0)));
        m.hello("p", "0").unwrap();
        let e = m.hello("p", "0").unwrap_err();
        assert_eq!(e.code, SourceCode::SourceRevoked);
        assert!(e.detail.contains("app-key-of-org-a") && !e.detail.contains("secret-value-77"), "{e}");
        assert!(m.revoked());
        // No later call reaches the source.
        assert_eq!(m.next(1, Duration::ZERO).unwrap_err().code, SourceCode::SourceRevoked);
    }

    #[test]
    fn a_credential_with_no_files_is_refused() {
        let mut m = Mint::new("k", Box::new(Revoking(0)));
        let job = JobId::new("j").unwrap();
        assert_eq!(m.mint(&job, "r").unwrap_err().code, SourceCode::CredentialUnavailable);
    }
}
