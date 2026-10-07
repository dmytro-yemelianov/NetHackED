//! Actual production births; pinned NetHack 5.0 mon.c:549–579,892–900,
//! mkobj.c:1200–1230,1389–1468,2090–2116; objnam.c:5037,5148–5169.
use nethacked_arena::{ActorSex, CorpseSpecies, ItemLocation};
use nethacked_data::containers::ContainerContext;
use nethacked_data::containers::GeneratedItem;
use nethacked_data::corpses::{CorpseTimeoutRecipe as Recipe, CorpseTimerOperation as Op};
use nethacked_data::generation::{DungeonAlign, MonsterGenContext};
use nethacked_data::monsters::CorpseTimeoutPolicy as Policy;
use nethacked_data::{ItemKindId as K, MonsterSpeciesId as M};
use nethacked_sim::{
    ActionAst, GameEvent, ObjectTimerKind as Kind, ObjectTimerTarget as Target, SimulationWorld,
    StoredLevel,
};
use nethacked_types::{BranchId, Buc, Coord, Tile};
use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::sync::Arc;

fn world(seed: u64) -> SimulationWorld {
    let mut world = SimulationWorld::new_with_seed(seed);
    world.arena.actors.retain(|id, _| id == world.player_id);
    world.arena.items.clear();
    world.equipment = Default::default();
    world.wielded_item = None;
    world.scheduler.turn = 73;
    world.depth = 10;
    world.arena.actors[world.player_id].level = 18;
    world.rng = ChaCha8Rng::seed_from_u64(seed);
    let mut rules = (*world.ruleset).clone();
    for def in &mut rules.monsters {
        def.gen_flags.push("nogen".into());
        def.frequency = 0;
    }
    let jackal = rules
        .monsters
        .iter_mut()
        .find(|d| d.id == Some(M::JACKAL))
        .unwrap();
    jackal.frequency = 1;
    jackal.difficulty = 5;
    jackal.gen_flags.clear();
    jackal.flags.clear();
    jackal.maligntyp = 0;
    rules.reindex();
    world.ruleset = Arc::new(rules);
    world
}

fn save(world: &SimulationWorld) -> serde_json::Value {
    serde_json::from_str(&world.to_save_json().unwrap()).unwrap()
}
fn loaded(world: &SimulationWorld) -> SimulationWorld {
    SimulationWorld::from_save_json(
        &world.to_save_json().unwrap(),
        world.ruleset.clone(),
        &world.ruleset_ref,
    )
    .unwrap()
}

/// Independent C rnd.c integer arithmetic; generation uses 25, gameplay 10.
/// Deliberately does not call the Rust corpse sampler or a construction helper.
fn c_rot_delay(rng: &mut ChaCha8Rng, adjustment: i64) -> i64 {
    let factor = 1000 + i64::from(rng.random_range(0..1000u32));
    let mut multiplier = 1;
    while multiplier < 6 && rng.random_range(0..4u32) == 0 {
        multiplier += 1;
    }
    let factor = factor * multiplier;
    let noise = if rng.random_range(0..2u32) != 0 {
        adjustment * factor / 1000
    } else {
        adjustment * 1000 / factor
    };
    250 + noise - adjustment
}
fn c_troll_recipe(rng: &mut ChaCha8Rng) -> (Kind, i64) {
    let rot = c_rot_delay(rng, 10);
    for delay in 2..=50 {
        if rng.random_range(0..37u32) == 0 {
            return (Kind::ReviveMonster, delay);
        }
    }
    (Kind::RotCorpse, rot)
}
fn c_initial_draws(rng: &mut ChaCha8Rng, selection_bound: u32) -> i64 {
    let _: u32 = rng.random_range(0..selection_bound);
    let _: u32 = rng.random_range(0..2); // discarded initial corpse gender
    c_rot_delay(rng, 10)
}

#[test]
fn public_death_captures_actual_cancelled_troll_and_replays_full_save_cache_rng() {
    let mut world = world(928);
    let mut actor = world
        .ruleset
        .monster_by_id(M::TROLL)
        .unwrap()
        .create_record_with_hp(world.level.stairs_down, 0, 3, 13);
    actor.name = "Individual troll".into();
    actor.sex = Some(ActorSex::Female);
    actor.is_dead = true;
    actor.is_cancelled = true;
    actor.is_fleeing = true;
    actor.is_peaceful = true;
    actor.mspec_used = 7;
    actor.intrinsics.fire_resistance = true;
    let mut expected_actor = actor.clone();
    expected_actor.max_hp = 8; // actual owned Troll baselevel7, not runtime level13
    let id = world.arena.spawn_actor(actor);
    let mut expected_rng = world.rng.clone();
    c_initial_draws(&mut expected_rng, 1);
    let (kind, delay) = c_troll_recipe(&mut expected_rng);
    assert!(
        delay > 4,
        "fixed cache fixture must retain its sampled timer"
    );
    world.on_actor_killed(id, id, &mut Vec::new());
    assert_eq!(world.rng.clone().next_u64(), expected_rng.next_u64());
    let (corpse, item) = world
        .arena
        .items
        .iter()
        .find(|(_, item)| item.is_corpse())
        .unwrap();
    assert_eq!(
        item.corpse_species,
        Some(CorpseSpecies::Vanilla("TROLL".into()))
    );
    assert_eq!(
        (item.enchantment, item.corpse_no_revive, item.corpse_age),
        (1, true, 0)
    );
    assert_eq!(
        item.saved_monster_traits.as_ref().unwrap().actor,
        expected_actor
    );
    assert_eq!(world.object_timers.entries().len(), 1);
    let timer = world.object_timers.entries()[0];
    assert_eq!(
        (timer.target, timer.serial, timer.kind, timer.due),
        (Target::Active(corpse), 1, kind, 73 + delay as u64)
    );
    assert_eq!(world.arena.actors[id].max_hp, 3);
    let mut copy = loaded(&world);
    assert_eq!(save(&copy), save(&world));
    assert_eq!(copy.rng.clone().next_u64(), world.rng.clone().next_u64());
    for sim in [&mut world, &mut copy] {
        sim.stored_levels.push((
            (BranchId::DungeonsOfDoom, 11),
            StoredLevel {
                level: sim.level.clone(),
                monsters: vec![],
                items: vec![],
                unpaid_items: vec![],
            },
        ));
        sim.level
            .set_tile(sim.level.stairs_down, Tile::Stairs { up: false });
        sim.arena.actors[sim.player_id].coord = sim.level.stairs_down;
        sim.step_player_action(ActionAst::Descend);
        assert!(matches!(
            sim.object_timers.entries()[0].target,
            Target::Cached { depth: 10, .. }
        ));
    }
    assert_eq!(save(&copy), save(&world));
    copy = loaded(&copy); // full mid-cache reload
    for sim in [&mut world, &mut copy] {
        sim.level
            .set_tile(sim.level.stairs_up, Tile::Stairs { up: true });
        sim.arena.actors[sim.player_id].coord = sim.level.stairs_up;
        sim.step_player_action(ActionAst::Ascend);
        let timer = sim.object_timers.entries()[0];
        assert_eq!(
            (timer.serial, timer.kind, timer.due),
            (1, kind, 73 + delay as u64)
        );
        let Target::Active(reminted) = timer.target else {
            panic!("not remapped");
        };
        assert_eq!(
            sim.arena.items[reminted]
                .saved_monster_traits
                .as_ref()
                .unwrap()
                .actor,
            expected_actor
        );
    }
    assert_eq!(save(&copy), save(&world));
    assert_eq!(copy.rng.next_u64(), world.rng.next_u64());
}

#[test]
fn public_generic_wish_uses_gameplay_clock_hero_geography_and_zero_final_gender() {
    for branch in [
        BranchId::DungeonsOfDoom,
        BranchId::GnomishMines,
        BranchId::Gehennom,
    ] {
        let mut world = world(423);
        world.current_branch = branch;
        if branch == BranchId::Gehennom {
            // Only hell geography permits this owned candidate in rndmonst.
            let mut rules = (*world.ruleset).clone();
            rules
                .monsters
                .iter_mut()
                .find(|d| d.id == Some(M::JACKAL))
                .unwrap()
                .gen_flags
                .push("hell".into());
            rules.reindex();
            world.ruleset = Arc::new(rules);
        }
        let wand = world
            .ruleset
            .create_item_record_by_id(
                K::WAN_WISHING,
                ItemLocation::CarriedBy(world.player_id),
                Buc::Uncursed,
            )
            .unwrap();
        world.arena.spawn_item(wand);
        let mut expected_rng = world.rng.clone();
        let delay = c_initial_draws(
            &mut expected_rng,
            if branch == BranchId::GnomishMines {
                3
            } else {
                1
            },
        );
        let events = world.step_player_action(ActionAst::Wish("blessed +5 corpse".into()));
        assert!(events
            .iter()
            .any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("corpse"))));
        assert_eq!(world.rng.clone().next_u64(), expected_rng.next_u64());
        let (id, corpse) = world
            .arena
            .items
            .iter()
            .find(|(_, item)| item.is_corpse())
            .unwrap();
        assert_eq!(
            corpse.corpse_species,
            Some(CorpseSpecies::Vanilla("JACKAL".into()))
        );
        assert_eq!((corpse.enchantment, corpse.buc), (0, Buc::Blessed));
        assert!(corpse.saved_monster_traits.is_none());
        assert_eq!(world.object_timers.entries().len(), 1);
        let timer = world.object_timers.entries()[0];
        assert_eq!(
            (timer.target, timer.serial, timer.kind, timer.due),
            (Target::Active(id), 0, Kind::RotCorpse, 73 + delay as u64)
        );
        assert_eq!(save(&loaded(&world)), save(&world));
    }
}

#[test]
fn public_wish_passes_actual_genocide_registry_to_initial_selection() {
    let mut world = world(423);
    world
        .genocide_registry
        .genocided_species
        .insert("jackal".into());
    let wand = world
        .ruleset
        .create_item_record_by_id(
            K::WAN_WISHING,
            ItemLocation::CarriedBy(world.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    world.arena.spawn_item(wand);
    let mut expected_rng = world.rng.clone();
    // No common candidate: C rndmonnum Plan B ignores G_GONE. The owned
    // compatibility guard caps at256; other canonical species have nogen.
    for _ in 0..256 {
        if expected_rng.random_range(0..M::LONG_WORM_TAIL.index() as u32)
            == M::JACKAL.index() as u32
        {
            break;
        }
    }
    let _: u32 = expected_rng.random_range(0..2);
    let delay = c_rot_delay(&mut expected_rng, 10);
    world.step_player_action(ActionAst::Wish("corpse".into()));
    assert_eq!(world.rng.clone().next_u64(), expected_rng.next_u64());
    let corpse = world
        .arena
        .items
        .values()
        .find(|item| item.is_corpse())
        .unwrap();
    assert_eq!(
        corpse.corpse_species,
        Some(CorpseSpecies::Vanilla("JACKAL".into()))
    );
    assert_eq!(world.object_timers.entries()[0].due, 73 + delay as u64);
}

#[test]
fn public_nocorpse_death_returns_before_any_initialization_draw() {
    let mut world = world(826);
    let mut rules = (*world.ruleset).clone();
    let def = rules
        .monsters
        .iter_mut()
        .find(|d| d.id == Some(M::JACKAL))
        .unwrap();
    def.gen_flags.push("nocorpse".into());
    let mut actor = def.create_record_with_hp(Coord::new_unchecked(8, 8), 0, 3, 7);
    actor.is_dead = true;
    actor.is_tame = true;
    rules.reindex();
    world.ruleset = Arc::new(rules);
    let id = world.arena.spawn_actor(actor);
    let mut expected_rng = world.rng.clone();
    world.on_actor_killed(id, id, &mut Vec::new());
    assert!(world.arena.items.is_empty());
    assert!(world.object_timers.entries().is_empty());
    assert_eq!(world.rng.next_u64(), expected_rng.next_u64());
}

#[test]
fn actual_melee_kill_creates_troll_traits_timer_and_final_actor_sex() {
    let mut kills = 0;
    for seed in 0..8 {
        let mut world = world(seed);
        let hero_coord = world.arena.actors[world.player_id].coord;
        let east = Coord::new_unchecked(hero_coord.x + 1, hero_coord.y);
        world.level.set_tile(east, Tile::Room);
        let mut actor = world
            .ruleset
            .monster_by_id(M::TROLL)
            .unwrap()
            .create_record_with_hp(east, 1, 3, 7);
        actor.sex = Some(ActorSex::Female);
        actor.ac = 40;
        actor.is_cancelled = true;
        let id = world.arena.spawn_actor(actor);
        let events = world.step_player_action(ActionAst::MeleeAttack(east));
        if !events
            .iter()
            .any(|event| matches!(event, GameEvent::AttackLanded { target, .. } if *target == id))
        {
            continue;
        }
        let corpse = world
            .arena
            .items
            .values()
            .find(|item| item.is_corpse())
            .unwrap();
        assert_eq!(
            corpse.corpse_species,
            Some(CorpseSpecies::Vanilla("TROLL".into()))
        );
        assert_eq!((corpse.enchantment, corpse.corpse_no_revive), (1, true));
        let actor = &corpse.saved_monster_traits.as_ref().unwrap().actor;
        assert_eq!(
            (actor.hp, actor.max_hp, actor.sex, actor.is_dead),
            (0, 8, Some(ActorSex::Female), true)
        );
        assert_eq!(world.object_timers.entries().len(), 1);
        assert_eq!(world.object_timers.entries()[0].serial, 1);
        kills += 1;
    }
    assert!(kills > 0);
}

#[test]
fn ordered_stream_is_authoritative_and_frozen_cancellation_follows_replacement() {
    for frozen in [false, true] {
        let mut world = world(71);
        let location = if frozen {
            let ice = world.arena.spawn_item(
                world
                    .ruleset
                    .create_item_record_by_id(K::ICE_BOX, ItemLocation::Limbo, Buc::Uncursed)
                    .unwrap(),
            );
            ItemLocation::InContainer(ice)
        } else {
            ItemLocation::Limbo
        };
        let tree = GeneratedItem {
            item: world
                .ruleset
                .create_corpse_record("human", ItemLocation::Limbo)
                .unwrap(),
            contents: vec![],
            corpse_timeout: Some(Recipe::Rot {
                sampled_at: 999,
                delay: 999,
            }), // observational only
            corpse_timer_operations: vec![
                Op::Start(Recipe::Rot {
                    sampled_at: 73,
                    delay: 245,
                }),
                Op::Cancel,
                Op::Start(Recipe::Revive {
                    sampled_at: 73,
                    delay: 10,
                }),
            ],
        };
        world.scheduler.turn = 78;
        let mut expected_rng = world.rng.clone();
        let id = world.attach_generated_item(tree, location).unwrap();
        assert_eq!(world.rng.next_u64(), expected_rng.next_u64());
        if frozen {
            assert!(world.object_timers.entries().is_empty());
        } else {
            let timer = world.object_timers.entries()[0];
            assert_eq!(
                (timer.target, timer.kind, timer.serial, timer.due),
                (Target::Active(id), Kind::ReviveMonster, 1, 83)
            );
        }
        assert_eq!(save(&loaded(&world)), save(&world));
        world.cancel_object_timers(Target::Active(id));
        assert_eq!(
            world.start_object_timer(Target::Active(id), Kind::RotCorpse, 20),
            Ok(2)
        );
    }
}

#[test]
fn public_legacy_unknown_sex_keeps_zero_flags_and_load_adds_no_draws() {
    let mut world = world(823);
    let mut actor = world
        .ruleset
        .monster_by_id(M::JACKAL)
        .unwrap()
        .create_record_with_hp(Coord::new_unchecked(8, 8), 0, 3, 7);
    actor.sex = None;
    actor.is_dead = true;
    let id = world.arena.spawn_actor(actor);
    let mut expected_rng = world.rng.clone();
    let delay = c_initial_draws(&mut expected_rng, 1);
    world.on_actor_killed(id, id, &mut Vec::new());
    assert_eq!(world.rng.clone().next_u64(), expected_rng.next_u64());
    assert_eq!(world.arena.actors[id].sex, None);
    let corpse = world
        .arena
        .items
        .values()
        .find(|item| item.is_corpse())
        .unwrap();
    assert_eq!(corpse.enchantment, 0);
    assert_eq!(world.object_timers.entries()[0].due, 73 + delay as u64);
    let encoded = world.to_save_json().unwrap();
    assert!(!save(&world)["arena"]["actors"]
        .to_string()
        .contains("\"sex\":null"));
    let mut copy =
        SimulationWorld::from_save_json(&encoded, world.ruleset.clone(), &world.ruleset_ref)
            .unwrap();
    assert_eq!(copy.arena.actors[id].sex, None);
    assert_eq!(save(&copy), save(&world));
    assert_eq!(copy.rng.next_u64(), world.rng.next_u64());
}

#[test]
fn literal_birth_transition_streams_preserve_installed_and_cancelled_serials() {
    for initial in 0..4 {
        for final_kind in 0..4 {
            let mut world = world(61);
            let mut rules = (*world.ruleset).clone();
            let initial_def = rules
                .monsters
                .iter_mut()
                .find(|d| d.id == Some(M::JACKAL))
                .unwrap();
            initial_def.glyph = if initial == 1 { 'T' } else { 'd' };
            initial_def.corpse_timeout_policy = match initial {
                2 => Policy::Never,
                3 => Policy::Rider,
                _ => Policy::Ordinary,
            };
            let mut final_def = initial_def.clone();
            final_def.id = None;
            final_def.name = "final specimen".into();
            final_def.gen_flags = vec!["nogen".into()];
            final_def.frequency = 0;
            final_def.glyph = if final_kind == 1 { 'T' } else { 'd' };
            final_def.corpse_timeout_policy = match final_kind {
                2 => Policy::Never,
                3 => Policy::Rider,
                _ => Policy::Ordinary,
            };
            let actor = final_def.create_record_with_hp(Coord::new_unchecked(8, 8), 0, 3, 7);
            rules.monsters.push(final_def);
            rules.reindex();
            let tree = rules
                .create_initialized_corpse_for_actor(
                    &actor,
                    ItemLocation::Limbo,
                    ContainerContext {
                        hero_level: 18,
                        level_difficulty: 10,
                        moves: 73,
                        in_mklev: false,
                    },
                    &MonsterGenContext {
                        hero_level: 18,
                        level_difficulty: 10,
                        in_hell: false,
                        align: DungeonAlign::None,
                        temperature: 0,
                    },
                    |_| false,
                    |bound| match bound {
                        1 | 2 | 37 | 1000 => 0,
                        4 => 1,
                        _ => panic!("unexpected literal bound"),
                    },
                )
                .unwrap();
            let id = world
                .attach_generated_item(tree, ItemLocation::Limbo)
                .unwrap();
            let initial_count = u64::from(initial == 0 || initial == 1);
            let replaced = initial != 0 || final_kind != 0;
            let final_count = u64::from(replaced && (final_kind == 0 || final_kind == 1));
            if final_kind == 2 || final_kind == 3 {
                assert!(world.object_timers.entries().is_empty());
            } else {
                assert_eq!(world.object_timers.entries().len(), 1);
                let timer = world.object_timers.entries()[0];
                assert_eq!(
                    (timer.target, timer.serial),
                    (Target::Active(id), if replaced { initial_count } else { 0 })
                );
                assert_eq!(
                    (timer.kind, timer.due),
                    if final_kind == 1 {
                        (Kind::ReviveMonster, 75)
                    } else {
                        (Kind::RotCorpse, 323)
                    }
                );
            }
            world.cancel_object_timers(Target::Active(id));
            assert_eq!(
                world.start_object_timer(Target::Active(id), Kind::RotCorpse, 10),
                Ok(initial_count + final_count)
            );
        }
    }
}
