use nethacked_arena::{EntityArena, ItemLocation};
use nethacked_data::{ItemKindId as K, Ruleset};
use nethacked_types::Buc;
use std::collections::BTreeSet;

fn item(rules: &Ruleset, kind: K) -> nethacked_arena::ItemRecord {
    rules
        .create_item_record_by_id(kind, ItemLocation::Limbo, Buc::Uncursed)
        .unwrap()
}

#[test]
fn existing_object_candidates_follow_c_order_and_exhaust_without_draw() {
    let rules = Ruleset::vanilla();
    // Independent artilist.h order, excluding nogen Excalibur.
    let expected = [
        "ART_FROST_BRAND",
        "ART_FIRE_BRAND",
        "ART_GIANTSLAYER",
        "ART_VORPAL_BLADE",
        "ART_SUNSWORD",
    ];
    let mut history = BTreeSet::new();
    for (index, id) in expected.iter().enumerate() {
        let mut sword = item(&rules, K::LONG_SWORD);
        let mut calls = 0;
        rules.make_existing_artifact(&mut sword, K::LONG_SWORD, "Knight", &mut history, |n| {
            calls += 1;
            assert_eq!(n, (5 - index) as u32);
            0
        });
        assert_eq!(calls, 1);
        assert_eq!(sword.artifact_id.as_deref(), Some(*id));
    }
    let mut sword = item(&rules, K::LONG_SWORD);
    rules.make_existing_artifact(&mut sword, K::LONG_SWORD, "Knight", &mut history, |_| {
        panic!("exhausted pool")
    });
    assert_eq!(sword.artifact_id, None);
    assert!(!history.contains("ART_EXCALIBUR"));
    let mut arrow = item(&rules, K::ARROW);
    rules.make_existing_artifact(&mut arrow, K::ARROW, "Knight", &mut history, |_| {
        panic!("no matching base")
    });
}

#[test]
fn probability_counts_destroyed_artifacts_and_armor_uses_its_own_bound() {
    let rules = Ruleset::vanilla();
    let mut arena = EntityArena::new();
    for kind in [K::ART_EXCALIBUR, K::ART_ORB_OF_FATE] {
        let id = arena.spawn_item(item(&rules, kind));
        arena.destroy_item(id).unwrap();
    }
    assert_eq!(arena.artifact_history.len(), 2);
    for (kind, bound) in [(K::LONG_SWORD, 40), (K::LEATHER_ARMOR, 60)] {
        for roll in [1, bound - 1] {
            let mut obj = item(&rules, kind);
            let mut calls = 0;
            rules.random_existing_artifact(
                &mut obj,
                kind,
                "Wizard",
                &mut arena.artifact_history,
                |n| {
                    calls += 1;
                    assert_eq!(n, bound);
                    roll
                },
            );
            assert_eq!(calls, 1);
            assert_eq!(obj.artifact_id, None);
        }
    }
    let mut dagger = item(&rules, K::ORCISH_DAGGER);
    let mut trace = [(40, 0), (1, 0)].into_iter();
    rules.random_existing_artifact(
        &mut dagger,
        K::ORCISH_DAGGER,
        "Wizard",
        &mut arena.artifact_history,
        |n| {
            let (bound, value) = trace.next().expect("extra draw");
            assert_eq!(n, bound);
            value
        },
    );
    assert!(trace.next().is_none());
    assert_eq!(dagger.artifact_id.as_deref(), Some("ART_GRIMTOOTH"));
    assert!(dagger.poisoned);
}

#[test]
fn unique_nogen_role_value_and_spe_boundaries_use_owned_metadata() {
    let mut rules = (*Ruleset::vanilla()).clone();
    let a = rules
        .artifacts
        .iter_mut()
        .find(|a| a.id == "ART_STING")
        .unwrap();
    a.gift_value = 100;
    a.role = Some("ARCHEOLOGIST".into());
    let mut dagger = item(&rules, K::ELVEN_DAGGER);
    rules.make_existing_artifact(
        &mut dagger,
        K::ELVEN_DAGGER,
        "Wizard",
        &mut BTreeSet::new(),
        |_| panic!("value excluded"),
    );
    assert!(dagger.artifact_id.is_none());
    for (before, after) in [(-14, -14), (-13, -10), (6, 9), (7, 7), (127, 127)] {
        let mut dagger = item(&rules, K::ELVEN_DAGGER);
        dagger.enchantment = before;
        dagger.erosion = 3;
        rules.make_existing_artifact(
            &mut dagger,
            K::ELVEN_DAGGER,
            "Archaeologist",
            &mut BTreeSet::new(),
            |n| {
                assert_eq!(n, 1);
                0
            },
        );
        assert_eq!(dagger.enchantment, after);
        assert_eq!(dagger.erosion, 0);
    }
    rules
        .items
        .iter_mut()
        .find(|d| d.id == Some(K::ELVEN_DAGGER))
        .unwrap()
        .unique = true;
    rules.make_existing_artifact(
        &mut dagger,
        K::ELVEN_DAGGER,
        "Archaeologist",
        &mut BTreeSet::new(),
        |_| panic!("unique excluded"),
    );
    rules
        .items
        .iter_mut()
        .find(|d| d.id == Some(K::ELVEN_DAGGER))
        .unwrap()
        .unique = false;
    rules
        .artifacts
        .iter_mut()
        .find(|a| a.id == "ART_STING")
        .unwrap()
        .nogen = true;
    rules.make_existing_artifact(
        &mut dagger,
        K::ELVEN_DAGGER,
        "Archaeologist",
        &mut BTreeSet::new(),
        |_| panic!("nogen excluded"),
    );
}

#[test]
fn renamed_owned_definitions_keep_base_overrides_and_stable_factory_identity() {
    let mut rules = (*Ruleset::vanilla()).clone();
    let base = rules
        .items
        .iter_mut()
        .find(|d| d.id == Some(K::ORCISH_DAGGER))
        .unwrap();
    base.name = "custom base".into();
    base.weight = 91;
    rules
        .items
        .iter_mut()
        .find(|d| d.id == Some(K::ART_GRIMTOOTH))
        .unwrap()
        .name = "custom artifact".into();
    rules.reindex();
    let explicit = item(&rules, K::ART_GRIMTOOTH);
    assert_eq!(explicit.artifact_id.as_deref(), Some("ART_GRIMTOOTH"));
    let mut obj = item(&rules, K::ORCISH_DAGGER);
    obj.enchantment = -4;
    obj.buc = Buc::Cursed;
    rules.make_existing_artifact(
        &mut obj,
        K::ORCISH_DAGGER,
        "Wizard",
        &mut BTreeSet::new(),
        |n| {
            assert_eq!(n, 1);
            0
        },
    );
    assert_eq!(obj.name, "custom artifact");
    assert_eq!(obj.weight, 91);
    assert_eq!(obj.enchantment, -4);
    assert_eq!(obj.buc, Buc::Cursed);
    assert!(obj.poisoned);
    let static_obj =
        nethacked_data::create_item_record(K::ART_GRIMTOOTH, ItemLocation::Limbo, Buc::Uncursed);
    assert_eq!(static_obj.artifact_id, obj.artifact_id);
}

#[test]
fn old_records_and_ruleset_fields_default_without_altering_old_json() {
    let rules = Ruleset::vanilla();
    let mut value = serde_json::to_value(item(&rules, K::LONG_SWORD)).unwrap();
    value.as_object_mut().unwrap().remove("artifact_id");
    let old: nethacked_arena::ItemRecord = serde_json::from_value(value.clone()).unwrap();
    assert!(old.artifact_id.is_none());
    assert_eq!(serde_json::to_value(old).unwrap(), value);
    let mut value = serde_json::to_value(&*rules).unwrap();
    value.as_object_mut().unwrap().remove("artifacts");
    for def in value["items"].as_array_mut().unwrap() {
        def.as_object_mut().unwrap().remove("unique");
    }
    let old: Ruleset = serde_json::from_value(value.clone()).unwrap();
    assert!(old.artifacts.is_empty());
    assert_eq!(serde_json::to_value(old).unwrap(), value);
}

#[test]
fn recursive_container_generation_and_explicit_inventory_disable_artifact_draws() {
    let mut rules = (*Ruleset::vanilla()).clone();
    // Box never selects weapons/armor in C. Ring child proves the disabled
    // hook preserves its complete independent initialization stream.
    for d in &mut rules.items {
        if d.class == nethacked_types::ItemClass::Ring {
            d.generation_weight = u32::from(d.id == Some(K::RIN_ADORNMENT));
        }
    }
    let mut trace = [(2, 1), (100, 94), (1, 0), (3, 1), (10, 0), (4, 3), (3, 0)].into_iter();
    let tree = rules
        .create_random_item_tree(
            &rules.item_by_id(K::SACK).unwrap().name,
            ItemLocation::Limbo,
            nethacked_data::containers::ContainerContext {
                hero_level: 1,
                level_difficulty: 1,
                moves: 1,
                in_mklev: true,
            },
            |n| {
                let (bound, roll) = trace.next().expect("unexpected artifact draw in child");
                assert_eq!(n, bound);
                roll
            },
        )
        .unwrap();
    assert!(trace.next().is_none());
    assert_eq!(tree.contents.len(), 1);
    assert!(tree.item.artifact_id.is_none());
    assert!(tree.contents[0].item.artifact_id.is_none());
    assert_eq!(tree.contents[0].item.enchantment, 3);
    // The supported-fields API is an explicit artifact-disabled context.
    let mut trace = [(11, 1), (10, 1), (10, 1)].into_iter();
    rules
        .create_random_item_supported_fields(K::LONG_SWORD, ItemLocation::Limbo, 1, |n| {
            let (bound, roll) = trace.next().expect("unexpected artifact draw");
            assert_eq!(n, bound);
            roll
        })
        .unwrap();
    assert!(trace.next().is_none());
    let mut world_rng_calls = 0;
    let _explicit = item(&rules, K::ART_EXCALIBUR);
    // No callback is involved in factories, including starting inventory.
    let mut arena = EntityArena::new();
    rules.spawn_player_character(
        &nethacked_data::CharacterConfig::default(),
        nethacked_types::Coord::new(3, 4).unwrap(),
        &mut arena,
    );
    let mut obj = item(&rules, K::SACK);
    rules.random_existing_artifact(&mut obj, K::SACK, "Wizard", &mut BTreeSet::new(), |_| {
        world_rng_calls += 1;
        0
    });
    assert_eq!(world_rng_calls, 0);
}
