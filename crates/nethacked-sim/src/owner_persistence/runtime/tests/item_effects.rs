use super::super::*;
use crate::owner_persistence::runtime::item_effects::{self, ItemEffectAction};
use crate::{ActionAst, GameEvent};
use nethacked_arena::{ActorId, ItemLocation};
use nethacked_core::buc::WaterType;
use nethacked_core::engraving::EngravingMedium;
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
        .with_item_effects()
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

#[test]
fn item_effects_manifest_bounds_and_profiles() {
    for item_effects in [false, true] {
        let w = SimulationWorld::new_with_seed(721);
        let mut c = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_timer_ticks();
        if item_effects {
            c = c.with_item_effects();
        }
        assert_eq!(profile_caller(&c).dependencies.len(), 64);
        assert_eq!(profile_provider(&c).exports.len(), 64);
        OwnerBridge::install(NativePartitions::from_native(w), c).unwrap();
    }

    // Microkernel transport manifests fit well within 64-port bounds
    assert!(item_effects::READS.len() + item_effects::WRITES.len() + 2 <= 64);
    assert_eq!(
        item_effects::READS.len() + item_effects::WRITES.len() + 2,
        19
    );
}

#[test]
fn item_effects_authentic_stale_refused() {
    let mut w = clean_world(804);
    carry(&mut w, "potion of healing", Buc::Uncursed);
    let bridge = install_bridge(&w);
    let mut req = bridge
        .prepare_item_effect(ItemEffectAction::Quaff(0))
        .unwrap();

    // Tampering with witness stamp triggers authentic stale refusal check
    req.guards[0].generation = req.guards[0].generation.wrapping_add(1);
    let err = bridge.execute_item_effect(req);
    assert!(matches!(
        err,
        Err(OwnerBridgeError::Owner(owners::Error::Stale))
    ));
    assert!(item_effects::stale_refusals() >= 1);
}

#[test]
fn item_effects_wrong_authority_refused() {
    let mut w = clean_world(804);
    carry(&mut w, "potion of healing", Buc::Uncursed);
    let bridge = install_bridge(&w);
    let req = bridge
        .prepare_item_effect(ItemEffectAction::Quaff(0))
        .unwrap();

    item_effects::trigger_wrong_resource(true);
    let res = bridge.execute_item_effect(req.clone());
    item_effects::trigger_wrong_resource(false);

    assert!(res.is_err());

    // State remains uncorrupted and request can still be executed successfully with valid authority
    let reply = bridge.execute_item_effect(req).unwrap();
    assert!(reply.spent_time);
}

#[test]
fn item_effects_zero_rounds_when_hero_dead() {
    let mut w = clean_world(804);
    w.arena.actors[w.player_id].is_dead = true;
    w.arena.actors[w.player_id].hp = 0;
    let bridge = install_bridge(&w);

    let reply = bridge.step_item_effect(ItemEffectAction::Quaff(0)).unwrap();
    assert!(!reply.spent_time);
    assert_eq!(reply.rounds, 0);
    assert!(reply.events.is_empty());
}

#[test]
fn item_effects_dip_dilutes_potion() {
    let mut w = clean_world(804);
    let potion = carry(&mut w, "potion of extra healing", Buc::Cursed);
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_item_effect(ItemEffectAction::Dip {
            item_index: 0,
            into_water: WaterType::Plain,
        })
        .unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("dip the potion of healing into the water")
    )));

    let parts = bridge.partitions().unwrap();
    let item = parts.objects.items.get(potion).unwrap();
    // C dipin: plain water dilutes extra healing and lifts the curse.
    assert_eq!(item.name, "potion of healing");
    assert_eq!(item.buc, Buc::Uncursed);
}

#[test]
fn item_effects_dip_missing_item_refused() {
    let w = clean_world(804);
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_item_effect(ItemEffectAction::Dip {
            item_index: 3,
            into_water: WaterType::Plain,
        })
        .unwrap();
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("don't have that item to dip")
    )));
}

#[test]
fn item_effects_quaff_healing_restores_hp() {
    let mut w = clean_world(804);
    w.arena.actors[w.player_id].hp = 30;
    let potion = carry(&mut w, "potion of healing", Buc::Uncursed);
    let bridge = install_bridge(&w);

    let reply = bridge.step_item_effect(ItemEffectAction::Quaff(0)).unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("feel much better")
    )));

    let parts = bridge.partitions().unwrap();
    assert!(parts.objects.items.get(potion).is_none());
    // Quaff heals 10 (30 -> 40); the turn loop's natural regeneration adds 1.
    assert_eq!(parts.actors.actors[w.player_id].hp, 41);
}

#[test]
fn item_effects_quaff_non_potion_refused() {
    let mut w = clean_world(804);
    let rock = carry(&mut w, "rock", Buc::Uncursed);
    let bridge = install_bridge(&w);

    let reply = bridge.step_item_effect(ItemEffectAction::Quaff(0)).unwrap();
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("only quaff potions")
    )));

    let parts = bridge.partitions().unwrap();
    assert!(parts.objects.items.get(rock).is_some());
}

#[test]
fn item_effects_read_scroll_destroys_it() {
    let mut w = clean_world(804);
    let scroll = carry(&mut w, "scroll of identify", Buc::Uncursed);
    let bridge = install_bridge(&w);

    let reply = bridge.step_item_effect(ItemEffectAction::Read(0)).unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("Knowledge fills your mind")
    )));

    let parts = bridge.partitions().unwrap();
    assert!(parts.objects.items.get(scroll).is_none());
}

#[test]
fn item_effects_read_non_scroll_refused() {
    let mut w = clean_world(804);
    let rock = carry(&mut w, "rock", Buc::Uncursed);
    let bridge = install_bridge(&w);

    let reply = bridge.step_item_effect(ItemEffectAction::Read(0)).unwrap();
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("only read scrolls or spellbooks")
    )));

    let parts = bridge.partitions().unwrap();
    assert!(parts.objects.items.get(rock).is_some());
}

#[test]
fn item_effects_eat_food_ration_gains_nutrition() {
    let mut w = clean_world(804);
    w.player_nutrition = 100;
    let ration = carry(&mut w, "food ration", Buc::Uncursed);
    let bridge = install_bridge(&w);

    let reply = bridge.step_item_effect(ItemEffectAction::Eat(0)).unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("eat the food ration")
    )));

    let parts = bridge.partitions().unwrap();
    assert!(parts.objects.items.get(ration).is_none());
    // oc_nutrition 800 on top of 100, less the turn loop's metabolic tick.
    assert!(parts.hero.player_nutrition > 800);
}

#[test]
fn item_effects_eat_inedible_refused() {
    let mut w = clean_world(804);
    let rock = carry(&mut w, "rock", Buc::Uncursed);
    let bridge = install_bridge(&w);

    let reply = bridge.step_item_effect(ItemEffectAction::Eat(0)).unwrap();
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("not edible")
    )));

    let parts = bridge.partitions().unwrap();
    assert!(parts.objects.items.get(rock).is_some());
}

#[test]
fn item_effects_rub_magic_lamp_summons_djinni() {
    let mut w = clean_world(804);
    let lamp = carry(&mut w, "magic lamp", Buc::Uncursed);
    let bridge = install_bridge(&w);

    let reply = bridge.step_item_effect(ItemEffectAction::Rub(0)).unwrap();
    assert!(reply.spent_time);
    assert!(!reply.events.is_empty());

    let parts = bridge.partitions().unwrap();
    // The lamp survives the rub (rub_lamp keeps it at this charge level).
    assert!(parts.objects.items.get(lamp).is_some());
}

#[test]
fn item_effects_rub_missing_item_refused() {
    let w = clean_world(804);
    let bridge = install_bridge(&w);

    let reply = bridge.step_item_effect(ItemEffectAction::Rub(1)).unwrap();
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("don't have that item to rub")
    )));
}

#[test]
fn item_effects_apply_lights_lamp() {
    let mut w = clean_world(804);
    let lamp = carry(&mut w, "oil lamp", Buc::Uncursed);
    let bridge = install_bridge(&w);

    let reply = bridge.step_item_effect(ItemEffectAction::Apply(0)).unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("You light the oil lamp")
    )));

    let parts = bridge.partitions().unwrap();
    assert_eq!(parts.objects.items.get(lamp).unwrap().enchantment, 1);
}

#[test]
fn item_effects_apply_unknown_item_refused() {
    let mut w = clean_world(804);
    let horn = carry(&mut w, "unicorn horn", Buc::Uncursed);
    let bridge = install_bridge(&w);

    let reply = bridge.step_item_effect(ItemEffectAction::Apply(0)).unwrap();
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("don't know how to apply the unicorn horn")
    )));

    let parts = bridge.partitions().unwrap();
    assert!(parts.objects.items.get(horn).is_some());
}

#[test]
fn item_effects_engrave_writes_on_floor() {
    let w = clean_world(804);
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_item_effect(ItemEffectAction::Engrave {
            text: "Elbereth".into(),
            medium: EngravingMedium::Burned,
        })
        .unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::EngravingAdded { coord, text }
            if *coord == Coord::new_unchecked(10, 10) && text == "Elbereth"
    )));

    let parts = bridge.partitions().unwrap();
    let engraving = parts
        .geography
        .level
        .get_engraving(Coord::new_unchecked(10, 10))
        .unwrap();
    assert_eq!(engraving.text, "Elbereth");
}

#[test]
fn item_effects_zap_wand_of_fire_hits_monster() {
    let mut w = clean_world(804);
    carry(&mut w, "wand of fire", Buc::Uncursed);
    let goblin = spawn_goblin(&mut w, Coord::new_unchecked(11, 10));
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_item_effect(ItemEffectAction::ZapWand {
            dir: Direction::East,
            energy: 6,
        })
        .unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::AttackLanded { target, .. } if *target == goblin
    )));

    let parts = bridge.partitions().unwrap();
    assert!(parts.actors.actors[goblin].hp < 20);
}

#[test]
fn item_effects_zap_selected_wand_uses_index() {
    let mut w = clean_world(804);
    carry(&mut w, "rock", Buc::Uncursed);
    let wand = carry(&mut w, "wand of fire", Buc::Uncursed);
    let goblin = spawn_goblin(&mut w, Coord::new_unchecked(11, 10));
    let bridge = install_bridge(&w);

    let reply = bridge
        .step_item_effect(ItemEffectAction::ZapSelectedWand {
            item_index: 1,
            expected_item: Some(wand),
            dir: Direction::East,
            energy: 6,
        })
        .unwrap();
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::AttackLanded { target, .. } if *target == goblin
    )));
}

#[test]
fn item_effects_zap_selected_wrong_expectation_refused() {
    let mut w = clean_world(804);
    let wand = carry(&mut w, "wand of fire", Buc::Uncursed);
    let other = carry(&mut w, "wand of cold", Buc::Uncursed);
    let bridge = install_bridge(&w);

    // The expected handle does not match the carried slot: no zap, no time.
    let reply = bridge
        .step_item_effect(ItemEffectAction::ZapSelectedWand {
            item_index: 0,
            expected_item: Some(other),
            dir: Direction::East,
            energy: 6,
        })
        .unwrap();
    assert!(!reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("don't have that object")
    )));

    let parts = bridge.partitions().unwrap();
    assert!(parts.objects.items.get(wand).is_some());
}

#[test]
fn item_effects_reload_and_continuation() {
    let mut w = clean_world(804);
    w.arena.actors[w.player_id].hp = 30;
    let potion = carry(&mut w, "potion of healing", Buc::Uncursed);
    let bridge = install_bridge(&w);

    let reply1 = bridge.step_item_effect(ItemEffectAction::Quaff(0)).unwrap();
    assert!(reply1.spent_time);

    // Save
    let wire = bridge.capture().unwrap();

    // Reconstruct into fresh bridge
    let reloaded = OwnerBridge::reconstruct(&wire, config(&w)).unwrap();

    // Continue with an engrave on the reloaded bridge
    let reply2 = reloaded
        .step_item_effect(ItemEffectAction::Engrave {
            text: "X".into(),
            medium: EngravingMedium::Burned,
        })
        .unwrap();
    assert!(reply2.spent_time);

    let parts = reloaded.partitions().unwrap();
    assert_eq!(parts.clock.scheduler.turn, 102);
    assert!(parts.objects.items.get(potion).is_none());
    // Quaff healed 10 (30 -> 40) plus one natural regeneration tick.
    assert_eq!(parts.actors.actors[w.player_id].hp, 41);
    assert!(parts
        .geography
        .level
        .get_engraving(Coord::new_unchecked(10, 10))
        .is_some());
}

// ---------------------------------------------------------------------------
// Legacy-oracle parity: the bridge must reproduce the native
// `step_player_action` bit-for-bit — event stream, full world state and RNG
// stream. The one documented exception is the session event log: legacy
// `record_events` appends to it, while the item-effects transport keeps
// Session read-only, so the field is normalised before comparing save
// snapshots.
// ---------------------------------------------------------------------------

fn oracle_json(w: &SimulationWorld) -> serde_json::Value {
    let mut v: serde_json::Value = serde_json::from_str(&w.to_save_json().unwrap()).unwrap();
    if let Some(obj) = v.as_object_mut() {
        obj.insert("event_log".into(), serde_json::Value::Array(vec![]));
    }
    v
}

fn run_item_effect_oracle(
    w: &SimulationWorld,
    ast: ActionAst,
    action: ItemEffectAction,
) -> (item_effects::ItemEffectsReply, SimulationWorld) {
    let mut legacy = w.clone();
    let bridge = install_bridge(w);
    let events = legacy.step_player_action(ast);
    let reply = bridge.step_item_effect(action).unwrap();
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
fn item_effects_oracle_dip_matches_legacy() {
    let mut w = clean_world(804);
    carry(&mut w, "potion of extra healing", Buc::Cursed);
    let (reply, _) = run_item_effect_oracle(
        &w,
        ActionAst::Dip {
            item_index: 0,
            into_water: WaterType::Plain,
        },
        ItemEffectAction::Dip {
            item_index: 0,
            into_water: WaterType::Plain,
        },
    );
    assert!(reply.spent_time);
}

#[test]
fn item_effects_oracle_quaff_matches_legacy() {
    let mut w = clean_world(804);
    w.arena.actors[w.player_id].hp = 30;
    carry(&mut w, "potion of healing", Buc::Uncursed);
    // A neighbour with movement budget exercises the turn loop's monster ticks.
    spawn_goblin(&mut w, Coord::new_unchecked(12, 10));
    let (reply, legacy) =
        run_item_effect_oracle(&w, ActionAst::Quaff(0), ItemEffectAction::Quaff(0));
    assert!(reply.spent_time);
    assert_eq!(legacy.arena.actors[legacy.player_id].hp, 40);
}

#[test]
fn item_effects_oracle_read_matches_legacy() {
    let mut w = clean_world(804);
    carry(&mut w, "scroll of remove curse", Buc::Uncursed);
    carry(&mut w, "dagger", Buc::Cursed);
    let (reply, legacy) = run_item_effect_oracle(&w, ActionAst::Read(0), ItemEffectAction::Read(0));
    assert!(reply.spent_time);
    // The uncurse scroll lifts the dagger's curse.
    let dagger = legacy
        .arena
        .items
        .values()
        .find(|it| it.name == "dagger")
        .unwrap();
    assert_eq!(dagger.buc, Buc::Uncursed);
}

#[test]
fn item_effects_oracle_eat_matches_legacy() {
    let mut w = clean_world(804);
    w.player_nutrition = 100;
    carry(&mut w, "food ration", Buc::Uncursed);
    let (reply, legacy) = run_item_effect_oracle(&w, ActionAst::Eat(0), ItemEffectAction::Eat(0));
    assert!(reply.spent_time);
    assert!(legacy.player_nutrition > 800);
}

#[test]
fn item_effects_oracle_rub_matches_legacy() {
    let mut w = clean_world(804);
    carry(&mut w, "magic lamp", Buc::Uncursed);
    let (reply, _) = run_item_effect_oracle(&w, ActionAst::Rub(0), ItemEffectAction::Rub(0));
    assert!(reply.spent_time);
}

#[test]
fn item_effects_oracle_apply_matches_legacy() {
    let mut w = clean_world(804);
    carry(&mut w, "oil lamp", Buc::Uncursed);
    let (reply, legacy) =
        run_item_effect_oracle(&w, ActionAst::Apply(0), ItemEffectAction::Apply(0));
    assert!(reply.spent_time);
    let lamp = legacy
        .arena
        .items
        .values()
        .find(|it| it.name == "oil lamp")
        .unwrap();
    assert_eq!(lamp.enchantment, 1);
}

#[test]
fn item_effects_oracle_engrave_matches_legacy() {
    let w = clean_world(804);
    let (reply, legacy) = run_item_effect_oracle(
        &w,
        ActionAst::Engrave {
            text: "Elbereth".into(),
            medium: EngravingMedium::Burned,
        },
        ItemEffectAction::Engrave {
            text: "Elbereth".into(),
            medium: EngravingMedium::Burned,
        },
    );
    assert!(reply.spent_time);
    assert!(legacy
        .level
        .get_engraving(Coord::new_unchecked(10, 10))
        .is_some());
}

#[test]
fn item_effects_oracle_zap_matches_legacy() {
    let mut w = clean_world(804);
    carry(&mut w, "wand of fire", Buc::Uncursed);
    let goblin = spawn_goblin(&mut w, Coord::new_unchecked(11, 10));
    let (reply, legacy) = run_item_effect_oracle(
        &w,
        ActionAst::ZapWand {
            dir: Direction::East,
            energy: 6,
        },
        ItemEffectAction::ZapWand {
            dir: Direction::East,
            energy: 6,
        },
    );
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::AttackLanded { target, .. } if *target == goblin
    )));
    assert!(legacy.arena.actors[goblin].hp < 20);
}

#[test]
fn item_effects_oracle_zap_selected_matches_legacy() {
    let mut w = clean_world(804);
    carry(&mut w, "rock", Buc::Uncursed);
    let wand = carry(&mut w, "wand of fire", Buc::Uncursed);
    let goblin = spawn_goblin(&mut w, Coord::new_unchecked(11, 10));
    let (reply, _) = run_item_effect_oracle(
        &w,
        ActionAst::ZapSelectedWand {
            item_index: 1,
            expected_item: Some(wand),
            dir: Direction::East,
            energy: 6,
        },
        ItemEffectAction::ZapSelectedWand {
            item_index: 1,
            expected_item: Some(wand),
            dir: Direction::East,
            energy: 6,
        },
    );
    assert!(reply.spent_time);
    assert!(reply.events.iter().any(|e| matches!(
        e,
        GameEvent::AttackLanded { target, .. } if *target == goblin
    )));
}

#[test]
fn item_effects_oracle_helpless_substitutes_wait_matches_legacy() {
    let mut w = clean_world(804);
    w.hero.afflictions.transient.helpless = 2;
    let potion = carry(&mut w, "potion of healing", Buc::Uncursed);

    // C multi < 0: the quaff degenerates to a stationary wait; the potion
    // stays in the pack and no quaff event is emitted.
    let (reply, legacy) =
        run_item_effect_oracle(&w, ActionAst::Quaff(0), ItemEffectAction::Quaff(0));
    assert!(reply.spent_time);
    assert!(!reply.events.iter().any(|e| matches!(
        e,
        GameEvent::LogMessage { text } if text.contains("quaff")
    )));
    assert!(legacy.arena.items.get(potion).is_some());
}

#[test]
fn item_effects_oracle_prayer_timeout_ticks_only_when_time_spent() {
    // No time spent: the legacy `if spent_time` guard leaves the timeout alone.
    let mut w = clean_world(804);
    w.divine_state.prayer_timeout = 5;
    let mut legacy = w.clone();
    let bridge = install_bridge(&w);
    let events = legacy.step_player_action(ActionAst::Quaff(0));
    let reply = bridge.step_item_effect(ItemEffectAction::Quaff(0)).unwrap();
    assert!(!reply.spent_time);
    assert_eq!(reply.events, events);
    let after = bridge.export_native().unwrap();
    assert_eq!(oracle_json(&after), oracle_json(&legacy));
    assert_eq!(after.divine_state.prayer_timeout, 5);

    // Time spent: the timeout ticks down exactly once.
    let mut w = clean_world(804);
    w.divine_state.prayer_timeout = 5;
    carry(&mut w, "potion of healing", Buc::Uncursed);
    let mut legacy = w.clone();
    let bridge = install_bridge(&w);
    let events = legacy.step_player_action(ActionAst::Quaff(0));
    let reply = bridge.step_item_effect(ItemEffectAction::Quaff(0)).unwrap();
    assert!(reply.spent_time);
    assert_eq!(reply.events, events);
    let after = bridge.export_native().unwrap();
    assert_eq!(oracle_json(&after), oracle_json(&legacy));
    assert_eq!(after.divine_state.prayer_timeout, 4);
}
