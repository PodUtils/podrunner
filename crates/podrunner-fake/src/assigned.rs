//! The answer of the `fake` source to `assigned` (T-493): a job of the
//! owner of the offer is kept, with the trust facts of the offer where the
//! real job gives none; a job of another owner ends with `not-of-this-source`.

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
    t.head_is_fork = match real.head_is_fork {
        Some(true) => Fact::Yes,
        Some(false) => Fact::No,
        None => t.head_is_fork,
    };
    Assigned::Keep(t)
}

fn end(code: &str, detail: &str) -> Assigned {
    Assigned::End { code: code.into(), detail: detail.into() }
}
