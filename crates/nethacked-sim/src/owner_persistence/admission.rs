//! Pure native profile admission. No owner factory, timer dispatch or World command.
use super::partitions::NativePartitions;
use crate::world::{validate_billing_parts, BillingLevel};
use crate::{ObjectTimerTarget, Ruleset};
use nethacked_arena::{ActorId, ActorRecord, ActorSpecies, ItemId, ItemLocation, ItemRecord};
use nethacked_owners::{Error, Result};
use std::collections::{BTreeMap, BTreeSet};

/// Scope zero is active; cached scopes follow the single Geography routing order.
/// Native keys remain reversible through export; kernel owner identities are
/// independently remapped by D4. Historical receipt/creditor keys are not live edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum NativeTarget {
    Item(usize, ItemId),
    Actor(usize, ActorId),
}
#[derive(Debug)]
pub(super) struct NativeReferences {
    nodes: BTreeSet<NativeTarget>,
    edges: Vec<NativeTarget>,
    unresolved_bills: Vec<(usize, ItemId)>,
}
impl NativeReferences {
    fn require(&mut self, target: NativeTarget) -> Result<()> {
        if !self.nodes.contains(&target) {
            return Err(Error::Stale);
        }
        self.edges.push(target);
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(super) enum AdmissionPolicy {
    Strict,
    WandGameplayV3,
}

fn item_definition(item: &ItemRecord, ruleset: &Ruleset, policy: AdmissionPolicy) -> Result<()> {
    // Missing legacy identity is not a claimed code binding. Unknown names
    // and unusable quantities preserve their existing command refusals.
    let Some(id) = &item.definition else {
        return Ok(());
    };
    let Some(def) = ruleset.item_for_definition(id) else {
        return match policy {
            AdmissionPolicy::Strict => Err(Error::Schema),
            AdmissionPolicy::WandGameplayV3 => Ok(()),
        };
    };
    if def.class != item.class {
        return Err(Error::Invariant);
    }
    Ok(())
}
fn actor_definition(actor: &ActorRecord, ruleset: &Ruleset) -> Result<()> {
    if actor.is_player {
        return Ok(());
    }
    let def = match &actor.species {
        Some(ActorSpecies::Vanilla(symbol)) => {
            nethacked_data::MonsterSpeciesId::from_c_name(symbol)
                .and_then(|id| ruleset.monster_by_id(id))
        }
        Some(ActorSpecies::Custom(key)) => ruleset.monster(key).filter(|def| def.id.is_none()),
        None => return Ok(()),
    };
    def.ok_or(Error::Schema).map(|_| ())
}

impl NativePartitions {
    pub(super) fn admit_references(&self, ruleset: &Ruleset) -> Result<NativeReferences> {
        self.admit_references_with_policy(ruleset, AdmissionPolicy::Strict)
    }

    pub(super) fn admit_references_with_policy(
        &self,
        ruleset: &Ruleset,
        policy: AdmissionPolicy,
    ) -> Result<NativeReferences> {
        let count = self.geography.stored_keys.len();
        if [
            self.geography.stored_levels.len(),
            self.objects.stored_items.len(),
            self.actors.stored_monsters.len(),
            self.billing.stored_unpaid_items.len(),
        ]
        .into_iter()
        .any(|n| n != count)
        {
            return Err(Error::Invariant);
        }
        // Equal native handles on different cached levels are distinct nodes.
        for (index, key) in self.geography.stored_keys.iter().enumerate() {
            if self.geography.stored_keys[..index].contains(key) {
                return Err(Error::Invariant);
            }
        }
        let mut graph = NativeReferences {
            nodes: BTreeSet::new(),
            edges: Vec::new(),
            unresolved_bills: Vec::new(),
        };
        let mut items = BTreeMap::new();
        for (id, item) in &self.objects.items {
            graph.nodes.insert(NativeTarget::Item(0, id));
            items.insert((0, id), item);
        }
        for (index, rows) in self.objects.stored_items.iter().enumerate() {
            for (id, item) in rows {
                if !graph.nodes.insert(NativeTarget::Item(index + 1, *id)) {
                    return Err(Error::Invariant);
                }
                items.insert((index + 1, *id), item);
            }
        }
        for (id, actor) in &self.actors.actors {
            graph.nodes.insert(NativeTarget::Actor(0, id));
            actor_definition(actor, ruleset)?;
        }
        for (index, rows) in self.actors.stored_monsters.iter().enumerate() {
            for (id, actor) in rows {
                if !graph.nodes.insert(NativeTarget::Actor(index + 1, *id)) {
                    return Err(Error::Invariant);
                }
                actor_definition(actor, ruleset)?;
            }
        }
        graph.require(NativeTarget::Actor(0, self.hero.player_id))?;
        if !self.actors.actors[self.hero.player_id].is_player {
            return Err(Error::Invariant);
        }
        if let Some(mount) = &self.hero.hero.mount {
            graph.require(NativeTarget::Actor(0, mount.steed_id))?;
        }
        let mut parents = BTreeMap::new();
        for (&(scope, id), item) in &items {
            item_definition(item, ruleset, policy)?;
            match item.location {
                ItemLocation::CarriedBy(actor) => {
                    let actor_scope = if actor == self.hero.player_id
                        && !graph.nodes.contains(&NativeTarget::Actor(scope, actor))
                    {
                        0
                    } else {
                        scope
                    };
                    graph.require(NativeTarget::Actor(actor_scope, actor))?
                }
                ItemLocation::InContainer(parent) => {
                    graph.require(NativeTarget::Item(scope, parent))?;
                    parents.insert((scope, id), (scope, parent));
                }
                ItemLocation::Floor(_) | ItemLocation::Limbo => (),
            }
        }
        // Each parent chain is traversed once, including deeply nested inputs.
        let mut finished = BTreeSet::new();
        for &start in parents.keys() {
            if finished.contains(&start) {
                continue;
            }
            let mut path = BTreeSet::new();
            let mut node = start;
            while !finished.contains(&node) {
                if !path.insert(node) {
                    return Err(Error::Invariant);
                }
                let Some(&parent) = parents.get(&node) else {
                    break;
                };
                node = parent;
            }
            finished.extend(path);
        }
        let carried = |id| -> Result<&ItemRecord> {
            let item = self.objects.items.get(id).ok_or(Error::Stale)?;
            if item.location != ItemLocation::CarriedBy(self.hero.player_id) {
                return Err(Error::Invariant);
            }
            Ok(item)
        };
        if let Some(id) = self.hero.hero.quivered_item {
            carried(id)?;
            graph.require(NativeTarget::Item(0, id))?;
        }
        if let Some(id) = self.equipment.wielded_item {
            carried(id)?;
            graph.require(NativeTarget::Item(0, id))?;
        }
        let mut armor_items = BTreeSet::new();
        for (index, (slot, id)) in self.equipment.equipment.armor.iter().enumerate() {
            if !armor_items.insert(*id)
                || self.equipment.equipment.armor[..index]
                    .iter()
                    .any(|(old, _)| old == slot)
            {
                return Err(Error::Invariant);
            }
            let item = carried(*id)?;
            if item.class != nethacked_types::ItemClass::Armor {
                return Err(Error::Invariant);
            }
            graph.require(NativeTarget::Item(0, *id))?;
        }
        if let Some(dressing) = &self.equipment.equipment.dressing {
            if carried(dressing.item)?.class != nethacked_types::ItemClass::Armor {
                return Err(Error::Invariant);
            }
            graph.require(NativeTarget::Item(0, dressing.item))?;
        }
        self.clock
            .object_timers
            .validate_snapshot()
            .map_err(|_| Error::Invariant)?;
        for timer in self.clock.object_timers.entries() {
            let (scope, id) = match timer.target {
                ObjectTimerTarget::Active(id) => (0, id),
                ObjectTimerTarget::Cached {
                    branch,
                    depth,
                    item,
                } => {
                    let index = self
                        .geography
                        .stored_keys
                        .iter()
                        .position(|key| *key == (branch, depth))
                        .ok_or(Error::Stale)?;
                    (index + 1, item)
                }
            };
            graph.require(NativeTarget::Item(scope, id))?;
        }
        let levels: Vec<_> = self
            .objects
            .stored_items
            .iter()
            .zip(&self.billing.stored_unpaid_items)
            .map(|(items, unpaid_items)| BillingLevel {
                items,
                unpaid_items,
            })
            .collect();
        validate_billing_parts(
            ruleset,
            &self.objects.items,
            &self.billing.unpaid_items,
            &self.billing.billing,
            &levels,
        )
        .map_err(|_| Error::Invariant)?;
        for (scope, ledger) in std::iter::once(&self.billing.unpaid_items)
            .chain(&self.billing.stored_unpaid_items)
            .enumerate()
        {
            for (id, _) in ledger {
                // Unresolved legacy debt stays visible when its object is gone.
                // Exact live receipt membership is checked by the shared validator.
                if graph.nodes.contains(&NativeTarget::Item(scope, *id)) {
                    graph.require(NativeTarget::Item(scope, *id))?;
                } else {
                    graph.unresolved_bills.push((scope, *id));
                }
            }
        }
        // Recorded live receipts are already checked for unique membership by
        // the shared validator; consumed origins and creditors stay historical.
        for (id, _) in &self.billing.billing.live_terms {
            let scope = items
                .keys()
                .find(|(_, key)| key == id)
                .ok_or(Error::Stale)?
                .0;
            graph.require(NativeTarget::Item(scope, *id))?;
        }
        Ok(graph)
    }
}
