//! The seam of the egress module as slots sees it.

use podrunner_schema::Secret;

/// What the module gives for a unit.
#[derive(Debug, Default)]
pub struct Opened {
    /// The count of allow rules of the policy under an open suffix (a
    /// host that anyone can publish under), for an audit line.
    pub flagged: usize,
    /// The CA bundle that the unit trusts (PEM), when the broker ends the
    /// TLS of its job: a credential or a rule with a path (T-563, T-524).
    pub ca_bundle: Option<Secret>,
    /// The variables that name the bundle in the unit (`ca::env` of
    /// `pod-broker`).
    pub ca_vars: Vec<String>,
}

/// The egress module as slots sees it (`podrunner-egress` behind it): one
/// listener for each unit, bound to its job, and a health.
pub trait EgressPort: Send {
    /// Serve `policy` (JSON of `broker-policy.v1`) for the unit `unit` on
    /// the socket `socket`.
    /// The count of allow rules of the policy under an open suffix (a
    /// host that anyone can publish under), for an audit line.
    fn open(&self, unit: &str, policy: &str, socket: &std::path::Path) -> Result<usize, String>;
    /// [`EgressPort::open`] with the values of the secrets that the
    /// credentials of `policy` name. A module with no TLS end holds no
    /// secret.
    fn open_with(
        &self,
        unit: &str,
        policy: &str,
        secrets: Vec<(String, Secret)>,
        socket: &std::path::Path,
    ) -> Result<Opened, String> {
        if !secrets.is_empty() {
            return Err("this egress module holds no secret".into());
        }
        self.open(unit, policy, socket).map(|flagged| Opened { flagged, ..Opened::default() })
    }
    fn close(&self, unit: &str);
    /// False when the module ended: slots gives no capacity and ends the
    /// running units.
    fn alive(&self) -> bool;
    /// The line of each connection that ended since the last call, with
    /// the unit of its listener (T-486).
    fn lines(&self) -> Vec<(String, serde_json::Map<String, serde_json::Value>)> {
        Vec::new()
    }
    /// The questions of the units since the last call (T-493).
    fn questions(&self) -> Vec<crate::assigned::Question> {
        Vec::new()
    }
}
