use nethacked_arena::ActorSpecies;
use nethacked_data::{AiBehavior, Attack, AttackType, DamageType, MonsterSpeciesId as M, Ruleset};
use nethacked_sim::{ActionAst, GameEvent, SimulationWorld, Tile};
use nethacked_types::Coord;
use rand::RngCore;
use std::sync::Arc;

const HERO: Coord = Coord::new_unchecked(10, 10);
const MON: Coord = Coord::new_unchecked(11, 10);

fn world(custom: bool) -> (SimulationWorld, nethacked_arena::ActorId) {
    let mut world = SimulationWorld::new_with_seed(413);
    world.arena.actors.retain(|id, _| id == world.player_id);
    world.arena.items.clear();
    for x in 5..=20 {
        for y in 5..=15 {
            world.level.set_tile(Coord::new_unchecked(x, y), Tile::Room);
        }
    }
    let hero = world.arena.actors.get_mut(world.player_id).unwrap();
    hero.coord = HERO;
    hero.hp = 10000;
    hero.max_hp = 10000;
    hero.ac = 10;
    let mut rules = (*Ruleset::vanilla()).clone();
    let def = rules
        .monsters
        .iter_mut()
        .find(|m| m.id == Some(M::GIANT_ANT))
        .unwrap();
    def.name = "Owned fighter".into();
    def.level = 100;
    def.speed = 12;
    def.sample_hp = false;
    def.base_hp = 51;
    def.max_hp = 61;
    def.always_hostile = true;
    def.peaceful_by_default = false;
    def.abilities.clear();
    def.attacks = vec![
        Attack {
            at: AttackType::Bite,
            ad: DamageType::Phys,
            n: 1,
            d: 1,
        },
        Attack {
            at: AttackType::Claw,
            ad: DamageType::Phys,
            n: 2,
            d: 1,
        },
    ];
    if custom {
        let mut added = def.clone();
        added.id = None;
        added.name = "Pack fighter".into();
        rules.monsters.push(added);
    }
    rules.reindex();
    world.ruleset = Arc::new(rules);
    let id = world
        .spawn_monster_by_name(
            if custom {
                "Pack fighter"
            } else {
                "Owned fighter"
            },
            MON,
        )
        .unwrap();
    world.arena.actors.get_mut(id).unwrap().name = "red dragon".into();
    world.arena.actors[id].movement = 12;
    (world, id)
}

#[test]
fn renamed_owned_attacks_continue_identically_after_full_save_reload() {
    for custom in [false, true] {
        let (mut world, id) = world(custom);
        // Isolate attack damage with a synthetic zero natural-healing threshold.
        // Keep ordinary-form regeneration eligible so its RNG draws still cross
        // the full-save boundary and participate in the continuation checks.
        world.arena.actors.get_mut(world.player_id).unwrap().level = 0;
        world.progress.attr_base[4] = 0;
        let actor = world.arena.actors.get(id).unwrap();
        assert_eq!((actor.hp, actor.max_hp), (51, 61));
        assert_eq!(
            actor.species,
            Some(if custom {
                ActorSpecies::Custom("pack fighter".into())
            } else {
                ActorSpecies::Vanilla("GIANT_ANT".into())
            })
        );
        let saved = world.to_save_json().unwrap();
        let mut loaded =
            SimulationWorld::from_save_json(&saved, world.ruleset.clone(), &world.ruleset_ref)
                .unwrap();
        assert_eq!(
            serde_json::to_value(&world).unwrap(),
            serde_json::to_value(&loaded).unwrap()
        );
        for _ in 0..5 {
            let before = world.arena.actors.get(world.player_id).unwrap().hp;
            let events = world.step_player_action(ActionAst::Wait);
            assert_eq!(
                world.arena.actors.get(world.player_id).unwrap().hp,
                before - 3
            );
            let damages: Vec<_> = events
                .iter()
                .filter_map(|e| match e {
                    GameEvent::AttackLanded {
                        attacker, damage, ..
                    } if *attacker == id => Some(*damage),
                    _ => None,
                })
                .collect();
            assert_eq!(damages, [1, 2]);
            assert_eq!(events, loaded.step_player_action(ActionAst::Wait));
            assert_eq!(
                serde_json::to_value(&world).unwrap(),
                serde_json::to_value(&loaded).unwrap()
            );
        }
        assert_eq!(world.rng.next_u64(), loaded.rng.next_u64());
    }
}

#[test]
fn renamed_peaceful_stationary_actor_uses_owned_ai() {
    for custom in [false, true] {
        let (mut world, id) = world(custom);
        let mut rules = (*world.ruleset).clone();
        let def = rules
            .monsters
            .iter_mut()
            .find(|m| {
                m.name
                    == if custom {
                        "Pack fighter"
                    } else {
                        "Owned fighter"
                    }
            })
            .unwrap();
        def.ai_behavior = AiBehavior::Stationary;
        rules.reindex();
        world.ruleset = Arc::new(rules);
        world.arena.actors.get_mut(id).unwrap().is_peaceful = true;
        for _ in 0..5 {
            world.step_player_action(ActionAst::Wait);
            assert_eq!(world.arena.actors.get(id).unwrap().coord, MON);
        }
    }
}

#[test]
fn pet_promotion_updates_identity_even_with_renamed_owned_definitions() {
    let (mut world, id) = world(false);
    let mut rules = (*world.ruleset).clone();
    rules
        .monsters
        .iter_mut()
        .find(|m| m.id == Some(M::LITTLE_DOG))
        .unwrap()
        .name = "Owned puppy".into();
    rules
        .monsters
        .iter_mut()
        .find(|m| m.id == Some(M::DOG))
        .unwrap()
        .name = "Owned adult".into();
    rules.reindex();
    world.ruleset = Arc::new(rules);
    let mut pet = world
        .ruleset
        .create_monster_record_by_id(M::LITTLE_DOG, MON, &mut world.rng)
        .unwrap();
    pet.name = "Fido".into();
    pet.is_tame = true;
    pet.level = 3;
    world.arena.actors[id] = pet;
    world.feed_companion_pet(id, 0);
    let pet = &world.arena.actors[id];
    assert_eq!(pet.species, Some(ActorSpecies::Vanilla("DOG".into())));
    assert_eq!((pet.name.as_str(), pet.hp, pet.max_hp), ("dog", 24, 24));
    assert_eq!(
        world.ruleset.monster_for_actor(pet).unwrap().name,
        "Owned adult"
    );
    let mut rules = (*world.ruleset).clone();
    rules.monsters.retain(|def| def.id != Some(M::LARGE_DOG));
    rules.reindex();
    world.ruleset = Arc::new(rules);
    world.arena.actors[id].level = 6;
    world.feed_companion_pet(id, 0);
    let pet = &world.arena.actors[id];
    assert_eq!(pet.species, Some(ActorSpecies::Vanilla("LARGE_DOG".into())));
    assert_eq!(
        (pet.name.as_str(), pet.hp, pet.max_hp),
        ("large dog", 45, 45)
    );
    assert!(world.ruleset.monster_for_actor(pet).is_none());
}

#[test]
fn full_legacy_world_load_retains_absent_identity_and_name_lookup() {
    let (world, id) = world(false);
    let mut old: serde_json::Value = serde_json::from_str(&world.to_save_json().unwrap()).unwrap();
    fn remove_identity(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.remove("species");
                for value in map.values_mut() {
                    remove_identity(value);
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    remove_identity(value);
                }
            }
            _ => {}
        }
    }
    remove_identity(&mut old);
    let loaded = SimulationWorld::from_save_json(
        &serde_json::to_string(&old).unwrap(),
        world.ruleset.clone(),
        &world.ruleset_ref,
    )
    .unwrap();
    assert!(loaded
        .arena
        .actors
        .values()
        .all(|actor| actor.species.is_none()));
    assert_eq!(
        loaded
            .ruleset
            .monster_for_actor(&loaded.arena.actors[id])
            .unwrap()
            .id,
        Some(M::RED_DRAGON)
    );
    assert_eq!(serde_json::to_value(&loaded).unwrap(), old);
}

#[test]
fn explicit_missing_identity_uses_generic_attack_without_display_species_lookup() {
    for missing in [
        ActorSpecies::Vanilla("MISSING".into()),
        ActorSpecies::Custom("missing".into()),
    ] {
        let (mut world, id) = world(false);
        world.arena.actors[id].species = Some(missing);
        let events = world.step_player_action(ActionAst::Wait);
        let damages: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                GameEvent::AttackLanded {
                    attacker, damage, ..
                } if *attacker == id => Some(*damage),
                _ => None,
            })
            .collect();
        assert_eq!(damages.len(), 1);
        assert!((1..=6).contains(&damages[0]));
    }
}
