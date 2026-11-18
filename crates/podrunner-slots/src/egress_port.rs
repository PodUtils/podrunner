//! The seam of the egress module as slots sees it.

/// The egress module as slots sees it (`podrunner-egress` behind it): one
/// listener for each unit, bound to its job, and a health.
pub trait EgressPort: Send {
    /// Serve `policy` (JSON of `broker-policy.v1`) for the unit `unit` on
    /// the socket `socket`.
    /// The count of allow rules of the policy under an open suffix (a
    /// host that anyone can publish under), for an audit line.
    fn open(&self, unit: &str, policy: &str, socket: &std::path::Path) -> Result<usize, String>;
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
