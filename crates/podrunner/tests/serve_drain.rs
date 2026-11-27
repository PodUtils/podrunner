//! T-501 (githome's G-26): `drain --source NAME` stops the offers of one
//! source, the other sources go on, and `resume --source NAME` takes its
//! offers again; `status --json` names the state of each source. `stop
//! --source NAME` also ends the source when its last job ends.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod scratch;

const BIN: &str = env!("CARGO_BIN_EXE_podrunner");

fn offer(job: &str, source: &str) -> String {
    format!(
        r#"{{"at_s":4,"offer":{{"schema":1,"job_id":"{job}","source":"{source}","labels":[],"arch":"{}","wait_deadline_s":120,"trust_inputs":{{"event":"push","head_is_fork":"no","actor_class":"member","repo_visibility":"private","owner":"example-org","repo":"app","run_id":1,"ref":"r","workflow_ref":"w"}}}}}}"#,
        std::env::consts::ARCH
    )
}

#[test]
fn a_drained_source_takes_no_offer_and_the_others_go_on() {
    let dir = scratch::Scratch::short("serve-drain");
    std::fs::write(dir.join("a"), format!("{}\n", offer("j-a", "a"))).unwrap();
    std::fs::write(dir.join("b"), format!("{}\n", offer("j-b", "b"))).unwrap();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() - 100;
    let source = |n: &str| {
        format!(
            "[[source]]\nname = \"{n}\"\nkind = \"fake\"\ntier = \"local\"\narch = \"{}\"\nscript = \"{}\"\n",
            std::env::consts::ARCH,
            dir.join(n).display()
        )
    };
    let config = format!(
        "host = \"host-a\"\nallow_fake = true\n{}{}[unit.fake]\nslots = 2\ncommand = [\"sleep\", \"1\"]\n[pin]\n\
         [[pin.payload]]\narch = \"{arch}\"\nreference = \"runner@sha256:{digest}\"\nbuilt_at_s = {now}\n",
        source("a"),
        source("b"),
        arch = std::env::consts::ARCH,
        digest = "1".repeat(64),
    );
    std::fs::write(dir.join("podrunner.toml"), config).unwrap();
    let state = dir.join("state");
    let mut serve = Command::new(BIN)
        .args(["serve", "--config"])
        .arg(dir.join("podrunner.toml"))
        .arg("--state")
        .arg(&state)
        .env_remove("NOTIFY_SOCKET")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let cli = |args: &[&str]| Command::new(BIN).args(args).arg("--state").arg(&state).output().unwrap();
    let t = Instant::now();
    // Drain `a` before its offer comes (4 s after the start).
    loop {
        let o = cli(&["drain", "--source", "a"]);
        if o.status.success() {
            break;
        }
        assert!(t.elapsed() < Duration::from_secs(3), "serve did not answer in time");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(cli(&["drain", "--source", "none"]).status.code(), Some(77));
    let journal = || std::fs::read_to_string(state.join("journal.jsonl")).unwrap_or_default();
    let reported = |job: &str| journal().lines().any(|l| l.contains(job) && l.contains("\"reported\""));
    while !reported("j-b") {
        assert!(t.elapsed() < Duration::from_secs(30), "b did not run: {}", journal());
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_secs(2));
    assert!(!journal().contains("j-a"), "a drained source took an offer: {}", journal());
    let v: serde_json::Value = serde_json::from_slice(&cli(&["status", "--json"]).stdout).unwrap();
    let draining =
        |n: &str| v["sources"].as_array().unwrap().iter().find(|s| s["name"] == n).unwrap()["draining"].clone();
    assert_eq!((draining("a"), draining("b")), (serde_json::json!(true), serde_json::json!(false)), "{v}");
    assert!(cli(&["resume", "--source", "a"]).status.success());
    while !reported("j-a") {
        assert!(t.elapsed() < Duration::from_secs(60), "a did not run after resume: {}", journal());
        std::thread::sleep(Duration::from_millis(100));
    }
    // SAFETY: kill(2) sends SIGTERM to the child that this test started.
    unsafe { libc::kill(serve.id() as libc::pid_t, libc::SIGTERM) };
    let _ = serve.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

/// `stop --source NAME`: the source ends when it has no job, `serve` says
/// so, the other source goes on, and `resume --source` builds it again.
#[test]
fn a_stopped_source_ends_and_resume_builds_it_again() {
    let dir = scratch::Scratch::short("serve-stop");
    std::fs::write(dir.join("a"), format!("{}\n", offer("j-a", "a"))).unwrap();
    std::fs::write(dir.join("b"), format!("{}\n", offer("j-b", "b"))).unwrap();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() - 100;
    let source = |n: &str| {
        format!(
            "[[source]]\nname = \"{n}\"\nkind = \"fake\"\ntier = \"local\"\narch = \"{}\"\nscript = \"{}\"\n",
            std::env::consts::ARCH,
            dir.join(n).display()
        )
    };
    std::fs::write(
        dir.join("podrunner.toml"),
        format!(
            "host = \"host-a\"\nallow_fake = true\n{}{}[unit.fake]\nslots = 2\ncommand = [\"sleep\", \"1\"]\n[pin]\n\
             [[pin.payload]]\narch = \"{arch}\"\nreference = \"runner@sha256:{digest}\"\nbuilt_at_s = {now}\n",
            source("a"),
            source("b"),
            arch = std::env::consts::ARCH,
            digest = "1".repeat(64),
        ),
    )
    .unwrap();
    let state = dir.join("state");
    let err = dir.join("serve.err");
    let mut serve = Command::new(BIN)
        .args(["serve", "--config"])
        .arg(dir.join("podrunner.toml"))
        .arg("--state")
        .arg(&state)
        .env_remove("NOTIFY_SOCKET")
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&err).unwrap())
        .spawn()
        .unwrap();
    let cli = |args: &[&str]| Command::new(BIN).args(args).arg("--state").arg(&state).output().unwrap();
    let t = Instant::now();
    loop {
        if cli(&["stop", "--source", "a"]).status.success() {
            break;
        }
        assert!(t.elapsed() < Duration::from_secs(3), "serve did not answer in time");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(cli(&["stop", "--source", "none"]).status.code(), Some(77));
    let said = || std::fs::read_to_string(&err).unwrap_or_default();
    while !said().contains("source a: stopped") {
        assert!(t.elapsed() < Duration::from_secs(10), "no line of the stop: {}", said());
        std::thread::sleep(Duration::from_millis(100));
    }
    let journal = || std::fs::read_to_string(state.join("journal.jsonl")).unwrap_or_default();
    let reported = |job: &str| journal().lines().any(|l| l.contains(job) && l.contains("\"reported\""));
    while !reported("j-b") {
        assert!(t.elapsed() < Duration::from_secs(30), "b did not run: {}", journal());
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_secs(1));
    assert!(!journal().contains("j-a"), "a stopped source took an offer: {}", journal());
    let o = cli(&["resume", "--source", "a"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    while !reported("j-a") {
        assert!(t.elapsed() < Duration::from_secs(60), "a did not run after resume: {}", journal());
        std::thread::sleep(Duration::from_millis(100));
    }
    // SAFETY: kill(2) sends SIGTERM to the child that this test started.
    unsafe { libc::kill(serve.id() as libc::pid_t, libc::SIGTERM) };
    let _ = serve.wait();
    let _ = std::fs::remove_dir_all(&dir);
}
