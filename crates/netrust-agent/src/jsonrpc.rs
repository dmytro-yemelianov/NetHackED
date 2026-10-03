//! JSON-RPC 2.0 standard protocol streaming over stdio for autonomous AI pairs.

use netrust_sim::ActionAst;
use serde_json::{json, Value};

use crate::rpc::{
    self, error_response, parse_direction, result_response, RpcRequest, INVALID_PARAMS,
    METHOD_NOT_FOUND,
};
use crate::session::AgentSession;

pub fn handle_jsonrpc_request(session: &mut AgentSession, line: &str) -> Option<Value> {
    handle_jsonrpc_line(session, Ok(line))
}

pub fn handle_jsonrpc_line(session: &mut AgentSession, line: Result<&str, ()>) -> Option<Value> {
    let req = match rpc::parse_request(line) {
        Ok(r) => r,
        Err(err) => return Some(err),
    };
    let RpcRequest { id, method, params } = req;
    let reply = dispatch(session, &method, &params);
    // Notifications (no id) never get a response.
    let id = id?;
    Some(match reply {
        Ok(result) => result_response(id, result),
        Err((code, msg)) => error_response(id, code, msg),
    })
}

fn dispatch(session: &mut AgentSession, method: &str, params: &Value) -> Result<Value, (i64, String)> {
    match method {
        "netrust.getObservation" => Ok(json!(session.get_observation())),
        "netrust.renderAscii" => Ok(json!({ "ascii": crate::ascii::render_ascii_map(&session.world) })),
        "netrust.step" => {
            let action_str = params.get("action").and_then(|a| a.as_str()).unwrap_or("wait");
            let action = match action_str {
                "wait" => ActionAst::Wait,
                "pickup" => ActionAst::PickUp,
                "pay" => ActionAst::Pay,
                "pray" => ActionAst::Pray,
                "descend" => ActionAst::Descend,
                "ascend" => ActionAst::Ascend,
                other => parse_direction(other)
                    .map(ActionAst::Move)
                    .ok_or_else(|| (INVALID_PARAMS, format!("Unknown action '{}'", other)))?,
            };
            Ok(json!(session.step(action)))
        }
        "netrust.inspectTile" => {
            let x = params.get("x").and_then(|v| v.as_u64())
                .ok_or((INVALID_PARAMS, "missing integer 'x'".to_string()))? as usize;
            let y = params.get("y").and_then(|v| v.as_u64())
                .ok_or((INVALID_PARAMS, "missing integer 'y'".to_string()))? as usize;
            session.inspect_tile(x, y).map(|i| json!(i)).map_err(|e| (INVALID_PARAMS, e))
        }
        _ => Err((METHOD_NOT_FOUND, format!("Method '{}' not found", method))),
    }
}

/// Run streaming JSON-RPC 2.0 loop over stdin/stdout.
pub fn run_jsonrpc_server(seed: u64) -> std::io::Result<()> {
    let mut session = AgentSession::new(seed);
    let stdin = std::io::stdin();
    crate::stdio::serve_lines(stdin.lock(), std::io::stdout(), |line| {
        handle_jsonrpc_line(&mut session, line).map(|v| v.to_string())
    })
}
