use nethacked_data::monsters::{CorpseTimeoutPolicy as Policy, MonsterSpeciesId};
use nethacked_data::ruleset::MonsterDef;
use nethacked_data::Ruleset;

#[test]
fn pinned_c_timeout_identity_predicates_and_class_are_retained() {
    // NetHack 16ff59115315917b93185d026aeefea06db9b0f4:
    // mkobj.c:1403-1406 returns for PM_LIZARD/PM_LICHEN before any RNG;
    // mkobj.c:1417-1420 tests is_rider before mlet == S_TROLL;
    // mondata.h:161-163 defines exactly Death, Famine and Pestilence.
    let rules = Ruleset::vanilla();
    let policies: Vec<_> = rules
        .monsters
        .iter()
        .filter(|m| m.corpse_timeout_policy != Policy::Ordinary)
        .map(|m| (m.id.unwrap().c_name(), m.corpse_timeout_policy))
        .collect();
    assert_eq!(
        policies,
        vec![
            ("LICHEN", Policy::Never),
            ("DEATH", Policy::Rider),
            ("PESTILENCE", Policy::Rider),
            ("FAMINE", Policy::Rider),
            ("LIZARD", Policy::Never),
        ]
    );
    // defsym.h:347 maps S_TROLL to 'T'; the extractor maps MON mlet via
    // MONSYM, not display-name matching. These are all source class members.
    let trolls: Vec<_> = rules
        .monsters
        .iter()
        .filter(|m| m.glyph == 'T')
        .map(|m| m.id.unwrap().c_name())
        .collect();
    assert_eq!(
        trolls,
        [
            "TROLL",
            "ICE_TROLL",
            "ROCK_TROLL",
            "WATER_TROLL",
            "OLOG_HAI"
        ]
    );
    for id in [MonsterSpeciesId::LICHEN, MonsterSpeciesId::LIZARD] {
        assert_eq!(
            rules.monster_by_id(id).unwrap().corpse_timeout_policy,
            Policy::Never
        );
    }
}

#[test]
fn owned_policy_and_class_survive_renaming_custom_identity_and_id_overrides() {
    let mut rules = (*Ruleset::vanilla()).clone();
    for m in &mut rules.monsters {
        if m.id == Some(MonsterSpeciesId::LIZARD) {
            m.name = "renamed everlasting creature".into();
            m.corpse_timeout_policy = Policy::Rider;
        } else if m.id == Some(MonsterSpeciesId::TROLL) {
            m.name = "renamed ordinary creature".into();
            m.glyph = 'x';
            m.corpse_timeout_policy = Policy::Never;
        }
    }
    let mut custom = rules.monster("jackal").unwrap().clone();
    custom.id = None;
    custom.name = "custom everlasting creature".into();
    custom.glyph = 'T';
    custom.corpse_timeout_policy = Policy::Never;
    rules.monsters.push(custom);
    rules.reindex();
    let renamed = rules.monster_by_id(MonsterSpeciesId::LIZARD).unwrap();
    assert_eq!(renamed.corpse_timeout_policy, Policy::Rider);
    assert_eq!(rules.monster(&renamed.name).unwrap(), renamed);
    let troll = rules.monster_by_id(MonsterSpeciesId::TROLL).unwrap();
    assert_eq!(
        (troll.glyph, troll.corpse_timeout_policy),
        ('x', Policy::Never)
    );
    let custom = rules.monster("custom everlasting creature").unwrap();
    assert_eq!(
        (custom.glyph, custom.corpse_timeout_policy),
        ('T', Policy::Never)
    );
    assert_eq!(custom.id, None);
    let encoded = serde_json::to_vec(custom).unwrap();
    assert_eq!(
        serde_json::from_slice::<MonsterDef>(&encoded).unwrap(),
        *custom
    );
}

#[test]
fn absent_policy_defaults_without_canonical_id_or_name_fallback() {
    for name in ["lizard", "lichen", "Death", "troll", "jackal"] {
        let mut value = serde_json::to_value(Ruleset::vanilla().monster(name).unwrap()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("corpse_timeout_policy");
        let loaded: MonsterDef = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(loaded.corpse_timeout_policy, Policy::Ordinary);
        assert_eq!(serde_json::to_value(loaded).unwrap(), value);
    }
    assert!(serde_json::from_str::<Policy>("\"unsupported\"").is_err());
}
