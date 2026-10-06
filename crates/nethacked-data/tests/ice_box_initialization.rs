use nethacked_arena::{CorpseSpecies, EntityArena, ItemLocation};
use nethacked_data::containers::ContainerContext;
use nethacked_data::corpses::undead_to_corpse;
use nethacked_data::generation::{DungeonAlign, MonsterGenContext};
use nethacked_data::{ItemKindId as K, MonsterSpeciesId as M, Ruleset};
use nethacked_types::Buc;

fn context() -> MonsterGenContext {
    MonsterGenContext {
        hero_level: 10,
        level_difficulty: 10,
        in_hell: false,
        align: DungeonAlign::None,
        temperature: 0,
    }
}
fn only_species(id: M) -> Ruleset {
    let mut rules = (*Ruleset::vanilla()).clone();
    for def in &mut rules.monsters {
        def.frequency = u32::from(def.id == Some(id));
        if def.id == Some(id) {
            def.difficulty = 5;
            def.gen_flags
                .retain(|f| f != "nogen" && f != "uniq" && f != "hell" && f != "nohell");
        }
    }
    rules
}

#[test]
fn ice_box_count_and_direct_corpse_draws_owned_weight_and_zero_age() {
    let mut rules = only_species(M::JACKAL);
    let index = rules
        .monsters
        .iter()
        .position(|d| d.id == Some(M::JACKAL))
        .unwrap();
    rules.monsters[index].name = "renamed canid".into();
    rules.monsters[index].body_weight = 321;
    rules.reindex();
    for count in [0, 1, 20] {
        let mut draws = vec![(21, count)];
        draws.extend((0..count).map(|_| (1, 0)));
        let mut expected = draws.iter();
        let tree = rules
            .create_random_item_tree_with_monsters(
                &rules.item_by_id(K::ICE_BOX).unwrap().name,
                ItemLocation::Limbo,
                ContainerContext {
                    hero_level: 10,
                    level_difficulty: 10,
                    moves: 200,
                    in_mklev: true,
                },
                &context(),
                |_| false,
                |n| {
                    let &(bound, value) = expected.next().expect("extra draw");
                    assert_eq!(n, bound);
                    value
                },
            )
            .unwrap();
        assert_eq!(expected.len(), 0);
        assert_eq!(tree.contents.len(), count as usize);
        assert!(tree.item.is_ice_box && tree.item.is_container);
        assert!(!tree.item.locked && !tree.item.trapped && !tree.item.trap_known);
        assert_eq!(tree.item.buc, Buc::Uncursed);
        for child in &tree.contents {
            assert!(child.contents.is_empty());
            assert_eq!(
                child.item.corpse_species,
                Some(CorpseSpecies::Vanilla("JACKAL".into()))
            );
            assert_eq!(child.item.corpse_race, None);
            assert_eq!(
                (
                    child.item.corpse_age,
                    child.item.weight,
                    child.item.quantity
                ),
                (0, 321, 1)
            );
        }
        let mut arena = EntityArena::default();
        let id = tree.attach(&mut arena, ItemLocation::Limbo);
        assert_eq!(
            arena.calculate_total_weight(id),
            rules.item_by_id(K::ICE_BOX).unwrap().weight + count * 321
        );
        arena.advance_corpse_ages();
        assert!(arena
            .items_in_container(id)
            .iter()
            .all(|&child| arena.items[child].corpse_age == 0));
    }
}

#[test]
fn no_corpse_selection_retries_exactly_fifty_then_human() {
    let mut rules = only_species(M::JACKAL);
    rules
        .monsters
        .iter_mut()
        .find(|d| d.id == Some(M::JACKAL))
        .unwrap()
        .gen_flags
        .push("nocorpse".into());
    let mut bounds = Vec::new();
    let selected = rules
        .random_corpse_definition(
            &context(),
            |_| false,
            |n| {
                bounds.push(n);
                0
            },
        )
        .unwrap();
    assert_eq!(selected.id, Some(M::HUMAN));
    assert_eq!(bounds, vec![1; 50]);
}

#[test]
fn undead_conversion_uses_c_ids_before_no_corpse_test() {
    for (source, target) in [
        (M::KOBOLD_ZOMBIE, M::KOBOLD),
        (M::KOBOLD_MUMMY, M::KOBOLD),
        (M::DWARF_ZOMBIE, M::DWARF),
        (M::DWARF_MUMMY, M::DWARF),
        (M::GNOME_ZOMBIE, M::GNOME),
        (M::GNOME_MUMMY, M::GNOME),
        (M::ORC_ZOMBIE, M::ORC),
        (M::ORC_MUMMY, M::ORC),
        (M::ELF_ZOMBIE, M::ELF),
        (M::ELF_MUMMY, M::ELF),
        (M::VAMPIRE, M::HUMAN),
        (M::VAMPIRE_LEADER, M::HUMAN),
        (M::HUMAN_ZOMBIE, M::HUMAN),
        (M::HUMAN_MUMMY, M::HUMAN),
        (M::GIANT_ZOMBIE, M::GIANT),
        (M::GIANT_MUMMY, M::GIANT),
        (M::ETTIN_ZOMBIE, M::ETTIN),
        (M::ETTIN_MUMMY, M::ETTIN),
    ] {
        assert_eq!(undead_to_corpse(source), target);
        let rules = only_species(source);
        let mut bounds = Vec::new();
        let selected = rules
            .random_corpse_definition(
                &context(),
                |_| false,
                |n| {
                    bounds.push(n);
                    0
                },
            )
            .unwrap();
        assert_eq!(selected.id, Some(target));
        assert_eq!(bounds, vec![1]);
    }
    assert_eq!(undead_to_corpse(M::LICH), M::LICH);
}

#[test]
fn rndmonnum_fallback_uniform_range_and_actual_registry_short_circuit() {
    let rules = only_species(M::JACKAL);
    let mut bounds = Vec::new();
    // Genocided species is excluded by rndmonst, but C Plan B ignores G_GONE.
    let selected = rules
        .random_corpse_definition(
            &context(),
            |_| true,
            |n| {
                bounds.push(n);
                M::JACKAL.index() as u32
            },
        )
        .unwrap();
    assert_eq!(selected.id, Some(M::JACKAL));
    assert_eq!(bounds, vec![M::LONG_WORM_TAIL.index() as u32]);
    let mut bounds = Vec::new();
    // Reject the canonical uniquely generated Wizard, then accept Jackal.
    let selected = rules
        .random_corpse_definition(
            &context(),
            |_| true,
            |n| {
                bounds.push(n);
                if bounds.len() == 1 {
                    M::WIZARD_OF_YENDOR.index() as u32
                } else {
                    M::JACKAL.index() as u32
                }
            },
        )
        .unwrap();
    assert_eq!(selected.id, Some(M::JACKAL));
    assert_eq!(bounds, vec![M::LONG_WORM_TAIL.index() as u32; 2]);
}

#[test]
fn pathological_fallback_is_bounded_and_no_safe_species_uses_human() {
    let mut rules = only_species(M::JACKAL);
    for def in &mut rules.monsters {
        def.gen_flags.push("nogen".into());
    }
    let mut calls = 0;
    let selected = rules
        .random_corpse_definition(
            &context(),
            |_| false,
            |n| {
                assert_eq!(n, M::LONG_WORM_TAIL.index() as u32);
                calls += 1;
                0
            },
        )
        .unwrap();
    assert_eq!(selected.id, Some(M::HUMAN));
    assert_eq!(calls, 256);
    let rules = only_species(M::JACKAL);
    let mut calls = 0;
    let selected = rules
        .random_corpse_definition(
            &context(),
            |_| true,
            |_| {
                calls += 1;
                M::WIZARD_OF_YENDOR.index() as u32
            },
        )
        .unwrap();
    assert_eq!(calls, 256);
    assert_eq!(selected.id, Some(M::GIANT_ANT));
}

#[test]
fn custom_species_payload_and_body_facts_are_owned_and_old_definitions_default() {
    let mut rules = only_species(M::JACKAL);
    let mut def = rules.monster_by_id(M::JACKAL).unwrap().clone();
    def.id = None;
    def.name = "pack creature".into();
    def.body_weight = 73;
    def.corpse_nutrition = 81;
    rules.monsters.iter_mut().for_each(|d| d.frequency = 0);
    rules.monsters.push(def.clone());
    rules.reindex();
    let selected = rules
        .random_corpse_definition(
            &context(),
            |_| false,
            |n| {
                assert_eq!(n, 1);
                0
            },
        )
        .unwrap();
    assert_eq!(selected.name, "pack creature");
    let corpse = rules
        .create_corpse_record(&selected.name, ItemLocation::Limbo)
        .unwrap();
    assert_eq!(
        corpse.corpse_species,
        Some(CorpseSpecies::Custom("pack creature".into()))
    );
    assert_eq!(corpse.weight, 73);
    assert_eq!(selected.corpse_nutrition, 81);
    let mut value = serde_json::to_value(&def).unwrap();
    value.as_object_mut().unwrap().remove("body_weight");
    value.as_object_mut().unwrap().remove("corpse_nutrition");
    let loaded: nethacked_data::MonsterDef = serde_json::from_value(value).unwrap();
    assert_eq!((loaded.body_weight, loaded.corpse_nutrition), (0, 0));
    let human = rules.monster_by_id(M::HUMAN).unwrap();
    let corpse = rules
        .create_corpse_record(&human.name, ItemLocation::Limbo)
        .unwrap();
    assert_eq!(
        corpse.corpse_species,
        Some(CorpseSpecies::Vanilla("HUMAN".into()))
    );
    assert_eq!(corpse.corpse_race.as_deref(), Some("human"));
    assert_eq!(corpse.weight, human.body_weight);
}

#[test]
fn extracted_body_facts_match_c_and_fallback_obeys_hell_exclusion() {
    let vanilla = Ruleset::vanilla();
    let human = vanilla.monster_by_id(M::HUMAN).unwrap();
    assert_eq!((human.body_weight, human.corpse_nutrition), (1450, 400));
    let ant = vanilla.monster_by_id(M::GIANT_ANT).unwrap();
    assert_eq!((ant.body_weight, ant.corpse_nutrition), (10, 10));
    for in_hell in [false, true] {
        let mut rules = only_species(M::JACKAL);
        let def = rules
            .monsters
            .iter_mut()
            .find(|d| d.id == Some(M::JACKAL))
            .unwrap();
        def.gen_flags
            .push(if in_hell { "nohell" } else { "hell" }.into());
        let mut draws = vec![M::JACKAL.index() as u32, M::GIANT_ANT.index() as u32].into_iter();
        let selected = rules
            .random_corpse_definition(
                &MonsterGenContext {
                    in_hell,
                    ..context()
                },
                |_| true,
                |n| {
                    assert_eq!(n, M::LONG_WORM_TAIL.index() as u32);
                    draws.next().expect("unexpected fallback retry")
                },
            )
            .unwrap();
        assert_eq!(selected.id, Some(M::GIANT_ANT));
        assert_eq!(draws.len(), 0);
    }
}
