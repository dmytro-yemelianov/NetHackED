use nethacked_data::monsters::{CorpseTimeoutPolicy as Policy, MonsterSpeciesId};
use nethacked_pack::{
    build_from_files, load_nhpack_bytes, nhpack_to_bytes, parse_pack_files, ruleset_hash,
    vanilla_pack_files,
};
use sha2::{Digest, Sha256};

fn patched(monsters: &str) -> nethacked_pack::NhPack {
    let mut files = vanilla_pack_files();
    files.0.insert("monsters.toml".into(), monsters.into());
    build_from_files(&files).unwrap().0
}

#[test]
fn patch_omission_preserves_explicit_ordinary_clears_and_custom_can_select_policy() {
    let pack = patched(
        r#"[[monster]]
name = "lizard"
ac = 2
[[monster]]
name = "lichen"
corpse_timeout_policy = "ordinary"
[[monster]]
name = "Death"
corpse_timeout_policy = "never"
[[monster]]
name = "jackal"
glyph = "T"
corpse_timeout_policy = "rider"
[[monster]]
name = "troll"
glyph = "x"
"#,
    );
    assert_eq!(
        pack.ruleset
            .monster("lizard")
            .unwrap()
            .corpse_timeout_policy,
        Policy::Never
    );
    assert_eq!(
        pack.ruleset
            .monster("lichen")
            .unwrap()
            .corpse_timeout_policy,
        Policy::Ordinary
    );
    let rider = pack.ruleset.monster_by_id(MonsterSpeciesId::DEATH).unwrap();
    assert_eq!(rider.corpse_timeout_policy, Policy::Never);
    let renamed_class = pack.ruleset.monster("jackal").unwrap();
    assert_eq!(
        (renamed_class.glyph, renamed_class.corpse_timeout_policy),
        ('T', Policy::Rider)
    );
    assert_eq!(pack.ruleset.monster("troll").unwrap().glyph, 'x');

    // A full new definition reuses exported required fields, but no vanilla ID.
    let files = vanilla_pack_files();
    let mut new = parse_pack_files(&files).unwrap().monsters.remove(0);
    new.name = "custom policy species".into();
    new.new = true;
    new.glyph = Some('T');
    for policy in [
        None,
        Some(Policy::Never),
        Some(Policy::Rider),
        Some(Policy::Ordinary),
    ] {
        new.corpse_timeout_policy = policy;
        let text = toml::to_string(&nethacked_pack::MonstersToml {
            monster: vec![new.clone()],
        })
        .unwrap();
        let pack = patched(&text);
        let def = pack.ruleset.monster(&new.name).unwrap();
        assert_eq!(def.id, None);
        assert_eq!(def.glyph, 'T');
        assert_eq!(def.corpse_timeout_policy, policy.unwrap_or_default());
    }
}

#[test]
fn export_rebuild_and_full_wire_retain_policies_and_canonical_hashes() {
    let files = vanilla_pack_files();
    let parsed = parse_pack_files(&files).unwrap();
    assert!(parsed
        .monsters
        .iter()
        .all(|m| m.corpse_timeout_policy.is_some()));
    let (pack, _) = build_from_files(&files).unwrap();
    let bytes = nhpack_to_bytes(&pack);
    let (loaded, reference, decoded) = load_nhpack_bytes(&bytes).unwrap();
    assert_eq!(reference.hash, pack.hash);
    assert_eq!(nhpack_to_bytes(&decoded), bytes);
    assert_eq!(
        loaded.monster("lizard").unwrap().corpse_timeout_policy,
        Policy::Never
    );
    assert_eq!(
        loaded.monster("Death").unwrap().corpse_timeout_policy,
        Policy::Rider
    );
    assert_eq!(pack.hash, build_from_files(&files).unwrap().0.hash);
    let mut changed = (*loaded).clone();
    changed.monsters[0].corpse_timeout_policy = Policy::Never;
    assert_ne!(ruleset_hash(&changed), pack.hash);
}

#[test]
fn captured_preedit_full_bundle_hash_and_bytes_still_load_without_new_facts() {
    // Captured before implementation: export-vanilla then build on the reviewed
    // P04-P08a working tree. Both literal hashes identify the actual old bundle,
    // rather than an expected value sampled from the new implementation.
    const OLD_HASH: &str =
        "sha256:5ec04ea1d6976c5f4bdb6fbe7e1b8818ee792962914fdc06815fef6af7851d08";
    const OLD_BYTES_SHA256: &str =
        "ab544c30e8dd2a37b219f9d510c3abbd9c30d014bbc730616c56ad4b74656d7a";
    let (mut old, _) = build_from_files(&vanilla_pack_files()).unwrap();
    for def in &mut old.ruleset.monsters {
        def.corpse_timeout_policy = Policy::Ordinary;
        def.corpse_thaw_policy = nethacked_data::monsters::CorpseThawPolicy::Ordinary;
    }
    for role in &mut old.ruleset.roles {
        role.original_monster = None;
    }
    old.hash = ruleset_hash(&old.ruleset);
    assert_eq!(old.hash, OLD_HASH);
    let bytes = nhpack_to_bytes(&old);
    assert_eq!(bytes.len(), 994652);
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), OLD_BYTES_SHA256);
    assert!(!std::str::from_utf8(&bytes)
        .unwrap()
        .contains("corpse_timeout_policy"));
    let (loaded, reference, decoded) = load_nhpack_bytes(&bytes).unwrap();
    assert_eq!(reference.hash, OLD_HASH);
    assert!(loaded
        .monsters
        .iter()
        .all(|m| m.corpse_timeout_policy == Policy::Ordinary));
    assert_eq!(nhpack_to_bytes(&decoded), bytes);
}

#[test]
fn extracted_source_policy_and_owned_c_class_agree_for_every_species() {
    let extracted: toml::Value =
        toml::from_str(include_str!("../../../data/nethack-5.0/monsters.toml")).unwrap();
    let rules = nethacked_data::Ruleset::vanilla();
    let monsters = extracted["monster"].as_array().unwrap();
    assert_eq!(monsters.len(), rules.monsters.len());
    for (source, owned) in monsters.iter().zip(&rules.monsters) {
        assert_eq!(source["id"].as_str().unwrap(), owned.id.unwrap().c_name());
        let policy: Policy = serde_json::from_value(serde_json::Value::String(
            source["corpse_timeout_policy"].as_str().unwrap().into(),
        ))
        .unwrap();
        assert_eq!(owned.corpse_timeout_policy, policy);
        assert_eq!(source["glyph"].as_str().unwrap(), owned.glyph.to_string());
        assert_eq!(
            source["class"].as_str().unwrap() == "troll",
            owned.glyph == 'T'
        );
    }
}

#[cfg(feature = "schema")]
#[test]
fn schema_exposes_typed_patch_and_owned_policy() {
    for schema in [
        serde_json::to_value(schemars::schema_for!(nethacked_pack::MonsterPatch)).unwrap(),
        serde_json::to_value(schemars::schema_for!(nethacked_data::ruleset::MonsterDef)).unwrap(),
    ] {
        assert!(schema["properties"].get("corpse_timeout_policy").is_some());
        assert_eq!(
            schema["definitions"]["CorpseTimeoutPolicy"]["enum"],
            serde_json::json!(["ordinary", "never", "rider"])
        );
    }
}
