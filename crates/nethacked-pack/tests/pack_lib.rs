//! Integration tests for nethacked-pack: format, merge, validation, hash, diff, and integrity.

use std::fs;
use std::path::{Path, PathBuf};

use nethacked_data::ruleset::Ruleset;
use nethacked_pack::{
    build, diff, load_nhpack, read_pack_dir, resolve, ruleset_hash, validate, write_nhpack,
    PackError,
};
use proptest::prelude::*;

fn temp_pack_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nethacked-pack-test-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_vanilla_manifest(dir: &Path, id: &str) {
    let manifest = format!(
        r#"id = "{id}"
name = "Test Pack"
version = "1.0.0"
base = "vanilla"
description = "A test pack"
"#
    );
    fs::write(dir.join("pack.toml"), manifest).unwrap();
}

#[test]
fn test_patch_jackal_level() {
    let dir = temp_pack_dir("patch-jackal");
    write_vanilla_manifest(&dir, "patch-jackal");

    fs::write(
        dir.join("monsters.toml"),
        r#"
[[monster]]
name = "jackal"
level = 3
"#,
    )
    .unwrap();

    let (nhpack, report) = build(&dir).unwrap();
    assert!(report.is_ok());

    let jackal = nhpack.ruleset.monster("jackal").expect("jackal exists");
    assert_eq!(jackal.level, 3);

    let vanilla = Ruleset::vanilla();
    let v_jackal = vanilla.monster("jackal").unwrap();
    assert_eq!(jackal.glyph, v_jackal.glyph);
    assert_eq!(jackal.speed, v_jackal.speed);
    assert_eq!(jackal.ac, v_jackal.ac);
    assert_eq!(jackal.base_hp, v_jackal.base_hp);
}

#[test]
fn test_add_dire_jackal() {
    let dir = temp_pack_dir("add-dire-jackal");
    write_vanilla_manifest(&dir, "add-dire-jackal");

    fs::write(
        dir.join("monsters.toml"),
        r#"
[[monster]]
name = "dire jackal"
new = true
glyph = "d"
base_hp = 10
max_hp = 10
ac = 5
level = 2
speed = 14
alignment = "Neutral"
intrinsics = {}
attacks = [{ at = "Bite", ad = "Phys", n = 2, d = 4 }]
size = "Medium"
peaceful_by_default = false
always_hostile = true
maligntyp = 0
msound = "Other"
is_human = false
is_unique = false
mindless = false
ai_behavior = "MeleeHunter"
abilities = []
"#,
    )
    .unwrap();

    let (nhpack, report) = build(&dir).unwrap();
    assert!(report.is_ok());

    let dire = nhpack
        .ruleset
        .monster("dire jackal")
        .expect("dire jackal exists");
    assert_eq!(dire.id, None);
    assert!(!dire.sample_hp);
    assert_eq!(dire.level, 2);
    assert_eq!(dire.speed, 14);
    assert_eq!(dire.ac, 5);

    // Test missing field on new monster
    let dir_bad = temp_pack_dir("add-bad-dire-jackal");
    write_vanilla_manifest(&dir_bad, "add-bad-dire-jackal");
    fs::write(
        dir_bad.join("monsters.toml"),
        r#"
[[monster]]
name = "broken jackal"
new = true
glyph = "d"
base_hp = 10
max_hp = 10
ac = 5
speed = 14
alignment = "Neutral"
intrinsics = {}
attacks = [{ at = "Bite", ad = "Phys", n = 2, d = 4 }]
size = "Medium"
peaceful_by_default = false
always_hostile = true
maligntyp = 0
msound = "Other"
is_human = false
is_unique = false
mindless = false
ai_behavior = "MeleeHunter"
abilities = []
"#,
    )
    .unwrap();

    let err = build(&dir_bad).unwrap_err();
    match err {
        PackError::Resolve(diag) => {
            assert_eq!(diag.entry, "broken jackal");
            assert_eq!(diag.field, Some("level".into()));
        }
        other => panic!("expected Resolve error, got {other:?}"),
    }
}

#[test]
fn pack_added_monsters_spawn_in_ordinary_rooms_with_generation_filters() {
    use nethacked_data::roles::CharacterConfig;
    use nethacked_sim::SimulationWorld;
    use nethacked_types::BranchId;

    for peaceful in [false, true] {
        let dir = temp_pack_dir(&format!("ordinary-room-custom-{peaceful}"));
        write_vanilla_manifest(&dir, "ordinary-room-custom");
        // Isolate the custom candidates using authored patches, leaving every
        // vanilla definition available for world initialization and items.
        let mut monsters = String::new();
        for def in &Ruleset::vanilla().monsters {
            monsters.push_str(&format!(
                "[[monster]]\nname = {:?}\nfrequency = 0\n",
                def.name
            ));
        }
        for (name, frequency, difficulty, flags) in [
            ("dire jackal", 7, 2, "[]"),
            ("silent jackal", 0, 2, "[]"),
            ("forbidden jackal", 7, 2, "[\"nogen\"]"),
            ("overpowered jackal", 7, 99, "[]"),
        ] {
            monsters.push_str(&format!(
                r#"
[[monster]]
name = "{name}"
new = true
glyph = "d"
base_hp = 17
max_hp = 17
ac = -3
level = 2
speed = 19
alignment = "Neutral"
intrinsics = {{ fire_resistance = true }}
attacks = [{{ at = "Bite", ad = "Phys", n = 2, d = 4 }}]
size = "Medium"
peaceful_by_default = {peaceful}
always_hostile = false
maligntyp = 0
msound = "Other"
is_human = false
is_unique = false
mindless = false
ai_behavior = "MeleeHunter"
abilities = []
frequency = {frequency}
difficulty = {difficulty}
gen_flags = {flags}
"#
            ));
        }
        fs::write(dir.join("monsters.toml"), monsters).unwrap();
        let (pack, report) = build(&dir).unwrap();
        assert!(report.is_ok(), "{report:?}");
        let path = dir.join("custom.nhpack");
        write_nhpack(&pack, &path).unwrap();
        let (rules, reference) = load_nhpack(&path).unwrap();
        let def = rules.monster("dire jackal").unwrap();
        assert_eq!(def.id, None);
        assert_eq!(def.frequency, 7);
        assert!(!def.sample_hp);

        for exclusion in ["none", "species", "class"] {
            let mut spawned = 0;
            for seed in 0..8 {
                let mut world = SimulationWorld::new_with_character_and_ruleset(
                    seed,
                    CharacterConfig::default(),
                    rules.clone(),
                    reference.clone(),
                );
                // Discard the starting floor's population before requesting a
                // real new floor through the same generator used by stairs.
                let old_ids: Vec<_> = world
                    .arena
                    .actors
                    .iter()
                    .filter_map(|(id, _)| (id != world.player_id).then_some(id))
                    .collect();
                for id in old_ids {
                    world.arena.actors.remove(id);
                }
                match exclusion {
                    "species" => {
                        world
                            .genocide_registry
                            .genocided_species
                            .insert("dire jackal".into());
                    }
                    "class" => {
                        world.genocide_registry.genocided_classes.insert('d');
                    }
                    _ => {}
                }
                world.unpack_or_generate_level(BranchId::DungeonsOfDoom, 4);
                assert!(world.level.rooms.len() > 2);
                let actors: Vec<_> = world
                    .arena
                    .actors
                    .iter()
                    .filter(|(id, _)| *id != world.player_id)
                    .map(|(_, actor)| actor)
                    .collect();
                if exclusion != "none" {
                    assert!(actors.is_empty(), "{exclusion}, seed {seed}: {actors:?}");
                    continue;
                }
                for actor in actors {
                    spawned += 1;
                    assert_eq!(actor.name, "dire jackal");
                    assert_eq!((actor.hp, actor.max_hp), (17, 17));
                    assert_eq!((actor.level, actor.ac, actor.speed), (2, -3, 19));
                    assert!(actor.intrinsics.fire_resistance);
                    assert_eq!(actor.is_peaceful, peaceful);
                    assert_eq!(
                        actor.malign,
                        nethacked_core::calculate_malign(
                            0,
                            world.arena.actors.get(world.player_id).unwrap().alignment,
                            peaceful,
                            false,
                            peaceful,
                            false,
                        )
                    );
                    assert!(world.level.is_passable(actor.coord));
                }
            }
            if exclusion == "none" {
                assert!(
                    spawned > 0,
                    "custom species must spawn, peaceful={peaceful}"
                );
            }
        }
    }
}

#[test]
fn test_remove_monster() {
    let dir = temp_pack_dir("remove-monster");
    write_vanilla_manifest(&dir, "remove-monster");

    // Remove non-required monster (goblin)
    fs::write(
        dir.join("monsters.toml"),
        r#"
[[monster]]
name = "goblin"
remove = true
"#,
    )
    .unwrap();

    let (nhpack, report) = build(&dir).unwrap();
    assert!(report.is_ok());
    assert!(nhpack.ruleset.monster("goblin").is_none());

    // Remove required monster (shopkeeper) -> validation error
    let dir_req = temp_pack_dir("remove-shopkeeper");
    write_vanilla_manifest(&dir_req, "remove-shopkeeper");
    fs::write(
        dir_req.join("monsters.toml"),
        r#"
[[monster]]
name = "shopkeeper"
remove = true
"#,
    )
    .unwrap();

    let err = build(&dir_req).unwrap_err();
    match err {
        PackError::Invalid(report) => {
            let found = report
                .errors
                .iter()
                .any(|d| d.entry == "shopkeeper" && d.message.contains("engine-required"));
            assert!(found, "expected shopkeeper removal diagnostic in errors");
        }
        other => panic!("expected Invalid error, got {other:?}"),
    }
}

#[test]
fn test_unknown_field() {
    let dir = temp_pack_dir("unknown-field");
    write_vanilla_manifest(&dir, "unknown-field");

    fs::write(
        dir.join("monsters.toml"),
        r#"
[[monster]]
name = "jackal"
levle = 3
"#,
    )
    .unwrap();

    let err = read_pack_dir(&dir).unwrap_err();
    match err {
        PackError::Toml { file, message } => {
            assert_eq!(file, "monsters.toml");
            assert!(
                message.contains("levle") || message.contains("unknown field"),
                "message was: {message}"
            );
        }
        other => panic!("expected Toml error, got {other:?}"),
    }
}

#[test]
fn test_range_validation_rules() {
    let test_cases = [
        ("monsters.toml", "[[monster]]\nname = \"jackal\"\nlevel = 128", "level"),
        ("monsters.toml", "[[monster]]\nname = \"jackal\"\nspeed = 61", "speed"),
        ("monsters.toml", "[[monster]]\nname = \"jackal\"\nac = 21", "ac"),
        ("monsters.toml", "[[monster]]\nname = \"jackal\"\nac = -21", "ac"),
        ("monsters.toml", "[[monster]]\nname = \"jackal\"\nbase_hp = 0", "base_hp"),
        ("monsters.toml", "[[monster]]\nname = \"jackal\"\nmax_hp = 10001", "max_hp"),
        ("monsters.toml", "[[monster]]\nname = \"jackal\"\nbase_hp = 50\nmax_hp = 10", "base_hp"),
        ("monsters.toml", "[[monster]]\nname = \"jackal\"\nglyph = \"🦀\"", "glyph"),
        ("monsters.toml", "[[monster]]\nname = \"jackal\"\nattacks = [{ at = \"Bite\", ad = \"Phys\", n = 1, d = 0 }]", "attacks"),
        ("items.toml", "[[item]]\nname = \"dagger\"\nweight = 10001", "weight"),
        ("items.toml", "[[item]]\nname = \"dagger\"\ncost = 1000001", "cost"),
        ("items.toml", "[[item]]\nname = \"dagger\"\ndamage_small = [1, 0]", "damage_small"),
    ];

    for (file, content, expected_field) in test_cases {
        let dir = temp_pack_dir(&format!("range-{expected_field}"));
        write_vanilla_manifest(&dir, "range-test");
        fs::write(dir.join(file), content).unwrap();

        let res = build(&dir);
        assert!(
            res.is_err(),
            "expected error for {file} with field {expected_field}"
        );
        match res.unwrap_err() {
            PackError::Invalid(report) => {
                let found = report
                    .errors
                    .iter()
                    .any(|d| d.field.as_deref() == Some(expected_field));
                assert!(
                    found,
                    "expected diagnostic for field '{expected_field}' in errors: {:?}",
                    report.errors
                );
            }
            other => panic!("expected Invalid error for {expected_field}, got {other:?}"),
        }
    }
}

#[test]
fn test_hash_determinism_and_tamper_rejection() {
    let dir = temp_pack_dir("hash-test");
    write_vanilla_manifest(&dir, "hash-test");
    fs::write(
        dir.join("monsters.toml"),
        "[[monster]]\nname = \"jackal\"\nlevel = 3\n",
    )
    .unwrap();

    let (nhpack1, _) = build(&dir).unwrap();
    let hash1 = ruleset_hash(&nhpack1.ruleset);
    assert_eq!(nhpack1.hash, hash1);

    // Save nhpack
    let nhpack_file = dir.join("test.nhpack");
    write_nhpack(&nhpack1, &nhpack_file).unwrap();

    // Load nhpack successfully
    let (loaded_rs, loaded_ref) = load_nhpack(&nhpack_file).unwrap();
    assert_eq!(loaded_ref.hash, hash1);
    assert_eq!(loaded_rs.monster("jackal").unwrap().level, 3);

    // Tamper one value in the file
    let mut json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&nhpack_file).unwrap()).unwrap();
    // Tamper a monster value inside ruleset
    json["ruleset"]["monsters"][0]["level"] = serde_json::json!(42);
    fs::write(&nhpack_file, serde_json::to_string_pretty(&json).unwrap()).unwrap();

    let tamper_err = load_nhpack(&nhpack_file).unwrap_err();
    match tamper_err {
        PackError::Hash { expected, found } => {
            assert_eq!(expected, hash1);
            assert_ne!(found, hash1);
        }
        other => panic!("expected Hash mismatch error, got {other:?}"),
    }

    // Tamper format version
    json["format"] = serde_json::json!(2);
    fs::write(&nhpack_file, serde_json::to_string_pretty(&json).unwrap()).unwrap();

    let format_err = load_nhpack(&nhpack_file).unwrap_err();
    match format_err {
        PackError::Format(v) => assert_eq!(v, 2),
        other => panic!("expected Format error, got {other:?}"),
    }
}

#[test]
fn test_diff_exact_and_deterministic() {
    let vanilla = Ruleset::vanilla();
    assert!(diff(&vanilla, &vanilla).is_empty());

    let dir = temp_pack_dir("diff-test");
    write_vanilla_manifest(&dir, "vanilla"); // keep id same as vanilla for pure table diff
    fs::write(
        dir.join("monsters.toml"),
        "[[monster]]\nname = \"jackal\"\nlevel = 4\n",
    )
    .unwrap();

    let (nhpack, _) = build(&dir).unwrap();
    let diffs = diff(&vanilla, &nhpack.ruleset);

    // Filter to monsters diff
    let m_diffs: Vec<_> = diffs.iter().filter(|d| d.table == "monsters").collect();
    assert_eq!(m_diffs.len(), 1);
    assert_eq!(m_diffs[0].entry, "jackal");
    assert_eq!(m_diffs[0].field, "level");
    assert_eq!(m_diffs[0].after, "4");

    // Output order deterministic across 10 runs
    for _ in 0..10 {
        let repeat_diffs = diff(&vanilla, &nhpack.ruleset);
        assert_eq!(diffs, repeat_diffs);
    }
}

#[test]
fn test_i18n_warnings() {
    let dir = temp_pack_dir("i18n-warnings");
    write_vanilla_manifest(&dir, "i18n-test");
    fs::write(
        dir.join("monsters.toml"),
        r#"
[[monster]]
name = "dire jackal"
new = true
glyph = "d"
base_hp = 10
max_hp = 10
ac = 5
level = 2
speed = 14
alignment = "Neutral"
intrinsics = {}
attacks = [{ at = "Bite", ad = "Phys", n = 2, d = 4 }]
size = "Medium"
peaceful_by_default = false
always_hostile = true
maligntyp = 0
msound = "Other"
is_human = false
is_unique = false
mindless = false
ai_behavior = "MeleeHunter"
abilities = []
"#,
    )
    .unwrap();

    let i18n_dir = dir.join("i18n");
    fs::create_dir_all(&i18n_dir).unwrap();
    fs::write(
        i18n_dir.join("uk.toml"),
        "# missing dire jackal translation\n",
    )
    .unwrap();

    let (_, report) = build(&dir).unwrap();
    assert!(report.is_ok());
    assert_eq!(report.warnings.len(), 1);
    assert_eq!(report.warnings[0].file, "i18n/uk.toml");
    assert_eq!(report.warnings[0].entry, "dire jackal");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn prop_patch_monsters(
        lvl in 0u32..=49,
        ac in -20i32..=20,
        spd in 0u32..=60
    ) {
        let vanilla = Ruleset::vanilla();
        let pack_monsters = vec![nethacked_pack::MonsterPatch {
            name: "jackal".into(),
            level: Some(lvl),
            ac: Some(ac),
            speed: Some(spd),
            ..Default::default()
        }];

        let pack_dir = nethacked_pack::PackDir {
            manifest: nethacked_pack::PackToml {
                id: "prop-test".into(),
                name: "Prop Test".into(),
                version: "1.0.0".into(),
                base: "vanilla".into(),
                description: None,
            },
            monsters: pack_monsters,
            items: Vec::new(),
            roles: Vec::new(),
            i18n: std::collections::BTreeMap::new(),
        };

        let resolved = resolve(&pack_dir, &vanilla).expect("resolve must succeed");
        let rep = validate(&resolved);
        prop_assert!(rep.is_ok());

        let j = resolved.monster("jackal").unwrap();
        prop_assert_eq!(j.level, lvl);
        prop_assert_eq!(j.ac, ac);
        prop_assert_eq!(j.speed, spd);

        let d = diff(&vanilla, &resolved);
        let jackal_diffs: Vec<_> = d.iter().filter(|e| e.table == "monsters" && e.entry == "jackal").collect();
        for entry in jackal_diffs {
            prop_assert!(["level", "ac", "speed"].contains(&entry.field.as_str()));
        }
    }
}

#[test]
fn hp_override_policy_is_fixed_unless_explicitly_sampled() {
    let vanilla = Ruleset::vanilla();
    let dir = temp_pack_dir("hp-policy");
    write_vanilla_manifest(&dir, "hp-policy");
    for sampled in [false, true] {
        let extra = if sampled { "sample_hp = true\n" } else { "" };
        fs::write(
            dir.join("monsters.toml"),
            format!("[[monster]]\nname = \"jackal\"\nbase_hp = 11\nmax_hp = 17\n{extra}"),
        )
        .unwrap();
        let (built, report) = build(&dir).unwrap();
        assert!(report.is_ok());
        let def = built.ruleset.monster("jackal").unwrap();
        assert_eq!((def.base_hp, def.max_hp, def.sample_hp), (11, 17, sampled));
        assert!(vanilla.monster("jackal").unwrap().sample_hp);
    }
}

#[test]
fn old_fixed_hp_pack_loads_without_changing_its_hash() {
    let mut rules = (*Ruleset::vanilla()).clone();
    for def in &mut rules.monsters {
        def.sample_hp = false;
    }
    let bundle = nethacked_pack::NhPack {
        format: nethacked_pack::PACK_FORMAT_VERSION,
        manifest: rules.manifest.clone(),
        hash: ruleset_hash(&rules),
        ruleset: rules,
    };
    let bytes = nethacked_pack::nhpack_to_bytes(&bundle);
    // False is omitted, preserving the pre-sampling canonical representation.
    assert!(!std::str::from_utf8(&bytes).unwrap().contains("sample_hp"));
    let (loaded, reference, _) = nethacked_pack::load_nhpack_bytes(&bytes).unwrap();
    assert_eq!(reference.hash, bundle.hash);
    assert!(loaded.monsters.iter().all(|def| !def.sample_hp));
}

#[test]
fn old_item_profiles_load_fixed_without_changing_hash() {
    use nethacked_data::item_init::ItemInitProfile;
    let mut rules = (*Ruleset::vanilla()).clone();
    for item in &mut rules.items {
        item.initialization = ItemInitProfile::Fixed;
        item.generation_weight = 0;
    }
    let bundle = nethacked_pack::NhPack {
        format: nethacked_pack::PACK_FORMAT_VERSION,
        manifest: rules.manifest.clone(),
        hash: ruleset_hash(&rules),
        ruleset: rules,
    };
    let bytes = nethacked_pack::nhpack_to_bytes(&bundle);
    assert!(!std::str::from_utf8(&bytes)
        .unwrap()
        .contains("initialization"));
    let (loaded, reference, _) = nethacked_pack::load_nhpack_bytes(&bytes).unwrap();
    assert_eq!(reference.hash, bundle.hash);
    assert!(loaded.items.iter().all(|i| i.initialization.is_fixed()));
    assert!(loaded.items.iter().all(|i| i.generation_weight == 0));
    assert!(!std::str::from_utf8(&bytes)
        .unwrap()
        .contains("generation_weight"));
}

#[test]
fn item_profiles_are_explicit_custom_opt_in_and_class_validated() {
    use nethacked_data::item_init::ItemInitProfile;
    let dir = temp_pack_dir("item-profile");
    write_vanilla_manifest(&dir, "item-profile");
    let custom = r#"
[[item]]
name = "custom camera"
new = true
class = "Tool"
weight = 17
cost = 30
damage_small = [0, 0]
damage_large = [0, 0]
ac_bonus = 0
is_container = false
is_bag_of_holding = false
oc_magic = false
nutrition = 0
"#;
    fs::write(dir.join("items.toml"), custom).unwrap();
    let (fixed, report) = build(&dir).unwrap();
    assert!(report.is_ok());
    assert_eq!(
        fixed.ruleset.item("custom camera").unwrap().initialization,
        ItemInitProfile::Fixed
    );
    let item = fixed
        .ruleset
        .create_random_item_supported_fields_by_name(
            "custom camera",
            serde_json::from_str("\"Limbo\"").unwrap(),
            1,
            |_| panic!("custom fixed draw"),
        )
        .unwrap();
    assert_eq!((item.weight, item.enchantment), (17, 0));
    fs::write(
        dir.join("items.toml"),
        format!("{custom}initialization = {{ kind = \"tool\", charges = \"camera\" }}\n"),
    )
    .unwrap();
    let (sampled, report) = build(&dir).unwrap();
    assert!(report.is_ok());
    let item = sampled
        .ruleset
        .create_random_item_supported_fields_by_name(
            "custom camera",
            serde_json::from_str("\"Limbo\"").unwrap(),
            1,
            |n| {
                assert_eq!(n, 70);
                69
            },
        )
        .unwrap();
    assert_eq!((item.weight, item.enchantment), (17, 99));
    fs::write(
        dir.join("items.toml"),
        format!("{custom}initialization = {{ kind = \"wand\", charges = \"standard\" }}\n"),
    )
    .unwrap();
    assert!(build(&dir).is_err());
}

#[test]
fn container_profiles_validate_flags_and_generation_weight() {
    use nethacked_data::item_init::{ContainerInit, ItemInitProfile};
    use nethacked_data::ItemKindId;
    let mut rules = (*Ruleset::vanilla()).clone();
    assert!(nethacked_pack::validate(&rules).errors.is_empty());
    let index = rules
        .items
        .iter()
        .position(|d| d.id == Some(ItemKindId::SACK))
        .unwrap();
    rules.items[index].is_container = false;
    assert!(nethacked_pack::validate(&rules)
        .errors
        .iter()
        .any(|d| d.message.contains("coherent container")));
    rules.items[index].is_container = true;
    rules.items[index].initialization = ItemInitProfile::Container {
        container: ContainerInit::HoldingBag,
    };
    assert!(nethacked_pack::validate(&rules)
        .errors
        .iter()
        .any(|d| d.message.contains("coherent container")));
    rules.items[index].is_bag_of_holding = true;
    rules.items[index].generation_weight = 10001;
    assert!(nethacked_pack::validate(&rules)
        .errors
        .iter()
        .any(|d| d.message.contains("generation_weight")));
}

#[test]
fn old_corpse_body_facts_load_defaults_and_preserve_canonical_hash() {
    let mut rules = (*Ruleset::vanilla()).clone();
    for def in &mut rules.monsters {
        def.body_weight = 0;
        def.corpse_nutrition = 0;
    }
    let bundle = nethacked_pack::NhPack {
        format: nethacked_pack::PACK_FORMAT_VERSION,
        manifest: rules.manifest.clone(),
        hash: ruleset_hash(&rules),
        ruleset: rules,
    };
    let bytes = nethacked_pack::nhpack_to_bytes(&bundle);
    let json = std::str::from_utf8(&bytes).unwrap();
    assert!(!json.contains("body_weight") && !json.contains("corpse_nutrition"));
    let (loaded, reference, _) = nethacked_pack::load_nhpack_bytes(&bytes).unwrap();
    assert_eq!(reference.hash, bundle.hash);
    assert!(loaded
        .monsters
        .iter()
        .all(|m| m.body_weight == 0 && m.corpse_nutrition == 0));
    let mut changed = (*loaded).clone();
    changed.monsters[0].body_weight = 19;
    assert_ne!(ruleset_hash(&changed), reference.hash);
    changed.monsters[0].body_weight = 0;
    changed.monsters[0].corpse_nutrition = 20;
    assert_ne!(ruleset_hash(&changed), reference.hash);
}

#[test]
fn artifact_metadata_rejects_invalid_identity_and_duplicate_at_pack_boundary() {
    let mut rs = (*Ruleset::vanilla()).clone();
    assert!(validate(&rs).errors.is_empty());
    rs.artifacts[0].id = "ART_FAKED".into();
    assert!(validate(&rs)
        .errors
        .iter()
        .any(|d| d.field.as_deref() == Some("artifacts")));
    let mut rs = (*Ruleset::vanilla()).clone();
    rs.artifacts.push(rs.artifacts[0].clone());
    assert!(validate(&rs)
        .errors
        .iter()
        .any(|d| d.field.as_deref() == Some("artifacts")));
    let mut rs = (*Ruleset::vanilla()).clone();
    rs.artifacts[0].base = rs.artifacts[0].item;
    assert!(validate(&rs)
        .errors
        .iter()
        .any(|d| d.field.as_deref() == Some("artifacts")));
}

#[test]
fn old_compiled_ruleset_preserves_canonical_hash_when_new_metadata_is_absent() {
    let mut value = serde_json::to_value(&*Ruleset::vanilla()).unwrap();
    value.as_object_mut().unwrap().remove("artifacts");
    for item in value["items"].as_array_mut().unwrap() {
        item.as_object_mut().unwrap().remove("unique");
    }
    let old_json = serde_json::to_vec(&value).unwrap();
    let rs: Ruleset = serde_json::from_slice(&old_json).unwrap();
    assert_eq!(
        serde_json::to_vec(&serde_json::to_value(&rs).unwrap()).unwrap(),
        old_json
    );
}

#[test]
fn old_compiled_artifact_metadata_defaults_load_with_verified_hash() {
    let mut value = serde_json::to_value(&*Ruleset::vanilla()).unwrap();
    value.as_object_mut().unwrap().remove("artifacts");
    for item in value["items"].as_array_mut().unwrap() {
        item.as_object_mut().unwrap().remove("unique");
    }
    let old: Ruleset = serde_json::from_value(value.clone()).unwrap();
    let hash = ruleset_hash(&old);
    let packed =
        serde_json::json!({"format":1,"manifest":value["manifest"],"ruleset":value,"hash":hash});
    let (loaded, reference, _) =
        nethacked_pack::load_nhpack_bytes(&serde_json::to_vec(&packed).unwrap()).unwrap();
    assert_eq!(reference.hash, hash);
    assert!(loaded.artifacts.is_empty());
    assert_eq!(
        loaded.artifact_id_for_item("Excalibur"),
        Some("ART_EXCALIBUR")
    );
}
