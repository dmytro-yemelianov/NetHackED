//! Integration test suite for NetRust simulation engine.

use netrust_arena::{ActorRecord, ItemLocation, ItemRecord};
use netrust_core::engraving::EngravingMedium;
use netrust_data::{
    create_item_record, create_monster_record, CharacterConfig, Gender, ItemKindId,
    MonsterSpeciesId, RaceId, RoleId,
};
use netrust_dungeon::RoomType;
use netrust_sim::{
    ActionAst, Alignment, Buc, Coord, Direction, DoorState, GameEvent, Intrinsics, ItemClass,
    SimulationWorld, SpellKind, Tile,
};

#[test]
fn test_simulation_world_init() {
    let world = SimulationWorld::new_with_seed(100);
    let player = world.arena.actors.get(world.player_id).unwrap();
    assert_eq!(player.hp, 18);
    assert!(player.is_player);
}

#[test]
fn test_simulation_step_wait() {
    let mut world = SimulationWorld::new_with_seed(100);
    let events = world.step_player_action(ActionAst::Wait);
    assert!(events.iter().any(|e| matches!(e, GameEvent::TurnAdvanced { .. })));
    assert_eq!(world.scheduler.turn, 2);
}

#[test]
fn test_deterministic_replay() {
    let mut sim1 = SimulationWorld::new_with_seed(555);
    let mut sim2 = SimulationWorld::new_with_seed(555);

    let actions = vec![
        ActionAst::Wait,
        ActionAst::Move(Direction::East),
        ActionAst::Wait,
        ActionAst::Move(Direction::South),
    ];

    let mut events1 = Vec::new();
    let mut events2 = Vec::new();

    for act in &actions {
        events1.extend(sim1.step_player_action(act.clone()));
        events2.extend(sim2.step_player_action(act.clone()));
    }

    assert_eq!(events1, events2);
    assert_eq!(sim1.scheduler.turn, sim2.scheduler.turn);
    assert_eq!(
        sim1.arena.actors.get(sim1.player_id).unwrap().coord,
        sim2.arena.actors.get(sim2.player_id).unwrap().coord
    );
}

#[test]
fn test_movement_into_wall_logs_bump() {
    let mut sim = SimulationWorld::new_with_seed(42);
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let wall_coord = Coord::new(p_coord.x + 1, p_coord.y).unwrap();
    sim.level.set_tile(wall_coord, Tile::Wall { horizontal: true });

    let events = sim.step_player_action(ActionAst::Move(Direction::East));
    assert!(events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("bump"))));
    assert_eq!(sim.arena.actors.get(sim.player_id).unwrap().coord, p_coord);
}

#[test]
fn test_door_open_close_kick_lifecycle() {
    let mut sim = SimulationWorld::new_with_seed(123);
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let door_coord = Coord::new(p_coord.x + 1, p_coord.y).unwrap();

    // 1. Setup closed door
    sim.level.set_tile(door_coord, Tile::Door { state: DoorState::Closed, trapped: false });

    // 2. Open door action
    let events = sim.step_player_action(ActionAst::OpenDoor(door_coord));
    assert!(events.iter().any(|e| matches!(e, GameEvent::DoorToggled { coord, new_state: DoorState::Open } if *coord == door_coord)));
    assert_eq!(sim.level.get_tile(door_coord), &Tile::Door { state: DoorState::Open, trapped: false });

    // 3. Close door action
    let events_close = sim.step_player_action(ActionAst::CloseDoor(door_coord));
    assert!(events_close.iter().any(|e| matches!(e, GameEvent::DoorToggled { coord, new_state: DoorState::Closed } if *coord == door_coord)));
    assert_eq!(sim.level.get_tile(door_coord), &Tile::Door { state: DoorState::Closed, trapped: false });

    // 4. Kick door action -> Broken
    let events_kick = sim.step_player_action(ActionAst::Kick(door_coord));
    assert!(events_kick.iter().any(|e| matches!(e, GameEvent::DoorToggled { coord, new_state: DoorState::Broken } if *coord == door_coord)));
    assert_eq!(sim.level.get_tile(door_coord), &Tile::Door { state: DoorState::Broken, trapped: false });
}

#[test]
fn test_melee_attack_action() {
    let mut sim = SimulationWorld::new_with_seed(999);
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let mon_coord = Coord::new(p_coord.x + 1, p_coord.y).unwrap();

    let goblin = ActorRecord {
        name: "Orc".into(),
        coord: mon_coord,
        hp: 5,
        max_hp: 5,
        ac: 10,
        level: 1,
        speed: 10,
        alignment: Alignment::Chaotic,
        intrinsics: Intrinsics::default(),
        is_player: false,
        is_dead: false,
        is_tame: false,
    };
    let mon_id = sim.arena.spawn_actor(goblin);

    let events = sim.step_player_action(ActionAst::MeleeAttack(mon_coord));
    assert!(events.iter().any(|e| matches!(e, GameEvent::AttackLanded { target, .. } if *target == mon_id)));
    let mon_after = sim.arena.actors.get(mon_id).unwrap();
    assert!(mon_after.hp < 5 || mon_after.is_dead);
}

#[test]
fn test_simulation_world_serde_roundtrip() {
    let sim = SimulationWorld::new_with_seed(777);
    let json = serde_json::to_string(&sim).expect("Serialization failed");
    let restored: SimulationWorld = serde_json::from_str(&json).expect("Deserialization failed");

    assert_eq!(sim.player_id, restored.player_id);
    assert_eq!(sim.level.rooms.len(), restored.level.rooms.len());
    assert_eq!(sim.scheduler.turn, restored.scheduler.turn);
}

#[test]
fn test_zap_wand_beam_propagation_and_damage() {
    let mut sim = SimulationWorld::new_with_seed(101);
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;
    for dx in 1..=4 {
        let c = Coord::new(p_coord.x + dx, p_coord.y).unwrap();
        sim.level.set_tile(c, Tile::Room);
    }

    let target_coord = Coord::new(p_coord.x + 2, p_coord.y).unwrap();
    let mon = ActorRecord {
        name: "Goblin Archer".into(),
        coord: target_coord,
        hp: 10,
        max_hp: 10,
        ac: 8,
        level: 1,
        speed: 10,
        alignment: Alignment::Chaotic,
        intrinsics: Intrinsics::default(),
        is_player: false,
        is_dead: false,
        is_tame: false,
    };
    let mon_id = sim.arena.spawn_actor(mon);

    let events = sim.step_player_action(ActionAst::ZapWand {
        dir: Direction::East,
        energy: 5,
    });

    assert!(events.iter().any(|e| matches!(e, GameEvent::BeamPropagated { .. })));
    assert!(events.iter().any(|e| matches!(e, GameEvent::AttackLanded { target, lethal: true, .. } if *target == mon_id)));
    let mon_after = sim.arena.actors.get(mon_id).unwrap();
    assert!(mon_after.is_dead);
}

#[test]
fn test_pickup_and_drop_lifecycle() {
    let mut sim = SimulationWorld::new_with_seed(102);
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;

    let gem_id = sim.arena.spawn_item(ItemRecord {
        name: "emerald".into(),
        class: ItemClass::Gem,
        weight: 1,
        buc: Buc::Uncursed,
        is_container: false,
        is_bag_of_holding: false,
        enchantment: 0,
        erosion: 0,
        proofed: false,
        location: ItemLocation::Floor(p_coord),
    });

    let events_pickup = sim.step_player_action(ActionAst::PickUp);
    assert!(events_pickup.iter().any(|e| matches!(e, GameEvent::ItemPickedUp { item, .. } if *item == gem_id)));
    assert_eq!(sim.arena.items.get(gem_id).unwrap().location, ItemLocation::CarriedBy(sim.player_id));

    let carried = sim.arena.items_carried_by(sim.player_id);
    let gem_idx = carried.iter().position(|&id| id == gem_id).unwrap();
    let events_drop = sim.step_player_action(ActionAst::Drop(gem_idx));
    assert!(events_drop.iter().any(|e| matches!(e, GameEvent::ItemDropped { item, .. } if *item == gem_id)));
    assert_eq!(sim.arena.items.get(gem_id).unwrap().location, ItemLocation::Floor(p_coord));
}

#[test]
fn test_wield_weapon() {
    let mut sim = SimulationWorld::new_with_seed(103);
    let sword_id = sim.arena.spawn_item(ItemRecord {
        name: "long sword".into(),
        class: ItemClass::Weapon,
        weight: 30,
        buc: Buc::Uncursed,
        is_container: false,
        is_bag_of_holding: false,
        enchantment: 0,
        erosion: 0,
        proofed: false,
        location: ItemLocation::CarriedBy(sim.player_id),
    });

    let carried = sim.arena.items_carried_by(sim.player_id);
    let idx = carried.iter().position(|&id| id == sword_id).unwrap();
    let events = sim.step_player_action(ActionAst::Wield(idx));
    assert!(events.iter().any(|e| matches!(e, GameEvent::ItemWielded { item, .. } if *item == sword_id)));
    assert_eq!(sim.wielded_item, Some(sword_id));
}

#[test]
fn test_descend_and_ascend_stairs() {
    let mut sim = SimulationWorld::new_with_seed(104);
    let down_stairs = sim.level.stairs_down;

    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = down_stairs;
    }

    let events_down = sim.step_player_action(ActionAst::Descend);
    assert!(events_down.iter().any(|e| matches!(e, GameEvent::LevelChanged { from_depth: 1, to_depth: 2 })));
    assert_eq!(sim.depth, 2);

    let events_up = sim.step_player_action(ActionAst::Ascend);
    assert!(events_up.iter().any(|e| matches!(e, GameEvent::LevelChanged { from_depth: 2, to_depth: 1 })));
    assert_eq!(sim.depth, 1);
    assert_eq!(sim.arena.actors.get(sim.player_id).unwrap().coord, down_stairs);
}

#[test]
fn test_container_put_and_take() {
    let mut sim = SimulationWorld::new_with_seed(105);
    let sack = sim.arena.spawn_item(create_item_record(ItemKindId::Sack, ItemLocation::CarriedBy(sim.player_id), Buc::Uncursed));
    let dagger = sim.arena.spawn_item(create_item_record(ItemKindId::Dagger, ItemLocation::CarriedBy(sim.player_id), Buc::Uncursed));

    let carried = sim.arena.items_carried_by(sim.player_id);
    let sack_idx = carried.iter().position(|&id| id == sack).unwrap();
    let dagger_idx = carried.iter().position(|&id| id == dagger).unwrap();

    let events = sim.step_player_action(ActionAst::PutInContainer {
        item_index: dagger_idx,
        container_index: sack_idx,
    });
    assert!(events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("put the dagger into the sack"))));
    assert_eq!(sim.arena.items.get(dagger).unwrap().location, ItemLocation::InContainer(sack));

    let carried_after = sim.arena.items_carried_by(sim.player_id);
    let sack_idx_after = carried_after.iter().position(|&id| id == sack).unwrap();
    let events_take = sim.step_player_action(ActionAst::TakeFromContainer {
        container_index: sack_idx_after,
        item_index: 0,
    });
    assert!(events_take.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("take the dagger out of the sack"))));
    assert_eq!(sim.arena.items.get(dagger).unwrap().location, ItemLocation::CarriedBy(sim.player_id));
}

#[test]
fn test_container_boh_in_boh_explosion() {
    let mut sim = SimulationWorld::new_with_seed(106);
    let boh1 = sim.arena.spawn_item(create_item_record(ItemKindId::BagOfHolding, ItemLocation::CarriedBy(sim.player_id), Buc::Uncursed));
    let boh2 = sim.arena.spawn_item(create_item_record(ItemKindId::BagOfHolding, ItemLocation::CarriedBy(sim.player_id), Buc::Uncursed));

    let carried = sim.arena.items_carried_by(sim.player_id);
    let idx1 = carried.iter().position(|&id| id == boh1).unwrap();
    let idx2 = carried.iter().position(|&id| id == boh2).unwrap();

    let initial_hp = sim.arena.actors.get(sim.player_id).unwrap().hp;
    let events = sim.step_player_action(ActionAst::PutInContainer {
        item_index: idx1,
        container_index: idx2,
    });

    assert!(events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("explodes"))));
    assert!(sim.arena.items.get(boh1).is_none());
    assert!(sim.arena.items.get(boh2).is_none());
    assert!(sim.arena.actors.get(sim.player_id).unwrap().hp < initial_hp);
}

#[test]
fn test_water_dipping() {
    use netrust_types::WaterType;
    let mut sim = SimulationWorld::new_with_seed(107);
    let weapon = sim.arena.spawn_item(create_item_record(ItemKindId::LongSword, ItemLocation::CarriedBy(sim.player_id), Buc::Cursed));

    let carried = sim.arena.items_carried_by(sim.player_id);
    let idx = carried.iter().position(|&id| id == weapon).unwrap();

    let events = sim.step_player_action(ActionAst::Dip {
        item_index: idx,
        into_water: WaterType::Holy,
    });
    assert!(events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("amber aura (blessed)"))));
    assert_eq!(sim.arena.items.get(weapon).unwrap().buc, Buc::Blessed);
}

#[test]
fn test_quaff_healing() {
    let mut sim = SimulationWorld::new_with_seed(108);
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.hp = 5;
    }

    let potion = sim.arena.spawn_item(create_item_record(ItemKindId::PotionOfHealing, ItemLocation::CarriedBy(sim.player_id), Buc::Uncursed));
    let carried = sim.arena.items_carried_by(sim.player_id);
    let idx = carried.iter().position(|&id| id == potion).unwrap();

    let events = sim.step_player_action(ActionAst::Quaff(idx));
    assert!(events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("much better"))));
    assert!(sim.arena.actors.get(sim.player_id).unwrap().hp > 5);
    assert!(sim.arena.items.get(potion).is_none());
}

#[test]
fn test_monster_dijkstra_hunting() {
    let mut sim = SimulationWorld::new_with_seed(109);
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;

    let mut m_coord = None;
    for dx in [2, -2, 3, -3] {
        let nx = p_coord.x as isize + dx;
        if nx >= 0 {
            if let Some(c) = Coord::new(nx as usize, p_coord.y) {
                if sim.level.is_passable(c) && sim.actor_at(c).is_none() {
                    m_coord = Some(c);
                    break;
                }
            }
        }
    }
    let m_coord = m_coord.unwrap_or(sim.level.rooms[0].center());
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let goblin_id = sim.arena.spawn_actor(create_monster_record(MonsterSpeciesId::Goblin, m_coord));

    let init_dist = sim.arena.actors.get(goblin_id).unwrap().coord.chebyshev_distance(p_coord);
    if init_dist > 1 {
        sim.step_player_action(ActionAst::Wait);

        let new_dist = sim.arena.actors.get(goblin_id).unwrap().coord.chebyshev_distance(p_coord);
        assert!(new_dist <= init_dist, "Monster should advance towards player along Dijkstra gradient: init {}, new {}", init_dist, new_dist);
    }
}

#[test]
fn test_floor_engraving_and_smudge() {
    let mut sim = SimulationWorld::new_with_seed(110);
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;

    let events = sim.step_player_action(ActionAst::Engrave {
        text: "Elbereth".into(),
        medium: EngravingMedium::Dust(1),
    });
    assert!(events.iter().any(|e| matches!(e, GameEvent::EngravingAdded { text, .. } if text == "Elbereth")));
    assert!(sim.level.get_engraving(p_coord).is_some());

    let mut target_dir = None;
    for d in Direction::all_compass() {
        if let Some(nc) = p_coord.step(d) {
            if sim.level.is_passable(nc) && sim.actor_at(nc).is_none() {
                target_dir = Some(d);
                break;
            }
        }
    }
    let dir = target_dir.expect("Must find passable direction to step");
    sim.step_player_action(ActionAst::Move(dir));

    let after_e = sim.level.get_engraving(p_coord).unwrap();
    assert_eq!(after_e.medium, EngravingMedium::Dust(0));
}

#[test]
fn test_elbereth_repels_monsters() {
    let mut sim = SimulationWorld::new_with_seed(111);
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;

    sim.step_player_action(ActionAst::Engrave {
        text: "Elbereth".into(),
        medium: EngravingMedium::Burned,
    });

    let mut m_coord = None;
    for d in Direction::all_compass() {
        if let Some(c) = p_coord.step(d) {
            if sim.level.is_passable(c) && sim.actor_at(c).is_none() {
                m_coord = Some(c);
                break;
            }
        }
    }
    let adj_coord = m_coord.expect("Must find adjacent tile");
    let goblin_id = sim.arena.spawn_actor(create_monster_record(MonsterSpeciesId::Goblin, adj_coord));

    let initial_player_hp = sim.arena.actors.get(sim.player_id).unwrap().hp;
    let events = sim.step_player_action(ActionAst::Wait);

    assert_eq!(sim.arena.actors.get(sim.player_id).unwrap().hp, initial_player_hp);
    assert!(events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("repelled by the sacred ward of Elbereth"))));
    let goblin_coord_after = sim.arena.actors.get(goblin_id).unwrap().coord;
    assert!(goblin_coord_after.chebyshev_distance(p_coord) >= 1);
}

#[test]
fn test_shop_and_shopkeeper_mechanics() {
    let mut sim = SimulationWorld::new_with_seed(42);
    assert!(!sim.unpaid_items.is_empty(), "World should have shop with unpaid items");

    let sk_opt = sim.arena.actors.iter().find(|(_, a)| a.name == "shopkeeper");
    assert!(sk_opt.is_some());
    let (sk_id, sk) = sk_opt.unwrap();
    assert_eq!(sk.alignment, Alignment::Neutral);

    let (unpaid_id, cost) = sim.unpaid_items[0];
    let item_coord = match sim.arena.items.get(unpaid_id).unwrap().location {
        ItemLocation::Floor(c) => c,
        _ => panic!("Item should be on floor"),
    };

    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = item_coord;
    }
    let pickup_events = sim.step_player_action(ActionAst::PickUp);
    assert!(pickup_events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("That will be"))));
    assert!(sim.is_unpaid(unpaid_id));

    sim.player_gold = 500;
    let initial_gold = sim.player_gold;
    let pay_events = sim.step_player_action(ActionAst::Pay);
    assert!(pay_events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("You pay the shopkeeper"))));
    assert!(!sim.is_unpaid(unpaid_id));
    assert_eq!(sim.player_gold, initial_gold - cost);

    if !sim.unpaid_items.is_empty() {
        let (item2_id, _) = sim.unpaid_items[0];
        let item2_coord = match sim.arena.items.get(item2_id).unwrap().location {
            ItemLocation::Floor(c) => c,
            _ => panic!("Second item should be on floor"),
        };
        if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
            p.coord = item2_coord;
        }
        sim.step_player_action(ActionAst::PickUp);
        assert!(sim.is_unpaid(item2_id));

        let shop_room = *sim.level.rooms.iter().find(|r| r.room_type == RoomType::Shop).unwrap();
        let shop_edge = Coord::new(shop_room.x1 + 1, shop_room.y1 + 1).unwrap();
        let outside_step = Coord::new(shop_room.x1, shop_room.y1 + 1).unwrap();
        sim.level.set_tile(outside_step, Tile::Corr);
        if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
            p.coord = shop_edge;
        }
        let move_events = sim.step_player_action(ActionAst::Move(Direction::West));
        assert!(move_events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("Stop, thief!"))));
        let sk_after = sim.arena.actors.get(sk_id).unwrap();
        assert_eq!(sk_after.alignment, Alignment::Chaotic);
    }
}

#[test]
fn test_altar_prayer_and_sacrifice() {
    let mut sim = SimulationWorld::new_with_seed(42);
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;

    sim.level.set_tile(p_coord, Tile::Altar { align: Alignment::Neutral });

    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.hp = 5;
        p.alignment = Alignment::Neutral;
    }

    let pray_events = sim.step_player_action(ActionAst::Pray);
    assert!(pray_events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("fully healed"))));
    assert_eq!(sim.arena.actors.get(sim.player_id).unwrap().hp, sim.arena.actors.get(sim.player_id).unwrap().max_hp);

    sim.level.set_tile(p_coord, Tile::Altar { align: Alignment::Chaotic });
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.alignment = Alignment::Lawful;
    }

    let carried = sim.arena.items_carried_by(sim.player_id);
    assert!(!carried.is_empty());
    let sac_events = sim.step_player_action(ActionAst::Sacrifice(0));
    assert!(sac_events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("converts to Lawful"))));
    assert_eq!(sim.level.get_tile(p_coord), &Tile::Altar { align: Alignment::Lawful });
}

#[test]
fn test_multi_floor_persistence_with_items_and_amulet() {
    let mut sim = SimulationWorld::new_with_seed(777);
    let stairs_down_l1 = sim.level.stairs_down;

    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = stairs_down_l1;
    }

    sim.step_player_action(ActionAst::Descend);
    assert_eq!(sim.depth, 2);

    let ruby = sim.arena.spawn_item(create_item_record(ItemKindId::ShortSword, ItemLocation::CarriedBy(sim.player_id), Buc::Blessed));
    let carried = sim.arena.items_carried_by(sim.player_id);
    let ruby_idx = carried.iter().position(|&id| id == ruby).unwrap();
    sim.step_player_action(ActionAst::Drop(ruby_idx));

    sim.step_player_action(ActionAst::Ascend);
    assert_eq!(sim.depth, 1);
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;
    assert!(!sim.arena.items_at_floor(p_coord).contains(&ruby));

    sim.step_player_action(ActionAst::Descend);
    assert_eq!(sim.depth, 2);
    let l2_p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let floor_items = sim.arena.items_at_floor(l2_p_coord);
    assert!(!floor_items.is_empty());
    assert_eq!(sim.arena.items.get(floor_items[0]).unwrap().name, "short sword");

    for d in 3..=5 {
        let stairs_down = sim.level.stairs_down;
        if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
            p.coord = stairs_down;
        }
        sim.step_player_action(ActionAst::Descend);
        assert_eq!(sim.depth, d);
    }
    assert_eq!(sim.depth, 5);
    let has_amulet = sim.arena.items.values().any(|it| it.name == "Amulet of Yendor");
    assert!(has_amulet);
}

#[test]
fn test_nutrition_decay_and_eating() {
    let mut sim = SimulationWorld::new_with_seed(888);
    assert_eq!(sim.player_nutrition, 900);

    for _ in 0..10 {
        sim.step_player_action(ActionAst::Wait);
    }
    assert!(sim.player_nutrition < 900);

    let carried = sim.arena.items_carried_by(sim.player_id);
    let ration_idx = carried.iter().position(|&id| {
        sim.arena.items.get(id).map(|it| it.name.contains("ration")).unwrap_or(false)
    }).unwrap();

    let nut_before = sim.player_nutrition;
    let eat_events = sim.step_player_action(ActionAst::Eat(ration_idx));
    assert!(eat_events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("eat the food ration"))));
    assert!(sim.player_nutrition > nut_before);

    let corpse = sim.arena.spawn_item(ItemRecord {
        name: "giant ant corpse".into(),
        class: ItemClass::Food,
        weight: 50,
        buc: Buc::Uncursed,
        is_container: false,
        is_bag_of_holding: false,
        enchantment: 0,
        erosion: 0,
        proofed: false,
        location: ItemLocation::CarriedBy(sim.player_id),
    });
    let carried2 = sim.arena.items_carried_by(sim.player_id);
    let corpse_idx = carried2.iter().position(|&id| id == corpse).unwrap();
    let eat_corpse_events = sim.step_player_action(ActionAst::Eat(corpse_idx));
    assert!(eat_corpse_events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("Poison Resistance"))));
    assert!(sim.arena.actors.get(sim.player_id).unwrap().intrinsics.poison_resistance);
}

#[test]
fn test_magic_spellcasting_and_mana() {
    let config = CharacterConfig {
        name: "Merlin".into(),
        role: RoleId::Wizard,
        race: RaceId::Elf,
        gender: Gender::Male,
        alignment: Alignment::Chaotic,
    };
    let mut sim = SimulationWorld::new_with_character(999, config);
    assert_eq!(sim.player_pw, 25);
    assert_eq!(sim.known_spells[0].0, SpellKind::ForceBolt);

    let cast_events = sim.step_player_action(ActionAst::Cast {
        spell_index: 0,
        dir: Direction::East,
    });
    assert!(cast_events.iter().any(|e| matches!(e, GameEvent::BeamPropagated { .. })));
    assert_eq!(sim.player_pw, 20);

    let spellbook = sim.arena.spawn_item(create_item_record(
        ItemKindId::SpellbookOfHealing,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Uncursed,
    ));
    let carried = sim.arena.items_carried_by(sim.player_id);
    let book_idx = carried.iter().position(|&id| id == spellbook).unwrap();
    let read_events = sim.step_player_action(ActionAst::Read(book_idx));
    assert!(read_events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("memorize the spell"))));
    assert!(sim.known_spells.iter().any(|(s, _)| *s == SpellKind::CureLightWounds));

    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.hp = 5;
    }
    let heal_idx = sim.known_spells.iter().position(|(s, _)| *s == SpellKind::CureLightWounds).unwrap();
    let heal_events = sim.step_player_action(ActionAst::Cast {
        spell_index: heal_idx,
        dir: Direction::None,
    });
    assert!(heal_events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("wounds close"))));
    assert_eq!(sim.arena.actors.get(sim.player_id).unwrap().hp, 12);
    assert_eq!(sim.player_pw, 15);
}

#[test]
fn test_boulder_push_floor() {
    let mut sim = SimulationWorld::new_with_seed(42);
    let p_coord = Coord::new_unchecked(10, 10);
    let b_coord = Coord::new_unchecked(11, 10);
    let dest_coord = Coord::new_unchecked(12, 10);

    sim.level.set_tile(p_coord, Tile::Room);
    sim.level.set_tile(b_coord, Tile::Room);
    sim.level.set_tile(dest_coord, Tile::Room);

    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = p_coord;
    }

    let boulder_id = sim.arena.spawn_item(create_item_record(
        ItemKindId::Boulder,
        ItemLocation::Floor(b_coord),
        Buc::Uncursed,
    ));

    let events = sim.step_player_action(ActionAst::Move(Direction::East));
    assert!(events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("You push the boulder"))));
    assert_eq!(sim.arena.items.get(boulder_id).unwrap().location, ItemLocation::Floor(dest_coord));
}

#[test]
fn test_boulder_push_pit_fill() {
    let mut sim = SimulationWorld::new_with_seed(42);
    let p_coord = Coord::new_unchecked(10, 10);
    let b_coord = Coord::new_unchecked(11, 10);
    let pit_coord = Coord::new_unchecked(12, 10);

    sim.level.set_tile(p_coord, Tile::Room);
    sim.level.set_tile(b_coord, Tile::Room);
    sim.level.set_tile(pit_coord, Tile::Pit { filled: false });

    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = p_coord;
    }

    let boulder_id = sim.arena.spawn_item(create_item_record(
        ItemKindId::Boulder,
        ItemLocation::Floor(b_coord),
        Buc::Uncursed,
    ));

    let events = sim.step_player_action(ActionAst::Move(Direction::East));
    assert!(events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("fills it"))));
    assert_eq!(sim.level.get_tile(pit_coord), &Tile::Pit { filled: true });
    assert!(sim.arena.items.get(boulder_id).is_none(), "Boulder must be consumed by filling pit");
}

#[test]
fn test_boulder_push_blocked_by_wall() {
    let mut sim = SimulationWorld::new_with_seed(42);
    let p_coord = Coord::new_unchecked(10, 10);
    let b_coord = Coord::new_unchecked(11, 10);
    let wall_coord = Coord::new_unchecked(12, 10);

    sim.level.set_tile(p_coord, Tile::Room);
    sim.level.set_tile(b_coord, Tile::Room);
    sim.level.set_tile(wall_coord, Tile::Wall { horizontal: false });

    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = p_coord;
    }

    let boulder_id = sim.arena.spawn_item(create_item_record(
        ItemKindId::Boulder,
        ItemLocation::Floor(b_coord),
        Buc::Uncursed,
    ));

    let events = sim.step_player_action(ActionAst::Move(Direction::East));
    assert!(events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("won't budge"))));
    assert_eq!(sim.arena.items.get(boulder_id).unwrap().location, ItemLocation::Floor(b_coord));
}

#[test]
fn test_branch_transition_sokoban() {
    use netrust_types::BranchId;
    let mut sim = SimulationWorld::new_with_seed(42);

    // Place branch stairs to Sokoban
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;
    sim.level.set_tile(p_coord, Tile::BranchStairs {
        branch: BranchId::Sokoban,
        level: 1,
        up: false,
    });

    let descend_events = sim.step_player_action(ActionAst::Descend);
    assert!(descend_events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("Sokoban"))));
    assert_eq!(sim.current_branch, BranchId::Sokoban);
    assert_eq!(sim.depth, 1);

    // Check that Sokoban prize (Bag of Holding) and boulders exist
    let has_boh = sim.arena.items.values().any(|it| it.name.contains("bag of holding"));
    assert!(has_boh, "Sokoban prize chamber must spawn Bag of Holding");

    let num_boulders = sim.arena.items.values().filter(|it| it.name == "boulder").count();
    assert!(num_boulders >= 4, "Sokoban level must spawn puzzle boulders");

    // Ascend back to Dungeons of Doom
    let soko_p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;
    assert_eq!(sim.level.get_tile(soko_p_coord), &Tile::BranchStairs {
        branch: BranchId::DungeonsOfDoom,
        level: 4,
        up: true,
    });

    let ascend_events = sim.step_player_action(ActionAst::Ascend);
    assert!(ascend_events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("return to DungeonsOfDoom"))));
    assert_eq!(sim.current_branch, BranchId::DungeonsOfDoom);
    assert_eq!(sim.depth, 4);
}

#[test]
fn test_companion_pet_displacement() {
    let mut sim = SimulationWorld::new_with_seed(42);
    let p_coord = Coord::new_unchecked(10, 10);
    let pet_coord = Coord::new_unchecked(11, 10);

    sim.level.set_tile(p_coord, Tile::Room);
    sim.level.set_tile(pet_coord, Tile::Room);

    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = p_coord;
    }

    let pet_id = sim.arena.spawn_actor(create_monster_record(
        MonsterSpeciesId::LittleDog,
        pet_coord,
    ));
    assert!(sim.arena.actors.get(pet_id).unwrap().is_tame);

    let initial_player_hp = sim.arena.actors.get(sim.player_id).unwrap().hp;
    let initial_pet_hp = sim.arena.actors.get(pet_id).unwrap().hp;

    let events = sim.step_player_action(ActionAst::Move(Direction::East));

    // Must be displaced, not attacked
    assert!(events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("You displace little dog"))));
    assert_eq!(sim.arena.actors.get(sim.player_id).unwrap().hp, initial_player_hp, "Displacement must be non-violent");
    assert_eq!(sim.arena.actors.get(pet_id).unwrap().hp, initial_pet_hp, "Displacement must be non-violent");

    // Coordinates swapped!
    assert_eq!(sim.arena.actors.get(sim.player_id).unwrap().coord, pet_coord);
    assert_eq!(sim.arena.actors.get(pet_id).unwrap().coord, p_coord);
}

#[test]
fn test_companion_pet_attacks_hostile() {
    let mut sim = SimulationWorld::new_with_seed(42);
    let p_coord = Coord::new_unchecked(10, 10);
    let pet_coord = Coord::new_unchecked(11, 10);
    let enemy_coord = Coord::new_unchecked(12, 10);

    sim.level.set_tile(p_coord, Tile::Room);
    sim.level.set_tile(pet_coord, Tile::Room);
    sim.level.set_tile(enemy_coord, Tile::Room);

    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = p_coord;
    }

    let _pet_id = sim.arena.spawn_actor(create_monster_record(
        MonsterSpeciesId::LittleDog,
        pet_coord,
    ));
    let goblin_id = sim.arena.spawn_actor(create_monster_record(
        MonsterSpeciesId::Goblin,
        enemy_coord,
    ));

    let goblin_initial_hp = sim.arena.actors.get(goblin_id).unwrap().hp;

    // Player waits, allowing monster turn to tick
    let _events = sim.step_player_action(ActionAst::Wait);

    // Goblin should have taken combat damage from little dog
    let goblin_after = sim.arena.actors.get(goblin_id).unwrap();
    assert!(goblin_after.hp < goblin_initial_hp || goblin_after.is_dead, "Pet must attack adjacent hostile monster");
    // Player was not attacked by pet
    let p_after = sim.arena.actors.get(sim.player_id).unwrap();
    assert_eq!(p_after.hp, p_after.max_hp);
}

#[test]
fn test_scroll_of_enchant_weapon() {
    let mut sim = SimulationWorld::new_with_seed(42);

    let sword = sim.arena.spawn_item(create_item_record(
        ItemKindId::LongSword,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Uncursed,
    ));
    sim.wielded_item = Some(sword);
    assert_eq!(sim.arena.items.get(sword).unwrap().enchantment, 0);

    let scroll = sim.arena.spawn_item(create_item_record(
        ItemKindId::ScrollOfEnchantWeapon,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Blessed,
    ));

    let carried = sim.arena.items_carried_by(sim.player_id);
    let scroll_idx = carried.iter().position(|&id| id == scroll).unwrap();

    let events = sim.step_player_action(ActionAst::Read(scroll_idx));
    assert!(events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("silvery aura"))));
    assert_eq!(sim.arena.items.get(sword).unwrap().enchantment, 2);
}

#[test]
fn test_scroll_of_enchant_armor() {
    let mut sim = SimulationWorld::new_with_seed(42);

    let scroll = sim.arena.spawn_item(create_item_record(
        ItemKindId::ScrollOfEnchantArmor,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Uncursed,
    ));

    let carried = sim.arena.items_carried_by(sim.player_id);
    let scroll_idx = carried.iter().position(|&id| id == scroll).unwrap();

    let events = sim.step_player_action(ActionAst::Read(scroll_idx));
    assert!(events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("protective silver sheen"))));
    let enchanted_armor = sim.arena.items_carried_by(sim.player_id).into_iter()
        .filter_map(|id| sim.arena.items.get(id))
        .find(|it| it.class == ItemClass::Armor)
        .unwrap();
    assert_eq!(enchanted_armor.enchantment, 1);
}

#[test]
fn test_alchemy_potion_mixing() {
    let mut sim = SimulationWorld::new_with_seed(42);

    let pot_heal = sim.arena.spawn_item(create_item_record(
        ItemKindId::PotionOfHealing,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Uncursed,
    ));
    let pot_speed = sim.arena.spawn_item(create_item_record(
        ItemKindId::PotionOfSpeed,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Uncursed,
    ));

    let carried = sim.arena.items_carried_by(sim.player_id);
    let heal_idx = carried.iter().position(|&id| id == pot_heal).unwrap();
    let speed_idx = carried.iter().position(|&id| id == pot_speed).unwrap();

    let events = sim.handle_dip_potion(speed_idx, heal_idx);
    assert!(events.iter().any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("potion of extra healing"))));
    assert_eq!(sim.arena.items.get(pot_heal).unwrap().name, "potion of extra healing");
    assert!(sim.arena.items.get(pot_speed).is_none(), "Reagent potion must be consumed");
}



