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
fn public_attack_and_bump_obey_literal_attribute_thresholds_and_draws() {
    // Independent released-C abon expectations. No production formula is
    // used to choose the attack threshold. Basic bare-hand hit bonus is +1.
    for (strength, dexterity, level, abon, dbon) in [
        (3, 3, 3, -5, -1),
        (10, 10, 1, 1, 0),
        (10, 10, 2, 1, 0),
        (10, 10, 3, 0, 0),
        (67, 14, 3, 1, 3),
        (68, 14, 3, 2, 3),
        (117, 15, 3, 3, 5),
        (118, 15, 3, 4, 6),
        (125, 25, 3, 14, 6),
    ] {
        for custom in [false, true] {
            for bump in [false, true] {
                for hit in [false, true] {
                    let (mut world, target) = world(custom);
                    world.progress.attr_base[0] = strength;
                    world.progress.attr_base[3] = dexterity;
                    world.progress.attr_max = world.progress.attr_base;
                    world.arena.actors.get_mut(world.player_id).unwrap().level = level;
                    world.rng = SeedableRng::seed_from_u64(37);
                    let mut expected_rng = world.rng.clone();
                    let roll = expected_rng.random_range(1..=20u32);
                    // tmp == roll misses; tmp == roll+1 hits (strict >).
                    world.arena.actors.get_mut(target).unwrap().ac =
                        roll as i32 - (1 + level as i32 + 1 + abon) + i32::from(hit);
                    let expected_damage = if hit {
                        // Literal released-C dbon expectations, combined
                        // before the existing minimum-one damage rule.
                        Some((expected_rng.random_range(1..=2u32) as i32 + dbon).max(1) as u32)
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
                    assert_eq!(attacks, [expected_damage], "STR{strength} DX{dexterity} level{level} custom{custom} bump{bump} hit{hit}");
                    assert_eq!(world.arena.actors.get(world.player_id).unwrap().coord, HERO);
                    assert_eq!(world.arena.actors.get(target).unwrap().max_hp, 61);
                    assert_eq!(
                        world.arena.actors.get(target).unwrap().hp,
                        51 - expected_damage.unwrap_or(0)
                    );
                    assert_eq!(
                        serde_json::to_value(&world).unwrap(),
                        serde_json::to_value(&loaded).unwrap()
                    );
                    let next = expected_rng.next_u64();
                    assert_eq!(world.rng.next_u64(), next, "hit/miss draw order");
                    assert_eq!(loaded.rng.next_u64(), next, "save RNG continuation");
                }
            }
        }
    }
}
