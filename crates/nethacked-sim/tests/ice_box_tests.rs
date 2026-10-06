use nethacked_arena::{CorpseSpecies, EntityArena, ItemLocation};
use nethacked_data::{ItemKindId as K, MonsterSpeciesId as M, Ruleset};
use nethacked_sim::{ActionAst, SimulationWorld, StoredLevel};
use nethacked_types::{BranchId, Buc};
use rand::RngCore;

fn world() -> SimulationWorld {
    let mut world = SimulationWorld::new_with_seed(404);
    let player = world.player_id;
    world.arena.actors.retain(|id, _| id == player);
    world
}
fn fixtures(
    arena: &mut EntityArena,
    rules: &Ruleset,
) -> (
    nethacked_arena::ItemId,
    nethacked_arena::ItemId,
    nethacked_arena::ItemId,
    nethacked_arena::ItemId,
) {
    let ice = arena.spawn_item(
        rules
            .create_item_record_by_id(K::ICE_BOX, ItemLocation::Limbo, Buc::Uncursed)
            .unwrap(),
    );
    let sack = arena.spawn_item(
        rules
            .create_item_record_by_id(K::SACK, ItemLocation::InContainer(ice), Buc::Uncursed)
            .unwrap(),
    );
    let def = rules.monster_by_id(M::HUMAN).unwrap();
    let mut corpse = rules
        .create_corpse_record(&def.name, ItemLocation::InContainer(sack))
        .unwrap();
    corpse.corpse_age = 7;
    corpse.enchantment = 3;
    corpse.buc = Buc::Blessed;
    let frozen = arena.spawn_item(corpse.clone());
    corpse.location = ItemLocation::Limbo;
    let warm = arena.spawn_item(corpse);
    (ice, sack, frozen, warm)
}

#[test]
fn elapsed_age_ticks_once_per_world_turn_even_with_multiple_hero_actions() {
    let mut world = world();
    let (_, _, frozen, warm) = fixtures(&mut world.arena, &world.ruleset);
    world.scheduler.hero_speed = 24;
    world.scheduler.hero_energy = 24;
    for action in 1..=6 {
        let before = world.scheduler.turn;
        world.step_player_action(ActionAst::Wait);
        assert_eq!(world.scheduler.turn, before + u64::from(action % 2 == 0));
        assert_eq!(world.arena.items[warm].corpse_age, 7 + action / 2);
        assert_eq!(world.arena.items[frozen].corpse_age, 7);
    }
    assert_eq!(world.arena.items[warm].enchantment, 3);
    assert_eq!(world.arena.items[warm].buc, Buc::Blessed);
}

#[test]
fn nested_ice_ancestry_freezes_existing_age_and_removal_resumes_across_save() {
    let mut world = world();
    let (ice, sack, frozen, warm) = fixtures(&mut world.arena, &world.ruleset);
    world.step_player_action(ActionAst::Wait);
    assert_eq!(
        (
            world.arena.items[frozen].corpse_age,
            world.arena.items[warm].corpse_age
        ),
        (7, 8)
    );
    let save = world.to_save_json().unwrap();
    let mut loaded =
        SimulationWorld::from_save_json(&save, world.ruleset.clone(), &world.ruleset_ref).unwrap();
    assert_eq!(
        loaded.arena.items[frozen].corpse_species,
        Some(CorpseSpecies::Vanilla("HUMAN".into()))
    );
    for w in [&mut world, &mut loaded] {
        // Moving the intermediate sack out resumes the descendant's age.
        w.arena.items[sack].location = ItemLocation::Limbo;
        w.step_player_action(ActionAst::Wait);
        assert_eq!(w.arena.items[frozen].corpse_age, 8);
        // Moving it back freezes again, preserving the newly accrued age.
        w.arena.items[sack].location = ItemLocation::InContainer(ice);
        w.step_player_action(ActionAst::Wait);
        assert_eq!(w.arena.items[frozen].corpse_age, 8);
        // Direct corpse removal also resumes age, without clearing other state.
        w.arena.items[frozen].location = ItemLocation::CarriedBy(w.player_id);
        w.step_player_action(ActionAst::Wait);
        assert_eq!(w.arena.items[frozen].corpse_age, 9);
        assert_eq!(w.arena.items[frozen].buc, Buc::Blessed);
        assert_eq!(w.arena.items[frozen].enchantment, 3);
    }
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&world.to_save_json().unwrap()).unwrap(),
        serde_json::from_str::<serde_json::Value>(&loaded.to_save_json().unwrap()).unwrap()
    );
    assert_eq!(world.rng.next_u64(), loaded.rng.next_u64());
}

#[test]
fn cached_level_original_handle_ancestry_ages_once_and_survives_reload() {
    let mut world = world();
    let (_, sack, frozen, warm) = fixtures(&mut world.arena, &world.ruleset);
    let items = world
        .arena
        .items
        .iter()
        .map(|(id, item)| (id, item.clone()))
        .collect();
    world.stored_levels.push((
        (BranchId::DungeonsOfDoom, 9),
        StoredLevel {
            level: world.level.clone(),
            monsters: vec![],
            items,
            unpaid_items: vec![],
        },
    ));
    let save = world.to_save_json().unwrap();
    let mut loaded =
        SimulationWorld::from_save_json(&save, world.ruleset.clone(), &world.ruleset_ref).unwrap();
    for w in [&mut world, &mut loaded] {
        w.step_player_action(ActionAst::Wait);
        let records = &mut w.stored_levels[0].1.items;
        assert_eq!(
            records
                .iter()
                .find(|(id, _)| *id == frozen)
                .unwrap()
                .1
                .corpse_age,
            7
        );
        assert_eq!(
            records
                .iter()
                .find(|(id, _)| *id == warm)
                .unwrap()
                .1
                .corpse_age,
            8
        );
        records
            .iter_mut()
            .find(|(id, _)| *id == sack)
            .unwrap()
            .1
            .location = ItemLocation::Limbo;
        w.step_player_action(ActionAst::Wait);
        let records = &w.stored_levels[0].1.items;
        assert_eq!(
            records
                .iter()
                .find(|(id, _)| *id == frozen)
                .unwrap()
                .1
                .corpse_age,
            8
        );
        assert_eq!(
            records
                .iter()
                .find(|(id, _)| *id == warm)
                .unwrap()
                .1
                .corpse_age,
            9
        );
        assert_eq!(w.arena.items[frozen].corpse_age, 7);
    }
    assert_eq!(
        serde_json::to_value(&world.stored_levels).unwrap(),
        serde_json::to_value(&loaded.stored_levels).unwrap()
    );
    assert_eq!(world.rng.next_u64(), loaded.rng.next_u64());
}

#[test]
fn ancestry_cycles_and_missing_handles_terminate_without_mutating_food_state() {
    let rules = Ruleset::vanilla();
    let mut arena = EntityArena::default();
    let (ice, sack, frozen, warm) = fixtures(&mut arena, &rules);
    arena.items[ice].is_ice_box = false;
    arena.items[ice].location = ItemLocation::InContainer(sack);
    let mut before = arena.items[frozen].clone();
    arena.advance_corpse_ages();
    before.corpse_age += 1;
    assert_eq!(arena.items[frozen], before);
    arena.items.remove(ice);
    arena.advance_corpse_ages();
    assert_eq!(arena.items[frozen].corpse_age, 9);
    arena.items[warm].corpse_age = u32::MAX;
    arena.advance_corpse_ages();
    assert_eq!(arena.items[warm].corpse_age, u32::MAX);
}

#[test]
fn missing_new_record_fields_load_and_legacy_race_corpses_age() {
    let rules = Ruleset::vanilla();
    let corpse = rules
        .create_corpse_record(
            &rules.monster_by_id(M::HUMAN).unwrap().name,
            ItemLocation::Limbo,
        )
        .unwrap();
    let mut value = serde_json::to_value(&corpse).unwrap();
    let obj = value.as_object_mut().unwrap();
    obj.remove("corpse_species");
    obj.remove("is_ice_box");
    let loaded: nethacked_arena::ItemRecord = serde_json::from_value(value).unwrap();
    assert_eq!(loaded.corpse_species, None);
    assert!(!loaded.is_ice_box);
    assert_eq!(loaded.corpse_race.as_deref(), Some("human"));
    let mut arena = EntityArena::default();
    let id = arena.spawn_item(loaded);
    arena.advance_corpse_ages();
    assert_eq!(arena.items[id].corpse_age, 1);
    let generic = rules
        .create_item_record_by_id(K::CORPSE, ItemLocation::Limbo, Buc::Uncursed)
        .unwrap();
    assert_eq!(generic.corpse_species, Some(CorpseSpecies::Unspecified));
    let id = arena.spawn_item(generic);
    arena.advance_corpse_ages();
    assert_eq!(arena.items[id].corpse_age, 1);
}

#[test]
fn old_save_missing_tags_restores_owned_ice_and_corpse_identity_without_age_reset() {
    let mut world = world();
    let (ice, _, frozen, warm) = fixtures(&mut world.arena, &world.ruleset);
    let generic = world.arena.spawn_item(
        world
            .ruleset
            .create_item_record_by_id(K::CORPSE, ItemLocation::Limbo, Buc::Uncursed)
            .unwrap(),
    );
    let original = world.to_save_json().unwrap();
    // Simulate optional-field omission recursively, including cached records.
    fn remove_tags(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.remove("is_ice_box");
                map.remove("corpse_species");
                for value in map.values_mut() {
                    remove_tags(value);
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    remove_tags(value);
                }
            }
            _ => {}
        }
    }
    let mut value: serde_json::Value = serde_json::from_str(&original).unwrap();
    remove_tags(&mut value);
    let mut loaded = SimulationWorld::from_save_json(
        &serde_json::to_string(&value).unwrap(),
        world.ruleset.clone(),
        &world.ruleset_ref,
    )
    .unwrap();
    assert!(loaded.arena.items[ice].is_ice_box);
    assert_eq!(
        loaded.arena.items[frozen].corpse_species,
        Some(CorpseSpecies::Unspecified)
    );
    assert_eq!(
        loaded.arena.items[generic].corpse_species,
        Some(CorpseSpecies::Unspecified)
    );
    loaded.step_player_action(ActionAst::Wait);
    assert_eq!(loaded.arena.items[frozen].corpse_age, 7);
    assert_eq!(loaded.arena.items[warm].corpse_age, 8);
    assert_eq!(loaded.arena.items[generic].corpse_age, 1);
}

proptest::proptest! {
    #[test]
    fn corpse_weight_matches_c_long_product_clipped_to_largest_int(body: u32, quantity: u32) {
        // Independent C oracle: global.h:135 fixes LARGEST_INT to 32767,
        // regardless of the host Rust/C integer width.
        let long_product = u64::from(body) * u64::from(quantity);
        let expected = if long_product > 32767 { 32767 } else { long_product as u32 };
        proptest::prop_assert_eq!(nethacked_core::item_init::corpse_stack_weight(body, quantity), expected);
    }
}
