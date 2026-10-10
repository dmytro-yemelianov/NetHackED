//! Versioned selected-wand protocol. Detached facts never confer write authority.
//! I1 remains read-only; the explicitly approved V2 profile adds legal native
//! domain preparation/publication. Production gameplay dispatch follows.
use super::partitions::*;
pub use super::runtime::wand_commands::{AuthenticatedWandRng, RawDrawError};
use crate::world::{BillError, BillTerms};
use nethacked_arena::{ActorId, ItemDefinition, ItemId, ItemLocation};
use nethacked_data::{ItemKindId, RulesetRef};
use nethacked_owners::{self as owners, Observation, Stamp};
use nethacked_types::{BranchId, Buc, Coord, Direction, ItemClass, Locale};
use serde::{Deserialize, Serialize};

pub const MAX_REQUEST_BYTES: usize = 8192;
pub const MAX_RESPONSE_BYTES: usize = 16384;
pub const MAX_CELLS: usize = 64;
pub const MAX_TARGETS: usize = 16;
pub const MAX_REFERENCES: usize = 16;
pub const MAX_DRAWS: u16 = 256;
pub const MAX_TEXT_BYTES: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Residency {
    Active,
    Cached { branch: BranchId, depth: usize },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectRef {
    /// ItemId includes the arena generation; residency disambiguates cache IDs.
    pub id: ItemId,
    pub residency: Residency,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionRequest {
    pub index: u32,
    pub expected: Option<ItemId>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObjectRequest {
    Selection(SelectionRequest),
    Identity(ObjectRef),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeographyRequest {
    pub residency: Residency,
    pub cells: Vec<Coord>,
    /// Version 1: a finite complete ray, bounded before tracing. Longer rays
    /// require a future explicit continuation version, never silent truncation.
    pub trajectory: Option<TrajectoryRequest>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrajectoryRequest {
    pub version: u32,
    pub origin: Coord,
    pub direction: Direction,
    pub energy: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorsRequest {
    pub residency: Residency,
    pub origin: ActorId,
    pub targets: Vec<ActorId>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WandQuery {
    Objects(ObjectRequest),
    Equipment(ObjectRef),
    Billing {
        target: ObjectRef,
        receipt: Option<BillTerms>,
    },
    ConsumedBilling {
        origin: ItemId,
    },
    Clock(ObjectRef),
    Random {
        draws: u16,
    },
    Geography(GeographyRequest),
    Actors(ActorsRequest),
    Hero,
    Session,
    Command(CommandQuery),
}
impl WandQuery {
    pub(crate) fn route(&self) -> (&'static str, &'static str) {
        match self {
            Self::Objects(_) => ("Objects", "wand_objects"),
            Self::Equipment(_) => ("Equipment", "wand_equipment"),
            Self::Billing { .. } => ("Billing", "wand_billing"),
            Self::ConsumedBilling { .. } => ("Billing", "wand_consumed"),
            Self::Clock(_) => ("Clock", "wand_clock"),
            Self::Random { .. } => ("Random", "wand_random"),
            Self::Geography(_) => ("Geography", "wand_geography"),
            Self::Actors(_) => ("Actors", "wand_actors"),
            Self::Hero => ("Hero", "wand_hero"),
            Self::Session => ("Session", "wand_session"),
            Self::Command(q) => q.route(),
        }
    }
    pub(crate) fn validate(&self) -> owners::Result<()> {
        match self {
            Self::Geography(q) => {
                bound(q.cells.len(), MAX_CELLS)?;
                for c in &q.cells {
                    coord(*c)?;
                }
                if let Some(t) = &q.trajectory {
                    if t.version != 1 {
                        return Err(owners::Error::Schema);
                    }
                    coord(t.origin)?;
                    bound(t.energy as usize, MAX_CELLS)?;
                    // Both requested cells and traced output share a finite budget.
                    bound(q.cells.len() + t.energy as usize, MAX_CELLS)?;
                }
            }
            Self::Actors(q) => {
                bound(q.targets.len(), MAX_TARGETS)?;
                let mut ids = std::collections::BTreeSet::new();
                if q.targets
                    .iter()
                    .any(|id| !ids.insert(*id) || *id == q.origin)
                {
                    return Err(owners::Error::Schema);
                }
            }
            Self::Random { draws } => bound(usize::from(*draws), usize::from(MAX_DRAWS))?,
            Self::Command(CommandQuery::Geography { coord: c }) => coord(*c)?,
            _ => {}
        }
        Ok(())
    }
}
/// A caller's expected live incarnation/revision. Never accepted as authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerWitness {
    pub owner: owners::OwnerId,
    pub generation: u64,
    pub revision: u64,
    pub schema: [u8; 32],
    pub artifact: [u8; 32],
}
impl From<&Stamp> for OwnerWitness {
    fn from(s: &Stamp) -> Self {
        Self {
            owner: s.owner,
            generation: s.generation,
            revision: s.revision,
            schema: s.schema,
            artifact: s.module.artifact,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WandRead {
    pub query: WandQuery,
    pub expected_owner: Option<OwnerWitness>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectFacts {
    pub target: ObjectRef,
    pub definition: Option<ItemDefinition>,
    pub resolved_kind: Option<ItemKindId>,
    pub effect_name: String,
    pub class: ItemClass,
    pub quantity: u32,
    pub charges: i8,
    pub buc: Buc,
    pub placement: ItemLocation,
    pub artifact: Option<String>,
    pub receipt_eligible: Result<(), BillError>,
    pub cached_alias: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentFacts {
    pub target: ObjectRef,
    pub wielded: bool,
    pub armor: Vec<nethacked_core::ac::ArmorSlot>,
    pub dressing: Option<crate::actions::equipment::Dressing>,
    pub quivered: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Liability {
    Unbilled,
    Unknown {
        legacy_base_cost: u32,
        quantity: u32,
    },
    Known {
        terms: BillTerms,
        quantity: u32,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BillingFacts {
    pub target: ObjectRef,
    pub liability: Liability,
    pub receipt_admission: Option<Result<(), BillError>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumedFacts {
    pub origin: ItemId,
    /// Receipt order is retained explicitly, without returning the consumed object.
    pub receipt_order: usize,
    pub quantity: u32,
    pub terms: BillTerms,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimerRegistration {
    pub timer: crate::ObjectTimer,
    /// Queue position, not serial, resolves ties after native reinsertion.
    pub queue_order: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClockFacts {
    pub target: ObjectRef,
    pub turn: u64,
    pub hero_energy: u32,
    pub hero_speed: u32,
    pub registrations: Vec<TimerRegistration>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RandomReservation {
    pub stream: u64,
    pub word_position: u128,
    pub max_draws: u16,
    /// This describes a prospective reservation. Reading never advances RNG.
    pub algorithm_version: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CellFacts {
    pub coord: Coord,
    pub tile: nethacked_types::Tile,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeographyFacts {
    pub residency: Residency,
    pub branch: BranchId,
    pub depth: usize,
    pub cells: Vec<CellFacts>,
    pub trajectory: Vec<Coord>,
    pub no_teleport: bool,
    pub stored_index: Option<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorFacts {
    pub id: ActorId,
    pub coord: Coord,
    pub name: String,
    pub species: Option<nethacked_arena::ActorSpecies>,
    pub hp: u32,
    pub max_hp: u32,
    pub ac: i32,
    pub level: u32,
    pub speed: u32,
    pub intrinsics: nethacked_types::Intrinsics,
    pub is_player: bool,
    pub is_unique: bool,
    pub is_dead: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorsFacts {
    pub residency: Residency,
    pub origin: ActorFacts,
    pub targets: Vec<ActorFacts>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeroFacts {
    pub player: ActorId,
    pub polymorph: Option<nethacked_types::PolymorphForm>,
    pub base_hp: i32,
    pub base_max_hp: i32,
    pub luck: i32,
    pub quivered: Option<ItemId>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionFacts {
    pub locale: Locale,
    pub ruleset: RulesetRef,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WandFacts {
    Objects(ObjectFacts),
    Equipment(EquipmentFacts),
    Billing(BillingFacts),
    ConsumedBilling(ConsumedFacts),
    Clock(ClockFacts),
    Random(RandomReservation),
    Geography(GeographyFacts),
    Actors(ActorsFacts),
    Hero(HeroFacts),
    Session(SessionFacts),
    Command(CommandFacts),
}
/// Concrete owner provenance is detached evidence. Publication must revalidate
/// through the original owner ports, never trust this public value as a permit.
#[derive(Debug, Clone)]
pub struct WandObservation {
    pub stamp: Stamp,
    pub dependencies: Vec<Stamp>,
    pub facts: WandFacts,
}
impl WandObservation {
    pub fn witness(&self) -> OwnerWitness {
        OwnerWitness::from(&self.stamp)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WireObservation {
    pub witness: OwnerWitness,
    pub dependencies: Vec<OwnerWitness>,
    pub facts: WandFacts,
}

fn bound(n: usize, max: usize) -> owners::Result<()> {
    if n > max {
        Err(owners::Error::Limit)
    } else {
        Ok(())
    }
}
fn coord(c: Coord) -> owners::Result<()> {
    if Coord::new(c.x, c.y).is_none() {
        Err(owners::Error::Schema)
    } else {
        Ok(())
    }
}
fn text(s: &str) -> owners::Result<()> {
    bound(s.len(), MAX_TEXT_BYTES)
}
fn read_fact(o: &Observation) -> owners::Result<WandFacts> {
    super::runtime::decode(o.bytes(), MAX_RESPONSE_BYTES)
}
fn geography(obs: &[Observation], index: usize) -> owners::Result<GeographyFacts> {
    match read_fact(obs.get(index).ok_or(owners::Error::Schema)?)? {
        WandFacts::Geography(g) => Ok(g),
        _ => Err(owners::Error::Schema),
    }
}
fn object(obs: &[Observation], target: ObjectRef) -> owners::Result<ObjectFacts> {
    match read_fact(obs.first().ok_or(owners::Error::Schema)?)? {
        WandFacts::Objects(o) if o.target == target => Ok(o),
        _ => Err(owners::Error::Schema),
    }
}
pub(crate) trait Domain {
    fn facts(
        &self,
        q: &WandQuery,
        obs: &[Observation],
        c: &super::runtime::OwnerBridgeConfig,
    ) -> owners::Result<WandFacts>;
}
pub(crate) fn query<T: Domain + super::runtime::wand_commands::CommandDomain>(
    s: &T,
    name: &str,
    request: &[u8],
    obs: &[Observation],
    c: &super::runtime::OwnerBridgeConfig,
) -> owners::Result<Vec<u8>> {
    let q: WandQuery = super::runtime::decode(request, MAX_REQUEST_BYTES)?;
    if q.route().1 != name {
        return Err(owners::Error::Schema);
    }
    q.validate()?;
    if let WandQuery::Command(q) = &q {
        if !c.wand_commands {
            return Err(owners::Error::Unsupported);
        }
        return super::runtime::encode(
            &WandFacts::Command(s.command_facts(q, obs, c)?),
            MAX_RESPONSE_BYTES,
        );
    }
    super::runtime::encode(&s.facts(&q, obs, c)?, MAX_RESPONSE_BYTES)
}
mod domains;
mod proposals;
pub use super::runtime::item_lifetime::{LifetimeAction, LifetimeError, LifetimeReply};
pub use proposals::*;

pub use super::runtime::wand_gameplay::{
    ActorQuery, FactQuery, FactValue, GameplayError, ObjectQuery, SemanticDraw, WandGameplayAction,
    WandGameplayReply,
};
