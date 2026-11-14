//! The holds of a local run (T-150): a step that `podrunner run --break`
//! puts in a job asks `GET /_podrunner/hold/JOB` each second; the broker
//! answers 503 until the user releases the job, then 200 for each later
//! ask of that job. One table for the process: the brokers of each job
//! and each slot share it, and the run reads it.

use std::collections::BTreeMap;
use std::sync::Mutex;

/// The path of the holds on a broker.
pub const PATH: &str = "/_podrunner/hold/";

/// Each job that asked, and whether it is released.
static HOLDS: Mutex<BTreeMap<String, bool>> = Mutex::new(BTreeMap::new());

/// The jobs that wait for a release.
pub fn waiting() -> Vec<String> {
    HOLDS.lock().expect("the holds").iter().filter(|(_, r)| !**r).map(|(j, _)| j.clone()).collect()
}

/// Release `job`: its step goes on, and each later hold of it too.
pub fn release(job: &str) {
    HOLDS.lock().expect("the holds").insert(job.to_string(), true);
}

/// Release each job (the end of a run, or a signal).
pub fn release_all() {
    for r in HOLDS.lock().expect("the holds").values_mut() {
        *r = true;
    }
}

/// The answer to a route of the holds, or `None` for another route.
pub fn route(method: &str, path: &str) -> Option<(u16, Vec<u8>)> {
    let job = path.split('?').next()?.strip_prefix(PATH)?;
    if method != "GET" || job.is_empty() || job.contains('/') {
        return Some((404, Vec::new()));
    }
    let mut holds = HOLDS.lock().expect("the holds");
    match *holds.entry(job.to_string()).or_insert(false) {
        true => Some((200, b"go\n".to_vec())),
        false => Some((503, b"held\n".to_vec())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hold_answers_503_until_its_release() {
        assert_eq!(route("GET", "/_podrunner/hold/t-150.0").unwrap().0, 503);
        assert!(waiting().contains(&"t-150.0".to_string()));
        release("t-150.0");
        assert_eq!(route("GET", "/_podrunner/hold/t-150.0").unwrap().0, 200);
        assert_eq!(route("GET", "/_podrunner/hold/t-150.0").unwrap().0, 200, "a later hold goes on");
        assert!(!waiting().contains(&"t-150.0".to_string()));
        assert_eq!(route("POST", "/_podrunner/hold/x").unwrap().0, 404);
        assert!(route("GET", "/message").is_none());
    }

    /// A request in the absolute form of a proxy takes the same route.
    #[test]
    fn the_broker_takes_the_absolute_form_of_a_proxy() {
        use std::io::{Read, Write};
        let broker = crate::Broker::start().unwrap();
        let hostport = broker.base.trim_start_matches("http://").trim_end_matches('/').to_string();
        let mut c = std::net::TcpStream::connect(&hostport).unwrap();
        write!(
            c,
            "GET http://broker.podrunner.test/_podrunner/hold/abs.0 HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\n\r\n"
        )
        .unwrap();
        let mut out = String::new();
        c.read_to_string(&mut out).unwrap();
        assert!(out.starts_with("HTTP/1.1 503"), "{out}");
        assert!(broker.seen().iter().any(|s| s == "GET /_podrunner/hold/abs.0 (absolute form)"), "{:?}", broker.seen());
    }
}
