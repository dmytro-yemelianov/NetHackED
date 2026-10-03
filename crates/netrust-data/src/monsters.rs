//! Declarative monster bestiary and species registry for NetRust.

use netrust_arena::ActorRecord;
use netrust_types::{Alignment, Coord, Intrinsics};
use serde::{Deserialize, Serialize};

/// Enumeration of canonical monster species.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MonsterSpeciesId {
    Goblin,
    Hobgoblin,
    Orc,
    Kobold,
    Jackal,
    GiantAnt,
    FloatingEye,
    Skeleton,
    Vampire,
    SilverDragon,
    Lich,
    Shopkeeper,
    LittleDog,
    Kitten,
}

/// Behavioral archetype for autonomous monster decision making.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiBehavior {
    /// Actively computes Dijkstra gradient to hunt and attack player.
    MeleeHunter,
    /// Pursues player, but reverses gradient to flee when HP < 30% or when facing Elbereth.
    CautiousHunter,
    /// Passive watcher, attacks only when provoked.
    Stationary,
    /// Peaceful shopkeeper defending store merchandise.
    Shopkeeper,
    /// Loyal pet following hero, displacing when stepped on, attacking hostiles.
    CompanionPet,
}

/// Declarative specification of a monster species.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonsterArchetype {
    pub id: MonsterSpeciesId,
    pub name: &'static str,
    pub glyph: char,
    pub base_hp: u32,
    pub max_hp: u32,
    pub ac: i32,
    pub level: u32,
    pub speed: u32,
    pub alignment: Alignment,
    pub intrinsics: Intrinsics,
    pub damage_dice: (u32, u32),
    pub ai_behavior: AiBehavior,
}

pub static BESTIARY: &[MonsterArchetype] = &[
    MonsterArchetype {
        id: MonsterSpeciesId::Goblin,
        name: "goblin",
        glyph: 'o',
        base_hp: 8,
        max_hp: 8,
        ac: 6,
        level: 1,
        speed: 9,
        alignment: Alignment::Chaotic,
        intrinsics: Intrinsics::empty(),
        damage_dice: (1, 6),
        ai_behavior: AiBehavior::CautiousHunter,
    },
    MonsterArchetype {
        id: MonsterSpeciesId::Hobgoblin,
        name: "hobgoblin",
        glyph: 'o',
        base_hp: 12,
        max_hp: 12,
        ac: 5,
        level: 2,
        speed: 9,
        alignment: Alignment::Chaotic,
        intrinsics: Intrinsics::empty(),
        damage_dice: (1, 8),
        ai_behavior: AiBehavior::CautiousHunter,
    },
    MonsterArchetype {
        id: MonsterSpeciesId::Orc,
        name: "hill orc",
        glyph: 'o',
        base_hp: 14,
        max_hp: 14,
        ac: 4,
        level: 2,
        speed: 9,
        alignment: Alignment::Chaotic,
        intrinsics: Intrinsics::empty(),
        damage_dice: (1, 8),
        ai_behavior: AiBehavior::MeleeHunter,
    },
    MonsterArchetype {
        id: MonsterSpeciesId::Kobold,
        name: "kobold",
        glyph: 'k',
        base_hp: 6,
        max_hp: 6,
        ac: 7,
        level: 1,
        speed: 6,
        alignment: Alignment::Chaotic,
        intrinsics: Intrinsics::empty().with_poison_resistance(),
        damage_dice: (1, 4),
        ai_behavior: AiBehavior::CautiousHunter,
    },
    MonsterArchetype {
        id: MonsterSpeciesId::Jackal,
        name: "jackal",
        glyph: 'd',
        base_hp: 5,
        max_hp: 5,
        ac: 7,
        level: 1,
        speed: 12,
        alignment: Alignment::Neutral,
        intrinsics: Intrinsics::empty(),
        damage_dice: (1, 4),
        ai_behavior: AiBehavior::MeleeHunter,
    },
    MonsterArchetype {
        id: MonsterSpeciesId::GiantAnt,
        name: "giant ant",
        glyph: 'a',
        base_hp: 16,
        max_hp: 16,
        ac: 3,
        level: 3,
        speed: 18,
        alignment: Alignment::Neutral,
        intrinsics: Intrinsics::empty().with_poison_resistance(),
        damage_dice: (2, 4),
        ai_behavior: AiBehavior::MeleeHunter,
    },
    MonsterArchetype {
        id: MonsterSpeciesId::FloatingEye,
        name: "floating eye",
        glyph: 'e',
        base_hp: 10,
        max_hp: 10,
        ac: 9,
        level: 2,
        speed: 1,
        alignment: Alignment::Neutral,
        intrinsics: Intrinsics::empty().with_see_invisible(),
        damage_dice: (0, 0),
        ai_behavior: AiBehavior::Stationary,
    },
    MonsterArchetype {
        id: MonsterSpeciesId::Skeleton,
        name: "skeleton",
        glyph: 'z',
        base_hp: 18,
        max_hp: 18,
        ac: 4,
        level: 3,
        speed: 10,
        alignment: Alignment::Chaotic,
        intrinsics: Intrinsics::empty().with_cold_resistance(),
        damage_dice: (1, 8),
        ai_behavior: AiBehavior::MeleeHunter,
    },
    MonsterArchetype {
        id: MonsterSpeciesId::Vampire,
        name: "vampire",
        glyph: 'V',
        base_hp: 45,
        max_hp: 45,
        ac: 1,
        level: 8,
        speed: 12,
        alignment: Alignment::Chaotic,
        intrinsics: Intrinsics::empty().with_cold_resistance(),
        damage_dice: (2, 8),
        ai_behavior: AiBehavior::MeleeHunter,
    },
    MonsterArchetype {
        id: MonsterSpeciesId::SilverDragon,
        name: "silver dragon",
        glyph: 'D',
        base_hp: 90,
        max_hp: 90,
        ac: -1,
        level: 15,
        speed: 12,
        alignment: Alignment::Lawful,
        intrinsics: Intrinsics::empty().with_cold_resistance().with_fire_resistance(),
        damage_dice: (4, 8),
        ai_behavior: AiBehavior::MeleeHunter,
    },
    MonsterArchetype {
        id: MonsterSpeciesId::Lich,
        name: "master lich",
        glyph: 'L',
        base_hp: 80,
        max_hp: 80,
        ac: 0,
        level: 14,
        speed: 12,
        alignment: Alignment::Chaotic,
        intrinsics: Intrinsics::empty().with_cold_resistance().with_telepathy(),
        damage_dice: (3, 8),
        ai_behavior: AiBehavior::MeleeHunter,
    },
    MonsterArchetype {
        id: MonsterSpeciesId::Shopkeeper,
        name: "shopkeeper",
        glyph: '@',
        base_hp: 60,
        max_hp: 60,
        ac: 0,
        level: 12,
        speed: 12,
        alignment: Alignment::Neutral,
        intrinsics: Intrinsics::empty().with_magic_resistance(),
        damage_dice: (2, 6),
        ai_behavior: AiBehavior::Shopkeeper,
    },
    MonsterArchetype {
        id: MonsterSpeciesId::LittleDog,
        name: "little dog",
        glyph: 'd',
        base_hp: 12,
        max_hp: 12,
        ac: 6,
        level: 2,
        speed: 12,
        alignment: Alignment::Neutral,
        intrinsics: Intrinsics::empty(),
        damage_dice: (1, 6),
        ai_behavior: AiBehavior::CompanionPet,
    },
    MonsterArchetype {
        id: MonsterSpeciesId::Kitten,
        name: "kitten",
        glyph: 'f',
        base_hp: 10,
        max_hp: 10,
        ac: 6,
        level: 2,
        speed: 12,
        alignment: Alignment::Neutral,
        intrinsics: Intrinsics::empty(),
        damage_dice: (1, 4),
        ai_behavior: AiBehavior::CompanionPet,
    },
];

/// Look up a monster archetype from the bestiary table.
pub fn get_monster_species(id: MonsterSpeciesId) -> &'static MonsterArchetype {
    BESTIARY
        .iter()
        .find(|m| m.id == id)
        .expect("All MonsterSpeciesId variants must have a bestiary entry")
}

/// Factory function to spawn an ActorRecord from declarative archetype data.
pub fn create_monster_record(id: MonsterSpeciesId, coord: Coord) -> ActorRecord {
    let arch = get_monster_species(id);
    ActorRecord {
        name: arch.name.to_string(),
        coord,
        hp: arch.base_hp,
        max_hp: arch.max_hp,
        ac: arch.ac,
        level: arch.level,
        speed: arch.speed,
        alignment: arch.alignment,
        intrinsics: arch.intrinsics,
        is_player: false,
        is_dead: false,
        is_tame: arch.ai_behavior == AiBehavior::CompanionPet,
    }
}
