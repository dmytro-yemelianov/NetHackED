use super::*;
use nethacked_arena::ItemLocation;
use nethacked_types::Tile;
use rand::RngCore;

fn valid_coord(c: nethacked_types::Coord) -> owners::Result<()> {
    if c.x >= nethacked_types::COLNO || c.y >= nethacked_types::ROWNO {
        Err(owners::Error::Schema)
    } else {
        Ok(())
    }
}
fn valid_text(s: &str) -> owners::Result<()> {
    if s.is_empty() || s.len() > MAX_TEXT_BYTES {
        Err(owners::Error::Schema)
    } else {
        Ok(())
    }
}
fn fact(obs: &[Observation], i: usize) -> owners::Result<WandFacts> {
    decode(
        obs.get(i).ok_or(owners::Error::Schema)?.bytes(),
        MAX_RESPONSE_BYTES,
    )
}
fn geography(obs: &[Observation], i: usize) -> owners::Result<GeographyFacts> {
    match fact(obs, i)? {
        WandFacts::Geography(g) => Ok(g),
        _ => Err(owners::Error::Schema),
    }
}
fn cache(obs: &[Observation], i: usize, target: ObjectRef) -> owners::Result<Option<usize>> {
    let g = geography(obs, i)?;
    if g.residency != target.residency {
        return Err(owners::Error::Schema);
    }
    Ok(g.stored_index)
}
fn object<'a>(
    s: &'a ObjectsState,
    obs: &[Observation],
    target: ObjectRef,
) -> owners::Result<&'a nethacked_arena::ItemRecord> {
    match cache(obs, 2, target)? {
        None => s.items.get(target.id),
        Some(i) => s.stored_items.get(i).and_then(|items| {
            items
                .iter()
                .find(|(id, _)| *id == target.id)
                .map(|(_, item)| item)
        }),
    }
    .ok_or(owners::Error::Stale)
}
impl CommandDomain for ObjectsState {
    fn command_facts(
        &self,
        q: &CommandQuery,
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<CommandFacts> {
        if let CommandQuery::LifetimeObject { target } = q {
            return super::super::item_lifetime::object_facts(self, *target, obs, c)
                .map(CommandFacts::LifetimeObject);
        }
        if let CommandQuery::Gameplay(q) = q {
            use super::super::wand_gameplay::{facts, FactQuery};
            if !c.wand_gameplay {
                return Err(owners::Error::Unsupported);
            }
            return Ok(CommandFacts::Gameplay(match q {
                FactQuery::Objects(q) => facts::objects(self, q, c)?,
                _ => return Err(owners::Error::Schema),
            }));
        }
        let CommandQuery::Object { target } = q else {
            return Err(owners::Error::Schema);
        };
        let item = object(self, obs, *target)?;
        let index = cache(obs, 2, *target)?;
        let frozen = nethacked_arena::frozen_in_ice_box(item, |id| match index {
            None => self.items.get(id),
            Some(i) => self.stored_items[i]
                .iter()
                .find(|(key, _)| *key == id)
                .map(|(_, item)| item),
        });
        Ok(CommandFacts::Object {
            target: *target,
            frozen,
        })
    }
    fn apply(
        &mut self,
        command: &WandOwnerCommand,
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<OperationResult> {
        let WandOwnerCommand::Objects(op) = command else {
            return Err(owners::Error::Schema);
        };
        match op {
            ObjectOperation::ChargeAttempt { target, expected } => {
                super::charge::apply(self, *target, *expected, obs, c)
            }
            ObjectOperation::PositiveCharge { target, expected } => {
                // This unit implements the positive-only prefix, never wrest or retirement.
                let current = object(self, obs, *target)?;
                if *expected <= 0
                    || current.enchantment != *expected
                    || current.class != nethacked_types::ItemClass::Wand
                {
                    return Err(owners::Error::Invariant);
                }
                if let Some(identity) = &current.definition {
                    let definition = c
                        .ruleset
                        .item_for_definition(identity)
                        .ok_or(owners::Error::Schema)?;
                    if definition.class != nethacked_types::ItemClass::Wand {
                        return Err(owners::Error::Schema);
                    }
                }
                let remaining = expected - 1;
                match cache(obs, 2, *target)? {
                    None => {
                        self.items
                            .get_mut(target.id)
                            .ok_or(owners::Error::Stale)?
                            .enchantment = remaining
                    }
                    Some(i) => {
                        self.stored_items[i]
                            .iter_mut()
                            .find(|(id, _)| *id == target.id)
                            .ok_or(owners::Error::Stale)?
                            .1
                            .enchantment = remaining
                    }
                }
                Ok(OperationResult::Charge { remaining })
            }
            ObjectOperation::PublishGenerated { object: item } => {
                valid_text(&item.name)?;
                if item.quantity == 0 || item.generated_bytes_too_large() {
                    return Err(owners::Error::Schema);
                }
                let def = match &item.definition {
                    Some(d) => c.ruleset.item_for_definition(d),
                    None => c.ruleset.item(&item.name),
                }
                .ok_or(owners::Error::Schema)?;
                if item.class != def.class {
                    return Err(owners::Error::Schema);
                }
                if let Some(id) = &item.artifact_id {
                    valid_text(id)?;
                    if !c.ruleset.artifacts.iter().any(|a| a.id == *id) {
                        return Err(owners::Error::Schema);
                    }
                    if self.artifact_history.contains(id) {
                        return Err(owners::Error::Invariant);
                    }
                }
                match item.location {
                    ItemLocation::Floor(coord) => {
                        valid_coord(coord)?;
                        let g = geography(obs, 2)?;
                        if g.residency != Residency::Active
                            || !g.cells.iter().any(|cell| cell.coord == coord)
                        {
                            return Err(owners::Error::Schema);
                        }
                    }
                    ItemLocation::InContainer(parent) => {
                        if !self.items.get(parent).is_some_and(|p| p.is_container) {
                            return Err(owners::Error::Stale);
                        }
                    }
                    _ => return Err(owners::Error::Unsupported),
                }
                if let Some(id) = &item.artifact_id {
                    self.artifact_history.insert(id.clone());
                }
                Ok(OperationResult::Object {
                    id: self.items.insert(item.clone()),
                })
            }
        }
    }
}
// ItemRecord is a single initialized generated value, bounded on the wire and
// again when encoded by the codec. No state or ID replacement is accepted.
trait GeneratedBound {
    fn generated_bytes_too_large(&self) -> bool;
}
impl GeneratedBound for nethacked_arena::ItemRecord {
    fn generated_bytes_too_large(&self) -> bool {
        serde_json::to_vec(self).map_or(true, |b| b.len() > MAX_REQUEST_BYTES / 2)
    }
}
impl CommandDomain for ActorsState {
    fn command_facts(
        &self,
        q: &CommandQuery,
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<CommandFacts> {
        if let CommandQuery::Gameplay(q) = q {
            use super::super::wand_gameplay::{facts, FactQuery};
            if !c.wand_gameplay {
                return Err(owners::Error::Unsupported);
            }
            return Ok(CommandFacts::Gameplay(match q {
                FactQuery::Actors(q) => facts::actors(self, q, c)?,
                FactQuery::Genocide { .. } => match fact(obs, 0)? {
                    WandFacts::Command(CommandFacts::Gameplay(
                        super::super::wand_gameplay::FactValue::Genocide {
                            species,
                            classes,
                            next,
                        },
                    )) => super::super::wand_gameplay::FactValue::Genocide {
                        species,
                        classes,
                        next,
                    },
                    _ => return Err(owners::Error::Schema),
                },
                _ => return Err(owners::Error::Schema),
            }));
        }
        let CommandQuery::Actor { target } = q else {
            return Err(owners::Error::Schema);
        };
        let actor = self.actors.get(*target).ok_or(owners::Error::Stale)?;
        valid_text(&actor.name)?;
        if actor.abilities.len() > MAX_TARGETS {
            return Err(owners::Error::Limit);
        }
        Ok(CommandFacts::Actor {
            target: *target,
            actor: actor.clone(),
        })
    }
    fn apply(
        &mut self,
        command: &WandOwnerCommand,
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<OperationResult> {
        let WandOwnerCommand::Actors(op) = command else {
            return Err(owners::Error::Schema);
        };
        if let ActorOperation::PublishInitialized { actor } = op {
            valid_coord(actor.coord)?;
            valid_text(&actor.name)?;
            let def = c
                .ruleset
                .monster_for_actor(actor)
                .ok_or(owners::Error::Schema)?;
            let g = geography(obs, 0)?;
            if g.residency != Residency::Active
                || !g.cells.iter().any(|cell| {
                    cell.coord == actor.coord
                        && crate::actions::wand::tile_placement(def, &cell.tile).allowed()
                })
                || self
                    .actors
                    .iter()
                    .any(|(_, a)| !a.is_dead && a.coord == actor.coord)
            {
                return Err(owners::Error::Invariant);
            }
            match fact(obs, 1)? {
                WandFacts::Command(CommandFacts::Genocide {
                    definition,
                    forbidden: false,
                }) if definition == actor.name => {}
                _ => return Err(owners::Error::Invariant),
            }
            let base = def.create_record_with_hp(actor.coord, actor.hp, actor.max_hp, actor.level);
            if actor.is_player
                || actor.is_dead
                || actor.is_tame
                || actor.tameness != 0
                || actor.hp == 0
                || (def.sample_hp && actor.hp != actor.max_hp)
                || (!def.sample_hp
                    && (actor.hp != def.base_hp
                        || actor.max_hp != def.max_hp
                        || actor.level != def.level))
                || actor.ac != base.ac
                || actor.speed != base.speed
                || actor.intrinsics != base.intrinsics
                || actor.abilities != base.abilities
                || actor.name != def.name
                || actor.species != base.species
                || actor.is_unique != base.is_unique
                || actor.abilities.len() > MAX_TARGETS
            {
                return Err(owners::Error::Schema);
            }
            self.actors = decode(&encode(&self.actors, PAYLOAD)?, PAYLOAD)?;
            return Ok(OperationResult::Actor {
                id: self.actors.insert(actor.clone()),
            });
        }
        let target = match op {
            ActorOperation::Damage { target, .. }
            | ActorOperation::Polymorph { target, .. }
            | ActorOperation::MakeHostile { target }
            | ActorOperation::ForceHostileMalign { target, .. }
            | ActorOperation::GainLevel { target, .. } => *target,
            _ => return Err(owners::Error::Schema),
        };
        let actor = self.actors.get_mut(target).ok_or(owners::Error::Stale)?;
        match op {
            ActorOperation::Damage { amount, .. } => {
                actor.hp = actor.hp.saturating_sub(*amount);
                if actor.hp == 0 {
                    actor.is_dead = true;
                }
                return Ok(OperationResult::Damage {
                    dead: actor.is_dead,
                });
            }
            ActorOperation::Polymorph { definition, .. } => {
                valid_text(definition)?;
                let def = c.ruleset.monster(definition).ok_or(owners::Error::Schema)?;
                actor.species = Some(def.actor_species());
                actor.name = def.name.clone();
                actor.hp = def.base_hp;
                actor.max_hp = def.max_hp;
                actor.ac = def.ac;
                actor.speed = def.speed;
                actor.level = def.level;
            }
            ActorOperation::MakeHostile { .. } => actor.is_peaceful = false,
            ActorOperation::ForceHostileMalign { malign, .. } => {
                actor.is_peaceful = false;
                actor.malign = *malign;
            }
            ActorOperation::GainLevel { hp, level, .. } => {
                if *level != actor.level.checked_add(1).ok_or(owners::Error::Limit)?
                    || *level > 30
                    || *hp == 0
                {
                    return Err(owners::Error::Invariant);
                }
                actor.hp = actor.hp.saturating_add(*hp);
                actor.max_hp = actor.max_hp.saturating_add(*hp);
                actor.level = *level;
            }
            _ => return Err(owners::Error::Schema),
        }
        Ok(OperationResult::Applied)
    }
}
impl CommandDomain for GeographyState {
    fn command_facts(
        &self,
        q: &CommandQuery,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<CommandFacts> {
        let CommandQuery::Geography { coord } = q else {
            return Err(owners::Error::Schema);
        };
        valid_coord(*coord)?;
        let engraving = self.level.get_engraving(*coord).cloned();
        if engraving
            .as_ref()
            .is_some_and(|e| serde_json::to_vec(e).map_or(true, |b| b.len() > MAX_TEXT_BYTES))
        {
            return Err(owners::Error::Limit);
        }
        Ok(CommandFacts::Geography {
            coord: *coord,
            engraving,
        })
    }
    fn apply(
        &mut self,
        command: &WandOwnerCommand,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<OperationResult> {
        let WandOwnerCommand::Geography { coord, effect } = command else {
            return Err(owners::Error::Schema);
        };
        valid_coord(*coord)?;
        let tile = self
            .level
            .tiles
            .get_mut(coord.x)
            .and_then(|col| col.get_mut(coord.y))
            .ok_or(owners::Error::Schema)?;
        match effect {
            GeographyOperation::RevealSecretDoor => {
                let changed = matches!(tile, Tile::SecretDoor { .. });
                if changed {
                    tile.reveal_secret_door();
                }
                return Ok(OperationResult::Revealed { changed });
            }
            GeographyOperation::CollapseDrawbridge => {
                if !matches!(tile, Tile::Drawbridge { .. }) {
                    return Err(owners::Error::Invariant);
                }
                *tile = Tile::Moat;
            }
            GeographyOperation::BreakDoor => tile.break_door(),
            GeographyOperation::FreezePool => {
                if !matches!(tile, Tile::Pool { .. }) {
                    return Err(owners::Error::Invariant);
                }
                *tile = Tile::Pool { frozen: true };
            }
            GeographyOperation::EraseEngraving => {
                self.level.remove_engraving(*coord);
            }
        }
        Ok(OperationResult::Applied)
    }
}
impl CommandDomain for HeroState {
    fn command_facts(
        &self,
        q: &CommandQuery,
        _: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<CommandFacts> {
        if let CommandQuery::Gameplay(q) = q {
            use super::super::wand_gameplay::{facts, FactQuery};
            if !c.wand_gameplay {
                return Err(owners::Error::Unsupported);
            }
            return Ok(CommandFacts::Gameplay(match q {
                FactQuery::GenocideRegistry { version, offset } => {
                    facts::genocide(self, *version, *offset)?
                }
                _ => return Err(owners::Error::Schema),
            }));
        }
        if let CommandQuery::Genocide { definition } = q {
            valid_text(definition)?;
            let def = c.ruleset.monster(definition).ok_or(owners::Error::Schema)?;
            return Ok(CommandFacts::Genocide {
                definition: definition.clone(),
                forbidden: nethacked_core::genocide::is_genocided(
                    &self.genocide_registry,
                    &def.name,
                    def.glyph,
                ),
            });
        }
        if !matches!(q, CommandQuery::Hero) {
            return Err(owners::Error::Schema);
        }
        Ok(CommandFacts::Hero {
            race: self.hero_race,
            alignment: self.alignment_record,
            pacifist: self.conducts.pacifist,
            quest: self.quest_state.clone(),
            experience: self.progress.exp,
            level_max: self.progress.level_max,
            constitution: self.progress.attr_base[crate::progress::A_CON],
            favor: self.divine_state.favor,
            protection: self.divine_protection,
            power: self.player_pw,
            max_power: self.player_max_pw,
        })
    }
    fn apply(
        &mut self,
        command: &WandOwnerCommand,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<OperationResult> {
        let WandOwnerCommand::Hero(op) = command else {
            return Err(owners::Error::Schema);
        };
        match op {
            HeroOperation::AdjustAlignment { delta, limit } => {
                if *limit < 0 {
                    return Err(owners::Error::Schema);
                }
                self.alignment_record =
                    nethacked_core::adjalign(self.alignment_record, *delta, *limit);
            }
            HeroOperation::RecordKill => nethacked_core::conducts::record_kill(&mut self.conducts),
            HeroOperation::MarkQuestLeaderKilled => self.quest_state.killed_leader = true,
            HeroOperation::DefeatQuestNemesis => {
                nethacked_core::attack_nemesis(&mut self.quest_state, 9999);
            }
            HeroOperation::AdjustFavor { delta } => {
                self.divine_state.favor = nethacked_core::religion::clamp_favor(
                    self.divine_state
                        .favor
                        .checked_add(*delta)
                        .ok_or(owners::Error::Limit)?,
                )
            }
            HeroOperation::AdjustLuck { delta } => {
                self.player_luck = nethacked_core::mines::clamp_luck(
                    self.player_luck
                        .checked_add(*delta)
                        .ok_or(owners::Error::Limit)?,
                )
            }
            HeroOperation::ClearDivineProtection => self.divine_protection = 0,
            HeroOperation::CreditExperience { amount } => {
                self.progress.exp = self.progress.exp.saturating_add(*amount)
            }
            HeroOperation::RecordLevelGain {
                from,
                hp,
                energy,
                experience,
            } => {
                if *from == 0
                    || *from >= 30
                    || *hp <= 0
                    || *energy < 0
                    || *from > self.progress.level_max
                    || *experience > self.progress.exp
                {
                    return Err(owners::Error::Invariant);
                }
                let idx = *from as usize;
                if self.progress.hp_increments.len() <= idx {
                    self.progress.hp_increments.resize(idx + 1, 0);
                }
                if self.progress.en_increments.len() <= idx {
                    self.progress.en_increments.resize(idx + 1, 0);
                }
                self.progress.hp_increments[idx] = *hp;
                self.progress.en_increments[idx] = *energy;
                self.player_max_pw = self.player_max_pw.saturating_add(*energy as u32);
                self.player_pw = self.player_pw.saturating_add(*energy as u32);
                self.progress.exp = *experience;
                self.progress.level_max = self.progress.level_max.max(from + 1);
            }
        }
        Ok(OperationResult::Applied)
    }
}
impl CommandDomain for ClockState {
    fn command_facts(
        &self,
        q: &CommandQuery,
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<CommandFacts> {
        if let CommandQuery::Gameplay(q) = q {
            use super::super::wand_gameplay::FactQuery;
            if !c.wand_gameplay {
                return Err(owners::Error::Unsupported);
            }
            return Ok(CommandFacts::Gameplay(match q {
                FactQuery::Clock => {
                    super::super::wand_gameplay::FactValue::Turn(self.scheduler.turn)
                }
                _ => return Err(owners::Error::Schema),
            }));
        }
        let CommandQuery::TimerAdmission { target, kind, due } = q else {
            return Err(owners::Error::Schema);
        };
        let target_exists = match fact(obs, 0)? {
            WandFacts::Objects(o) if o.target == *target => o.target == *target,
            _ => return Err(owners::Error::Schema),
        };
        let target = timer_target(*target);
        let mut queue = self.object_timers.clone();
        Ok(CommandFacts::TimerAdmission {
            result: queue
                .start_at_preserving(target, *kind, *due, target_exists)
                .map_err(TimerFailure::from),
        })
    }

    fn apply(
        &mut self,
        command: &WandOwnerCommand,
        obs: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<OperationResult> {
        let WandOwnerCommand::Clock(op) = command else {
            return Err(owners::Error::Schema);
        };
        if matches!(op, ClockOperation::HeroAct) {
            self.scheduler
                .hero_act(nethacked_core::energy::NORMAL_SPEED);
            return Ok(OperationResult::Applied);
        }
        let target = match op {
            ClockOperation::StartCorpseTimer { target, .. }
            | ClockOperation::CancelCorpseTimers { target } => *target,
            _ => return Err(owners::Error::Schema),
        };
        let target_exists = match fact(obs, 0)? {
            WandFacts::Command(CommandFacts::Object { target: actual, .. }) if actual == target => {
                actual == target
            }
            _ => return Err(owners::Error::Schema),
        };
        let timer_target = timer_target(target);
        match op {
            ClockOperation::StartCorpseTimer { kind, due, .. } => {
                // Domain-private candidate queue. Any helper refusal discards it;
                // only a successful sealed publication exposes a serial/timer.
                let serial = self
                    .object_timers
                    .start_at_preserving(timer_target, *kind, *due, target_exists)
                    .map_err(|_| owners::Error::Invariant)?;
                Ok(OperationResult::Timer { serial })
            }
            ClockOperation::CancelCorpseTimers { .. } => {
                self.object_timers.cancel(timer_target);
                Ok(OperationResult::Applied)
            }
            _ => Err(owners::Error::Schema),
        }
    }
}
impl CommandDomain for RandomState {
    fn command_facts(
        &self,
        q: &CommandQuery,
        _: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<CommandFacts> {
        if !c.wand_charge_attempts || !matches!(q, CommandQuery::WrestPreview) {
            return Err(owners::Error::Unsupported);
        }
        let reservation = RandomReservation {
            stream: self.rng.get_stream(),
            word_position: self.rng.get_word_pos(),
            max_draws: 1,
            algorithm_version: 1,
        };
        let mut local = self.clone();
        let OperationResult::Sample(draw) = super::super::wand_gameplay::facts::sample(
            &mut local,
            &reservation,
            &super::super::wand_gameplay::SemanticDraw::Below(121),
            c,
        )?
        else {
            return Err(owners::Error::Invariant);
        };
        Ok(CommandFacts::WrestPreview { reservation, draw })
    }
    fn apply(
        &mut self,
        command: &WandOwnerCommand,
        _: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<OperationResult> {
        if let WandOwnerCommand::Sample { reservation, draw } = command {
            return super::super::wand_gameplay::facts::sample(self, reservation, draw, c);
        }
        let WandOwnerCommand::Random { reservation, word } = command else {
            return Err(owners::Error::Schema);
        };
        let words = match word {
            RawWord::U32 => 1,
            RawWord::U64 => 2,
        };
        if reservation.algorithm_version != 1
            || reservation.max_draws != words
            || reservation.stream != self.rng.get_stream()
            || reservation.word_position != self.rng.get_word_pos()
        {
            return Err(owners::Error::Stale);
        }
        Ok(match word {
            RawWord::U32 => OperationResult::Raw32(self.rng.next_u32()),
            RawWord::U64 => OperationResult::Raw64(self.rng.next_u64()),
        })
    }
}
impl CommandDomain for SessionState {
    fn command_facts(
        &self,
        q: &CommandQuery,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<CommandFacts> {
        if !matches!(q, CommandQuery::Session) {
            return Err(owners::Error::Schema);
        }
        valid_text(&self.role_name)?;
        Ok(CommandFacts::Session {
            role_id: self.role_id,
            role_name: self.role_name.clone(),
        })
    }
}
impl CommandDomain for EquipmentState {}
impl CommandDomain for BillingState {
    fn command_facts(
        &self,
        q: &CommandQuery,
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<CommandFacts> {
        let CommandQuery::LifetimeBilling { target } = q else {
            return Err(owners::Error::Unsupported);
        };
        super::super::item_lifetime::billing_facts(self, *target, obs, c)
            .map(CommandFacts::LifetimeBilling)
    }
}

fn timer_target(target: ObjectRef) -> crate::ObjectTimerTarget {
    match target.residency {
        Residency::Active => crate::ObjectTimerTarget::Active(target.id),
        Residency::Cached { branch, depth } => crate::ObjectTimerTarget::Cached {
            branch,
            depth,
            item: target.id,
        },
    }
}
