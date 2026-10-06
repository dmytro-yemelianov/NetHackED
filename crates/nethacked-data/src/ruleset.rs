//! Owned game definition types, vanilla ruleset generator, and indexed accessors.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use nethacked_arena::{ActorId, ActorRecord, EntityArena, ItemId, ItemLocation, ItemRecord};
use nethacked_core::ac::ArmorSlot;
use nethacked_core::quest::get_role_quest_config;
use nethacked_types::{
    Alignment, Attack, Buc, Coord, Intrinsics, ItemClass, MonsterAbility, SkillClass, SkillLevel,
};
use serde::{Deserialize, Serialize};

use crate::items::{get_item_archetype, ItemKindId, WandDir, ITEM_CATALOG};
use crate::monsters::{AiBehavior, MonsterSize, MonsterSound, MonsterSpeciesId, BESTIARY};
use crate::pantheons::get_pantheon_for_role;
use crate::roles::{
    starting_item_spe, starting_skills, CharacterConfig, RaceId, RoleId, RACES, ROLES,
};

/// Owned definition of a monster species.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct MonsterDef {
    #[serde(default)]
    pub id: Option<MonsterSpeciesId>,
    pub name: String,
    pub glyph: char,
    pub base_hp: u32,
    pub max_hp: u32,
    pub ac: i32,
    pub level: u32,
    pub speed: u32,
    pub alignment: Alignment,
    pub intrinsics: Intrinsics,
    pub attacks: Vec<Attack>,
    pub size: MonsterSize,
    pub peaceful_by_default: bool,
    pub always_hostile: bool,
    pub maligntyp: i8,
    pub msound: MonsterSound,
    pub m2_race: Option<RaceId>,
    pub is_human: bool,
    pub is_unique: bool,
    pub mindless: bool,
    pub ai_behavior: AiBehavior,
    pub abilities: Vec<MonsterAbility>,
    /// C `difficulty` (monsters.h): rndmonst() keeps species within the level's range.
    #[serde(default)]
    pub difficulty: u32,
    /// C `G_FREQ` weight for random generation; 0 = never generated at random.
    #[serde(default)]
    pub frequency: u32,
    /// C `G_*` flags, lowercase without prefix (`nogen`, `uniq`, `hell`, `nohell`, `genocide`, ...).
    #[serde(default)]
    pub gen_flags: Vec<String>,
    /// C `M1_*`/`M2_*`/`M3_*` flags, lowercase without prefix (`fly`, `nasty`, ...).
    #[serde(default)]
    pub flags: Vec<String>,
}

impl MonsterDef {
    /// Does the species carry the C monster flag `flag` (e.g. `"nasty"` for `M2_NASTY`)?
    pub fn has_flag(&self, flag: &str) -> bool {
        self.flags.iter().any(|f| f == flag)
    }
}

impl MonsterDef {
    /// Returns whether this monster is an undead species (skeleton, vampire, lich, master lich, ghost, zombie).
    pub fn is_undead(&self) -> bool {
        matches!(
            self.name.to_lowercase().as_str(),
            "skeleton" | "vampire" | "lich" | "master lich" | "ghost" | "zombie"
        )
    }

    /// Returns whether this monster is a demon species.
    pub fn is_demon(&self) -> bool {
        self.name.to_lowercase().contains("demon")
    }

    /// Returns whether this monster is considered demon or undead (e.g. for holy damage bonuses).
    pub fn is_demon_or_undead(&self) -> bool {
        self.is_demon() || self.is_undead()
    }

    /// Returns whether this monster is a temple priest or priest archetype.
    pub fn is_priest(&self) -> bool {
        self.name.to_lowercase().contains("priest")
    }

    /// Pet promotion tier for canine and feline pets (1 = little dog / kitten, 2 = dog / housecat, 3 = large dog / large cat).
    pub fn pet_tier(&self) -> Option<u32> {
        let n = self.name.to_lowercase();
        if n.contains("little dog") || n.contains("kitten") {
            Some(1)
        } else if n.contains("large dog") || n.contains("large cat") {
            Some(3)
        } else if n.contains("dog") || n.contains("housecat") {
            Some(2)
        } else {
            None
        }
    }
}

/// Owned definition of armor slot and base AC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ArmorDef {
    pub slot: ArmorSlot,
    pub base_ac: i32,
    /// C `a_can` (objects.h `oc2` for armor): magic cancellation level 0..=3.
    #[serde(default)]
    pub magic_cancellation: u32,
}

/// Owned definition of an item archetype.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ItemDef {
    #[serde(default)]
    pub id: Option<ItemKindId>,
    pub name: String,
    pub class: ItemClass,
    pub weight: u32,
    pub cost: u32,
    pub damage_small: (u32, u32),
    pub damage_large: (u32, u32),
    pub ac_bonus: i32,
    pub is_container: bool,
    pub is_bag_of_holding: bool,
    pub oc_magic: bool,
    pub wand_dir: Option<WandDir>,
    pub nutrition: u32,
    /// Armor slot and C base AC (from `nethacked_core::ac` table); `None` for non-armor.
    pub armor: Option<ArmorDef>,
}

impl ItemDef {
    /// Canonical weapon skill associated with this item archetype.
    pub fn weapon_skill(&self) -> Option<SkillClass> {
        let n = self.name.to_lowercase();
        if n.contains("dagger") {
            Some(SkillClass::Dagger)
        } else if n.contains("short sword") {
            Some(SkillClass::ShortSword)
        } else if n.contains("long sword") || n.contains("excalibur") || n.contains("vorpal blade")
        {
            Some(SkillClass::LongSword)
        } else if n.contains("crossbow") {
            Some(SkillClass::Crossbow)
        } else if n.contains("bow") {
            Some(SkillClass::Bow)
        } else if n.contains("club") {
            Some(SkillClass::Club)
        } else {
            None
        }
    }

    /// Returns the weapon base dice (small, large) for melee combat.
    pub fn weapon_damage_dice(&self) -> Option<(u32, u32)> {
        if self.damage_small.1 > 0 || self.damage_large.1 > 0 {
            Some((self.damage_small.1, self.damage_large.1))
        } else {
            None
        }
    }

    /// Returns whether consuming this item breaks vegetarian conduct (meat, corpse, tripe, etc.).
    pub fn is_meat(&self) -> bool {
        let n = self.name.to_lowercase();
        n.contains("corpse") || n.contains("tripe") || n.contains("meat")
    }

    /// Returns whether this item is a light source (lamp, lantern, candle).
    pub fn is_light_source(&self) -> bool {
        let n = self.name.to_lowercase();
        n.contains("lamp") || n.contains("lantern") || n.contains("candle")
    }

    /// Returns the light emission radius of this light source.
    pub fn light_radius(&self) -> u32 {
        let n = self.name.to_lowercase();
        if n.contains("lantern") {
            3
        } else if n.contains("candle") {
            1
        } else if n.contains("lamp") {
            2
        } else {
            0
        }
    }
}

/// Starting item grant for a player role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct StartingItem {
    pub item: String,
    #[serde(default)]
    pub spe: Option<i8>,
}

/// Quest trial configuration for a player role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct QuestDef {
    pub leader: String,
    pub nemesis: String,
    pub guardian: String,
    pub artifact: String,
    pub home_desc: String,
    pub goal_desc: String,
}

/// Owned definition of a player character role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct RoleDef {
    pub id: RoleId,
    pub name: String,
    pub base_hp: u32,
    pub ac: i32,
    pub speed: u32,
    pub default_alignment: Alignment,
    pub starting_items: Vec<StartingItem>,
    pub skills: Vec<(SkillClass, SkillLevel)>,
    pub pantheon: [String; 3],
    pub quest: Option<QuestDef>,
    /// NetHack 5.0 C `urole.initrecord` (`role.c`, `attrib.c:1094`).
    #[serde(default = "default_initial_alignment_record")]
    pub initial_alignment_record: i32,
}

fn default_initial_alignment_record() -> i32 {
    10
}

/// Owned definition of a player character race.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct RaceDef {
    pub id: RaceId,
    pub name: String,
    pub intrinsics: Intrinsics,
}

/// Metadata header for a rule pack or ruleset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PackManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub base: String,
    #[serde(default)]
    pub description: String,
}

/// Mechanics parameters section (reserved for P2).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct MechanicsSection {}

/// Reference and integrity hash for a ruleset.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct RulesetRef {
    pub id: String,
    pub version: String,
    pub hash: String,
}

impl RulesetRef {
    pub fn vanilla() -> Self {
        Self {
            id: "vanilla".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            hash: "vanilla".to_string(),
        }
    }
}

impl Default for RulesetRef {
    fn default() -> Self {
        Self::vanilla()
    }
}

#[derive(Debug, Clone, Default)]
struct RulesetIndex {
    monsters_by_name: HashMap<String, usize>,
    monsters_by_id: HashMap<MonsterSpeciesId, usize>,
    items_by_name: HashMap<String, usize>,
    items_by_id: HashMap<ItemKindId, usize>,
    roles_by_id: HashMap<RoleId, usize>,
    races_by_id: HashMap<RaceId, usize>,
}

/// Complete, self-contained game ruleset containing all entities, tables, and roles.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Ruleset {
    pub manifest: PackManifest,
    pub monsters: Vec<MonsterDef>,
    pub items: Vec<ItemDef>,
    pub roles: Vec<RoleDef>,
    pub races: Vec<RaceDef>,
    #[serde(default)]
    pub mechanics: MechanicsSection,
    #[serde(skip)]
    index: RulesetIndex,
}

impl PartialEq for Ruleset {
    fn eq(&self, other: &Self) -> bool {
        self.manifest == other.manifest
            && self.monsters == other.monsters
            && self.items == other.items
            && self.roles == other.roles
            && self.races == other.races
            && self.mechanics == other.mechanics
    }
}

static VANILLA: OnceLock<Arc<Ruleset>> = OnceLock::new();

impl Ruleset {
    /// Return the singleton vanilla NetHack 3.6.1 ruleset.
    pub fn vanilla() -> Arc<Ruleset> {
        Arc::clone(VANILLA.get_or_init(|| Arc::new(build_vanilla_ruleset())))
    }

    /// Construct a Ruleset from its individual components, automatically building lookup indices.
    pub fn from_parts(
        manifest: PackManifest,
        monsters: Vec<MonsterDef>,
        items: Vec<ItemDef>,
        roles: Vec<RoleDef>,
        races: Vec<RaceDef>,
        mechanics: MechanicsSection,
    ) -> Self {
        let mut rs = Self {
            manifest,
            monsters,
            items,
            roles,
            races,
            mechanics,
            index: RulesetIndex::default(),
        };
        rs.reindex();
        rs
    }

    /// Rebuild all internal lookup indices after deserialization or modification.
    pub fn reindex(&mut self) {
        let mut idx = RulesetIndex::default();
        for (i, m) in self.monsters.iter().enumerate() {
            idx.monsters_by_name.insert(m.name.to_ascii_lowercase(), i);
            if let Some(id) = m.id {
                idx.monsters_by_id.insert(id, i);
            }
        }
        for (i, it) in self.items.iter().enumerate() {
            idx.items_by_name.insert(it.name.to_ascii_lowercase(), i);
            if let Some(id) = it.id {
                idx.items_by_id.insert(id, i);
            }
        }
        for (i, r) in self.roles.iter().enumerate() {
            idx.roles_by_id.insert(r.id, i);
        }
        for (i, rc) in self.races.iter().enumerate() {
            idx.races_by_id.insert(rc.id, i);
        }
        self.index = idx;
    }

    /// Look up a monster by name, stripping runtime decorations ("hostile ", "ghost of ").
    pub fn monster(&self, name: &str) -> Option<&MonsterDef> {
        let lower = name.to_ascii_lowercase();
        let base = if lower.starts_with("ghost of ") {
            "ghost"
        } else {
            lower.strip_prefix("hostile ").unwrap_or(&lower)
        };
        self.index
            .monsters_by_name
            .get(base)
            .and_then(|&i| self.monsters.get(i))
    }

    /// Look up a monster by its canonical species ID.
    pub fn monster_by_id(&self, id: MonsterSpeciesId) -> Option<&MonsterDef> {
        self.index
            .monsters_by_id
            .get(&id)
            .and_then(|&i| self.monsters.get(i))
    }

    /// Look up an item by name (case-insensitive).
    pub fn item(&self, name: &str) -> Option<&ItemDef> {
        let lower = name.to_ascii_lowercase();
        self.index
            .items_by_name
            .get(&lower)
            .and_then(|&i| self.items.get(i))
    }

    /// Look up an item by its canonical item kind ID.
    pub fn item_by_id(&self, id: ItemKindId) -> Option<&ItemDef> {
        self.index
            .items_by_id
            .get(&id)
            .and_then(|&i| self.items.get(i))
    }

    /// Look up a player role by role ID.
    pub fn role(&self, id: RoleId) -> Option<&RoleDef> {
        self.index
            .roles_by_id
            .get(&id)
            .and_then(|&i| self.roles.get(i))
    }

    /// Look up a player race by race ID.
    pub fn race(&self, id: RaceId) -> Option<&RaceDef> {
        self.index
            .races_by_id
            .get(&id)
            .and_then(|&i| self.races.get(i))
    }

    /// Return the glyph/class character for a monster name.
    pub fn monster_class_of(&self, name: &str) -> Option<char> {
        self.monster(name).map(|m| m.glyph)
    }

    /// Spawn an ActorRecord from ruleset monster data.
    pub fn create_monster_record(&self, name: &str, coord: Coord) -> Option<ActorRecord> {
        let def = self.monster(name)?;
        Some(ActorRecord {
            name: def.name.clone(),
            coord,
            hp: def.base_hp,
            max_hp: def.max_hp,
            ac: def.ac,
            level: def.level,
            speed: def.speed,
            alignment: def.alignment,
            intrinsics: def.intrinsics,
            is_player: false,
            is_unique: def.is_unique,
            is_dead: false,
            is_tame: def.ai_behavior == AiBehavior::CompanionPet,
            tameness: if def.ai_behavior == AiBehavior::CompanionPet {
                5
            } else {
                0
            },
            abilities: def.abilities.clone(),
            is_peaceful: def.peaceful_by_default,
            mspec_used: 0,
            malign: 0,
        })
    }

    /// Spawn an ActorRecord by its canonical species ID.
    pub fn create_monster_record_by_id(
        &self,
        id: MonsterSpeciesId,
        coord: Coord,
    ) -> Option<ActorRecord> {
        let def = self.monster_by_id(id)?;
        self.create_monster_record(&def.name, coord)
    }

    /// Spawn an ItemRecord from ruleset item data.
    pub fn create_item_record(
        &self,
        name: &str,
        location: ItemLocation,
        buc: Buc,
    ) -> Option<ItemRecord> {
        let def = self.item(name)?;
        let charges = if def.class != ItemClass::Wand {
            0
        } else if def.id == Some(ItemKindId::WAN_WISHING) {
            1
        } else {
            match def.wand_dir {
                Some(WandDir::NoDir) => 13,
                _ => 6,
            }
        };
        Some(ItemRecord {
            name: def.name.clone(),
            class: def.class,
            weight: def.weight,
            buc,
            is_container: def.is_container,
            is_bag_of_holding: def.is_bag_of_holding,
            enchantment: charges,
            erosion: 0,
            proofed: false,
            location,
            corpse_race: None,
            corpse_age: 0,
            rot_threshold: 50,
            recharged: 0,
        })
    }

    /// Spawn an ItemRecord by its canonical item kind ID.
    pub fn create_item_record_by_id(
        &self,
        id: ItemKindId,
        location: ItemLocation,
        buc: Buc,
    ) -> Option<ItemRecord> {
        let def = self.item_by_id(id)?;
        self.create_item_record(&def.name, location, buc)
    }

    /// Spawn player actor and initial inventory into the entity arena.
    pub fn spawn_player_character(
        &self,
        cfg: &CharacterConfig,
        coord: Coord,
        arena: &mut EntityArena,
    ) -> (ActorId, Vec<ItemId>) {
        let role = self.role(cfg.role).expect("Role must exist in ruleset");
        let race = self.race(cfg.race).expect("Race must exist in ruleset");

        let actor = ActorRecord {
            name: cfg.name.clone(),
            coord,
            hp: role.base_hp,
            max_hp: role.base_hp,
            ac: role.ac,
            level: 1,
            speed: role.speed,
            alignment: cfg.alignment,
            intrinsics: race.intrinsics,
            is_player: true,
            is_unique: true,
            is_dead: false,
            is_tame: false,
            tameness: 0,
            abilities: Vec::new(),
            is_peaceful: false,
            mspec_used: 0,
            malign: 0,
        };
        let player_id = arena.spawn_actor(actor);

        let mut item_ids = Vec::new();
        for starting_item in &role.starting_items {
            if let Some(mut item) = self.create_item_record(
                &starting_item.item,
                ItemLocation::CarriedBy(player_id),
                Buc::Uncursed,
            ) {
                if let Some(spe) = starting_item.spe {
                    item.enchantment = spe;
                }
                let id = arena.spawn_item(item);
                item_ids.push(id);
            }
        }

        (player_id, item_ids)
    }
}

fn build_vanilla_ruleset() -> Ruleset {
    let manifest = PackManifest {
        id: "vanilla".to_string(),
        name: "Vanilla NetHack 3.6.1".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        base: "".to_string(),
        description: "Vanilla NetHack 3.6.1 ruleset".to_string(),
    };

    let monsters: Vec<MonsterDef> = BESTIARY
        .iter()
        .map(|arch| MonsterDef {
            id: Some(arch.id),
            name: arch.name.to_string(),
            glyph: arch.glyph,
            base_hp: arch.base_hp,
            max_hp: arch.max_hp,
            ac: arch.ac,
            level: arch.level,
            speed: arch.speed,
            alignment: arch.alignment,
            intrinsics: arch.intrinsics,
            attacks: arch.attacks.to_vec(),
            size: arch.size,
            peaceful_by_default: arch.peaceful_by_default,
            always_hostile: arch.always_hostile,
            maligntyp: arch.maligntyp,
            msound: arch.msound,
            m2_race: arch.m2_race,
            is_human: arch.is_human,
            is_unique: arch.is_unique,
            mindless: arch.mindless,
            ai_behavior: arch.ai_behavior,
            abilities: arch.abilities.to_vec(),
            difficulty: arch.difficulty,
            frequency: arch.frequency,
            gen_flags: arch.gen_flags.iter().map(|s| s.to_string()).collect(),
            flags: arch.flags.iter().map(|s| s.to_string()).collect(),
        })
        .collect();

    let items: Vec<ItemDef> = ITEM_CATALOG
        .iter()
        .map(|arch| {
            let armor = arch.armor_slot.map(|slot| ArmorDef {
                slot,
                base_ac: arch.ac_bonus,
                magic_cancellation: arch.magic_cancellation,
            });
            ItemDef {
                id: Some(arch.id),
                name: arch.name.to_string(),
                class: arch.class,
                weight: arch.weight,
                cost: arch.cost,
                damage_small: arch.damage_small,
                damage_large: arch.damage_large,
                ac_bonus: arch.ac_bonus,
                is_container: arch.is_container,
                is_bag_of_holding: arch.is_bag_of_holding,
                oc_magic: arch.oc_magic,
                wand_dir: arch.wand_dir,
                nutrition: arch.nutrition,
                armor,
            }
        })
        .collect();

    let roles: Vec<RoleDef> = ROLES
        .iter()
        .map(|role| {
            let starting_items = role
                .starting_items
                .iter()
                .map(|&kind| {
                    let item_arch = get_item_archetype(kind);
                    StartingItem {
                        item: item_arch.name.to_string(),
                        spe: starting_item_spe(role.id, kind),
                    }
                })
                .collect();
            let skills = starting_skills(role.id);
            let pantheon = get_pantheon_for_role(role.id);
            let quest = get_role_quest_config(role.name).map(|q| QuestDef {
                leader: q.leader_name.to_string(),
                nemesis: q.nemesis_name.to_string(),
                guardian: q.guardian_name.to_string(),
                artifact: q.artifact_name.to_string(),
                home_desc: q.home_desc.to_string(),
                goal_desc: q.goal_desc.to_string(),
            });

            RoleDef {
                id: role.id,
                name: role.name.to_string(),
                base_hp: role.base_hp,
                ac: role.ac,
                speed: role.speed,
                default_alignment: role.default_alignment,
                starting_items,
                skills,
                pantheon: [
                    pantheon.lawful.name,
                    pantheon.neutral.name,
                    pantheon.chaotic.name,
                ],
                quest,
                initial_alignment_record: role.initial_alignment_record,
            }
        })
        .collect();

    let races: Vec<RaceDef> = RACES
        .iter()
        .map(|race| RaceDef {
            id: race.id,
            name: race.name.to_string(),
            intrinsics: race.intrinsics,
        })
        .collect();

    Ruleset::from_parts(
        manifest,
        monsters,
        items,
        roles,
        races,
        MechanicsSection::default(),
    )
}

/// Canonical list of monsters required by core simulation mechanics.
pub const ENGINE_REQUIRED_MONSTERS: &[&str] = &[
    "shopkeeper",
    "ghost",
    "djinni",
    "little dog",
    "kitten",
    "Norn",
    "Neferet the Green",
    "Pelias",
    "King Arthur",
    "Grand Master",
    "Master Assassin",
    "Hippocrates",
    "Twoflower",
    "Lord Carnarvon",
    "Lord Surtur",
    "Dark One",
    "Thoth Amon",
    "Ixoth",
    "Master Kaen",
    "Master of Thieves",
    "Cyclops",
    "Minion of Huhetotl",
    "student",
    "chieftain",
    "attendant",
    "page",
    "abbot",
    "thug",
    "guide",
    "warrior",
    "apprentice",
    "Wizard of Yendor",
    "Vlad the Impaler",
    "Croesus",
    "Medusa",
    "master lich",
];

/// Canonical list of items required by core simulation mechanics.
pub const ENGINE_REQUIRED_ITEMS: &[&str] = &[
    "Amulet of Yendor",
    "Bell of Opening",
    "Candelabrum of Invocation",
    "Book of the Dead",
    "boulder",
    "gold piece",
    "luckstone",
    "The Orb of Fate",
    "The Eye of the Aethiopica",
    "The Heart of Ahriman",
    "The Magic Mirror of Merlin",
    "The Eyes of the Overworld",
    "The Master Key of Thievery",
    "The Tsurugi of Muramasa",
    "The Platinum Yendorian Express Card",
    "The Staff of Aesculapius",
    "The Orb of Detection",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_def_serde_default_initial_alignment_record() {
        let json_without_field = r#"{
            "id": "Barbarian",
            "name": "Barbarian",
            "base_hp": 20,
            "ac": 10,
            "speed": 12,
            "default_alignment": "Chaotic",
            "starting_items": [],
            "skills": [],
            "pantheon": ["Mitra", "Crom", "Set"],
            "quest": null
        }"#;

        let role_def: RoleDef =
            serde_json::from_str(json_without_field).expect("deserialization should succeed");
        assert_eq!(
            role_def.initial_alignment_record, 10,
            "omitted initial_alignment_record must default to 10"
        );
    }

    #[test]
    fn test_vanilla_roles_canonical_initrecord_values() {
        let ruleset = Ruleset::vanilla();
        for role in &ruleset.roles {
            let expected = match role.id {
                RoleId::Valkyrie | RoleId::Wizard | RoleId::Tourist => 0,
                RoleId::Barbarian
                | RoleId::Rogue
                | RoleId::Knight
                | RoleId::Monk
                | RoleId::Healer
                | RoleId::Archaeologist => 10,
            };
            assert_eq!(
                role.initial_alignment_record, expected,
                "role {:?} initial_alignment_record should match NetHack C urole.initrecord",
                role.id
            );
        }
    }
}
