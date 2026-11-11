//! `mint` and `sign` (GitHub #27). `sign` holds the private key of the
//! GitHub App and signs one JWT shape; `mint` holds the source credentials
//! and runs the source sidecar as its child (GitHub #29), which asks `sign`
//! and never holds the key.

mod mint;
pub mod sign;

pub use mint::Mint;
