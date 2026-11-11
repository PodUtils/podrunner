//! The canned-message fake broker of GitHub Actions (GitHub #38, level 2):
//! the official runner runs a job offline. It serves the routes that runner
//! v2.338.2 calls with a JIT configuration of a scale set: the token of the
//! OAuth client, the broker session and its messages, and the run service
//! (`acquirejob`, `renewjob`, `completejob`). It checks no credential: it
//! listens on the loopback only, for tests. The fakes of GitHub stay in
//! podrunner (PodUtils/podcrates@T-106).

pub mod files;
pub mod message;

use std::collections::VecDeque;
use std::io::BufReader;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use podrunner_hook::http::{read_request, write_response};
use serde_json::{Value, json};

pub use message::Job;

/// The end of a job, as the runner reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub request: u64,
    /// `succeeded`, `failed`, `cancelled`.
    pub conclusion: String,
    /// The names of the steps that the runner reports.
    pub steps: Vec<String>,
}

#[derive(Default)]
struct State {
    queue: VecDeque<(u64, Job)>,
    acquired: Vec<(u64, Job)>,
    done: Vec<Completion>,
    /// Each request line, for a test that looks at the routes.
    seen: Vec<String>,
    next: u64,
}

/// A fake broker on the loopback.
pub struct Broker {
    pub base: String,
    state: Arc<Mutex<State>>,
}

/// How long a long poll of `/message` waits for a job.
const POLL: Duration = Duration::from_secs(2);

impl Broker {
    /// Listen on a free port of the loopback.
    pub fn start() -> std::io::Result<Broker> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let base = format!("http://{}/", listener.local_addr()?);
        let state: Arc<Mutex<State>> = Arc::default();
        let (s, b) = (state.clone(), base.clone());
        std::thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                let (s, b) = (s.clone(), b.clone());
                std::thread::spawn(move || serve(conn, &s, &b));
            }
        });
        Ok(Broker { base, state })
    }

    /// Offer `job` to the next long poll; its request id.
    pub fn queue(&self, job: Job) -> u64 {
        let mut st = self.state.lock().expect("the broker");
        st.next += 1;
        let id = st.next;
        st.queue.push_back((id, job));
        id
    }

    /// The jobs that ended.
    pub fn completions(&self) -> Vec<Completion> {
        self.state.lock().expect("the broker").done.clone()
    }

    /// The request line of each call.
    pub fn seen(&self) -> Vec<String> {
        self.state.lock().expect("the broker").seen.clone()
    }
}

fn reply(json: &Value) -> (u16, Vec<u8>) {
    (200, json.to_string().into_bytes())
}

fn serve(conn: TcpStream, state: &Mutex<State>, base: &str) {
    let Ok(mut w) = conn.try_clone() else { return };
    let Ok(req) = read_request(&mut BufReader::new(conn)) else { return };
    let path = req.path.split('?').next().unwrap_or("").to_string();
    state.lock().expect("the broker").seen.push(format!("{} {path}", req.method));
    let body: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
    let (status, out) = match (req.method.as_str(), path.as_str()) {
        ("POST", "/_apis/oauth2/token") => {
            reply(&json!({ "access_token": message::job_token(), "token_type": "bearer", "expires_in": 3600 }))
        }
        ("POST", "/session") => {
            let mut session = body;
            session["sessionId"] = json!("11111111-1111-1111-1111-111111111111");
            reply(&session)
        }
        ("DELETE", "/session") => (200, Vec::new()),
        ("GET", "/message") => {
            let end = Instant::now() + POLL;
            loop {
                let next = state.lock().expect("the broker").queue.pop_front();
                if let Some((id, job)) = next {
                    state.lock().expect("the broker").acquired.push((id, job));
                    break reply(&message::job_request(base, id));
                }
                if Instant::now() >= end {
                    break (202, Vec::new());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        ("POST", "/acquirejob") => {
            let id: u64 = body["jobMessageId"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0);
            let st = state.lock().expect("the broker");
            match st.acquired.iter().find(|(i, _)| *i == id).or(st.acquired.last()) {
                Some((i, job)) => reply(&message::job_message(base, *i, job)),
                None => (404, Vec::new()),
            }
        }
        ("POST", "/renewjob") => reply(&json!({ "lockedUntil": "2099-01-01T00:00:00Z" })),
        ("POST", "/completejob") => {
            let steps = body["stepResults"]
                .as_array()
                .map(|a| a.iter().filter_map(|s| s["name"].as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            let mut st = state.lock().expect("the broker");
            let request = st.acquired.last().map_or(0, |(i, _)| *i);
            let conclusion = body["conclusion"].as_str().unwrap_or("").to_string();
            st.done.push(Completion { request, conclusion, steps });
            (200, Vec::new())
        }
        _ => (404, Vec::new()),
    };
    let headers = [("Content-Type", "application/json".to_string())];
    let _ = write_response(&mut w, status, &headers, &out);
}
