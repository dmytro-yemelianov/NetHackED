//! Monster AI, Dijkstra pathfinding gradient stepping, Elbereth fear warding,
//! and tactical monster abilities (Dragon Breath, Gaze, Spellcasting).

use netrust_arena::ActorId;
use netrust_core::pathfinding::DijkstraField;
use netrust_data::AiBehavior;
use netrust_i18n::Messages;
use netrust_types::{
    Attack, AttackType, BreathType, Buc, Coord, DamageType, GazeEffect, GazeType, MonsterAbility,
    MonsterSpell, Tile, COLNO, ROWNO,
};
use rand::Rng;

use crate::events::GameEvent;
use crate::world::SimulationWorld;

/// C `BOLT_LIM` (hack.h): maximum `distmin` for a lined-up ranged attack.
const BOLT_LIM: usize = 8;

impl SimulationWorld {
    fn is_line_clear(&self, from: Coord, to: Coord) -> bool {
        let dx = to.x as isize - from.x as isize;
        let dy = to.y as isize - from.y as isize;
        let steps = dx.abs().max(dy.abs());
        if steps <= 1 {
            return true;
        }
        if dx != 0 && dy != 0 && dx.abs() != dy.abs() {
            return false;
        }
        let step_x = dx.signum();
        let step_y = dy.signum();
        let mut cx = from.x as isize + step_x;
        let mut cy = from.y as isize + step_y;
        while cx != to.x as isize || cy != to.y as isize {
            if cx < 0 || cy < 0 || cx >= COLNO as isize || cy >= ROWNO as isize {
                return false;
            }
            let c = Coord::new_unchecked(cx as usize, cy as usize);
            if !self.level.is_passable(c) {
                return false;
            }
            cx += step_x;
            cy += step_y;
        }
        true
    }

    pub(crate) fn step_monsters(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let player_coord = self
            .arena
            .actors
            .get(self.player_id)
            .filter(|p| !p.is_dead)
            .map(|p| p.coord);
        if let Some(pc) = player_coord {
            let dijkstra = DijkstraField::compute(pc, |c| self.level.is_passable(c));
            let player_engraving = self.level.get_engraving(pc).cloned();

            let mon_ids: Vec<ActorId> = self
                .arena
                .actors
                .iter()
                .filter(|(_, a)| !a.is_player && !a.is_dead)
                .map(|(id, _)| id)
                .collect();

            for mon_id in mon_ids {
                let Some(mon) = self.arena.actors.get(mon_id).cloned() else {
                    continue;
                };
                if mon.is_dead {
                    continue;
                }

                // Companion Pet AI
                if mon.is_tame {
                    let adjacent_hostile = mon.coord.neighbors().into_iter().find_map(|adj| {
                        if let Some(other_id) = self.actor_at(adj) {
                            if other_id != self.player_id
                                && !self
                                    .arena
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

                    // Tactical Co-op Priority: check if hostile directly threatens hero
                    let hostile_threatening_hero = pc.neighbors().into_iter().find_map(|adj| {
                        if let Some(other_id) = self.actor_at(adj) {
                            if other_id != self.player_id
                                && !self
                                    .arena
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

                    let goal = netrust_core::choose_pet_goal(
                        hostile_threatening_hero.or(adjacent_hostile),
                        None,
                    );

                    match goal {
                        netrust_core::PetGoal::AttackHostile(target_enemy) => {
                            let enemy_coord = self
                                .arena
                                .actors
                                .get(target_enemy)
                                .map(|a| a.coord)
                                .unwrap_or(pc);
                            if mon.coord.chebyshev_distance(enemy_coord) <= 1 {
                                let enemy_name = self
                                    .arena
                                    .actors
                                    .get(target_enemy)
                                    .map(|a| a.name.clone())
                                    .unwrap_or_else(|| "monster".into());
                                if hostile_threatening_hero == Some(target_enemy) {
                                    events.push(GameEvent::LogMessage {
                                        text: Messages::pet_defends_hero(
                                            &mon.name,
                                            &enemy_name,
                                            self.locale,
                                        ),
                                    });
                                }
                                let combat_events = self.resolve_combat(mon_id, target_enemy);
                                events.extend(combat_events);
                            } else {
                                let threat_dijkstra = DijkstraField::compute(enemy_coord, |c| {
                                    self.level.is_passable(c)
                                });
                                if let Some(step_c) = threat_dijkstra.steepest_descent(mon.coord) {
                                    let floor_items = self.arena.items_at_floor(step_c);
                                    let bucs: Vec<Buc> = floor_items
                                        .iter()
                                        .filter_map(|&id| self.arena.items.get(id).map(|it| it.buc))
                                        .collect();
                                    if !netrust_core::pet_tile_steppable(&bucs) {
                                        events.push(GameEvent::LogMessage {
                                            text: Messages::pet_whimpers_at_cursed(
                                                &mon.name,
                                                self.locale,
                                            ),
                                        });
                                    } else if self.level.is_passable(step_c)
                                        && self.actor_at(step_c).is_none()
                                    {
                                        let from = mon.coord;
                                        if let Some(m) = self.arena.actors.get_mut(mon_id) {
                                            m.coord = step_c;
                                        }
                                        events.push(GameEvent::ActorMoved {
                                            actor: mon_id,
                                            from,
                                            to: step_c,
                                        });
                                    }
                                }
                            }
                        }
                        _ => {
                            if mon.coord.chebyshev_distance(pc) > 2 {
                                if let Some(next_c) = dijkstra.steepest_descent(mon.coord) {
                                    let floor_items = self.arena.items_at_floor(next_c);
                                    let bucs: Vec<Buc> = floor_items
                                        .iter()
                                        .filter_map(|&id| self.arena.items.get(id).map(|it| it.buc))
                                        .collect();
                                    if !netrust_core::pet_tile_steppable(&bucs) {
                                        events.push(GameEvent::LogMessage {
                                            text: Messages::pet_whimpers_at_cursed(
                                                &mon.name,
                                                self.locale,
                                            ),
                                        });
                                    } else if self.level.is_passable(next_c)
                                        && self.actor_at(next_c).is_none()
                                    {
                                        let from = mon.coord;
                                        if let Some(m) = self.arena.actors.get_mut(mon_id) {
                                            m.coord = next_c;
                                        }
                                        events.push(GameEvent::ActorMoved {
                                            actor: mon_id,
                                            from,
                                            to: next_c,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    continue;
                }

                let archetype = netrust_data::monster_archetype_by_name(&mon.name);

                if mon.is_peaceful {
                    // Peaceful monsters do not attack or approach the hero.
                    // Stationary monsters (shopkeepers, priests, watchmen, quest leaders/guardians) stay put.
                    let is_stationary = archetype
                        .map(|a| a.ai_behavior == AiBehavior::Stationary)
                        .unwrap_or(false)
                        || mon.name == "shopkeeper"
                        || mon.name == "priest";
                    if !is_stationary {
                        let neighbors = mon.coord.neighbors();
                        let passable_neighbors: Vec<Coord> = neighbors
                            .into_iter()
                            .filter(|&c| {
                                c != pc && self.level.is_passable(c) && self.actor_at(c).is_none()
                            })
                            .collect();
                        if !passable_neighbors.is_empty() {
                            let idx = self.rng.random_range(0..passable_neighbors.len());
                            let next_c = passable_neighbors[idx];
                            let from = mon.coord;
                            if let Some(m) = self.arena.actors.get_mut(mon_id) {
                                m.coord = next_c;
                            }
                            events.push(GameEvent::ActorMoved {
                                actor: mon_id,
                                from,
                                to: next_c,
                            });
                        }
                    }
                    continue;
                }
                // An ability acts only when the C attack list has the matching
                // attack type (actors without an archetype keep their abilities).
                let has_attack = |at: AttackType| {
                    archetype.is_none_or(|arch| arch.attacks.iter().any(|a| a.at == at))
                };
                let mut acted_special = false;
                // C mattacku AT_BREA (mhitu.c:873): ranged only (`range2`), via breamm.
                if let Some(breath) = archetype
                    .and_then(|arch| arch.attacks.iter().find(|a| a.at == AttackType::Breath))
                {
                    let dist = mon.coord.chebyshev_distance(pc);
                    // m_lined_up / linedup (mthrowu.c:1314): straight or diagonal,
                    // distmin < BOLT_LIM, no blocking terrain; not adjacent.
                    if dist > 1 && dist < BOLT_LIM && self.is_line_clear(mon.coord, pc) {
                        // breamm (mthrowu.c:1117): `!mspec_used && rn2(3)`;
                        // `rn2(3)` is drawn only when the cooldown is over.
                        if mon.mspec_used == 0 && self.rng.random_range(0..3u32) != 0 {
                            events.extend(self.monster_breathes(&mon.name, breath, pc));
                            acted_special = true;
                            // mthrowu.c:1131-1132 (target is the hero):
                            // `if (!rn2(3)) mspec_used = 8 + rn2(18)` (8..=25).
                            if self.rng.random_range(0..3u32) == 0 {
                                let cooldown = 8 + self.rng.random_range(0..18u8);
                                if let Some(m) = self.arena.actors.get_mut(mon_id) {
                                    m.mspec_used = cooldown;
                                }
                            }
                        }
                    }
                }
                for ability in &mon.abilities {
                    if acted_special {
                        break;
                    }
                    match *ability {
                        MonsterAbility::Gaze { gaze } => {
                            if has_attack(AttackType::Gaze)
                                && mon.coord.chebyshev_distance(pc) <= 4
                                && self.is_line_clear(mon.coord, pc)
                            {
                                if let Some(player) = self.arena.actors.get(self.player_id).cloned()
                                {
                                    let gaze_effect = netrust_core::resolve_gaze(
                                        gaze,
                                        player.intrinsics.reflection,
                                        false,
                                    );
                                    match gaze_effect {
                                        GazeEffect::ReflectedToAttacker => {
                                            events.push(GameEvent::LogMessage {
                                                text: Messages::gaze_reflected(
                                                    &mon.name,
                                                    self.locale,
                                                ),
                                            });
                                            if gaze == GazeType::Petrification {
                                                if let Some(m) = self.arena.actors.get_mut(mon_id) {
                                                    m.hp = 0;
                                                    m.is_dead = true;
                                                }
                                            } else if gaze == GazeType::Paralysis {
                                                if let Some(m) = self.arena.actors.get_mut(mon_id) {
                                                    m.hp = m.hp.saturating_sub(10);
                                                    if m.hp == 0 {
                                                        m.is_dead = true;
                                                    }
                                                }
                                            }
                                        }
                                        GazeEffect::BlindImmune => {}
                                        GazeEffect::Afflicted(g) => {
                                            events.push(GameEvent::LogMessage {
                                                text: Messages::gaze_afflicted(
                                                    &mon.name,
                                                    &format!("{g:?}"),
                                                    self.locale,
                                                ),
                                            });
                                            let dmg =
                                                if g == GazeType::Petrification { 30 } else { 8 };
                                            if let Some(p) =
                                                self.arena.actors.get_mut(self.player_id)
                                            {
                                                p.hp = p.hp.saturating_sub(dmg);
                                                if p.hp == 0 {
                                                    p.is_dead = true;
                                                }
                                            }
                                        }
                                    }
                                    acted_special = true;
                                    break;
                                }
                            }
                        }
                        // Breath is driven by the archetype's AT_BREA entry above
                        // (C dice); a Breath ability alone (e.g. on an old save) is
                        // not a C attack and does nothing.
                        MonsterAbility::Breath { .. } => {}
                        MonsterAbility::Spellcaster {
                            spell,
                            cooldown_turns,
                        } => {
                            if has_attack(AttackType::Magic)
                                && self.scheduler.turn % (cooldown_turns as u64) == 0
                                && mon.coord.chebyshev_distance(pc) <= 6
                            {
                                match spell {
                                    MonsterSpell::SummonMonsters => {
                                        let cur_count = self
                                            .arena
                                            .actors
                                            .iter()
                                            .filter(|(_, a)| !a.is_player && !a.is_dead)
                                            .count();
                                        let spawn_count =
                                            netrust_core::calculate_summon_count(cur_count, 30, 2);
                                        let mut spawned = 0;
                                        for neighbor in mon.coord.neighbors() {
                                            if spawned >= spawn_count {
                                                break;
                                            }
                                            if self.level.is_passable(neighbor)
                                                && self.actor_at(neighbor).is_none()
                                            {
                                                let skeleton = netrust_data::create_monster_record(
                                                    netrust_data::MonsterSpeciesId::Skeleton,
                                                    neighbor,
                                                );
                                                let arch = netrust_data::get_monster_species(
                                                    netrust_data::MonsterSpeciesId::Skeleton,
                                                );
                                                if !netrust_core::genocide::is_genocided(
                                                    &self.genocide_registry,
                                                    arch.name,
                                                    arch.glyph,
                                                ) {
                                                    self.arena.spawn_actor(skeleton);
                                                    spawned += 1;
                                                }
                                            }
                                        }
                                        if spawned > 0 {
                                            events.push(GameEvent::LogMessage {
                                                text: Messages::monster_summon_incantation(
                                                    &mon.name,
                                                    spawned,
                                                    self.locale,
                                                ),
                                            });
                                            acted_special = true;
                                            break;
                                        }
                                    }
                                    MonsterSpell::CurseItems => {
                                        let carried = self.arena.items_carried_by(self.player_id);
                                        for item_id in carried {
                                            if let Some(item) = self.arena.items.get_mut(item_id) {
                                                if item.buc != Buc::Cursed {
                                                    item.buc = Buc::Cursed;
                                                    events.push(GameEvent::LogMessage {
                                                        text: Messages::monster_curse_item(
                                                            &item.name,
                                                            self.locale,
                                                        ),
                                                    });
                                                    acted_special = true;
                                                    break;
                                                }
                                            }
                                        }
                                        if acted_special {
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }

                if acted_special {
                    continue;
                }

                if mon.coord.chebyshev_distance(pc) == 1 {
                    // C onscary (monmove.c:240-302): `@`-class, unique, shopkeeper,
                    // blind and peaceful monsters ignore a written Elbereth.
                    let repelled = self.elbereth_scares(&mon, player_engraving.as_ref());

                    if repelled {
                        // Monster repelled by Elbereth! Cannot attack, forced to retreat!
                        events.push(GameEvent::LogMessage {
                            text: format!(
                                "{} is repelled by the sacred ward of Elbereth and retreats!",
                                mon.name
                            ),
                        });
                        if let Some(flee_c) = dijkstra.steepest_ascent(mon.coord) {
                            if self.level.is_passable(flee_c) && self.actor_at(flee_c).is_none() {
                                let from = mon.coord;
                                if let Some(m) = self.arena.actors.get_mut(mon_id) {
                                    m.coord = flee_c;
                                }
                                events.push(GameEvent::ActorMoved {
                                    actor: mon_id,
                                    from,
                                    to: flee_c,
                                });
                            }
                        }
                    } else {
                        // Monster is adjacent to player -> Melee Attack
                        let combat_events = self.resolve_combat(mon_id, self.player_id);
                        events.extend(combat_events);
                    }
                } else {
                    // Dijkstra metric gradient step: flee if low on HP
                    let should_flee = mon.hp <= (mon.max_hp / 3).max(1);
                    let target_opt = if should_flee {
                        dijkstra.steepest_ascent(mon.coord)
                    } else {
                        dijkstra.steepest_descent(mon.coord)
                    };

                    if let Some(nc) = target_opt {
                        if self.level.is_passable(nc) && self.actor_at(nc).is_none() {
                            let from = mon.coord;
                            if let Some(m) = self.arena.actors.get_mut(mon_id) {
                                m.coord = nc;
                            }
                            events.push(GameEvent::ActorMoved {
                                actor: mon_id,
                                from,
                                to: nc,
                            });
                        }
                    }
                }
            }
        }
        events
    }

    /// A monster breathes its AT_BREA attack at the hero (C `breamm`
    /// `mthrowu.c:1093` -> `buzz` `zap.c:4957` -> `zhitu` `zap.c:4406`).
    ///
    /// RNG draws after the caller's `rn2(3)` gate, in C order:
    /// 1. `zap_hit(u.uac, 0)` (`zap.c:4962`): `rn2(20)`, then `rnd(10)` if it
    ///    was 0, else `rnd(-u.uac)` for `AC_VALUE` when `u.uac < 0`.
    /// 2. Not reflected: `d(nd, 6)` with `nd = damn` (`zap.c:4422`/`:4441`;
    ///    `n` draws `rnd(6)`), drawn even when the hero resists.
    ///
    /// The `mspec_used` cooldown is applied by the caller (`step_monsters`).
    /// Not modelled (documented): the `rn1(7, 7)` beam range, bounces,
    /// item destruction, and the reflected beam's path
    /// back (it deals no damage here; every breather resists its own element).
    pub(crate) fn monster_breathes(
        &mut self,
        mon_name: &str,
        attack: &Attack,
        pc: Coord,
    ) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let breath = match attack.ad {
            DamageType::Fire => BreathType::Fire,
            DamageType::Cold => BreathType::Cold,
            _ => return events,
        };
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };
        let hero_ac = self.defender_ac(self.player_id, &player);
        let chance = self.rng.random_range(0..20u32);
        let (rnd10, ac_roll) = if chance == 0 {
            (self.rng.random_range(1..=10u32), 1)
        } else if hero_ac < 0 {
            (1, self.rng.random_range(1..=hero_ac.unsigned_abs()))
        } else {
            (1, 1)
        };
        if !netrust_core::combat::zap_hit(hero_ac, chance, rnd10, ac_roll) {
            events.push(GameEvent::LogMessage {
                text: Messages::breath_misses(&format!("{breath:?}"), self.locale),
            });
            return events;
        }
        if player.intrinsics.reflection {
            events.push(GameEvent::LogMessage {
                text: Messages::breath_reflected(mon_name, self.locale),
            });
            return events;
        }
        let rolls: Vec<u32> = (0..attack.n)
            .map(|_| self.rng.random_range(1..=6u32))
            .collect();
        let raw_damage =
            netrust_core::combat::monster_attack_damage(&Attack { d: 6, ..*attack }, &rolls);
        let (dmg, _) = netrust_core::resolve_breath_damage(raw_damage, breath, &player.intrinsics);
        if dmg == 0 {
            events.push(GameEvent::LogMessage {
                text: Messages::breath_absorbed(&format!("{breath:?}"), self.locale),
            });
        } else {
            events.push(GameEvent::LogMessage {
                text: Messages::dragon_breath(mon_name, &format!("{breath:?}"), dmg, self.locale),
            });
            if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                p.hp = p.hp.saturating_sub(dmg);
                if p.hp == 0 {
                    p.is_dead = true;
                }
            }
            if breath == BreathType::Cold
                && matches!(
                    self.level.get_tile(pc),
                    Tile::Pool { frozen: false } | Tile::Moat
                )
            {
                self.level.set_tile(pc, Tile::Pool { frozen: true });
                events.push(GameEvent::LogMessage {
                    text: Messages::pool_frozen(self.locale).into(),
                });
            }
        }
        // C zhitu ZT_FIRE (zap.c:4432): burn_away_slime() runs after the
        // Fire_resistance check, whether or not the hero resisted.
        if breath == BreathType::Fire && self.hero.afflictions.sliming.is_some() {
            netrust_core::afflictions::cure_sliming(&mut self.hero);
            events.push(GameEvent::LogMessage {
                text: Messages::slime_burned(self.locale).to_string(),
            });
        }
        events
    }

    /// Feeds a companion pet, increasing loyalty and potentially promoting its species.
    pub fn feed_companion_pet(&mut self, pet_id: ActorId, nutrition: u32) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(pet) = self.arena.actors.get_mut(pet_id) else {
            return events;
        };
        if !pet.is_tame {
            return events;
        }

        let (new_tameness, is_tame) =
            netrust_core::pet::feed_pet(pet.tameness, pet.is_tame, nutrition);
        pet.tameness = new_tameness;
        pet.is_tame = is_tame;
        pet.hp = (pet.hp + nutrition / 20).min(pet.max_hp);

        // Check for level advancement and promotion
        let exp_gain = nutrition / 50 + 1;
        let new_level = pet.level + exp_gain;
        pet.level = new_level;

        let cur_tier = if pet.name.contains("little dog") {
            Some(netrust_core::PetSpeciesTier::LittleDog)
        } else if pet.name.contains("large dog") {
            Some(netrust_core::PetSpeciesTier::LargeDog)
        } else if pet.name.contains("dog") {
            Some(netrust_core::PetSpeciesTier::Dog)
        } else if pet.name.contains("kitten") {
            Some(netrust_core::PetSpeciesTier::Kitten)
        } else if pet.name.contains("large cat") {
            Some(netrust_core::PetSpeciesTier::LargeCat)
        } else if pet.name.contains("housecat") {
            Some(netrust_core::PetSpeciesTier::Housecat)
        } else {
            None
        };

        if let Some(tier) = cur_tier {
            let promoted = netrust_core::promote_pet(tier, pet.level);
            if promoted != tier {
                let old_name = pet.name.clone();
                let (new_name, new_max_hp) = match promoted {
                    netrust_core::PetSpeciesTier::Dog => ("dog", 24),
                    netrust_core::PetSpeciesTier::LargeDog => ("large dog", 45),
                    netrust_core::PetSpeciesTier::Housecat => ("housecat", 20),
                    netrust_core::PetSpeciesTier::LargeCat => ("large cat", 40),
                    _ => (old_name.as_str(), pet.max_hp),
                };
                pet.name = new_name.into();
                pet.max_hp = new_max_hp;
                pet.hp = new_max_hp;
                events.push(GameEvent::LogMessage {
                    text: netrust_i18n::Messages::pet_grows(&old_name, new_name, self.locale),
                });
            }
        }

        events
    }
}
