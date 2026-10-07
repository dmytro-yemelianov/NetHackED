//! Source: NetHack-5.0.0_Released (16ff5911), allmain.c:625-679.
//! Natural ordinary-form HP only: one rn2(100) per eligible world turn;
//! heal one iff level + ACURR(CON) > draw.
use nethacked_arena::ItemLocation;
use nethacked_data::ItemKindId;
use nethacked_sim::{ActionAst, Coord, Direction, GameEvent, SimulationWorld, Tile};
use nethacked_types::{Buc, PetrificationState, PolymorphForm, SlimingState};
use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;

const HERE: Coord = Coord::new_unchecked(10, 10);
const EAST: Coord = Coord::new_unchecked(11, 10);

fn world() -> SimulationWorld {
    let mut w = SimulationWorld::new_with_seed(413);
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.arena.items.clear();
    w.wielded_item = None;
    w.level.traps.clear();
    w.level.set_tile(HERE, Tile::Room);
    w.level.set_tile(EAST, Tile::Room);
    let hero = w.arena.actors.get_mut(w.player_id).unwrap();
    hero.coord = HERE;
    hero.hp = 10;
    hero.max_hp = 100;
    hero.level = 10;
    w.progress.attr_base[0] = 100;
    w.progress.attr_base[4] = 13;
    w.player_nutrition = 1000;
    w.rng = ChaCha8Rng::seed_from_u64(0);
    w
}

fn hp(w: &SimulationWorld) -> u32 {
    w.arena.actors[w.player_id].hp
}

fn carry(w: &mut SimulationWorld, weight: u32) {
    let mut item = w
        .ruleset
        .create_item_record_by_id(
            ItemKindId::DAGGER,
            ItemLocation::CarriedBy(w.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    item.weight = weight;
    w.arena.spawn_item(item);
}

#[test]
fn public_each_world_turn_draws_once_and_save_continues_the_same_stream() {
    let mut w = world();
    let mut expected_rng = w.rng.clone();
    for literal in [65, 70, 72, 46, 50, 69, 56] {
        let roll: u32 = expected_rng.random_range(0..100);
        assert_eq!(roll, literal);
        let old_hp = hp(&w);
        let turn = w.scheduler.turn;
        let events = w.step_player_action(ActionAst::Wait);
        assert_eq!(w.scheduler.turn, turn + 1);
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, GameEvent::TurnAdvanced { .. }))
                .count(),
            1
        );
        assert_eq!(hp(&w), old_hp + u32::from(roll < 23));
        assert_eq!(w.rng.clone().next_u64(), expected_rng.clone().next_u64());
    }
    let json = w.to_save_json().unwrap();
    let mut loaded =
        SimulationWorld::from_save_json(&json, w.ruleset.clone(), &w.ruleset_ref).unwrap();
    for _ in 0..8 {
        assert_eq!(
            w.step_player_action(ActionAst::Wait),
            loaded.step_player_action(ActionAst::Wait)
        );
        assert_eq!(
            serde_json::to_value(&w).unwrap(),
            serde_json::to_value(&loaded).unwrap()
        );
    }
    assert_eq!(w.rng.next_u64(), loaded.rng.next_u64());
}

#[test]
fn public_burden_gates_actual_walking_but_not_wait_or_refused_movement() {
    use nethacked_core::inventory::EncumbranceTier::{Burdened, Stressed};
    for (weight, tier, draws) in [(1000, Burdened, true), (1500, Stressed, false)] {
        let mut w = world();
        w.scheduler.hero_speed = 24;
        carry(&mut w, weight);
        assert_eq!(w.hero_encumbrance(), tier);
        let mut control = w.rng.clone();
        let roll: u32 = if draws {
            control.random_range(0..100)
        } else {
            99
        };
        w.step_player_action(ActionAst::Move(Direction::East));
        assert_eq!(w.arena.actors[w.player_id].coord, EAST);
        assert_eq!(hp(&w), 10 + u32::from(roll < 23));
        assert_eq!(w.rng.next_u64(), control.next_u64());
    }
    for blocked in [false, true] {
        let mut w = world();
        w.scheduler.hero_speed = 24;
        carry(&mut w, 1500);
        if blocked {
            w.level.set_tile(
                EAST,
                Tile::Door {
                    state: nethacked_types::DoorState::Closed,
                    trapped: false,
                },
            );
        }
        let mut control = w.rng.clone();
        let roll: u32 = control.random_range(0..100);
        w.step_player_action(if blocked {
            ActionAst::Move(Direction::East)
        } else {
            ActionAst::Wait
        });
        assert_eq!(w.arena.actors[w.player_id].coord, HERE);
        assert_eq!(hp(&w), 10 + u32::from(roll < 23));
        assert_eq!(w.rng.next_u64(), control.next_u64());
    }
}

#[test]
fn public_full_health_polyform_and_zero_hp_skip_draws() {
    for state in 0..4 {
        let mut w = world();
        match state {
            0 => w.arena.actors.get_mut(w.player_id).unwrap().hp = 100,
            1 => {
                w.hero.polymorph = Some(PolymorphForm {
                    monster_id: 0,
                    hp: 10,
                    max_hp: 100,
                    duration: 20,
                })
            }
            2 => w.arena.actors.get_mut(w.player_id).unwrap().hp = 0,
            _ => w.arena.actors.get_mut(w.player_id).unwrap().is_dead = true,
        }
        let before = hp(&w);
        let mut control = w.rng.clone();
        w.step_player_action(ActionAst::Wait);
        assert_eq!(hp(&w), before);
        assert_eq!(w.rng.next_u64(), control.next_u64());
    }
}

#[test]
fn public_fatal_afflictions_preserve_early_return_and_do_not_regenerate() {
    for stone in [false, true] {
        let mut w = world();
        w.scheduler.hero_speed = 6;
        w.scheduler.hero_energy = 12;
        w.hero.afflictions.transient.confused = 2;
        w.hero.afflictions.transient.helpless = 2;
        if stone {
            w.hero.afflictions.petrification = Some(PetrificationState { turns_remaining: 1 });
        } else {
            w.hero.afflictions.sliming = Some(SlimingState { turns_remaining: 1 });
        }
        let mut control = w.rng.clone();
        w.step_player_action(ActionAst::Wait);
        assert!(w.arena.actors[w.player_id].is_dead);
        assert_eq!(hp(&w), 10);
        assert_eq!(w.hero.afflictions.transient.confused, 2);
        assert_eq!(w.hero.afflictions.transient.helpless, 2);
        assert_eq!(w.rng.next_u64(), control.next_u64());
        let turn = w.scheduler.turn;
        assert!(w.step_player_action(ActionAst::Wait).is_empty());
        assert_eq!(w.scheduler.turn, turn);
    }
}

#[test]
fn public_action_clock_two_actions_per_tick_and_two_ticks_per_action() {
    for (speed, energy, advances) in [(24, 24, vec![0, 1, 0, 1]), (6, 12, vec![2, 2, 2, 2])] {
        let mut w = world();
        w.scheduler.hero_speed = speed;
        w.scheduler.hero_energy = energy;
        let mut control = w.rng.clone();
        let mut expected_hp = 10;
        for ticks in advances {
            let turn = w.scheduler.turn;
            for _ in 0..ticks {
                let roll: u32 = control.random_range(0..100);
                expected_hp += u32::from(roll < 23);
            }
            let events = w.step_player_action(ActionAst::Wait);
            assert_eq!(w.scheduler.turn, turn + ticks);
            assert_eq!(
                events
                    .iter()
                    .filter(|e| matches!(e, GameEvent::TurnAdvanced { .. }))
                    .count() as u64,
                ticks
            );
            assert_eq!(hp(&w), expected_hp);
            assert_eq!(w.rng.clone().next_u64(), control.clone().next_u64());
        }
    }
}

#[test]
fn public_literal_thresholds_cap_and_helpless_has_no_sleepy_bonus() {
    // The eighth seeded bounded draw is literally 6. Vary the level and CON
    // separately across strict equality; sleep/paralysis alone adds no healing.
    for (level, con, expected) in [(3, 3, 10), (4, 3, 11), (1, 5, 10), (1, 6, 11)] {
        let mut w = world();
        w.arena.actors.get_mut(w.player_id).unwrap().level = level;
        w.progress.attr_base[4] = con;
        for _ in 0..7 {
            let _: u32 = w.rng.random_range(0..100);
        }
        let mut control = w.rng.clone();
        assert_eq!(control.random_range(0..100u32), 6);
        w.hero.afflictions.transient.helpless = 2;
        w.step_player_action(ActionAst::Move(Direction::East));
        assert_eq!(hp(&w), expected);
        assert_eq!(w.arena.actors[w.player_id].coord, HERE);
        assert_eq!(w.rng.next_u64(), control.next_u64());
    }
    let mut w = world();
    w.arena.actors.get_mut(w.player_id).unwrap().hp = 99;
    w.arena.actors.get_mut(w.player_id).unwrap().level = 30;
    w.progress.attr_base[4] = 25;
    for _ in 0..7 {
        let _: u32 = w.rng.random_range(0..100);
    }
    let mut control = w.rng.clone();
    assert_eq!(control.random_range(0..100u32), 6);
    w.step_player_action(ActionAst::Wait);
    assert_eq!(hp(&w), 100);
    for _ in 0..3 {
        w.step_player_action(ActionAst::Wait);
        assert_eq!(hp(&w), 100);
    }
    assert_eq!(w.rng.next_u64(), control.next_u64());
}

#[test]
fn public_failed_wall_move_spends_no_time_and_no_draw() {
    let mut w = world();
    carry(&mut w, 1500);
    w.level.set_tile(EAST, Tile::Wall { horizontal: false });
    let turn = w.scheduler.turn;
    let mut control = w.rng.clone();
    w.step_player_action(ActionAst::Move(Direction::East));
    assert_eq!(w.scheduler.turn, turn);
    assert_eq!(hp(&w), 10);
    assert_eq!(w.rng.next_u64(), control.next_u64());
}

#[test]
fn public_peaceful_bump_spends_time_without_walking_and_can_heal_when_stressed() {
    let mut w = world();
    w.scheduler.hero_speed = 24;
    carry(&mut w, 1500);
    let mut blocker = w.arena.actors[w.player_id].clone();
    blocker.name = "shopkeeper".into();
    blocker.species = None;
    blocker.is_player = false;
    blocker.is_peaceful = true;
    blocker.coord = EAST;
    w.arena.spawn_actor(blocker);
    let mut control = w.rng.clone();
    let _: u32 = control.random_range(0..7);
    let _: u32 = control.random_range(0..12); // P09 stationary actor allocation.
    let roll: u32 = control.random_range(0..100);
    let turn = w.scheduler.turn;
    w.step_player_action(ActionAst::Move(Direction::East));
    assert_eq!(w.scheduler.turn, turn + 1);
    assert_eq!(w.arena.actors[w.player_id].coord, HERE);
    assert_eq!(hp(&w), 10 + u32::from(roll < 23));
    assert_eq!(w.rng.next_u64(), control.next_u64());
}

#[test]
fn public_natural_healing_precedes_hunger_and_never_resurrects_after_starvation() {
    let mut w = world();
    w.arena.actors.get_mut(w.player_id).unwrap().hp = 1;
    w.arena.actors.get_mut(w.player_id).unwrap().level = 30;
    w.progress.attr_base[4] = 25;
    for _ in 0..7 {
        let _: u32 = w.rng.random_range(0..100);
    }
    w.scheduler.turn = 9;
    w.player_nutrition = 0;
    w.step_player_action(ActionAst::Wait);
    // Literal draw 6 heals 1 HP before turn-10 fainting removes 1 HP.
    assert_eq!(hp(&w), 1);
    assert!(!w.arena.actors[w.player_id].is_dead);
    w.player_nutrition = -350;
    w.step_player_action(ActionAst::Wait);
    assert_eq!(hp(&w), 0);
    assert!(w.arena.actors[w.player_id].is_dead);
    let mut control = w.rng.clone();
    assert!(w.step_player_action(ActionAst::Wait).is_empty());
    assert_eq!(hp(&w), 0);
    assert_eq!(w.rng.next_u64(), control.next_u64());
}
