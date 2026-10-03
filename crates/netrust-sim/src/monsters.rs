//! Monster AI, Dijkstra pathfinding gradient stepping, Elbereth fear warding,
//! and tactical monster abilities (Dragon Breath, Gaze, Spellcasting).

use netrust_arena::ActorId;
use netrust_core::pathfinding::DijkstraField;
use netrust_i18n::Messages;
use netrust_types::{
    Alignment, Buc, Coord, GazeEffect, GazeType, MonsterAbility, MonsterSpell, Tile, COLNO, ROWNO,
};

use crate::events::GameEvent;
use crate::world::SimulationWorld;

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
        let player_coord = self.arena.actors.get(self.player_id).filter(|p| !p.is_dead).map(|p| p.coord);
        if let Some(pc) = player_coord {
            let dijkstra = DijkstraField::compute(pc, |c| self.level.is_passable(c));
            let player_engraving = self.level.get_engraving(pc);
            let is_elbereth_active = netrust_core::engraving::is_elbereth_ward_active(player_engraving, false, false);

            let mon_ids: Vec<ActorId> = self.arena.actors.iter()
                .filter(|(_, a)| !a.is_player && !a.is_dead)
                .map(|(id, _)| id)
                .collect();

            for mon_id in mon_ids {
                let Some(mon) = self.arena.actors.get(mon_id).cloned() else { continue; };
                if mon.is_dead { continue; }

                // Companion Pet AI
                if mon.is_tame {
                    let adjacent_hostile = mon.coord.neighbors().into_iter().find_map(|adj| {
                        if let Some(other_id) = self.actor_at(adj) {
                            if other_id != self.player_id && !self.arena.actors.get(other_id).map(|a| a.is_tame).unwrap_or(false) {
                                Some(other_id)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    });

                    if let Some(target_enemy) = adjacent_hostile {
                        let combat_events = self.resolve_combat(mon_id, target_enemy);
                        events.extend(combat_events);
                    } else if mon.coord.chebyshev_distance(pc) > 2 {
                        if let Some(next_c) = dijkstra.steepest_descent(mon.coord) {
                            if self.level.is_passable(next_c) && self.actor_at(next_c).is_none() {
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
                    continue;
                }

                // Peaceful shopkeeper will not attack unless provoked or shoplifted
                if mon.name == "shopkeeper" && mon.alignment == Alignment::Neutral {
                    continue;
                }

                // Special Monster Tactical Abilities
                let mut acted_special = false;
                for ability in &mon.abilities {
                    match *ability {
                        MonsterAbility::Gaze { gaze } => {
                            if mon.coord.chebyshev_distance(pc) <= 4 && self.is_line_clear(mon.coord, pc) {
                                if let Some(player) = self.arena.actors.get(self.player_id).cloned() {
                                    let gaze_effect = netrust_core::resolve_gaze(gaze, player.intrinsics.reflection, false);
                                    match gaze_effect {
                                        GazeEffect::ReflectedToAttacker => {
                                            events.push(GameEvent::LogMessage {
                                                text: Messages::gaze_reflected(&mon.name, self.locale),
                                            });
                                            if gaze == GazeType::Petrification {
                                                if let Some(m) = self.arena.actors.get_mut(mon_id) {
                                                    m.hp = 0;
                                                    m.is_dead = true;
                                                }
                                            } else if gaze == GazeType::Paralysis {
                                                if let Some(m) = self.arena.actors.get_mut(mon_id) {
                                                    m.hp = m.hp.saturating_sub(10);
                                                    if m.hp == 0 { m.is_dead = true; }
                                                }
                                            }
                                        }
                                        GazeEffect::BlindImmune => {}
                                        GazeEffect::Afflicted(g) => {
                                            events.push(GameEvent::LogMessage {
                                                text: Messages::gaze_afflicted(&mon.name, &format!("{:?}", g), self.locale),
                                            });
                                            let dmg = if g == GazeType::Petrification { 30 } else { 8 };
                                            if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                                                p.hp = p.hp.saturating_sub(dmg);
                                                if p.hp == 0 { p.is_dead = true; }
                                            }
                                        }
                                    }
                                    acted_special = true;
                                    break;
                                }
                            }
                        }
                        MonsterAbility::Breath { breath, range, damage_dice } => {
                            if mon.coord.chebyshev_distance(pc) <= range && self.is_line_clear(mon.coord, pc) {
                                if let Some(player) = self.arena.actors.get(self.player_id).cloned() {
                                    let raw_damage = damage_dice.0 * damage_dice.1;
                                    let (dmg, reflected) = netrust_core::resolve_breath_damage(raw_damage, breath, &player.intrinsics);
                                    if reflected {
                                        events.push(GameEvent::LogMessage {
                                            text: Messages::breath_reflected(&mon.name, self.locale),
                                        });
                                        if let Some(m) = self.arena.actors.get_mut(mon_id) {
                                            m.hp = m.hp.saturating_sub(raw_damage);
                                            if m.hp == 0 { m.is_dead = true; }
                                        }
                                    } else if dmg == 0 {
                                        events.push(GameEvent::LogMessage {
                                            text: Messages::breath_absorbed(&format!("{:?}", breath), self.locale),
                                        });
                                    } else {
                                        events.push(GameEvent::LogMessage {
                                            text: Messages::dragon_breath(&mon.name, &format!("{:?}", breath), dmg, self.locale),
                                        });
                                        if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                                            p.hp = p.hp.saturating_sub(dmg);
                                            if p.hp == 0 { p.is_dead = true; }
                                        }
                                        if breath == netrust_types::BreathType::Cold {
                                            if matches!(self.level.get_tile(pc), Tile::Pool { frozen: false } | Tile::Moat) {
                                                self.level.set_tile(pc, Tile::Pool { frozen: true });
                                                events.push(GameEvent::LogMessage {
                                                    text: Messages::pool_frozen(self.locale).into(),
                                                });
                                            }
                                        }
                                    }
                                    acted_special = true;
                                    break;
                                }
                            }
                        }
                        MonsterAbility::Spellcaster { spell, cooldown_turns } => {
                            if self.scheduler.turn % (cooldown_turns as u64) == 0 && mon.coord.chebyshev_distance(pc) <= 6 {
                                match spell {
                                    MonsterSpell::SummonMonsters => {
                                        let cur_count = self.arena.actors.iter().filter(|(_, a)| !a.is_player && !a.is_dead).count();
                                        let spawn_count = netrust_core::calculate_summon_count(cur_count, 30, 2);
                                        let mut spawned = 0;
                                        for neighbor in mon.coord.neighbors() {
                                            if spawned >= spawn_count { break; }
                                            if self.level.is_passable(neighbor) && self.actor_at(neighbor).is_none() {
                                                let skeleton = netrust_data::create_monster_record(
                                                    netrust_data::MonsterSpeciesId::Skeleton,
                                                    neighbor,
                                                );
                                                self.arena.spawn_actor(skeleton);
                                                spawned += 1;
                                            }
                                        }
                                        if spawned > 0 {
                                            events.push(GameEvent::LogMessage {
                                                text: Messages::monster_summon_incantation(&mon.name, spawned, self.locale),
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
                                                        text: Messages::monster_curse_item(&item.name, self.locale),
                                                    });
                                                    acted_special = true;
                                                    break;
                                                }
                                            }
                                        }
                                        if acted_special { break; }
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
                    if is_elbereth_active {
                        // Monster repelled by Elbereth! Cannot attack, forced to retreat!
                        events.push(GameEvent::LogMessage {
                            text: format!("{} is repelled by the sacred ward of Elbereth and retreats!", mon.name),
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
                    // Dijkstra metric gradient step: flee if low on HP or facing active Elbereth
                    let should_flee = is_elbereth_active || (mon.hp <= (mon.max_hp / 3).max(1));
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
}
