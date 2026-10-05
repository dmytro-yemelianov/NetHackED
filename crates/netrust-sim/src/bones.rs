//! Graveyard bones level persistence and ghost reincarnation.

use netrust_arena::{ItemLocation, ItemRecord};
use netrust_core::{corrupt_buc_on_death, is_valid_bones_level};
use netrust_data::create_ghost_record;
use netrust_i18n::Messages;
use netrust_types::{BonesData, BonesItem};
use rand::Rng;

use crate::events::GameEvent;
use crate::world::SimulationWorld;

/// Items that `resetobjs` always curses in bones (C `bones.c:170-189`): the Amulet of
/// Yendor (fake Amulet, `:173`) and the invocation items (Candelabrum `:183`, Bell `:186`,
/// Book of the Dead `:189`). Quest artifacts are NOT in this list: they take the normal
/// `rn2(5)` curse roll (`bones.c:291`). Bones records do not carry the item kind, so
/// detection is by name.
pub fn always_cursed_in_bones(name: &str) -> bool {
    const ALWAYS: [&str; 4] = [
        "Amulet of Yendor",
        "Candelabrum of Invocation",
        "Bell of Opening",
        "Book of the Dead",
    ];
    ALWAYS.iter().any(|q| q.eq_ignore_ascii_case(name))
}

impl SimulationWorld {
    /// Saves dead adventurer state and corrupted gear to the bones graveyard file.
    pub fn save_bones(&mut self, killer: &str) -> Option<BonesData> {
        if !is_valid_bones_level(self.depth as u32) {
            return None;
        }

        let player = self.arena.actors.get(self.player_id).cloned()?;
        let carried_ids = self.arena.items_carried_by(self.player_id);

        let mut bones_items = Vec::new();
        for id in carried_ids {
            let roll = self.rng.random_range(0..5u32);
            if let Some(item) = self.arena.items.get(id) {
                bones_items.push(BonesItem {
                    name: item.name.clone(),
                    class: item.class,
                    weight: item.weight,
                    buc: corrupt_buc_on_death(item.buc, always_cursed_in_bones(&item.name), roll),
                    enchantment: item.enchantment,
                });
            }
        }

        let bones = BonesData {
            depth: self.depth as u32,
            hero_name: player.name.clone(),
            hero_level: player.level,
            max_hp: player.max_hp,
            ac: player.ac,
            death_coord: player.coord,
            items: bones_items,
            killer: killer.to_string(),
        };

        self.bones_storage.push(bones.clone());
        Some(bones)
    }

    /// Checks if a graveyard bones file exists for the current depth and spawns the ghost and corrupted items.
    pub fn check_and_load_bones(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let depth_val = self.depth as u32;

        if let Some(idx) = self.bones_storage.iter().position(|b| b.depth == depth_val) {
            let bones = self.bones_storage.remove(idx);

            // 1. Spawn the vengeful ghost at the death site
            let ghost_hp = netrust_core::create_ghost_hp(bones.max_hp);
            let mut ghost = create_ghost_record(
                &bones.hero_name,
                bones.hero_level,
                ghost_hp,
                bones.death_coord,
            );
            if let Some(def) = self.ruleset.monster("ghost") {
                self.set_monster_malign(&mut ghost, def);
            }
            self.arena.spawn_actor(ghost);

            // 2. Scatter corrupted (cursed) equipment across neighboring floor tiles
            let mut scatter_coords = bones.death_coord.neighbors();
            scatter_coords.insert(0, bones.death_coord);

            for (i, b_item) in bones.items.into_iter().enumerate() {
                let target_coord = scatter_coords[i % scatter_coords.len()];
                if self.level.is_passable(target_coord) {
                    let item_record = ItemRecord {
                        name: b_item.name,
                        class: b_item.class,
                        weight: b_item.weight,
                        buc: b_item.buc,
                        is_container: false,
                        is_bag_of_holding: false,
                        enchantment: b_item.enchantment,
                        erosion: 0,
                        proofed: false,
                        location: ItemLocation::Floor(target_coord),
                        corpse_race: None,
                        corpse_age: 0,
                        rot_threshold: 50,
                        recharged: 0,
                    };
                    self.arena.spawn_item(item_record);
                }
            }

            events.push(GameEvent::LogMessage {
                text: Messages::ghost_encounter(&bones.hero_name, self.locale),
            });
        }

        events
    }
}
