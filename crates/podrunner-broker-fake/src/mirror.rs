//! A read-only registry for the engine of a guest (T-157). The engine of a
//! guest takes the broker as its mirror of Docker Hub
//! (`--registry-mirror`), so the runner's `docker pull NAME@sha256:HEX`
//! reads each pinned image from the store of the run, not from a registry.
//! A blob or a manifest that the mirror does not have answers 404, and the
//! engine then asks Docker Hub through the egress policy of the job.
//!
//! The directory is an OCI layout (`blobs/sha256/HEX`). The mirror serves
//! by digest only: a pinned image never needs a tag.

use std::io::Write;
use std::path::PathBuf;

/// The headers of an answer.
pub type Headers = Vec<(&'static str, String)>;

/// The blobs of the pinned images of a run.
pub struct Mirror {
    dir: PathBuf,
}

/// The hex of `sha256:HEX`, when it is a full digest.
fn hex_of(digest: &str) -> Option<&str> {
    let hex = digest.strip_prefix("sha256:")?;
    (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))).then_some(hex)
}

fn unknown(code: &str) -> (u16, Vec<u8>, Headers) {
    let body = format!("{{\"errors\":[{{\"code\":\"{code}\",\"message\":\"not in the store of the run\"}}]}}");
    (404, body.into_bytes(), vec![("Content-Type", "application/json".into())])
}

impl Mirror {
    pub fn new(dir: PathBuf) -> Mirror {
        Mirror { dir }
    }

    /// The answer to a request of the registry API, or `None` for a path
    /// outside `/v2/`.
    pub fn route(&self, method: &str, path: &str) -> Option<(u16, Vec<u8>, Headers)> {
        let rest = path.split('?').next()?.strip_prefix("/v2")?;
        if !rest.is_empty() && !rest.starts_with('/') {
            return None;
        }
        let api = ("Docker-Distribution-API-Version", "registry/2.0".to_string());
        if method != "GET" && method != "HEAD" {
            return Some((405, Vec::new(), vec![api]));
        }
        if rest.is_empty() || rest == "/" {
            return Some((200, b"{}".to_vec(), vec![api, ("Content-Type", "application/json".into())]));
        }
        let (kind, reference) = match (rest.rsplit_once("/manifests/"), rest.rsplit_once("/blobs/")) {
            (Some((_, r)), _) => ("manifest", r),
            (None, Some((_, r))) => ("blob", r),
            _ => return Some(unknown("NAME_UNKNOWN")),
        };
        let code = if kind == "manifest" { "MANIFEST_UNKNOWN" } else { "BLOB_UNKNOWN" };
        let Some(hex) = hex_of(reference) else { return Some(unknown(code)) };
        let Ok(body) = std::fs::read(self.dir.join("blobs/sha256").join(hex)) else { return Some(unknown(code)) };
        let kind_type = match kind {
            "manifest" => serde_json::from_slice::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v["mediaType"].as_str().map(str::to_string))
                .unwrap_or_else(|| "application/vnd.oci.image.manifest.v1+json".into()),
            _ => "application/octet-stream".into(),
        };
        let headers = vec![api, ("Content-Type", kind_type), ("Docker-Content-Digest", format!("sha256:{hex}"))];
        Some((200, body, headers))
    }
}

/// Write an answer of the mirror. A HEAD answer has the length of the body
/// and no body.
pub fn write(
    w: &mut impl Write,
    method: &str,
    status: u16,
    headers: &[(&str, String)],
    body: &[u8],
) -> std::io::Result<()> {
    if method != "HEAD" {
        return podrunner_hook::http::write_response(w, status, headers, body);
    }
    let reason = if status == 200 { "OK" } else { "Not Found" };
    let mut head = format!("HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    w.write_all(head.as_bytes())?;
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mirror_serves_blobs_and_manifests_by_digest() {
        let dir = std::env::temp_dir().join(format!("podrunner-mirror-{}", std::process::id()));
        let blobs = dir.join("blobs/sha256");
        std::fs::create_dir_all(&blobs).unwrap();
        let (m, l) = ("a".repeat(64), "b".repeat(64));
        std::fs::write(blobs.join(&m), r#"{"mediaType":"application/vnd.docker.distribution.manifest.v2+json"}"#)
            .unwrap();
        std::fs::write(blobs.join(&l), b"layer").unwrap();
        let mirror = Mirror::new(dir.clone());
        assert_eq!(mirror.route("GET", "/v2/").unwrap().0, 200);
        assert_eq!(mirror.route("GET", "/v2").unwrap().0, 200);
        let (s, body, h) = mirror.route("GET", &format!("/v2/library/busybox/manifests/sha256:{m}")).unwrap();
        assert_eq!(s, 200);
        assert!(body.starts_with(b"{"));
        assert!(h.contains(&("Content-Type", "application/vnd.docker.distribution.manifest.v2+json".into())));
        assert!(h.contains(&("Docker-Content-Digest", format!("sha256:{m}"))));
        let (s, body, _) = mirror.route("GET", &format!("/v2/library/busybox/blobs/sha256:{l}")).unwrap();
        assert_eq!((s, body.as_slice()), (200, b"layer".as_slice()));
        assert_eq!(mirror.route("GET", "/v2/library/busybox/manifests/latest").unwrap().0, 404, "no tag");
        assert_eq!(mirror.route("GET", "/v2/x/blobs/sha256:../../etc").unwrap().0, 404);
        assert_eq!(mirror.route("GET", &format!("/v2/x/blobs/sha256:{}", "c".repeat(64))).unwrap().0, 404);
        assert_eq!(mirror.route("PUT", "/v2/x/blobs/uploads/").unwrap().0, 405);
        assert!(mirror.route("GET", "/v2x").is_none() && mirror.route("GET", "/message").is_none());
        let mut out = Vec::new();
        write(&mut out, "HEAD", 200, &[], b"layer").unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains("Content-Length: 5\r\n") && out.ends_with("\r\n\r\n"), "{out}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
