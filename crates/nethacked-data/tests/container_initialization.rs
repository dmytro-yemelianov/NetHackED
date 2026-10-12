use nethacked_arena::{EntityArena, ItemLocation};
use nethacked_data::containers::{ContainerContext, GeneratedItem};
use nethacked_data::item_init::{ContainerInit, ItemInitProfile};
use nethacked_data::{ItemKindId as K, Ruleset};
use nethacked_types::{Buc, ItemClass};

fn ctx() -> ContainerContext {
    ContainerContext {
        hero_level: 1,
        level_difficulty: 4,
        moves: 1,
        in_mklev: true,
    }
}
fn scripted(
    rules: &Ruleset,
    kind: K,
    context: ContainerContext,
    draws: &[(u32, u32)],
) -> GeneratedItem {
    let mut draws = draws.iter();
    let result = rules
        .create_random_item_tree(
            &rules.item_by_id(kind).unwrap().name,
            ItemLocation::Limbo,
            context,
            |n| {
                let &(bound, value) = draws.next().expect("unexpected container draw");
                assert_eq!(n, bound);
                assert!(value < n);
                value
            },
        )
        .unwrap();
    assert_eq!(draws.len(), 0);
    result
}
fn only_weight(rules: &mut Ruleset, class: ItemClass, selected: K, weight: u32) {
    for item in &mut rules.items {
        if item.class == class {
            item.generation_weight = u32::from(item.id == Some(selected)) * weight;
        }
    }
}
#[test]
fn count_caps_trap_short_circuit_and_initial_sack_rn2_one() {
    let rules = Ruleset::vanilla();
    for (kind, locked, cap) in [
        (K::CHEST, 0, 6),
        (K::CHEST, 1, 8),
        (K::LARGE_BOX, 0, 4),
        (K::LARGE_BOX, 1, 6),
    ] {
        let result = scripted(&rules, kind, ctx(), &[(5, locked), (10, 1), (cap, 0)]);
        assert_eq!(result.item.locked, locked != 0);
        assert!(!result.item.trapped && !result.item.trap_known);
        assert!(result.contents.is_empty());
        let result = scripted(
            &rules,
            kind,
            ctx(),
            &[(5, locked), (10, 0), (100, 0), (cap, 0)],
        );
        assert!(result.item.trapped && result.item.trap_known);
        let result = scripted(
            &rules,
            kind,
            ctx(),
            &[(5, locked), (10, 0), (100, 1), (cap, 0)],
        );
        assert!(result.item.trapped && !result.item.trap_known);
    }
    for kind in [K::SACK, K::OILSKIN_SACK] {
        scripted(
            &rules,
            kind,
            ContainerContext {
                in_mklev: false,
                ..ctx()
            },
            &[(1, 0)],
        );
        scripted(
            &rules,
            kind,
            ContainerContext {
                in_mklev: false,
                moves: 2,
                ..ctx()
            },
            &[(2, 0)],
        );
        scripted(&rules, kind, ctx(), &[(2, 0)]);
    }
    scripted(
        &rules,
        K::BAG_OF_HOLDING,
        ContainerContext {
            in_mklev: false,
            ..ctx()
        },
        &[(2, 0)],
    );
}
#[test]
fn coin_contents_have_c_quantity_weight_and_attachment() {
    let rules = Ruleset::vanilla();
    let result = scripted(
        &rules,
        K::SACK,
        ctx(),
        &[(2, 1), (100, 81), (1000, 0), (6, 5), (75, 74)],
    );
    let coin = &result.contents[0].item;
    assert_eq!((coin.quantity, coin.weight), (450, 5));
    let mut arena = EntityArena::new();
    let root = result.attach(&mut arena, ItemLocation::Limbo);
    let child = arena.items_in_container(root)[0];
    assert_eq!(arena.items[child].location, ItemLocation::InContainer(root));
    assert_eq!(
        arena.calculate_total_weight(root),
        rules.item_by_id(K::SACK).unwrap().weight + 5
    );
    for (buc, expected) in [(Buc::Blessed, 2), (Buc::Uncursed, 3), (Buc::Cursed, 10)] {
        arena.items[root].is_bag_of_holding = true;
        arena.items[root].buc = buc;
        assert_eq!(
            arena.calculate_total_weight(root),
            rules.item_by_id(K::SACK).unwrap().weight + expected
        );
    }
    let result = scripted(
        &rules,
        K::SACK,
        ctx(),
        &[(2, 1), (100, 81), (1000, 0), (6, 0), (75, 0)],
    );
    assert_eq!(
        (
            result.contents[0].item.quantity,
            result.contents[0].item.weight
        ),
        (1, 1)
    );
}
#[test]
fn rock_replacement_uses_owned_c_range_and_preserves_initialized_state() {
    let mut rules = (*Ruleset::vanilla()).clone();
    only_weight(&mut rules, ItemClass::Gem, K::ROCK, 10);
    rules
        .items
        .iter_mut()
        .find(|d| d.id == Some(K::LOADSTONE))
        .unwrap()
        .generation_weight = 3;
    // Gem class total=13, rock occupies final 10; range total=3.
    let result = scripted(
        &rules,
        K::SACK,
        ctx(),
        &[(2, 1), (100, 0), (13, 12), (6, 5), (3, 0)],
    );
    let child = &result.contents[0].item;
    assert_eq!(child.name, rules.item_by_id(K::LOADSTONE).unwrap().name);
    assert_eq!((child.quantity, child.buc), (1, Buc::Uncursed));
    assert_eq!(child.weight, rules.item_by_id(K::LOADSTONE).unwrap().weight);
}
#[test]
fn holding_bag_sanitizes_cancellation_without_reinitializing_charges() {
    let mut rules = (*Ruleset::vanilla()).clone();
    only_weight(&mut rules, ItemClass::Wand, K::WAN_CANCELLATION, 7);
    let light = rules
        .items
        .iter_mut()
        .find(|d| d.id == Some(K::WAN_LIGHT))
        .unwrap();
    light.generation_weight = 2;
    light.name = "renamed light".into();
    light.weight = 99;
    rules.reindex();
    let result = scripted(
        &rules,
        K::BAG_OF_HOLDING,
        ctx(),
        &[
            (2, 1),
            (100, 88),
            (9, 8),
            (5, 4),
            (17, 0),
            (2, 1),
            (9, 8),
            (9, 0),
        ],
    );
    let child = &result.contents[0].item;
    assert_eq!(
        (&*child.name, child.enchantment, child.buc, child.weight),
        ("renamed light", 8, Buc::Blessed, 99)
    );
}
#[test]
fn holding_bag_sanitizes_magical_bag_after_child_initialization() {
    // C's box table has no tools; a coherent owned Wand-class/profile override
    // exercises this defensive branch. The scripted charge draw initializes
    // spe=8 before conversion to a sack must reset it to zero.
    let mut rules = (*Ruleset::vanilla()).clone();
    only_weight(&mut rules, ItemClass::Wand, K::BAG_OF_TRICKS, 0);
    let bag = rules
        .items
        .iter_mut()
        .find(|d| d.id == Some(K::BAG_OF_TRICKS))
        .unwrap();
    bag.class = ItemClass::Wand;
    bag.generation_weight = 5;
    bag.initialization = ItemInitProfile::Wand {
        charges: nethacked_data::item_init::WandCharges::Standard,
    };
    let result = scripted(
        &rules,
        K::BAG_OF_HOLDING,
        ctx(),
        &[(2, 1), (100, 88), (5, 0), (5, 4), (17, 1)],
    );
    assert_eq!(
        result.contents[0].item.name,
        rules.item_by_id(K::SACK).unwrap().name
    );
    assert_eq!(result.contents[0].item.enchantment, 0);
}
#[test]
fn owned_custom_selection_fixed_profile_and_zero_range_fallback() {
    let mut rules = (*Ruleset::vanilla()).clone();
    let mut custom = rules.item_by_id(K::DIAMOND).unwrap().clone();
    custom.id = None;
    custom.name = "custom gem".into();
    custom.generation_weight = 11;
    custom.initialization = ItemInitProfile::Fixed;
    only_weight(&mut rules, ItemClass::Gem, K::ROCK, 0);
    rules.items.push(custom);
    rules.reindex();
    let result = scripted(&rules, K::SACK, ctx(), &[(2, 1), (100, 0), (11, 0)]);
    assert_eq!(result.contents[0].item.name, "custom gem");
    let mut seen = Vec::new();
    let first = rules
        .ranged_item_definition(K::DILITHIUM_CRYSTAL, K::LOADSTONE, |n| {
            seen.push(n);
            0
        })
        .unwrap();
    assert_eq!(first.id, Some(K::DILITHIUM_CRYSTAL));
    assert_eq!(
        seen,
        vec![(K::LOADSTONE.index() - K::DILITHIUM_CRYSTAL.index() + 1) as u32]
    );
    rules
        .ranged_item_definition(K::DIAMOND, K::DIAMOND, |_| {
            panic!("singleton draws nothing")
        })
        .unwrap();
    for def in &mut rules.items {
        def.generation_weight = u32::MAX;
    }
    assert!(rules.box_item_definition(|_| 0).is_none());
}
#[test]
fn owned_box_selection_matches_c_catalog_weight_order() {
    for seed in 0..300u32 {
        let rules = Ruleset::vanilla();
        let mut state = seed;
        let mut rolls = Vec::new();
        let expected = nethacked_data::generation::mkobj_kind(
            None,
            nethacked_data::generation::ObjectTable::Box,
            |n| {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                let value = state % n;
                rolls.push((n, value));
                value
            },
        )
        .unwrap();
        let mut cursor = rolls.iter();
        let actual = rules
            .box_item_definition(|n| {
                let &(bound, value) = cursor.next().unwrap();
                assert_eq!(n, bound);
                value
            })
            .unwrap();
        assert_eq!(actual.id, Some(expected));
        assert_eq!(cursor.len(), 0);
    }
    assert_eq!(
        Ruleset::vanilla()
            .item_by_id(K::CHEST)
            .unwrap()
            .initialization,
        ItemInitProfile::Container {
            container: ContainerInit::Chest
        }
    );
}

#[test]
fn full_chest_count_and_nested_attachment_weights() {
    let rules = Ruleset::vanilla();
    let mut draws = vec![(5, 1), (10, 1), (8, 7)];
    for _ in 0..7 {
        draws.extend([(100, 81), (1000, 0), (6, 0), (75, 0)]);
    }
    let result = scripted(&rules, K::CHEST, ctx(), &draws);
    assert_eq!(result.contents.len(), 7);
    let mut nested = scripted(&rules, K::BAG_OF_HOLDING, ctx(), &[(2, 0)]);
    nested.item.buc = Buc::Blessed;
    nested.contents.push(result);
    let mut arena = EntityArena::new();
    let root = nested.attach(&mut arena, ItemLocation::Limbo);
    let chest = arena.items_in_container(root)[0];
    assert_eq!(arena.items_in_container(chest).len(), 7);
    assert!(arena
        .items_in_container(chest)
        .iter()
        .all(|&id| arena.items[id].location == ItemLocation::InContainer(chest)));
    let chest_weight = rules.item_by_id(K::CHEST).unwrap().weight + 7;
    assert_eq!(arena.calculate_total_weight(chest), chest_weight);
    assert_eq!(
        arena.calculate_total_weight(root),
        rules.item_by_id(K::BAG_OF_HOLDING).unwrap().weight + chest_weight.div_ceil(4)
    );
}

#[test]
fn pathological_cancellation_weights_have_bounded_safe_fallback() {
    let mut rules = (*Ruleset::vanilla()).clone();
    only_weight(&mut rules, ItemClass::Wand, K::WAN_CANCELLATION, 1);
    let mut draws = vec![(2, 1), (100, 88), (1, 0), (5, 0), (17, 1)];
    draws.extend(std::iter::repeat_n((1, 0), 64));
    let result = scripted(&rules, K::BAG_OF_HOLDING, ctx(), &draws);
    assert_eq!(
        result.contents[0].item.name,
        rules.item_by_id(K::WAN_LIGHT).unwrap().name
    );
    assert_eq!(result.contents[0].item.enchantment, 4);
    rules
        .items
        .retain(|d| d.class != ItemClass::Wand || d.id == Some(K::WAN_CANCELLATION));
    rules.reindex();
    assert!(scripted(&rules, K::BAG_OF_HOLDING, ctx(), &draws)
        .contents
        .is_empty());
}

#[test]
fn represented_gem_stack_quantity_matches_c_branch_endpoints() {
    let rules = Ruleset::vanilla();
    for roll in 0..6 {
        for kind in [K::ROCK, K::DIAMOND] {
            let tree = scripted(&rules, kind, ctx(), &[(6, roll)]);
            let expected = if kind == K::ROCK {
                roll + 6
            } else if roll == 0 {
                2
            } else {
                1
            };
            assert_eq!(tree.item.quantity, expected);
            assert_eq!(
                tree.item.weight,
                rules.item_by_id(kind).unwrap().weight * expected
            );
        }
    }
    let luck = scripted(&rules, K::LUCKSTONE, ctx(), &[]);
    assert_eq!(luck.item.quantity, 1);
    let load = scripted(&rules, K::LOADSTONE, ctx(), &[]);
    assert_eq!((load.item.quantity, load.item.buc), (1, Buc::Cursed));
}
