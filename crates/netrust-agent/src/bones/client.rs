//! Networked Bones Client.
//!
//! Provides synchronous HTTP/1.1 client communication with the NetRust
//! shared graveyard server using zero external HTTP dependencies.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;
use netrust_types::{BonesData, GraveRecord, GraveyardStats};

/// Client for communicating with the networked bones server.
#[derive(Debug, Clone)]
pub struct BonesClient {
    pub host_port: String,
    pub host_header: String,
}

impl BonesClient {
    /// Creates a new BonesClient pointing to the given base URL or host:port string.
    pub fn new(base_url: &str) -> Self {
        let trimmed = base_url
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .trim_end_matches('/');

        let host_port = if !trimmed.contains(':') {
            format!("{}:80", trimmed)
        } else {
            trimmed.to_string()
        };

        Self {
            host_header: trimmed.to_string(),
            host_port,
        }
    }

    /// Uploads a deceased adventurer's bones to the server and retrieves the gravestone memorial.
    pub fn upload_bones(&self, bones: &BonesData) -> Result<GraveRecord, String> {
        let body = serde_json::to_string(bones).map_err(|e| e.to_string())?;
        let (status, resp_body) = self.send_request("POST", "/api/v1/bones", Some(&body))?;

        if status == 200 || status == 201 {
            serde_json::from_str(&resp_body).map_err(|e| format!("Failed to parse GraveRecord: {}", e))
        } else {
            Err(format!("Upload failed with status {}: {}", status, resp_body))
        }
    }

    /// Fetches a bones file for the given dungeon depth, if one is available.
    pub fn fetch_bones(&self, depth: u32) -> Result<Option<BonesData>, String> {
        let path = format!("/api/v1/bones/{}", depth);
        let (status, resp_body) = self.send_request("GET", &path, None)?;

        if status == 200 {
            let bones: Option<BonesData> = serde_json::from_str(&resp_body)
                .map_err(|e| format!("Failed to parse BonesData: {}", e))?;
            Ok(bones)
        } else {
            Err(format!("Fetch bones failed with status {}: {}", status, resp_body))
        }
    }

    /// Fetches all registered gravestone memorials from the server.
    pub fn fetch_graves(&self) -> Result<Vec<GraveRecord>, String> {
        let (status, resp_body) = self.send_request("GET", "/api/v1/graves", None)?;

        if status == 200 {
            serde_json::from_str(&resp_body).map_err(|e| format!("Failed to parse graves: {}", e))
        } else {
            Err(format!("Fetch graves failed with status {}: {}", status, resp_body))
        }
    }

    /// Fetches a specific gravestone memorial by hero name.
    pub fn fetch_grave(&self, hero_name: &str) -> Result<Option<GraveRecord>, String> {
        let path = format!("/api/v1/graves/{}", hero_name);
        let (status, resp_body) = self.send_request("GET", &path, None)?;

        if status == 200 {
            let grave: Option<GraveRecord> = serde_json::from_str(&resp_body)
                .map_err(|e| format!("Failed to parse GraveRecord: {}", e))?;
            Ok(grave)
        } else if status == 404 {
            Ok(None)
        } else {
            Err(format!("Fetch grave failed with status {}: {}", status, resp_body))
        }
    }

    /// Fetches aggregate graveyard statistics from the server.
    pub fn fetch_stats(&self) -> Result<GraveyardStats, String> {
        let (status, resp_body) = self.send_request("GET", "/api/v1/stats", None)?;

        if status == 200 {
            serde_json::from_str(&resp_body).map_err(|e| format!("Failed to parse GraveyardStats: {}", e))
        } else {
            Err(format!("Fetch stats failed with status {}: {}", status, resp_body))
        }
    }

    /// Resets the remote graveyard state (useful for automated integration tests).
    pub fn reset(&self) -> Result<(), String> {
        let (status, _) = self.send_request("POST", "/api/v1/reset", None)?;
        if status == 200 {
            Ok(())
        } else {
            Err(format!("Reset failed with status {}", status))
        }
    }

    fn send_request(&self, method: &str, path: &str, body: Option<&str>) -> Result<(u16, String), String> {
        let mut stream = TcpStream::connect(&self.host_port)
            .map_err(|e| format!("Could not connect to bones server at {}: {}", self.host_port, e))?;

        stream.set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;

        let body_bytes = body.unwrap_or("");
        let content_length = body_bytes.as_bytes().len();

        let request = format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            method, path, self.host_header, content_length, body_bytes
        );

        stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
        stream.flush().map_err(|e| e.to_string())?;

        let mut reader = BufReader::new(stream);
        let mut status_line = String::new();
        reader.read_line(&mut status_line).map_err(|e| e.to_string())?;

        let status_code: u16 = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(500);

        // Read headers until blank line
        let mut content_len: Option<usize> = None;
        loop {
            let mut header_line = String::new();
            let bytes_read = reader.read_line(&mut header_line).map_err(|e| e.to_string())?;
            if bytes_read == 0 || header_line.trim().is_empty() {
                break;
            }
            if header_line.to_ascii_lowercase().starts_with("content-length:") {
                if let Some(val_str) = header_line.split(':').nth(1) {
                    content_len = val_str.trim().parse().ok();
                }
            }
        }

        // Read body
        let mut body_buf = Vec::new();
        if let Some(len) = content_len {
            body_buf.resize(len, 0);
            reader.read_exact(&mut body_buf).map_err(|e| e.to_string())?;
        } else {
            reader.read_to_end(&mut body_buf).map_err(|e| e.to_string())?;
        }

        let body_str = String::from_utf8_lossy(&body_buf).to_string();
        Ok((status_code, body_str))
    }
}
