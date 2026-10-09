use nethacked_arena::{ItemLocation, ItemRecord};
use nethacked_core::ActionAst;
use nethacked_sim::{
    owner_persistence::{wand_ports::*, OwnerBridge, OwnerBridgeConfig},
    SimulationWorld,
};
use nethacked_types::{Buc, Direction};
fn fixture() -> (OwnerBridge, SimulationWorld, nethacked_arena::ItemId) {
    let mut w = SimulationWorld::new_with_seed(724);
    w.hero.skills.skills.clear();
    w.arena.items.clear();
    w.wielded_item = None;
    w.equipment = Default::default();
    w.unpaid_items.clear();
    w.billing = Default::default();
    let item = w
        .ruleset
        .create_item_record(
            "wand of fire",
            ItemLocation::CarriedBy(w.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    let id = w.arena.spawn_item(ItemRecord {
        enchantment: 3,
        ..item
    });
    let json = w.to_save_json().unwrap();
    let b = OwnerBridge::import_native(
        &json,
        json.len(),
        OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_wand_gameplay(),
    )
    .unwrap();
    (b, w, id)
}
#[test]
fn public_action_dispatch_cancels_and_refuses_mismatch_without_time() {
    let (b, w, id) = fixture();
    let before = w.to_save_json().unwrap();
    let r = b
        .dispatch_wand_action(ActionAst::CancelZapSelection)
        .unwrap();
    assert_eq!(r.dependency_calls, 0);
    assert_eq!(r.events.len(), 1);
    for index in [1, u32::MAX] {
        let r = b
            .dispatch_wand_action(ActionAst::ZapSelectedWand {
                item_index: index,
                expected_item: Some(id),
                dir: Direction::East,
                energy: 6,
            })
            .unwrap();
        assert!(r.error.is_none());
        assert_eq!(r.events.len(), 1);
    }
    assert!(b.export_native().unwrap().to_save_json().unwrap() == before);
    assert!(b.dispatch_wand_action(ActionAst::Wait).is_err());
}
#[test]
fn public_legacy_and_selected_use_actual_native_caller() {
    let (b, _, id) = fixture();
    let (c, _, other) = fixture();
    let selected = b
        .dispatch_wand_action(ActionAst::ZapSelectedWand {
            item_index: 0,
            expected_item: Some(id),
            dir: Direction::East,
            energy: 0,
        })
        .unwrap();
    let legacy = c
        .dispatch_wand_action(ActionAst::ZapWand {
            dir: Direction::East,
            energy: 0,
        })
        .unwrap();
    assert!(selected.error.is_none() && legacy.error.is_none());
    assert_eq!(selected.events, legacy.events);
    assert!(selected.dependency_calls > 0 && legacy.dependency_calls > 0);
    assert_eq!(b.export_native().unwrap().arena.items[id].enchantment, 2);
    assert_eq!(c.export_native().unwrap().arena.items[other].enchantment, 2);
}
#[test]
fn public_command_profile_cannot_dispatch_gameplay() {
    let (_, w, _) = fixture();
    let json = w.to_save_json().unwrap();
    let b = OwnerBridge::import_native(
        &json,
        json.len(),
        OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_wand_commands(),
    )
    .unwrap();
    assert!(b.dispatch_wand(WandGameplayAction::Cancel).is_err());
    assert!(b.export_native().unwrap().to_save_json().unwrap() == json);
}
#[test]
fn public_selection_checks_expected_token_and_non_wand() {
    let (_, mut w, id) = fixture();
    let second = w
        .ruleset
        .create_item_record(
            "long sword",
            ItemLocation::CarriedBy(w.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    let other = w.arena.spawn_item(second);
    let carried: Vec<_> = w
        .arena
        .items
        .iter()
        .filter_map(|(id, item)| {
            (item.location == ItemLocation::CarriedBy(w.player_id)).then_some(id)
        })
        .collect();
    let wand_index = carried.iter().position(|key| *key == id).unwrap() as u32;
    let other_index = carried.iter().position(|key| *key == other).unwrap() as u32;
    let json = w.to_save_json().unwrap();
    let b = OwnerBridge::import_native(
        &json,
        json.len(),
        OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_wand_gameplay(),
    )
    .unwrap();
    for (index, expected, text) in [
        (wand_index, other, "You don't have that object."),
        (other_index, other, "That is a silly thing to zap."),
    ] {
        let r = b
            .dispatch_wand_action(ActionAst::ZapSelectedWand {
                item_index: index,
                expected_item: Some(expected),
                dir: Direction::East,
                energy: 6,
            })
            .unwrap();
        assert!(r.error.is_none());
        assert!(
            matches!(&r.events[..],[nethacked_sim::GameEvent::LogMessage{text:t}] if t == text)
        );
    }
    let actual = b.export_native().unwrap();
    assert_eq!(actual.arena.items[id].enchantment, 3);
    assert!(actual.to_save_json().unwrap() == json);
}
