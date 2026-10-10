//! Persisted object timeouts on the scheduler's world clock.
//!
//! NetHack-5.0.0_Released SHA 16ff59115315917b93185d026aeefea06db9b0f4:
//! timeout.c:2222-2291,2467-2482,2660-2725; do.c:1816-1823.
//! Generated corpse recipes install into this owner without additional draws.
//! Selected corpse transfers and absence-only legacy scheduling share this
//! owner. Ordinary rot deletes the body; ordinary Troll revival shares it.

use crate::{GameEvent, SimulationWorld};
use nethacked_arena::{ItemId, ItemLocation};
use nethacked_types::BranchId;
use serde::{Deserialize, Serialize};

/// Callback identity, independent of species and display name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectTimerKind {
    RotCorpse,
    ReviveMonster,
}

/// Cache scope disambiguates historical handles shared by saved levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectTimerTarget {
    Active(ItemId),
    Cached {
        branch: BranchId,
        depth: usize,
        item: ItemId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectTimer {
    pub target: ObjectTimerTarget,
    pub kind: ObjectTimerKind,
    /// Preserve the original deadline during overdue level catch-up.
    pub due: u64,
    /// Increasing identity, retained across restoration. Queue order, rather
    /// than this identity, resolves equal deadlines after C-style reinsertion.
    pub serial: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ObjectTimerQueue {
    entries: Vec<ObjectTimer>,
    next_serial: u64,
}

impl ObjectTimerQueue {
    pub(crate) fn validate_snapshot(&self) -> Result<(), String> {
        let timers = &self.entries;
        for (index, timer) in timers.iter().enumerate() {
            if timer.serial >= self.next_serial {
                return Err("object timer serial exceeds persisted insertion counter".into());
            }
            if timers[..index].iter().any(|previous| {
                previous.serial == timer.serial
                    || (previous.target == timer.target && previous.kind == timer.kind)
            }) {
                return Err("duplicate persisted object timer".into());
            }
            if index > 0 {
                let previous = timers[index - 1];
                if previous.due > timer.due {
                    return Err("persisted object timers are out of due order".into());
                }
            }
        }
        Ok(())
    }

    /// Remove the first active due entry, preserving cached entries and tie order.
    pub(crate) fn pop_due_active(&mut self, turn: u64) -> Option<ObjectTimer> {
        let index = self.entries.iter().position(|timer| {
            timer.due <= turn && matches!(timer.target, ObjectTimerTarget::Active(_))
        })?;
        Some(self.entries.remove(index))
    }

    pub fn entries(&self) -> &[ObjectTimer] {
        &self.entries
    }

    /// Borrowed clock operation; callers supply only object residency lookup.
    pub(crate) fn reconcile(&mut self, mut exists: impl FnMut(ObjectTimerTarget) -> bool) {
        self.entries.retain(|timer| exists(timer.target));
    }

    pub(crate) fn start_at(
        &mut self,
        target: ObjectTimerTarget,
        kind: ObjectTimerKind,
        due: Option<u64>,
        mut exists: impl FnMut(ObjectTimerTarget) -> bool,
    ) -> Result<u64, ObjectTimerError> {
        self.reconcile(&mut exists);
        self.start_at_preserving(target, kind, due, exists(target))
    }

    /// Insert with an authenticated observation of this target only. Other
    /// registrations remain untouched when their residency was not observed.
    pub(crate) fn start_at_preserving(
        &mut self,
        target: ObjectTimerTarget,
        kind: ObjectTimerKind,
        due: Option<u64>,
        target_exists: bool,
    ) -> Result<u64, ObjectTimerError> {
        if !target_exists {
            return Err(ObjectTimerError::MissingObject);
        }
        if self
            .entries
            .iter()
            .any(|t| t.target == target && t.kind == kind)
        {
            return Err(ObjectTimerError::Duplicate);
        }
        let due = due.ok_or(ObjectTimerError::ClockOverflow)?;
        let serial = self.next_serial;
        let next = serial
            .checked_add(1)
            .ok_or(ObjectTimerError::SerialOverflow)?;
        self.next_serial = next;
        self.insert(ObjectTimer {
            target,
            kind,
            due,
            serial,
        });
        Ok(serial)
    }

    pub(crate) fn cancel(&mut self, target: ObjectTimerTarget) -> usize {
        let before = self.entries.len();
        self.entries.retain(|timer| timer.target != target);
        before - self.entries.len()
    }
    fn insert(&mut self, timer: ObjectTimer) {
        // C insert_timer stops at >=: every insertion precedes existing ties,
        // including restored records whose serial is older than live timers.
        let index = self.entries.partition_point(|old| old.due < timer.due);
        self.entries.insert(index, timer);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectTimerError {
    MissingObject,
    Duplicate,
    ClockOverflow,
    SerialOverflow,
}

impl SimulationWorld {
    /// Compatibility migration only for saves lacking the entire timer owner.
    /// Iterate active arena, then cached levels/items in their persisted order;
    /// preserve body identity, elapsed age, traits, sex and norevive verbatim.
    pub(crate) fn migrate_absent_corpse_timers(&mut self) -> Result<(), ObjectTimerError> {
        use nethacked_data::corpses::{
            CorpseDefinition as Definition, CorpseTimeoutRecipe as Recipe,
        };
        use rand::Rng;
        let mut candidates: Vec<_> = self
            .arena
            .items
            .iter()
            .filter(|(_, item)| {
                item.is_corpse()
                    && !nethacked_arena::frozen_in_ice_box(item, |parent| {
                        self.arena.items.get(parent)
                    })
            })
            .map(|(id, item)| {
                (
                    ObjectTimerTarget::Active(id),
                    self.current_branch,
                    self.depth,
                    item.clone(),
                )
            })
            .collect();
        for ((branch, depth), stored) in &self.stored_levels {
            for (id, item) in &stored.items {
                if item.is_corpse()
                    && !nethacked_arena::frozen_in_ice_box(item, |parent| {
                        stored
                            .items
                            .iter()
                            .find(|(id, _)| *id == parent)
                            .map(|(_, item)| item)
                    })
                {
                    candidates.push((
                        ObjectTimerTarget::Cached {
                            branch: *branch,
                            depth: *depth,
                            item: *id,
                        },
                        *branch,
                        *depth,
                        item.clone(),
                    ));
                }
            }
        }
        let rs = std::sync::Arc::clone(&self.ruleset);
        for (target, branch, depth, item) in candidates {
            let (caller, _) = self.item_birth_context(branch, depth, false);
            let ctx = nethacked_core::corpse_timeout::CorpseTimeoutContext {
                elapsed_age: item.corpse_age,
                moves: caller.moves,
                hero_level: caller.hero_level,
                in_mklev: false,
            };
            let recipe = match rs.corpse_body_definition(&item) {
                Definition::Owned(def) => {
                    nethacked_data::corpses::sample_corpse_timeout(def, ctx, |n| {
                        self.rng.random_range(0..n)
                    })
                }
                Definition::OrdinaryUnspecified => Some(Recipe::Rot {
                    sampled_at: ctx.moves,
                    delay: nethacked_core::corpse_timeout::initial_rot_delay(ctx, |n| {
                        self.rng.random_range(0..n)
                    }),
                }),
                Definition::Unresolved => continue,
            };
            let (kind, sampled_at, delay) = match recipe {
                Some(Recipe::Rot { sampled_at, delay }) => {
                    (ObjectTimerKind::RotCorpse, sampled_at, delay)
                }
                Some(Recipe::Revive { sampled_at, delay }) => {
                    (ObjectTimerKind::ReviveMonster, sampled_at, delay)
                }
                // Rider replacement needs its own source family; never generic rot.
                Some(Recipe::UnsupportedRider) | None => continue,
            };
            self.start_object_timer_at(target, kind, sampled_at.checked_add_signed(delay))?;
        }
        Ok(())
    }

    /// C pickup.c:2644–2663,2758–2797 acts on the selected object only.
    /// Moving an enclosing container never changes descendant corpse timers.
    pub(crate) fn corpse_freeze_snapshot(&self, root: ItemId) -> Vec<(ItemId, bool)> {
        let Some(item) = self.arena.items.get(root).filter(|item| item.is_corpse()) else {
            return Vec::new();
        };
        vec![(
            root,
            nethacked_arena::frozen_in_ice_box(item, |parent| self.arena.items.get(parent)),
        )]
    }

    /// Apply only the selected corpse's successful direct-parent transition. Elapsed
    /// age stays on the record; signed recipes use the actual gameplay clock.
    pub(crate) fn apply_corpse_freeze_transitions(&mut self, before: Vec<(ItemId, bool)>) {
        use nethacked_data::corpses::{
            CorpseDefinition as Definition, CorpseTimeoutRecipe as Recipe,
        };
        use nethacked_data::monsters::CorpseThawPolicy;
        let rs = std::sync::Arc::clone(&self.ruleset);
        for (id, was_frozen) in before {
            let Some(item) = self.arena.items.get(id).cloned() else {
                continue;
            };
            let frozen =
                nethacked_arena::frozen_in_ice_box(&item, |parent| self.arena.items.get(parent));
            if frozen == was_frozen {
                continue;
            }
            let target = ObjectTimerTarget::Active(id);
            if frozen {
                self.cancel_object_timers(target);
                // C pickup.c:2648–2663 tests BODY Ice Troll, not saved species.
                if matches!(rs.corpse_body_definition(&item), Definition::Owned(def)
                    if def.corpse_thaw_policy == CorpseThawPolicy::IceTroll)
                {
                    if let Some(traits) = self.arena.items[id].saved_monster_traits.as_mut() {
                        traits.actor.is_cancelled = false;
                    }
                }
                continue;
            }
            let no_revive = match rs.corpse_revival_definition(&item) {
                Definition::Owned(def) => def.corpse_thaw_policy == CorpseThawPolicy::Ordinary,
                Definition::OrdinaryUnspecified => true,
                // Explicitly unsupported original keys: no body/name guess.
                Definition::Unresolved => continue,
            };
            let body = rs.corpse_body_definition(&item);
            if matches!(body, Definition::Unresolved) {
                continue;
            }
            self.arena.items[id].corpse_no_revive = no_revive;
            self.cancel_object_timers(target);
            let (ctx, _) = self.item_birth_context(self.current_branch, self.depth, false);
            let ctx = nethacked_core::corpse_timeout::CorpseTimeoutContext {
                elapsed_age: item.corpse_age,
                moves: ctx.moves,
                hero_level: ctx.hero_level,
                in_mklev: false,
            };
            use rand::Rng;
            let recipe = match body {
                Definition::Owned(def) => {
                    nethacked_data::corpses::sample_corpse_timeout(def, ctx, |n| {
                        self.rng.random_range(0..n)
                    })
                }
                Definition::OrdinaryUnspecified => Some(Recipe::Rot {
                    sampled_at: ctx.moves,
                    delay: nethacked_core::corpse_timeout::initial_rot_delay(ctx, |n| {
                        self.rng.random_range(0..n)
                    }),
                }),
                Definition::Unresolved => unreachable!("checked supplied body key above"),
            };
            let (kind, sampled_at, delay) = match recipe {
                Some(Recipe::Rot { sampled_at, delay }) => {
                    (ObjectTimerKind::RotCorpse, sampled_at, delay)
                }
                Some(Recipe::Revive { sampled_at, delay }) => {
                    (ObjectTimerKind::ReviveMonster, sampled_at, delay)
                }
                Some(Recipe::UnsupportedRider) | None => continue,
            };
            self.start_object_timer_at(target, kind, sampled_at.checked_add_signed(delay))
                .expect("thawed corpse deadline fits the world clock");
        }
    }

    /// C start_timer: one timer per object/callback pair, moves + delay.
    /// No RNG, age reset or implicit scheduling occurs here.
    pub fn start_object_timer(
        &mut self,
        target: ObjectTimerTarget,
        kind: ObjectTimerKind,
        delay: u64,
    ) -> Result<u64, ObjectTimerError> {
        let due = self.scheduler.turn.checked_add(delay);
        self.start_object_timer_at(target, kind, due)
    }

    /// C's signed `long when` permits zero and negative initial rot delays.
    /// Preserve valid past deadlines; reject unrepresentable absolute clocks.
    pub fn start_object_timer_signed(
        &mut self,
        target: ObjectTimerTarget,
        kind: ObjectTimerKind,
        delay: i64,
    ) -> Result<u64, ObjectTimerError> {
        let due = self.scheduler.turn.checked_add_signed(delay);
        self.start_object_timer_at(target, kind, due)
    }

    fn start_object_timer_at(
        &mut self,
        target: ObjectTimerTarget,
        kind: ObjectTimerKind,
        due: Option<u64>,
    ) -> Result<u64, ObjectTimerError> {
        let items = &self.arena.items;
        let cached = &self.stored_levels;
        self.object_timers
            .start_at(target, kind, due, |target| match target {
                ObjectTimerTarget::Active(id) => items.contains_key(id),
                ObjectTimerTarget::Cached {
                    branch,
                    depth,
                    item,
                } => cached
                    .iter()
                    .find(|(key, _)| *key == (branch, depth))
                    .is_some_and(|(_, stored)| stored.items.iter().any(|(id, _)| *id == item)),
            })
    }

    /// Materialize a sampled tree once, preserving generation order and clock.
    /// No RNG or age mutation occurs here. C mkbox_cnts starts then cancels ice
    /// corpse timers: cancelled entries still consume an insertion identity.
    pub fn attach_generated_item(
        &mut self,
        mut tree: nethacked_data::containers::GeneratedItem,
        location: ItemLocation,
    ) -> Result<ItemId, ObjectTimerError> {
        tree.item.location = location;
        let id = self.arena.spawn_item(tree.item);
        use nethacked_data::corpses::{CorpseTimeoutRecipe as Recipe, CorpseTimerOperation as Op};
        // Explicit compatibility for old handbuilt primitive recipes. A
        // nonempty stream is authoritative; its summary is never replayed.
        if tree.corpse_timer_operations.is_empty() {
            if let Some(recipe) = tree.corpse_timeout {
                tree.corpse_timer_operations.push(Op::Start(recipe));
            }
        }
        let target = ObjectTimerTarget::Active(id);
        for operation in tree.corpse_timer_operations {
            match operation {
                Op::Cancel => {
                    self.cancel_object_timers(target);
                }
                Op::Start(recipe) => {
                    let (sampled_at, delay, kind) = match recipe {
                        Recipe::Rot { sampled_at, delay } => {
                            (sampled_at, delay, ObjectTimerKind::RotCorpse)
                        }
                        Recipe::Revive { sampled_at, delay } => {
                            (sampled_at, delay, ObjectTimerKind::ReviveMonster)
                        }
                        Recipe::UnsupportedRider => continue,
                    };
                    self.start_object_timer_at(target, kind, sampled_at.checked_add_signed(delay))?;
                }
            }
        }
        if nethacked_arena::frozen_in_ice_box(&self.arena.items[id], |parent| {
            self.arena.items.get(parent)
        }) {
            self.cancel_object_timers(target);
        }
        for child in tree.contents {
            self.attach_generated_item(child, ItemLocation::InContainer(id))?;
        }
        Ok(id)
    }

    /// Cancel without changing the object; overdue timers have zero remaining.
    pub fn stop_object_timer(
        &mut self,
        target: ObjectTimerTarget,
        kind: ObjectTimerKind,
    ) -> Option<u64> {
        let index = self
            .object_timers
            .entries
            .iter()
            .position(|t| t.target == target && t.kind == kind)?;
        Some(
            self.object_timers
                .entries
                .remove(index)
                .due
                .saturating_sub(self.scheduler.turn),
        )
    }

    /// C obj_stop_timers: invalidate every callback on the object.
    pub fn cancel_object_timers(&mut self, target: ObjectTimerTarget) -> usize {
        self.object_timers.cancel(target)
    }

    /// C splitobj copies each surviving callback onto the extracted unit,
    /// then disposal cancels those copies. Only the insertion counter survives.
    pub(crate) fn consume_split_timer_serials(&mut self, target: ObjectTimerTarget) {
        let count = self
            .object_timers
            .entries
            .iter()
            .filter(|t| t.target == target)
            .count();
        self.object_timers.next_serial = self
            .object_timers
            .next_serial
            .checked_add(count as u64)
            .expect("split timer serial fits");
    }

    /// Direct arena removals are supported: stale generations never dispatch.
    pub(crate) fn reconcile_object_timers(&mut self) {
        let items = &self.arena.items;
        let cached = &self.stored_levels;
        self.object_timers.reconcile(|target| match target {
            ObjectTimerTarget::Active(id) => items.contains_key(id),
            ObjectTimerTarget::Cached {
                branch,
                depth,
                item,
            } => cached
                .iter()
                .find(|(key, _)| *key == (branch, depth))
                .is_some_and(|(_, stored)| stored.items.iter().any(|(id, _)| *id == item)),
        });
    }

    pub(crate) fn validate_object_timers(&self) -> Result<(), String> {
        self.object_timers.validate_snapshot()
    }

    pub(crate) fn cache_object_timers(&mut self, branch: BranchId, depth: usize, items: &[ItemId]) {
        for timer in &mut self.object_timers.entries {
            if let ObjectTimerTarget::Active(item) = timer.target {
                if items.contains(&item) {
                    timer.target = ObjectTimerTarget::Cached {
                        branch,
                        depth,
                        item,
                    };
                }
            }
        }
    }

    pub(crate) fn restore_object_timers(
        &mut self,
        branch: BranchId,
        depth: usize,
        lookup: impl Fn(ItemId) -> Option<ItemId>,
    ) {
        let saved = std::mem::take(&mut self.object_timers.entries);
        let mut restored = Vec::new();
        for mut timer in saved {
            if let ObjectTimerTarget::Cached {
                branch: b,
                depth: d,
                item,
            } = timer.target
            {
                if (b, d) == (branch, depth) {
                    let Some(new) = lookup(item) else {
                        continue;
                    };
                    timer.target = ObjectTimerTarget::Active(new);
                    restored.push(timer);
                    continue;
                }
            }
            self.object_timers.entries.push(timer);
        }
        // C writes in queue order and restores sequentially with insert-before
        // equals. Local restored ties therefore precede preexisting globals.
        for timer in restored {
            self.object_timers.insert(timer);
        }
    }

    /// C obj_is_local follows the outer container (timeout.c:2560-2580).
    /// Hero inventory/migrating (represented here by Limbo) is global; floor
    /// objects and objects carried by resident monsters are level-local.
    fn timer_object_is_global(&self, item: ItemId) -> bool {
        let mut current = item;
        let mut seen = Vec::new();
        loop {
            if seen.contains(&current) {
                return true;
            }
            seen.push(current);
            let Some(record) = self.arena.items.get(current) else {
                return true;
            };
            match record.location {
                ItemLocation::InContainer(parent) => current = parent,
                ItemLocation::CarriedBy(actor) => return actor == self.player_id,
                ItemLocation::Floor(_) => return false,
                ItemLocation::Limbo => return true,
            }
        }
    }

    /// Full C recovery restores globals, then reloads current-level locals
    /// (restore.c:811-898). Other levels are restored and written back while
    /// recovering, so their saved same-deadline order reverses once too.
    pub(crate) fn restore_saved_object_timers(&mut self) {
        let saved = std::mem::take(&mut self.object_timers.entries);
        let mut global = Vec::new();
        let mut local = Vec::new();
        for timer in saved {
            match timer.target {
                ObjectTimerTarget::Active(item) => {
                    if self.timer_object_is_global(item) {
                        global.push(timer);
                    } else {
                        local.push(timer);
                    }
                }
                ObjectTimerTarget::Cached { .. } => self.object_timers.insert(timer),
            }
        }
        for timer in global.into_iter().chain(local) {
            self.object_timers.insert(timer);
        }
    }

    /// Off-level timers retain deadlines until restoration. Consume before
    /// delivery so the eventual callback may reschedule itself.
    pub(crate) fn run_object_timers(&mut self) -> Vec<GameEvent> {
        self.reconcile_object_timers();
        let mut events = Vec::new();
        while let Some(timer) = self.object_timers.pop_due_active(self.scheduler.turn) {
            events.push(GameEvent::ObjectTimerExpired { timer });
            if timer.kind == ObjectTimerKind::ReviveMonster {
                let ObjectTimerTarget::Active(id) = timer.target else {
                    unreachable!()
                };
                self.revive_ordinary_troll(id);
            }
            if timer.kind == ObjectTimerKind::RotCorpse {
                let ObjectTimerTarget::Active(id) = timer.target else {
                    unreachable!()
                };
                // C dig.c:2127–2140 buries a corpse's contents before disposal.
                // That owner is missing: no recursive destruction substitute.
                if !self
                    .arena
                    .items
                    .get(id)
                    .is_some_and(|item| item.is_corpse())
                    || !self.arena.items_in_container(id).is_empty()
                {
                    continue;
                }
                // C dealloc_obj cancels all remaining object timers; the due
                // entry and expiry envelope already precede this real effect.
                self.cancel_object_timers(timer.target);
                self.arena.destroy_item(id);
                self.forget_rot_corpse_references(id);
            }
        }
        events
    }
}
