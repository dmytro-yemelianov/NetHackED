//! Movement action handling, shopkeeper theft detection, and engraving interaction.

use netrust_core::energy::NORMAL_SPEED;
use netrust_dungeon::RoomType;
use netrust_types::{Coord, Direction, DoorState, Tile};
use rand::Rng;

use crate::events::GameEvent;
use crate::world::SimulationWorld;

impl SimulationWorld {
    /// C `is_safemon(mon)` (display.h:159-161): `flags.safe_dog` (on by default),
    /// a peaceful monster the hero can spot, and the hero not confused,
    /// hallucinating or stunned. "Can spot" is approximated by "hero not blind".
    /// Pets keep the sim's own displacement (handled before this check).
    pub(crate) fn is_safemon(&self, target_id: netrust_arena::ActorId) -> bool {
        let Some(target) = self.arena.actors.get(target_id) else {
            return false;
        };
        let hero_blind = self
            .arena
            .actors
            .get(self.player_id)
            .is_some_and(|p| p.intrinsics.blind);
        let t = &self.hero.afflictions.transient;
        target.is_peaceful
            && !hero_blind
            && t.confused == 0
            && t.hallucinating == 0
            && t.stunned == 0
    }

    /// The hero walks into a safe peaceful monster (`!context.forcefight`).
    ///
    /// C `do_attack` (uhitm.c:462-509): `foo = Punished || !rn2(7) || ...`; if
    /// `foo` or the monster is in a tended shop, "You stop.  <Mon> is in the
    /// way!". Otherwise C `domove_swap_with_pet` (hack.c:2154-2176): a
    /// `mundisplaceable` monster (temple priest, shopkeeper, vault guard, Oracle,
    /// the quest leader; monst.h:227-230), or one that would be moved onto a
    /// trap, refuses ("You stop.  <Mon> doesn't want to swap places."); anyone
    /// else swaps places with the hero ("You swap places with the peaceful <mon>.").
    ///
    /// Not modelled: Punished, long worms, the `dopay()` bump on a blocking
    /// shopkeeper (uhitm.c:492-494), the `mmove == 0 && rn2(6)` "doesn't seem to
    /// move" case (no BESTIARY entry has speed 0), monster traps/liquids after
    /// the swap. Every branch uses the hero's move.
    pub(crate) fn bump_peaceful(
        &mut self,
        target_id: netrust_arena::ActorId,
        hero_from: Coord,
        target_coord: Coord,
    ) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(target) = self.arena.actors.get(target_id).cloned() else {
            return events;
        };
        // uhitm.c:475 `!rn2(7)` (Punished is not modelled).
        let must_stop = self.rng.random_range(0..7u32) == 0; // C `foo`
                                                             // uhitm.c:481-486: only checked when there is no other reason to stop.
        let inshop = !must_stop
            && self.level.room_at(target_coord).is_some_and(|room| {
                room.room_type == RoomType::Shop
                    && self.arena.actors.values().any(|a| {
                        !a.is_dead && a.name == "shopkeeper" && room.contains_inner(a.coord)
                    })
            });
        if must_stop || inshop {
            events.push(GameEvent::LogMessage {
                text: netrust_i18n::Messages::peaceful_in_the_way(
                    &target.name,
                    crate::peace::monnam_article(&self.ruleset, &target.name),
                    self.locale,
                ),
            });
            return events;
        }
        let quest_cfg = netrust_core::get_role_quest_config_or_default(&self.role_name);
        let mundisplaceable = self.ruleset.monster(&target.name).is_some_and(|a| {
            matches!(
                a.id,
                Some(
                    netrust_data::MonsterSpeciesId::Priest
                        | netrust_data::MonsterSpeciesId::Shopkeeper,
                )
            )
        }) || target.name.eq_ignore_ascii_case(quest_cfg.leader_name);
        let trap_at_hero = self.level.traps.contains_key(&hero_from);
        if mundisplaceable || trap_at_hero || !self.level.is_passable(hero_from) {
            events.push(GameEvent::LogMessage {
                text: netrust_i18n::Messages::peaceful_wont_swap(
                    &target.name,
                    crate::peace::monnam_article(&self.ruleset, &target.name),
                    self.locale,
                ),
            });
            return events;
        }
        if let Some(p) = self.arena.actors.get_mut(self.player_id) {
            p.coord = target_coord;
        }
        if let Some(m) = self.arena.actors.get_mut(target_id) {
            m.coord = hero_from;
        }
        events.push(GameEvent::ActorMoved {
            actor: self.player_id,
            from: hero_from,
            to: target_coord,
        });
        events.push(GameEvent::ActorMoved {
            actor: target_id,
            from: target_coord,
            to: hero_from,
        });
        events.push(GameEvent::LogMessage {
            text: netrust_i18n::Messages::swap_with_peaceful(&target.name, self.locale),
        });
        events
    }

    pub(crate) fn handle_move(&mut self, dir: Direction) -> Vec<GameEvent> {
        let mut events = Vec::new();
        let Some(player) = self.arena.actors.get(self.player_id).cloned() else {
            return events;
        };

        let move_cost = if true {
            let pc = &self.hero;
            let mount_cost = pc.mount.as_ref().and_then(|m| {
                // Determine mount's movement cost based on its speed
                // For simplicity, we assume speed acts as cost if it's lower, or we map it.
                // The prompt says min(unmounted, mount_cost).
                // Let's just use 12 for unmounted, and if mounted, we can derive a cost from steed's speed.
                // A fast mount (speed > 12) should cost less energy. Energy cost = 12 * 12 / speed.
                self.arena
                    .actors
                    .get(m.steed_id)
                    .map(|s| (NORMAL_SPEED * 12) / s.speed.max(1))
            });
            netrust_core::ranged::effective_movement_cost(NORMAL_SPEED, mount_cost)
        } else {
            NORMAL_SPEED
        };

        let (dx, dy) = dir.delta();
        let nx = player.coord.x as isize + dx;
        let ny = player.coord.y as isize + dy;

        if let Some(target_coord) = Coord::new(nx as usize, ny as usize) {
            // Check if actor at target
            if let Some(target_id) = self.actor_at(target_coord) {
                let is_target_tame = self
                    .arena
                    .actors
                    .get(target_id)
                    .map(|a| a.is_tame)
                    .unwrap_or(false);
                if is_target_tame {
                    // Displacement! Non-violent position swap verified in Lean 4
                    let pet_name = self
                        .arena
                        .actors
                        .get(target_id)
                        .map(|a| a.name.clone())
                        .unwrap_or_else(|| "pet".into());
                    let from = player.coord;
                    if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                        p.coord = target_coord;
                    }
                    if let Some(pet) = self.arena.actors.get_mut(target_id) {
                        pet.coord = from;
                    }
                    events.push(GameEvent::ActorMoved {
                        actor: self.player_id,
                        from,
                        to: target_coord,
                    });
                    events.push(GameEvent::ActorMoved {
                        actor: target_id,
                        from: target_coord,
                        to: from,
                    });
                    events.push(GameEvent::LogMessage {
                        text: format!("You displace {pet_name}."),
                    });
                    self.scheduler.hero_act(move_cost);
                } else if self.is_safemon(target_id) {
                    // Walking into a peaceful never attacks it (C `is_safemon`).
                    events.extend(self.bump_peaceful(target_id, player.coord, target_coord));
                    self.scheduler.hero_act(move_cost);
                } else {
                    let combat_events = self.resolve_combat(self.player_id, target_id);
                    events.extend(combat_events);
                    self.scheduler.hero_act(move_cost);
                }
            } else if let Some(boulder_id) = self.arena.items.iter().find_map(|(id, item)| {
                if item.location == netrust_arena::ItemLocation::Floor(target_coord)
                    && item.name == "boulder"
                {
                    Some(id)
                } else {
                    None
                }
            }) {
                // Boulder pushing mechanics are modeled in Lean 4
                if let Some(next_c) = target_coord.step(dir) {
                    let is_occupied = self.actor_at(next_c).is_some()
                        || self.arena.items.values().any(|it| {
                            it.location == netrust_arena::ItemLocation::Floor(next_c)
                                && it.name == "boulder"
                        });
                    let outcome = netrust_core::sokoban::push_boulder(
                        target_coord,
                        dir,
                        self.level.get_tile(next_c),
                        is_occupied,
                    );
                    match outcome {
                        netrust_core::sokoban::PushOutcome::Moved(new_pos) => {
                            if let Some(it) = self.arena.items.get_mut(boulder_id) {
                                it.location = netrust_arena::ItemLocation::Floor(new_pos);
                            }
                            events.push(GameEvent::LogMessage {
                                text: "You push the boulder.".into(),
                            });
                            self.scheduler.hero_act(move_cost);
                        }
                        netrust_core::sokoban::PushOutcome::FilledPit(pit_pos) => {
                            self.arena.destroy_item(boulder_id);
                            self.level.set_tile(pit_pos, Tile::Pit { filled: true });
                            events.push(GameEvent::LogMessage {
                                text: "The boulder falls into the pit and fills it!".into(),
                            });
                            self.scheduler.hero_act(move_cost);
                        }
                        netrust_core::sokoban::PushOutcome::Blocked => {
                            events.push(GameEvent::LogMessage {
                                text: "You try to move the boulder, but it won't budge.".into(),
                            });
                        }
                    }
                } else {
                    events.push(GameEvent::LogMessage {
                        text: "You try to move the boulder, but it won't budge.".into(),
                    });
                }
            } else {
                // Check tile
                let tile = self.level.get_tile(target_coord).clone();
                match tile {
                    Tile::Door {
                        state: DoorState::Closed,
                        trapped,
                    } => {
                        self.level.set_tile(
                            target_coord,
                            Tile::Door {
                                state: DoorState::Open,
                                trapped,
                            },
                        );
                        events.push(GameEvent::DoorToggled {
                            coord: target_coord,
                            new_state: DoorState::Open,
                        });
                        self.scheduler.hero_act(move_cost);
                    }
                    Tile::Drawbridge { open: false } => {
                        self.level
                            .set_tile(target_coord, Tile::Drawbridge { open: true });
                        events.push(GameEvent::LogMessage {
                            text: "You lower the drawbridge over the moat. The portcullis creaks open.".into(),
                        });
                        self.scheduler.hero_act(move_cost);
                    }
                    _ if tile.is_passable() => {
                        let from = player.coord;
                        if let Some(p) = self.arena.actors.get_mut(self.player_id) {
                            p.coord = target_coord;
                        }
                        events.push(GameEvent::ActorMoved {
                            actor: self.player_id,
                            from,
                            to: target_coord,
                        });

                        // Smudge engraving if stepping off a dust engraving
                        if let Some(outcome) = self.level.smudge_engraving(from) {
                            match outcome {
                                Some(remaining_e) => {
                                    events.push(GameEvent::EngravingDegraded {
                                        coord: from,
                                        remaining: Some(remaining_e.text),
                                    });
                                }
                                None => {
                                    events.push(GameEvent::EngravingDegraded {
                                        coord: from,
                                        remaining: None,
                                    });
                                    events.push(GameEvent::LogMessage {
                                        text: "The engraving in the dust has been completely wiped away by your footsteps.".into(),
                                    });
                                }
                            }
                        }

                        // If stepping onto a tile with an engraving, notify player!
                        if let Some(e) = self.level.get_engraving(target_coord) {
                            events.push(GameEvent::LogMessage {
                                text: format!(
                                    "There is something written on the floor here: \"{}\".",
                                    e.text
                                ),
                            });
                        }

                        // Trigger trap if present
                        let is_flying = self
                            .arena
                            .actors
                            .get(self.player_id)
                            .map(|a| a.intrinsics.levitation)
                            .unwrap_or(false);
                        // C `rn2(5)` seen-trap escape draw, once per encounter (trap.c:3040).
                        let rn2_5: u32 = if self.level.traps.contains_key(&target_coord) {
                            self.rng.random_range(0..5)
                        } else {
                            1
                        };
                        let triggered = self.level.traps.get_mut(&target_coord).and_then(|trap| {
                            netrust_core::traps::trigger_trap(trap, is_flying, rn2_5)
                        });
                        if let Some(triggered_type) = triggered {
                            match triggered_type {
                                netrust_types::TrapType::Arrow | netrust_types::TrapType::Dart => {
                                    events.push(GameEvent::LogMessage {
                                        text: format!("A {triggered_type:?} trap shoots you!"),
                                    });
                                    events.extend(self.damage_player(
                                        2,
                                        &format!("{triggered_type:?} trap").to_lowercase(),
                                    ));
                                }
                                netrust_types::TrapType::Teleport => {
                                    events.push(GameEvent::LogMessage {
                                        text: "You trigger a teleport trap!".into(),
                                    });
                                    // Teleport logic omitted for brevity, just send event
                                }
                                netrust_types::TrapType::LevelTeleport => {
                                    events.push(GameEvent::LogMessage {
                                        text: "You trigger a level teleport trap!".into(),
                                    });
                                }
                                netrust_types::TrapType::Pit
                                | netrust_types::TrapType::SpikedPit => {
                                    events.push(GameEvent::LogMessage {
                                        text: "You fall into a pit!".into(),
                                    });
                                }
                                _ => {
                                    events.push(GameEvent::LogMessage {
                                        text: format!("You trigger a {triggered_type:?} trap!"),
                                    });
                                }
                            }
                        }

                        // Check if player is leaving a shop with unpaid merchandise!
                        let from_shop = self
                            .level
                            .room_at(from)
                            .map(|r| r.room_type == RoomType::Shop)
                            .unwrap_or(false);
                        let to_shop = self
                            .level
                            .room_at(target_coord)
                            .map(|r| r.room_type == RoomType::Shop)
                            .unwrap_or(false);
                        if from_shop && !to_shop {
                            let has_unpaid = self
                                .arena
                                .items_carried_by(self.player_id)
                                .iter()
                                .any(|id| self.is_unpaid(*id));
                            if has_unpaid {
                                events.push(GameEvent::LogMessage {
                                    text: netrust_i18n::Messages::shopkeeper_shout(self.locale)
                                        .into(),
                                });
                                // Turn shopkeeper hostile
                                for (_, actor) in self.arena.actors.iter_mut() {
                                    if actor.name == "shopkeeper" && !actor.is_dead {
                                        actor.is_peaceful = false;
                                    }
                                }
                            }
                        }

                        self.scheduler.hero_act(move_cost);
                    }
                    _ => {
                        // Impassable obstacle; do not consume energy
                        events.push(GameEvent::LogMessage {
                            text: netrust_i18n::Messages::bump_wall(self.locale).into(),
                        });
                    }
                }
            }
        }

        events
    }
}
