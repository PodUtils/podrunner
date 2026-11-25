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
    asks: Sender,
    asked: std::sync::Mutex<std::sync::mpsc::Receiver<podrunner_slots::assigned::Question>>,
    /// The units with a question in flight: one each, so a unit that asks
    /// in a loop takes no place of another unit (T-660).
    in_flight: Shared<std::collections::HashSet<String>>,
    /// The units whose hook marked the first step of their job, each once
    /// (T-660): the size is bounded by the units, not by a queue that one
    /// unit can fill.
    steps: Shared<Vec<podrunner_schema::UnitId>>,
    fronts: std::sync::Mutex<std::collections::HashMap<String, front::Front>>,
}

type Shared<T> = std::sync::Arc<std::sync::Mutex<T>>;
type Sender = std::sync::mpsc::Sender<podrunner_slots::assigned::Question>;

/// The place of a unit in `in_flight`, given back at its drop.
struct InFlight(Shared<std::collections::HashSet<String>>, String);

impl Drop for InFlight {
    fn drop(&mut self) {
        if let Ok(mut m) = self.0.lock() {
            m.remove(&self.1);
        }
    }
}

impl EgressOf {
    pub(super) fn new(egress: podrunner_egress::Egress, ttl: Duration) -> EgressOf {
        // No bound of its own: each unit has one question in flight at most.
        let (asks, asked) = std::sync::mpsc::channel();
        EgressOf {
            egress,
            ttl,
            asks,
            asked: std::sync::Mutex::new(asked),
            in_flight: Default::default(),
            steps: Default::default(),
            fronts: Default::default(),
        }
    }

    /// The front of the socket of `unit`: a question goes to slots, each
    /// other request to the module on `egress-inner.sock`. Until the module
    /// opens the job there, a request leads nowhere (a warm unit, T-487).
    fn front(&self, id: &podrunner_schema::UnitId, socket: &Path) -> Result<(), String> {
        let inner = socket.with_file_name("egress-inner.sock");
        let asks = std::sync::Mutex::new(self.asks.clone());
        let (in_flight, steps) = (self.in_flight.clone(), self.steps.clone());
        let unit = id.clone();
        let ask: front::Ask = std::sync::Arc::new(move |route: &str, body: &[u8]| {
            if route == "/v1/started" {
                // The hook does not wait for the tick: the slots journal
                // the mark at the next tick.
                if let Ok(mut s) = steps.lock()
                    && !s.contains(&unit)
                {
                    s.push(unit.clone());
                }
                return r#"{"ok":true}"#.to_string();
            }
            // The hook asks once, before the first step of the job: a
            // second question in flight is a step of the job.
            let first = in_flight.lock().is_ok_and(|mut m| m.insert(unit.to_string()));
            if !first {
                return r#"{"verdict":"end","code":"question-in-flight"}"#.to_string();
            }
            let _place = InFlight(in_flight.clone(), unit.to_string());
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            let sent = asks.lock().map(|a| a.send((unit.clone(), body.to_vec(), tx)).is_ok()).unwrap_or(false);
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
        if let Ok(mut s) = self.steps.lock() {
            s.retain(|u| u.as_str() != unit);
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
        self.steps.lock().map(|mut s| std::mem::take(&mut *s)).unwrap_or_default()
    }
}
