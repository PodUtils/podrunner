//! T-642: `status` answers while the first tick of `serve` waits on its
//! source. A program source that answers `hello` and then nothing holds
//! the first tick for the reply limit; `status` takes the snapshot that
//! `serve` posts before its loop, and does not get `busy`.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const BIN: &str = env!("CARGO_BIN_EXE_podrunner");

#[test]
fn status_answers_while_the_first_tick_waits_on_the_source() {
    let dir = std::env::temp_dir().join(format!("podrunner-serve-status-wait-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // The program reads each line and answers the first with `hello`: the
    // next request gets no reply, and the program ends with its stdin.
    let program = dir.join("quiet-source");
    std::fs::write(
        &program,
        "#!/bin/sh\nread line\necho '{\"reply\":\"hello\",\"source\":\"quiet\",\"operator_controlled\":false}'\n\
         while read line; do :; done\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() - 100;
    std::fs::write(
        dir.join("podrunner.toml"),
        format!(
            "host = \"host-a\"\nallow_fake = true\ndriver_dir = \"{dir}\"\n[[source]]\nname = \"quiet\"\n\
             kind = \"program\"\ntier = \"trusted\"\narch = \"{arch}\"\nprogram = \"{program}\"\n\
             [unit.fake]\nslots = 1\ncommand = [\"sleep\", \"1\"]\nclaim = \"confined\"\n\
             [trust]\norganizations = [{{ login = \"example-org\" }}]\n[pin]\n[[pin.payload]]\narch = \"{arch}\"\n\
             reference = \"runner@sha256:{digest}\"\nbuilt_at_s = {now}\n",
            dir = dir.display(),
            program = program.display(),
            arch = std::env::consts::ARCH,
            digest = "1".repeat(64),
        ),
    )
    .unwrap();
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
    let t = Instant::now();
    while !state.join("ctl.sock").exists() {
        assert!(t.elapsed() < Duration::from_secs(10), "no control socket");
        assert!(serve.try_wait().unwrap().is_none(), "serve ended");
        std::thread::sleep(Duration::from_millis(20));
    }
    let o = Command::new(BIN).args(["status", "--json", "--state"]).arg(&state).output().unwrap();
    let text = String::from_utf8_lossy(&o.stderr).into_owned();
    let _ = serve.kill();
    let _ = serve.wait();
    assert!(o.status.success(), "{text}");
    assert!(!text.contains("did not answer"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}
