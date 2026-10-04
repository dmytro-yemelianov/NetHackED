//! Simulation integrity regressions: damage, genocide, wands/wishes, melee variance, timers.

use netrust_arena::ItemLocation;
use netrust_data::{create_item_record, create_monster_record, ItemKindId, MonsterSpeciesId};
use netrust_sim::{ActionAst, Buc, Coord, Direction, SimulationWorld, Tile};
use netrust_types::{TrapRecord, TrapState, TrapType};

fn open_east(sim: &mut SimulationWorld) -> Coord {
    let p = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let east = Coord::new(p.x + 1, p.y).unwrap();
    sim.level.set_tile(east, Tile::Room);
    if let Some(id) = sim.actor_at(east) {
        sim.arena.actors.remove(id);
    }
    east
}

#[test]
fn arrow_trap_at_one_hp_kills_without_wrapping() {
    let mut sim = SimulationWorld::new_with_seed(5);
    let east = open_east(&mut sim);
    sim.level.traps.insert(east, TrapRecord { id: 1, trap_type: TrapType::Arrow, state: TrapState::Hidden, coord: east });
    sim.arena.actors.get_mut(sim.player_id).unwrap().hp = 1;
    sim.step_player_action(ActionAst::Move(Direction::East));
    let p = sim.arena.actors.get(sim.player_id).unwrap();
    assert_eq!(p.hp, 0);
    assert!(p.is_dead);
}

#[test]
fn damage_player_saturates_and_marks_death() {
    let mut sim = SimulationWorld::new_with_seed(5);
    sim.arena.actors.get_mut(sim.player_id).unwrap().hp = 3;
    let ev = sim.damage_player(2, "test");
    assert!(ev.is_empty());
    assert!(!sim.arena.actors.get(sim.player_id).unwrap().is_dead);
    assert_eq!(sim.arena.actors.get(sim.player_id).unwrap().hp, 1);
    let ev = sim.damage_player(50, "an arrow trap");
    let p = sim.arena.actors.get(sim.player_id).unwrap();
    assert_eq!(p.hp, 0);
    assert!(p.is_dead);
    assert!(!ev.is_empty());
}

fn genocide_scroll(sim: &mut SimulationWorld, buc: Buc) -> usize {
    let mut scroll = create_item_record(ItemKindId::ScrollOfIdentify, ItemLocation::CarriedBy(sim.player_id), buc);
    scroll.name = "scroll of genocide".into();
    let id = sim.arena.spawn_item(scroll);
    sim.arena.items_carried_by(sim.player_id).iter().position(|&i| i == id).unwrap()
}

#[test]
fn blessed_genocide_spares_non_lich_actors_and_drops_items() {
    let mut sim = SimulationWorld::new_with_seed(77);
    let p = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let c1 = Coord::new(p.x + 2, p.y).unwrap();
    let c2 = Coord::new(p.x + 3, p.y).unwrap();
    let lich = sim.arena.spawn_actor(create_monster_record(MonsterSpeciesId::Lich, c1));
    let dog = sim.arena.spawn_actor(create_monster_record(MonsterSpeciesId::LittleDog, c2));
    let loot = sim.arena.spawn_item(create_item_record(ItemKindId::LongSword, ItemLocation::CarriedBy(lich), Buc::Uncursed));
    let before = sim.arena.actors.len();

    let idx = genocide_scroll(&mut sim, Buc::Blessed);
    sim.step_player_action(ActionAst::Read(idx));

    assert!(!sim.arena.actors.contains_key(lich));
    assert!(sim.arena.actors.contains_key(dog));
    assert_eq!(sim.arena.actors.len(), before - 1);
    assert_eq!(sim.arena.items.get(loot).unwrap().location, ItemLocation::Floor(c1));
}

#[test]
fn uncursed_genocide_hits_bestiary_goblins() {
    let mut sim = SimulationWorld::new_with_seed(78);
    let p = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let gob = sim.arena.spawn_actor(create_monster_record(MonsterSpeciesId::Goblin, Coord::new(p.x + 2, p.y).unwrap()));
    let idx = genocide_scroll(&mut sim, Buc::Uncursed);
    sim.step_player_action(ActionAst::Read(idx));
    assert!(!sim.arena.actors.contains_key(gob));
    assert!(netrust_core::genocide::is_genocided(&sim.genocide_registry, "goblin", 'o'));
}

#[test]
fn monster_class_lookup() {
    assert_eq!(netrust_data::monster_class_of("master lich"), Some('L'));
    assert_eq!(netrust_data::monster_class_of("GOBLIN"), Some('o'));
    assert_eq!(netrust_data::monster_class_of("no such thing"), None);
}
