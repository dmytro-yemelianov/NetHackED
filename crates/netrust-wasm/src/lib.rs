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
            "read" => ActionAst::Read(arg.and_then(|s| s.parse().ok()).unwrap_or(0)),
            "cast" => ActionAst::Cast {
                spell_index: arg.and_then(|s| s.parse().ok()).unwrap_or(0),
                dir: Direction::None,
            },
            "zap" => ActionAst::ZapWand { dir: Direction::East, energy: 6 },
            "pay" => ActionAst::Pay,
            "pray" => ActionAst::Pray,
            "sacrifice" => ActionAst::Sacrifice(arg.and_then(|s| s.parse().ok()).unwrap_or(0)),
            "descend" => ActionAst::Descend,
            "ascend" => ActionAst::Ascend,
            "wait" => ActionAst::Wait,
            _ => ActionAst::Wait,
        };

        let obs = self.session.step(action);
        serde_json::to_string(&obs).unwrap_or_default()
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
}
