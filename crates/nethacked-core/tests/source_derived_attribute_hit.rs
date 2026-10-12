use nethacked_core::combat::{ordinary_hero_attribute_hit_bonus, to_hit_value};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    strength: i32,
    dexterity: i32,
    level: u32,
    expected: i32,
}

#[test]
fn ordinary_abon_source_derived_boundaries() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/abon_ordinary_v1.json")).unwrap();
    assert_eq!(fixture["schema_version"], 1);
    assert_eq!(fixture["evidence_kind"], "source-derived");
    assert_eq!(fixture["upstream"]["tag"], "NetHack-5.0.0_Released");
    let cases: Vec<Case> = serde_json::from_value(fixture["cases"].clone()).unwrap();
    for case in cases {
        assert_eq!(
            ordinary_hero_attribute_hit_bonus(case.strength, case.dexterity, case.level),
            case.expected,
            "{}",
            case.id
        );
    }
}

#[test]
fn abon_low_level_tuning_does_not_duplicate_experience_level() {
    // C find_roll_to_hit: 1 + abon + AC + ulevel (neutral other terms).
    for (level, expected) in [(1, 13), (2, 14), (3, 14)] {
        assert_eq!(
            to_hit_value(level, 0, 0, 0, 10)
                + ordinary_hero_attribute_hit_bonus(10, 10, level as u32),
            expected
        );
    }
}
