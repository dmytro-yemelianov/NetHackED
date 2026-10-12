use super::*;
use crate::{ObjectTimerError, ObjectTimerKind, ObjectTimerTarget};
use nethacked_data::{ruleset::MonsterDef, ItemKindId, RaceId, RoleId};
use nethacked_types::Tile;
use p::*;
type Error = GameplayError;
pub(super) struct Objects<'a, 'v>(pub &'a Invocation<'a, 'v>);
pub(super) struct Actors<'a, 'v>(pub &'a Invocation<'a, 'v>);
pub(super) struct Geography<'a, 'v>(pub &'a Invocation<'a, 'v>);
pub(super) struct Hero<'a, 'v> {
    pub invocation: &'a Invocation<'a, 'v>,
    pub role: String,
}
pub(super) struct Clock<'a, 'v>(pub &'a Invocation<'a, 'v>);
pub(super) struct Random<'a, 'v>(pub &'a Invocation<'a, 'v>);
fn target(id: ItemId) -> ObjectRef {
    ObjectRef {
        id,
        residency: Residency::Active,
    }
}
fn timer_target(t: ObjectTimerTarget) -> ObjectRef {
    match t {
        ObjectTimerTarget::Active(id) => target(id),
        ObjectTimerTarget::Cached {
            branch,
            depth,
            item,
        } => ObjectRef {
            id: item,
            residency: Residency::Cached { branch, depth },
        },
    }
}
fn applied(r: OperationResult) -> Result<(), Error> {
    if matches!(r, OperationResult::Applied) {
        Ok(())
    } else {
        Err(Error::Protocol)
    }
}
impl TryObjectFacts<Error> for Objects<'_, '_> {
    fn carried(&self, a: ActorId) -> Result<Vec<ItemId>, Error> {
        match self.0.facts(FactQuery::Objects(ObjectQuery::Carried(a)))? {
            FactValue::Items(v) => Ok(v),
            _ => Err(Error::Protocol),
        }
    }
    fn object(&self, id: ItemId) -> Result<Option<SelectedObject>, Error> {
        match self
            .0
            .facts(FactQuery::Objects(ObjectQuery::Selected(id)))?
        {
            FactValue::Selected(v) => Ok(v.map(|s| SelectedObject {
                class: s.class,
                name: s.name,
                definition: s.definition,
                buc: s.buc,
                enchantment: s.enchantment,
            })),
            _ => Err(Error::Protocol),
        }
    }
    fn floor_kind(&self, coord: Coord, kind: ItemKindId, _: &Ruleset) -> Result<bool, Error> {
        self.boolean(ObjectQuery::Floor { coord, kind })
    }
    fn has_amulet(&self, a: ActorId) -> Result<bool, Error> {
        self.boolean(ObjectQuery::Amulet(a))
    }
}
impl Objects<'_, '_> {
    fn boolean(&self, q: ObjectQuery) -> Result<bool, Error> {
        match self.0.facts(FactQuery::Objects(q))? {
            FactValue::Bool(b) => Ok(b),
            _ => Err(Error::Protocol),
        }
    }
}
impl TryObjects<Error> for Objects<'_, '_> {
    fn source_charge_policy(&self) -> bool {
        self.0.config.wand_charge_attempts
    }
    fn charge_attempt(
        &mut self,
        id: ItemId,
        spe: i8,
        supported: bool,
    ) -> Result<Option<nethacked_core::artifacts_wands::WandChargeAttempt>, Error> {
        if !supported {
            return Err(Error::Protocol);
        }
        let OperationResult::ChargeAttempt {
            remaining,
            ready,
            wrested,
        } = self
            .0
            .command(WandOwnerCommand::Objects(ObjectOperation::ChargeAttempt {
                target: target(id),
                expected: spe,
            }))?
        else {
            return Err(Error::Protocol);
        };
        Ok(Some(if ready {
            nethacked_core::artifacts_wands::WandChargeAttempt::Ready {
                spe: remaining,
                wrested,
            }
        } else {
            nethacked_core::artifacts_wands::WandChargeAttempt::Unzappable { spe: remaining }
        }))
    }
    fn spend_positive_charge(&mut self, id: ItemId, remaining: i8) -> Result<(), Error> {
        match self
            .0
            .command(WandOwnerCommand::Objects(ObjectOperation::PositiveCharge {
                target: target(id),
                expected: remaining.checked_add(1).ok_or(Error::Protocol)?,
            }))? {
            OperationResult::Charge { remaining: r } if r == remaining => Ok(()),
            _ => Err(Error::Protocol),
        }
    }
    fn insert_generated(&mut self, object: ItemRecord) -> Result<ItemId, Error> {
        match self.0.command(WandOwnerCommand::Objects(
            ObjectOperation::PublishGenerated { object },
        ))? {
            OperationResult::Object { id } => Ok(id),
            _ => Err(Error::Protocol),
        }
    }
    fn timer_target_exists(&self, t: ObjectTimerTarget) -> Result<bool, Error> {
        self.boolean(ObjectQuery::Exists(timer_target(t)))
    }
    fn frozen(&self, id: ItemId) -> Result<bool, Error> {
        match self
            .0
            .read(WandQuery::Command(CommandQuery::Object {
                target: target(id),
            }))?
            .facts
        {
            WandFacts::Command(CommandFacts::Object { frozen, .. }) => Ok(frozen),
            _ => Err(Error::Protocol),
        }
    }
}
impl TryActors<Error> for Actors<'_, '_> {
    fn actor(&self, id: ActorId) -> Result<Option<ActorRecord>, Error> {
        match self.0.facts(FactQuery::Actors(ActorQuery::Record(id)))? {
            FactValue::Actor(v) => Ok(v),
            _ => Err(Error::Protocol),
        }
    }
    fn at(&self, coord: Coord) -> Result<Option<ActorId>, Error> {
        match self.0.facts(FactQuery::Actors(ActorQuery::At(coord)))? {
            FactValue::At(v) => Ok(v),
            _ => Err(Error::Protocol),
        }
    }
    fn damage(&mut self, target: ActorId, amount: u32) -> Result<Option<bool>, Error> {
        match self
            .0
            .command(WandOwnerCommand::Actors(ActorOperation::Damage {
                target,
                amount,
            }))? {
            OperationResult::Damage { dead } => Ok(Some(dead)),
            _ => Err(Error::Protocol),
        }
    }
    fn polymorph(&mut self, target: ActorId, d: &MonsterDef) -> Result<(), Error> {
        applied(
            self.0
                .command(WandOwnerCommand::Actors(ActorOperation::Polymorph {
                    target,
                    definition: d.name.clone(),
                }))?,
        )
    }
    fn make_hostile(&mut self, target: ActorId) -> Result<(), Error> {
        applied(
            self.0
                .command(WandOwnerCommand::Actors(ActorOperation::MakeHostile {
                    target,
                }))?,
        )
    }
    fn guardians(
        &self,
        name: &str,
        include_sound: bool,
        _: &Ruleset,
    ) -> Result<Vec<ActorId>, Error> {
        match self.0.facts(FactQuery::Actors(ActorQuery::Guardians {
            name: name.into(),
            include_sound,
        }))? {
            FactValue::Actors(v) => Ok(v),
            _ => Err(Error::Protocol),
        }
    }
    fn spawn_initialized(&mut self, actor: ActorRecord) -> Result<ActorId, Error> {
        match self.0.command(WandOwnerCommand::Actors(
            ActorOperation::PublishInitialized { actor },
        ))? {
            OperationResult::Actor { id } => Ok(id),
            _ => Err(Error::Protocol),
        }
    }
    fn force_hostile_malign(&mut self, target: ActorId, malign: i32) -> Result<(), Error> {
        applied(self.0.command(WandOwnerCommand::Actors(
            ActorOperation::ForceHostileMalign { target, malign },
        ))?)
    }
    fn gain_level(&mut self, target: ActorId, hp: u32, level: u32) -> Result<(), Error> {
        applied(
            self.0
                .command(WandOwnerCommand::Actors(ActorOperation::GainLevel {
                    target,
                    hp,
                    level,
                }))?,
        )
    }
    fn genocided(&self, name: &str, glyph: char) -> Result<bool, Error> {
        Ok(self.genocide_filter()?(name, glyph))
    }
    fn genocide_filter(&self) -> Result<impl Fn(&str, char) -> bool + '_, Error> {
        let mut species = BTreeSet::new();
        let mut classes = BTreeSet::new();
        let mut offset = 0;
        loop {
            match self.0.facts(FactQuery::Genocide { version: 3, offset })? {
                FactValue::Genocide {
                    species: s,
                    classes: c,
                    next,
                } => {
                    species.extend(s);
                    classes.extend(c);
                    if let Some(n) = next {
                        if n <= offset {
                            return Err(Error::Protocol);
                        }
                        offset = n;
                    } else {
                        break;
                    }
                }
                _ => return Err(Error::Protocol),
            }
        }
        Ok(move |name: &str, glyph: char| species.contains(name) || classes.contains(&glyph))
    }
}
impl TryGeographyFacts<Error> for Geography<'_, '_> {
    fn tile(&self, coord: Coord) -> Result<Tile, Error> {
        let g = self.read(vec![coord], None)?;
        g.cells
            .into_iter()
            .find(|c| c.coord == coord)
            .map(|c| c.tile)
            .ok_or(Error::Protocol)
    }
    fn trajectory(
        &self,
        origin: Coord,
        direction: Direction,
        energy: u32,
    ) -> Result<Vec<Coord>, Error> {
        Ok(self
            .read(
                vec![],
                Some(TrajectoryRequest {
                    version: 1,
                    origin,
                    direction,
                    energy,
                }),
            )?
            .trajectory)
    }
    fn engraving(
        &self,
        coord: Coord,
    ) -> Result<Option<nethacked_core::engraving::Engraving>, Error> {
        match self
            .0
            .read(WandQuery::Command(CommandQuery::Geography { coord }))?
            .facts
        {
            WandFacts::Command(CommandFacts::Geography { engraving, .. }) => Ok(engraving),
            _ => Err(Error::Protocol),
        }
    }
}
impl Geography<'_, '_> {
    fn read(
        &self,
        cells: Vec<Coord>,
        trajectory: Option<TrajectoryRequest>,
    ) -> Result<wand::GeographyFacts, Error> {
        match self
            .0
            .read(WandQuery::Geography(GeographyRequest {
                residency: Residency::Active,
                cells,
                trajectory,
            }))?
            .facts
        {
            WandFacts::Geography(g) => Ok(g),
            _ => Err(Error::Protocol),
        }
    }
    fn effect(&self, coord: Coord, effect: GeographyOperation) -> Result<OperationResult, Error> {
        self.0
            .command(WandOwnerCommand::Geography { coord, effect })
    }
}
impl TryGeography<Error> for Geography<'_, '_> {
    fn reveal_secret_door(&mut self, c: Coord) -> Result<bool, Error> {
        match self.effect(c, GeographyOperation::RevealSecretDoor)? {
            OperationResult::Revealed { changed } => Ok(changed),
            _ => Err(Error::Protocol),
        }
    }
    fn collapse_drawbridge(&mut self, c: Coord) -> Result<(), Error> {
        applied(self.effect(c, GeographyOperation::CollapseDrawbridge)?)
    }
    fn break_door(&mut self, c: Coord) -> Result<(), Error> {
        applied(self.effect(c, GeographyOperation::BreakDoor)?)
    }
    fn freeze_pool(&mut self, c: Coord) -> Result<(), Error> {
        applied(self.effect(c, GeographyOperation::FreezePool)?)
    }
    fn erase_engraving(&mut self, c: Coord) -> Result<(), Error> {
        applied(self.effect(c, GeographyOperation::EraseEngraving)?)
    }
}
impl Hero<'_, '_> {
    fn facts(&self) -> Result<CommandFacts, Error> {
        match self
            .invocation
            .read(WandQuery::Command(CommandQuery::Hero))?
            .facts
        {
            WandFacts::Command(c) => Ok(c),
            _ => Err(Error::Protocol),
        }
    }
    fn effect(&self, op: HeroOperation) -> Result<(), Error> {
        applied(self.invocation.command(WandOwnerCommand::Hero(op))?)
    }
}
impl TryHeroFacts<Error> for Hero<'_, '_> {
    fn role_id(&self) -> Result<Option<RoleId>, Error> {
        match self
            .invocation
            .read(WandQuery::Command(CommandQuery::Session))?
            .facts
        {
            WandFacts::Command(CommandFacts::Session { role_id, .. }) => Ok(role_id),
            _ => Err(Error::Protocol),
        }
    }
    fn role_name(&self) -> Result<&str, Error> {
        Ok(&self.role)
    }
    fn race(&self) -> Result<RaceId, Error> {
        match self.facts()? {
            CommandFacts::Hero { race, .. } => Ok(race),
            _ => Err(Error::Protocol),
        }
    }
    fn alignment_record(&self) -> Result<i32, Error> {
        match self.facts()? {
            CommandFacts::Hero { alignment, .. } => Ok(alignment),
            _ => Err(Error::Protocol),
        }
    }
}
impl TryHero<Error> for Hero<'_, '_> {
    fn polymorphed(&self) -> Result<bool, Error> {
        match self.invocation.read(WandQuery::Hero)?.facts {
            WandFacts::Hero(h) => Ok(h.polymorph.is_some()),
            _ => Err(Error::Protocol),
        }
    }
    fn adjust_alignment(&mut self, delta: i32, limit: i32) -> Result<(), Error> {
        self.effect(HeroOperation::AdjustAlignment { delta, limit })
    }
    fn record_kill(&mut self) -> Result<(), Error> {
        self.effect(HeroOperation::RecordKill)
    }
    fn quest_leader_killed(&self) -> Result<bool, Error> {
        match self.facts()? {
            CommandFacts::Hero { quest, .. } => Ok(quest.killed_leader),
            _ => Err(Error::Protocol),
        }
    }
    fn quest_progress(&self) -> Result<nethacked_core::QuestProgress, Error> {
        match self.facts()? {
            CommandFacts::Hero { quest, .. } => Ok(quest.progress),
            _ => Err(Error::Protocol),
        }
    }
    fn mark_quest_leader_killed(&mut self) -> Result<(), Error> {
        self.effect(HeroOperation::MarkQuestLeaderKilled)
    }
    fn defeat_quest_nemesis(&mut self) -> Result<(), Error> {
        self.effect(HeroOperation::DefeatQuestNemesis)
    }
    fn adjust_favor(&mut self, delta: i32) -> Result<(), Error> {
        self.effect(HeroOperation::AdjustFavor { delta })
    }
    fn adjust_luck(&mut self, delta: i32) -> Result<(), Error> {
        self.effect(HeroOperation::AdjustLuck { delta })
    }
    fn clear_divine_protection(&mut self) -> Result<(), Error> {
        self.effect(HeroOperation::ClearDivineProtection)
    }
    fn credit_experience(&mut self, amount: u64) -> Result<(), Error> {
        self.effect(HeroOperation::CreditExperience { amount })
    }
    fn experience(&self) -> Result<u64, Error> {
        match self.facts()? {
            CommandFacts::Hero { experience, .. } => Ok(experience),
            _ => Err(Error::Protocol),
        }
    }
    fn level_max(&self) -> Result<u32, Error> {
        match self.facts()? {
            CommandFacts::Hero { level_max, .. } => Ok(level_max),
            _ => Err(Error::Protocol),
        }
    }
    fn constitution(&self) -> Result<i32, Error> {
        match self.facts()? {
            CommandFacts::Hero { constitution, .. } => Ok(constitution),
            _ => Err(Error::Protocol),
        }
    }
    fn record_level_gain(
        &mut self,
        from: u32,
        hp: i32,
        energy: i32,
        experience: u64,
    ) -> Result<(), Error> {
        self.effect(HeroOperation::RecordLevelGain {
            from,
            hp,
            energy,
            experience,
        })
    }
}
impl TryWorldClock<Error> for Clock<'_, '_> {
    fn turn(&self) -> Result<u64, Error> {
        match self.0.facts(FactQuery::Clock)? {
            FactValue::Turn(t) => Ok(t),
            _ => Err(Error::Protocol),
        }
    }
}
impl TryClock<Error> for Clock<'_, '_> {
    fn hero_act(&mut self) -> Result<(), Error> {
        if self.0.config.wand_charge_attempts {
            if self.0.deferred_time.replace(true) {
                return Err(Error::Protocol);
            }
            return Ok(());
        }
        applied(
            self.0
                .command(WandOwnerCommand::Clock(ClockOperation::HeroAct))?,
        )
    }
    fn start_corpse_timer(
        &mut self,
        target: ObjectTimerTarget,
        kind: ObjectTimerKind,
        due: Option<u64>,
        _: &mut impl TryObjects<Error>,
    ) -> Result<Result<u64, ObjectTimerError>, Error> {
        match self
            .0
            .command(WandOwnerCommand::Clock(ClockOperation::StartCorpseTimer {
                target: timer_target(target),
                kind,
                due,
            }))? {
            OperationResult::Timer { serial } => Ok(Ok(serial)),
            OperationResult::TimerRefused { reason } => Ok(Err(reason.into())),
            _ => Err(Error::Protocol),
        }
    }
    fn cancel_corpse_timers(&mut self, target: ObjectTimerTarget) -> Result<(), Error> {
        applied(self.0.command(WandOwnerCommand::Clock(
            ClockOperation::CancelCorpseTimers {
                target: timer_target(target),
            },
        ))?)
    }
}
impl p::Random<Error> for Random<'_, '_> {
    fn sample(&mut self, draw: WandDraw) -> Result<u32, Error> {
        let reservation = match self.0.read(WandQuery::Random { draws: 1 })?.facts {
            WandFacts::Random(r) => r,
            _ => return Err(Error::Protocol),
        };
        match self.0.command(WandOwnerCommand::Sample {
            reservation,
            draw: draw.into(),
        })? {
            OperationResult::Sample(v) => Ok(v),
            _ => Err(Error::Protocol),
        }
    }
}
