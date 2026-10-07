use nethacked_data::monsters::{CorpseThawPolicy as Policy, MonsterSpeciesId};
use nethacked_data::Ruleset;

#[test]
fn extracted_exception_is_exact_and_owned_for_every_species() {
    let rules = Ruleset::vanilla();
    let exceptions: Vec<_> = rules
        .monsters
        .iter()
        .filter(|m| m.corpse_thaw_policy != Policy::Ordinary)
        .map(|m| (m.id.unwrap().c_name(), m.corpse_thaw_policy))
        .collect();
    assert_eq!(exceptions, vec![("ICE_TROLL", Policy::IceTroll)]);
}

#[test]
fn legacy_omission_defaults_and_owned_overrides_survive_identity_and_names() {
    let mut def = Ruleset::vanilla()
        .monster_by_id(MonsterSpeciesId::ICE_TROLL)
        .unwrap()
        .clone();
    let mut old = serde_json::to_value(&def).unwrap();
    old.as_object_mut().unwrap().remove("corpse_thaw_policy");
    let loaded: nethacked_data::ruleset::MonsterDef = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(loaded.corpse_thaw_policy, Policy::Ordinary);
    assert_eq!(serde_json::to_value(&loaded).unwrap(), old);
    def.corpse_thaw_policy = Policy::Ordinary;
    def.name = "renamed frost species".into();
    assert_eq!(def.corpse_thaw_policy, Policy::Ordinary);
    def.id = None;
    def.corpse_thaw_policy = Policy::IceTroll;
    let back: nethacked_data::ruleset::MonsterDef =
        serde_json::from_value(serde_json::to_value(&def).unwrap()).unwrap();
    assert_eq!(back, def);
}
