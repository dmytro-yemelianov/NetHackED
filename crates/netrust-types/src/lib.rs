//! Foundational types, coordinates, and ontology enums for NetRust.

use serde::{Deserialize, Serialize};
use slotmap::new_key_type;

new_key_type! {
    /// Safe, generational handle for an Item.
    pub struct ItemId;
    /// Safe, generational handle for an Actor (Hero or Monster).
    pub struct ActorId;
    /// Safe, generational handle for a Dungeon Level.
    pub struct LevelId;
}

pub type SlotId = ItemId;


pub const COLNO: usize = 80;
pub const ROWNO: usize = 21;

/// Supported language locales for NetRust internationalization (i18n).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Locale {
    #[default]
    En,
    Uk,
}

impl Locale {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "uk" | "ua" | "uk_ua" | "uk-ua" | "ukrainian" | "укр" | "українська" => Locale::Uk,
            _ => Locale::En,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::Uk => "uk",
        }
    }
}


/// Bounded coordinate on the $80 \times 21$ dungeon grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "RawCoord")]
pub struct Coord {
    pub x: usize,
    pub y: usize,
}

/// Unvalidated wire form of `Coord`; deserialization goes through `Coord::new`.
#[derive(Deserialize)]
struct RawCoord {
    x: usize,
    y: usize,
}

impl TryFrom<RawCoord> for Coord {
    type Error = String;

    fn try_from(raw: RawCoord) -> Result<Self, Self::Error> {
        Coord::new(raw.x, raw.y).ok_or_else(|| {
            format!("coordinate ({}, {}) out of bounds {}x{}", raw.x, raw.y, COLNO, ROWNO)
        })
    }
}

impl Coord {
    pub const fn new(x: usize, y: usize) -> Option<Self> {
        if x < COLNO && y < ROWNO {
            Some(Self { x, y })
        } else {
            None
        }
    }

    /// Unchecked constructor for const contexts or validated algorithms.
    pub const fn new_unchecked(x: usize, y: usize) -> Self {
        Self { x, y }
    }

    /// Chebyshev distance (king's move distance in 8 directions).
    pub fn chebyshev_distance(self, other: Coord) -> usize {
        let dx = (self.x as isize - other.x as isize).unsigned_abs();
        let dy = (self.y as isize - other.y as isize).unsigned_abs();
        dx.max(dy)
    }

    /// Manhattan distance ($|x_1 - x_2| + |y_1 - y_2|$).
    pub fn manhattan_distance(self, other: Coord) -> usize {
        let dx = (self.x as isize - other.x as isize).unsigned_abs();
        let dy = (self.y as isize - other.y as isize).unsigned_abs();
        dx + dy
    }

    /// Returns whether this coordinate is orthogonally or diagonally adjacent to other.
    pub fn is_adjacent(self, other: Coord) -> bool {
        self != other && self.chebyshev_distance(other) == 1
    }

    /// Step one coordinate in a given direction, if within bounds.
    pub fn step(self, dir: Direction) -> Option<Coord> {
        let (dx, dy) = dir.delta();
        let nx = self.x as isize + dx;
        let ny = self.y as isize + dy;
        if nx >= 0 && nx < COLNO as isize && ny >= 0 && ny < ROWNO as isize {
            Some(Coord::new_unchecked(nx as usize, ny as usize))
        } else {
            None
        }
    }

    /// Returns all valid adjacent neighbors on the grid.
    pub fn neighbors(self) -> Vec<Coord> {
        Direction::all_compass()
            .iter()
            .filter_map(|&d| self.step(d))
            .collect()
    }
}

/// 8-way directional navigation plus vertical and stationary actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Direction {
    North,
    East,
    South,
    West,
    NorthEast,
    SouthEast,
    SouthWest,
    NorthWest,
    Up,
    Down,
    None,
}

impl Direction {
    pub const fn delta(self) -> (isize, isize) {
        match self {
            Direction::North => (0, -1),
            Direction::East => (1, 0),
            Direction::South => (0, 1),
            Direction::West => (-1, 0),
            Direction::NorthEast => (1, -1),
            Direction::SouthEast => (1, 1),
            Direction::SouthWest => (-1, 1),
            Direction::NorthWest => (-1, -1),
            Direction::Up | Direction::Down | Direction::None => (0, 0),
        }
    }

    pub const fn all_compass() -> [Direction; 8] {
        [
            Direction::North,
            Direction::East,
            Direction::South,
            Direction::West,
            Direction::NorthEast,
            Direction::SouthEast,
            Direction::SouthWest,
            Direction::NorthWest,
        ]
    }
}

/// Canonical NetHack dungeon branch taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BranchId {
    DungeonsOfDoom,
    GnomishMines,
    Sokoban,
    Gehennom,
    AstralPlane,
    Quest,
    WizardsTower,
    VladsTower,
    FortLudios,
    RogueLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrapType {
    Arrow,
    Dart,
    RockFall,
    Pit,
    SpikedPit,
    Teleport,
    Fire,
    LevelTeleport,
    Polymorph,
    AntiMagic,
    SleepingGas,
    Rust,
    Web,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrapState {
    Hidden,
    Revealed,
    Disarmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrapRecord {
    pub id: usize,
    pub trap_type: TrapType,
    pub state: TrapState,
    pub coord: Coord,
}

/// Discrete branch coordinate: (BranchId, LevelWithinBranch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BranchCoord {
    pub branch: BranchId,
    pub depth: usize,
}

/// The 5 Endgame Planes leading to the Astral Plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EndgamePlane {
    Earth,
    Air,
    Fire,
    Water,
    Astral,
}

/// State of the Castle drawbridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DrawbridgeState {
    Open,
    Closed,
    Destroyed,
}

/// Result of operating a drawbridge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrawbridgeTransition {
    Lowered,
    Raised { crushed_damage: u32 },
    DestroyedAndFellInMoat,
}

/// Outcome of offering the Amulet of Yendor on a High Altar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AscensionOutcome {
    Ascended(Alignment),
    Rejected(String),
}

/// Alignment in NetHack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Alignment {
    Lawful,
    Neutral,
    Chaotic,
    Unaligned,
}

/// A specific deity in the pantheon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deity {
    pub name: String,
    pub align: Alignment,
}

/// 3-deity Pantheon for a character role (Lawful, Neutral, Chaotic).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pantheon {
    pub lawful: Deity,
    pub neutral: Deity,
    pub chaotic: Deity,
}

/// Character divine state tracking favor, prayer cooldown, and gifts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DivineState {
    pub favor: i32,
    pub prayer_timeout: u32,
    pub gift_count: u32,
}

impl Default for DivineState {
    fn default() -> Self {
        Self {
            favor: 5,
            prayer_timeout: 0,
            gift_count: 0,
        }
    }
}

/// Result of sacrificing a corpse on an altar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SacrificeResult {
    AltarConverted(Alignment),
    FavorIncreased(i32),
    DivineGift(String),
}

/// Canonical NetHack signature named artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArtifactKind {
    Excalibur,
    VorpalBlade,
    Mjollnir,
    Magicbane,
    EyeOfTheAethiopica,
    TheOrbOfFate,
    TheHeartOfAhriman,
    TheMagicMirrorOfMerlin,
    TheEyesOfTheOverworld,
    TheMasterKeyOfThievery,
    TheTsurugiOfMuramasa,
    ThePlatinumYendorianExpressCard,
    TheStaffOfAesculapius,
    TheOrbOfDetection,
}

/// Wand charge state tracking current charges and number of recharges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WandCharges {
    pub charges: u32,
    pub recharges: u32,
}

impl WandCharges {
    pub const fn new(charges: u32) -> Self {
        Self { charges, recharges: 0 }
    }
}

/// Wand recharging outcome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RechargeResult {
    Success(WandCharges),
    Exploded,
}

/// Door states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DoorState {
    Open,
    Closed,
    Locked,
    Broken,
}

/// Map tile representation eliminating C NetHack rm.flags bitfield collisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tile {
    Stone,
    Wall { horizontal: bool },
    Corr,
    Room,
    Door { state: DoorState, trapped: bool },
    SecretDoor { locked: bool },
    Stairs { up: bool },
    BranchStairs { branch: BranchId, level: usize, up: bool },
    Pit { filled: bool },
    Altar { align: Alignment },
    HighAltar { align: Alignment },
    Drawbridge { open: bool },
    Moat,
    Pool { frozen: bool },
    Lava,
}

impl Tile {
    pub fn is_passable(&self) -> bool {
        match self {
            Tile::Corr
            | Tile::Room
            | Tile::Stairs { .. }
            | Tile::BranchStairs { .. }
            | Tile::Altar { .. }
            | Tile::HighAltar { .. } => true,
            Tile::Drawbridge { open } => *open,
            Tile::Pit { filled } => *filled,
            Tile::Door { state, .. } => matches!(state, DoorState::Open | DoorState::Broken),
            Tile::Pool { frozen } => *frozen,
            _ => false,
        }
    }

    pub fn is_transparent(&self) -> bool {
        match self {
            Tile::Room
            | Tile::Corr
            | Tile::Stairs { .. }
            | Tile::BranchStairs { .. }
            | Tile::Pit { .. }
            | Tile::Altar { .. }
            | Tile::HighAltar { .. }
            | Tile::Pool { .. }
            | Tile::Lava
            | Tile::Moat => true,
            Tile::Drawbridge { open } => *open,
            Tile::Door { state, .. } => matches!(state, DoorState::Open | DoorState::Broken),
            _ => false,
        }
    }

    pub fn open_door(&mut self) {
        if let Tile::Door { state, .. } = self {
            if *state == DoorState::Closed {
                *state = DoorState::Open;
            }
        }
    }

    pub fn close_door(&mut self) {
        if let Tile::Door { state, .. } = self {
            if *state == DoorState::Open {
                *state = DoorState::Closed;
            }
        }
    }

    pub fn unlock_door(&mut self) {
        if let Tile::Door { state, .. } = self {
            if *state == DoorState::Locked {
                *state = DoorState::Closed;
            }
        }
    }

    pub fn break_door(&mut self) {
        if let Tile::Door { state, trapped } = self {
            *state = DoorState::Broken;
            *trapped = false;
        }
    }

    pub fn reveal_secret_door(&mut self) {
        if let Tile::SecretDoor { locked } = self {
            let state = if *locked {
                DoorState::Locked
            } else {
                DoorState::Closed
            };
            *self = Tile::Door {
                state,
                trapped: false,
            };
        }
    }
}

/// NetHack's 17 core item classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemClass {
    Illegal,
    Weapon,
    Armor,
    Ring,
    Amulet,
    Tool,
    Food,
    Potion,
    Scroll,
    Spellbook,
    Wand,
    Coin,
    Gem,
    Rock,
    Ball,
    Chain,
    Venom,
}

impl ItemClass {
    pub const fn symbol(self) -> char {
        match self {
            ItemClass::Illegal => ']',
            ItemClass::Weapon => ')',
            ItemClass::Armor => '[',
            ItemClass::Ring => '=',
            ItemClass::Amulet => '"',
            ItemClass::Tool => '(',
            ItemClass::Food => '%',
            ItemClass::Potion => '!',
            ItemClass::Scroll => '?',
            ItemClass::Spellbook => '+',
            ItemClass::Wand => '/',
            ItemClass::Coin => '$',
            ItemClass::Gem => '*',
            ItemClass::Rock => '`',
            ItemClass::Ball => '0',
            ItemClass::Chain => '_',
            ItemClass::Venom => '.',
        }
    }
}

/// Mutually exclusive 3-state BUC lattice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Buc {
    Blessed,
    Uncursed,
    Cursed,
}

impl Default for Buc {
    fn default() -> Self {
        Self::Uncursed
    }
}

impl Buc {
    #[inline]
    pub fn is_blessed(self) -> bool {
        matches!(self, Buc::Blessed)
    }

    #[inline]
    pub fn is_uncursed(self) -> bool {
        matches!(self, Buc::Uncursed)
    }

    #[inline]
    pub fn is_cursed(self) -> bool {
        matches!(self, Buc::Cursed)
    }
}

/// Types of water used for dipping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WaterType {
    Holy,
    Plain,
    Unholy,
}

/// Damage and attack types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DamageType {
    Physical,
    Magic,
    Fire,
    Cold,
    Electric,
    Poison,
    Acid,
    Disintegration,
    LevelDrain,
}

/// Intrinsic and Extrinsic flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, Serialize, Deserialize)]
pub struct Intrinsics {
    pub fire_resistance: bool,
    pub cold_resistance: bool,
    pub shock_resistance: bool,
    pub sleep_resistance: bool,
    pub poison_resistance: bool,
    pub acid_resistance: bool,
    pub disintegration_resistance: bool,
    pub magic_resistance: bool,
    pub reflection: bool,
    pub see_invisible: bool,
    pub telepathy: bool,
    pub stealth: bool,
    pub fast: bool,
    pub very_fast: bool,
    pub levitation: bool,
    pub blind: bool,
}

impl Intrinsics {
    pub const fn empty() -> Self {
        Self {
            fire_resistance: false,
            cold_resistance: false,
            shock_resistance: false,
            sleep_resistance: false,
            poison_resistance: false,
            acid_resistance: false,
            disintegration_resistance: false,
            magic_resistance: false,
            reflection: false,
            see_invisible: false,
            telepathy: false,
            stealth: false,
            fast: false,
            very_fast: false,
            levitation: false,
            blind: false,
        }
    }

    pub const fn with_blind(mut self) -> Self {
        self.blind = true;
        self
    }

    pub const fn with_fire_resistance(mut self) -> Self {
        self.fire_resistance = true;
        self
    }

    pub const fn with_cold_resistance(mut self) -> Self {
        self.cold_resistance = true;
        self
    }

    pub const fn with_poison_resistance(mut self) -> Self {
        self.poison_resistance = true;
        self
    }

    pub const fn with_magic_resistance(mut self) -> Self {
        self.magic_resistance = true;
        self
    }

    pub const fn with_see_invisible(mut self) -> Self {
        self.see_invisible = true;
        self
    }

    pub const fn with_telepathy(mut self) -> Self {
        self.telepathy = true;
        self
    }

    pub const fn with_fast(mut self) -> Self {
        self.fast = true;
        self
    }

    pub const fn with_reflection(mut self) -> Self {
        self.reflection = true;
        self
    }
}

/// Element type for monster breath weapons and beam attacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BreathType {
    Fire,
    Cold,
    Shock,
    Sleep,
    Poison,
    Disintegration,
}

/// Types of gaze attacks (e.g. Floating Eye, Medusa).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GazeType {
    Paralysis,
    Petrification,
    Confusion,
}

/// Outcome of a gaze attack considering defender's reflection and sight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GazeEffect {
    ReflectedToAttacker,
    BlindImmune,
    Afflicted(GazeType),
}

/// Spells castable by intelligent monsters (e.g. liches, demons).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MonsterSpell {
    SummonMonsters,
    CurseItems,
    RaiseDead,
    CauseWounds,
}

/// Special tactical attack ability of a monster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MonsterAbility {
    Breath { breath: BreathType, range: usize, damage_dice: (u32, u32) },
    Gaze { gaze: GazeType },
    Spellcaster { spell: MonsterSpell, cooldown_turns: u32 },
}

/// An item preserved in a graveyard bones file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BonesItem {
    pub name: String,
    pub class: ItemClass,
    pub weight: u32,
    pub buc: Buc,
    pub enchantment: i8,
}

/// Graveyard bones record capturing an adventurer's demise for cross-run persistence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BonesData {
    pub depth: u32,
    pub hero_name: String,
    pub hero_level: u32,
    pub max_hp: u32,
    pub ac: i32,
    pub death_coord: Coord,
    pub items: Vec<BonesItem>,
    pub killer: String,
}

/// Graveyard headstone record and death cause memorial.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraveRecord {
    pub hero_name: String,
    pub hero_level: u32,
    pub depth: u32,
    pub killer: String,
    pub date: String,
    pub epitaph: String,
    pub ascii_headstone: String,
}

/// Statistics and telemetry for the networked shared graveyard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GraveyardStats {
    pub total_deaths: usize,
    pub active_bones_count: usize,
    pub total_graves: usize,
    pub haunted_depths: Vec<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coord_bounds_and_distance() {
        let c1 = Coord::new(10, 10).unwrap();
        let c2 = Coord::new(12, 10).unwrap();
        assert_eq!(c1.chebyshev_distance(c2), 2);
        assert_eq!(c1.manhattan_distance(c2), 2);
        assert!(!c1.is_adjacent(c2));

        let c_adj = Coord::new(11, 11).unwrap();
        assert!(c1.is_adjacent(c_adj));
        assert_eq!(c1.chebyshev_distance(c_adj), 1);
        assert_eq!(c1.manhattan_distance(c_adj), 2);
    }

    #[test]
    fn test_coord_stepping() {
        let origin = Coord::new(0, 0).unwrap();
        assert_eq!(origin.step(Direction::North), None);
        assert_eq!(origin.step(Direction::West), None);
        assert_eq!(origin.step(Direction::East), Coord::new(1, 0));
        assert_eq!(origin.step(Direction::South), Coord::new(0, 1));
        assert_eq!(origin.step(Direction::SouthEast), Coord::new(1, 1));
    }

    #[test]
    fn test_item_class_symbols() {
        assert_eq!(ItemClass::Weapon.symbol(), ')');
        assert_eq!(ItemClass::Potion.symbol(), '!');
        assert_eq!(ItemClass::Wand.symbol(), '/');
        assert_eq!(ItemClass::Coin.symbol(), '$');
    }

    #[test]
    fn coord_deserialize_rejects_out_of_bounds() {
        let ok: Coord = serde_json::from_str(r#"{"x":79,"y":20}"#).unwrap();
        assert_eq!(ok, Coord::new(79, 20).unwrap());
        assert!(serde_json::from_str::<Coord>(r#"{"x":80,"y":0}"#).is_err());
        assert!(serde_json::from_str::<Coord>(r#"{"x":0,"y":21}"#).is_err());
        assert!(serde_json::from_str::<Coord>(r#"{"x":1000,"y":5}"#).is_err());
        let json = serde_json::to_string(&Coord::new(3, 4).unwrap()).unwrap();
        assert_eq!(json, r#"{"x":3,"y":4}"#);
    }
}


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PetrificationState {
    pub turns_remaining: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlimingState {
    pub turns_remaining: u8,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransientAfflictions {
    pub confused: u32,
    pub stunned: u32,
    pub hallucinating: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AfflictionState {
    pub petrification: Option<PetrificationState>,
    pub sliming: Option<SlimingState>,
    pub transient: TransientAfflictions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SkillClass {
    Dagger,
    LongSword,
    ShortSword,
    Bow,
    Crossbow,
    Club,
    BareHanded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SkillLevel {
    Unskilled,
    Basic,
    Skilled,
    Expert,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillTree {
    pub skills: std::collections::HashMap<SkillClass, SkillLevel>,
    pub available_slots: u8,
}

pub type MonsterId = usize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolymorphForm {
    pub monster_id: MonsterId,
    pub hp: i32,
    pub max_hp: i32,
    pub duration: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LycanthropyState {
    pub species: MonsterId,
    pub turns_infected: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountState {
    pub steed_id: ActorId,
    pub saddle_equipped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hero {
    pub base_hp: i32,
    pub base_max_hp: i32,
    pub polymorph: Option<PolymorphForm>,
    pub lycanthropy: Option<LycanthropyState>,
    #[serde(default)]
    pub afflictions: AfflictionState,
    #[serde(default)]
    pub skills: SkillTree,
    pub quivered_item: Option<SlotId>,
    pub mount: Option<MountState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolypileResult {
    pub transformed: usize,
    pub destroyed: usize,
    pub unchanged: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EquipSlot {
    Helmet,
    Suit,
    Shirt,
    Cloak,
    Gloves,
    Boots,
    Shield,
    Amulet,
    LeftRing,
    RightRing,
    Weapon,
    Quiver,
}

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GenocideTarget {
    Species(String),
    Class(char),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenocideRegistry {
    pub genocided_species: HashSet<String>,
    pub genocided_classes: HashSet<char>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConductTracker {
    pub pacifist: bool,
    pub vegan: bool,
    pub vegetarian: bool,
    pub atheist: bool,
    pub illiterate: bool,
    pub genocideless: bool,
    pub polypileless: bool,
    pub wishless: bool,
}

impl Default for ConductTracker {
    fn default() -> Self {
        Self {
            pacifist: true,
            vegan: true,
            vegetarian: true,
            atheist: true,
            illiterate: true,
            genocideless: true,
            polypileless: true,
            wishless: true,
        }
    }
}
