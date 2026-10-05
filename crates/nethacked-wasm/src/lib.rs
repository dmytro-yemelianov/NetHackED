//! WebAssembly (WASM) Bindings for NetHackED.
//!
//! Enables zero-install in-browser NetHack gameplay and remote agent control.

use nethacked_agent::{
    parse_action, parse_character, render_ascii_map, run_seed_games, summarize, ActionArgs,
    AgentSession, RunResult,
};
use nethacked_data::roles::{CharacterConfig, RoleId};
use nethacked_types::Coord;
use wasm_bindgen::prelude::*;

mod packs;
pub mod pixel;
pub use packs::*;
#[cfg(target_arch = "wasm32")]
pub use pixel::PixelScreen;

/// Installs the panic hook so Rust panics show up in the browser console.
#[wasm_bindgen(start)]
pub fn wasm_start() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();
}

/// Actions whose argument is an inventory/spell index or amount.
const INDEX_ACTIONS: &[&str] = &[
    "drop",
    "wield",
    "eat",
    "quaff",
    "read",
    "rub",
    "price_check",
    "appraise",
    "apply",
    "light",
    "sacrifice",
    "donate",
];
/// Actions whose argument is a direction.
const DIRECTION_ACTIONS: &[&str] = &[
    "move",
    "cast",
    "zap",
    "fire",
    "kick",
    "open_door",
    "close_door",
    "untrap",
];
/// Actions whose argument is free text.
const TEXT_ACTIONS: &[&str] = &["wish", "engrave", "dip"];

/// Map a raw string argument onto the shared [`ActionArgs`] fields for `action`.
fn action_args_for(action: &str, arg: Option<String>, player: Option<Coord>) -> ActionArgs {
    let a = action.trim().to_lowercase();
    let mut args = ActionArgs {
        player,
        ..ActionArgs::default()
    };
    if INDEX_ACTIONS.contains(&a.as_str()) {
        args.index = arg.and_then(|s| s.trim().parse::<usize>().ok());
    } else if DIRECTION_ACTIONS.contains(&a.as_str()) {
        args.direction = arg;
    } else if TEXT_ACTIONS.contains(&a.as_str()) {
        args.text = arg;
    }
    args
}

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
    /// Returns an error string for an unknown role or race.
    #[wasm_bindgen]
    pub fn new_with_character(
        seed: u64,
        role: &str,
        race: &str,
        name: &str,
    ) -> Result<WasmGameSession, JsValue> {
        let name = if name.is_empty() { None } else { Some(name) };
        let mut config = parse_character(name, Some(role), Some(race), Some("female"), None)
            .map_err(|e| JsValue::from_str(&e))?;
        // The web UI keeps each role's natural alignment.
        config.alignment = nethacked_data::roles::get_role(config.role).default_alignment;
        Ok(Self {
            session: AgentSession::new_with_character(seed, config),
        })
    }

    /// Like `new_with_character` but plays on the given rule pack's ruleset.
    #[wasm_bindgen(js_name = newWithPack)]
    pub fn new_with_pack(
        seed: u64,
        role: &str,
        race: &str,
        name: &str,
        pack: &WasmPack,
    ) -> Result<WasmGameSession, JsValue> {
        let name = if name.is_empty() { None } else { Some(name) };
        let mut config = parse_character(name, Some(role), Some(race), Some("female"), None)
            .map_err(|e| JsValue::from_str(&e))?;
        config.alignment = pack
            .ruleset
            .role(config.role)
            .map(|r| r.default_alignment)
            .unwrap_or_else(|| nethacked_data::roles::get_role(config.role).default_alignment);
        Ok(Self {
            session: AgentSession::new_with_ruleset(
                seed,
                config,
                std::sync::Arc::clone(&pack.ruleset),
                pack.rref.clone(),
            ),
        })
    }

    /// Id of the ruleset this session plays on (`vanilla` or a pack id).
    #[wasm_bindgen(js_name = rulesetId)]
    pub fn ruleset_id(&self) -> String {
        self.session.world.ruleset_ref.id.clone()
    }

    /// Hash of the ruleset this session plays on.
    #[wasm_bindgen(js_name = rulesetHash)]
    pub fn ruleset_hash(&self) -> String {
        self.session.world.ruleset_ref.hash.clone()
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
        let name = action_str.trim().to_lowercase();
        if INDEX_ACTIONS.contains(&name.as_str()) {
            if let Some(a) = &arg {
                if a.trim().parse::<usize>().is_err() {
                    return serde_json::json!({"error": "index must be a number"}).to_string();
                }
            }
        }
        let args = action_args_for(&name, arg, self.session.player_coord());
        let action = match parse_action(&name, &args) {
            Ok(a) => a,
            Err(e) => return serde_json::json!({"error": e}).to_string(),
        };

        let obs = self.session.step(action);
        serde_json::to_string(&obs).unwrap_or_default()
    }

    /// Return carried inventory as JSON array with detailed stats for UI rendering.
    #[wasm_bindgen]
    pub fn get_inventory_json(&self) -> String {
        let carried_ids = self
            .session
            .world
            .arena
            .items_carried_by(self.session.world.player_id);
        let items: Vec<serde_json::Value> = carried_ids
            .into_iter()
            .enumerate()
            .filter_map(|(idx, id)| {
                self.session.world.arena.items.get(id).map(|it| {
                    let unpaid = self.session.world.get_unpaid_cost(id);
                    serde_json::json!({
                        "index": idx,
                        "name": it.name,
                        "display_name": nethacked_i18n::t_item(&it.name, self.session.world.locale),
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
        self.session
            .world
            .arena
            .actors
            .get(self.session.world.player_id)
            .map(|p| p.hp)
            .unwrap_or(0)
    }

    #[wasm_bindgen]
    pub fn get_player_max_hp(&self) -> u32 {
        self.session
            .world
            .arena
            .actors
            .get(self.session.world.player_id)
            .map(|p| p.max_hp)
            .unwrap_or(0)
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
    pub fn get_player_nutrition(&self) -> i32 {
        self.session.world.player_nutrition
    }

    #[wasm_bindgen]
    pub fn get_hunger_state(&self) -> String {
        format!("{:?}", self.session.world.hunger_state())
    }

    #[wasm_bindgen]
    pub fn get_player_ac(&self) -> i32 {
        self.session
            .world
            .arena
            .actors
            .get(self.session.world.player_id)
            .map(|p| p.ac)
            .unwrap_or(10)
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
        self.session
            .world
            .arena
            .actors
            .get(self.session.world.player_id)
            .map(|p| p.name.clone())
            .unwrap_or_default()
    }

    #[wasm_bindgen]
    pub fn get_player_alignment(&self) -> String {
        self.session
            .world
            .arena
            .actors
            .get(self.session.world.player_id)
            .map(|p| format!("{:?}", p.alignment))
            .unwrap_or_default()
    }

    #[wasm_bindgen]
    pub fn set_locale(&mut self, locale_str: &str) {
        let loc = nethacked_types::Locale::parse(locale_str);
        self.session.world.set_locale(loc);
    }

    #[wasm_bindgen]
    pub fn get_locale(&self) -> String {
        self.session.world.get_locale().code().to_string()
    }

    #[wasm_bindgen]
    pub fn get_localized_hunger_state(&self) -> String {
        let hunger_state = format!("{:?}", self.session.world.hunger_state());
        nethacked_i18n::t_hunger_str(&hunger_state, self.session.world.locale).to_string()
    }

    #[wasm_bindgen]
    pub fn get_localized_alignment(&self) -> String {
        if let Some(p) = self
            .session
            .world
            .arena
            .actors
            .get(self.session.world.player_id)
        {
            nethacked_i18n::t_align(p.alignment, self.session.world.locale).to_string()
        } else {
            "".to_string()
        }
    }

    #[wasm_bindgen]
    pub fn get_i18n_strings_json(locale_str: &str) -> String {
        let loc = nethacked_types::Locale::parse(locale_str);
        let keys = [
            "hero",
            "align",
            "dlvl",
            "gold",
            "hp",
            "pw",
            "ac",
            "nutr",
            "turn",
            "eat",
            "cast",
            "read",
            "pickup",
            "pay",
            "pray",
            "sacrifice",
            "wield",
            "drop",
            "descend",
            "ascend",
            "wait",
            "leaderboard",
            "character",
            "reset",
            "inventory",
            "quaff",
            "dip",
            "rub",
            "price_check",
            "engrave",
        ];
        let mut map = std::collections::HashMap::new();
        for k in keys {
            map.insert(k, nethacked_i18n::t(k, loc));
        }
        map.insert("welcome", nethacked_i18n::Messages::welcome(loc));
        serde_json::to_string(&map).unwrap_or_default()
    }

    /// Return catalog of available roles and races in JSON format.
    #[wasm_bindgen]
    pub fn get_roles_json() -> String {
        let rs = nethacked_data::ruleset::Ruleset::vanilla();
        let data = serde_json::json!({
            "roles": rs.roles.iter().map(|r| serde_json::json!({
                "id": format!("{:?}", r.id),
                "name": r.name,
                "base_hp": r.base_hp,
                "ac": r.ac,
                "speed": r.speed,
                "default_alignment": format!("{:?}", r.default_alignment),
            })).collect::<Vec<_>>(),
            "races": rs.races.iter().map(|r| serde_json::json!({
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
        let player = self
            .session
            .world
            .arena
            .actors
            .get(self.session.world.player_id);
        let player_coord = player.map(|p| p.coord);

        let mut tiles_data = Vec::with_capacity(nethacked_types::ROWNO);
        for y in 0..nethacked_types::ROWNO {
            let mut row = Vec::with_capacity(nethacked_types::COLNO);
            for x in 0..nethacked_types::COLNO {
                let c = nethacked_types::Coord::new_unchecked(x, y);
                let tile = self.session.world.level.get_tile(c);
                let is_vis = visible.contains(&c);
                let is_dark = self.session.world.level.is_dark_at(c);
                let tile_kind = match tile {
                    nethacked_types::Tile::Stone => "stone",
                    nethacked_types::Tile::Wall { horizontal } => {
                        if *horizontal {
                            "wall_h"
                        } else {
                            "wall_v"
                        }
                    }
                    nethacked_types::Tile::Room => "room",
                    nethacked_types::Tile::Corr => "corr",
                    nethacked_types::Tile::Door { state, .. } => match state {
                        nethacked_types::DoorState::Open => "door_open",
                        nethacked_types::DoorState::Broken | nethacked_types::DoorState::NoDoor => {
                            "door_broken"
                        }
                        _ => "door_closed",
                    },
                    nethacked_types::Tile::SecretDoor { .. } => "stone",
                    nethacked_types::Tile::Stairs { up } => {
                        if *up {
                            "stairs_up"
                        } else {
                            "stairs_down"
                        }
                    }
                    nethacked_types::Tile::BranchStairs { up, .. } => {
                        if *up {
                            "stairs_up"
                        } else {
                            "stairs_down"
                        }
                    }
                    nethacked_types::Tile::Pit { filled } => {
                        if *filled {
                            "room"
                        } else {
                            "pit"
                        }
                    }
                    nethacked_types::Tile::Altar { align }
                    | nethacked_types::Tile::HighAltar { align } => match align {
                        nethacked_types::Alignment::Lawful => "altar_lawful",
                        nethacked_types::Alignment::Neutral => "altar_neutral",
                        _ => "altar_chaotic",
                    },
                    nethacked_types::Tile::Drawbridge { open } => {
                        if *open {
                            "bridge_open"
                        } else {
                            "bridge_closed"
                        }
                    }
                    nethacked_types::Tile::Moat => "water",
                    nethacked_types::Tile::Pool { frozen } => {
                        if *frozen {
                            "ice"
                        } else {
                            "water"
                        }
                    }
                    nethacked_types::Tile::Lava => "lava",
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
            if let nethacked_arena::ItemLocation::Floor(c) = item.location {
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
            if id != self.session.world.player_id
                && !actor.is_dead
                && (detected_monsters.contains(&id) || visible.contains(&actor.coord))
            {
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

/// Incremental tournament runner so the UI can show progress between seeds.
#[wasm_bindgen]
pub struct TournamentRun {
    seeds: Vec<u64>,
    roles: Vec<RoleId>,
    max_turns: u64,
    done: usize,
    results: Vec<RunResult>,
}

#[wasm_bindgen]
impl TournamentRun {
    #[wasm_bindgen(constructor)]
    pub fn new(num_seeds: u32, max_turns: u32) -> TournamentRun {
        TournamentRun {
            seeds: (1..=(num_seeds as u64).max(1)).collect(),
            roles: vec![RoleId::Valkyrie, RoleId::Wizard],
            max_turns: (max_turns as u64).max(10),
            done: 0,
            results: Vec::new(),
        }
    }

    /// Run up to `k` more seeds. Returns true once every seed has completed.
    pub fn step(&mut self, k: u32) -> bool {
        let end = self.seeds.len().min(self.done.saturating_add(k as usize));
        while self.done < end {
            let seed = self.seeds[self.done];
            self.results
                .extend(run_seed_games(seed, &self.roles, self.max_turns));
            self.done += 1;
        }
        self.done >= self.seeds.len()
    }

    /// Seeds completed so far.
    pub fn progress(&self) -> u32 {
        self.done as u32
    }

    /// Total seeds to run.
    pub fn total(&self) -> u32 {
        self.seeds.len() as u32
    }

    /// Summary JSON over the runs completed so far (keys sorted for stable output).
    pub fn report_json(&self) -> String {
        let summary = summarize(&self.results);
        serde_json::to_value(&summary)
            .map(|v| v.to_string())
            .unwrap_or_default()
    }
}

/// Run tournament benchmark across seeds comparing Random, Survival, Speedrunner, and PetTesterTactical.
#[wasm_bindgen]
pub fn run_tournament_benchmark(num_seeds: u32, max_turns: u32) -> String {
    let mut t = TournamentRun::new(num_seeds, max_turns);
    while !t.step(u32::MAX) {}
    t.report_json()
}

/// Run a match with PetTesterTacticalPolicy and return full decision trajectory JSON for replay in Web UI.
#[wasm_bindgen]
pub fn run_tactical_trajectory(seed: u64, max_turns: u32) -> String {
    let config = CharacterConfig::default();
    let (_res, traj) = nethacked_agent::run_game_with_trajectory(
        nethacked_agent::PetTesterTacticalPolicy::new(),
        seed,
        config,
        (max_turns as u64).max(10),
    );
    serde_json::to_string(&traj).unwrap_or_default()
}

/// Localized labels the clean terminal page draws itself: role and race
/// names and the status-line labels, for `locale` (`"en"` / `"uk"`).
#[wasm_bindgen(js_name = uiStringsJson)]
pub fn ui_strings_json(locale: &str) -> String {
    use nethacked_i18n::t;
    let loc = nethacked_types::Locale::parse(locale);
    let uk = loc == nethacked_types::Locale::Uk;
    let roles = serde_json::json!({
        "valkyrie": t("role.valkyrie", loc),
        "wizard": t("role.wizard", loc),
        "barbarian": t("role.barbarian", loc),
        "rogue": t("role.rogue", loc),
        "knight": t("role.knight", loc),
        "monk": t("role.monk", loc),
        "healer": t("role.healer", loc),
        "tourist": t("role.tourist", loc),
        "archaeologist": t("role.archaeologist", loc),
    });
    let race = |en: &'static str, ua: &'static str| if uk { ua } else { en };
    let races = serde_json::json!({
        "human": race("human", "людина"),
        "elf": race("elf", "ельф"),
        "dwarf": race("dwarf", "дворф"),
        "gnome": race("gnome", "гном"),
        "orc": race("orc", "орк"),
    });
    let status = serde_json::json!({
        "dlvl": t("dlvl", loc),
        "gold": t("gold", loc),
        "hp": t("hp", loc),
        "pw": t("pw", loc),
        "ac": t("ac", loc),
        "turn": t("turn", loc),
    });
    serde_json::json!({ "role": roles, "race": races, "status": status }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_has_localized_display_name() {
        let mut s = WasmGameSession::new_with_character(7, "valkyrie", "human", "T").unwrap();
        let en: serde_json::Value = serde_json::from_str(&s.get_inventory_json()).unwrap();
        assert_eq!(en[0]["display_name"], en[0]["name"]);
        s.set_locale("uk");
        let uk: serde_json::Value = serde_json::from_str(&s.get_inventory_json()).unwrap();
        assert_eq!(uk[0]["name"], "long sword");
        assert_eq!(uk[0]["display_name"], "довгий меч");
    }

    #[test]
    fn ui_strings_cover_roles_races_and_status_labels() {
        let uk: serde_json::Value = serde_json::from_str(&ui_strings_json("uk")).unwrap();
        assert_eq!(uk["role"]["valkyrie"], "Валькірія");
        assert_eq!(
            uk["role"]["archaeologist"].as_str().map(|s| s.is_empty()),
            Some(false)
        );
        assert_eq!(
            uk["race"]["human"].as_str().map(|s| s.is_empty()),
            Some(false)
        );
        assert_eq!(uk["status"]["turn"], "Хід");
        let en: serde_json::Value = serde_json::from_str(&ui_strings_json("en")).unwrap();
        assert_eq!(en["role"]["valkyrie"], "Valkyrie");
        assert_eq!(en["status"]["turn"], "Turn");
    }

    #[test]
    fn step_rejects_unknown_action_without_advancing() {
        let mut s = WasmGameSession::new(7);
        let turn = s.get_turn();
        let out = s.step("dance", None);
        assert!(out.contains("\"error\""));
        assert_eq!(s.get_turn(), turn);
    }

    #[test]
    fn cast_and_zap_take_direction() {
        let args = action_args_for("cast", Some("west".into()), None);
        assert_eq!(args.direction.as_deref(), Some("west"));
        let args = action_args_for("drop", Some("2".into()), None);
        assert_eq!(args.index, Some(2));
    }

    #[test]
    fn non_numeric_index_is_error() {
        let mut s = WasmGameSession::new(7);
        assert!(s.step("drop", Some("abc".into())).contains("\"error\""));
    }

    #[test]
    fn chunked_tournament_matches_one_shot() {
        // Independent one-shot path: the agent crate's suite, not TournamentRun.
        let (results, _) = nethacked_agent::run_evaluation_suite(
            &[1, 2, 3],
            &[RoleId::Valkyrie, RoleId::Wizard],
            30,
        );
        let one = serde_json::to_value(summarize(&results))
            .unwrap()
            .to_string();
        let mut t = TournamentRun::new(3, 30);
        assert!(!t.step(0));
        assert_eq!(t.progress(), 0);
        while !t.step(1) {}
        assert_eq!(t.progress(), 3);
        assert!(t.step(1), "after completion step is a no-op returning true");
        assert_eq!(t.report_json(), one);
    }

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
        let wasm_sess =
            WasmGameSession::new_with_character(42, "barbarian", "orc", "Conan").unwrap();
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

    #[test]
    fn test_wasm_get_roles_json_parity() {
        let json = WasmGameSession::get_roles_json();
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(val["roles"].as_array().unwrap().len(), 9);
        assert_eq!(val["races"].as_array().unwrap().len(), 5);
        assert_eq!(val["roles"][0]["name"], "Valkyrie");
        assert_eq!(val["races"][0]["name"], "Human");
    }
}
