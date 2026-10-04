//! NetHack Role and Race definitions and character generator.

use crate::items::{create_item_record, ItemKindId};
use netrust_arena::{ActorId, ActorRecord, EntityArena, ItemId, ItemLocation};
use netrust_types::{Alignment, Buc, Coord, Intrinsics, SkillClass, SkillLevel};
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
        ac: 10,
        speed: 12,
        default_alignment: Alignment::Neutral,
        starting_items: &[ItemKindId::LongSword, ItemKindId::PotionOfHealing],
    },
    RoleSpec {
        id: RoleId::Wizard,
        name: "Wizard",
        base_hp: 12,
        ac: 10,
        speed: 12,
        default_alignment: Alignment::Neutral,
        starting_items: &[
            ItemKindId::WandOfStriking,
            ItemKindId::ScrollOfIdentify,
            ItemKindId::CloakOfMagicResistance,
        ],
    },
    RoleSpec {
        id: RoleId::Barbarian,
        name: "Barbarian",
        base_hp: 20,
        ac: 10,
        speed: 12,
        default_alignment: Alignment::Chaotic,
        starting_items: &[ItemKindId::LongSword],
    },
    RoleSpec {
        id: RoleId::Rogue,
        name: "Rogue",
        base_hp: 14,
        ac: 10,
        speed: 12,
        default_alignment: Alignment::Chaotic,
        starting_items: &[
            ItemKindId::Dagger,
            ItemKindId::ShortSword,
            ItemKindId::LeatherArmor,
            ItemKindId::Sack,
        ],
    },
    RoleSpec {
        id: RoleId::Knight,
        name: "Knight",
        base_hp: 16,
        ac: 10,
        speed: 12,
        default_alignment: Alignment::Lawful,
        starting_items: &[ItemKindId::LongSword],
    },
    RoleSpec {
        id: RoleId::Monk,
        name: "Monk",
        base_hp: 14,
        ac: 10,
        speed: 12,
        default_alignment: Alignment::Neutral,
        starting_items: &[
            ItemKindId::PotionOfHealing,
            ItemKindId::ScrollOfTeleportation,
        ],
    },
    RoleSpec {
        id: RoleId::Healer,
        name: "Healer",
        base_hp: 14,
        ac: 10,
        speed: 12,
        default_alignment: Alignment::Neutral,
        starting_items: &[
            ItemKindId::PotionOfHealing,
            ItemKindId::PotionOfExtraHealing,
        ],
    },
    RoleSpec {
        id: RoleId::Tourist,
        name: "Tourist",
        base_hp: 10,
        ac: 10,
        speed: 12,
        default_alignment: Alignment::Neutral,
        starting_items: &[
            ItemKindId::GoldPieces,
            ItemKindId::PotionOfExtraHealing,
            ItemKindId::BagOfHolding,
        ],
    },
    RoleSpec {
        id: RoleId::Archaeologist,
        name: "Archaeologist",
        base_hp: 14,
        ac: 10,
        speed: 12,
        default_alignment: Alignment::Lawful,
        starting_items: &[ItemKindId::ShortSword, ItemKindId::Sack],
    },
];

/// Weapon skill class of a weapon item kind, for the skill classes NetRust models
/// (C `weapon_type`, `weapon.c:1514`). Weapons whose C skill has no `SkillClass`
/// (saber, mace, quarterstaff, spear, knife, dart, whip, ...) map to `None` and are
/// skipped.
fn weapon_skill_class(kind: ItemKindId) -> Option<SkillClass> {
    match kind {
        ItemKindId::Dagger => Some(SkillClass::Dagger),
        ItemKindId::ShortSword => Some(SkillClass::ShortSword),
        ItemKindId::LongSword => Some(SkillClass::LongSword),
        _ => None,
    }
}

/// Weapon skill classes NetRust models that appear in the role's C skill table
/// (`u_init.c:257-572`, `Skill_A` .. `Skill_W`); a class absent here is restricted
/// for the role and never advances. The bool is whether the role's maximum
/// bare-handed/martial-arts skill exceeds Expert (`weapon.c:1784`).
fn role_skill_table(role: RoleId) -> (&'static [SkillClass], bool) {
    use SkillClass::*;
    match role {
        // Skill_A (u_init.c:257)
        RoleId::Archaeologist => (&[Dagger, ShortSword, Club, BareHanded], false),
        // Skill_B (u_init.c:279); bare-handed Master
        RoleId::Barbarian => (
            &[Dagger, ShortSword, LongSword, Club, Bow, BareHanded],
            true,
        ),
        // Skill_H (u_init.c:327)
        RoleId::Healer => (&[Dagger, ShortSword, Club, BareHanded], false),
        // Skill_K (u_init.c:346)
        RoleId::Knight => (
            &[
                Dagger, ShortSword, LongSword, Club, Bow, Crossbow, BareHanded,
            ],
            false,
        ),
        // Skill_Mon (u_init.c:375); martial arts Grand Master
        RoleId::Monk => (&[Crossbow, BareHanded], true),
        // Skill_R (u_init.c:414)
        RoleId::Rogue => (
            &[Dagger, ShortSword, LongSword, Club, Crossbow, BareHanded],
            false,
        ),
        // Skill_T (u_init.c:490)
        RoleId::Tourist => (
            &[Dagger, ShortSword, LongSword, Bow, Crossbow, BareHanded],
            false,
        ),
        // Skill_V (u_init.c:525)
        RoleId::Valkyrie => (&[Dagger, ShortSword, LongSword, BareHanded], false),
        // Skill_W (u_init.c:548)
        RoleId::Wizard => (&[Dagger, ShortSword, Club, BareHanded], false),
    }
}

/// C-inventory weapons that NetRust's simplified `starting_items` lacks but whose
/// skill class NetRust models (each starts Basic via `skill_init`): the Valkyrie's
/// dagger (`u_init.c:160` `Valkyrie[]`). Other roles' extra C weapons (spear,
/// quarterstaff, bullwhip, scalpel, darts, lance, axes) have no `SkillClass`.
fn c_extra_weapons(role: RoleId) -> &'static [ItemKindId] {
    match role {
        RoleId::Valkyrie => &[ItemKindId::Dagger],
        _ => &[],
    }
}

/// Weapon skills a role starts with above Unskilled (everything else is Unskilled).
///
/// C `skill_init` (`weapon.c:1738`): every non-ammo weapon in the starting
/// inventory sets its skill to Basic (`weapon.c:1752`), applied here to NetRust's
/// `starting_items` plus [`c_extra_weapons`]; skills restricted by the role's table
/// (`u_init.c:257-572`) are skipped. Roles whose maximum bare-handed/martial-arts
/// skill exceeds Expert (Barbarian, Monk) start Basic bare-handed (`weapon.c:1784`).
/// Wizard: C's quarterstaff has no `SkillClass`, and dagger is only
/// Unskilled-but-allowed in `Skill_W`, so no weapon class starts Basic.
/// Spell skills and riding are not modelled.
pub fn starting_skills(role: RoleId) -> Vec<(SkillClass, SkillLevel)> {
    let (allowed, bare_basic) = role_skill_table(role);
    let mut out: Vec<(SkillClass, SkillLevel)> = Vec::new();
    let spec = get_role(role);
    for &kind in spec.starting_items.iter().chain(c_extra_weapons(role)) {
        if let Some(class) = weapon_skill_class(kind) {
            if allowed.contains(&class) && !out.iter().any(|(c, _)| *c == class) {
                out.push((class, SkillLevel::Basic));
            }
        }
    }
    if bare_basic {
        out.push((SkillClass::BareHanded, SkillLevel::Basic));
    }
    out
}

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
        is_unique: true,
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
        is_tame: false,
        tameness: 0,
        abilities: Vec::new(),
        is_peaceful: false,
        mspec_used: 0,
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

/// Spawns an initial companion pet (Little Dog or Kitten) adjacent to the hero.
pub fn spawn_starting_pet(
    role: RoleId,
    hero_coord: Coord,
    arena: &mut EntityArena,
) -> Option<ActorId> {
    let species = match role {
        RoleId::Wizard | RoleId::Healer => crate::monsters::MonsterSpeciesId::Kitten,
        _ => crate::monsters::MonsterSpeciesId::LittleDog,
    };
    let pet_coord = hero_coord
        .step(netrust_types::Direction::East)
        .unwrap_or(hero_coord);
    let pet_record = crate::monsters::create_monster_record(species, pet_coord);
    Some(arena.spawn_actor(pet_record))
}
