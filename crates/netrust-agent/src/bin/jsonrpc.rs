//! NetRust JSON Streaming Agent Line-Protocol.
//!
//! Feeds continuous structured JSON observations to autonomous agents,
//! bash pipes, or RL environments.

use netrust_agent::AgentSession;
use netrust_sim::{ActionAst, Direction};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

fn main() -> io::Result<()> {
    let mut session = AgentSession::new(42);
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    // Print initial observation
    let init_obs = session.get_observation();
    writeln!(stdout, "{}", serde_json::to_string(&init_obs)?)?;
    stdout.flush()?;

    for line_res in stdin.lock().lines() {
        let line = line_res?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let cmd: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                writeln!(stdout, "{}", json!({ "error": format!("Invalid JSON: {}", e) }))?;
                stdout.flush()?;
                continue;
            }
        };

        let action_name = cmd.get("action").and_then(|a| a.as_str()).unwrap_or("wait");
        let obs = match action_name {
            "move_north" => session.step(ActionAst::Move(Direction::North)),
            "move_east" => session.step(ActionAst::Move(Direction::East)),
            "move_south" => session.step(ActionAst::Move(Direction::South)),
            "move_west" => session.step(ActionAst::Move(Direction::West)),
            "move_northeast" => session.step(ActionAst::Move(Direction::NorthEast)),
            "move_northwest" => session.step(ActionAst::Move(Direction::NorthWest)),
            "move_southeast" => session.step(ActionAst::Move(Direction::SouthEast)),
            "move_southwest" => session.step(ActionAst::Move(Direction::SouthWest)),
            "pickup" => session.step(ActionAst::PickUp),
            "pay" => session.step(ActionAst::Pay),
            "pray" => session.step(ActionAst::Pray),
            "sacrifice" => {
                let idx = cmd.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                session.step(ActionAst::Sacrifice(idx))
            }
            "wait" => session.step(ActionAst::Wait),
            "get_state" => session.get_observation(),
            _ => {
                writeln!(
                    stdout,
                    "{}",
                    json!({ "error": format!("Unknown action: {}", action_name) })
                )?;
                stdout.flush()?;
                continue;
            }
        };

        writeln!(stdout, "{}", serde_json::to_string(&obs)?)?;
        stdout.flush()?;
    }

    Ok(())
}
