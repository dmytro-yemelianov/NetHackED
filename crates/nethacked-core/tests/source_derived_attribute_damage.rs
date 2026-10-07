use nethacked_core::combat::{melee_damage, ordinary_hero_strength_damage_bonus};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    strength: i32,
    expected: i32,
}

#[test]
fn ordinary_dbon_source_derived_boundaries() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/dbon_ordinary_v1.json")).unwrap();
    assert_eq!(fixture["schema_version"], 1);
    assert_eq!(fixture["evidence_kind"], "source-derived");
    assert_eq!(fixture["upstream"]["tag"], "NetHack-5.0.0_Released");
    let cases: Vec<Case> = serde_json::from_value(fixture["cases"].clone()).unwrap();
    for case in cases {
        assert_eq!(
            ordinary_hero_strength_damage_bonus(case.strength),
            case.expected,
            "{}",
            case.id
        );
    }
}

#[test]
fn strength_combines_with_enchantment_and_skill_before_damage_floor() {
    assert_eq!(
        melee_damage(1, 0, ordinary_hero_strength_damage_bonus(3)),
        1
    );
    assert_eq!(
        melee_damage(2, 0, ordinary_hero_strength_damage_bonus(3)),
        1
    );
    assert_eq!(
        melee_damage(2, 1, ordinary_hero_strength_damage_bonus(3)),
        2
    );
    assert_eq!(
        melee_damage(3, -5, 2 + ordinary_hero_strength_damage_bonus(18)),
        2
    );
    assert_eq!(
        melee_damage(4, 2, 1 + ordinary_hero_strength_damage_bonus(118)),
        13
    );
}
