//! Released C pickup.c:2648–2663,2790–2797 and mkobj.c:1401–1425.
//! Transfers/wire only; actual callbacks and absent scheduling remain pending.
use nethacked_arena::{
    ActorSex, ActorSpecies, CorpseSpecies, ItemId, ItemLocation, SavedMonsterTraits,
};
use nethacked_data::monsters::CorpseThawPolicy as Thaw;
use nethacked_data::{ItemKindId as K, MonsterSpeciesId as M};
use nethacked_sim::{
    ActionAst, ObjectTimerKind as Kind, ObjectTimerTarget as Target, SimulationWorld, StoredLevel,
};
use nethacked_types::{BranchId, Buc, Coord, Tile};
use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::sync::Arc;

fn world() -> SimulationWorld {
    let mut w = SimulationWorld::new_with_seed(61);
    w.arena.actors.retain(|id, _| id == w.player_id);
    // Use a fresh allocator to isolate fixture handles from initial equipment.
    w.arena.items = Default::default();
    w.equipment = Default::default();
    w.wielded_item = None;
    w.arena.actors[w.player_id].level = 18;
    w.scheduler.turn = 73;
    w.rng = ChaCha8Rng::seed_from_u64(61);
    w
}
fn container(w: &mut SimulationWorld, kind: K, location: ItemLocation) -> ItemId {
    let mut item = w
        .ruleset
        .create_item_record_by_id(kind, location, Buc::Uncursed)
        .unwrap();
    item.weight = 1; // capacity is tested separately, independent of lifecycle
    w.arena.spawn_item(item)
}
fn corpse(w: &mut SimulationWorld, body: M, saved: Option<M>, location: ItemLocation) -> ItemId {
    let mut item = w
        .ruleset
        .create_corpse_record(&w.ruleset.monster_by_id(body).unwrap().name, location)
        .unwrap();
    item.name = "renamed corpse with misleading ice troll label".into();
    item.corpse_age = 7;
    item.weight = 1;
    item.corpse_no_revive = true;
    if let Some(saved) = saved {
        let def = w.ruleset.monster_by_id(saved).unwrap();
        let mut actor = def.create_record_with_hp(Coord::new_unchecked(8, 8), 0, 3, 7);
        actor.is_cancelled = true;
        actor.is_dead = true;
        item.saved_monster_traits =
            Some(Box::new(SavedMonsterTraits::from_actor(&actor, def.level)));
    }
    w.arena.spawn_item(item)
}
fn carried(w: &SimulationWorld, id: ItemId) -> usize {
    w.arena
        .items_carried_by(w.player_id)
        .iter()
        .position(|&found| found == id)
        .unwrap()
}
fn put(w: &mut SimulationWorld, item: ItemId, into: ItemId) {
    w.step_player_action(ActionAst::PutInContainer {
        item_index: carried(w, item),
        container_index: carried(w, into),
    });
}
fn take(w: &mut SimulationWorld, from: ItemId, item: ItemId) {
    let index = w
        .arena
        .items_in_container(from)
        .iter()
        .position(|&found| found == item)
        .unwrap();
    w.step_player_action(ActionAst::TakeFromContainer {
        container_index: carried(w, from),
        item_index: index,
    });
}
fn save(w: &SimulationWorld) -> serde_json::Value {
    serde_json::from_str(&w.to_save_json().unwrap()).unwrap()
}
fn load(w: &SimulationWorld) -> SimulationWorld {
    SimulationWorld::from_save_json(
        &w.to_save_json().unwrap(),
        w.ruleset.clone(),
        &w.ruleset_ref,
    )
    .unwrap()
}
fn draw(rng: &mut ChaCha8Rng, trace: &mut Vec<(u32, u32)>, bound: u32) -> u32 {
    let value = rng.random_range(0..bound);
    trace.push((bound, value));
    value
}
/// Independent literal source formula; never calls the production sampler.
fn c_thaw_sample(rng: &mut ChaCha8Rng, age: u32, troll: bool) -> (Kind, i64, Vec<(u32, u32)>) {
    let mut trace = Vec::new();
    let mut scale = 1000 + i64::from(draw(rng, &mut trace, 1000));
    let mut multiplier = 1;
    while multiplier < 6 && draw(rng, &mut trace, 4) == 0 {
        multiplier += 1;
    }
    scale *= multiplier;
    let noise = if draw(rng, &mut trace, 2) != 0 {
        10 * scale / 1000
    } else {
        10 * 1000 / scale
    };
    let rot = 250 - i64::from(age) + noise - 10;
    if troll {
        for age in 2..=50 {
            if draw(rng, &mut trace, 37) == 0 {
                return (Kind::ReviveMonster, age, trace);
            }
        }
    }
    (Kind::RotCorpse, rot, trace)
}

#[test]
fn mismatched_body_traits_control_distinct_freeze_thaw_and_timeout_facts() {
    for (body, original, no_revive, uncancel) in [
        (M::JACKAL, M::ICE_TROLL, false, false),
        (M::ICE_TROLL, M::JACKAL, true, true),
    ] {
        let mut w = world();
        let player = w.player_id;
        let ice = container(&mut w, K::ICE_BOX, ItemLocation::CarriedBy(player));
        let id = corpse(
            &mut w,
            body,
            Some(original),
            ItemLocation::CarriedBy(player),
        );
        w.start_object_timer(Target::Active(id), Kind::RotCorpse, 200)
            .unwrap();
        w.start_object_timer(Target::Active(id), Kind::ReviveMonster, 201)
            .unwrap();
        let mut before_rng = w.rng.clone();
        put(&mut w, id, ice);
        assert!(w.object_timers.entries().is_empty());
        assert_eq!(w.rng.clone().next_u64(), before_rng.next_u64());
        assert_eq!(w.arena.items[id].corpse_age, 7);
        assert_eq!(
            w.arena.items[id]
                .saved_monster_traits
                .as_ref()
                .unwrap()
                .actor
                .is_cancelled,
            !uncancel
        );
        let mut expected_rng = w.rng.clone();
        let (kind, delay, trace) = c_thaw_sample(&mut expected_rng, 7, body == M::ICE_TROLL);
        let mut expected_trace = vec![(1000, 351), (4, 1), (2, 0)];
        if body == M::ICE_TROLL {
            expected_trace.extend(
                [36, 12, 26, 11, 16, 19, 16, 29, 32, 23, 7, 7, 30, 22, 25, 0]
                    .map(|value| (37, value)),
            );
        }
        assert_eq!(trace, expected_trace);
        assert_eq!(delay, if body == M::ICE_TROLL { 17 } else { 240 });
        take(&mut w, ice, id);
        assert_eq!(w.rng.clone().next_u64(), expected_rng.next_u64());
        assert_eq!(w.arena.items[id].corpse_no_revive, no_revive);
        assert_eq!(w.arena.items[id].corpse_age, 8); // exactly one post-action tick
        assert_eq!(w.object_timers.entries().len(), 1);
        let timer = w.object_timers.entries()[0];
        assert_eq!(
            (timer.target, timer.kind, timer.serial, timer.due),
            (Target::Active(id), kind, 2, 74 + delay as u64)
        );
        assert_eq!(save(&load(&w)), save(&w));
    }
}

#[test]
fn enclosing_container_moves_preserve_descendant_timers_flags_rng_and_direct_ages() {
    let mut w = world();
    let player = w.player_id;
    let outer = container(&mut w, K::ICE_BOX, ItemLocation::CarriedBy(player));
    let sack = container(&mut w, K::SACK, ItemLocation::CarriedBy(player));
    let inner = container(&mut w, K::SACK, ItemLocation::InContainer(sack));
    let troll = corpse(
        &mut w,
        M::ICE_TROLL,
        Some(M::ICE_TROLL),
        ItemLocation::InContainer(inner),
    );
    let ordinary = corpse(&mut w, M::JACKAL, None, ItemLocation::InContainer(sack));
    let local_ice = container(&mut w, K::ICE_BOX, ItemLocation::InContainer(sack));
    let frozen = corpse(
        &mut w,
        M::ICE_TROLL,
        Some(M::ICE_TROLL),
        ItemLocation::InContainer(local_ice),
    );
    w.arena.items[troll].name = "running troll".into();
    w.arena.items[ordinary].name = "running ordinary".into();
    w.arena.items[frozen].name = "directly frozen troll".into();
    w.start_object_timer(Target::Active(troll), Kind::ReviveMonster, 200)
        .unwrap();
    w.start_object_timer(Target::Active(ordinary), Kind::RotCorpse, 201)
        .unwrap();
    let timers = w.object_timers.clone();
    let expected_rng = w.rng.clone();
    // C pickup.c:2644–2663 only tests the selected sack, never its contents.
    put(&mut w, sack, outer);
    assert_eq!(w.object_timers, timers);
    assert_eq!(w.rng.clone().next_u64(), expected_rng.clone().next_u64());
    assert_eq!(
        (
            w.arena.items[troll].corpse_age,
            w.arena.items[ordinary].corpse_age,
            w.arena.items[frozen].corpse_age
        ),
        (8, 8, 7)
    );
    // C pickup.c:2758–2797 likewise removes only the selected sack.
    take(&mut w, outer, sack);
    assert_eq!(w.object_timers, timers);
    assert_eq!(w.rng.clone().next_u64(), expected_rng.clone().next_u64());
    assert_eq!(
        (
            w.arena.items[troll].corpse_age,
            w.arena.items[ordinary].corpse_age,
            w.arena.items[frozen].corpse_age
        ),
        (9, 9, 7)
    );
    for id in [troll, frozen] {
        assert!(
            w.arena.items[id]
                .saved_monster_traits
                .as_ref()
                .unwrap()
                .actor
                .is_cancelled
        );
        assert!(w.arena.items[id].corpse_no_revive);
    }
    // Handbuilt inner ice topology exercises direct-parent ownership, not a
    // claim that vanilla permits inserting an ice box inside another container.
    w.step_player_action(ActionAst::Drop(carried(&w, sack)));
    w.stored_levels.push((
        (BranchId::DungeonsOfDoom, 2),
        StoredLevel {
            level: w.level.clone(),
            monsters: vec![],
            items: vec![],
            unpaid_items: vec![],
        },
    ));
    w.level
        .set_tile(w.level.stairs_down, Tile::Stairs { up: false });
    w.arena.actors[player].coord = w.level.stairs_down;
    w.step_player_action(ActionAst::Descend);
    w.step_player_action(ActionAst::Wait);
    // Active 3 ticks and cached 2 ticks: ordinary descendants age, direct ice
    // remains frozen. No ancestor lifecycle callback or sample exists in C.
    let stored = &w
        .stored_levels
        .iter()
        .find(|(key, _)| *key == (BranchId::DungeonsOfDoom, 1))
        .unwrap()
        .1;
    for (name, age) in [
        ("running troll", 12),
        ("running ordinary", 12),
        ("directly frozen troll", 7),
    ] {
        let item = &stored
            .items
            .iter()
            .find(|(_, item)| item.name == name)
            .unwrap()
            .1;
        assert_eq!(item.corpse_age, age);
        assert!(item.corpse_no_revive);
    }
    let mut copy = load(&w);
    assert_eq!(save(&copy), save(&w));
    for sim in [&mut w, &mut copy] {
        sim.level
            .set_tile(sim.level.stairs_up, Tile::Stairs { up: true });
        sim.arena.actors[player].coord = sim.level.stairs_up;
        sim.step_player_action(ActionAst::Ascend);
        for (name, age, serial, kind, due) in [
            ("running troll", 13, Some(0), Kind::ReviveMonster, 273),
            ("running ordinary", 13, Some(1), Kind::RotCorpse, 274),
            ("directly frozen troll", 7, None, Kind::ReviveMonster, 0),
        ] {
            let (id, item) = sim
                .arena
                .items
                .iter()
                .find(|(_, item)| item.name == name)
                .unwrap();
            assert_eq!(item.corpse_age, age);
            assert!(item.corpse_no_revive);
            if let Some(traits) = &item.saved_monster_traits {
                assert!(traits.actor.is_cancelled);
            }
            let timer = sim
                .object_timers
                .entries()
                .iter()
                .find(|timer| timer.target == Target::Active(id));
            if let Some(serial) = serial {
                let timer = timer.unwrap();
                assert_eq!((timer.serial, timer.kind, timer.due), (serial, kind, due));
            } else {
                assert!(timer.is_none());
            }
        }
        assert_eq!(save(sim)["object_timers"]["next_serial"], 2);
        assert_eq!(sim.rng.clone().next_u64(), expected_rng.clone().next_u64());
    }
    assert_eq!(save(&copy), save(&w));
}

#[test]
fn nested_sack_attachment_keeps_initial_timer_and_age_without_extra_draws() {
    use nethacked_data::containers::GeneratedItem;
    use nethacked_data::corpses::{
        CorpseTimeoutRecipe as Recipe, CorpseTimerOperation as Operation,
    };
    let mut w = world();
    let player = w.player_id;
    let ice = container(&mut w, K::ICE_BOX, ItemLocation::CarriedBy(player));
    let item = w
        .ruleset
        .create_corpse_record("jackal", ItemLocation::Limbo)
        .unwrap();
    let recipe = Recipe::Rot {
        sampled_at: 73,
        delay: 240,
    };
    let corpse = GeneratedItem {
        item,
        contents: vec![],
        corpse_timeout: Some(recipe),
        corpse_timer_operations: vec![Operation::Start(recipe)],
    };
    let sack = GeneratedItem {
        item: w
            .ruleset
            .create_item_record_by_id(K::SACK, ItemLocation::Limbo, Buc::Uncursed)
            .unwrap(),
        contents: vec![corpse],
        corpse_timeout: None,
        corpse_timer_operations: vec![],
    };
    let expected_rng = w.rng.clone();
    let sack = w
        .attach_generated_item(sack, ItemLocation::InContainer(ice))
        .unwrap();
    let id = w.arena.items_in_container(sack)[0];
    let timer = w.object_timers.entries()[0];
    assert_eq!(
        (timer.target, timer.serial, timer.kind, timer.due),
        (Target::Active(id), 0, Kind::RotCorpse, 313)
    );
    assert_eq!(w.arena.items[id].corpse_age, 0);
    w.step_player_action(ActionAst::Wait);
    assert_eq!(w.arena.items[id].corpse_age, 1);
    assert_eq!(w.object_timers.entries(), &[timer]);
    assert_eq!(w.rng.clone().next_u64(), expected_rng.clone().next_u64());
    assert_eq!(save(&load(&w)), save(&w));
}

#[test]
fn authored_custom_policies_override_names_canonical_families_and_saved_body_choice() {
    let mut w = world();
    let player = w.player_id;
    let mut rules = (*w.ruleset).clone();
    let mut custom = rules.monster_by_id(M::JACKAL).unwrap().clone();
    custom.id = None;
    custom.name = "pack ice impostor".into();
    custom.corpse_thaw_policy = Thaw::IceTroll;
    rules.monsters.push(custom);
    rules.reindex();
    w.ruleset = Arc::new(rules);
    let ice = container(&mut w, K::ICE_BOX, ItemLocation::CarriedBy(player));
    let id = corpse(
        &mut w,
        M::JACKAL,
        Some(M::JACKAL),
        ItemLocation::CarriedBy(player),
    );
    w.arena.items[id].corpse_species = Some(CorpseSpecies::Custom("PACK ICE IMPOSTOR".into()));
    let mut before_rng = w.rng.clone();
    put(&mut w, id, ice);
    assert_eq!(w.rng.clone().next_u64(), before_rng.next_u64());
    assert!(
        !w.arena.items[id]
            .saved_monster_traits
            .as_ref()
            .unwrap()
            .actor
            .is_cancelled
    );
    let mut expected_rng = w.rng.clone();
    let (_, delay, _) = c_thaw_sample(&mut expected_rng, 7, false);
    take(&mut w, ice, id);
    assert!(w.arena.items[id].corpse_no_revive); // saved ordinary identity wins
    assert_eq!(
        (
            w.object_timers.entries()[0].kind,
            w.object_timers.entries()[0].due
        ),
        (Kind::RotCorpse, 74 + delay as u64)
    );
    assert_eq!(w.rng.clone().next_u64(), expected_rng.next_u64());
    let mut rules = (*w.ruleset).clone();
    rules
        .monsters
        .iter_mut()
        .find(|d| d.id == Some(M::ICE_TROLL))
        .unwrap()
        .corpse_thaw_policy = Thaw::Ordinary;
    rules.reindex();
    w.ruleset = Arc::new(rules);
    let canonical = corpse(
        &mut w,
        M::ICE_TROLL,
        Some(M::ICE_TROLL),
        ItemLocation::CarriedBy(player),
    );
    put(&mut w, canonical, ice);
    assert!(
        w.arena.items[canonical]
            .saved_monster_traits
            .as_ref()
            .unwrap()
            .actor
            .is_cancelled
    );
    take(&mut w, ice, canonical);
    assert!(w.arena.items[canonical].corpse_no_revive);
}

#[test]
fn rejected_weight_and_invalid_container_transfers_leave_flags_timers_rng_unchanged() {
    let mut w = world();
    let player = w.player_id;
    let ice = container(&mut w, K::ICE_BOX, ItemLocation::CarriedBy(player));
    let id = corpse(
        &mut w,
        M::ICE_TROLL,
        Some(M::ICE_TROLL),
        ItemLocation::InContainer(ice),
    );
    w.arena.items[id].weight = u32::MAX;
    let old_item = w.arena.items[id].clone();
    let timers = w.object_timers.clone();
    let mut expected_rng = w.rng.clone();
    take(&mut w, ice, id);
    assert_eq!(w.arena.items[id], old_item);
    assert_eq!(w.object_timers, timers);
    assert_eq!(w.rng.clone().next_u64(), expected_rng.next_u64());
    w.arena.items[id].weight = 1;
    w.arena.items[id].location = ItemLocation::CarriedBy(player);
    let rock = container(&mut w, K::ROCK, ItemLocation::CarriedBy(player));
    let old_item = w.arena.items[id].clone();
    let mut expected_rng = w.rng.clone();
    put(&mut w, id, rock);
    assert_eq!(w.arena.items[id], old_item);
    assert_eq!(w.object_timers, timers);
    assert_eq!(w.rng.next_u64(), expected_rng.next_u64());
}

#[test]
fn unknown_saved_and_supplied_body_keys_are_unsupported_without_identity_guesses() {
    for invalid in 0..4 {
        let mut w = world();
        let player = w.player_id;
        let ice = container(&mut w, K::ICE_BOX, ItemLocation::CarriedBy(player));
        let id = corpse(
            &mut w,
            M::JACKAL,
            Some(M::ICE_TROLL),
            ItemLocation::InContainer(ice),
        );
        match invalid {
            0 => {
                w.arena.items[id]
                    .saved_monster_traits
                    .as_mut()
                    .unwrap()
                    .actor
                    .species = None
            }
            1 => {
                w.arena.items[id]
                    .saved_monster_traits
                    .as_mut()
                    .unwrap()
                    .actor
                    .species = Some(ActorSpecies::Vanilla("NOT_A_SPECIES".into()))
            }
            2 => {
                w.arena.items[id].corpse_species =
                    Some(CorpseSpecies::Vanilla("NOT_A_SPECIES".into()))
            }
            _ => w.arena.items[id].corpse_species = Some(CorpseSpecies::Custom("jackal".into())), // not a custom definition
        }
        let mut expected_rng = w.rng.clone();
        take(&mut w, ice, id);
        assert!(w.object_timers.entries().is_empty());
        assert!(w.arena.items[id].corpse_no_revive);
        assert_eq!(w.rng.next_u64(), expected_rng.next_u64());
    }
}

#[test]
fn unspecified_and_race_only_thaw_use_ordinary_compatibility_without_species_invention() {
    for race_only in [false, true] {
        let mut w = world();
        let player = w.player_id;
        let ice = container(&mut w, K::ICE_BOX, ItemLocation::CarriedBy(player));
        let id = corpse(&mut w, M::JACKAL, None, ItemLocation::InContainer(ice));
        w.arena.items[id].corpse_species = if race_only {
            None
        } else {
            Some(CorpseSpecies::Unspecified)
        };
        w.arena.items[id].corpse_race = Some("human".into());
        let expected_species = w.arena.items[id].corpse_species.clone();
        let mut expected_rng = w.rng.clone();
        let (_, delay, _) = c_thaw_sample(&mut expected_rng, 7, false);
        take(&mut w, ice, id);
        assert_eq!(w.arena.items[id].corpse_species, expected_species);
        assert!(w.arena.items[id].corpse_no_revive);
        assert_eq!(
            (
                w.object_timers.entries()[0].kind,
                w.object_timers.entries()[0].due
            ),
            (Kind::RotCorpse, 73 + delay as u64)
        );
        assert_eq!(w.rng.next_u64(), expected_rng.next_u64());
    }
}

#[test]
fn actual_initialized_cancelled_troll_freeze_midcache_save_and_public_thaw() {
    for species in [M::TROLL, M::ICE_TROLL] {
        let mut w = world();
        let player = w.player_id;
        let ice = container(&mut w, K::ICE_BOX, ItemLocation::CarriedBy(player));
        let mut actor = w
            .ruleset
            .monster_by_id(species)
            .unwrap()
            .create_record_with_hp(w.arena.actors[player].coord, 0, 3, 7);
        actor.sex = Some(ActorSex::Female);
        actor.is_dead = true;
        actor.is_cancelled = true;
        let actor = w.arena.spawn_actor(actor);
        w.on_actor_killed(actor, actor, &mut Vec::new());
        let id = w
            .arena
            .items
            .iter()
            .find(|(_, item)| item.is_corpse())
            .unwrap()
            .0;
        w.arena.items[id].weight = 1;
        w.step_player_action(ActionAst::PickUp);
        let next_serial = w.object_timers.entries()[0].serial + 1;
        let mut expected_rng = w.rng.clone();
        put(&mut w, id, ice);
        assert!(w.object_timers.entries().is_empty());
        assert_eq!(w.rng.clone().next_u64(), expected_rng.next_u64());
        assert_eq!(
            w.arena.items[id]
                .saved_monster_traits
                .as_ref()
                .unwrap()
                .actor
                .is_cancelled,
            species != M::ICE_TROLL
        );
        let frozen_age = w.arena.items[id].corpse_age;
        w.step_player_action(ActionAst::Drop(carried(&w, ice)));
        w.stored_levels.push((
            (BranchId::DungeonsOfDoom, 2),
            StoredLevel {
                level: w.level.clone(),
                monsters: vec![],
                items: vec![],
                unpaid_items: vec![],
            },
        ));
        w.level
            .set_tile(w.level.stairs_down, Tile::Stairs { up: false });
        w.arena.actors[player].coord = w.level.stairs_down;
        w.step_player_action(ActionAst::Descend);
        let mut copy = load(&w);
        assert_eq!(save(&copy), save(&w));
        for sim in [&mut w, &mut copy] {
            sim.level
                .set_tile(sim.level.stairs_up, Tile::Stairs { up: true });
            sim.arena.actors[player].coord = sim.level.stairs_up;
            sim.step_player_action(ActionAst::Ascend);
            let id = sim
                .arena
                .items
                .iter()
                .find(|(_, item)| item.is_corpse())
                .unwrap()
                .0;
            let ItemLocation::InContainer(ice) = sim.arena.items[id].location else {
                panic!("lost frozen container");
            };
            assert_eq!(sim.arena.items[id].corpse_age, frozen_age);
            let ItemLocation::Floor(at) = sim.arena.items[ice].location else {
                panic!("lost cache root");
            };
            sim.arena.actors[player].coord = at;
            sim.step_player_action(ActionAst::PickUp);
            let thaw_at = sim.scheduler.turn;
            let mut expected_rng = sim.rng.clone();
            let (kind, delay, _) = c_thaw_sample(&mut expected_rng, frozen_age, true);
            take(sim, ice, id);
            assert_eq!(sim.rng.clone().next_u64(), expected_rng.next_u64());
            assert_eq!(
                sim.arena.items[id].corpse_no_revive,
                species != M::ICE_TROLL
            );
            let timer = sim.object_timers.entries()[0];
            assert_eq!(
                (timer.target, timer.serial, timer.kind, timer.due),
                (
                    Target::Active(id),
                    next_serial,
                    kind,
                    thaw_at + delay as u64
                )
            );
        }
        assert_eq!(save(&copy), save(&w));
        assert_eq!(copy.rng.next_u64(), w.rng.next_u64());
    }
}

#[test]
fn modern_empty_cancelled_and_absent_timer_wire_have_one_explicit_owner() {
    let mut w = world();
    assert_eq!(
        save(&w)["object_timers"],
        serde_json::json!({"entries": [], "next_serial": 0})
    );
    let id = corpse(&mut w, M::JACKAL, None, ItemLocation::Limbo);
    w.start_object_timer(Target::Active(id), Kind::RotCorpse, 200)
        .unwrap();
    w.cancel_object_timers(Target::Active(id));
    assert_eq!(
        save(&w)["object_timers"],
        serde_json::json!({"entries": [], "next_serial": 1})
    );
    let mut copy = load(&w);
    assert_eq!(save(&copy), save(&w));
    assert_eq!(copy.rng.next_u64(), w.rng.clone().next_u64());
    let mut legacy = save(&w);
    legacy.as_object_mut().unwrap().remove("object_timers");
    let mut restored =
        SimulationWorld::from_save_json(&legacy.to_string(), w.ruleset.clone(), &w.ruleset_ref)
            .unwrap();
    let mut expected_rng = w.rng.clone();
    let (kind, delay, trace) = c_thaw_sample(&mut expected_rng, 7, false);
    assert_eq!(trace, vec![(1000, 351), (4, 1), (2, 0)]);
    assert_eq!(delay, 240);
    let timer = restored.object_timers.entries()[0];
    assert_eq!(
        (timer.target, timer.kind, timer.serial, timer.due),
        (Target::Active(id), kind, 0, 313)
    );
    assert_eq!(restored.rng.next_u64(), expected_rng.next_u64());
    assert_eq!(
        save(&restored)["object_timers"],
        serde_json::json!({"entries": [{"target": {"Active": serde_json::to_value(id).unwrap()}, "kind": "RotCorpse", "due": 313, "serial": 0}], "next_serial": 1})
    );
    legacy["object_timers"] = serde_json::Value::Null;
    assert!(SimulationWorld::from_save_json(
        &legacy.to_string(),
        w.ruleset.clone(),
        &w.ruleset_ref
    )
    .is_err());
    assert!(save(&w)
        .as_object()
        .unwrap()
        .keys()
        .all(|key| !key.contains("legacy_object_timer")));
}
