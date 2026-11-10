//! The little HTTP/1.1 the remote-control page needs: requests with a
//! `Content-Length` body (no chunked bodies), read with a limit on every
//! part and a deadline, so a slow, huge or odd request cannot hold the
//! server; and responses with the page's safety headers on every one.

use std::io::{self, Read, Write};
use std::time::Instant;

/// The longest request line and headers taken.
pub const MAX_HEAD: usize = 8 * 1024;
/// The longest body taken: a link and a few fields.
pub const MAX_BODY: usize = 16 * 1024;

/// Sent with every response. Nothing is cached or framed, scripts and
/// styles come only from the page's own address, and it talks to nothing
/// else. No `Access-Control-Allow-Origin`: other sites' pages cannot read
/// an answer.
const SAFETY_HEADERS: &str = "Cache-Control: no-store\r\n\
Pragma: no-cache\r\n\
X-Content-Type-Options: nosniff\r\n\
X-Frame-Options: DENY\r\n\
Referrer-Policy: no-referrer\r\n\
Content-Security-Policy: default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; \
connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'\r\n\
Cross-Origin-Opener-Policy: same-origin\r\n\
Cross-Origin-Resource-Policy: same-origin\r\n\
Permissions-Policy: camera=(), microphone=(), geolocation=()\r\n";

/// A request as read.
#[derive(Debug)]
pub struct Request {
    pub method: String,
    /// Without the query.
    pub path: String,
    /// Names in lower case.
    headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// Whether the client keeps the connection for another request.
    pub keep_alive: bool,
}

impl Request {
    /// The value of header `name` (lower case), if sent.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }
}

/// Why no request was read.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    /// The client closed the connection between requests.
    #[error("closed")]
    Closed,
    #[error("timed out")]
    TimedOut,
    #[error("request too large")]
    TooLarge,
    #[error("bad request: {0}")]
    Bad(&'static str),
    #[error(transparent)]
    Io(io::Error),
}

/// Read one request from `stream`, keeping in `buffer` what came after it
/// (a pipelined request). Each read waits as the stream's timeout says;
/// past `deadline` the request is given up.
pub fn read_request(stream: &mut impl Read, buffer: &mut Vec<u8>, deadline: Instant) -> Result<Request, ReadError> {
    let head_end = loop {
        if let Some(at) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
        if buffer.len() > MAX_HEAD {
            return Err(ReadError::TooLarge);
        }
        fill(stream, buffer, deadline, true)?;
    };
    if head_end > MAX_HEAD {
        return Err(ReadError::TooLarge);
    }
    let head = std::str::from_utf8(&buffer[..head_end]).map_err(|_| ReadError::Bad("the head is not text"))?;
    let mut lines = head.split("\r\n");
    let mut start = lines.next().unwrap_or("").split(' ');
    let (method, target, version) = match (start.next(), start.next(), start.next(), start.next()) {
        (Some(m), Some(t), Some(v), None) if !m.is_empty() && t.starts_with('/') => (m, t, v),
        _ => return Err(ReadError::Bad("no request line")),
    };
    if version != "HTTP/1.1" && version != "HTTP/1.0" {
        return Err(ReadError::Bad("not HTTP/1.x"));
    }
    let mut headers = Vec::new();
    for line in lines.filter(|l| !l.is_empty()) {
        let (name, value) = line.split_once(':').ok_or(ReadError::Bad("a header without a colon"))?;
        if name.is_empty() || name.contains(' ') {
            return Err(ReadError::Bad("a bad header name"));
        }
        headers.push((name.to_ascii_lowercase(), value.trim().to_string()));
    }
    let find = |n: &str| headers.iter().find(|(name, _)| name == n).map(|(_, v)| v.as_str());
    if find("transfer-encoding").is_some() {
        return Err(ReadError::Bad("chunked bodies are not taken"));
    }
    if headers.iter().filter(|(n, _)| n == "content-length").count() > 1 {
        return Err(ReadError::Bad("two lengths"));
    }
    let length = match find("content-length") {
        Some(n) => n.parse::<usize>().map_err(|_| ReadError::Bad("a bad length"))?,
        None => 0,
    };
    if length > MAX_BODY {
        return Err(ReadError::TooLarge);
    }
    let connection = find("connection").map(str::to_ascii_lowercase);
    let keep_alive = match version {
        "HTTP/1.1" => connection.as_deref() != Some("close"),
        _ => connection.as_deref() == Some("keep-alive"),
    };
    let path = target.split(['?', '#']).next().unwrap_or(target).to_string();
    let method = method.to_string();
    while buffer.len() < head_end + length {
        fill(stream, buffer, deadline, false)?;
    }
    let body = buffer[head_end..head_end + length].to_vec();
    buffer.drain(..head_end + length);
    Ok(Request { method, path, headers, body, keep_alive })
}

/// Read more of `stream` into `buffer`.
fn fill(stream: &mut impl Read, buffer: &mut Vec<u8>, deadline: Instant, between: bool) -> Result<(), ReadError> {
    if Instant::now() > deadline {
        return Err(ReadError::TimedOut);
    }
    let mut chunk = [0u8; 4096];
    match stream.read(&mut chunk) {
        Ok(0) if between && buffer.is_empty() => Err(ReadError::Closed),
        Ok(0) => Err(ReadError::Bad("the connection closed mid-request")),
        Ok(n) => {
            buffer.extend_from_slice(&chunk[..n]);
            Ok(())
        }
        Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => Err(ReadError::TimedOut),
        Err(e) if e.kind() == io::ErrorKind::Interrupted => Ok(()),
        Err(e) => Err(ReadError::Io(e)),
    }
}

/// A response to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
    /// Seconds to wait, for `429 Too Many Requests`.
    pub retry_after: Option<u64>,
}

impl Response {
    pub fn new(status: u16, content_type: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Response { status, content_type, body: body.into(), retry_after: None }
    }

    /// A JSON answer.
    pub fn json(status: u16, value: &serde_json::Value) -> Self {
        Response::new(status, "application/json", value.to_string())
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}

/// Send `response`, saying whether the connection stays open. A `HEAD`
/// request gets the headers only.
pub fn write_response(stream: &mut impl Write, response: &Response, keep_alive: bool, head: bool) -> io::Result<()> {
    let mut out = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n{SAFETY_HEADERS}Connection: {}\r\n",
        response.status,
        reason(response.status),
        response.content_type,
        response.body.len(),
        if keep_alive { "keep-alive" } else { "close" },
    );
    if let Some(wait) = response.retry_after {
        out.push_str(&format!("Retry-After: {wait}\r\n"));
    }
    out.push_str("\r\n");
    let mut bytes = out.into_bytes();
    if !head {
        bytes.extend_from_slice(&response.body);
    }
    stream.write_all(&bytes)?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn read(raw: &[u8]) -> Result<Request, ReadError> {
        let mut buffer = Vec::new();
        read_request(&mut &raw[..], &mut buffer, Instant::now() + Duration::from_secs(5))
    }

    #[test]
    fn a_request_has_its_method_path_headers_and_body() {
        let raw = b"POST /api/add?x=1 HTTP/1.1\r\nHost: 10.0.0.2:18443\r\nContent-Type: application/json\r\nContent-Length: 9\r\n\r\n{\"a\":\"b\"}";
        let req = read(raw).unwrap();
        assert_eq!((req.method.as_str(), req.path.as_str()), ("POST", "/api/add"));
        assert_eq!(req.header("host"), Some("10.0.0.2:18443"));
        assert_eq!(req.header("content-type"), Some("application/json"));
        assert_eq!(req.body, b"{\"a\":\"b\"}");
        assert!(req.keep_alive);
    }

    #[test]
    fn pipelined_requests_are_read_in_turn() {
        let raw = b"GET / HTTP/1.1\r\n\r\nGET /app.js HTTP/1.1\r\nConnection: close\r\n\r\n";
        let mut buffer = Vec::new();
        let mut stream = &raw[..];
        let deadline = Instant::now() + Duration::from_secs(5);
        assert_eq!(read_request(&mut stream, &mut buffer, deadline).unwrap().path, "/");
        let second = read_request(&mut stream, &mut buffer, deadline).unwrap();
        assert_eq!(second.path, "/app.js");
        assert!(!second.keep_alive);
        assert!(matches!(read_request(&mut stream, &mut buffer, deadline), Err(ReadError::Closed)));
    }

    #[test]
    fn odd_or_huge_requests_are_refused() {
        assert!(matches!(read(b"GET / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n"), Err(ReadError::Bad(_))));
        assert!(matches!(read(b"GET http://x/ HTTP/1.1\r\n\r\n"), Err(ReadError::Bad(_))));
        assert!(matches!(read(b"GET / SPDY/3\r\n\r\n"), Err(ReadError::Bad(_))));
        assert!(matches!(
            read(b"POST / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nab"),
            Err(ReadError::Bad(_))
        ));
        let big = format!("POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n", MAX_BODY + 1);
        assert!(matches!(read(big.as_bytes()), Err(ReadError::TooLarge)));
        let long = format!("GET / HTTP/1.1\r\nX: {}\r\n\r\n", "a".repeat(MAX_HEAD));
        assert!(matches!(read(long.as_bytes()), Err(ReadError::TooLarge)));
        assert!(matches!(read(b"POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\nab"), Err(ReadError::Bad(_))));
        let mut buffer = Vec::new();
        let late = read_request(&mut &b"GET"[..], &mut buffer, Instant::now() - Duration::from_secs(1));
        assert!(matches!(late, Err(ReadError::TimedOut)));
    }

    #[test]
    fn every_response_carries_the_safety_headers_and_no_cors() {
        let mut out = Vec::new();
        let response = Response { retry_after: Some(30), ..Response::new(429, "application/json", "{}") };
        write_response(&mut out, &response, false, false).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("HTTP/1.1 429 Too Many Requests\r\n"));
        for header in [
            "Cache-Control: no-store",
            "X-Content-Type-Options: nosniff",
            "frame-ancestors 'none'",
            "X-Frame-Options: DENY",
            "Retry-After: 30",
            "Connection: close",
            "Content-Length: 2\r\n",
        ] {
            assert!(text.contains(header), "{header}");
        }
        assert!(!text.to_ascii_lowercase().contains("access-control"));
        assert!(text.ends_with("\r\n\r\n{}"));
    }
}
