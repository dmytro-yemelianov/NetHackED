use nethacked_data::monsters::{compute_monster_hp, MonsterSpawnContext};
use nethacked_data::{create_ghost_record, MonsterSpeciesId as Id, Ruleset};
use nethacked_types::{Coord, EndgamePlane};
use rand::rngs::StdRng;
use rand::{Rng, RngCore, SeedableRng};

fn dice(rng: &mut StdRng, count: u32, sides: u32) -> u32 {
    (0..count).map(|_| rng.random_range(1..=sides)).sum()
}

#[test]
fn seeded_dice_formulas_and_draw_order() {
    let rules = Ruleset::vanilla();
    for seed in 0..64 {
        for id in [
            Id::WARG,
            Id::DEATH,
            Id::FAMINE,
            Id::PESTILENCE,
            Id::RED_DRAGON,
            Id::BABY_RED_DRAGON,
            Id::JACKAL,
        ] {
            let def = rules.monster_by_id(id).unwrap();
            let mut actual = StdRng::seed_from_u64(seed);
            let mut expected = actual.clone();
            let (hp, minimum) = match id {
                Id::DEATH | Id::FAMINE | Id::PESTILENCE => (dice(&mut expected, 10, 8), 10),
                Id::RED_DRAGON => (4 * def.level + dice(&mut expected, def.level, 4), def.level),
                _ if def.level == 0 => (expected.random_range(1..=4u32), 1),
                _ => (dice(&mut expected, def.level, 8), def.level),
            };
            let hp = hp + u32::from(hp == minimum);
            assert_eq!(
                compute_monster_hp(def, MonsterSpawnContext::default(), &mut actual),
                hp,
                "species {id:?}, seed {seed}"
            );
            assert_eq!(actual.next_u64(), expected.next_u64());
        }
    }
}

#[test]
fn fixed_special_cases_do_not_draw_rng() {
    let rules = Ruleset::vanilla();
    for (id, hp) in [
        (Id::STRAW_GOLEM, 20),
        (Id::PAPER_GOLEM, 20),
        (Id::ROPE_GOLEM, 30),
        (Id::LEATHER_GOLEM, 40),
        (Id::GOLD_GOLEM, 60),
        (Id::WOOD_GOLEM, 50),
        (Id::FLESH_GOLEM, 40),
        (Id::CLAY_GOLEM, 70),
        (Id::STONE_GOLEM, 100),
        (Id::GLASS_GOLEM, 80),
        (Id::IRON_GOLEM, 120),
    ] {
        let mut rng = StdRng::seed_from_u64(9);
        let mut expected = rng.clone();
        assert_eq!(
            compute_monster_hp(
                rules.monster_by_id(id).unwrap(),
                MonsterSpawnContext::default(),
                &mut rng
            ),
            hp
        );
        assert_eq!(rng.next_u64(), expected.next_u64());
    }
    let mut encoded = rules.monster("jackal").unwrap().clone();
    encoded.level = 50;
    encoded.flags.push("male".into());
    let mut rng = StdRng::seed_from_u64(9);
    let mut expected = rng.clone();
    let actor = rules.create_monster_record_from_def(
        &encoded,
        Coord::new_unchecked(1, 1),
        MonsterSpawnContext::default(),
        &mut rng,
    );
    assert_eq!((actor.hp, actor.max_hp, actor.level), (88, 88, 22));
    assert_eq!(rng.next_u64(), expected.next_u64());
    let context = MonsterSpawnContext {
        endgame_plane: Some(EndgamePlane::Astral),
    };
    let dragon = rules.monster("red dragon").unwrap();
    assert_eq!(
        compute_monster_hp(dragon, context, &mut rng),
        8 * dragon.level
    );
    assert_eq!(rng.next_u64(), expected.next_u64());
}

#[test]
fn level_zero_and_minimum_boost() {
    let rules = Ruleset::vanilla();
    let mut def = rules.monster("jackal").unwrap().clone();
    for level in [0, 1] {
        def.level = level;
        let mut observed_minimum = false;
        for seed in 0..128 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut expected = rng.clone();
            let rolled: u32 = expected.random_range(1..=if level == 0 { 4 } else { 8 });
            observed_minimum |= rolled == 1;
            assert_eq!(
                compute_monster_hp(&def, MonsterSpawnContext::default(), &mut rng),
                rolled.max(2)
            );
            assert_eq!(rng.next_u64(), expected.next_u64());
        }
        assert!(observed_minimum);
    }
}

#[test]
fn home_elementals_triple_only_on_matching_plane() {
    let rules = Ruleset::vanilla();
    for (id, plane) in [
        (Id::AIR_ELEMENTAL, EndgamePlane::Air),
        (Id::EARTH_ELEMENTAL, EndgamePlane::Earth),
        (Id::FIRE_ELEMENTAL, EndgamePlane::Fire),
        (Id::WATER_ELEMENTAL, EndgamePlane::Water),
    ] {
        let def = rules.monster_by_id(id).unwrap();
        let mut rng = StdRng::seed_from_u64(3);
        let mut expected = rng.clone();
        let hp = dice(&mut expected, def.level, 8);
        assert_eq!(
            compute_monster_hp(
                def,
                MonsterSpawnContext {
                    endgame_plane: Some(plane)
                },
                &mut rng
            ),
            3 * hp
        );
        assert_eq!(rng.next_u64(), expected.next_u64());
        let mut rng = StdRng::seed_from_u64(3);
        assert_eq!(
            compute_monster_hp(
                def,
                MonsterSpawnContext {
                    endgame_plane: Some(EndgamePlane::Astral)
                },
                &mut rng
            ),
            hp + u32::from(hp == def.level)
        );
    }
}

#[test]
fn old_definition_loads_fixed_and_round_trips_policy() {
    let rules = Ruleset::vanilla();
    let mut value = serde_json::to_value(rules.monster("jackal").unwrap()).unwrap();
    value.as_object_mut().unwrap().remove("sample_hp");
    let mut def: nethacked_data::MonsterDef = serde_json::from_value(value).unwrap();
    assert!(!def.sample_hp);
    def.flags.push("male".into());
    def.base_hp = 11;
    def.max_hp = 17;
    let mut rng = StdRng::seed_from_u64(4);
    let mut expected = rng.clone();
    let actor = rules.create_monster_record_from_def(
        &def,
        Coord::new_unchecked(1, 1),
        MonsterSpawnContext::default(),
        &mut rng,
    );
    assert_eq!((actor.hp, actor.max_hp), (11, 17));
    assert_eq!(rng.next_u64(), expected.next_u64());
    for policy in [false, true] {
        def.sample_hp = policy;
        let loaded: nethacked_data::MonsterDef =
            serde_json::from_slice(&serde_json::to_vec(&def).unwrap()).unwrap();
        assert_eq!(loaded, def);
    }
    let ghost = create_ghost_record("bones", 7, 53, Coord::new_unchecked(1, 1));
    assert_eq!((ghost.hp, ghost.max_hp, ghost.level), (53, 53, 7));
}

#[test]
fn all_minimum_dice_boost_and_special_branch_order() {
    struct MinimumRng;
    impl RngCore for MinimumRng {
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
    let rules = Ruleset::vanilla();
    assert_eq!(
        compute_monster_hp(
            rules.monster("Death").unwrap(),
            MonsterSpawnContext::default(),
            &mut MinimumRng
        ),
        11
    );
    assert_eq!(
        compute_monster_hp(
            rules.monster("warg").unwrap(),
            MonsterSpawnContext::default(),
            &mut MinimumRng
        ),
        8
    );
    let mut elemental = rules.monster("air elemental").unwrap().clone();
    elemental.level = 1;
    assert_eq!(
        compute_monster_hp(
            &elemental,
            MonsterSpawnContext {
                endgame_plane: Some(EndgamePlane::Air)
            },
            &mut MinimumRng
        ),
        3
    );
    let mut golem = rules.monster("straw golem").unwrap().clone();
    golem.level = 50;
    let actor = rules.create_monster_record_from_def(
        &golem,
        Coord::new_unchecked(1, 1),
        MonsterSpawnContext::default(),
        &mut MinimumRng,
    );
    assert_eq!((actor.hp, actor.level), (20, 50));
    let mut rider = rules.monster("Death").unwrap().clone();
    rider.level = 50;
    let actor = rules.create_monster_record_from_def(
        &rider,
        Coord::new_unchecked(1, 1),
        MonsterSpawnContext::default(),
        &mut MinimumRng,
    );
    assert_eq!((actor.hp, actor.level), (11, 50));
}
