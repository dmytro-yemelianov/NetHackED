//! Exclusive borrowed compatibility publication, with all refusals before edits.
//! This adapter borrows participating fields; it never retains a World.
use crate::{
    billing::{
        bill_ledger, validate_bill_object, BillError, BillTerms, BillingState, WholeDisposal,
    },
    item_lifetime::{
        clear_equipment, clear_reference, finalize_survivor, LifetimePort, PreparedWholeDisposal,
    },
    ObjectTimerQueue, ObjectTimerTarget, SimulationWorld,
};
use nethacked_arena::{ActorId, ItemId, ItemRecord, ItemStorage};
use nethacked_data::ruleset::Ruleset;

type CachedBilling<'a> = (&'a [(ItemId, ItemRecord)], &'a [(ItemId, u32)]);
struct BorrowedLifetime<'a> {
    player: ActorId,
    ruleset: &'a Ruleset,
    items: &'a mut ItemStorage,
    equipment: &'a mut super::equipment::EquipmentState,
    wielded: &'a mut Option<ItemId>,
    quivered: &'a mut Option<ItemId>,
    timers: &'a mut ObjectTimerQueue,
    billing: &'a mut BillingState,
    unpaid: &'a mut Vec<(ItemId, u32)>,
    cached: Vec<CachedBilling<'a>>,
}
impl LifetimePort for BorrowedLifetime<'_> {
    type Target = ItemId;
    type Error = BillError;
    fn object(&mut self, target: ItemId) -> Result<Option<ItemRecord>, BillError> {
        Ok(self.items.get(target).cloned())
    }
    fn origin(&self, target: ItemId) -> ItemId {
        target
    }
    fn accounting(
        &mut self,
        target: ItemId,
        object: &ItemRecord,
    ) -> Result<(Option<u32>, Option<BillTerms>), BillError> {
        let current =
            validate_bill_object(target, self.items, self.player, self.billing, self.ruleset)?;
        if current != object {
            return Err(BillError::StaleItem);
        }
        let (base, receipt) = bill_ledger(
            target,
            self.unpaid,
            self.billing,
            self.cached.iter().flat_map(|(_, unpaid)| unpaid.iter()),
            self.cached
                .iter()
                .any(|(items, _)| items.iter().any(|(id, _)| *id == target)),
        )?;
        Ok((base, receipt.cloned()))
    }
    fn publish(
        &mut self,
        target: ItemId,
        prepared: PreparedWholeDisposal,
    ) -> Result<WholeDisposal, BillError> {
        // Preparation completed before any field changes; the borrow is exclusive.
        clear_equipment(self.equipment, self.wielded, target);
        clear_reference(self.quivered, target);
        self.timers.cancel(ObjectTimerTarget::Active(target));
        let result = if let Some(record) = prepared.into_consumed() {
            self.billing.consumed.push(record);
            WholeDisposal::RetainedBilled { origin: target }
        } else {
            WholeDisposal::Unbilled { origin: target }
        };
        self.items.remove(target);
        self.unpaid.retain(|(id, _)| *id != target);
        self.billing.live_terms.retain(|(id, _)| *id != target);
        Ok(result)
    }
}

pub(crate) fn finalize(
    world: &mut SimulationWorld,
    survivor: Option<ItemId>,
) -> Result<Option<WholeDisposal>, BillError> {
    let mut port = BorrowedLifetime {
        player: world.player_id,
        ruleset: &world.ruleset,
        items: &mut world.arena.items,
        equipment: &mut world.equipment,
        wielded: &mut world.wielded_item,
        quivered: &mut world.hero.quivered_item,
        timers: &mut world.object_timers,
        billing: &mut world.billing,
        unpaid: &mut world.unpaid_items,
        cached: world
            .stored_levels
            .iter()
            .map(|(_, l)| (l.items.as_slice(), l.unpaid_items.as_slice()))
            .collect(),
    };
    finalize_survivor(&mut port, survivor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{actions::equipment::Dressing, ObjectTimerKind};
    use nethacked_arena::ItemLocation;
    use nethacked_core::ac::ArmorSlot;
    use nethacked_data::{CharacterConfig, ItemKindId};
    use nethacked_types::{BranchId, Buc, Direction};

    fn fixture() -> SimulationWorld {
        let mut world = SimulationWorld::new_with_character(33, CharacterConfig::default());
        world.arena.items.clear();
        world.unpaid_items.clear();
        world.billing = Default::default();
        world.equipment = Default::default();
        world.object_timers = Default::default();
        world.wielded_item = None;
        world.hero.quivered_item = None;
        world
    }
    fn wand(world: &mut SimulationWorld, quantity: u32, charges: i8) -> ItemId {
        let mut object = world
            .ruleset
            .create_item_record_by_id(
                ItemKindId::WAN_FIRE,
                ItemLocation::CarriedBy(world.player_id),
                Buc::Uncursed,
            )
            .unwrap();
        object.quantity = quantity;
        object.enchantment = charges;
        world.arena.items.insert(object)
    }
    fn attach(world: &mut SimulationWorld, target: ItemId, other: ItemId) {
        world.wielded_item = Some(target);
        world.hero.quivered_item = Some(target);
        world.equipment.armor = vec![(ArmorSlot::Suit, target), (ArmorSlot::Helmet, other)];
        world.equipment.dressing = Some(Dressing {
            item: target,
            putting_on: true,
            turns_left: 3,
        });
        for (target, kind) in [
            (
                ObjectTimerTarget::Active(target),
                ObjectTimerKind::RotCorpse,
            ),
            (
                ObjectTimerTarget::Active(target),
                ObjectTimerKind::ReviveMonster,
            ),
            (ObjectTimerTarget::Active(other), ObjectTimerKind::RotCorpse),
            (
                ObjectTimerTarget::Cached {
                    branch: BranchId::DungeonsOfDoom,
                    depth: 3,
                    item: target,
                },
                ObjectTimerKind::RotCorpse,
            ),
        ] {
            world
                .object_timers
                .start_at_preserving(target, kind, Some(9), true)
                .unwrap();
        }
    }
    fn bytes(world: &SimulationWorld) -> Vec<u8> {
        serde_json::to_vec(world).unwrap()
    }

    #[test]
    fn complete_cleanup_retains_known_unknown_whole_quantity_and_unrelated_state() {
        for quantity in [1, 3] {
            for liability in 0..3 {
                let mut world = fixture();
                let target = wand(&mut world, quantity, -1);
                let other = wand(&mut world, 1, 7);
                if liability != 0 {
                    world.unpaid_items.push((target, 30));
                }
                let terms = crate::billing::BillTerms {
                    billed_quantity: Some(quantity),
                    unit_price: Some(40),
                    creditor: None,
                    legacy_base_cost: 30,
                };
                if liability == 2 {
                    world.record_bill_terms(target, terms.clone()).unwrap();
                }
                attach(&mut world, target, other);
                let unrelated_timers: Vec<_> = world
                    .object_timers
                    .entries()
                    .iter()
                    .copied()
                    .filter(|timer| timer.target != ObjectTimerTarget::Active(target))
                    .collect();
                let scheduler = serde_json::to_vec(&world.scheduler).unwrap();
                let rng = world.rng.get_word_pos();
                let other_object = world.arena.items[other].clone();
                assert_eq!(
                    finalize(&mut world, Some(target)).unwrap(),
                    Some(if liability == 0 {
                        WholeDisposal::Unbilled { origin: target }
                    } else {
                        WholeDisposal::RetainedBilled { origin: target }
                    })
                );
                assert!(world.arena.items.get(target).is_none());
                assert_eq!(world.arena.items[other], other_object);
                assert_eq!(world.wielded_item, None);
                assert_eq!(world.hero.quivered_item, None);
                assert_eq!(world.equipment.armor, vec![(ArmorSlot::Helmet, other)]);
                assert_eq!(world.equipment.dressing, None);
                assert_eq!(world.object_timers.entries(), unrelated_timers);
                assert_eq!(serde_json::to_vec(&world.scheduler).unwrap(), scheduler);
                assert_eq!(world.rng.get_word_pos(), rng);
                assert!(world.unpaid_items.is_empty());
                assert!(world.billing.live_terms.is_empty());
                assert_eq!(world.billing.consumed.len(), usize::from(liability != 0));
                if liability != 0 {
                    let record = &world.billing.consumed[0];
                    assert_eq!(record.origin, target);
                    assert_eq!(record.object.quantity, quantity);
                    assert_eq!(record.object.location, ItemLocation::Limbo);
                    assert_eq!(
                        record.terms,
                        if liability == 2 {
                            terms
                        } else {
                            crate::billing::unknown_bill_terms(30)
                        }
                    );
                }
                let after = bytes(&world);
                assert_eq!(finalize(&mut world, Some(target)).unwrap(), None);
                assert_eq!(bytes(&world), after);
            }
        }
    }

    #[test]
    fn inconsistent_receipt_refuses_before_any_reference_or_timer_cleanup() {
        let mut world = fixture();
        let target = wand(&mut world, 1, -1);
        let other = wand(&mut world, 1, 3);
        world.unpaid_items.push((target, 30));
        world.billing.live_terms.push((
            target,
            crate::billing::BillTerms {
                billed_quantity: Some(3),
                unit_price: Some(40),
                creditor: None,
                legacy_base_cost: 30,
            },
        ));
        attach(&mut world, target, other);
        let before = bytes(&world);
        assert_eq!(
            finalize(&mut world, Some(target)),
            Err(BillError::UnsupportedBillingScope)
        );
        assert_eq!(bytes(&world), before);
    }

    #[test]
    fn absent_stale_and_nonnegative_survivors_are_complete_noops() {
        let mut world = fixture();
        let stale = wand(&mut world, 1, -1);
        world.arena.items.remove(stale);
        let current = wand(&mut world, 1, 0);
        assert_ne!(current, stale);
        world.wielded_item = Some(current);
        let before = bytes(&world);
        for survivor in [None, Some(stale), Some(current)] {
            assert_eq!(finalize(&mut world, survivor).unwrap(), None);
            assert_eq!(bytes(&world), before);
        }
    }

    #[test]
    fn existing_empty_negative_zap_does_not_admit_retirement() {
        let mut world = fixture();
        let target = wand(&mut world, 3, -1);
        world.wielded_item = Some(target);
        let rng = world.rng.get_word_pos();
        super::super::wand_legacy::zap(
            &mut world,
            super::super::wand::Selection::Selected {
                index: 0,
                expected: Some(target),
            },
            Direction::East,
            3,
        );
        assert_eq!(world.arena.items[target].enchantment, -1);
        assert_eq!(world.arena.items[target].quantity, 3);
        assert_eq!(world.wielded_item, Some(target));
        assert_eq!(world.rng.get_word_pos(), rng);
        assert!(world.billing.consumed.is_empty());
    }
}
