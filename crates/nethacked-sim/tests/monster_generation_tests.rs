//! P10a independent literals/native draw fixtures, released C 16ff5911.
//! Oracles never call the production HP/level/spawn/placement helpers.
use nethacked_arena::{ActorSex, ActorSpecies, ItemLocation};
use nethacked_core::engraving::{Engraving, EngravingMedium};
use nethacked_data::{CharacterConfig, MonsterSpeciesId as M, Ruleset};
use nethacked_sim::{ActionAst, SimulationWorld};
use nethacked_types::{BranchId, Buc, Coord, Tile};
use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::sync::Arc;

struct ZeroRng;
impl RngCore for ZeroRng {
    fn next_u32(&mut self) -> u32 {
        0
    }
    fn next_u64(&mut self) -> u64 {
        0
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        dest.fill(0);
    }
}

const DEST: Coord = Coord::new_unchecked(20, 10);
const OTHER: Coord = Coord::new_unchecked(21, 10);

fn rules(custom: bool, sampled: bool) -> Ruleset {
    let mut rules = (*Ruleset::vanilla()).clone();
    for def in &mut rules.monsters {
        def.frequency = 0;
    }
    let def = rules
        .monsters
        .iter_mut()
        .find(|d| d.id == Some(M::GIANT_ANT))
        .unwrap();
    def.name = "Owned candidate".into();
    def.id = if custom { None } else { Some(M::GIANT_ANT) };
    def.frequency = 1;
    def.difficulty = 1;
    def.level = 2;
    def.base_hp = 17;
    def.max_hp = 19;
    def.sample_hp = sampled;
    def.speed = 0;
    def.glyph = 'c';
    def.flags = vec!["female".into()];
    def.gen_flags = vec!["sgroup".into()];
    def.always_hostile = true;
    def.peaceful_by_default = false;
    def.m2_race = None;
    def.maligntyp = 0;
    def.is_unique = false;
    def.abilities.clear();
    rules.reindex();
    rules
}

fn world(rules: Ruleset) -> SimulationWorld {
    let mut w = SimulationWorld::new_with_character_and_ruleset(
        41,
        CharacterConfig::default(),
        Arc::new(rules),
        nethacked_data::ruleset::RulesetRef::vanilla(),
    );
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.arena.items.clear();
    w.equipment = Default::default();
    w.wielded_item = None;
    w.unpaid_items.clear();
    w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Room);
    w.arena.actors[w.player_id].coord = Coord::new_unchecked(5, 5);
    w.arena.actors[w.player_id].level = 2;
    w.arena.actors[w.player_id].hp = 1000;
    w.arena.actors[w.player_id].max_hp = 1000;
    w.current_branch = BranchId::DungeonsOfDoom;
    w.depth = 7;
    w.rng = ChaCha8Rng::seed_from_u64(41);
    w
}

fn load(w: &SimulationWorld) -> SimulationWorld {
    SimulationWorld::from_save_json(
        &w.to_save_json().unwrap(),
        w.ruleset.clone(),
        &w.ruleset_ref,
    )
    .unwrap()
}

fn canonical_save(w: &SimulationWorld) -> serde_json::Value {
    serde_json::from_str(&w.to_save_json().unwrap()).unwrap()
}

fn native_hp(rng: &mut ChaCha8Rng, level: u32) -> u32 {
    let hp = (0..level).map(|_| rng.random_range(1..=8u32)).sum();
    if hp == level {
        hp + 1
    } else {
        hp
    }
}

#[test]
fn public_exact_location_sampled_custom_and_renamed_native_order_no_groups() {
    for custom in [false, true] {
        let mut w = world(rules(custom, true));
        let mut expected = w.rng.clone();
        let _ = expected.random_range(0..1u32); // single owned reservoir candidate
        let _ = expected.random_range(0..2u32); // next_ident BEFORE HP
        let hp = native_hp(&mut expected, 3); // base2, depth7, hero2 -> adjusted3
        let id = w.spawn_random_monster_at(DEST).unwrap();
        let actor = &w.arena.actors[id];
        assert_eq!(
            (actor.coord, actor.level, actor.hp, actor.max_hp),
            (DEST, 3, hp, hp)
        );
        assert_eq!(actor.name, "Owned candidate");
        assert_eq!(actor.sex, Some(ActorSex::Female));
        assert!(!actor.is_peaceful);
        assert_eq!(actor.movement, 0);
        assert_eq!(
            actor.species,
            Some(if custom {
                ActorSpecies::Custom("owned candidate".into())
            } else {
                ActorSpecies::Vanilla("GIANT_ANT".into())
            })
        );
        assert_eq!(w.arena.actors.len(), 2); // source MM_NOGRP despite owned SGROUP
        assert_eq!(w.rng, expected);
    }
}

#[test]
fn public_fixed_policy_keeps_level_hp_without_discarded_hp_or_allocation_draws() {
    let mut w = world(rules(true, false));
    let mut expected = w.rng.clone();
    let _ = expected.random_range(0..1u32);
    let _ = expected.random_range(0..2u32);
    let id = w.spawn_random_monster_at(DEST).unwrap();
    let a = &w.arena.actors[id];
    assert_eq!((a.level, a.hp, a.max_hp, a.movement), (2, 17, 19, 0));
    assert_eq!(a.sex, Some(ActorSex::Female));
    assert_eq!(w.rng, expected);
}

#[test]
fn public_custom_random_sex_then_short_circuit_peace_draw() {
    let mut rs = rules(true, true);
    let def = rs
        .monsters
        .iter_mut()
        .find(|d| d.name == "Owned candidate")
        .unwrap();
    def.flags.clear();
    def.always_hostile = false;
    let mut w = world(rs);
    w.alignment_record = -20; // rn2(1)==0, no second peace draw
    let mut expected = w.rng.clone();
    let _ = expected.random_range(0..1u32);
    let _ = expected.random_range(0..2u32);
    let hp = native_hp(&mut expected, 3);
    let sex = if expected.random_range(0..2u32) == 0 {
        ActorSex::Male
    } else {
        ActorSex::Female
    };
    let _ = expected.random_range(0..1u32);
    let id = w.spawn_random_monster_at(DEST).unwrap();
    assert_eq!(w.arena.actors[id].hp, hp);
    assert_eq!(w.arena.actors[id].sex, Some(sex));
    assert!(!w.arena.actors[id].is_peaceful);
    assert_eq!(w.rng, expected);
}

#[test]
fn public_exact_stairs_and_owned_boulder_terrain_acceptance() {
    let mut w = world(rules(true, false));
    w.level.stairs_down = DEST;
    w.level.set_tile(DEST, Tile::Stairs { up: false });
    let id = w.spawn_random_monster_at(DEST).unwrap();
    assert_eq!(w.arena.actors[id].coord, DEST); // no generic stairs rejection
    let boulder = w
        .ruleset
        .create_item_record_by_id(
            nethacked_data::ItemKindId::BOULDER,
            ItemLocation::Floor(OTHER),
            Buc::Uncursed,
        )
        .unwrap();
    w.arena.spawn_item(boulder);
    let mut rs = (*w.ruleset).clone();
    rs.monsters
        .iter_mut()
        .find(|d| d.name == "Owned candidate")
        .unwrap()
        .flags
        .push("rockthrow".into());
    rs.reindex();
    w.ruleset = Arc::new(rs);
    let mut expected = w.rng.clone();
    let _ = expected.random_range(0..1u32);
    let _ = expected.random_range(0..2u32);
    let id = w.spawn_random_monster_at(OTHER).unwrap();
    assert_eq!(w.arena.actors[id].coord, OTHER);
    assert_eq!(w.rng, expected);
}

#[test]
fn public_occupied_hero_column_zero_no_eligible_and_genocide_reject_without_initialization() {
    for control in 0..6 {
        let mut rs = rules(true, true);
        let def = rs
            .monsters
            .iter_mut()
            .find(|d| d.name == "Owned candidate")
            .unwrap();
        if control == 1 {
            def.frequency = 0;
        }
        if control == 2 {
            def.difficulty = 5;
        } // max=(7+2)/2=4
        if control == 3 {
            def.gen_flags.push("nogen".into());
        }
        let mut w = world(rs);
        if control == 4 {
            w.genocide_registry
                .genocided_species
                .insert("owned candidate".into());
        }
        if control == 5 {
            w.genocide_registry.genocided_classes.insert('c');
        }
        let dest = if control == 0 {
            w.arena.actors[w.player_id].coord
        } else {
            DEST
        };
        let before = w.rng.clone();
        assert_eq!(w.spawn_random_monster_at(dest), None);
        assert_eq!(w.rng, before);
        assert_eq!(w.arena.actors.len(), 1);
    }
    let mut w = world(rules(false, true));
    let id = w.spawn_random_monster_at(DEST).unwrap();
    let before = w.rng.clone();
    assert_eq!(w.spawn_random_monster_at(DEST), None);
    assert_eq!(w.spawn_random_monster_at(Coord::new_unchecked(0, 10)), None);
    assert_eq!(
        w.spawn_random_monster_at(Coord {
            x: nethacked_types::COLNO,
            y: 10
        }),
        None
    );
    assert_eq!(
        w.spawn_random_monster_at(Coord {
            x: 20,
            y: nethacked_types::ROWNO
        }),
        None
    );
    assert_eq!(w.rng, before);
    assert_eq!(w.arena.actors[id].coord, DEST);
    assert_eq!(w.arena.actors.len(), 2);
}

#[test]
fn public_source_51st_selection_accepts_without_goodpos_and_never_moves_leader() {
    // C :1226 checks goodpos only first50 choices, then accepts51st.
    for obstruction in [Tile::Stone, Tile::Room] {
        let mut w = world(rules(true, true));
        w.level.set_tile(DEST, obstruction);
        w.level
            .set_engraving(DEST, Engraving::new("Elbereth", EngravingMedium::Burned));
        let mut expected = w.rng.clone();
        for _ in 0..51 {
            let _ = expected.random_range(0..1u32);
        }
        let _ = expected.random_range(0..2u32);
        let hp = native_hp(&mut expected, 3);
        let id = w.spawn_random_monster_at(DEST).unwrap();
        assert_eq!(w.arena.actors[id].coord, DEST);
        assert_eq!(w.arena.actors[id].hp, hp);
        assert_eq!(w.rng, expected);
        assert_eq!(w.arena.actors.len(), 2);
    }
}

#[test]
fn public_difficulty_min_max_boundaries_and_gen_flags_remain_owned() {
    for (difficulty, frequency, flags, accepted) in [
        (0, 1, vec![], false), // minimum depth7/6=1
        (1, 1, vec![], true),
        (4, 1, vec!["sgroup"], true), // max=(7+2)/2=4
        (5, 1, vec![], false),
        (1, 0, vec![], false),
        (1, 1, vec!["uniq"], false),
        (1, 1, vec!["nogen"], false),
        (1, 1, vec!["hell"], false),
    ] {
        let mut rs = rules(true, false);
        let def = rs
            .monsters
            .iter_mut()
            .find(|d| d.name == "Owned candidate")
            .unwrap();
        def.difficulty = difficulty;
        def.frequency = frequency;
        def.gen_flags = flags.into_iter().map(str::to_owned).collect();
        let mut w = world(rs);
        assert_eq!(w.spawn_random_monster_at(DEST).is_some(), accepted);
        assert_eq!(w.arena.actors.len(), if accepted { 2 } else { 1 });
    }
}

#[test]
fn public_full_save_native_continuation_and_legacy_no_new_state_or_load_rng() {
    let mut direct = world(rules(true, true));
    direct.spawn_random_monster_at(DEST).unwrap();
    let before_load = direct.rng.clone();
    let mut restored = load(&direct);
    assert_eq!(restored.rng, before_load);
    assert_eq!(canonical_save(&direct), canonical_save(&restored));
    let a = direct.spawn_random_monster_at(OTHER).unwrap();
    let b = restored.spawn_random_monster_at(OTHER).unwrap();
    assert_eq!(a, b);
    assert_eq!(canonical_save(&direct), canonical_save(&restored));
    assert_eq!(direct.rng.next_u64(), restored.rng.next_u64());

    // No P10a state fields were introduced; old owned definitions omit metadata.
    let mut wire = serde_json::to_value(&*direct.ruleset).unwrap();
    for def in wire["monsters"].as_array_mut().unwrap() {
        def.as_object_mut().unwrap().remove("gen_flags");
        def.as_object_mut().unwrap().remove("frequency");
        def.as_object_mut().unwrap().remove("difficulty");
    }
    let mut old: Ruleset = serde_json::from_value(wire).unwrap();
    old.reindex();
    assert!(old
        .monsters
        .iter()
        .all(|d| d.gen_flags.is_empty() && d.frequency == 0 && d.difficulty == 0));
}

#[test]
fn literal_adjusted_hp_families_original_encoded_level_and_fixed_policy() {
    let rs = rules(true, true);
    let custom = rs.monster("Owned candidate").unwrap();
    for (base, difficulty, hero, level, hp) in [
        (0, 0, 1, 0, 2),
        (1, 0, 1, 0, 2),
        (2, 2, 2, 2, 3),
        (2, 6, 2, 2, 3),
        (2, 7, 2, 3, 4),
        (2, 2, 5, 2, 3),
        (2, 2, 6, 3, 4),
        (10, 100, 100, 15, 16),
        (40, 0, 1, 39, 40),
        (40, 100, 100, 49, 50),
        (50, 100, 100, 22, 88),
    ] {
        let mut def = custom.clone();
        def.level = base;
        let mut zero = ZeroRng;
        let a = rs.create_random_monster_record_from_def(
            &def,
            DEST,
            Default::default(),
            (difficulty, hero),
            &mut zero,
        );
        assert_eq!((a.level, a.hp, a.max_hp), (level, hp, hp));
    }
    for (species, base, level, hp) in [
        (M::IRON_GOLEM, 40, 49, 120),
        (M::DEATH, 40, 49, 11),
        (M::RED_DRAGON, 2, 3, 15),
        (M::AIR_ELEMENTAL, 2, 3, 3),
    ] {
        let mut def = custom.clone();
        def.id = Some(species);
        def.level = base;
        let mut zero = ZeroRng;
        let context = nethacked_data::monsters::MonsterSpawnContext {
            endgame_plane: if species == M::AIR_ELEMENTAL {
                Some(nethacked_types::EndgamePlane::Air)
            } else {
                None
            },
        };
        let a =
            rs.create_random_monster_record_from_def(&def, DEST, context, (100, 100), &mut zero);
        // Use narrowly chosen adjusted boundaries for each special family.
        if base == 40 {
            assert_eq!((a.level, a.hp), (level, hp));
        } else if species == M::RED_DRAGON {
            assert_eq!((a.level, a.hp), (3, 15));
        } else {
            assert_eq!((a.level, a.hp), (3, 9));
        }
    }
}

#[test]
fn actual_public_descend_room_route_exact_owned_hp_and_nogrp() {
    let mut w = world(rules(true, true));
    w.depth = 6;
    w.level.stairs_down = w.arena.actors[w.player_id].coord;
    w.level
        .set_tile(w.level.stairs_down, Tile::Stairs { up: false });
    let amulet = w
        .ruleset
        .create_item_record_by_id(
            nethacked_data::ItemKindId::AMULET_OF_YENDOR,
            ItemLocation::CarriedBy(w.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    w.arena.spawn_item(amulet); // skips room probability gate, source amulet branch
    let mut expected_rng = w.rng.clone();
    let level = nethacked_dungeon::generate_dungeon_level(&mut expected_rng);
    let mut expected = vec![];
    for room in level.rooms.iter().skip(1).take(level.rooms.len() - 2) {
        let _ = expected_rng.random_range(0..1u32);
        let _ = expected_rng.random_range(0..2u32);
        expected.push((room.center(), native_hp(&mut expected_rng, 3), 3));
    }
    assert!(!expected.is_empty());
    w.step_player_action(ActionAst::Descend);
    assert_eq!(w.depth, 7);
    let mut got: Vec<_> = w
        .arena
        .actors
        .iter()
        .filter(|(_, a)| !a.is_player)
        .map(|(_, a)| {
            assert_eq!(
                a.species,
                Some(ActorSpecies::Custom("owned candidate".into()))
            );
            assert_eq!(a.name, "Owned candidate");
            assert_eq!(a.movement, 0);
            (a.coord, a.hp, a.level)
        })
        .collect();
    // Arena reuses vacated slots; compare exact spatial/HP tuples, not slot order.
    got.sort_by_key(|a| (a.0.x, a.0.y));
    expected.sort_by_key(|a| (a.0.x, a.0.y));
    assert_eq!(got, expected); // exact room-count and HP; no SGROUP expansion
    let mut restored = load(&w);
    let next = w.level.stairs_down;
    w.arena.actors[w.player_id].coord = next;
    restored.arena.actors[restored.player_id].coord = next;
    assert_eq!(
        w.step_player_action(ActionAst::Descend),
        restored.step_player_action(ActionAst::Descend)
    );
    assert_eq!(canonical_save(&w), canonical_save(&restored));
    assert_eq!(w.rng.next_u64(), restored.rng.next_u64());
}

#[test]
fn public_requested_room_geography_overrides_stale_current_depth_and_branch() {
    let mut rs = rules(true, true);
    rs.monsters
        .iter_mut()
        .find(|d| d.name == "Owned candidate")
        .unwrap()
        .difficulty = 3;
    let mut w = world(rs);
    w.depth = 1; // current max=(1+2)/2=1 rejects candidate difficulty3
    w.current_branch = BranchId::GnomishMines; // requested Doom has no align shift
    let amulet = w
        .ruleset
        .create_item_record_by_id(
            nethacked_data::ItemKindId::AMULET_OF_YENDOR,
            ItemLocation::CarriedBy(w.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    w.arena.spawn_item(amulet);
    let mut expected_rng = w.rng.clone();
    let level = nethacked_dungeon::generate_dungeon_level(&mut expected_rng);
    let mut expected = vec![];
    for room in level.rooms.iter().skip(1).take(level.rooms.len() - 2) {
        let _ = expected_rng.random_range(0..1u32);
        let _ = expected_rng.random_range(0..2u32);
        expected.push((room.center(), native_hp(&mut expected_rng, 3), 3));
    }
    assert!(!expected.is_empty());
    w.unpack_or_generate_level(BranchId::DungeonsOfDoom, 7);
    assert_eq!((w.current_branch, w.depth), (BranchId::GnomishMines, 1));
    let mut got: Vec<_> = w
        .arena
        .actors
        .iter()
        .filter(|(_, a)| !a.is_player)
        .map(|(_, a)| {
            assert_eq!(
                a.species,
                Some(ActorSpecies::Custom("owned candidate".into()))
            );
            assert_eq!(a.movement, 0);
            (a.coord, a.hp, a.level)
        })
        .collect();
    got.sort_by_key(|a| (a.0.x, a.0.y));
    expected.sort_by_key(|a| (a.0.x, a.0.y));
    assert_eq!(got, expected); // depth7 gives adjusted3; requested context selects difficulty3
}

#[test]
fn public_random_spawn_noncanonical_actor_removals_preserve_full_save_ids_and_native_rng() {
    let mut direct = world(rules(true, true));
    // Fresh dense actor arena: hero then three monsters occupy slots1/2/3/4.
    // Remove the last monster, then the first, deliberately reversing the
    // freelist reconstructed by locked SlotMap serialization.
    let hero = direct.arena.actors[direct.player_id].clone();
    direct.arena.actors = Default::default();
    direct.player_id = direct.arena.spawn_actor(hero);
    let first = direct.spawn_random_monster_at(DEST).unwrap();
    let middle = direct.spawn_random_monster_at(OTHER).unwrap();
    let last = direct
        .spawn_random_monster_at(Coord::new_unchecked(22, 10))
        .unwrap();
    assert_eq!(direct.arena.actors.len(), 4);
    assert!(direct.arena.actors.remove(last).is_some());
    assert!(direct.arena.actors.remove(first).is_some());
    assert_eq!(direct.arena.actors.len(), 2);
    let live_middle = serde_json::to_value(&direct.arena.actors[middle]).unwrap();
    direct.rng = ChaCha8Rng::seed_from_u64(99);
    let before_load = direct.rng.clone();
    let mut restored = load(&direct);
    assert_eq!(restored.rng, before_load);
    assert_eq!(canonical_save(&direct), canonical_save(&restored));

    let mut expected = before_load;
    let _ = expected.random_range(0..1u32); // one candidate
    let _ = expected.random_range(0..2u32); // next_ident
    let hp = native_hp(&mut expected, 3); // literal adjusted3, female/hostile
    let a = direct.spawn_random_monster_at(DEST).unwrap();
    let b = restored.spawn_random_monster_at(DEST).unwrap();
    for (w, id) in [(&direct, a), (&restored, b)] {
        let actor = &w.arena.actors[id];
        assert_eq!(
            (actor.coord, actor.level, actor.hp, actor.max_hp),
            (DEST, 3, hp, hp)
        );
        assert_eq!(
            actor.species,
            Some(ActorSpecies::Custom("owned candidate".into()))
        );
        assert_eq!(actor.sex, Some(ActorSex::Female));
        assert!(!actor.is_peaceful);
        assert_eq!(actor.malign, 0); // hostile, maligntyp0
        assert_eq!(actor.movement, 0);
        assert_eq!(
            serde_json::to_value(&w.arena.actors[middle]).unwrap(),
            live_middle
        );
        assert_eq!(w.rng, expected); // allocator normalization must never draw
    }
    assert_eq!(
        a, b,
        "same public insertion ID after noncanonical removal/save boundary"
    );
    assert_eq!(canonical_save(&direct), canonical_save(&restored));
    assert_eq!(direct.rng.next_u64(), restored.rng.next_u64());
}
