//! The web view of a local run (T-149): `--web ADDR` serves one page on a
//! loopback address only, under a random token in its path. The page shows
//! each job with its state and its live log (T-134), and each artifact of
//! the run (T-119) for download. The page is in the binary: it loads no
//! script from another host. Each log line is masked before it reaches the
//! view, as on stderr. After the run, the view stays until SIGINT.

use std::io::{BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use podrunner_hook::http::{read_request_with, write_response};
use podrunner_local::artifacts::ArtifactStore;
use podrunner_local::drive::Line;
use podrunner_local::exec::Live;
use podrunner_schema::JobId;
use serde_json::{Value, json};

use crate::cli::Out;

/// The page of the view.
const PAGE: &str = include_str!("web.html");

/// The most log text that the view keeps for each job.
const LOG_MAX: usize = 4 << 20;

#[derive(Default)]
struct View {
    /// Each job: its id, its name, its state and its log.
    jobs: Vec<(String, String, String, String)>,
    done: bool,
}

impl View {
    fn job(&mut self, id: &str) -> &mut (String, String, String, String) {
        if let Some(i) = self.jobs.iter().position(|j| j.0 == id) {
            return &mut self.jobs[i];
        }
        self.jobs.push((id.to_string(), id.to_string(), "running".into(), String::new()));
        self.jobs.last_mut().expect("a job")
    }
}

/// The web view of a run.
pub(super) struct Web {
    view: Arc<Mutex<View>>,
    pub url: String,
}

/// A random token for the path: 16 bytes of `/dev/urandom` as hex.
fn token() -> Result<String, String> {
    let mut b = [0u8; 16];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).map_err(|e| format!("--web: {e}"))?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

/// `ADDR` of `--web`: an address of the loopback with a port (0 for any).
pub(super) fn parse_addr(v: &str) -> Result<SocketAddr, String> {
    let addr: SocketAddr = v.parse().map_err(|_| format!("--web {v:?} is not IP:PORT"))?;
    if !addr.ip().is_loopback() {
        return Err(format!("--web {v:?}: the view binds only an address of the loopback"));
    }
    Ok(addr)
}

fn state_json(view: &View, artifacts: &ArtifactStore) -> Value {
    let jobs: Vec<Value> = view
        .jobs
        .iter()
        .map(|(id, name, state, log)| json!({ "id": id, "name": name, "state": state, "log": log }))
        .collect();
    let names: Vec<Value> = artifacts.list().iter().map(|a| a["name"].clone()).collect();
    json!({ "jobs": jobs, "artifacts": names, "done": view.done })
}

/// A path part with each `%XX` decoded (`encodeURIComponent` of the page).
fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = (b[i] == b'%' && i + 2 < b.len()).then(|| std::str::from_utf8(&b[i + 1..i + 3]).ok()).flatten();
        match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
            Some(v) => {
                out.push(v);
                i += 3;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn serve(conn: TcpStream, token: &str, view: &Mutex<View>, artifacts: &ArtifactStore) {
    let Ok(mut w) = conn.try_clone() else { return };
    let Ok(req) = read_request_with(&mut BufReader::new(conn), 64 << 10) else { return };
    let path = req.path.split('?').next().unwrap_or("");
    let Some(rest) = path.strip_prefix('/').and_then(|p| p.strip_prefix(token)) else {
        let _ = write_response(&mut w, 404, &[], b"");
        return;
    };
    let html = ("Content-Type", "text/html; charset=utf-8".to_string());
    let no_cache = ("Cache-Control", "no-store".to_string());
    let _ = match (req.method.as_str(), rest) {
        ("GET", "" | "/") => write_response(&mut w, 200, &[html, no_cache], PAGE.as_bytes()),
        ("GET", "/state.json") => {
            let body = state_json(&view.lock().expect("the view"), artifacts).to_string();
            write_response(&mut w, 200, &[("Content-Type", "application/json".into()), no_cache], body.as_bytes())
        }
        ("GET", p) if p.starts_with("/artifacts/") => {
            let name = decode(&p["/artifacts/".len()..]);
            match artifacts.zip_of(&name) {
                Some(bytes) => {
                    let disposition = format!("attachment; filename=\"{}.zip\"", name.replace('"', ""));
                    write_response(&mut w, 200, &[("Content-Disposition", disposition)], &bytes)
                }
                None => write_response(&mut w, 404, &[], b""),
            }
        }
        _ => write_response(&mut w, 404, &[], b""),
    };
    let _ = w.flush();
}

impl Web {
    /// Bind `addr` and serve the view of a run over `artifacts`.
    pub(super) fn start(addr: SocketAddr, artifacts: Arc<ArtifactStore>) -> Result<Web, String> {
        let listener = TcpListener::bind(addr).map_err(|e| format!("--web {addr}: {e}"))?;
        let local = listener.local_addr().map_err(|e| format!("--web: {e}"))?;
        let token = token()?;
        let url = format!("http://{local}/{token}/");
        let view: Arc<Mutex<View>> = Arc::default();
        let shared = view.clone();
        std::thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                let (token, view, artifacts) = (token.clone(), shared.clone(), artifacts.clone());
                std::thread::spawn(move || serve(conn, &token, &view, &artifacts));
            }
        });
        Ok(Web { view, url })
    }

    /// The live sink of the view, with each secret masked, before `then`.
    pub(super) fn live(&self, secrets: Vec<String>, then: Option<Live>) -> Live {
        let view = self.view.clone();
        Arc::new(move |job: &JobId, text: &str| {
            {
                let mut v = view.lock().expect("the view");
                let log = &mut v.job(job.as_str()).3;
                if log.len() < LOG_MAX {
                    log.push_str(&super::logs::mask(text, &secrets));
                }
            }
            if let Some(t) = &then {
                t(job, text);
            }
        })
    }

    /// The end of a job.
    pub(super) fn line(&self, l: &Line) {
        let mut v = self.view.lock().expect("the view");
        let (id, name, state) = match l {
            Line::Ran { job, name, result } | Line::Kept { job, name, result } => {
                (job.to_string(), name.clone(), result.as_str().to_string())
            }
            Line::Refused { job, name, reason } => (job.to_string(), name.clone(), format!("failure: {reason}")),
            Line::NotSelected { job, name, reason } => (job.to_string(), name.clone(), format!("skipped: {reason}")),
            Line::Skipped { job, condition } => (job.clone(), job.clone(), format!("skipped: if: {condition}")),
        };
        let j = v.job(&id);
        j.1 = name;
        j.2 = state;
    }

    /// The run ended.
    pub(super) fn done(&self) {
        self.view.lock().expect("the view").done = true;
    }
}

/// The end of a run with a web view: print its lines, mark the run as
/// ended, and serve the view until SIGINT or SIGTERM. The code stays.
pub(super) fn hold(web: Option<&Web>, mut out: Out) -> Out {
    let Some(web) = web else { return out };
    print!("{}", out.stdout);
    eprint!("{}", out.stderr);
    out.stdout.clear();
    out.stderr.clear();
    web.done();
    eprintln!("podrunner: the run ended; the web view stays at {} until SIGINT", web.url);
    while !podrunner_local::cancel::requested() {
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(url: &str) -> (u16, String) {
        let rest = url.strip_prefix("http://").unwrap();
        let (host, path) = rest.split_once('/').unwrap();
        let mut c = TcpStream::connect(host).unwrap();
        write!(c, "GET /{path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n").unwrap();
        let mut out = String::new();
        c.read_to_string(&mut out).unwrap();
        let status = out.split(' ').nth(1).unwrap().parse().unwrap();
        (status, out.split_once("\r\n\r\n").map(|x| x.1.to_string()).unwrap_or_default())
    }

    #[test]
    fn the_view_serves_the_jobs_under_its_token_only() {
        assert!(parse_addr("0.0.0.0:8080").unwrap_err().contains("loopback"));
        assert!(parse_addr("localhost").is_err());
        let dir = std::env::temp_dir().join(format!("podrunner-web-{}", std::process::id()));
        let store = Arc::new(ArtifactStore::new(&dir, 1 << 20).unwrap());
        assert!(store.zip_of("../etc").is_none() && store.zip_of(".pending").is_none());
        let web = Web::start(parse_addr("127.0.0.1:0").unwrap(), store).unwrap();
        let live = web.live(vec!["made-up-secret".into()], None);
        let job = JobId::new("build.0").unwrap();
        live(&job, "step one made-up-secret\n");
        web.line(&Line::Ran {
            job: job.clone(),
            name: "build".into(),
            result: pod_actions_workflow::JobResult::Success,
        });
        let (status, page) = get(&web.url);
        assert_eq!(status, 200);
        assert!(page.contains("<html") && !page.contains("https://"), "the page loads nothing from another host");
        let (_, state) = get(&format!("{}state.json", web.url));
        let v: Value = serde_json::from_str(&state).unwrap();
        assert_eq!(v["jobs"][0]["state"], "success");
        assert_eq!(v["jobs"][0]["log"], "step one ***\n", "the secret is masked");
        let wrong = web.url.replacen(&web.url[web.url.len() - 33..web.url.len() - 1], &"0".repeat(32), 1);
        assert_eq!(get(&format!("{wrong}state.json")).0, 404, "another token gets nothing");
        assert_eq!(get(&format!("{}artifacts/none", web.url)).0, 404);
        assert_eq!(decode("a%20b%2Fc%zz"), "a b/c%zz");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
