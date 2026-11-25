//! T-480 with T-493: the forge can give the runner of a job with a cache a
//! queued job of another run. podrunner made the cache of the unit for
//! the offer; the real job keeps it only when its own facts give the same
//! scope and a mode that is not lower.

use podrunner_schema::{Boundary, EgressHold, Principal, UnitId};
use podrunner_source::RealJob;
use serde_json::{Value, json};

use super::*;
use crate::assigned::Verdict;
use crate::state::State;

/// A unit of a trusted job with a cache (the offer `with`), whose runner
/// waits.
fn cached(with: Value) -> (Bed, UnitId) {
    let mut bed = Bed::new(
        config("[cache]"),
        vec![("a", "trusted", None, vec![offer(0, "j-1", with)])],
        vec![KindSpec {
            name: "fake",
            slots: 1,
            capability: Capability {
                mount_modes: vec!["ro".into(), "rw".into()],
                ..capability(Boundary::Confined, Principal::Distinct, EgressHold::Advisory)
            },
            command: sh("exec sleep 300"),
        }],
    );
    bed.until(10, |s| s.jobs().any(|j| j.state == State::Started));
    let j = bed.slots.jobs().next().expect("a job");
    assert!(j.times.cache.is_some(), "the job has a cache");
    let unit = j.unit.clone().expect("a unit");
    (bed, unit)
}

fn real(repository: &str, run_id: u64, event: &str) -> RealJob {
    RealJob {
        run_id,
        job_name: "test".into(),
        event: event.into(),
        repository: repository.into(),
        head_repository: None,
        head_is_fork: Some(false),
        actor: "a".into(),
        job_id: None,
        git_ref: None,
        repo_id: None,
    }
}

fn ask(with: Value, job: RealJob) -> Verdict {
    let (mut bed, unit) = cached(with);
    bed.slots.assigned(&unit, &job)
}

fn end(code: &str) -> Verdict {
    Verdict::End { code: code.into() }
}

#[test]
fn a_job_of_another_run_does_not_get_the_rw_cache_of_the_offer() {
    // The push to main of the offer has `rw`.
    let push = json!({});
    assert_eq!(ask(push.clone(), real("example-org/app", 1, "push")), Verdict::Keep, "the run of the offer");
    // A pull request of the same repository would write what the next
    // push to main reads.
    assert_eq!(ask(push.clone(), real("example-org/app", 2, "pull_request")), end("cache-mismatch"));
    // A job of another repository has another scope.
    assert_eq!(ask(push.clone(), real("example-org/lib", 3, "push")), end("cache-mismatch"));
    // The source does not know the ref of the push of another run: the
    // mode of its facts is lower.
    assert_eq!(ask(push, real("example-org/app", 4, "push")), end("cache-mismatch"));
}

#[test]
fn a_job_of_another_run_keeps_a_ro_cache_of_its_own_scope() {
    let pr = json!({ "trust_inputs": { "event": "pull_request", "ref": "refs/pull/7/merge" } });
    assert_eq!(ask(pr.clone(), real("example-org/app", 2, "pull_request")), Verdict::Keep);
    assert_eq!(ask(pr, real("example-org/lib", 3, "pull_request")), end("cache-mismatch"));
}

/// P2-A of the review of T-480: the hook does not send the ref of the
/// real job, so a source that gives the ref of the offer in `keep` would
/// keep a push to another branch with `rw` on the scope of the default
/// branch. A job of another run never keeps `rw`.
#[test]
fn a_job_of_another_run_never_keeps_rw() {
    let (bed, _) = cached(json!({}));
    let key = (0, podrunner_schema::JobId::new("j-1").unwrap());
    let mut facts = bed.slots.jobs().next().unwrap().offer.clone().unwrap().trust_inputs;
    facts.run_id = 4;
    assert!(!bed.slots.cache_holds(&key, &real("example-org/app", 4, "push"), &facts), "rw for another run");
}
