//! Literal expectations from NetHack-5.0.0_Released 16ff59115315917b93185d026aeefea06db9b0f4.
//! objects.h:595-597 leather delay3; do_wear.c:2375 immediate slot,
//! 887-929 callbacks; 1686-1724 asymmetric stop; 2785 curse; allmain.c:379-387.
use nethacked_arena::{ItemId, ItemLocation};
use nethacked_core::{ac::ArmorSlot, ActionAst};
use nethacked_data::{ruleset::PackManifest, CharacterConfig, ItemKindId, RoleId};
use nethacked_sim::{Buc, Coord, GameEvent, Ruleset, SimulationWorld, Tile};
use nethacked_types::{Attack, AttackType, DamageType};
use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::sync::Arc;

fn world() -> SimulationWorld {
    let mut sim = SimulationWorld::new_with_character(
        42,
        CharacterConfig {
            role: RoleId::Valkyrie,
            ..Default::default()
        },
    );
    let others: Vec<_> = sim
        .arena
        .actors
        .iter()
        .filter(|(id, _)| *id != sim.player_id)
        .map(|(id, _)| id)
        .collect();
    for id in others {
        sim.arena.actors.remove(id);
    }
    let items: Vec<_> = sim.arena.items.iter().map(|(id, _)| id).collect();
    for id in items {
        sim.arena.destroy_item(id);
    }
    sim.equipment.armor.clear();
    sim.wielded_item = None;
    sim.scheduler.monster_speed = 0;
    sim.scheduler.monster_energy = 0;
    sim.rng = ChaCha8Rng::seed_from_u64(7);
    sim
}
fn add(sim: &mut SimulationWorld, kind: ItemKindId) -> ItemId {
    sim.arena.spawn_item(
        sim.ruleset
            .create_item_record_by_id(kind, ItemLocation::CarriedBy(sim.player_id), Buc::Uncursed)
            .unwrap(),
    )
}
fn index(sim: &SimulationWorld, id: ItemId) -> usize {
    sim.arena
        .items_carried_by(sim.player_id)
        .iter()
        .position(|item| *item == id)
        .unwrap()
}
fn wear(sim: &mut SimulationWorld, id: ItemId) {
    sim.step_player_action(ActionAst::Wear(index(sim, id)));
}
fn complete(sim: &mut SimulationWorld) {
    while sim.equipment.dressing.is_some() {
        sim.step_player_action(ActionAst::Wait);
    }
}
fn finished(events: &[GameEvent]) -> bool {
    events
        .iter()
        .any(|e| matches!(e, GameEvent::DressingFinished { .. }))
}

#[test]
fn released_suit_start_progress_finish_and_remove_cost_three_world_turns() {
    let mut sim = world();
    let suit = add(&mut sim, ItemKindId::LEATHER_ARMOR);
    sim.recompute_hero_ac();
    assert_eq!(sim.compute_hero_ac(), 10); // merely carried
    let turn = sim.scheduler.turn;
    let nutrition = sim.player_nutrition;
    let start = sim.step_player_action(ActionAst::Wear(index(&sim, suit)));
    assert!(start.contains(&GameEvent::DressingStarted {
        item: suit,
        putting_on: true
    }));
    assert!(!finished(&start));
    assert_eq!(sim.compute_hero_ac(), 8);
    assert_eq!(sim.equipment.worn(ArmorSlot::Suit), Some(suit));
    assert_eq!(sim.equipment.dressing.as_ref().unwrap().turns_left, 2);
    // All commands during negative multi consume waiting time, never execute.
    let coord = sim.hero_coord();
    let progress = sim.step_player_action(ActionAst::Drop(index(&sim, suit)));
    assert!(!finished(&progress));
    assert_eq!(sim.hero_coord(), coord);
    assert_eq!(
        sim.arena.items.get(suit).unwrap().location,
        ItemLocation::CarriedBy(sim.player_id)
    );
    assert_eq!(sim.equipment.dressing.as_ref().unwrap().turns_left, 1);
    let end = sim.step_player_action(ActionAst::Wait);
    assert!(end.contains(&GameEvent::DressingFinished {
        item: suit,
        putting_on: true
    }));
    assert_eq!(sim.scheduler.turn - turn, 3);
    assert_eq!(nutrition - sim.player_nutrition, 3);
    assert!(sim.equipment.dressing.is_none());
    let turn = sim.scheduler.turn;
    sim.step_player_action(ActionAst::Remove(index(&sim, suit)));
    assert_eq!(sim.compute_hero_ac(), 8);
    assert_eq!(sim.equipment.dressing.as_ref().unwrap().turns_left, 2);
    sim.step_player_action(ActionAst::Wait);
    assert_eq!(sim.compute_hero_ac(), 8);
    let end = sim.step_player_action(ActionAst::Wait);
    assert!(end.contains(&GameEvent::DressingFinished {
        item: suit,
        putting_on: false
    }));
    assert_eq!(sim.compute_hero_ac(), 10);
    assert_eq!(sim.equipment.worn(ArmorSlot::Suit), None);
    assert_eq!(sim.scheduler.turn - turn, 3);
    assert!(!finished(&sim.step_player_action(ActionAst::Wait))); // callback exactly once
}
#[test]
fn cursed_removal_is_zero_time_and_retry_requires_uncursing() {
    let mut sim = world();
    let suit = add(&mut sim, ItemKindId::LEATHER_ARMOR);
    wear(&mut sim, suit);
    complete(&mut sim);
    sim.arena.items.get_mut(suit).unwrap().buc = Buc::Cursed;
    let turn = sim.scheduler.turn;
    let energy = sim.scheduler.hero_energy;
    let nutrition = sim.player_nutrition;
    let mut rng = sim.rng.clone();
    let refused = sim.step_player_action(ActionAst::Remove(index(&sim, suit)));
    assert_eq!(
        refused,
        vec![GameEvent::LogMessage {
            text: "You can't. It is cursed.".into()
        }]
    );
    assert_eq!(sim.scheduler.turn, turn);
    assert_eq!(sim.scheduler.hero_energy, energy);
    assert_eq!(sim.player_nutrition, nutrition);
    assert_eq!(sim.rng.next_u64(), rng.next_u64());
    assert_eq!(sim.compute_hero_ac(), 8);
    assert!(sim.equipment.dressing.is_none());
    sim.arena.items.get_mut(suit).unwrap().buc = Buc::Uncursed;
    sim.step_player_action(ActionAst::Remove(index(&sim, suit)));
    complete(&mut sim);
    assert_eq!(sim.scheduler.turn - turn, 3);
    assert_eq!(sim.compute_hero_ac(), 10);
}
#[test]
fn dressing_is_world_turn_owned_even_when_hero_has_two_actions_per_tick() {
    let mut sim = world();
    let suit = add(&mut sim, ItemKindId::LEATHER_ARMOR);
    sim.scheduler.hero_speed = 24;
    wear(&mut sim, suit);
    let turn = sim.scheduler.turn;
    assert_eq!(sim.equipment.dressing.as_ref().unwrap().turns_left, 2);
    sim.step_player_action(ActionAst::Wait);
    assert_eq!(sim.scheduler.turn, turn);
    assert_eq!(sim.equipment.dressing.as_ref().unwrap().turns_left, 2);
    complete(&mut sim);
    assert_eq!(sim.scheduler.turn, 4);
}
fn custom(delay: u32, base_ac: i32, slot: ArmorSlot) -> Arc<Ruleset> {
    let vanilla = Ruleset::vanilla();
    let mut items = vanilla.items.clone();
    let suit = items
        .iter_mut()
        .find(|d| d.id == Some(ItemKindId::LEATHER_ARMOR))
        .unwrap();
    suit.oc_delay = delay;
    suit.armor.as_mut().unwrap().base_ac = base_ac;
    suit.armor.as_mut().unwrap().slot = slot;
    Arc::new(Ruleset::from_parts(
        PackManifest {
            id: "owned-dressing".into(),
            name: "Owned dressing".into(),
            version: "1".into(),
            base: "vanilla".into(),
            description: String::new(),
        },
        vanilla.monsters.clone(),
        items,
        vanilla.roles.clone(),
        vanilla.races.clone(),
        vanilla.mechanics.clone(),
    ))
}
#[test]
fn owned_overrides_control_delay_ac_and_slot_and_zero_still_spends_one_turn() {
    let mut sim = world();
    sim.ruleset = custom(5, 6, ArmorSlot::Suit);
    let suit = add(&mut sim, ItemKindId::LEATHER_ARMOR);
    wear(&mut sim, suit);
    assert_eq!(sim.compute_hero_ac(), 4);
    assert_eq!(sim.equipment.dressing.as_ref().unwrap().turns_left, 4);
    complete(&mut sim);
    assert_eq!(sim.scheduler.turn, 6);
    let mut sim = world();
    sim.ruleset = custom(0, 4, ArmorSlot::Suit);
    let suit = add(&mut sim, ItemKindId::LEATHER_ARMOR);
    let events = sim.step_player_action(ActionAst::Wear(index(&sim, suit)));
    assert!(finished(&events));
    assert!(sim.equipment.dressing.is_none());
    assert_eq!(sim.scheduler.turn, 2);
    assert_eq!(sim.compute_hero_ac(), 6);
    let mut sim = world();
    sim.ruleset = custom(3, 2, ArmorSlot::Helmet);
    let suit = add(&mut sim, ItemKindId::LEATHER_ARMOR);
    wear(&mut sim, suit);
    assert_eq!(sim.scheduler.turn, 1);
    assert_eq!(sim.compute_hero_ac(), 10);
    assert!(sim.equipment.dressing.is_none());
}
#[test]
fn occupied_suit_actual_cloak_and_invalid_index_refuse_without_spending_time() {
    let mut sim = world();
    let suit = add(&mut sim, ItemKindId::LEATHER_ARMOR);
    let other = add(&mut sim, ItemKindId::CHAIN_MAIL);
    wear(&mut sim, suit);
    complete(&mut sim);
    let turn = sim.scheduler.turn;
    wear(&mut sim, other);
    assert_eq!(sim.scheduler.turn, turn);
    assert_eq!(sim.equipment.worn(ArmorSlot::Suit), Some(suit));
    sim.step_player_action(ActionAst::Wear(999));
    assert_eq!(sim.scheduler.turn, turn);
    let cloak = add(&mut sim, ItemKindId::CLOAK_OF_MAGIC_RESISTANCE);
    sim.equipment.armor.push((ArmorSlot::Cloak, cloak));
    sim.step_player_action(ActionAst::Remove(index(&sim, suit)));
    assert_eq!(sim.scheduler.turn, turn);
    assert!(sim.equipment.dressing.is_none());
}
#[test]
fn save_load_mid_don_and_doff_preserves_callback_events_state_and_rng() {
    for removing in [false, true] {
        let mut sim = world();
        let suit = add(&mut sim, ItemKindId::LEATHER_ARMOR);
        wear(&mut sim, suit);
        if removing {
            complete(&mut sim);
            sim.step_player_action(ActionAst::Remove(index(&sim, suit)));
        }
        let saved = sim.to_save_json().unwrap();
        let mut loaded =
            SimulationWorld::from_save_json(&saved, Arc::clone(&sim.ruleset), &sim.ruleset_ref)
                .unwrap();
        for _ in 0..4 {
            assert_eq!(
                sim.step_player_action(ActionAst::Wait),
                loaded.step_player_action(ActionAst::Wait)
            );
            let before: serde_json::Value =
                serde_json::from_str(&sim.to_save_json().unwrap()).unwrap();
            let after: serde_json::Value =
                serde_json::from_str(&loaded.to_save_json().unwrap()).unwrap();
            assert_same(&before, &after, "");
        }
        assert_eq!(sim.rng.next_u64(), loaded.rng.next_u64());
    }
}
#[test]
fn absent_owner_migrates_first_carried_per_slot_but_explicit_empty_does_not() {
    let mut sim = world();
    add(&mut sim, ItemKindId::LEATHER_ARMOR);
    add(&mut sim, ItemKindId::CHAIN_MAIL);
    // Arena slot order, not insertion order, defines the old first-carried rule.
    let carried = sim.arena.items_carried_by(sim.player_id);
    let first = carried[0];
    let stronger = carried[1];
    sim.arena.items.get_mut(first).unwrap().name = "leather armor".into();
    sim.arena.items.get_mut(stronger).unwrap().name = "chain mail".into();
    sim.arena.items.get_mut(stronger).unwrap().enchantment = 5;
    let mut value: serde_json::Value = serde_json::from_str(&sim.to_save_json().unwrap()).unwrap();
    value.as_object_mut().unwrap().remove("equipment");
    let loaded = SimulationWorld::from_save_json(
        &value.to_string(),
        Arc::clone(&sim.ruleset),
        &sim.ruleset_ref,
    )
    .unwrap();
    assert_eq!(loaded.equipment.worn(ArmorSlot::Suit), Some(first));
    assert_eq!(loaded.compute_hero_ac(), 8);
    assert!(loaded.equipment.dressing.is_none());
    let explicit = SimulationWorld::from_save_json(
        &sim.to_save_json().unwrap(),
        Arc::clone(&sim.ruleset),
        &sim.ruleset_ref,
    )
    .unwrap();
    assert!(explicit.equipment.armor.is_empty());
    assert_eq!(explicit.compute_hero_ac(), 10);
}
#[test]
fn relocation_or_destruction_never_leaves_stale_slot_or_callback_references() {
    for destroy in [false, true] {
        let mut sim = world();
        let suit = add(&mut sim, ItemKindId::LEATHER_ARMOR);
        wear(&mut sim, suit);
        if destroy {
            sim.arena.destroy_item(suit);
        } else {
            sim.arena.items.get_mut(suit).unwrap().location = ItemLocation::Floor(sim.hero_coord());
        }
        let mut loaded = SimulationWorld::from_save_json(
            &sim.to_save_json().unwrap(),
            Arc::clone(&sim.ruleset),
            &sim.ruleset_ref,
        )
        .unwrap();
        assert!(loaded.equipment.armor.is_empty());
        assert!(loaded.equipment.dressing.is_none());
        assert_eq!(loaded.compute_hero_ac(), 10);
        assert!(!finished(&loaded.step_player_action(ActionAst::Wait)));
    }
}

fn thief(sim: &mut SimulationWorld) -> nethacked_arena::ActorId {
    let vanilla = Arc::clone(&sim.ruleset);
    let mut monsters = vanilla.monsters.clone();
    let def = monsters
        .iter_mut()
        .find(|m| m.name == "wood nymph")
        .unwrap();
    def.attacks = vec![Attack {
        at: AttackType::Claw,
        ad: DamageType::StealItem,
        n: 0,
        d: 0,
    }];
    def.level = 100;
    sim.ruleset = Arc::new(Ruleset::from_parts(
        PackManifest {
            id: "test-thief".into(),
            name: "Test thief".into(),
            version: "1".into(),
            base: "vanilla".into(),
            description: String::new(),
        },
        monsters,
        vanilla.items.clone(),
        vanilla.roles.clone(),
        vanilla.races.clone(),
        vanilla.mechanics.clone(),
    ));
    let coord = Coord::new(sim.hero_coord().x + 1, sim.hero_coord().y).unwrap();
    sim.level.set_tile(coord, Tile::Room);
    let id = sim.spawn_monster_by_name("wood nymph", coord).unwrap();
    sim.arena.actors.get_mut(id).unwrap().is_peaceful = false;
    sim.arena.actors[id].movement = 12;
    sim.arena.actors[id].speed = 0;
    sim.scheduler.hero_energy = 24;
    sim.level.no_teleport = true;
    id
}
#[test]
fn genuine_other_item_theft_interrupts_don_and_doff_asymmetrically_and_allows_retry() {
    for removing in [false, true] {
        let mut sim = world();
        let suit = add(&mut sim, ItemKindId::LEATHER_ARMOR);
        let loot = add(&mut sim, ItemKindId::DAGGER);
        if removing {
            wear(&mut sim, suit);
            complete(&mut sim);
        }
        let id = thief(&mut sim);
        sim.rng = ChaCha8Rng::seed_from_u64(7);
        let action = if removing {
            ActionAst::Remove(index(&sim, suit))
        } else {
            ActionAst::Wear(index(&sim, suit))
        };
        let events = sim.step_player_action(action);
        assert!(
            events.contains(&GameEvent::ItemStolen {
                actor: id,
                item: loot
            }),
            "{events:?}"
        );
        assert!(events.contains(&GameEvent::DressingInterrupted {
            item: suit,
            putting_on: !removing
        }));
        assert!(!finished(&events));
        assert!(sim.equipment.dressing.is_none());
        assert_eq!(
            sim.arena.items.get(loot).unwrap().location,
            ItemLocation::CarriedBy(id)
        );
        assert_eq!(
            sim.equipment.worn(ArmorSlot::Suit),
            if removing { Some(suit) } else { None }
        );
        assert_eq!(sim.compute_hero_ac(), if removing { 8 } else { 10 });
        sim.arena.actors.get_mut(id).unwrap().is_peaceful = true;
        let turn = sim.scheduler.turn;
        let retry = if removing {
            ActionAst::Remove(index(&sim, suit))
        } else {
            ActionAst::Wear(index(&sim, suit))
        };
        sim.step_player_action(retry);
        complete(&mut sim);
        assert_eq!(sim.scheduler.turn - turn, 3);
        assert_eq!(sim.compute_hero_ac(), if removing { 10 } else { 8 });
    }
}

fn assert_same(a: &serde_json::Value, b: &serde_json::Value, path: &str) {
    if a == b {
        return;
    }
    match (a, b) {
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "keys at {path}"
            );
            for (key, value) in a {
                assert_same(value, &b[key], &format!("{path}/{key}"));
            }
        }
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) if a.len() == b.len() => {
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                assert_same(a, b, &format!("{path}/{i}"));
            }
        }
        _ => panic!("save difference at {path}: {a} != {b}"),
    }
}

#[test]
fn slow_hero_counts_world_turns_and_callback_precedes_next_energy_boundary() {
    let mut sim = world();
    let suit = add(&mut sim, ItemKindId::LEATHER_ARMOR);
    sim.scheduler.hero_speed = 6;
    wear(&mut sim, suit);
    assert_eq!(sim.scheduler.turn, 3);
    assert_eq!(sim.scheduler.hero_energy, 12);
    assert_eq!(sim.equipment.dressing.as_ref().unwrap().turns_left, 1);
    let saved = sim.to_save_json().unwrap();
    let mut loaded =
        SimulationWorld::from_save_json(&saved, Arc::clone(&sim.ruleset), &sim.ruleset_ref)
            .unwrap();
    let events = sim.step_player_action(ActionAst::Wait);
    assert_eq!(events, loaded.step_player_action(ActionAst::Wait));
    assert_eq!(sim.scheduler.turn, 5);
    assert_eq!(sim.scheduler.hero_energy, 12);
    assert!(sim.equipment.dressing.is_none());
    let tick = events
        .iter()
        .position(|e| *e == GameEvent::TurnAdvanced { turn: 4 })
        .unwrap();
    assert_eq!(
        events[tick + 1],
        GameEvent::DressingFinished {
            item: suit,
            putting_on: true
        }
    );
    assert!(!finished(&sim.step_player_action(ActionAst::Wait)));
}
#[test]
fn destroyed_target_after_removal_start_cancels_without_finishing() {
    let mut sim = world();
    let suit = add(&mut sim, ItemKindId::LEATHER_ARMOR);
    wear(&mut sim, suit);
    complete(&mut sim);
    sim.step_player_action(ActionAst::Remove(index(&sim, suit)));
    sim.arena.destroy_item(suit);
    let events = sim.step_player_action(ActionAst::Wait);
    assert!(!finished(&events));
    assert!(sim.equipment.armor.is_empty());
    assert!(sim.equipment.dressing.is_none());
}

#[test]
fn renamed_owned_suit_can_resume_its_custom_delay_after_save() {
    let owned = custom(5, 6, ArmorSlot::Suit);
    let mut items = owned.items.clone();
    items
        .iter_mut()
        .find(|item| item.id == Some(ItemKindId::LEATHER_ARMOR))
        .unwrap()
        .name = "woven test suit".into();
    let rules = Arc::new(Ruleset::from_parts(
        owned.manifest.clone(),
        owned.monsters.clone(),
        items,
        owned.roles.clone(),
        owned.races.clone(),
        owned.mechanics.clone(),
    ));
    let mut sim = world();
    sim.ruleset = rules;
    let suit = add(&mut sim, ItemKindId::LEATHER_ARMOR);
    wear(&mut sim, suit);
    assert_eq!(sim.arena.items.get(suit).unwrap().name, "woven test suit");
    assert_eq!(sim.compute_hero_ac(), 4);
    let mut loaded = SimulationWorld::from_save_json(
        &sim.to_save_json().unwrap(),
        Arc::clone(&sim.ruleset),
        &sim.ruleset_ref,
    )
    .unwrap();
    for _ in 0..4 {
        assert_eq!(
            sim.step_player_action(ActionAst::Wait),
            loaded.step_player_action(ActionAst::Wait)
        );
    }
    assert!(loaded.equipment.dressing.is_none());
    assert_eq!(loaded.compute_hero_ac(), 4);
    assert_eq!(loaded.scheduler.turn, 6);
    assert_eq!(sim.rng.next_u64(), loaded.rng.next_u64());
}

#[test]
fn genuine_theft_during_a_pending_wait_never_executes_the_requested_move() {
    for removing in [false, true] {
        let mut sim = world();
        let suit = add(&mut sim, ItemKindId::LEATHER_ARMOR);
        let loot = add(&mut sim, ItemKindId::DAGGER);
        wear(&mut sim, suit);
        if removing {
            complete(&mut sim);
            sim.step_player_action(ActionAst::Remove(index(&sim, suit)));
        }
        assert_eq!(sim.equipment.dressing.as_ref().unwrap().turns_left, 2);
        let id = thief(&mut sim);
        sim.rng = ChaCha8Rng::seed_from_u64(7);
        let coord = sim.hero_coord();
        let events = sim.step_player_action(ActionAst::Move(nethacked_sim::Direction::East));
        assert!(events.contains(&GameEvent::ItemStolen {
            actor: id,
            item: loot
        }));
        assert_eq!(sim.hero_coord(), coord);
        assert!(events.contains(&GameEvent::DressingInterrupted {
            item: suit,
            putting_on: !removing
        }));
        assert_eq!(
            sim.equipment.worn(ArmorSlot::Suit),
            if removing { Some(suit) } else { None }
        );
    }
}

#[test]
fn theft_of_the_incomplete_suit_unworns_before_transfer_and_clears_owner() {
    let mut sim = world();
    let suit = add(&mut sim, ItemKindId::LEATHER_ARMOR);
    add(&mut sim, ItemKindId::DAGGER);
    let id = thief(&mut sim);
    sim.rng = ChaCha8Rng::seed_from_u64(0);
    let events = sim.step_player_action(ActionAst::Wear(index(&sim, suit)));
    assert!(events.contains(&GameEvent::ItemStolen {
        actor: id,
        item: suit
    }));
    assert!(sim.equipment.armor.is_empty());
    assert!(sim.equipment.dressing.is_none());
    assert_eq!(
        sim.arena.items.get(suit).unwrap().location,
        ItemLocation::CarriedBy(id)
    );
    assert_eq!(sim.compute_hero_ac(), 10);
    assert!(!finished(&sim.step_player_action(ActionAst::Wait)));
}

// Repair1 source: do.c665-670,714-719; pickup.c2573-2576,3365-3391;
// wield.c565-567; dothrow.c118-120. No private handler/cancel invocation.
#[test]
fn public_inventory_routes_cannot_remove_a_worn_or_cursed_suit() {
    for cursed in [false, true] {
        for route in 0..4 {
            let mut sim = world();
            let suit = add(&mut sim, ItemKindId::LEATHER_ARMOR);
            let sack = add(&mut sim, ItemKindId::SACK);
            wear(&mut sim, suit);
            complete(&mut sim);
            if cursed {
                sim.arena.items.get_mut(suit).unwrap().buc = Buc::Cursed;
            }
            if route == 3 {
                sim.hero.quivered_item = Some(suit);
            } // imported stale selection
            let action = match route {
                0 => ActionAst::Drop(index(&sim, suit)),
                1 => ActionAst::PutInContainer {
                    item_index: index(&sim, suit),
                    container_index: index(&sim, sack),
                },
                2 => ActionAst::Quiver(suit),
                _ => ActionAst::Fire(nethacked_sim::Direction::East),
            };
            let turn = sim.scheduler.turn;
            let nutrition = sim.player_nutrition;
            let energy = sim.scheduler.hero_energy;
            let mut rng = sim.rng.clone();
            let events = sim.step_player_action(action);
            let cost = u64::from(route == 1); // explicit C menu selection counts n=1 even callback0
            assert_eq!(sim.scheduler.turn - turn, cost);
            assert_eq!(nutrition - sim.player_nutrition, cost as i32);
            assert_eq!(sim.scheduler.hero_energy, energy);
            assert_eq!(sim.rng.next_u64(), rng.next_u64());
            assert_eq!(sim.equipment.worn(ArmorSlot::Suit), Some(suit));
            assert_eq!(sim.compute_hero_ac(), 8);
            assert_eq!(
                sim.arena.items.get(suit).unwrap().location,
                ItemLocation::CarriedBy(sim.player_id)
            );
            assert!(!events.iter().any(|e| matches!(
                e,
                GameEvent::ItemDropped { .. }
                    | GameEvent::AttackLanded { .. }
                    | GameEvent::DressingStarted { .. }
            )));
            assert!(sim.equipment.dressing.is_none());
        }
    }
}
#[test]
fn wearing_clears_weapon_and_preexisting_quiver_then_fire_cannot_eject_the_suit() {
    let mut sim = world();
    let suit = add(&mut sim, ItemKindId::LEATHER_ARMOR);
    sim.step_player_action(ActionAst::Wield(index(&sim, suit)));
    sim.step_player_action(ActionAst::Quiver(suit));
    assert_eq!(sim.wielded_item, Some(suit));
    assert_eq!(sim.hero.quivered_item, Some(suit));
    wear(&mut sim, suit);
    complete(&mut sim);
    assert_eq!(sim.wielded_item, None);
    assert_eq!(sim.hero.quivered_item, None);
    let turn = sim.scheduler.turn;
    let mut rng = sim.rng.clone();
    sim.step_player_action(ActionAst::Fire(nethacked_sim::Direction::East));
    assert_eq!(sim.scheduler.turn, turn);
    assert_eq!(sim.rng.next_u64(), rng.next_u64());
    assert_eq!(sim.equipment.worn(ArmorSlot::Suit), Some(suit));
    assert_eq!(
        sim.arena.items.get(suit).unwrap().location,
        ItemLocation::CarriedBy(sim.player_id)
    );
}

fn alter_thief(sim: &mut SimulationWorld, multi: bool, animal: bool) {
    let rs = Arc::clone(&sim.ruleset);
    let mut monsters = rs.monsters.clone();
    let def = monsters
        .iter_mut()
        .find(|def| def.name == "wood nymph")
        .unwrap();
    def.flags.retain(|flag| flag != "animal");
    if animal {
        def.flags.push("animal".into());
    }
    if multi {
        def.attacks.extend([
            Attack {
                at: AttackType::Claw,
                ad: DamageType::Phys,
                n: 10,
                d: 10,
            },
            Attack {
                at: AttackType::Claw,
                ad: DamageType::StealItem,
                n: 0,
                d: 0,
            },
            Attack {
                at: AttackType::Magic,
                ad: DamageType::Spell,
                n: 0,
                d: 0,
            },
        ]);
    }
    sim.ruleset = Arc::new(Ruleset::from_parts(
        rs.manifest.clone(),
        monsters,
        rs.items.clone(),
        rs.roles.clone(),
        rs.races.clone(),
        rs.mechanics.clone(),
    ));
}
fn open_terrain(sim: &mut SimulationWorld) {
    for x in 0..80 {
        for y in 0..21 {
            sim.level.set_tile(Coord::new(x, y).unwrap(), Tile::Room);
        }
    }
    sim.arena.actors.get_mut(sim.player_id).unwrap().coord = Coord::new(10, 10).unwrap();
}

#[test]
fn high_mc_does_not_block_theft_and_source_draws_exclude_covered_cloak() {
    let mut sim = world();
    let suit = add(&mut sim, ItemKindId::LEATHER_ARMOR);
    let loot = add(&mut sim, ItemKindId::DAGGER);
    wear(&mut sim, suit);
    let rs = Arc::clone(&sim.ruleset);
    let mut items = rs.items.clone();
    let cloak = items
        .iter_mut()
        .find(|def| def.id == Some(ItemKindId::CLOAK_OF_PROTECTION))
        .unwrap();
    cloak.armor.as_mut().unwrap().base_ac = 0;
    sim.ruleset = Arc::new(Ruleset::from_parts(
        rs.manifest.clone(),
        rs.monsters.clone(),
        items,
        rs.roles.clone(),
        rs.races.clone(),
        rs.mechanics.clone(),
    ));
    let cloak = add(&mut sim, ItemKindId::CLOAK_OF_PROTECTION);
    sim.equipment.armor.push((ArmorSlot::Cloak, cloak));
    assert_eq!(
        sim.ruleset
            .item("cloak of protection")
            .unwrap()
            .armor
            .as_ref()
            .unwrap()
            .magic_cancellation,
        3
    );
    let id = thief(&mut sim);
    sim.rng = ChaCha8Rng::seed_from_u64(7);
    let mut expected = sim.rng.clone();
    let _hit = expected.random_range(1..=20u32);
    assert_eq!(expected.random_range(0..6u32), 0); // worn suit5+unworn loot1; cloak excluded
    let events = sim.step_player_action(ActionAst::Wait);
    assert!(events.contains(&GameEvent::ItemStolen {
        actor: id,
        item: loot
    }));
    assert!(events.contains(&GameEvent::DressingInterrupted {
        item: suit,
        putting_on: true
    }));
    assert_eq!(sim.rng.next_u64(), expected.next_u64());
    assert!(sim.arena.actors.get(id).unwrap().is_fleeing);
    assert_eq!(sim.equipment.worn(ArmorSlot::Cloak), Some(cloak));
    assert_eq!(sim.equipment.worn(ArmorSlot::Suit), None);
}

#[test]
fn successful_theft_ends_remaining_attacks_and_queued_magic_then_really_flees() {
    use nethacked_types::{MonsterAbility, MonsterSpell};
    let mut sim = world();
    open_terrain(&mut sim);
    add(&mut sim, ItemKindId::DAGGER);
    add(&mut sim, ItemKindId::FOOD_RATION);
    let id = thief(&mut sim);
    alter_thief(&mut sim, true, false);
    sim.arena.actors.get_mut(id).unwrap().abilities = vec![MonsterAbility::Spellcaster {
        spell: MonsterSpell::CurseItems,
        cooldown_turns: 1,
    }];
    sim.rng = ChaCha8Rng::seed_from_u64(7);
    let mut expected = sim.rng.clone();
    expected.random_range(1..=20u32);
    expected.random_range(0..2u32);
    let hp = sim.arena.actors.get(sim.player_id).unwrap().hp;
    let events = sim.step_player_action(ActionAst::Wait);
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, GameEvent::ItemStolen { .. }))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, GameEvent::AttackLanded { .. }))
            .count(),
        1
    );
    assert!(!events
        .iter()
        .any(|e| matches!(e,GameEvent::LogMessage{text} if text.contains("incantation"))));
    assert_eq!(sim.arena.actors.get(sim.player_id).unwrap().hp, hp);
    assert_eq!(sim.rng.next_u64(), expected.next_u64());
    let actor = sim.arena.actors.get_mut(id).unwrap();
    assert!(actor.is_fleeing);
    actor.hp = actor.max_hp - 1; // C still draws rn2(40), but skips rn2(25)
    let from = actor.coord;
    let distance = from.chebyshev_distance(sim.hero_coord());
    let mut expected = sim.rng.clone();
    expected.random_range(0..40u32);
    sim.arena.actors[id].movement = 12;
    sim.scheduler.hero_energy = 24;
    let events = sim.step_player_action(ActionAst::Wait);
    assert!(
        sim.arena
            .actors
            .get(id)
            .unwrap()
            .coord
            .chebyshev_distance(sim.hero_coord())
            > distance
    );
    assert!(events
        .iter()
        .any(|e| matches!(e,GameEvent::ActorMoved{actor,..} if *actor==id)));
    assert!(!events.iter().any(|e| matches!(
        e,
        GameEvent::ItemStolen { .. } | GameEvent::AttackLanded { .. }
    )));
    assert_eq!(sim.rng.next_u64(), expected.next_u64());
}

#[test]
fn ordinary_nymph_teleports_with_source_xy_draws_but_animal_or_restricted_level_does_not() {
    for (animal, restricted, teleports) in [
        (false, false, true),
        (false, true, false),
        (true, false, false),
    ] {
        let mut sim = world();
        open_terrain(&mut sim);
        add(&mut sim, ItemKindId::DAGGER);
        add(&mut sim, ItemKindId::FOOD_RATION);
        let id = thief(&mut sim);
        alter_thief(&mut sim, false, animal);
        sim.level.no_teleport = restricted;
        sim.rng = ChaCha8Rng::seed_from_u64(7);
        let mut expected = sim.rng.clone();
        expected.random_range(1..=20u32);
        expected.random_range(0..2u32);
        let from = sim.arena.actors.get(id).unwrap().coord;
        let to = if teleports {
            Coord::new(
                expected.random_range(1..80usize),
                expected.random_range(0..21usize),
            )
            .unwrap()
        } else {
            from
        };
        assert_ne!(to, sim.hero_coord());
        let events = sim.step_player_action(ActionAst::Wait);
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, GameEvent::ItemStolen { .. }))
                .count(),
            1
        );
        assert_eq!(sim.arena.actors.get(id).unwrap().coord, to);
        assert!(sim.arena.actors.get(id).unwrap().is_fleeing);
        assert_eq!(
            events
                .iter()
                .any(|e| matches!(e,GameEvent::ActorMoved{actor,..}if *actor==id)),
            teleports
        );
        assert_eq!(sim.rng.next_u64(), expected.next_u64());
    }
}

#[test]
fn fleeing_and_restriction_owner_old_load_defaults_and_save_continuation() {
    let mut sim = world();
    open_terrain(&mut sim);
    let id = thief(&mut sim);
    sim.arena.actors.get_mut(id).unwrap().is_fleeing = true;
    sim.arena.actors.get_mut(id).unwrap().hp -= 1;
    sim.level.no_teleport = true;
    let saved = sim.to_save_json().unwrap();
    let mut loaded =
        SimulationWorld::from_save_json(&saved, Arc::clone(&sim.ruleset), &sim.ruleset_ref)
            .unwrap();
    assert!(loaded.level.no_teleport);
    assert!(loaded.arena.actors.get(id).unwrap().is_fleeing);
    assert_eq!(
        sim.step_player_action(ActionAst::Wait),
        loaded.step_player_action(ActionAst::Wait)
    );
    assert_eq!(sim.rng.next_u64(), loaded.rng.next_u64());
    let mut value: serde_json::Value = serde_json::from_str(&saved).unwrap();
    fn remove_new(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.remove("is_fleeing");
                map.remove("is_cancelled");
                map.remove("no_teleport");
                for child in map.values_mut() {
                    remove_new(child);
                }
            }
            serde_json::Value::Array(array) => {
                for child in array {
                    remove_new(child);
                }
            }
            _ => {}
        }
    }
    remove_new(&mut value);
    let old = SimulationWorld::from_save_json(
        &value.to_string(),
        Arc::clone(&sim.ruleset),
        &sim.ruleset_ref,
    )
    .unwrap();
    assert!(!old.level.no_teleport);
    assert!(!old.arena.actors.get(id).unwrap().is_fleeing);
    assert!(!old.arena.actors.get(id).unwrap().is_cancelled);
}

#[test]
fn cancelled_animal_theft_uses_no_inventory_or_armor_mc_draw() {
    let mut sim = world();
    add(&mut sim, ItemKindId::DAGGER);
    let id = thief(&mut sim);
    alter_thief(&mut sim, false, true);
    sim.arena.actors.get_mut(id).unwrap().is_cancelled = true;
    sim.rng = ChaCha8Rng::seed_from_u64(7);
    let mut expected = sim.rng.clone();
    expected.random_range(1..=20u32);
    let events = sim.step_player_action(ActionAst::Wait);
    assert!(!events
        .iter()
        .any(|e| matches!(e, GameEvent::ItemStolen { .. })));
    assert_eq!(sim.rng.next_u64(), expected.next_u64());
    assert!(!sim.arena.actors.get(id).unwrap().is_fleeing);
}

fn fleeing_fixture(
    seed: u64,
    tport: bool,
    restricted: bool,
    injured: bool,
) -> (SimulationWorld, nethacked_arena::ActorId) {
    let mut sim = world();
    open_terrain(&mut sim);
    let id = thief(&mut sim);
    let rs = Arc::clone(&sim.ruleset);
    let mut monsters = rs.monsters.clone();
    let def = monsters
        .iter_mut()
        .find(|def| def.name == "wood nymph")
        .unwrap();
    if !tport {
        def.flags.retain(|flag| flag != "tport");
    }
    sim.ruleset = Arc::new(Ruleset::from_parts(
        rs.manifest.clone(),
        monsters,
        rs.items.clone(),
        rs.roles.clone(),
        rs.races.clone(),
        rs.mechanics.clone(),
    ));
    let mon = sim.arena.actors.get_mut(id).unwrap();
    mon.coord = Coord::new(20, 10).unwrap();
    mon.is_fleeing = true;
    if injured {
        mon.hp -= 1;
    }
    sim.level.no_teleport = restricted;
    sim.rng = ChaCha8Rng::seed_from_u64(seed);
    (sim, id)
}

#[test]
fn fleeing_source_short_circuit_chance_restriction_and_courage_order() {
    // monmove.c:744-749 precedes 757-760. Each row prescribes C draws;
    // full-HP courage0 resumes pursuit; injured/courage-nonzero retreats.
    for (seed, tport, restricted, injured, chance, courage, retreat) in [
        (27, false, false, true, 0, None, true),
        (27, true, true, true, 0, None, true),
        (12, true, false, true, 2, None, true),
        (27, false, false, false, 0, Some(16), true),
        (27, true, true, false, 0, Some(16), true),
        (12, true, false, false, 2, Some(0), false),
    ] {
        let (mut sim, id) = fleeing_fixture(seed, tport, restricted, injured);
        let from = sim.arena.actors.get(id).unwrap().coord;
        let mut expected = sim.rng.clone();
        assert_eq!(expected.random_range(0..40u32), chance);
        if let Some(courage) = courage {
            assert_eq!(expected.random_range(0..25u32), courage);
        }
        let events = sim.step_player_action(ActionAst::Wait);
        let actor = sim.arena.actors.get(id).unwrap();
        assert_eq!(actor.is_fleeing, retreat);
        assert_eq!(
            actor.coord.chebyshev_distance(sim.hero_coord())
                > from.chebyshev_distance(sim.hero_coord()),
            retreat
        );
        assert!(events
            .iter()
            .any(|event| matches!(event,GameEvent::ActorMoved{actor,..}if *actor==id)));
        assert!(!events.iter().any(|event| matches!(
            event,
            GameEvent::AttackLanded { .. } | GameEvent::ItemStolen { .. }
        )));
        assert_eq!(sim.rng.next_u64(), expected.next_u64());
    }
}

#[test]
fn fleeing_nymph_teleport_chance_consumes_turn_before_courage_or_retreat() {
    for injured in [true, false] {
        let (mut sim, id) = fleeing_fixture(27, true, false, injured);
        let mut expected = sim.rng.clone();
        assert_eq!(expected.random_range(0..40u32), 0);
        let to = Coord::new(
            expected.random_range(1..80usize),
            expected.random_range(0..21usize),
        )
        .unwrap();
        assert_eq!(to, Coord::new(53, 0).unwrap());
        assert_ne!(to, sim.hero_coord());
        let events = sim.step_player_action(ActionAst::Wait);
        assert_eq!(sim.arena.actors.get(id).unwrap().coord, to);
        assert!(sim.arena.actors.get(id).unwrap().is_fleeing);
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e,GameEvent::ActorMoved{actor,..}if *actor==id))
                .count(),
            1
        );
        assert!(!events.iter().any(|event| matches!(
            event,
            GameEvent::AttackLanded { .. } | GameEvent::ItemStolen { .. }
        )));
        assert_eq!(sim.rng.next_u64(), expected.next_u64());
    }
}

#[test]
fn teleport_fallback_keeps_hero_in_literal_candidate_shuffle_then_rejects_it() {
    // teleport.c:1863-1894: 50 failed rnd79/rn2(21) attempts, then
    // collect_coords orders [destination <40,10>, hero <41,10>];
    // m_at excludes the current monster <39,10>. Shuffle1 selects hero first.
    let (mut sim, id) = fleeing_fixture(27, true, false, true);
    for x in 0..80 {
        for y in 0..21 {
            sim.level.set_tile(Coord::new(x, y).unwrap(), Tile::Stone);
        }
    }
    let hero = Coord::new(41, 10).unwrap();
    let from = Coord::new(39, 10).unwrap();
    let to = Coord::new(40, 10).unwrap();
    sim.arena.actors.get_mut(sim.player_id).unwrap().coord = hero;
    sim.arena.actors.get_mut(id).unwrap().coord = from;
    for coord in [hero, from, to] {
        sim.level.set_tile(coord, Tile::Room);
    }
    let mut expected = sim.rng.clone();
    assert_eq!(expected.random_range(0..40u32), 0);
    for _ in 0..50 {
        let xy = Coord::new(
            expected.random_range(1..80usize),
            expected.random_range(0..21usize),
        )
        .unwrap();
        assert!(xy != from && xy != to); // wall or hero: no valid random destination
    }
    assert_eq!(expected.random_range(0..2usize), 1); // hero rejected by goodpos
    assert_eq!(expected.random_range(0..1usize), 0); // remaining destination
    assert_eq!(expected.clone().next_u64(), 12_943_007_396_277_832_523);
    let events = sim.step_player_action(ActionAst::Wait);
    assert_eq!(sim.arena.actors.get(id).unwrap().coord, to);
    assert!(events.contains(&GameEvent::ActorMoved {
        actor: id,
        from,
        to
    }));
    assert_eq!(sim.rng.next_u64(), expected.next_u64());
}
