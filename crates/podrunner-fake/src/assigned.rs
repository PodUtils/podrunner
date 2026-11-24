//! The answer of the `fake` source to `assigned` (T-493): a job of the
//! owner of the offer is kept, with the actor and the visibility of the
//! offer, and the event, the run and the fork fact of the real job; a job
//! of another owner ends with `not-of-this-source`.

use podrunner_schema::Fact;
use podrunner_schema::offer::{Event as ForgeEvent, TrustInputs};
use podrunner_source::{Assigned, Offer, RealJob};

/// The answer for the offer `offer` and the real job `real`.
pub(crate) fn answer(offer: &Offer, real: &RealJob) -> Assigned {
    let Some(mut t) =
        offer.fields.get("trust_inputs").and_then(|v| serde_json::from_value::<TrustInputs>(v.clone()).ok())
    else {
        return end("no-trust-inputs", "the offer has no trust facts");
    };
    let Some((owner, repo)) = real.repository.split_once('/') else {
        return end("bad-repository", "the repository is not OWNER/REPO");
    };
    if !owner.eq_ignore_ascii_case(&t.owner) {
        return end("not-of-this-source", "the job is of another owner");
    }
    t.repo = repo.to_string();
    t.run_id = real.run_id;
    t.event = serde_json::from_value(serde_json::Value::String(real.event.clone())).unwrap_or(ForgeEvent::Unknown);
    // Never the fact of the offer: the real job can be of another run and
    // another event. A head is a fork when it is not the base.
    t.head_is_fork = match (real.head_is_fork, &real.head_repository) {
        (Some(true), _) => Fact::Yes,
        (Some(false), _) => Fact::No,
        (None, Some(head)) if head.is_empty() => Fact::Unknown,
        (None, Some(head)) if head.eq_ignore_ascii_case(&real.repository) => Fact::No,
        (None, Some(_)) => Fact::Yes,
        (None, None) => Fact::Unknown,
    };
    Assigned::Keep(t)
}

fn end(code: &str, detail: &str) -> Assigned {
    Assigned::End { code: code.into(), detail: detail.into() }
}
