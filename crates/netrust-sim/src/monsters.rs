//! Monster AI, Dijkstra pathfinding gradient stepping, and Elbereth fear warding.

use netrust_arena::ActorId;
use netrust_core::pathfinding::DijkstraField;
use netrust_types::Alignment;

use crate::events::GameEvent;
use crate::world::SimulationWorld;

impl SimulationWorld {
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
                    // 1. Attack adjacent hostile enemies
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
                        // Follow hero using Dijkstra gradient towards hero
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
