//! The seam of the egress module as slots sees it.

use podrunner_schema::Secret;

/// The value of `JAVA_TOOL_OPTIONS` that names a trust store at a path of
/// the unit, before a value of the job (`pod_broker::ca::java_options`).
pub type JavaOptions = fn(&std::path::Path, Option<&str>) -> Result<String, String>;

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
    /// The same bundle as a PKCS #12 trust store for a JVM (T-577), and the
    /// value of `JAVA_TOOL_OPTIONS` that names it.
    pub trust_store: Option<(Vec<u8>, JavaOptions)>,
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
    /// Serve the socket of a warm unit with no job (T-487): each request
    /// leads nowhere until `open_with` of its job, which keeps the same
    /// socket. A module that cannot hold gives no warm unit.
    fn hold(&self, unit: &str, socket: &std::path::Path) -> Result<(), String> {
        let _ = (unit, socket);
        Err("this egress module holds no unit with no job".into())
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
    /// The units whose job-started hook marked the first step of their job
    /// since the last call (`POST /v1/started`, T-660).
    fn first_steps(&self) -> Vec<podrunner_schema::UnitId> {
        Vec::new()
    }
}
