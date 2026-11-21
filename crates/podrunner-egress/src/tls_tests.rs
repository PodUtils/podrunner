//! The TLS end of the module (T-563): a unit with a placeholder reaches a
//! stand-in host, which gets the value of the secret; the unit sees the
//! placeholder alone. The far end is a TLS server in memory with a leaf of
//! its own CA, which the broker trusts.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pod_broker::ca::JobCa;
use pod_broker::target::Host;
use pod_broker::{Broker, Secrets, Settings};
use pod_core::{BoxFuture, Connected, Dial, Failure, Target};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::Egress;
use super::tls::{HostTls, provider};

/// A made-up value of a secret: the test proves that it never reaches
/// the unit, a line or an error.
const SECRET: &str = "made-up-secret-value-T563-e5f6a7b8";
/// The placeholder that the unit holds.
const PH: &str = "PODCRED_TEST_TOKEN_0001";
const HOST: &str = "api.example.com";

fn policy(credential: bool) -> String {
    let credentials = if credential {
        format!(r#","credentials":[{{"placeholder":"{PH}","secret":"token","hosts":["{HOST}"]}}]"#)
    } else {
        String::new()
    };
    format!(r#"{{"schema":1,"mode":"allow","allow":[{{"host":"{HOST}"}}]{credentials}}}"#)
}

/// The far end: a TLS server with a leaf of its CA for [`HOST`]; it keeps
/// the head of each request and answers with it in the body.
struct Far {
    ca: JobCa,
    seen: Arc<Mutex<String>>,
}

impl Dial for Far {
    fn label(&self) -> String {
        "far".into()
    }

    fn dial<'a>(&'a self, _: &'a Target, _: tokio::time::Instant) -> BoxFuture<'a, Result<Connected, Failure>> {
        let leaf = self.ca.leaf(&Host::Name(HOST.into())).unwrap();
        let seen = self.seen.clone();
        Box::pin(async move {
            let (ours, theirs) = tokio::io::duplex(64 * 1024);
            let mut config = rustls::ServerConfig::builder_with_provider(provider())
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_no_client_auth()
                .with_cert_resolver(Arc::new(One(leaf)));
            config.alpn_protocols = vec![b"http/1.1".to_vec()];
            let acceptor = pod_tls::server::TlsAcceptor::from(Arc::new(config));
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(theirs).await else { return };
                let mut head = Vec::new();
                let mut b = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") && tls.read(&mut b).await.is_ok_and(|n| n == 1) {
                    head.push(b[0]);
                }
                let head = String::from_utf8_lossy(&head).into_owned();
                seen.lock().unwrap().push_str(&head);
                let answer = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{head}", head.len());
                let _ = tls.write_all(answer.as_bytes()).await;
                let _ = tls.flush().await;
            });
            Ok(Connected::new(Box::new(ours)))
        })
    }
}

/// The one leaf of the far end.
#[derive(Debug)]
struct One(Arc<rustls::sign::CertifiedKey>);

impl rustls::server::ResolvesServerCert for One {
    fn resolve(&self, _: rustls::server::ClientHello<'_>) -> Option<Arc<rustls::sign::CertifiedKey>> {
        Some(self.0.clone())
    }
}

fn trusting(pem: &str) -> Arc<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    for cert in pod_tls::bundle::pem_certificates(pem.as_bytes()).unwrap() {
        roots.add(cert).unwrap();
    }
    let config = rustls::ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Arc::new(config)
}

/// A module with the TLS end, whose far end is `far`; the roots of a job
/// are `roots`.
fn egress(roots: Option<Vec<u8>>) -> (Egress, Arc<Mutex<String>>) {
    let far = Far { ca: JobCa::new("far", Duration::from_secs(600)).unwrap(), seen: Arc::default() };
    let seen = far.seen.clone();
    let tls = HostTls { trust: trusting(far.ca.pem()), roots_pem: roots };
    let broker = Broker::new(Settings::default(), Arc::new(super::tests::Public), Arc::new(far));
    (Egress::with_tls(broker, tls).unwrap(), seen)
}

/// A made-up root of a host: a CA that signs nothing here.
fn roots() -> Vec<u8> {
    JobCa::new("roots", Duration::from_secs(600)).unwrap().pem().as_bytes().to_vec()
}

fn dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("podrunner-egress-tls-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A request of the unit through a CONNECT and its own TLS, which trusts
/// `bundle`; the answer.
fn job_request(socket: &std::path::Path, bundle: &str, request: &str) -> String {
    let mut s = UnixStream::connect(socket).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    s.write_all(format!("CONNECT {HOST}:443 HTTP/1.1\r\nHost: {HOST}:443\r\n\r\n").as_bytes()).unwrap();
    let mut r = BufReader::new(s);
    let mut line = String::new();
    r.read_line(&mut line).unwrap();
    assert!(line.contains(" 200 "), "{line}");
    while r.read_line(&mut line).is_ok_and(|n| n > 0) && !line.ends_with("\r\n\r\n") {}
    let s = r.into_inner();
    let mut config = (*trusting(bundle)).clone();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let name = rustls::pki_types::ServerName::try_from(HOST).unwrap();
    let conn = rustls::ClientConnection::new(Arc::new(config), name).unwrap();
    let mut tls = rustls::StreamOwned::new(conn, s);
    tls.write_all(request.as_bytes()).unwrap();
    let mut r = BufReader::new(tls);
    let (mut answer, mut length) = (String::new(), 0usize);
    loop {
        let mut l = String::new();
        if r.read_line(&mut l).unwrap_or(0) == 0 {
            break;
        }
        if let Some(v) = l.to_ascii_lowercase().strip_prefix("content-length:") {
            length = v.trim().parse().unwrap();
        }
        answer.push_str(&l);
        if l == "\r\n" {
            break;
        }
    }
    let mut body = vec![0u8; length];
    r.read_exact(&mut body).unwrap();
    answer + &String::from_utf8_lossy(&body)
}

#[test]
fn the_far_end_gets_the_value_and_the_unit_sees_the_placeholder() {
    let (e, seen) = egress(Some(roots()));
    let d = dir("swap");
    let socket = d.join("u-1.sock");
    let secrets = Secrets::new().with("token", SECRET);
    let opened = e.open_with("u-1", &policy(true), secrets, &socket, Duration::from_secs(600)).unwrap();
    let bundle = opened.ca_bundle.expect("a CA for a job with a credential");
    assert!(bundle.matches("BEGIN CERTIFICATE").count() >= 2, "the CA of the job and the roots");
    assert!(!bundle.contains(SECRET));
    let request = format!("GET /user HTTP/1.1\r\nHost: {HOST}\r\nAuthorization: Bearer {PH}\r\n\r\n");
    let answer = job_request(&socket, &bundle, &request);
    let far = seen.lock().unwrap().clone();
    assert!(far.contains(&format!("Bearer {SECRET}")), "the far end gets the value: {far}");
    assert!(!far.contains(PH), "{far}");
    assert!(answer.contains(&format!("Bearer {PH}")), "the answer holds the placeholder: {answer}");
    assert!(!answer.contains(SECRET), "the unit never sees the value: {answer}");
    e.close("u-1");
    let lines = format!("{:?}", e.take_lines());
    assert!(!lines.contains(SECRET), "{lines}");
    let _ = std::fs::remove_dir_all(d);
}

/// A job with no credential and no path gets no CA; a credential with no
/// value, or a host with no roots, is refused, and no text holds a value.
#[test]
fn a_job_with_no_credential_gets_no_ca_and_a_missing_value_is_refused() {
    let (e, _) = egress(Some(roots()));
    let d = dir("refuse");
    let none = e.open_with("u-1", &policy(false), Secrets::new(), &d.join("a.sock"), Duration::from_secs(60)).unwrap();
    assert!(none.ca_bundle.is_none());
    let err =
        e.open_with("u-2", &policy(true), Secrets::new(), &d.join("b.sock"), Duration::from_secs(60)).unwrap_err();
    assert!(err.0.contains("token"), "names the secret: {err}");
    let other = Secrets::new().with("other", SECRET);
    let err = e.open_with("u-3", &policy(false), other, &d.join("c.sock"), Duration::from_secs(60)).unwrap_err();
    assert!(!err.0.contains(SECRET), "{err}");
    let (bare, _) = egress(None);
    let secrets = Secrets::new().with("token", SECRET);
    let err = bare.open_with("u-4", &policy(true), secrets, &d.join("d.sock"), Duration::from_secs(60)).unwrap_err();
    assert!(err.0.contains("no CA file"), "{err}");
    assert!(!d.join("d.sock").exists(), "no listener for a refused job");
    let _ = std::fs::remove_dir_all(d);
}

/// T-524: a rule with a path holds through the TLS end: the unit gets the
/// CA bundle with no credential, a request under the path reaches the far
/// end, and one outside it gets 403 from the broker.
#[test]
fn a_path_rule_is_held_by_the_tls_end() {
    let (e, seen) = egress(Some(roots()));
    let d = dir("path");
    let socket = d.join("u-1.sock");
    let policy = format!(r#"{{"schema":1,"mode":"allow","allow":[{{"host":"{HOST}","path":"/v1"}}]}}"#);
    let opened = e.open_with("u-1", &policy, Secrets::new(), &socket, Duration::from_secs(600)).unwrap();
    let bundle = opened.ca_bundle.expect("a CA for a job with a path rule");
    let get = |path: &str| job_request(&socket, &bundle, &format!("GET {path} HTTP/1.1\r\nHost: {HOST}\r\n\r\n"));
    let ok = get("/v1/repos");
    assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
    let refused = get("/v2");
    assert!(refused.starts_with("HTTP/1.1 403"), "{refused}");
    let far = seen.lock().unwrap().clone();
    assert!(far.contains("GET /v1/repos") && !far.contains("/v2"), "{far}");
    e.kill();
    let _ = std::fs::remove_dir_all(d);
}

/// The roots of a job: the CA file of the environment, then the first
/// system bundle with a certificate.
#[test]
fn the_roots_are_the_file_of_the_environment_and_a_system_bundle() {
    let d = dir("roots");
    let (env, empty, system) = (d.join("env.pem"), d.join("empty.pem"), d.join("system.pem"));
    let a = JobCa::new("a", Duration::from_secs(60)).unwrap();
    let b = JobCa::new("b", Duration::from_secs(60)).unwrap();
    std::fs::write(&env, a.pem()).unwrap();
    std::fs::write(&empty, "no certificate").unwrap();
    std::fs::write(&system, b.pem()).unwrap();
    let got = super::tls::roots_pem(Some(&env), &[empty.clone(), system]).unwrap();
    let got = String::from_utf8(got).unwrap();
    assert!(got.starts_with(a.pem()) && got.ends_with(b.pem()), "{got}");
    assert_eq!(super::tls::roots_pem(None, std::slice::from_ref(&empty)), None);
    let _ = std::fs::remove_dir_all(d);
}
