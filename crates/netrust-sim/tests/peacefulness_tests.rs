//! Integration tests for NetHack 5.0 C monster peacefulness and Elbereth exemptions.
//!
//! NetHack C reference:
//! - `makemon.c:2268-2308` (`peace_minded`)
//! - `monmove.c:240-303` (`onscary`)
//! - `dogmove.c:1119-1128` (pet skipping peaceful monsters)

use netrust_arena::ActorRecord;
use netrust_core::engraving::EngravingMedium;
use netrust_core::ActionAst;
use netrust_data::{create_monster_record, MonsterSpeciesId};
use netrust_sim::{GameEvent, SimulationWorld};
use netrust_types::{BranchId, Coord, Direction, Tile};

#[test]
fn test_peaceful_monsters_spawn_peaceful_and_never_attack_over_50_turns() {
    let mut sim = SimulationWorld::new_with_seed(42);
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;

    // Clear surroundings
    for dy in -2..=2 {
        for dx in -2..=2 {
            if let Some(c) = Coord::new(
                (p_coord.x as isize + dx) as usize,
                (p_coord.y as isize + dy) as usize,
            ) {
                sim.level.set_tile(c, Tile::Room);
            }
        }
    }

    // Spawn watchman, priest, and shopkeeper adjacent to hero
    let c_watchman = p_coord.step(Direction::East).unwrap();
    let c_priest = p_coord.step(Direction::West).unwrap();
    let c_shopkeeper = p_coord.step(Direction::North).unwrap();

    let watchman_id = sim.arena.spawn_actor(create_monster_record(
        MonsterSpeciesId::Watchman,
        c_watchman,
    ));
    let priest_id = sim
        .arena
        .spawn_actor(create_monster_record(MonsterSpeciesId::Priest, c_priest));
    let shopkeeper_id = sim.arena.spawn_actor(create_monster_record(
        MonsterSpeciesId::Shopkeeper,
        c_shopkeeper,
    ));

    assert!(sim.arena.actors.get(watchman_id).unwrap().is_peaceful);
    assert!(sim.arena.actors.get(priest_id).unwrap().is_peaceful);
    assert!(sim.arena.actors.get(shopkeeper_id).unwrap().is_peaceful);

    let initial_hp = sim.arena.actors.get(sim.player_id).unwrap().hp;

    // Over 50 turns of waiting, none of the peaceful monsters should ever attack the hero
    for _ in 0..50 {
        let events = sim.step_player_action(ActionAst::Wait);
        for e in events {
            if let GameEvent::AttackLanded { target, .. } = e {
                assert_ne!(target, sim.player_id, "Peaceful monster attacked the hero!");
            }
        }
        assert_eq!(
            sim.arena.actors.get(sim.player_id).unwrap().hp,
            initial_hp,
            "Hero HP should not decrease from peaceful monsters"
        );
    }
}

#[test]
fn test_hero_attack_makes_peaceful_monster_hostile_permanently() {
    let mut sim = SimulationWorld::new_with_seed(101);
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;

    let c_priest = p_coord.step(Direction::East).unwrap();
    sim.level.set_tile(c_priest, Tile::Room);

    let priest_id = sim
        .arena
        .spawn_actor(create_monster_record(MonsterSpeciesId::Priest, c_priest));
    assert!(sim.arena.actors.get(priest_id).unwrap().is_peaceful);

    // Boost hero HP so hero survives the high-damage priest attack
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.hp = 500;
        p.max_hp = 500;
    }

    // Hero attacks the peaceful priest
    let attack_events = sim.step_player_action(ActionAst::MeleeAttack(c_priest));
    assert!(
        attack_events.iter().any(
            |e| matches!(e, GameEvent::LogMessage { text } if text.contains("The priest gets angry!"))
        ),
        "Attack on peaceful monster must log C setmangry's \"%s gets angry!\" (mon.c:4306)"
    );

    let priest_after = sim.arena.actors.get(priest_id).unwrap();
    assert!(
        !priest_after.is_peaceful,
        "Priest must no longer be peaceful"
    );

    // On the hero's action or immediate next wait, the now-hostile priest attacks back
    let wait_events = sim.step_player_action(ActionAst::Wait);
    let priest_attacked = attack_events.iter().chain(wait_events.iter()).any(|e| {
        matches!(
            e,
            GameEvent::AttackLanded {
                attacker, target, ..
            } | GameEvent::AttackMissed { attacker, target }
            if *attacker == priest_id && *target == sim.player_id
        )
    });
    assert!(priest_attacked, "Now-hostile priest must attack hero back");
}

#[test]
fn test_quest_leader_and_guardians_are_peaceful_not_tame() {
    let mut sim = SimulationWorld::new_with_seed(42);
    sim.role_name = "Valkyrie".to_string();
    sim.current_branch = BranchId::Quest;
    sim.depth = 1;

    let _ = sim.unpack_or_generate_level(BranchId::Quest, 1);

    let cfg = netrust_core::get_role_quest_config("Valkyrie").unwrap();

    let leader = sim
        .arena
        .actors
        .values()
        .find(|a| a.name == cfg.leader_name)
        .expect("Quest leader must exist");
    assert!(leader.is_peaceful, "Quest leader must be peaceful");
    assert!(!leader.is_tame, "Quest leader must NOT be tame");

    let guardian = sim
        .arena
        .actors
        .values()
        .find(|a| a.name == cfg.guardian_name)
        .expect("Quest guardian must exist");
    assert!(guardian.is_peaceful, "Quest guardian must be peaceful");
    assert!(!guardian.is_tame, "Quest guardian must NOT be tame");

    // Stepping towards the quest leader must not displace it like a pet
    // (C mundisplaceable, hack.c:2154-2161: "You stop. ... doesn't want to swap places.")
    let leader_coord = leader.coord;
    // Set hero adjacent to leader
    let adj = leader_coord.step(Direction::West).unwrap();
    sim.level.set_tile(adj, Tile::Room);
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = adj;
    }

    let move_events = sim.step_player_action(ActionAst::Move(Direction::East));
    assert!(
        !move_events
            .iter()
            .any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("You displace"))),
        "Hero must not displace quest leader like a pet"
    );
}

#[test]
fn test_elbereth_exemptions_and_repulsion() {
    let mut sim = SimulationWorld::new_with_seed(55);
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;

    // Engrave Elbereth on hero tile
    sim.step_player_action(ActionAst::Engrave {
        text: "Elbereth".into(),
        medium: EngravingMedium::Burned,
    });

    let c_east = p_coord.step(Direction::East).unwrap();
    sim.level.set_tile(c_east, Tile::Room);

    // 1. Goblin (not exempt, hostile) is repelled by Elbereth
    let goblin_id = sim
        .arena
        .spawn_actor(create_monster_record(MonsterSpeciesId::Goblin, c_east));
    let events = sim.step_player_action(ActionAst::Wait);
    assert!(
        events.iter().any(|e| matches!(
            e,
            GameEvent::LogMessage { text } if text.contains("repelled by the sacred ward of Elbereth")
        )),
        "Goblin must be repelled by Elbereth"
    );
    sim.arena.destroy_actor(goblin_id);

    // 2. Human / Shopkeeper (exempt) is NOT repelled even when hostile
    let c_north = p_coord.step(Direction::North).unwrap();
    sim.level.set_tile(c_north, Tile::Room);
    let mut shk_rec = create_monster_record(MonsterSpeciesId::Shopkeeper, c_north);
    shk_rec.is_peaceful = false; // Make hostile
    let shk_id = sim.arena.spawn_actor(shk_rec);

    let shk_events = sim.step_player_action(ActionAst::Wait);
    assert!(
        !shk_events.iter().any(|e| matches!(
            e,
            GameEvent::LogMessage { text } if text.contains("repelled by the sacred ward of Elbereth")
        )),
        "Shopkeeper must be exempt from Elbereth"
    );
    assert!(
        shk_events.iter().any(|e| matches!(
            e,
            GameEvent::AttackLanded { attacker, .. } | GameEvent::AttackMissed { attacker, .. }
            if *attacker == shk_id
        )),
        "Hostile shopkeeper ignores Elbereth and attacks hero"
    );
}

#[test]
fn test_companion_pet_does_not_attack_peaceful_monsters() {
    let mut sim = SimulationWorld::new_with_seed(1234);
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;

    // Pet adjacent to hero
    let pet_coord = p_coord.step(Direction::East).unwrap();
    sim.level.set_tile(pet_coord, Tile::Room);
    let mut pet_rec = create_monster_record(MonsterSpeciesId::LittleDog, pet_coord);
    pet_rec.is_tame = true;
    let pet_id = sim.arena.spawn_actor(pet_rec);

    // Peaceful watchman adjacent to pet
    let watchman_coord = pet_coord.step(Direction::East).unwrap();
    sim.level.set_tile(watchman_coord, Tile::Room);
    let watchman_id = sim.arena.spawn_actor(create_monster_record(
        MonsterSpeciesId::Watchman,
        watchman_coord,
    ));
    assert!(sim.arena.actors.get(watchman_id).unwrap().is_peaceful);

    let initial_watchman_hp = sim.arena.actors.get(watchman_id).unwrap().hp;

    for _ in 0..20 {
        let events = sim.step_player_action(ActionAst::Wait);
        for e in events {
            if let GameEvent::AttackLanded {
                attacker, target, ..
            } = e
            {
                assert!(
                    !(attacker == pet_id && target == watchman_id),
                    "Pet must not attack peaceful watchman!"
                );
            }
        }
    }

    assert_eq!(
        sim.arena.actors.get(watchman_id).unwrap().hp,
        initial_watchman_hp,
        "Watchman must not take damage from companion pet"
    );
}

#[test]
fn test_serde_default_actor_record_is_peaceful() {
    let mut actor = create_monster_record(MonsterSpeciesId::Priest, Coord::new(1, 1).unwrap());
    actor.is_peaceful = true;
    let mut v = serde_json::to_value(&actor).expect("Serialization must succeed");
    v.as_object_mut().unwrap().remove("is_peaceful");

    let restored: ActorRecord = serde_json::from_value(v).expect("Deserialization must succeed");
    assert!(
        !restored.is_peaceful,
        "Missing is_peaceful must default to false"
    );
    assert_eq!(restored.name, "priest");
}

// ---------------------------------------------------------------------------
// D2 fix wave C: peace_minded at spawn, setmangry, bumping peacefuls.
// ---------------------------------------------------------------------------

use netrust_core::engraving::Engraving;
use netrust_data::{CharacterConfig, Gender, RaceId, RoleId};
use netrust_types::Alignment;
use rand::Rng;

fn sim_with(race: RaceId, alignment: Alignment, role: RoleId, seed: u64) -> SimulationWorld {
    let mut sim = SimulationWorld::new_with_character(
        seed,
        CharacterConfig {
            name: "Hero".into(),
            role,
            race,
            gender: Gender::Female,
            alignment,
        },
    );
    let pc = sim.arena.actors.get(sim.player_id).unwrap().coord;
    for dy in -2..=2isize {
        for dx in -2..=2isize {
            if let Some(c) =
                Coord::new((pc.x as isize + dx) as usize, (pc.y as isize + dy) as usize)
            {
                if c != pc {
                    sim.level.set_tile(c, Tile::Room);
                    if let Some(id) = sim.actor_at(c) {
                        sim.arena.destroy_actor(id);
                    }
                }
            }
        }
    }
    sim
}

fn hero_coord(sim: &SimulationWorld) -> Coord {
    sim.arena.actors.get(sim.player_id).unwrap().coord
}

/// Spawn next to the hero (east) through the sim's generation path.
fn spawn_east(sim: &mut SimulationWorld, species: MonsterSpeciesId) -> netrust_arena::ActorId {
    let c = hero_coord(sim).step(Direction::East).unwrap();
    sim.spawn_monster_near(species, c).expect("spawn spot")
}

/// makemon.c:1299 -> peace_minded (makemon.c:2305-2307): a co-aligned goblin
/// for an orcish (chaotic) hero is peaceful with probability
/// (a-1)(b-1)/(ab), a = 16 + record, b = 2 + |mal| = 5.
#[test]
fn test_coaligned_spawn_peaceful_rate_matches_c() {
    let mut sim = sim_with(RaceId::Orc, Alignment::Chaotic, RoleId::Barbarian, 7);
    sim.alignment_record = 0;
    let (a, b) = (16.0, 5.0);
    let expected = (a - 1.0) * (b - 1.0) / (a * b); // 0.75
    let n = 4000;
    let mut peaceful = 0;
    for _ in 0..n {
        let id = spawn_east(&mut sim, MonsterSpeciesId::Goblin);
        if sim.arena.actors.get(id).unwrap().is_peaceful {
            peaceful += 1;
        }
        sim.arena.destroy_actor(id);
    }
    let rate = peaceful as f64 / n as f64;
    assert!(
        (rate - expected).abs() < 0.03,
        "peaceful rate {rate} vs C {expected}"
    );
}

/// record <= -15 makes the first draw rn2(1) == 0: hostile after exactly one draw.
#[test]
fn test_coaligned_spawn_with_record_minus_15_is_hostile_after_one_draw() {
    let mut sim = sim_with(RaceId::Orc, Alignment::Chaotic, RoleId::Barbarian, 8);
    sim.alignment_record = -20;
    for _ in 0..50 {
        let mut expected_rng = sim.rng.clone();
        let _ = expected_rng.random_range(0..1u32);
        let id = spawn_east(&mut sim, MonsterSpeciesId::Goblin);
        assert!(!sim.arena.actors.get(id).unwrap().is_peaceful);
        assert_eq!(sim.rng, expected_rng, "exactly one rn2(1) draw");
        sim.arena.destroy_actor(id);
    }
}

/// Flag, race and cross-alignment decisions draw nothing and follow C.
#[test]
fn test_spawn_decisions_without_a_draw() {
    // Neutral human hero: kobold M2_HOSTILE, goblin race_hostile (human hates
    // orcs), gnome race_hostile, dwarf cross-aligned (lawful), shopkeeper and
    // watchman M2_PEACEFUL, Master Assassin MS_NEMESIS/M2_HOSTILE.
    let mut sim = sim_with(RaceId::Human, Alignment::Neutral, RoleId::Valkyrie, 9);
    for (species, want) in [
        (MonsterSpeciesId::Kobold, false),
        (MonsterSpeciesId::Goblin, false),
        (MonsterSpeciesId::Gnome, false),
        (MonsterSpeciesId::Dwarf, false),
        (MonsterSpeciesId::Shopkeeper, true),
        (MonsterSpeciesId::Watchman, true),
        (MonsterSpeciesId::MasterAssassin, false),
        (MonsterSpeciesId::Warrior, true),
    ] {
        let before = sim.rng.clone();
        let id = spawn_east(&mut sim, species);
        assert_eq!(
            sim.arena.actors.get(id).unwrap().is_peaceful,
            want,
            "{species:?}"
        );
        assert_eq!(sim.rng, before, "{species:?} must not draw");
        sim.arena.destroy_actor(id);
    }
    // Dwarvish hero: dwarves and gnomes are race_peaceful (role.c:634).
    let mut sim = sim_with(RaceId::Dwarf, Alignment::Lawful, RoleId::Valkyrie, 10);
    for species in [MonsterSpeciesId::Dwarf, MonsterSpeciesId::Gnome] {
        let before = sim.rng.clone();
        let id = spawn_east(&mut sim, species);
        assert!(sim.arena.actors.get(id).unwrap().is_peaceful, "{species:?}");
        assert_eq!(sim.rng, before);
        sim.arena.destroy_actor(id);
    }
}

/// makemon.c:2294: a chaotic monster is hostile to a hero carrying the Amulet.
#[test]
fn test_amulet_makes_chaotic_spawns_hostile() {
    let mut sim = sim_with(RaceId::Orc, Alignment::Chaotic, RoleId::Barbarian, 11);
    sim.alignment_record = 10;
    sim.arena.spawn_item(netrust_data::create_item_record(
        netrust_data::ItemKindId::AmuletOfYendor,
        netrust_arena::ItemLocation::CarriedBy(sim.player_id),
        netrust_types::Buc::Blessed,
    ));
    for _ in 0..20 {
        let before = sim.rng.clone();
        let id = spawn_east(&mut sim, MonsterSpeciesId::Goblin);
        assert!(!sim.arena.actors.get(id).unwrap().is_peaceful);
        assert_eq!(sim.rng, before);
        sim.arena.destroy_actor(id);
    }
}

/// Tou-goal.lua:117 creates the Master of Thieves with `peaceful = 0`.
#[test]
fn test_tourist_nemesis_master_of_thieves_is_hostile() {
    let mut sim = SimulationWorld::new_with_seed(42);
    sim.role_name = "Tourist".to_string();
    sim.current_branch = BranchId::Quest;
    sim.depth = 3;
    let _ = sim.unpack_or_generate_level(BranchId::Quest, 3);
    let nemesis = sim
        .arena
        .actors
        .values()
        .find(|a| a.name == "Master of Thieves")
        .expect("nemesis");
    assert!(!nemesis.is_peaceful);
}

fn place_east(sim: &mut SimulationWorld, mut rec: ActorRecord) -> netrust_arena::ActorId {
    rec.coord = hero_coord(sim).step(Direction::East).unwrap();
    rec.hp = 500;
    rec.max_hp = 500;
    sim.arena.spawn_actor(rec)
}

fn has_msg(events: &[GameEvent], needle: &str) -> bool {
    events
        .iter()
        .any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains(needle)))
}

/// mon.c:4296-4303 setmangry: attacking a peaceful non-priest costs adjalign(-1)
/// and prints "<Mon> gets angry!".
#[test]
fn test_attacking_peaceful_costs_one_alignment() {
    let mut sim = sim_with(RaceId::Human, Alignment::Neutral, RoleId::Valkyrie, 12);
    sim.alignment_record = 7;
    let c = hero_coord(&sim).step(Direction::East).unwrap();
    let id = place_east(
        &mut sim,
        create_monster_record(MonsterSpeciesId::Watchman, c),
    );
    let ev = sim.step_player_action(ActionAst::MeleeAttack(c));
    assert!(!sim.arena.actors.get(id).unwrap().is_peaceful);
    assert!(has_msg(&ev, "The watchman gets angry!"), "{ev:?}");
    assert_eq!(sim.alignment_record, 6);
}

/// mon.c:4297-4301: a co-aligned temple priest costs -5, a cross-aligned one +2.
#[test]
fn test_attacking_priest_alignment_effects() {
    let mut sim = sim_with(RaceId::Human, Alignment::Neutral, RoleId::Valkyrie, 13);
    sim.alignment_record = 8;
    let c = hero_coord(&sim).step(Direction::East).unwrap();
    place_east(&mut sim, create_monster_record(MonsterSpeciesId::Priest, c));
    sim.step_player_action(ActionAst::MeleeAttack(c));
    assert_eq!(sim.alignment_record, 3, "co-aligned priest: adjalign(-5)");

    let mut sim = sim_with(RaceId::Human, Alignment::Neutral, RoleId::Valkyrie, 14);
    sim.alignment_record = 3;
    let mut priest = create_monster_record(MonsterSpeciesId::Priest, c);
    priest.alignment = Alignment::Lawful;
    let c = hero_coord(&sim).step(Direction::East).unwrap();
    priest.coord = c;
    place_east(&mut sim, priest);
    sim.step_player_action(ActionAst::MeleeAttack(c));
    assert_eq!(
        sim.alignment_record, 5,
        "cross-aligned priest: adjalign(+2)"
    );
}

/// mon.c:4267-4285: attacking from an Elbereth square a monster that Elbereth
/// scares (or a peaceful one) is hypocritical: -5 (record > 5) and the
/// engraving is erased.
#[test]
fn test_attacking_from_elbereth_is_hypocritical() {
    let mut sim = sim_with(RaceId::Human, Alignment::Neutral, RoleId::Valkyrie, 15);
    sim.alignment_record = 9;
    let pc = hero_coord(&sim);
    sim.level
        .set_engraving(pc, Engraving::new("Elbereth", EngravingMedium::Burned));
    let c = pc.step(Direction::East).unwrap();
    let mut gob = create_monster_record(MonsterSpeciesId::Goblin, c);
    gob.is_peaceful = false;
    place_east(&mut sim, gob);
    let ev = sim.step_player_action(ActionAst::MeleeAttack(c));
    assert!(has_msg(&ev, "You feel like a hypocrite."), "{ev:?}");
    assert!(has_msg(&ev, "The engraving beneath you fades."));
    assert!(sim.level.get_engraving(pc).is_none());
    assert_eq!(sim.alignment_record, 4, "hostile goblin: only the -5");

    // Peaceful target from Elbereth: -5 then setmangry's -1.
    let mut sim = sim_with(RaceId::Human, Alignment::Neutral, RoleId::Valkyrie, 16);
    sim.alignment_record = 9;
    let pc = hero_coord(&sim);
    sim.level
        .set_engraving(pc, Engraving::new("Elbereth", EngravingMedium::Burned));
    let c = pc.step(Direction::East).unwrap();
    place_east(
        &mut sim,
        create_monster_record(MonsterSpeciesId::Watchman, c),
    );
    sim.step_player_action(ActionAst::MeleeAttack(c));
    assert_eq!(sim.alignment_record, 3);

    // record <= 5: the penalty is -rnd(5).
    let mut sim = sim_with(RaceId::Human, Alignment::Neutral, RoleId::Valkyrie, 17);
    sim.alignment_record = 2;
    let pc = hero_coord(&sim);
    sim.level
        .set_engraving(pc, Engraving::new("Elbereth", EngravingMedium::Burned));
    let c = pc.step(Direction::East).unwrap();
    let mut gob = create_monster_record(MonsterSpeciesId::Goblin, c);
    gob.is_peaceful = false;
    place_east(&mut sim, gob);
    sim.step_player_action(ActionAst::MeleeAttack(c));
    assert!((-3..=1).contains(&sim.alignment_record));
}

/// A blind or exempt (`@`) hostile target is not scared, so no hypocrisy.
#[test]
fn test_attacking_unscared_hostile_from_elbereth_is_not_hypocritical() {
    let mut sim = sim_with(RaceId::Human, Alignment::Neutral, RoleId::Valkyrie, 18);
    sim.alignment_record = 9;
    let pc = hero_coord(&sim);
    sim.level
        .set_engraving(pc, Engraving::new("Elbereth", EngravingMedium::Burned));
    let c = pc.step(Direction::East).unwrap();
    let mut wm = create_monster_record(MonsterSpeciesId::Watchman, c);
    wm.is_peaceful = false;
    place_east(&mut sim, wm);
    let ev = sim.step_player_action(ActionAst::MeleeAttack(c));
    assert!(!has_msg(&ev, "hypocrite"));
    assert!(sim.level.get_engraving(pc).is_some());
    assert_eq!(sim.alignment_record, 9);
}

/// mon.c:4310-4311 -> qst_guardians_respond (mon.c:4135-4159).
#[test]
fn test_attacking_quest_leader_angers_guardians() {
    let mut sim = SimulationWorld::new_with_seed(42);
    sim.role_name = "Valkyrie".to_string();
    sim.current_branch = BranchId::Quest;
    sim.depth = 1;
    let _ = sim.unpack_or_generate_level(BranchId::Quest, 1);
    let cfg = netrust_core::get_role_quest_config("Valkyrie").unwrap();
    let (leader_id, leader_c) = sim
        .arena
        .actors
        .iter()
        .find(|(_, a)| a.name == cfg.leader_name)
        .map(|(id, a)| (id, a.coord))
        .unwrap();
    if let Some(l) = sim.arena.actors.get_mut(leader_id) {
        l.hp = 5000;
        l.max_hp = 5000;
    }
    let adj = leader_c.step(Direction::West).unwrap();
    sim.level.set_tile(adj, Tile::Room);
    if let Some(id) = sim.actor_at(adj) {
        sim.arena.destroy_actor(id);
    }
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = adj;
        p.hp = 5000;
        p.max_hp = 5000;
    }
    let ev = sim.step_player_action(ActionAst::MeleeAttack(leader_c));
    assert!(has_msg(&ev, "appear to be angry too"), "{ev:?}");
    assert!(sim
        .arena
        .actors
        .values()
        .filter(|a| a.name == cfg.guardian_name)
        .all(|a| !a.is_peaceful));
}

/// uhitm.c:462-509 + hack.c:2154-2161: walking into a peaceful never attacks
/// it. The quest leader is mundisplaceable: "You stop. ... doesn't want to swap places."
#[test]
fn test_moving_into_peaceful_leader_does_not_attack() {
    let mut sim = SimulationWorld::new_with_seed(42);
    sim.role_name = "Valkyrie".to_string();
    sim.current_branch = BranchId::Quest;
    sim.depth = 1;
    let _ = sim.unpack_or_generate_level(BranchId::Quest, 1);
    let cfg = netrust_core::get_role_quest_config("Valkyrie").unwrap();
    let (leader_id, leader_c) = sim
        .arena
        .actors
        .iter()
        .find(|(_, a)| a.name == cfg.leader_name)
        .map(|(id, a)| (id, a.coord))
        .unwrap();
    let adj = leader_c.step(Direction::West).unwrap();
    sim.level.set_tile(adj, Tile::Room);
    if let Some(id) = sim.actor_at(adj) {
        sim.arena.destroy_actor(id);
    }
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = adj;
    }
    let record = sim.alignment_record;
    let hp = sim.arena.actors.get(leader_id).unwrap().hp;
    for _ in 0..10 {
        let ev = sim.step_player_action(ActionAst::Move(Direction::East));
        assert!(
            !ev.iter().any(|e| matches!(
                e,
                GameEvent::AttackLanded { target, .. } | GameEvent::AttackMissed { target, .. }
                if *target == leader_id
            )),
            "{ev:?}"
        );
        assert!(
            has_msg(&ev, "You stop."),
            "leader must not be displaced: {ev:?}"
        );
        let leader = sim.arena.actors.get(leader_id).unwrap();
        assert!(leader.is_peaceful);
        assert_eq!(leader.hp, hp);
        assert_eq!(leader.coord, leader_c);
        assert_eq!(hero_coord(&sim), adj);
    }
    assert_eq!(sim.alignment_record, record);
}

/// A displaceable peaceful (a gnome for a gnomish hero) either blocks
/// ("You stop. ... is in the way!", `!rn2(7)`) or swaps places; never fought.
#[test]
fn test_moving_into_peaceful_gnome_swaps_or_stops() {
    let mut swapped = 0;
    let mut stopped = 0;
    for seed in 0..60 {
        let mut sim = sim_with(
            RaceId::Gnome,
            Alignment::Neutral,
            RoleId::Wizard,
            100 + seed,
        );
        let pc = hero_coord(&sim);
        let id = spawn_east(&mut sim, MonsterSpeciesId::Gnome);
        let gc = sim.arena.actors.get(id).unwrap().coord;
        assert_eq!(gc, pc.step(Direction::East).unwrap());
        assert!(sim.arena.actors.get(id).unwrap().is_peaceful);
        let ev = sim.step_player_action(ActionAst::Move(Direction::East));
        assert!(!ev.iter().any(|e| matches!(
            e,
            GameEvent::AttackLanded { target, .. } | GameEvent::AttackMissed { target, .. }
            if *target == id
        )));
        assert!(sim.arena.actors.get(id).unwrap().is_peaceful);
        if has_msg(&ev, "You swap places with") {
            swapped += 1;
            assert_eq!(hero_coord(&sim), gc);
        } else {
            assert!(has_msg(&ev, "is in the way!"), "{ev:?}");
            stopped += 1;
        }
    }
    assert!(
        swapped > 0 && stopped > 0,
        "swapped {swapped}, stopped {stopped}"
    );
}

/// The explicit attack action still attacks a peaceful deliberately.
#[test]
fn test_explicit_attack_on_peaceful_still_attacks() {
    let mut sim = sim_with(RaceId::Human, Alignment::Neutral, RoleId::Valkyrie, 19);
    let c = hero_coord(&sim).step(Direction::East).unwrap();
    let id = place_east(
        &mut sim,
        create_monster_record(MonsterSpeciesId::Watchman, c),
    );
    let ev = sim.step_player_action(ActionAst::MeleeAttack(c));
    assert!(ev.iter().any(|e| matches!(
        e,
        GameEvent::AttackLanded { attacker, target, .. } | GameEvent::AttackMissed { attacker, target }
        if *attacker == sim.player_id && *target == id
    )));
}

/// New `SimulationWorld::hero_race` field: absent in old saves -> Human.
#[test]
fn test_serde_default_hero_race() {
    let sim = sim_with(RaceId::Gnome, Alignment::Neutral, RoleId::Wizard, 20);
    assert_eq!(sim.hero_race, RaceId::Gnome);
    let mut v = serde_json::to_value(&sim).unwrap();
    v.as_object_mut().unwrap().remove("hero_race");
    let restored: SimulationWorld = serde_json::from_value(v).unwrap();
    assert_eq!(restored.hero_race, RaceId::Human);
}

// ---------------------------------------------------------------------------
// D2 final fix wave: the Sanctum's high priest of Moloch.
// ---------------------------------------------------------------------------

/// C `intemple` (priest.c:449-456): the Sanctum priest is created peaceful by
/// `priestini` but turns hostile ("Infidel, you have entered Moloch's
/// Sanctum!" / "Be gone!") when the hero enters; the sim does it on arrival.
#[test]
fn test_sanctum_priest_is_hostile_and_attacks() {
    let mut sim = SimulationWorld::new_with_seed(77);
    sim.current_branch = BranchId::Gehennom;
    sim.depth = netrust_sim::actions::stairs::SANCTUM_DEPTH;
    let pid = sim.player_id;
    sim.arena.actors.retain(|id, _| id == pid);
    let events = sim.unpack_or_generate_level(
        BranchId::Gehennom,
        netrust_sim::actions::stairs::SANCTUM_DEPTH,
    );
    assert!(has_msg(
        &events,
        "Infidel, you have entered Moloch's Sanctum!"
    ));
    assert!(has_msg(&events, "Be gone!"));
    let (priest, pcoord) = sim
        .arena
        .actors
        .iter()
        .find(|(_, a)| a.name == "priest")
        .map(|(id, a)| (id, a.coord))
        .expect("Sanctum priest spawned");
    assert!(!sim.arena.actors.get(priest).unwrap().is_peaceful);

    // Put the hero next to the priest with a huge HP pool.
    let spot = pcoord
        .neighbors()
        .into_iter()
        .find(|&c| sim.level.is_passable(c) && sim.actor_at(c).is_none())
        .expect("free square next to the priest");
    {
        let p = sim.arena.actors.get_mut(pid).unwrap();
        p.coord = spot;
        p.hp = 5000;
        p.max_hp = 5000;
    }
    let mut attacked = false;
    for _ in 0..30 {
        let ev = sim.step_player_action(ActionAst::Wait);
        attacked |= ev.iter().any(|e| matches!(
            e,
            GameEvent::AttackLanded { attacker, target, .. } | GameEvent::AttackMissed { attacker, target }
            if *attacker == priest && *target == pid
        ));
        if attacked {
            break;
        }
        // Keep the priest adjacent if it wandered.
        let pc = sim.arena.actors.get(priest).unwrap().coord;
        if pc.chebyshev_distance(spot) != 1 {
            sim.arena.actors.get_mut(priest).unwrap().coord = pcoord;
        }
        sim.arena.actors.get_mut(pid).unwrap().coord = spot;
    }
    assert!(attacked, "the hostile Sanctum priest attacks the hero");
}
