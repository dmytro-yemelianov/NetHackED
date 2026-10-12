use super::super::*;
use crate::owner_persistence::runtime::services::{self, ServiceAction};
use crate::{ActionAst, GameEvent};
use nethacked_arena::{ActorId, ItemLocation};
use nethacked_core::SpellKind;
use nethacked_types::{Alignment, Buc, Coord, Direction, MountState, Tile};
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
        .with_services()
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

/// Spawn a goblin record renamed to `name` (priest / shopkeeper stand-ins).
fn spawn_named(w: &mut SimulationWorld, name: &str, coord: Coord) -> ActorId {
    let mut actor = w
        .ruleset
        .create_monster_record("goblin", coord, &mut w.rng)
        .unwrap();
    actor.name = name.into();
    actor.hp = 20;
    actor.max_hp = 20;
    actor.movement = 12;
    w.arena.spawn_actor(actor)
}

/// Place an altar tile under the hero at (10, 10).
fn set_altar(w: &mut SimulationWorld, tile: Tile) {
    w.level.set_tile(Coord::new_unchecked(10, 10), tile);
}

#[test]
fn services_manifest_bounds_and_profiles() {
    for enabled in [false, true] {
        let w = SimulationWorld::new_with_seed(721);
        let mut c = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_timer_ticks();
        if enabled {
            c = c.with_services();
        }
        assert_eq!(profile_caller(&c).dependencies.len(), 64);
        assert_eq!(profile_provider(&c).exports.len(), 64);
        OwnerBridge::install(NativePartitions::from_native(w), c).unwrap();
    }

    // Microkernel transport manifests fit well within 64-port bounds
    assert!(services::READS.len() + services::WRITES.len() + 2 <= 64);
    assert_eq!(services::READS.len() + services::WRITES.len() + 2, 19);
}

#[test]
fn services_authentic_stale_refused() {
    let w = clean_world(804);
    let bridge = install_bridge(&w);
    let mut req = bridge.prepare_service(ServiceAction::Pray).unwrap();

    // Tampering with witness stamp triggers authentic stale refusal check
    req.guards[0].generation = req.guards[0].generation.wrapping_add(1);
    let err = bridge.execute_service(req);
    assert!(matches!(
        err,
        Err(OwnerBridgeError::Owner(owners::Error::Stale))
    ));
    assert!(services::stale_refusals() >= 1);
}

#[test]
fn services_wrong_authority_refused() {
    let w = clean_world(804);
    let bridge = install_bridge(&w);
    let req = bridge.prepare_service(ServiceAction::Pray).unwrap();

    services::trigger_wrong_resource(true);
    let res = bridge.execute_service(req.clone());
    services::trigger_wrong_resource(false);

    assert!(res.is_err());

    // State remains uncorrupted and request can still be executed successfully with valid authority
    let reply = bridge.execute_service(req).unwrap();
    assert!(reply.spent_time);
}

#[test]
fn services_zero_rounds_when_hero_dead() {
    let mut w = clean_world(804);
    w.arena.actors[w.player_id].is_dead = true;
    w.arena.actors[w.player_id].hp = 0;
    let bridge = install_bridge(&w);

    let reply = bridge.step_service(ServiceAction::Pray).unwrap();
    assert!(!reply.spent_time);
    assert_eq!(reply.rounds, 0);
    assert!(reply.events.is_empty());
}

#[test]
fn services_cast_force_bolt_slays_goblin() {
    let mut w = clean_world(804);
    w.known_spells.push((SpellKind::ForceBolt, 0));
    w.player_pw = 50;
    let goblin = spawn_goblin(&mut w, Coord::new_unchecked(11, 10));
    w.arena.actors[goblin].hp = 10;
    w.arena.actors[goblin].max_hp = 10;
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_service(ServiceAction::Cast {
            spell_index: 0,
            dir: Direction::East,
        })
        .unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::AttackLanded {
            target,
            lethal: true,
            ..
        } if *target == goblin
    )));
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("is slain by magic!")
    )));
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("You cast ForceBolt!")
    )));

    let parts = bridge.partitions().unwrap();
    assert!(parts.actors.actors[goblin].is_dead);
    assert_eq!(
        parts.hero.player_pw,
        50 - nethacked_core::mana_cost(SpellKind::ForceBolt)
    );
    // The kill drops a corpse where the goblin fell.
    assert!(parts.objects.items.values().any(|it| {
        it.is_corpse() && it.location == ItemLocation::Floor(Coord::new_unchecked(11, 10))
    }));
}

#[test]
fn services_cast_insufficient_mana_refused() {
    let mut w = clean_world(804);
    w.known_spells.push((SpellKind::ForceBolt, 0));
    w.player_pw = 3;
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_service(ServiceAction::Cast {
            spell_index: 0,
            dir: Direction::East,
        })
        .unwrap();
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("don't have enough mana")
    )));

    let parts = bridge.partitions().unwrap();
    assert_eq!(parts.hero.player_pw, 3);
}

#[test]
fn services_cast_unknown_spell_refused() {
    let w = clean_world(804);
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_service(ServiceAction::Cast {
            spell_index: 0,
            dir: Direction::East,
        })
        .unwrap();
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("do not know that spell")
    )));
}

#[test]
fn services_cast_cure_light_wounds_heals() {
    let mut w = clean_world(804);
    w.known_spells.push((SpellKind::CureLightWounds, 0));
    w.player_pw = 50;
    w.arena.actors[w.player_id].hp = 30;
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_service(ServiceAction::Cast {
            spell_index: 0,
            dir: Direction::East,
        })
        .unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("You cast Cure Light Wounds!")
    )));

    let parts = bridge.partitions().unwrap();
    // Cure light heals 12 (30 -> 42); the turn loop's natural regeneration may add 1.
    assert!(parts.actors.actors[w.player_id].hp >= 42);
    assert_eq!(
        parts.hero.player_pw,
        50 - nethacked_core::mana_cost(SpellKind::CureLightWounds)
    );
}

#[test]
fn services_pray_coaligned_altar_heals_and_consecrates_water() {
    let mut w = clean_world(804);
    set_altar(
        &mut w,
        Tile::Altar {
            align: Alignment::Neutral,
        },
    );
    w.arena.actors[w.player_id].hp = 40;
    let water = carry(&mut w, "potion of water", Buc::Uncursed);
    let bridge = install_bridge(&w);

    let reply = bridge.step_service(ServiceAction::Pray).unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("devoutly reconciled")
    )));
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("consecrates your water into Holy Water")
    )));

    let parts = bridge.partitions().unwrap();
    assert_eq!(parts.actors.actors[w.player_id].hp, 50);
    assert_eq!(parts.hero.divine_state.favor, 6);
    // The 300-turn cooldown is set by the prayer and ticks once post-loop.
    assert_eq!(parts.hero.divine_state.prayer_timeout, 299);
    // Favor 6 > 5 consecrates the uncursed water into blessed holy water.
    assert_eq!(parts.objects.items.get(water).unwrap().buc, Buc::Blessed);
}

#[test]
fn services_pray_too_soon_smites_hero() {
    let mut w = clean_world(804);
    w.divine_state.prayer_timeout = 5;
    let bridge = install_bridge(&w);

    let reply = bridge.step_service(ServiceAction::Pray).unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("You pray too soon!")
    )));

    let parts = bridge.partitions().unwrap();
    // Favor 5 - 3 = 2; the smite deals 8 (50 -> 42) and the timeout ticks to 4.
    assert_eq!(parts.hero.divine_state.favor, 2);
    assert_eq!(parts.hero.divine_state.prayer_timeout, 4);
    let hp = parts.actors.actors[w.player_id].hp;
    assert!((42..50).contains(&hp), "unexpected smite hp {hp}");
}

#[test]
fn services_sacrifice_rock_increases_favor() {
    let mut w = clean_world(804);
    set_altar(
        &mut w,
        Tile::Altar {
            align: Alignment::Neutral,
        },
    );
    let rock = carry(&mut w, "rock", Buc::Uncursed);
    let bridge = install_bridge(&w);

    let reply = bridge.step_service(ServiceAction::Sacrifice(0)).unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("You sacrifice the rock")
    )));

    let parts = bridge.partitions().unwrap();
    assert!(parts.objects.items.get(rock).is_none());
    // Co-aligned rock (nutrition 100 < 200): favor 5 + 1 = 6, +1 max HP.
    assert_eq!(parts.hero.divine_state.favor, 6);
    assert_eq!(parts.actors.actors[w.player_id].max_hp, 51);
    assert_eq!(parts.actors.actors[w.player_id].hp, 51);
}

#[test]
fn services_sacrifice_missing_item_refused() {
    let mut w = clean_world(804);
    set_altar(
        &mut w,
        Tile::Altar {
            align: Alignment::Neutral,
        },
    );
    let bridge = install_bridge(&w);

    let reply = bridge.step_service(ServiceAction::Sacrifice(0)).unwrap();
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("don't have that item in your pack to sacrifice")
    )));
}

#[test]
fn services_sacrifice_amulet_on_high_altar_wins() {
    let mut w = clean_world(804);
    set_altar(
        &mut w,
        Tile::HighAltar {
            align: Alignment::Neutral,
        },
    );
    let amulet = carry(&mut w, "Amulet of Yendor", Buc::Uncursed);
    let bridge = install_bridge(&w);

    let reply = bridge.step_service(ServiceAction::Sacrifice(0)).unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(e, GameEvent::Victory)));

    let parts = bridge.partitions().unwrap();
    assert!(parts.objects.items.get(amulet).is_none());
}

#[test]
fn services_donate_to_priest_deducts_gold() {
    let mut w = clean_world(804);
    spawn_named(&mut w, "priest of Moloch", Coord::new_unchecked(12, 10));
    w.player_gold = 1000;
    let bridge = install_bridge(&w);

    let reply = bridge.step_service(ServiceAction::Donate(100)).unwrap();
    assert!(reply.spent_time);

    let parts = bridge.partitions().unwrap();
    // The offer is capped at the hero's gold; 100 of 1000 is donated.
    assert_eq!(parts.billing.player_gold, 900);
}

#[test]
fn services_donate_without_priest_refused() {
    let mut w = clean_world(804);
    w.player_gold = 1000;
    let bridge = install_bridge(&w);

    let reply = bridge.step_service(ServiceAction::Donate(100)).unwrap();
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("no priest nearby")
    )));

    let parts = bridge.partitions().unwrap();
    assert_eq!(parts.billing.player_gold, 1000);
}

#[test]
fn services_pay_settles_unpaid_items() {
    let mut w = clean_world(804);
    let dagger = carry(&mut w, "dagger", Buc::Uncursed);
    w.unpaid_items.push((dagger, 100));
    w.player_gold = 500;
    let bridge = install_bridge(&w);

    let reply = bridge.step_service(ServiceAction::Pay).unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("You pay the shopkeeper")
    )));

    let parts = bridge.partitions().unwrap();
    assert!(parts.billing.unpaid_items.is_empty());
    // The due is the shopkeeper's buy price for the 100-zm base cost.
    let due = nethacked_core::buy_price(
        100,
        crate::actions::economy::DEFAULT_CHARISMA,
        nethacked_core::dunce_or_tourist_surcharge(false, false, 10, false),
        false,
        false,
        false,
    );
    assert_eq!(parts.billing.player_gold, 500 - due);
}

#[test]
fn services_pay_insufficient_gold_refused() {
    let mut w = clean_world(804);
    let dagger = carry(&mut w, "dagger", Buc::Uncursed);
    w.unpaid_items.push((dagger, 100));
    w.player_gold = 0;
    let bridge = install_bridge(&w);

    let reply = bridge.step_service(ServiceAction::Pay).unwrap();
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("don't have enough gold")
    )));

    let parts = bridge.partitions().unwrap();
    assert_eq!(parts.billing.player_gold, 0);
    assert_eq!(parts.billing.unpaid_items.len(), 1);
}

#[test]
fn services_pay_deferred_settlement_spends_nothing() {
    let mut w = clean_world(804);
    // Admission only accepts live bill terms on a carried non-container wand.
    let wand = carry(&mut w, "wand of fire", Buc::Uncursed);
    w.unpaid_items.push((wand, 100));
    w.player_gold = 500;
    // A live bill term defers settlement: Pay settles nothing and spends no time.
    w.billing.live_terms.push((
        wand,
        BillTerms {
            billed_quantity: Some(1),
            unit_price: Some(100),
            creditor: None,
            legacy_base_cost: 100,
        },
    ));
    let bridge = install_bridge(&w);

    let reply = bridge.step_service(ServiceAction::Pay).unwrap();
    assert!(!reply.spent_time);
    assert_eq!(reply.rounds, 0);
    assert!(reply.events.is_empty());

    let parts = bridge.partitions().unwrap();
    assert_eq!(parts.billing.player_gold, 500);
    assert_eq!(parts.billing.unpaid_items.len(), 1);
}

#[test]
fn services_price_check_appraises_with_shopkeeper() {
    let mut w = clean_world(804);
    carry(&mut w, "dagger", Buc::Uncursed);
    spawn_named(&mut w, "shopkeeper", Coord::new_unchecked(12, 10));
    let bridge = install_bridge(&w);

    let reply = bridge.step_service(ServiceAction::PriceCheck(0)).unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("dagger")
    )));
}

#[test]
fn services_price_check_without_shopkeeper_refused() {
    let mut w = clean_world(804);
    carry(&mut w, "dagger", Buc::Uncursed);
    let bridge = install_bridge(&w);

    let reply = bridge.step_service(ServiceAction::PriceCheck(0)).unwrap();
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("no shopkeeper here to appraise")
    )));
}

#[test]
fn services_price_check_missing_item_refused() {
    let mut w = clean_world(804);
    carry(&mut w, "dagger", Buc::Uncursed);
    spawn_named(&mut w, "shopkeeper", Coord::new_unchecked(12, 10));
    let bridge = install_bridge(&w);

    let reply = bridge.step_service(ServiceAction::PriceCheck(5)).unwrap();
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("don't have that item to appraise")
    )));
}

#[test]
fn services_wish_grants_dagger() {
    let mut w = clean_world(804);
    let wand = carry(&mut w, "wand of wishing", Buc::Uncursed);
    w.arena.items.get_mut(wand).unwrap().enchantment = 3;
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_service(ServiceAction::Wish("dagger".into()))
        .unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("miraculously drops from the heavens")
    )));

    let parts = bridge.partitions().unwrap();
    // One charge is spent (3 -> 2) and the dagger lands at the hero's feet.
    assert_eq!(parts.objects.items.get(wand).unwrap().enchantment, 2);
    assert!(parts.objects.items.values().any(|it| {
        it.name == "dagger" && it.location == ItemLocation::Floor(Coord::new_unchecked(10, 10))
    }));
}

#[test]
fn services_wish_without_wand_refused() {
    let w = clean_world(804);
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_service(ServiceAction::Wish("dagger".into()))
        .unwrap();
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("no means of wishing")
    )));
}

#[test]
fn services_wish_unmatched_spends_time() {
    let mut w = clean_world(804);
    let wand = carry(&mut w, "wand of wishing", Buc::Uncursed);
    w.arena.items.get_mut(wand).unwrap().enchantment = 3;
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_service(ServiceAction::Wish("zzzqqq xyzzy".into()))
        .unwrap();
    // An unmatched wish still spends the turn and consumes a charge.
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("vague sense of loss")
    )));

    let parts = bridge.partitions().unwrap();
    assert_eq!(parts.objects.items.get(wand).unwrap().enchantment, 2);
}

#[test]
fn services_wish_blank_mind_spends_time() {
    let mut w = clean_world(804);
    let wand = carry(&mut w, "wand of wishing", Buc::Uncursed);
    w.arena.items.get_mut(wand).unwrap().enchantment = 3;
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_service(ServiceAction::Wish("   ".into()))
        .unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("Your mind draws a blank")
    )));

    let parts = bridge.partitions().unwrap();
    assert_eq!(parts.objects.items.get(wand).unwrap().enchantment, 2);
}

#[test]
fn services_mount_tame_steed_then_dismount() {
    let mut w = clean_world(804);
    let goblin = spawn_goblin(&mut w, Coord::new_unchecked(11, 10));
    w.arena.actors[goblin].is_tame = true;
    let bridge = install_bridge(&w);

    let reply = bridge.step_service(ServiceAction::Mount(goblin)).unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("You mount the steed")
    )));

    let parts = bridge.partitions().unwrap();
    let mount = parts.hero.hero.mount.clone().unwrap();
    assert_eq!(mount.steed_id, goblin);
    assert!(mount.saddle_equipped);
    drop(parts);

    let reply = bridge.step_service(ServiceAction::Dismount).unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("You dismount")
    )));

    let parts = bridge.partitions().unwrap();
    assert!(parts.hero.hero.mount.is_none());
}

#[test]
fn services_dismount_when_not_mounted_spends_time() {
    let w = clean_world(804);
    let bridge = install_bridge(&w);

    // Dismount spends the turn first, then reports the missing steed.
    let reply = bridge.step_service(ServiceAction::Dismount).unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("You are not mounted")
    )));
}

#[test]
fn services_reload_and_continuation() {
    let w = clean_world(804);
    let bridge = install_bridge(&w);

    let reply1 = bridge.step_service(ServiceAction::Pray).unwrap();
    assert!(reply1.spent_time);

    // Save
    let wire = bridge.capture().unwrap();

    // Reconstruct into fresh bridge
    let reloaded = OwnerBridge::reconstruct(&wire, config(&w)).unwrap();

    // Continue with a dismount on the reloaded bridge
    let reply2 = reloaded.step_service(ServiceAction::Dismount).unwrap();
    assert!(reply2.spent_time);

    let parts = reloaded.partitions().unwrap();
    // Each spent action advances the world clock by exactly one round.
    assert_eq!(parts.clock.scheduler.turn, 102);
    // Prayer set the 300-turn cooldown; each spent action ticks it once.
    assert_eq!(parts.hero.divine_state.prayer_timeout, 298);
}

// ---------------------------------------------------------------------------
// Legacy-oracle parity: the bridge must reproduce the native
// `step_player_action` bit-for-bit — event stream, full world state and RNG
// stream. The one documented exception is the session event log: legacy
// `record_events` appends to it, while the services transport keeps Session
// read-only, so the field is normalised before comparing save snapshots.
// ---------------------------------------------------------------------------

fn oracle_json(w: &SimulationWorld) -> serde_json::Value {
    let mut v: serde_json::Value = serde_json::from_str(&w.to_save_json().unwrap()).unwrap();
    if let Some(obj) = v.as_object_mut() {
        obj.insert("event_log".into(), serde_json::Value::Array(vec![]));
    }
    v
}

fn run_service_oracle(
    w: &SimulationWorld,
    ast: ActionAst,
    action: ServiceAction,
) -> (services::ServicesReply, SimulationWorld) {
    let mut legacy = w.clone();
    let bridge = install_bridge(w);
    let events = legacy.step_player_action(ast);
    let reply = bridge.step_service(action).unwrap();
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
fn services_oracle_cast_force_bolt_kill_matches_legacy() {
    let mut w = clean_world(804);
    w.known_spells.push((SpellKind::ForceBolt, 0));
    w.player_pw = 50;
    let goblin = spawn_goblin(&mut w, Coord::new_unchecked(11, 10));
    w.arena.actors[goblin].hp = 10;
    w.arena.actors[goblin].max_hp = 10;
    let (reply, legacy) = run_service_oracle(
        &w,
        ActionAst::Cast {
            spell_index: 0,
            dir: Direction::East,
        },
        ServiceAction::Cast {
            spell_index: 0,
            dir: Direction::East,
        },
    );
    assert!(reply.spent_time);
    assert!(legacy.arena.actors[goblin].is_dead);
    assert_eq!(
        legacy.player_pw,
        50 - nethacked_core::mana_cost(SpellKind::ForceBolt)
    );
}

#[test]
fn services_oracle_cast_cure_light_matches_legacy() {
    let mut w = clean_world(804);
    w.known_spells.push((SpellKind::CureLightWounds, 0));
    w.player_pw = 50;
    w.arena.actors[w.player_id].hp = 30;
    let (reply, legacy) = run_service_oracle(
        &w,
        ActionAst::Cast {
            spell_index: 0,
            dir: Direction::East,
        },
        ServiceAction::Cast {
            spell_index: 0,
            dir: Direction::East,
        },
    );
    assert!(reply.spent_time);
    assert!(legacy.arena.actors[legacy.player_id].hp >= 42);
}

#[test]
fn services_oracle_pray_coaligned_matches_legacy() {
    let mut w = clean_world(804);
    set_altar(
        &mut w,
        Tile::Altar {
            align: Alignment::Neutral,
        },
    );
    w.arena.actors[w.player_id].hp = 40;
    carry(&mut w, "potion of water", Buc::Uncursed);
    let (reply, legacy) = run_service_oracle(&w, ActionAst::Pray, ServiceAction::Pray);
    assert!(reply.spent_time);
    assert_eq!(legacy.arena.actors[legacy.player_id].hp, 50);
    assert_eq!(legacy.divine_state.favor, 6);
    assert_eq!(legacy.divine_state.prayer_timeout, 299);
}

#[test]
fn services_oracle_pray_timeout_matches_legacy() {
    let mut w = clean_world(804);
    w.divine_state.prayer_timeout = 5;
    let (reply, legacy) = run_service_oracle(&w, ActionAst::Pray, ServiceAction::Pray);
    assert!(reply.spent_time);
    assert_eq!(legacy.divine_state.favor, 2);
    assert_eq!(legacy.divine_state.prayer_timeout, 4);
}

#[test]
fn services_oracle_sacrifice_rock_matches_legacy() {
    let mut w = clean_world(804);
    set_altar(
        &mut w,
        Tile::Altar {
            align: Alignment::Neutral,
        },
    );
    carry(&mut w, "rock", Buc::Uncursed);
    let (reply, legacy) =
        run_service_oracle(&w, ActionAst::Sacrifice(0), ServiceAction::Sacrifice(0));
    assert!(reply.spent_time);
    assert_eq!(legacy.divine_state.favor, 6);
    assert_eq!(legacy.arena.actors[legacy.player_id].max_hp, 51);
}

#[test]
fn services_oracle_sacrifice_amulet_victory_matches_legacy() {
    let mut w = clean_world(804);
    set_altar(
        &mut w,
        Tile::HighAltar {
            align: Alignment::Neutral,
        },
    );
    carry(&mut w, "Amulet of Yendor", Buc::Uncursed);
    let (reply, _) = run_service_oracle(&w, ActionAst::Sacrifice(0), ServiceAction::Sacrifice(0));
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(e, GameEvent::Victory)));
}

#[test]
fn services_oracle_donate_matches_legacy() {
    let mut w = clean_world(804);
    spawn_named(&mut w, "priest of Moloch", Coord::new_unchecked(12, 10));
    w.player_gold = 1000;
    let (reply, legacy) =
        run_service_oracle(&w, ActionAst::Donate(100), ServiceAction::Donate(100));
    assert!(reply.spent_time);
    assert_eq!(legacy.player_gold, 900);
}

#[test]
fn services_oracle_pay_matches_legacy() {
    let mut w = clean_world(804);
    let dagger = carry(&mut w, "dagger", Buc::Uncursed);
    w.unpaid_items.push((dagger, 100));
    w.player_gold = 500;
    let (reply, legacy) = run_service_oracle(&w, ActionAst::Pay, ServiceAction::Pay);
    assert!(reply.spent_time);
    assert!(legacy.unpaid_items.is_empty());
    assert!(legacy.player_gold < 500);
}

#[test]
fn services_oracle_pay_deferred_matches_legacy() {
    let mut w = clean_world(804);
    // Admission only accepts live bill terms on a carried non-container wand.
    let wand = carry(&mut w, "wand of fire", Buc::Uncursed);
    w.unpaid_items.push((wand, 100));
    w.player_gold = 500;
    w.billing.live_terms.push((
        wand,
        BillTerms {
            billed_quantity: Some(1),
            unit_price: Some(100),
            creditor: None,
            legacy_base_cost: 100,
        },
    ));
    let (reply, legacy) = run_service_oracle(&w, ActionAst::Pay, ServiceAction::Pay);
    assert!(!reply.spent_time);
    assert_eq!(reply.rounds, 0);
    assert_eq!(legacy.player_gold, 500);
    assert_eq!(legacy.unpaid_items.len(), 1);
}

#[test]
fn services_oracle_price_check_matches_legacy() {
    let mut w = clean_world(804);
    carry(&mut w, "dagger", Buc::Uncursed);
    spawn_named(&mut w, "shopkeeper", Coord::new_unchecked(12, 10));
    let (reply, _) = run_service_oracle(&w, ActionAst::PriceCheck(0), ServiceAction::PriceCheck(0));
    assert!(reply.spent_time);
}

#[test]
fn services_oracle_wish_dagger_matches_legacy() {
    let mut w = clean_world(804);
    let wand = carry(&mut w, "wand of wishing", Buc::Uncursed);
    w.arena.items.get_mut(wand).unwrap().enchantment = 3;
    let (reply, legacy) = run_service_oracle(
        &w,
        ActionAst::Wish("dagger".into()),
        ServiceAction::Wish("dagger".into()),
    );
    assert!(reply.spent_time);
    assert_eq!(legacy.arena.items.get(wand).unwrap().enchantment, 2);
    assert!(legacy.arena.items.values().any(|it| it.name == "dagger"
        && it.location == ItemLocation::Floor(Coord::new_unchecked(10, 10))));
}

#[test]
fn services_oracle_wish_corpse_matches_legacy() {
    let mut w = clean_world(804);
    let wand = carry(&mut w, "wand of wishing", Buc::Uncursed);
    w.arena.items.get_mut(wand).unwrap().enchantment = 3;
    let (reply, legacy) = run_service_oracle(
        &w,
        ActionAst::Wish("corpse".into()),
        ServiceAction::Wish("corpse".into()),
    );
    assert!(reply.spent_time);
    // A generic corpse wish rolls a random corpse through the monster context.
    assert!(legacy.arena.items.values().any(|it| it.is_corpse()));
}

#[test]
fn services_oracle_wish_amulet_imitation_matches_legacy() {
    let mut w = clean_world(804);
    let wand = carry(&mut w, "wand of wishing", Buc::Uncursed);
    w.arena.items.get_mut(wand).unwrap().enchantment = 3;
    let (reply, legacy) = run_service_oracle(
        &w,
        ActionAst::Wish("amulet of Yendor".into()),
        ServiceAction::Wish("amulet of Yendor".into()),
    );
    assert!(reply.spent_time);
    // The genuine Amulet cannot be wished for; a plastic imitation drops instead.
    assert!(legacy
        .arena
        .items
        .values()
        .any(|it| it.name == "cheap plastic imitation of the Amulet of Yendor"));
}

#[test]
fn services_oracle_mount_matches_legacy() {
    let mut w = clean_world(804);
    let goblin = spawn_goblin(&mut w, Coord::new_unchecked(11, 10));
    w.arena.actors[goblin].is_tame = true;
    let (reply, legacy) =
        run_service_oracle(&w, ActionAst::Mount(goblin), ServiceAction::Mount(goblin));
    assert!(reply.spent_time);
    assert_eq!(legacy.hero.mount.as_ref().map(|m| m.steed_id), Some(goblin));
}

#[test]
fn services_oracle_dismount_matches_legacy() {
    let mut w = clean_world(804);
    let goblin = spawn_goblin(&mut w, Coord::new_unchecked(11, 10));
    w.hero.mount = Some(MountState {
        steed_id: goblin,
        saddle_equipped: true,
    });
    let (reply, legacy) = run_service_oracle(&w, ActionAst::Dismount, ServiceAction::Dismount);
    assert!(reply.spent_time);
    assert!(legacy.hero.mount.is_none());
}

#[test]
fn services_oracle_helpless_substitutes_wait_matches_legacy() {
    let mut w = clean_world(804);
    w.hero.afflictions.transient.helpless = 2;

    // C multi < 0: the prayer degenerates to a stationary wait; no prayer
    // event is emitted, but the turn is still spent.
    let (reply, legacy) = run_service_oracle(&w, ActionAst::Pray, ServiceAction::Pray);
    assert!(reply.spent_time);
    assert!(!reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("pray") || text.contains("Pray")
    )));
    // The wait round ticks the helpless affliction down by one.
    assert_eq!(legacy.hero.afflictions.transient.helpless, 1);
}

#[test]
fn services_oracle_prayer_timeout_ticks_only_when_time_spent() {
    // No time spent: the legacy `if spent_time` guard leaves the timeout alone.
    let mut w = clean_world(804);
    w.divine_state.prayer_timeout = 5;
    let mut legacy = w.clone();
    let bridge = install_bridge(&w);
    let events = legacy.step_player_action(ActionAst::Wish("dagger".into()));
    let reply = bridge
        .step_service(ServiceAction::Wish("dagger".into()))
        .unwrap();
    assert!(!reply.spent_time);
    assert_eq!(reply.events, events);
    let after = bridge.export_native().unwrap();
    assert_eq!(oracle_json(&after), oracle_json(&legacy));
    assert_eq!(after.divine_state.prayer_timeout, 5);

    // Time spent: the timeout ticks down exactly once.
    let mut w = clean_world(804);
    w.divine_state.prayer_timeout = 5;
    let mut legacy = w.clone();
    let bridge = install_bridge(&w);
    let events = legacy.step_player_action(ActionAst::Pray);
    let reply = bridge.step_service(ServiceAction::Pray).unwrap();
    assert!(reply.spent_time);
    assert_eq!(reply.events, events);
    let after = bridge.export_native().unwrap();
    assert_eq!(oracle_json(&after), oracle_json(&legacy));
    assert_eq!(after.divine_state.prayer_timeout, 4);
}
