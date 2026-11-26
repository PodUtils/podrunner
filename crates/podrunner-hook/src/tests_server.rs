//! The control socket: the HTTP reader, the routes, and the relay script
//! against a live socket.

use std::io::Cursor;
use std::sync::{Arc, Mutex};

use podrunner_schema::RefusalCode;

use crate::http::{HttpError, read_request};
use crate::input::{Command, HookInput};
use crate::policy::Refusal;
use crate::server::{Answer, JobEvent, Unit, answer, bind, respond, serve};

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
        ("POST /x HTTP/2\r\n\r\n", 505),
        ("POST /x HTTP/1.1\r\nno colon\r\n\r\n", 400),
        ("POST /x HTTP/1.1", 400),
        // T-406: no escape sequence of a terminal in a path.
        ("GET /\x1b]0;x\x1b[2J HTTP/1.1\r\n\r\n", 400),
        ("GET /a\tb HTTP/1.1\r\n\r\n", 400),
        ("GET /a\u{7f} HTTP/1.1\r\n\r\n", 400),
        ("GET /a\u{9b}31m HTTP/1.1\r\n\r\n", 400),
    ] {
        assert_eq!(req(text).unwrap_err().0, status, "{text:?}");
    }
    let long = format!("POST /{} HTTP/1.1\r\n\r\n", "a".repeat(17_000));
    assert_eq!(req(&long).unwrap_err().0, 431);
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
    // A request that the host cannot read (a body over 1 MiB) gives the
    // step no byte of the answer, and the script fails: 4 for the HTTP
    // error, or 3 when the host closes before curl sent the whole body.
    let pad = "x".repeat(crate::input::BODY_MAX + 1);
    let o = run(format!(r#"{{"command":"run_script_step","responseFile":null,"args":{{}},"state":"{pad}"}}"#));
    assert!(matches!(o.status.code(), Some(3 | 4)) && o.stdout.is_empty(), "{o:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_socket_path_longer_than_sun_path_binds() {
    // The rootfs of a podvm unit can be deeper than the 108 bytes of
    // `sun_path`.
    let mut dir = std::env::temp_dir().join(format!("podrunner-hook-long-{}", std::process::id()));
    while dir.as_os_str().len() < 140 {
        dir = dir.join("deep-directory-name");
    }
    std::fs::create_dir_all(&dir).unwrap();
    let sock = dir.join("u.ctl");
    let listener = bind(&sock).unwrap();
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};
    let meta = std::fs::metadata(&sock).unwrap();
    assert!(meta.file_type().is_socket());
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    let unit: Arc<Mutex<Recorder>> = Arc::default();
    let shared: Arc<Mutex<dyn Unit>> = unit.clone();
    std::thread::spawn(move || serve(listener, shared));
    use std::io::{Read, Write};
    let mut c = crate::server::connect(&sock).unwrap();
    c.write_all(b"POST /v1/job_state HTTP/1.1\r\nContent-Length: 27\r\n\r\n{\"event\":\"job-started\"}    ").unwrap();
    let mut out = String::new();
    c.read_to_string(&mut out).unwrap();
    assert!(out.starts_with("HTTP/1.1 "), "{out}");
    let _ = std::fs::remove_dir_all(std::env::temp_dir().join(format!("podrunner-hook-long-{}", std::process::id())));
}

/// A unit that gives output live, then refuses: the end of a driver that
/// fails after the step printed.
struct Late;

impl Unit for Late {
    fn hook(&mut self, _: HookInput) -> Result<Answer, Refusal> {
        Ok(Answer { exit: 0, output: b"whole".to_vec() })
    }
    fn hook_live(&mut self, _: HookInput, live: &mut dyn FnMut(&[u8])) -> Result<Answer, Refusal> {
        live(b"one\n");
        live(b"");
        live(b"two\n");
        Err(Refusal { code: RefusalCode::ContainerNotSupported, why: "the driver did not do the container operation" })
    }
    fn job_state(&mut self, _: JobEvent) -> Result<(), Refusal> {
        Ok(())
    }
}

fn live(path: &str, body: &str, unit: &mut dyn Unit) -> String {
    let r = req(&format!("POST {path} HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}", body.len())).unwrap();
    let mut out = Vec::new();
    respond(&r, unit, &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

/// T-694: the live route gives each part of the output as one chunk, an
/// empty part as none, and the exit code or the refusal in the trailer.
#[test]
fn the_live_route_streams_and_ends_with_a_trailer() {
    let step = r#"{"command":"run_script_step","responseFile":null,"args":{"entryPoint":"sh"},"state":null}"#;
    let out = live("/v2/hook/run_script_step", step, &mut Late);
    let (head, body) = out.split_once("\r\n\r\n").unwrap();
    assert!(head.starts_with("HTTP/1.1 200 OK\r\n") && head.contains("Transfer-Encoding: chunked"), "{head}");
    assert!(!head.contains("Content-Length"), "{head}");
    let refusal = r#"X-Podrunner-Refusal: {"code":"container-not-supported","why":"the driver did not do the container operation"}"#;
    assert_eq!(body, format!("4\r\none\n\r\n4\r\ntwo\n\r\n0\r\n{refusal}\r\n\r\n"));
    // A unit with no live output gives its whole answer as one chunk.
    let out = live("/v2/hook/run_script_step", step, &mut Recorder::default());
    assert!(out.ends_with("\r\n\r\nc\r\nstep output\n\r\n0\r\nX-Podrunner-Exit: 3\r\n\r\n"), "{out}");
    // An input that does not parse is a plain 400, before any chunk.
    assert!(live("/v2/hook/run_script_step", "{", &mut Late).starts_with("HTTP/1.1 400 "));
    assert!(live("/v2/hook/build", "{}", &mut Late).starts_with("HTTP/1.1 404 "));
    // The whole answer has no live route.
    let r = req(&format!("POST /v2/hook/run_script_step HTTP/1.1\r\nContent-Length: {}\r\n\r\n{step}", step.len()));
    assert_eq!(answer(&r.unwrap(), &mut Late).0, 400);
}
