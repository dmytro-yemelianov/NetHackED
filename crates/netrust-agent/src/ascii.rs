//! 80x21 ASCII map rendering with FOV lighting.

use std::collections::HashSet;
use netrust_dungeon::compute_fov;
use netrust_sim::{Coord, DoorState, SimulationWorld, Tile, COLNO, ROWNO};

/// Renders the dungeon level and entities as an 80x21 ASCII string with FOV shading.
pub fn render_ascii_map(world: &SimulationWorld) -> String {
    let p_coord = world
        .arena
        .actors
        .get(world.player_id)
        .map(|p| p.coord)
        .unwrap_or(Coord::new_unchecked(0, 0));

    let visible: HashSet<Coord> = compute_fov(&world.level, p_coord, 8);

    let mut lines = Vec::with_capacity(ROWNO);
    for y in 0..ROWNO {
        let mut row = String::with_capacity(COLNO);
        for x in 0..COLNO {
            let c = Coord::new_unchecked(x, y);
            if !visible.contains(&c) {
                row.push(' ');
                continue;
            }

            // Check if actor occupies tile
            if let Some(actor_id) = world.actor_at(c) {
                if actor_id == world.player_id {
                    row.push('@');
                    continue;
                } else if let Some(actor) = world.arena.actors.get(actor_id) {
                    let ch = actor.name.chars().next().unwrap_or('m').to_ascii_lowercase();
                    row.push(ch);
                    continue;
                }
            }

            // Tile glyph
            let glyph = match world.level.get_tile(c) {
                Tile::Stone => ' ',
                Tile::Wall { horizontal } => {
                    if *horizontal {
                        '-'
                    } else {
                        '|'
                    }
                }
                Tile::Room => '.',
                Tile::Corr => '#',
                Tile::Door { state, .. } => match state {
                    DoorState::Open => '/',
                    DoorState::Closed | DoorState::Locked => '+',
                    DoorState::Broken => '*',
                },
                Tile::SecretDoor { .. } => ' ',
                Tile::Stairs { up } => {
                    if *up {
                        '<'
                    } else {
                        '>'
                    }
                }
                Tile::BranchStairs { up, .. } => {
                    if *up {
                        '<'
                    } else {
                        '>'
                    }
                }
                Tile::Pit { filled } => {
                    if *filled {
                        '.'
                    } else {
                        '0'
                    }
                }
                Tile::Altar { .. } => '_',
                Tile::Pool { frozen } => {
                    if *frozen {
                        '='
                    } else {
                        '}'
                    }
                }
                Tile::Lava => '^',
            };
            row.push(glyph);
        }
        lines.push(row);
    }

    lines.join("\n")
}
