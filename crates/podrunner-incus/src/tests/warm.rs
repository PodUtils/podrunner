//! T-487: warm instances. A warm instance starts with no stdin and no argv
//! of a job; its claim goes on the stdin of one `podvm exec -i`; a claim
//! with other limits is refused before the exec; the script of the
//! instance takes the claim before its channel and its engine.

use std::io::Write;
use std::process::Stdio;

use podrunner_unit::limits::Limits;
use podrunner_unit::warm::{ClaimSpec, WarmSpec, env_sh};

use super::*;
use crate::warm::{CLAIM_DIR, CLAIM_SCRIPT, READY, claim_stream};

fn warm_spec(unit: &str) -> WarmSpec {
    let s = spec(unit);
    let env = BTreeMap::from([("PODRUNNER_UNIT".to_string(), unit.to_string())]);
    WarmSpec { unit: s.unit, image: s.image, sockets: None, env }
}

fn claim_spec() -> ClaimSpec {
    let env = BTreeMap::from([("A".to_string(), "1".to_string()), ("B".to_string(), "it's".to_string())]);
    let argv = vec!["/home/runner/run.sh".to_string(), "a b".to_string()];
    ClaimSpec { argv, env, credential: canary_credential(), limits: Limits::default() }
}

fn lines_of(d: &Path, name: &str, ext: &str) -> Vec<String> {
    let f = fs::read_to_string(d.join("store").join(format!("{name}.{ext}"))).unwrap_or_default();
    f.lines().map(String::from).collect()
}

#[test]
fn a_warm_instance_starts_with_no_stdin_no_argv_of_a_job_and_the_limits_of_the_kind() {
    let (u, d) = driver("warm", "sleep 30");
    let kind = Limits { memory_bytes: Some(2 << 30), cpus: None, pids: Some(256), disk_bytes: None };
    let u = u.with_limits(kind);
    let w = warm_spec("u-1");
    u.warm(&w).unwrap();
    let name = u.name(&w.unit);
    let argv = lines_of(&d, &name, "argv");
    assert_eq!(&argv[..3], ["-d", "--name", name.as_str()], "no -i: {argv:?}");
    assert!(argv.contains(&"PODRUNNER_WARM=1".to_string()), "{argv:?}");
    assert_eq!(&argv[argv.len() - 2..], ["podrunner", "/podrunner/.runner"], "no argv of a job");
    assert!(argv.join(" ").contains("--memory 2147483648 --pids-limit 256"), "{argv:?}");
    assert!(lines_of(&d, &name, "stdin").is_empty(), "no stdin");
    assert!(!u.warm_ready(&w.unit).unwrap());
    let work = u.observe(&w.unit).unwrap().unit_root.unwrap();
    fs::write(work.join(READY), "").unwrap();
    assert!(u.warm_ready(&w.unit).unwrap());
    assert_eq!(u.warm(&w).unwrap_err().code, UnitCode::UnitInUse);
    assert!(u.destroy(&w.unit).unwrap().is_clean());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_claim_goes_on_the_stdin_of_one_podvm_exec() {
    let (u, d) = driver("claim", "sleep 30");
    let w = warm_spec("u-1");
    u.warm(&w).unwrap();
    let c = claim_spec();
    u.claim(&w.unit, &c).unwrap();
    let name = u.name(&w.unit);
    let argv = lines_of(&d, &name, "exec");
    assert_eq!(&argv[..4], ["-i", "--podvm-tier=incus", name.as_str(), "/bin/sh"], "{argv:?}");
    assert_eq!(&argv[argv.len() - 2..], ["podrunner", "/podrunner/.runner"]);
    let all = argv.join("\n");
    assert!(!all.contains(CANARY) && !all.contains("it's") && !all.contains("run.sh"), "{all}");
    let stdin = fs::read(d.join("store").join(format!("{name}.claim"))).unwrap();
    assert_eq!(stdin, claim_stream(&c.credential, &env_sh(&c.env, &c.argv).unwrap()).unwrap());
    assert!(!String::from_utf8_lossy(&stdin).contains(CANARY), "the files go as base64");
    assert!(!u.unit_dir(&w.unit).join("claim.err").exists());
    // One claim; a unit of start and an unknown unit take none.
    assert_eq!(u.claim(&w.unit, &c).unwrap_err().code, UnitCode::UnitInUse);
    let s = spec("u-2");
    u.start(&s).unwrap();
    assert_eq!(u.claim(&s.unit, &c).unwrap_err().code, UnitCode::UnitInUse);
    let unknown = UnitId::parse("u-3").unwrap();
    assert_eq!(u.claim(&unknown, &c).unwrap_err().code, UnitCode::UnknownUnit);
    assert!(u.destroy(&w.unit).unwrap().is_clean());
    assert!(u.destroy(&s.unit).unwrap().is_clean());
    let _ = fs::remove_dir_all(&d);
}

/// Match or cold: an offer whose lesser limits are not those of the
/// instance is refused before the exec; podvm cannot change them
/// (COORD-132).
#[test]
fn a_claim_with_other_limits_is_refused_before_the_exec() {
    let (u, d) = driver("limits-warm", "sleep 30");
    let u = u.with_limits(Limits { pids: Some(256), ..Limits::default() });
    let w = warm_spec("u-1");
    u.warm(&w).unwrap();
    let low = Limits { pids: Some(64), ..Limits::default() };
    assert_ne!(u.applied_limits(&low), u.applied_limits(&Limits::default()));
    let high = Limits { pids: Some(1000), ..Limits::default() };
    assert_eq!(u.applied_limits(&high), u.applied_limits(&Limits::default()), "the kind holds");
    let err = u.claim(&w.unit, &ClaimSpec { limits: low, ..claim_spec() }).unwrap_err();
    assert_eq!(err.code, UnitCode::UnitStartFailed);
    assert!(lines_of(&d, &u.name(&w.unit), "exec").is_empty(), "no exec");
    u.claim(&w.unit, &ClaimSpec { limits: high, ..claim_spec() }).unwrap();
    assert!(u.destroy(&w.unit).unwrap().is_clean());
    let _ = fs::remove_dir_all(&d);
}

#[test]
fn a_failed_exec_fails_the_claim_with_the_line_of_podvm() {
    let (u, d) = driver("noexec", "sleep 30");
    fs::write(d.join("podvm.noexec"), "").unwrap();
    let w = warm_spec("u-1");
    u.warm(&w).unwrap();
    let err = u.claim(&w.unit, &claim_spec()).unwrap_err();
    assert_eq!(err.code, UnitCode::UnitStartFailed);
    assert!(err.detail.contains("the claim failed"), "{}", err.detail);
    assert!(u.destroy(&w.unit).unwrap().is_clean());
    let _ = fs::remove_dir_all(&d);
}

/// The warm branch of the script and the script of the claim, on the host
/// with the paths of the instance in a test directory: the job gets its
/// argv and variables, the files are there at mode 0600, and a second
/// claim fails.
#[test]
fn the_script_takes_the_claim_before_the_channel_and_the_engine() {
    let script = crate::guest::SCRIPT;
    let at = |w: &str| script.find(w).unwrap_or_else(|| panic!("{w}"));
    assert!(at("if [ \"${PODRUNNER_WARM:-}\" = 1 ]") < at("if [ -n \"${PODRUNNER_CHANNEL_PORT"));
    assert!(at("if [ -n \"${PODRUNNER_CHANNEL_PORT") < at("if [ -x /podrunner-engine/dockerd ]"));
    assert!(script.contains(&format!("{CLAIM_DIR}/go")) && script.contains(READY));
    let t = dir("script");
    let (work, claim, root) = (t.join("work"), t.join("claim"), t.join("root"));
    fs::create_dir_all(&work).unwrap();
    let here = |s: &str| {
        s.replace("/podrunner-work", &work.display().to_string()).replace(CLAIM_DIR, &claim.display().to_string())
    };
    let head = here(&script[..at("# The channel to the egress socket")]);
    let body = format!("{head}echo \"$#|$1|$2|$B|${{PODRUNNER_WARM:-none}}\" > {}/out", t.display());
    let mut warm = std::process::Command::new("sh")
        .args(["-c", &body, "podrunner", &root.display().to_string()])
        .env("PODRUNNER_WARM", "1")
        .spawn()
        .unwrap();
    let end = std::time::Instant::now() + Duration::from_secs(10);
    while !work.join(READY).exists() && std::time::Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(work.join(READY).exists(), "the instance marks itself ready");
    let c = claim_spec();
    let stream = claim_stream(&c.credential, &env_sh(&c.env, &c.argv).unwrap()).unwrap();
    let run_claim = || {
        let mut p = std::process::Command::new("sh")
            .args(["-c", &here(CLAIM_SCRIPT), "podrunner", &root.display().to_string()])
            .stdin(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        p.stdin.take().unwrap().write_all(&stream).unwrap();
        p.wait().unwrap()
    };
    assert!(run_claim().success());
    assert!(warm.wait().unwrap().success());
    let out = fs::read_to_string(t.join("out")).unwrap();
    assert_eq!(out.trim(), "2|/home/runner/run.sh|a b|it's|none");
    for f in &c.credential.files {
        let m = fs::metadata(root.join(&f.name)).unwrap();
        assert_eq!(m.permissions().mode() & 0o777, 0o600, "{}", f.name);
        assert_eq!(fs::read(root.join(&f.name)).unwrap(), podrunner_schema::FileBytes::bytes(f).unwrap().as_ref());
    }
    assert_eq!(fs::metadata(&claim).unwrap().permissions().mode() & 0o777, 0o700);
    assert!(!claim.join("env.sh").exists() && !work.join(READY).exists());
    assert!(!run_claim().success(), "a second claim");
    let _ = fs::remove_dir_all(&t);
}

/// A stream that a name broke (the old frame ended the files at a line
/// `.`): the script of the claim ends with no `go` and sources nothing; a
/// stream with fewer files than its count ends too.
#[test]
fn a_broken_stream_of_a_claim_gives_no_go() {
    let t = dir("frame");
    let (claim, root) = (t.join("claim"), t.join("root"));
    let mark = t.join("pwned");
    let env = format!("echo pwned > {}\nset --\n", mark.display());
    let run = |stream: &str| {
        let script = CLAIM_SCRIPT.replace(CLAIM_DIR, &claim.display().to_string());
        let mut p = std::process::Command::new("sh")
            .args(["-c", &script, "podrunner", &root.display().to_string()])
            .stdin(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        p.stdin.take().unwrap().write_all(stream.as_bytes()).unwrap();
        let ok = p.wait().unwrap().success();
        let go = claim.join("go").exists();
        let _ = fs::remove_dir_all(&claim);
        (ok, go)
    };
    // The name "a\n." of a source: its "." line, then a line of its own.
    let broken = format!("2\na\n. eA==\nexport X=1 eA==\n{env}");
    assert_eq!(run(&broken), (false, false), "a name \".\" ends the claim");
    assert_eq!(run(&format!("3\n.runner eA==\n{env}")), (false, false), "fewer files than the count");
    assert_eq!(run(&format!("x\n{env}")), (false, false), "no count");
    assert!(!mark.exists(), "nothing of the stream ran");
    let _ = fs::remove_dir_all(&t);
}
