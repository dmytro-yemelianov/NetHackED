use nethacked_core::ac::ArmorSlot;
use nethacked_data::{items::WornProperty, ItemKindId as K, Ruleset};
#[test]
fn released_red_dragon_scale_mail_owns_fire_resistance() {
    // Released 16ff59115315917b93185d026aeefea06db9b0f4 objects.h:513.
    let rs = Ruleset::vanilla();
    let item = rs.item_by_id(K::RED_DRAGON_SCALE_MAIL).unwrap();
    assert_eq!(item.worn_property, Some(WornProperty::FireResistance));
    assert_eq!(item.oc_delay, 5);
    assert_eq!(item.armor.as_ref().unwrap().slot, ArmorSlot::Suit);
    assert_eq!(rs.item_by_id(K::LEATHER_ARMOR).unwrap().worn_property, None);
}
#[test]
fn old_owned_metadata_absent_is_authoritative_even_for_c_identity() {
    let mut value = serde_json::to_value(
        Ruleset::vanilla()
            .item_by_id(K::RED_DRAGON_SCALE_MAIL)
            .unwrap(),
    )
    .unwrap();
    value.as_object_mut().unwrap().remove("worn_property");
    let old: nethacked_data::ItemDef = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(old.worn_property, None);
    assert_eq!(serde_json::to_value(old).unwrap(), value);
}
#[cfg(feature = "schema")]
#[test]
fn owned_property_schema_is_optional_and_typed() {
    let schema = serde_json::to_value(schemars::schema_for!(nethacked_data::ItemDef)).unwrap();
    assert!(schema["properties"].get("worn_property").is_some());
    assert!(!schema["required"]
        .as_array()
        .unwrap()
        .contains(&"worn_property".into()));
    assert!(schema.to_string().contains("fire_resistance"));
}
