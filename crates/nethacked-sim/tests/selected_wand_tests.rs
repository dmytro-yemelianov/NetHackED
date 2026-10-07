//! Backend selected-wand fixtures; accepted public regression helpers reused below.
use nethacked_arena::{ActorId, ActorSex, ActorSpecies, ItemDefinition, ItemId, ItemLocation};
use nethacked_data::{CharacterConfig, ItemKindId as I, MonsterSpeciesId as M, RoleId, Ruleset};
use nethacked_sim::{ActionAst, GameEvent, SimulationWorld};
use nethacked_types::{BranchId, Buc, Coord, Direction, Tile, COLNO, ROWNO};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::{collections::HashSet, sync::Arc};

const HERO: Coord = Coord::new_unchecked(20, 10);

fn api1_canonical_save(w: &SimulationWorld) -> Vec<u8> {
    let value: serde_json::Value = serde_json::from_str(&w.to_save_json().unwrap()).unwrap();
    serde_json::to_vec(&value).unwrap()
}

#[test]
fn api1_positive_boundaries_preserve_selected_generation_recharges_and_no_charge_draw() {
    // F14: source literals, including old-save values above SPE_LIM99.
    for custom in [false, true] {
        for (charge, post) in [(1, 0), (99, 98), (127, 126)] {
            let mut w = identity_world(identity_rules());
            let first = spawn(&mut w, I::WAN_FIRE, 7, Buc::Uncursed);
            let chosen = spawn(&mut w, I::WAN_FIRE, charge, Buc::Uncursed);
            if custom {
                w.arena.items[chosen].definition = Some(ItemDefinition::Custom("pack rod".into()));
                w.arena.items[chosen].name = "Pack ROD".into();
            }
            w.arena.items[chosen].recharged = 3;
            ready(&mut w);
            let first_record = w.arena.items[first].clone();
            let mut expected_record = w.arena.items[chosen].clone();
            expected_record.enchantment = post;
            let rng = w.rng.clone(); // Empty beam and healthy hero ticks have no draws.
            let mut action = selected(&w, chosen, true);
            if let ActionAst::ZapSelectedWand { energy, .. } = &mut action {
                *energy = 6;
            }
            let mut restored = SimulationWorld::from_save_json(
                &w.to_save_json().unwrap(),
                w.ruleset.clone(),
                &w.ruleset_ref,
            )
            .unwrap();
            let events = w.step_player_action(action.clone());
            assert_eq!(events, restored.step_player_action(action));
            let expected_path: Vec<_> =
                (4..10).rev().map(|y| Coord::new_unchecked(20, y)).collect();
            assert_eq!(
                events,
                vec![
                    GameEvent::BeamPropagated {
                        path: expected_path
                    },
                    GameEvent::TurnAdvanced { turn: 1 }
                ]
            );
            assert_eq!(w.rng, rng);
            assert_eq!(w.scheduler.turn, 1);
            assert_eq!(w.scheduler.hero_energy, 12);
            assert_eq!(w.arena.items[first], first_record);
            assert_eq!(w.arena.items[chosen], expected_record);
            assert_eq!(api1_canonical_save(&w), api1_canonical_save(&restored));
            assert_eq!(w.rng, restored.rng);
        }
    }
}

#[test]
fn api1_active_cached_signed_oldsave_roundtrip_and_public_continuation() {
    let mut original = identity_world(identity_rules());
    let mut active = Vec::new();
    let mut cached = Vec::new();
    // Negative boundary and over-normal positive boundary must not be clamped.
    for charge in [-128, -1, 0, 1, 99, 127] {
        let id = spawn(&mut original, I::WAN_FIRE, charge, Buc::Uncursed);
        original.arena.items[id].recharged = 3;
        active.push((id, charge));
        let cached_id = spawn(&mut original, I::WAN_FIRE, charge, Buc::Uncursed);
        let mut record = original.arena.items.remove(cached_id).unwrap();
        record.location = ItemLocation::Floor(HERO);
        record.recharged = 3;
        cached.push((cached_id, record));
    }
    original.stored_levels.push((
        (BranchId::DungeonsOfDoom, 8),
        nethacked_sim::world::StoredLevel {
            level: original.level.clone(),
            monsters: vec![],
            items: cached,
            unpaid_items: vec![],
        },
    ));
    ready(&mut original);
    for oldsave in [false, true] {
        let mut wire: serde_json::Value =
            serde_json::from_str(&original.to_save_json().unwrap()).unwrap();
        if oldsave {
            // Strip only optional identity keys from the native save.
            // Existing S1 setup and assertions remain untouched.
            fn strip(value: &mut serde_json::Value) {
                match value {
                    serde_json::Value::Object(map) => {
                        map.remove("definition");
                        for child in map.values_mut() {
                            strip(child);
                        }
                    }
                    serde_json::Value::Array(array) => {
                        for child in array {
                            strip(child);
                        }
                    }
                    _ => {}
                }
            }
            strip(&mut wire);
        }
        let mut direct = SimulationWorld::from_save_json(
            &wire.to_string(),
            original.ruleset.clone(),
            &original.ruleset_ref,
        )
        .unwrap();
        let roundtrip: serde_json::Value =
            serde_json::from_str(&direct.to_save_json().unwrap()).unwrap();
        assert_eq!(roundtrip, wire);
        assert_eq!(direct.rng, original.rng);
        for ((id, charge), (_, record)) in active.iter().zip(&direct.stored_levels[0].1.items) {
            assert_eq!(direct.arena.items[*id].enchantment, *charge);
            assert_eq!(record.enchantment, *charge);
            assert_eq!(record.recharged, 3);
            assert_eq!(direct.arena.items[*id].recharged, 3);
            assert_eq!(direct.arena.items[*id].definition.is_none(), oldsave);
            assert_eq!(record.definition.is_none(), oldsave);
        }
        let cached_before = serde_json::to_value(&direct.stored_levels).unwrap();
        let mut restored = SimulationWorld::from_save_json(
            &direct.to_save_json().unwrap(),
            direct.ruleset.clone(),
            &direct.ruleset_ref,
        )
        .unwrap();
        // Cached objects are absent from the carried snapshot and cannot be selected.
        assert_eq!(direct.arena.items_carried_by(direct.player_id).len(), 6);
        for (index, expected) in [(3, 0), (4, 98), (5, 126)] {
            let id = active[index].0;
            let rng = direct.rng.clone();
            let turn = direct.scheduler.turn;
            let mut action = selected(&direct, id, true);
            if let ActionAst::ZapSelectedWand { energy, .. } = &mut action {
                *energy = 6;
            }
            assert_eq!(
                direct.step_player_action(action.clone()),
                restored.step_player_action(action)
            );
            assert_eq!(direct.arena.items[id].enchantment, expected);
            assert_eq!(direct.arena.items[id].recharged, 3);
            assert_eq!(direct.rng, rng);
            assert_eq!(direct.rng, restored.rng);
            assert_eq!(direct.scheduler.turn, turn + 1);
            assert_eq!(direct.scheduler.hero_energy, 12);
            assert_eq!(api1_canonical_save(&direct), api1_canonical_save(&restored));
            assert_eq!(
                serde_json::to_value(&direct.stored_levels).unwrap(),
                cached_before
            );
            restored = SimulationWorld::from_save_json(
                &restored.to_save_json().unwrap(),
                restored.ruleset.clone(),
                &restored.ruleset_ref,
            )
            .unwrap();
        }
        for (id, charge) in &active[..3] {
            assert_eq!(direct.arena.items[*id].enchantment, *charge);
        }
        let rng = direct.rng.clone();
        assert_eq!(
            direct.step_player_action(ActionAst::Wait),
            restored.step_player_action(ActionAst::Wait)
        );
        assert_eq!(direct.rng, rng);
        assert_eq!(direct.rng, restored.rng);
        assert_eq!(api1_canonical_save(&direct), api1_canonical_save(&restored));
        assert_eq!(
            serde_json::to_value(&direct.stored_levels).unwrap(),
            cached_before
        );
    }
}

fn rules() -> Ruleset {
    let mut rs = (*Ruleset::vanilla()).clone();
    for d in &mut rs.monsters {
        d.frequency = 0;
    }
    let d = rs
        .monsters
        .iter_mut()
        .find(|d| d.id == Some(M::GIANT_ANT))
        .unwrap();
    d.id = None;
    d.name = "Owned candidate".into();
    d.glyph = 'c';
    d.frequency = 1;
    d.difficulty = 1;
    d.level = 2;
    d.sample_hp = true;
    d.speed = 0;
    d.flags = vec!["female".into()];
    d.gen_flags = vec![];
    d.always_hostile = true;
    d.peaceful_by_default = false;
    d.m2_race = None;
    d.maligntyp = 0;
    d.is_unique = false;
    d.abilities.clear();
    rs.items
        .iter_mut()
        .find(|d| d.id == Some(I::WAN_CREATE_MONSTER))
        .unwrap()
        .name = "Owned rod".into();
    rs.roles
        .iter_mut()
        .find(|d| d.id == RoleId::Healer)
        .unwrap()
        .name = "Owned profession".into();
    rs.monsters
        .iter_mut()
        .find(|d| d.id == Some(M::HEALER))
        .unwrap()
        .name = "Owned original healer".into();
    rs.reindex();
    rs
}

fn world(rs: Ruleset, seed: u64) -> SimulationWorld {
    let config = CharacterConfig {
        role: RoleId::Healer,
        ..Default::default()
    };
    let mut w = SimulationWorld::new_with_character_and_ruleset(
        seed,
        config,
        Arc::new(rs),
        nethacked_data::ruleset::RulesetRef::vanilla(),
    );
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.arena.items.clear();
    w.equipment = Default::default();
    w.wielded_item = None;
    w.unpaid_items.clear();
    w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Room);
    let hero = &mut w.arena.actors[w.player_id];
    hero.coord = HERO;
    hero.level = 2;
    hero.hp = 1000;
    hero.max_hp = 1000;
    hero.intrinsics.fast = false;
    hero.intrinsics.very_fast = false;
    w.current_branch = BranchId::DungeonsOfDoom;
    w.depth = 7;
    w.scheduler.hero_energy = 12;
    w.scheduler.hero_speed = 12;
    w.scheduler.turn = 0;
    let mut wand = w
        .ruleset
        .create_item_record(
            "Owned rod",
            ItemLocation::CarriedBy(w.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    wand.enchantment = 3;
    w.arena.spawn_item(wand);
    w.rng = ChaCha8Rng::seed_from_u64(seed);
    w
}
fn save(w: &SimulationWorld) -> serde_json::Value {
    serde_json::from_str(&w.to_save_json().unwrap()).unwrap()
}
fn load(w: &SimulationWorld) -> SimulationWorld {
    SimulationWorld::from_save_json(
        &w.to_save_json().unwrap(),
        w.ruleset.clone(),
        &w.ruleset_ref,
    )
    .unwrap()
}
fn actors(w: &SimulationWorld) -> Vec<(ActorId, nethacked_arena::ActorRecord)> {
    w.arena
        .actors
        .iter()
        .filter(|(id, _)| *id != w.player_id)
        .map(|(id, a)| (id, a.clone()))
        .collect()
}
// Independent perimeter construction; source y-major ring order, no production rings/goodpos calls.
fn shuffled(rng: &mut ChaCha8Rng, max: i32) -> Vec<Coord> {
    let mut result = Vec::new();
    for radius in 1..=max {
        let mut ring = Vec::new();
        for dy in -radius..=radius {
            let xs: Vec<i32> = if dy.abs() == radius {
                (-radius..=radius).collect()
            } else {
                vec![-radius, radius]
            };
            for dx in xs {
                let (x, y) = (20 + dx, 10 + dy);
                if x > 0 && x < COLNO as i32 && y >= 0 && y < ROWNO as i32 {
                    ring.push(Coord::new_unchecked(x as usize, y as usize));
                }
            }
        }
        for index in 0..ring.len().saturating_sub(1) {
            let offset = rng.random_range(0..(ring.len() - index) as u32) as usize;
            ring.swap(index, index + offset);
        }
        result.extend(ring);
    }
    result
}
fn placement(rng: &mut ChaCha8Rng, legal: impl Fn(Coord, bool) -> bool) -> Option<Coord> {
    for scary in [true, false] {
        let near = shuffled(rng, 3);
        if let Some(c) = near.iter().find(|&&c| legal(c, scary)) {
            return Some(*c);
        }
        let all = shuffled(rng, 80);
        if let Some(c) = all.into_iter().skip(near.len()).find(|&c| legal(c, scary)) {
            return Some(c);
        }
    }
    None
}
fn count(rng: &mut ChaCha8Rng) -> u32 {
    if rng.random_range(0..23u32) != 0 {
        1
    } else {
        rng.random_range(0..7u32) + 2
    }
}
fn seed_for(quantity: u32) -> u64 {
    (0..10000)
        .find(|&seed| count(&mut ChaCha8Rng::seed_from_u64(seed)) == quantity)
        .unwrap()
}
fn hp(rng: &mut ChaCha8Rng) -> u32 {
    let sum: u32 = (0..3).map(|_| rng.random_range(1..=8u32)).sum();
    sum + u32::from(sum == 3)
}
fn initialize(rng: &mut ChaCha8Rng, attempts: u32) -> u32 {
    for _ in 0..attempts {
        let _ = rng.random_range(0..1u32);
    }
    let _ = rng.random_range(0..2u32);
    hp(rng)
}
fn allocations(rng: &mut ChaCha8Rng, quantity: usize) {
    for _ in 0..quantity {
        let _ = rng.random_range(0..12u32);
    }
}

#[test]
fn selected_create_count_one_two_eight_preserves_native_draw_and_scheduler_order() {
    for quantity in [1, 2, 8] {
        let mut w = world(rules(), seed_for(quantity));
        w.arena.items.clear();
        let nonwand = spawn(&mut w, I::DAGGER, 0, Buc::Uncursed);
        let first = spawn(&mut w, I::WAN_FIRE, 7, Buc::Uncursed);
        let chosen = spawn(&mut w, I::WAN_CREATE_MONSTER, 2, Buc::Uncursed);
        let carried = w.arena.items_carried_by(w.player_id);
        let records = [nonwand, first, chosen].map(|id| w.arena.items[id].clone());
        for (&id, record) in carried.iter().zip(records) {
            w.arena.items[id] = record;
        }
        let [nonwand, first, chosen] = carried.as_slice() else {
            panic!("expected complete three-item snapshot");
        };
        let (nonwand, first, chosen) = (*nonwand, *first, *chosen);
        assert_eq!(
            w.arena.items[nonwand].class,
            nethacked_types::ItemClass::Weapon
        );
        assert_eq!(
            selected(&w, chosen, true),
            ActionAst::ZapSelectedWand {
                item_index: 2,
                expected_item: Some(chosen),
                dir: Direction::North,
                energy: 999,
            }
        );
        let action = selected(&w, chosen, true);
        let mut restored = load(&w);
        assert_eq!(save(&w), save(&restored));
        assert_eq!(w.rng, restored.rng);
        let mut oracle = w.rng.clone();
        assert_eq!(count(&mut oracle), quantity);
        let mut occupied = HashSet::new();
        occupied.insert(HERO);
        let mut expected = Vec::new();
        for _ in 0..quantity {
            let c = placement(&mut oracle, |c, _| !occupied.contains(&c)).unwrap();
            occupied.insert(c);
            expected.push((c, initialize(&mut oracle, 1)));
        }
        allocations(&mut oracle, quantity as usize);
        let turn = w.scheduler.turn;
        let events = w.step_player_action(action.clone());
        assert_eq!(events, restored.step_player_action(action));
        assert!(!events
            .iter()
            .any(|event| matches!(event, GameEvent::BeamPropagated { .. })));
        assert_eq!(save(&w), save(&restored));
        let actual = actors(&w);
        assert_eq!(actual.len(), quantity as usize); // No selected group flag in this leader/turn fixture
        for ((_, actor), (coord, hp)) in actual.iter().zip(expected) {
            assert_eq!(
                (actor.coord, actor.level, actor.hp, actor.max_hp),
                (coord, 3, hp, hp)
            );
            assert_eq!(actor.name, "Owned candidate");
            assert_eq!(
                actor.species,
                Some(ActorSpecies::Custom("owned candidate".into()))
            );
            assert_eq!(actor.sex, Some(ActorSex::Female));
            assert!(!actor.is_peaceful);
            assert_eq!(actor.movement, 0);
        }
        assert_eq!(w.rng, oracle);
        assert_eq!(w.scheduler.turn, turn + 1);
        assert_eq!(w.scheduler.hero_energy, 12);
        assert_eq!(w.arena.items[chosen].enchantment, 1);
        assert_eq!(w.arena.items[first].enchantment, 7);
        let mut next = load(&w);
        assert_eq!(
            w.step_player_action(ActionAst::Wait),
            next.step_player_action(ActionAst::Wait)
        );
        assert_eq!(save(&w), save(&next));
    }
}

fn identity_rules() -> Ruleset {
    let mut rules = (*Ruleset::vanilla()).clone();
    for m in &mut rules.monsters {
        m.frequency = 0;
    }
    let m = rules
        .monsters
        .iter_mut()
        .find(|m| m.id == Some(M::GIANT_ANT))
        .unwrap();
    m.name = "Owned candidate".into();
    m.frequency = 1;
    m.difficulty = 1;
    m.level = 1;
    m.sample_hp = false;
    m.base_hp = 71;
    m.max_hp = 83;
    m.speed = 0;
    m.gen_flags.clear();
    m.abilities.clear();
    let def = rules
        .items
        .iter_mut()
        .find(|d| d.id == Some(I::WAN_CREATE_MONSTER))
        .unwrap();
    def.name = "Owned rod".into();
    let mut custom = def.clone();
    custom.id = None;
    custom.name = "Pack ROD".into();
    rules.items.push(custom);
    rules.reindex();
    rules
}

fn identity_world(rules: Ruleset) -> SimulationWorld {
    let mut world = SimulationWorld::new_with_character_and_ruleset(
        33,
        CharacterConfig::default(),
        Arc::new(rules),
        nethacked_data::ruleset::RulesetRef::vanilla(),
    );
    world.arena.actors.retain(|id, _| id == world.player_id);
    world.arena.items.clear();
    world.equipment = Default::default();
    world.wielded_item = None;
    world.unpaid_items.clear();
    world.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Room);
    world.arena.actors[world.player_id].coord = HERO;
    world.arena.actors[world.player_id].level = 2;
    world.arena.actors[world.player_id].hp = 1000;
    world.arena.actors[world.player_id].max_hp = 1000;
    world.scheduler.turn = 0;
    world.scheduler.hero_energy = 12;
    world.scheduler.hero_speed = 12;
    world.depth = 7;
    world.rng = ChaCha8Rng::seed_from_u64(33);
    world
}

fn identity_wand(w: &mut SimulationWorld, custom: bool, display: &str) -> ItemId {
    let mut item = if custom {
        w.ruleset
            .create_item_record(
                "pack rod",
                ItemLocation::CarriedBy(w.player_id),
                Buc::Uncursed,
            )
            .unwrap()
    } else {
        w.ruleset
            .create_item_record_by_id(
                I::WAN_CREATE_MONSTER,
                ItemLocation::CarriedBy(w.player_id),
                Buc::Uncursed,
            )
            .unwrap()
    };
    item.name = display.into();
    item.enchantment = 2;
    w.arena.spawn_item(item)
}

fn spawn(w: &mut SimulationWorld, kind: I, charge: i8, buc: Buc) -> ItemId {
    let mut item = w
        .ruleset
        .create_item_record_by_id(kind, ItemLocation::CarriedBy(w.player_id), buc)
        .unwrap();
    item.enchantment = charge;
    w.arena.spawn_item(item)
}
fn selected(w: &SimulationWorld, id: ItemId, token: bool) -> ActionAst {
    ActionAst::ZapSelectedWand {
        item_index: w
            .arena
            .items_carried_by(w.player_id)
            .iter()
            .position(|&current| current == id)
            .unwrap()
            .try_into()
            .unwrap(),
        expected_item: token.then_some(id),
        dir: Direction::North,
        energy: 999,
    }
}
fn ready(w: &mut SimulationWorld) {
    // Reconcile through the public no-time path before taking the snapshot.
    assert_eq!(
        w.step_player_action(ActionAst::CancelZapSelection),
        vec![GameEvent::LogMessage {
            text: "Never mind.".into()
        }]
    );
    assert!(!w.arena.actors[w.player_id].is_dead);
    assert_eq!(w.hero.afflictions.transient.helpless, 0);
    assert!(w.equipment.dressing.is_none());
    assert_eq!(w.scheduler.hero_energy, 12);
}
fn no_time(w: &mut SimulationWorld, action: ActionAst, message: &str) {
    ready(w);
    let before = save(w);
    let rng = w.rng.clone();
    let events = w.step_player_action(action);
    assert_eq!(
        events,
        vec![GameEvent::LogMessage {
            text: message.into()
        }]
    );
    assert_eq!(w.rng, rng);
    let mut after = save(w);
    // Event recording is the only permitted canonical-world difference.
    assert_ne!(after["event_log"], before["event_log"]);
    after["event_log"] = before["event_log"].clone();
    assert_eq!(after, before);
}

#[test]
fn cancellation_two_charged_wands_has_only_exact_log() {
    let mut w = identity_world(identity_rules());
    spawn(&mut w, I::WAN_FIRE, 5, Buc::Uncursed);
    identity_wand(&mut w, false, "Owned rod");
    no_time(&mut w, ActionAst::CancelZapSelection, "Never mind.");
}

#[test]
fn range_token_class_and_identity_refusals_are_ordered_and_no_time() {
    let mut w = identity_world(identity_rules());
    let nonwand = spawn(&mut w, I::DAGGER, 0, Buc::Uncursed);
    let first = spawn(&mut w, I::WAN_FIRE, 5, Buc::Uncursed);
    let chosen = identity_wand(&mut w, false, "Owned rod");
    w.arena.items[nonwand].definition = Some(ItemDefinition::Custom("missing".into()));
    for index in [3, u32::MAX] {
        no_time(
            &mut w,
            ActionAst::ZapSelectedWand {
                item_index: index,
                expected_item: Some(first),
                dir: Direction::East,
                energy: 6,
            },
            "You don't have that object.",
        );
    }
    let nonwand_action = selected(&w, nonwand, true);
    no_time(&mut w, nonwand_action, "That is a silly thing to zap.");
    let mut wrong_token = selected(&w, nonwand, true);
    if let ActionAst::ZapSelectedWand { expected_item, .. } = &mut wrong_token {
        *expected_item = Some(chosen);
    }
    no_time(&mut w, wrong_token, "You don't have that object.");
    for key in [
        ItemDefinition::Vanilla("UNKNOWN_SYMBOL".into()),
        ItemDefinition::Custom("missing key".into()),
        ItemDefinition::Vanilla("WAN_CREATE_MONSTER".into()),
    ] {
        w.arena.items[chosen].definition = Some(key.clone());
        if key == ItemDefinition::Vanilla("WAN_CREATE_MONSTER".into()) {
            let mut rs = (*w.ruleset).clone();
            rs.items.retain(|d| d.id != Some(I::WAN_CREATE_MONSTER));
            rs.reindex();
            w.ruleset = Arc::new(rs);
        }
        let witness = w
            .ruleset
            .monster_by_id(M::GIANT_ANT)
            .unwrap()
            .create_record_with_hp(Coord::new_unchecked(21, 10), 77, 77, 1);
        w.arena.spawn_actor(witness);
        w.level
            .set_tile(Coord::new_unchecked(22, 10), Tile::Pool { frozen: false });
        let action = selected(&w, chosen, true);
        no_time(
            &mut w,
            action,
            "You cannot zap that wand: its definition is unavailable.",
        );
    }
}

#[test]
fn indirect_floor_moved_deleted_recycled_and_reordered_tokens_never_choose_first_wand() {
    for mode in [
        "container",
        "floor",
        "moved",
        "deleted",
        "recycled",
        "reordered",
    ] {
        let mut w = identity_world(identity_rules());
        let first = spawn(&mut w, I::WAN_FIRE, 5, Buc::Uncursed);
        let chosen = identity_wand(&mut w, false, "Owned rod");
        let records = [first, chosen].map(|id| w.arena.items[id].clone());
        let carried = w.arena.items_carried_by(w.player_id);
        for (&id, record) in carried.iter().zip(records) {
            w.arena.items[id] = record;
        }
        let (first, chosen) = (carried[0], carried[1]);
        let action = selected(&w, chosen, true);
        match mode {
            "container" => w.arena.items[chosen].location = ItemLocation::InContainer(first),
            "floor" => w.arena.items[chosen].location = ItemLocation::Floor(HERO),
            "moved" => w.arena.items[chosen].location = ItemLocation::Limbo,
            "deleted" => {
                w.arena.items.remove(chosen);
            }
            "recycled" => {
                w.arena.items.remove(chosen);
                let replacement = identity_wand(&mut w, false, "Owned rod");
                assert_ne!(replacement, chosen);
                assert_eq!(w.arena.items_carried_by(w.player_id).len(), 2);
            }
            "reordered" => {
                w.arena.items[first].location = ItemLocation::Limbo;
                assert_eq!(w.arena.items_carried_by(w.player_id), vec![chosen]);
            }
            _ => unreachable!(),
        }
        no_time(&mut w, action, "You don't have that object.");
    }
}

#[test]
fn index_only_uses_current_snapshot_without_stale_detection_claim() {
    let mut w = identity_world(identity_rules());
    let removed = spawn(&mut w, I::WAN_FIRE, 5, Buc::Uncursed);
    let chosen = identity_wand(&mut w, false, "Owned rod");
    w.arena.items.remove(removed);
    let action = selected(&w, chosen, false);
    w.step_player_action(action);
    assert_eq!(w.arena.items[chosen].enchantment, 1);
}

#[test]
fn canonical_and_custom_display_renames_keep_owned_effects_and_witnesses() {
    for custom in [false, true] {
        let mut a = identity_world(identity_rules());
        spawn(&mut a, I::WAN_FIRE, 7, Buc::Uncursed);
        let id = identity_wand(
            &mut a,
            custom,
            "wand of create monster cold death secret door",
        );
        let witness = a
            .ruleset
            .monster_by_id(M::GIANT_ANT)
            .unwrap()
            .create_record_with_hp(Coord::new_unchecked(22, 10), 77, 77, 1);
        let target = a.arena.spawn_actor(witness.clone());
        let mut b = load(&a);
        b.arena.items[id].name = if custom { "Pack ROD" } else { "Owned rod" }.into();
        let action = selected(&a, id, true);
        assert_eq!(
            a.step_player_action(action.clone()),
            b.step_player_action(action)
        );
        assert_eq!(a.rng, b.rng);
        assert_eq!(a.arena.actors[target], witness);
        assert_eq!(a.arena.items[id].enchantment, 1);
        assert_eq!(a.arena.actors.len(), if custom { 2 } else { 3 });
        if !custom {
            let created = a
                .arena
                .actors
                .iter()
                .find(|(key, _)| *key != a.player_id && *key != target)
                .unwrap()
                .1;
            assert_eq!(
                (created.name.as_str(), created.hp, created.max_hp),
                ("Owned candidate", 71, 83)
            );
        }
    }
}

fn omit_definitions(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("definition");
            for child in map.values_mut() {
                omit_definitions(child);
            }
        }
        serde_json::Value::Array(array) => {
            for child in array {
                omit_definitions(child);
            }
        }
        _ => {}
    }
}
#[test]
fn legacy_definition_oldload_and_selected_cancel_json_transcripts_continue_canonically() {
    let mut original = identity_world(identity_rules());
    let id = identity_wand(&mut original, false, "Owned rod");
    let mut wire = save(&original);
    omit_definitions(&mut wire);
    let mut direct = SimulationWorld::from_save_json(
        &wire.to_string(),
        original.ruleset.clone(),
        &original.ruleset_ref,
    )
    .unwrap();
    assert_eq!(save(&direct), wire);
    assert_eq!(direct.rng, original.rng);
    assert!(direct.arena.items[id].definition.is_none());
    let mut restored = load(&direct);
    for action in [
        selected(&direct, id, true),
        ActionAst::CancelZapSelection,
        selected(&direct, id, false),
        ActionAst::CancelZapSelection,
        ActionAst::Wait,
    ] {
        let transcript = serde_json::to_value(&action).unwrap();
        let replay: ActionAst = serde_json::from_value(transcript).unwrap();
        assert_eq!(
            direct.step_player_action(action),
            restored.step_player_action(replay)
        );
        assert_eq!(save(&direct), save(&restored));
        assert_eq!(direct.rng, restored.rng);
        restored = load(&restored);
    }
    assert!(direct.arena.items[id].definition.is_none());
    assert_eq!(direct.arena.items[id].enchantment, 0);
}

#[test]
fn old_action_json_and_new_optional_token_omission_are_canonical() {
    let old = serde_json::json!({"ZapWand":{"dir":"East","energy":6}});
    let action: ActionAst = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(
        serde_json::to_vec(&serde_json::to_value(action).unwrap()).unwrap(),
        serde_json::to_vec(&old).unwrap()
    );
    let new =
        serde_json::json!({"ZapSelectedWand":{"item_index":4294967295u32,"dir":"East","energy":6}});
    let action: ActionAst = serde_json::from_value(new.clone()).unwrap();
    assert_eq!(serde_json::to_value(action).unwrap(), new);
    assert_eq!(
        serde_json::to_value(ActionAst::CancelZapSelection).unwrap(),
        serde_json::json!("CancelZapSelection")
    );
}

#[test]
fn existing_charge_behavior_matches_legacy_for_empty_negative_zero_cursed_blessed() {
    for charge in [-1, 0, 2] {
        for buc in [Buc::Uncursed, Buc::Cursed, Buc::Blessed] {
            let mut a = identity_world(identity_rules());
            let id = spawn(&mut a, I::WAN_FIRE, charge, buc);
            let mut b = load(&a);
            assert_eq!(
                a.step_player_action(selected(&b, id, true)),
                b.step_player_action(ActionAst::ZapWand {
                    dir: Direction::North,
                    energy: 999
                })
            );
            assert_eq!(save(&a), save(&b));
            assert_eq!(a.rng, b.rng);
        }
    }
    let mut empty = identity_world(identity_rules());
    no_time(
        &mut empty,
        ActionAst::ZapSelectedWand {
            item_index: 0,
            expected_item: None,
            dir: Direction::East,
            energy: 6,
        },
        "You don't have that object.",
    );
}

#[test]
fn helpless_and_dressing_still_replace_selection_actions_with_wait() {
    for dressing in [false, true] {
        let mut a = identity_world(identity_rules());
        spawn(&mut a, I::WAN_FIRE, 5, Buc::Uncursed);
        if dressing {
            let armor = spawn(&mut a, I::PLATE_MAIL, 0, Buc::Uncursed);
            let index = a
                .arena
                .items_carried_by(a.player_id)
                .iter()
                .position(|&id| id == armor)
                .unwrap();
            a.step_player_action(ActionAst::Wear(index));
            assert!(a.equipment.dressing.is_some());
        } else {
            a.hero.afflictions.transient.helpless = 2;
        }
        for action in [
            ActionAst::CancelZapSelection,
            ActionAst::ZapSelectedWand {
                item_index: u32::MAX,
                expected_item: None,
                dir: Direction::East,
                energy: 6,
            },
        ] {
            let mut selected_world = load(&a);
            let mut wait_world = load(&a);
            assert_eq!(
                selected_world.step_player_action(action),
                wait_world.step_player_action(ActionAst::Wait)
            );
            assert_eq!(save(&selected_world), save(&wait_world));
        }
    }
}
