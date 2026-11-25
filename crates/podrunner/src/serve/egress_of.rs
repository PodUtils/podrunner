//! The egress module behind slots (T-493, T-487): the socket of each unit
//! is a front, and the module of its job listens on an inner socket.

use std::path::Path;
use std::time::Duration;

use super::front;

/// The egress module behind slots. The module of a unit listens on an
/// inner socket; the socket that the unit sees is a front that also
/// carries the questions of its job-started hook (T-493).
pub(super) struct EgressOf {
    egress: podrunner_egress::Egress,
    ttl: Duration,
    asks: std::sync::mpsc::SyncSender<podrunner_slots::assigned::Question>,
    asked: std::sync::Mutex<std::sync::mpsc::Receiver<podrunner_slots::assigned::Question>>,
    /// The units whose hook marked the first step of their job (T-660).
    steps: std::sync::mpsc::SyncSender<podrunner_schema::UnitId>,
    stepped: std::sync::Mutex<std::sync::mpsc::Receiver<podrunner_schema::UnitId>>,
    fronts: std::sync::Mutex<std::collections::HashMap<String, front::Front>>,
}

impl EgressOf {
    pub(super) fn new(egress: podrunner_egress::Egress, ttl: Duration) -> EgressOf {
        // A bounded queue: a unit that asks too much gets `end`.
        let (asks, asked) = std::sync::mpsc::sync_channel(64);
        // A mark that finds the queue full is lost: the slots read the
        // queue each tick, and a job has one first step.
        let (steps, stepped) = std::sync::mpsc::sync_channel(64);
        EgressOf {
            egress,
            ttl,
            asks,
            asked: std::sync::Mutex::new(asked),
            steps,
            stepped: std::sync::Mutex::new(stepped),
            fronts: Default::default(),
        }
    }

    /// The front of the socket of `unit`: a question goes to slots, each
    /// other request to the module on `egress-inner.sock`. Until the module
    /// opens the job there, a request leads nowhere (a warm unit, T-487).
    fn front(&self, id: &podrunner_schema::UnitId, socket: &Path) -> Result<(), String> {
        let inner = socket.with_file_name("egress-inner.sock");
        let asks = std::sync::Mutex::new(self.asks.clone());
        let steps = std::sync::Mutex::new(self.steps.clone());
        let unit = id.clone();
        let ask: front::Ask = std::sync::Arc::new(move |route: &str, body: &[u8]| {
            if route == "/v1/started" {
                // The hook does not wait for the tick: the slots journal
                // the mark at the next tick.
                let _ = steps.lock().map(|s| s.try_send(unit.clone()));
                return r#"{"ok":true}"#.to_string();
            }
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            let sent = asks.lock().map(|a| a.try_send((unit.clone(), body.to_vec(), tx)).is_ok()).unwrap_or(false);
            match sent.then(|| rx.recv_timeout(ASK_WAIT).ok()).flatten() {
                Some(answer) => answer,
                None => r#"{"verdict":"end","code":"source-unavailable"}"#.to_string(),
            }
        });
        let f = front::serve(socket, inner, ask, front::Bounds::default())
            .map_err(|e| format!("the front of the egress socket: {}", e.kind()))?;
        if let Ok(mut m) = self.fronts.lock() {
            m.insert(id.to_string(), f);
        }
        Ok(())
    }
}

/// The time that a question waits for the slots; then the job ends (fail
/// closed).
const ASK_WAIT: Duration = Duration::from_secs(10);

impl podrunner_slots::EgressPort for EgressOf {
    fn open(&self, unit: &str, policy: &str, socket: &Path) -> Result<usize, String> {
        self.open_with(unit, policy, Vec::new(), socket).map(|o| o.flagged)
    }
    fn open_with(
        &self,
        unit: &str,
        policy: &str,
        secrets: Vec<(String, podrunner_schema::Secret)>,
        socket: &Path,
    ) -> Result<podrunner_slots::Opened, String> {
        let id = podrunner_schema::UnitId::parse(unit).map_err(|e| e.to_string())?;
        let inner = socket.with_file_name("egress-inner.sock");
        let values = secrets.iter().fold(podrunner_egress::Secrets::new(), |s, (n, v)| s.with(n, v.expose()));
        let opened = self.egress.open_with(unit, policy, values, &inner, self.ttl).map_err(|e| e.to_string())?;
        let out = podrunner_slots::Opened {
            flagged: opened.flagged,
            ca_bundle: opened.ca_bundle.map(podrunner_schema::Secret::new),
            ca_vars: podrunner_egress::tls::ca_vars(),
            trust_store: opened
                .trust_store
                .map(|s| (s, podrunner_egress::tls::java_options as podrunner_slots::JavaOptions)),
        };
        // A unit that has a front keeps it: a new bind would give another
        // socket file, and the drop of the old front removes the path.
        if self.fronts.lock().is_ok_and(|m| m.contains_key(unit)) {
            return Ok(out);
        }
        if let Err(e) = self.front(&id, socket) {
            self.egress.close(unit);
            return Err(e);
        }
        Ok(out)
    }
    fn hold(&self, unit: &str, socket: &Path) -> Result<(), String> {
        let id = podrunner_schema::UnitId::parse(unit).map_err(|e| e.to_string())?;
        if !self.egress.alive() {
            return Err("egress is down".into());
        }
        self.front(&id, socket)
    }
    fn close(&self, unit: &str) {
        if let Ok(mut m) = self.fronts.lock() {
            m.remove(unit);
        }
        self.egress.close(unit);
    }
    fn alive(&self) -> bool {
        self.egress.alive()
    }
    fn lines(&self) -> Vec<(String, serde_json::Map<String, serde_json::Value>)> {
        self.egress.take_lines()
    }
    fn questions(&self) -> Vec<podrunner_slots::assigned::Question> {
        self.asked.lock().map(|r| r.try_iter().collect()).unwrap_or_default()
    }
    fn first_steps(&self) -> Vec<podrunner_schema::UnitId> {
        self.stepped.lock().map(|r| r.try_iter().collect()).unwrap_or_default()
    }
}
