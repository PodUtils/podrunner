//! The signals of `podrunner run`. T-639: a second SIGINT during the stop
//! is not lost: it prints a line, and the run still ends with 130. T-636:
//! SIGHUP prints the same line as SIGINT, and a third signal leaves no
//! process of the job and no file of the run in `TMPDIR`.
//!
//! Each test waits for an event (a file of the runner, a line of stderr,
//! the end of a process), never for a fixed time.

use std::io::BufRead;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use scratch::Scratch;

mod scratch;

/// The limit of each wait for an event.
const LIMIT: Duration = Duration::from_secs(30);

/// A runner directory in `dir/r` whose runner never reaches its broker: the
/// job runs until the cancel. The runner writes its pid to `dir/started`.
/// With `stubborn`, it ignores SIGTERM, so the stop of its group takes 5 s
/// and a third signal comes while the run still stops; it ends by itself
/// after about 20 s, so an old binary leaves no process for long.
fn runner(dir: &Path, stubborn: bool) {
    std::fs::create_dir_all(dir.join("r/bin")).unwrap();
    let body = match stubborn {
        true => {
            "trap '' TERM\necho $$ > ../started.tmp && mv ../started.tmp ../started\n\
                 i=0; while [ $i -lt 200 ]; do sleep 0.1; i=$((i+1)); done\n"
        }
        false => "echo $$ > ../started.tmp && mv ../started.tmp ../started\nexec sleep 30\n",
    };
    podrunner_unit::program_file::write(dir.join("r/run.sh"), format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::write(dir.join("r/bin/Runner.Listener"), "").unwrap();
    std::fs::write(dir.join("w.yml"), "on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n      - run: sleep 30\n")
        .unwrap();
}

/// Start `podrunner run` in `dir` with its own `TMPDIR` (`dir/tmp`), and a
/// reader of its stderr that sends each line.
fn start(dir: &Path) -> (Child, Receiver<String>, std::thread::JoinHandle<String>) {
    std::fs::create_dir_all(dir.join("tmp")).unwrap();
    let mut run = Command::new(env!("CARGO_BIN_EXE_podrunner"))
        .args(["run", "w.yml", "--runner-dir", "r", "--no-cache"])
        .current_dir(dir)
        .env("TMPDIR", dir.join("tmp"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (tx, rx) = channel();
    let err = run.stderr.take().unwrap();
    // The reader ends at the end of the pipe: when each process that holds
    // it ended.
    let reader = std::thread::spawn(move || {
        let mut all = String::new();
        for line in std::io::BufReader::new(err).lines().map_while(Result::ok) {
            all.push_str(&line);
            all.push('\n');
            let _ = tx.send(line);
        }
        all
    });
    (run, rx, reader)
}

/// Wait until `ok` holds, at most [`LIMIT`].
fn wait_until(what: &str, mut ok: impl FnMut() -> bool) {
    let t = Instant::now();
    while !ok() {
        assert!(t.elapsed() < LIMIT, "no {what} in {LIMIT:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Wait for a line of stderr that contains `text`.
fn line(rx: &Receiver<String>, text: &str) {
    let end = Instant::now() + LIMIT;
    loop {
        let left = end.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(l) if l.contains(text) => return,
            Ok(_) => continue,
            Err(e) => panic!("no line {text:?}: {e}"),
        }
    }
}

fn signal(run: &Child, sig: libc::c_int) {
    // SAFETY: kill(2) sends a signal to the child that this test started.
    unsafe { libc::kill(run.id() as libc::pid_t, sig) };
}

/// The pid that the runner wrote, once it runs.
fn started(dir: &Path) -> u32 {
    wait_until("start of the runner", || dir.join("started").exists());
    std::fs::read_to_string(dir.join("started")).unwrap().trim().parse().unwrap()
}

/// Whether the process `pid` is gone or a zombie.
fn gone(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Err(_) => true,
        Ok(s) => s.rsplit_once(')').is_some_and(|(_, r)| r.trim_start().starts_with(['Z', 'X'])),
    }
}

#[test]
fn a_second_signal_says_that_the_stop_goes_on() {
    let dir = Scratch::new("run-signal");
    runner(&dir, false);
    let (run, rx, reader) = start(&dir);
    started(&dir);
    let t = Instant::now();
    signal(&run, libc::SIGINT);
    line(&rx, "podrunner: stopping: the running job ends");
    signal(&run, libc::SIGINT);
    let out = run.wait_with_output().unwrap();
    let err = reader.join().unwrap();
    assert_eq!(out.status.code(), Some(130), "{err}");
    assert!(t.elapsed() < Duration::from_secs(20), "{:?}", t.elapsed());
    assert!(err.contains("podrunner: stopping: already stopping; a third signal ends podrunner at once"), "{err}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("cancelled a"));
}

/// T-636: SIGHUP (a closed terminal) says that the run stops, as SIGINT
/// and SIGTERM do; stderr is a pipe here, so the line stays.
#[test]
fn a_hangup_says_that_the_run_stops() {
    let dir = Scratch::new("run-hangup");
    runner(&dir, false);
    let (run, _rx, reader) = start(&dir);
    started(&dir);
    signal(&run, libc::SIGHUP);
    let out = run.wait_with_output().unwrap();
    let err = reader.join().unwrap();
    assert_eq!(out.status.code(), Some(130), "{err}");
    assert!(err.contains("podrunner: stopping: the running job ends"), "{err}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("cancelled a"));
}

/// T-636: a third signal ends podrunner at once with 130, ends the process
/// group of the runner, and a helper removes the files of the run: no
/// entry of podrunner stays in `TMPDIR`.
#[test]
fn a_third_signal_leaves_no_process_and_no_file() {
    let dir = Scratch::new("run-third");
    runner(&dir, true);
    let (run, rx, reader) = start(&dir);
    let pid = started(&dir);
    signal(&run, libc::SIGINT);
    line(&rx, "podrunner: stopping: the running job ends");
    signal(&run, libc::SIGINT);
    line(&rx, "podrunner: stopping: already stopping");
    signal(&run, libc::SIGINT);
    let out = run.wait_with_output().unwrap();
    // The end of stderr: the helper ended too.
    let err = reader.join().unwrap();
    assert_eq!(out.status.code(), Some(130), "{err}");
    assert!(err.contains("podrunner: ended at once"), "{err}");
    wait_until("end of the runner", || gone(pid));
    let left: Vec<String> = std::fs::read_dir(dir.join("tmp"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("podrunner-"))
        .collect();
    assert!(left.is_empty(), "the run left {left:?}\n{err}");
}

/// The manifest of the image of the stand-in of podvm.
const MANIFEST: &str = "sha256:4444444444444444444444444444444444444444444444444444444444444444";

/// A stand-in of podvm in `dir` whose unit `hung` does not go: `rm -f` of
/// that unit returns only when its caller (the run) is gone or a zombie.
/// `run -d` writes `dir/started`; `wait` of `hung` writes its pid to
/// `dir/waiter` and does not end for 60 s (it has the tag of the unit, so
/// the helper ends it); `rm` of `hung` writes its pid to `dir/rm`. Each
/// other unit ends at once with exit 1 and goes at once.
fn hung_podvm(dir: &Path, hung: &str) -> String {
    let d = dir.display();
    let script = format!(
        r#"#!/bin/sh
S=$PODVM_STORE
arg() {{ k=$1; shift; while [ $# -gt 0 ]; do [ "$1" = "$k" ] && {{ echo "$2"; return; }}; shift; done; }}
image() {{ mkdir -p "$S/rootfs/img/rootfs/etc"; }}
case "$1" in
probe) echo '{{"podvm":"0.2.0-beta.40","rung":"userland"}}' ;;
inspect)
  if [ "$2" = --format ]; then n=$4; [ -d "$S/trees/$n" ] || exit 1; echo "$S/trees/$n/rootfs"
  else echo '[{{"State":{{"Status":"running"}}}}]'; fi ;;
save) o=$(arg -o "$@"); t={d}/index
  mkdir -p "$t" && echo '{{"schemaVersion":2,"manifests":[{{"digest":"{MANIFEST}"}}]}}' > "$t/index.json"
  tar -cf "$o" -C "$t" index.json ;;
load) echo "runner@{MANIFEST}" >> "$S/images.txt" ;;
images) cat "$S/images.txt" 2>/dev/null ;;
extract) image; echo "$S/rootfs/img/rootfs" ;;
run) image; n=$(arg --name "$@"); mkdir -p "$S/trees/$n"; cp -R "$S/rootfs/img/rootfs" "$S/trees/$n/rootfs"
  : > {d}/started ;;
wait) [ "$2" = {hung} ] || {{ echo 1; exit 0; }}
  echo $$ > {d}/waiter.tmp && mv {d}/waiter.tmp {d}/waiter; i=0; while [ $i -lt 600 ]; do sleep 0.1; i=$((i+1)); done ;;
rm) [ "$3" = {hung} ] || exit 0
  echo $$ > {d}/rm.tmp && mv {d}/rm.tmp {d}/rm; p=$PPID
  while [ -e /proc/$p ] && ! grep -q '^State:[[:space:]]*Z' /proc/$p/status 2>/dev/null; do sleep 0.05; done ;;
esac
exit 0
"#
    );
    let p = dir.join("podvm");
    podrunner_unit::program_file::write(&p, script).unwrap();
    p.display().to_string()
}

/// A child that gets SIGKILL at the drop: a failed test leaves no run.
struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Start `podrunner run w.yml` in `dir` on the stand-in `podvm` with a
/// bound of 300 ms on the wait for a unit at a cancel, the extra `args`,
/// its own `TMPDIR`, and a reader of its stderr that sends each line.
fn start_podvm(dir: &Path, podvm: &str, args: &[&str]) -> (Killed, Receiver<String>, std::thread::JoinHandle<String>) {
    std::fs::create_dir_all(dir.join("tmp")).unwrap();
    std::fs::create_dir_all(dir.join("store")).unwrap();
    let image = format!("runner@sha256:{}", "3".repeat(64));
    let mut run = Command::new(env!("CARGO_BIN_EXE_podrunner"))
        .args(["--podrunner-unit-bound-ms", "300", "run", "w.yml", "--no-cache", "--podvm", podvm, "--images"])
        .arg(dir.join("store"))
        .args(["--image", &image])
        .args(args)
        .current_dir(dir)
        .env("TMPDIR", dir.join("tmp"))
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let (tx, rx) = channel();
    let err = run.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut all = String::new();
        for line in std::io::BufReader::new(err).lines().map_while(Result::ok) {
            all.push_str(&line);
            all.push('\n');
            let _ = tx.send(line);
        }
        all
    });
    (Killed(run), rx, reader)
}

/// Each entry of podrunner that stays in `dir/tmp`.
fn left_in_tmp(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir.join("tmp"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("podrunner-"))
        .collect()
}

/// T-716: at a cancel, a unit that does not go holds the run only until the
/// bound (a short one from the private first argument): then the run says
/// so, ends with 130, and the helper of a third signal ends the unit and
/// removes the files of the run. No process of the unit and no entry of
/// podrunner in `TMPDIR` stays.
#[test]
fn a_unit_that_does_not_go_is_handed_over_at_the_bound() {
    let dir = Scratch::new("run-bound");
    std::fs::write(dir.join("w.yml"), "on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n      - run: sleep 30\n")
        .unwrap();
    let podvm = hung_podvm(&dir, "u-1");
    let (mut run, rx, reader) = start_podvm(&dir, &podvm, &[]);
    wait_until("start of the unit", || dir.join("started").exists() && dir.join("waiter").exists());
    let waiter: u32 = std::fs::read_to_string(dir.join("waiter")).unwrap().trim().parse().unwrap();
    let t = Instant::now();
    signal(&run.0, libc::SIGINT);
    line(&rx, "podrunner: a.0: stopping: the run waits for unit u-1 to go");
    wait_until("end of the run", || run.0.try_wait().unwrap().is_some());
    let took = t.elapsed();
    let code = run.0.wait().unwrap().code();
    // The end of stderr: the helper ended too.
    let err = reader.join().unwrap();
    assert_eq!(code, Some(130), "{err}");
    assert!(took >= Duration::from_millis(300), "the run ended before the bound: {took:?}\n{err}");
    assert!(
        err.contains("podrunner: a.0: unit u-1 did not go in 0 s; podrunner ends now, and a helper ends the unit"),
        "{err}"
    );
    assert!(err.contains("podrunner: ended at once; a helper ends the units of the run"), "{err}");
    assert!(err.contains("the files of a run that died are gone (1 units"), "{err}");
    let rm: u32 = std::fs::read_to_string(dir.join("rm")).unwrap().trim().parse().unwrap();
    wait_until("end of the processes of the unit", || gone(waiter) && gone(rm));
    let left = left_in_tmp(&dir);
    assert!(left.is_empty(), "the run left {left:?}\n{err}");
}

/// T-720: at a stop by `fail-fast` (not a signal), a unit that does not go
/// holds its job only until the bound: its end goes on in the background,
/// the log of its job names it, and the run goes on with the next job. At
/// the end of the run, a helper ends the unit and removes the files of the
/// run; the exit code is the one of the run.
#[test]
fn a_unit_that_does_not_go_at_fail_fast_does_not_hold_the_run() {
    let dir = Scratch::new("run-fail-fast");
    std::fs::write(
        dir.join("w.yml"),
        "on: push\njobs:\n  a:\n    runs-on: x\n    strategy:\n      matrix:\n        n: [1, 2]\n    \
         steps:\n      - run: sleep 30\n  c:\n    needs: a\n    if: always()\n    runs-on: x\n    \
         steps:\n      - run: true\n",
    )
    .unwrap();
    // One cell gets u-1, ends with exit 1 and fails; `fail-fast` stops the
    // other cell, whose unit u-2 does not go.
    let podvm = hung_podvm(&dir, "u-2");
    let logs = dir.join("logs").display().to_string();
    let (mut run, rx, reader) = start_podvm(&dir, &podvm, &["--jobs", "2", "--log-dir", &logs]);
    wait_until("start of the unit", || dir.join("waiter").exists());
    let waiter: u32 = std::fs::read_to_string(dir.join("waiter")).unwrap().trim().parse().unwrap();
    line(&rx, "stopping: the run waits for unit u-2 to go");
    wait_until("end of the run", || run.0.try_wait().unwrap().is_some());
    let code = run.0.wait().unwrap().code();
    let out = std::io::read_to_string(run.0.stdout.take().unwrap()).unwrap();
    // The end of stderr: the helper ended too.
    let err = reader.join().unwrap();
    assert_eq!(code, Some(1), "{out}\n{err}");
    assert!(err.contains(": unit u-2 did not go in 0 s; the run goes on, and the unit goes in the background"), "{err}");
    // The job `c` needs the cell of u-2: it ran after the bound.
    assert!(err.contains("unit u-3 starts"), "the run did not go on: {out}\n{err}");
    assert!(out.lines().any(|l| l.contains(" c ") || l.ends_with(" c")), "{out}");
    assert!(err.contains("podrunner: 1 units did not go at the cancel; podrunner ends now, and a helper ends them"), "{err}");
    assert!(err.contains("the files of a run that died are gone (1 units"), "{err}");
    let named = std::fs::read_dir(dir.join("logs"))
        .unwrap()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .any(|l| l.contains("podrunner: unit u-2 did not go in 0 s at the cancel; its end goes on in the background"));
    assert!(named, "no log of a job names the unit u-2\n{err}");
    let rm: u32 = std::fs::read_to_string(dir.join("rm")).unwrap().trim().parse().unwrap();
    wait_until("end of the processes of the unit", || gone(waiter) && gone(rm));
    let left = left_in_tmp(&dir);
    assert!(left.is_empty(), "the run left {left:?}\n{err}");
}
