//! Generational handle-based entity storage for NetHackED.
//!
//! Replaces NetHack's intrusive `union vptrs` and linked lists with type-safe generational handles.

use nethacked_types::{Alignment, Buc, Coord, Intrinsics, ItemClass, MonsterAbility};
use serde::{Deserialize, Serialize};
use slotmap::SlotMap;

pub use nethacked_types::{ActorId, ItemDefinition, ItemId, LevelId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemLocation {
    Floor(Coord),
    InContainer(ItemId),
    CarriedBy(ActorId),
    Limbo,
}

/// Stable species identity, independent of table order and display names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CorpseSpecies {
    /// Explicit corpse factory without supplied species (no invented identity).
    Unspecified,
    /// Canonical C PM_* symbol, with its prefix omitted.
    Vanilla(String),
    /// Owned ruleset key for a pack-added species.
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemRecord {
    /// Owned archetype identity; absence retains legacy exact-name lookup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<ItemDefinition>,
    /// Detached supported monster state (C oextra->omonst), never a live actor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_monster_traits: Option<Box<SavedMonsterTraits>>,
    /// Canonical C ART_* symbol; independent of display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
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
    /// Species identity is distinct from the race used for cannibalism.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corpse_species: Option<CorpseSpecies>,
    /// Persisted C obj->norevive; independent of corpse age and species.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub corpse_no_revive: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_ice_box: bool,
    pub corpse_race: Option<String>,
    pub corpse_age: u32,
    pub rot_threshold: u32,
    /// Times recharged (C `obj->recharged`, read.c:729); distinct from `erosion`.
    #[serde(default)]
    pub recharged: u8,
    /// Stack size; weight stores the total stack weight (C obj->owt).
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub quantity: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub poisoned: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub trapped: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub trap_known: bool,
}

/// C pickup.c:64,2644–2663 freezes only an object directly in an ice box.
/// An enclosing ice box does not freeze a corpse inside an ordinary sack.
pub fn frozen_in_ice_box<'a>(
    item: &ItemRecord,
    lookup: impl Fn(ItemId) -> Option<&'a ItemRecord>,
) -> bool {
    match item.location {
        ItemLocation::InContainer(id) => lookup(id).is_some_and(|parent| parent.is_ice_box),
        _ => false,
    }
}

impl ItemRecord {
    /// Legacy records with a supplied race remain corpses even without payload.
    pub fn is_corpse(&self) -> bool {
        self.corpse_species.is_some() || self.corpse_race.is_some()
    }
}

fn one() -> u32 {
    1
}
fn is_one(value: &u32) -> bool {
    *value == 1
}

/// Persisted species key; display names may change independently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActorSpecies {
    /// Canonical C PM_* symbol, without its prefix.
    Vanilla(String),
    /// Case-normalized exact owned definition name, without runtime decorations.
    Custom(String),
}

/// Actual persisted actor sex; absence records a legacy or unsupplied unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActorSex {
    Male,
    Female,
    Neuter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sex: Option<ActorSex>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub species: Option<ActorSpecies>,
    pub name: String,
    pub coord: Coord,
    pub hp: u32,
    pub max_hp: u32,
    pub ac: i32,
    pub level: u32,
    pub speed: u32,
    /// C per-monster movement credit; constructors and legacy loads start at zero.
    #[serde(default)]
    pub movement: u32,
    pub alignment: Alignment,
    pub intrinsics: Intrinsics,
    pub is_player: bool,
    pub is_unique: bool,
    pub is_dead: bool,
    pub is_tame: bool,
    pub tameness: u32,
    pub abilities: Vec<MonsterAbility>,
    /// NetHack 5.0 C `mpeaceful` (`makemon.c:1299`). Monster does not attack or approach hero.
    #[serde(default)]
    pub is_peaceful: bool,
    /// C `mspec_used` (`monst.h`): turns until a special attack (breath) can
    /// be used again. Set by `breamm` (`mthrowu.c:1132`), decremented once per
    /// turn by `mon_regen` (`monmove.c:311`).
    #[serde(default)]
    pub mspec_used: u8,
    /// C `malign` (`makemon.c:2320-2366`). Precalculated alignment adjustment upon death.
    #[serde(default)]
    pub malign: i32,
    /// C `mflee`, including untimed flight after a successful theft.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_fleeing: bool,
    /// C `mcan`: monster cancellation is independent of hero armor MC.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_cancelled: bool,
}

/// Owned saved traits for a corpse or statue. ActorRecord contains no arena
/// handles: inventory, weapons and unsupported mextra are excluded. Per-actor
/// movement credit is copied, matching the C full-struct traits copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedMonsterTraits {
    pub actor: ActorRecord,
}

impl SavedMonsterTraits {
    /// Copy supported state without attaching it to an object or active arena.
    /// `base_level` must come from the actor's owned monster definition.
    /// C mkobj.c:2157–2193 keeps zero HP and clamps a drained maximum/current HP.
    pub fn from_actor(actor: &ActorRecord, base_level: u32) -> Self {
        let mut actor = actor.clone();
        if actor.max_hp <= base_level {
            actor.max_hp = base_level.saturating_add(1);
        }
        actor.hp = actor.hp.min(actor.max_hp);
        Self { actor }
    }
}

/// Native item allocation state, including generational slots.
pub type ItemStorage = SlotMap<ItemId, ItemRecord>;
/// Native actor allocation state, including generational slots.
pub type ActorStorage = SlotMap<ActorId, ActorRecord>;

/// The centralized entity arena replacing all ambient pointers.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct EntityArena {
    /// All artifacts ever created, including destroyed and cached objects.
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub artifact_history: std::collections::BTreeSet<String>,
    pub items: ItemStorage,
    pub actors: ActorStorage,
}

impl EntityArena {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert an item into the arena.
    pub fn spawn_item(&mut self, item: ItemRecord) -> ItemId {
        self.register_artifact(&item);
        self.items.insert(item)
    }

    pub fn register_artifact(&mut self, item: &ItemRecord) {
        if let Some(id) = &item.artifact_id {
            self.artifact_history.insert(id.clone());
        }
    }

    /// Insert an actor into the arena.
    pub fn spawn_actor(&mut self, actor: ActorRecord) -> ActorId {
        self.actors.insert(actor)
    }

    /// Safely remove an item.
    pub fn destroy_item(&mut self, id: ItemId) -> Option<ItemRecord> {
        let item = self.items.remove(id)?;
        self.register_artifact(&item);
        Some(item)
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

    /// Advance represented elapsed corpse age, once per world turn. Ice boxes
    /// preserve the current age; removal resumes from that preserved value.
    pub fn advance_corpse_ages(&mut self) {
        let advancing: Vec<_> = self
            .items
            .iter()
            .filter_map(|(id, item)| {
                (item.is_corpse() && !frozen_in_ice_box(item, |parent| self.items.get(parent)))
                    .then_some(id)
            })
            .collect();
        for id in advancing {
            if let Some(item) = self.items.get_mut(id) {
                item.corpse_age = item.corpse_age.saturating_add(1);
            }
        }
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
            .fold(0u32, |weight, child| {
                weight.saturating_add(self.calculate_total_weight(child))
            });

        let effective_inner = if item.is_bag_of_holding {
            // NetHack 5.0 rounding up: (cwt + 1) / 2 for uncursed default
            match item.buc {
                Buc::Blessed => inner_weight.div_ceil(4),
                Buc::Uncursed => inner_weight.div_ceil(2),
                Buc::Cursed => inner_weight.saturating_mul(2),
            }
        } else {
            inner_weight
        };

        item.weight.saturating_add(effective_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_record_without_recharged_deserializes_to_zero() {
        let mut arena = EntityArena::new();
        let id = arena.spawn_item(ItemRecord {
            definition: None,
            saved_monster_traits: None,
            artifact_id: None,
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
            corpse_no_revive: false,
            corpse_species: None,
            is_ice_box: false,
            corpse_race: None,
            corpse_age: 0,
            rot_threshold: 50,
            recharged: 3,
            quantity: 1,
            poisoned: false,
            locked: false,
            trapped: false,
            trap_known: false,
        });
        let mut v = serde_json::to_value(arena.items.get(id).unwrap()).unwrap();
        assert_eq!(v["recharged"], 3);
        v.as_object_mut().unwrap().remove("recharged");
        let back: ItemRecord = serde_json::from_value(v).unwrap();
        assert_eq!(back.recharged, 0);
        assert_eq!(back.erosion, 1);
        assert_eq!(back.quantity, 1);
        assert!(!back.poisoned);
        let mut old = serde_json::to_value(&back).unwrap();
        assert!(old.get("quantity").is_none());
        assert!(old.get("poisoned").is_none());
        old.as_object_mut().unwrap().remove("quantity");
        old.as_object_mut().unwrap().remove("poisoned");
        let mut loaded: ItemRecord = serde_json::from_value(old).unwrap();
        assert_eq!(loaded.quantity, 1);
        assert!(!loaded.poisoned);
        assert!(!loaded.locked && !loaded.trapped && !loaded.trap_known);
        loaded.quantity = 9;
        loaded.poisoned = true;
        loaded.locked = true;
        loaded.trapped = true;
        loaded.trap_known = true;
        let encoded = serde_json::to_string(&loaded).unwrap();
        assert_eq!(
            serde_json::from_str::<ItemRecord>(&encoded).unwrap(),
            loaded
        );
    }

    #[test]
    fn corpse_no_revive_defaults_omits_false_and_persists_true() {
        let legacy = serde_json::json!({
            "name": "human corpse", "class": "Food", "weight": 1450,
            "buc": "Uncursed", "is_container": false, "is_bag_of_holding": false,
            "enchantment": 0, "erosion": 0, "proofed": false, "location": "Limbo",
            "corpse_race": "human", "corpse_age": 0, "rot_threshold": 250
        });
        let mut loaded: ItemRecord = serde_json::from_value(legacy).unwrap();
        assert!(!loaded.corpse_no_revive);
        let value = serde_json::to_value(&loaded).unwrap();
        assert!(value.get("corpse_no_revive").is_none());
        loaded.corpse_no_revive = true;
        let value = serde_json::to_value(&loaded).unwrap();
        assert_eq!(value["corpse_no_revive"], true);
        assert_eq!(serde_json::from_value::<ItemRecord>(value).unwrap(), loaded);
        loaded.corpse_no_revive = false;
        assert!(serde_json::to_value(&loaded)
            .unwrap()
            .get("corpse_no_revive")
            .is_none());
    }

    #[test]
    fn test_spawn_and_retrieve_actor() {
        let mut arena = EntityArena::new();
        let actor = ActorRecord {
            sex: None,
            species: None,
            name: "Agent".into(),
            coord: Coord::new(5, 5).unwrap(),
            hp: 20,
            max_hp: 20,
            ac: 10,
            level: 1,
            speed: 12,
            movement: 0,
            alignment: Alignment::Neutral,
            intrinsics: Intrinsics::default(),
            is_player: true,
            is_dead: false,
            is_tame: false,
            tameness: 0,
            is_unique: false,
            abilities: Vec::new(),
            is_peaceful: false,
            mspec_used: 0,
            malign: 0,
            is_fleeing: false,
            is_cancelled: false,
        };
        let id = arena.spawn_actor(actor);
        assert_eq!(arena.actors.get(id).unwrap().hp, 20);

        arena.destroy_actor(id);
        assert!(arena.actors.get(id).is_none());
    }

    #[test]
    fn test_actor_record_serde_default_is_peaceful() {
        let mut arena = EntityArena::new();
        let actor = ActorRecord {
            sex: None,
            species: None,
            name: "priest".into(),
            coord: Coord::new(5, 5).unwrap(),
            hp: 60,
            max_hp: 60,
            ac: 10,
            level: 12,
            speed: 12,
            movement: 0,
            alignment: Alignment::Neutral,
            intrinsics: Intrinsics::default(),
            is_player: false,
            is_dead: false,
            is_tame: false,
            tameness: 0,
            is_unique: false,
            abilities: Vec::new(),
            is_peaceful: true,
            mspec_used: 0,
            malign: 0,
            is_fleeing: false,
            is_cancelled: false,
        };
        let id = arena.spawn_actor(actor);
        let mut v = serde_json::to_value(arena.actors.get(id).unwrap()).unwrap();
        assert_eq!(v["is_peaceful"], true);
        v.as_object_mut().unwrap().remove("is_peaceful");
        let back: ActorRecord = serde_json::from_value(v).unwrap();
        assert!(!back.is_peaceful);
        assert_eq!(back.name, "priest");
    }

    #[test]
    fn test_actor_record_serde_default_mspec_used() {
        let mut arena = EntityArena::new();
        let id = arena.spawn_actor(ActorRecord {
            sex: None,
            species: None,
            name: "red dragon".into(),
            coord: Coord::new(5, 5).unwrap(),
            hp: 80,
            max_hp: 80,
            ac: -1,
            level: 15,
            speed: 9,
            movement: 0,
            alignment: Alignment::Chaotic,
            intrinsics: Intrinsics::default(),
            is_player: false,
            is_dead: false,
            is_tame: false,
            tameness: 0,
            is_unique: false,
            abilities: Vec::new(),
            is_peaceful: false,
            mspec_used: 12,
            malign: 5,
            is_fleeing: false,
            is_cancelled: false,
        });
        let mut v = serde_json::to_value(arena.actors.get(id).unwrap()).unwrap();
        assert_eq!(v["mspec_used"], 12);
        // A save written before the field existed loads with no cooldown.
        v.as_object_mut().unwrap().remove("mspec_used");
        let back: ActorRecord = serde_json::from_value(v).unwrap();
        assert_eq!(back.mspec_used, 0);
        assert_eq!(back.name, "red dragon");
    }

    #[test]
    fn test_actor_record_serde_default_malign() {
        let mut arena = EntityArena::new();
        let id = arena.spawn_actor(ActorRecord {
            sex: None,
            species: None,
            name: "goblin".into(),
            coord: Coord::new(5, 5).unwrap(),
            hp: 8,
            max_hp: 8,
            ac: 10,
            level: 0,
            speed: 6,
            movement: 0,
            alignment: Alignment::Chaotic,
            intrinsics: Intrinsics::default(),
            is_player: false,
            is_dead: false,
            is_tame: false,
            tameness: 0,
            is_unique: false,
            abilities: Vec::new(),
            is_peaceful: false,
            mspec_used: 0,
            malign: 3,
            is_fleeing: false,
            is_cancelled: false,
        });
        let mut v = serde_json::to_value(arena.actors.get(id).unwrap()).unwrap();
        assert_eq!(v["malign"], 3);
        // A save written before malign existed loads with default 0.
        v.as_object_mut().unwrap().remove("malign");
        let back: ActorRecord = serde_json::from_value(v).unwrap();
        assert_eq!(back.malign, 0);
        assert_eq!(back.name, "goblin");
    }

    #[test]
    fn test_container_hierarchy_and_weight() {
        let mut arena = EntityArena::new();
        let dagger = arena.spawn_item(ItemRecord {
            definition: None,
            saved_monster_traits: None,
            artifact_id: None,
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
            corpse_no_revive: false,
            corpse_species: None,
            is_ice_box: false,
            corpse_race: None,
            corpse_age: 0,
            rot_threshold: 50,
            recharged: 0,
            quantity: 1,
            poisoned: false,
            locked: false,
            trapped: false,
            trap_known: false,
        });

        let boh = arena.spawn_item(ItemRecord {
            definition: None,
            saved_monster_traits: None,
            artifact_id: None,
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
            corpse_no_revive: false,
            corpse_species: None,
            is_ice_box: false,
            corpse_race: None,
            corpse_age: 0,
            rot_threshold: 50,
            recharged: 0,
            quantity: 1,
            poisoned: false,
            locked: false,
            trapped: false,
            trap_known: false,
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
            definition: None,
            saved_monster_traits: None,
            artifact_id: None,
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
            corpse_no_revive: false,
            corpse_species: None,
            is_ice_box: false,
            corpse_race: None,
            corpse_age: 0,
            rot_threshold: 50,
            recharged: 0,
            quantity: 1,
            poisoned: false,
            locked: false,
            trapped: false,
            trap_known: false,
        });
        let box2 = arena.spawn_item(ItemRecord {
            definition: None,
            saved_monster_traits: None,
            artifact_id: None,
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
            corpse_no_revive: false,
            corpse_species: None,
            is_ice_box: false,
            corpse_race: None,
            corpse_age: 0,
            rot_threshold: 50,
            recharged: 0,
            quantity: 1,
            poisoned: false,
            locked: false,
            trapped: false,
            trap_known: false,
        });

        assert!(arena.contains_transitive(box1, box2));
        assert!(!arena.contains_transitive(box2, box1));
    }
    #[test]
    fn actor_sex_legacy_wire_omits_unknown_and_round_trips_known() {
        let actor = ActorRecord {
            sex: None,
            species: None,
            name: "priest".into(),
            coord: Coord::new(5, 5).unwrap(),
            hp: 60,
            max_hp: 60,
            ac: 10,
            level: 12,
            speed: 12,
            movement: 0,
            alignment: Alignment::Neutral,
            intrinsics: Intrinsics::default(),
            is_player: false,
            is_dead: false,
            is_tame: false,
            tameness: 0,
            is_unique: false,
            abilities: Vec::new(),
            is_peaceful: true,
            mspec_used: 0,
            malign: 0,
            is_fleeing: false,
            is_cancelled: false,
        };
        let old = serde_json::to_value(&actor).unwrap();
        assert!(old.get("sex").is_none());
        let loaded: ActorRecord = serde_json::from_value(old.clone()).unwrap();
        assert_eq!(loaded.sex, None);
        assert_eq!(serde_json::to_value(&loaded).unwrap(), old);
        for sex in [ActorSex::Male, ActorSex::Female, ActorSex::Neuter] {
            let mut known = loaded.clone();
            known.sex = Some(sex);
            let value = serde_json::to_value(&known).unwrap();
            assert_eq!(value["sex"], format!("{sex:?}"));
            assert_eq!(serde_json::from_value::<ActorRecord>(value).unwrap(), known);
        }
    }
    #[test]
    fn saved_monster_traits_absent_literal_wire_is_unchanged() {
        // A complete literal item wire from before saved traits existed.
        let old = serde_json::json!({
            "name": "troll corpse", "class": "Food", "weight": 800,
            "buc": "Uncursed", "is_container": false,
            "is_bag_of_holding": false, "enchantment": 1, "erosion": 0,
            "proofed": false, "location": "Limbo", "corpse_race": null,
            "corpse_age": 23, "rot_threshold": 50, "recharged": 0
        });
        let encoded = serde_json::to_vec(&old).unwrap();
        let item: ItemRecord = serde_json::from_slice(&encoded).unwrap();
        assert!(item.saved_monster_traits.is_none());
        assert_eq!(serde_json::to_value(&item).unwrap(), old);
        assert_eq!(
            serde_json::to_vec(&serde_json::to_value(&item).unwrap()).unwrap(),
            encoded
        );
    }
}
