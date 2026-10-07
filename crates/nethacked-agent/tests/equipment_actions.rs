use nethacked_agent::commands::{parse_action, ActionArgs};
use nethacked_arena::ItemLocation;
use nethacked_sim::{ActionAst, CharacterConfig, GameEvent, ItemKindId, RoleId, SimulationWorld};
use nethacked_types::Buc;

#[test]
fn public_parser_starts_and_completes_suit_dressing() {
    let mut sim = SimulationWorld::new_with_character(
        42,
        CharacterConfig {
            role: RoleId::Valkyrie,
            ..Default::default()
        },
    );
    let monsters: Vec<_> = sim
        .arena
        .actors
        .iter()
        .filter(|(id, _)| *id != sim.player_id)
        .map(|(id, _)| id)
        .collect();
    for id in monsters {
        sim.arena.actors.remove(id);
    }
    let suit = sim.arena.spawn_item(
        sim.ruleset
            .create_item_record_by_id(
                ItemKindId::LEATHER_ARMOR,
                ItemLocation::CarriedBy(sim.player_id),
                Buc::Uncursed,
            )
            .unwrap(),
    );
    let index = sim
        .arena
        .items_carried_by(sim.player_id)
        .iter()
        .position(|id| *id == suit)
        .unwrap();
    let args = ActionArgs {
        index: Some(index),
        ..Default::default()
    };
    assert_eq!(parse_action("wear", &args).unwrap(), ActionAst::Wear(index));
    assert_eq!(
        parse_action("remove", &args).unwrap(),
        ActionAst::Remove(index)
    );
    assert_eq!(
        parse_action("takeoff", &args).unwrap(),
        ActionAst::Remove(index)
    );
    assert!(parse_action("cancel", &args).is_err());
    let start = sim.step_player_action(parse_action("wear", &args).unwrap());
    assert!(start.contains(&GameEvent::DressingStarted {
        item: suit,
        putting_on: true
    }));
    assert_eq!(sim.compute_hero_ac(), 8);
    sim.step_player_action(parse_action("wait", &Default::default()).unwrap());
    let finish = sim.step_player_action(parse_action("wait", &Default::default()).unwrap());
    assert!(finish.contains(&GameEvent::DressingFinished {
        item: suit,
        putting_on: true
    }));
    sim.step_player_action(parse_action("remove", &args).unwrap());
    assert_eq!(sim.compute_hero_ac(), 8);
    sim.step_player_action(parse_action("wait", &Default::default()).unwrap());
    let finish = sim.step_player_action(parse_action("wait", &Default::default()).unwrap());
    assert!(finish.contains(&GameEvent::DressingFinished {
        item: suit,
        putting_on: false
    }));
    assert_eq!(sim.compute_hero_ac(), 10);
}
