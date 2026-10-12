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
    d.gen_flags = vec!["sgroup".into()];
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
fn shuffled(rng: &mut ChaCha8Rng, origin: Coord, max: i32) -> Vec<Coord> {
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
                let (x, y) = (origin.x as i32 + dx, origin.y as i32 + dy);
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
fn placement(
    rng: &mut ChaCha8Rng,
    origin: Coord,
    legal: impl Fn(Coord, bool) -> bool,
) -> Option<Coord> {
    for scary in [true, false] {
        let near = shuffled(rng, origin, 3);
        if let Some(c) = near.iter().find(|&&c| legal(c, scary)) {
            return Some(*c);
        }
        let all = shuffled(rng, origin, 80);
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
fn hp(rng: &mut ChaCha8Rng) -> u32 {
    let sum: u32 = (0..3).map(|_| rng.random_range(1..=8u32)).sum();
    sum + u32::from(sum == 3)
}

#[derive(Clone, Copy, PartialEq)]
enum Attitude {
    Hostile,
    Peaceful,
    Coaligned,
}
#[derive(Clone, Copy, PartialEq)]
enum Map {
    Room,
    Exhausted,
    Remote,
    Ward,
}
#[derive(Clone, Copy)]
struct Policy {
    hero: u32,
    enabled: bool,
    attitude: Attitude,
    variable_sex: bool,
    fixed: bool,
    eligible: bool,
    map: Map,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            hero: 2,
            enabled: true,
            attitude: Attitude::Hostile,
            variable_sex: false,
            fixed: false,
            eligible: true,
            map: Map::Room,
        }
    }
}
#[derive(Debug)]
struct Expected {
    coord: Coord,
    hp: u32,
    max_hp: u32,
    level: u32,
    sex: ActorSex,
    peace: bool,
    malign: i32,
}
struct Trace {
    rng: ChaCha8Rng,
    actors: Vec<Expected>,
    gates: Vec<(u32, Option<u32>)>,
    checks: Vec<(bool, Option<bool>)>,
    origins: Vec<(Coord, Option<Coord>)>,
    leaders: u32,
}
fn peace(rng: &mut ChaCha8Rng, attitude: Attitude) -> bool {
    match attitude {
        Attitude::Hostile => false,
        Attitude::Peaceful => true,
        Attitude::Coaligned => rng.random_range(0..26u32) != 0 && rng.random_range(0..2u32) != 0,
    }
}
fn normal(rng: &mut ChaCha8Rng, c: Coord, p: Policy) -> Expected {
    let _ = rng.random_range(0..2u32); // identity precedes HP, including fixed packs
    let (hp, max_hp, level) = if p.fixed {
        (17, 19, 2)
    } else {
        let hp = hp(rng);
        (hp, hp, 3)
    };
    let sex = if p.variable_sex {
        if rng.random_range(0..2u32) == 0 {
            ActorSex::Male
        } else {
            ActorSex::Female
        }
    } else {
        ActorSex::Female
    };
    let peaceful = peace(rng, p.attitude);
    let malign = match p.attitude {
        Attitude::Hostile => 0,
        Attitude::Peaceful => -15,
        Attitude::Coaligned => {
            if peaceful {
                -9
            } else {
                3
            }
        }
    };
    Expected {
        coord: c,
        hp,
        max_hp,
        level,
        sex,
        peace: peaceful,
        malign,
    }
}
fn oracle(seed: u64, p: Policy, existing: &[Coord]) -> Trace {
    oracle_with_blocks(seed, p, existing, &[])
}
fn oracle_with_blocks(seed: u64, p: Policy, existing: &[Coord], blocks: &[Coord]) -> Trace {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let leaders = count(&mut rng);
    let mut trace = Trace {
        rng: rng.clone(),
        actors: vec![],
        gates: vec![],
        checks: vec![],
        origins: vec![],
        leaders,
    };
    let mut occupied: HashSet<_> = existing
        .iter()
        .copied()
        .chain(blocks.iter().copied())
        .chain([HERO])
        .collect();
    for _ in 0..leaders {
        let destination = placement(&mut rng, HERO, |c, _| {
            !occupied.contains(&c)
                && (p.map != Map::Remote
                    || c == Coord::new_unchecked(19, 9)
                    || c == Coord::new_unchecked(30, 10))
        });
        let Some(c) = destination else {
            continue;
        };
        if !p.eligible {
            continue;
        }
        for _ in 0..if matches!(p.map, Map::Exhausted | Map::Ward) {
            51
        } else {
            1
        } {
            let _ = rng.random_range(0..1u32);
        }
        let leader = normal(&mut rng, c, p);
        occupied.insert(c);
        trace.actors.push(leader);
        if !p.enabled {
            continue;
        }
        let gate = rng.random_range(0..2u32);
        if gate == 0 {
            trace.gates.push((gate, None));
            continue;
        }
        let raw = rng.random_range(0..3u32) + 1;
        trace.gates.push((gate, Some(raw)));
        // Source literals: h1/2 divisor4; h3/4 divisor2; h5 divisor1.
        let attempts = (raw
            / if p.hero < 3 {
                4
            } else if p.hero < 5 {
                2
            } else {
                1
            })
        .max(1);
        let mut origin = c;
        for _ in 0..attempts {
            let first = peace(&mut rng, p.attitude);
            if first {
                trace.checks.push((true, None));
                continue;
            }
            let next = placement(&mut rng, origin, |c, scary| {
                !occupied.contains(&c)
                    && match p.map {
                        Map::Room => true,
                        Map::Ward => !scary,
                        Map::Exhausted => false,
                        Map::Remote => {
                            c == Coord::new_unchecked(19, 9) || c == Coord::new_unchecked(30, 10)
                        }
                    }
            });
            trace.origins.push((origin, next));
            let Some(c) = next else {
                trace.checks.push((false, None));
                continue;
            };
            origin = c;
            let mut follower = normal(&mut rng, c, p);
            trace.checks.push((false, Some(follower.peace)));
            follower.peace = false;
            follower.malign = if p.attitude == Attitude::Hostile {
                0
            } else {
                3
            };
            occupied.insert(c);
            trace.actors.push(follower);
        }
    }
    for _ in 0..trace.actors.len() + existing.len() {
        let _ = rng.random_range(0..12u32);
    } // P09, unconditional speed0 draw
    trace.rng = rng;
    trace
}
fn find(p: Policy, predicate: impl Fn(&Trace) -> bool) -> u64 {
    (0..100000)
        .find(|&seed| predicate(&oracle(seed, p, &[])))
        .unwrap()
}
fn apply_policy(rs: &mut Ruleset, p: Policy) {
    let d = rs
        .monsters
        .iter_mut()
        .find(|d| d.name == "Owned candidate")
        .unwrap();
    if !p.enabled {
        d.gen_flags.clear();
    }
    d.always_hostile = p.attitude == Attitude::Hostile;
    d.peaceful_by_default = p.attitude == Attitude::Peaceful;
    if p.variable_sex {
        d.flags.clear();
    }
    if p.fixed {
        d.sample_hp = false;
        d.base_hp = 17;
        d.max_hp = 19;
    }
    if !p.eligible {
        d.frequency = 0;
    }
    rs.reindex();
}
fn configured(p: Policy, seed: u64) -> SimulationWorld {
    let mut rs = rules();
    apply_policy(&mut rs, p);
    if p.map == Map::Exhausted {
        let mut original = rs.monster_by_id(M::HEALER).unwrap().clone();
        original.id = None;
        original.name = "Wall original".into();
        original.flags = vec!["wallwalk".into()];
        rs.monsters.push(original);
        rs.roles
            .iter_mut()
            .find(|r| r.id == RoleId::Healer)
            .unwrap()
            .original_monster = Some(ActorSpecies::Custom("wall original".into()));
        rs.reindex();
    }
    let mut w = world(rs, seed);
    w.arena.actors[w.player_id].level = p.hero;
    w.alignment_record = 10;
    if matches!(p.map, Map::Exhausted | Map::Remote) {
        w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Stone);
        w.level.set_tile(HERO, Tile::Room);
    }
    if p.map == Map::Remote {
        w.level.set_tile(Coord::new_unchecked(19, 9), Tile::Room);
        w.level.set_tile(Coord::new_unchecked(30, 10), Tile::Room);
    }
    if p.map == Map::Ward {
        for y in 0..ROWNO {
            for x in 1..COLNO {
                w.level.set_engraving(
                    Coord::new_unchecked(x, y),
                    Engraving::new("Elbereth", EngravingMedium::Burned),
                );
            }
        }
    }
    w
}
fn verify(w: &SimulationWorld, t: &Trace, existing: &[ActorId]) {
    let actual: Vec<_> = actors(w)
        .into_iter()
        .filter(|(id, _)| !existing.contains(id))
        .collect();
    assert_eq!(actual.len(), t.actors.len());
    assert_eq!(
        actual
            .iter()
            .map(|(_, a)| a.coord)
            .collect::<HashSet<_>>()
            .len(),
        actual.len()
    );
    assert_eq!(
        t.actors
            .iter()
            .map(|a| a.coord)
            .collect::<HashSet<_>>()
            .len(),
        t.actors.len()
    );
    for (_, a) in actual {
        let expected = t.actors.iter().find(|x| x.coord == a.coord).unwrap();
        assert_eq!(
            (a.hp, a.max_hp, a.level, a.sex, a.is_peaceful, a.malign),
            (
                expected.hp,
                expected.max_hp,
                expected.level,
                Some(expected.sex),
                expected.peace,
                expected.malign
            )
        );
        assert_eq!(a.movement, 0);
        assert_eq!(a.name, "Owned candidate");
    }
    assert_eq!(w.rng, t.rng);
    assert_eq!(w.scheduler.turn, 1);
    assert_eq!(w.scheduler.hero_energy, 12);
    assert_eq!(
        w.arena
            .items
            .values()
            .find(|i| i.name == "Owned rod")
            .unwrap()
            .enchantment,
        2
    );
}

#[test]
fn actual_zap_literal_gate_and_all_raw_counts_at_hero_one_through_five() {
    const COUNTS: [[usize; 3]; 5] = [[1, 1, 1], [1, 1, 1], [1, 1, 1], [1, 1, 1], [1, 2, 3]];
    for hero in 1..=5 {
        for raw in 1..=3 {
            let p = Policy {
                hero,
                ..Default::default()
            };
            let seed = find(p, |t| t.leaders == 1 && t.gates == [(1, Some(raw))]);
            let trace = oracle(seed, p, &[]);
            assert_eq!(
                trace.actors.len(),
                1 + COUNTS[(hero - 1) as usize][(raw - 1) as usize]
            );
            let mut w = configured(p, seed);
            zap(&mut w);
            verify(&w, &trace, &[]);
            for (_, a) in actors(&w) {
                assert_eq!(
                    a.species,
                    Some(ActorSpecies::Custom("owned candidate".into()))
                );
            }
        }
    }
    let p = Policy::default();
    let seed = find(p, |t| t.leaders == 1 && t.gates == [(0, None)]);
    let trace = oracle(seed, p, &[]);
    assert_eq!(trace.actors.len(), 1);
    let mut w = configured(p, seed);
    zap(&mut w);
    verify(&w, &trace, &[]); // no count/peace/placement/child draws
}

#[test]
fn disabled_group_and_renamed_canonical_selected_species_have_source_native_identity() {
    for enabled in [false, true] {
        let p = Policy {
            enabled,
            ..Default::default()
        };
        let seed = if enabled {
            find(p, |t| t.leaders == 1 && t.gates == [(1, Some(3))])
        } else {
            find(p, |t| t.leaders == 1)
        };
        let mut w = configured(p, seed);
        let mut rs = (*w.ruleset).clone();
        rs.monsters
            .iter_mut()
            .find(|d| d.name == "Owned candidate")
            .unwrap()
            .id = Some(M::GIANT_ANT);
        rs.reindex();
        w.ruleset = Arc::new(rs);
        let trace = oracle(seed, p, &[]);
        zap(&mut w);
        verify(&w, &trace, &[]);
        for (_, a) in actors(&w) {
            assert_eq!(a.species, Some(ActorSpecies::Vanilla("GIANT_ANT".into())));
        }
        if !enabled {
            assert!(trace.gates.is_empty());
            assert_eq!(trace.actors.len(), 1);
        }
    }
}

#[test]
fn group_checks_peace_even_for_peaceful_leader_consumes_second_check_and_forces_malign() {
    let p = Policy {
        attitude: Attitude::Coaligned,
        variable_sex: true,
        ..Default::default()
    };
    let seed = find(p, |t| {
        t.leaders == 1
            && t.actors.first().is_some_and(|a| a.peace)
            && t.checks == [(false, Some(true))]
    });
    let trace = oracle(seed, p, &[]);
    assert_eq!(trace.actors.len(), 2);
    assert_eq!(trace.actors[0].malign, -9);
    assert_eq!(trace.actors[1].malign, 3);
    assert!(!trace.actors[1].peace);
    let mut w = configured(p, seed);
    zap(&mut w);
    verify(&w, &trace, &[]);
    let p = Policy {
        attitude: Attitude::Peaceful,
        hero: 5,
        ..Default::default()
    };
    let seed = find(p, |t| t.leaders == 1 && t.gates == [(1, Some(3))]);
    let trace = oracle(seed, p, &[]);
    assert_eq!(trace.checks, [(true, None), (true, None), (true, None)]);
    assert_eq!(trace.actors.len(), 1);
    assert!(trace.origins.is_empty());
    let mut w = configured(p, seed);
    zap(&mut w);
    verify(&w, &trace, &[]);
}

#[test]
fn selected_species_exhaustion_wholemap_and_remaining_attempts_retain_correct_origin() {
    for map in [Map::Exhausted, Map::Remote] {
        let p = Policy {
            hero: 5,
            map,
            ..Default::default()
        };
        let seed = find(p, |t| t.leaders == 1 && t.gates == [(1, Some(3))]);
        let trace = oracle(seed, p, &[]);
        assert_eq!(trace.origins.len(), 3);
        if map == Map::Exhausted {
            assert_eq!(trace.actors.len(), 1);
            assert!(trace.origins.iter().all(|x| x.1.is_none()));
            assert!(trace.origins.iter().all(|x| x.0 == trace.actors[0].coord));
        } else {
            assert_eq!(trace.actors.len(), 2);
            assert_eq!(trace.origins[0].1, Some(Coord::new_unchecked(30, 10)));
            assert_eq!(
                trace.origins[1..],
                [
                    (Coord::new_unchecked(30, 10), None),
                    (Coord::new_unchecked(30, 10), None)
                ]
            );
        }
        let mut w = configured(p, seed);
        zap(&mut w);
        verify(&w, &trace, &[]);
    }
}

#[test]
fn dungeon_difficulty_selects_species_separately_and_genocide_rejects_before_groups() {
    let p = Policy {
        hero: 5,
        fixed: true,
        ..Default::default()
    };
    let seed = find(p, |t| t.leaders == 1 && t.gates == [(1, Some(3))]);
    for depth in [2, 18] {
        let mut w = configured(p, seed);
        w.depth = depth;
        let mut rs = (*w.ruleset).clone();
        rs.monsters
            .iter_mut()
            .find(|d| d.name == "Owned candidate")
            .unwrap()
            .difficulty = 2;
        rs.reindex();
        w.ruleset = Arc::new(rs);
        let eligible = depth == 2;
        let trace = oracle(seed, Policy { eligible, ..p }, &[]);
        zap(&mut w);
        verify(&w, &trace, &[]);
        assert_eq!(actors(&w).len(), if eligible { 4 } else { 0 });
    }
    let mut w = configured(p, seed);
    w.genocide_registry.genocided_classes.insert('c');
    let trace = oracle(
        seed,
        Policy {
            eligible: false,
            ..p
        },
        &[],
    );
    zap(&mut w);
    verify(&w, &trace, &[]);
    assert!(actors(&w).is_empty());
}

#[test]
fn multiple_wand_leaders_finish_groups_before_next_hero_origin_reset() {
    let p = Policy {
        hero: 5,
        ..Default::default()
    };
    let seed = find(p, |t| t.leaders == 2 && t.gates.iter().all(|g| g.0 == 1));
    let trace = oracle(seed, p, &[]);
    assert_eq!(trace.gates.len(), 2);
    assert!(trace.actors.len() > 2);
    let mut w = configured(p, seed);
    zap(&mut w);
    verify(&w, &trace, &[]);
}

#[test]
fn noncanonical_live_keys_group_actor_ids_full_canonical_save_and_native_continue() {
    let p = Policy {
        hero: 5,
        ..Default::default()
    };
    let seed = find(p, |t| t.leaders == 1 && t.gates == [(1, Some(3))]);
    let mut direct = configured(p, seed);
    let mut dummy = direct.arena.actors[direct.player_id].clone();
    dummy.is_player = false;
    dummy.speed = 0;
    dummy.movement = 0;
    dummy.coord = Coord::new_unchecked(60, 10);
    let two = direct.arena.spawn_actor(dummy.clone());
    let three = direct.arena.spawn_actor(dummy.clone());
    let four = direct.arena.spawn_actor(dummy.clone());
    direct.arena.actors.remove(four);
    direct.arena.actors.remove(two);
    let mut restored = load(&direct);
    assert_eq!(save(&direct), save(&restored));
    assert_eq!(direct.rng, restored.rng);
    let trace = oracle(seed, p, &[dummy.coord]);
    zap(&mut direct);
    zap(&mut restored);
    verify(&direct, &trace, &[three]);
    assert_eq!(direct.arena.actors[three], dummy);
    assert_eq!(restored.arena.actors[three], dummy);
    assert_eq!(actors(&direct), actors(&restored));
    assert_eq!(save(&direct), save(&restored));
    assert_eq!(direct.rng.next_u64(), restored.rng.next_u64());
    let mut next = load(&direct);
    zap(&mut direct);
    zap(&mut next);
    assert_eq!(actors(&direct), actors(&next));
    assert_eq!(save(&direct), save(&next));
    assert_eq!(direct.rng.next_u64(), next.rng.next_u64());
}

#[test]
fn group_selected_species_real_ward_fallback_is_independent_of_original_human() {
    let p = Policy {
        hero: 5,
        map: Map::Ward,
        ..Default::default()
    };
    let seed = find(p, |t| t.leaders == 1 && t.gates == [(1, Some(3))]);
    let trace = oracle(seed, p, &[]);
    assert_eq!(trace.actors.len(), 4);
    assert_eq!(trace.origins.len(), 3);
    let mut w = configured(p, seed);
    zap(&mut w);
    verify(&w, &trace, &[]);
    // Original human accepts wards before selector's51st acceptance. Each
    // selected nonhuman follower consumes full scary failure then non-scary
    // placement, and explicit child initialization has no51-selection loop.
}

#[test]
fn owned_renamed_boulder_rejects_first_follower_candidate_without_actor_allocation() {
    let p = Policy {
        hero: 5,
        ..Default::default()
    };
    let seed = find(p, |t| t.leaders == 1 && t.gates == [(1, Some(3))]);
    let initial = oracle(seed, p, &[]);
    let obstacle = initial.actors[1].coord;
    let trace = oracle_with_blocks(seed, p, &[], &[obstacle]);
    assert_eq!(trace.actors.len(), 4);
    assert!(trace.actors.iter().all(|a| a.coord != obstacle));
    let mut w = configured(p, seed);
    let mut rs = (*w.ruleset).clone();
    rs.items
        .iter_mut()
        .find(|d| d.id == Some(I::BOULDER))
        .unwrap()
        .name = "Owned barrier".into();
    rs.reindex();
    w.ruleset = Arc::new(rs);
    let item = w
        .ruleset
        .create_item_record(
            "Owned barrier",
            ItemLocation::Floor(obstacle),
            Buc::Uncursed,
        )
        .unwrap();
    w.arena.spawn_item(item);
    zap(&mut w);
    verify(&w, &trace, &[]);
}
