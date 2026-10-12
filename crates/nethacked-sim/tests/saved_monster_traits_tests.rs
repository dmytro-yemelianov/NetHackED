//! Detached metadata only: source-only NetHack 5.0 mkobj.c:2157–2222.
use nethacked_arena::{
    ActorRecord, ActorSex, ActorSpecies, CorpseSpecies, ItemLocation, SavedMonsterTraits,
};
use nethacked_data::{ItemKindId as K, Ruleset};
use nethacked_sim::{SimulationWorld, StoredLevel};
use nethacked_types::{Alignment, Buc, Coord, Intrinsics, MonsterAbility, MonsterSpell};
use rand::RngCore;

fn troll() -> ActorRecord {
    ActorRecord {
        sex: Some(ActorSex::Female),
        species: Some(ActorSpecies::Vanilla("TROLL".into())),
        name: "Individual troll".into(),
        coord: Coord::new_unchecked(12, 11),
        hp: 0,
        max_hp: 3,
        ac: -2,
        level: 13,
        speed: 18,
        movement: 0,
        alignment: Alignment::Chaotic,
        intrinsics: Intrinsics {
            fire_resistance: true,
            cold_resistance: true,
            ..Intrinsics::default()
        },
        is_player: false,
        is_unique: true,
        is_dead: true,
        is_tame: true,
        tameness: 19,
        abilities: vec![MonsterAbility::Spellcaster {
            spell: MonsterSpell::CauseWounds,
            cooldown_turns: 17,
        }],
        is_peaceful: true,
        mspec_used: 7,
        malign: -4,
        is_fleeing: true,
        is_cancelled: true,
    }
}

#[test]
fn ordinary_troll_full_supported_snapshot_and_clone_are_detached() {
    let mut source = troll();
    let rules = Ruleset::vanilla();
    let def = rules.monster("troll").unwrap();
    assert_eq!(def.level, 7);
    assert_eq!(def.glyph, 'T');
    let traits = SavedMonsterTraits::from_actor(&source, def.level);
    let mut expected = troll();
    expected.max_hp = 8; // pinned C: drained maximum <= base level becomes base + 1
    assert_eq!(traits.actor, expected);
    assert_eq!((source.hp, source.max_hp, source.is_dead), (0, 3, true));
    source.name = "Changed source".into();
    source.species = None;
    source.sex = None;
    source.abilities.clear();
    source.intrinsics.fire_resistance = false;
    assert_eq!(traits.actor, expected);
    let mut independent = traits.clone();
    independent.actor.name.clear();
    independent.actor.abilities.clear();
    assert_eq!(traits.actor, expected);
    assert_ne!(independent, traits);
    let encoded = serde_json::to_value(&traits).unwrap();
    for forbidden in [
        "actor_id",
        "inventory",
        "weapon",
        "scheduler",
        "mextra",
        "items",
    ] {
        assert!(encoded.get(forbidden).is_none());
        assert!(encoded["actor"].get(forbidden).is_none());
    }
    assert_eq!(
        serde_json::from_value::<SavedMonsterTraits>(encoded).unwrap(),
        traits
    );
}

#[test]
fn literal_hp_boundaries_keep_zero_dead_and_clamp_excess() {
    for (base, hp, max, expected_hp, expected_max) in [
        (7, 0, 3, 0, 8),
        (7, 4, 7, 4, 8),
        (7, 20, 7, 8, 8),
        (7, 20, 8, 8, 8),
        (7, 5, 12, 5, 12),
        (0, 0, 0, 0, 1),
        (0, 9, 0, 1, 1),
        (0, 9, 4, 4, 4),
    ] {
        let mut actor = troll();
        actor.hp = hp;
        actor.max_hp = max;
        let traits = SavedMonsterTraits::from_actor(&actor, base);
        assert_eq!(
            (traits.actor.hp, traits.actor.max_hp),
            (expected_hp, expected_max)
        );
        assert!(traits.actor.is_dead);
        assert_eq!((actor.hp, actor.max_hp), (hp, max));
    }
}

#[test]
fn payload_identity_and_unknown_sex_are_independent_of_corpse_and_display() {
    let rules = Ruleset::vanilla();
    let mut actor = troll();
    actor.species = Some(ActorSpecies::Custom("owned troll".into()));
    actor.sex = None;
    actor.name = "Medusa".into();
    let traits = SavedMonsterTraits::from_actor(&actor, 7);
    let mut corpse = rules
        .create_item_record_by_id(K::CORPSE, ItemLocation::Limbo, Buc::Uncursed)
        .unwrap();
    assert!(corpse.saved_monster_traits.is_none());
    corpse.saved_monster_traits = Some(Box::new(traits.clone()));
    corpse.corpse_species = Some(CorpseSpecies::Vanilla("JACKAL".into()));
    corpse.name = "red dragon corpse".into();
    corpse.enchantment = 2;
    let roundtrip: nethacked_arena::ItemRecord =
        serde_json::from_value(serde_json::to_value(&corpse).unwrap()).unwrap();
    assert_eq!(roundtrip, corpse);
    assert_eq!(roundtrip.saved_monster_traits.as_deref(), Some(&traits));
    assert_eq!(
        traits.actor.species,
        Some(ActorSpecies::Custom("owned troll".into()))
    );
    assert_eq!(traits.actor.sex, None);
}

#[test]
fn full_active_cached_save_preserves_snapshot_after_source_and_inventory_removal() {
    let mut world = SimulationWorld::new_with_seed(91);
    let source = world.arena.spawn_actor(troll());
    let carried = world
        .ruleset
        .create_item_record_by_id(K::DAGGER, ItemLocation::CarriedBy(source), Buc::Uncursed)
        .unwrap();
    let weapon = world.arena.spawn_item(carried);
    let def = world.ruleset.monster("troll").unwrap();
    let traits = SavedMonsterTraits::from_actor(&world.arena.actors[source], def.level);
    let mut corpse = world
        .ruleset
        .create_item_record_by_id(K::CORPSE, ItemLocation::Limbo, Buc::Uncursed)
        .unwrap();
    corpse.saved_monster_traits = Some(Box::new(traits.clone()));
    corpse.corpse_species = Some(CorpseSpecies::Vanilla("TROLL".into()));
    corpse.location = ItemLocation::Floor(world.hero_coord());
    let active = world.arena.spawn_item(corpse.clone());
    let cached_id = world.arena.spawn_item(corpse);
    let cached = world.arena.items.remove(cached_id).unwrap();
    let key = (world.current_branch, world.depth + 1);
    world.stored_levels.push((
        key,
        StoredLevel {
            level: world.level.clone(),
            monsters: vec![],
            items: vec![(cached_id, cached)],
            unpaid_items: vec![],
        },
    ));
    world.arena.items.remove(weapon);
    world.arena.actors.remove(source);
    let mut expected_rng = world.rng.clone();
    let save = world.to_save_json().unwrap();
    let mut loaded =
        SimulationWorld::from_save_json(&save, world.ruleset.clone(), &world.ruleset_ref).unwrap();
    assert_eq!(
        serde_json::to_value(&world).unwrap(),
        serde_json::to_value(&loaded).unwrap()
    );
    assert_eq!(
        loaded.arena.items[active].saved_monster_traits.as_deref(),
        Some(&traits)
    );
    assert_eq!(
        loaded.stored_levels[0].1.items[0]
            .1
            .saved_monster_traits
            .as_deref(),
        Some(&traits)
    );
    assert!(loaded.arena.actors.get(source).is_none());
    assert!(loaded.arena.items.get(weapon).is_none());
    assert_eq!(
        loaded.unpack_or_generate_level(key.0, key.1),
        world.unpack_or_generate_level(key.0, key.1)
    );
    let restored = loaded
        .arena
        .items
        .values()
        .filter(|i| i.saved_monster_traits.is_some())
        .collect::<Vec<_>>();
    assert_eq!(restored.len(), 2);
    assert!(restored
        .iter()
        .all(|i| i.saved_monster_traits.as_deref() == Some(&traits)));
    assert_eq!(
        serde_json::to_value(&world).unwrap(),
        serde_json::to_value(&loaded).unwrap()
    );
    let expected_word = expected_rng.next_u64();
    assert_eq!(loaded.rng.next_u64(), expected_word);
    assert_eq!(world.rng.next_u64(), expected_word);
}

#[test]
fn legacy_full_save_keeps_active_cached_corpse_payloads_absent_without_rng() {
    let mut world = SimulationWorld::new_with_seed(11);
    let mut corpse = world
        .ruleset
        .create_item_record_by_id(K::CORPSE, ItemLocation::Limbo, Buc::Uncursed)
        .unwrap();
    corpse.corpse_species = Some(CorpseSpecies::Vanilla("TROLL".into()));
    let active = world.arena.spawn_item(corpse.clone());
    let cached_id = world.arena.spawn_item(corpse);
    let cached = world.arena.items.remove(cached_id).unwrap();
    let key = (world.current_branch, world.depth + 1);
    world.stored_levels.push((
        key,
        StoredLevel {
            level: world.level.clone(),
            monsters: vec![],
            items: vec![(cached_id, cached)],
            unpaid_items: vec![],
        },
    ));
    let mut expected_rng = world.rng.clone();
    let old = world.to_save_json().unwrap();
    assert!(!old.contains("saved_monster_traits"));
    let mut loaded =
        SimulationWorld::from_save_json(&old, world.ruleset.clone(), &world.ruleset_ref).unwrap();
    assert!(loaded.arena.items[active].saved_monster_traits.is_none());
    assert!(loaded.stored_levels[0].1.items[0]
        .1
        .saved_monster_traits
        .is_none());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&old).unwrap(),
        serde_json::from_str::<serde_json::Value>(&loaded.to_save_json().unwrap()).unwrap()
    );
    assert_eq!(loaded.rng.next_u64(), expected_rng.next_u64());
}
