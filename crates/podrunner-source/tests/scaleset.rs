//! The source conformance suite against the scale-set sidecar
//! (`tools/scaleset-sidecar`) on the fake scale-set service
//! (`tools/fake-scaleset`). Ignored by default: it needs both Go binaries.
//!
//!   PODRUNNER_FAKE_SCALESET=PATH PODRUNNER_SCALESET_SIDECAR=PATH \
//!   cargo test -p podrunner-source --test scaleset -- --ignored

use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use podrunner_source::conformance::{self, Fixture};
use podrunner_source::{JobSource, WireSource};

const SCRIPT: &str = r#"{"scaleSetName":"podrunner","jobs":[
 {"requestId":11,"jobId":"a","owner":"example-org","repo":"app","workflowRunId":101,"event":"push","actor":"alice","actorIsMember":true,"visibility":"private","labels":["podrunner"]},
 {"requestId":12,"jobId":"b","owner":"example-org","repo":"app","workflowRunId":102,"event":"pull_request","actor":"mallory","headIsFork":true,"visibility":"public","labels":["podrunner"]},
 {"requestId":13,"jobId":"c","owner":"example-org","repo":"app","workflowRunId":103,"event":"push","actor":"alice","actorIsMember":true,"visibility":"private","labels":["podrunner"]}]}"#;

/// One fake service and one sidecar; the drop kills both.
struct Pair {
    fake: Child,
    sidecar: Child,
    /// The base URL of the fake service.
    url: String,
}

impl Drop for Pair {
    fn drop(&mut self) {
        let _ = self.sidecar.kill();
        let _ = self.sidecar.wait();
        let _ = self.fake.kill();
        let _ = self.fake.wait();
    }
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

/// A sidecar on a fresh fake, its stdio, and a sign that answers each
/// request with a made-up token.
fn pair() -> (Pair, ChildStdin, ChildStdout) {
    let dir = std::env::temp_dir().join(format!("podrunner-scaleset-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("script.json");
    std::fs::write(&script, SCRIPT).unwrap();
    let mut fake = Command::new(env("PODRUNNER_FAKE_SCALESET"))
        .args(["-listen", "127.0.0.1:0", "-script"])
        .arg(&script)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut url = String::new();
    BufReader::new(fake.stdout.take().unwrap()).read_line(&mut url).unwrap();
    let url = url.trim().to_string();
    let (ours, theirs) = UnixStream::pair().unwrap();
    std::thread::spawn(move || {
        let mut w = ours.try_clone().unwrap();
        for line in BufReader::new(ours).lines() {
            if line.is_err() {
                return;
            }
            let _ = w.write_all(b"{\"token\":\"a.b.c\"}\n");
        }
    });
    let fd = theirs.as_raw_fd();
    let mut cmd = Command::new(env("PODRUNNER_SCALESET_SIDECAR"));
    cmd.args(["-config-url", &format!("{url}/example-org"), "-api-url", &format!("{url}/api/v3")])
        .args(["-installation-id", "7", "-scale-set", "podrunner", "-source", "org-trusted", "-operator-controlled"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    // SAFETY: dup2 in the child before exec, with a descriptor that is open.
    unsafe {
        cmd.pre_exec(move || if libc::dup2(fd, 3) < 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) });
    }
    let mut sidecar = cmd.spawn().unwrap();
    drop(theirs);
    let (stdin, stdout) = (sidecar.stdin.take().unwrap(), sidecar.stdout.take().unwrap());
    (Pair { fake, sidecar, url }, stdin, stdout)
}

/// A JobSource that keeps its processes alive while it lives.
struct Live {
    wire: WireSource<BufReader<ChildStdout>, ChildStdin>,
    _pair: Pair,
}

macro_rules! forward {
    ($($name:ident($($a:ident: $t:ty),*) -> $r:ty;)*) => {
        $(fn $name(&mut self, $($a: $t),*) -> $r { self.wire.$name($($a),*) })*
    };
}

impl JobSource for Live {
    forward! {
        hello(c: &str, v: &str) -> Result<podrunner_source::Hello, podrunner_source::SourceError>;
        next(c: u32, w: std::time::Duration) -> Result<Vec<podrunner_source::Event>, podrunner_source::SourceError>;
        ack(j: &podrunner_schema::JobId) -> Result<(), podrunner_source::SourceError>;
        claim(j: &podrunner_schema::JobId, r: &str) -> Result<(), podrunner_source::SourceError>;
        mint(j: &podrunner_schema::JobId, r: &str) -> Result<podrunner_schema::Credential, podrunner_source::SourceError>;
        report(j: &podrunner_schema::JobId, m: &serde_json::Map<String, serde_json::Value>) -> Result<(), podrunner_source::SourceError>;
        audit(m: &serde_json::Map<String, serde_json::Value>) -> Result<(), podrunner_source::SourceError>;
        withdraw(j: &podrunner_schema::JobId, r: &str) -> Result<(), podrunner_source::SourceError>;
        release(j: &podrunner_schema::JobId) -> Result<(), podrunner_source::SourceError>;
    }
}

struct Fx;

impl Fixture for Fx {
    fn fresh(&self) -> Box<dyn JobSource> {
        let (p, stdin, stdout) = pair();
        Box::new(Live { wire: WireSource::new(BufReader::new(stdout), stdin), _pair: p })
    }

    fn exchange(&self, line: &[u8]) -> String {
        let (_p, mut stdin, stdout) = pair();
        stdin.write_all(line).unwrap();
        stdin.write_all(b"\n").unwrap();
        let mut reply = String::new();
        BufReader::new(stdout).take(1 << 21).read_line(&mut reply).unwrap();
        reply
    }
}

#[test]
#[ignore]
fn the_sidecar_passes_the_source_tests_on_the_fake() {
    let results = conformance::run(&Fx, &[]);
    for r in &results {
        println!("{r}");
    }
    assert!(conformance::not_passed(&results).is_empty());
}

/// A plain GET of the fake service.
fn get(url: &str, path: &str) -> String {
    let address = url.trim_start_matches("http://");
    let mut tcp = std::net::TcpStream::connect(address).unwrap();
    write!(tcp, "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n").unwrap();
    let mut text = String::new();
    tcp.read_to_string(&mut text).unwrap();
    text
}

/// A release of a minted job removes its runner at the forge: a runner of a
/// revoked host never takes a job (GitHub #31).
#[test]
#[ignore]
fn a_release_removes_the_runner_of_a_minted_job() {
    let (p, stdin, stdout) = pair();
    let mut s = WireSource::new(BufReader::new(stdout), stdin);
    s.hello("podrunner", "0").unwrap();
    let mut offer = None;
    for _ in 0..50 {
        let events = s.next(1, std::time::Duration::from_secs(1)).unwrap();
        if let Some(podrunner_source::Event::Offer { offer: o }) = events.into_iter().next() {
            offer = Some(o.job_id);
            break;
        }
    }
    let job = offer.expect("an offer");
    s.ack(&job).unwrap();
    s.claim(&job, "runner-u-1").unwrap();
    s.mint(&job, "runner-u-1").unwrap();
    assert!(!get(&p.url, "/fake/removed").contains("[1]"), "a mint removes no runner");
    s.release(&job).unwrap();
    assert!(get(&p.url, "/fake/removed").contains(r#""removed":[1]"#), "{}", get(&p.url, "/fake/removed"));
}
