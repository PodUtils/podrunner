//! A small HTTP/1.1 reader and writer for the control socket of a unit: one
//! request for each connection, a `Content-Length` body of at most
//! [`BODY_MAX`], no chunked body, and bounded lines.

use std::io::{BufRead, Read, Write};

use crate::input::BODY_MAX;

/// The longest request line and header line.
const LINE_MAX: usize = 8 * 1024;
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
}

/// Why a request is refused: the status and a short reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpError(pub u16, pub &'static str);

fn line(r: &mut impl BufRead) -> Result<String, HttpError> {
    let mut buf = Vec::new();
    let n = r.take(LINE_MAX as u64 + 1).read_until(b'\n', &mut buf).map_err(|_| HttpError(400, "a read failed"))?;
    if n > LINE_MAX || !buf.ends_with(b"\n") {
        return Err(HttpError(400, "a line is too long or has no end"));
    }
    let text = String::from_utf8(buf).map_err(|_| HttpError(400, "a line is not UTF-8"))?;
    Ok(text.trim_end_matches(['\r', '\n']).to_string())
}

/// Read one request.
pub fn read_request(r: &mut impl BufRead) -> Result<Request, HttpError> {
    read_request_with(r, BODY_MAX)
}

/// Read one request with a body of at most `max` bytes: the fake broker
/// takes the blocks of an artifact (8 MiB each from the blob client).
pub fn read_request_with(r: &mut impl BufRead, max: usize) -> Result<Request, HttpError> {
    let first = line(r)?;
    let mut parts = first.split(' ');
    let (Some(method), Some(path), Some(version), None) = (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(HttpError(400, "a bad request line"));
    };
    // The origin form `/PATH`, or the absolute form `http://HOST/PATH` of a
    // request through a proxy (the front of a guest of the local mode).
    let absolute = path.starts_with("http://") || path.starts_with("https://");
    if !matches!(version, "HTTP/1.1" | "HTTP/1.0") || !(path.starts_with('/') || absolute) {
        return Err(HttpError(400, "a bad request line"));
    }
    let mut length: Option<usize> = None;
    let mut range = None;
    for i in 0.. {
        let h = line(r)?;
        if h.is_empty() {
            break;
        }
        if i >= HEADERS_MAX {
            return Err(HttpError(431, "too many headers"));
        }
        let (name, value) = h.split_once(':').ok_or(HttpError(400, "a bad header"))?;
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => {
                if length.is_some() {
                    return Err(HttpError(400, "two lengths"));
                }
                length = Some(value.trim().parse().map_err(|_| HttpError(400, "a bad length"))?);
            }
            "transfer-encoding" => return Err(HttpError(501, "no chunked body")),
            "range" => range = Some(value.trim().to_string()),
            _ => {}
        }
    }
    let length = length.unwrap_or(0);
    if length > max {
        return Err(HttpError(413, "the body is too large"));
    }
    let mut body = vec![0; length];
    r.read_exact(&mut body).map_err(|_| HttpError(400, "the body is short"))?;
    Ok(Request { method: method.into(), path: path.into(), body, range })
}

/// Write one response, with extra headers, and close.
pub fn write_response(w: &mut impl Write, status: u16, headers: &[(&str, String)], body: &[u8]) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        206 => "Partial Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Content Too Large",
        416 => "Range Not Satisfiable",
        431 => "Request Header Fields Too Large",
        501 => "Not Implemented",
        _ => "Error",
    };
    let mut head = format!("HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    w.write_all(head.as_bytes())?;
    w.write_all(body)?;
    w.flush()
}
