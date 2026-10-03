//! 80x21 ASCII map rendering with FOV lighting.

use netrust_sim::{Coord, DoorState, SimulationWorld, Tile, COLNO, ROWNO};

/// Renders the dungeon level and entities as an 80x21 ASCII string with dynamic lighting, darkness, and telepathy.
pub fn render_ascii_map(world: &SimulationWorld) -> String {
    let (visible, detected_monsters) = world.compute_perception();

    let mut lines = Vec::with_capacity(ROWNO);
    for y in 0..ROWNO {
        let mut row = String::with_capacity(COLNO);
        for x in 0..COLNO {
            let c = Coord::new_unchecked(x, y);

            // Hero glyph
            if let Some(player) = world.arena.actors.get(world.player_id) {
                if player.coord == c {
                    row.push('@');
                    continue;
                }
            }

            // Detected monsters (either visually seen or detected via ESP telepathy)
            if let Some(actor_id) = world.actor_at(c) {
                if detected_monsters.contains(&actor_id) {
                    if let Some(actor) = world.arena.actors.get(actor_id) {
                        let ch = actor.name.chars().next().unwrap_or('m').to_ascii_lowercase();
                        row.push(ch);
                        continue;
                    }
                }
            }

            if !visible.contains(&c) {
                row.push(' ');
                continue;
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
                Tile::HighAltar { .. } => '_',
                Tile::Drawbridge { open } => {
                    if *open {
                        '.'
                    } else {
                        '#'
                    }
                }
                Tile::Moat => '}',
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
