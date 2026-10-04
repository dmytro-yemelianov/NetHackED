//! Networked Bones Client.
//!
//! Provides synchronous HTTP/1.1 client communication with the NetRust
//! shared graveyard server using zero external HTTP dependencies.

use netrust_types::{BonesData, GraveRecord, GraveyardStats};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

pub const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_LINE_BYTES: usize = 8 * 1024;

/// Client for communicating with the networked bones server.
#[derive(Debug, Clone)]
pub struct BonesClient {
    pub host_port: String,
    pub host_header: String,
    pub tls_requested: bool,
}

impl BonesClient {
    /// Creates a new BonesClient pointing to the given base URL or host:port string.
    pub fn new(base_url: &str) -> Self {
        let tls_requested = base_url.starts_with("https://");

        let trimmed = base_url
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .trim_end_matches('/');

        let host_port = if !trimmed.contains(':') {
            format!("{trimmed}:80")
        } else {
            trimmed.to_string()
        };

        Self {
            host_header: trimmed.to_string(),
            host_port,
            tls_requested,
        }
    }

    /// Uploads a deceased adventurer's bones to the server and retrieves the gravestone memorial.
    pub fn upload_bones(&self, bones: &BonesData) -> Result<GraveRecord, String> {
        let body = serde_json::to_string(bones).map_err(|e| e.to_string())?;
        let (status, resp_body) = self.send_request("POST", "/api/v1/bones", Some(&body))?;

        if status == 200 || status == 201 {
            serde_json::from_str(&resp_body)
                .map_err(|e| format!("Failed to parse GraveRecord: {e}"))
        } else {
            Err(format!("Upload failed with status {status}: {resp_body}"))
        }
    }

    /// Fetches a bones file for the given dungeon depth, if one is available.
    pub fn fetch_bones(&self, depth: u32) -> Result<Option<BonesData>, String> {
        let path = format!("/api/v1/bones/{depth}");
        let (status, resp_body) = self.send_request("GET", &path, None)?;

        if status == 200 {
            let bones: Option<BonesData> = serde_json::from_str(&resp_body)
                .map_err(|e| format!("Failed to parse BonesData: {e}"))?;
            Ok(bones)
        } else {
            Err(format!(
                "Fetch bones failed with status {status}: {resp_body}"
            ))
        }
    }

    /// Fetches all registered gravestone memorials from the server.
    pub fn fetch_graves(&self) -> Result<Vec<GraveRecord>, String> {
        let (status, resp_body) = self.send_request("GET", "/api/v1/graves", None)?;

        if status == 200 {
            serde_json::from_str(&resp_body).map_err(|e| format!("Failed to parse graves: {e}"))
        } else {
            Err(format!(
                "Fetch graves failed with status {status}: {resp_body}"
            ))
        }
    }

    /// Fetches a specific gravestone memorial by hero name.
    pub fn fetch_grave(&self, hero_name: &str) -> Result<Option<GraveRecord>, String> {
        let path = format!("/api/v1/graves/{}", encode_path_segment(hero_name));
        let (status, resp_body) = self.send_request("GET", &path, None)?;

        if status == 200 {
            let grave: Option<GraveRecord> = serde_json::from_str(&resp_body)
                .map_err(|e| format!("Failed to parse GraveRecord: {e}"))?;
            Ok(grave)
        } else if status == 404 {
            Ok(None)
        } else {
            Err(format!(
                "Fetch grave failed with status {status}: {resp_body}"
            ))
        }
    }

    /// Fetches aggregate graveyard statistics from the server.
    pub fn fetch_stats(&self) -> Result<GraveyardStats, String> {
        let (status, resp_body) = self.send_request("GET", "/api/v1/stats", None)?;

        if status == 200 {
            serde_json::from_str(&resp_body)
                .map_err(|e| format!("Failed to parse GraveyardStats: {e}"))
        } else {
            Err(format!(
                "Fetch stats failed with status {status}: {resp_body}"
            ))
        }
    }

    /// Resets the remote graveyard state (useful for automated integration tests).
    pub fn reset(&self) -> Result<(), String> {
        let (status, _) = self.send_request("POST", "/api/v1/reset", None)?;
        if status == 200 {
            Ok(())
        } else {
            Err(format!("Reset failed with status {status}"))
        }
    }

    fn send_request(
        &self,
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> Result<(u16, String), String> {
        if self.tls_requested {
            return Err("https not supported by BonesClient; use http://".into());
        }

        let mut stream = TcpStream::connect(&self.host_port).map_err(|e| {
            format!(
                "Could not connect to bones server at {}: {}",
                self.host_port, e
            )
        })?;

        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;

        let body_bytes = body.unwrap_or("");
        let content_length = body_bytes.len();

        let request = format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            method, path, self.host_header, content_length, body_bytes
        );

        stream
            .write_all(request.as_bytes())
            .map_err(|e| e.to_string())?;
        stream.flush().map_err(|e| e.to_string())?;

        let mut reader = BufReader::new(stream);
        parse_response(&mut reader)
    }
}

/// Percent-encodes everything except RFC 3986 unreserved characters.
pub fn encode_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn read_limited_line<R: BufRead>(r: &mut R, max: usize) -> Result<String, String> {
    let mut buf = Vec::new();
    r.by_ref()
        .take(max as u64 + 1)
        .read_until(b'\n', &mut buf)
        .map_err(|e| e.to_string())?;
    if buf.len() > max {
        return Err("response line too long".into());
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn parse_response<R: BufRead>(reader: &mut R) -> Result<(u16, String), String> {
    let status_line = read_limited_line(reader, MAX_LINE_BYTES)?;
    let status_code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(500);
    let mut content_len: Option<usize> = None;
    loop {
        let header_line = read_limited_line(reader, MAX_LINE_BYTES)?;
        if header_line.trim().is_empty() {
            break;
        }
        if header_line
            .to_ascii_lowercase()
            .starts_with("content-length:")
        {
            content_len = header_line
                .split(':')
                .nth(1)
                .and_then(|v| v.trim().parse().ok());
        }
    }
    let mut body_buf = Vec::new();
    match content_len {
        Some(len) if len > MAX_BODY_BYTES => {
            return Err(format!("response body too large: {len} bytes"))
        }
        Some(len) => {
            body_buf.resize(len, 0);
            reader
                .read_exact(&mut body_buf)
                .map_err(|e| e.to_string())?;
        }
        None => {
            reader
                .by_ref()
                .take(MAX_BODY_BYTES as u64 + 1)
                .read_to_end(&mut body_buf)
                .map_err(|e| e.to_string())?;
            if body_buf.len() > MAX_BODY_BYTES {
                return Err("response body too large".into());
            }
        }
    }
    Ok((status_code, String::from_utf8_lossy(&body_buf).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn encodes_path_segments() {
        assert_eq!(encode_path_segment("Sir Lancelot"), "Sir%20Lancelot");
        assert_eq!(encode_path_segment("a/b\r\n"), "a%2Fb%0D%0A");
        assert_eq!(
            encode_path_segment("Тарас"),
            "%D0%A2%D0%B0%D1%80%D0%B0%D1%81"
        );
        assert_eq!(encode_path_segment("ok-_.~9"), "ok-_.~9");
    }

    #[test]
    fn https_is_refused_not_downgraded() {
        let c = BonesClient::new("https://example.com");
        assert!(c.tls_requested);
        let err = c.fetch_stats().unwrap_err();
        assert!(err.contains("https"));
    }

    #[test]
    fn limited_line_rejects_overlong() {
        let mut r = Cursor::new(vec![b'a'; MAX_LINE_BYTES + 10]);
        assert!(read_limited_line(&mut r, MAX_LINE_BYTES).is_err());
        let mut r = Cursor::new(b"HTTP/1.1 200 OK\r\n".to_vec());
        assert_eq!(
            read_limited_line(&mut r, MAX_LINE_BYTES).unwrap(),
            "HTTP/1.1 200 OK\r\n"
        );
    }

    #[test]
    fn oversized_content_length_rejected() {
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY_BYTES + 1
        );
        let err = parse_response(&mut Cursor::new(resp.into_bytes())).unwrap_err();
        assert!(err.contains("too large"));
    }
}
