//! Literal construction order from NetHack 5.0 mkobj.c:1200–1230,2052–2116,
//! mon.c:549–579,892–900; no lifecycle callbacks are exercised.
use nethacked_arena::{ActorSex, CorpseSpecies, ItemLocation};
use nethacked_data::containers::ContainerContext;
use nethacked_data::corpses::{CorpseTimeoutRecipe as Recipe, CorpseTimerOperation as Op};
use nethacked_data::generation::{DungeonAlign, MonsterGenContext};
use nethacked_data::monsters::CorpseTimeoutPolicy as Policy;
use nethacked_data::{ItemKindId as K, MonsterSpeciesId as M, Ruleset};
use nethacked_types::{Coord, DamageType};
use rand::{Rng, RngCore, SeedableRng};

fn context() -> (ContainerContext, MonsterGenContext) {
    (
        ContainerContext {
            hero_level: 18,
            level_difficulty: 10,
            moves: 73,
            in_mklev: false,
        },
        MonsterGenContext {
            hero_level: 18,
            level_difficulty: 10,
            in_hell: false,
            align: DungeonAlign::None,
            temperature: 0,
        },
    )
}

fn rules(initial: usize, final_kind: usize) -> Ruleset {
    let mut rules = (*Ruleset::vanilla()).clone();
    for def in &mut rules.monsters {
        def.frequency = u32::from(def.id == Some(M::JACKAL));
        def.difficulty = 5;
        def.gen_flags
            .retain(|f| f != "nogen" && f != "uniq" && f != "hell" && f != "nohell");
    }
    let def = rules
        .monsters
        .iter_mut()
        .find(|d| d.id == Some(M::JACKAL))
        .unwrap();
    def.flags.clear();
    def.glyph = if initial == 1 { 'T' } else { 'd' };
    def.corpse_timeout_policy = match initial {
        2 => Policy::Never,
        3 => Policy::Rider,
        _ => Policy::Ordinary,
    };
    let mut final_def = def.clone();
    final_def.id = None;
    final_def.name = "final specimen".into();
    final_def.frequency = 0;
    final_def.level = 7;
    final_def.flags = vec!["male".into()];
    final_def.glyph = if final_kind == 1 { 'T' } else { 'd' };
    final_def.corpse_timeout_policy = match final_kind {
        2 => Policy::Never,
        3 => Policy::Rider,
        _ => Policy::Ordinary,
    };
    rules.monsters.push(final_def);
    rules.reindex();
    rules
}

#[test]
fn literal_old_or_final_special_transitions_preserve_every_draw_and_operation() {
    let (ctx, monsters) = context();
    for initial in 0..4 {
        for final_kind in 0..4 {
            let rules = rules(initial, final_kind);
            let actor = rules
                .monster("final specimen")
                .unwrap()
                .create_record_with_hp(Coord::new_unchecked(8, 8), 0, 3, 7);
            let mut trace = vec![(1, 0), (2, 1)]; // owned selection then initial female
            if initial != 2 {
                trace.extend([(1000, 999), (4, 1), (2, 0)]);
                if initial == 1 {
                    trace.push((37, 0));
                }
            }
            let replacement = initial != 0 || final_kind != 0;
            if replacement && final_kind != 2 {
                trace.extend([(1000, 0), (4, 1), (2, 1)]);
                if final_kind == 1 {
                    trace.extend([(37, 1), (37, 1), (37, 0)]);
                }
            }
            let initial_recipe = match initial {
                0 => Some(Recipe::Rot {
                    sampled_at: 73,
                    delay: 245,
                }),
                1 => Some(Recipe::Revive {
                    sampled_at: 73,
                    delay: 2,
                }),
                2 => None,
                _ => Some(Recipe::UnsupportedRider),
            };
            let final_recipe = if replacement {
                match final_kind {
                    0 => Some(Recipe::Rot {
                        sampled_at: 73,
                        delay: 250,
                    }),
                    1 => Some(Recipe::Revive {
                        sampled_at: 73,
                        delay: 4,
                    }),
                    2 => None,
                    _ => Some(Recipe::UnsupportedRider),
                }
            } else {
                initial_recipe
            };
            let mut expected_ops = Vec::new();
            if let Some(recipe) = initial_recipe {
                expected_ops.push(Op::Start(recipe));
            }
            if replacement {
                expected_ops.push(Op::Cancel);
                if let Some(recipe) = final_recipe {
                    expected_ops.push(Op::Start(recipe));
                }
            }
            let mut rng = rand::rngs::StdRng::seed_from_u64(928);
            let mut expected_rng = rng.clone();
            for &(bound, _) in &trace {
                let _: u32 = expected_rng.random_range(0..bound);
            }
            let mut calls = trace.iter();
            let tree = rules
                .create_initialized_corpse_for_actor(
                    &actor,
                    ItemLocation::Limbo,
                    ctx,
                    &monsters,
                    |_| false,
                    |bound| {
                        let &(expected, value) = calls.next().expect("extra birth draw");
                        assert_eq!(bound, expected, "initial {initial}, final {final_kind}");
                        let _: u32 = rng.random_range(0..bound);
                        value
                    },
                )
                .unwrap();
            assert_eq!(calls.len(), 0);
            assert_eq!(rng.next_u64(), expected_rng.next_u64());
            assert_eq!(tree.corpse_timer_operations, expected_ops);
            assert_eq!(tree.corpse_timeout, final_recipe);
            assert_eq!(tree.item.enchantment, 2); // actual actor overrides initial female
            assert_eq!(
                tree.item.corpse_species,
                Some(CorpseSpecies::Custom("final specimen".into()))
            );
            assert_eq!(
                tree.item.saved_monster_traits.is_some(),
                final_kind == 1 || final_kind == 3
            );
        }
    }
}

#[test]
fn corpse_gender_has_distinct_neuter_first_precedence_and_exact_variable_draw() {
    for (flags, gender, draw) in [
        (vec!["neuter", "female", "male"], 3, None),
        (vec!["female", "male"], 1, None),
        (vec!["male"], 2, None),
        (vec![], 2, Some(0)),
        (vec![], 1, Some(1)),
    ] {
        let mut rules = rules(2, 0); // Never: no timeout draws, but gender still initializes
        rules
            .monsters
            .iter_mut()
            .find(|d| d.id == Some(M::JACKAL))
            .unwrap()
            .flags = flags.into_iter().map(String::from).collect();
        rules.reindex();
        let mut trace = vec![(1, 0)];
        if let Some(draw) = draw {
            trace.push((2, draw));
        }
        let mut calls = trace.iter();
        let (ctx, monsters) = context();
        let tree = rules
            .create_random_item_tree_with_monsters(
                "corpse",
                ItemLocation::Limbo,
                ctx,
                &monsters,
                |_| false,
                |bound| {
                    let &(expected, value) = calls.next().expect("extra gender draw");
                    assert_eq!(bound, expected);
                    value
                },
            )
            .unwrap();
        assert_eq!(calls.len(), 0);
        assert_eq!(tree.item.enchantment, gender);
        assert_eq!(tree.corpse_timeout, None);
        assert!(tree.corpse_timer_operations.is_empty());
    }
}

#[test]
fn supported_keeptraits_uses_owned_species_facts_and_actual_tameness() {
    for predicate in 0..6 {
        let mut rules = rules(2, 0);
        let def = rules
            .monsters
            .iter_mut()
            .find(|d| d.name == "final specimen")
            .unwrap();
        def.is_unique = predicate == 1;
        if predicate == 2 {
            def.attacks[0].ad = DamageType::Seduce;
        }
        if predicate == 3 {
            def.attacks[0].ad = DamageType::SeduceSex;
        }
        if predicate == 4 {
            def.glyph = 'T';
            def.corpse_timeout_policy = Policy::Never;
        }
        let mut actor = def.create_record_with_hp(Coord::new_unchecked(8, 8), 0, 3, 7);
        actor.is_tame = predicate == 0;
        actor.is_unique = true; // does not stand in for the source species G_UNIQ
        actor.is_cancelled = true;
        actor.name = "polymorphed quest leader".into(); // no static-name approximation
        let (ctx, monsters) = context();
        let mut trace = vec![(1, 0), (2, 0)];
        if predicate != 4 {
            trace.extend([(1000, 0), (4, 1), (2, 1)]);
        }
        let mut calls = trace.iter();
        let tree = rules
            .create_initialized_corpse_for_actor(
                &actor,
                ItemLocation::Limbo,
                ctx,
                &monsters,
                |_| false,
                |bound| {
                    let &(expected, value) = calls.next().unwrap();
                    assert_eq!(bound, expected);
                    value
                },
            )
            .unwrap();
        assert_eq!(calls.len(), 0);
        assert_eq!(tree.item.saved_monster_traits.is_some(), predicate != 5);
        assert_eq!(tree.item.corpse_no_revive, predicate != 5);
        if let Some(traits) = tree.item.saved_monster_traits {
            let mut expected = actor.clone();
            expected.max_hp = 8;
            assert_eq!(traits.actor, expected);
        }
    }
}

#[test]
fn unknown_sex_zero_flags_and_unresolved_identity_are_explicit_compatibility() {
    let rules = rules(2, 0);
    let mut actor = rules
        .monster("final specimen")
        .unwrap()
        .create_record_with_hp(Coord::new_unchecked(8, 8), 0, 3, 7);
    actor.sex = None;
    let (ctx, monsters) = context();
    let trace = [(1, 0), (2, 1), (1000, 0), (4, 1), (2, 1)];
    let mut calls = trace.iter();
    let tree = rules
        .create_initialized_corpse_for_actor(
            &actor,
            ItemLocation::Limbo,
            ctx,
            &monsters,
            |_| false,
            |bound| {
                let &(expected, value) = calls.next().unwrap();
                assert_eq!(bound, expected);
                value
            },
        )
        .unwrap();
    assert_eq!(calls.len(), 0);
    assert_eq!(tree.item.enchantment, 0);
    assert_eq!(actor.sex, None);
    actor.species = None;
    actor.name = "unresolved".into();
    let tree = rules
        .create_initialized_corpse_for_actor(
            &actor,
            ItemLocation::Limbo,
            ctx,
            &monsters,
            |_| false,
            |_| panic!("unresolved compatibility draw"),
        )
        .unwrap();
    assert_eq!(tree.item.corpse_species, Some(CorpseSpecies::Unspecified));
    assert!(tree.corpse_timer_operations.is_empty());
    assert!(tree.corpse_timeout.is_none());
    assert!(rules
        .create_item_record_by_id(
            K::CORPSE,
            ItemLocation::Limbo,
            nethacked_types::Buc::Uncursed
        )
        .unwrap()
        .saved_monster_traits
        .is_none());
    assert_eq!(actor.sex, None);
    // Pure factories remain RNG-free and retain unspecified zero gender.
    assert_eq!(
        rules
            .create_corpse_record_for_actor(&actor, ItemLocation::Limbo)
            .unwrap()
            .enchantment,
        0
    );
    assert_ne!(Some(ActorSex::Male), actor.sex);
}
