use super::super::runtime::OwnerBridgeConfig;
use super::*;
use crate::world::{bill_ledger, validate_bill_object, validate_bill_terms};

fn hero(obs: &[Observation], index: usize) -> owners::Result<HeroFacts> {
    match read_fact(obs.get(index).ok_or(owners::Error::Schema)?)? {
        WandFacts::Hero(h) => Ok(h),
        _ => Err(owners::Error::Schema),
    }
}
fn cached_index(g: &GeographyState, r: Residency) -> owners::Result<Option<usize>> {
    match r {
        Residency::Active => Ok(None),
        Residency::Cached { branch, depth } => g
            .stored_keys
            .iter()
            .position(|key| *key == (branch, depth))
            .map(Some)
            .ok_or(owners::Error::Stale),
    }
}
impl Domain for GeographyState {
    fn facts(
        &self,
        q: &WandQuery,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<WandFacts> {
        let WandQuery::Geography(q) = q else {
            return Err(owners::Error::Schema);
        };
        let (level, branch, depth, stored_index) = match cached_index(self, q.residency)? {
            None => (&self.level, self.current_branch, self.depth, None),
            Some(i) => (
                &self.stored_levels[i],
                self.stored_keys[i].0,
                self.stored_keys[i].1,
                Some(i),
            ),
        };
        if level.tiles.len() != nethacked_types::COLNO
            || level
                .tiles
                .iter()
                .any(|column| column.len() != nethacked_types::ROWNO)
        {
            return Err(owners::Error::Schema);
        }
        let trajectory = q
            .trajectory
            .as_ref()
            .map(|t| nethacked_dungeon::trace_beam_path(level, t.origin, t.direction, t.energy))
            .unwrap_or_default();
        let mut cells = Vec::with_capacity(q.cells.len() + trajectory.len());
        for c in q.cells.iter().chain(&trajectory) {
            coord(*c)?;
            // Never index malformed native geometry, even if import admitted it.
            let tile = level
                .tiles
                .get(c.x)
                .and_then(|column| column.get(c.y))
                .ok_or(owners::Error::Schema)?;
            cells.push(CellFacts {
                coord: *c,
                tile: tile.clone(),
            });
        }
        Ok(WandFacts::Geography(GeographyFacts {
            residency: q.residency,
            branch,
            depth,
            cells,
            trajectory,
            no_teleport: level.no_teleport,
            stored_index,
        }))
    }
}
impl Domain for HeroState {
    fn facts(
        &self,
        q: &WandQuery,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<WandFacts> {
        if !matches!(q, WandQuery::Hero) {
            return Err(owners::Error::Schema);
        }
        Ok(WandFacts::Hero(HeroFacts {
            player: self.player_id,
            polymorph: self.hero.polymorph.clone(),
            base_hp: self.hero.base_hp,
            base_max_hp: self.hero.base_max_hp,
            luck: self.player_luck,
            quivered: self.hero.quivered_item,
        }))
    }
}
impl Domain for SessionState {
    fn facts(
        &self,
        q: &WandQuery,
        _: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<WandFacts> {
        if !matches!(q, WandQuery::Session) || self.ruleset_ref != c.ruleset_ref {
            return Err(owners::Error::Schema);
        }
        text(&self.ruleset_ref.id)?;
        text(&self.ruleset_ref.version)?;
        text(&self.ruleset_ref.hash)?;
        Ok(WandFacts::Session(SessionFacts {
            locale: self.locale,
            ruleset: self.ruleset_ref.clone(),
        }))
    }
}
impl Domain for ObjectsState {
    fn facts(
        &self,
        q: &WandQuery,
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<WandFacts> {
        let WandQuery::Objects(q) = q else {
            return Err(owners::Error::Schema);
        };
        let h = hero(obs, 0)?;
        match read_fact(obs.get(1).ok_or(owners::Error::Schema)?)? {
            WandFacts::Session(s) if s.ruleset == c.ruleset_ref => {}
            _ => return Err(owners::Error::Schema),
        }
        let g = geography(obs, 2)?;
        let target = match q {
            ObjectRequest::Identity(r) => *r,
            ObjectRequest::Selection(r) => {
                if g.residency != Residency::Active {
                    return Err(owners::Error::Schema);
                }
                // Same arena order as the legacy selector, without allocating an inventory.
                let id = self
                    .items
                    .iter()
                    .filter(|(_, item)| item.location == ItemLocation::CarriedBy(h.player))
                    .nth(usize::try_from(r.index).map_err(|_| owners::Error::Limit)?)
                    .map(|(id, _)| id)
                    .ok_or(owners::Error::Stale)?;
                if r.expected.is_some_and(|expected| expected != id) {
                    return Err(owners::Error::Stale);
                }
                ObjectRef {
                    id,
                    residency: Residency::Active,
                }
            }
        };
        if target.residency != g.residency {
            return Err(owners::Error::Schema);
        }
        let record = match g.stored_index {
            None => self.items.get(target.id),
            Some(i) => self.stored_items.get(i).and_then(|items| {
                items
                    .iter()
                    .find(|(id, _)| *id == target.id)
                    .map(|(_, item)| item)
            }),
        }
        .ok_or(owners::Error::Stale)?;
        if matches!(q, ObjectRequest::Selection(_)) && record.class != ItemClass::Wand {
            return Err(owners::Error::Invariant);
        }
        let definition = match &record.definition {
            Some(id) => c.ruleset.item_for_definition(id),
            None => c.ruleset.item(&record.name),
        }
        .ok_or(owners::Error::Schema)?;
        if definition.class != record.class || record.quantity == 0 {
            return Err(owners::Error::Invariant);
        }
        text(&definition.name)?;
        if let Some(ItemDefinition::Vanilla(s) | ItemDefinition::Custom(s)) = &record.definition {
            text(s)?;
        }
        if let ItemLocation::Floor(c) = &record.location {
            coord(*c)?;
        }
        if let Some(artifact) = &record.artifact_id {
            text(artifact)?;
        }
        // The identity itself also obeys the small response budget.
        let receipt_eligible = if target.residency == Residency::Active {
            validate_bill_object(
                target.id,
                &self.items,
                h.player,
                &Default::default(),
                &c.ruleset,
            )
            .map(|_| ())
        } else {
            Err(BillError::UnsupportedBillingScope)
        };
        Ok(WandFacts::Objects(ObjectFacts {
            target,
            definition: record.definition.clone(),
            resolved_kind: definition.id,
            effect_name: definition.name.clone(),
            class: record.class,
            quantity: record.quantity,
            charges: record.enchantment,
            buc: record.buc,
            placement: record.location.clone(),
            artifact: record.artifact_id.clone(),
            receipt_eligible,
            cached_alias: self
                .stored_items
                .iter()
                .flatten()
                .any(|(id, _)| *id == target.id),
        }))
    }
}
impl Domain for EquipmentState {
    fn facts(
        &self,
        q: &WandQuery,
        obs: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<WandFacts> {
        let WandQuery::Equipment(target) = q else {
            return Err(owners::Error::Schema);
        };
        object(obs, *target)?;
        let h = hero(obs, 1)?;
        let active = target.residency == Residency::Active;
        let armor = self
            .equipment
            .armor
            .iter()
            .filter(|(_, id)| active && *id == target.id)
            .map(|(slot, _)| *slot)
            .collect::<Vec<_>>();
        bound(armor.len(), MAX_REFERENCES)?;
        let dressing = self
            .equipment
            .dressing
            .as_ref()
            .filter(|d| active && d.item == target.id)
            .cloned();
        Ok(WandFacts::Equipment(EquipmentFacts {
            target: *target,
            wielded: active && self.wielded_item == Some(target.id),
            armor,
            dressing,
            quivered: active && h.quivered == Some(target.id),
        }))
    }
}
impl Domain for ClockState {
    fn facts(
        &self,
        q: &WandQuery,
        obs: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<WandFacts> {
        let WandQuery::Clock(target) = q else {
            return Err(owners::Error::Schema);
        };
        object(obs, *target)?;
        let timer_target = match target.residency {
            Residency::Active => crate::ObjectTimerTarget::Active(target.id),
            Residency::Cached { branch, depth } => crate::ObjectTimerTarget::Cached {
                branch,
                depth,
                item: target.id,
            },
        };
        let mut registrations = Vec::new();
        for (queue_order, timer) in self.object_timers.entries().iter().enumerate() {
            if timer.target == timer_target {
                bound(registrations.len() + 1, MAX_REFERENCES)?;
                registrations.push(TimerRegistration {
                    timer: *timer,
                    queue_order,
                });
            }
        }
        Ok(WandFacts::Clock(ClockFacts {
            target: *target,
            turn: self.scheduler.turn,
            hero_energy: self.scheduler.hero_energy,
            hero_speed: self.scheduler.hero_speed,
            registrations,
        }))
    }
}
impl Domain for RandomState {
    fn facts(
        &self,
        q: &WandQuery,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<WandFacts> {
        let WandQuery::Random { draws } = q else {
            return Err(owners::Error::Schema);
        };
        Ok(WandFacts::Random(RandomReservation {
            stream: self.rng.get_stream(),
            word_position: self.rng.get_word_pos(),
            max_draws: *draws,
            algorithm_version: 1,
        }))
    }
}
fn actor(id: ActorId, record: &nethacked_arena::ActorRecord) -> owners::Result<ActorFacts> {
    text(&record.name)?;
    coord(record.coord)?;
    if let Some(
        nethacked_arena::ActorSpecies::Vanilla(s) | nethacked_arena::ActorSpecies::Custom(s),
    ) = &record.species
    {
        text(s)?;
    }
    Ok(ActorFacts {
        id,
        coord: record.coord,
        name: record.name.clone(),
        species: record.species.clone(),
        hp: record.hp,
        max_hp: record.max_hp,
        ac: record.ac,
        level: record.level,
        speed: record.speed,
        intrinsics: record.intrinsics,
        is_player: record.is_player,
        is_unique: record.is_unique,
        is_dead: record.is_dead,
    })
}
impl Domain for ActorsState {
    fn facts(
        &self,
        q: &WandQuery,
        obs: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<WandFacts> {
        let WandQuery::Actors(q) = q else {
            return Err(owners::Error::Schema);
        };
        let g = geography(obs, 0)?;
        if g.residency != q.residency {
            return Err(owners::Error::Schema);
        }
        let get = |id| {
            match g.stored_index {
                None => self.actors.get(id),
                Some(i) => self.stored_monsters.get(i).and_then(|actors| {
                    actors
                        .iter()
                        .find(|(key, _)| *key == id)
                        .map(|(_, record)| record)
                }),
            }
            .ok_or(owners::Error::Stale)
            .and_then(|r| actor(id, r))
        };
        let origin = get(q.origin)?;
        let targets = q
            .targets
            .iter()
            .map(|id| get(*id))
            .collect::<owners::Result<Vec<_>>>()?;
        Ok(WandFacts::Actors(ActorsFacts {
            residency: q.residency,
            origin,
            targets,
        }))
    }
}
impl Domain for BillingState {
    fn facts(
        &self,
        q: &WandQuery,
        obs: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<WandFacts> {
        if let WandQuery::ConsumedBilling { origin } = q {
            let (receipt_order, bill) = self
                .billing
                .consumed
                .iter()
                .enumerate()
                .find(|(_, b)| b.origin == *origin)
                .ok_or(owners::Error::Stale)?;
            return Ok(WandFacts::ConsumedBilling(ConsumedFacts {
                origin: *origin,
                receipt_order,
                quantity: bill.object.quantity,
                terms: bill.terms.clone(),
            }));
        }
        let WandQuery::Billing { target, receipt } = q else {
            return Err(owners::Error::Schema);
        };
        let o = object(obs, *target)?;
        if self.billing.consumed.iter().any(|b| b.origin == target.id) {
            return Err(owners::Error::Invariant);
        }
        let (base, terms) = if target.residency == Residency::Active {
            bill_ledger(
                target.id,
                &self.unpaid_items,
                &self.billing,
                self.stored_unpaid_items.iter().flatten(),
                o.cached_alias,
            )
            .map_err(|_| owners::Error::Invariant)?
        } else {
            // Cached scopes are disambiguated by the Objects dependency's geography provenance.
            let WandFacts::Geography(g) = read_fact(obs.get(1).ok_or(owners::Error::Schema)?)?
            else {
                return Err(owners::Error::Schema);
            };
            let i = g.stored_index.ok_or(owners::Error::Schema)?;
            let ledger = self
                .stored_unpaid_items
                .get(i)
                .ok_or(owners::Error::Schema)?;
            let mut matches = ledger.iter().filter(|(id, _)| *id == target.id);
            let base = matches.next().map(|(_, base)| *base);
            if matches.next().is_some() {
                return Err(owners::Error::Invariant);
            }
            (base, None)
        };
        let liability = match (base, terms) {
            (None, None) => Liability::Unbilled,
            (Some(legacy_base_cost), None) => Liability::Unknown {
                legacy_base_cost,
                quantity: o.quantity,
            },
            (Some(_), Some(terms)) => Liability::Known {
                terms: terms.clone(),
                quantity: o.quantity,
            },
            _ => return Err(owners::Error::Invariant),
        };
        let receipt_admission = receipt.as_ref().map(|r| {
            o.receipt_eligible.clone()?;
            let base = base.ok_or(BillError::InvalidReceipt)?;
            if terms.is_some() {
                return Err(BillError::ReceiptAlreadyRecorded);
            }
            validate_bill_terms(r, o.quantity, base)
        });
        Ok(WandFacts::Billing(BillingFacts {
            target: *target,
            liability,
            receipt_admission,
        }))
    }
}
