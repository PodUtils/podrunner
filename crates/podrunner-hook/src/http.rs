//! A small HTTP/1.1 reader and writer for the control socket of a unit: one
//! request for each connection, a `Content-Length` body of at most
//! [`BODY_MAX`], and no chunked body. The head is read by
//! `pod_core::http1` (podcrates `v0.72.0`): each reader of PodUtils has the
//! same bounds and refuses the same heads.

use std::io::{BufRead, Write};

use pod_core::http1::{self, Bounds, MAX_HEAD, RequestError};

use crate::input::BODY_MAX;

/// The most header lines.
const HEADERS_MAX: usize = 64;

/// One request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub body: Vec<u8>,
    /// The `Range` header: the cache of the fake broker serves a part of a
    /// blob.
    pub range: Option<String>,
    /// The `Sec-WebSocket-Key` header: the live feed of the fake broker.
    pub ws_key: Option<String>,
    /// The `Authorization` header: the fake broker checks its tokens.
    pub authorization: Option<String>,
}

/// Why a request is refused: the status and a short reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpError(pub u16, pub &'static str);

impl From<RequestError> for HttpError {
    fn from(e: RequestError) -> HttpError {
        match e {
            RequestError::Gone(_) => HttpError(400, "the request has no end"),
            RequestError::TooLarge => HttpError(431, "the head is too large"),
            RequestError::Malformed { status: 505, .. } => HttpError(505, "another version of HTTP"),
            RequestError::Malformed { status, .. } => HttpError(status, "a bad request head"),
        }
    }
}

/// The bytes of the head, to its empty line and not a byte further: the
/// body stays in `r`.
fn head(r: &mut impl BufRead) -> Result<Vec<u8>, HttpError> {
    let bounds = Bounds { bytes: MAX_HEAD, lines: HEADERS_MAX + 1 };
    let mut buf = Vec::new();
    loop {
        let more = r.fill_buf().map_err(|_| HttpError(400, "a read failed"))?;
        if more.is_empty() {
            return Err(RequestError::Gone("end of stream".into()).into());
        }
        let (old, n) = (buf.len(), more.len());
        buf.extend_from_slice(more);
        if let Some((_, end)) = http1::find_request(&buf, bounds)? {
            r.consume(end - old);
            buf.truncate(end);
            return Ok(buf);
        }
        r.consume(n);
    }
}

/// Read one request.
pub fn read_request(r: &mut impl BufRead) -> Result<Request, HttpError> {
    read_request_with(r, BODY_MAX)
}

/// Read one request with a body of at most `max` bytes: the fake broker
/// takes the blocks of an artifact (8 MiB each from the blob client).
pub fn read_request_with(r: &mut impl BufRead, max: usize) -> Result<Request, HttpError> {
    let h = http1::parse_request(&head(r)?)?;
    // The origin form `/PATH`, or the absolute form `http://HOST/PATH` of a
    // request through a proxy (the front of a guest of the local mode).
    let path = &h.target;
    if !(path.starts_with('/') || path.starts_with("http://") || path.starts_with("https://")) {
        return Err(HttpError(400, "a bad request line"));
    }
    if h.has("transfer-encoding") {
        return Err(HttpError(501, "no chunked body"));
    }
    let mut lengths = h.headers.iter().filter(|(n, _)| n == "content-length").map(|(_, v)| v);
    let length = match (lengths.next(), lengths.next()) {
        (None, _) => 0,
        (Some(v), None) => v.parse().map_err(|_| HttpError(400, "a bad length"))?,
        (Some(_), Some(_)) => return Err(HttpError(400, "two lengths")),
    };
    if length > max {
        return Err(HttpError(413, "the body is too large"));
    }
    let mut body = vec![0; length];
    r.read_exact(&mut body).map_err(|_| HttpError(400, "the body is short"))?;
    let field = |n: &str| h.header(n).map(str::to_string);
    Ok(Request {
        method: h.method.clone(),
        path: h.target.clone(),
        body,
        range: field("range"),
        ws_key: field("sec-websocket-key"),
        authorization: field("authorization"),
    })
}

/// Write one response, with extra headers, and close.
pub fn write_response(w: &mut impl Write, status: u16, headers: &[(&str, String)], body: &[u8]) -> std::io::Result<()> {
    // The phrase of 200 in `pod_core::http1` is the one of a CONNECT.
    let reason = if status == 200 { "OK" } else { http1::reason(status) };
    let mut head = format!("HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    w.write_all(head.as_bytes())?;
    w.write_all(body)?;
    w.flush()
}
