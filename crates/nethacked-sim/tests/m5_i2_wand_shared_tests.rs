//! Compatibility witnesses for the extracted domain seams.
use nethacked_arena::{ActorId, ItemId, ItemLocation};
use nethacked_core::ActionAst;
use nethacked_data::{ItemKindId, MonsterSpeciesId};
use nethacked_sim::{
    object_timers::{ObjectTimerError, ObjectTimerKind, ObjectTimerTarget},
    GameEvent, SimulationWorld,
};
use nethacked_types::{Buc, Coord, Direction, Locale, PolymorphForm, Tile};
use rand::{Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;

const ORIGIN: Coord = Coord::new_unchecked(20, 10);
const TARGET: Coord = Coord::new_unchecked(21, 10);
fn fixture(wand: &str, target: Option<&str>) -> (SimulationWorld, ItemId, Option<ActorId>) {
    let mut world = SimulationWorld::new_with_seed(53);
    world.arena.actors.retain(|id, _| id == world.player_id);
    world.arena.items.clear();
    world.unpaid_items.clear();
    world.billing = Default::default();
    world.equipment = Default::default();
    world.wielded_item = None;
    world.level = nethacked_dungeon::DungeonLevel::new_solid(Tile::Room);
    let player = &mut world.arena.actors[world.player_id];
    player.coord = ORIGIN;
    player.hp = 1000;
    player.max_hp = 1000;
    player.level = 2;
    world.scheduler.hero_energy = 120;
    world.scheduler.hero_speed = 12;
    world.scheduler.turn = 0;
    let mut item = world
        .ruleset
        .create_item_record(
            wand,
            ItemLocation::CarriedBy(world.player_id),
            Buc::Uncursed,
        )
        .unwrap();
    item.enchantment = 3;
    let id = world.arena.spawn_item(item);
    let actor = target.map(|name| {
        let mut actor = world
            .ruleset
            .create_monster_record(name, TARGET, &mut world.rng)
            .unwrap_or_else(|| panic!("missing fixture species: {name}"));
        actor.hp = 1;
        actor.max_hp = 1;
        actor.speed = 0;
        actor.movement = 0;
        world.arena.spawn_actor(actor)
    });
    world.rng = ChaCha8Rng::seed_from_u64(100);
    (world, id, actor)
}
fn selected(id: ItemId) -> ActionAst {
    ActionAst::ZapSelectedWand {
        item_index: 0,
        expected_item: Some(id),
        dir: Direction::East,
        energy: 6,
    }
}
fn snapshot(value: &(impl serde::Serialize + ?Sized)) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
}

#[test]
fn borrowed_timer_queue_preserves_reconciliation_serials_ties_and_error_precedence() {
    let (mut world, wand, _) = fixture("wand of striking", None);
    let a = world.arena.spawn_item(
        world
            .ruleset
            .create_item_record("rock", ItemLocation::Floor(TARGET), Buc::Uncursed)
            .unwrap(),
    );
    let b = world.arena.spawn_item(
        world
            .ruleset
            .create_item_record("rock", ItemLocation::Floor(TARGET), Buc::Uncursed)
            .unwrap(),
    );
    let active = ObjectTimerTarget::Active;
    let rot = ObjectTimerKind::RotCorpse;
    let first = world.start_object_timer(active(a), rot, 40).unwrap();
    let second = world.start_object_timer(active(b), rot, 40).unwrap();
    assert_eq!(
        world
            .object_timers
            .entries()
            .iter()
            .map(|t| t.serial)
            .collect::<Vec<_>>(),
        vec![second, first]
    );
    world.arena.items.remove(a);
    assert_eq!(
        world.start_object_timer(active(b), rot, u64::MAX),
        Err(ObjectTimerError::Duplicate)
    );
    assert_eq!(world.object_timers.entries().len(), 1);
    assert_eq!(
        world.start_object_timer(active(a), rot, u64::MAX),
        Err(ObjectTimerError::MissingObject)
    );
    assert_eq!(world.cancel_object_timers(active(b)), 1);
    let third = world.start_object_timer(active(wand), rot, 40).unwrap();
    assert_eq!(third, second + 1);
    world.scheduler.turn = 1;
    assert_eq!(
        world.start_object_timer(active(wand), ObjectTimerKind::ReviveMonster, u64::MAX),
        Err(ObjectTimerError::ClockOverflow)
    );
    assert_eq!(
        world.start_object_timer_signed(active(wand), ObjectTimerKind::ReviveMonster, -1),
        Ok(third + 1)
    );
    assert_eq!(world.object_timers.entries()[0].due, 0);
}

#[test]
fn lethal_shared_prefix_matches_existing_death_helper_including_experience_quest_and_corpse() {
    // The unchanged melee death helper is the independent compatibility oracle.
    let template = SimulationWorld::new_with_seed(53);
    let quest = nethacked_core::get_role_quest_config_or_default(&template.role_name);
    let cleric = template
        .ruleset
        .monster_by_id(MonsterSpeciesId::ALIGNED_CLERIC)
        .unwrap();
    let mut corpse_witnesses = 0;
    let mut level_witnesses = 0;
    for name in [
        "goblin",
        cleric.name.as_str(),
        quest.leader_name,
        quest.nemesis_name,
        quest.guardian_name,
    ] {
        for (peaceful, tame) in [(false, false), (true, false), (true, true)] {
            let (mut actual, wand, target) = fixture("wand of death", Some(name));
            let (mut reference, reference_wand, reference_target) =
                fixture("wand of death", Some(name));
            let target = target.unwrap();
            let reference_target = reference_target.unwrap();
            for (world, id) in [(&mut actual, target), (&mut reference, reference_target)] {
                world.arena.actors[id].is_peaceful = peaceful;
                world.arena.actors[id].is_tame = tame;
                world.divine_protection = 5;
                let mut guardian = world
                    .ruleset
                    .create_monster_record(
                        quest.guardian_name,
                        Coord::new_unchecked(30, 10),
                        &mut world.rng,
                    )
                    .unwrap();
                guardian.speed = 0;
                guardian.movement = 0;
                guardian.is_peaceful = true;
                guardian.is_tame = false;
                world.arena.spawn_actor(guardian);
                world.progress.exp = nethacked_sim::progress::newuexp(2) - 1;
            }
            let mut expected = Vec::new();
            reference.arena.actors[reference_target].hp = 0;
            reference.arena.actors[reference_target].is_dead = true;
            reference.arena.items[reference_wand].enchantment -= 1;
            reference.on_actor_killed(reference.player_id, reference_target, &mut expected);
            expected.extend(reference.step_player_action(ActionAst::Wait));
            let events = actual.step_player_action(selected(wand));
            assert!(matches!(events[0], GameEvent::BeamPropagated { .. }));
            assert!(matches!(
                events[1],
                GameEvent::AttackLanded { lethal: true, .. }
            ));
            assert_eq!(
                snapshot(&events[3..]),
                snapshot(&expected),
                "{name}, peaceful={peaceful}"
            );
            assert_eq!(
                snapshot(&actual.arena.actors),
                snapshot(&reference.arena.actors)
            );
            assert_eq!(
                snapshot(&actual.arena.items),
                snapshot(&reference.arena.items)
            );
            assert_eq!(
                actual.arena.artifact_history,
                reference.arena.artifact_history
            );
            assert_eq!(actual.object_timers, reference.object_timers);
            assert_eq!(snapshot(&actual.conducts), snapshot(&reference.conducts));
            assert_eq!(actual.progress, reference.progress);
            assert_eq!(
                snapshot(&actual.quest_state),
                snapshot(&reference.quest_state)
            );
            assert_eq!(actual.alignment_record, reference.alignment_record);
            assert_eq!(actual.player_luck, reference.player_luck);
            assert_eq!(actual.divine_protection, reference.divine_protection);
            assert_eq!(
                snapshot(&actual.divine_state),
                snapshot(&reference.divine_state)
            );
            assert_eq!(
                (actual.player_pw, actual.player_max_pw),
                (reference.player_pw, reference.player_max_pw)
            );
            assert_eq!(snapshot(&actual.scheduler), snapshot(&reference.scheduler));
            assert_eq!(actual.rng.next_u64(), reference.rng.next_u64());
            corpse_witnesses += usize::from(!actual.object_timers.entries().is_empty());
            level_witnesses += usize::from(actual.arena.actors[actual.player_id].level > 2);
        }
    }
    assert!(corpse_witnesses > 0);
    assert!(level_witnesses > 0);
}

#[test]
fn empty_locale_and_polymorphed_creation_keep_their_existing_branches_without_draws() {
    let (mut empty, wand, _) = fixture("wand of striking", None);
    empty.locale = Locale::Uk;
    empty.arena.items[wand].enchantment = -1;
    let mut expected_rng = empty.rng.clone();
    let events = empty.step_player_action(selected(wand));
    assert_eq!(
        snapshot(&events),
        snapshot(&vec![GameEvent::LogMessage {
            text: nethacked_i18n::Messages::wand_empty(Locale::Uk).into()
        }])
    );
    assert_eq!(empty.arena.items[wand].enchantment, -1);
    assert_eq!(empty.scheduler.hero_energy, 108);
    assert_eq!(empty.rng.next_u64(), expected_rng.next_u64());

    let (mut polymorphed, wand, _) = fixture("wand of create monster", None);
    polymorphed.hero.polymorph = Some(PolymorphForm {
        monster_id: 1,
        hp: 5,
        max_hp: 20,
        duration: 100,
    });
    let mut expected_rng = polymorphed.rng.clone();
    let events = polymorphed.step_player_action(selected(wand));
    assert!(matches!(
        events.as_slice(),
        [GameEvent::BeamPropagated { .. }]
    ));
    assert_eq!(polymorphed.arena.actors.len(), 1);
    assert_eq!(polymorphed.arena.items[wand].enchantment, 2);
    assert_eq!(polymorphed.rng.next_u64(), expected_rng.next_u64());
}

#[test]
fn nonlethal_beams_keep_actor_effects_and_legacy_selection_equivalent() {
    for name in [
        "wand of digging",
        "wand of teleportation",
        "wand of polymorph",
        "wand of cold",
        "wand of striking",
    ] {
        let (mut selected_world, wand, target) = fixture(name, Some("giant ant"));
        let (mut legacy, legacy_wand, legacy_target) = fixture(name, Some("giant ant"));
        let target = target.unwrap();
        let legacy_target = legacy_target.unwrap();
        for (world, id) in [(&mut selected_world, target), (&mut legacy, legacy_target)] {
            let actor = &mut world.arena.actors[id];
            actor.hp = 1000;
            actor.max_hp = 1000;
            actor.is_peaceful = true;
        }
        let selected_events = selected_world.step_player_action(selected(wand));
        let legacy_events = legacy.step_player_action(ActionAst::ZapWand {
            dir: Direction::East,
            energy: 6,
        });
        assert_eq!(
            snapshot(&selected_events),
            snapshot(&legacy_events),
            "{name}"
        );
        assert_eq!(
            snapshot(&selected_world.arena.actors),
            snapshot(&legacy.arena.actors)
        );
        assert_eq!(selected_world.arena.items[wand].enchantment, 2);
        assert_eq!(legacy.arena.items[legacy_wand].enchantment, 2);
        if name.contains("digging") {
            assert_eq!(selected_world.arena.actors[target].hp, 1000);
            assert!(selected_world.arena.actors[target].is_peaceful);
        } else {
            assert!(!selected_world.arena.actors[target].is_peaceful);
            if name.contains("teleport") {
                assert_eq!(selected_world.arena.actors[target].hp, 1000);
            } else if name.contains("polymorph") {
                assert_eq!(
                    selected_world.arena.actors[target].species,
                    Some(
                        selected_world
                            .ruleset
                            .monster_by_id(MonsterSpeciesId::GOBLIN)
                            .unwrap()
                            .actor_species()
                    )
                );
            } else {
                assert_eq!(
                    selected_world.arena.actors[target].hp,
                    if name.contains("cold") { 982 } else { 988 }
                );
            }
        }
        assert_eq!(selected_world.rng.next_u64(), legacy.rng.next_u64());
    }
}

#[test]
fn terrain_operations_and_appended_blocking_cell_preserve_events() {
    let (mut striking, wand, _) = fixture("wand of striking", None);
    striking
        .level
        .set_tile(TARGET, Tile::Drawbridge { open: false });
    let mut action = selected(wand);
    if let ActionAst::ZapSelectedWand { energy, .. } = &mut action {
        *energy = 0;
    }
    let events = striking.step_player_action(action);
    assert!(matches!(striking.level.get_tile(TARGET), Tile::Moat));
    assert!(events.iter().any(|event| matches!(event, GameEvent::LogMessage { text } if text == nethacked_i18n::Messages::drawbridge_collapse(striking.locale))));
    let (mut cold, wand, _) = fixture("wand of cold", None);
    cold.level.set_tile(TARGET, Tile::Pool { frozen: false });
    cold.step_player_action(selected(wand));
    assert!(matches!(
        cold.level.get_tile(TARGET),
        Tile::Pool { frozen: true }
    ));
    let (mut detection, wand, _) = fixture("wand of secret door detection", None);
    detection
        .level
        .set_tile(TARGET, Tile::SecretDoor { locked: true });
    let events = detection.step_player_action(selected(wand));
    assert!(matches!(
        detection.level.get_tile(TARGET),
        Tile::Door { .. }
    ));
    assert_eq!(
        snapshot(&events),
        snapshot(&vec![GameEvent::LogMessage {
            text: nethacked_i18n::Messages::secret_doors_found(1, detection.locale)
        }])
    );
    assert_eq!(
        detection
            .ruleset
            .item("wand of secret door detection")
            .unwrap()
            .id,
        Some(ItemKindId::WAN_SECRET_DOOR_DETECTION)
    );
}

#[test]
fn anger_preserves_elbereth_draw_alignment_engraving_and_localized_event_order() {
    use nethacked_core::engraving::{Engraving, EngravingMedium};
    for locale in [Locale::En, Locale::Uk] {
        let (mut world, wand, target) = fixture("wand of teleportation", Some("giant ant"));
        let target = target.unwrap();
        world.locale = locale;
        world.arena.actors[target].is_peaceful = true;
        world.arena.actors[target].is_tame = false;
        world.alignment_record = 0;
        world
            .level
            .set_engraving(ORIGIN, Engraving::new("Elbereth", EngravingMedium::Burned));
        let mut expected_rng = world.rng.clone();
        let delta = -expected_rng.random_range(1..=5i32);
        let limit = nethacked_core::alignlim(world.scheduler.turn);
        let expected_alignment =
            nethacked_core::adjalign(nethacked_core::adjalign(0, delta, limit), -1, limit);
        let events = world.step_player_action(selected(wand));
        assert_eq!(world.alignment_record, expected_alignment);
        assert!(world.level.get_engraving(ORIGIN).is_none());
        assert!(!world.arena.actors[target].is_peaceful);
        let logs: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                GameEvent::LogMessage { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(logs[1], nethacked_i18n::Messages::feel_hypocrite(locale));
        assert_eq!(logs[2], nethacked_i18n::Messages::engraving_fades(locale));
        assert_eq!(
            logs[3],
            nethacked_i18n::Messages::gets_angry("giant ant", true, locale)
        );
        assert_eq!(world.rng.next_u64(), expected_rng.next_u64());
    }
}
