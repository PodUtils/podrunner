//! `podrunner mint-serve` as a process on the loopback (GitHub #31): the
//! mint holds the fake source, and this test is the unit host over mutual
//! TLS. A fake unit meets no floor of a forge tier, so `serve` with a
//! remote source runs no job of the fake mint. Design source test 3 for the
//! mint role: the credential that the mint gives (a positive control: it
//! holds the canary) is in no stderr and no file of the state of the mint.
//! Needs openssl; with no openssl the test passes with no check.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use pod_tls::mutual::{Pin, Pins, client_config};
use pod_tls::server::certified;
use podrunner_remote::transport::{ALPN, Https, provider};
use podrunner_schema::JobId;
use podrunner_source::{Event, JobSource, Wire};

const BIN: &str = env!("CARGO_BIN_EXE_podrunner");
/// The canned credential of the fake source holds this client id.
const CANARY: &str = "fake-client";

/// A P-256 key and its certificate (mode 0600), from openssl; its pin.
fn key(at: &Path, name: &str) -> Option<(PathBuf, PathBuf, Pin)> {
    let (k, c) = (at.join(format!("{name}.key")), at.join(format!("{name}.pem")));
    let ok = Command::new("openssl")
        .args(["req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1", "-nodes", "-days", "1"])
        .args(["-subj", &format!("/CN={name}"), "-keyout"])
        .arg(&k)
        .arg("-out")
        .arg(&c)
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        return None;
    }
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&k, std::fs::Permissions::from_mode(0o600)).ok()?;
    let der = pod_tls::server::certificates(&std::fs::read(&c).ok()?).ok()?.remove(0);
    Some((k, c, Pin::of_certificate(&der).ok()?))
}

/// Each file under `dir` that holds `text`.
fn files_holding(dir: &Path, text: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            match p.symlink_metadata() {
                Ok(m) if m.is_dir() => todo.push(p),
                Ok(m)
                    if m.is_file()
                        && std::fs::read(&p).is_ok_and(|b| b.windows(text.len()).any(|w| w == text.as_bytes())) =>
                {
                    out.push(p)
                }
                _ => {}
            }
        }
    }
    out
}

/// SIGTERM, then the stderr of the process.
fn end(mut c: Child) -> String {
    // SAFETY: kill(2) sends SIGTERM to a child that this test started.
    unsafe { libc::kill(c.id() as libc::pid_t, libc::SIGTERM) };
    let t = Instant::now();
    while c.try_wait().unwrap().is_none() {
        if t.elapsed() > Duration::from_secs(20) {
            let _ = c.kill();
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut s = String::new();
    std::io::Read::read_to_string(&mut c.stderr.take().unwrap(), &mut s).unwrap();
    s
}

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_mint_process_gives_a_credential_and_shows_it_nowhere() {
    let dir = Dir(std::env::temp_dir().join(format!("podrunner-mint-{}", std::process::id())));
    let d = &dir.0;
    let _ = std::fs::remove_dir_all(d);
    std::fs::create_dir_all(d).unwrap();
    let (Some(mint), Some(host)) = (key(d, "mint"), key(d, "host")) else {
        return; // No openssl on this host.
    };
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let arch = std::env::consts::ARCH;
    let offer = format!(
        r#"{{"at_s":0,"offer":{{"schema":1,"job_id":"j-1","source":"local","labels":[],"arch":"{arch}","wait_deadline_s":60,"trust_inputs":{{"event":"push","head_is_fork":"no","actor_class":"member","repo_visibility":"private","owner":"example-org","repo":"app","run_id":1,"ref":"r","workflow_ref":"w"}}}}}}"#
    );
    std::fs::write(d.join("script"), format!("{offer}\n")).unwrap();
    let mint_toml = format!(
        "listen = \"127.0.0.1:{port}\"\ncert_file = \"{}\"\nkey_file = \"{}\"\nstate = \"{}\"\nallow_fake = true\n\
         [source]\nname = \"src\"\nkind = \"fake\"\ntier = \"trusted\"\narch = \"{arch}\"\nscript = \"{}\"\n\
         [trust]\norganizations = [{{ login = \"example-org\" }}]\n[[host]]\nname = \"host-a\"\npin = \"{}\"\nslots = 1\n",
        mint.1.display(),
        mint.0.display(),
        d.join("mint-state").display(),
        d.join("script").display(),
        host.2,
    );
    std::fs::write(d.join("mint.toml"), mint_toml).unwrap();
    let proc = Command::new(BIN)
        .args(["mint-serve", "--config"])
        .arg(d.join("mint.toml"))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let (hk, hc) = (std::fs::read(&host.0).unwrap(), std::fs::read(&host.1).unwrap());
    let config = client_config(provider(), certified(&hc, &hk).unwrap(), Pins::new([mint.2]), &[ALPN]).unwrap();
    let mut wire = Wire(Https { address: format!("127.0.0.1:{port}"), config, lookup: None });
    let t = Instant::now();
    while wire.hello("podrunner", "0").is_err() {
        assert!(t.elapsed() < Duration::from_secs(10), "the mint does not answer: {}", end(proc));
        std::thread::sleep(Duration::from_millis(50));
    }
    let events = wire.next(1, Duration::from_secs(2)).unwrap();
    assert!(matches!(&events[..], [Event::Offer { offer }] if offer.job_id.as_str() == "j-1"), "{events:?}");
    let j = JobId::new("j-1").unwrap();
    wire.ack(&j).unwrap();
    wire.claim(&j, "r").unwrap();
    let credential = wire.mint(&j, "r").unwrap();
    // The positive control: the credential holds the canary.
    assert!(credential.files.iter().any(|f| f.content.expose().contains(CANARY)));
    assert!(!files_holding(&d.join("mint-state"), "j-1").is_empty(), "the scan does not find the ledger line");
    wire.release(&j).unwrap();

    let stderr = end(proc);
    assert!(!stderr.contains(CANARY), "the stderr of the mint holds the credential");
    assert_eq!(files_holding(&d.join("mint-state"), CANARY), Vec::<PathBuf>::new());
}
