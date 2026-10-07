use nethacked_data::{items::WornProperty, ItemKindId as K, Ruleset};
use nethacked_pack::{
    build_from_files, load_nhpack_bytes, nhpack_to_bytes, ruleset_hash, PackFiles,
};
fn files(patch: &str) -> PackFiles {
    PackFiles(std::collections::BTreeMap::from([
        (
            "pack.toml".into(),
            "id='property'\nname='Property'\nversion='1'\nbase='vanilla'\n".into(),
        ),
        (
            "items.toml".into(),
            format!("[[item]]\nname='red dragon scale mail'\n{patch}\n"),
        ),
    ]))
}
#[test]
fn authored_omission_preserves_source_clear_removes_source_and_conflict_rejects() {
    for (patch, expected) in [
        ("", Some(WornProperty::FireResistance)),
        ("clear_worn_property=true", None),
        (
            "worn_property='fire_resistance'",
            Some(WornProperty::FireResistance),
        ),
    ] {
        let (pack, _) = build_from_files(&files(patch)).unwrap();
        let (loaded, _, _) = load_nhpack_bytes(&nhpack_to_bytes(&pack)).unwrap();
        assert_eq!(
            loaded
                .item_by_id(K::RED_DRAGON_SCALE_MAIL)
                .unwrap()
                .worn_property,
            expected
        );
        assert_eq!(ruleset_hash(&loaded), pack.hash);
    }
    assert!(build_from_files(&files(
        "worn_property='fire_resistance'\nclear_worn_property=true"
    ))
    .is_err());
}
#[test]
fn absent_property_preserves_captured_pre_p07_export_hash_and_old_bundle() {
    let mut rs = (*Ruleset::vanilla()).clone();
    rs.manifest = nethacked_data::PackManifest {
        id: "vanilla-export".into(),
        name: "Vanilla Export".into(),
        version: "0.1.0".into(),
        base: "vanilla".into(),
        description: "Exported vanilla NetHackED data".into(),
    };
    let mut json = serde_json::to_value(&rs).unwrap();
    for monster in json["monsters"].as_array_mut().unwrap() {
        monster
            .as_object_mut()
            .unwrap()
            .remove("corpse_timeout_policy");
        monster
            .as_object_mut()
            .unwrap()
            .remove("corpse_thaw_policy");
    }
    for item in json["items"].as_array_mut().unwrap() {
        item.as_object_mut().unwrap().remove("worn_property");
    }
    for role in json["roles"].as_array_mut().unwrap() {
        role.as_object_mut().unwrap().remove("original_monster");
    }
    let legacy: Ruleset = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(serde_json::to_value(&legacy).unwrap(), json);
    // Captured by pre-edit binary export/build, /tmp/netrust-p07-old.nhpack.
    let old_hash = "sha256:e98024c6838e7c43e6a4323bc0ec02aeeef2c3ee36a673f4dfc064be14436d49";
    assert_eq!(ruleset_hash(&legacy), old_hash);
    let bundle =
        serde_json::json!({"format":1,"manifest":legacy.manifest,"ruleset":json,"hash":old_hash});
    let (loaded, _, _) = load_nhpack_bytes(&serde_json::to_vec(&bundle).unwrap()).unwrap();
    assert_eq!(
        loaded
            .item_by_id(K::RED_DRAGON_SCALE_MAIL)
            .unwrap()
            .worn_property,
        None
    );
    assert_eq!(ruleset_hash(&loaded), old_hash);
}
#[test]
fn exported_custom_property_and_clear_schema_roundtrip() {
    let mut files = nethacked_pack::vanilla_pack_files();
    let mut items: nethacked_pack::ItemsToml = toml::from_str(&files.0["items.toml"]).unwrap();
    let mut custom = items
        .item
        .iter()
        .find(|i| i.name == "red dragon scale mail")
        .unwrap()
        .clone();
    custom.name = "owned custom suit".into();
    custom.new = true;
    items.item = vec![custom];
    files
        .0
        .insert("items.toml".into(), toml::to_string(&items).unwrap());
    let (pack, _) = build_from_files(&files).unwrap();
    let (loaded, _, _) = load_nhpack_bytes(&nhpack_to_bytes(&pack)).unwrap();
    assert_eq!(loaded.item("owned custom suit").unwrap().id, None);
    assert_eq!(
        loaded.item("owned custom suit").unwrap().worn_property,
        Some(WornProperty::FireResistance)
    );
    #[cfg(feature = "schema")]
    {
        let schema =
            serde_json::to_value(schemars::schema_for!(nethacked_pack::ItemPatch)).unwrap();
        assert!(schema["properties"].get("clear_worn_property").is_some());
        assert!(!schema["required"]
            .as_array()
            .unwrap()
            .contains(&"worn_property".into()));
    }
}
