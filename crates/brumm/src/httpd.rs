//! Just enough HTTP/1.1 to serve brumm's two loopback pages: the playback
//! page Chrome loads and the sign-in page the browser opens. One request
//! per connection, bounded in size and time.

use std::collections::HashMap;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct Request {
    pub method: String,
    /// The path, without the query.
    pub path: String,
    /// By lowercase name.
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

const MAX_HEAD: usize = 16 << 10;
const READ_HEAD: Duration = Duration::from_secs(5);

/// Reads one request, its body at most max_body bytes; None for anything
/// malformed, oversized or slow.
pub async fn read_request(s: &mut TcpStream, max_body: usize) -> Option<Request> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 4096];
    let head_end = tokio::time::timeout(READ_HEAD, async {
        loop {
            let n = s.read(&mut chunk).await.ok()?;
            if n == 0 {
                return None;
            }
            buf.extend_from_slice(&chunk[..n]);
            if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                return Some(i + 4);
            }
            if buf.len() > MAX_HEAD {
                return None;
            }
        }
    })
    .await
    .ok()??;
    let head = std::str::from_utf8(&buf[..head_end]).ok()?;
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, target) = (first.next()?.to_string(), first.next()?);
    let path = target.split('?').next().unwrap_or("").to_string();
    let mut headers = HashMap::new();
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    let len: usize = match headers.get("content-length") {
        Some(v) => v.parse().ok()?,
        None => 0,
    };
    if len > max_body {
        return None;
    }
    let mut body = buf[head_end..].to_vec();
    tokio::time::timeout(READ_HEAD, async {
        while body.len() < len {
            let n = s.read(&mut chunk).await.ok()?;
            if n == 0 {
                return None;
            }
            body.extend_from_slice(&chunk[..n]);
        }
        Some(())
    })
    .await
    .ok()??;
    body.truncate(len);
    Some(Request {
        method,
        path,
        headers,
        body,
    })
}

/// Answers and closes.
pub async fn respond(s: &mut TcpStream, status: u16, headers: &[(&str, &str)], body: &[u8]) {
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Status",
    };
    let mut head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    let _ = tokio::time::timeout(Duration::from_secs(5), async {
        s.write_all(head.as_bytes()).await?;
        s.write_all(body).await?;
        s.shutdown().await
    })
    .await;
}
