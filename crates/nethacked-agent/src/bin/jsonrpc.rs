//! NetHackED JSON Streaming Agent Line-Protocol.
//!
//! Feeds continuous structured JSON observations to autonomous agents,
//! bash pipes, or RL environments.

use nethacked_agent::commands::{action_args_from_json, parse_action};
use nethacked_agent::stdio::serve_lines;
use nethacked_agent::AgentSession;
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
            let args = match action_args_from_json(&cmd, session.player_coord()) {
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
