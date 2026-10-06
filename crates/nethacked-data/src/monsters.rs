//! Declarative monster bestiary and species registry for NetHackED.

use nethacked_arena::ActorRecord;
use nethacked_types::{
    Alignment, BreathType, Coord, GazeType, Intrinsics, MonsterAbility, MonsterSpell,
};
use serde::{Deserialize, Serialize};

use crate::roles::RaceId;
use rand::Rng;

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
    /// Extracted C cwt and cnutrit, independent of the item catalog.
    pub body_weight: u32,
    pub corpse_nutrition: u32,
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

/// Spawn location facts used by C `newmonhp()` special cases.
/// The simulation currently exposes only the Astral branch; callers with an
/// elemental-plane model can supply the matching plane here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MonsterSpawnContext {
    pub endgame_plane: Option<nethacked_types::EndgamePlane>,
}

/// C `newmonhp()` (makemon.c:1012–1053), using the owned species definition.
/// `level` is the definition's spawn level; dungeon/hero `adj_lev` is not modeled.
/// This sampler is independent of the definition's fixed/sampled spawn policy.
pub fn compute_monster_hp(
    def: &crate::ruleset::MonsterDef,
    context: MonsterSpawnContext,
    rng: &mut impl Rng,
) -> u32 {
    use nethacked_types::EndgamePlane;
    let (hp, minimum) = if let Some(hp) = golem_hp(def.id) {
        (hp, 0)
    } else if matches!(
        def.id,
        Some(MonsterSpeciesId::FAMINE | MonsterSpeciesId::PESTILENCE | MonsterSpeciesId::DEATH)
    ) {
        (roll_hp_dice(rng, 10, 8), 10)
    } else if def.level > 49 {
        (2 * (def.level - 6), 0)
    } else if matches!(
        def.id,
        Some(
            MonsterSpeciesId::RED_DRAGON
                | MonsterSpeciesId::BLACK_DRAGON
                | MonsterSpeciesId::GREEN_DRAGON
                | MonsterSpeciesId::WHITE_DRAGON
                | MonsterSpeciesId::BLUE_DRAGON
                | MonsterSpeciesId::SILVER_DRAGON
                | MonsterSpeciesId::GRAY_DRAGON
                | MonsterSpeciesId::YELLOW_DRAGON
                | MonsterSpeciesId::ORANGE_DRAGON
        )
    ) {
        (
            if context.endgame_plane.is_some() {
                8 * def.level
            } else {
                4 * def.level + roll_hp_dice(rng, def.level, 4)
            },
            def.level,
        )
    } else if def.level == 0 {
        (rng.random_range(1..=4), 1)
    } else {
        let home = matches!(
            (def.id, context.endgame_plane),
            (
                Some(MonsterSpeciesId::AIR_ELEMENTAL),
                Some(EndgamePlane::Air)
            ) | (
                Some(MonsterSpeciesId::EARTH_ELEMENTAL),
                Some(EndgamePlane::Earth)
            ) | (
                Some(MonsterSpeciesId::FIRE_ELEMENTAL),
                Some(EndgamePlane::Fire)
            ) | (
                Some(MonsterSpeciesId::WATER_ELEMENTAL),
                Some(EndgamePlane::Water)
            )
        );
        (
            roll_hp_dice(rng, def.level, 8) * if home { 3 } else { 1 },
            def.level,
        )
    };
    if hp == minimum {
        hp + 1
    } else {
        hp
    }
}

/// Factory for vanilla species; shares the owned-ruleset spawning policy.
pub fn create_monster_record(
    id: MonsterSpeciesId,
    coord: Coord,
    rng: &mut impl Rng,
) -> ActorRecord {
    crate::ruleset::Ruleset::vanilla()
        .create_monster_record_by_id(id, coord, rng)
        .expect("vanilla species exists")
}

/// Bones HP is already determined; never sample and discard random HP.
pub fn create_ghost_record(name: &str, level: u32, hp: u32, coord: Coord) -> ActorRecord {
    let ruleset = crate::ruleset::Ruleset::vanilla();
    let def = ruleset
        .monster_by_id(MonsterSpeciesId::GHOST)
        .expect("ghost exists");
    let mut rec = def.create_record_with_hp(coord, hp, hp, level);
    rec.name = format!("ghost of {name}");
    rec
}

fn roll_hp_dice(rng: &mut impl Rng, count: u32, sides: u32) -> u32 {
    (0..count).map(|_| rng.random_range(1..=sides)).sum()
}

/// C golemhp(), makemon.c:2233–2263. Fixed species facts, distinct
/// from the catalog's historical average HP estimates.
fn golem_hp(id: Option<MonsterSpeciesId>) -> Option<u32> {
    match id {
        Some(MonsterSpeciesId::STRAW_GOLEM | MonsterSpeciesId::PAPER_GOLEM) => Some(20),
        Some(MonsterSpeciesId::ROPE_GOLEM) => Some(30),
        Some(MonsterSpeciesId::LEATHER_GOLEM | MonsterSpeciesId::FLESH_GOLEM) => Some(40),
        Some(MonsterSpeciesId::GOLD_GOLEM) => Some(60),
        Some(MonsterSpeciesId::WOOD_GOLEM) => Some(50),
        Some(MonsterSpeciesId::CLAY_GOLEM) => Some(70),
        Some(MonsterSpeciesId::STONE_GOLEM) => Some(100),
        Some(MonsterSpeciesId::GLASS_GOLEM) => Some(80),
        Some(MonsterSpeciesId::IRON_GOLEM) => Some(120),
        _ => None,
    }
}

/// C newmonhp branch ordering: golems and Riders precede encoded mlevel.
pub(crate) fn monster_spawn_level(def: &crate::ruleset::MonsterDef, hp: u32) -> u32 {
    if def.level > 49
        && golem_hp(def.id).is_none()
        && !matches!(
            def.id,
            Some(MonsterSpeciesId::DEATH | MonsterSpeciesId::FAMINE | MonsterSpeciesId::PESTILENCE)
        )
    {
        hp / 4
    } else {
        def.level
    }
}
