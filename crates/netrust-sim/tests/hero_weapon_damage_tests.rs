//! Tests for NetHack fidelity D2 Task 5: hero weapon damage via dmgval.
//!
//! Validates that hero melee damage reflects the wielded weapon's small/large
//! dice from ITEM_CATALOG (C `weapon.c:216-293`), target monster size from the
//! bestiary (large >= MZ_LARGE), bare-hands damage 1..=2 (C `uhitm.c:847`), and
//! Monk martial-arts bare-hands damage 1..=4 (C `uhitm.c:847`).

use netrust_arena::ItemLocation;
use netrust_data::{
    create_item_record, create_monster_record, ItemKindId, MonsterSpeciesId, RoleId,
};
use netrust_sim::{ActionAst, CharacterConfig, GameEvent, SimulationWorld};
use netrust_types::{Buc, Coord, SkillClass, SkillLevel};
use std::collections::BTreeSet;

/// Set up an open floor tile adjacent to the player and return its coordinate.
fn open_east(sim: &mut SimulationWorld) -> Coord {
    let p = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let east = Coord::new(p.x + 1, p.y).unwrap();
    sim.level.set_tile(east, netrust_types::Tile::Room);
    east
}

#[test]
fn test_hero_dagger_damage_vs_small_and_large_monsters() {
    // Dagger: oc_wsdam = 4, oc_wldam = 3 (1d4 vs small, 1d3 vs large).
    let mut damages_small = BTreeSet::new();
    let mut damages_large = BTreeSet::new();

    // Small target: Jackal (size Small)
    for seed in 0..100u64 {
        let mut sim = SimulationWorld::new_with_seed(seed + 1000);
        // Set Dagger skill to Basic so skill damage bonus is 0
        sim.hero
            .skills
            .skills
            .insert(SkillClass::Dagger, SkillLevel::Basic);

        let dagger = sim.arena.spawn_item(create_item_record(
            ItemKindId::Dagger,
            ItemLocation::CarriedBy(sim.player_id),
            Buc::Uncursed,
        ));
        let carried = sim.arena.items_carried_by(sim.player_id);
        let idx = carried.iter().position(|&id| id == dagger).unwrap();
        sim.step_player_action(ActionAst::Wield(idx));

        let east = open_east(&mut sim);
        let mut mon = create_monster_record(MonsterSpeciesId::Jackal, east);
        mon.ac = 30; // guarantees hit
        mon.hp = 1000;
        mon.max_hp = 1000;
        let mid = sim.arena.spawn_actor(mon);

        let events = sim.step_player_action(ActionAst::MeleeAttack(east));
        for ev in events {
            if let GameEvent::AttackLanded { target, damage, .. } = ev {
                if target == mid {
                    damages_small.insert(damage);
                }
            }
        }
    }

    // Large target: Red Dragon (size Large)
    for seed in 0..100u64 {
        let mut sim = SimulationWorld::new_with_seed(seed + 2000);
        sim.hero
            .skills
            .skills
            .insert(SkillClass::Dagger, SkillLevel::Basic);

        let dagger = sim.arena.spawn_item(create_item_record(
            ItemKindId::Dagger,
            ItemLocation::CarriedBy(sim.player_id),
            Buc::Uncursed,
        ));
        let carried = sim.arena.items_carried_by(sim.player_id);
        let idx = carried.iter().position(|&id| id == dagger).unwrap();
        sim.step_player_action(ActionAst::Wield(idx));

        let east = open_east(&mut sim);
        let mut mon = create_monster_record(MonsterSpeciesId::RedDragon, east);
        mon.ac = 30; // guarantees hit
        mon.hp = 1000;
        mon.max_hp = 1000;
        let mid = sim.arena.spawn_actor(mon);

        let events = sim.step_player_action(ActionAst::MeleeAttack(east));
        for ev in events {
            if let GameEvent::AttackLanded { target, damage, .. } = ev {
                if target == mid {
                    damages_large.insert(damage);
                }
            }
        }
    }

    // Dagger vs small: 1d4 (values in 1..=4, 4 observed)
    assert!(
        damages_small.iter().all(|&d| (1..=4).contains(&d)),
        "small damages out of bounds: {damages_small:?}"
    );
    assert!(
        damages_small.contains(&4),
        "expected 4 against small target: {damages_small:?}"
    );

    // Dagger vs large: 1d3 (values in 1..=3, 3 observed, 4 never observed)
    assert!(
        damages_large.iter().all(|&d| (1..=3).contains(&d)),
        "large damages out of bounds: {damages_large:?}"
    );
    assert!(
        damages_large.contains(&3),
        "expected 3 against large target: {damages_large:?}"
    );
    assert!(
        !damages_large.contains(&4),
        "4 should never appear against large target: {damages_large:?}"
    );
}

#[test]
fn test_hero_long_sword_damage_vs_small_and_large_monsters() {
    // Long sword: oc_wsdam = 8, oc_wldam = 12 (1d8 vs small, 1d12 vs large).
    let mut damages_small = BTreeSet::new();
    let mut damages_large = BTreeSet::new();

    // Small target: Jackal (size Small)
    for seed in 0..100u64 {
        let mut sim = SimulationWorld::new_with_seed(seed + 3000);
        sim.hero
            .skills
            .skills
            .insert(SkillClass::LongSword, SkillLevel::Basic);

        let sword = sim.arena.spawn_item(create_item_record(
            ItemKindId::LongSword,
            ItemLocation::CarriedBy(sim.player_id),
            Buc::Uncursed,
        ));
        let carried = sim.arena.items_carried_by(sim.player_id);
        let idx = carried.iter().position(|&id| id == sword).unwrap();
        sim.step_player_action(ActionAst::Wield(idx));

        let east = open_east(&mut sim);
        let mut mon = create_monster_record(MonsterSpeciesId::Jackal, east);
        mon.ac = 30;
        mon.hp = 1000;
        mon.max_hp = 1000;
        let mid = sim.arena.spawn_actor(mon);

        let events = sim.step_player_action(ActionAst::MeleeAttack(east));
        for ev in events {
            if let GameEvent::AttackLanded { target, damage, .. } = ev {
                if target == mid {
                    damages_small.insert(damage);
                }
            }
        }
    }

    // Large target: Red Dragon (size Large)
    for seed in 0..100u64 {
        let mut sim = SimulationWorld::new_with_seed(seed + 4000);
        sim.hero
            .skills
            .skills
            .insert(SkillClass::LongSword, SkillLevel::Basic);

        let sword = sim.arena.spawn_item(create_item_record(
            ItemKindId::LongSword,
            ItemLocation::CarriedBy(sim.player_id),
            Buc::Uncursed,
        ));
        let carried = sim.arena.items_carried_by(sim.player_id);
        let idx = carried.iter().position(|&id| id == sword).unwrap();
        sim.step_player_action(ActionAst::Wield(idx));

        let east = open_east(&mut sim);
        let mut mon = create_monster_record(MonsterSpeciesId::RedDragon, east);
        mon.ac = 30;
        mon.hp = 1000;
        mon.max_hp = 1000;
        let mid = sim.arena.spawn_actor(mon);

        let events = sim.step_player_action(ActionAst::MeleeAttack(east));
        for ev in events {
            if let GameEvent::AttackLanded { target, damage, .. } = ev {
                if target == mid {
                    damages_large.insert(damage);
                }
            }
        }
    }

    // Long sword vs small: 1d8
    assert!(
        damages_small.iter().all(|&d| (1..=8).contains(&d)),
        "small damages out of bounds: {damages_small:?}"
    );
    assert!(
        damages_small.iter().any(|&d| d > 4),
        "expected damage > 4: {damages_small:?}"
    );

    // Long sword vs large: 1d12
    assert!(
        damages_large.iter().all(|&d| (1..=12).contains(&d)),
        "large damages out of bounds: {damages_large:?}"
    );
    assert!(
        damages_large.iter().any(|&d| d > 8),
        "expected damage > 8: {damages_large:?}"
    );
}

#[test]
fn test_hero_bare_hands_damage_non_monk() {
    // Valkyrie unarmed: C uhitm.c:847 bare hands rnd(2), skill bonus 0 at Basic.
    let mut damages = BTreeSet::new();

    for seed in 0..60u64 {
        let mut sim = SimulationWorld::new_with_character(
            seed + 5000,
            CharacterConfig {
                role: RoleId::Valkyrie,
                ..CharacterConfig::default()
            },
        );
        // Unwield starting weapon
        sim.wielded_item = None;
        sim.hero
            .skills
            .skills
            .insert(SkillClass::BareHanded, SkillLevel::Basic);

        let east = open_east(&mut sim);
        let mut mon = create_monster_record(MonsterSpeciesId::Jackal, east);
        mon.ac = 30;
        mon.hp = 1000;
        mon.max_hp = 1000;
        let mid = sim.arena.spawn_actor(mon);

        let events = sim.step_player_action(ActionAst::MeleeAttack(east));
        for ev in events {
            if let GameEvent::AttackLanded { target, damage, .. } = ev {
                if target == mid {
                    damages.insert(damage);
                }
            }
        }
    }

    assert_eq!(
        damages,
        BTreeSet::from([1, 2]),
        "bare hands must deal 1..=2: {damages:?}"
    );
}

#[test]
fn test_hero_bare_hands_damage_monk_martial_arts() {
    // Monk unarmed: C uhitm.c:847 martial arts rnd(4), skill bonus 0 at Basic.
    let mut damages = BTreeSet::new();

    for seed in 0..100u64 {
        let mut sim = SimulationWorld::new_with_character(
            seed + 6000,
            CharacterConfig {
                role: RoleId::Monk,
                ..CharacterConfig::default()
            },
        );
        sim.wielded_item = None;
        sim.hero
            .skills
            .skills
            .insert(SkillClass::BareHanded, SkillLevel::Basic);

        let east = open_east(&mut sim);
        let mut mon = create_monster_record(MonsterSpeciesId::Jackal, east);
        mon.ac = 30;
        mon.hp = 1000;
        mon.max_hp = 1000;
        let mid = sim.arena.spawn_actor(mon);

        let events = sim.step_player_action(ActionAst::MeleeAttack(east));
        for ev in events {
            if let GameEvent::AttackLanded { target, damage, .. } = ev {
                if target == mid {
                    damages.insert(damage);
                }
            }
        }
    }

    assert_eq!(
        damages,
        BTreeSet::from([1, 2, 3, 4]),
        "monk martial arts bare hands must deal 1..=4: {damages:?}"
    );
}
