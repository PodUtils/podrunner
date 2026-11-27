//! T-498 (githome's G-22): a new key of a source with no drain.
//! `source restart` keeps the running job of the source; a source that
//! revoked podrunner gets no job, does not start again in a loop, and
//! starts again when its key file changes.

use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod scratch;

const BIN: &str = env!("CARGO_BIN_EXE_podrunner");

struct Bed {
    dir: scratch::Scratch,
}

impl Bed {
    /// A source with a key file and the lines of `script`, and a fake unit
    /// that runs `sleep SECS`. A program source needs a tier above `local`,
    /// which the fake unit does not serve: the fake source takes its place,
    /// built again by the same path.
    fn new(name: &str, script: &str, secs: u32) -> Bed {
        let dir = scratch::Scratch::short(&format!("restart-{name}"));
        std::fs::write(dir.join("script"), script).unwrap();
        // A made-up key: the fake source does not read it.
        std::fs::write(dir.join("key"), "made-up-key-1\n").unwrap();
        std::fs::set_permissions(dir.join("key"), std::fs::Permissions::from_mode(0o600)).unwrap();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() - 100;
        let config = format!(
            "host = \"host-a\"\nallow_fake = true\n[[source]]\nname = \"local\"\nkind = \"fake\"\ntier = \"local\"\n\
             arch = \"{arch}\"\nscript = \"{script}\"\nkey_file = \"{key}\"\n[unit.fake]\nslots = 1\ncommand = [\"sleep\", \"{secs}\"]\n\
             [trust]\norganizations = [{{ login = \"example-org\" }}]\n[pin]\n[[pin.payload]]\narch = \"{arch}\"\n\
             reference = \"runner@sha256:{digest}\"\nbuilt_at_s = {now}\n",
            arch = std::env::consts::ARCH,
            script = dir.join("script").display(),
            key = dir.join("key").display(),
            digest = "1".repeat(64),
        );
        std::fs::write(dir.join("podrunner.toml"), config).unwrap();
        Bed { dir }
    }

    fn state(&self) -> PathBuf {
        self.dir.join("state")
    }

    fn serve(&self) -> Child {
        let mut c = Command::new(BIN);
        c.args(["serve", "--config"]).arg(self.dir.join("podrunner.toml")).arg("--state").arg(self.state());
        c.stdout(Stdio::null()).stderr(Stdio::piped()).env_remove("NOTIFY_SOCKET").spawn().unwrap()
    }

    fn cli(&self, args: &[&str]) -> std::process::Output {
        Command::new(BIN).args(args).arg("--state").arg(self.state()).output().unwrap()
    }

    fn status(&self) -> serde_json::Value {
        serde_json::from_slice(&self.cli(&["status", "--json"]).stdout).unwrap_or_default()
    }

    fn journal(&self) -> String {
        std::fs::read_to_string(self.state().join("journal.jsonl")).unwrap_or_default()
    }

    fn wait(&self, what: &str, limit: Duration, f: impl Fn(&Bed) -> bool) {
        let t = Instant::now();
        while !f(self) {
            assert!(t.elapsed() < limit, "{what}: {}", self.journal());
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Stop `serve` and give its stderr.
    fn stop(&self, mut serve: Child) -> String {
        // SAFETY: kill(2) sends SIGTERM to the child that this test started.
        unsafe { libc::kill(serve.id() as libc::pid_t, libc::SIGTERM) };
        let t = Instant::now();
        while serve.try_wait().unwrap().is_none() {
            assert!(t.elapsed() < Duration::from_secs(30), "serve did not end");
            std::thread::sleep(Duration::from_millis(50));
        }
        let mut err = String::new();
        serve.stderr.take().unwrap().read_to_string(&mut err).unwrap();
        err
    }
}

impl Drop for Bed {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn offer(job: &str) -> String {
    format!(
        r#"{{"at_s":0,"offer":{{"schema":1,"job_id":"{job}","source":"local","labels":[],"arch":"{}","wait_deadline_s":60,"trust_inputs":{{"event":"push","head_is_fork":"no","actor_class":"member","repo_visibility":"private","owner":"example-org","repo":"app","run_id":1,"ref":"r","workflow_ref":"w"}}}}}}"#,
        std::env::consts::ARCH
    )
}

fn revoked(s: &serde_json::Value) -> Option<bool> {
    s["sources"].as_array()?.iter().find(|x| x["name"] == "local")?["revoked"].as_bool()
}

#[test]
fn source_restart_keeps_the_running_job() {
    let bed = Bed::new("keep", &format!("{}\n", offer("j-1")), 6);
    let serve = bed.serve();
    bed.wait("no start", Duration::from_secs(20), |b| b.journal().contains("\"started\""));
    std::fs::write(bed.dir.join("key"), "made-up-key-2\n").unwrap();
    let out = bed.cli(&["source", "restart", "--name", "local"]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "\"restarted\"");
    let none = bed.cli(&["source", "restart", "--name", "other"]);
    assert_eq!(none.status.code(), Some(77), "{}", String::from_utf8_lossy(&none.stderr));
    // The job of before the restart ends in its unit and is reported.
    bed.wait("no report", Duration::from_secs(30), |b| b.journal().contains("\"reported\""));
    let journal = bed.journal();
    assert!(!journal.contains("\"lost\""), "{journal}");
    let err = bed.stop(serve);
    assert!(!err.contains("made-up-key"), "{err}");
}

#[test]
fn a_revoked_source_waits_for_a_new_key_file() {
    let script =
        format!("{}\n{{\"at_s\":0,\"mint_error\":{{\"job_id\":\"j-1\",\"code\":\"source-revoked\"}}}}\n", offer("j-1"));
    let bed = Bed::new("revoked", &script, 1);
    let serve = bed.serve();
    bed.wait("not revoked", Duration::from_secs(20), |b| revoked(&b.status()) == Some(true));
    // No start again in a loop while the key file is the same.
    std::thread::sleep(Duration::from_secs(3));
    assert_eq!(revoked(&bed.status()), Some(true));
    // A new key: the source starts again by itself.
    std::fs::write(bed.dir.join("key"), "made-up-key-2\n").unwrap();
    bed.wait("not started again", Duration::from_secs(20), |b| revoked(&b.status()) == Some(false));
    let err = bed.stop(serve);
    assert_eq!(err.matches("the source started again").count(), 1, "{err}");
}
