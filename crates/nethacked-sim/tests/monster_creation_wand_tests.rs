//! Independent released-C draw/coordinate fixtures through actual public Zap.
use nethacked_arena::{ActorId, ActorSex, ActorSpecies, ItemLocation};
use nethacked_core::engraving::{Engraving, EngravingMedium};
use nethacked_data::{CharacterConfig, ItemKindId as I, MonsterSpeciesId as M, RoleId, Ruleset};
use nethacked_sim::{ActionAst, SimulationWorld};
use nethacked_types::{BranchId, Buc, Coord, Direction, Tile, COLNO, ROWNO};
use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::{collections::HashSet, sync::Arc};

const HERO: Coord = Coord::new_unchecked(20, 10);

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
fn zap(w: &mut SimulationWorld) {
    w.step_player_action(ActionAst::ZapWand {
        dir: Direction::North,
        energy: 999,
    });
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
fn actual_zap_count_one_two_eight_renamed_owned_values_and_native_turn_order() {
    for quantity in [1, 2, 8] {
        let mut w = world(rules(), seed_for(quantity));
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
        zap(&mut w);
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
        assert_eq!(w.arena.items.values().next().unwrap().enchantment, 2);
    }
}

#[test]
fn direction_energy_ignored_and_nonzero_count_omits_optional_seven_draw() {
    let seed = seed_for(1);
    let mut a = world(rules(), seed);
    let mut b = load(&a);
    let mut gate = a.rng.clone();
    assert_ne!(gate.random_range(0..23u32), 0);
    zap(&mut a);
    b.step_player_action(ActionAst::ZapWand {
        dir: Direction::South,
        energy: 0,
    });
    assert_eq!(save(&a), save(&b));
    assert_eq!(a.rng.next_u64(), b.rng.next_u64());
}

#[test]
fn original_human_ward_exemption_precedes_fifty_rejections_then_unconditional_fifty_first() {
    let seed = seed_for(1);
    let mut w = world(rules(), seed);
    for y in 0..ROWNO {
        for x in 1..COLNO {
            w.level.engravings.insert(
                Coord::new_unchecked(x, y),
                Engraving::new("Elbereth", EngravingMedium::Carved(100)),
            );
        }
    }
    let mut oracle = w.rng.clone();
    assert_eq!(count(&mut oracle), 1);
    let expected = placement(&mut oracle, |c, _| c != HERO).unwrap(); // original '@' accepts ward immediately
    let expected_hp = initialize(&mut oracle, 51);
    allocations(&mut oracle, 1);
    zap(&mut w);
    let actual = actors(&w);
    assert_eq!(actual.len(), 1);
    assert_eq!((actual[0].1.coord, actual[0].1.hp), (expected, expected_hp));
    assert_eq!(w.rng, oracle);
}

#[test]
fn custom_original_wallwalk_terrain_override_precedes_selected_species_compatibility() {
    let mut rs = rules();
    let mut original = rs.monster_by_id(M::HEALER).unwrap().clone();
    original.id = None;
    original.name = "Custom original".into();
    original.glyph = 'x';
    original.flags = vec!["wallwalk".into()];
    rs.monsters.push(original);
    rs.roles
        .iter_mut()
        .find(|d| d.id == RoleId::Healer)
        .unwrap()
        .original_monster = Some(ActorSpecies::Custom("custom original".into()));
    rs.reindex();
    let mut w = world(rs, seed_for(1));
    w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Stone);
    w.level.set_tile(HERO, Tile::Room);
    let mut oracle = w.rng.clone();
    assert_eq!(count(&mut oracle), 1);
    let c = placement(&mut oracle, |c, _| c != HERO).unwrap();
    let expected_hp = initialize(&mut oracle, 51);
    allocations(&mut oracle, 1);
    zap(&mut w);
    assert_eq!(
        (actors(&w)[0].1.coord, actors(&w)[0].1.hp),
        (c, expected_hp)
    );
    assert_eq!(w.rng, oracle);
}

#[test]
fn blocked_all_map_attempts_and_genocided_disabled_or_exhausted_selection_have_exact_rng() {
    for mode in ["blocked", "genocide", "disabled", "frequency"] {
        let mut rs = rules();
        if mode == "disabled" {
            rs.monsters
                .iter_mut()
                .find(|d| d.name == "Owned candidate")
                .unwrap()
                .gen_flags = vec!["nogen".into()];
        }
        if mode == "frequency" {
            rs.monsters
                .iter_mut()
                .find(|d| d.name == "Owned candidate")
                .unwrap()
                .frequency = 0;
        }
        rs.reindex();
        let mut w = world(rs, seed_for(2));
        if mode == "blocked" {
            w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Stone);
            w.level.set_tile(HERO, Tile::Room);
        }
        if mode == "genocide" {
            w.genocide_registry
                .genocided_species
                .insert("owned candidate".into());
        }
        let mut oracle = w.rng.clone();
        assert_eq!(count(&mut oracle), 2);
        for _ in 0..2 {
            assert_eq!(
                placement(&mut oracle, |c, _| mode != "blocked" && c != HERO).is_some(),
                mode != "blocked"
            );
        }
        zap(&mut w);
        assert!(actors(&w).is_empty());
        assert_eq!(w.rng, oracle);
        assert_eq!(w.scheduler.turn, 1);
        assert_eq!(w.arena.items.values().next().unwrap().enchantment, 2);
    }
}

#[test]
fn whole_map_placement_and_owned_boulder_obstruction_follow_literal_shuffle_order() {
    for mode in ["wholemap", "boulder"] {
        let seed = seed_for(1);
        let mut w = world(rules(), seed);
        let mut initial = w.rng.clone();
        assert_eq!(count(&mut initial), 1);
        let first = placement(&mut initial, |c, _| c != HERO).unwrap();
        let remote = Coord::new_unchecked(30, 10);
        if mode == "wholemap" {
            w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Stone);
            w.level.set_tile(HERO, Tile::Room);
            w.level.set_tile(remote, Tile::Room);
        } else {
            let name = w.ruleset.item_by_id(I::BOULDER).unwrap().name.clone();
            let b = w
                .ruleset
                .create_item_record(&name, ItemLocation::Floor(first), Buc::Uncursed)
                .unwrap();
            w.arena.spawn_item(b);
        }
        let mut oracle = w.rng.clone();
        assert_eq!(count(&mut oracle), 1);
        let expected = placement(&mut oracle, |c, _| {
            if mode == "wholemap" {
                c == remote
            } else {
                c != HERO && c != first
            }
        })
        .unwrap();
        if mode == "wholemap" {
            assert_eq!(expected, remote);
        } else {
            assert_ne!(expected, first);
        }
        let expected_hp = initialize(&mut oracle, 1);
        allocations(&mut oracle, 1);
        zap(&mut w);
        assert_eq!(
            (actors(&w)[0].1.coord, actors(&w)[0].1.hp),
            (expected, expected_hp)
        );
        assert_eq!(w.rng, oracle);
    }
}

#[test]
fn custom_original_scary_phase_fallback_and_selected_species_ward_check_stay_distinct() {
    let mut rs = rules();
    let mut original = rs.monster_by_id(M::HEALER).unwrap().clone();
    original.id = None;
    original.name = "Ward-sensitive original".into();
    original.glyph = 'x';
    original.is_unique = false;
    original.flags.clear();
    rs.monsters.push(original);
    rs.roles
        .iter_mut()
        .find(|r| r.id == RoleId::Healer)
        .unwrap()
        .original_monster = Some(ActorSpecies::Custom("ward-sensitive original".into()));
    rs.reindex();
    let mut w = world(rs, seed_for(1));
    for y in 0..ROWNO {
        for x in 1..COLNO {
            w.level.set_engraving(
                Coord::new_unchecked(x, y),
                Engraving::new("Elbereth", EngravingMedium::Burned),
            );
        }
    }
    let mut oracle = w.rng.clone();
    assert_eq!(count(&mut oracle), 1);
    let expected = placement(&mut oracle, |c, scary| !scary && c != HERO).unwrap();
    let expected_hp = initialize(&mut oracle, 51);
    allocations(&mut oracle, 1);
    zap(&mut w);
    assert_eq!(
        (actors(&w)[0].1.coord, actors(&w)[0].1.hp),
        (expected, expected_hp)
    );
    assert_eq!(w.rng, oracle);
}

#[test]
fn variable_sex_and_coaligned_owned_peace_draws_follow_hp_before_turn_allocation() {
    let mut rs = rules();
    let d = rs
        .monsters
        .iter_mut()
        .find(|d| d.name == "Owned candidate")
        .unwrap();
    d.flags.clear();
    d.always_hostile = false;
    rs.reindex();
    let mut w = world(rs, seed_for(1));
    w.alignment_record = 10;
    let mut oracle = w.rng.clone();
    assert_eq!(count(&mut oracle), 1);
    let c = placement(&mut oracle, |c, _| c != HERO).unwrap();
    let expected_hp = initialize(&mut oracle, 1);
    let expected_sex = if oracle.random_range(0..2u32) == 0 {
        ActorSex::Male
    } else {
        ActorSex::Female
    };
    let expected_peace = oracle.random_range(0..26u32) != 0 && oracle.random_range(0..2u32) != 0;
    allocations(&mut oracle, 1);
    zap(&mut w);
    let a = &actors(&w)[0].1;
    assert_eq!(
        (a.coord, a.hp, a.sex, a.is_peaceful),
        (c, expected_hp, Some(expected_sex), expected_peace)
    );
    assert_eq!(w.rng, oracle);
}

#[test]
fn new_and_legacy_role_identity_full_load_uses_canonical_identifier_without_load_rng() {
    let mut rs = rules();
    rs.roles
        .iter_mut()
        .find(|d| d.id == RoleId::Healer)
        .unwrap()
        .original_monster = None;
    rs.reindex();
    let original = world(rs, seed_for(1));
    assert_eq!(original.role_id, Some(RoleId::Healer));
    for legacy in [false, true] {
        let mut wire = save(&original);
        if legacy {
            wire.as_object_mut().unwrap().remove("role_id");
        }
        let mut loaded = SimulationWorld::from_save_json(
            &wire.to_string(),
            original.ruleset.clone(),
            &original.ruleset_ref,
        )
        .unwrap();
        assert_eq!(
            loaded.role_id,
            if legacy { None } else { Some(RoleId::Healer) }
        );
        assert_eq!(loaded.rng, original.rng);
        assert_eq!(save(&loaded), wire);
        let mut direct = load(&original);
        zap(&mut direct);
        zap(&mut loaded);
        assert_eq!(actors(&direct), actors(&loaded));
        assert_eq!(direct.rng, loaded.rng);
        let mut full = save(&loaded);
        if legacy {
            full["role_id"] = serde_json::json!("Healer");
        }
        assert_eq!(save(&direct), full);
    }
}

#[test]
fn unknown_legacy_or_missing_owned_identity_consumes_charge_time_without_creation_draws() {
    for mode in [
        "unknown",
        "missing-role",
        "missing-species",
        "explicit-missing",
    ] {
        let mut w = world(rules(), seed_for(1));
        if mode == "unknown" {
            w.role_id = None;
            w.role_name = "Owned profession".into();
        } else {
            let mut rs = (*w.ruleset).clone();
            match mode {
                "missing-role" => rs.roles.retain(|r| r.id != RoleId::Healer),
                "missing-species" => rs.monsters.retain(|d| d.id != Some(M::HEALER)),
                _ => {
                    rs.roles
                        .iter_mut()
                        .find(|r| r.id == RoleId::Healer)
                        .unwrap()
                        .original_monster = Some(ActorSpecies::Custom("missing".into()))
                }
            }
            rs.reindex();
            w.ruleset = Arc::new(rs);
        }
        let mut loaded = load(&w);
        assert_eq!(save(&loaded), save(&w));
        assert_eq!(loaded.rng, w.rng);
        let before_rng = loaded.rng.clone();
        let events = loaded.step_player_action(ActionAst::ZapWand {
            dir: Direction::North,
            energy: 0,
        });
        assert!(events.iter().any(|event|matches!(event,nethacked_sim::GameEvent::LogMessage{text} if text=="Your original form is unknown.")));
        assert!(actors(&loaded).is_empty());
        assert_eq!(loaded.rng, before_rng);
        assert_eq!(loaded.scheduler.turn, 1);
        assert_eq!(loaded.arena.items.values().next().unwrap().enchantment, 2);
    }
}

#[test]
fn noncanonical_removals_preserve_new_actor_ids_full_canonical_save_and_native_continuation() {
    let mut direct = world(rules(), seed_for(2));
    let mut dummy = direct.arena.actors[direct.player_id].clone();
    dummy.is_player = false;
    dummy.speed = 0;
    dummy.movement = 0;
    dummy.name = "Existing fixture".into();
    dummy.coord = Coord::new_unchecked(60, 10);
    let two = direct.arena.spawn_actor(dummy.clone());
    let three = direct.arena.spawn_actor(dummy.clone());
    let four = direct.arena.spawn_actor(dummy);
    direct.arena.actors.remove(four);
    direct.arena.actors.remove(two);
    let live = direct.arena.actors[three].clone();
    let mut restored = load(&direct);
    assert_eq!(save(&direct), save(&restored));
    assert_eq!(direct.rng, restored.rng);
    let mut oracle = direct.rng.clone();
    assert_eq!(count(&mut oracle), 2);
    let mut occupied = HashSet::from([HERO, live.coord]);
    let mut expected = Vec::new();
    for _ in 0..2 {
        let c = placement(&mut oracle, |c, _| !occupied.contains(&c)).unwrap();
        occupied.insert(c);
        expected.push((c, initialize(&mut oracle, 1)));
    }
    allocations(&mut oracle, 3);
    zap(&mut direct);
    zap(&mut restored);
    assert_eq!(direct.arena.actors[three], live);
    assert_eq!(restored.arena.actors[three], live);
    let created: Vec<_> = actors(&direct)
        .into_iter()
        .filter(|(id, _)| *id != three)
        .collect();
    assert_eq!(created.len(), 2);
    assert_eq!(
        created
            .iter()
            .map(|(_, a)| a.coord)
            .collect::<HashSet<_>>()
            .len(),
        2
    );
    assert_eq!(
        expected
            .iter()
            .map(|(c, _)| *c)
            .collect::<HashSet<_>>()
            .len(),
        2
    );
    for (_, a) in &created {
        let &(c, hp) = expected.iter().find(|(c, _)| *c == a.coord).unwrap();
        assert_eq!((a.coord, a.hp), (c, hp));
    }
    assert_eq!(actors(&direct), actors(&restored));
    assert_eq!(save(&direct), save(&restored));
    assert_eq!(direct.rng, oracle);
    assert_eq!(direct.rng.next_u64(), restored.rng.next_u64());
    // Save after action and continue again: complete arena/charge/clock/native equality.
    let mut next = load(&direct);
    zap(&mut direct);
    zap(&mut next);
    assert_eq!(actors(&direct), actors(&next));
    assert_eq!(save(&direct), save(&next));
    assert_eq!(direct.rng.next_u64(), next.rng.next_u64());
}

#[test]
fn canonical_renamed_selected_species_owned_peace_override_precedes_hostility_without_draw() {
    let mut rs = rules();
    let d = rs
        .monsters
        .iter_mut()
        .find(|d| d.name == "Owned candidate")
        .unwrap();
    d.id = Some(M::GIANT_ANT);
    d.peaceful_by_default = true; // M2_PEACEFUL precedes M2_HOSTILE
    rs.reindex();
    let mut w = world(rs, seed_for(1));
    let mut oracle = w.rng.clone();
    assert_eq!(count(&mut oracle), 1);
    let c = placement(&mut oracle, |c, _| c != HERO).unwrap();
    let expected_hp = initialize(&mut oracle, 1);
    allocations(&mut oracle, 1);
    zap(&mut w);
    let a = &actors(&w)[0].1;
    assert_eq!((a.coord, a.hp, a.is_peaceful), (c, expected_hp, true));
    assert_eq!(a.species, Some(ActorSpecies::Vanilla("GIANT_ANT".into())));
    assert_eq!(a.name, "Owned candidate");
    assert_eq!(w.rng, oracle);
}

#[test]
fn real_owned_scare_scroll_is_exempt_for_original_human_but_not_selected_species() {
    let mut w = world(rules(), seed_for(1));
    let destination = Coord::new_unchecked(19, 9);
    w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Stone);
    w.level.set_tile(HERO, Tile::Room);
    w.level.set_tile(destination, Tile::Room);
    let name = w
        .ruleset
        .item_by_id(I::SCR_SCARE_MONSTER)
        .unwrap()
        .name
        .clone();
    let scroll = w
        .ruleset
        .create_item_record(&name, ItemLocation::Floor(destination), Buc::Uncursed)
        .unwrap();
    w.arena.spawn_item(scroll);
    let mut oracle = w.rng.clone();
    assert_eq!(count(&mut oracle), 1);
    assert_eq!(
        placement(&mut oracle, |c, _| c == destination),
        Some(destination)
    );
    let expected_hp = initialize(&mut oracle, 51);
    allocations(&mut oracle, 1);
    zap(&mut w);
    assert_eq!(
        (actors(&w)[0].1.coord, actors(&w)[0].1.hp),
        (destination, expected_hp)
    );
    assert_eq!(w.rng, oracle);
}

#[test]
fn unknown_original_identity_still_allocates_existing_speed_zero_actor_normally() {
    let mut w = world(rules(), seed_for(1));
    w.role_id = None;
    w.role_name = "Unknown".into();
    let mut actor = w.arena.actors[w.player_id].clone();
    actor.is_player = false;
    actor.coord = Coord::new_unchecked(60, 10);
    actor.speed = 0;
    actor.movement = 0;
    let id = w.arena.spawn_actor(actor.clone());
    let mut oracle = w.rng.clone();
    allocations(&mut oracle, 1);
    zap(&mut w);
    assert_eq!(w.arena.actors[id], actor);
    assert_eq!(actors(&w).len(), 1);
    assert_eq!(w.rng, oracle);
    assert_eq!(w.scheduler.turn, 1);
    assert_eq!(w.arena.items.values().next().unwrap().enchantment, 2);
}
