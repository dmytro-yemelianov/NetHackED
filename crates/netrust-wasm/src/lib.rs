//! WebAssembly (WASM) Bindings for NetRust.
//!
//! Enables zero-install in-browser NetHack gameplay and remote agent control.

use netrust_agent::{render_ascii_map, AgentSession};
use netrust_core::ActionAst;
use netrust_data::roles::{CharacterConfig, Gender, RaceId, RoleId, RACES, ROLES};
use netrust_types::Direction;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct WasmGameSession {
    session: AgentSession,
}

#[wasm_bindgen]
impl WasmGameSession {
    /// Create a new session with seed and default character.
    #[wasm_bindgen(constructor)]
    pub fn new(seed: u64) -> Self {
        Self {
            session: AgentSession::new(seed),
        }
    }

    /// Create a new session with a custom character configuration.
    #[wasm_bindgen]
    pub fn new_with_character(seed: u64, role: &str, race: &str, name: &str) -> Self {
        let role_id = match role.to_lowercase().as_str() {
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

        let race_id = match race.to_lowercase().as_str() {
            "elf" => RaceId::Elf,
            "dwarf" => RaceId::Dwarf,
            "gnome" => RaceId::Gnome,
            "orc" => RaceId::Orc,
            _ => RaceId::Human,
        };

        let config = CharacterConfig {
            name: if name.is_empty() { "Hero".into() } else { name.to_string() },
            role: role_id,
            race: race_id,
            gender: Gender::Female,
            alignment: netrust_data::roles::get_role(role_id).default_alignment,
        };

        Self {
            session: AgentSession::new_with_character(seed, config),
        }
    }

    /// Render 80x21 ASCII viewport with FOV shading.
    #[wasm_bindgen]
    pub fn render_ascii(&self) -> String {
        render_ascii_map(&self.session.world)
    }

    /// Return full JSON observation payload for LLM/Agent interfaces.
    #[wasm_bindgen]
    pub fn get_observation_json(&self) -> String {
        let obs = self.session.get_observation();
        serde_json::to_string(&obs).unwrap_or_default()
    }

    /// Step the simulation with an action string (e.g. "move", "wait", "pickup", "pay", "pray", "sacrifice").
    #[wasm_bindgen]
    pub fn step(&mut self, action_str: &str, arg: Option<String>) -> String {
        let action = match action_str.to_lowercase().as_str() {
            "move" => {
                let dir = match arg.as_deref().unwrap_or("none") {
                    "north" | "k" => Direction::North,
                    "south" | "j" => Direction::South,
                    "east" | "l" => Direction::East,
                    "west" | "h" => Direction::West,
                    "northeast" | "u" => Direction::NorthEast,
                    "northwest" | "y" => Direction::NorthWest,
                    "southeast" | "n" => Direction::SouthEast,
                    "southwest" | "b" => Direction::SouthWest,
                    _ => Direction::None,
                };
                ActionAst::Move(dir)
            }
            "pickup" => ActionAst::PickUp,
            "drop" => ActionAst::Drop(arg.and_then(|s| s.parse().ok()).unwrap_or(0)),
            "wield" => ActionAst::Wield(arg.and_then(|s| s.parse().ok()).unwrap_or(0)),
            "eat" => ActionAst::Eat(arg.and_then(|s| s.parse().ok()).unwrap_or(0)),
            "quaff" => ActionAst::Quaff(arg.and_then(|s| s.parse().ok()).unwrap_or(0)),
            "read" => ActionAst::Read(arg.and_then(|s| s.parse().ok()).unwrap_or(0)),
            "rub" => ActionAst::Rub(arg.and_then(|s| s.parse().ok()).unwrap_or(0)),
            "price_check" | "appraise" => ActionAst::PriceCheck(arg.and_then(|s| s.parse().ok()).unwrap_or(0)),
            "dip" => {
                let (idx, water) = if let Some(ref s) = arg {
                    if let Some((idx_s, w_s)) = s.split_once(':') {
                        let i = idx_s.parse().unwrap_or(0);
                        let w = match w_s.to_lowercase().as_str() {
                            "unholy" | "cursed" => netrust_types::WaterType::Unholy,
                            "plain" | "uncursed" => netrust_types::WaterType::Plain,
                            _ => netrust_types::WaterType::Holy,
                        };
                        (i, w)
                    } else {
                        (s.parse().unwrap_or(0), netrust_types::WaterType::Holy)
                    }
                } else {
                    (0, netrust_types::WaterType::Holy)
                };
                ActionAst::Dip { item_index: idx, into_water: water }
            }
            "engrave" => {
                let text = arg.unwrap_or_else(|| "Elbereth".to_string());
                ActionAst::Engrave {
                    text,
                    medium: netrust_core::engraving::EngravingMedium::Dust(1),
                }
            }
            "wish" => ActionAst::Wish(arg.unwrap_or_default()),
            "cast" => ActionAst::Cast {
                spell_index: arg.and_then(|s| s.parse().ok()).unwrap_or(0),
                dir: Direction::None,
            },
            "zap" => ActionAst::ZapWand { dir: Direction::East, energy: 6 },
            "pay" => ActionAst::Pay,
            "pray" => ActionAst::Pray,
            "sacrifice" => ActionAst::Sacrifice(arg.and_then(|s| s.parse().ok()).unwrap_or(0)),
            "donate" => ActionAst::Donate(arg.and_then(|s| s.parse().ok()).unwrap_or(0)),
            "apply" | "light" => ActionAst::Apply(arg.and_then(|s| s.parse().ok()).unwrap_or(0)),
            "descend" => ActionAst::Descend,
            "ascend" => ActionAst::Ascend,
            "wait" => ActionAst::Wait,
            _ => ActionAst::Wait,
        };

        let obs = self.session.step(action);
        serde_json::to_string(&obs).unwrap_or_default()
    }

    /// Return carried inventory as JSON array with detailed stats for UI rendering.
    #[wasm_bindgen]
    pub fn get_inventory_json(&self) -> String {
        let carried_ids = self.session.world.arena.items_carried_by(self.session.world.player_id);
        let items: Vec<serde_json::Value> = carried_ids
            .into_iter()
            .enumerate()
            .filter_map(|(idx, id)| {
                self.session.world.arena.items.get(id).map(|it| {
                    let unpaid = self.session.world.get_unpaid_cost(id);
                    serde_json::json!({
                        "index": idx,
                        "name": it.name,
                        "class": format!("{:?}", it.class),
                        "weight": it.weight,
                        "buc": format!("{:?}", it.buc),
                        "enchantment": it.enchantment,
                        "erosion": it.erosion,
                        "proofed": it.proofed,
                        "is_container": it.is_container,
                        "unpaid_cost": unpaid,
                    })
                })
            })
            .collect();
        serde_json::to_string(&items).unwrap_or_default()
    }

    #[wasm_bindgen]
    pub fn get_player_hp(&self) -> u32 {
        self.session.world.arena.actors.get(self.session.world.player_id).map(|p| p.hp).unwrap_or(0)
    }

    #[wasm_bindgen]
    pub fn get_player_max_hp(&self) -> u32 {
        self.session.world.arena.actors.get(self.session.world.player_id).map(|p| p.max_hp).unwrap_or(0)
    }

    #[wasm_bindgen]
    pub fn get_player_pw(&self) -> u32 {
        self.session.world.player_pw
    }

    #[wasm_bindgen]
    pub fn get_player_max_pw(&self) -> u32 {
        self.session.world.player_max_pw
    }

    #[wasm_bindgen]
    pub fn get_player_nutrition(&self) -> u32 {
        self.session.world.player_nutrition
    }

    #[wasm_bindgen]
    pub fn get_hunger_state(&self) -> String {
        format!("{:?}", self.session.world.hunger_state())
    }

    #[wasm_bindgen]
    pub fn get_player_ac(&self) -> i32 {
        self.session.world.arena.actors.get(self.session.world.player_id).map(|p| p.ac).unwrap_or(10)
    }

    #[wasm_bindgen]
    pub fn get_player_gold(&self) -> u32 {
        self.session.world.player_gold
    }

    #[wasm_bindgen]
    pub fn get_turn(&self) -> u64 {
        self.session.world.scheduler.turn
    }

    #[wasm_bindgen]
    pub fn get_depth(&self) -> usize {
        self.session.world.depth
    }

    #[wasm_bindgen]
    pub fn get_player_name(&self) -> String {
        self.session.world.arena.actors.get(self.session.world.player_id).map(|p| p.name.clone()).unwrap_or_default()
    }

    #[wasm_bindgen]
    pub fn get_player_alignment(&self) -> String {
        self.session.world.arena.actors.get(self.session.world.player_id).map(|p| format!("{:?}", p.alignment)).unwrap_or_default()
    }

    #[wasm_bindgen]
    pub fn set_locale(&mut self, locale_str: &str) {
        let loc = netrust_types::Locale::parse(locale_str);
        self.session.world.set_locale(loc);
    }

    #[wasm_bindgen]
    pub fn get_locale(&self) -> String {
        self.session.world.get_locale().code().to_string()
    }

    #[wasm_bindgen]
    pub fn get_localized_hunger_state(&self) -> String {
        let hunger_state = format!("{:?}", self.session.world.hunger_state());
        netrust_i18n::t_hunger_str(&hunger_state, self.session.world.locale).to_string()
    }

    #[wasm_bindgen]
    pub fn get_localized_alignment(&self) -> String {
        if let Some(p) = self.session.world.arena.actors.get(self.session.world.player_id) {
            netrust_i18n::t_align(p.alignment, self.session.world.locale).to_string()
        } else {
            "".to_string()
        }
    }

    #[wasm_bindgen]
    pub fn get_i18n_strings_json(locale_str: &str) -> String {
        let loc = netrust_types::Locale::parse(locale_str);
        let keys = [
            "hero", "align", "dlvl", "gold", "hp", "pw", "ac", "nutr", "turn",
            "eat", "cast", "read", "pickup", "pay", "pray", "sacrifice",
            "wield", "drop", "descend", "ascend", "wait", "leaderboard", "character", "reset",
            "inventory", "quaff", "dip", "rub", "price_check", "engrave"
        ];
        let mut map = std::collections::HashMap::new();
        for k in keys {
            map.insert(k, netrust_i18n::t(k, loc));
        }
        map.insert("welcome", netrust_i18n::Messages::welcome(loc));
        serde_json::to_string(&map).unwrap_or_default()
    }

    /// Return catalog of available roles and races in JSON format.
    #[wasm_bindgen]
    pub fn get_roles_json() -> String {
        let data = serde_json::json!({
            "roles": ROLES.iter().map(|r| serde_json::json!({
                "id": format!("{:?}", r.id),
                "name": r.name,
                "base_hp": r.base_hp,
                "ac": r.ac,
                "speed": r.speed,
                "default_alignment": format!("{:?}", r.default_alignment),
            })).collect::<Vec<_>>(),
            "races": RACES.iter().map(|r| serde_json::json!({
                "id": format!("{:?}", r.id),
                "name": r.name,
            })).collect::<Vec<_>>()
        });
        serde_json::to_string(&data).unwrap_or_default()
    }

    /// Return full 80x21 grid tile matrix, floor items, visible actors, and illumination for 2D retro canvas rendering.
    #[wasm_bindgen]
    pub fn get_canvas_render_data_json(&self) -> String {
        let (visible, detected_monsters) = self.session.world.compute_perception();
        let player = self.session.world.arena.actors.get(self.session.world.player_id);
        let player_coord = player.map(|p| p.coord);

        let mut tiles_data = Vec::with_capacity(netrust_types::ROWNO);
        for y in 0..netrust_types::ROWNO {
            let mut row = Vec::with_capacity(netrust_types::COLNO);
            for x in 0..netrust_types::COLNO {
                let c = netrust_types::Coord::new_unchecked(x, y);
                let tile = self.session.world.level.get_tile(c);
                let is_vis = visible.contains(&c);
                let is_dark = self.session.world.level.is_dark_at(c);
                let tile_kind = match tile {
                    netrust_types::Tile::Stone => "stone",
                    netrust_types::Tile::Wall { horizontal } => if *horizontal { "wall_h" } else { "wall_v" },
                    netrust_types::Tile::Room => "room",
                    netrust_types::Tile::Corr => "corr",
                    netrust_types::Tile::Door { state, .. } => match state {
                        netrust_types::DoorState::Open => "door_open",
                        netrust_types::DoorState::Broken => "door_broken",
                        _ => "door_closed",
                    },
                    netrust_types::Tile::SecretDoor { .. } => "stone",
                    netrust_types::Tile::Stairs { up } => if *up { "stairs_up" } else { "stairs_down" },
                    netrust_types::Tile::BranchStairs { up, .. } => if *up { "stairs_up" } else { "stairs_down" },
                    netrust_types::Tile::Pit { filled } => if *filled { "room" } else { "pit" },
                    netrust_types::Tile::Altar { align } | netrust_types::Tile::HighAltar { align } => match align {
                        netrust_types::Alignment::Lawful => "altar_lawful",
                        netrust_types::Alignment::Neutral => "altar_neutral",
                        _ => "altar_chaotic",
                    },
                    netrust_types::Tile::Drawbridge { open } => if *open { "bridge_open" } else { "bridge_closed" },
                    netrust_types::Tile::Moat => "water",
                    netrust_types::Tile::Pool { frozen } => if *frozen { "ice" } else { "water" },
                    netrust_types::Tile::Lava => "lava",
                };
                row.push(serde_json::json!({
                    "kind": tile_kind,
                    "visible": is_vis,
                    "dark": is_dark,
                }));
            }
            tiles_data.push(row);
        }

        // Collect floor items
        let mut floor_items = Vec::new();
        for (_, item) in self.session.world.arena.items.iter() {
            if let netrust_arena::ItemLocation::Floor(c) = item.location {
                if visible.contains(&c) {
                    floor_items.push(serde_json::json!({
                        "x": c.x,
                        "y": c.y,
                        "name": item.name,
                        "symbol": item.class.symbol().to_string(),
                        "class": format!("{:?}", item.class).to_lowercase(),
                    }));
                }
            }
        }

        // Collect actors
        let mut actors = Vec::new();
        if let Some(p) = player {
            actors.push(serde_json::json!({
                "x": p.coord.x,
                "y": p.coord.y,
                "name": p.name,
                "is_player": true,
                "hp": p.hp,
                "max_hp": p.max_hp,
            }));
        }
        for (id, actor) in self.session.world.arena.actors.iter() {
            if id != self.session.world.player_id && !actor.is_dead
                && (detected_monsters.contains(&id) || visible.contains(&actor.coord)) {
                    actors.push(serde_json::json!({
                        "x": actor.coord.x,
                        "y": actor.coord.y,
                        "name": actor.name,
                        "is_player": false,
                        "hp": actor.hp,
                        "max_hp": actor.max_hp,
                    }));
                }
        }

        let payload = serde_json::json!({
            "player_coord": player_coord.map(|c| (c.x, c.y)),
            "tiles": tiles_data,
            "items": floor_items,
            "actors": actors,
            "turn": self.session.world.scheduler.turn,
        });

        serde_json::to_string(&payload).unwrap_or_default()
    }
}

/// Run tournament benchmark across seeds comparing Random, Survival, Speedrunner, and PetTesterTactical.
#[wasm_bindgen]
pub fn run_tournament_benchmark(num_seeds: u32, max_turns: u32) -> String {
    let seeds: Vec<u64> = (1..=(num_seeds as u64).max(1)).collect();
    let roles = vec![RoleId::Valkyrie, RoleId::Wizard];
    let (_results, summary) = netrust_agent::run_evaluation_suite(&seeds, &roles, (max_turns as u64).max(10));
    serde_json::to_string(&summary).unwrap_or_default()
}

/// Run a match with PetTesterTacticalPolicy and return full decision trajectory JSON for replay in Web UI.
#[wasm_bindgen]
pub fn run_tactical_trajectory(seed: u64, max_turns: u32) -> String {
    let config = CharacterConfig::default();
    let (_res, traj) = netrust_agent::run_game_with_trajectory(
        netrust_agent::PetTesterTacticalPolicy::new(),
        seed,
        config,
        (max_turns as u64).max(10),
    );
    serde_json::to_string(&traj).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_session_init_and_step() {
        let mut wasm_sess = WasmGameSession::new(42);
        assert_eq!(wasm_sess.get_player_hp(), 18);
        assert_eq!(wasm_sess.get_turn(), 1);

        let ascii = wasm_sess.render_ascii();
        assert!(ascii.contains('@'));

        let obs_json = wasm_sess.get_observation_json();
        assert!(obs_json.contains("player_hp"));

        let step_res = wasm_sess.step("wait", None);
        assert!(step_res.contains("turn"));
        assert_eq!(wasm_sess.get_turn(), 2);
    }

    #[test]
    fn test_wasm_session_character_creation() {
        let wasm_sess = WasmGameSession::new_with_character(42, "barbarian", "orc", "Conan");
        assert_eq!(wasm_sess.get_player_hp(), 20);
        assert_eq!(wasm_sess.get_player_name(), "Conan");
        assert_eq!(wasm_sess.get_player_alignment(), "Chaotic");
    }

    #[test]
    fn test_wasm_inventory_and_extended_actions() {
        let mut wasm_sess = WasmGameSession::new(42);
        let inv_json = wasm_sess.get_inventory_json();
        assert!(inv_json.contains("name"));
        assert!(inv_json.contains("buc"));

        // Test dip, engrave, price_check actions
        let res_dip = wasm_sess.step("dip", Some("0:holy".into()));
        assert!(res_dip.contains("turn"));

        let res_engrave = wasm_sess.step("engrave", Some("Elbereth".into()));
        assert!(res_engrave.contains("turn"));

        let res_price = wasm_sess.step("price_check", Some("0".into()));
        assert!(res_price.contains("turn"));
    }
}
