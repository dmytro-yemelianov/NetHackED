use super::super::*;
use rand::RngCore;
fn config(w: &SimulationWorld) -> OwnerBridgeConfig {
    OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_natural_hp_regeneration()
}
fn value(w: &SimulationWorld) -> serde_json::Value {
    serde_json::from_str(&w.to_save_json().unwrap()).unwrap()
}
fn check(seed: u64, moved: bool, weight: u32) -> bool {
    let mut w = SimulationWorld::new_with_seed(seed);
    w.arena.actors[w.player_id].hp = 3;
    w.arena.actors[w.player_id].max_hp = 30;
    w.scheduler.turn = 19;
    if weight > 0 {
        let item = w.arena.items.iter().next().map(|(id, _)| id);
        if let Some(id) = item {
            w.arena.items[id].location = nethacked_arena::ItemLocation::CarriedBy(w.player_id);
            w.arena.items[id].weight = weight;
        }
    }
    let b = OwnerBridge::install(NativePartitions::from_native(w.clone()), config(&w)).unwrap();
    let mut expected = w;
    let old_hp = expected.arena.actors[expected.player_id].hp;
    let before_rng = expected.rng.get_word_pos();
    expected.regenerate_natural_hero_hp(moved);
    let req = b.prepare_regeneration_phase(moved).unwrap();
    b.execute_regeneration_phase(req.clone()).unwrap();
    assert_eq!(value(&b.export_native().unwrap()), value(&expected));
    let mut actual = b.export_native().unwrap();
    let before = value(&actual);
    assert!(actual.rng.get_word_pos() > before_rng);
    assert_eq!(
        (0..4).map(|_| actual.rng.next_u64()).collect::<Vec<_>>(),
        (0..4).map(|_| expected.rng.next_u64()).collect::<Vec<_>>()
    );
    assert!(b.execute_regeneration_phase(req).is_err());
    assert_eq!(value(&b.export_native().unwrap()), before);
    let saved = b.capture().unwrap();
    let restored = OwnerBridge::reconstruct(&saved, config(&expected)).unwrap();
    assert_eq!(value(&restored.export_native().unwrap()), before);
    let json = b.export_native().unwrap().to_save_json().unwrap();
    let reloaded = OwnerBridge::import_native(&json, json.len(), config(&expected)).unwrap();
    assert_eq!(value(&reloaded.export_native().unwrap()), before);
    let next =
        |mut world: SimulationWorld| (0..4).map(|_| world.rng.next_u64()).collect::<Vec<_>>();
    let continuation = next(b.export_native().unwrap());
    assert_eq!(next(restored.export_native().unwrap()), continuation);
    assert_eq!(next(reloaded.export_native().unwrap()), continuation);
    expected.arena.actors[expected.player_id].hp > old_hp
}
#[test]
fn ordinary_stationary_and_moving_match_legacy_phase() {
    let mut outcomes = [None, None];
    for seed in 0..256 {
        let mut sample = SimulationWorld::new_with_seed(seed);
        sample.arena.actors[sample.player_id].hp = 3;
        sample.arena.actors[sample.player_id].max_hp = 30;
        sample.regenerate_natural_hero_hp(false);
        let healed = sample.arena.actors[sample.player_id].hp > 3;
        outcomes[usize::from(healed)] = Some(seed);
        if outcomes.iter().all(Option::is_some) {
            break;
        }
    }
    let [Some(missed_seed), Some(healed_seed)] = outcomes else {
        panic!("seed search must cover both roll outcomes")
    };
    for moved in [false, true] {
        assert!(!check(missed_seed, moved, 0));
        assert!(check(healed_seed, moved, 0));
    }
}

fn history_len(b: &OwnerBridge) -> usize {
    b.session
        .with_runtime(&b.credential.borrow(), |s| s.host().history())
        .unwrap()
        .unwrap()
        .len()
}
fn raw_world() -> SimulationWorld {
    let mut w = SimulationWorld::new_with_seed(719);
    w.arena.actors[w.player_id].hp = 3;
    w.arena.actors[w.player_id].max_hp = 30;
    w.scheduler.turn = 19;
    w
}
fn compare_case(w: SimulationWorld, moved: bool, draws: bool) {
    let c = config(&w);
    let b = OwnerBridge::install(NativePartitions::from_native(w.clone()), c).unwrap();
    let mut expected = w;
    let pos = expected.rng.get_word_pos();
    let history = history_len(&b);
    expected.regenerate_natural_hero_hp(moved);
    let req = b.prepare_regeneration_phase(moved).unwrap();
    b.execute_regeneration_phase(req).unwrap();
    assert_eq!(value(&b.export_native().unwrap()), value(&expected));
    assert_eq!(expected.rng.get_word_pos() > pos, draws);
    assert_eq!(history_len(&b) - history, usize::from(draws));
}
#[test]
fn skips_preserve_native_bytes_rng_and_history() {
    for kind in 0..5 {
        let mut w = raw_world();
        match kind {
            0 => w.arena.actors[w.player_id].is_dead = true,
            1 => w.arena.actors[w.player_id].hp = 0,
            2 => w.arena.actors[w.player_id].hp = 30,
            3 => w.arena.actors[w.player_id].hp = 31,
            _ => {
                w.hero.polymorph = Some(nethacked_types::PolymorphForm {
                    monster_id: 1,
                    hp: 5,
                    max_hp: 10,
                    duration: 20,
                })
            }
        }
        compare_case(w, false, false);
    }
}
fn bag_world(buc: nethacked_types::Buc, child_weight: u32) -> SimulationWorld {
    let mut w = raw_world();
    w.arena.items.clear();
    w.equipment = Default::default();
    w.wielded_item = None;
    w.unpaid_items.clear();
    w.billing = Default::default();
    let mut bag = w
        .ruleset
        .create_item_record(
            "bag of holding",
            nethacked_arena::ItemLocation::CarriedBy(w.player_id),
            buc,
        )
        .unwrap();
    bag.is_container = true;
    bag.is_bag_of_holding = true;
    bag.buc = buc;
    bag.weight = 0;
    bag.location = nethacked_arena::ItemLocation::CarriedBy(w.player_id);
    let id = w.arena.spawn_item(bag);
    let mut child = w
        .ruleset
        .create_item_record(
            "dagger",
            nethacked_arena::ItemLocation::InContainer(id),
            nethacked_types::Buc::Uncursed,
        )
        .unwrap();
    child.is_container = false;
    child.is_bag_of_holding = false;
    child.weight = child_weight;
    child.location = nethacked_arena::ItemLocation::InContainer(id);
    w.arena.spawn_item(child);
    w
}
#[test]
fn moving_burden_and_bags_apply_before_random() {
    let cap = raw_world().hero_weight_capacity();
    assert!(cap > 4);
    let burdened = bag_world(nethacked_types::Buc::Uncursed, 2 * (cap + 1));
    assert_eq!(burdened.hero_carried_weight(), cap + 1);
    compare_case(burdened, true, true);
    for buc in [
        nethacked_types::Buc::Blessed,
        nethacked_types::Buc::Uncursed,
        nethacked_types::Buc::Cursed,
    ] {
        let stressed = bag_world(buc, cap.saturating_mul(8));
        assert!(
            stressed.hero_encumbrance() >= nethacked_core::inventory::EncumbranceTier::Stressed
        );
        compare_case(stressed, true, false);
        compare_case(bag_world(buc, cap.saturating_mul(8)), false, true);
    }
}
#[test]
fn supported_nested_bags_use_recursive_weight() {
    for buc in [
        nethacked_types::Buc::Blessed,
        nethacked_types::Buc::Uncursed,
        nethacked_types::Buc::Cursed,
    ] {
        let mut w = bag_world(buc, 5);
        let inner = w
            .arena
            .items
            .iter()
            .find_map(|(id, item)| {
                matches!(item.location, nethacked_arena::ItemLocation::InContainer(_)).then_some(id)
            })
            .unwrap();
        w.arena.items[inner].is_container = true;
        w.arena.items[inner].is_bag_of_holding = true;
        w.arena.items[inner].buc = buc;
        let mut child = w
            .ruleset
            .create_item_record(
                "dagger",
                nethacked_arena::ItemLocation::InContainer(inner),
                nethacked_types::Buc::Uncursed,
            )
            .unwrap();
        child.weight = 17;
        w.arena.spawn_item(child);
        assert_eq!(
            w.hero_carried_weight(),
            nethacked_arena::carried_weight(&w.arena.items, w.player_id)
        );
        compare_case(w, true, true);
    }
}
#[test]
fn stored_strength_constitution_and_levitation_change_moving_capacity() {
    let mut weak = bag_world(nethacked_types::Buc::Uncursed, 700);
    weak.progress.attr_base[crate::progress::A_STR] = 3;
    weak.progress.attr_base[crate::progress::A_CON] = 3;
    assert_eq!(weak.hero_carried_weight(), 350);
    assert!(weak.hero_encumbrance() >= nethacked_core::inventory::EncumbranceTier::Stressed);
    compare_case(weak.clone(), true, false);
    for kind in 0..3 {
        let mut w = weak.clone();
        match kind {
            0 => w.progress.attr_base[crate::progress::A_STR] = 25,
            1 => w.progress.attr_base[crate::progress::A_CON] = 25,
            _ => w.arena.actors[w.player_id].intrinsics.levitation = true,
        }
        assert!(w.hero_encumbrance() < nethacked_core::inventory::EncumbranceTier::Stressed);
        compare_case(w, true, true);
    }
}
#[test]
fn guaranteed_heal_still_draws_and_replay_is_stale() {
    let mut w = raw_world();
    w.arena.actors[w.player_id].level = 100;
    w.progress.attr_base[crate::progress::A_CON] = 25;
    let c = config(&w);
    let b = OwnerBridge::install(NativePartitions::from_native(w.clone()), c.clone()).unwrap();
    let req = b.prepare_regeneration_phase(false).unwrap();
    let before = w.rng.get_word_pos();
    let reply = b.execute_regeneration_phase(req.clone()).unwrap();
    assert!(reply.healed);
    let native = b.export_native().unwrap();
    assert_eq!(native.arena.actors[native.player_id].hp, 4);
    assert!(native.rng.get_word_pos() > before);
    let after = value(&native);
    let history = history_len(&b);
    assert!(b.execute_regeneration_phase(req).is_err());
    assert_eq!(value(&b.export_native().unwrap()), after);
    assert_eq!(history_len(&b), history);
    let saved = b.capture().unwrap();
    let restored = OwnerBridge::reconstruct(&saved, c).unwrap();
    assert_eq!(value(&restored.export_native().unwrap()), after);
}
#[test]
fn different_actor_consequence_reaches_atomic_accounting_refusal() {
    let w = raw_world();
    let b = OwnerBridge::install(NativePartitions::from_native(w.clone()), config(&w)).unwrap();
    let before = owner_snapshot(&b);
    let prior = super::super::regeneration_ticks::accounting_refusals();
    let req = b.prepare_regeneration_phase(false).unwrap();
    b.execute_regeneration_phase(req).unwrap();
    assert_eq!(
        super::super::regeneration_ticks::accounting_refusals(),
        prior + 1
    );
    assert_eq!(owner_snapshot(&b).2, before.2 + 1);
}
fn regeneration_scope(b: &OwnerBridge, random_write: bool) -> Scope {
    b.session
        .with_runtime(&b.credential.borrow(), |s| {
            let mut keys = BTreeSet::new();
            keys.insert(
                s.owner::<HeroState>("Hero")
                    .unwrap()
                    .owner
                    .resource("regeneration_hero_i2c")?,
            );
            for n in ["wand_actors_facts_v2", "regeneration_heal_i2c"] {
                keys.insert(
                    s.owner::<ActorsState>("Actors")
                        .unwrap()
                        .owner
                        .resource(n)?,
                );
            }
            keys.insert(
                s.owner::<GeographyState>("Geography")
                    .unwrap()
                    .owner
                    .resource("wand_geography_facts_v2")?,
            );
            let random_ports = if random_write {
                &["regeneration_random_i2c", "regeneration_draw_i2c"][..]
            } else {
                &["regeneration_random_i2c"][..]
            };
            for n in random_ports {
                keys.insert(
                    s.owner::<RandomState>("Random")
                        .unwrap()
                        .owner
                        .resource(n)?,
                );
            }
            Ok::<_, owners::Error>(Scope::Keys(keys))
        })
        .unwrap()
        .unwrap()
}
#[test]
fn random_write_scope_refusal_is_atomic_before_publication() {
    let w = raw_world();
    let b = OwnerBridge::install(NativePartitions::from_native(w.clone()), config(&w)).unwrap();
    let req = b.prepare_regeneration_phase(false).unwrap();
    let scope = regeneration_scope(&b, false);
    let before = value(&b.export_native().unwrap());
    let history = history_len(&b);
    let accounting_before = super::super::regeneration_ticks::accounting_refusals();
    let error = b.execute_regeneration_phase_scoped(req, scope).unwrap_err();
    assert!(matches!(
        error,
        OwnerBridgeError::Kernel(kernel::Error::NativeRefusal)
    ));
    assert_eq!(value(&b.export_native().unwrap()), before);
    assert_eq!(history_len(&b), history);
    assert_eq!(
        super::super::regeneration_ticks::accounting_refusals(),
        accounting_before,
        "missing Random write scope refuses during prepare"
    );
}
#[test]
fn missing_publication_scope_refuses_before_prepare() {
    let w = raw_world();
    let b = OwnerBridge::install(NativePartitions::from_native(w.clone()), config(&w)).unwrap();
    let req = b.prepare_regeneration_phase(false).unwrap();
    let before = owner_snapshot(&b);
    let native = value(&b.export_native().unwrap());
    let accounting_before = super::super::regeneration_ticks::accounting_refusals();
    let error = b
        .execute_regeneration_phase_scoped(req, regeneration_scope(&b, true))
        .unwrap_err();
    assert!(matches!(
        error,
        OwnerBridgeError::Kernel(kernel::Error::NativeRefusal)
    ));
    assert_eq!(owner_snapshot(&b), before);
    assert_eq!(value(&b.export_native().unwrap()), native);
    assert_eq!(
        super::super::regeneration_ticks::accounting_refusals(),
        accounting_before
    );
}
#[test]
fn historical_profiles_restore_without_regeneration_grant() {
    let w = raw_world();
    let base = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone()).unwrap();
    for old in [
        base.clone().with_passive_metabolic_ticks(),
        base.clone().with_passive_affliction_ticks(),
        base.clone().with_default_commands(),
        base.clone().with_wand_gameplay(),
        base.clone().with_wand_commands(),
        base.with_wand_ports(),
    ] {
        let b = OwnerBridge::install(NativePartitions::from_native(w.clone()), old).unwrap();
        let saved = b.capture().unwrap();
        let restored = OwnerBridge::reconstruct(&saved, config(&w)).unwrap();
        assert!(restored.prepare_regeneration_phase(false).is_err());
        assert_eq!(value(&restored.export_native().unwrap()), value(&w));
    }
}

fn owner_snapshot(b: &OwnerBridge) -> (Vec<u8>, Vec<u8>, usize) {
    b.session
        .with_runtime(&b.credential.borrow(), |s| {
            Ok::<_, owners::Error>((
                s.owner::<ActorsState>("Actors").unwrap().owner.inspect()?,
                s.owner::<RandomState>("Random").unwrap().owner.inspect()?,
                s.host().history()?.len(),
            ))
        })
        .unwrap()
        .unwrap()
}
fn complete_owner_snapshot(b: &OwnerBridge) -> (Vec<Vec<u8>>, String) {
    b.session
        .with_runtime(&b.credential.borrow(), |s| {
            Ok::<_, owners::Error>((
                vec![
                    s.owner::<GeographyState>("Geography")
                        .unwrap()
                        .owner
                        .inspect()?,
                    s.owner::<ObjectsState>("Objects")
                        .unwrap()
                        .owner
                        .inspect()?,
                    s.owner::<ActorsState>("Actors").unwrap().owner.inspect()?,
                    s.owner::<HeroState>("Hero").unwrap().owner.inspect()?,
                    s.owner::<EquipmentState>("Equipment")
                        .unwrap()
                        .owner
                        .inspect()?,
                    s.owner::<BillingState>("Billing")
                        .unwrap()
                        .owner
                        .inspect()?,
                    s.owner::<ClockState>("Clock").unwrap().owner.inspect()?,
                    s.owner::<RandomState>("Random").unwrap().owner.inspect()?,
                    s.owner::<SessionState>("Session")
                        .unwrap()
                        .owner
                        .inspect()?,
                ],
                format!("{:?}", s.host().history()?),
            ))
        })
        .unwrap()
        .unwrap()
}
#[test]
fn missing_player_actor_skips_without_random_or_publication() {
    let mut w = raw_world();
    let id = w.player_id;
    w.arena.actors.remove(id);
    let b = OwnerBridge::install(NativePartitions::from_native(w.clone()), config(&w)).unwrap();
    let before = owner_snapshot(&b);
    let request = b.prepare_regeneration_phase(false).unwrap();
    assert!(!b.execute_regeneration_phase(request).unwrap().healed);
    assert_eq!(owner_snapshot(&b), before);
}
#[test]
fn carried_tree_overflow_refuses_before_random() {
    let mut w = raw_world();
    w.arena.items.clear();
    w.equipment = Default::default();
    w.wielded_item = None;
    w.unpaid_items.clear();
    w.billing = Default::default();
    let mut parent = None;
    for _ in 0..33 {
        let location = parent
            .map(nethacked_arena::ItemLocation::InContainer)
            .unwrap_or(nethacked_arena::ItemLocation::CarriedBy(w.player_id));
        let mut bag = w
            .ruleset
            .create_item_record("bag of holding", location, nethacked_types::Buc::Uncursed)
            .unwrap();
        bag.weight = 1;
        parent = Some(w.arena.spawn_item(bag));
    }
    let mut skipped = w.clone();
    skipped.arena.actors[skipped.player_id].hp = 30;
    let early = OwnerBridge::install(
        NativePartitions::from_native(skipped.clone()),
        config(&skipped),
    )
    .unwrap();
    let prior = owner_snapshot(&early);
    let request = early.prepare_regeneration_phase(true).unwrap();
    assert!(!early.execute_regeneration_phase(request).unwrap().healed);
    assert_eq!(owner_snapshot(&early), prior);
    let b = OwnerBridge::install(NativePartitions::from_native(w.clone()), config(&w)).unwrap();
    let before = owner_snapshot(&b);
    assert!(b.prepare_regeneration_phase(true).is_err());
    assert_eq!(owner_snapshot(&b), before);
}
fn shallow_inventory(count: usize) -> SimulationWorld {
    let mut w = raw_world();
    w.arena.items.clear();
    w.equipment = Default::default();
    w.wielded_item = None;
    w.hero.quivered_item = None;
    w.unpaid_items.clear();
    w.billing = Default::default();
    for _ in 0..count {
        let item = w
            .ruleset
            .create_item_record(
                "dagger",
                nethacked_arena::ItemLocation::CarriedBy(w.player_id),
                nethacked_types::Buc::Uncursed,
            )
            .unwrap();
        w.arena.spawn_item(item);
    }
    w
}
#[test]
fn shallow_carried_count_accepts_32_and_refuses_33() {
    use super::super::regeneration_ticks::RegenerationDomain;
    for count in [32, 33] {
        let w = shallow_inventory(count);
        let b = OwnerBridge::install(NativePartitions::from_native(w.clone()), config(&w)).unwrap();
        let prior = value(&b.export_native().unwrap());
        let snapshot = owner_snapshot(&b);
        let complete_snapshot = complete_owner_snapshot(&b);
        let owner_result = b
            .session
            .with_runtime(&b.credential.borrow(), |s| {
                let bytes = s
                    .owner::<ObjectsState>("Objects")
                    .unwrap()
                    .owner
                    .inspect()?;
                let state: ObjectsState = decode(&bytes, PAYLOAD)?;
                state.regeneration_query(
                    "regeneration_objects_i2c",
                    &encode(&w.player_id, wand::MAX_REQUEST_BYTES)?,
                    &config(&w),
                )
            })
            .unwrap();
        if count == 32 {
            assert!(owner_result.is_ok());
            assert!(b.prepare_regeneration_phase(true).is_ok());
        } else {
            assert!(matches!(owner_result, Err(owners::Error::Limit)));
            assert!(matches!(
                b.prepare_regeneration_phase(true),
                Err(OwnerBridgeError::Kernel(kernel::Error::NativeRefusal))
            ));
        }
        assert_eq!(value(&b.export_native().unwrap()), prior);
        assert_eq!(owner_snapshot(&b), snapshot);
        assert_eq!(complete_owner_snapshot(&b), complete_snapshot);
    }
}
#[test]
fn moving_early_skips_before_unsupported_shallow_inventory() {
    for case in 0..6 {
        let mut w = shallow_inventory(33);
        match case {
            0 => {
                w.arena.actors.remove(w.player_id);
            }
            1 => w.arena.actors[w.player_id].is_dead = true,
            2 => w.arena.actors[w.player_id].hp = 0,
            3 => w.arena.actors[w.player_id].hp = 30,
            4 => w.arena.actors[w.player_id].hp = 31,
            _ => {
                w.hero.polymorph = Some(nethacked_types::PolymorphForm {
                    monster_id: 1,
                    hp: 5,
                    max_hp: 10,
                    duration: 20,
                });
            }
        }
        let b = OwnerBridge::install(NativePartitions::from_native(w.clone()), config(&w)).unwrap();
        let before = (case != 0).then(|| value(&b.export_native().unwrap()));
        let all_before = complete_owner_snapshot(&b);
        let snapshot = owner_snapshot(&b);
        let request = b.prepare_regeneration_phase(true).unwrap();
        assert!(!b.execute_regeneration_phase(request).unwrap().healed);
        if let Some(before) = before {
            assert_eq!(value(&b.export_native().unwrap()), before);
        }
        assert_eq!(complete_owner_snapshot(&b), all_before);
        assert_eq!(owner_snapshot(&b), snapshot);
    }
}
#[test]
fn genuine_objects_revision_makes_moving_request_stale() {
    let mut w = raw_world();
    w.arena.items.clear();
    w.equipment = Default::default();
    w.wielded_item = None;
    w.hero.quivered_item = None;
    w.unpaid_items.clear();
    w.billing = Default::default();
    let mut wand = w
        .ruleset
        .create_item_record(
            "wand of fire",
            nethacked_arena::ItemLocation::CarriedBy(w.player_id),
            nethacked_types::Buc::Uncursed,
        )
        .unwrap();
    wand.enchantment = 3;
    let id = w.arena.spawn_item(wand);
    let b = OwnerBridge::install(NativePartitions::from_native(w.clone()), config(&w)).unwrap();
    let old = b.prepare_regeneration_phase(true).unwrap();
    let before = owner_snapshot(&b);
    let revision = b
        .session
        .with_runtime(&b.credential.borrow(), |s| {
            s.owner::<ObjectsState>("Objects")
                .unwrap()
                .owner
                .stamp()
                .map(|x| x.revision)
        })
        .unwrap()
        .unwrap();
    super::super::regeneration_ticks::mutate_object_on_run(id);
    let refused = b.execute_regeneration_phase(old);
    let after_revision = b
        .session
        .with_runtime(&b.credential.borrow(), |s| {
            s.owner::<ObjectsState>("Objects")
                .unwrap()
                .owner
                .stamp()
                .map(|x| x.revision)
        })
        .unwrap()
        .unwrap();
    assert_eq!(
        after_revision,
        revision + 1,
        "Objects must actually publish"
    );
    assert!(matches!(
        refused,
        Err(OwnerBridgeError::Kernel(kernel::Error::NativeRefusal))
    ));
    let after = owner_snapshot(&b);
    assert_eq!(after.0, before.0);
    assert_eq!(after.1, before.1);
    assert_eq!(after.2, before.2 + 1);
    let post_mutation = super::super::regeneration_ticks::object_mutation_state().unwrap();
    let current = b
        .session
        .with_runtime(&b.credential.borrow(), |s| {
            Ok::<_, owners::Error>((
                s.owner::<ObjectsState>("Objects")
                    .unwrap()
                    .owner
                    .inspect()?,
                s.owner::<ActorsState>("Actors").unwrap().owner.inspect()?,
                s.owner::<RandomState>("Random").unwrap().owner.inspect()?,
                s.host().history()?,
            ))
        })
        .unwrap()
        .unwrap();
    assert_eq!(
        current, post_mutation,
        "stale refusal must preserve the committed mutation, Actors, Random and whole history"
    );
    w.arena.items[id].enchantment = 2;
    assert_eq!(value(&b.export_native().unwrap()), value(&w));
}
