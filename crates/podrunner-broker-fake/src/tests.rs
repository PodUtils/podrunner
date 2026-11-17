use std::io::{Read, Write};

use super::*;

fn ask(broker: &Broker, line: &str) -> String {
    let addr = broker.base.trim_start_matches("http://").trim_end_matches('/');
    let mut c = TcpStream::connect(addr).unwrap();
    c.write_all(format!("{line} HTTP/1.1\r\nHost: b\r\nContent-Length: 0\r\n\r\n").as_bytes()).unwrap();
    let mut out = String::new();
    let _ = c.read_to_string(&mut out);
    out
}

fn put(broker: &Broker, path: &str, body: &str) -> String {
    // The signature of this broker in place of `sig=x` (T-406).
    let sig = broker.state.lock().unwrap().auth.blob.clone();
    let path = path.replace("sig=x", &format!("sig={sig}"));
    let addr = broker.base.trim_start_matches("http://").trim_end_matches('/');
    let mut c = TcpStream::connect(addr).unwrap();
    c.write_all(format!("PUT {path} HTTP/1.1\r\nHost: b\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes())
        .unwrap();
    let mut out = String::new();
    let _ = c.read_to_string(&mut out);
    out
}

#[test]
fn the_blobs_keep_their_kind_and_the_new_text_comes_once() {
    let broker = Broker::start().unwrap();
    let _ = put(&broker, "/blobs/step-1?sig=x", "line one\n");
    let _ = put(&broker, "/blobs/summary-1?sig=x", "# the summary\n");
    assert_eq!(broker.take_new(), [("step-1".to_string(), "line one\n".to_string())], "no summary in the live lines");
    let _ = put(&broker, "/blobs/step-1?sig=x", "line two\n");
    let _ = put(&broker, "/blobs/GetJobLogsSignedBlobURL?sig=x", "\u{feff}line one\nline two\n");
    assert_eq!(broker.take_new(), [("step-1".to_string(), "line two\n".to_string())], "only the new text");
    assert!(broker.take_new().is_empty());
    assert!(!broker.logs().contains("summary"), "the logs have no summary");
    assert_eq!(broker.logs(), "line one\nline two\n", "the job log once, with no byte order mark");
    assert!(broker.blobs().iter().any(|(k, v)| k == "summary-1" && v.contains("# the summary")));
}

#[test]
fn a_request_with_no_route_is_kept_with_its_service() {
    let broker = Broker::start().unwrap();
    assert!(ask(&broker, "POST /_apis/artifactcache/caches?x=1").starts_with("HTTP/1.1 404"));
    assert!(ask(&broker, "GET /no/such/route").starts_with("HTTP/1.1 404"));
    let missing = broker.missing();
    assert_eq!(missing.len(), 2, "{missing:?}");
    assert!(missing[0].starts_with("POST /_apis/artifactcache/caches (the legacy cache routes"), "{missing:?}");
    assert_eq!(missing[1], "GET /no/such/route");
    assert_eq!(service_of("/twirp/x.OIDC/idtoken"), Some("the OIDC id-token of the job (T-140)"));
}

/// T-406: a local process with no token of the runner gets no job: the
/// long poll, `acquirejob` and the token of the OAuth client refuse it.
#[test]
fn a_caller_with_no_token_gets_no_job() {
    let broker = Broker::start().unwrap();
    let mut job = Job::scripts("j", &["echo hi"]);
    job.secrets.push(("S".into(), "a-made-up-secret".into()));
    broker.queue(job);
    let polled = ask(&broker, "GET /message");
    assert!(polled.starts_with("HTTP/1.1 401"), "{polled}");
    let acquired = ask(&broker, "POST /acquirejob");
    assert!(acquired.starts_with("HTTP/1.1 401") && !acquired.contains("a-made-up-secret"), "{acquired}");
    let token = ask(&broker, "POST /_apis/oauth2/token");
    assert!(token.starts_with("HTTP/1.1 401"), "{token}");
    // The twirp services and the live feed take the job token.
    for line in [
        "POST /twirp/github.actions.results.api.v1.CacheService/GetCacheEntryDownloadURL",
        "GET /_podrunner/feed",
        "GET /_checkout.tar.gz",
        "GET /_podrunner/hold/build.0",
    ] {
        let out = ask(&broker, line);
        assert!(out.starts_with("HTTP/1.1 401"), "{line}: {out}");
    }
    // A blob URL with another signature writes no line into a log.
    let forged = put(&broker, "/blobs/step-1?sig=fake", "a forged line\n");
    assert!(forged.starts_with("HTTP/1.1 403"), "{forged}");
    assert!(put(&broker, "/blobs/step-1", "x").starts_with("HTTP/1.1 403"), "no signature");
    assert!(broker.take_new().is_empty() && broker.logs().is_empty());
    assert!(put(&broker, "/blobs/step-1?sig=x", "a line\n").starts_with("HTTP/1.1 201"));
}
