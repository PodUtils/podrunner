use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use pod_control::conformance;

use super::*;

struct Echo(Vec<Request>);

impl Handler for Echo {
    fn handle(&mut self, req: &Request) -> Outcome {
        self.0.push(req.clone());
        match req {
            Request::Status => Ok(json!({ "draining": false, "kinds": [{ "slots": 2, "busy": 1 }] })),
            _ => Ok(Value::from("done")),
        }
    }
}

fn dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("podrunner-ctl-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Answer the calls of `incoming` with `h` in a thread, until `stop`.
fn answer(incoming: Incoming, stop: std::sync::Arc<std::sync::atomic::AtomicBool>) -> std::thread::JoinHandle<Echo> {
    std::thread::spawn(move || {
        let mut h = Echo(Vec::new());
        while !stop.load(std::sync::atomic::Ordering::SeqCst) {
            incoming.answer_waiting(&mut AsHost(&mut h), Duration::from_millis(20));
        }
        h
    })
}

#[test]
fn each_request_has_its_method_and_the_server_reads_it_back() {
    let all = [
        Request::Status,
        Request::Drain,
        Request::Stop,
        Request::Capacity,
        Request::ClearDegraded,
        Request::Withdraw { job_id: "j".into() },
        Request::RestartSource { source: "s".into() },
        Request::DrainSource { source: "s".into() },
        Request::ResumeSource { source: "s".into() },
        Request::StopSource { source: "s".into() },
    ];
    for req in all {
        let (method, params) = req.method();
        let line = pod_control::wire::request_line(7, method, &params, None);
        assert_eq!(parse(line.as_bytes()), Ok(req), "{line}");
    }
    // `drain` with no params is the drain of pod-control: exit after it.
    assert_eq!(parse(br#"{"schema":1,"id":1,"method":"drain"}"#), Ok(Request::Stop));
}

#[test]
fn bad_lines_and_bad_params_are_refused_with_no_byte_of_the_line() {
    for (line, code) in [
        (&b"x"[..], Code::BadRequest),
        (br#"{"v":1,"op":"status"}"#, Code::BadRequest),
        (br#"{"schema":2,"id":1,"method":"status"}"#, Code::BadRequest),
        (br#"{"schema":1,"id":1,"method":"rm"}"#, Code::UnknownMethod),
        (br#"{"schema":1,"id":1,"method":"status","params":{"secret-1":1}}"#, Code::BadParams),
        (br#"{"schema":1,"id":1,"method":"withdraw","params":{"job_id":"x","secret-1":1}}"#, Code::BadParams),
        (br#"{"schema":1,"id":1,"method":"drain-source","params":{}}"#, Code::BadParams),
        (b"\xff", Code::NotUtf8),
    ] {
        let fault = parse(line).unwrap_err();
        assert_eq!(fault.code, code, "{}", String::from_utf8_lossy(line));
        assert!(!fault.detail.contains("secret-1"), "{fault}");
    }
    assert_eq!(parse(&vec![b' '; pod_control::LINE_MAX + 1]).unwrap_err().code, Code::TooLong);
}

#[test]
fn the_catalogue_holds_each_extra_and_restart_source_takes_the_master_tier() {
    let c = catalogue();
    for name in extra_names() {
        assert!(c.find(&name).is_some(), "{name}");
    }
    assert_eq!(c.find("restart-source").unwrap().tier(), Tier::Master);
    assert_eq!(c.find("capacity").unwrap().tier(), Tier::Read);
    assert_eq!(c.find("drain-source").unwrap().tier(), Tier::Operate);
    assert_eq!(settings().same_user, Tier::Master);
}

#[test]
fn the_socket_has_mode_0600_and_answers_each_method() {
    let dir = dir("one");
    let path = dir.join("ctl.sock");
    let (server, incoming) = serve(&path).unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let host = answer(incoming, stop.clone());
    assert_eq!(call(&path, &Request::Capacity).unwrap(), Value::from("done"));
    assert_eq!(call(&path, &Request::Stop).unwrap(), Value::from("done"));
    assert_eq!(call(&path, &Request::Drain).unwrap(), Value::from("done"));
    let mut c = Client::connect_unix(&path, ClientSettings::default()).unwrap();
    assert_eq!(c.version().unwrap().program, PROGRAM);
    assert_eq!(c.metrics().unwrap().gauges["slots"], 2);
    assert_eq!(c.metrics().unwrap().gauges["busy"], 1);
    assert_eq!(c.stop().unwrap(), Value::from("done"));
    assert_eq!(c.reload().unwrap_err().code(), Some(Code::Unsupported));
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    let h = host.join().unwrap();
    assert_eq!(h.0[..3], [Request::Capacity, Request::Stop, Request::Drain]);
    drop(server);
    assert!(!path.exists(), "the drop of the server removes its socket");
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-576: the conformance check of `pod-control` against the server of
/// `serve`, at the tier of podrunner and at each lower tier.
#[test]
fn the_server_of_serve_conforms_to_pod_control_at_each_tier() {
    let dir = dir("conformance");
    for tier in [Tier::Master, Tier::Operate, Tier::Read] {
        let path = dir.join(format!("{}.sock", tier.name()));
        let mut s = settings();
        s.same_user = tier;
        let (_server, incoming) = pod_control::unix::serve(&path, s, catalogue()).unwrap();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let host = answer(incoming, stop.clone());
        let mut connect = || Client::connect_unix(&path, ClientSettings::default());
        let options = conformance::Options { extras: extra_names(), ..conformance::Options::default() };
        let faults = conformance::check(&mut connect, &options);
        assert!(faults.is_empty(), "{tier:?}: {faults:#?}");
        stop.store(true, std::sync::atomic::Ordering::SeqCst);
        // The check calls no method that changes the program.
        assert!(host.join().unwrap().0.iter().all(|r| *r == Request::Status), "{tier:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn silent_clients_delay_no_answer_and_a_ninth_is_busy() {
    let dir = dir("idle");
    let path = dir.join("ctl.sock");
    let (_server, incoming) = serve(&path).unwrap();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let host = answer(incoming, stop.clone());
    // Seven silent clients hold seven of the eight places.
    let idle: Vec<UnixStream> = (0..7).map(|_| UnixStream::connect(&path).unwrap()).collect();
    std::thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    assert_eq!(call(&path, &Request::Capacity).unwrap(), Value::from("done"));
    assert!(started.elapsed() < Duration::from_secs(1), "{:?}", started.elapsed());
    // Eight silent clients now: a ninth gets busy.
    let _eighth = UnixStream::connect(&path).unwrap();
    // The server takes the eighth place when it accepts the connection,
    // which a slow host does late: ask again until the answer is busy.
    let deadline = Instant::now() + Duration::from_secs(10);
    let e = loop {
        match call(&path, &Request::Status) {
            Err(e) => break e,
            Ok(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Ok(v) => panic!("no busy in 10 s: {v}"),
        }
    };
    assert_eq!(e.code(), Some(Code::Busy), "{e}");
    drop(idle);
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    host.join().unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

/// T-184: while the supervisor answers nothing, `status` gets the last
/// snapshot, and another request gets `busy`.
#[test]
fn status_takes_the_snapshot_while_the_supervisor_is_busy() {
    let dir = dir("snapshot");
    let path = dir.join("ctl.sock");
    let mut s = settings();
    s.answer_wait = Duration::from_millis(300);
    let (server, _incoming) = pod_control::unix::serve(&path, s, catalogue()).unwrap();
    server.snapshot().set(json!({ "jobs": 3 }));
    assert_eq!(call(&path, &Request::Status).unwrap(), json!({ "jobs": 3 }));
    assert_eq!(call(&path, &Request::Drain).unwrap_err().code(), Some(Code::Busy));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_slow_client_is_cut_at_the_deadline_of_its_line() {
    let dir = dir("slow");
    let path = dir.join("ctl.sock");
    let (_server, _incoming) = serve(&path).unwrap();
    let limit = settings().read_timeout;
    let mut s = UnixStream::connect(&path).unwrap();
    let started = Instant::now();
    // One byte each 500 ms, never a line end: the server ends the
    // connection at the deadline of the line.
    let mut closed = false;
    while started.elapsed() < limit + Duration::from_secs(3) {
        if s.write_all(b" ").is_err() {
            closed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    assert!(closed, "the connection was not cut");
    assert!(started.elapsed() >= limit, "{:?}", started.elapsed());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn serve_keeps_a_live_socket_and_a_file_and_replaces_a_stale_socket() {
    let dir = dir("bind");
    let path = dir.join("ctl.sock");
    let live = serve(&path).unwrap();
    assert_eq!(serve(&path).err().unwrap().kind(), std::io::ErrorKind::AddrInUse);
    drop(live);
    // The file of a dead listener is stale: it is replaced.
    let stale = std::os::unix::net::UnixListener::bind(&path).unwrap();
    drop(stale);
    assert!(serve(&path).is_ok());
    let file = dir.join("file");
    std::fs::write(&file, "x").unwrap();
    assert_eq!(serve(&file).err().unwrap().kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(&file).unwrap(), b"x");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_metrics_of_a_status() {
    let status = json!({
        "draining": true, "degraded": "a unit leaked", "no_tier_served": false, "units": 2,
        "spooled_reports": 1, "ended_without_completion": 4,
        "kinds": [{ "slots": 2, "busy": 1 }, { "slots": 3, "busy": 2 }],
        "sources": [
            { "jobs": { "running": 1 }, "refused": { "tier": 2 } },
            { "jobs": { "running": 2, "done": 5 }, "refused": {} }
        ]
    });
    let m = metrics_of(&status);
    assert_eq!((m.gauges["draining"], m.gauges["degraded"], m.gauges["no_tier_served"]), (1, 1, 0));
    assert_eq!((m.gauges["slots"], m.gauges["busy"], m.gauges["units"]), (5, 3, 2));
    assert_eq!((m.gauges["jobs_running"], m.gauges["jobs_done"]), (3, 5));
    assert_eq!((m.counters["refused_tier"], m.counters["ended_without_completion"]), (2, 4));
    assert!(m.prometheus("podrunner").contains("podrunner_busy 3\n"));
}
