//! Simulation integrity regressions: damage, genocide, wands/wishes, melee variance, timers.

use nethacked_arena::ItemLocation;
use nethacked_data::{create_item_record, create_monster_record, ItemKindId, MonsterSpeciesId};
use nethacked_sim::{ActionAst, Buc, Coord, Direction, SimulationWorld, Tile};
use nethacked_types::{TrapRecord, TrapState, TrapType};

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
    sim.level.traps.insert(
        east,
        TrapRecord {
            id: 1,
            trap_type: TrapType::Arrow,
            state: TrapState::Hidden,
            coord: east,
        },
    );
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
    let mut scroll = create_item_record(
        ItemKindId::SCR_IDENTIFY,
        ItemLocation::CarriedBy(sim.player_id),
        buc,
    );
    scroll.name = "scroll of genocide".into();
    let id = sim.arena.spawn_item(scroll);
    sim.arena
        .items_carried_by(sim.player_id)
        .iter()
        .position(|&i| i == id)
        .unwrap()
}

#[test]
fn blessed_genocide_spares_non_lich_actors_and_drops_items() {
    let mut sim = SimulationWorld::new_with_seed(77);
    let p = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let c1 = Coord::new(p.x + 2, p.y).unwrap();
    let c2 = Coord::new(p.x + 3, p.y).unwrap();
    let lich = sim
        .arena
        .spawn_actor(create_monster_record(MonsterSpeciesId::MASTER_LICH, c1));
    let dog = sim
        .arena
        .spawn_actor(create_monster_record(MonsterSpeciesId::LITTLE_DOG, c2));
    let loot = sim.arena.spawn_item(create_item_record(
        ItemKindId::LONG_SWORD,
        ItemLocation::CarriedBy(lich),
        Buc::Uncursed,
    ));
    let before = sim.arena.actors.len();

    let idx = genocide_scroll(&mut sim, Buc::Blessed);
    sim.step_player_action(ActionAst::Read(idx));

    assert!(!sim.arena.actors.contains_key(lich));
    assert!(sim.arena.actors.contains_key(dog));
    assert_eq!(sim.arena.actors.len(), before - 1);
    assert_eq!(
        sim.arena.items.get(loot).unwrap().location,
        ItemLocation::Floor(c1)
    );
}

#[test]
fn uncursed_genocide_hits_bestiary_goblins() {
    let mut sim = SimulationWorld::new_with_seed(78);
    let p = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let gob = sim.arena.spawn_actor(create_monster_record(
        MonsterSpeciesId::GOBLIN,
        Coord::new(p.x + 2, p.y).unwrap(),
    ));
    let idx = genocide_scroll(&mut sim, Buc::Uncursed);
    sim.step_player_action(ActionAst::Read(idx));
    assert!(!sim.arena.actors.contains_key(gob));
    assert!(nethacked_core::genocide::is_genocided(
        &sim.genocide_registry,
        "goblin",
        'o'
    ));
}

#[test]
fn monster_class_lookup() {
    assert_eq!(nethacked_data::monster_class_of("master lich"), Some('L'));
    assert_eq!(nethacked_data::monster_class_of("GOBLIN"), Some('o'));
    assert_eq!(nethacked_data::monster_class_of("no such thing"), None);
    // Class letters follow C defsym.h: skeleton is S_ZOMBIE 'Z', gnome S_GNOME 'G', ghost S_GHOST ' '.
    assert_eq!(nethacked_data::monster_class_of("skeleton"), Some('Z'));
    assert_eq!(nethacked_data::monster_class_of("gnome"), Some('G'));
    assert_eq!(nethacked_data::monster_class_of("ghost"), Some(' '));
    assert_eq!(nethacked_data::monster_class_of("dwarf"), Some('h'));
}

fn has_item(sim: &SimulationWorld, name: &str) -> bool {
    sim.arena.items.values().any(|it| it.name == name)
}

#[test]
fn zap_without_wand_is_free_noop() {
    let mut sim = SimulationWorld::new_with_seed(90);
    for id in sim.arena.items_carried_by(sim.player_id) {
        if sim.arena.items.get(id).unwrap().class == nethacked_types::ItemClass::Wand {
            sim.arena.items.remove(id);
        }
    }
    let energy = sim.scheduler.hero_energy;
    let ev = sim.step_player_action(ActionAst::ZapWand {
        dir: Direction::East,
        energy: 6,
    });
    assert!(ev.iter().any(|e| format!("{e:?}").contains("no wand")));
    assert!(!ev
        .iter()
        .any(|e| matches!(e, nethacked_sim::GameEvent::BeamPropagated { .. })));
    assert_eq!(sim.scheduler.hero_energy, energy);
}

#[test]
fn wish_requires_charged_wand_of_wishing() {
    let mut sim = SimulationWorld::new_with_seed(91);
    let swords_before = sim
        .arena
        .items
        .values()
        .filter(|it| it.name == "long sword")
        .count();
    sim.step_player_action(ActionAst::Wish("long sword".into()));
    assert_eq!(
        sim.arena
            .items
            .values()
            .filter(|it| it.name == "long sword")
            .count(),
        swords_before
    );

    sim.arena.spawn_item(create_item_record(
        ItemKindId::WAN_WISHING,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Uncursed,
    ));
    let pcoord = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let floor_swords = |sim: &SimulationWorld| {
        sim.arena
            .items
            .values()
            .filter(|it| it.name == "long sword" && it.location == ItemLocation::Floor(pcoord))
            .count()
    };
    let floor_before = floor_swords(&sim);
    sim.step_player_action(ActionAst::Wish("a blessed +2 long sword".into()));
    assert_eq!(floor_swords(&sim), floor_before + 1);
    let sword = sim
        .arena
        .items
        .values()
        .find(|it| it.name == "long sword" && it.buc == Buc::Blessed && it.enchantment == 2)
        .expect("wish granted");
    assert_eq!(sword.buc, Buc::Blessed);
    assert_eq!(sword.enchantment, 2);
    // Wand had 1 charge: the next wish must not create anything.
    let dagger_count_before = sim
        .arena
        .items
        .values()
        .filter(|it| it.name == "dagger")
        .count();
    sim.step_player_action(ActionAst::Wish("dagger".into()));
    assert_eq!(
        sim.arena
            .items
            .values()
            .filter(|it| it.name == "dagger")
            .count(),
        dagger_count_before
    );
}

#[test]
fn wishing_for_the_amulet_gives_imitation() {
    let mut sim = SimulationWorld::new_with_seed(92);
    sim.arena.spawn_item(create_item_record(
        ItemKindId::WAN_WISHING,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Uncursed,
    ));
    sim.step_player_action(ActionAst::Wish("the Amulet of Yendor".into()));
    assert!(!has_item(&sim, "Amulet of Yendor"));
    assert!(has_item(
        &sim,
        "cheap plastic imitation of the Amulet of Yendor"
    ));
}

#[test]
fn wish_substring_does_not_match() {
    let mut sim = SimulationWorld::new_with_seed(93);
    sim.arena.spawn_item(create_item_record(
        ItemKindId::WAN_WISHING,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Uncursed,
    ));
    let before = sim.arena.items.len();
    sim.step_player_action(ActionAst::Wish("sword".into()));
    assert_eq!(sim.arena.items.len(), before);
}

#[test]
fn catalog_wands_start_charged() {
    let w = create_item_record(ItemKindId::WAN_STRIKING, ItemLocation::Limbo, Buc::Uncursed);
    assert_eq!(w.enchantment, 6);
    let w = create_item_record(ItemKindId::WAN_WISHING, ItemLocation::Limbo, Buc::Uncursed);
    assert_eq!(w.enchantment, 1);
    let s = create_item_record(ItemKindId::LONG_SWORD, ItemLocation::Limbo, Buc::Uncursed);
    assert_eq!(s.enchantment, 0);
}

#[test]
fn normalize_wish_strips_articles() {
    assert_eq!(
        nethacked_sim::normalize_wish_name("  The Amulet of Yendor "),
        "amulet of yendor"
    );
    assert_eq!(
        nethacked_sim::normalize_wish_name("an elven mithril-coat"),
        "elven mithril-coat"
    );
}

#[test]
fn melee_outcomes_vary_with_seed() {
    use std::collections::BTreeSet;
    let mut outcomes = BTreeSet::new();
    for seed in 0..40u64 {
        let mut sim = SimulationWorld::new_with_seed(seed);
        let east = open_east(&mut sim);
        let mut mon = create_monster_record(MonsterSpeciesId::GOBLIN, east);
        mon.hp = 1000;
        mon.max_hp = 1000;
        // C to-hit: tmp = 1 + AC 13 + lvl 1 + long sword unskilled -4 = 11, so a
        // d20 of 1..=10 hits (~50%): both misses and varied damage occur.
        mon.ac = 13;
        let mid = sim.arena.spawn_actor(mon);
        sim.step_player_action(ActionAst::MeleeAttack(east));
        let hp = sim.arena.actors.get(mid).map(|m| m.hp).unwrap_or(0);
        outcomes.insert(1000 - hp);
    }
    assert!(outcomes.len() >= 3, "melee is not random: {outcomes:?}");
}

#[test]
fn bumping_a_wall_does_not_tick_prayer_timeout() {
    let mut sim = SimulationWorld::new_with_seed(12);
    let p = sim.arena.actors.get(sim.player_id).unwrap().coord;
    assert!(p.x > 0, "seed 12 player position must have x > 0");
    let west = Coord::new(p.x - 1, p.y).unwrap();
    sim.level.set_tile(west, Tile::Wall { horizontal: false });
    sim.divine_state.prayer_timeout = 100;
    let turn = sim.scheduler.turn;
    for _ in 0..10 {
        sim.step_player_action(ActionAst::Move(Direction::West));
    }
    assert_eq!(sim.scheduler.turn, turn, "wall bump should take no time");
    assert_eq!(sim.divine_state.prayer_timeout, 100);
}

#[test]
fn fake_amulet_does_not_win_the_game() {
    let mut sim = SimulationWorld::new_with_seed(94);
    sim.arena.spawn_item(create_item_record(
        ItemKindId::WAN_WISHING,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Uncursed,
    ));
    sim.step_player_action(ActionAst::Wish("the Amulet of Yendor".into()));
    let fake = sim
        .arena
        .items
        .iter()
        .find(|(_, it)| it.name == "cheap plastic imitation of the Amulet of Yendor")
        .map(|(id, _)| id)
        .expect("fake amulet");
    sim.arena.items.get_mut(fake).unwrap().location = ItemLocation::CarriedBy(sim.player_id);
    let up = sim.level.stairs_up;
    sim.arena.actors.get_mut(sim.player_id).unwrap().coord = up;
    assert_eq!(sim.depth, 1);
    let ev = sim.step_player_action(ActionAst::Ascend);
    assert!(!ev
        .iter()
        .any(|e| matches!(e, nethacked_sim::GameEvent::Victory)));
}

#[test]
fn wished_wands_keep_initial_charges() {
    let mut sim = SimulationWorld::new_with_seed(95);
    for q in ["wand of striking", "+100 wand of death"] {
        let mut w = create_item_record(
            ItemKindId::WAN_WISHING,
            ItemLocation::CarriedBy(sim.player_id),
            Buc::Uncursed,
        );
        w.enchantment = 3;
        sim.arena.spawn_item(w);
        sim.step_player_action(ActionAst::Wish(q.into()));
    }
    for name in ["wand of striking", "wand of death"] {
        let w = sim
            .arena
            .items
            .values()
            .find(|it| it.name == name && matches!(it.location, ItemLocation::Floor(_)))
            .unwrap_or_else(|| panic!("{name} not wished"));
        assert_eq!(w.enchantment, 6, "{name}");
    }
}
