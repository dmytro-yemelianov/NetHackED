//! JSON-RPC 2.0 standard protocol streaming over stdio for autonomous AI pairs.

use netrust_sim::{ActionAst, Direction};
use serde_json::{json, Value};

use crate::session::AgentSession;

pub fn handle_jsonrpc_request(session: &mut AgentSession, line: &str) -> Option<Value> {
    let req: Value = serde_json::from_str(line).ok()?;
    let id = req.get("id").cloned();
    let method = req.get("method")?.as_str()?;
    let params = req.get("params").cloned().unwrap_or(json!({}));

    let result = match method {
        "netrust.getObservation" => {
            let obs = session.get_observation();
            json!(obs)
        }
        "netrust.renderAscii" => {
            json!({ "ascii": crate::ascii::render_ascii_map(&session.world) })
        }
        "netrust.step" => {
            let action_str = params.get("action").and_then(|a| a.as_str()).unwrap_or("wait");
            let action = match action_str {
                "north" | "k" => ActionAst::Move(Direction::North),
                "east" | "l" => ActionAst::Move(Direction::East),
                "south" | "j" => ActionAst::Move(Direction::South),
                "west" | "h" => ActionAst::Move(Direction::West),
                "pickup" => ActionAst::PickUp,
                "pay" => ActionAst::Pay,
                "pray" => ActionAst::Pray,
                "descend" => ActionAst::Descend,
                "ascend" => ActionAst::Ascend,
                _ => ActionAst::Wait,
            };
            let obs = session.step(action);
            json!(obs)
        }
        "netrust.inspectTile" => {
            let x = params.get("x").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let y = params.get("y").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            match session.inspect_tile(x, y) {
                Ok(i) => json!(i),
                Err(e) => json!({ "error": e }),
            }
        }
        _ => return Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": format!("Method '{}' not found", method) }
        })),
    };

    Some(json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    }))
}

/// Run streaming JSON-RPC 2.0 loop over stdin/stdout.
pub fn run_jsonrpc_server(seed: u64) -> std::io::Result<()> {
    use std::io::{BufRead, Write};

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut session = AgentSession::new(seed);

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        if let Some(resp) = handle_jsonrpc_request(&mut session, &line) {
            let out_str = serde_json::to_string(&resp).unwrap_or_default();
            writeln!(stdout, "{}", out_str)?;
            stdout.flush()?;
        }
    }

    Ok(())
}
