//! Monster attacks use their NetHack attack lists and dice (D2 Task 4).
//!
//! C references: `mattacku` (mhitu.c:491, loop :768), `hitmu` (mhitu.c:1144,
//! `d(damn, damd)` :1187), `mattackm` (mhitm.c:293, `tmp = find_mac + m_lev`
//! :320, `rnd(20 + i)` :441), `breamm` (mthrowu.c:1093).

use nethacked_arena::{ActorId, ActorRecord, ItemLocation, ItemRecord};
use nethacked_data::{create_monster_record, MonsterSpeciesId};
use nethacked_sim::{ActionAst, Alignment, Coord, GameEvent, Intrinsics, SimulationWorld, Tile};
use nethacked_types::{Buc, ItemClass, SlimingState};

const HERO: Coord = Coord::new_unchecked(10, 10);
const HERO_HP: i32 = 5000;

/// A world with only the hero at `HERO` (AC 10, so no `rnd(-AC)` absorption),
/// a clear room around it and a huge HP pool.
fn arena_world(seed: u64) -> SimulationWorld {
    let mut sim = SimulationWorld::new_with_seed(seed);
    let pid = sim.player_id;
    sim.arena.actors.retain(|id, _| id == pid);
    sim.arena.items.clear();
    for x in 5..=20 {
        for y in 5..=15 {
            sim.level.set_tile(Coord::new_unchecked(x, y), Tile::Room);
        }
    }
    let p = sim.arena.actors.get_mut(pid).unwrap();
    p.coord = HERO;
    p.ac = 10;
    p.hp = HERO_HP as u32;
    p.max_hp = HERO_HP as u32;
    p.intrinsics = Intrinsics::default();
    sim.hero.polymorph = None;
    sim
}

/// Reset the hero's HP and the monster's position/HP before each turn.
fn reset(sim: &mut SimulationWorld, mon: ActorId, at: Coord) {
    let pid = sim.player_id;
    let p = sim.arena.actors.get_mut(pid).unwrap();
    p.hp = HERO_HP as u32;
    p.coord = HERO;
    let m = sim.arena.actors.get_mut(mon).unwrap();
    m.coord = at;
    m.hp = m.max_hp;
}

/// `(landed damage or None for a miss)` for every melee event by `attacker`.
fn attack_events(events: &[GameEvent], attacker: ActorId) -> Vec<Option<u32>> {
    events
        .iter()
        .filter_map(|e| match e {
            GameEvent::AttackLanded {
                attacker: a,
                damage,
                ..
            } if *a == attacker => Some(Some(*damage)),
            GameEvent::AttackMissed { attacker: a, .. } if *a == attacker => Some(None),
            _ => None,
        })
        .collect()
}

/// The hero's displayed HP (the player actor's `hp`).
fn hero_hp(sim: &SimulationWorld) -> i32 {
    sim.arena.actors.get(sim.player_id).unwrap().hp as i32
}

fn spawn(sim: &mut SimulationWorld, id: MonsterSpeciesId, at: Coord) -> ActorId {
    sim.arena.spawn_actor(create_monster_record(id, at))
}

fn east(n: usize) -> Coord {
    Coord::new_unchecked(HERO.x + n, HERO.y)
}

#[test]
fn jackal_bite_is_1d2_one_attack_per_move() {
    let mut sim = arena_world(7);
    let jackal = spawn(&mut sim, MonsterSpeciesId::Jackal, east(1));
    let mut landed = 0;
    let mut seen = [false; 3];
    for _ in 0..200 {
        reset(&mut sim, jackal, east(1));
        let events = sim.step_player_action(ActionAst::Wait);
        for hit in attack_events(&events, jackal).into_iter().flatten() {
            // C jackal: ATTK(AT_BITE, AD_PHYS, 1, 2); hero AC 10, no absorption.
            assert!((1..=2).contains(&hit), "jackal bite dealt {hit}");
            seen[hit as usize] = true;
            landed += 1;
        }
    }
    assert!(landed > 50, "jackal (tmp 20) should land most bites");
    assert!(seen[1] && seen[2], "both faces of the d2 appear");
}

#[test]
fn red_dragon_adjacent_bites_and_claws_in_order() {
    let mut sim = arena_world(11);
    let dragon = spawn(&mut sim, MonsterSpeciesId::RedDragon, east(1));
    let mut groups = 0;
    for _ in 0..60 {
        reset(&mut sim, dragon, east(1));
        let base_before = hero_hp(&sim);
        let events = sim.step_player_action(ActionAst::Wait);
        let atks = attack_events(&events, dragon);
        // C red dragon: Br 6d6 (ranged only, mhitu.c:873), B 3d8, C 1d4, C 1d4.
        // Adjacent: no breath, three melee attacks per move.
        assert_eq!(atks.len() % 3, 0, "bite+claw+claw per move: {atks:?}");
        let mut total = 0;
        for chunk in atks.chunks(3) {
            groups += 1;
            if let Some(b) = chunk[0] {
                assert!((3..=24).contains(&b), "bite 3d8 dealt {b}");
                total += b;
            }
            for c in chunk[1..].iter().flatten() {
                assert!((1..=4).contains(c), "claw 1d4 dealt {c}");
                total += c;
            }
        }
        // All hero damage this turn came from the landed melee hits.
        assert_eq!(base_before - hero_hp(&sim), total as i32);
    }
    assert!(groups > 0, "the dragon attacked at least once");
}

#[test]
fn red_dragon_breathes_6d6_fire_only_at_range_and_resistance_zeroes_it() {
    let mut sim = arena_world(5);
    let dragon = spawn(&mut sim, MonsterSpeciesId::RedDragon, east(3));
    let mut breaths = 0;
    for _ in 0..60 {
        reset(&mut sim, dragon, east(3));
        let before = hero_hp(&sim);
        let events = sim.step_player_action(ActionAst::Wait);
        // At range the bite/claws (range2) never fire.
        assert!(attack_events(&events, dragon).is_empty());
        let dmg = before - hero_hp(&sim);
        if dmg > 0 {
            // C zhitu: d(nd, 6) with nd = damn = 6 (zap.c:4422).
            assert!((6..=36).contains(&dmg), "fire breath 6d6 dealt {dmg}");
            breaths += 1;
        }
    }
    assert!(breaths > 0, "the dragon breathed at least once");

    // Fire resistance zeroes the breath (zap.c ZT_FIRE).
    let pid = sim.player_id;
    sim.arena
        .actors
        .get_mut(pid)
        .unwrap()
        .intrinsics
        .fire_resistance = true;
    let mut resisted = 0;
    for _ in 0..60 {
        reset(&mut sim, dragon, east(3));
        let events = sim.step_player_action(ActionAst::Wait);
        assert_eq!(hero_hp(&sim), HERO_HP);
        if events.iter().any(|e| {
            matches!(e, GameEvent::LogMessage { text } if text.contains("Fire") || text.contains("fire"))
        }) {
            resisted += 1;
        }
    }
    assert!(resisted > 0, "a resisted breath was reported");
}

#[test]
fn cold_touch_is_zeroed_by_cold_resistance() {
    // C master lich: ATTK(AT_TUCH, AD_COLD, 3, 6); mhitm_ad_cold (uhitm.c:2626).
    let mut sim = arena_world(3);
    let lich = spawn(&mut sim, MonsterSpeciesId::Lich, east(1));
    sim.arena.actors.get_mut(lich).unwrap().abilities.clear();
    let mut landed = 0;
    for _ in 0..40 {
        reset(&mut sim, lich, east(1));
        let before = hero_hp(&sim);
        let events = sim.step_player_action(ActionAst::Wait);
        let hits: Vec<u32> = attack_events(&events, lich).into_iter().flatten().collect();
        for h in &hits {
            assert!((3..=18).contains(h), "cold touch 3d6 dealt {h}");
        }
        landed += hits.len();
        assert_eq!(before - hero_hp(&sim), hits.iter().sum::<u32>() as i32);
    }
    assert!(landed > 0);

    let pid = sim.player_id;
    sim.arena
        .actors
        .get_mut(pid)
        .unwrap()
        .intrinsics
        .cold_resistance = true;
    let mut resisted_hits = 0;
    for _ in 0..40 {
        reset(&mut sim, lich, east(1));
        let events = sim.step_player_action(ActionAst::Wait);
        for h in attack_events(&events, lich).into_iter().flatten() {
            assert_eq!(h, 0, "resisted cold touch deals 0");
            resisted_hits += 1;
        }
        assert_eq!(hero_hp(&sim), HERO_HP);
    }
    assert!(resisted_hits > 0);
}

#[test]
fn pet_uses_mhitm_to_hit_without_plus_ten() {
    // little dog m_lev 2 vs goblin AC -1: mhitm tmp = -1 + 2 = 1, never > rnd(20+i).
    // (The mhitu formula would give AC_VALUE(-1) + 10 + 2 = 11 and hit often.)
    let mut sim = arena_world(21);
    let pet_at = Coord::new_unchecked(10, 11);
    let foe_at = east(1);
    let dog = spawn(&mut sim, MonsterSpeciesId::LittleDog, pet_at);
    sim.arena.actors.get_mut(dog).unwrap().is_tame = true;
    let goblin = spawn(&mut sim, MonsterSpeciesId::Goblin, foe_at);
    sim.arena.actors.get_mut(goblin).unwrap().ac = -1;
    let mut misses = 0;
    for _ in 0..40 {
        reset(&mut sim, goblin, foe_at);
        reset(&mut sim, dog, pet_at);
        let events = sim.step_player_action(ActionAst::Wait);
        for a in attack_events(&events, dog) {
            assert_eq!(a, None, "tmp 1 never beats rnd(20)");
            misses += 1;
        }
    }
    assert!(misses > 0, "the pet attacked");

    // AC 20: tmp 22 always hits; damage is the little dog's B 1d6.
    sim.arena.actors.get_mut(goblin).unwrap().ac = 20;
    let mut hits = 0;
    for _ in 0..40 {
        reset(&mut sim, goblin, foe_at);
        reset(&mut sim, dog, pet_at);
        let events = sim.step_player_action(ActionAst::Wait);
        for a in attack_events(&events, dog) {
            let d = a.expect("tmp 22 always hits");
            assert!((1..=6).contains(&d));
            hits += 1;
        }
    }
    assert!(hits > 0);
}

fn named_monster(name: &str, at: Coord) -> ActorRecord {
    ActorRecord {
        name: name.into(),
        coord: at,
        hp: 50,
        max_hp: 50,
        ac: 10,
        level: 0,
        speed: 12,
        alignment: Alignment::Chaotic,
        intrinsics: Intrinsics::default(),
        is_player: false,
        is_unique: false,
        is_dead: false,
        is_tame: false,
        tameness: 0,
        abilities: Vec::new(),
        is_peaceful: false,
        mspec_used: 0,
        malign: 0,
    }
}

/// Max single-hit damage over many turns for a monster with this name.
fn max_hit(name: &str, seed: u64) -> u32 {
    let mut sim = arena_world(seed);
    let mon = sim.arena.spawn_actor(named_monster(name, east(1)));
    let mut max = 0;
    for _ in 0..200 {
        reset(&mut sim, mon, east(1));
        let events = sim.step_player_action(ActionAst::Wait);
        for d in attack_events(&events, mon).into_iter().flatten() {
            max = max.max(d);
        }
    }
    max
}

#[test]
fn hostile_goblin_uses_goblin_weapon_1d4() {
    // "hostile goblin" resolves to the goblin archetype: ATTK(AT_WEAP, AD_PHYS, 1, 4).
    let max = max_hit("hostile goblin", 31);
    assert!((1..=4).contains(&max), "goblin W1d4 max {max}");
}

#[test]
fn unknown_species_falls_back_to_one_d6_attack() {
    // No archetype (renamed / custom actor): a single d(1, 6) attack.
    let max = max_hit("Dummy", 31);
    assert!((5..=6).contains(&max), "fallback d6 max {max}");
}

fn quaff_healing(sim: &mut SimulationWorld) {
    let pid = sim.player_id;
    let potion = sim.arena.spawn_item(ItemRecord {
        name: "potion of healing".into(),
        class: ItemClass::Potion,
        weight: 20,
        buc: Buc::Uncursed,
        is_container: false,
        is_bag_of_holding: false,
        enchantment: 0,
        erosion: 0,
        proofed: false,
        location: ItemLocation::CarriedBy(pid),
        corpse_race: None,
        corpse_age: 0,
        rot_threshold: 0,
        recharged: 0,
    });
    let idx = sim
        .arena
        .items_carried_by(pid)
        .iter()
        .position(|&id| id == potion)
        .unwrap();
    sim.step_player_action(ActionAst::Quaff(idx));
}

/// Wait with `mon` kept at `at` until one of its melee hits lands; returns the
/// total landed melee damage. Asserts every turn's HP loss equals that turn's
/// landed hits (the hero's HP is never reset from a stale copy).
fn wait_for_melee_hit(sim: &mut SimulationWorld, mon: ActorId, at: Coord) -> i32 {
    let mut total = 0;
    for _ in 0..200 {
        sim.arena.actors.get_mut(mon).unwrap().coord = at;
        let before = hero_hp(sim);
        let events = sim.step_player_action(ActionAst::Wait);
        let landed: u32 = attack_events(&events, mon).into_iter().flatten().sum();
        assert_eq!(
            before - hero_hp(sim),
            landed as i32,
            "HP loss must equal the landed melee damage"
        );
        total += landed as i32;
        if landed > 0 {
            return total;
        }
    }
    panic!("no melee hit landed");
}

#[test]
fn healing_persists_through_a_later_melee_hit() {
    // One HP source of truth: the player actor's hp. A melee hit must
    // subtract from the healed value, not reset HP from a stale hero copy.
    let mut sim = arena_world(41);
    let pid = sim.player_id;
    sim.arena.actors.get_mut(pid).unwrap().hp = 100;
    quaff_healing(&mut sim);
    let healed = hero_hp(&sim);
    assert_eq!(healed, 110, "potion of healing restores 10 HP");
    let jackal = spawn(&mut sim, MonsterSpeciesId::Jackal, east(1));
    let dmg = wait_for_melee_hit(&mut sim, jackal, east(1));
    assert_eq!(hero_hp(&sim), healed - dmg);
}

#[test]
fn breath_damage_persists_through_a_later_melee_hit() {
    let mut sim = arena_world(43);
    let dragon = spawn(&mut sim, MonsterSpeciesId::RedDragon, east(3));
    let mut breathed = false;
    for _ in 0..200 {
        sim.arena.actors.get_mut(dragon).unwrap().coord = east(3);
        sim.step_player_action(ActionAst::Wait);
        if hero_hp(&sim) < HERO_HP {
            breathed = true;
            break;
        }
    }
    assert!(breathed, "the dragon's breath hit");
    let after_breath = hero_hp(&sim);
    let dmg = wait_for_melee_hit(&mut sim, dragon, east(1));
    assert_eq!(hero_hp(&sim), after_breath - dmg, "breath damage persists");
}

fn is_breath_message(e: &GameEvent) -> bool {
    matches!(e, GameEvent::LogMessage { text } if text.contains("blast") || text.contains("breath"))
}

#[test]
fn breath_cooldown_keeps_breath_rate_far_below_two_thirds() {
    // C breamm (mthrowu.c:1117-1132): `!mspec_used && rn2(3)` gates the
    // breath; after breathing at the hero, `!rn2(3)` sets
    // `mspec_used = 8 + rn2(18)`, decremented once per turn (mon_regen,
    // monmove.c:311). Without the cooldown a lined-up dragon breathes on
    // 2/3 of its moves.
    let mut sim = arena_world(47);
    let dragon = spawn(&mut sim, MonsterSpeciesId::RedDragon, east(3));
    let turns = 400;
    let mut breaths = 0;
    for _ in 0..turns {
        reset(&mut sim, dragon, east(3));
        let events = sim.step_player_action(ActionAst::Wait);
        breaths += events.iter().filter(|e| is_breath_message(e)).count();
    }
    assert!(breaths > 0, "the dragon breathed");
    let rate = breaths as f64 / turns as f64;
    assert!(rate < 0.3, "breath rate {rate} should be far below 2/3");
}

#[test]
fn fire_breath_burns_away_slime_even_when_resisted() {
    // C zhitu ZT_FIRE (zap.c:4421-4432): burn_away_slime() runs after the
    // Fire_resistance check, whether or not the hero resisted.
    let mut sim = arena_world(53);
    let pid = sim.player_id;
    sim.arena
        .actors
        .get_mut(pid)
        .unwrap()
        .intrinsics
        .fire_resistance = true;
    let dragon = spawn(&mut sim, MonsterSpeciesId::RedDragon, east(3));
    for _ in 0..200 {
        sim.hero.afflictions.sliming = Some(SlimingState {
            turns_remaining: 10,
        });
        reset(&mut sim, dragon, east(3));
        let events = sim.step_player_action(ActionAst::Wait);
        let resisted = events.iter().any(|e| {
            matches!(e, GameEvent::LogMessage { text } if text.contains("engulfed in the blast"))
        });
        if resisted {
            assert!(
                sim.hero.afflictions.sliming.is_none(),
                "a resisted fire breath still burns away the slime"
            );
            return;
        }
    }
    panic!("no resisted fire breath hit the hero");
}

// ---------------------------------------------------------------------------
// D2 final fix wave: gaze/spell turns still resolve melee; breath vs polyform.
// ---------------------------------------------------------------------------

/// Remove every actor except the hero and `keep` (e.g. summoned skeletons).
fn clear_others(sim: &mut SimulationWorld, keep: ActorId) {
    let pid = sim.player_id;
    sim.arena.actors.retain(|id, _| id == pid || id == keep);
}

fn msg_index(events: &[GameEvent], needle: &str) -> Option<usize> {
    events
        .iter()
        .position(|e| matches!(e, GameEvent::LogMessage { text } if text.contains(needle)))
}

fn first_attack_index(events: &[GameEvent], attacker: ActorId) -> Option<usize> {
    events.iter().position(|e| {
        matches!(e, GameEvent::AttackLanded { attacker: a, .. }
            | GameEvent::AttackMissed { attacker: a, .. } if *a == attacker)
    })
}

#[test]
fn adjacent_medusa_gazes_then_resolves_her_melee_attacks() {
    // C dochug: m_respond -> m_respond_medusa gazes (monmove.c:753,
    // mon.c:4109-4118), then mattacku (monmove.c:971) resolves
    // W 2d4, C 1d8, B 1d6 and skips her AT_GAZE slot (mhitu.c:832-836).
    let mut sim = arena_world(29);
    let medusa = spawn(&mut sim, MonsterSpeciesId::Medusa, east(1));
    let mut turns_with_both = 0;
    for _ in 0..30 {
        reset(&mut sim, medusa, east(1));
        let events = sim.step_player_action(ActionAst::Wait);
        let Some(gaze) = msg_index(&events, "gaze") else {
            continue;
        };
        let atks = attack_events(&events, medusa);
        assert_eq!(
            atks.len(),
            3,
            "weapon, claw and bite after the gaze: {atks:?}"
        );
        let first = first_attack_index(&events, medusa).unwrap();
        assert!(gaze < first, "gaze (m_respond) precedes mattacku");
        turns_with_both += 1;
    }
    assert!(
        turns_with_both > 0,
        "Medusa gazed and attacked in the same turn"
    );
}

#[test]
fn adjacent_master_lich_touches_on_a_spell_turn() {
    // C mattacku loop (mhitu.c:768): slot 0 AT_TUCH 3d6 cold resolves
    // before slot 1 AT_MAGC (`castmu`, mhitu.c:926-931) in the same round.
    let mut sim = arena_world(31);
    let lich = spawn(&mut sim, MonsterSpeciesId::Lich, east(1));
    let mut spell_turns = 0;
    for _ in 0..80 {
        clear_others(&mut sim, lich);
        reset(&mut sim, lich, east(1));
        let events = sim.step_player_action(ActionAst::Wait);
        let Some(spell) = msg_index(&events, "incantation") else {
            continue;
        };
        let first = first_attack_index(&events, lich).expect("touch resolved on a spell turn");
        assert!(first < spell, "AT_TUCH (slot 0) before AT_MAGC (slot 1)");
        spell_turns += 1;
    }
    assert!(
        spell_turns > 0,
        "the lich cast at least once while adjacent"
    );
}

#[test]
fn breath_on_a_polymorphed_hero_rehumanizes_instead_of_killing() {
    // C zhitu -> losehp (hack.c:4256): when polymorphed, damage goes to
    // u.mh and u.mh < 1 calls rehumanize() (base u.uhp unchanged).
    let mut sim = arena_world(61);
    let dragon = spawn(&mut sim, MonsterSpeciesId::RedDragon, east(3));
    let pid = sim.player_id;
    for _ in 0..300 {
        reset(&mut sim, dragon, east(3));
        sim.hero.base_hp = 40;
        sim.hero.base_max_hp = 40;
        sim.hero.polymorph = Some(nethacked_types::PolymorphForm {
            monster_id: 1,
            hp: 5,
            max_hp: 20,
            duration: 100,
        });
        {
            let p = sim.arena.actors.get_mut(pid).unwrap();
            p.hp = 5;
            p.max_hp = 20;
        }
        let events = sim.step_player_action(ActionAst::Wait);
        if events
            .iter()
            .any(|e| matches!(e, GameEvent::LogMessage { text } if text.contains("fiery blast")))
        {
            let p = sim.arena.actors.get(pid).unwrap();
            assert!(
                !p.is_dead,
                "6d6 breath on a 5 HP polyform reverts, not kills"
            );
            assert!(sim.hero.polymorph.is_none(), "rehumanized");
            assert_eq!(p.hp, 40, "base HP unchanged after rehumanize");
            return;
        }
    }
    panic!("the dragon never breathed on the hero");
}
