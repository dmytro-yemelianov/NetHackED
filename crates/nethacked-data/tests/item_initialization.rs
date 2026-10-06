use nethacked_arena::{EntityArena, ItemLocation};
use nethacked_data::item_init::{initialize_supported_fields, ItemInitProfile};
use nethacked_data::{ItemKindId as K, Ruleset, WandDir, ITEM_CATALOG};
use nethacked_types::Buc;

fn scripted(kind: K, draws: &[(u32, u32)]) -> nethacked_arena::ItemRecord {
    let rules = Ruleset::vanilla();
    let mut next = draws.iter();
    let item = rules
        .create_random_item_supported_fields(kind, ItemLocation::Limbo, 1, |n| {
            let &(bound, value) = next.next().expect("unexpected draw");
            assert_eq!(n, bound, "draw order for {kind:?}");
            assert!(value < n);
            value
        })
        .unwrap();
    assert_eq!(next.len(), 0, "missing draws for {kind:?}");
    item
}

#[test]
fn weapon_quantity_buc_poison_and_weight_order() {
    let item = scripted(K::ARROW, &[(6, 0), (11, 0), (3, 1), (2, 1), (100, 0)]);
    assert_eq!(
        (item.quantity, item.enchantment, item.buc, item.poisoned),
        (6, 1, Buc::Blessed, true)
    );
    let rules = Ruleset::vanilla();
    assert_eq!(item.weight, 6 * rules.item_by_id(K::ARROW).unwrap().weight);
    let mut arena = EntityArena::new();
    let id = arena.spawn_item(item);
    assert_eq!(arena.calculate_total_weight(id), 6);
    let item = scripted(K::LONG_SWORD, &[(11, 1), (10, 0), (3, 0), (3, 1)]);
    assert_eq!((item.enchantment, item.buc), (-2, Buc::Cursed));
    let item = scripted(K::LONG_SWORD, &[(11, 1), (10, 1), (10, 0), (2, 0)]);
    assert_eq!((item.enchantment, item.buc), (0, Buc::Cursed));
}

#[test]
fn armor_draw_order_and_short_circuits() {
    let item = scripted(K::FUMBLE_BOOTS, &[(10, 1), (3, 1)]);
    assert_eq!((item.enchantment, item.buc), (-1, Buc::Cursed));
    let item = scripted(
        K::LEATHER_ARMOR,
        &[(10, 1), (11, 1), (10, 0), (2, 1), (3, 1)],
    );
    assert_eq!((item.enchantment, item.buc), (1, Buc::Blessed));
    let item = scripted(K::LEATHER_ARMOR, &[(10, 0), (10, 1), (10, 1)]);
    assert_eq!((item.enchantment, item.buc), (0, Buc::Uncursed));
    let item = scripted(K::LEATHER_ARMOR, &[(10, 1), (11, 0), (3, 1)]);
    assert_eq!(item.enchantment, -1);
}

#[test]
fn ring_inner_draw_sign_and_zero_fallback() {
    let item = scripted(
        K::RIN_PROTECTION,
        &[(3, 1), (10, 1), (10, 1), (2, 0), (3, 1), (5, 1)],
    );
    assert_eq!((item.enchantment, item.buc), (-1, Buc::Cursed));
    let item = scripted(
        K::RIN_PROTECTION,
        &[(3, 0), (2, 1), (10, 1), (10, 1), (3, 1)],
    );
    assert_eq!((item.enchantment, item.buc), (1, Buc::Blessed));
    let item = scripted(
        K::RIN_PROTECTION,
        &[(3, 1), (10, 0), (4, 0), (3, 2), (5, 0)],
    );
    assert_eq!((item.enchantment, item.buc), (-2, Buc::Uncursed));
    let item = scripted(K::RIN_TELEPORTATION, &[(10, 1)]);
    assert_eq!(item.buc, Buc::Cursed);
    scripted(K::RIN_FIRE_RESISTANCE, &[(10, 1), (9, 1)]);
    scripted(K::RIN_TELEPORTATION, &[(10, 0)]);
}

#[test]
fn charge_endpoints_and_class_buc() {
    for (kind, draws, expected) in [
        (K::WAN_WISHING, vec![(17, 1)], 1),
        (K::WAN_STASIS, vec![(4, 0), (17, 1)], 3),
        (K::WAN_STASIS, vec![(4, 3), (17, 1)], 6),
        (K::WAN_LIGHT, vec![(5, 0), (17, 1)], 11),
        (K::WAN_LIGHT, vec![(5, 4), (17, 1)], 15),
        (K::WAN_FIRE, vec![(5, 0), (17, 1)], 4),
        (K::WAN_FIRE, vec![(5, 4), (17, 1)], 8),
        (K::MAGIC_MARKER, vec![(70, 69)], 99),
        (K::CAN_OF_GREASE, vec![(21, 20), (10, 0), (2, 0)], 25),
        (K::CRYSTAL_BALL, vec![(5, 4), (2, 0), (2, 1)], 7),
        (K::BAG_OF_TRICKS, vec![(18, 17)], 20),
        (K::MAGIC_FLUTE, vec![(5, 4)], 8),
        (K::BELL_OF_OPENING, vec![], 3),
        (K::MAGIC_LAMP, vec![(2, 1)], 1),
    ] {
        let item = scripted(kind, &draws);
        assert_eq!(item.enchantment, expected, "{kind:?}");
        assert_eq!(item.recharged, 0);
    }
    assert_eq!(
        scripted(K::AMULET_OF_STRANGULATION, &[(10, 1)]).buc,
        Buc::Cursed
    );
    assert_eq!(
        scripted(K::AMULET_OF_LIFE_SAVING, &[(10, 1), (10, 0), (2, 1)]).buc,
        Buc::Blessed
    );
    assert_eq!(scripted(K::POT_HEALING, &[(4, 0), (2, 0)]).buc, Buc::Cursed);
    assert_eq!(
        scripted(K::SCR_IDENTIFY, &[(4, 0), (2, 1)]).buc,
        Buc::Blessed
    );
    assert_eq!(scripted(K::SPE_FORCE_BOLT, &[(17, 1)]).buc, Buc::Uncursed);
    assert_eq!(scripted(K::LOADSTONE, &[]).buc, Buc::Cursed);
}

#[test]
fn rne_stops_at_level_dependent_cap_without_extra_draw() {
    for (level, cap) in [(14, 5), (15, 5), (18, 6), (30, 10)] {
        let mut draws = vec![(11, 0)];
        draws.extend(std::iter::repeat_n((3, 0), cap - 1));
        draws.push((2, 1));
        let rules = Ruleset::vanilla();
        let mut next = draws.iter();
        let item = rules
            .create_random_item_supported_fields(K::LONG_SWORD, ItemLocation::Limbo, level, |n| {
                let &(bound, value) = next.next().unwrap();
                assert_eq!(bound, n);
                value
            })
            .unwrap();
        assert_eq!(item.enchantment, cap as i8);
        assert_eq!(next.len(), 0);
    }
}

#[test]
fn already_blessed_needs_no_buc_draw_and_out_of_range_rolls_clamp() {
    let rules = Ruleset::vanilla();
    let item = rules
        .create_item_record_by_id(K::POT_HEALING, ItemLocation::Limbo, Buc::Blessed)
        .unwrap();
    let item = initialize_supported_fields(item, ItemInitProfile::Potion, None, 1, |_| {
        panic!("BC draw for blessed object")
    });
    assert_eq!(item.buc, Buc::Blessed);
    let item = rules
        .create_random_item_supported_fields(K::WAN_FIRE, ItemLocation::Limbo, 1, |_| u32::MAX)
        .unwrap();
    assert_eq!((item.enchantment, item.buc), (8, Buc::Uncursed));
}

#[test]
fn generated_profiles_use_c_facts_and_exclude_contextual_payloads() {
    for kind in [K::ARROW, K::CROSSBOW_BOLT, K::DART, K::SHURIKEN] {
        assert_eq!(
            Ruleset::vanilla().item_by_id(kind).unwrap().initialization,
            ItemInitProfile::Weapon {
                multigen: true,
                poisonable: true
            }
        );
    }
    for kind in [
        K::CORPSE,
        K::EGG,
        K::TIN,
        K::FIGURINE,
        K::STATUE,
        K::OIL_LAMP,
        K::WAX_CANDLE,
        K::CHEST,
    ] {
        assert!(Ruleset::vanilla()
            .item_by_id(kind)
            .unwrap()
            .initialization
            .is_fixed());
    }
    for arch in ITEM_CATALOG {
        assert!(
            arch.initialization.matches_class(arch.class),
            "{}",
            arch.name
        );
    }
}

#[test]
fn renamed_owned_override_controls_weight_direction_and_fixed_opt_out() {
    let mut rules = (*Ruleset::vanilla()).clone();
    let def = rules
        .items
        .iter_mut()
        .find(|d| d.id == Some(K::WAN_LIGHT))
        .unwrap();
    def.name = "renamed beam".into();
    def.weight = 47;
    def.wand_dir = Some(WandDir::Ray);
    rules.reindex();
    let item = rules
        .create_random_item_supported_fields(K::WAN_LIGHT, ItemLocation::Limbo, 1, |n| n - 1)
        .unwrap();
    assert_eq!(
        (&*item.name, item.weight, item.enchantment),
        ("renamed beam", 47, 8)
    );
    rules
        .items
        .iter_mut()
        .find(|d| d.id == Some(K::WAN_LIGHT))
        .unwrap()
        .initialization = ItemInitProfile::Fixed;
    rules.reindex();
    assert_eq!(
        rules
            .create_random_item_supported_fields(K::WAN_LIGHT, ItemLocation::Limbo, 1, |_| panic!(
                "fixed RNG"
            ))
            .unwrap()
            .enchantment,
        6
    );
}
