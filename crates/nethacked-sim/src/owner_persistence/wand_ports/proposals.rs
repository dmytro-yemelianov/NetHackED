use super::*;

/// V2 requests are available only under the explicitly approved command profile.
/// The witness is a compare guard, not authority; live invocation grants and the
/// sealed owner ticket remain mandatory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WandOperation {
    pub expected_owner: OwnerWitness,
    pub dependencies: Vec<OwnerWitness>,
    pub command: WandOwnerCommand,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WandOwnerCommand {
    Sample {
        reservation: RandomReservation,
        draw: super::super::runtime::wand_gameplay::SemanticDraw,
    },
    Objects(ObjectOperation),
    Actors(ActorOperation),
    Geography {
        coord: Coord,
        effect: GeographyOperation,
    },
    Hero(HeroOperation),
    Clock(ClockOperation),
    Random {
        reservation: RandomReservation,
        word: RawWord,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObjectOperation {
    ChargeAttempt {
        target: ObjectRef,
        expected: i8,
    },
    PositiveCharge {
        target: ObjectRef,
        expected: i8,
    },
    /// A single initialized generated object, never a partition replacement.
    PublishGenerated {
        object: nethacked_arena::ItemRecord,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActorOperation {
    Damage {
        target: ActorId,
        amount: u32,
    },
    Polymorph {
        target: ActorId,
        definition: String,
    },
    MakeHostile {
        target: ActorId,
    },
    ForceHostileMalign {
        target: ActorId,
        malign: i32,
    },
    PublishInitialized {
        actor: nethacked_arena::ActorRecord,
    },
    GainLevel {
        target: ActorId,
        hp: u32,
        level: u32,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GeographyOperation {
    RevealSecretDoor,
    CollapseDrawbridge,
    BreakDoor,
    FreezePool,
    EraseEngraving,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HeroOperation {
    AdjustAlignment {
        delta: i32,
        limit: i32,
    },
    RecordKill,
    MarkQuestLeaderKilled,
    DefeatQuestNemesis,
    AdjustFavor {
        delta: i32,
    },
    AdjustLuck {
        delta: i32,
    },
    ClearDivineProtection,
    CreditExperience {
        amount: u64,
    },
    RecordLevelGain {
        from: u32,
        hp: i32,
        energy: i32,
        experience: u64,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClockOperation {
    HeroAct,
    StartCorpseTimer {
        target: ObjectRef,
        kind: crate::ObjectTimerKind,
        due: Option<u64>,
    },
    CancelCorpseTimers {
        target: ObjectRef,
    },
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum RawWord {
    U32,
    U64,
}

/// Additional finite facts needed by the shared ports. V1 query encodings and
/// responses retain their original meaning and profile identities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandQuery {
    WrestPreview,
    Gameplay(super::super::runtime::wand_gameplay::FactQuery),
    LifetimeObject {
        target: ObjectRef,
    },
    LifetimeBilling {
        target: ObjectRef,
    },
    Object {
        target: ObjectRef,
    },
    Actor {
        target: ActorId,
    },
    Geography {
        coord: Coord,
    },
    Hero,
    Genocide {
        definition: String,
    },
    Session,
    TimerAdmission {
        target: ObjectRef,
        kind: crate::ObjectTimerKind,
        due: Option<u64>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandFacts {
    WrestPreview {
        reservation: RandomReservation,
        draw: u32,
    },
    Gameplay(super::super::runtime::wand_gameplay::FactValue),
    LifetimeObject(super::super::runtime::item_lifetime::ObjectFacts),
    LifetimeBilling(super::super::runtime::item_lifetime::LedgerFacts),
    Object {
        target: ObjectRef,
        frozen: bool,
    },
    Actor {
        target: ActorId,
        actor: nethacked_arena::ActorRecord,
    },
    Geography {
        coord: Coord,
        engraving: Option<nethacked_core::engraving::Engraving>,
    },
    Hero {
        race: nethacked_data::RaceId,
        alignment: i32,
        pacifist: bool,
        quest: nethacked_core::QuestState,
        experience: u64,
        level_max: u32,
        constitution: i32,
        favor: i32,
        protection: u32,
        power: u32,
        max_power: u32,
    },
    Genocide {
        definition: String,
        forbidden: bool,
    },
    Session {
        role_id: Option<nethacked_data::RoleId>,
        role_name: String,
    },
    TimerAdmission {
        result: Result<u64, TimerFailure>,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimerFailure {
    MissingObject,
    Duplicate,
    ClockOverflow,
    SerialOverflow,
}
impl From<crate::object_timers::ObjectTimerError> for TimerFailure {
    fn from(error: crate::object_timers::ObjectTimerError) -> Self {
        use crate::object_timers::ObjectTimerError as E;
        match error {
            E::MissingObject => Self::MissingObject,
            E::Duplicate => Self::Duplicate,
            E::ClockOverflow => Self::ClockOverflow,
            E::SerialOverflow => Self::SerialOverflow,
        }
    }
}
impl From<TimerFailure> for crate::object_timers::ObjectTimerError {
    fn from(error: TimerFailure) -> Self {
        match error {
            TimerFailure::MissingObject => Self::MissingObject,
            TimerFailure::Duplicate => Self::Duplicate,
            TimerFailure::ClockOverflow => Self::ClockOverflow,
            TimerFailure::SerialOverflow => Self::SerialOverflow,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationResult {
    ChargeAttempt {
        remaining: i8,
        ready: bool,
        wrested: bool,
    },
    Applied,
    Charge {
        remaining: i8,
    },
    Object {
        id: ItemId,
    },
    Actor {
        id: ActorId,
    },
    Damage {
        dead: bool,
    },
    Revealed {
        changed: bool,
    },
    Timer {
        serial: u64,
    },
    TimerRefused {
        reason: TimerFailure,
    },
    Sample(u32),
    Raw32(u32),
    Raw64(u64),
}

/// Legal domain requests for subsequent sealed preparation. None is a state
/// replacement, and I1 grants no write endpoint for these requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObjectCommand {
    UseCharge {
        target: ObjectRef,
        expected_charges: i8,
        wrest_draw: Option<u32>,
    },
    DisposeWhole {
        target: ObjectRef,
        expected_quantity: u32,
    },
    Place {
        target: ObjectRef,
        destination: ItemLocation,
    },
    Create {
        definition: ItemDefinition,
        quantity: u32,
        buc: Buc,
        placement: ItemLocation,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EquipmentCommand {
    ClearMatching { target: ObjectRef },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BillingCommand {
    AdmitReceipt {
        target: ObjectRef,
        terms: BillTerms,
    },
    RetainConsumed {
        target: ObjectRef,
        expected_quantity: u32,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClockCommand {
    SpendHeroEnergy {
        cost: u32,
    },
    CancelMatching {
        target: ObjectRef,
    },
    Register {
        target: ObjectRef,
        kind: crate::ObjectTimerKind,
        due: u64,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RandomDrawRequest {
    pub reservation: RandomReservation,
    pub upper_bounds: Vec<u32>,
}
impl RandomDrawRequest {
    pub fn validate(&self) -> owners::Result<()> {
        bound(
            usize::from(self.reservation.max_draws),
            usize::from(MAX_DRAWS),
        )?;
        bound(self.upper_bounds.len(), usize::from(MAX_DRAWS))?;
        bound(
            self.upper_bounds.len(),
            usize::from(self.reservation.max_draws),
        )?;
        if self.reservation.algorithm_version != 1 || self.upper_bounds.contains(&0) {
            return Err(owners::Error::Schema);
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GeographyEffect {
    BreakDoor,
    CollapseDrawbridge,
    FreezePool,
    RevealSecretDoor,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeographyCommand {
    pub residency: Residency,
    pub cells: Vec<(Coord, GeographyEffect)>,
}
impl GeographyCommand {
    pub fn validate(&self) -> owners::Result<()> {
        bound(self.cells.len(), MAX_CELLS)?;
        for (c, _) in &self.cells {
            coord(*c)?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActorEffect {
    Damage {
        amount: u32,
    },
    Polymorph {
        species: nethacked_data::MonsterSpeciesId,
    },
    Move {
        destination: Coord,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorsCommand {
    pub residency: Residency,
    pub targets: Vec<(ActorId, ActorEffect)>,
}
impl ActorsCommand {
    pub fn validate(&self) -> owners::Result<()> {
        bound(self.targets.len(), MAX_TARGETS)?;
        let mut ids = std::collections::BTreeSet::new();
        for (id, effect) in &self.targets {
            if !ids.insert(*id) {
                return Err(owners::Error::Schema);
            }
            if let ActorEffect::Move { destination } = effect {
                coord(*destination)?;
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HeroCommand {
    Damage { amount: u32 },
    ExerciseStrength { positive: bool },
}

fn placement(p: &ItemLocation) -> owners::Result<()> {
    if let ItemLocation::Floor(c) = p {
        coord(*c)?;
    }
    Ok(())
}
fn definition(d: &ItemDefinition) -> owners::Result<()> {
    match d {
        ItemDefinition::Vanilla(s) | ItemDefinition::Custom(s) => {
            text(s)?;
            if s.is_empty() {
                return Err(owners::Error::Schema);
            }
        }
    }
    Ok(())
}
impl ObjectCommand {
    pub fn validate(&self) -> owners::Result<()> {
        match self {
            Self::UseCharge {
                expected_charges,
                wrest_draw,
                ..
            } => {
                // Use the already accepted prescribed-draw formula; no RNG here.
                match nethacked_core::artifacts_wands::wand_charge_attempt(
                    *expected_charges,
                    *wrest_draw,
                ) {
                    Ok(nethacked_core::artifacts_wands::WandChargeAttempt::Ready { .. }) => {}
                    _ => return Err(owners::Error::Invariant),
                }
            }
            Self::DisposeWhole {
                expected_quantity: 0,
                ..
            }
            | Self::Create { quantity: 0, .. } => return Err(owners::Error::Invariant),
            Self::Create {
                definition: d,
                placement: p,
                ..
            } => {
                definition(d)?;
                placement(p)?;
            }
            Self::Place { destination, .. } => placement(destination)?,
            _ => {}
        }
        Ok(())
    }
}
impl BillingCommand {
    pub fn validate(&self) -> owners::Result<()> {
        match self {
            Self::RetainConsumed {
                expected_quantity: 0,
                ..
            } => Err(owners::Error::Invariant),
            Self::AdmitReceipt { terms, .. } => crate::world::validate_bill_terms(
                terms,
                terms.billed_quantity.ok_or(owners::Error::Invariant)?,
                terms.legacy_base_cost,
            )
            .map_err(|_| owners::Error::Invariant),
            _ => Ok(()),
        }
    }
}
