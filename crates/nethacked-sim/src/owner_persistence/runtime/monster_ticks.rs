//! Explicit M6-I3 monster phase with movement budgets, AI, and microkernel transport.
use super::*;
use crate::GameEvent;
use nethacked_arena::{
    ActorId, ActorRecord, ActorSpecies, ActorStorage, ItemId, ItemLocation, ItemStorage,
};
use nethacked_core::pathfinding::DijkstraField;
use nethacked_data::monsters::MonsterSpeciesId;
use nethacked_dungeon::DungeonLevel;
use nethacked_i18n::Messages;
use nethacked_kernel::ModuleHandle;
use nethacked_types::{
    Alignment, Attack, AttackType, BranchId, Buc, Coord, GazeEffect, GazeType, GenocideRegistry,
    Locale, MonsterAbility, MonsterSpell, COLNO, ROWNO,
};
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const HOME: &str = "monsters-home-i3-v1";
pub const CALLER: &str = "monsters-caller-i3-v1";
pub const PREVIEW: &str = "monsters_preview_i3";
pub const RUN: &str = "monsters_run_i3";
pub const READS: [&str; 9] = [
    "monsters_clock_read",
    "monsters_objects_read",
    "monsters_actors_read",
    "monsters_hero_read",
    "monsters_equipment_read",
    "monsters_billing_read",
    "monsters_random_read",
    "monsters_geography_read",
    "monsters_session_read",
];
pub const WRITES: [&str; 7] = [
    "monsters_clock_prepare",
    "monsters_objects_prepare",
    "monsters_actors_prepare",
    "monsters_hero_prepare",
    "monsters_equipment_prepare",
    "monsters_billing_prepare",
    "monsters_random_prepare",
];
pub const POLICY: [u8; 32] = [93; 32];

fn native_refusal(_: impl std::fmt::Debug) -> kernel::Error {
    kernel::Error::NativeRefusal
}

fn native_refusal_step(_: &str, _: impl std::fmt::Debug) -> kernel::Error {
    kernel::Error::NativeRefusal
}

#[cfg(test)]
thread_local! {
    static STALE_REFUSALS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static WRONG_RESOURCE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
#[cfg(test)]
pub(super) fn stale_refusals() -> usize {
    STALE_REFUSALS.with(std::cell::Cell::get)
}
#[cfg(test)]
pub(super) fn trigger_wrong_resource(wrong: bool) {
    WRONG_RESOURCE.with(|s| s.set(wrong));
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonsterRequest {
    pub actor: ActorId,
    pub guards: Vec<wand::OwnerWitness>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonsterReply {
    pub actor: ActorId,
    pub outcome: MonsterOutcome,
    pub events: Vec<GameEvent>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActorTickRequest {
    actor: ActorId,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrepareV1 {
    actor: ActorId,
    outcome: MonsterOutcome,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonsterOutcome {
    pub actor_id: ActorId,
    pub movement_spent: u32,
    pub new_coord: Option<Coord>,
    pub actor_hp: Option<u32>,
    pub is_fleeing: Option<bool>,
    pub is_dead: bool,
    pub hero_damage: Option<u32>,
    pub hero_hp: Option<u32>,
    pub hero_dead: bool,
    pub target_actor_id: Option<ActorId>,
    pub target_hp: Option<u32>,
    pub target_dead: Option<bool>,
    pub spawned_actors: Vec<ActorRecord>,
    pub cursed_items: Vec<ItemId>,
    pub stolen_item: Option<ItemId>,
    pub events: Vec<GameEvent>,
}

impl MonsterOutcome {
    pub fn skipped(actor_id: ActorId) -> Self {
        Self {
            actor_id,
            movement_spent: 0,
            new_coord: None,
            actor_hp: None,
            is_fleeing: None,
            is_dead: false,
            hero_damage: None,
            hero_hp: None,
            hero_dead: false,
            target_actor_id: None,
            target_hp: None,
            target_dead: None,
            spawned_actors: Vec::new(),
            cursed_items: Vec::new(),
            stolen_item: None,
            events: Vec::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct ClockFacts {
    pub(super) turn: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct ObjectFacts {
    pub(super) items: ItemStorage,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct ActorFacts {
    pub(super) actors: ActorStorage,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct HeroFacts {
    pub(super) player: ActorId,
    pub(super) alignment_record: i32,
    pub(super) luck: i32,
    pub(super) genocide: GenocideRegistry,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct GeographyFacts {
    #[serde(with = "nethacked_dungeon::compact_level")]
    pub(super) level: DungeonLevel,
    pub(super) branch: BranchId,
    pub(super) depth: usize,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct SessionFacts {
    pub(super) role: String,
    pub(super) locale: Locale,
}

#[derive(Clone, Serialize, Deserialize)]
enum EquipmentReadV1 {
    State(crate::actions::equipment::EquipmentState, Option<ItemId>),
}

#[derive(Clone, Serialize, Deserialize)]
enum BillingReadV1 {
    Unreferenced(bool),
}

fn fact<T: DeserializeOwned + Serialize>(obs: &[Observation], index: usize) -> owners::Result<T> {
    let o = obs
        .iter()
        .find(|o| o.query() == READS[index])
        .ok_or(owners::Error::Schema)?;
    if o.query() != READS[index] {
        return Err(owners::Error::Schema);
    }
    decode(o.bytes(), PAYLOAD)
}

fn outcome(
    actor_id: ActorId,
    obs: &[Observation],
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> owners::Result<MonsterOutcome> {
    let c: ClockFacts = fact(obs, 0)?;
    let o: ObjectFacts = fact(obs, 1)?;
    let a: ActorFacts = fact(obs, 2)?;
    let h: HeroFacts = fact(obs, 3)?;
    let g: GeographyFacts = fact(obs, 7)?;
    let s: SessionFacts = fact(obs, 8)?;
    compute_monster_outcome(actor_id, &c, &o, &a, &h, &g, &s, config, rng)
}

#[allow(clippy::too_many_arguments, clippy::manual_filter)]
pub(super) fn compute_monster_outcome(
    actor_id: ActorId,
    c: &ClockFacts,
    o: &ObjectFacts,
    a: &ActorFacts,
    h: &HeroFacts,
    g: &GeographyFacts,
    s: &SessionFacts,
    config: &OwnerBridgeConfig,
    rng: &mut rand_chacha::ChaCha8Rng,
) -> owners::Result<MonsterOutcome> {
    let Some(hero_actor) = a.actors.get(h.player).cloned() else {
        return Err(owners::Error::Unsupported);
    };
    if hero_actor.is_dead || hero_actor.hp == 0 {
        return Ok(MonsterOutcome::skipped(actor_id));
    }
    let pc = hero_actor.coord;

    let Some(actor) = a.actors.get(actor_id).cloned() else {
        return Err(owners::Error::Unsupported);
    };
    if actor.is_player {
        return Err(owners::Error::Unsupported);
    }
    if actor.is_dead || actor.hp == 0 || actor.movement < nethacked_core::energy::NORMAL_SPEED {
        return Ok(MonsterOutcome::skipped(actor_id));
    }

    let mut out = MonsterOutcome::skipped(actor_id);
    out.movement_spent = nethacked_core::energy::NORMAL_SPEED;

    let dijkstra = DijkstraField::compute(pc, |coord| g.level.is_passable(coord));
    let player_engraving = g.level.get_engraving(pc).cloned();

    let actor_at = |coord: Coord| -> Option<ActorId> {
        a.actors.iter().find_map(|(id, act)| {
            if !act.is_dead && act.coord == coord {
                Some(id)
            } else {
                None
            }
        })
    };

    // 1. Fleeing teleportation
    if actor.is_fleeing
        && rng.random_range(0..40u32) == 0
        && config.ruleset.monster_for_actor(&actor).is_some_and(|def| {
            def.has_flag("tport") && def.id != Some(MonsterSpeciesId::WIZARD_OF_YENDOR)
        })
        && !g.level.no_teleport
    {
        let mut candidates: Vec<Coord> = Vec::new();
        for y in 0..ROWNO {
            for x in 0..COLNO {
                if let Some(coord) = Coord::new(x, y) {
                    if g.level.is_passable(coord) && actor_at(coord).is_none() {
                        candidates.push(coord);
                    }
                }
            }
        }
        if !candidates.is_empty() {
            let idx = rng.random_range(0..candidates.len());
            let target = candidates[idx];
            out.new_coord = Some(target);
            out.events.push(GameEvent::ActorMoved {
                actor: actor_id,
                from: actor.coord,
                to: target,
            });
        }
        return Ok(out);
    }

    // 2. Fleeing courage regain
    let mut is_fleeing = actor.is_fleeing;
    if is_fleeing && actor.hp == actor.max_hp && rng.random_range(0..25u32) == 0 {
        is_fleeing = false;
        out.is_fleeing = Some(false);
    }
    if is_fleeing {
        if let Some(to) = dijkstra.steepest_ascent(actor.coord) {
            if g.level.is_passable(to) && actor_at(to).is_none() {
                out.new_coord = Some(to);
                out.events.push(GameEvent::ActorMoved {
                    actor: actor_id,
                    from: actor.coord,
                    to,
                });
            }
        }
        return Ok(out);
    }

    // 3. Companion Pet AI
    if actor.is_tame {
        let adjacent_hostile = actor.coord.neighbors().into_iter().find_map(|adj| {
            if let Some(other_id) = actor_at(adj) {
                if other_id != h.player
                    && !a
                        .actors
                        .get(other_id)
                        .map(|a| a.is_tame || a.is_peaceful)
                        .unwrap_or(false)
                {
                    Some(other_id)
                } else {
                    None
                }
            } else {
                None
            }
        });

        let hostile_threatening_hero = pc.neighbors().into_iter().find_map(|adj| {
            if let Some(other_id) = actor_at(adj) {
                if other_id != h.player
                    && !a
                        .actors
                        .get(other_id)
                        .map(|a| a.is_tame || a.is_peaceful)
                        .unwrap_or(false)
                {
                    Some(other_id)
                } else {
                    None
                }
            } else {
                None
            }
        });

        let goal =
            nethacked_core::choose_pet_goal(hostile_threatening_hero.or(adjacent_hostile), None);

        match goal {
            nethacked_core::PetGoal::AttackHostile(target_enemy) => {
                let enemy_coord = a.actors.get(target_enemy).map(|a| a.coord).unwrap_or(pc);
                if actor.coord.chebyshev_distance(enemy_coord) <= 1 {
                    let enemy_name = a
                        .actors
                        .get(target_enemy)
                        .map(|a| a.name.clone())
                        .unwrap_or_else(|| "monster".into());
                    if hostile_threatening_hero == Some(target_enemy) {
                        out.events.push(GameEvent::LogMessage {
                            text: Messages::pet_defends_hero(&actor.name, &enemy_name, s.locale),
                        });
                    }
                    let target = a.actors.get(target_enemy).unwrap();
                    let def_ac = target.ac;
                    let m_lev = actor.level as i32;
                    let (tmp, die) = nethacked_core::combat::mhitm_to_hit(m_lev, def_ac, 0);
                    let roll = rng.random_range(1..=die);
                    if nethacked_core::combat::monster_attack_hits(tmp, die, roll) {
                        let dmg = rng.random_range(1..=6);
                        let new_hp = target.hp.saturating_sub(dmg);
                        let lethal = new_hp == 0;
                        out.target_actor_id = Some(target_enemy);
                        out.target_hp = Some(new_hp);
                        out.target_dead = Some(lethal);
                        out.events.push(GameEvent::AttackLanded {
                            attacker: actor_id,
                            target: target_enemy,
                            damage: dmg,
                            lethal,
                        });
                        out.events.push(GameEvent::LogMessage {
                            text: Messages::attack_hit(&actor.name, &enemy_name, dmg, s.locale),
                        });
                        if lethal {
                            out.events.push(GameEvent::LogMessage {
                                text: Messages::killed(&enemy_name, s.locale),
                            });
                        }
                    } else {
                        out.events.push(GameEvent::AttackMissed {
                            attacker: actor_id,
                            target: target_enemy,
                        });
                        out.events.push(GameEvent::LogMessage {
                            text: Messages::attack_miss(&actor.name, &enemy_name, s.locale),
                        });
                    }
                } else {
                    let threat_dijkstra =
                        DijkstraField::compute(enemy_coord, |c| g.level.is_passable(c));
                    if let Some(step_c) = threat_dijkstra.steepest_descent(actor.coord) {
                        let bucs: Vec<Buc> = o
                            .items
                            .iter()
                            .filter_map(|(_, it)| {
                                if it.location == ItemLocation::Floor(step_c) {
                                    Some(it.buc)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        if !nethacked_core::pet_tile_steppable(&bucs) {
                            out.events.push(GameEvent::LogMessage {
                                text: Messages::pet_whimpers_at_cursed(&actor.name, s.locale),
                            });
                        } else if g.level.is_passable(step_c) && actor_at(step_c).is_none() {
                            out.new_coord = Some(step_c);
                            out.events.push(GameEvent::ActorMoved {
                                actor: actor_id,
                                from: actor.coord,
                                to: step_c,
                            });
                        }
                    }
                }
            }
            _ => {
                if actor.coord.chebyshev_distance(pc) > 2 {
                    if let Some(next_c) = dijkstra.steepest_descent(actor.coord) {
                        let bucs: Vec<Buc> = o
                            .items
                            .iter()
                            .filter_map(|(_, it)| {
                                if it.location == ItemLocation::Floor(next_c) {
                                    Some(it.buc)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        if !nethacked_core::pet_tile_steppable(&bucs) {
                            out.events.push(GameEvent::LogMessage {
                                text: Messages::pet_whimpers_at_cursed(&actor.name, s.locale),
                            });
                        } else if g.level.is_passable(next_c) && actor_at(next_c).is_none() {
                            out.new_coord = Some(next_c);
                            out.events.push(GameEvent::ActorMoved {
                                actor: actor_id,
                                from: actor.coord,
                                to: next_c,
                            });
                        }
                    }
                }
            }
        }
        return Ok(out);
    }

    let archetype = config.ruleset.monster_for_actor(&actor);

    // 4. Peaceful Monster AI
    if actor.is_peaceful {
        let is_stationary = archetype
            .map(|a| a.ai_behavior == nethacked_data::AiBehavior::Stationary)
            .unwrap_or(false)
            || actor.name == "shopkeeper"
            || actor.name == "priest";
        if !is_stationary {
            let neighbors = actor.coord.neighbors();
            let passable_neighbors: Vec<Coord> = neighbors
                .into_iter()
                .filter(|&coord| {
                    coord != pc && g.level.is_passable(coord) && actor_at(coord).is_none()
                })
                .collect();
            if !passable_neighbors.is_empty() {
                let idx = rng.random_range(0..passable_neighbors.len());
                let next_c = passable_neighbors[idx];
                out.new_coord = Some(next_c);
                out.events.push(GameEvent::ActorMoved {
                    actor: actor_id,
                    from: actor.coord,
                    to: next_c,
                });
            }
        }
        return Ok(out);
    }

    // 5. Hostile Monster Special Abilities
    let has_attack =
        |at: AttackType| archetype.is_none_or(|arch| arch.attacks.iter().any(|a| a.at == at));
    let mut acted_special = false;

    // Breath (AT_BREA)
    if let Some(breath_attack) =
        archetype.and_then(|arch| arch.attacks.iter().find(|a| a.at == AttackType::Breath))
    {
        let dist = actor.coord.chebyshev_distance(pc);
        if dist <= 8 && is_line_clear(&g.level, actor.coord, pc) {
            let def_ac = hero_actor.ac;
            let m_lev = actor.level as i32;
            let ac_roll = if def_ac < 0 {
                rng.random_range(1..=def_ac.unsigned_abs())
            } else {
                1
            };
            let _ = nethacked_core::combat::monster_to_hit_value(m_lev, def_ac, ac_roll);
            let raw_damage = (0..breath_attack.n)
                .map(|_| rng.random_range(1..=u32::from(breath_attack.d)))
                .sum::<u32>();
            let final_damage = raw_damage.max(1);
            let new_hp = hero_actor.hp.saturating_sub(final_damage);
            out.hero_damage = Some(final_damage);
            out.hero_hp = Some(new_hp);
            out.hero_dead = new_hp == 0;
            out.events.push(GameEvent::LogMessage {
                text: format!("{} breathes!", actor.name),
            });
            out.events.push(GameEvent::AttackLanded {
                attacker: actor_id,
                target: h.player,
                damage: final_damage,
                lethal: out.hero_dead,
            });
            if out.hero_dead {
                out.events.push(GameEvent::LogMessage {
                    text: Messages::killed(&hero_actor.name, s.locale),
                });
            }
            acted_special = true;
        }
    }

    let adjacent = actor.coord.chebyshev_distance(pc) == 1;
    let mut melee_spells: Vec<MonsterSpell> = Vec::new();
    for ability in &actor.abilities {
        if acted_special {
            break;
        }
        match *ability {
            MonsterAbility::Gaze { gaze } => {
                if has_attack(AttackType::Gaze)
                    && actor.coord.chebyshev_distance(pc) <= 4
                    && is_line_clear(&g.level, actor.coord, pc)
                {
                    let gaze_effect =
                        nethacked_core::resolve_gaze(gaze, hero_actor.intrinsics.reflection, false);
                    match gaze_effect {
                        GazeEffect::ReflectedToAttacker => {
                            out.events.push(GameEvent::LogMessage {
                                text: Messages::gaze_reflected(&actor.name, s.locale),
                            });
                            if gaze == GazeType::Petrification {
                                out.actor_hp = Some(0);
                                out.is_dead = true;
                            } else if gaze == GazeType::Paralysis {
                                let hp = actor.hp.saturating_sub(10);
                                out.actor_hp = Some(hp);
                                if hp == 0 {
                                    out.is_dead = true;
                                }
                            }
                            acted_special = true;
                            break;
                        }
                        GazeEffect::BlindImmune => {}
                        GazeEffect::Afflicted(g) => {
                            out.events.push(GameEvent::LogMessage {
                                text: Messages::gaze_afflicted(
                                    &actor.name,
                                    &format!("{g:?}"),
                                    s.locale,
                                ),
                            });
                            let dmg = if g == GazeType::Petrification { 30 } else { 8 };
                            let new_hp = hero_actor.hp.saturating_sub(dmg);
                            out.hero_damage = Some(dmg);
                            out.hero_hp = Some(new_hp);
                            out.hero_dead = new_hp == 0;
                            acted_special = true;
                            break;
                        }
                    }
                }
            }
            MonsterAbility::Breath { .. } => {}
            MonsterAbility::Spellcaster {
                spell,
                cooldown_turns,
            } => {
                if has_attack(AttackType::Magic)
                    && cooldown_turns > 0
                    && c.turn % (cooldown_turns as u64) == 0
                    && actor.coord.chebyshev_distance(pc) <= 6
                {
                    if adjacent {
                        melee_spells.push(spell);
                    } else {
                        match spell {
                            MonsterSpell::SummonMonsters => {
                                let cur_count = a
                                    .actors
                                    .iter()
                                    .filter(|(_, act)| !act.is_player && !act.is_dead)
                                    .count();
                                let spawn_count =
                                    nethacked_core::calculate_summon_count(cur_count, 30, 2);
                                let mut spawned = 0;
                                for neighbor in actor.coord.neighbors() {
                                    if spawned >= spawn_count {
                                        break;
                                    }
                                    if g.level.is_passable(neighbor) && actor_at(neighbor).is_none()
                                    {
                                        if let Some(arch) =
                                            config.ruleset.monster_by_id(MonsterSpeciesId::SKELETON)
                                        {
                                            if !nethacked_core::genocide::is_genocided(
                                                &h.genocide,
                                                &arch.name,
                                                arch.glyph,
                                            ) {
                                                let skeleton = ActorRecord {
                                                    sex: None,
                                                    species: Some(ActorSpecies::Vanilla(
                                                        "skeleton".into(),
                                                    )),
                                                    name: arch.name.clone(),
                                                    coord: neighbor,
                                                    hp: 12,
                                                    max_hp: 12,
                                                    ac: 8,
                                                    level: 2,
                                                    speed: 12,
                                                    movement: 0,
                                                    alignment: Alignment::Neutral,
                                                    intrinsics: arch.intrinsics,
                                                    is_player: false,
                                                    is_unique: false,
                                                    is_dead: false,
                                                    is_tame: false,
                                                    tameness: 0,
                                                    abilities: Vec::new(),
                                                    is_peaceful: false,
                                                    mspec_used: 0,
                                                    malign: 0,
                                                    is_fleeing: false,
                                                    is_cancelled: false,
                                                };
                                                out.spawned_actors.push(skeleton);
                                                spawned += 1;
                                            }
                                        }
                                    }
                                }
                                if spawned > 0 {
                                    out.events.push(GameEvent::LogMessage {
                                        text: Messages::monster_summon_incantation(
                                            &actor.name,
                                            spawned,
                                            s.locale,
                                        ),
                                    });
                                    acted_special = true;
                                    break;
                                }
                            }
                            MonsterSpell::CurseItems => {
                                let carried: Vec<(ItemId, String)> = o
                                    .items
                                    .iter()
                                    .filter_map(|(id, it)| {
                                        if it.location == ItemLocation::CarriedBy(h.player)
                                            && it.buc != Buc::Cursed
                                        {
                                            Some((id, it.name.clone()))
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                                if let Some((target_item, item_name)) = carried.first() {
                                    out.cursed_items.push(*target_item);
                                    out.events.push(GameEvent::LogMessage {
                                        text: Messages::monster_curse_item(item_name, s.locale),
                                    });
                                    acted_special = true;
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    if acted_special {
        return Ok(out);
    }

    // 6. Melee / Movement
    if actor.coord.chebyshev_distance(pc) == 1 {
        let is_s_human = archetype.is_some_and(|arch| arch.is_human);
        let is_shk = config
            .ruleset
            .monster_for_actor(&actor)
            .is_some_and(|a| a.id == Some(MonsterSpeciesId::SHOPKEEPER));
        let exempt = nethacked_core::engraving::onscary_exempt(is_s_human, false, is_shk, false);
        let repelled = nethacked_core::engraving::is_elbereth_ward_active(
            player_engraving.as_ref(),
            actor.intrinsics.blind,
            actor.is_unique,
            actor.is_peaceful,
            exempt,
        );

        if repelled {
            out.events.push(GameEvent::LogMessage {
                text: format!(
                    "{} is repelled by the sacred ward of Elbereth and retreats!",
                    actor.name
                ),
            });
            if let Some(flee_c) = dijkstra.steepest_ascent(actor.coord) {
                if g.level.is_passable(flee_c) && actor_at(flee_c).is_none() {
                    out.new_coord = Some(flee_c);
                    out.events.push(GameEvent::ActorMoved {
                        actor: actor_id,
                        from: actor.coord,
                        to: flee_c,
                    });
                }
            }
        } else {
            // Melee Attack against player
            let fallback_slice = [crate::combat::FALLBACK_ATTACK];
            let attacks: &[Attack] = archetype
                .map(|arch| arch.attacks.as_slice())
                .unwrap_or(&fallback_slice);
            let def_ac = hero_actor.ac;
            let m_lev = actor.level as i32;
            let ac_roll = if def_ac < 0 {
                rng.random_range(1..=def_ac.unsigned_abs())
            } else {
                1
            };
            let hero_tmp = nethacked_core::combat::monster_to_hit_value(m_lev, def_ac, ac_roll);

            let mut current_hero_hp = hero_actor.hp;
            for (i, attack) in attacks.iter().enumerate() {
                if !nethacked_core::combat::is_melee_attack(attack.at) {
                    continue;
                }
                if current_hero_hp == 0 {
                    break;
                }
                let i = i as u32;
                let die = nethacked_core::combat::mattacku_die(i);
                let roll = rng.random_range(1..=die);
                if !nethacked_core::combat::monster_attack_hits(hero_tmp, die, roll) {
                    out.events.push(GameEvent::AttackMissed {
                        attacker: actor_id,
                        target: h.player,
                    });
                    out.events.push(GameEvent::LogMessage {
                        text: Messages::attack_miss(&actor.name, &hero_actor.name, s.locale),
                    });
                    continue;
                }
                let rolls: Vec<u32> = if attack.d == 0 {
                    Vec::new()
                } else {
                    (0..attack.n)
                        .map(|_| rng.random_range(1..=u32::from(attack.d)))
                        .collect()
                };
                let is_resisted =
                    nethacked_core::combat::resisted(attack.ad, &hero_actor.intrinsics);
                let raw = if is_resisted {
                    0
                } else {
                    nethacked_core::combat::monster_attack_damage(attack, &rolls)
                };
                let absorb = if raw > 0 && def_ac < 0 {
                    rng.random_range(1..=def_ac.unsigned_abs())
                } else {
                    1
                };
                let dmg = nethacked_core::combat::monster_hit_damage(
                    attack,
                    &rolls,
                    is_resisted,
                    Some((def_ac, absorb)),
                );
                current_hero_hp = current_hero_hp.saturating_sub(dmg);
                let lethal = current_hero_hp == 0;
                out.hero_damage = Some(dmg);
                out.hero_hp = Some(current_hero_hp);
                out.hero_dead = lethal;
                out.events.push(GameEvent::AttackLanded {
                    attacker: actor_id,
                    target: h.player,
                    damage: dmg,
                    lethal,
                });
                out.events.push(GameEvent::LogMessage {
                    text: Messages::attack_hit(&actor.name, &hero_actor.name, dmg, s.locale),
                });
                if lethal {
                    out.events.push(GameEvent::LogMessage {
                        text: Messages::killed(&hero_actor.name, s.locale),
                    });
                    break;
                }
            }

            if current_hero_hp > 0 {
                for spell in melee_spells {
                    if spell == MonsterSpell::CurseItems {
                        let carried: Vec<ItemId> = o
                            .items
                            .iter()
                            .filter_map(|(id, it)| {
                                if it.location == ItemLocation::CarriedBy(h.player)
                                    && it.buc != Buc::Cursed
                                {
                                    Some(id)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        if !carried.is_empty() {
                            out.cursed_items.push(carried[0]);
                            out.events.push(GameEvent::LogMessage {
                                text: format!("{} points at you, then curses!", actor.name),
                            });
                            break;
                        }
                    }
                }
            }
        }
    } else {
        // Dijkstra gradient step
        let should_flee = actor.hp <= (actor.max_hp / 3).max(1);
        let target_opt = if should_flee {
            dijkstra.steepest_ascent(actor.coord)
        } else {
            dijkstra.steepest_descent(actor.coord)
        };
        if let Some(nc) = target_opt {
            if g.level.is_passable(nc) && actor_at(nc).is_none() {
                out.new_coord = Some(nc);
                out.events.push(GameEvent::ActorMoved {
                    actor: actor_id,
                    from: actor.coord,
                    to: nc,
                });
            }
        }
    }

    Ok(out)
}

fn is_line_clear(level: &DungeonLevel, from: Coord, to: Coord) -> bool {
    let dx = (to.x as isize - from.x as isize).signum();
    let dy = (to.y as isize - from.y as isize).signum();
    let mut curr = from;
    loop {
        let nx = curr.x as isize + dx;
        let ny = curr.y as isize + dy;
        if nx < 0 || ny < 0 {
            return false;
        }
        let Some(next) = Coord::new(nx as usize, ny as usize) else {
            return false;
        };
        if next == to {
            return true;
        }
        if !level.is_passable(next) {
            return false;
        }
        curr = next;
    }
}

trait Domain: Clone + Serialize + 'static {
    const INDEX: usize;
    fn read(
        &self,
        id: ActorId,
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>>;
    fn prepare(
        &self,
        id: ActorId,
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>>;
    fn prepare_intent(
        &self,
        _: PrepareV1,
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
}

struct MonsterCodec<T>(OwnerBridgeConfig, std::marker::PhantomData<T>);
impl<T: Domain> Schema<T> for MonsterCodec<T> {
    fn validate(&self, _: &T) -> owners::Result<()> {
        Ok(())
    }
    fn encode(&self, s: &T) -> owners::Result<Vec<u8>> {
        encode(s, PAYLOAD)
    }
    fn query(
        &self,
        s: &T,
        name: &str,
        request: &[u8],
        obs: &[Observation],
    ) -> owners::Result<Vec<u8>> {
        if name != READS[T::INDEX] {
            return Err(owners::Error::Unsupported);
        }
        let req: ActorTickRequest = decode(request, PAYLOAD)?;
        s.read(req.actor, obs, &self.0)
    }
}

impl<T: Domain> NativeBehavior<T> for MonsterCodec<T> {
    fn prepare(
        &self,
        s: &T,
        name: &str,
        request: &[u8],
        obs: &[Observation],
    ) -> owners::Result<Proposal<T>> {
        if WRITES.get(T::INDEX).copied() != Some(name) {
            return Err(owners::Error::Unsupported);
        }
        if T::INDEX == 6 {
            let req: ActorTickRequest = decode(request, PAYLOAD)?;
            return s.prepare(req.actor, obs, &self.0);
        }
        if !obs.is_empty() {
            return Err(owners::Error::Schema);
        }
        let req: PrepareV1 = decode(request, PAYLOAD)?;
        s.prepare_intent(req, &self.0)
    }
}

fn proposed<T>(
    next: T,
    events: Vec<Vec<u8>>,
    result: &MonsterOutcome,
) -> owners::Result<Proposal<T>> {
    Ok(Proposal {
        next,
        events,
        reply: encode(result, PAYLOAD)?,
    })
}

fn preview(obs: &[Observation]) -> owners::Result<MonsterOutcome> {
    fact(obs, 6)
}

impl Domain for ClockState {
    const INDEX: usize = 0;
    fn read(
        &self,
        _: ActorId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        encode(
            &ClockFacts {
                turn: self.scheduler.turn,
            },
            PAYLOAD,
        )
    }
    fn prepare(
        &self,
        _: ActorId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
    fn prepare_intent(
        &self,
        req: PrepareV1,
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let result = req.outcome;
        let next = self.clone();
        proposed(next, vec![], &result)
    }
}

impl Domain for ObjectsState {
    const INDEX: usize = 1;
    fn read(
        &self,
        _: ActorId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        encode(
            &ObjectFacts {
                items: self.items.clone(),
            },
            PAYLOAD,
        )
    }
    fn prepare(
        &self,
        _: ActorId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
    fn prepare_intent(
        &self,
        req: PrepareV1,
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let result = req.outcome;
        let mut next = self.clone();
        for id in &result.cursed_items {
            if let Some(it) = next.items.get_mut(*id) {
                it.buc = Buc::Cursed;
            }
        }
        proposed(next, vec![], &result)
    }
}

impl Domain for ActorsState {
    const INDEX: usize = 2;
    fn read(
        &self,
        _: ActorId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        encode(
            &ActorFacts {
                actors: self.actors.clone(),
            },
            PAYLOAD,
        )
    }
    fn prepare(
        &self,
        _: ActorId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
    fn prepare_intent(
        &self,
        req: PrepareV1,
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let result = req.outcome;
        let mut next = self.clone();
        if let Some(actor) = next.actors.get_mut(result.actor_id) {
            actor.movement = actor.movement.saturating_sub(result.movement_spent);
            if let Some(coord) = result.new_coord {
                actor.coord = coord;
            }
            if let Some(hp) = result.actor_hp {
                actor.hp = hp;
            }
            if let Some(fleeing) = result.is_fleeing {
                actor.is_fleeing = fleeing;
            }
            if result.is_dead {
                actor.is_dead = true;
            }
        }
        if let Some(target_id) = result.target_actor_id {
            if let Some(target) = next.actors.get_mut(target_id) {
                if let Some(hp) = result.target_hp {
                    target.hp = hp;
                }
                if let Some(dead) = result.target_dead {
                    target.is_dead = dead;
                }
            }
        }
        if let Some(hero_hp) = result.hero_hp {
            if let Some((_, h)) = next.actors.iter_mut().find(|(_, a)| a.is_player) {
                h.hp = hero_hp;
                if result.hero_dead {
                    h.is_dead = true;
                }
            }
        }
        for spawned in &result.spawned_actors {
            next.actors.insert(spawned.clone());
        }
        let events = result
            .events
            .iter()
            .map(|e| encode(e, PAYLOAD))
            .collect::<owners::Result<Vec<_>>>()?;
        proposed(next, events, &result)
    }
}

impl Domain for HeroState {
    const INDEX: usize = 3;
    fn read(
        &self,
        _: ActorId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        encode(
            &HeroFacts {
                player: self.player_id,
                alignment_record: self.alignment_record,
                luck: self.player_luck,
                genocide: self.genocide_registry.clone(),
            },
            PAYLOAD,
        )
    }
    fn prepare(
        &self,
        _: ActorId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
    fn prepare_intent(
        &self,
        req: PrepareV1,
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let result = req.outcome;
        let mut next = self.clone();
        if let Some(hp) = result.hero_hp {
            next.hero.base_hp = hp as i32;
        }
        proposed(next, vec![], &result)
    }
}

impl Domain for EquipmentState {
    const INDEX: usize = 4;
    fn read(
        &self,
        _: ActorId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        encode(
            &EquipmentReadV1::State(self.equipment.clone(), self.wielded_item),
            PAYLOAD,
        )
    }
    fn prepare(
        &self,
        _: ActorId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
    fn prepare_intent(
        &self,
        req: PrepareV1,
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let result = req.outcome;
        let next = self.clone();
        proposed(next, vec![], &result)
    }
}

impl Domain for BillingState {
    const INDEX: usize = 5;
    fn read(
        &self,
        _: ActorId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        encode(
            &BillingReadV1::Unreferenced(self.unpaid_items.is_empty()),
            PAYLOAD,
        )
    }
    fn prepare(
        &self,
        _: ActorId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
    fn prepare_intent(
        &self,
        req: PrepareV1,
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let result = req.outcome;
        let next = self.clone();
        proposed(next, vec![], &result)
    }
}

impl Domain for RandomState {
    const INDEX: usize = 6;
    fn read(
        &self,
        actor: ActorId,
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        encode(&outcome(actor, obs, c, &mut self.rng.clone())?, PAYLOAD)
    }
    fn prepare(
        &self,
        actor: ActorId,
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        let mut next = self.clone();
        let result = outcome(actor, obs, c, &mut next.rng)?;
        proposed(next, vec![], &result)
    }
}

impl Domain for GeographyState {
    const INDEX: usize = 7;
    fn read(
        &self,
        _: ActorId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        encode(
            &GeographyFacts {
                level: self.level.clone(),
                branch: self.current_branch,
                depth: self.depth,
            },
            PAYLOAD,
        )
    }
    fn prepare(
        &self,
        _: ActorId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
}

impl Domain for SessionState {
    const INDEX: usize = 8;
    fn read(
        &self,
        _: ActorId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Vec<u8>> {
        encode(
            &SessionFacts {
                role: self.role_name.clone(),
                locale: self.locale,
            },
            PAYLOAD,
        )
    }
    fn prepare(
        &self,
        _: ActorId,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        Err(owners::Error::Unsupported)
    }
}

pub(super) struct MonsterAccounting(pub(super) OwnerBridgeConfig);
impl Accounting for MonsterAccounting {
    fn validate(&self, p: &[ProposalView], obs: &[Observation]) -> owners::Result<()> {
        if obs.len() != 9 {
            return Err(owners::Error::Schema);
        }
        let random = p
            .iter()
            .find(|x| x.receipt.command == WRITES[6])
            .ok_or(owners::Error::Invariant)?;
        let before: RandomState = decode(&random.before, PAYLOAD)?;
        let result: MonsterOutcome = decode(&random.reply, PAYLOAD)?;
        let mut expected = before.clone();
        let expected_outcome = outcome(result.actor_id, obs, &self.0, &mut expected.rng)?;
        if encode(&expected, PAYLOAD)? != random.proposed
            || encode(&expected_outcome, PAYLOAD)? != random.reply
            || encode(&expected_outcome, PAYLOAD)?
                != fact::<MonsterOutcome>(obs, 6).and_then(|r| encode(&r, PAYLOAD))?
            || !random.events.is_empty()
        {
            return Err(owners::Error::Invariant);
        }
        let has_hero = result.hero_hp.is_some() || result.hero_damage.is_some();
        let has_objects = !result.cursed_items.is_empty() || result.stolen_item.is_some();
        let expected_count = 2 + (has_hero as usize) + (has_objects as usize);
        if p.len() != expected_count {
            return Err(owners::Error::Invariant);
        }
        let req = PrepareV1 {
            actor: result.actor_id,
            outcome: result.clone(),
        };
        for x in p.iter().filter(|x| x.receipt.command != WRITES[6]) {
            macro_rules! check {
                ($state:ty) => {{
                    let old: $state = decode(&x.before, PAYLOAD)?;
                    let expected = old.prepare_intent(req.clone(), &self.0)?;
                    if encode(&expected.next, PAYLOAD)? != x.proposed
                        || expected.events != x.events
                        || expected.reply != x.reply
                    {
                        return Err(owners::Error::Invariant);
                    }
                }};
            }
            match x.receipt.command.as_str() {
                "monsters_actors_prepare" => check!(ActorsState),
                "monsters_hero_prepare" if has_hero => check!(HeroState),
                "monsters_objects_prepare" if has_objects => check!(ObjectsState),
                _ => return Err(owners::Error::Invariant),
            }
        }
        for n in [WRITES[2], WRITES[6]]
            .into_iter()
            .chain(if has_hero { vec![WRITES[3]] } else { vec![] })
            .chain(if has_objects { vec![WRITES[1]] } else { vec![] })
        {
            if p.iter().filter(|x| x.receipt.command == n).count() != 1 {
                return Err(owners::Error::Invariant);
            }
        }
        Ok(())
    }
}

fn identity() -> op::ArtifactIdentity {
    artifact("native-monster-accounting-i3", 93)
}

pub(super) fn capture(
    a: &mut op::AccountingCaptureCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.monster_ticks {
        a.register(identity(), POLICY, Rc::new(MonsterAccounting(c.clone())))?;
    }
    Ok(())
}

pub(super) fn restore(
    a: &mut op::AccountingRestoreCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.monster_ticks {
        let cc = c.clone();
        a.allow(identity(), POLICY, move || {
            Ok(Rc::new(MonsterAccounting(cc.clone())))
        })?;
    }
    Ok(())
}

fn cap(name: &str) -> Capability {
    Capability {
        provider: HOME.into(),
        endpoint: name.into(),
        major: 1,
    }
}

fn manifest(
    id: &str,
    exports: BTreeMap<String, Endpoint>,
    dependencies: Vec<kernel::Dependency>,
) -> Manifest {
    Manifest {
        id: id.into(),
        exports,
        dependencies,
        extension_points: BTreeSet::new(),
        extension: None,
    }
}

fn live_endpoint(access: Access) -> Endpoint {
    Endpoint {
        major: 1,
        access,
        max_request: 16 * 1024,
        max_reply: 16 * 1024,
    }
}

fn scoped(access: Access, resources: Vec<Resource>) -> Authority {
    Authority {
        access,
        scope: Scope::Keys(resources.into_iter().collect()),
    }
}

pub(super) struct MonsterTransport {
    kernel: Kernel,
    caller: ModuleHandle,
    routes: Rc<Routes>,
    resources: Vec<Resource>,
}

impl MonsterTransport {
    pub(super) fn install(
        parts: NativePartitions,
        config: OwnerBridgeConfig,
    ) -> Result<Self, OwnerBridgeError> {
        let host = Host::new(limits())?;
        host.configure_publication(publication_limits())?;
        let mut kernel = Kernel::new(kernel::Limits::default())?;
        let routes = Rc::new(Routes::Initial(RefCell::new(None)));
        let r = routes.clone();
        let c = config.clone();
        let r_preview = routes.clone();
        let c_preview = config.clone();

        let preview_port: NativePort = Rc::new(move |bytes, view| {
            let req: ActorTickRequest = decode(bytes, PAYLOAD).map_err(native_refusal)?;
            let r = r_preview.get()?;
            let read = || -> owners::Result<Vec<Observation>> {
                let mut obs = vec![
                    r.clock.query(READS[0], bytes, &[], view)?,
                    r.objects.query(READS[1], bytes, &[], view)?,
                    r.actors.query(READS[2], bytes, &[], view)?,
                    r.hero.query(READS[3], bytes, &[], view)?,
                    r.equipment.query(READS[4], bytes, &[], view)?,
                    r.billing.query(READS[5], bytes, &[], view)?,
                    r.geography.query(READS[7], bytes, &[], view)?,
                    r.session.query(READS[8], bytes, &[], view)?,
                ];
                obs.push(r.random.query(READS[6], bytes, &obs, view)?);
                Ok(obs)
            };
            let obs = read().map_err(native_refusal)?;
            let _ = outcome(
                req.actor,
                &obs,
                &c_preview,
                &mut rand_chacha::ChaCha8Rng::seed_from_u64(0),
            )
            .map_err(native_refusal)?;
            let guards = obs
                .iter()
                .map(|o| wand::OwnerWitness::from(o.stamp()))
                .collect();
            encode(
                &MonsterRequest {
                    actor: req.actor,
                    guards,
                },
                PAYLOAD,
            )
            .map_err(native_refusal)
        });

        let run_port: NativePort = Rc::new(move |bytes, view| {
            let req: ActorTickRequest = decode(bytes, PAYLOAD).map_err(native_refusal)?;
            let r = r.get()?;
            let read = || -> owners::Result<Vec<Observation>> {
                let mut obs = vec![
                    r.clock.query(READS[0], bytes, &[], view)?,
                    r.objects.query(READS[1], bytes, &[], view)?,
                    r.actors.query(READS[2], bytes, &[], view)?,
                    r.hero.query(READS[3], bytes, &[], view)?,
                    r.equipment.query(READS[4], bytes, &[], view)?,
                    r.billing.query(READS[5], bytes, &[], view)?,
                    r.geography.query(READS[7], bytes, &[], view)?,
                    r.session.query(READS[8], bytes, &[], view)?,
                ];
                obs.push(r.random.query(READS[6], bytes, &obs, view)?);
                Ok(obs)
            };
            let obs = read().map_err(|e| native_refusal_step("read", e))?;

            let result = preview(&obs).map_err(|e| native_refusal_step("preview", e))?;
            let intent = encode(
                &PrepareV1 {
                    actor: req.actor,
                    outcome: result.clone(),
                },
                PAYLOAD,
            )
            .map_err(|e| native_refusal_step("intent encode", e))?;
            let other: Vec<_> = obs
                .iter()
                .filter(|o| o.query() != READS[6])
                .cloned()
                .collect();
            let mut candidates = vec![
                r.actors
                    .prepare(WRITES[2], &intent, vec![], view)
                    .map_err(|e| native_refusal_step("actors.prepare", e))?
                    .erase(),
                r.random
                    .prepare(WRITES[6], bytes, other, view)
                    .map_err(|e| native_refusal_step("random.prepare", e))?
                    .erase(),
            ];
            let mut writes = vec![r.actors.id(), r.random.id()];

            let has_hero = result.hero_hp.is_some() || result.hero_damage.is_some();
            if has_hero {
                candidates.push(
                    r.hero
                        .prepare(WRITES[3], &intent, vec![], view)
                        .map_err(|e| native_refusal_step("hero.prepare", e))?
                        .erase(),
                );
                writes.push(r.hero.id());
            }

            let has_objects = !result.cursed_items.is_empty() || result.stolen_item.is_some();
            if has_objects {
                candidates.push(
                    r.objects
                        .prepare(WRITES[1], &intent, vec![], view)
                        .map_err(|e| native_refusal_step("objects.prepare", e))?
                        .erase(),
                );
                writes.push(r.objects.id());
            }
            let batch = r
                .host
                .batch(
                    candidates,
                    obs,
                    &writes,
                    writes.clone(),
                    POLICY,
                    Rc::new(MonsterAccounting(c.clone())),
                    cap(RUN),
                    view,
                )
                .map_err(|e| native_refusal_step("host.batch", e))?;
            if let Err(e) = batch.commit(view) {
                batch
                    .discard()
                    .map_err(|e| native_refusal_step("batch.discard", e))?;
                return Err(native_refusal_step("batch.commit", e));
            }
            encode(&result, PAYLOAD).map_err(|e| native_refusal_step("encode result", e))
        });

        let exports: BTreeMap<_, _> = READS
            .into_iter()
            .map(|n| (n.into(), live_endpoint(Access::Read)))
            .chain(
                WRITES
                    .into_iter()
                    .map(|n| (n.into(), live_endpoint(Access::Write))),
            )
            .chain([
                (PREVIEW.into(), live_endpoint(Access::Read)),
                (RUN.into(), live_endpoint(Access::Write)),
            ])
            .collect();

        let refuse: NativePort = Rc::new(|_, _| Err(kernel::Error::NativeRefusal));
        let mut ports: BTreeMap<String, NativePort> = exports
            .keys()
            .cloned()
            .map(|n| (n, refuse.clone()))
            .collect();
        ports.insert(PREVIEW.into(), preview_port);
        ports.insert(RUN.into(), run_port);

        let home =
            kernel.register_native(manifest(HOME, exports.clone(), vec![]), &[94; 32], ports)?;
        let deps: Vec<_> = READS
            .into_iter()
            .map(|n| kernel::Dependency {
                capability: cap(n),
                access: Access::Read,
            })
            .chain(WRITES.into_iter().map(|n| kernel::Dependency {
                capability: cap(n),
                access: Access::Write,
            }))
            .chain([
                kernel::Dependency {
                    capability: cap(PREVIEW),
                    access: Access::Read,
                },
                kernel::Dependency {
                    capability: cap(RUN),
                    access: Access::Write,
                },
            ])
            .collect();
        let caller_preview: NativePort = Rc::new(|b, v| v.call_dependency(16, b));
        let caller_run: NativePort = Rc::new(|b, v| v.call_dependency(17, b));
        let caller = kernel.register_native(
            manifest(
                CALLER,
                [
                    (PREVIEW.into(), live_endpoint(Access::Read)),
                    (RUN.into(), live_endpoint(Access::Write)),
                ]
                .into(),
                deps,
            ),
            &[95; 32],
            [(PREVIEW.into(), caller_preview), (RUN.into(), caller_run)].into(),
        )?;

        macro_rules! enroll {
            ($state:expr, $ty:ty, $deps:expr) => {{
                let index = <$ty>::INDEX;
                let mut ports = BTreeMap::from([(
                    READS[index].into(),
                    Port {
                        access: Access::Read,
                        major: 1,
                        dependencies: $deps,
                    },
                )]);
                if let Some(write_name) = WRITES.get(index) {
                    ports.insert(
                        (*write_name).into(),
                        Port {
                            access: Access::Write,
                            major: 1,
                            dependencies: if index == 6 {
                                ports[READS[index]].dependencies.clone()
                            } else {
                                vec![]
                            },
                        },
                    );
                }
                host.enroll(
                    &kernel,
                    OwnerSpec {
                        module: home,
                        schema: [index as u8 + 190; 32],
                        ports,
                    },
                    $state,
                    Backend::Native(Rc::new(MonsterCodec::<$ty>(
                        config.clone(),
                        std::marker::PhantomData,
                    ))),
                )?
            }};
        }

        let clock = enroll!(parts.clock, ClockState, vec![]);
        let objects = enroll!(parts.objects, ObjectsState, vec![]);
        let actors = enroll!(parts.actors, ActorsState, vec![]);
        let hero = enroll!(parts.hero, HeroState, vec![]);
        let equipment = enroll!(parts.equipment, EquipmentState, vec![]);
        let billing = enroll!(parts.billing, BillingState, vec![]);
        let geography = enroll!(parts.geography, GeographyState, vec![]);
        let session = enroll!(parts.session, SessionState, vec![]);
        let mut deps = vec![];
        macro_rules! dependency {
            ($owner:ident, $index:expr) => {
                deps.push(owners::Dependency {
                    owner: $owner.id(),
                    query: READS[$index].into(),
                    schema: $owner.stamp().map_err(native_refusal)?.schema,
                });
            };
        }
        dependency!(clock, 0);
        dependency!(objects, 1);
        dependency!(actors, 2);
        dependency!(hero, 3);
        dependency!(equipment, 4);
        dependency!(billing, 5);
        dependency!(geography, 7);
        dependency!(session, 8);
        let random = enroll!(parts.random, RandomState, deps);
        let r = SourceRoutes {
            host: host.clone(),
            objects,
            hero,
            session,
            billing,
            geography,
            actors,
            equipment,
            clock,
            random,
        };
        let mut resources = vec![];
        macro_rules! resource {
            ($owner:ident, $index:expr) => {
                resources.push(r.$owner.resource(READS[$index]).map_err(native_refusal)?);
                if let Some(write_name) = WRITES.get($index) {
                    resources.push(r.$owner.resource(write_name).map_err(native_refusal)?);
                }
            };
        }
        resource!(clock, 0);
        resource!(objects, 1);
        resource!(actors, 2);
        resource!(hero, 3);
        resource!(equipment, 4);
        resource!(billing, 5);
        resource!(random, 6);
        resource!(geography, 7);
        resource!(session, 8);
        for (slot, n) in READS.into_iter().chain(WRITES).enumerate() {
            let resource = resources
                .iter()
                .find(|res| res.key.ends_with(&format!(".{n}")))
                .ok_or(owners::Error::Schema)?
                .clone();
            kernel.issue_grant(
                caller,
                slot,
                scoped(
                    if slot < 9 {
                        Access::Read
                    } else {
                        Access::Write
                    },
                    vec![resource],
                ),
            )?;
        }
        // Slot 16: PREVIEW
        kernel.issue_grant(caller, 16, scoped(Access::Read, resources.clone()))?;
        // Slot 17: RUN
        resources.push(host.publication_resource());
        kernel.issue_grant(caller, 17, scoped(Access::Write, resources.clone()))?;

        if let Routes::Initial(slot) = routes.as_ref() {
            *slot.borrow_mut() = Some(r);
        }
        Ok(Self {
            kernel,
            caller,
            routes,
            resources,
        })
    }

    pub(super) fn preview(&mut self, actor: ActorId) -> Result<MonsterRequest, OwnerBridgeError> {
        let r = self.routes.get()?;
        let cleanup = r.host.cleanup_identity()?;
        let authority = scoped(Access::Read, self.resources.clone());
        let lease = self
            .kernel
            .capture_pipeline(self.caller, PREVIEW, authority.clone())?;
        let op_id = self.kernel.issue_operation()?;
        let operation =
            self.kernel
                .operation_binding_with_cleanup(op_id, lease, 1, PREVIEW, &[cleanup])?;
        let reply = self.kernel.invoke_operation(
            &operation,
            &encode(&ActorTickRequest { actor }, PAYLOAD)?,
            authority,
        );
        let _ = self.kernel.release_pipeline(lease);
        decode(&reply?, PAYLOAD).map_err(Into::into)
    }

    pub(super) fn execute(
        &mut self,
        request: MonsterRequest,
    ) -> Result<(MonsterReply, NativePartitions), OwnerBridgeError> {
        let r = self.routes.get()?;
        let cleanup = r.host.cleanup_identity()?;
        #[allow(unused_mut)]
        let mut authority = scoped(Access::Write, self.resources.clone());
        #[cfg(test)]
        if WRONG_RESOURCE.with(std::cell::Cell::get) {
            authority.scope = Scope::Keys(BTreeSet::new());
        }
        let lease = self
            .kernel
            .capture_pipeline(self.caller, RUN, authority.clone())?;
        let op_id = self.kernel.issue_operation()?;
        let operation =
            self.kernel
                .operation_binding_with_cleanup(op_id, lease, 1, RUN, &[cleanup])?;
        let reply = self.kernel.invoke_operation(
            &operation,
            &encode(
                &ActorTickRequest {
                    actor: request.actor,
                },
                PAYLOAD,
            )?,
            authority,
        );
        let _ = self.kernel.release_pipeline(lease);
        let outcome: MonsterOutcome = decode(&reply?, PAYLOAD)?;

        let events = outcome.events.clone();

        let updated = NativePartitions {
            geography: decode(&r.geography.inspect()?, PAYLOAD)?,
            objects: decode(&r.objects.inspect()?, PAYLOAD)?,
            actors: decode(&r.actors.inspect()?, PAYLOAD)?,
            hero: decode(&r.hero.inspect()?, PAYLOAD)?,
            equipment: decode(&r.equipment.inspect()?, PAYLOAD)?,
            billing: decode(&r.billing.inspect()?, PAYLOAD)?,
            clock: decode(&r.clock.inspect()?, PAYLOAD)?,
            random: decode(&r.random.inspect()?, PAYLOAD)?,
            session: decode(&r.session.inspect()?, PAYLOAD)?,
        };

        Ok((
            MonsterReply {
                actor: request.actor,
                outcome,
                events,
            },
            updated,
        ))
    }
}

impl OwnerBridge {
    fn monster_session_witnesses(&self) -> Result<Vec<wand::OwnerWitness>, OwnerBridgeError> {
        self.session.with_runtime(&self.credential.borrow(), |b| {
            Ok(bridge_stamps(b)?
                .iter()
                .map(wand::OwnerWitness::from)
                .collect())
        })?
    }

    pub fn prepare_monster_tick(&self, actor: ActorId) -> Result<MonsterRequest, OwnerBridgeError> {
        if !self.config.monster_ticks {
            return Err(owners::Error::Unsupported.into());
        }
        let parts = self.partitions()?;
        let mut transport = MonsterTransport::install(parts, self.config.clone())?;
        let _ = transport.preview(actor)?;
        let guards = self.monster_session_witnesses()?;
        Ok(MonsterRequest { actor, guards })
    }

    pub fn execute_monster_tick(
        &self,
        request: MonsterRequest,
    ) -> Result<MonsterReply, OwnerBridgeError> {
        if !self.config.monster_ticks {
            return Err(owners::Error::Unsupported.into());
        }
        let current_witnesses = self.monster_session_witnesses()?;
        if request.guards != current_witnesses {
            #[cfg(test)]
            STALE_REFUSALS.with(|x| x.set(x.get() + 1));
            return Err(owners::Error::Stale.into());
        }
        let parts = self.partitions()?;
        let mut transport = MonsterTransport::install(parts, self.config.clone())?;
        let (reply, updated) = transport.execute(request)?;
        let source = Self::install(updated, self.config.clone())?;
        let wire = source.capture()?;
        drop(source);
        self.restore(&wire)?;
        Ok(reply)
    }

    pub fn step_monster(&self, actor: ActorId) -> Result<MonsterReply, OwnerBridgeError> {
        let req = self.prepare_monster_tick(actor)?;
        self.execute_monster_tick(req)
    }

    pub fn step_monsters(&self) -> Result<Vec<GameEvent>, OwnerBridgeError> {
        if !self.config.monster_ticks {
            return Err(owners::Error::Unsupported.into());
        }
        let parts = self.partitions()?;
        let player_coord = parts
            .actors
            .actors
            .get(parts.hero.player_id)
            .filter(|p| !p.is_dead)
            .map(|p| p.coord);
        let Some(_pc) = player_coord else {
            return Ok(Vec::new());
        };

        let mon_ids: Vec<ActorId> = parts
            .actors
            .actors
            .iter()
            .filter(|(_, a)| !a.is_player && !a.is_dead)
            .map(|(id, _)| id)
            .collect();

        let mut all_events = Vec::new();
        for mon_id in mon_ids {
            let parts = self.partitions()?;
            let hero_alive = parts
                .actors
                .actors
                .get(parts.hero.player_id)
                .is_some_and(|h| !h.is_dead && h.hp > 0);
            if !hero_alive {
                break;
            }
            let can_move = parts.actors.actors.get(mon_id).is_some_and(|m| {
                !m.is_dead && m.hp > 0 && m.movement >= nethacked_core::energy::NORMAL_SPEED
            });
            if !can_move {
                continue;
            }
            let reply = self.step_monster(mon_id)?;
            all_events.extend(reply.events);
        }
        Ok(all_events)
    }
}
