use nethacked_arena::ActorId;
use nethacked_data::{AiBehavior, MonsterSpeciesId, Ruleset};
use nethacked_sim::{ActionAst, Coord, Direction, GameEvent, SimulationWorld, Tile};
use nethacked_types::{SkillClass, SkillLevel};
use rand::{Rng, RngCore, SeedableRng};
use std::sync::Arc;

const HERO: Coord = Coord::new_unchecked(10, 10);
const TARGET: Coord = Coord::new_unchecked(11, 10);

fn world(custom: bool) -> (SimulationWorld, ActorId) {
    let mut world = SimulationWorld::new_with_seed(413);
    world.arena.actors.retain(|id, _| id == world.player_id);
    world.arena.items.clear();
    world.wielded_item = None;
    world.player_luck = 0;
    world
        .hero
        .skills
        .skills
        .insert(SkillClass::BareHanded, SkillLevel::Basic);
    world.arena.actors.get_mut(world.player_id).unwrap().coord = HERO;
    world.level.set_tile(HERO, Tile::Room);
    world.level.set_tile(TARGET, Tile::Room);
    let mut rules = (*Ruleset::vanilla()).clone();
    let def = rules
        .monsters
        .iter_mut()
        .find(|m| m.id == Some(MonsterSpeciesId::GIANT_ANT))
        .unwrap();
    def.name = "Owned target".into();
    def.sample_hp = false;
    def.base_hp = 51;
    def.max_hp = 61;
    def.speed = 0;
    def.ai_behavior = AiBehavior::Stationary;
    def.always_hostile = true;
    def.peaceful_by_default = false;
    def.attacks.clear();
    if custom {
        let mut added = def.clone();
        added.id = None;
        added.name = "Pack target".into();
        rules.monsters.push(added);
    }
    rules.reindex();
    world.ruleset = Arc::new(rules);
    let id = world
        .spawn_monster_by_name(
            if custom {
                "Pack target"
            } else {
                "Owned target"
            },
            TARGET,
        )
        .unwrap();
    // Combat must keep using owned identity, even after a display rename.
    world.arena.actors.get_mut(id).unwrap().name = "red dragon".into();
    assert_eq!(
        (
            world.arena.actors.get(id).unwrap().hp,
            world.arena.actors.get(id).unwrap().max_hp
        ),
        (51, 61)
    );
    world.scheduler.hero_energy = 24;
    (world, id)
}

#[test]
fn public_strength_damage_preserves_weapon_skill_floor_owned_target_and_rng() {
    // Literal C dbon/abon at every damage tier edge; Basic barehand +1
    // hit/0 damage, Expert dagger +3 hit/+2 damage in the existing sim.
    for (strength, dbon, abon) in [
        (3, -1, -2),
        (5, -1, -2),
        (6, 0, -1),
        (15, 0, 0),
        (16, 1, 0),
        (17, 1, 1),
        (18, 2, 1),
        (19, 3, 1),
        (93, 3, 2),
        (94, 4, 2),
        (108, 4, 2),
        (109, 5, 2),
        (117, 5, 2),
        (118, 6, 3),
        (119, 6, 3),
        (125, 6, 3),
    ] {
        for custom in [false, true] {
            for bump in [false, true] {
                for weapon in [false, true] {
                    for hit in [false, true] {
                        let (mut world, target) = world(custom);
                        world.progress.attr_base[0] = strength;
                        world.progress.attr_base[3] = 14;
                        world.progress.attr_max = world.progress.attr_base;
                        world.arena.actors.get_mut(world.player_id).unwrap().level = 3;
                        let (die, enchant, skill_hit, skill_damage) = if weapon {
                            let mut item = nethacked_data::create_item_record(
                                nethacked_data::ItemKindId::DAGGER,
                                nethacked_arena::ItemLocation::CarriedBy(world.player_id),
                                nethacked_types::Buc::Uncursed,
                            );
                            item.enchantment = -5;
                            world.wielded_item = Some(world.arena.spawn_item(item));
                            world
                                .hero
                                .skills
                                .skills
                                .insert(SkillClass::Dagger, SkillLevel::Expert);
                            // Owned ant-sized identity selects d4 despite
                            // display rename to red dragon (large uses d3).
                            (4, -5, 3, 2)
                        } else {
                            (2, 0, 1, 0)
                        };
                        world.rng = SeedableRng::seed_from_u64(37);
                        let mut expected_rng = world.rng.clone();
                        let roll = expected_rng.random_range(1..=20u32);
                        world.arena.actors.get_mut(target).unwrap().ac =
                            roll as i32 - (1 + 3 + enchant + skill_hit + abon) + i32::from(hit);
                        let damage = if hit {
                            Some(
                                (expected_rng.random_range(1..=die) + enchant + skill_damage + dbon)
                                    .max(1) as u32,
                            )
                        } else {
                            None
                        };
                        let saved = world.to_save_json().unwrap();
                        let mut loaded = SimulationWorld::from_save_json(
                            &saved,
                            world.ruleset.clone(),
                            &world.ruleset_ref,
                        )
                        .unwrap();
                        let action = if bump {
                            ActionAst::Move(Direction::East)
                        } else {
                            ActionAst::MeleeAttack(TARGET)
                        };
                        let events = world.step_player_action(action.clone());
                        assert_eq!(events, loaded.step_player_action(action));
                        let attacks: Vec<_> = events
                            .iter()
                            .filter_map(|event| match event {
                                GameEvent::AttackLanded {
                                    attacker,
                                    target: id,
                                    damage,
                                    ..
                                } if *attacker == world.player_id && *id == target => {
                                    Some(Some(*damage))
                                }
                                GameEvent::AttackMissed {
                                    attacker,
                                    target: id,
                                    ..
                                } if *attacker == world.player_id && *id == target => Some(None),
                                _ => None,
                            })
                            .collect();
                        assert_eq!(
                            attacks,
                            [damage],
                            "STR{strength} weapon{weapon} custom{custom} bump{bump} hit{hit}"
                        );
                        assert_eq!(
                            world.arena.actors.get(target).unwrap().hp,
                            51 - damage.unwrap_or(0)
                        );
                        assert_eq!(world.arena.actors.get(target).unwrap().max_hp, 61);
                        assert_eq!(world.arena.actors.get(world.player_id).unwrap().coord, HERO);
                        assert_eq!(
                            serde_json::to_value(&world).unwrap(),
                            serde_json::to_value(&loaded).unwrap()
                        );
                        let next = expected_rng.next_u64();
                        assert_eq!(world.rng.next_u64(), next);
                        assert_eq!(loaded.rng.next_u64(), next);
                    }
                }
            }
        }
    }
}

#[test]
fn polymorphed_hero_does_not_receive_stored_strength_damage() {
    let (mut world, target) = world(true);
    world.progress.attr_base[0] = 125;
    world.hero.polymorph = Some(nethacked_types::PolymorphForm {
        monster_id: 1,
        hp: 20,
        max_hp: 20,
        duration: 100,
    });
    world.arena.actors.get_mut(target).unwrap().ac = 30;
    world.rng = SeedableRng::seed_from_u64(37);
    let mut expected_rng = world.rng.clone();
    let _ = expected_rng.random_range(1..=20u32);
    let damage = expected_rng.random_range(1..=2u32);
    let events = world.step_player_action(ActionAst::MeleeAttack(TARGET));
    assert!(events.iter().any(|event| matches!(event, GameEvent::AttackLanded { attacker, target: id, damage: actual, .. } if *attacker == world.player_id && *id == target && *actual == damage)));
    assert_eq!(world.arena.actors.get(target).unwrap().hp, 51 - damage);
    assert_eq!(world.rng.next_u64(), expected_rng.next_u64());
}
