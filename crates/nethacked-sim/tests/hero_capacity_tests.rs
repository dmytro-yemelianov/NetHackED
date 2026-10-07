//! Released5.0 source-only expectations: attrib.c1245-1265,
//! hack.c2616-2635/4295-4395, pickup.c1583-1609, mkobj.c1947-1955.
use nethacked_arena::{ItemId, ItemLocation};
use nethacked_core::inventory::EncumbranceTier::*;
use nethacked_data::{ItemKindId, Ruleset};
use nethacked_sim::{ActionAst, Coord, Direction, GameEvent, SimulationWorld, Tile};
use nethacked_types::Buc;
use rand::RngCore;
use std::sync::Arc;

const HERE: Coord = Coord::new_unchecked(10, 10);
const EAST: Coord = Coord::new_unchecked(11, 10);

fn assert_same_state(world: &SimulationWorld, wait: &SimulationWorld, refusal: &str) {
    let refusal_event = GameEvent::LogMessage {
        text: refusal.into(),
    };
    assert_eq!(
        world
            .event_log
            .iter()
            .filter(|event| **event == refusal_event)
            .count(),
        1
    );
    let remaining_events: Vec<_> = world
        .event_log
        .iter()
        .filter(|event| **event != refusal_event)
        .cloned()
        .collect();
    assert_eq!(remaining_events, wait.event_log);
    let actual = serde_json::to_value(world).unwrap();
    let mut expected = serde_json::to_value(wait).unwrap();
    // The refused action contributes precisely its verified message above;
    // all other serialized state must match the normal-action Wait control.
    expected["event_log"] = actual["event_log"].clone();
    for (key, value) in actual.as_object().unwrap() {
        assert!(value == &expected[key], "state field differs: {key}");
    }
}

fn world() -> SimulationWorld {
    let mut world = SimulationWorld::new_with_seed(413);
    world.arena.actors.retain(|id, _| id == world.player_id);
    world.arena.items.clear();
    world.wielded_item = None;
    world.arena.actors.get_mut(world.player_id).unwrap().coord = HERE;
    world.level.set_tile(HERE, Tile::Room);
    world.level.set_tile(EAST, Tile::Room);
    world.progress.attr_base[0] = 100;
    world.progress.attr_base[4] = 10;
    world
}

fn owned_item(world: &mut SimulationWorld, weight: u32, location: ItemLocation) -> ItemId {
    let mut rules = (*Ruleset::vanilla()).clone();
    let def = rules
        .items
        .iter_mut()
        .find(|d| d.id == Some(ItemKindId::DAGGER))
        .unwrap();
    def.weight = weight;
    rules.reindex();
    world.ruleset = Arc::new(rules);
    let item = world
        .ruleset
        .create_item_record_by_id(ItemKindId::DAGGER, location, Buc::Uncursed)
        .unwrap();
    assert_eq!(item.weight, weight);
    world.arena.spawn_item(item)
}

#[test]
fn public_capacity_uses_encoded_strength_constitution_and_levitation() {
    let mut world = world();
    for (strength, con, expected) in [
        (49, 10, 775),
        (50, 10, 800),
        (99, 10, 800),
        (100, 10, 825),
        (121, 10, 825),
        (122, 10, 850),
        (125, 25, 1000),
        (3, 3, 200),
        (3, 0, 200),
    ] {
        world.progress.attr_base[0] = strength;
        world.progress.attr_base[4] = con;
        assert_eq!(world.hero_weight_capacity(), expected);
    }
    world
        .arena
        .actors
        .get_mut(world.player_id)
        .unwrap()
        .intrinsics
        .levitation = true;
    assert_eq!(world.hero_weight_capacity(), 1000);
}

#[test]
fn public_owned_pickup_drop_and_exact_lifting_boundary() {
    for (weight, accepted, tier) in [
        (825, true, Unencumbered),
        (826, true, Burdened),
        (1238, true, Stressed),
        (1650, true, Strained),
        (2063, true, Overtaxed),
        (2474, true, Overtaxed),
        (2475, false, Unencumbered),
        (u32::MAX, false, Unencumbered),
    ] {
        let mut world = world();
        let item = owned_item(&mut world, weight, ItemLocation::Floor(HERE));
        let mut expected_rng = world.rng.clone();
        let mut wait: SimulationWorld =
            serde_json::from_value(serde_json::to_value(&world).unwrap()).unwrap();
        wait.step_player_action(ActionAst::Wait);
        let turn = world.scheduler.turn;
        let events = world.step_player_action(ActionAst::PickUp);
        assert_eq!(
            events
                .iter()
                .any(|e| matches!(e, GameEvent::ItemPickedUp { .. })),
            accepted
        );
        assert_eq!(
            world.hero_carried_weight(),
            if accepted { weight } else { 0 }
        );
        assert_eq!(world.hero_encumbrance(), tier);
        if !accepted {
            assert_eq!(world.scheduler.turn, wait.scheduler.turn);
            assert_eq!(world.scheduler.hero_energy, wait.scheduler.hero_energy);
            assert_same_state(&world, &wait, "You are carrying too much to pick that up.");
        }
        if accepted {
            assert_eq!(world.rng.next_u64(), expected_rng.next_u64());
            assert!(world.scheduler.turn > turn);
            world.step_player_action(ActionAst::Drop(0));
            assert_eq!(world.hero_carried_weight(), 0);
        } else {
            assert!(world.scheduler.turn > turn);
            assert_eq!(world.rng.next_u64(), wait.rng.next_u64());
        }
        assert_eq!(
            world.arena.items.get(item).unwrap().location,
            ItemLocation::Floor(HERE)
        );
    }
}

#[test]
fn overloaded_public_movement_blocks_and_drop_restores_movement_without_rng() {
    let mut world = world();
    let hero = world.player_id;
    owned_item(&mut world, 2475, ItemLocation::CarriedBy(hero));
    let mut wait: SimulationWorld =
        serde_json::from_value(serde_json::to_value(&world).unwrap()).unwrap();
    wait.step_player_action(ActionAst::Wait);
    let events = world.step_player_action(ActionAst::Move(Direction::East));
    assert!(events.iter().any(
        |e| matches!(e, GameEvent::LogMessage{text} if text == "You collapse under your load.")
    ));
    assert!(!events.iter().any(|e| matches!(
        e,
        GameEvent::ActorMoved { .. }
            | GameEvent::AttackLanded { .. }
            | GameEvent::AttackMissed { .. }
    )));
    assert_eq!(world.arena.actors.get(hero).unwrap().coord, HERE);
    assert_eq!(world.scheduler.turn, wait.scheduler.turn);
    assert_eq!(world.scheduler.hero_energy, wait.scheduler.hero_energy);
    assert_eq!(world.rng.next_u64(), wait.rng.next_u64());
    world.progress.attr_base[0] = 125;
    assert_eq!(world.hero_encumbrance(), Overtaxed);
    world.step_player_action(ActionAst::Move(Direction::East));
    assert_eq!(world.arena.actors.get(hero).unwrap().coord, EAST);
    world.step_player_action(ActionAst::Drop(0));
    assert_eq!(world.hero_encumbrance(), Unencumbered);
}

#[test]
fn container_actions_recompute_nested_bag_weights_and_save_roundtrip() {
    for (buc, expected) in [(Buc::Blessed, 22), (Buc::Uncursed, 28), (Buc::Cursed, 67)] {
        let mut world = world();
        let hero = world.player_id;
        let bag = world.arena.spawn_item(
            world
                .ruleset
                .create_item_record_by_id(
                    ItemKindId::BAG_OF_HOLDING,
                    ItemLocation::CarriedBy(hero),
                    buc,
                )
                .unwrap(),
        );
        let sack = world.arena.spawn_item(
            world
                .ruleset
                .create_item_record_by_id(
                    ItemKindId::SACK,
                    ItemLocation::CarriedBy(hero),
                    Buc::Uncursed,
                )
                .unwrap(),
        );
        owned_item(&mut world, 11, ItemLocation::InContainer(sack));
        assert_eq!(world.hero_carried_weight(), 41);
        let carried = world.arena.items_carried_by(hero);
        world.step_player_action(ActionAst::PutInContainer {
            item_index: carried.iter().position(|id| *id == sack).unwrap(),
            container_index: carried.iter().position(|id| *id == bag).unwrap(),
        });
        assert_eq!(world.hero_carried_weight(), expected);
        let mut restored: SimulationWorld =
            serde_json::from_str(&serde_json::to_string(&world).unwrap()).unwrap();
        assert_eq!(restored.hero_carried_weight(), expected);
        assert_eq!(restored.hero_encumbrance(), world.hero_encumbrance());
        assert_eq!(restored.rng.next_u64(), world.rng.next_u64());
        world.step_player_action(ActionAst::TakeFromContainer {
            container_index: 0,
            item_index: 0,
        });
        assert_eq!(world.hero_carried_weight(), 41);
        assert_eq!(
            world.arena.items.get(bag).unwrap().location,
            ItemLocation::CarriedBy(hero)
        );
    }
}

#[test]
fn container_removal_refusal_projects_weight_and_restores_parent() {
    let mut world = world();
    let hero = world.player_id;
    let bag = world.arena.spawn_item(
        world
            .ruleset
            .create_item_record_by_id(
                ItemKindId::BAG_OF_HOLDING,
                ItemLocation::CarriedBy(hero),
                Buc::Blessed,
            )
            .unwrap(),
    );
    let item = owned_item(&mut world, 2460, ItemLocation::InContainer(bag));
    assert_eq!(world.hero_carried_weight(), 630);
    let mut wait: SimulationWorld =
        serde_json::from_value(serde_json::to_value(&world).unwrap()).unwrap();
    wait.step_player_action(ActionAst::Wait);
    world.step_player_action(ActionAst::TakeFromContainer {
        container_index: 0,
        item_index: 0,
    });
    assert_eq!(
        world.arena.items.get(item).unwrap().location,
        ItemLocation::InContainer(bag)
    );
    assert_eq!(world.hero_carried_weight(), 630);
    assert_eq!(world.scheduler.turn, wait.scheduler.turn);
    assert_eq!(world.scheduler.hero_energy, wait.scheduler.hero_energy);
    assert_same_state(
        &world,
        &wait,
        "You are carrying too much to remove that item.",
    );
    assert_eq!(world.rng.next_u64(), wait.rng.next_u64());
    world.progress.attr_base[4] = 11;
    world.step_player_action(ActionAst::TakeFromContainer {
        container_index: 0,
        item_index: 0,
    });
    assert_eq!(world.hero_carried_weight(), 2475);
    assert_eq!(
        world.arena.items.get(item).unwrap().location,
        ItemLocation::CarriedBy(hero)
    );
}

#[test]
fn recursive_contents_and_root_overflow_saturate() {
    let mut world = world();
    let hero = world.player_id;
    let sack = world.arena.spawn_item(
        world
            .ruleset
            .create_item_record_by_id(
                ItemKindId::SACK,
                ItemLocation::CarriedBy(hero),
                Buc::Uncursed,
            )
            .unwrap(),
    );
    owned_item(&mut world, u32::MAX, ItemLocation::InContainer(sack));
    owned_item(&mut world, 1, ItemLocation::InContainer(sack));
    owned_item(&mut world, 1, ItemLocation::CarriedBy(hero));
    assert_eq!(world.hero_carried_weight(), u32::MAX);
    assert_eq!(world.hero_encumbrance(), Overloaded);
}

#[test]
fn custom_owned_pack_weight_blocks_pickup_after_display_rename() {
    let mut world = world();
    let mut rules = (*Ruleset::vanilla()).clone();
    let mut def = rules.item_by_id(ItemKindId::DAGGER).unwrap().clone();
    def.id = None;
    def.name = "Pack ballast".into();
    def.weight = 2475;
    rules.items.push(def);
    rules.reindex();
    world.ruleset = Arc::new(rules);
    let mut record = world
        .ruleset
        .create_item_record("Pack ballast", ItemLocation::Floor(HERE), Buc::Uncursed)
        .unwrap();
    assert_eq!(record.weight, 2475);
    record.name = "dagger".into();
    let item = world.arena.spawn_item(record);
    world.step_player_action(ActionAst::PickUp);
    assert_eq!(
        world.arena.items.get(item).unwrap().location,
        ItemLocation::Floor(HERE)
    );
    assert_eq!(world.hero_carried_weight(), 0);
}

#[test]
fn overloaded_move_cannot_bypass_through_bump_or_pet_displacement() {
    for tame in [false, true] {
        let mut world = world();
        let hero = world.player_id;
        owned_item(&mut world, 2475, ItemLocation::CarriedBy(hero));
        let target = world.spawn_monster_by_name("giant ant", EAST).unwrap();
        let actor = world.arena.actors.get_mut(target).unwrap();
        actor.is_tame = tame;
        actor.speed = 0;
        let hp = actor.hp;
        world.step_player_action(ActionAst::Move(Direction::East));
        assert_eq!(world.arena.actors.get(hero).unwrap().coord, HERE);
        assert_eq!(world.arena.actors.get(target).unwrap().coord, EAST);
        assert_eq!(world.arena.actors.get(target).unwrap().hp, hp);
    }
}

#[test]
fn overloaded_impaired_move_matches_wait_without_direction_rng() {
    for (confused, stunned) in [(100, 0), (0, 100), (100, 100)] {
        let mut world = world();
        let hero = world.player_id;
        owned_item(&mut world, 2475, ItemLocation::CarriedBy(hero));
        world.hero.afflictions.transient.confused = confused;
        world.hero.afflictions.transient.stunned = stunned;
        let mut wait: SimulationWorld =
            serde_json::from_value(serde_json::to_value(&world).unwrap()).unwrap();
        wait.step_player_action(ActionAst::Wait);
        let events = world.step_player_action(ActionAst::Move(Direction::East));
        assert!(events.iter().any(|event| matches!(event, GameEvent::LogMessage {text} if text == "You collapse under your load.")));
        assert_eq!(world.arena.actors.get(hero).unwrap().coord, HERE);
        assert_eq!(world.scheduler.turn, wait.scheduler.turn);
        assert_eq!(world.scheduler.hero_energy, wait.scheduler.hero_energy);
        assert_same_state(&world, &wait, "You collapse under your load.");
        assert_eq!(world.rng.next_u64(), wait.rng.next_u64());
    }
}
