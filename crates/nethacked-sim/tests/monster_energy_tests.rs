//! P09 literal/native fixtures from released C 16ff5911, not production helpers.
use nethacked_arena::{ActorId, ItemLocation};
use nethacked_core::energy::{hero_movement_gain, monster_movement_gain};
use nethacked_core::inventory::EncumbranceTier as Tier;
use nethacked_sim::{
    ActionAst, Buc, Coord, Direction, GameEvent, ItemKindId, SimulationWorld, Tile,
};
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;

fn world() -> SimulationWorld {
    let mut w = SimulationWorld::new_with_seed(42);
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.arena.items = Default::default();
    w.equipment = Default::default();
    w.wielded_item = None;
    w.unpaid_items.clear();
    w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Room);
    w.arena.actors[w.player_id].coord = Coord::new_unchecked(5, 10);
    w.progress.attr_base = [16, 12, 12, 12, 15, 12];
    w.progress.attr_max = w.progress.attr_base;
    w.scheduler.hero_energy = 12;
    w.scheduler.hero_speed = 12;
    w.rng = ChaCha8Rng::from_seed([0; 32]);
    w
}
fn monster(w: &mut SimulationWorld, speed: u32, x: usize, y: usize) -> ActorId {
    let id = w
        .spawn_monster_by_name("human", Coord::new_unchecked(x, y))
        .unwrap();
    let a = &mut w.arena.actors[id];
    a.speed = speed;
    a.is_peaceful = false;
    a.is_tame = false;
    a.abilities.clear();
    assert_eq!(a.movement, 0);
    w.rng = ChaCha8Rng::from_seed([0; 32]);
    id
}
fn moved(events: &[GameEvent], actor: ActorId) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, GameEvent::ActorMoved { actor: a, .. } if *a == actor))
        .count()
}
fn waits(w: &mut SimulationWorld, id: ActorId, n: usize) -> (Vec<u64>, Vec<usize>) {
    let mut turns = vec![];
    let mut actions = vec![];
    for _ in 0..n {
        let before = w.scheduler.turn;
        let e = w.step_player_action(ActionAst::Wait);
        turns.push(w.scheduler.turn - before);
        actions.push(moved(&e, id));
    }
    (turns, actions)
}
fn load(w: &SimulationWorld) -> SimulationWorld {
    SimulationWorld::from_save_json(
        &w.to_save_json().unwrap(),
        w.ruleset.clone(),
        &w.ruleset_ref,
    )
    .unwrap()
}
fn weight(w: &mut SimulationWorld, amount: u32) -> nethacked_arena::ItemId {
    let mut item = w
        .ruleset
        .create_item_record_by_id(
            ItemKindId::DAGGER,
            ItemLocation::CarriedBy(w.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    item.weight = amount;
    w.arena.spawn_item(item)
}
#[test]
fn prescribed_c_rounding_and_burden_subtraction() {
    for (speed, draw, expected) in [
        (0, 0, 0),
        (0, 11, 0),
        (1, 0, 12),
        (1, 1, 0),
        (6, 5, 12),
        (6, 6, 0),
        (11, 10, 12),
        (11, 11, 0),
        (12, 0, 12),
        (12, 11, 12),
        (13, 0, 24),
        (13, 1, 12),
        (23, 10, 24),
        (23, 11, 12),
        (24, 11, 24),
    ] {
        let mut bounds = vec![];
        assert_eq!(
            monster_movement_gain(speed, |bound| {
                bounds.push(bound);
                draw
            }),
            expected
        );
        assert_eq!(bounds, [12]);
    }
    for (gain, expected) in [
        (12, [12, 9, 6, 3, 2, 12]),
        (13, [13, 10, 7, 4, 2, 13]),
        (24, [24, 18, 12, 6, 3, 24]),
    ] {
        for (tier, result) in [
            Tier::Unencumbered,
            Tier::Burdened,
            Tier::Stressed,
            Tier::Strained,
            Tier::Overtaxed,
            Tier::Overloaded,
        ]
        .into_iter()
        .zip(expected)
        {
            assert_eq!(
                hero_movement_gain(gain, false, false, tier, |_| panic!(
                    "ordinary hero does not draw"
                )),
                result
            );
        }
    }
}
#[test]
fn public_native_speed_counts_and_unconditional_allocation_draws() {
    for (speed, expected) in [
        (6, [0, 1, 0, 0, 0, 0]),
        (12, [0, 1, 1, 1, 1, 1]),
        (24, [0, 2, 2, 2, 2, 2]),
    ] {
        let mut w = world();
        let id = monster(&mut w, speed, 70, 10);
        assert_eq!(waits(&mut w, id, 6), (vec![1; 6], expected.to_vec()));
    }
    for (speed, credit) in [
        (0, 0),
        (1, 0),
        (6, 12),
        (11, 12),
        (12, 12),
        (13, 12),
        (23, 24),
        (24, 24),
    ] {
        let mut w = world();
        let id = monster(&mut w, speed, 70, 10);
        assert_eq!(moved(&w.step_player_action(ActionAst::Wait), id), 0);
        assert_eq!(w.arena.actors[id].movement, credit);
        assert_eq!(w.rng.next_u64(), 0xe8b85b7fd6405f89);
    }
}
#[test]
fn public_credit_thresholds_and_snapshot_pass_order() {
    for (credit, count, remainder) in [
        (0, 0, 0),
        (11, 0, 11),
        (12, 1, 0),
        (23, 1, 11),
        (24, 2, 0),
        (36, 3, 0),
    ] {
        let mut w = world();
        let id = monster(&mut w, 0, 70, 10);
        w.arena.actors[id].movement = credit;
        assert_eq!(moved(&w.step_player_action(ActionAst::Wait), id), count);
        assert_eq!(w.arena.actors[id].movement, remainder);
        assert_eq!(w.scheduler.turn, 2);
    }
    let mut w = world();
    let a = monster(&mut w, 24, 70, 5);
    let b = monster(&mut w, 24, 70, 15);
    w.step_player_action(ActionAst::Wait);
    let e = w.step_player_action(ActionAst::Wait);
    let order: Vec<_> = e
        .iter()
        .filter_map(|e| match e {
            GameEvent::ActorMoved { actor, .. } => Some(*actor),
            _ => None,
        })
        .collect();
    assert_eq!(order, [a, b, a, b]);
    assert_eq!(w.rng.next_u64(), 0x3b7f9acec30e842c);
}
#[test]
fn public_fast_slow_hero_and_live_speed_properties() {
    let mut w = world();
    let id = monster(&mut w, 24, 70, 10);
    w.scheduler.hero_speed = 24;
    w.scheduler.hero_energy = 24;
    assert_eq!(waits(&mut w, id, 4), (vec![0, 1, 0, 1], vec![0, 0, 1, 1]));
    let mut w = world();
    let id = monster(&mut w, 24, 70, 10);
    w.scheduler.hero_speed = 6;
    assert_eq!(waits(&mut w, id, 2), (vec![2, 2], vec![2, 4]));
    for (fast, very_fast, turns, energies) in [
        (true, false, [1, 0, 1, 1], [24, 12, 12, 12]),
        (false, true, [1, 1, 0, 1], [12, 24, 12, 24]),
        (true, true, [1, 1, 0, 1], [12, 24, 12, 24]),
    ] {
        let mut w = world();
        w.arena.actors[w.player_id].intrinsics.fast = fast;
        w.arena.actors[w.player_id].intrinsics.very_fast = very_fast;
        for (turn, gain) in turns.into_iter().zip(energies) {
            let before = w.scheduler.turn;
            w.step_player_action(ActionAst::Wait);
            assert_eq!(w.scheduler.turn - before, turn);
            assert_eq!(w.scheduler.hero_energy, gain);
        }
        assert_eq!(w.rng.next_u64(), 0xc30e842ca1a5091f);
    }
}
#[test]
fn public_live_inventory_six_tiers_clock_and_actions() {
    for (load, turns, actions) in [
        (825, [1, 1, 1, 1], [0, 1, 1, 1]),
        (826, [2, 1, 1, 2], [1, 1, 1, 2]),
        (1238, [2, 2, 2, 2], [1, 2, 2, 2]),
        (1650, [4, 4, 4, 4], [3, 4, 4, 4]),
        (2063, [6, 6, 6, 6], [5, 6, 6, 6]),
        (2475, [1, 1, 1, 1], [0, 1, 1, 1]),
    ] {
        let mut w = world();
        assert_eq!(w.hero_weight_capacity(), 825);
        weight(&mut w, load);
        let id = monster(&mut w, 12, 70, 10);
        let nutrition = w.player_nutrition;
        w.arena.actors[id].mspec_used = 30;
        assert_eq!(waits(&mut w, id, 4), (turns.to_vec(), actions.to_vec()));
        let clock: u64 = turns.iter().sum();
        assert_eq!(w.player_nutrition, nutrition - clock as i32);
        assert_eq!(w.arena.actors[id].mspec_used, 30 - clock as u8);
    }
}
#[test]
fn pickup_drop_attributes_and_nested_bag_change_actual_gain() {
    let mut w = world();
    let id = weight(&mut w, 1238);
    w.arena.items[id].location = ItemLocation::Floor(w.hero_coord());
    w.step_player_action(ActionAst::PickUp);
    assert_eq!(w.hero_encumbrance(), Tier::Stressed);
    assert_eq!(w.scheduler.turn, 3);
    w.step_player_action(ActionAst::Drop(0));
    assert_eq!(w.hero_encumbrance(), Tier::Unencumbered);
    assert_eq!(w.scheduler.turn, 4);
    w.step_player_action(ActionAst::PickUp);
    w.progress.attr_base[0] = 25;
    w.progress.attr_base[4] = 25;
    assert_eq!(w.hero_weight_capacity(), 1000);
    assert_eq!(w.hero_encumbrance(), Tier::Burdened);
    let mut w = world();
    let loot = weight(&mut w, 1238);
    let bag = w.arena.spawn_item(
        w.ruleset
            .create_item_record_by_id(
                ItemKindId::BAG_OF_HOLDING,
                ItemLocation::CarriedBy(w.player_id),
                Buc::Uncursed,
            )
            .unwrap(),
    );
    let idx = |w: &SimulationWorld, id| {
        w.arena
            .items_carried_by(w.player_id)
            .iter()
            .position(|x| *x == id)
            .unwrap()
    };
    w.step_player_action(ActionAst::PutInContainer {
        item_index: idx(&w, loot),
        container_index: idx(&w, bag),
    });
    assert_eq!(w.hero_carried_weight(), 634);
    assert_eq!(w.hero_encumbrance(), Tier::Unencumbered);
    let sack = w.arena.spawn_item(
        w.ruleset
            .create_item_record_by_id(
                ItemKindId::SACK,
                ItemLocation::CarriedBy(w.player_id),
                Buc::Uncursed,
            )
            .unwrap(),
    );
    w.step_player_action(ActionAst::PutInContainer {
        item_index: idx(&w, bag),
        container_index: idx(&w, sack),
    });
    assert_eq!(w.hero_carried_weight(), 649);
    assert_eq!(w.hero_encumbrance(), Tier::Unencumbered);
    let before = w.scheduler.turn;
    w.step_player_action(ActionAst::Wait);
    assert_eq!(w.scheduler.turn - before, 1);
}
#[test]
fn no_time_refusal_dead_actor_and_hero_death_stop_actions() {
    let mut w = world();
    let id = monster(&mut w, 24, 70, 10);
    w.arena.actors[id].movement = 24;
    let rng = w.rng.clone().next_u64();
    let turn = w.scheduler.turn;
    w.step_player_action(ActionAst::PriceCheck(999));
    assert_eq!(w.arena.actors[id].movement, 24);
    assert_eq!(w.scheduler.turn, turn);
    assert_eq!(w.rng.clone().next_u64(), rng);
    w.level.set_tile(Coord::new_unchecked(6, 10), Tile::Stone);
    w.step_player_action(ActionAst::Move(Direction::East));
    assert_eq!(w.arena.actors[id].movement, 24);
    assert_eq!(w.scheduler.turn, turn);
    w.arena.actors[id].is_dead = true;
    w.step_player_action(ActionAst::Wait);
    assert_eq!(w.arena.actors[id].movement, 24);
    let mut w = world();
    let first = monster(&mut w, 24, 6, 10);
    let later = monster(&mut w, 24, 70, 10);
    w.arena.actors[first].movement = 36;
    w.arena.actors[later].movement = 36;
    w.arena.actors[w.player_id].hp = 1;
    w.arena.actors[w.player_id].ac = 100;
    w.arena.actors[first].level = 100;
    let e = w.step_player_action(ActionAst::Wait);
    assert!(w.arena.actors[w.player_id].is_dead);
    assert_eq!(moved(&e, later), 0);
    assert_eq!(w.arena.actors[later].movement, 36);
    assert_eq!(w.scheduler.turn, 1);
}
#[test]
fn full_save_holes_credit_native_continuation_and_legacy_defaults() {
    let mut w = world();
    let hole = monster(&mut w, 1, 60, 5);
    let id = monster(&mut w, 13, 70, 10);
    w.arena.actors.remove(hole);
    w.arena.actors[id].movement = 23;
    w.scheduler.monster_energy = 999;
    w.scheduler.monster_speed = 999;
    let mut copy = load(&w);
    assert_eq!(copy.arena.actors[id].movement, 23);
    for _ in 0..4 {
        assert_eq!(
            w.step_player_action(ActionAst::Wait),
            copy.step_player_action(ActionAst::Wait)
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&w.to_save_json().unwrap()).unwrap(),
            serde_json::from_str::<serde_json::Value>(&copy.to_save_json().unwrap()).unwrap()
        );
    }
    assert_eq!(w.rng.next_u64(), copy.rng.next_u64());
    let mut value: serde_json::Value = serde_json::from_str(&w.to_save_json().unwrap()).unwrap();
    fn erase(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(o) => {
                o.remove("movement");
                for v in o.values_mut() {
                    erase(v);
                }
            }
            serde_json::Value::Array(a) => {
                for v in a {
                    erase(v);
                }
            }
            _ => {}
        }
    }
    erase(&mut value);
    let legacy =
        SimulationWorld::from_save_json(&value.to_string(), w.ruleset.clone(), &w.ruleset_ref)
            .unwrap();
    assert!(legacy.arena.actors.iter().all(|(_, a)| a.movement == 0));
    assert_eq!(legacy.rng.clone().next_u64(), w.rng.clone().next_u64());
}

#[test]
fn owned_custom_speed_is_snapshotted_and_constructors_never_allocate() {
    use nethacked_arena::ActorSex;
    use nethacked_data::monsters::MonsterSpawnContext;
    use rand::Rng;
    use std::sync::Arc;
    let mut w = world();
    let mut rules = (*w.ruleset).clone();
    let mut custom = rules.monster("human").unwrap().clone();
    custom.id = None;
    custom.name = "clockwork walker".into();
    custom.speed = 24;
    custom.sample_hp = false;
    rules.monsters.push(custom);
    let def = rules
        .monsters
        .iter_mut()
        .find(|d| d.name == "human")
        .unwrap();
    def.speed = 6;
    rules.reindex();
    w.ruleset = Arc::new(rules);
    let id = w
        .spawn_monster_by_name("clockwork walker", Coord::new_unchecked(70, 10))
        .unwrap();
    assert_eq!(w.arena.actors[id].speed, 24);
    assert_eq!(w.arena.actors[id].movement, 0);
    w.arena.actors[id].is_peaceful = false;
    let mut changed = (*w.ruleset).clone();
    changed
        .monsters
        .iter_mut()
        .find(|d| d.name == "clockwork walker")
        .unwrap()
        .speed = 0;
    changed.reindex();
    w.ruleset = Arc::new(changed);
    w.rng = ChaCha8Rng::from_seed([0; 32]);
    assert_eq!(waits(&mut w, id, 3), (vec![1; 3], vec![0, 2, 2]));
    let mut w = world();
    let mut rules = (*w.ruleset).clone();
    rules
        .monsters
        .iter_mut()
        .find(|d| d.name == "human")
        .unwrap()
        .speed = 6;
    rules.reindex();
    w.ruleset = Arc::new(rules);
    let id = w
        .spawn_monster_by_name("human", Coord::new_unchecked(70, 10))
        .unwrap();
    w.arena.actors[id].is_peaceful = false;
    w.rng = ChaCha8Rng::from_seed([0; 32]);
    assert_eq!(waits(&mut w, id, 6), (vec![1; 6], vec![0, 1, 0, 0, 0, 0]));
    let def = w.ruleset.monster("human").unwrap();
    let static_actor = nethacked_data::monsters::create_monster_record(
        nethacked_data::MonsterSpeciesId::HUMAN,
        Coord::new_unchecked(70, 10),
        &mut w.rng,
    );
    assert_eq!(static_actor.movement, 0);
    let mut rng = ChaCha8Rng::from_seed([0; 32]);
    let mut expected = rng.clone();
    expected.random_range(0..2u32);
    expected.random_range(0..4u32);
    let dummy = w.ruleset.create_revival_record_from_def(
        def,
        Coord::new_unchecked(70, 10),
        MonsterSpawnContext::default(),
        (1, 1),
        Some(ActorSex::Male),
        &mut rng,
    );
    assert_eq!(dummy.movement, 0);
    assert_eq!(rng, expected);
    assert_eq!(w.arena.actors[w.player_id].movement, 0);
    let mut individual = dummy;
    individual.movement = 23;
    let saved = nethacked_arena::SavedMonsterTraits::from_actor(&individual, 0);
    assert_eq!(saved.actor.movement, 23);
    assert_eq!(
        serde_json::from_value::<nethacked_arena::SavedMonsterTraits>(
            serde_json::to_value(&saved).unwrap()
        )
        .unwrap()
        .actor
        .movement,
        23
    );
}

#[test]
fn public_burden_counts_repeat_after_inventory_and_attribute_transitions() {
    for (load, turns, actions) in [
        (825, [1, 1, 1, 1], [0, 1, 1, 1]),
        (826, [2, 1, 1, 2], [1, 1, 1, 2]),
        (1238, [2, 2, 2, 2], [1, 2, 2, 2]),
        (1650, [4, 4, 4, 4], [3, 4, 4, 4]),
        (2063, [6, 6, 6, 6], [5, 6, 6, 6]),
        (2475, [1, 1, 1, 1], [0, 1, 1, 1]),
    ] {
        for route in ["pickup_attributes", "nested_transfer"] {
            let mut w = world();
            if route == "pickup_attributes" {
                w.progress.attr_base[0] = 25;
                w.progress.attr_base[4] = 25;
                let item = weight(&mut w, load);
                w.arena.items[item].location = ItemLocation::Floor(w.hero_coord());
                w.step_player_action(ActionAst::PickUp);
                assert_eq!(
                    w.arena.items[item].location,
                    ItemLocation::CarriedBy(w.player_id)
                );
                w.step_player_action(ActionAst::Drop(0));
                assert_eq!(w.hero_carried_weight(), 0);
                w.step_player_action(ActionAst::PickUp);
                w.progress.attr_base[0] = 16;
                w.progress.attr_base[4] = 15;
            } else {
                let sack = w.arena.spawn_item(
                    w.ruleset
                        .create_item_record_by_id(
                            ItemKindId::SACK,
                            ItemLocation::CarriedBy(w.player_id),
                            Buc::Uncursed,
                        )
                        .unwrap(),
                );
                let bag = w.arena.spawn_item(
                    w.ruleset
                        .create_item_record_by_id(
                            ItemKindId::BAG_OF_HOLDING,
                            ItemLocation::CarriedBy(w.player_id),
                            Buc::Uncursed,
                        )
                        .unwrap(),
                );
                let item = weight(&mut w, 2 * (load - 30));
                let idx = |w: &SimulationWorld, id| {
                    w.arena
                        .items_carried_by(w.player_id)
                        .iter()
                        .position(|x| *x == id)
                        .unwrap()
                };
                w.step_player_action(ActionAst::PutInContainer {
                    item_index: idx(&w, item),
                    container_index: idx(&w, bag),
                });
                w.step_player_action(ActionAst::PutInContainer {
                    item_index: idx(&w, bag),
                    container_index: idx(&w, sack),
                });
            }
            assert_eq!(w.hero_weight_capacity(), 825);
            assert_eq!(w.hero_carried_weight(), load);
            let id = monster(&mut w, 12, 70, 10);
            w.scheduler.hero_energy = 12;
            let mut copy = load_world(&w);
            assert_eq!(
                waits(&mut w, id, 4),
                (turns.to_vec(), actions.to_vec()),
                "{route} load{load}"
            );
            assert_eq!(waits(&mut copy, id, 4), (turns.to_vec(), actions.to_vec()));
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&w.to_save_json().unwrap()).unwrap(),
                serde_json::from_str::<serde_json::Value>(&copy.to_save_json().unwrap()).unwrap()
            );
        }
    }
}
fn load_world(w: &SimulationWorld) -> SimulationWorld {
    load(w)
}

#[test]
fn public_cache_departure_save_and_arrival_preserve_remaining_credit() {
    use nethacked_sim::StoredLevel;
    use nethacked_types::BranchId;
    let mut w = world();
    let id = monster(&mut w, 0, 70, 10);
    w.arena.actors[id].movement = 23;
    w.level.stairs_down = Coord::new_unchecked(6, 5);
    w.level.stairs_up = Coord::new_unchecked(5, 5);
    w.level
        .set_tile(w.level.stairs_down, Tile::Stairs { up: false });
    w.level
        .set_tile(w.level.stairs_up, Tile::Stairs { up: true });
    w.arena.actors[w.player_id].coord = w.level.stairs_down;
    w.stored_levels.push((
        (BranchId::DungeonsOfDoom, 2),
        StoredLevel {
            level: w.level.clone(),
            monsters: vec![],
            items: vec![],
            unpaid_items: vec![],
        },
    ));
    w.step_player_action(ActionAst::Descend);
    assert!(w
        .stored_levels
        .iter()
        .any(|(_, level)| level.monsters.iter().any(|(_, a)| a.movement == 23)));
    let mut copy = load(&w);
    assert!(copy
        .stored_levels
        .iter()
        .any(|(_, level)| level.monsters.iter().any(|(_, a)| a.movement == 23)));
    for s in [&mut w, &mut copy] {
        s.arena.actors[s.player_id].coord = s.level.stairs_up;
    }
    let e = w.step_player_action(ActionAst::Ascend);
    assert_eq!(e, copy.step_player_action(ActionAst::Ascend));
    let actor = w.arena.actors.iter().find(|(_, a)| !a.is_player).unwrap();
    assert_eq!(actor.1.movement, 11);
    assert_eq!(moved(&e, actor.0), 1);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&w.to_save_json().unwrap()).unwrap(),
        serde_json::from_str::<serde_json::Value>(&copy.to_save_json().unwrap()).unwrap()
    );
    assert_eq!(w.rng.next_u64(), copy.rng.next_u64());
}
