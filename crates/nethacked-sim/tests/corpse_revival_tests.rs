//! Independent literal/native RNG oracles from pinned C 16ff5911 only.
use nethacked_arena::{ActorSex, CorpseSpecies, ItemId, ItemLocation, SavedMonsterTraits};
use nethacked_data::{ItemKindId as K, MonsterSpeciesId as M};
use nethacked_sim::{
    ActionAst, GameEvent, ObjectTimerKind as Kind, ObjectTimerTarget as Target, SimulationWorld,
    StoredLevel,
};
use nethacked_types::{BranchId, Buc, Coord, Tile, COLNO, ROWNO};
use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::sync::Arc;
const ORIGIN: Coord = Coord::new_unchecked(20, 10);
fn world(seed: u64) -> SimulationWorld {
    let mut w = SimulationWorld::new_with_seed(seed);
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.arena.items = Default::default();
    w.equipment = Default::default();
    w.wielded_item = None;
    w.unpaid_items.clear();
    w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Room);
    w.level.stairs_up = Coord::new_unchecked(5, 5);
    w.level.stairs_down = Coord::new_unchecked(6, 5);
    w.arena.actors[w.player_id].coord = Coord::new_unchecked(5, 5);
    w.arena.actors[w.player_id].level = 18;
    w.depth = 10;
    w.scheduler.turn = 73;
    w.scheduler.monster_speed = 0;
    w.scheduler.monster_energy = 0;
    w.rng = ChaCha8Rng::seed_from_u64(seed);
    w
}
fn corpse(w: &mut SimulationWorld, location: ItemLocation) -> ItemId {
    let i = w.ruleset.create_corpse_record("troll", location).unwrap();
    w.arena.spawn_item(i)
}
fn container(w: &mut SimulationWorld, kind: K, location: ItemLocation) -> ItemId {
    let i = w
        .ruleset
        .create_item_record_by_id(kind, location, Buc::Uncursed)
        .unwrap();
    w.arena.spawn_item(i)
}
fn load(w: &SimulationWorld) -> SimulationWorld {
    SimulationWorld::from_save_json(
        &w.to_save_json().unwrap(),
        w.ruleset.clone(),
        &w.ruleset_ref,
    )
    .unwrap()
}
fn save(w: &SimulationWorld) -> serde_json::Value {
    serde_json::from_str(&w.to_save_json().unwrap()).unwrap()
}
fn run(w: &mut SimulationWorld, id: ItemId) -> SimulationWorld {
    w.start_object_timer(Target::Active(id), Kind::ReviveMonster, 0)
        .unwrap();
    load(w)
}
fn draw(r: &mut ChaCha8Rng, bound: u32) -> u32 {
    r.random_range(0..bound)
}
fn hp(r: &mut ChaCha8Rng, level: u32, sex: bool) -> u32 {
    draw(r, 2); // C next_ident, preceding every HP initialization
    let h = if level == 0 {
        draw(r, 4) + 1
    } else {
        (0..level).map(|_| draw(r, 8) + 1).sum()
    };
    if sex {
        draw(r, 2);
    }
    if h == level.max(1) {
        h + 1
    } else {
        h
    }
}
fn rot(r: &mut ChaCha8Rng, age: u32) -> u64 {
    ((0..5).map(|_| i64::from(draw(r, 50)) + 1).sum::<i64>() - i64::from(age)).max(1) as u64
}
fn birth_recipe(r: &mut ChaCha8Rng, adjustment: i64) -> u64 {
    let mut factor = 1000 + i64::from(draw(r, 1000));
    let mut multiplier = 1;
    while multiplier < 6 && draw(r, 4) == 0 {
        multiplier += 1;
    }
    factor *= multiplier;
    let noise = if draw(r, 2) != 0 {
        adjustment * factor / 1000
    } else {
        adjustment * 1000 / factor
    };
    let rot = 250 + noise - adjustment;
    for delay in 2..=50 {
        if draw(r, 37) == 0 {
            return delay;
        }
    }
    rot as u64
}
fn same_rng(w: &SimulationWorld, r: &ChaCha8Rng) {
    assert_eq!(w.rng.clone().next_u64(), r.clone().next_u64());
}
fn revived(w: &SimulationWorld) -> &nethacked_arena::ActorRecord {
    let actors: Vec<_> = w
        .arena
        .actors
        .iter()
        .filter(|(id, a)| *id != w.player_id && !a.is_dead)
        .collect();
    assert_eq!(actors.len(), 1);
    actors[0].1
}
// Independent coordinate oracle: source row-major clipped rings, then forward
// Fisher-Yates; each near ring is shuffled before ANY placement evaluation.
fn rings(rng: &mut ChaCha8Rng, origin: Coord, max: usize) -> Vec<Coord> {
    let mut coords = Vec::new();
    for r in 1..=max {
        let mut ring = Vec::new();
        for y in 0..ROWNO {
            for x in 1..COLNO {
                if x.abs_diff(origin.x).max(y.abs_diff(origin.y)) == r {
                    ring.push(Coord::new_unchecked(x, y));
                }
            }
        }
        for offset in 0..ring.len().saturating_sub(1) {
            let j = offset + draw(rng, (ring.len() - offset) as u32) as usize;
            ring.swap(offset, j);
        }
        coords.extend(ring);
    }
    coords
}
#[test]
fn literal_adjusted_levels_and_allocation_hp_sex_order() {
    use nethacked_data::monsters::ordinary_adjusted_level as adj;
    for (base, depth, hero, expected) in [
        (7, 10, 18, 9),
        (7, 1, 1, 6),
        (7, 100, 100, 10),
        (0, 1, 1, 0),
        (1, 1, 100, 1),
        (7, 7, 7, 7),
    ] {
        assert_eq!(adj(base, depth, hero), expected);
    }
    for flags in [0, 1, 2, 3] {
        let mut w = world(61);
        let id = corpse(&mut w, ItemLocation::Floor(ORIGIN));
        w.arena.items[id].enchantment = flags;
        let mut r = w.rng.clone();
        let expected = hp(&mut r, 9, flags == 0 || flags == 3);
        assert_eq!(expected, 41); // literal nine-d8 native oracle, seed 61
        let out = run(&mut w, id);
        let a = revived(&out);
        assert_eq!(
            a.movement, 0,
            "fresh revival misses allocation on direct callback"
        );
        assert_eq!(
            (a.coord, a.level, a.hp, a.max_hp),
            (ORIGIN, 9, expected, expected)
        );
        assert!(!a.is_peaceful);
        assert!(!out.arena.items.contains_key(id));
        same_rng(&out, &r);
        assert_eq!(out.scheduler.turn, 73);
        assert!(matches!(
            out.event_log.last(),
            Some(GameEvent::ObjectTimerExpired { .. })
        ));
        if flags == 1 {
            assert_eq!(a.sex, Some(ActorSex::Female));
        }
        if flags == 2 {
            assert_eq!(a.sex, Some(ActorSex::Male));
        }
    }
    // Explicit makemon on an unoccupied nonhero origin has no goodpos gate.
    let mut w = world(61);
    w.level.set_tile(ORIGIN, Tile::Stone);
    let id = corpse(&mut w, ItemLocation::Floor(ORIGIN));
    let mut r = w.rng.clone();
    hp(&mut r, 9, true);
    let out = run(&mut w, id);
    assert_eq!(revived(&out).coord, ORIGIN);
    same_rng(&out, &r);
}
#[test]
fn saved_traits_heal_restore_and_repair() {
    for seed in 0..32 {
        for low in [false, true] {
            let mut w = world(seed);
            if low {
                w.depth = 1;
                w.arena.actors[w.player_id].level = 1;
            }
            let id = corpse(&mut w, ItemLocation::Floor(ORIGIN));
            let mut saved = w
                .ruleset
                .monster_by_id(M::TROLL)
                .unwrap()
                .create_record_with_hp(ORIGIN, 0, 85, 13);
            saved.movement = 23;
            saved.name = "individual".into();
            saved.sex = None;
            saved.is_dead = true;
            saved.is_fleeing = true;
            saved.is_cancelled = true;
            saved.is_peaceful = true;
            saved.ac = -4;
            saved.mspec_used = 7;
            saved.malign = -9;
            saved.intrinsics.fire_resistance = true;
            saved.intrinsics.blind = true;
            w.arena.items[id].enchantment = 1;
            w.arena.items[id].saved_monster_traits = Some(Box::new(SavedMonsterTraits {
                actor: saved.clone(),
            }));
            let mut r = w.rng.clone();
            let mut sampled = hp(&mut r, if low { 6 } else { 9 }, true);
            let mut level = 13;
            if low {
                let repair = draw(&mut r, 8) + 1;
                if repair > 6 {
                    sampled += (6..repair).map(|_| draw(&mut r, 8) + 1).sum::<u32>();
                    level = repair;
                }
            }
            saved.coord = ORIGIN;
            saved.level = level;
            saved.max_hp = 85.max(sampled);
            saved.hp = saved.max_hp;
            saved.is_dead = false;
            saved.is_fleeing = false;
            saved.is_cancelled = false;
            saved.intrinsics.blind = false;
            let out = run(&mut w, id);
            assert_eq!(revived(&out), &saved);
            same_rng(&out, &r);
        }
    }
    let mut w = world(61);
    let id = corpse(&mut w, ItemLocation::Floor(ORIGIN));
    let mut saved = w
        .ruleset
        .monster_by_id(M::JACKAL)
        .unwrap()
        .create_record_with_hp(ORIGIN, 0, 85, 13);
    saved.name = "misleading troll name".into();
    saved.sex = None;
    saved.is_dead = true;
    w.arena.items[id].saved_monster_traits = Some(Box::new(SavedMonsterTraits {
        actor: saved.clone(),
    }));
    let mut r = w.rng.clone();
    hp(&mut r, 1, true);
    saved.hp = 85;
    saved.is_dead = false;
    let out = run(&mut w, id);
    assert_eq!(revived(&out), &saved);
    same_rng(&out, &r);
}
#[test]
fn carried_and_occupied_complete_ring_placement() {
    for mode in ["hero", "monster", "occupied"] {
        let mut w = world(61);
        let mut holder = w.player_id;
        if mode != "hero" {
            holder = w.arena.spawn_actor(
                w.ruleset
                    .monster_by_id(M::HUMAN)
                    .unwrap()
                    .create_record_with_hp(ORIGIN, 10, 10, 1),
            );
        }
        let center = w.arena.actors[holder].coord;
        let location = if mode == "occupied" {
            ItemLocation::Floor(center)
        } else {
            ItemLocation::CarriedBy(holder)
        };
        let id = corpse(&mut w, location);
        let mut r = w.rng.clone();
        let near = rings(&mut r, center, 3);
        assert_eq!(near.len(), 48);
        let expected_hp = hp(&mut r, 9, true);
        let out = run(&mut w, id);
        let a = out
            .arena
            .actors
            .iter()
            .find(|(_, a)| {
                a.species == Some(nethacked_arena::ActorSpecies::Vanilla("TROLL".into()))
            })
            .unwrap()
            .1;
        assert_eq!((a.coord, a.hp), (near[0], expected_hp));
        same_rng(&out, &r);
        assert_eq!(out.arena.actors.len(), w.arena.actors.len() + 1);
    }
    // Owned stairs survive goodpos. Boulder blocks the closest stair; every
    // whole-map ring is shuffled again before the far legal spot is evaluated.
    for obstruction in ["boulder", "scroll", "Elbereth"] {
        let mut w = world(61);
        w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Stone);
        let near = Coord::new_unchecked(21, 10);
        let far = Coord::new_unchecked(30, 10);
        w.level.set_tile(near, Tile::Stairs { up: true });
        w.arena.spawn_actor(
            w.ruleset
                .monster_by_id(M::HUMAN)
                .unwrap()
                .create_record_with_hp(ORIGIN, 10, 10, 1),
        );
        if obstruction == "boulder" {
            container(&mut w, K::BOULDER, ItemLocation::Floor(near));
            w.level.set_tile(far, Tile::Room);
        } else if obstruction == "scroll" {
            container(&mut w, K::SCR_SCARE_MONSTER, ItemLocation::Floor(near));
        } else {
            w.level.set_engraving(
                near,
                nethacked_core::engraving::Engraving::new(
                    "Elbereth",
                    nethacked_core::engraving::EngravingMedium::Burned,
                ),
            );
        }
        let id = corpse(&mut w, ItemLocation::Floor(ORIGIN));
        let mut r = w.rng.clone();
        rings(&mut r, ORIGIN, 3);
        rings(&mut r, ORIGIN, COLNO);
        if obstruction != "boulder" {
            rings(&mut r, ORIGIN, 3);
        }
        let expected_hp = hp(&mut r, 9, true);
        let out = run(&mut w, id);
        let a = out
            .arena
            .actors
            .iter()
            .find(|(_, a)| {
                a.species == Some(nethacked_arena::ActorSpecies::Vanilla("TROLL".into()))
            })
            .unwrap()
            .1;
        assert_eq!(
            (a.coord, a.hp),
            (
                if obstruction == "boulder" { far } else { near },
                expected_hp
            )
        );
        same_rng(&out, &r);
    }
}
#[test]
fn immediate_container_guards_and_nesting() {
    for (layers, locked, kind, success) in [
        (1, false, K::SACK, true),
        (3, false, K::SACK, true),
        (4, false, K::SACK, false),
        (1, true, K::SACK, false),
        (1, false, K::STATUE, false),
        (1, false, K::BAG_OF_HOLDING, false),
    ] {
        let mut w = world(61);
        let mut parent = container(&mut w, K::SACK, ItemLocation::Floor(ORIGIN));
        for _ in 1..layers {
            parent = container(&mut w, K::SACK, ItemLocation::InContainer(parent));
        }
        w.arena.items[parent].locked = locked;
        if kind != K::SACK {
            let old = w.arena.items[parent].location.clone();
            w.arena.items[parent] = w
                .ruleset
                .create_item_record_by_id(kind, old, Buc::Uncursed)
                .unwrap();
        }
        let id = corpse(&mut w, ItemLocation::InContainer(parent));
        let mut r = w.rng.clone();
        if kind == K::BAG_OF_HOLDING {
            assert_ne!(draw(&mut r, 40), 0);
        }
        let h = if success {
            hp(&mut r, 9, true)
        } else {
            rot(&mut r, 0) as u32
        };
        let out = run(&mut w, id);
        assert_eq!(out.arena.items.contains_key(id), !success);
        assert!(out.arena.items.contains_key(parent));
        same_rng(&out, &r);
        if success {
            assert_eq!(revived(&out).hp, h);
            assert!(out.arena.items_in_container(parent).is_empty());
        } else {
            assert_eq!(out.object_timers.entries()[0].due, 73 + u64::from(h));
        }
    }
    // An outer locked bag doesn't invoke immediate-container guards or RNG.
    let mut w = world(61);
    let outer = container(&mut w, K::BAG_OF_HOLDING, ItemLocation::Floor(ORIGIN));
    w.arena.items[outer].locked = true;
    let inner = container(&mut w, K::SACK, ItemLocation::InContainer(outer));
    let id = corpse(&mut w, ItemLocation::InContainer(inner));
    let mut r = w.rng.clone();
    hp(&mut r, 9, true);
    let out = run(&mut w, id);
    assert!(!out.arena.items.contains_key(id));
    same_rng(&out, &r);
    let seed = (0..300)
        .find(|seed| draw(&mut ChaCha8Rng::seed_from_u64(*seed), 40) == 0)
        .unwrap();
    let mut w = world(seed);
    let bag = container(&mut w, K::BAG_OF_HOLDING, ItemLocation::Floor(ORIGIN));
    let id = corpse(&mut w, ItemLocation::InContainer(bag));
    let mut r = w.rng.clone();
    assert_eq!(draw(&mut r, 40), 0);
    hp(&mut r, 9, true);
    let out = run(&mut w, id);
    assert!(!out.arena.items.contains_key(id));
    assert!(out.arena.items.contains_key(bag));
    same_rng(&out, &r);
}
#[test]
fn failure_rot_and_existing_rot_draw_only() {
    for mode in [
        "limbo",
        "norevive",
        "genocide",
        "blocked",
        "existing",
        "ancient",
        "occupied_norevive",
        "hero_genocide",
    ] {
        let mut w = world(61);
        let mut origin = ORIGIN;
        if mode == "hero_genocide" || mode == "blocked" {
            origin = w.arena.actors[w.player_id].coord;
        }
        let id = corpse(
            &mut w,
            if mode == "limbo" {
                ItemLocation::Limbo
            } else {
                ItemLocation::Floor(origin)
            },
        );
        if mode == "norevive"
            || mode == "occupied_norevive"
            || mode == "existing"
            || mode == "ancient"
        {
            w.arena.items[id].corpse_no_revive = true;
        }
        if mode == "genocide" || mode == "hero_genocide" {
            w.genocide_registry.genocided_species.insert("troll".into());
        }
        if mode == "ancient" {
            w.arena.items[id].corpse_age = 300;
        }
        if mode == "occupied_norevive" {
            w.arena.spawn_actor(
                w.ruleset
                    .monster_by_id(M::HUMAN)
                    .unwrap()
                    .create_record_with_hp(origin, 10, 10, 1),
            );
        }
        if mode == "blocked" {
            w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Stone);
        }
        let mut r = w.rng.clone();
        if mode == "occupied_norevive" || mode == "hero_genocide" {
            rings(&mut r, origin, 3);
        }
        if mode == "blocked" {
            for _ in 0..2 {
                rings(&mut r, origin, 3);
                rings(&mut r, origin, COLNO);
            }
        }
        if mode == "existing" {
            w.start_object_timer(Target::Active(id), Kind::RotCorpse, 500)
                .unwrap();
        }
        let delay = rot(&mut r, w.arena.items[id].corpse_age);
        let out = run(&mut w, id);
        same_rng(&out, &r);
        assert!(out.arena.items.contains_key(id));
        assert_eq!(out.arena.items[id].corpse_age, w.arena.items[id].corpse_age);
        assert_eq!(out.object_timers.entries().len(), 1);
        assert_eq!(
            out.object_timers.entries()[0].due,
            73 + if mode == "existing" { 500 } else { delay }
        );
        assert_eq!(
            out.object_timers.entries()[0].serial,
            if mode == "existing" { 0 } else { 1 }
        );
    }
}
#[test]
fn stack_copies_timer_serials_and_preserves_references() {
    for quantity in [1, 3] {
        let mut w = world(61);
        let id = corpse(&mut w, ItemLocation::Floor(ORIGIN));
        w.arena.items[id].quantity = quantity;
        w.wielded_item = Some(id);
        w.hero.quivered_item = Some(id);
        w.unpaid_items.push((id, 12));
        w.start_object_timer(Target::Active(id), Kind::RotCorpse, 500)
            .unwrap();
        let mut r = w.rng.clone();
        hp(&mut r, 9, true);
        if quantity > 1 {
            draw(&mut r, 2);
        }
        let out = run(&mut w, id);
        same_rng(&out, &r);
        assert_eq!(out.arena.actors.len(), 2);
        let value: serde_json::Value = serde_json::from_str(&out.to_save_json().unwrap()).unwrap();
        assert_eq!(
            value["object_timers"]["next_serial"],
            if quantity > 1 { 3 } else { 2 }
        );
        if quantity > 1 {
            assert_eq!(out.arena.items[id].quantity, 2);
            assert_eq!(out.object_timers.entries()[0].serial, 0);
            assert_eq!(out.wielded_item, Some(id));
            assert_eq!(out.hero.quivered_item, Some(id));
            assert!(out.is_unpaid(id));
        } else {
            assert!(!out.arena.items.contains_key(id));
            assert!(out.object_timers.entries().is_empty());
            assert_eq!(out.wielded_item, None);
            assert_eq!(out.hero.quivered_item, None);
            assert!(!out.is_unpaid(id));
        }
    }
}
#[test]
fn full_save_and_cache_arrival_remap_one_clock() {
    // Exercise nontrivial live allocator holes before an ordinary world tick.
    // IDs/iteration match across save; this does not claim C global ID parity.
    let mut live = world(61);
    let mut keys = Vec::new();
    for x in 40..48 {
        keys.push(
            live.arena.spawn_actor(
                live.ruleset
                    .monster_by_id(M::HUMAN)
                    .unwrap()
                    .create_record_with_hp(Coord::new_unchecked(x, 15), 10, 10, 1),
            ),
        );
    }
    for index in [6, 1, 4] {
        live.arena.actors.remove(keys[index]);
    }
    let id = corpse(&mut live, ItemLocation::Floor(ORIGIN));
    live.start_object_timer(Target::Active(id), Kind::ReviveMonster, 1)
        .unwrap();
    let mut restored = load(&live);
    assert_eq!(
        live.step_player_action(ActionAst::Wait),
        restored.step_player_action(ActionAst::Wait)
    );
    assert_eq!(
        live.arena.actors.iter().collect::<Vec<_>>(),
        restored.arena.actors.iter().collect::<Vec<_>>()
    );
    assert_eq!(save(&live), save(&restored));
    assert!(live
        .arena
        .actors
        .iter()
        .any(|(_, a)| a.name == "troll" && a.movement == 0));
    let mut w = world(61);
    let id = corpse(&mut w, ItemLocation::Floor(ORIGIN));
    w.start_object_timer(Target::Active(id), Kind::ReviveMonster, 2)
        .unwrap();
    w.stored_levels.push((
        (BranchId::DungeonsOfDoom, 11),
        StoredLevel {
            level: w.level.clone(),
            monsters: vec![],
            items: vec![],
            unpaid_items: vec![],
        },
    ));
    w.level
        .set_tile(w.level.stairs_down, Tile::Stairs { up: false });
    w.arena.actors[w.player_id].coord = w.level.stairs_down;
    w.step_player_action(ActionAst::Descend);
    assert_eq!(w.scheduler.turn, 74);
    assert!(matches!(
        w.object_timers.entries()[0].target,
        Target::Cached { .. }
    ));
    let mut copy = load(&w);
    let mut r = w.rng.clone();
    hp(&mut r, 9, true);
    draw(&mut r, 12); // P09 arrival revival precedes this action's allocation.
    for s in [&mut w, &mut copy] {
        s.scheduler.turn = 76;
        s.level
            .set_tile(s.level.stairs_up, Tile::Stairs { up: true });
        s.arena.actors[s.player_id].coord = s.level.stairs_up;
    }
    assert_eq!(
        w.step_player_action(ActionAst::Ascend),
        copy.step_player_action(ActionAst::Ascend)
    );
    assert_eq!(w.scheduler.turn, 77);
    assert!(w.object_timers.entries().is_empty());
    assert_eq!(w.arena.actors.len(), 2);
    assert_eq!(save(&w), save(&copy));
    same_rng(&w, &r);
    let a = revived(&w);
    assert_eq!((a.coord, a.level), (ORIGIN, 9));
    assert_eq!(
        a.movement, 12,
        "arriving fresh troll joins the allocation after its zero-credit pass"
    );
    let arriving = w
        .arena
        .actors
        .iter()
        .find(|(_, a)| a.name == "troll")
        .unwrap()
        .0;
    let mut restored = load(&w);
    assert_eq!(
        w.step_player_action(ActionAst::Wait),
        restored.step_player_action(ActionAst::Wait)
    );
    assert_eq!(save(&w), save(&restored));
    assert_ne!(
        w.arena.actors[arriving].coord, ORIGIN,
        "the next public action spends arriving credit"
    );
    assert_eq!(w.arena.actors[arriving].movement, 12);
}
#[test]
fn actual_death_generated_and_wish_revival() {
    for mode in ["death", "generated", "wish"] {
        let mut w = world(928);
        let mut rules = (*w.ruleset).clone();
        for d in &mut rules.monsters {
            d.frequency = 0;
            d.gen_flags.push("nogen".into());
        }
        let d = rules
            .monsters
            .iter_mut()
            .find(|d| d.id == Some(M::TROLL))
            .unwrap();
        d.frequency = 1;
        d.difficulty = 5;
        d.gen_flags.clear();
        rules.reindex();
        w.ruleset = Arc::new(rules);
        let mut birth_rng = w.rng.clone();
        draw(&mut birth_rng, 1);
        draw(&mut birth_rng, 2);
        let mut delay = birth_recipe(&mut birth_rng, if mode == "generated" { 25 } else { 10 });
        if mode == "death" {
            delay = birth_recipe(&mut birth_rng, 10);
        }
        if mode == "death" {
            let mut a = w
                .ruleset
                .monster_by_id(M::TROLL)
                .unwrap()
                .create_record_with_hp(ORIGIN, 0, 8, 13);
            a.is_dead = true;
            a.sex = Some(ActorSex::Female);
            a.intrinsics.blind = true;
            a.intrinsics.fire_resistance = true;
            let actor = w.arena.spawn_actor(a);
            w.on_actor_killed(actor, actor, &mut vec![]);
        } else if mode == "wish" {
            let hero = w.player_id;
            container(&mut w, K::WAN_WISHING, ItemLocation::CarriedBy(hero));
            w.step_player_action(ActionAst::Wish("corpse".into()));
        } else {
            use nethacked_data::{
                containers::ContainerContext,
                generation::{DungeonAlign, MonsterGenContext},
            };
            let rs = w.ruleset.clone();
            let tree = rs
                .create_random_item_tree_with_monsters(
                    "corpse",
                    ItemLocation::Limbo,
                    ContainerContext {
                        hero_level: 18,
                        level_difficulty: 10,
                        moves: 73,
                        in_mklev: true,
                    },
                    &MonsterGenContext {
                        hero_level: 18,
                        level_difficulty: 10,
                        in_hell: false,
                        align: DungeonAlign::None,
                        temperature: 0,
                    },
                    |_| false,
                    |n| draw(&mut w.rng, n),
                )
                .unwrap();
            w.attach_generated_item(tree, ItemLocation::Floor(ORIGIN))
                .unwrap();
        }
        let (id, item) = w.arena.items.iter().find(|(_, i)| i.is_corpse()).unwrap();
        assert_eq!(
            item.corpse_species,
            Some(CorpseSpecies::Vanilla("TROLL".into()))
        );
        let t = *w.object_timers.entries().first().unwrap();
        assert_eq!(
            (t.due, t.serial),
            (73 + delay, if mode == "death" { 1 } else { 0 })
        );
        same_rng(&w, &birth_rng);
        if mode == "wish" {
            assert_eq!(item.enchantment, 0);
        }
        assert_eq!(
            t.kind,
            Kind::ReviveMonster,
            "fixture requires actual sampled revival: {mode}"
        );
        let mut r = w.rng.clone();
        let origin = if mode == "wish" {
            w.arena.actors[w.player_id].coord
        } else {
            ORIGIN
        };
        let coord = if mode == "wish" {
            rings(&mut r, origin, 3)[0]
        } else {
            ORIGIN
        };
        let expected_hp = hp(&mut r, 9, mode != "generated");
        let mut copy = load(&w);
        for s in [&mut w, &mut copy] {
            s.scheduler.turn = t.due - 1;
        }
        let events = w.step_player_action(ActionAst::Wait);
        assert_eq!(events, copy.step_player_action(ActionAst::Wait));
        assert!(!w.arena.items.contains_key(id));
        assert_eq!(w.scheduler.turn, t.due);
        let a = revived(&w);
        assert_eq!((a.coord, a.hp), (coord, expected_hp));
        if mode == "death" {
            assert_eq!(a.level, 13);
            assert_eq!(a.sex, Some(ActorSex::Female));
            assert!(!a.intrinsics.blind);
            assert!(a.intrinsics.fire_resistance);
        }
        assert_eq!(save(&w), save(&copy));
        same_rng(&w, &r);
    }
}
