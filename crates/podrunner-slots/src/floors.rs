//! The floors of the tiers.

use podrunner_schema::{Boundary, Tier};

/// The lowest floor of each tier that the configuration may set. `trusted`
/// may go to `redirect` with the mitigations of DR-4.
pub fn lowest_floor(t: Tier) -> Boundary {
    match t {
        Tier::Untrusted => Boundary::Guest,
        Tier::Trusted => Boundary::Redirect,
        Tier::Local => Boundary::None,
    }
}

/// The floor of a tier that the configuration does not set: `redirect`
/// for `trusted` is a choice of the operator, never a default.
pub fn default_floor(t: Tier) -> Boundary {
    match t {
        Tier::Trusted => Boundary::Confined,
        t => lowest_floor(t),
    }
}
