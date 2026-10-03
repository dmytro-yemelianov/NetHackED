//! Foundational types, coordinates, and ontology enums for NetRust.

use serde::{Deserialize, Serialize};

pub const COLNO: usize = 80;
pub const ROWNO: usize = 21;

/// Bounded coordinate on the $80 \times 21$ dungeon grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Coord {
    pub x: usize,
    pub y: usize,
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
        }
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
}
