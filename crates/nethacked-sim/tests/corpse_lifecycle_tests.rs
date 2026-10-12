//! Independent pinned C dig.c:2127–2193, timeout.c:2222–2241,
//! mkobj.c:1389–1431,2557–2589,2763–2766. Revival/burial remain separate.
use nethacked_arena::{ActorSex, CorpseSpecies, ItemId, ItemLocation};
use nethacked_core::ac::ArmorSlot;
use nethacked_data::containers::ContainerContext;
use nethacked_data::generation::{DungeonAlign, MonsterGenContext};
use nethacked_data::monsters::CorpseTimeoutPolicy as Policy;
use nethacked_data::{ItemKindId as K, MonsterSpeciesId as M};
use nethacked_sim::actions::equipment::Dressing;
use nethacked_sim::{
    ActionAst, GameEvent, ObjectTimer, ObjectTimerError, ObjectTimerKind as Kind,
    ObjectTimerTarget as Target, SimulationWorld, StoredLevel,
};
use nethacked_types::{BranchId, Buc};
use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::sync::Arc;

fn world(seed: u64) -> SimulationWorld {
    let mut w = SimulationWorld::new_with_seed(seed);
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.arena.items = Default::default();
    w.equipment = Default::default();
    w.wielded_item = None;
    w.unpaid_items.clear();
    w.scheduler.turn = 73;
    w.depth = 10;
    w.arena.actors[w.player_id].level = 18;
    w.rng = ChaCha8Rng::seed_from_u64(seed);
    let mut rules = (*w.ruleset).clone();
    for def in &mut rules.monsters {
        def.frequency = 0;
        def.gen_flags.push("nogen".into());
    }
    let jackal = rules
        .monsters
        .iter_mut()
        .find(|def| def.id == Some(M::JACKAL))
        .unwrap();
    jackal.frequency = 1;
    jackal.difficulty = 5;
    jackal.gen_flags.clear();
    jackal.flags.clear();
    jackal.maligntyp = 0;
    rules.reindex();
    w.ruleset = Arc::new(rules);
    w
}
fn save(w: &SimulationWorld) -> serde_json::Value {
    serde_json::from_str(&w.to_save_json().unwrap()).unwrap()
}
fn restore(w: &SimulationWorld, value: &serde_json::Value) -> SimulationWorld {
    SimulationWorld::from_save_json(&value.to_string(), w.ruleset.clone(), &w.ruleset_ref).unwrap()
}
fn absent(w: &SimulationWorld) -> serde_json::Value {
    let mut value = save(w);
    value.as_object_mut().unwrap().remove("object_timers");
    value
}
fn corpse(w: &mut SimulationWorld, body: M, location: ItemLocation, age: u32) -> ItemId {
    let mut item = w
        .ruleset
        .create_corpse_record(&w.ruleset.monster_by_id(body).unwrap().name, location)
        .unwrap();
    item.corpse_age = age;
    w.arena.spawn_item(item)
}
fn container(w: &mut SimulationWorld, kind: K, location: ItemLocation) -> ItemId {
    w.arena.spawn_item(
        w.ruleset
            .create_item_record_by_id(kind, location, Buc::Uncursed)
            .unwrap(),
    )
}
fn expired(events: &[GameEvent]) -> Vec<ObjectTimer> {
    events
        .iter()
        .filter_map(|event| {
            if let GameEvent::ObjectTimerExpired { timer } = event {
                Some(*timer)
            } else {
                None
            }
        })
        .collect()
}
fn draw(rng: &mut ChaCha8Rng, trace: &mut Vec<(u32, u32)>, bound: u32) -> u32 {
    let value = rng.random_range(0..bound);
    trace.push((bound, value));
    value
}
/// Literal C rnd.c/rnz integer arithmetic and mkobj elapsed-age branch, without
/// calling the production sampler, recipe, timer, or load implementation.
fn c_rot(
    rng: &mut ChaCha8Rng,
    trace: &mut Vec<(u32, u32)>,
    age: u32,
    hero_level: u32,
    adjustment: i64,
) -> i64 {
    let factor = 1000 + i64::from(draw(rng, trace, 1000));
    let cap = (hero_level / 3).max(5);
    let mut multiplier = 1;
    while multiplier < cap && draw(rng, trace, 4) == 0 {
        multiplier += 1;
    }
    let factor = factor * i64::from(multiplier);
    let noise = if draw(rng, trace, 2) != 0 {
        adjustment * factor / 1000
    } else {
        adjustment * 1000 / factor
    };
    let base = if age > 250 {
        adjustment
    } else {
        250 - i64::from(age)
    };
    base + noise - adjustment
}
fn cached(
    w: &mut SimulationWorld,
    branch: BranchId,
    depth: usize,
    items: Vec<(ItemId, nethacked_arena::ItemRecord)>,
) {
    w.stored_levels.push((
        (branch, depth),
        StoredLevel {
            level: w.level.clone(),
            monsters: vec![],
            items,
            unpaid_items: vec![],
        },
    ));
}

#[test]
fn public_actual_death_generic_wish_and_generated_body_rot_through_wait_and_full_save() {
    for mode in ["death", "wish", "generated"] {
        let mut w = world(423);
        let mut expected_rng = w.rng.clone();
        let mut trace = Vec::new();
        draw(&mut expected_rng, &mut trace, 1); // initial owned Jackal selection
        draw(&mut expected_rng, &mut trace, 2); // initial gender, consumed for wish too
        let delay = c_rot(
            &mut expected_rng,
            &mut trace,
            0,
            18,
            if mode == "generated" { 25 } else { 10 },
        );
        if mode == "death" {
            let mut actor = w
                .ruleset
                .monster_by_id(M::JACKAL)
                .unwrap()
                .create_record_with_hp(w.level.stairs_up, 0, 3, 1);
            actor.sex = Some(ActorSex::Male);
            actor.is_dead = true;
            let id = w.arena.spawn_actor(actor);
            w.on_actor_killed(id, id, &mut Vec::new());
        } else if mode == "wish" {
            let player = w.player_id;
            container(&mut w, K::WAN_WISHING, ItemLocation::CarriedBy(player));
            w.step_player_action(ActionAst::Wish("corpse".into()));
        } else {
            let rules = w.ruleset.clone();
            let tree = rules
                .create_random_item_tree_with_monsters(
                    "corpse",
                    ItemLocation::Limbo,
                    ContainerContext {
                        hero_level: 18,
                        level_difficulty: 10,
                        moves: 73,
                        in_mklev: true,
                    },
                    &MonsterGenContext {
                        hero_level: 18,
                        level_difficulty: 10,
                        in_hell: false,
                        align: DungeonAlign::None,
                        temperature: 0,
                    },
                    |_| false,
                    |n| w.rng.random_range(0..n),
                )
                .unwrap();
            w.attach_generated_item(tree, ItemLocation::Floor(w.level.stairs_up))
                .unwrap();
        }
        let id = w
            .arena
            .items
            .iter()
            .find(|(_, item)| item.is_corpse())
            .unwrap()
            .0;
        let timer = w.object_timers.entries()[0];
        assert_eq!(
            (timer.target, timer.kind, timer.serial, timer.due),
            (Target::Active(id), Kind::RotCorpse, 0, 73 + delay as u64)
        );
        assert_eq!(w.rng.clone().next_u64(), expected_rng.clone().next_u64());
        let mut copy = restore(&w, &save(&w));
        let mut delivered = Vec::new();
        while w.scheduler.turn < timer.due {
            assert!(w.arena.items.contains_key(id));
            assert_eq!(w.arena.items[id].corpse_age, (w.scheduler.turn - 73) as u32);
            let events = w.step_player_action(ActionAst::Wait);
            assert_eq!(copy.step_player_action(ActionAst::Wait), events);
            delivered.extend(expired(&events));
        }
        assert_eq!(delivered, vec![timer]);
        assert!(!w.arena.items.contains_key(id));
        assert!(!copy.arena.items.contains_key(id));
        assert!(w.object_timers.entries().is_empty());
        assert_eq!(save(&w), save(&copy));
        assert_eq!(w.rng.clone().next_u64(), expected_rng.next_u64());
        assert_eq!(w.rng.next_u64(), copy.rng.next_u64());
    }
}

#[test]
fn rot_clears_all_owned_references_cancels_future_callbacks_and_rejects_stale_generation() {
    let mut w = world(61);
    let player = w.player_id;
    let id = corpse(&mut w, M::JACKAL, ItemLocation::CarriedBy(player), 7);
    w.wielded_item = Some(id);
    w.hero.quivered_item = Some(id);
    w.unpaid_items.push((id, 12));
    // A handbuilt payload carrying valid owned armor metadata makes each
    // reference owner observable before immediate load catch-up.
    w.arena.items[id].name = "leather armor".into();
    w.arena.items[id].class = nethacked_types::ItemClass::Armor;
    w.equipment.armor.push((ArmorSlot::Suit, id));
    w.equipment.dressing = Some(Dressing {
        item: id,
        putting_on: true,
        turns_left: 2,
    });
    w.recompute_hero_ac();
    assert_eq!(w.arena.actors[player].ac, 8);
    w.start_object_timer(Target::Active(id), Kind::ReviveMonster, 10)
        .unwrap();
    w.start_object_timer(Target::Active(id), Kind::RotCorpse, 0)
        .unwrap();
    let mut expected_rng = w.rng.clone();
    let mut restored = restore(&w, &save(&w));
    assert_eq!(restored.scheduler.turn, 73);
    assert_eq!(
        expired(&restored.event_log),
        vec![ObjectTimer {
            target: Target::Active(id),
            kind: Kind::RotCorpse,
            due: 73,
            serial: 1
        }]
    );
    assert!(!restored.arena.items.contains_key(id));
    assert_eq!(restored.wielded_item, None);
    assert_eq!(restored.hero.quivered_item, None);
    assert!(!restored.is_unpaid(id));
    assert!(restored.equipment.armor.is_empty());
    assert_eq!(restored.equipment.dressing, None);
    assert_eq!(restored.arena.actors[player].ac, 10);
    assert!(restored.object_timers.entries().is_empty());
    assert_eq!(
        restored.start_object_timer(Target::Active(id), Kind::RotCorpse, 1),
        Err(ObjectTimerError::MissingObject)
    );
    let fresh = corpse(&mut restored, M::JACKAL, ItemLocation::Limbo, 7);
    assert_ne!(fresh, id);
    assert_eq!(save(&restored)["object_timers"]["next_serial"], 2);
    for _ in 0..11 {
        assert!(expired(&restored.step_player_action(ActionAst::Wait)).is_empty());
    }
    assert!(restored.arena.items.contains_key(fresh));
    assert_eq!(restored.rng.next_u64(), expected_rng.next_u64());
}

#[test]
fn ordinary_contained_rot_removes_membership_and_weight_without_destroying_container() {
    let mut w = world(61);
    let player = w.player_id;
    let sack = container(&mut w, K::SACK, ItemLocation::CarriedBy(player));
    let id = corpse(&mut w, M::JACKAL, ItemLocation::InContainer(sack), 7);
    let base = w.arena.items[sack].weight;
    assert_eq!(
        w.arena.calculate_total_weight(sack),
        base + w.arena.items[id].weight
    );
    w.start_object_timer(Target::Active(id), Kind::RotCorpse, 1)
        .unwrap();
    let events = w.step_player_action(ActionAst::Wait);
    assert_eq!(
        expired(&events),
        vec![ObjectTimer {
            target: Target::Active(id),
            kind: Kind::RotCorpse,
            due: 74,
            serial: 0
        }]
    );
    assert!(!w.arena.items.contains_key(id));
    assert!(w.arena.items.contains_key(sack));
    assert!(w.arena.items_in_container(sack).is_empty());
    assert_eq!(w.arena.calculate_total_weight(sack), base);
    assert_eq!(save(&restore(&w, &save(&w))), save(&w));
}

#[test]
fn zero_and_negative_deadlines_delete_on_load_without_clock_age_advance_and_revive_fails_to_rot() {
    for delay in [0, -8] {
        let mut w = world(61);
        let id = corpse(&mut w, M::JACKAL, ItemLocation::Limbo, 7);
        let neutral = corpse(&mut w, M::TROLL, ItemLocation::Limbo, 11);
        w.start_object_timer_signed(Target::Active(id), Kind::RotCorpse, delay)
            .unwrap();
        w.start_object_timer(Target::Active(neutral), Kind::ReviveMonster, 0)
            .unwrap();
        let mut expected_rng = w.rng.clone();
        let rot_delay = (0..5)
            .map(|_| i64::from(expected_rng.random_range(0..50u32)) + 1)
            .sum::<i64>()
            - 11;
        let restored = restore(&w, &save(&w));
        assert_eq!(restored.scheduler.turn, 73);
        assert!(!restored.arena.items.contains_key(id));
        assert_eq!(restored.arena.items[neutral].corpse_age, 11);
        let due = expired(&restored.event_log);
        assert_eq!(
            due.iter()
                .find(|timer| timer.target == Target::Active(id))
                .unwrap()
                .due,
            (73i64 + delay) as u64
        );
        assert_eq!(due.len(), 2);
        assert_eq!(
            restored.object_timers.entries(),
            &[ObjectTimer {
                target: Target::Active(neutral),
                kind: Kind::RotCorpse,
                due: 73 + rot_delay.max(1) as u64,
                serial: 2,
            }]
        );
        assert_eq!(restored.rng.clone().next_u64(), expected_rng.next_u64());
    }
}

#[test]
fn absent_migration_body_samples_active_then_cache_in_exact_order_and_modern_owner_is_authoritative(
) {
    let mut w = world(61);
    let player = w.player_id;
    let first = corpse(&mut w, M::JACKAL, ItemLocation::Limbo, 7);
    let mut original = w
        .ruleset
        .monster_by_id(M::ICE_TROLL)
        .unwrap()
        .create_record_with_hp(w.level.stairs_up, 0, 3, 9);
    original.sex = None;
    original.is_cancelled = true;
    original.is_dead = true;
    w.arena.items[first].saved_monster_traits = Some(Box::new(
        nethacked_arena::SavedMonsterTraits::from_actor(&original, 9),
    ));
    w.arena.items[first].corpse_no_revive = true;
    let ice = container(&mut w, K::ICE_BOX, ItemLocation::CarriedBy(player));
    let sack = container(&mut w, K::SACK, ItemLocation::InContainer(ice));
    w.arena.items[first].location = ItemLocation::InContainer(sack); // enclosing ice does not freeze
    let frozen = corpse(&mut w, M::JACKAL, ItemLocation::InContainer(ice), 7);
    let never = corpse(&mut w, M::LICHEN, ItemLocation::Limbo, 7);
    let unresolved = corpse(&mut w, M::JACKAL, ItemLocation::Limbo, 7);
    w.arena.items[unresolved].corpse_species = Some(CorpseSpecies::Vanilla("MISSING".into()));
    let race = corpse(&mut w, M::HUMAN, ItemLocation::Limbo, 19);
    w.arena.items[race].corpse_species = None;
    w.arena.items[race].corpse_race = Some("human".into());
    let unspecified = corpse(&mut w, M::HUMAN, ItemLocation::Limbo, 7);
    w.arena.items[unspecified].corpse_species = Some(CorpseSpecies::Unspecified);
    assert_eq!(
        w.ruleset
            .monster_by_id(M::LICHEN)
            .unwrap()
            .corpse_timeout_policy,
        Policy::Never
    );
    let mut local = w.arena.items[first].clone();
    local.location = ItemLocation::Floor(w.level.stairs_up);
    local.corpse_age = 19;
    let items = vec![
        (first, local.clone()),
        (ice, w.arena.items[ice].clone()),
        (frozen, w.arena.items[frozen].clone()),
        (never, w.arena.items[never].clone()),
        (unresolved, w.arena.items[unresolved].clone()),
    ];
    cached(&mut w, BranchId::DungeonsOfDoom, 5, items);
    local.corpse_age = 23;
    cached(&mut w, BranchId::Gehennom, 3, vec![(first, local)]);
    let modern = restore(&w, &save(&w));
    let mut restored_tags = w.clone();
    // Existing owned-tag restoration canonicalizes the race-only legacy body;
    // this does not select a species or schedule a modern-empty owner.
    restored_tags.arena.items[race].corpse_species = Some(CorpseSpecies::Unspecified);
    assert_eq!(save(&modern), save(&restored_tags));
    assert_eq!(modern.rng.clone().next_u64(), w.rng.clone().next_u64());
    let mut expected_rng = w.rng.clone();
    let mut trace = Vec::new();
    let mut expected = Vec::new();
    for (serial, target, age) in [
        (0, Target::Active(first), 7),
        (1, Target::Active(race), 19),
        (2, Target::Active(unspecified), 7),
        (
            3,
            Target::Cached {
                branch: BranchId::DungeonsOfDoom,
                depth: 5,
                item: first,
            },
            19,
        ),
        (
            4,
            Target::Cached {
                branch: BranchId::Gehennom,
                depth: 3,
                item: first,
            },
            23,
        ),
    ] {
        let delay = c_rot(&mut expected_rng, &mut trace, age, 18, 10);
        expected.push(ObjectTimer {
            target,
            kind: Kind::RotCorpse,
            due: (73i64 + delay) as u64,
            serial,
        });
    }
    assert_eq!(
        trace,
        vec![
            (1000, 351),
            (4, 1),
            (2, 0),
            (1000, 986),
            (4, 1),
            (2, 1),
            (1000, 312),
            (4, 1),
            (2, 1),
            (1000, 452),
            (4, 3),
            (2, 1),
            (1000, 639),
            (4, 0),
            (4, 0),
            (4, 3),
            (2, 1)
        ]
    );
    assert_eq!(
        expected
            .iter()
            .map(|timer| (timer.serial, timer.due))
            .collect::<Vec<_>>(),
        vec![(0, 313), (1, 313), (2, 319), (3, 308), (4, 339)]
    );
    // Independent C insert-before-equals ordering, never a production queue helper.
    let mut queue = Vec::new();
    for timer in expected {
        let index = queue
            .iter()
            .position(|old: &ObjectTimer| old.due >= timer.due)
            .unwrap_or(queue.len());
        queue.insert(index, timer);
    }
    let migrated = restore(&w, &absent(&w));
    assert_eq!(migrated.object_timers.entries(), queue);
    assert_eq!(
        migrated
            .object_timers
            .entries()
            .iter()
            .map(|timer| timer.serial)
            .collect::<Vec<_>>(),
        vec![3, 1, 0, 2, 4]
    );
    assert_eq!(save(&migrated)["object_timers"]["next_serial"], 5);
    assert_eq!(migrated.scheduler.turn, 73);
    for id in [first, frozen, never, unresolved] {
        assert_eq!(migrated.arena.items[id], w.arena.items[id]);
    }
    assert_eq!(
        serde_json::to_value(&migrated.stored_levels).unwrap(),
        serde_json::to_value(&w.stored_levels).unwrap()
    );
    assert_eq!(
        migrated.rng.clone().next_u64(),
        expected_rng.clone().next_u64()
    );
    let continued = restore(&migrated, &save(&migrated));
    // A subsequent modern save uses C restoration, reversing its saved ties;
    // initial absence migration above must not apply that reversal prematurely.
    assert_eq!(
        continued
            .object_timers
            .entries()
            .iter()
            .map(|timer| timer.serial)
            .collect::<Vec<_>>(),
        vec![3, 0, 1, 2, 4]
    );
    assert_eq!(continued.rng.clone().next_u64(), expected_rng.next_u64());
    assert_eq!(continued.scheduler.turn, 73);
    assert_eq!(continued.arena.items[first].corpse_age, 7);
}

#[test]
fn absent_frozen_never_and_bad_body_keys_draw_nothing_after_owned_tag_restoration() {
    let mut w = world(61);
    let player = w.player_id;
    let ice = container(&mut w, K::ICE_BOX, ItemLocation::CarriedBy(player));
    let frozen = corpse(&mut w, M::JACKAL, ItemLocation::InContainer(ice), 7);
    let never = corpse(&mut w, M::LICHEN, ItemLocation::Limbo, 7);
    let bad = corpse(&mut w, M::JACKAL, ItemLocation::Limbo, 7);
    w.arena.items[bad].corpse_species = Some(CorpseSpecies::Custom("jackal".into())); // vanilla is not custom
    let items = vec![
        (ice, w.arena.items[ice].clone()),
        (frozen, w.arena.items[frozen].clone()),
        (never, w.arena.items[never].clone()),
        (bad, w.arena.items[bad].clone()),
    ];
    cached(&mut w, BranchId::DungeonsOfDoom, 5, items);
    let mut legacy = absent(&w);
    fn remove_ice_tag(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.remove("is_ice_box");
                for value in map.values_mut() {
                    remove_ice_tag(value);
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    remove_ice_tag(value);
                }
            }
            _ => (),
        }
    }
    remove_ice_tag(&mut legacy);
    let restored = restore(&w, &legacy);
    assert!(restored.arena.items[ice].is_ice_box);
    assert!(restored.stored_levels[0].1.items[0].1.is_ice_box);
    assert!(restored.object_timers.entries().is_empty());
    assert_eq!(save(&restored)["object_timers"]["next_serial"], 0);
    assert_eq!(restored.scheduler.turn, 73);
    assert_eq!(restored.arena.items[frozen].corpse_age, 7);
    assert_eq!(restored.rng.clone().next_u64(), w.rng.clone().next_u64());
}

#[test]
fn absent_signed_overdue_active_deletes_now_cached_defers_until_remapped_arrival() {
    for offlevel in [false, true] {
        let mut w = world(61);
        let id = corpse(&mut w, M::JACKAL, ItemLocation::Limbo, 250);
        if offlevel {
            let item = w.arena.destroy_item(id).unwrap();
            cached(&mut w, BranchId::DungeonsOfDoom, 5, vec![(id, item)]);
        }
        let mut expected_rng = w.rng.clone();
        let mut trace = Vec::new();
        let delay = c_rot(&mut expected_rng, &mut trace, 250, 18, 10);
        assert_eq!(trace, vec![(1000, 351), (4, 1), (2, 0)]);
        assert_eq!(delay, -3);
        let mut migrated = restore(&w, &absent(&w));
        assert_eq!(migrated.scheduler.turn, 73);
        assert_eq!(
            migrated.rng.clone().next_u64(),
            expected_rng.clone().next_u64()
        );
        if offlevel {
            assert!(expired(&migrated.event_log).is_empty());
            assert_eq!(migrated.stored_levels[0].1.items[0].1.corpse_age, 250);
            assert_eq!(
                migrated.object_timers.entries(),
                &[ObjectTimer {
                    target: Target::Cached {
                        branch: BranchId::DungeonsOfDoom,
                        depth: 5,
                        item: id
                    },
                    kind: Kind::RotCorpse,
                    due: 70,
                    serial: 0
                }]
            );
            let mut copy = restore(&migrated, &save(&migrated));
            let events = migrated.unpack_or_generate_level(BranchId::DungeonsOfDoom, 5);
            assert_eq!(
                events,
                copy.unpack_or_generate_level(BranchId::DungeonsOfDoom, 5)
            );
            let due = expired(&events);
            let Target::Active(reminted) = due[0].target else {
                panic!("not remapped")
            };
            assert_eq!((due[0].due, due[0].serial), (70, 0));
            assert_ne!(reminted, id);
            assert!(!migrated.arena.items.contains_key(reminted));
            assert!(migrated.object_timers.entries().is_empty());
            assert_eq!(save(&copy), save(&migrated));
            assert_eq!(migrated.scheduler.turn, 73);
        } else {
            assert!(!migrated.arena.items.contains_key(id));
            assert!(migrated.object_timers.entries().is_empty());
            assert_eq!(
                expired(&migrated.event_log),
                vec![ObjectTimer {
                    target: Target::Active(id),
                    kind: Kind::RotCorpse,
                    due: 70,
                    serial: 0
                }]
            );
        }
    }
}
