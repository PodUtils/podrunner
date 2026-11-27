//! T-220: the fake broker on a unix socket answers as on TCP, and names the
//! address of its host in each URL.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use podrunner_broker_fake::Broker;

#[test]
fn the_broker_on_a_unix_socket_answers() {
    // Not in TMPDIR: `sun_path` holds 107 bytes, and a long TMPDIR passes
    // that limit (T-735).
    let path = std::path::Path::new("/tmp").join(format!("pb-broker-unix-{}.sock", std::process::id()));
    let broker = Broker::start_unix(&path, "broker.podrunner.test").unwrap();
    assert_eq!(broker.base, "http://broker.podrunner.test/");
    let mut c = UnixStream::connect(&path).unwrap();
    c.write_all(b"GET /_apis/connectionData HTTP/1.1\r\nHost: broker.podrunner.test\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut out = String::new();
    let _ = c.read_to_string(&mut out);
    assert!(out.starts_with("HTTP/1.1 "), "{out}");
    assert!(broker.seen().iter().any(|s| s.contains("/_apis/connectionData")), "{:?}", broker.seen());
    let _ = std::fs::remove_file(&path);
}
