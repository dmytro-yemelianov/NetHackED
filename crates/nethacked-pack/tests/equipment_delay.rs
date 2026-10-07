use nethacked_data::{ItemKindId as K, Ruleset};
use nethacked_pack::{
    build_from_files, load_nhpack_bytes, nhpack_to_bytes, ruleset_hash, PackFiles,
};

#[test]
fn absent_delay_preserves_pre_p06_canonical_hash_and_loads_old_bundle() {
    let mut json = serde_json::to_value(&*Ruleset::vanilla()).unwrap();
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
        item.as_object_mut().unwrap().remove("oc_delay");
        item.as_object_mut().unwrap().remove("worn_property");
    }
    for role in json["roles"].as_array_mut().unwrap() {
        role.as_object_mut().unwrap().remove("original_monster");
    }
    let legacy: Ruleset = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(serde_json::to_value(&legacy).unwrap(), json);
    // Captured from the real pre-P06 vanilla Ruleset before source edits.
    let old_hash = "sha256:f05b40120d03a525e667745d2375187700cb5a96a8c2a6125084f6b29cc2c46e";
    assert_eq!(ruleset_hash(&legacy), old_hash);
    let bundle = serde_json::json!({"format": 1, "manifest": legacy.manifest, "ruleset": json, "hash": old_hash});
    let (loaded, _, _) = load_nhpack_bytes(&serde_json::to_vec(&bundle).unwrap()).unwrap();
    assert_eq!(loaded.item_by_id(K::LEATHER_ARMOR).unwrap().oc_delay, 0);
    assert_eq!(ruleset_hash(&loaded), old_hash);
    assert_ne!(ruleset_hash(&Ruleset::vanilla()), old_hash);
}

#[test]
fn authored_delay_zero_and_nonzero_override_roundtrip_and_hash() {
    for delay in [0, 9] {
        let files = PackFiles(std::collections::BTreeMap::from([
            (
                "pack.toml".into(),
                "id = 'delay-test'\nname = 'Delay test'\nversion = '1.0.0'\nbase = 'vanilla'\n"
                    .into(),
            ),
            (
                "items.toml".into(),
                format!("[[item]]\nname = 'leather armor'\noc_delay = {delay}\n"),
            ),
        ]));
        let (pack, report) = build_from_files(&files).unwrap();
        assert!(report.is_ok());
        assert_eq!(
            pack.ruleset.item_by_id(K::LEATHER_ARMOR).unwrap().oc_delay,
            delay
        );
        let expected_hash = pack.hash.clone();
        let (loaded, _, _) = load_nhpack_bytes(&nhpack_to_bytes(&pack)).unwrap();
        assert_eq!(loaded.item("LEATHER ARMOR").unwrap().oc_delay, delay);
        assert_eq!(ruleset_hash(&loaded), expected_hash);
        let mut changed = (*loaded).clone();
        changed
            .items
            .iter_mut()
            .find(|d| d.id == Some(K::LEATHER_ARMOR))
            .unwrap()
            .oc_delay = 3;
        assert_ne!(ruleset_hash(&changed), expected_hash);
        let json = serde_json::to_value(loaded.item("leather armor").unwrap()).unwrap();
        if delay == 0 {
            assert!(json.get("oc_delay").is_none());
        } else {
            assert_eq!(json["oc_delay"], 9);
        }
    }
}

#[test]
fn exported_custom_item_delay_and_patch_schema_are_retained() {
    let mut files = nethacked_pack::vanilla_pack_files();
    let mut items: nethacked_pack::ItemsToml = toml::from_str(&files.0["items.toml"]).unwrap();
    let mut custom = items
        .item
        .iter()
        .find(|i| i.name == "leather armor")
        .unwrap()
        .clone();
    assert_eq!(custom.oc_delay, Some(3));
    custom.name = "custom cuirass".into();
    custom.new = true;
    custom.oc_delay = Some(11);
    items.item = vec![custom];
    files
        .0
        .insert("items.toml".into(), toml::to_string(&items).unwrap());
    let (pack, _) = build_from_files(&files).unwrap();
    let (loaded, _, _) = load_nhpack_bytes(&nhpack_to_bytes(&pack)).unwrap();
    let item = loaded.item("custom cuirass").unwrap();
    assert_eq!(item.id, None);
    assert_eq!(item.oc_delay, 11);
    #[cfg(feature = "schema")]
    {
        let schema =
            serde_json::to_value(schemars::schema_for!(nethacked_pack::ItemPatch)).unwrap();
        assert!(schema["properties"].get("oc_delay").is_some());
        assert!(!schema["required"]
            .as_array()
            .unwrap()
            .contains(&"oc_delay".into()));
    }
}
