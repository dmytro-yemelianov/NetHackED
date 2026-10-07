use nethacked_core::ac::ArmorSlot;
use nethacked_data::{ItemKindId as K, Ruleset};

#[test]
fn released_c_armor_delay_and_slot_boundaries() {
    // NetHack-5.0.0_Released, 16ff59115315917b93185d026aeefea06db9b0f4.
    // Independent literal facts from include/objects.h:452-454, 595-609.
    // ARMOR forwards delay unchanged to OBJECT; ARM_SUIT/HELM/SHIRT
    // are separate eligibility categories even when their delays are zero.
    let rules = Ruleset::vanilla();
    for (id, delay, slot) in [
        (K::LEATHER_ARMOR, 3, ArmorSlot::Suit),
        (K::LEATHER_JACKET, 0, ArmorSlot::Suit),
        (K::FEDORA, 0, ArmorSlot::Helmet),
        (K::HAWAIIAN_SHIRT, 0, ArmorSlot::Shirt),
        (K::T_SHIRT, 0, ArmorSlot::Shirt),
    ] {
        let item = rules.item_by_id(id).unwrap();
        assert_eq!(item.oc_delay, delay, "{}", id.c_name());
        assert_eq!(item.armor.as_ref().unwrap().slot, slot);
    }
    let arrow = rules.item_by_id(K::ARROW).unwrap();
    assert_eq!(arrow.oc_delay, 0);
    assert!(arrow.armor.is_none());
}

#[test]
fn renamed_and_custom_definitions_keep_owned_delay_after_loading() {
    let mut rules = (*Ruleset::vanilla()).clone();
    let leather = rules
        .items
        .iter_mut()
        .find(|d| d.id == Some(K::LEATHER_ARMOR))
        .unwrap();
    leather.name = "slow cuirass".into();
    leather.oc_delay = 17;
    let mut custom = leather.clone();
    custom.name = "quick custom suit".into();
    custom.id = None;
    custom.oc_delay = 0;
    rules.items.push(custom);
    let mut loaded: Ruleset =
        serde_json::from_value(serde_json::to_value(&rules).unwrap()).unwrap();
    loaded.reindex();
    assert_eq!(loaded.item_by_id(K::LEATHER_ARMOR).unwrap().oc_delay, 17);
    assert_eq!(loaded.item("SLOW CUIRASS").unwrap().oc_delay, 17);
    assert_eq!(loaded.item("quick custom suit").unwrap().oc_delay, 0);
    assert_eq!(
        loaded
            .item("quick custom suit")
            .unwrap()
            .armor
            .as_ref()
            .unwrap()
            .slot,
        ArmorSlot::Suit
    );
}

#[cfg(feature = "schema")]
#[test]
fn delay_schema_is_optional_and_unsigned() {
    let schema = serde_json::to_value(schemars::schema_for!(nethacked_data::ItemDef)).unwrap();
    assert_eq!(schema["properties"]["oc_delay"]["minimum"], 0.0);
    assert_eq!(schema["properties"]["oc_delay"]["type"], "integer");
    assert!(!schema["required"]
        .as_array()
        .unwrap()
        .contains(&"oc_delay".into()));
}
