//! NetRust JSON Streaming Agent Line-Protocol.
//!
//! Feeds continuous structured JSON observations to autonomous agents,
//! bash pipes, or RL environments.

use netrust_agent::commands::parse_action;
use netrust_agent::mcp::action_args;
use netrust_agent::stdio::serve_lines;
use netrust_agent::AgentSession;
use netrust_sim::Coord;
use serde_json::{json, Value};
use std::io::{self, Write};

fn main() -> io::Result<()> {
    let mut session = AgentSession::new(42);
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    // Print initial observation
    let init_obs = session.get_observation();
    writeln!(stdout, "{}", serde_json::to_string(&init_obs)?)?;
    stdout.flush()?;

    serve_lines(stdin.lock(), stdout, |line| {
        let Ok(trimmed) = line else {
            return Some(json!({ "error": "Invalid UTF-8" }).to_string());
        };
        let cmd: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => return Some(json!({ "error": format!("Invalid JSON: {}", e) }).to_string()),
        };

        let action_name = cmd.get("action").and_then(|a| a.as_str()).unwrap_or("wait");
        let obs = if action_name == "get_state" {
            session.get_observation()
        } else {
            let player = session
                .world
                .arena
                .actors
                .get(session.world.player_id)
                .map(|p| p.coord)
                .unwrap_or(Coord::new_unchecked(0, 0));
            let args = match action_args(&cmd, player) {
                Ok(a) => a,
                Err(e) => return Some(json!({ "error": e }).to_string()),
            };
            match parse_action(action_name, &args) {
                Ok(action) => session.step(action),
                Err(e) => return Some(json!({ "error": e }).to_string()),
            }
        };
        Some(serde_json::to_string(&obs).unwrap_or_default())
    })
}
