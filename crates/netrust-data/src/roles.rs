//! NetHack Role and Race definitions and character generator.

use crate::items::{create_item_record, ItemKindId};
use netrust_arena::{ActorId, ActorRecord, EntityArena, ItemId, ItemLocation};
use netrust_types::{Alignment, Buc, Coord, Intrinsics};
use serde::{Deserialize, Serialize};

/// Classic NetHack Player Roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RoleId {
    Valkyrie,
    Wizard,
    Barbarian,
    Rogue,
    Knight,
    Monk,
    Healer,
    Tourist,
    Archaeologist,
}

/// Player Character Races.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RaceId {
    Human,
    Elf,
    Dwarf,
    Gnome,
    Orc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Gender {
    Male,
    Female,
}

/// Declarative specification of a player class/role.
#[derive(Debug, Clone, Serialize)]
pub struct RoleSpec {
    pub id: RoleId,
    pub name: &'static str,
    pub base_hp: u32,
    pub ac: i32,
    pub speed: u32,
    pub default_alignment: Alignment,
    pub starting_items: &'static [ItemKindId],
}

pub static ROLES: &[RoleSpec] = &[
    RoleSpec {
        id: RoleId::Valkyrie,
        name: "Valkyrie",
        base_hp: 18,
        ac: 7,
        speed: 12,
        default_alignment: Alignment::Neutral,
        starting_items: &[ItemKindId::LongSword, ItemKindId::LeatherArmor, ItemKindId::PotionOfHealing],
    },
    RoleSpec {
        id: RoleId::Wizard,
        name: "Wizard",
        base_hp: 12,
        ac: 10,
        speed: 12,
        default_alignment: Alignment::Neutral,
        starting_items: &[ItemKindId::WandOfStriking, ItemKindId::ScrollOfIdentify, ItemKindId::CloakOfMagicResistance],
    },
    RoleSpec {
        id: RoleId::Barbarian,
        name: "Barbarian",
        base_hp: 20,
        ac: 6,
        speed: 12,
        default_alignment: Alignment::Chaotic,
        starting_items: &[ItemKindId::LongSword, ItemKindId::LeatherArmor],
    },
    RoleSpec {
        id: RoleId::Rogue,
        name: "Rogue",
        base_hp: 14,
        ac: 8,
        speed: 12,
        default_alignment: Alignment::Chaotic,
        starting_items: &[ItemKindId::Dagger, ItemKindId::ShortSword, ItemKindId::Sack],
    },
    RoleSpec {
        id: RoleId::Knight,
        name: "Knight",
        base_hp: 16,
        ac: 5,
        speed: 12,
        default_alignment: Alignment::Lawful,
        starting_items: &[ItemKindId::LongSword, ItemKindId::ChainMail],
    },
    RoleSpec {
        id: RoleId::Monk,
        name: "Monk",
        base_hp: 14,
        ac: 10,
        speed: 12,
        default_alignment: Alignment::Neutral,
        starting_items: &[ItemKindId::PotionOfHealing, ItemKindId::ScrollOfTeleportation],
    },
    RoleSpec {
        id: RoleId::Healer,
        name: "Healer",
        base_hp: 14,
        ac: 9,
        speed: 12,
        default_alignment: Alignment::Neutral,
        starting_items: &[ItemKindId::PotionOfHealing, ItemKindId::PotionOfExtraHealing],
    },
    RoleSpec {
        id: RoleId::Tourist,
        name: "Tourist",
        base_hp: 10,
        ac: 10,
        speed: 12,
        default_alignment: Alignment::Neutral,
        starting_items: &[ItemKindId::GoldPieces, ItemKindId::PotionOfExtraHealing, ItemKindId::BagOfHolding],
    },
    RoleSpec {
        id: RoleId::Archaeologist,
        name: "Archaeologist",
        base_hp: 14,
        ac: 8,
        speed: 12,
        default_alignment: Alignment::Lawful,
        starting_items: &[ItemKindId::ShortSword, ItemKindId::LeatherArmor, ItemKindId::Sack],
    },
];

/// Specification of a race and its passive intrinsics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaceSpec {
    pub id: RaceId,
    pub name: &'static str,
    pub intrinsics: Intrinsics,
}

pub static RACES: &[RaceSpec] = &[
    RaceSpec {
        id: RaceId::Human,
        name: "Human",
        intrinsics: Intrinsics::empty(),
    },
    RaceSpec {
        id: RaceId::Elf,
        name: "Elf",
        intrinsics: Intrinsics::empty().with_see_invisible(),
    },
    RaceSpec {
        id: RaceId::Dwarf,
        name: "Dwarf",
        intrinsics: Intrinsics::empty(),
    },
    RaceSpec {
        id: RaceId::Gnome,
        name: "Gnome",
        intrinsics: Intrinsics::empty(),
    },
    RaceSpec {
        id: RaceId::Orc,
        name: "Orc",
        intrinsics: Intrinsics::empty().with_poison_resistance(),
    },
];

pub fn get_role(id: RoleId) -> &'static RoleSpec {
    ROLES.iter().find(|r| r.id == id).expect("Role must exist")
}

pub fn get_race(id: RaceId) -> &'static RaceSpec {
    RACES.iter().find(|r| r.id == id).expect("Race must exist")
}

/// User-selected character creation profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterConfig {
    pub name: String,
    pub role: RoleId,
    pub race: RaceId,
    pub gender: Gender,
    pub alignment: Alignment,
}

impl Default for CharacterConfig {
    fn default() -> Self {
        Self {
            name: "Hero".to_string(),
            role: RoleId::Valkyrie,
            race: RaceId::Human,
            gender: Gender::Female,
            alignment: Alignment::Neutral,
        }
    }
}

/// Spawns a fully equipped player actor and starting inventory into the entity arena.
pub fn spawn_player_character(
    config: &CharacterConfig,
    coord: Coord,
    arena: &mut EntityArena,
) -> (ActorId, Vec<ItemId>) {
    let role = get_role(config.role);
    let race = get_race(config.race);

    let actor = ActorRecord {
        name: config.name.clone(),
        coord,
        hp: role.base_hp,
        max_hp: role.base_hp,
        ac: role.ac,
        level: 1,
        speed: role.speed,
        alignment: config.alignment,
        intrinsics: race.intrinsics,
        is_player: true,
        is_dead: false,
    };
    let player_id = arena.spawn_actor(actor);

    let mut item_ids = Vec::new();
    for &kind in role.starting_items {
        let item = create_item_record(kind, ItemLocation::CarriedBy(player_id), Buc::Uncursed);
        let id = arena.spawn_item(item);
        item_ids.push(id);
    }

    (player_id, item_ids)
}
