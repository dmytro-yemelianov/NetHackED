//! Native boundary fixtures derived from billing SAVE1 and object_timer_tests.
use std::sync::Arc;

use nethacked_arena::{ItemId, ItemLocation};
use nethacked_data::{CharacterConfig, ItemKindId as K, MonsterSpeciesId as M, Ruleset};
use nethacked_sim::owner_persistence::PreparedNativeImport;
use nethacked_sim::world::{BillCreditor, BillTerms};
use nethacked_sim::{
    ActionAst, GameEvent, LoadError, ObjectTimerKind as Kind, ObjectTimerTarget as Target,
    SimulationWorld, StoredLevel,
};
use nethacked_types::{BranchId, Buc};
use rand::RngCore;

fn world() -> SimulationWorld {
    let mut w = SimulationWorld::new_with_seed(804);
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.arena.items.clear();
    w.equipment = Default::default();
    w.wielded_item = None;
    w.hero.skills.skills.clear();
    w
}

fn corpse(w: &mut SimulationWorld, location: ItemLocation) -> ItemId {
    let name = &w.ruleset.monster_by_id(M::HUMAN).unwrap().name;
    let mut item = w.ruleset.create_corpse_record(name, location).unwrap();
    item.corpse_age = 7;
    w.arena.spawn_item(item)
}

fn prepare(w: &SimulationWorld, json: &str) -> Result<PreparedNativeImport, LoadError> {
    PreparedNativeImport::bounded(json, json.len(), Arc::clone(&w.ruleset), &w.ruleset_ref)
}

fn compare_load(w: &SimulationWorld, json: &str) -> SimulationWorld {
    let completed = prepare(w, json).unwrap().complete().unwrap();
    let public = SimulationWorld::from_save_json(json, w.ruleset.clone(), &w.ruleset_ref).unwrap();
    same(&completed, &public);
    completed
}

fn same(a: &SimulationWorld, b: &SimulationWorld) {
    assert_eq!(
        serde_json::to_value(a).unwrap(),
        serde_json::to_value(b).unwrap()
    );
    assert_eq!(a.to_save_json().unwrap(), b.to_save_json().unwrap());
    assert_eq!(a.rng.clone().next_u64(), b.rng.clone().next_u64());
    assert_eq!(a.outstanding_bills(), b.outstanding_bills());
}

#[test]
fn byte_limit_precedes_parsing_and_accepts_exact_utf8_byte_length() {
    let w = world();
    for json in ["{ malformed", "é"] {
        let error =
            PreparedNativeImport::bounded(json, json.len() - 1, w.ruleset.clone(), &w.ruleset_ref)
                .err()
                .unwrap();
        assert_eq!(
            error.to_string(),
            "JSON load error: native save exceeds byte limit"
        );
    }
    let json = w.to_save_json().unwrap();
    compare_load(&w, &json);
    assert!(PreparedNativeImport::bounded(
        &json,
        json.len() - 1,
        w.ruleset.clone(),
        &w.ruleset_ref
    )
    .is_err());
}

#[test]
fn due_ties_global_local_and_offlevel_complete_once_with_native_order() {
    // Existing global/local restore and offlevel same-handle fixtures combined.
    let mut w = world();
    let player = w.player_id;
    let a = corpse(&mut w, ItemLocation::CarriedBy(player));
    let floor = ItemLocation::Floor(w.level.stairs_up);
    let b = corpse(&mut w, floor.clone());
    let c = corpse(&mut w, ItemLocation::CarriedBy(player));
    let d = corpse(&mut w, floor);
    let branch = BranchId::DungeonsOfDoom;
    w.stored_levels.push((
        (branch, 9),
        StoredLevel {
            level: w.level.clone(),
            monsters: vec![],
            items: vec![(a, w.arena.items[a].clone())],
            unpaid_items: vec![],
        },
    ));
    for id in [a, b, c, d] {
        w.start_object_timer(Target::Active(id), Kind::RotCorpse, 0)
            .unwrap();
    }
    let cached = Target::Cached {
        branch,
        depth: 9,
        item: a,
    };
    w.start_object_timer(cached, Kind::RotCorpse, 0).unwrap();
    let mut loaded = compare_load(&w, &w.to_save_json().unwrap());
    let serials: Vec<_> = loaded
        .event_log
        .iter()
        .filter_map(|event| match event {
            GameEvent::ObjectTimerExpired { timer } => Some(timer.serial),
            _ => None,
        })
        .collect();
    assert_eq!(serials, vec![1, 3, 0, 2]);
    assert_eq!(loaded.scheduler.turn, w.scheduler.turn);
    assert_eq!(loaded.object_timers.entries().len(), 1);
    assert_eq!(loaded.object_timers.entries()[0].target, cached);
    assert_eq!(loaded.rng.clone().next_u64(), w.rng.clone().next_u64());
    let before = loaded.event_log.len();
    loaded.step_player_action(ActionAst::Wait);
    assert!(loaded.event_log[before..]
        .iter()
        .all(|e| !matches!(e, GameEvent::ObjectTimerExpired { .. })));
    assert_eq!(
        loaded
            .unpack_or_generate_level(branch, 9)
            .iter()
            .filter(|e| matches!(e, GameEvent::ObjectTimerExpired { .. }))
            .count(),
        1
    );
}

#[test]
fn future_revival_continuation_preserves_rng_and_events() {
    let mut w = world();
    let a = corpse(&mut w, ItemLocation::Limbo);
    let player = w.player_id;
    let b = corpse(&mut w, ItemLocation::CarriedBy(player));
    w.start_object_timer(Target::Active(a), Kind::RotCorpse, 3)
        .unwrap();
    w.start_object_timer(Target::Active(b), Kind::ReviveMonster, 4)
        .unwrap();
    w.step_player_action(ActionAst::Wait);
    let json = w.to_save_json().unwrap();
    let mut loaded = compare_load(&w, &json);
    for _ in 0..3 {
        assert_eq!(
            w.step_player_action(ActionAst::Wait),
            loaded.step_player_action(ActionAst::Wait)
        );
        same(&w, &loaded);
    }
}

#[test]
fn known_unknown_and_consumed_billing_fixtures_and_refusal_order() {
    for custom in [false, true] {
        for q in [1, 3] {
            let mut rs = (*Ruleset::vanilla()).clone();
            let mut rod = rs.item_by_id(K::WAN_FIRE).unwrap().clone();
            rod.id = None;
            rod.name = "pack rod".into();
            rs.items.push(rod);
            rs.reindex();
            let mut w = SimulationWorld::new_with_character_and_ruleset(
                33,
                CharacterConfig::default(),
                Arc::new(rs),
                Default::default(),
            );
            w.hero.skills.skills.clear();
            let mut item = w
                .ruleset
                .create_item_record(
                    if custom { "pack rod" } else { "wand of fire" },
                    ItemLocation::CarriedBy(w.player_id),
                    Buc::Cursed,
                )
                .unwrap();
            item.quantity = q;
            let id = w.arena.spawn_item(item.clone());
            let unknown = w.arena.spawn_item(item);
            w.unpaid_items.extend([(id, 30), (unknown, 30)]);
            w.record_bill_terms(
                id,
                BillTerms {
                    billed_quantity: Some(q),
                    unit_price: Some(40),
                    legacy_base_cost: 30,
                    creditor: Some(BillCreditor {
                        branch: w.current_branch,
                        depth: w.depth,
                        shopkeeper: w.player_id,
                    }),
                },
            )
            .unwrap();
            compare_load(&w, &w.to_save_json().unwrap());
            let timer_item = corpse(&mut w, ItemLocation::Limbo);
            w.start_object_timer(Target::Active(timer_item), Kind::RotCorpse, 0)
                .unwrap();
            let mut wire = serde_json::to_value(&w).unwrap();
            wire["billing"]["live_terms"][0][1]["billed_quantity"] = 0.into();
            let duplicate = wire["object_timers"]["entries"][0].clone();
            wire["object_timers"]["entries"]
                .as_array_mut()
                .unwrap()
                .push(duplicate);
            let before = w.to_save_json().unwrap();
            let json = wire.to_string();
            let error = prepare(&w, &json).err().unwrap().to_string();
            assert!(error.starts_with("JSON load error: billing:"), "{error}");
            assert_eq!(
                error,
                SimulationWorld::from_save_json(&json, w.ruleset.clone(), &w.ruleset_ref)
                    .err()
                    .unwrap()
                    .to_string()
            );
            let mut wrong = w.ruleset_ref.clone();
            wrong.id.push_str("-mismatch");
            assert!(matches!(
                PreparedNativeImport::bounded(&json, json.len(), w.ruleset.clone(), &wrong),
                Err(LoadError::RulesetMismatch { .. })
            ));
            assert_eq!(before, w.to_save_json().unwrap());
            w.dispose_whole_carried_item(id).unwrap();
            compare_load(&w, &w.to_save_json().unwrap());
        }
    }
}

#[test]
fn absent_vs_explicit_empty_native_owners_remain_distinct() {
    let mut w = world();
    let player = w.player_id;
    corpse(&mut w, ItemLocation::CarriedBy(player));
    let armor = w
        .ruleset
        .items
        .iter()
        .find(|d| d.armor.is_some())
        .unwrap()
        .name
        .clone();
    let armor = w
        .ruleset
        .create_item_record(&armor, ItemLocation::CarriedBy(player), Buc::Uncursed)
        .unwrap();
    let armor = w.arena.spawn_item(armor);
    let mut empty = serde_json::to_value(&w).unwrap();
    let explicit = compare_load(&w, &empty.to_string());
    assert!(explicit.equipment.armor.is_empty());
    assert!(explicit.object_timers.entries().is_empty());
    for field in ["equipment", "object_timers", "billing"] {
        empty.as_object_mut().unwrap().remove(field);
    }
    let absent = compare_load(&w, &empty.to_string());
    assert!(absent.equipment.armor.iter().any(|(_, id)| *id == armor));
    assert!(!absent.object_timers.entries().is_empty());
    assert_eq!(
        serde_json::to_value(absent.billing).unwrap(),
        serde_json::to_value(explicit.billing).unwrap()
    );
}

#[test]
fn native_unknown_and_duplicate_field_policies_are_retained() {
    let w = world();
    let mut wire = serde_json::to_value(&w).unwrap();
    wire["future_field"] = serde_json::json!({"anything": true});
    compare_load(&w, &wire.to_string());
    let json = w.to_save_json().unwrap();
    let duplicate = format!("{{\"seed\":804,{}", &json[1..]);
    let error = prepare(&w, &duplicate).err().unwrap().to_string();
    assert!(error.contains("duplicate field"), "{error}");
    assert_eq!(
        error,
        SimulationWorld::from_save_json(&duplicate, w.ruleset.clone(), &w.ruleset_ref)
            .err()
            .unwrap()
            .to_string()
    );
}

#[test]
fn explicit_completion_revalidates_without_changing_public_legacy_acceptance() {
    // Native serde accepts a wand carrying an old corpse tag. Billing validates
    // the wand definition/class; RotCorpse observes the tag and deletes it.
    // Preserve that public behavior while refusing invalid completed receipts.
    let mut w = world();
    let mut wand = w
        .ruleset
        .create_item_record_by_id(
            K::WAN_FIRE,
            ItemLocation::CarriedBy(w.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    wand.corpse_species = Some(nethacked_arena::CorpseSpecies::Unspecified);
    let id = w.arena.spawn_item(wand);
    w.unpaid_items.push((id, 30));
    w.record_bill_terms(
        id,
        BillTerms {
            billed_quantity: Some(1),
            unit_price: Some(40),
            legacy_base_cost: 30,
            creditor: None,
        },
    )
    .unwrap();
    w.start_object_timer(Target::Active(id), Kind::RotCorpse, 0)
        .unwrap();
    let json = w.to_save_json().unwrap();
    let prepared = prepare(&w, &json).unwrap();
    let public = SimulationWorld::from_save_json(&json, w.ruleset.clone(), &w.ruleset_ref).unwrap();
    assert!(!public.arena.items.contains_key(id));
    assert_eq!(public.billing.live_terms.len(), 1);
    assert!(prepared
        .complete()
        .err()
        .unwrap()
        .to_string()
        .starts_with("JSON load error: billing:"));
}

#[path = "owner_persistence/runtime.rs"]
mod runtime;
