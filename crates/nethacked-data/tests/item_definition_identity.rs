use nethacked_arena::{EntityArena, ItemDefinition, ItemLocation, ItemRecord};
use nethacked_data::item_init::{initialize_supported_fields, ItemInitProfile};
use nethacked_data::{ItemKindId as I, Ruleset, WandDir};
use nethacked_types::{BonesItem, Buc};
use std::collections::BTreeSet;

#[test]
fn owned_and_static_factories_use_canonical_or_exact_normalized_custom_keys() {
    let mut rules = (*Ruleset::vanilla()).clone();
    let def = rules
        .items
        .iter_mut()
        .find(|d| d.id == Some(I::WAN_CREATE_MONSTER))
        .unwrap();
    def.name = "Owned rod".into();
    def.weight = 91;
    def.wand_dir = Some(WandDir::NoDir);
    let mut custom = def.clone();
    custom.id = None;
    custom.name = "Pack ROD".into();
    rules.items.push(custom);
    rules.reindex();
    let mut canonical = rules
        .create_item_record_by_id(I::WAN_CREATE_MONSTER, ItemLocation::Limbo, Buc::Uncursed)
        .unwrap();
    let key = ItemDefinition::Vanilla("WAN_CREATE_MONSTER".into());
    assert_eq!(canonical.definition, Some(key.clone()));
    assert_eq!((canonical.weight, canonical.enchantment), (91, 13));
    canonical.name = "artifact display cold death".into();
    assert_eq!(
        rules
            .item_for_definition(canonical.definition.as_ref().unwrap())
            .unwrap()
            .name,
        "Owned rod"
    );
    let static_item = nethacked_data::create_item_record(
        I::WAN_CREATE_MONSTER,
        ItemLocation::Limbo,
        Buc::Uncursed,
    );
    assert_eq!(static_item.definition, Some(key.clone()));
    let mut custom = rules
        .create_item_record("PACK rod", ItemLocation::Limbo, Buc::Uncursed)
        .unwrap();
    assert_eq!(
        custom.definition,
        Some(ItemDefinition::Custom("pack rod".into()))
    );
    let restored: ItemRecord =
        serde_json::from_str(&serde_json::to_string(&custom).unwrap()).unwrap();
    assert_eq!(restored, custom);
    custom.name = "wand of create monster".into();
    let mut trace = [(5, 1), (17, 1)].into_iter();
    let custom = initialize_supported_fields(
        custom,
        ItemInitProfile::Wand {
            charges: nethacked_data::item_init::WandCharges::Standard,
        },
        Some(WandDir::NoDir),
        1,
        |n| {
            let (bound, value) = trace.next().unwrap();
            assert_eq!(n, bound);
            value
        },
    );
    assert!(trace.next().is_none());
    let restored: ItemRecord =
        serde_json::from_str(&serde_json::to_string(&custom).unwrap()).unwrap();
    assert_eq!(restored, custom);
    assert_eq!(
        custom.definition,
        Some(ItemDefinition::Custom("pack rod".into()))
    );
    assert_eq!(
        rules
            .item_for_definition(custom.definition.as_ref().unwrap())
            .unwrap()
            .id,
        None
    );
    assert!(rules
        .item_for_definition(&ItemDefinition::Custom("Pack ROD".into()))
        .is_none());
    assert!(rules
        .item_for_definition(&ItemDefinition::Custom("owned rod".into()))
        .is_none());
    rules
        .items
        .retain(|d| d.id != Some(I::WAN_CREATE_MONSTER) && d.name != "Pack ROD");
    rules.reindex();
    for missing in [
        key,
        ItemDefinition::Vanilla("UNKNOWN".into()),
        ItemDefinition::Custom("pack rod".into()),
    ] {
        assert!(rules.item_for_definition(&missing).is_none());
    }
}

#[test]
fn missing_legacy_item_and_bones_identity_keeps_canonical_bytes_omitted() {
    let item = nethacked_data::create_item_record(
        I::WAN_CREATE_MONSTER,
        ItemLocation::Limbo,
        Buc::Uncursed,
    );
    let mut old = serde_json::to_value(item).unwrap();
    old.as_object_mut().unwrap().remove("definition");
    let bytes = serde_json::to_vec(&old).unwrap();
    let item: ItemRecord = serde_json::from_value(old).unwrap();
    assert!(item.definition.is_none());
    assert_eq!(
        serde_json::to_vec(&serde_json::to_value(item).unwrap()).unwrap(),
        bytes
    );
    let old = serde_json::json!({"name":"wand of create monster", "class":"Wand", "weight":7, "buc":"Uncursed", "enchantment":2});
    let bytes = serde_json::to_vec(&old).unwrap();
    let bones: BonesItem = serde_json::from_value(old).unwrap();
    assert!(bones.definition.is_none());
    assert_eq!(
        serde_json::to_vec(&serde_json::to_value(bones).unwrap()).unwrap(),
        bytes
    );
}

#[test]
fn artifact_identity_and_creation_history_remain_separate_from_base_definition() {
    let rules = Ruleset::vanilla();
    let mut sword = rules
        .create_item_record_by_id(I::LONG_SWORD, ItemLocation::Limbo, Buc::Uncursed)
        .unwrap();
    let mut history = BTreeSet::new();
    rules.make_existing_artifact(&mut sword, I::LONG_SWORD, "Knight", &mut history, |n| {
        assert_eq!(n, 5);
        0
    });
    assert_eq!(sword.artifact_id.as_deref(), Some("ART_FROST_BRAND"));
    assert_eq!(
        sword.definition,
        Some(ItemDefinition::Vanilla("LONG_SWORD".into()))
    );
    assert_eq!(history, BTreeSet::from(["ART_FROST_BRAND".into()]));
    let mut arena = EntityArena::new();
    let id = arena.spawn_item(sword);
    arena.destroy_item(id).unwrap();
    assert!(arena.artifact_history.contains("ART_FROST_BRAND"));
    let explicit =
        nethacked_data::create_item_record(I::ART_EXCALIBUR, ItemLocation::Limbo, Buc::Uncursed);
    assert_eq!(
        explicit.definition,
        Some(ItemDefinition::Vanilla(I::ART_EXCALIBUR.c_name().into()))
    );
    assert_eq!(explicit.artifact_id.as_deref(), Some("ART_EXCALIBUR"));
}
