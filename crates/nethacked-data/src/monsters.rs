//! Declarative monster bestiary and species registry for NetHackED.

use nethacked_arena::ActorRecord;
use nethacked_types::{
    Alignment, BreathType, Coord, GazeType, Intrinsics, MonsterAbility, MonsterSpell,
};
use serde::{Deserialize, Serialize};

use crate::roles::RaceId;

/// Re-exported from `nethacked-types` so `nethacked-core` combat can take an [`Attack`].
pub use nethacked_types::{Attack, AttackType, DamageType};

/// Behavioral archetype for autonomous monster decision making.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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

/// C `msound`, reduced to the values `peace_minded` tests
/// (`makemon.c:2276-2279`: `MS_LEADER`, `MS_GUARDIAN`, `MS_NEMESIS`); every
/// other `MS_*` is [`MonsterSound::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum MonsterSound {
    Leader,
    Guardian,
    Nemesis,
    Other,
}

/// C monster size (`MZ_*`, include/monflag.h:174-180).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum MonsterSize {
    Tiny,
    Small,
    Medium,
    Large,
    Huge,
    Gigantic,
}

/// Declarative specification of a monster species.
#[derive(Debug, Clone, Serialize)]
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
    /// C `mattk[]` (up to 6 attacks; only trailing NO_ATTK slots are omitted, so
    /// the slice index is the C slot `i` of the `rnd(20 + i)` to-hit die). The sim
    /// resolves melee from it (`nethacked-sim` combat) and breath from its AT_BREA.
    pub attacks: &'static [Attack],
    /// C `msize` (`MZ_*`; `MZ_HUMAN` is `MZ_MEDIUM`).
    pub size: MonsterSize,
    /// C `M2_PEACEFUL` (monst.c flags; always peaceful before alignment checks).
    pub peaceful_by_default: bool,
    /// C `M2_HOSTILE` (`always_hostile`, mondata.h:116; `peace_minded` makemon.c:2274).
    pub always_hostile: bool,
    /// C `maligntyp` (5th `LVL()` argument; `A_NONE` = -128, align.h). `alignment`
    /// is its sign; `peace_minded` also uses its magnitude (`rn2(2 + abs(mal))`).
    pub maligntyp: i8,
    /// C `msound` as far as `peace_minded` tests it.
    pub msound: MonsterSound,
    /// C race flag of `mflags2` (`M2_HUMAN`/`M2_ELF`/`M2_DWARF`/`M2_GNOME`/`M2_ORC`),
    /// matched against the hero race's love/hate masks (`race_peaceful`/`race_hostile`,
    /// mondata.h:118-119). No BESTIARY entry has more than one race flag.
    pub m2_race: Option<RaceId>,
    /// C `M2_HUMAN`.
    pub is_human: bool,
    /// C `G_UNIQ`.
    pub is_unique: bool,
    /// C `M1_MINDLESS`.
    pub mindless: bool,
    pub ai_behavior: AiBehavior,
    /// Sim tactical abilities. Gaze and spellcasting only act when `attacks`
    /// has the matching AT_GAZE / AT_MAGC entry (Medusa's gaze; lich, Dark One,
    /// Thoth Amon and Wizard of Yendor spells). Breath is driven by AT_BREA.
    pub abilities: &'static [MonsterAbility],
    /// C `difficulty` (monsters.h), used by monster generation.
    pub difficulty: u32,
    /// C `G_FREQ` generation frequency (0 = never randomly generated).
    pub frequency: u32,
    /// C `G_*` generation flags, lowercase without prefix (`nogen`, `uniq`, `hell`, ...).
    pub gen_flags: &'static [&'static str],
    /// C `M1_*`/`M2_*`/`M3_*` flags, lowercase without prefix (`fly`, `nasty`, ...).
    pub flags: &'static [&'static str],
}

include!("generated/monsters.rs");

/// NetHack monster class letter (bestiary glyph) for a species name, case-insensitive.
pub fn monster_class_of(name: &str) -> Option<char> {
    BESTIARY
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case(name))
        .map(|m| m.glyph)
}

/// Find a bestiary entry by display name, case-insensitively.
///
/// Strips the sim's runtime name decorations first: a leading `"hostile "`
/// (pacified/angered monsters, e.g. `"hostile djinni"`) and a leading
/// `"ghost of "` (bones ghosts, resolving to the `ghost` species).
pub fn monster_archetype_by_name(name: &str) -> Option<&'static MonsterArchetype> {
    let lower = name.to_ascii_lowercase();
    let base = if lower.starts_with("ghost of ") {
        "ghost"
    } else {
        lower.strip_prefix("hostile ").unwrap_or(&lower)
    };
    BESTIARY.iter().find(|m| m.name.eq_ignore_ascii_case(base))
}

/// Look up a monster archetype from the bestiary table.
pub fn get_monster_species(id: MonsterSpeciesId) -> &'static MonsterArchetype {
    &BESTIARY[id.index()]
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
        is_unique: arch.is_unique,
        is_dead: false,
        is_tame: arch.ai_behavior == AiBehavior::CompanionPet,
        tameness: if arch.ai_behavior == AiBehavior::CompanionPet {
            5
        } else {
            0
        },
        abilities: arch.abilities.to_vec(),
        is_peaceful: arch.peaceful_by_default,
        mspec_used: 0,
        malign: 0,
    }
}

/// Spawns a hostile ghost representing a deceased adventurer from a graveyard bones file.
pub fn create_ghost_record(name: &str, level: u32, hp: u32, coord: Coord) -> ActorRecord {
    let mut rec = create_monster_record(MonsterSpeciesId::GHOST, coord);
    rec.name = format!("ghost of {name}");
    rec.level = level;
    rec.max_hp = hp;
    rec.hp = hp;
    rec
}
