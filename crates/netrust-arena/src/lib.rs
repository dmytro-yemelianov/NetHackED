//! Generational handle-based entity storage for NetRust.
//!
//! Replaces NetHack's intrusive `union vptrs` and linked lists with type-safe generational handles.

use netrust_types::{Alignment, Buc, Coord, Intrinsics, ItemClass, MonsterAbility};
use serde::{Deserialize, Serialize};
use slotmap::SlotMap;

pub use netrust_types::{ActorId, ItemId, LevelId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemLocation {
    Floor(Coord),
    InContainer(ItemId),
    CarriedBy(ActorId),
    Limbo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemRecord {
    pub name: String,
    pub class: ItemClass,
    pub weight: u32,
    pub buc: Buc,
    pub is_container: bool,
    pub is_bag_of_holding: bool,
    pub enchantment: i8,
    pub erosion: u8,
    pub proofed: bool,
    pub location: ItemLocation,
    pub corpse_race: Option<String>,
    pub corpse_age: u32,
    pub rot_threshold: u32,
    /// Times recharged (C `obj->recharged`, read.c:729); distinct from `erosion`.
    #[serde(default)]
    pub recharged: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorRecord {
    pub name: String,
    pub coord: Coord,
    pub hp: u32,
    pub max_hp: u32,
    pub ac: i32,
    pub level: u32,
    pub speed: u32,
    pub alignment: Alignment,
    pub intrinsics: Intrinsics,
    pub is_player: bool,
    pub is_unique: bool,
    pub is_dead: bool,
    pub is_tame: bool,
    pub tameness: u32,
    pub abilities: Vec<MonsterAbility>,
}

/// The centralized entity arena replacing all ambient pointers.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct EntityArena {
    pub items: SlotMap<ItemId, ItemRecord>,
    pub actors: SlotMap<ActorId, ActorRecord>,
}

impl EntityArena {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert an item into the arena.
    pub fn spawn_item(&mut self, item: ItemRecord) -> ItemId {
        self.items.insert(item)
    }

    /// Insert an actor into the arena.
    pub fn spawn_actor(&mut self, actor: ActorRecord) -> ActorId {
        self.actors.insert(actor)
    }

    /// Safely remove an item.
    pub fn destroy_item(&mut self, id: ItemId) -> Option<ItemRecord> {
        self.items.remove(id)
    }

    /// Safely remove an actor.
    pub fn destroy_actor(&mut self, id: ActorId) -> Option<ActorRecord> {
        self.actors.remove(id)
    }

    /// Query all items directly on the floor at a coordinate.
    pub fn items_at_floor(&self, coord: Coord) -> Vec<ItemId> {
        self.items
            .iter()
            .filter_map(|(id, item)| {
                if item.location == ItemLocation::Floor(coord) {
                    Some(id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Query all items carried by a specific actor.
    pub fn items_carried_by(&self, actor: ActorId) -> Vec<ItemId> {
        self.items
            .iter()
            .filter_map(|(id, item)| {
                if item.location == ItemLocation::CarriedBy(actor) {
                    Some(id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Query all items directly contained within a container.
    pub fn items_in_container(&self, container: ItemId) -> Vec<ItemId> {
        self.items
            .iter()
            .filter_map(|(id, item)| {
                if item.location == ItemLocation::InContainer(container) {
                    Some(id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Check if container contains item (directly or transitively) to prevent cycles.
    pub fn contains_transitive(&self, container: ItemId, target: ItemId) -> bool {
        let mut queue = vec![container];
        while let Some(current) = queue.pop() {
            if current == target {
                return true;
            }
            for child in self.items_in_container(current) {
                if child == target {
                    return true;
                }
                if self.items.get(child).is_some_and(|it| it.is_container) {
                    queue.push(child);
                }
            }
        }
        false
    }

    /// Calculate the recursive weight of an item or container in the arena.
    pub fn calculate_total_weight(&self, id: ItemId) -> u32 {
        let Some(item) = self.items.get(id) else {
            return 0;
        };

        if !item.is_container {
            return item.weight;
        }

        let inner_weight: u32 = self
            .items_in_container(id)
            .into_iter()
            .map(|child| self.calculate_total_weight(child))
            .sum();

        let effective_inner = if item.is_bag_of_holding {
            // NetHack 5.0 rounding up: (cwt + 1) / 2 for uncursed default
            inner_weight.div_ceil(2)
        } else {
            inner_weight
        };

        item.weight + effective_inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_record_without_recharged_deserializes_to_zero() {
        let mut arena = EntityArena::new();
        let id = arena.spawn_item(ItemRecord {
            name: "wand of striking".into(),
            class: ItemClass::Wand,
            weight: 7,
            buc: Buc::Uncursed,
            is_container: false,
            is_bag_of_holding: false,
            enchantment: 4,
            erosion: 1,
            proofed: false,
            location: ItemLocation::Floor(Coord::new(1, 1).unwrap()),
            corpse_race: None,
            corpse_age: 0,
            rot_threshold: 50,
            recharged: 3,
        });
        let mut v = serde_json::to_value(arena.items.get(id).unwrap()).unwrap();
        assert_eq!(v["recharged"], 3);
        v.as_object_mut().unwrap().remove("recharged");
        let back: ItemRecord = serde_json::from_value(v).unwrap();
        assert_eq!(back.recharged, 0);
        assert_eq!(back.erosion, 1);
    }

    #[test]
    fn test_spawn_and_retrieve_actor() {
        let mut arena = EntityArena::new();
        let actor = ActorRecord {
            name: "Agent".into(),
            coord: Coord::new(5, 5).unwrap(),
            hp: 20,
            max_hp: 20,
            ac: 10,
            level: 1,
            speed: 12,
            alignment: Alignment::Neutral,
            intrinsics: Intrinsics::default(),
            is_player: true,
            is_dead: false,
            is_tame: false,
            tameness: 0,
            is_unique: false,
            abilities: Vec::new(),
        };
        let id = arena.spawn_actor(actor);
        assert_eq!(arena.actors.get(id).unwrap().hp, 20);

        arena.destroy_actor(id);
        assert!(arena.actors.get(id).is_none());
    }

    #[test]
    fn test_container_hierarchy_and_weight() {
        let mut arena = EntityArena::new();
        let dagger = arena.spawn_item(ItemRecord {
            name: "dagger".into(),
            class: ItemClass::Weapon,
            weight: 12,
            buc: Buc::Uncursed,
            is_container: false,
            is_bag_of_holding: false,
            enchantment: 0,
            erosion: 0,
            proofed: false,
            location: ItemLocation::Limbo,
            corpse_race: None,
            corpse_age: 0,
            rot_threshold: 50,
            recharged: 0,
        });

        let boh = arena.spawn_item(ItemRecord {
            name: "bag of holding".into(),
            class: ItemClass::Tool,
            weight: 15,
            buc: Buc::Uncursed,
            is_container: true,
            is_bag_of_holding: true,
            enchantment: 0,
            erosion: 0,
            proofed: false,
            location: ItemLocation::Floor(Coord::new(10, 10).unwrap()),
            corpse_race: None,
            corpse_age: 0,
            rot_threshold: 50,
            recharged: 0,
        });

        // Place dagger inside bag
        arena.items.get_mut(dagger).unwrap().location = ItemLocation::InContainer(boh);

        assert_eq!(arena.items_in_container(boh), vec![dagger]);
        assert_eq!(arena.items_at_floor(Coord::new(10, 10).unwrap()), vec![boh]);

        // Inner weight 12 -> (12 + 1) / 2 = 6. Base weight 15 -> total 21.
        assert_eq!(arena.calculate_total_weight(boh), 21);
    }

    #[test]
    fn test_cycle_detection() {
        let mut arena = EntityArena::new();
        let box1 = arena.spawn_item(ItemRecord {
            name: "box 1".into(),
            class: ItemClass::Tool,
            weight: 10,
            buc: Buc::Uncursed,
            is_container: true,
            is_bag_of_holding: false,
            enchantment: 0,
            erosion: 0,
            proofed: false,
            location: ItemLocation::Limbo,
            corpse_race: None,
            corpse_age: 0,
            rot_threshold: 50,
            recharged: 0,
        });
        let box2 = arena.spawn_item(ItemRecord {
            name: "box 2".into(),
            class: ItemClass::Tool,
            weight: 10,
            buc: Buc::Uncursed,
            is_container: true,
            is_bag_of_holding: false,
            enchantment: 0,
            erosion: 0,
            proofed: false,
            location: ItemLocation::InContainer(box1),
            corpse_race: None,
            corpse_age: 0,
            rot_threshold: 50,
            recharged: 0,
        });

        assert!(arena.contains_transitive(box1, box2));
        assert!(!arena.contains_transitive(box2, box1));
    }
}
