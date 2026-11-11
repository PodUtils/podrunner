//! `podrunner serve` as a process, with the fake source and the fake unit:
//! the lock of the state directory (GitHub #54), the drain on SIGTERM and
//! the sd_notify datagrams (GitHub #60), and `stop --wait`.

use std::os::unix::net::UnixDatagram;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const BIN: &str = env!("CARGO_BIN_EXE_podrunner");

struct Bed {
    dir: PathBuf,
}

impl Bed {
    /// A configuration with one fake job whose unit runs `sleep SECS`.
    fn new(name: &str, secs: u32) -> Bed {
        let dir = std::env::temp_dir().join(format!("podrunner-serve-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let offer = r#"{"at_s":0,"offer":{"schema":1,"job_id":"j-1","source":"local","labels":[],"arch":"x86_64","wait_deadline_s":60,"trust_inputs":{"event":"push","head_is_fork":"no","actor_class":"member","repo_visibility":"private","owner":"example-org","repo":"app","run_id":1,"ref":"r","workflow_ref":"w"}}}"#;
        std::fs::write(dir.join("script"), format!("{offer}\n")).unwrap();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() - 100;
        let config = format!(
            "host = \"host-a\"\nallow_fake = true\n[[source]]\nname = \"local\"\nkind = \"fake\"\ntier = \"local\"\n\
             arch = \"{arch}\"\nscript = \"{script}\"\n[unit.fake]\nslots = 1\ncommand = [\"sleep\", \"{secs}\"]\n\
             [trust]\norganizations = [{{ login = \"example-org\" }}]\n[pin]\n[[pin.payload]]\narch = \"{arch}\"\n\
             reference = \"runner@sha256:{digest}\"\nbuilt_at_s = {now}\n",
            arch = std::env::consts::ARCH,
            script = dir.join("script").display(),
            digest = "1".repeat(64),
        );
        std::fs::write(dir.join("podrunner.toml"), config).unwrap();
        Bed { dir }
    }

    fn state(&self) -> PathBuf {
        self.dir.join("state")
    }

    fn serve(&self, notify: Option<&Path>) -> Child {
        let mut c = Command::new(BIN);
        c.args(["serve", "--config"]).arg(self.dir.join("podrunner.toml")).arg("--state").arg(self.state());
        c.stdout(Stdio::null()).stderr(Stdio::piped()).env_remove("NOTIFY_SOCKET");
        if let Some(n) = notify {
            c.env("NOTIFY_SOCKET", n);
        }
        c.spawn().unwrap()
    }

    fn cli(&self, args: &[&str]) -> std::process::Output {
        Command::new(BIN).args(args).arg("--state").arg(self.state()).output().unwrap()
    }

    fn journal(&self) -> String {
        std::fs::read_to_string(self.state().join("journal.jsonl")).unwrap_or_default()
    }

    /// Wait until the journal shows the job running in its unit.
    fn wait_started(&self) {
        let t = Instant::now();
        while !self.journal().contains("\"started\"") {
            assert!(t.elapsed() < Duration::from_secs(20), "no start: {}", self.journal());
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

impl Drop for Bed {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn wait_exit(c: &mut Child, limit: Duration) -> Option<std::process::ExitStatus> {
    let t = Instant::now();
    while t.elapsed() < limit {
        if let Some(s) = c.try_wait().unwrap() {
            return Some(s);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

#[test]
fn a_second_serve_is_refused_before_the_replay() {
    let bed = Bed::new("lock", 30);
    let mut first = bed.serve(None);
    bed.wait_started();
    let before = bed.journal();
    let socket = bed.state().join("ctl.sock");
    use std::os::unix::fs::MetadataExt;
    let inode = std::fs::metadata(&socket).unwrap().ino();

    let second = bed.serve(None).wait_with_output().unwrap();
    assert_eq!(second.status.code(), Some(69), "{}", String::from_utf8_lossy(&second.stderr));
    assert!(String::from_utf8_lossy(&second.stderr).contains("another podrunner serve"));
    // The journal, the socket and the job of the first are untouched.
    assert_eq!(bed.journal(), before);
    assert_eq!(std::fs::metadata(&socket).unwrap().ino(), inode);
    let status = bed.cli(&["status", "--json"]);
    assert!(String::from_utf8_lossy(&status.stdout).contains("\"started\""), "{status:?}");
    assert!(first.try_wait().unwrap().is_none());

    // After a SIGKILL of the holder, the next serve starts at once.
    first.kill().unwrap();
    first.wait().unwrap();
    let mut third = bed.serve(None);
    let t = Instant::now();
    while bed.cli(&["status"]).status.code() != Some(0) {
        assert!(t.elapsed() < Duration::from_secs(10), "the third serve did not start");
        assert!(third.try_wait().unwrap().is_none(), "the third serve ended");
        std::thread::sleep(Duration::from_millis(100));
    }
    third.kill().unwrap();
    let _ = third.wait();
}

#[test]
fn sigterm_drains_and_notify_says_ready_and_stopping() {
    let bed = Bed::new("term", 2);
    let notify = bed.dir.join("notify.sock");
    let rx = UnixDatagram::bind(&notify).unwrap();
    rx.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    let mut serve = bed.serve(Some(&notify));
    let mut buf = [0u8; 64];
    let n = rx.recv(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"READY=1");
    // READY comes after the socket answers.
    assert_eq!(bed.cli(&["status"]).status.code(), Some(0));
    bed.wait_started();
    // SAFETY: kill(2) sends SIGTERM to the child that this test started.
    unsafe { libc::kill(serve.id() as libc::pid_t, libc::SIGTERM) };
    let n = rx.recv(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"STOPPING=1");
    // serve ends only when the job is final, and then exits 0.
    let status = wait_exit(&mut serve, Duration::from_secs(30)).expect("serve did not end");
    assert!(status.success(), "{status:?}");
    let journal = bed.journal();
    assert!(journal.contains("\"reported\""), "{journal}");
    assert!(!journal.contains("\"lost\""), "{journal}");
}

#[test]
fn stop_wait_returns_after_serve_ends_and_times_out_with_75() {
    let bed = Bed::new("wait", 3);
    let mut serve = bed.serve(None);
    bed.wait_started();
    let t = Instant::now();
    let short = bed.cli(&["stop", "--wait", "--timeout", "1"]);
    assert_eq!(short.status.code(), Some(75), "{short:?}");
    let full = bed.cli(&["stop", "--wait"]);
    assert_eq!(full.status.code(), Some(0), "{full:?}");
    // It returned after serve ended, not at the reply.
    assert!(serve.try_wait().unwrap().is_some());
    assert!(t.elapsed() >= Duration::from_secs(1));
    let _ = serve.wait();
}
