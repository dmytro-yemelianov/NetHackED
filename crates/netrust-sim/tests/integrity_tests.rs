//! Simulation integrity regressions: damage, genocide, wands/wishes, melee variance, timers.

use netrust_sim::{ActionAst, Coord, Direction, SimulationWorld, Tile};
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
