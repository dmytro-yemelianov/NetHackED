use nethacked_data::{generation::ObjectTable, ItemKindId as K, Ruleset};
use nethacked_types::ItemClass;

#[test]
fn vanilla_class_boundaries_and_kind_boundaries_match_c_facts() {
    // Independently specified C mkobjprobs boundaries; no production selector
    // supplies the expected class or draw bound.
    let rules = Ruleset::vanilla();
    for (first, last, class, total) in [
        (0, 9, ItemClass::Weapon, 1002),
        (10, 20, ItemClass::Armor, 1000),
        (21, 40, ItemClass::Food, 1000),
        (41, 48, ItemClass::Tool, 1000),
        (49, 55, ItemClass::Gem, 1000),
        (56, 71, ItemClass::Potion, 1000),
        (72, 87, ItemClass::Scroll, 1000),
        (88, 91, ItemClass::Spellbook, 1000),
        (92, 95, ItemClass::Wand, 1000),
        (96, 98, ItemClass::Ring, 28),
        (99, 99, ItemClass::Amulet, 1000),
    ] {
        for value in [first, last] {
            let mut draws = [(100, value), (total, 0)].into_iter();
            let selected = rules
                .room_item_definition(None, ObjectTable::Dungeon, |n| {
                    let (bound, value) = draws.next().unwrap();
                    assert_eq!(n, bound);
                    value
                })
                .unwrap();
            assert_eq!(selected.class, class);
            assert!(draws.next().is_none());
        }
    }
    // First C weapon entries: arrow 55, elven arrow 20, orcish arrow 20.
    for (value, expected) in [
        (0, K::ARROW),
        (54, K::ARROW),
        (55, K::ELVEN_ARROW),
        (74, K::ELVEN_ARROW),
        (75, K::ORCISH_ARROW),
    ] {
        assert_eq!(
            rules
                .room_item_definition(Some(ItemClass::Weapon), ObjectTable::Dungeon, |n| {
                    assert_eq!(n, 1002);
                    value
                })
                .unwrap()
                .id,
            Some(expected)
        );
    }
}

#[test]
fn owned_weights_names_canonical_ids_custom_order_zero_and_artifacts() {
    let mut rules = (*Ruleset::vanilla()).clone();
    for d in &mut rules.items {
        d.generation_weight = 0;
    }
    let arrow = rules
        .items
        .iter_mut()
        .find(|d| d.id == Some(K::ARROW))
        .unwrap();
    arrow.name = "renamed arrow".into();
    arrow.generation_weight = 2;
    let elven = rules
        .items
        .iter_mut()
        .find(|d| d.id == Some(K::ELVEN_ARROW))
        .unwrap();
    elven.generation_weight = 3;
    // Deliberately reverse built-ins; custom Vec order remains z, a.
    rules.items.reverse();
    for (name, weight) in [("z custom", 4), ("a custom", 5)] {
        let mut d = rules
            .items
            .iter()
            .find(|d| d.id == Some(K::ARROW))
            .unwrap()
            .clone();
        d.id = None;
        d.name = name.into();
        d.generation_weight = weight;
        rules.items.push(d);
    }
    let artifact = rules.artifacts[0].item;
    let d = rules
        .items
        .iter_mut()
        .find(|d| d.id == Some(artifact))
        .unwrap();
    d.class = ItemClass::Weapon;
    d.generation_weight = 10000;
    rules.reindex();
    for (value, name) in [
        (0, "renamed arrow"),
        (1, "renamed arrow"),
        (2, "elven arrow"),
        (4, "elven arrow"),
        (5, "z custom"),
        (8, "z custom"),
        (9, "a custom"),
        (13, "a custom"),
    ] {
        let selected = rules
            .room_item_definition(Some(ItemClass::Weapon), ObjectTable::Dungeon, |n| {
                assert_eq!(n, 14);
                value
            })
            .unwrap();
        assert_eq!(selected.name, name);
    }
}

#[test]
fn empty_zero_overflow_pools_do_not_draw_or_retry() {
    let mut rules = (*Ruleset::vanilla()).clone();
    for d in &mut rules.items {
        d.generation_weight = 0;
    }
    assert!(rules
        .room_item_definition(Some(ItemClass::Weapon), ObjectTable::Dungeon, |_| panic!(
            "zero pool kind draw"
        ))
        .is_none());
    let mut calls = 0;
    assert!(rules
        .room_item_definition(None, ObjectTable::Dungeon, |n| {
            calls += 1;
            assert_eq!(n, 100);
            0
        })
        .is_none());
    assert_eq!(calls, 1);
    rules
        .items
        .iter_mut()
        .find(|d| d.id == Some(K::ARROW))
        .unwrap()
        .generation_weight = u32::MAX;
    assert_eq!(
        rules
            .room_item_definition(Some(ItemClass::Weapon), ObjectTable::Dungeon, |n| {
                assert_eq!(n, u32::MAX);
                n - 1
            })
            .unwrap()
            .id,
        Some(K::ARROW)
    );
    for id in [K::ARROW, K::ELVEN_ARROW] {
        rules
            .items
            .iter_mut()
            .find(|d| d.id == Some(id))
            .unwrap()
            .generation_weight = u32::MAX;
    }
    assert!(rules
        .room_item_definition(Some(ItemClass::Weapon), ObjectTable::Dungeon, |_| panic!(
            "overflow pool kind draw"
        ))
        .is_none());
    rules.items.clear();
    assert!(rules
        .room_item_definition(Some(ItemClass::Weapon), ObjectTable::Dungeon, |_| panic!(
            "empty pool draw"
        ))
        .is_none());
}

#[test]
fn legacy_absence_and_explicit_zero_round_trip_distinctly() {
    let mut json = serde_json::to_value(&*Ruleset::vanilla()).unwrap();
    for d in json["items"].as_array_mut().unwrap() {
        d.as_object_mut().unwrap().remove("generation_weight");
        d.as_object_mut().unwrap().remove("initialization");
    }
    let mut legacy: Ruleset = serde_json::from_value(json.clone()).unwrap();
    legacy.reindex();
    assert_eq!(serde_json::to_value(&legacy).unwrap(), json);
    assert_eq!(
        legacy
            .room_item_definition(Some(ItemClass::Weapon), ObjectTable::Dungeon, |n| {
                assert_eq!(n, 1002);
                55
            })
            .unwrap()
            .id,
        Some(K::ELVEN_ARROW)
    );
    for d in json["items"].as_array_mut().unwrap() {
        d["generation_weight"] = 0.into();
    }
    let explicit: Ruleset = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(serde_json::to_value(&explicit).unwrap(), json);
    assert!(explicit
        .room_item_definition(Some(ItemClass::Weapon), ObjectTable::Dungeon, |_| panic!(
            "explicit zero fallback"
        ))
        .is_none());
    let mut custom = legacy.items[0].clone();
    custom.id = None;
    legacy.items = vec![custom];
    assert!(legacy
        .room_item_definition(
            Some(legacy.items[0].class),
            ObjectTable::Dungeon,
            |_| panic!("absent custom fallback")
        )
        .is_none());
}

#[test]
fn weight_provenance_is_not_a_serialized_field_and_unknown_fields_reject() {
    let mut json = serde_json::to_value(Ruleset::vanilla().items[0].clone()).unwrap();
    assert!(json.get("generation_weight_missing").is_none());
    json["generation_weight_missing"] = true.into();
    assert!(serde_json::from_value::<nethacked_data::ItemDef>(json).is_err());
}

#[cfg(feature = "schema")]
#[test]
fn item_schema_exposes_weight_without_provenance() {
    let schema = serde_json::to_value(schemars::schema_for!(nethacked_data::ItemDef)).unwrap();
    let text = serde_json::to_string(&schema).unwrap();
    assert!(text.contains("generation_weight"));
    assert!(!text.contains("generation_weight_missing"));
}

#[test]
fn alternate_class_tables_preserve_c_boundaries() {
    use ItemClass::*;
    let rules = Ruleset::vanilla();
    let fixtures: &[(ObjectTable, &[(u32, ItemClass)])] = &[
        (
            ObjectTable::Hell,
            &[
                (20, Weapon),
                (20, Armor),
                (16, Food),
                (12, Tool),
                (10, Gem),
                (1, Potion),
                (1, Scroll),
                (8, Wand),
                (8, Ring),
                (4, Amulet),
            ],
        ),
        (
            ObjectTable::Rogue,
            &[
                (12, Weapon),
                (12, Armor),
                (22, Food),
                (22, Potion),
                (22, Scroll),
                (5, Wand),
                (5, Ring),
            ],
        ),
        (
            ObjectTable::Box,
            &[
                (18, Gem),
                (15, Food),
                (18, Potion),
                (18, Scroll),
                (12, Spellbook),
                (7, Coin),
                (6, Wand),
                (5, Ring),
                (1, Amulet),
            ],
        ),
    ];
    for &(table, classes) in fixtures {
        let mut start = 0;
        for &(weight, class) in classes {
            for value in [start, start + weight - 1] {
                let total = match class {
                    Weapon => 1002,
                    Ring => 28,
                    _ => 1000,
                };
                let mut draws = [(100, value), (total, total - 1)].into_iter();
                let selected = rules
                    .room_item_definition(None, table, |n| {
                        let (bound, value) = draws.next().unwrap();
                        assert_eq!(n, bound);
                        value
                    })
                    .unwrap();
                assert_eq!(selected.class, class);
                assert!(draws.next().is_none());
            }
            start += weight;
        }
        assert_eq!(start, 100);
    }
}
