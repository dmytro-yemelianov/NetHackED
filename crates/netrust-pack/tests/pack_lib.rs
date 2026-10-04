//! Integration tests for netrust-pack: format, merge, validation, hash, diff, and integrity.

use std::fs;
use std::path::{Path, PathBuf};

use netrust_data::ruleset::Ruleset;
use netrust_pack::{
    build, diff, load_nrpack, read_pack_dir, resolve, ruleset_hash, validate, write_nrpack,
    PackError,
};
use proptest::prelude::*;

fn temp_pack_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("netrust-pack-test-{name}"));
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

    let (nrpack, report) = build(&dir).unwrap();
    assert!(report.is_ok());

    let jackal = nrpack.ruleset.monster("jackal").expect("jackal exists");
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

    let (nrpack, report) = build(&dir).unwrap();
    assert!(report.is_ok());

    let dire = nrpack
        .ruleset
        .monster("dire jackal")
        .expect("dire jackal exists");
    assert_eq!(dire.id, None);
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

    let (nrpack, report) = build(&dir).unwrap();
    assert!(report.is_ok());
    assert!(nrpack.ruleset.monster("goblin").is_none());

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
        ("monsters.toml", "[[monster]]\nname = \"jackal\"\nlevel = 50", "level"),
        ("monsters.toml", "[[monster]]\nname = \"jackal\"\nspeed = 61", "speed"),
        ("monsters.toml", "[[monster]]\nname = \"jackal\"\nac = 21", "ac"),
        ("monsters.toml", "[[monster]]\nname = \"jackal\"\nac = -21", "ac"),
        ("monsters.toml", "[[monster]]\nname = \"jackal\"\nbase_hp = 0", "base_hp"),
        ("monsters.toml", "[[monster]]\nname = \"jackal\"\nmax_hp = 10001", "max_hp"),
        ("monsters.toml", "[[monster]]\nname = \"jackal\"\nbase_hp = 50\nmax_hp = 10", "base_hp"),
        ("monsters.toml", "[[monster]]\nname = \"jackal\"\nglyph = \"🦀\"", "glyph"),
        ("monsters.toml", "[[monster]]\nname = \"jackal\"\nattacks = [{ at = \"Bite\", ad = \"Phys\", n = 0, d = 2 }]", "attacks"),
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

    let (nrpack1, _) = build(&dir).unwrap();
    let hash1 = ruleset_hash(&nrpack1.ruleset);
    assert_eq!(nrpack1.hash, hash1);

    // Save nrpack
    let nrpack_file = dir.join("test.nrpack");
    write_nrpack(&nrpack1, &nrpack_file).unwrap();

    // Load nrpack successfully
    let (loaded_rs, loaded_ref) = load_nrpack(&nrpack_file).unwrap();
    assert_eq!(loaded_ref.hash, hash1);
    assert_eq!(loaded_rs.monster("jackal").unwrap().level, 3);

    // Tamper one value in the file
    let mut json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&nrpack_file).unwrap()).unwrap();
    // Tamper a monster value inside ruleset
    json["ruleset"]["monsters"][0]["level"] = serde_json::json!(42);
    fs::write(&nrpack_file, serde_json::to_string_pretty(&json).unwrap()).unwrap();

    let tamper_err = load_nrpack(&nrpack_file).unwrap_err();
    match tamper_err {
        PackError::Hash { expected, found } => {
            assert_eq!(expected, hash1);
            assert_ne!(found, hash1);
        }
        other => panic!("expected Hash mismatch error, got {other:?}"),
    }

    // Tamper format version
    json["format"] = serde_json::json!(2);
    fs::write(&nrpack_file, serde_json::to_string_pretty(&json).unwrap()).unwrap();

    let format_err = load_nrpack(&nrpack_file).unwrap_err();
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

    let (nrpack, _) = build(&dir).unwrap();
    let diffs = diff(&vanilla, &nrpack.ruleset);

    // Filter to monsters diff
    let m_diffs: Vec<_> = diffs.iter().filter(|d| d.table == "monsters").collect();
    assert_eq!(m_diffs.len(), 1);
    assert_eq!(m_diffs[0].entry, "jackal");
    assert_eq!(m_diffs[0].field, "level");
    assert_eq!(m_diffs[0].after, "4");

    // Output order deterministic across 10 runs
    for _ in 0..10 {
        let repeat_diffs = diff(&vanilla, &nrpack.ruleset);
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
        let pack_monsters = vec![netrust_pack::MonsterPatch {
            name: "jackal".into(),
            level: Some(lvl),
            ac: Some(ac),
            speed: Some(spd),
            ..Default::default()
        }];

        let pack_dir = netrust_pack::PackDir {
            manifest: netrust_pack::PackToml {
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
