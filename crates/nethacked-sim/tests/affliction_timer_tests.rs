use nethacked_sim::{ActionAst, GameEvent, SimulationWorld};
use nethacked_types::{PetrificationState, SlimingState, TransientAfflictions};

const RECOVERIES: [&str; 4] = [
    "You feel a bit steadier now.",
    "You feel less confused now.",
    "You can see again.",
    "You wake up.",
];

fn isolated_world() -> SimulationWorld {
    let mut world = SimulationWorld::new_with_seed(100);
    let player_id = world.player_id;
    world.arena.actors.retain(|id, _| id == player_id);
    world
}

fn timers(duration: u32) -> TransientAfflictions {
    TransientAfflictions {
        stunned: duration,
        confused: duration,
        hallucinating: duration,
        blinded: duration,
        helpless: duration,
    }
}

fn message_count(events: &[GameEvent], message: &str) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, GameEvent::LogMessage { text } if text == message))
        .count()
}

#[test]
fn transient_timers_decrement_once_and_recover_once_per_world_turn() {
    for duration in [1, 2, 4] {
        let mut world = isolated_world();
        world.hero.afflictions.transient = timers(duration);
        world.arena.actors[world.player_id].intrinsics.blind = true;
        let mut all_events = Vec::new();
        for elapsed in 1..=duration + 1 {
            let before = world.scheduler.turn;
            let events = world.step_player_action(ActionAst::Wait);
            assert_eq!(world.scheduler.turn, before + 1);
            let remaining = duration.saturating_sub(elapsed);
            assert_eq!(world.hero.afflictions.transient, timers(remaining));
            assert_eq!(
                world.arena.actors[world.player_id].intrinsics.blind,
                remaining > 0
            );
            for message in RECOVERIES {
                assert_eq!(
                    message_count(&events, message),
                    usize::from(elapsed == duration),
                    "duration {duration}, elapsed {elapsed}, message {message}"
                );
            }
            all_events.extend(events);
        }
        for message in RECOVERIES {
            assert_eq!(message_count(&all_events, message), 1);
        }
    }
}

#[test]
fn multiple_hero_actions_in_one_world_turn_do_not_tick_timers() {
    let mut world = isolated_world();
    world.scheduler.hero_speed = 24;
    world.scheduler.hero_energy = 24;
    world.hero.afflictions.transient = timers(2);
    for action in 1..=6 {
        let before = world.scheduler.turn;
        let events = world.step_player_action(ActionAst::Wait);
        let advances = u64::from(action % 2 == 0);
        assert_eq!(world.scheduler.turn, before + advances);
        assert_eq!(
            world.hero.afflictions.transient,
            timers(2u32.saturating_sub(action / 2))
        );
        for message in RECOVERIES {
            assert_eq!(message_count(&events, message), usize::from(action == 4));
        }
    }
}

#[test]
fn lethal_afflictions_keep_their_countdown_and_skip_transient_recovery() {
    for petrifying in [true, false] {
        for duration in [1u8, 2, 4] {
            let mut world = isolated_world();
            if petrifying {
                world.hero.afflictions.petrification = Some(PetrificationState {
                    turns_remaining: duration,
                });
            } else {
                world.hero.afflictions.sliming = Some(SlimingState {
                    turns_remaining: duration,
                });
            }
            world.hero.afflictions.transient = timers(u32::from(duration));
            world.arena.actors[world.player_id].intrinsics.blind = true;
            for elapsed in 1..=duration {
                let before = world.scheduler.turn;
                let events = world.step_player_action(ActionAst::Wait);
                assert_eq!(world.scheduler.turn, before + 1);
                assert_eq!(
                    world.arena.actors[world.player_id].is_dead,
                    elapsed == duration
                );
                let remaining = if petrifying {
                    world
                        .hero
                        .afflictions
                        .petrification
                        .as_ref()
                        .unwrap()
                        .turns_remaining
                } else {
                    world
                        .hero
                        .afflictions
                        .sliming
                        .as_ref()
                        .unwrap()
                        .turns_remaining
                };
                assert_eq!(remaining, duration - elapsed);
                assert_eq!(
                    world.hero.afflictions.transient,
                    timers(u32::from(duration - elapsed).max(1))
                );
                for message in RECOVERIES {
                    assert_eq!(message_count(&events, message), 0);
                }
            }
            assert!(world.arena.actors[world.player_id].intrinsics.blind);
            let turn = world.scheduler.turn;
            assert!(world.step_player_action(ActionAst::Wait).is_empty());
            assert_eq!(world.scheduler.turn, turn);
        }
    }
}

#[test]
fn petrification_keeps_precedence_over_sliming() {
    let mut world = isolated_world();
    world.hero.afflictions.petrification = Some(PetrificationState { turns_remaining: 1 });
    world.hero.afflictions.sliming = Some(SlimingState { turns_remaining: 1 });
    world.step_player_action(ActionAst::Wait);
    assert!(world.arena.actors[world.player_id].is_dead);
    assert_eq!(
        world
            .hero
            .afflictions
            .petrification
            .as_ref()
            .unwrap()
            .turns_remaining,
        0
    );
    assert_eq!(
        world
            .hero
            .afflictions
            .sliming
            .as_ref()
            .unwrap()
            .turns_remaining,
        1
    );
}
