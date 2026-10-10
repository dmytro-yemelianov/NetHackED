//! Bounded immutable disposal preparation shared by lifetime owners and adapters.
//! This module accepts one observed object and its selected accounting facts;
//! the caller retains responsibility for authority, references and publication.
use crate::billing::{
    unknown_bill_terms, validate_bill_terms, BillError, BillTerms, ConsumedBillRecord,
};
use nethacked_arena::{ItemId, ItemLocation, ItemRecord};

pub(crate) struct PreparedWholeDisposal {
    pub(crate) origin: ItemId,
    object: ItemRecord,
    terms: Option<BillTerms>,
}

pub(crate) fn prepare_disposal(
    origin: ItemId,
    object: &ItemRecord,
    base: Option<u32>,
    receipt: Option<&BillTerms>,
) -> Result<PreparedWholeDisposal, BillError> {
    if let Some(terms) = receipt {
        validate_bill_terms(
            terms,
            object.quantity,
            base.ok_or(BillError::InvalidReceipt)?,
        )?;
    }
    Ok(PreparedWholeDisposal {
        origin,
        object: object.clone(),
        terms: receipt.cloned().or_else(|| base.map(unknown_bill_terms)),
    })
}

impl PreparedWholeDisposal {
    pub(crate) fn into_consumed(mut self) -> Option<ConsumedBillRecord> {
        let terms = self.terms?;
        self.object.location = ItemLocation::Limbo;
        Some(ConsumedBillRecord {
            origin: self.origin,
            object: self.object,
            terms,
        })
    }
}

/// A retained survivor is a generation-aware identity, never an inventory index.
/// Native callers use ObjectRef here so residency participates in comparison.
pub(crate) fn negative_survivor<T: Copy + Eq>(
    retained: Option<T>,
    observed: Option<(T, &ItemRecord)>,
) -> Option<T> {
    let retained = retained?;
    let (current, object) = observed?;
    (retained == current
        && object.class == nethacked_types::ItemClass::Wand
        && object.enchantment < 0)
        .then_some(retained)
}

pub(crate) fn clear_reference(reference: &mut Option<ItemId>, target: ItemId) {
    if *reference == Some(target) {
        *reference = None;
    }
}

pub(crate) fn clear_equipment(
    equipment: &mut crate::actions::equipment::EquipmentState,
    wielded: &mut Option<ItemId>,
    target: ItemId,
) {
    clear_reference(wielded, target);
    equipment.armor.retain(|(_, id)| *id != target);
    if equipment
        .dressing
        .as_ref()
        .is_some_and(|d| d.item == target)
    {
        equipment.dressing = None;
    }
}

/// Finite lifetime capability: one object, its accounting, one publication.
/// Native publication uses a sealed owner batch; compatibility publication uses
/// an exclusive borrow. Neither backend changes the shared decision ordering.
pub(crate) trait LifetimePort {
    type Target: Copy + Eq;
    type Error: From<BillError>;
    fn object(&mut self, target: Self::Target) -> Result<Option<ItemRecord>, Self::Error>;
    fn origin(&self, target: Self::Target) -> ItemId;
    fn accounting(
        &mut self,
        target: Self::Target,
        object: &ItemRecord,
    ) -> Result<(Option<u32>, Option<BillTerms>), Self::Error>;
    fn publish(
        &mut self,
        target: Self::Target,
        prepared: PreparedWholeDisposal,
    ) -> Result<crate::billing::WholeDisposal, Self::Error>;
}

pub(crate) fn finalize_survivor<P: LifetimePort>(
    port: &mut P,
    retained: Option<P::Target>,
) -> Result<Option<crate::billing::WholeDisposal>, P::Error> {
    let Some(target) = retained else {
        return Ok(None);
    };
    let Some(object) = port.object(target)? else {
        return Ok(None);
    };
    let Some(target) = negative_survivor(retained, Some((target, &object))) else {
        return Ok(None);
    };
    let (base, terms) = port.accounting(target, &object)?;
    let prepared = prepare_disposal(port.origin(target), &object, base, terms.as_ref())?;
    port.publish(target, prepared).map(Some)
}
