//! Shared JSON-RPC 2.0 request parsing and response builders for stdio servers.

use netrust_sim::Direction;
use serde_json::{json, Value};

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;

/// A structurally valid JSON-RPC request. `id == None` means notification.
#[derive(Debug, Clone)]
pub struct RpcRequest {
    pub id: Option<Value>,
    pub method: String,
    pub params: Value,
}

pub fn error_response(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message.into() } })
}

pub fn result_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// Parse one line. `Err(())` input means the line was not valid UTF-8.
/// On failure returns a ready-to-send error response.
pub fn parse_request(line: Result<&str, ()>) -> Result<RpcRequest, Value> {
    let text = line.map_err(|_| error_response(Value::Null, PARSE_ERROR, "Parse error: invalid UTF-8"))?;
    let req: Value = serde_json::from_str(text)
        .map_err(|e| error_response(Value::Null, PARSE_ERROR, format!("Parse error: {}", e)))?;
    let Some(obj) = req.as_object() else {
        return Err(error_response(Value::Null, INVALID_REQUEST, "Invalid Request: expected object"));
    };
    let id = obj.get("id").cloned();
    let Some(method) = obj.get("method").and_then(|m| m.as_str()) else {
        return Err(error_response(id.unwrap_or(Value::Null), INVALID_REQUEST, "Invalid Request: missing method"));
    };
    Ok(RpcRequest {
        id,
        method: method.to_string(),
        params: obj.get("params").cloned().unwrap_or_else(|| json!({})),
    })
}

/// Parse a compass direction name or vi-key.
pub fn parse_direction(s: &str) -> Option<Direction> {
    match s.to_lowercase().as_str() {
        "north" | "k" => Some(Direction::North),
        "south" | "j" => Some(Direction::South),
        "east" | "l" => Some(Direction::East),
        "west" | "h" => Some(Direction::West),
        "northeast" | "u" => Some(Direction::NorthEast),
        "northwest" | "y" => Some(Direction::NorthWest),
        "southeast" | "n" => Some(Direction::SouthEast),
        "southwest" | "b" => Some(Direction::SouthWest),
        _ => None,
    }
}
