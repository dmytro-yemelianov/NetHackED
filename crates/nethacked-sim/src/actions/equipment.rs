//! One public suit slot, using released C negative-multi/afternmv timing.
use nethacked_arena::{ItemId, ItemLocation};
use nethacked_core::{ac::ArmorSlot, energy::NORMAL_SPEED};
use nethacked_types::{Buc, ItemClass};
use serde::{Deserialize, Serialize};

use crate::{GameEvent, SimulationWorld};

/// One owner for actual armor slots and the single dressing callback.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquipmentState {
    #[serde(default)]
    pub armor: Vec<(ArmorSlot, ItemId)>,
    #[serde(default)]
    pub dressing: Option<Dressing>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dressing {
    pub item: ItemId,
    pub putting_on: bool,
    pub turns_left: u32,
}

impl EquipmentState {
    pub fn worn(&self, slot: ArmorSlot) -> Option<ItemId> {
        self.armor
            .iter()
            .find(|(s, _)| *s == slot)
            .map(|(_, id)| *id)
    }
}

impl SimulationWorld {
    /// Read effective properties without changing the independent intrinsic owner.
    pub fn effective_hero_intrinsics(&self) -> nethacked_types::Intrinsics {
        let mut properties = self
            .arena
            .actors
            .get(self.player_id)
            .map(|hero| hero.intrinsics)
            .unwrap_or_default();
        let source = self
            .equipment
            .worn(ArmorSlot::Suit)
            .and_then(|id| self.arena.items.get(id))
            .filter(|item| {
                item.location == ItemLocation::CarriedBy(self.player_id)
                    && item.class == ItemClass::Armor
            })
            .and_then(|item| self.ruleset.item(&item.name))
            .filter(|def| {
                def.armor
                    .as_ref()
                    .is_some_and(|a| a.slot == ArmorSlot::Suit)
            })
            .and_then(|def| def.worn_property);
        properties.fire_resistance |=
            source == Some(nethacked_data::items::WornProperty::FireResistance);
        properties
    }

    pub(crate) fn selected_suit_is_worn(&self, id: ItemId) -> bool {
        self.equipment.worn(ArmorSlot::Suit) == Some(id)
    }

    pub(crate) fn handle_dressing(&mut self, index: usize, putting_on: bool) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(item_id) = self
            .arena
            .items_carried_by(self.player_id)
            .get(index)
            .copied()
        else {
            return vec![GameEvent::LogMessage {
                text: "You don't have that item.".into(),
            }];
        };
        let item = self.arena.items.get(item_id).unwrap();
        let Some(def) = self.ruleset.item(&item.name) else {
            return events;
        };
        // Eligibility and delay are owned facts, never display-name guesses.
        if item.class != ItemClass::Armor
            || !def
                .armor
                .as_ref()
                .is_some_and(|a| a.slot == ArmorSlot::Suit)
        {
            return vec![GameEvent::LogMessage {
                text: "This action requires a suit of armor.".into(),
            }];
        }
        // Form fitting/racial exceptions need the effective-form equipment path.
        if self.hero.polymorph.is_some() {
            return vec![GameEvent::LogMessage {
                text: "Suit dressing in this form is not supported.".into(),
            }];
        }
        let worn = self.equipment.worn(ArmorSlot::Suit);
        if self.equipment.worn(ArmorSlot::Cloak).is_some()
            || (putting_on && worn.is_some())
            || (!putting_on && worn != Some(item_id))
        {
            return vec![GameEvent::LogMessage {
                text: "You cannot wear or remove that suit now.".into(),
            }];
        }
        // C select_off's basic curse check returns without setting takeoff.mask;
        // armor_or_accessory_off then returns ECMD_OK, spending no turn.
        if !putting_on && item.buc == Buc::Cursed {
            return vec![GameEvent::LogMessage {
                text: "You can't. It is cursed.".into(),
            }];
        }
        let delay = def.oc_delay;
        if putting_on {
            self.equipment.armor.push((ArmorSlot::Suit, item_id));
            // Released C dowear removes W_WEAPONS before setting W_ARM.
            if self.wielded_item == Some(item_id) {
                self.wielded_item = None;
            }
            if self.hero.quivered_item == Some(item_id) {
                self.hero.quivered_item = None;
            }
        }
        self.equipment.dressing = Some(Dressing {
            item: item_id,
            putting_on,
            turns_left: delay,
        });
        events.push(GameEvent::DressingStarted {
            item: item_id,
            putting_on,
        });
        if delay == 0 {
            self.finish_dressing(&mut events);
        }
        self.scheduler.hero_act(NORMAL_SPEED);
        events
    }

    fn finish_dressing(&mut self, events: &mut Vec<GameEvent>) {
        let Some(dressing) = self.equipment.dressing.take() else {
            return;
        };
        if !dressing.putting_on {
            self.equipment.armor.retain(|(_, id)| *id != dressing.item);
        }
        events.push(GameEvent::DressingFinished {
            item: dressing.item,
            putting_on: dressing.putting_on,
        });
        self.recompute_hero_ac();
    }

    pub(crate) fn tick_dressing(&mut self, events: &mut Vec<GameEvent>) {
        self.reconcile_equipment();
        if self
            .arena
            .actors
            .get(self.player_id)
            .is_none_or(|hero| hero.is_dead)
        {
            self.equipment.dressing = None;
            return;
        }
        if let Some(dressing) = &mut self.equipment.dressing {
            dressing.turns_left = dressing.turns_left.saturating_sub(1);
            if dressing.turns_left == 0 {
                self.finish_dressing(events);
            }
        }
    }

    /// C stop_donning (do_wear.c:1686-1724), called before a theft transfer.
    pub(crate) fn interrupt_dressing(&mut self, events: &mut Vec<GameEvent>) {
        if let Some(dressing) = self.equipment.dressing.take() {
            if dressing.putting_on {
                self.equipment.armor.retain(|(_, id)| *id != dressing.item);
            }
            events.push(GameEvent::DressingInterrupted {
                item: dressing.item,
                putting_on: dressing.putting_on,
            });
            self.recompute_hero_ac();
        }
    }

    /// C cancel_doff/cancel_don: vanished/relocated items cannot be callback targets.
    pub(crate) fn reconcile_equipment(&mut self) {
        let arena = &self.arena;
        let player = self.player_id;
        let rules = &self.ruleset;
        let mut seen = Vec::new();
        self.equipment.armor.retain(|(slot, id)| {
            let valid = arena.items.get(*id).is_some_and(|item| {
                item.location == ItemLocation::CarriedBy(player)
                    && item.class == ItemClass::Armor
                    && rules
                        .item(&item.name)
                        .and_then(|def| def.armor.as_ref())
                        .is_none_or(|armor| armor.slot == *slot)
            }) && !seen.contains(slot);
            if valid {
                seen.push(*slot);
            }
            valid
        });
        if self.equipment.dressing.as_ref().is_some_and(|d| {
            d.turns_left == 0
                || self.equipment.worn(ArmorSlot::Suit) != Some(d.item)
                || !arena
                    .items
                    .get(d.item)
                    .and_then(|item| rules.item(&item.name))
                    .and_then(|def| def.armor.as_ref())
                    .is_some_and(|armor| armor.slot == ArmorSlot::Suit)
        }) {
            self.equipment.dressing = None;
        }
    }

    /// C dig.c:2164–2167 removes worn references before corpse disposal.
    /// A vanished item must not remain a pending dressing callback target.
    pub(crate) fn remove_destroyed_item_equipment(&mut self, id: ItemId) {
        self.equipment.armor.retain(|(_, item)| *item != id);
        if self
            .equipment
            .dressing
            .as_ref()
            .is_some_and(|d| d.item == id)
        {
            self.equipment.dressing = None;
        }
        self.reconcile_equipment();
        self.recompute_hero_ac();
    }
}
