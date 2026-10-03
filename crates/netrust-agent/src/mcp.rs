//! JSON-RPC 2.0 message parser and dispatcher for Model Context Protocol (MCP).

use netrust_data::roles::{CharacterConfig, Gender, RaceId, RoleId, ROLES, RACES};
use netrust_sim::{ActionAst, Coord, Direction, SimulationWorld};
use netrust_types::Alignment;
use serde_json::{json, Value};

use crate::session::AgentSession;

pub fn handle_mcp_request(session: &mut AgentSession, line: &str) -> Option<Value> {
    let req: Value = serde_json::from_str(line).ok()?;
    let id = req.get("id").cloned();
    let method = req.get("method")?.as_str()?;

    match method {
        "initialize" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "netrust-mcp",
                    "version": "0.1.0"
                }
            }
        })),
        "notifications/initialized" => None,
        "tools/list" => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
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
                        "description": "Execute a game action (Move, OpenDoor, CloseDoor, Kick, MeleeAttack, ZapWand, Wait) and return the next observation.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "action": {
                                    "type": "string",
                                    "enum": [
                                        "move_north", "move_east", "move_south", "move_west",
                                        "move_northeast", "move_northwest", "move_southeast", "move_southwest",
                                        "wait", "pickup", "pay", "pray", "sacrifice",
                                        "kick_east", "kick_west", "kick_north", "kick_south"
                                    ]
                                }
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
            }
        })),
        "tools/call" => {
            let params = req.get("params")?;
            let tool_name = params.get("name")?.as_str()?;
            let args = params.get("arguments").cloned().unwrap_or(json!({}));

            let content_str = match tool_name {
                "netrust_get_observation" => {
                    let obs = session.get_observation();
                    serde_json::to_string_pretty(&obs).unwrap_or_default()
                }
                "netrust_step" => {
                    let act_str = args.get("action").and_then(|a| a.as_str()).unwrap_or("wait");
                    let p_coord = session.world.arena.actors.get(session.world.player_id).map(|p| p.coord).unwrap_or(Coord::new_unchecked(0, 0));
                    let action = match act_str {
                        "move_north" => ActionAst::Move(Direction::North),
                        "move_east" => ActionAst::Move(Direction::East),
                        "move_south" => ActionAst::Move(Direction::South),
                        "move_west" => ActionAst::Move(Direction::West),
                        "move_northeast" => ActionAst::Move(Direction::NorthEast),
                        "move_northwest" => ActionAst::Move(Direction::NorthWest),
                        "move_southeast" => ActionAst::Move(Direction::SouthEast),
                        "move_southwest" => ActionAst::Move(Direction::SouthWest),
                        "pickup" => ActionAst::PickUp,
                        "pay" => ActionAst::Pay,
                        "pray" => ActionAst::Pray,
                        "sacrifice" => ActionAst::Sacrifice(args.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize),
                        "eat" => ActionAst::Eat(args.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize),
                        "cast" => {
                            let s_idx = args.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                            let cast_dir = match args.get("direction").and_then(|v| v.as_str()) {
                                Some("north") => Direction::North,
                                Some("south") => Direction::South,
                                Some("west") => Direction::West,
                                _ => Direction::East,
                            };
                            ActionAst::Cast { spell_index: s_idx, dir: cast_dir }
                        }
                        "ascend" => ActionAst::Ascend,
                        "descend" => ActionAst::Descend,
                        "kick_east" => ActionAst::Kick(Coord::new_unchecked(p_coord.x + 1, p_coord.y)),
                        "kick_west" => ActionAst::Kick(Coord::new_unchecked(p_coord.x.saturating_sub(1), p_coord.y)),
                        "kick_north" => ActionAst::Kick(Coord::new_unchecked(p_coord.x, p_coord.y.saturating_sub(1))),
                        "kick_south" => ActionAst::Kick(Coord::new_unchecked(p_coord.x, p_coord.y + 1)),
                        _ => ActionAst::Wait,
                    };
                    let obs = session.step(action);
                    serde_json::to_string_pretty(&obs).unwrap_or_default()
                }
                "netrust_inspect_tile" => {
                    let x = args.get("x").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    let y = args.get("y").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                    match session.inspect_tile(x, y) {
                        Ok(i) => serde_json::to_string_pretty(&i).unwrap_or_default(),
                        Err(e) => e,
                    }
                }
                "netrust_render_map" => {
                    crate::ascii::render_ascii_map(&session.world)
                }
                "netrust_reset_game" => {
                    let seed = args.get("seed").and_then(|v| v.as_u64()).unwrap_or(42);
                    *session = AgentSession::new(seed);
                    let obs = session.get_observation();
                    serde_json::to_string_pretty(&obs).unwrap_or_default()
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
                    serde_json::to_string_pretty(&data).unwrap_or_default()
                }
                "netrust_reset_with_character" => {
                    let seed = args.get("seed").and_then(|v| v.as_u64()).unwrap_or(42);
                    let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("Hero").to_string();
                    let role_str = args.get("role").and_then(|v| v.as_str()).unwrap_or("valkyrie");
                    let race_str = args.get("race").and_then(|v| v.as_str()).unwrap_or("human");
                    let gender_str = args.get("gender").and_then(|v| v.as_str()).unwrap_or("female");
                    let align_str = args.get("alignment").and_then(|v| v.as_str()).unwrap_or("neutral");

                    let role_id = match role_str.to_lowercase().as_str() {
                        "wizard" => RoleId::Wizard,
                        "barbarian" => RoleId::Barbarian,
                        "rogue" => RoleId::Rogue,
                        "knight" => RoleId::Knight,
                        "monk" => RoleId::Monk,
                        "healer" => RoleId::Healer,
                        "tourist" => RoleId::Tourist,
                        "archaeologist" => RoleId::Archaeologist,
                        _ => RoleId::Valkyrie,
                    };

                    let race_id = match race_str.to_lowercase().as_str() {
                        "elf" => RaceId::Elf,
                        "dwarf" => RaceId::Dwarf,
                        "gnome" => RaceId::Gnome,
                        "orc" => RaceId::Orc,
                        _ => RaceId::Human,
                    };

                    let gender = if gender_str.to_lowercase() == "male" { Gender::Male } else { Gender::Female };
                    let alignment = match align_str.to_lowercase().as_str() {
                        "lawful" => Alignment::Lawful,
                        "chaotic" => Alignment::Chaotic,
                        _ => Alignment::Neutral,
                    };

                    let config = CharacterConfig {
                        name,
                        role: role_id,
                        race: race_id,
                        gender,
                        alignment,
                    };

                    session.world = SimulationWorld::new_with_character(seed, config);
                    session.last_events.clear();
                    let obs = session.get_observation();
                    serde_json::to_string_pretty(&obs).unwrap_or_default()
                }
                _ => return Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("Tool '{}' not found", tool_name)
                    }
                })),
            };

            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [
                        {
                            "type": "text",
                            "text": content_str
                        }
                    ]
                }
            }))
        }
        _ => Some(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": format!("Method '{}' not found", method)
            }
        })),
    }
}

/// Run Model Context Protocol server over standard I/O.
pub fn run_mcp_server(seed: u64) -> std::io::Result<()> {
    use std::io::{BufRead, Write};

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut session = AgentSession::new(seed);

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        if let Some(resp) = handle_mcp_request(&mut session, &line) {
            let out_str = serde_json::to_string(&resp).unwrap_or_default();
            writeln!(stdout, "{}", out_str)?;
            stdout.flush()?;
        }
    }

    Ok(())
}
