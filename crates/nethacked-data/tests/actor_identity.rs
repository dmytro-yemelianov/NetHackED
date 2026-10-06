use nethacked_arena::{ActorRecord, ActorSpecies, CorpseSpecies, ItemLocation};
use nethacked_data::{create_ghost_record, MonsterSpeciesId as M, Ruleset};
use nethacked_types::Coord;
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};

const AT: Coord = Coord::new_unchecked(10, 10);

#[test]
fn owned_factories_keep_canonical_and_exact_custom_identity_and_fixed_hp() {
    let mut rules = (*Ruleset::vanilla()).clone();
    let canonical = rules
        .monsters
        .iter_mut()
        .find(|m| m.id == Some(M::GIANT_ANT))
        .unwrap();
    canonical.name = "Owned ant".into();
    canonical.sample_hp = false;
    canonical.base_hp = 71;
    canonical.max_hp = 83;
    let mut custom = canonical.clone();
    custom.id = None;
    custom.name = "Ghost of PACK Beast".into();
    custom.base_hp = 91;
    rules.monsters.push(custom.clone());
    rules.reindex();
    let mut rng = StdRng::seed_from_u64(9);
    let mut untouched = rng.clone();
    let mut canonical = rules
        .create_monster_record_by_id(M::GIANT_ANT, AT, &mut rng)
        .unwrap();
    assert_eq!(
        canonical.species,
        Some(ActorSpecies::Vanilla("GIANT_ANT".into()))
    );
    assert_eq!((canonical.hp, canonical.max_hp), (71, 83));
    canonical.name = "red dragon".into();
    assert_eq!(
        rules.monster_for_actor(&canonical).unwrap().name,
        "Owned ant"
    );
    let mut custom_actor =
        rules.create_monster_record_from_def(&custom, AT, Default::default(), &mut rng);
    assert_eq!(
        custom_actor.species,
        Some(ActorSpecies::Custom("ghost of pack beast".into()))
    );
    custom_actor.name = "Owned ant".into();
    assert_eq!(rules.monster_for_actor(&custom_actor), Some(&custom));
    assert_eq!((custom_actor.hp, custom_actor.max_hp), (91, 83));
    assert_eq!(rng.next_u64(), untouched.next_u64());
    let corpse = rules
        .create_corpse_record_for_actor(&canonical, ItemLocation::Floor(AT))
        .unwrap();
    assert_eq!(
        corpse.corpse_species,
        Some(CorpseSpecies::Vanilla("GIANT_ANT".into()))
    );
    let corpse = rules
        .create_corpse_record_for_actor(&custom_actor, ItemLocation::Floor(AT))
        .unwrap();
    assert_eq!(
        corpse.corpse_species,
        Some(CorpseSpecies::Custom(custom.name.clone()))
    );
    rules
        .monsters
        .retain(|m| m.id != Some(M::GIANT_ANT) && m.name != custom.name);
    rules.reindex();
    assert!(rules.monster_for_actor(&canonical).is_none());
    assert!(rules.monster_for_actor(&custom_actor).is_none());
    canonical.species = Some(ActorSpecies::Vanilla("MISSING".into()));
    assert!(rules.monster_for_actor(&canonical).is_none());
    custom_actor.species = Some(ActorSpecies::Custom("red dragon".into()));
    assert!(rules.monster_for_actor(&custom_actor).is_none()); // custom cannot become canonical
}

#[test]
fn legacy_load_omits_identity_and_preserves_canonical_json_and_decorations() {
    let rules = Ruleset::vanilla();
    let mut rng = StdRng::seed_from_u64(3);
    let actor = rules
        .create_monster_record("giant ant", AT, &mut rng)
        .unwrap();
    let mut old = serde_json::to_value(actor).unwrap();
    old.as_object_mut().unwrap().remove("species");
    old["name"] = "hostile giant ant".into();
    let canonical = serde_json::to_vec(&old).unwrap();
    let actor: ActorRecord = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(actor.species, None);
    assert_eq!(
        rules.monster_for_actor(&actor).unwrap().id,
        Some(M::GIANT_ANT)
    );
    assert_eq!(
        serde_json::to_vec(&serde_json::to_value(actor).unwrap()).unwrap(),
        canonical
    );
    old["name"] = "ghost of old hero".into();
    let actor: ActorRecord = serde_json::from_value(old).unwrap();
    assert_eq!(rules.monster_for_actor(&actor).unwrap().id, Some(M::GHOST));
    let ghost = create_ghost_record("old hero", 7, 61, AT);
    assert_eq!(ghost.species, Some(ActorSpecies::Vanilla("GHOST".into())));
    assert_eq!((ghost.level, ghost.hp, ghost.max_hp), (7, 61, 61));
}
