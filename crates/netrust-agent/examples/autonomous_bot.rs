//! Autonomous NetRust AI Agent Example.
//!
//! Demonstrates how an AI agent, RL model, or autonomous bot connects to NetRust,
//! perceives structured observations (FOV, tile inspections, visible monsters, logs),
//! and executes optimal actions in the verified dungeon.

use netrust_agent::AgentSession;
use netrust_data::roles::{CharacterConfig, Gender, RaceId, RoleId};
use netrust_sim::{ActionAst, Direction};
use netrust_types::Alignment;

fn main() {
    println!("🤖 Initializing Autonomous NetRust Agent...");

    // 1. Create a character (Valkyrie, Human, Female, Neutral)
    let config = CharacterConfig {
        name: "Brunhilde".to_string(),
        role: RoleId::Valkyrie,
        race: RaceId::Human,
        gender: Gender::Female,
        alignment: Alignment::Neutral,
    };

    let mut session = AgentSession::new_with_character(1337, config);
    println!("⚔️ Character initialized: Brunhilde the Valkyrie");

    // 2. Initial observation
    let mut obs = session.get_observation();
    println!(
        "📍 Starting position: ({}, {}) | Turn: {} | HP: {}/{}",
        obs.player_coord.x, obs.player_coord.y, obs.turn, obs.player_hp, obs.player_max_hp
    );
    println!("\n--- Dungeon Viewport ---");
    println!("{}", obs.ascii_map);
    println!("------------------------\n");

    // 3. Autonomous decision loop for 10 turns
    let directions = [
        Direction::East,
        Direction::East,
        Direction::South,
        Direction::South,
        Direction::East,
        Direction::North,
    ];

    for &dir in directions.iter() {
        println!("Turn {}: Agent deciding next action...", obs.turn);

        // Check visible actors
        for actor in &obs.visible_actors {
            if !actor.is_player {
                println!(
                    "  👁️ Monster detected in FOV: {} at ({}, {}) [HP: {}/{}]",
                    actor.name, actor.coord.x, actor.coord.y, actor.hp, actor.max_hp
                );
            }
        }

        // Inspect adjacent tile
        let target_x = match dir {
            Direction::East => obs.player_coord.x + 1,
            Direction::West => obs.player_coord.x.saturating_sub(1),
            _ => obs.player_coord.x,
        };
        let target_y = match dir {
            Direction::South => obs.player_coord.y + 1,
            Direction::North => obs.player_coord.y.saturating_sub(1),
            _ => obs.player_coord.y,
        };

        if let Ok(inspection) = session.inspect_tile(target_x, target_y) {
            println!(
                "  🔍 Target Tile ({}, {}): {:?} (passable: {})",
                target_x, target_y, inspection.tile, inspection.is_passable
            );
        }

        // Execute action
        let action = ActionAst::Move(dir);
        println!("  ⚡ Executing: Move({dir:?})");
        obs = session.step(action);

        // Process game events / logs
        for event in &obs.last_events {
            println!("  📜 Event: {event:?}");
        }

        println!(
            "  ❤️ Player HP: {}/{} | Position: ({}, {})\n",
            obs.player_hp, obs.player_max_hp, obs.player_coord.x, obs.player_coord.y
        );

        if obs.is_game_over {
            println!("💀 Game over!");
            break;
        }
    }

    println!("--- Final Dungeon Viewport ---");
    println!("{}", obs.ascii_map);
    println!("------------------------------");
    println!("✅ Autonomous Agent session concluded successfully!");
}
