use super::super::*;
use crate::owner_persistence::runtime::inventory_actions::{self, InventoryAction};
use crate::{ActionAst, GameEvent};
use nethacked_arena::{ActorId, ItemLocation};
use nethacked_core::ac::ArmorSlot;
use nethacked_core::engraving::{Engraving, EngravingMedium};
use nethacked_types::{Buc, Coord, Direction, Tile};
use rand::SeedableRng;

fn clean_world(seed: u64) -> SimulationWorld {
    let mut w = SimulationWorld::new_with_seed(seed);
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.arena.items = Default::default();
    w.equipment = Default::default();
    w.wielded_item = None;
    w.hero.quivered_item = None;
    w.unpaid_items.clear();
    w.hero.skills.skills.clear();
    w.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Room);
    w.arena.actors[w.player_id].coord = Coord::new_unchecked(10, 10);
    w.arena.actors[w.player_id].level = 10;
    w.arena.actors[w.player_id].hp = 50;
    w.arena.actors[w.player_id].max_hp = 50;
    w.arena.actors[w.player_id].ac = 10;
    w.depth = 5;
    w.scheduler.turn = 100;
    w.scheduler.hero_energy = 12;
    w.scheduler.hero_speed = 12;
    w.rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
    w
}

fn config(w: &SimulationWorld) -> OwnerBridgeConfig {
    OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_inventory_actions()
}

fn install_bridge(w: &SimulationWorld) -> OwnerBridge {
    OwnerBridge::install(NativePartitions::from_native(w.clone()), config(w)).unwrap()
}

fn carry(w: &mut SimulationWorld, name: &str, buc: Buc) -> ItemId {
    let item = w
        .ruleset
        .create_item_record(name, ItemLocation::CarriedBy(w.player_id), buc)
        .unwrap();
    w.arena.spawn_item(item)
}

fn floor_item(w: &mut SimulationWorld, name: &str, coord: Coord) -> ItemId {
    let item = w
        .ruleset
        .create_item_record(name, ItemLocation::Floor(coord), Buc::Uncursed)
        .unwrap();
    w.arena.spawn_item(item)
}

fn spawn_goblin(w: &mut SimulationWorld, coord: Coord) -> ActorId {
    let mut goblin = w
        .ruleset
        .create_monster_record("goblin", coord, &mut w.rng)
        .unwrap();
    goblin.hp = 20;
    goblin.max_hp = 20;
    goblin.movement = 12;
    w.arena.spawn_actor(goblin)
}

#[test]
fn inventory_manifest_bounds_and_profiles() {
    for inventory in [false, true] {
        let w = SimulationWorld::new_with_seed(721);
        let mut c = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_timer_ticks();
        if inventory {
            c = c.with_inventory_actions();
        }
        assert_eq!(profile_caller(&c).dependencies.len(), 64);
        assert_eq!(profile_provider(&c).exports.len(), 64);
        OwnerBridge::install(NativePartitions::from_native(w), c).unwrap();
    }

    // Microkernel transport manifests fit well within 64-port bounds
    assert!(inventory_actions::READS.len() + inventory_actions::WRITES.len() + 2 <= 64);
    assert_eq!(
        inventory_actions::READS.len() + inventory_actions::WRITES.len() + 2,
        18
    );
}

#[test]
fn inventory_authentic_stale_refused() {
    let mut w = clean_world(804);
    floor_item(&mut w, "rock", Coord::new_unchecked(10, 10));
    let bridge = install_bridge(&w);
    let mut req = bridge.prepare_inventory(InventoryAction::PickUp).unwrap();

    // Tampering with witness stamp triggers authentic stale refusal check
    req.guards[0].generation = req.guards[0].generation.wrapping_add(1);
    let err = bridge.execute_inventory(req);
    assert!(matches!(
        err,
        Err(OwnerBridgeError::Owner(owners::Error::Stale))
    ));
    assert!(inventory_actions::stale_refusals() >= 1);
}

#[test]
fn inventory_wrong_authority_refused() {
    let mut w = clean_world(804);
    floor_item(&mut w, "rock", Coord::new_unchecked(10, 10));
    let bridge = install_bridge(&w);
    let req = bridge.prepare_inventory(InventoryAction::PickUp).unwrap();

    inventory_actions::trigger_wrong_resource(true);
    let res = bridge.execute_inventory(req.clone());
    inventory_actions::trigger_wrong_resource(false);

    assert!(res.is_err());

    // State remains uncorrupted and request can still be executed successfully with valid authority
    let reply = bridge.execute_inventory(req).unwrap();
    assert!(reply.spent_time);
}

#[test]
fn inventory_zero_rounds_when_hero_dead() {
    let mut w = clean_world(804);
    w.arena.actors[w.player_id].is_dead = true;
    w.arena.actors[w.player_id].hp = 0;
    let bridge = install_bridge(&w);

    let reply = bridge.step_inventory(InventoryAction::PickUp).unwrap();
    assert!(!reply.spent_time);
    assert_eq!(reply.rounds, 0);
    assert!(reply.events.is_empty());
}

#[test]
fn inventory_pickup_floor_item() {
    let mut w = clean_world(804);
    let rock = floor_item(&mut w, "rock", Coord::new_unchecked(10, 10));
    let bridge = install_bridge(&w);

    let reply = bridge.step_inventory(InventoryAction::PickUp).unwrap();
    assert!(reply.spent_time);
    assert_eq!(reply.rounds, 1);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::ItemPickedUp { actor, item }
            if *actor == w.player_id && *item == rock
    )));

    let parts = bridge.partitions().unwrap();
    assert_eq!(
        parts.objects.items.get(rock).unwrap().location,
        ItemLocation::CarriedBy(w.player_id)
    );
}

#[test]
fn inventory_pickup_nothing_spends_no_time() {
    let w = clean_world(804);
    let bridge = install_bridge(&w);

    let reply = bridge.step_inventory(InventoryAction::PickUp).unwrap();
    assert!(!reply.spent_time);
    assert_eq!(reply.rounds, 0);
    assert_eq!(reply.events.len(), 1);
    assert!(matches!(reply.events[0], GameEvent::LogMessage { .. }));
}

#[test]
fn inventory_pickup_too_heavy_spends_time() {
    let mut w = clean_world(804);
    carry(&mut w, "boulder", Buc::Uncursed);
    let rock = floor_item(&mut w, "rock", Coord::new_unchecked(10, 10));
    let bridge = install_bridge(&w);

    let reply = bridge.step_inventory(InventoryAction::PickUp).unwrap();
    // C pickup refuses an unliftable item but still spends the turn.
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("carrying too much")
    )));

    let parts = bridge.partitions().unwrap();
    assert_eq!(
        parts.objects.items.get(rock).unwrap().location,
        ItemLocation::Floor(Coord::new_unchecked(10, 10))
    );
}

#[test]
fn inventory_drop_carried_item() {
    let mut w = clean_world(804);
    let rock = carry(&mut w, "rock", Buc::Uncursed);
    let bridge = install_bridge(&w);

    let reply = bridge.step_inventory(InventoryAction::Drop(0)).unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::ItemDropped { actor, item }
            if *actor == w.player_id && *item == rock
    )));

    let parts = bridge.partitions().unwrap();
    assert_eq!(
        parts.objects.items.get(rock).unwrap().location,
        ItemLocation::Floor(Coord::new_unchecked(10, 10))
    );
}

#[test]
fn inventory_drop_worn_suit_refused() {
    let mut w = clean_world(804);
    let suit = carry(&mut w, "gray dragon scale mail", Buc::Uncursed);
    w.equipment.armor.push((ArmorSlot::Suit, suit));
    let bridge = install_bridge(&w);

    let reply = bridge.step_inventory(InventoryAction::Drop(0)).unwrap();
    // C canletgo: no time and no transfer for a worn suit.
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("cannot drop something you are wearing")
    )));

    let parts = bridge.partitions().unwrap();
    assert_eq!(
        parts.objects.items.get(suit).unwrap().location,
        ItemLocation::CarriedBy(w.player_id)
    );
}

#[test]
fn inventory_wield_carried_item() {
    let mut w = clean_world(804);
    let dagger = carry(&mut w, "dagger", Buc::Uncursed);
    let bridge = install_bridge(&w);

    let reply = bridge.step_inventory(InventoryAction::Wield(0)).unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::ItemWielded { actor, item }
            if *actor == w.player_id && *item == dagger
    )));

    let parts = bridge.partitions().unwrap();
    assert_eq!(parts.equipment.wielded_item, Some(dagger));
}

#[test]
fn inventory_wear_suit_starts_dressing() {
    let mut w = clean_world(804);
    let suit = carry(&mut w, "gray dragon scale mail", Buc::Uncursed);
    let bridge = install_bridge(&w);

    let reply = bridge.step_inventory(InventoryAction::Wear(0)).unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::DressingStarted { item, putting_on }
            if *item == suit && *putting_on
    )));
    // oc_delay 5: dressing is in progress, not finished.
    assert!(!reply
        .events
        .iter()
        .any(|e| matches!(e, GameEvent::DressingFinished { .. })));

    let parts = bridge.partitions().unwrap();
    let dressing = parts.equipment.equipment.dressing.as_ref().unwrap();
    assert_eq!(dressing.item, suit);
    assert!(dressing.putting_on);
    // oc_delay 5, ticked once by the turn loop's passive dressing pass.
    assert_eq!(dressing.turns_left, 4);
    assert_eq!(parts.equipment.equipment.worn(ArmorSlot::Suit), Some(suit));
}

#[test]
fn inventory_remove_cursed_suit_refused() {
    let mut w = clean_world(804);
    let suit = carry(&mut w, "gray dragon scale mail", Buc::Cursed);
    w.equipment.armor.push((ArmorSlot::Suit, suit));
    let bridge = install_bridge(&w);

    let reply = bridge.step_inventory(InventoryAction::Remove(0)).unwrap();
    // C select_off curse check: ECMD_OK, no turn spent.
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("cursed")
    )));

    let parts = bridge.partitions().unwrap();
    assert_eq!(parts.equipment.equipment.worn(ArmorSlot::Suit), Some(suit));
}

#[test]
fn inventory_put_and_take_from_container() {
    let mut w = clean_world(804);
    let ice_box = carry(&mut w, "ice box", Buc::Uncursed);
    let rock = carry(&mut w, "rock", Buc::Uncursed);
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_inventory(InventoryAction::PutInContainer {
            item_index: 1,
            container_index: 0,
        })
        .unwrap();
    assert!(reply.spent_time);
    let parts = bridge.partitions().unwrap();
    assert_eq!(
        parts.objects.items.get(rock).unwrap().location,
        ItemLocation::InContainer(ice_box)
    );

    let reply = bridge
        .step_inventory(InventoryAction::TakeFromContainer {
            container_index: 0,
            item_index: 0,
        })
        .unwrap();
    assert!(reply.spent_time);
    let parts = bridge.partitions().unwrap();
    assert_eq!(
        parts.objects.items.get(rock).unwrap().location,
        ItemLocation::CarriedBy(w.player_id)
    );
}

#[test]
fn inventory_put_into_non_container_refused() {
    let mut w = clean_world(804);
    let rock = carry(&mut w, "rock", Buc::Uncursed);
    carry(&mut w, "dagger", Buc::Uncursed);
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_inventory(InventoryAction::PutInContainer {
            item_index: 0,
            container_index: 1,
        })
        .unwrap();
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("is not a container")
    )));

    let parts = bridge.partitions().unwrap();
    assert_eq!(
        parts.objects.items.get(rock).unwrap().location,
        ItemLocation::CarriedBy(w.player_id)
    );
}

#[test]
fn inventory_corpse_ice_box_freeze_transitions() {
    let mut w = clean_world(804);
    let ice_box = carry(&mut w, "ice box", Buc::Uncursed);
    let corpse = w
        .ruleset
        .create_corpse_record("goblin", ItemLocation::CarriedBy(w.player_id))
        .unwrap();
    let corpse = w.arena.spawn_item(corpse);
    let bridge = install_bridge(&w);

    // Into the ice box: frozen, any rot timer cancelled.
    let reply = bridge
        .step_inventory(InventoryAction::PutInContainer {
            item_index: 1,
            container_index: 0,
        })
        .unwrap();
    assert!(reply.spent_time);
    let parts = bridge.partitions().unwrap();
    let item = parts.objects.items.get(corpse).unwrap();
    assert!(nethacked_arena::frozen_in_ice_box(item, |parent| parts
        .objects
        .items
        .get(parent)));
    assert!(
        !parts.clock.object_timers.entries().iter().any(|t| matches!(
            t.target,
            crate::object_timers::ObjectTimerTarget::Active(id) if id == corpse
        ))
    );

    // Out of the ice box: thawed, a fresh rot timer is scheduled.
    let reply = bridge
        .step_inventory(InventoryAction::TakeFromContainer {
            container_index: 0,
            item_index: 0,
        })
        .unwrap();
    assert!(reply.spent_time);
    let parts = bridge.partitions().unwrap();
    let item = parts.objects.items.get(corpse).unwrap();
    assert!(!nethacked_arena::frozen_in_ice_box(item, |parent| parts
        .objects
        .items
        .get(parent)));
    assert!(parts.clock.object_timers.entries().iter().any(|t| matches!(
        (t.target, t.kind),
        (
            crate::object_timers::ObjectTimerTarget::Active(id),
            crate::object_timers::ObjectTimerKind::RotCorpse
        ) if id == corpse
    )));
    let _ = ice_box;
}

#[test]
fn inventory_bag_of_holding_explosion() {
    let mut w = clean_world(804);
    let boh = carry(&mut w, "bag of holding", Buc::Uncursed);
    let wand = carry(&mut w, "wand of cancellation", Buc::Uncursed);
    let bridge = install_bridge(&w);

    // A charged wand of cancellation inserted into a BoH always detonates at
    // depth 0 (mbag_explodes bound 1).
    let reply = bridge
        .step_inventory(InventoryAction::PutInContainer {
            item_index: 1,
            container_index: 0,
        })
        .unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("explodes")
    )));

    let parts = bridge.partitions().unwrap();
    // The bag and the inserted wand are destroyed; the hero takes d(6,6).
    assert!(parts.objects.items.get(boh).is_none());
    assert!(parts.objects.items.get(wand).is_none());
    let hero = &parts.actors.actors[w.player_id];
    assert!(hero.hp < 50);
    assert!(hero.hp > 0);
    assert!(!hero.is_dead);
}

#[test]
fn inventory_quiver_carried_item() {
    let mut w = clean_world(804);
    let dart = carry(&mut w, "dart", Buc::Uncursed);
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_inventory(InventoryAction::Quiver(dart))
        .unwrap();
    assert!(reply.spent_time);

    let parts = bridge.partitions().unwrap();
    assert_eq!(parts.hero.hero.quivered_item, Some(dart));
}

#[test]
fn inventory_quiver_worn_suit_refused() {
    let mut w = clean_world(804);
    let suit = carry(&mut w, "gray dragon scale mail", Buc::Uncursed);
    w.equipment.armor.push((ArmorSlot::Suit, suit));
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_inventory(InventoryAction::Quiver(suit))
        .unwrap();
    // C doquiver_core refuses worn armor with ECMD_OK: no time spent.
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("cannot ready that")
    )));

    let parts = bridge.partitions().unwrap();
    assert_eq!(parts.hero.hero.quivered_item, None);
}

#[test]
fn inventory_fire_hits_adjacent_monster() {
    let mut w = clean_world(804);
    let dart = carry(&mut w, "dart", Buc::Uncursed);
    w.hero.quivered_item = Some(dart);
    let goblin = spawn_goblin(&mut w, Coord::new_unchecked(11, 10));
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_inventory(InventoryAction::Fire(Direction::East))
        .unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::AttackLanded {
            attacker,
            target,
            damage,
            lethal
        } if *attacker == w.player_id && *target == goblin && *damage == 2 && !lethal
    )));

    let parts = bridge.partitions().unwrap();
    // hmon -> wakeup -> setmangry: the goblin turns hostile.
    assert!(!parts.actors.actors[goblin].is_peaceful);
    assert_eq!(parts.actors.actors[goblin].hp, 18);
    // The quiver slot is cleared on every fired path.
    assert_eq!(parts.hero.hero.quivered_item, None);
}

#[test]
fn inventory_fire_empty_quiver_refused() {
    let w = clean_world(804);
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_inventory(InventoryAction::Fire(Direction::East))
        .unwrap();
    assert!(!reply.spent_time);
    assert_eq!(reply.events.len(), 1);
    assert!(matches!(reply.events[0], GameEvent::LogMessage { .. }));
}

#[test]
fn inventory_fire_elbereth_breaks_alignment() {
    let mut w = clean_world(804);
    w.alignment_record = 10;
    let dart = carry(&mut w, "dart", Buc::Uncursed);
    w.hero.quivered_item = Some(dart);
    w.level.set_engraving(
        Coord::new_unchecked(10, 10),
        Engraving::new("Elbereth", EngravingMedium::Burned),
    );
    let goblin = spawn_goblin(&mut w, Coord::new_unchecked(11, 10));
    w.arena.actors[goblin].is_peaceful = true;
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_inventory(InventoryAction::Fire(Direction::East))
        .unwrap();
    assert!(reply.spent_time);

    let parts = bridge.partitions().unwrap();
    // Attacking a peaceful monster on Elbereth: -5 hypocrite (record > 5),
    // then -1 for angering it; the engraving fades.
    assert_eq!(parts.hero.alignment_record, 4);
    assert!(parts
        .geography
        .level
        .get_engraving(Coord::new_unchecked(10, 10))
        .is_none());
    assert!(!parts.actors.actors[goblin].is_peaceful);
}

#[test]
fn inventory_reload_and_continuation() {
    let mut w = clean_world(804);
    let rock_a = carry(&mut w, "rock", Buc::Uncursed);
    floor_item(&mut w, "rock", Coord::new_unchecked(10, 10));
    let bridge = install_bridge(&w);

    let reply1 = bridge.step_inventory(InventoryAction::PickUp).unwrap();
    assert_eq!(reply1.rounds, 1);

    // Save
    let wire = bridge.capture().unwrap();

    // Reconstruct into fresh bridge
    let reloaded = OwnerBridge::reconstruct(&wire, config(&w)).unwrap();

    // Continue with a drop on the reloaded bridge
    let reply2 = reloaded.step_inventory(InventoryAction::Drop(0)).unwrap();
    assert_eq!(reply2.rounds, 1);

    let parts = reloaded.partitions().unwrap();
    assert_eq!(parts.clock.scheduler.turn, 102);
    assert_eq!(
        parts.objects.items.get(rock_a).unwrap().location,
        ItemLocation::Floor(Coord::new_unchecked(10, 10))
    );
}

// ---------------------------------------------------------------------------
// Legacy-oracle parity: the bridge must reproduce the native
// `step_player_action` bit-for-bit — event stream, full world state and RNG
// stream. The one documented exception is the session event log: legacy
// `record_events` appends to it, while the inventory transport keeps Session
// read-only, so the field is normalised before comparing save snapshots.
// ---------------------------------------------------------------------------

fn oracle_json(w: &SimulationWorld) -> serde_json::Value {
    let mut v: serde_json::Value = serde_json::from_str(&w.to_save_json().unwrap()).unwrap();
    if let Some(obj) = v.as_object_mut() {
        obj.insert("event_log".into(), serde_json::Value::Array(vec![]));
    }
    v
}

fn run_inventory_oracle(
    w: &SimulationWorld,
    ast: ActionAst,
    action: InventoryAction,
) -> (inventory_actions::InventoryReply, SimulationWorld) {
    let mut legacy = w.clone();
    let bridge = install_bridge(w);
    let events = legacy.step_player_action(ast);
    let reply = bridge.step_inventory(action).unwrap();
    assert_eq!(reply.events, events, "event stream diverged from legacy");
    let after = bridge.export_native().unwrap();
    assert_eq!(
        oracle_json(&after),
        oracle_json(&legacy),
        "post-action world state diverged from legacy"
    );
    (reply, legacy)
}

#[test]
fn inventory_oracle_pickup_matches_legacy() {
    let mut w = clean_world(804);
    carry(&mut w, "dagger", Buc::Uncursed);
    let rock = floor_item(&mut w, "rock", Coord::new_unchecked(10, 10));
    let (reply, _) = run_inventory_oracle(&w, ActionAst::PickUp, InventoryAction::PickUp);
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::ItemPickedUp { item, .. } if *item == rock
    )));
}

#[test]
fn inventory_oracle_drop_matches_legacy() {
    let mut w = clean_world(804);
    carry(&mut w, "rock", Buc::Uncursed);
    let dagger = carry(&mut w, "dagger", Buc::Uncursed);
    let (reply, _) = run_inventory_oracle(&w, ActionAst::Drop(1), InventoryAction::Drop(1));
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::ItemDropped { item, .. } if *item == dagger
    )));
}

#[test]
fn inventory_oracle_wield_matches_legacy() {
    let mut w = clean_world(804);
    let dagger = carry(&mut w, "dagger", Buc::Uncursed);
    let (reply, _) = run_inventory_oracle(&w, ActionAst::Wield(0), InventoryAction::Wield(0));
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::ItemWielded { item, .. } if *item == dagger
    )));
}

#[test]
fn inventory_oracle_wear_matches_legacy() {
    let mut w = clean_world(804);
    let suit = carry(&mut w, "gray dragon scale mail", Buc::Uncursed);
    let (reply, _) = run_inventory_oracle(&w, ActionAst::Wear(0), InventoryAction::Wear(0));
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::DressingStarted { item, putting_on }
            if *item == suit && *putting_on
    )));
}

#[test]
fn inventory_oracle_remove_cursed_matches_legacy() {
    let mut w = clean_world(804);
    let suit = carry(&mut w, "gray dragon scale mail", Buc::Cursed);
    w.equipment.armor.push((ArmorSlot::Suit, suit));
    let (reply, _) = run_inventory_oracle(&w, ActionAst::Remove(0), InventoryAction::Remove(0));
    assert!(!reply.spent_time);
}

#[test]
fn inventory_oracle_put_take_container_match_legacy() {
    let mut w = clean_world(804);
    carry(&mut w, "ice box", Buc::Uncursed);
    let rock = carry(&mut w, "rock", Buc::Uncursed);

    let (reply, legacy) = run_inventory_oracle(
        &w,
        ActionAst::PutInContainer {
            item_index: 1,
            container_index: 0,
        },
        InventoryAction::PutInContainer {
            item_index: 1,
            container_index: 0,
        },
    );
    assert!(reply.spent_time);
    assert_eq!(
        legacy.arena.items.get(rock).unwrap().location,
        ItemLocation::InContainer(legacy.arena.items_carried_by(legacy.player_id)[0])
    );

    let (reply, _) = run_inventory_oracle(
        &legacy,
        ActionAst::TakeFromContainer {
            container_index: 0,
            item_index: 0,
        },
        InventoryAction::TakeFromContainer {
            container_index: 0,
            item_index: 0,
        },
    );
    assert!(reply.spent_time);
}

#[test]
fn inventory_oracle_ice_box_freeze_cycle_matches_legacy() {
    let mut w = clean_world(804);
    carry(&mut w, "ice box", Buc::Uncursed);
    let corpse = w
        .ruleset
        .create_corpse_record("goblin", ItemLocation::CarriedBy(w.player_id))
        .unwrap();
    let corpse = w.arena.spawn_item(corpse);

    // Into the ice box: frozen, rot timer cancelled.
    let (_, legacy) = run_inventory_oracle(
        &w,
        ActionAst::PutInContainer {
            item_index: 1,
            container_index: 0,
        },
        InventoryAction::PutInContainer {
            item_index: 1,
            container_index: 0,
        },
    );
    assert!(nethacked_arena::frozen_in_ice_box(
        legacy.arena.items.get(corpse).unwrap(),
        |parent| legacy.arena.items.get(parent)
    ));

    // Out of the ice box: thawed, a fresh rot timer is scheduled. The timer
    // sampling draws from the shared RNG stream, so this also locks RNG parity
    // across the freeze transition.
    let (reply, legacy) = run_inventory_oracle(
        &legacy,
        ActionAst::TakeFromContainer {
            container_index: 0,
            item_index: 0,
        },
        InventoryAction::TakeFromContainer {
            container_index: 0,
            item_index: 0,
        },
    );
    assert!(reply.spent_time);
    assert!(!nethacked_arena::frozen_in_ice_box(
        legacy.arena.items.get(corpse).unwrap(),
        |parent| legacy.arena.items.get(parent)
    ));
    assert!(legacy.object_timers.entries().iter().any(|t| matches!(
        (t.target, t.kind),
        (
            crate::object_timers::ObjectTimerTarget::Active(id),
            crate::object_timers::ObjectTimerKind::RotCorpse
        ) if id == corpse
    )));
}

#[test]
fn inventory_oracle_boh_explosion_matches_legacy() {
    let mut w = clean_world(804);
    let boh = carry(&mut w, "bag of holding", Buc::Uncursed);
    carry(&mut w, "wand of cancellation", Buc::Uncursed);
    let (reply, legacy) = run_inventory_oracle(
        &w,
        ActionAst::PutInContainer {
            item_index: 1,
            container_index: 0,
        },
        InventoryAction::PutInContainer {
            item_index: 1,
            container_index: 0,
        },
    );
    assert!(reply.spent_time);
    assert!(legacy.arena.items.get(boh).is_none());
    assert!(legacy.arena.actors[legacy.player_id].hp < 50);
}

#[test]
fn inventory_oracle_quiver_fire_match_legacy() {
    let mut w = clean_world(804);
    let dart = carry(&mut w, "dart", Buc::Uncursed);
    w.hero.quivered_item = Some(dart);
    let goblin = spawn_goblin(&mut w, Coord::new_unchecked(11, 10));

    let (reply, legacy) = run_inventory_oracle(
        &w,
        ActionAst::Fire(Direction::East),
        InventoryAction::Fire(Direction::East),
    );
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::AttackLanded { target, .. } if *target == goblin
    )));
    assert_eq!(legacy.arena.actors[goblin].hp, 18);
    assert_eq!(legacy.hero.quivered_item, None);
}

#[test]
fn inventory_oracle_fire_elbereth_matches_legacy() {
    let mut w = clean_world(804);
    w.alignment_record = 10;
    let dart = carry(&mut w, "dart", Buc::Uncursed);
    w.hero.quivered_item = Some(dart);
    w.level.set_engraving(
        Coord::new_unchecked(10, 10),
        Engraving::new("Elbereth", EngravingMedium::Burned),
    );
    let goblin = spawn_goblin(&mut w, Coord::new_unchecked(11, 10));
    w.arena.actors[goblin].is_peaceful = true;

    let (reply, legacy) = run_inventory_oracle(
        &w,
        ActionAst::Fire(Direction::East),
        InventoryAction::Fire(Direction::East),
    );
    assert!(reply.spent_time);
    assert_eq!(legacy.alignment_record, 4);
    assert!(legacy
        .level
        .get_engraving(Coord::new_unchecked(10, 10))
        .is_none());
    assert!(!legacy.arena.actors[goblin].is_peaceful);
}

#[test]
fn inventory_oracle_helpless_substitutes_wait_matches_legacy() {
    let mut w = clean_world(804);
    w.hero.afflictions.transient.helpless = 2;
    let rock = floor_item(&mut w, "rock", Coord::new_unchecked(10, 10));

    // C multi < 0: the pickup degenerates to a stationary wait; the rock
    // stays on the floor and no pickup event is emitted.
    let (reply, legacy) = run_inventory_oracle(&w, ActionAst::PickUp, InventoryAction::PickUp);
    assert!(reply.spent_time);
    assert!(!reply
        .events
        .iter()
        .any(|e| matches!(e, GameEvent::ItemPickedUp { .. })));
    assert_eq!(
        legacy.arena.items.get(rock).unwrap().location,
        ItemLocation::Floor(Coord::new_unchecked(10, 10))
    );
}

#[test]
fn inventory_oracle_prayer_timeout_ticks_only_when_time_spent() {
    // No time spent: the legacy `if spent_time` guard leaves the timeout alone.
    let mut w = clean_world(804);
    w.divine_state.prayer_timeout = 5;
    let mut legacy = w.clone();
    let bridge = install_bridge(&w);
    let events = legacy.step_player_action(ActionAst::PickUp);
    let reply = bridge.step_inventory(InventoryAction::PickUp).unwrap();
    assert!(!reply.spent_time);
    assert_eq!(reply.events, events);
    let after = bridge.export_native().unwrap();
    assert_eq!(oracle_json(&after), oracle_json(&legacy));
    assert_eq!(after.divine_state.prayer_timeout, 5);

    // Time spent: the timeout ticks down exactly once.
    let mut w = clean_world(804);
    w.divine_state.prayer_timeout = 5;
    floor_item(&mut w, "rock", Coord::new_unchecked(10, 10));
    let mut legacy = w.clone();
    let bridge = install_bridge(&w);
    let events = legacy.step_player_action(ActionAst::PickUp);
    let reply = bridge.step_inventory(InventoryAction::PickUp).unwrap();
    assert!(reply.spent_time);
    assert_eq!(reply.events, events);
    let after = bridge.export_native().unwrap();
    assert_eq!(oracle_json(&after), oracle_json(&legacy));
    assert_eq!(after.divine_state.prayer_timeout, 4);
}
