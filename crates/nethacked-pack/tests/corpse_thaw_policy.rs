use nethacked_data::monsters::{CorpseThawPolicy as Policy, MonsterSpeciesId};
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
fn patch_omission_inherits_explicit_ordinary_clears_and_custom_selects() {
    let inherited = patched("[[monster]]\nname = \"ice troll\"\nac = 2\n");
    assert_eq!(
        inherited
            .ruleset
            .monster_by_id(MonsterSpeciesId::ICE_TROLL)
            .unwrap()
            .corpse_thaw_policy,
        Policy::IceTroll
    );
    let overridden = patched("[[monster]]\nname = \"ice troll\"\ncorpse_thaw_policy = \"ordinary\"\n[[monster]]\nname = \"jackal\"\ncorpse_thaw_policy = \"ice_troll\"\n");
    assert_eq!(
        overridden
            .ruleset
            .monster_by_id(MonsterSpeciesId::ICE_TROLL)
            .unwrap()
            .corpse_thaw_policy,
        Policy::Ordinary
    );
    assert_eq!(
        overridden
            .ruleset
            .monster("jackal")
            .unwrap()
            .corpse_thaw_policy,
        Policy::IceTroll
    );
    let mut new = parse_pack_files(&vanilla_pack_files())
        .unwrap()
        .monsters
        .remove(0);
    new.name = "custom thaw species".into();
    new.new = true;
    for policy in [None, Some(Policy::Ordinary), Some(Policy::IceTroll)] {
        new.corpse_thaw_policy = policy;
        let text = toml::to_string(&nethacked_pack::MonstersToml {
            monster: vec![new.clone()],
        })
        .unwrap();
        let pack = patched(&text);
        let def = pack.ruleset.monster(&new.name).unwrap();
        assert_eq!(def.id, None);
        assert_eq!(def.corpse_thaw_policy, policy.unwrap_or_default());
    }
}

#[test]
fn exported_full_bundle_retains_owned_policy_and_hash() {
    let files = vanilla_pack_files();
    let patches = parse_pack_files(&files).unwrap();
    assert!(patches
        .monsters
        .iter()
        .all(|m| m.corpse_thaw_policy.is_some()));
    let (pack, _) = build_from_files(&files).unwrap();
    let bytes = nhpack_to_bytes(&pack);
    let (loaded, reference, decoded) = load_nhpack_bytes(&bytes).unwrap();
    assert_eq!(reference.hash, pack.hash);
    assert_eq!(nhpack_to_bytes(&decoded), bytes);
    assert_eq!(
        loaded
            .monster_by_id(MonsterSpeciesId::ICE_TROLL)
            .unwrap()
            .corpse_thaw_policy,
        Policy::IceTroll
    );
    assert_eq!(build_from_files(&files).unwrap().0.hash, pack.hash);
    let mut custom = (*loaded).clone();
    custom
        .monsters
        .iter_mut()
        .find(|m| m.id == Some(MonsterSpeciesId::ICE_TROLL))
        .unwrap()
        .corpse_thaw_policy = Policy::Ordinary;
    assert_ne!(ruleset_hash(&custom), pack.hash);
}

#[test]
fn actual_captured_preedit_bundle_hash_and_bytes_remain_exact() {
    // Captured via the preedit binary export/build, before any source edit.
    const OLD_HASH: &str =
        "sha256:da0a7374b2b56c0276bd7393b7325e3275ed2efb91ef206258b7640bbc3d79d7";
    const OLD_BYTES_SHA256: &str =
        "36e437cbb031cc04af655a89f6d5e01b65a7217d267df540c40fcecd854687ff";
    let (mut old, _) = build_from_files(&vanilla_pack_files()).unwrap();
    for def in &mut old.ruleset.monsters {
        def.corpse_thaw_policy = Policy::Ordinary;
    }
    for role in &mut old.ruleset.roles {
        role.original_monster = None;
    }
    old.hash = ruleset_hash(&old.ruleset);
    assert_eq!(old.hash, OLD_HASH);
    let bytes = nhpack_to_bytes(&old);
    assert_eq!(bytes.len(), 994862);
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), OLD_BYTES_SHA256);
    assert!(!std::str::from_utf8(&bytes)
        .unwrap()
        .contains("corpse_thaw_policy"));
    let (loaded, reference, decoded) = load_nhpack_bytes(&bytes).unwrap();
    assert_eq!(reference.hash, OLD_HASH);
    assert!(loaded
        .monsters
        .iter()
        .all(|m| m.corpse_thaw_policy == Policy::Ordinary));
    assert_eq!(nhpack_to_bytes(&decoded), bytes);
}

#[cfg(feature = "schema")]
#[test]
fn schema_exposes_typed_owned_and_patch_policy() {
    for schema in [
        serde_json::to_value(schemars::schema_for!(nethacked_pack::MonsterPatch)).unwrap(),
        serde_json::to_value(schemars::schema_for!(nethacked_data::ruleset::MonsterDef)).unwrap(),
    ] {
        assert!(schema["properties"].get("corpse_thaw_policy").is_some());
        assert_eq!(
            schema["definitions"]["CorpseThawPolicy"]["enum"],
            serde_json::json!(["ordinary", "ice_troll"])
        );
    }
}

#[test]
fn extracted_thaw_policy_matches_exact_exception_for_every_species() {
    let rules = nethacked_data::Ruleset::vanilla();
    let source: toml::Value =
        toml::from_str(include_str!("../../../data/nethack-5.0/monsters.toml")).unwrap();
    let records = source["monster"].as_array().unwrap();
    assert_eq!(records.len(), rules.monsters.len());
    for (record, owned) in records.iter().zip(&rules.monsters) {
        assert_eq!(record["id"].as_str().unwrap(), owned.id.unwrap().c_name());
        let expected = if record["id"].as_str().unwrap() == "ICE_TROLL" {
            Policy::IceTroll
        } else {
            Policy::Ordinary
        };
        let extracted: Policy = serde_json::from_value(serde_json::json!(record
            ["corpse_thaw_policy"]
            .as_str()
            .unwrap()))
        .unwrap();
        assert_eq!(extracted, expected);
        assert_eq!(owned.corpse_thaw_policy, expected);
    }
}
