//! T-511 (githome's G-40): `serve` drives a program source of a forge tier
//! with the fake kind that claims `confined`, so that a forge tests its
//! source against the released binary with no unit and no build.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod scratch;
use scratch::SpawnKilled;

const BIN: &str = env!("CARGO_BIN_EXE_podrunner");

fn config(dir: &std::path::Path, claim: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() - 100;
    format!(
        "host = \"host-a\"\nallow_fake = true\n[[source]]\nname = \"githome\"\nkind = \"program\"\ntier = \"trusted\"\n\
         arch = \"{arch}\"\nprogram = \"{BIN}\"\nprogram_args = [\"fake-source\", \"--script\", \"{script}\"]\n\
         [unit.fake]\nslots = 1\ncommand = [\"sleep\", \"1\"]\n{claim}\n\
         [trust]\norganizations = [{{ login = \"example-org\" }}]\n[pin]\n[[pin.payload]]\narch = \"{arch}\"\n\
         reference = \"runner@sha256:{digest}\"\nbuilt_at_s = {now}\n",
        arch = std::env::consts::ARCH,
        script = dir.join("script").display(),
        digest = "1".repeat(64),
    )
}

#[test]
fn serve_runs_a_job_of_a_trusted_program_source_on_a_fake_that_claims_confined() {
    let dir = scratch::Scratch::short("serve-program");
    let offer = format!(
        r#"{{"at_s":0,"offer":{{"schema":1,"job_id":"j-1","source":"githome","labels":[],"arch":"{}","wait_deadline_s":60,"trust_inputs":{{"event":"push","head_is_fork":"no","actor_class":"member","repo_visibility":"private","owner":"example-org","repo":"app","run_id":1,"ref":"r","workflow_ref":"w"}}}}}}"#,
        std::env::consts::ARCH
    );
    std::fs::write(dir.join("script"), format!("{offer}\n")).unwrap();
    // With no claim, check refuses the file: no kind serves `trusted`.
    std::fs::write(dir.join("no-claim.toml"), config(&dir, "")).unwrap();
    let o = Command::new(BIN).args(["check", "--config"]).arg(dir.join("no-claim.toml")).output().unwrap();
    assert_eq!(o.status.code(), Some(78), "{}", String::from_utf8_lossy(&o.stderr));
    // A claim of another kind, or of a boundary that is not confined or guest.
    std::fs::write(dir.join("bad.toml"), config(&dir, "claim = \"none\"")).unwrap();
    let o = Command::new(BIN).args(["check", "--config"]).arg(dir.join("bad.toml")).output().unwrap();
    assert!(String::from_utf8_lossy(&o.stderr).contains("confined or guest"), "{o:?}");
    std::fs::write(dir.join("podrunner.toml"), config(&dir, "claim = \"confined\"")).unwrap();
    let state = dir.join("state");
    let mut serve = Command::new(BIN)
        .args(["serve", "--config"])
        .arg(dir.join("podrunner.toml"))
        .arg("--state")
        .arg(&state)
        .env_remove("NOTIFY_SOCKET")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn_killed();
    let journal = || std::fs::read_to_string(state.join("journal.jsonl")).unwrap_or_default();
    let t = Instant::now();
    while !journal().contains("\"reported\"") {
        assert!(t.elapsed() < Duration::from_secs(30), "no report: {}", journal());
        assert!(serve.try_wait().unwrap().is_none(), "serve ended");
        std::thread::sleep(Duration::from_millis(100));
    }
    // SAFETY: kill(2) sends SIGTERM to the child that this test started.
    unsafe { libc::kill(serve.id() as libc::pid_t, libc::SIGTERM) };
    let _ = serve.wait();
    let mut err = String::new();
    serve.stderr.take().unwrap().read_to_string(&mut err).unwrap();
    assert!(err.contains("[unit.fake] claims Confined"), "{err}");
    assert!(journal().contains("\"started\""), "{}", journal());
    let _ = std::fs::remove_dir_all(&dir);
}
