//! The control socket: the HTTP reader, the routes, and the relay script
//! against a live socket.

use std::io::Cursor;
use std::sync::{Arc, Mutex};

use podrunner_schema::RefusalCode;

use crate::http::{HttpError, read_request};
use crate::input::{Command, HookInput};
use crate::policy::Refusal;
use crate::server::{Answer, JobEvent, Unit, answer, bind, serve};

/// A unit that records each call: prepare_job answers a response file, a
/// step answers its output and exit code 3, and run_container_step is
/// refused.
#[derive(Default)]
struct Recorder {
    calls: Vec<Command>,
    events: Vec<JobEvent>,
}

impl Unit for Recorder {
    fn hook(&mut self, input: HookInput) -> Result<Answer, Refusal> {
        self.calls.push(input.command);
        match input.command {
            Command::PrepareJob => Ok(Answer { exit: 0, output: br#"{"state":{},"isAlpine":false}"#.to_vec() }),
            Command::RunContainerStep => {
                Err(Refusal { code: RefusalCode::BuildNotSupported, why: "a Dockerfile action" })
            }
            _ => Ok(Answer { exit: 3, output: b"step output\n".to_vec() }),
        }
    }
    fn job_state(&mut self, event: JobEvent) -> Result<(), Refusal> {
        self.events.push(event);
        Ok(())
    }
}

fn req(text: &str) -> Result<crate::http::Request, HttpError> {
    read_request(&mut Cursor::new(text.as_bytes().to_vec()))
}

#[test]
fn the_reader_holds_its_limits() {
    let ok = req("POST /v1/job_state HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}").unwrap();
    assert_eq!((ok.method.as_str(), ok.path.as_str(), ok.body.as_slice()), ("POST", "/v1/job_state", &b"{}"[..]));
    for (text, status) in [
        ("POST /x HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n", 501),
        ("POST /x HTTP/1.1\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\n{}", 400),
        ("POST /x HTTP/1.1\r\nContent-Length: 9999999\r\n\r\n", 413),
        ("POST /x HTTP/1.1\r\nContent-Length: 5\r\n\r\n{}", 400),
        ("POST x HTTP/1.1\r\n\r\n", 400),
        ("POST /x HTTP/2\r\n\r\n", 400),
        ("POST /x HTTP/1.1\r\nno colon\r\n\r\n", 400),
        ("POST /x HTTP/1.1", 400),
    ] {
        assert_eq!(req(text).unwrap_err().0, status, "{text:?}");
    }
    let long = format!("POST /{} HTTP/1.1\r\n\r\n", "a".repeat(9000));
    assert_eq!(req(&long).unwrap_err().0, 400);
    let many = format!("POST /x HTTP/1.1\r\n{}\r\n", "A: b\r\n".repeat(70));
    assert_eq!(req(&many).unwrap_err().0, 431);
}

#[test]
fn the_routes_answer() {
    let mut u = Recorder::default();
    let post = |path: &str, body: &str| {
        req(&format!("POST {path} HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}", body.len())).unwrap()
    };
    let (status, _, _) = answer(&post("/v1/job_state", r#"{"event":"job-started"}"#), &mut u);
    assert_eq!((status, u.events.as_slice()), (200, &[JobEvent::JobStarted][..]));
    assert_eq!(answer(&post("/v1/job_state", r#"{"event":"x"}"#), &mut u).0, 400);
    assert_eq!(answer(&post("/v1/hook/build", "{}"), &mut u).0, 404);
    assert_eq!(answer(&post("/v1/hook/cleanup_job", "{"), &mut u).0, 400);
    let get = req("GET /v1/job_state HTTP/1.1\r\n\r\n").unwrap();
    assert_eq!(answer(&get, &mut u).0, 405);
    let body = r#"{"command":"run_container_step","responseFile":null,"args":{"dockerfile":"D"},"state":null}"#;
    let (status, _, out) = answer(&post("/v1/hook/run_container_step", body), &mut u);
    assert_eq!(status, 403);
    assert!(String::from_utf8(out).unwrap().contains("build-not-supported"));
}

/// The relay script, as the runner runs it, against a live socket: the
/// response file of prepare_job, the output and the exit code of a step,
/// and a refusal that fails the script.
#[test]
fn the_relay_reaches_the_socket() {
    if !std::process::Command::new("curl").arg("--version").output().is_ok_and(|o| o.status.success()) {
        return; // No curl on this host; the unit image has one.
    }
    let dir = std::env::temp_dir().join(format!("podrunner-hook-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (sock, relay, resp) = (dir.join("ctl.sock"), dir.join("hook.sh"), dir.join("r.json"));
    std::fs::write(&relay, crate::RELAY).unwrap();
    let listener = bind(&sock).unwrap();
    let unit: Arc<Mutex<Recorder>> = Arc::default();
    let shared: Arc<Mutex<dyn Unit>> = unit.clone();
    std::thread::spawn(move || serve(listener, shared));
    let run = |input: String| {
        let mut child = std::process::Command::new("sh")
            .arg(&relay)
            .env("PODRUNNER_CTL", &sock)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        child.wait_with_output().unwrap()
    };
    let r = resp.display();
    let o = run(format!(
        r#"{{"command":"prepare_job","responseFile":"{r}","args":{{"container":null,"services":[]}},"state":{{}}}}"#
    ));
    assert_eq!(o.status.code(), Some(0), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(std::fs::read_to_string(&resp).unwrap(), r#"{"state":{},"isAlpine":false}"#);
    let o = run(r#"{"command":"run_script_step","responseFile":null,"args":{"entryPoint":"sh"},"state":null}"#.into());
    assert_eq!((o.status.code(), o.stdout.as_slice()), (Some(3), &b"step output\n"[..]));
    let o =
        run(r#"{"command":"run_container_step","responseFile":null,"args":{"dockerfile":"D"},"state":null}"#.into());
    assert_eq!(o.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&o.stderr).contains("build-not-supported"));
    assert_eq!(unit.lock().unwrap().calls, [Command::PrepareJob, Command::RunScriptStep, Command::RunContainerStep]);
    let _ = std::fs::remove_dir_all(&dir);
}
