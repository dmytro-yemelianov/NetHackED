//! Shared current-Rust equipment and derived-AC laws. No host or mutable context.
use super::equipment::EquipmentState;
use nethacked_arena::{ActorId, ItemId, ItemLocation, ItemStorage};
use nethacked_core::ac::ArmorSlot;
use nethacked_data::Ruleset;
use nethacked_types::ItemClass;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArmorItem {
    pub location: ItemLocation,
    pub class: ItemClass,
    pub slot: Option<ArmorSlot>,
    pub base_ac: Option<i32>,
    pub fallback: i32,
    pub enchantment: i8,
    pub erosion: u8,
}
pub trait Objects {
    fn armor(&self, id: ItemId) -> Option<ArmorItem>;
}
pub struct BorrowedObjects<'a> {
    pub items: &'a ItemStorage,
    pub rules: &'a Ruleset,
}
impl Objects for BorrowedObjects<'_> {
    fn armor(&self, id: ItemId) -> Option<ArmorItem> {
        self.items.get(id).map(|i| {
            let a = self.rules.item(&i.name).and_then(|d| d.armor.as_ref());
            ArmorItem {
                location: i.location.clone(),
                class: i.class,
                slot: a.map(|a| a.slot),
                base_ac: a.map(|a| a.base_ac),
                fallback: nethacked_core::armor_base_ac(&i.name),
                enchantment: i.enchantment,
                erosion: i.erosion,
            }
        })
    }
}
pub fn reconcile(e: &EquipmentState, objects: &impl Objects, player: ActorId) -> EquipmentState {
    let mut next = e.clone();
    let mut seen = Vec::new();
    next.armor.retain(|(slot, id)| {
        let valid = objects.armor(*id).is_some_and(|i| {
            i.location == ItemLocation::CarriedBy(player)
                && i.class == ItemClass::Armor
                && i.slot.is_none_or(|s| s == *slot)
        }) && !seen.contains(slot);
        if valid {
            seen.push(*slot);
        }
        valid
    });
    if next.dressing.as_ref().is_some_and(|d| {
        d.turns_left == 0
            || next.worn(ArmorSlot::Suit) != Some(d.item)
            || !objects
                .armor(d.item)
                .is_some_and(|i| i.slot == Some(ArmorSlot::Suit))
    }) {
        next.dressing = None;
    }
    next
}
pub fn armor_class(
    e: &EquipmentState,
    objects: &impl Objects,
    player: ActorId,
    protection: u32,
) -> i32 {
    let armor = e
        .armor
        .iter()
        .filter_map(|(slot, id)| {
            let i = objects.armor(*id)?;
            if i.location != ItemLocation::CarriedBy(player) {
                return None;
            }
            let base = i
                .base_ac
                .filter(|_| i.slot == Some(*slot))
                .or_else(|| (*slot != ArmorSlot::Suit).then_some(i.fallback))?;
            Some((base, i.enchantment as i32, i.erosion))
        })
        .collect::<Vec<_>>();
    nethacked_core::find_ac(10, &armor, protection as i32)
}
