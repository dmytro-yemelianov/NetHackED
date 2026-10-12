use nethacked_arena::ItemLocation;
use nethacked_core::corpse_timeout::CorpseTimeoutContext;
use nethacked_data::containers::ContainerContext;
use nethacked_data::corpses::{sample_corpse_timeout, CorpseTimeoutRecipe as Recipe};
use nethacked_data::monsters::CorpseTimeoutPolicy as Policy;
use nethacked_data::{ItemKindId as K, MonsterSpeciesId as M, Ruleset};
use rand::{Rng, RngCore, SeedableRng};

fn context() -> CorpseTimeoutContext {
    CorpseTimeoutContext {
        elapsed_age: 0,
        moves: 73,
        hero_level: 18,
        in_mklev: true,
    }
}
fn only_jackal() -> Ruleset {
    let mut rules = (*Ruleset::vanilla()).clone();
    for def in &mut rules.monsters {
        def.frequency = u32::from(def.id == Some(M::JACKAL));
        def.difficulty = 5;
        def.gen_flags
            .retain(|flag| flag != "nogen" && flag != "uniq" && flag != "hell" && flag != "nohell");
    }
    rules.reindex();
    rules
}

#[test]
fn owned_never_and_rider_policies_preserve_source_draw_boundary() {
    let rules = Ruleset::vanilla();
    for id in [M::LIZARD, M::LICHEN] {
        assert_eq!(
            sample_corpse_timeout(rules.monster_by_id(id).unwrap(), context(), |_| panic!(
                "Never must draw nothing"
            )),
            None
        );
    }
    let trace = [(1000, 999), (4, 1), (2, 0)];
    let mut draws = trace.iter();
    assert_eq!(
        sample_corpse_timeout(rules.monster_by_id(M::DEATH).unwrap(), context(), |bound| {
            let &(expected, value) = draws.next().unwrap();
            assert_eq!(bound, expected);
            value
        }),
        Some(Recipe::UnsupportedRider)
    );
    assert_eq!(draws.len(), 0);
    // Custom names and canonical identity do not override owned policy.
    let mut def = rules.monster_by_id(M::LIZARD).unwrap().clone();
    def.corpse_timeout_policy = Policy::Ordinary;
    def.name = "renamed lizard".into();
    let mut draws = trace.iter();
    assert_eq!(
        sample_corpse_timeout(&def, context(), |bound| {
            let &(expected, value) = draws.next().unwrap();
            assert_eq!(bound, expected);
            value
        }),
        Some(Recipe::Rot {
            sampled_at: 73,
            delay: 237
        })
    );
    assert_eq!(draws.len(), 0);
    def.id = None;
    def.name = "pack specimen".into();
    def.corpse_timeout_policy = Policy::Never;
    assert_eq!(
        sample_corpse_timeout(&def, context(), |_| panic!("custom Never")),
        None
    );
}

#[test]
fn generated_ice_children_sample_before_next_selection_with_native_rng_continuation() {
    let rules = only_jackal();
    let trace = [
        (21, 2),
        (1, 0),
        (2, 0),
        (1000, 999),
        (4, 1),
        (2, 0),
        (1, 0),
        (2, 0),
        (1000, 0),
        (4, 0),
        (4, 1),
        (2, 1),
    ];
    let mut rng = rand::rngs::StdRng::seed_from_u64(831);
    let mut expected_rng = rng.clone();
    for &(bound, _) in &trace {
        let _: u32 = expected_rng.random_range(0..bound);
    }
    let mut draws = trace.iter();
    let tree = rules
        .create_random_item_tree(
            &rules.item_by_id(K::ICE_BOX).unwrap().name,
            ItemLocation::Limbo,
            ContainerContext {
                hero_level: 18,
                level_difficulty: 10,
                moves: 73,
                in_mklev: true,
            },
            |bound| {
                let &(expected, value) = draws.next().expect("extra tree draw");
                assert_eq!(bound, expected);
                let _: u32 = rng.random_range(0..bound);
                value
            },
        )
        .unwrap();
    assert_eq!(draws.len(), 0);
    assert_eq!(rng.next_u64(), expected_rng.next_u64());
    assert_eq!(tree.contents.len(), 2);
    assert_eq!(
        tree.contents[0].corpse_timeout,
        Some(Recipe::Rot {
            sampled_at: 73,
            delay: 237
        })
    );
    assert_eq!(
        tree.contents[1].corpse_timeout,
        Some(Recipe::Rot {
            sampled_at: 73,
            delay: 275
        })
    );
    assert!(tree.contents.iter().all(|child| child.item.corpse_age == 0));
}

#[test]
fn pure_corpse_factories_have_no_transient_recipe_or_random_draw_requirement() {
    let rules = Ruleset::vanilla();
    let a = rules
        .create_corpse_record("human", ItemLocation::Limbo)
        .unwrap();
    let b = rules
        .create_corpse_record("human", ItemLocation::Limbo)
        .unwrap();
    assert_eq!(a, b);
    assert_eq!(a.corpse_age, 0);
}

#[test]
fn generated_context_and_policy_controls_have_complete_literal_traces() {
    let mut rules = only_jackal();
    let name = rules.item_by_id(K::CORPSE).unwrap().name.clone();
    // XL18 has five zero rn2(4) draws, not XL14/15's four. At moves=0
    // the fresh corpse has elapsed age0; rnz(10)=60 gives delay300.
    let trace = [
        (1, 0),
        (2, 0),
        (1000, 0),
        (4, 0),
        (4, 0),
        (4, 0),
        (4, 0),
        (4, 0),
        (2, 1),
    ];
    let mut draws = trace.iter();
    let tree = rules
        .create_random_item_tree(
            &name,
            ItemLocation::Limbo,
            ContainerContext {
                hero_level: 18,
                level_difficulty: 10,
                moves: 0,
                in_mklev: false,
            },
            |bound| {
                let &(expected, value) = draws.next().expect("extra context draw");
                assert_eq!(bound, expected);
                value
            },
        )
        .unwrap();
    assert_eq!(draws.len(), 0);
    assert_eq!(
        tree.corpse_timeout,
        Some(Recipe::Rot {
            sampled_at: 0,
            delay: 300
        })
    );
    let def = rules
        .monsters
        .iter_mut()
        .find(|d| d.id == Some(M::JACKAL))
        .unwrap();
    def.corpse_timeout_policy = Policy::Never;
    def.name = "renamed timeout exemption".into();
    rules.reindex();
    let mut rng = rand::rngs::StdRng::seed_from_u64(913);
    let mut expected_rng = rng.clone();
    let _: u32 = expected_rng.random_range(0..1);
    let _: u32 = expected_rng.random_range(0..2);
    let mut calls = 0;
    let never = rules
        .create_random_item_tree(
            &name,
            ItemLocation::Limbo,
            ContainerContext {
                hero_level: 18,
                level_difficulty: 10,
                moves: 100,
                in_mklev: true,
            },
            |bound| {
                calls += 1;
                assert_eq!(bound, [1, 2][calls - 1]);
                rng.random_range(0..bound)
            },
        )
        .unwrap();
    assert_eq!(calls, 2);
    assert_eq!(never.corpse_timeout, None);
    assert_eq!(rng.next_u64(), expected_rng.next_u64());
}

#[test]
fn owned_troll_trials_follow_one_noise_sample_with_literal_success_failure_and_rng() {
    let rules = Ruleset::vanilla();
    let mut troll = rules.monster_by_id(M::TROLL).unwrap().clone();
    // Owned class wins over both canonical identity and display name.
    troll.id = None;
    troll.name = "custom timeout specimen".into();
    for (success_age, age, expected) in [
        (
            Some(2),
            249,
            Recipe::Revive {
                sampled_at: 73,
                delay: 2,
            },
        ),
        (
            Some(50),
            250,
            Recipe::Revive {
                sampled_at: 73,
                delay: 50,
            },
        ),
        (
            None,
            249,
            Recipe::Rot {
                sampled_at: 73,
                delay: -12,
            },
        ),
    ] {
        // rnz(25)=12; at elapsed249 ordinary rot delay is 1+12-25=-12.
        let mut trace = vec![(1000, 999), (4, 1), (2, 0)];
        let trials = success_age.map_or(49, |age| age - 1);
        trace.extend((0..trials).map(|i| (37, u32::from(Some(i + 2) != success_age))));
        let mut rng = rand::rngs::StdRng::seed_from_u64(831);
        let mut expected_rng = rng.clone();
        for &(bound, _) in &trace {
            let _: u32 = expected_rng.random_range(0..bound);
        }
        let mut draws = trace.iter();
        assert_eq!(
            sample_corpse_timeout(
                &troll,
                CorpseTimeoutContext {
                    elapsed_age: age,
                    ..context()
                },
                |bound| {
                    let &(expected, value) = draws.next().expect("extra Troll draw");
                    assert_eq!(bound, expected);
                    let _: u32 = rng.random_range(0..bound);
                    value
                }
            ),
            Some(expected)
        );
        assert_eq!(draws.len(), 0);
        assert_eq!(rng.next_u64(), expected_rng.next_u64());
    }
    // A Troll-like name and canonical Troll id cannot replace a non-T class.
    troll.id = Some(M::TROLL);
    troll.name = "troll".into();
    troll.glyph = 'd';
    let mut draws = [(1000, 0), (4, 1), (2, 1)].into_iter();
    assert_eq!(
        sample_corpse_timeout(&troll, context(), |bound| {
            let (expected, value) = draws.next().expect("class inference draw");
            assert_eq!(bound, expected);
            value
        }),
        Some(Recipe::Rot {
            sampled_at: 73,
            delay: 250
        })
    );
    assert_eq!(draws.len(), 0);
}

#[test]
fn troll_class_never_and_rider_policies_precede_trials() {
    let rules = Ruleset::vanilla();
    let mut troll = rules.monster_by_id(M::TROLL).unwrap().clone();
    troll.corpse_timeout_policy = Policy::Never;
    assert_eq!(
        sample_corpse_timeout(&troll, context(), |_| panic!("Never draws")),
        None
    );
    troll.corpse_timeout_policy = Policy::Rider;
    let mut draws = [(1000, 0), (4, 1), (2, 1)].into_iter();
    assert_eq!(
        sample_corpse_timeout(&troll, context(), |bound| {
            let (expected, value) = draws.next().expect("Rider fell through to Troll trials");
            assert_eq!(bound, expected);
            value
        }),
        Some(Recipe::UnsupportedRider)
    );
    assert_eq!(draws.len(), 0);
}
