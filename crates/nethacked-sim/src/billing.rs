//! Historical accounting values and bounded receipt validation.
//! Missing source terms remain unknown; these facts confer no live authority.
use nethacked_arena::{ActorId, ItemId, ItemLocation};
use nethacked_data::ruleset::Ruleset;
use nethacked_types::ItemClass;
use serde::{Deserialize, Serialize};

/// Historical source receipt identity; never an active shopkeeper lookup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillCreditor {
    pub branch: nethacked_types::BranchId,
    pub depth: usize,
    pub shopkeeper: ActorId,
}

/// Source-provided historical facts. None means unavailable, never paid or zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillTerms {
    pub billed_quantity: Option<u32>,
    pub unit_price: Option<u32>,
    pub creditor: Option<BillCreditor>,
    pub legacy_base_cost: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsumedBillRecord {
    pub origin: ItemId,
    pub object: nethacked_arena::ItemRecord,
    pub terms: BillTerms,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillingState {
    pub live_terms: Vec<(ItemId, BillTerms)>,
    pub consumed: Vec<ConsumedBillRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BillError {
    AlreadyConsumed,
    StaleItem,
    NotCarried,
    InvalidDefinition,
    ZeroQuantity,
    DuplicateLedger,
    InvalidReceipt,
    UnsupportedBillingScope,
    LifecycleReferencesPresent,
    ReceiptAlreadyRecorded,
    SettlementDeferred,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WholeDisposal {
    Unbilled { origin: ItemId },
    RetainedBilled { origin: ItemId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BillValuation {
    Known {
        total: u64,
    },
    UnresolvedTerms {
        base: u32,
        current_consumed_quantity: Option<u32>,
    },
}

/// Live ledger order followed by detached consumption order. Missing live
/// objects remain visible as unresolved obligations rather than disappearing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillView {
    pub origin: ItemId,
    pub object: Option<nethacked_arena::ItemRecord>,
    pub terms: BillTerms,
    pub outstanding: bool,
    pub useup: bool,
    pub valuation: BillValuation,
}

pub(crate) fn validate_bill_terms(
    terms: &BillTerms,
    quantity: u32,
    base: u32,
) -> Result<(), BillError> {
    if terms.legacy_base_cost != base {
        return Err(BillError::InvalidReceipt);
    }
    let (Some(q), Some(price)) = (terms.billed_quantity, terms.unit_price) else {
        return Err(BillError::InvalidReceipt);
    };
    if q == 0 || price == 0 || u64::from(q) * u64::from(price) > i64::MAX as u64 {
        return Err(BillError::InvalidReceipt);
    }
    if q != quantity {
        return Err(BillError::UnsupportedBillingScope);
    }
    Ok(())
}

pub(crate) fn unknown_bill_terms(base: u32) -> BillTerms {
    BillTerms {
        billed_quantity: None,
        unit_price: None,
        creditor: None,
        legacy_base_cost: base,
    }
}

pub(crate) fn validate_bill_object<'a>(
    id: ItemId,
    items: &'a nethacked_arena::ItemStorage,
    player_id: ActorId,
    billing: &BillingState,
    ruleset: &Ruleset,
) -> Result<&'a nethacked_arena::ItemRecord, BillError> {
    if billing.consumed.iter().any(|bill| bill.origin == id) {
        return Err(BillError::AlreadyConsumed);
    }
    let object = items.get(id).ok_or(BillError::StaleItem)?;
    if object.location != ItemLocation::CarriedBy(player_id) {
        return Err(BillError::NotCarried);
    }
    let rs = ruleset;
    let def = match &object.definition {
        Some(identity) => rs.item_for_definition(identity),
        None => rs.item(&object.name),
    }
    .ok_or(BillError::InvalidDefinition)?;
    if def.class != object.class {
        return Err(BillError::InvalidDefinition);
    }
    if object.class != ItemClass::Wand
        || object.is_container
        || items
            .iter()
            .any(|(_, item)| item.location == ItemLocation::InContainer(id))
    {
        return Err(BillError::UnsupportedBillingScope);
    }
    if object.quantity == 0 {
        return Err(BillError::ZeroQuantity);
    }
    Ok(object)
}

pub(crate) fn bill_ledger<'a>(
    id: ItemId,
    unpaid_items: &'a [(ItemId, u32)],
    billing: &'a BillingState,
    cached_unpaid: impl Iterator<Item = &'a (ItemId, u32)>,
    cached_contains: bool,
) -> Result<(Option<u32>, Option<&'a BillTerms>), BillError> {
    let tuples: Vec<_> = unpaid_items
        .iter()
        .chain(cached_unpaid)
        .filter(|(i, _)| *i == id)
        .collect();
    let receipts: Vec<_> = billing
        .live_terms
        .iter()
        .filter(|(i, _)| *i == id)
        .collect();
    if tuples.len() > 1 || receipts.len() > 1 || cached_contains {
        return Err(BillError::DuplicateLedger);
    }
    if !receipts.is_empty() && tuples.is_empty() {
        return Err(BillError::InvalidReceipt);
    }
    // A carried item must belong to the active compatibility ledger.
    if !tuples.is_empty() && !unpaid_items.iter().any(|(i, _)| *i == id) {
        return Err(BillError::InvalidReceipt);
    }
    Ok((
        tuples.first().map(|(_, base)| *base),
        receipts.first().map(|(_, terms)| terms),
    ))
}
