//! P07: released 16ff59115315917b93185d026aeefea06db9b0f4.
//! objects.h:513; worn.c:73; do_wear.c:909,1690,2375;
//! mhitu.c:1187 -> uhitm.c:2561-2575; zap.c:4421-4429.
use nethacked_arena::{ItemId, ItemLocation};
use nethacked_core::ActionAst;
use nethacked_data::{ruleset::PackManifest, CharacterConfig, ItemKindId, RoleId};
use nethacked_sim::{Buc, Coord, GameEvent, Ruleset, SimulationWorld, Tile};
use nethacked_types::{Attack, AttackType, DamageType};
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::sync::Arc;

fn world() -> SimulationWorld {
    let mut sim = SimulationWorld::new_with_character(
        42,
        CharacterConfig {
            role: RoleId::Valkyrie,
            ..Default::default()
        },
    );
    let others: Vec<_> = sim
        .arena
        .actors
        .iter()
        .filter(|(id, _)| *id != sim.player_id)
        .map(|(id, _)| id)
        .collect();
    for id in others {
        sim.arena.actors.remove(id);
    }
    let items: Vec<_> = sim.arena.items.iter().map(|(id, _)| id).collect();
    for id in items {
        sim.arena.destroy_item(id);
    }
    sim.equipment.armor.clear();
    sim.wielded_item = None;
    sim.scheduler.monster_speed = 0;
    sim.scheduler.monster_energy = 0;
    sim.rng = ChaCha8Rng::seed_from_u64(7);
    sim
}
fn add(sim: &mut SimulationWorld, kind: ItemKindId) -> ItemId {
    sim.arena.spawn_item(
        sim.ruleset
            .create_item_record_by_id(kind, ItemLocation::CarriedBy(sim.player_id), Buc::Uncursed)
            .unwrap(),
    )
}
fn index(sim: &SimulationWorld, id: ItemId) -> usize {
    sim.arena
        .items_carried_by(sim.player_id)
        .iter()
        .position(|item| *item == id)
        .unwrap()
}
fn wear(sim: &mut SimulationWorld, id: ItemId) {
    sim.step_player_action(ActionAst::Wear(index(sim, id)));
}
fn complete(sim: &mut SimulationWorld) {
    while sim.equipment.dressing.is_some() {
        sim.step_player_action(ActionAst::Wait);
    }
}
fn thief(sim: &mut SimulationWorld) -> nethacked_arena::ActorId {
    let vanilla = Arc::clone(&sim.ruleset);
    let mut monsters = vanilla.monsters.clone();
    let def = monsters
        .iter_mut()
        .find(|m| m.name == "wood nymph")
        .unwrap();
    def.attacks = vec![Attack {
        at: AttackType::Claw,
        ad: DamageType::StealItem,
        n: 0,
        d: 0,
    }];
    def.level = 100;
    sim.ruleset = Arc::new(Ruleset::from_parts(
        PackManifest {
            id: "test-thief".into(),
            name: "Test thief".into(),
            version: "1".into(),
            base: "vanilla".into(),
            description: String::new(),
        },
        monsters,
        vanilla.items.clone(),
        vanilla.roles.clone(),
        vanilla.races.clone(),
        vanilla.mechanics.clone(),
    ));
    let coord = Coord::new(sim.hero_coord().x + 1, sim.hero_coord().y).unwrap();
    sim.level.set_tile(coord, Tile::Room);
    let id = sim.spawn_monster_by_name("wood nymph", coord).unwrap();
    sim.arena.actors.get_mut(id).unwrap().is_peaceful = false;
    sim.arena.actors[id].movement = 12;
    sim.arena.actors[id].speed = 0;
    sim.scheduler.hero_energy = 24;
    sim.level.no_teleport = true;
    id
}
fn pickup(sim: &mut SimulationWorld) -> ItemId {
    let id = sim.arena.spawn_item(
        sim.ruleset
            .create_item_record_by_id(
                ItemKindId::RED_DRAGON_SCALE_MAIL,
                ItemLocation::Floor(sim.hero_coord()),
                Buc::Uncursed,
            )
            .unwrap(),
    );
    sim.step_player_action(ActionAst::PickUp);
    assert_eq!(
        sim.arena.items.get(id).unwrap().location,
        ItemLocation::CarriedBy(sim.player_id)
    );
    id
}
fn intrinsic(sim: &SimulationWorld) -> bool {
    sim.arena
        .actors
        .get(sim.player_id)
        .unwrap()
        .intrinsics
        .fire_resistance
}
fn effective(sim: &SimulationWorld) -> bool {
    sim.effective_hero_intrinsics().fire_resistance
}
fn spawn_attack(sim: &mut SimulationWorld, breath: bool) -> nethacked_arena::ActorId {
    if breath {
        // Preserve the pinned fire/dressing stream independently of monster sex.
        let mut rules = (*sim.ruleset).clone();
        let def = rules
            .monsters
            .iter_mut()
            .find(|def| def.id == Some(nethacked_data::MonsterSpeciesId::RED_DRAGON))
            .unwrap();
        def.flags.push("male".into());
        rules.reindex();
        sim.ruleset = Arc::new(rules);
    }
    let pc = Coord::new_unchecked(10, 10);
    sim.arena.actors.get_mut(sim.player_id).unwrap().coord = pc;
    for x in 8..=15 {
        for y in 8..=12 {
            sim.level.set_tile(Coord::new_unchecked(x, y), Tile::Room);
        }
    }
    let id = sim
        .spawn_monster_by_name(
            if breath {
                "red dragon"
            } else {
                "fire elemental"
            },
            Coord::new_unchecked(if breath { 13 } else { 11 }, 10),
        )
        .unwrap();
    let mon = sim.arena.actors.get_mut(id).unwrap();
    mon.is_peaceful = false;
    mon.speed = 0;
    sim.arena.actors.get_mut(sim.player_id).unwrap().max_hp = 5000;
    sim.arena.actors.get_mut(sim.player_id).unwrap().hp = 5000;
    id
}
fn attack(sim: &mut SimulationWorld, id: nethacked_arena::ActorId, breath: bool, resisted: bool) {
    let mut observed = false;
    for _ in 0..80 {
        let p = sim.arena.actors.get_mut(sim.player_id).unwrap();
        p.hp = 5000;
        let mon = sim.arena.actors.get_mut(id).unwrap();
        mon.coord = Coord::new_unchecked(if breath { 13 } else { 11 }, 10);
        mon.mspec_used = 0;
        sim.arena.actors[id].movement = 12;
        sim.scheduler.hero_energy = 24;
        let events = sim.step_player_action(ActionAst::Wait);
        let hp = sim.arena.actors.get(sim.player_id).unwrap().hp;
        if breath {
            if hp < 5000 {
                assert!(!resisted);
                assert!((6..=36).contains(&(5000 - hp)));
                observed = true;
            }
            if events.iter().any(|e|matches!(e,GameEvent::LogMessage{text} if text.contains("engulfed in the blast"))) { assert!(resisted); observed=true; }
        } else {
            for event in events {
                if let GameEvent::AttackLanded {
                    attacker, damage, ..
                } = event
                {
                    if attacker == id {
                        if resisted {
                            assert_eq!(damage, 0);
                        } else {
                            assert!((3..=18).contains(&damage));
                        }
                        observed = true;
                    }
                }
            }
        }
        if resisted {
            assert_eq!(hp, 5000);
        }
        if observed {
            break;
        }
    }
    assert!(observed, "public attack did not exercise HP branch");
}
#[test]
fn public_acquisition_wear_both_hp_paths_removal_and_intrinsic_source_preservation() {
    for breath in [false, true] {
        for independent in [false, true] {
            let mut sim = world();
            sim.arena
                .actors
                .get_mut(sim.player_id)
                .unwrap()
                .intrinsics
                .fire_resistance = independent;
            assert_eq!(effective(&sim), independent);
            let suit = pickup(&mut sim);
            assert_eq!(effective(&sim), independent); // carried alone
            let mon = spawn_attack(&mut sim, breath);
            attack(&mut sim, mon, breath, independent);
            sim.arena.actors[mon].movement = 0;
            wear(&mut sim, suit);
            assert_eq!(sim.equipment.dressing.as_ref().unwrap().turns_left, 4);
            assert!(effective(&sim));
            assert_eq!(intrinsic(&sim), independent);
            attack(&mut sim, mon, breath, true);
            complete(&mut sim);
            sim.arena.actors[mon].movement = 0;
            sim.step_player_action(ActionAst::Remove(index(&sim, suit)));
            assert_eq!(sim.equipment.dressing.as_ref().unwrap().turns_left, 4);
            assert!(effective(&sim));
            assert_eq!(intrinsic(&sim), independent);
            attack(&mut sim, mon, breath, true);
            sim.arena.actors[mon].movement = 0;
            complete(&mut sim);
            assert_eq!(effective(&sim), independent);
            assert_eq!(intrinsic(&sim), independent);
            attack(&mut sim, mon, breath, independent);
        }
    }
}
#[test]
fn genuine_interruption_clears_don_preserves_doff_and_intrinsic_then_retry_works() {
    for removing in [false, true] {
        for independent in [false, true] {
            let mut sim = world();
            let suit = pickup(&mut sim);
            let loot = add(&mut sim, ItemKindId::DAGGER);
            sim.arena
                .actors
                .get_mut(sim.player_id)
                .unwrap()
                .intrinsics
                .fire_resistance = independent;
            if removing {
                wear(&mut sim, suit);
                complete(&mut sim);
            }
            let id = thief(&mut sim);
            sim.rng = ChaCha8Rng::seed_from_u64(7);
            let action = if removing {
                ActionAst::Remove(index(&sim, suit))
            } else {
                ActionAst::Wear(index(&sim, suit))
            };
            let events = sim.step_player_action(action);
            assert!(
                events.contains(&GameEvent::ItemStolen {
                    actor: id,
                    item: loot
                }),
                "{events:?}"
            );
            assert!(events.contains(&GameEvent::DressingInterrupted {
                item: suit,
                putting_on: !removing
            }));
            assert!(sim.equipment.dressing.is_none());
            assert_eq!(effective(&sim), removing || independent);
            assert_eq!(intrinsic(&sim), independent);
            sim.arena.actors.remove(id);
            sim.scheduler.monster_energy = 0;
            let action = if removing {
                ActionAst::Remove(index(&sim, suit))
            } else {
                ActionAst::Wear(index(&sim, suit))
            };
            sim.step_player_action(action);
            complete(&mut sim);
            assert_eq!(effective(&sim), !removing || independent);
            assert_eq!(intrinsic(&sim), independent);
        }
    }
}
#[test]
fn owned_renamed_custom_and_explicit_clear_metadata_survive_full_midoperation_save() {
    for property in [
        None,
        Some(nethacked_data::items::WornProperty::FireResistance),
    ] {
        for custom in [false, true] {
            for removing in [false, true] {
                let mut sim = world();
                let mut rs = (*sim.ruleset).clone();
                let def = rs
                    .items
                    .iter_mut()
                    .find(|d| d.id == Some(ItemKindId::RED_DRAGON_SCALE_MAIL))
                    .unwrap();
                def.name = "owned coat".into();
                def.worn_property = property;
                if custom {
                    def.id = None;
                }
                rs.reindex();
                sim.ruleset = Arc::new(rs);
                let suit = sim.arena.spawn_item(
                    sim.ruleset
                        .create_item_record(
                            "owned coat",
                            ItemLocation::Floor(sim.hero_coord()),
                            Buc::Uncursed,
                        )
                        .unwrap(),
                );
                sim.step_player_action(ActionAst::PickUp);
                wear(&mut sim, suit);
                if removing {
                    complete(&mut sim);
                    sim.step_player_action(ActionAst::Remove(index(&sim, suit)));
                }
                assert_eq!(effective(&sim), property.is_some());
                assert!(!intrinsic(&sim));
                let mut loaded = SimulationWorld::from_save_json(
                    &sim.to_save_json().unwrap(),
                    Arc::clone(&sim.ruleset),
                    &sim.ruleset_ref,
                )
                .unwrap();
                while sim.equipment.dressing.is_some() {
                    assert_eq!(
                        sim.step_player_action(ActionAst::Wait),
                        loaded.step_player_action(ActionAst::Wait)
                    );
                    assert_eq!(effective(&sim), effective(&loaded));
                    assert_eq!(
                        serde_json::from_str::<serde_json::Value>(&sim.to_save_json().unwrap())
                            .unwrap(),
                        serde_json::from_str::<serde_json::Value>(&loaded.to_save_json().unwrap())
                            .unwrap()
                    );
                }
                assert_eq!(effective(&loaded), property.is_some() && !removing);
                let mon = spawn_attack(&mut sim, true);
                let loaded_mon = spawn_attack(&mut loaded, true);
                assert_eq!(mon, loaded_mon);
                sim.arena.actors[mon].movement = 12;
                loaded.arena.actors[loaded_mon].movement = 12;
                sim.scheduler.hero_energy = 24;
                loaded.scheduler.hero_energy = 24;
                assert_eq!(
                    sim.step_player_action(ActionAst::Wait),
                    loaded.step_player_action(ActionAst::Wait)
                );
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&sim.to_save_json().unwrap())
                        .unwrap(),
                    serde_json::from_str::<serde_json::Value>(&loaded.to_save_json().unwrap())
                        .unwrap()
                );
                attack(&mut sim, mon, true, property.is_some() && !removing);
                attack(
                    &mut loaded,
                    loaded_mon,
                    true,
                    property.is_some() && !removing,
                );
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&sim.to_save_json().unwrap())
                        .unwrap(),
                    serde_json::from_str::<serde_json::Value>(&loaded.to_save_json().unwrap())
                        .unwrap()
                );
                sim.arena.actors.remove(mon);
                loaded.arena.actors.remove(loaded_mon);
                let mon = spawn_attack(&mut sim, false);
                let loaded_mon = spawn_attack(&mut loaded, false);
                attack(&mut sim, mon, false, property.is_some() && !removing);
                attack(
                    &mut loaded,
                    loaded_mon,
                    false,
                    property.is_some() && !removing,
                );
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&sim.to_save_json().unwrap())
                        .unwrap(),
                    serde_json::from_str::<serde_json::Value>(&loaded.to_save_json().unwrap())
                        .unwrap()
                );
                assert_eq!(sim.rng.next_u64(), loaded.rng.next_u64());
            }
        }
    }
}
#[test]
fn native_public_attack_draws_preserve_c_hp_guard_order_with_and_without_worn_source() {
    use rand::Rng;
    for breath in [false, true] {
        for worn in [false, true] {
            let mut sim = world();
            let suit = pickup(&mut sim);
            if worn {
                wear(&mut sim, suit);
                complete(&mut sim);
            }
            let id = spawn_attack(&mut sim, breath);
            sim.arena.actors.get_mut(sim.player_id).unwrap().hp = 5000;
            sim.arena.actors[id].movement = 12;
            sim.scheduler.hero_energy = 12;
            sim.rng = ChaCha8Rng::seed_from_u64(if breath { 1 } else { 7 });
            let mut expected = sim.rng.clone();
            let raw = if breath {
                // breamm rn2(3), zhitu rnd/chance then d(6,6), cooldown.
                assert_ne!(expected.random_range(0..3u32), 0);
                let chance = expected.random_range(0..20u32);
                assert!(chance > 0 && chance <= (if worn { 1 } else { 10 }) + 10);
                let damage = (0..6).map(|_| expected.random_range(1..=6u32)).sum::<u32>();
                if expected.random_range(0..3u32) == 0 {
                    expected.random_range(0..18u8);
                }
                damage
            } else {
                // mhitu mattacku rnd(20), hitmu d(3,6) before resistance.
                let hit = expected.random_range(1..=20u32);
                assert!(hit < 8 + (if worn { 1 } else { 10 }) + 10);
                (0..3).map(|_| expected.random_range(1..=6u32)).sum::<u32>()
            };
            // P09 exhausted-round allocation is unconditional even at speed zero.
            expected.random_range(0..12u32);
            // P05 allmain regeneration follows the attack, drawing only when injured.
            let heal = if worn {
                0
            } else {
                u32::from(
                    sim.arena.actors.get(sim.player_id).unwrap().level as i32 + sim.hero_attr(4)
                        > expected.random_range(0..100u32) as i32,
                )
            };
            let events = sim.step_player_action(ActionAst::Wait);
            let hp = sim.arena.actors.get(sim.player_id).unwrap().hp;
            assert_eq!(hp, 5000 - if worn { 0 } else { raw - heal }, "{events:?}");
            assert_eq!(sim.rng, expected, "no source-dependent RNG draws");
        }
    }
}
