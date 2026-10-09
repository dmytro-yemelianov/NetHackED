use super::*;

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
