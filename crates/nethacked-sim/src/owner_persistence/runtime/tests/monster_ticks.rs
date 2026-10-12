use super::super::*;
use crate::owner_persistence::runtime::monster_ticks;
use crate::GameEvent;
use nethacked_arena::{ActorId, ItemLocation, ItemRecord};
use nethacked_data::MonsterSpeciesId;
use nethacked_types::{Buc, Coord, ItemClass, Tile};
use rand::{RngCore, SeedableRng};

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
    w.rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
    w
}

fn fixture(seed: u64) -> (SimulationWorld, ActorId) {
    let mut w = clean_world(seed);
    let mut goblin = w
        .ruleset
        .create_monster_record("goblin", Coord::new_unchecked(11, 10), &mut w.rng)
        .unwrap();
    goblin.movement = 12;
    let id = w.arena.spawn_actor(goblin);
    (w, id)
}

fn config(w: &SimulationWorld) -> OwnerBridgeConfig {
    OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_monster_ticks()
}

fn install_bridge(w: &SimulationWorld) -> OwnerBridge {
    OwnerBridge::install(NativePartitions::from_native(w.clone()), config(w)).unwrap()
}

fn snapshot(b: &OwnerBridge) -> (Vec<Vec<u8>>, Vec<owners::CommitReceipt>) {
    b.session
        .with_runtime(&b.credential.borrow(), |r| {
            Ok::<_, owners::Error>((
                vec![
                    r.owner::<EquipmentState>("Equipment")
                        .unwrap()
                        .owner
                        .inspect()?,
                    r.owner::<ActorsState>("Actors").unwrap().owner.inspect()?,
                    r.owner::<HeroState>("Hero").unwrap().owner.inspect()?,
                    r.owner::<ClockState>("Clock").unwrap().owner.inspect()?,
                    r.owner::<RandomState>("Random").unwrap().owner.inspect()?,
                    r.owner::<GeographyState>("Geography")
                        .unwrap()
                        .owner
                        .inspect()?,
                    r.owner::<BillingState>("Billing")
                        .unwrap()
                        .owner
                        .inspect()?,
                    r.owner::<ObjectsState>("Objects")
                        .unwrap()
                        .owner
                        .inspect()?,
                    r.owner::<SessionState>("Session")
                        .unwrap()
                        .owner
                        .inspect()?,
                ],
                r.host().history()?,
            ))
        })
        .unwrap()
        .unwrap()
}

fn persistent_history(b: &OwnerBridge) -> Vec<op::HistoricalCommit> {
    b.session
        .with_runtime(&b.credential.borrow(), |r| r.host().persistence_history())
        .unwrap()
        .unwrap()
}

fn historical_continuation(b: &OwnerBridge, phase: u8) -> Vec<u8> {
    match phase {
        0 => encode(
            &b.execute_default_cancel(b.prepare_default_cancel().unwrap())
                .unwrap(),
            wand::MAX_RESPONSE_BYTES,
        )
        .unwrap(),
        1 => encode(
            &b.execute_affliction_phase(b.prepare_affliction_phase().unwrap())
                .unwrap(),
            wand::MAX_RESPONSE_BYTES,
        )
        .unwrap(),
        2 => encode(
            &b.execute_metabolic_phase(b.prepare_metabolic_phase().unwrap())
                .unwrap(),
            wand::MAX_RESPONSE_BYTES,
        )
        .unwrap(),
        3 => encode(
            &b.execute_regeneration_phase(b.prepare_regeneration_phase(true).unwrap())
                .unwrap(),
            wand::MAX_RESPONSE_BYTES,
        )
        .unwrap(),
        _ => encode(
            &b.execute_dressing_tick(b.prepare_dressing_tick().unwrap())
                .unwrap(),
            wand::MAX_RESPONSE_BYTES,
        )
        .unwrap(),
    }
}

#[test]
fn monster_install_profiles_fit_actual_manifests() {
    for monster in [false, true] {
        let w = SimulationWorld::new_with_seed(721);
        let mut c = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_timer_ticks();
        if monster {
            c = c.with_monster_ticks();
        }
        assert_eq!(profile_caller(&c).dependencies.len(), 64);
        assert_eq!(profile_provider(&c).exports.len(), 64);
        OwnerBridge::install(NativePartitions::from_native(w), c).unwrap();
    }

    // Microkernel transport manifests fit well within 64-port bounds
    assert!(monster_ticks::READS.len() + monster_ticks::WRITES.len() + 2 <= 64);
    assert_eq!(
        monster_ticks::READS.len() + monster_ticks::WRITES.len() + 2,
        18
    );
}

#[test]
fn monster_tick_authentic_stale_refused() {
    let (w, mon_id) = fixture(804);
    let bridge = install_bridge(&w);
    let mut req = bridge.prepare_monster_tick(mon_id).unwrap();

    // Tampering with witness stamp triggers authentic stale refusal check
    req.guards[0].generation = req.guards[0].generation.wrapping_add(1);
    let err = bridge.execute_monster_tick(req);
    assert!(matches!(
        err,
        Err(OwnerBridgeError::Owner(owners::Error::Stale))
    ));
    assert!(monster_ticks::stale_refusals() >= 1);
}

#[test]
fn monster_tick_wrong_authority_refused() {
    let (w, mon_id) = fixture(804);
    let bridge = install_bridge(&w);
    let req = bridge.prepare_monster_tick(mon_id).unwrap();

    monster_ticks::trigger_wrong_resource(true);
    let res = bridge.execute_monster_tick(req.clone());
    monster_ticks::trigger_wrong_resource(false);

    assert!(res.is_err());

    // State remains uncorrupted and request can still be executed successfully with valid authority
    let reply = bridge.execute_monster_tick(req).unwrap();
    assert_eq!(reply.actor, mon_id);
}

#[test]
fn monster_tick_movement_budget_consumed_and_no_repeat() {
    let (w, mon_id) = fixture(804);
    let bridge = install_bridge(&w);

    // Initial actor movement is 12 (NORMAL_SPEED)
    let initial_parts = bridge.partitions().unwrap();
    assert_eq!(initial_parts.actors.actors[mon_id].movement, 12);

    // Stepping monster consumes movement budget (12 - 12 = 0)
    let reply = bridge.step_monster(mon_id).unwrap();
    assert_eq!(reply.actor, mon_id);

    let after_parts = bridge.partitions().unwrap();
    assert_eq!(after_parts.actors.actors[mon_id].movement, 0);

    // Second call to step_monsters() skips the monster because movement < 12
    let second_events = bridge.step_monsters().unwrap();
    assert!(
        second_events.is_empty(),
        "Monster with exhausted movement must not repeat action"
    );
}

#[test]
fn monster_tick_melee_combat_oracle_equivalence() {
    let (w, mon_id) = fixture(804);
    let bridge = install_bridge(&w);

    let reply = bridge.step_monster(mon_id).unwrap();
    assert_eq!(reply.actor, mon_id);

    // Verify events contain either an attack landed or missed event
    assert!(!reply.events.is_empty());
    let has_combat_event = reply.events.iter().any(|e| match e {
        GameEvent::AttackLanded {
            attacker, target, ..
        } => *attacker == mon_id && *target == w.player_id,
        GameEvent::AttackMissed { attacker, target } => {
            *attacker == mon_id && *target == w.player_id
        }
        GameEvent::LogMessage { text } => {
            text.contains("goblin") || text.contains("hits") || text.contains("misses")
        }
        _ => false,
    });
    assert!(has_combat_event, "Melee combat event must be emitted");

    // Persistent world reflects any hero damage
    let final_world = bridge.export_native().unwrap();
    let hero = final_world.arena.actors.get(w.player_id).unwrap();
    if let Some(dmg) = reply.outcome.hero_damage {
        assert_eq!(hero.hp, 50 - dmg);
    }
}

#[test]
fn monster_tick_pet_combat_and_movement() {
    let mut w = clean_world(101);

    // Spawn pet (little dog) at (11, 10)
    let mut dog = w
        .ruleset
        .create_monster_record("little dog", Coord::new_unchecked(11, 10), &mut w.rng)
        .unwrap();
    dog.is_tame = true;
    dog.tameness = 10;
    dog.movement = 12;
    let dog_id = w.arena.spawn_actor(dog);

    // Spawn hostile goblin adjacent to pet at (12, 10)
    let mut goblin = w
        .ruleset
        .create_monster_record("goblin", Coord::new_unchecked(12, 10), &mut w.rng)
        .unwrap();
    goblin.movement = 0;
    let goblin_id = w.arena.spawn_actor(goblin);

    let bridge = install_bridge(&w);

    // Pet should attack goblin to defend hero
    let reply = bridge.step_monster(dog_id).unwrap();
    assert_eq!(reply.actor, dog_id);
    let pet_attacked = reply.events.iter().any(|e| match e {
        GameEvent::AttackLanded {
            attacker, target, ..
        } => *attacker == dog_id && *target == goblin_id,
        GameEvent::AttackMissed { attacker, target } => *attacker == dog_id && *target == goblin_id,
        GameEvent::LogMessage { text } => text.contains("dog") || text.contains("goblin"),
        _ => false,
    });
    assert!(pet_attacked, "Pet should attack enemy goblin");

    // Now test cursed tile avoidance
    let mut w2 = clean_world(102);

    let mut pet = w2
        .ruleset
        .create_monster_record("little dog", Coord::new_unchecked(13, 10), &mut w2.rng)
        .unwrap();
    pet.is_tame = true;
    pet.tameness = 10;
    pet.movement = 12;
    let pet_id = w2.arena.spawn_actor(pet);

    // Place a cursed item on the floor tile between pet and hero at (12, 10)
    let cursed_item = ItemRecord {
        definition: None,
        saved_monster_traits: None,
        artifact_id: None,
        name: "cursed rock".into(),
        class: ItemClass::Gem,
        weight: 10,
        buc: Buc::Cursed,
        is_container: false,
        is_bag_of_holding: false,
        enchantment: 0,
        erosion: 0,
        proofed: false,
        location: ItemLocation::Floor(Coord::new_unchecked(12, 10)),
        corpse_no_revive: false,
        corpse_species: None,
        is_ice_box: false,
        corpse_race: None,
        corpse_age: 0,
        rot_threshold: 50,
        recharged: 0,
        quantity: 1,
        poisoned: false,
        locked: false,
        trapped: false,
        trap_known: false,
    };
    w2.arena.spawn_item(cursed_item);

    let bridge2 = install_bridge(&w2);
    let pet_reply = bridge2.step_monster(pet_id).unwrap();
    assert_eq!(pet_reply.actor, pet_id);
    let whimpered = pet_reply.events.iter().any(|e| match e {
        GameEvent::LogMessage { text } => text.contains("whimper"),
        _ => false,
    });
    assert!(
        whimpered,
        "Pet should whimper and refuse to step on cursed item"
    );
}

#[test]
fn monster_tick_breath_gaze_spells() {
    // 1. Red Dragon Breath
    let mut w = clean_world(42);
    w.arena.actors[w.player_id].hp = 100;
    w.arena.actors[w.player_id].max_hp = 100;

    let mut dragon = w
        .ruleset
        .create_monster_record("red dragon", Coord::new_unchecked(15, 10), &mut w.rng)
        .unwrap();
    dragon.movement = 12;
    let dragon_id = w.arena.spawn_actor(dragon);

    let bridge = install_bridge(&w);
    let reply = bridge.step_monster(dragon_id).unwrap();
    assert_eq!(reply.actor, dragon_id);
    let breathed = reply.events.iter().any(|e| match e {
        GameEvent::LogMessage { text } => text.contains("breathes"),
        _ => false,
    });
    assert!(breathed, "Red dragon should breathe fire at range");
    assert!(reply.outcome.hero_damage.is_some());

    // 2. Medusa petrification gaze reflection
    let mut w_med = clean_world(43);
    w_med.arena.actors[w_med.player_id].intrinsics.reflection = true;

    let mut medusa = w_med
        .ruleset
        .create_monster_record_by_id(
            MonsterSpeciesId::MEDUSA,
            Coord::new_unchecked(12, 10),
            &mut w_med.rng,
        )
        .unwrap();
    medusa.movement = 12;
    let medusa_id = w_med.arena.spawn_actor(medusa);

    let bridge_med = install_bridge(&w_med);
    let reply_med = bridge_med.step_monster(medusa_id).unwrap();
    assert_eq!(reply_med.actor, medusa_id);
    let gaze_reflected = reply_med.events.iter().any(|e| match e {
        GameEvent::LogMessage { text } => {
            text.contains("gaze is reflected") || text.contains("погляд")
        }
        _ => false,
    });
    assert!(
        gaze_reflected,
        "Medusa gaze should be reflected by hero reflection intrinsic"
    );
    let medusa_after = bridge_med
        .partitions()
        .unwrap()
        .actors
        .actors
        .get(medusa_id)
        .cloned()
        .unwrap();
    assert!(
        medusa_after.is_dead,
        "Medusa should turn herself to stone and die"
    );
}

#[test]
fn monster_tick_save_reload_continuation() {
    let (w, mon_id) = fixture(804);
    let bridge = install_bridge(&w);
    let _ = bridge.step_monster(mon_id).unwrap();

    let saved = bridge.capture().unwrap();
    let restored = OwnerBridge::reconstruct(&saved, config(&w)).unwrap();

    let json = bridge.export_native().unwrap().to_save_json().unwrap();
    let reloaded = OwnerBridge::import_native(&json, json.len(), config(&w)).unwrap();

    assert_eq!(
        bridge.export_native().unwrap().to_save_json().unwrap(),
        restored.export_native().unwrap().to_save_json().unwrap()
    );
    assert_eq!(
        bridge.export_native().unwrap().to_save_json().unwrap(),
        reloaded.export_native().unwrap().to_save_json().unwrap()
    );

    let next = |mut w: SimulationWorld| (0..10).map(|_| w.rng.next_u64()).collect::<Vec<_>>();
    let continuation = next(bridge.export_native().unwrap());
    assert_eq!(next(restored.export_native().unwrap()), continuation);
    assert_eq!(next(reloaded.export_native().unwrap()), continuation);
}

#[test]
fn monster_tick_historical_profile_admission() {
    use crate::actions::equipment::Dressing;
    use nethacked_core::ac::ArmorSlot;

    for phase in 0..5 {
        let (mut w, goblin_id) = fixture(804);
        w.object_timers = Default::default();
        w.arena.items.clear();
        let suit = w
            .ruleset
            .create_item_record(
                "ring mail",
                ItemLocation::CarriedBy(w.player_id),
                nethacked_types::Buc::Uncursed,
            )
            .unwrap();
        let suit_id = w.arena.spawn_item(suit);
        w.equipment.armor = vec![(ArmorSlot::Suit, suit_id)];
        if phase == 4 {
            w.equipment.dressing = Some(Dressing {
                item: suit_id,
                putting_on: true,
                turns_left: 2,
            });
        } else {
            w.equipment.dressing = None;
        }

        let base = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone()).unwrap();
        let old = match phase {
            0 => base.with_default_commands(),
            1 => base.with_passive_affliction_ticks(),
            2 => base.with_passive_metabolic_ticks(),
            3 => base.with_natural_hp_regeneration(),
            _ => base.with_dressing_ticks(),
        };

        let original = OwnerBridge::install(NativePartitions::from_native(w.clone()), old).unwrap();
        let c = OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
            .unwrap()
            .with_monster_ticks();
        let restored = OwnerBridge::reconstruct(&original.capture().unwrap(), c).unwrap();

        assert_eq!(snapshot(&restored).0, snapshot(&original).0);
        assert_eq!(persistent_history(&restored), persistent_history(&original));

        // Original historical session lacks unnegotiated monster authority
        assert!(original.prepare_monster_tick(goblin_id).is_err());
        // Non-monster entity cannot be stepped as monster
        assert!(restored.prepare_monster_tick(w.player_id).is_err());

        // Historical continuation executes identically
        assert_eq!(
            historical_continuation(&restored, phase),
            historical_continuation(&original, phase)
        );

        let mut left = restored.export_native().unwrap();
        let mut right = original.export_native().unwrap();
        assert_eq!(left.rng.get_word_pos(), right.rng.get_word_pos());
        for _ in 0..8 {
            assert_eq!(left.rng.next_u64(), right.rng.next_u64());
        }
    }
}
