//! JSON-RPC 2.0 message parser and dispatcher for Model Context Protocol (MCP).

use netrust_data::roles::{RACES, ROLES};
use netrust_sim::SimulationWorld;
use serde_json::{json, Value};

use crate::commands::{action_args_from_json, parse_action, parse_character};
use crate::rpc::{
    self, error_response, result_response, RpcRequest, INVALID_PARAMS, METHOD_NOT_FOUND,
};
use crate::session::AgentSession;

/// Every action string `netrust_step` accepts; also used as the schema enum.
pub const STEP_ACTIONS: &[&str] = &[
    "move_north",
    "move_east",
    "move_south",
    "move_west",
    "move_northeast",
    "move_northwest",
    "move_southeast",
    "move_southwest",
    "wait",
    "pickup",
    "pay",
    "pray",
    "sacrifice",
    "eat",
    "cast",
    "ascend",
    "descend",
    "kick_north",
    "kick_east",
    "kick_south",
    "kick_west",
];

pub fn handle_mcp_request(session: &mut AgentSession, line: &str) -> Option<Value> {
    handle_mcp_line(session, Ok(line))
}

pub fn handle_mcp_line(session: &mut AgentSession, line: Result<&str, ()>) -> Option<Value> {
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

fn dispatch(
    session: &mut AgentSession,
    method: &str,
    params: &Value,
) -> Result<Value, (i64, String)> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "netrust-mcp", "version": "0.1.0" }
        })),
        "notifications/initialized" => Ok(Value::Null),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools_list()),
        "tools/call" => {
            let tool_name = params.get("name").and_then(|n| n.as_str()).ok_or((
                INVALID_PARAMS,
                "tools/call requires params.name".to_string(),
            ))?;
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let text = call_tool(session, tool_name, &args).map_err(|m| (INVALID_PARAMS, m))?;
            Ok(json!({ "content": [ { "type": "text", "text": text } ] }))
        }
        _ => Err((METHOD_NOT_FOUND, format!("Method '{method}' not found"))),
    }
}

fn tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "netrust_get_observation",
                "description": "Get the current structured game observation, player stats, visible monsters, and ASCII viewport.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "netrust_step",
                "description": "Execute a game action and return the next observation. 'sacrifice'/'eat'/'cast' take an optional 'index'; 'cast' takes an optional 'direction' (8 compass names).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": STEP_ACTIONS
                        },
                        "index": { "type": "integer" },
                        "direction": { "type": "string" }
                    },
                    "required": ["action"]
                }
            },
            {
                "name": "netrust_inspect_tile",
                "description": "Inspect tile properties and occupant at specific (x, y) coordinate.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "x": { "type": "integer" },
                        "y": { "type": "integer" }
                    },
                    "required": ["x", "y"]
                }
            },
            {
                "name": "netrust_render_map",
                "description": "Render the full 80x21 ASCII dungeon map as visible to the player.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "netrust_reset_game",
                "description": "Reset the game session with a new seed.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "seed": { "type": "integer" }
                    }
                }
            },
            {
                "name": "netrust_get_roles",
                "description": "Get available NetHack player roles, races, and their attributes.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "netrust_reset_with_character",
                "description": "Reset the game simulation with customized character role, race, and alignment.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "seed": { "type": "integer" },
                        "name": { "type": "string" },
                        "role": { "type": "string", "enum": ["valkyrie", "wizard", "barbarian", "rogue", "knight", "monk", "healer", "tourist", "archaeologist"] },
                        "race": { "type": "string", "enum": ["human", "elf", "dwarf", "gnome", "orc"] },
                        "gender": { "type": "string", "enum": ["male", "female"] },
                        "alignment": { "type": "string", "enum": ["lawful", "neutral", "chaotic"] }
                    }
                }
            }
        ]
    })
}

fn call_tool(session: &mut AgentSession, name: &str, args: &Value) -> Result<String, String> {
    match name {
        "netrust_get_observation" => {
            let obs = session.get_observation();
            Ok(serde_json::to_string_pretty(&obs).unwrap_or_default())
        }
        "netrust_step" => {
            let act_str = args
                .get("action")
                .and_then(|a| a.as_str())
                .ok_or("missing 'action'")?;
            let a = action_args_from_json(args, session.player_coord())?;
            let action = parse_action(act_str, &a)?;
            let obs = session.step(action);
            Ok(serde_json::to_string_pretty(&obs).unwrap_or_default())
        }
        "netrust_inspect_tile" => {
            let x = args
                .get("x")
                .and_then(|v| v.as_u64())
                .ok_or("missing integer 'x'")? as usize;
            let y = args
                .get("y")
                .and_then(|v| v.as_u64())
                .ok_or("missing integer 'y'")? as usize;
            let insp = session.inspect_tile(x, y)?;
            Ok(serde_json::to_string_pretty(&insp).unwrap_or_default())
        }
        "netrust_render_map" => Ok(crate::ascii::render_ascii_map(&session.world)),
        "netrust_reset_game" => {
            let seed = args.get("seed").and_then(|v| v.as_u64()).unwrap_or(42);
            *session = AgentSession::new(seed);
            let obs = session.get_observation();
            Ok(serde_json::to_string_pretty(&obs).unwrap_or_default())
        }
        "netrust_get_roles" => {
            let data = json!({
                "roles": ROLES.iter().map(|r| json!({
                    "id": format!("{:?}", r.id),
                    "name": r.name,
                    "base_hp": r.base_hp,
                    "ac": r.ac,
                    "speed": r.speed,
                    "default_alignment": format!("{:?}", r.default_alignment),
                })).collect::<Vec<_>>(),
                "races": RACES.iter().map(|r| json!({
                    "id": format!("{:?}", r.id),
                    "name": r.name,
                })).collect::<Vec<_>>()
            });
            Ok(serde_json::to_string_pretty(&data).unwrap_or_default())
        }
        "netrust_reset_with_character" => {
            let seed = args.get("seed").and_then(|v| v.as_u64()).unwrap_or(42);
            let name = args
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("Hero")
                .to_string();
            let field = |k: &str| args.get(k).and_then(|v| v.as_str());
            let config = parse_character(
                Some(name.as_str()),
                field("role"),
                field("race"),
                field("gender"),
                field("alignment"),
            )?;

            session.world = SimulationWorld::new_with_character(seed, config);
            session.last_events.clear();
            let obs = session.get_observation();
            Ok(serde_json::to_string_pretty(&obs).unwrap_or_default())
        }
        _ => Err(format!("Tool '{name}' not found")),
    }
}

/// Run Model Context Protocol server over standard I/O.
pub fn run_mcp_server(seed: u64) -> std::io::Result<()> {
    let mut session = AgentSession::new(seed);
    let stdin = std::io::stdin();
    crate::stdio::serve_lines(stdin.lock(), std::io::stdout(), |line| {
        handle_mcp_line(&mut session, line).map(|v| v.to_string())
    })
}
