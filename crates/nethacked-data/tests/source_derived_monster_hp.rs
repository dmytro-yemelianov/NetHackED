#[path = "support/source_derived.rs"]
mod source_derived;

use nethacked_data::monsters::{compute_monster_hp, MonsterSpawnContext};
use nethacked_data::Ruleset;
use nethacked_types::Coord;
use rand::{Rng, RngCore};
use serde::Deserialize;
use source_derived::{DrawBlock, PrescribedRng};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expected {
    hp: u32,
    max_hp: u32,
    level: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    monster: String,
    catalog_level: u32,
    definition_level: u32,
    draws: Vec<DrawBlock>,
    expected: Expected,
    derivation: String,
}

fn cases() -> Vec<Case> {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/newmonhp_ordinary_v1.json")).unwrap();
    assert_eq!(fixture["schema_version"], 1);
    assert_eq!(fixture["evidence_kind"], "source-derived");
    assert_eq!(fixture["upstream"]["tag"], "NetHack-5.0.0_Released");
    serde_json::from_value(fixture["cases"].clone()).unwrap()
}

#[test]
fn c_prescribed_ordinary_hp_cases() {
    let rules = Ruleset::vanilla();
    for case in cases() {
        let mut def = rules.monster(&case.monster).unwrap().clone();
        assert_eq!(def.level, case.catalog_level, "{} catalog", case.id);
        def.level = case.definition_level;
        let mut rng = PrescribedRng::new(&case.draws);
        assert_eq!(
            compute_monster_hp(&def, MonsterSpawnContext::default(), &mut rng),
            case.expected.hp,
            "{}: {}",
            case.id,
            case.derivation
        );
        rng.assert_exhausted();
    }
}

#[test]
fn public_named_factory_obeys_source_expected_state_and_draws() {
    for case in cases() {
        let mut rules = (*Ruleset::vanilla()).clone();
        let def = rules
            .monsters
            .iter_mut()
            .find(|def| def.name == case.monster)
            .unwrap();
        assert_eq!(def.level, case.catalog_level, "{} catalog", case.id);
        assert!(def.sample_hp, "{} vanilla sampling policy", case.id);
        def.level = case.definition_level;
        // HP-only fixture: fixed owned sex adds no draw after its prescribed HP.
        def.flags.push("male".into());
        rules.reindex();
        let coord = Coord::new_unchecked(11, 7);
        let mut rng = PrescribedRng::new(&case.draws);
        let actor = rules
            .create_monster_record(&case.monster, coord, &mut rng)
            .unwrap();
        assert_eq!(
            (actor.hp, actor.max_hp, actor.level),
            (case.expected.hp, case.expected.max_hp, case.expected.level),
            "{}: {}",
            case.id,
            case.derivation
        );
        assert_eq!(actor.coord, coord);
        rng.assert_exhausted();
    }
}

#[test]
fn prescribed_draw_adapter_failure_controls() {
    let block = [DrawBlock {
        bound: 8,
        values: vec![7],
    }];
    // A d4 substitution cannot satisfy the independently prescribed d8 case.
    let case = cases()
        .into_iter()
        .find(|case| case.id == "two-dice-mixed-endpoints")
        .unwrap();
    let mut wrong_bound = PrescribedRng::new(&case.draws);
    let wrong_hp = (0..case.definition_level)
        .map(|_| wrong_bound.random_range(1..=4u32))
        .sum::<u32>();
    assert_ne!(wrong_hp, case.expected.hp);
    wrong_bound.assert_exhausted();
    assert!(std::panic::catch_unwind(|| PrescribedRng::new(&block).assert_exhausted()).is_err());
    assert!(std::panic::catch_unwind(|| {
        let mut rng = PrescribedRng::new(&block);
        rng.next_u32();
        rng.next_u32();
    })
    .is_err());
    assert!(std::panic::catch_unwind(|| PrescribedRng::new(&block).next_u64()).is_err());
}
