//! Shared implementation of the currently supported selected and legacy wands.
//! Authorities expose bounded domain facts and legal wand operations separately.
pub(super) mod creation;
pub(crate) use creation::tile_placement;
mod effects;
pub mod ports;
use crate::GameEvent;
use nethacked_arena::{ActorId, ItemId};
use nethacked_data::ItemKindId;
use nethacked_types::{Buc, Coord, Direction, ItemClass, Tile, COLNO, ROWNO};
use ports::*;
use rand::Rng;

#[derive(Debug, PartialEq, Eq)]
pub enum WandError<E> {
    Authority(E),
    Timer(crate::object_timers::ObjectTimerError),
}
impl<E> From<E> for WandError<E> {
    fn from(error: E) -> Self {
        Self::Authority(error)
    }
}
/// Failure retains only the ordered events already emitted. Applied port
/// mutations and successful draws are never rolled back or replayed.
#[derive(Debug)]
pub struct WandFailure<E> {
    pub error: WandError<E>,
    pub events: Vec<GameEvent>,
}

#[derive(Clone, Copy)]
pub enum Selection {
    First,
    Selected {
        index: u32,
        expected: Option<ItemId>,
    },
}
fn message(text: impl Into<String>) -> Vec<GameEvent> {
    vec![GameEvent::LogMessage { text: text.into() }]
}

#[allow(clippy::too_many_arguments)]
pub fn try_zap<E>(
    selection: Selection,
    player_id: ActorId,
    dir: Direction,
    energy: u32,
    objects: &mut impl TryObjects<E>,
    actors: &mut impl TryActors<E>,
    geography: &mut impl TryGeography<E>,
    hero: &mut impl TryHero<E>,
    clock: &mut impl TryClock<E>,
    rng: &mut impl Random<E>,
    session: &Session<'_>,
) -> Result<Vec<GameEvent>, WandFailure<E>> {
    let mut events = Vec::new();
    let result = (|| -> Result<(), WandError<E>> {
        let wid = match selection {
            Selection::First => {
                if actors.actor(player_id)?.is_none() {
                    return Ok(());
                }
                let mut first = None;
                for id in objects.carried(player_id)? {
                    if objects
                        .object(id)?
                        .is_some_and(|item| item.class == ItemClass::Wand)
                    {
                        first = Some(id);
                        break;
                    }
                }
                let Some(id) = first else {
                    events.extend(message("You have no wand to zap."));
                    return Ok(());
                };
                id
            }
            Selection::Selected { index, expected } => {
                let carried = objects.carried(player_id)?;
                let Some(wid) = usize::try_from(index)
                    .ok()
                    .and_then(|i| carried.get(i))
                    .copied()
                else {
                    events.extend(message("You don't have that object."));
                    return Ok(());
                };
                if expected.is_some_and(|id| id != wid) {
                    events.extend(message("You don't have that object."));
                    return Ok(());
                }
                let Some(item) = objects.object(wid)? else {
                    events.extend(message("You don't have that object."));
                    return Ok(());
                };
                if item.class != ItemClass::Wand {
                    events.extend(message("That is a silly thing to zap."));
                    return Ok(());
                }
                wid
            }
        };
        let Some(player) = actors.actor(player_id)? else {
            return Ok(());
        };
        let Some(wand) = objects.object(wid)? else {
            return Ok(());
        };
        // Explicit identities never fall back to display names or static tables.
        let definition = match &wand.definition {
            Some(identity) => {
                let Some(def) = session.ruleset.item_for_definition(identity) else {
                    events.extend(message(
                        "You cannot zap that wand: its definition is unavailable.",
                    ));
                    return Ok(());
                };
                Some(def)
            }
            None => session.ruleset.item(&wand.name),
        };
        let resolved_id = definition.and_then(|def| def.id);
        let effect_name = if wand.definition.is_some() {
            definition.map(|def| def.name.clone())
        } else {
            None
        };
        let ordinary_creation = wand.buc == Buc::Uncursed
            && wand.enchantment > 0
            && resolved_id == Some(ItemKindId::WAN_CREATE_MONSTER)
            && !hero.polymorphed()?
            && !matches!(
                geography.tile(player.coord)?,
                Tile::Pool { frozen: false } | Tile::Moat
            );
        // Preserve the positive-only runtime activation. No wrest/dust branch.
        let wand_name = if wand.enchantment > 0 {
            let Ok(nethacked_core::artifacts_wands::WandChargeAttempt::Ready { spe, .. }) =
                nethacked_core::artifacts_wands::wand_charge_attempt(wand.enchantment, None)
            else {
                return Ok(());
            };
            objects.spend_positive_charge(wid, spe)?;
            effect_name.unwrap_or(wand.name)
        } else {
            events.extend(message(nethacked_i18n::Messages::wand_empty(
                session.locale,
            )));
            clock.hero_act()?;
            return Ok(());
        };
        if ordinary_creation {
            if let Err(text) = creation::try_create_by_hero(
                player_id, objects, actors, geography, hero, clock, rng, session,
            )? {
                events.extend(message(text));
            }
            clock.hero_act()?;
            return Ok(());
        }
        if wand_name.contains("secret door") {
            let mut revealed = 0;
            for dy in -3..=3 {
                for dx in -3..=3 {
                    let nx = player.coord.x as i32 + dx;
                    let ny = player.coord.y as i32 + dy;
                    if nx >= 0
                        && nx < COLNO as i32
                        && ny >= 0
                        && ny < ROWNO as i32
                        && geography
                            .reveal_secret_door(Coord::new_unchecked(nx as usize, ny as usize))?
                    {
                        revealed += 1;
                    }
                }
            }
            events.extend(message(if revealed > 0 {
                nethacked_i18n::Messages::secret_doors_found(revealed, session.locale)
            } else {
                "You feel a tingling sensation, but sense no hidden doors.".into()
            }));
            clock.hero_act()?;
            return Ok(());
        }
        let path = geography.trajectory(player.coord, dir, energy)?;
        events.push(GameEvent::BeamPropagated { path: path.clone() });
        let mut hit_coords = path;
        let (dx, dy) = dir.delta();
        if dx != 0 || dy != 0 {
            let last = hit_coords.last().copied().unwrap_or(player.coord);
            let tx = last.x as i32 + dx as i32;
            let ty = last.y as i32 + dy as i32;
            if tx >= 0 && tx < COLNO as i32 && ty >= 0 && ty < ROWNO as i32 {
                hit_coords.push(Coord::new_unchecked(tx as usize, ty as usize));
            }
        }
        for coord in hit_coords {
            let tile = geography.tile(coord)?;
            if wand_name.contains("striking") {
                if matches!(tile, Tile::Drawbridge { .. }) {
                    let _ = nethacked_core::endgame::destroy_drawbridge();
                    geography.collapse_drawbridge(coord)?;
                    events.extend(message(nethacked_i18n::Messages::drawbridge_collapse(
                        session.locale,
                    )));
                    break;
                } else if matches!(tile, Tile::Door { .. }) {
                    geography.break_door(coord)?;
                    events.extend(message(nethacked_i18n::Messages::door_splinters(
                        session.locale,
                    )));
                }
            } else if wand_name.contains("cold") && matches!(tile, Tile::Pool { .. }) {
                geography.freeze_pool(coord)?;
                events.extend(message(nethacked_i18n::Messages::pool_frozen(
                    session.locale,
                )));
            }
            if let Some(target_id) = actors.at(coord)? {
                if target_id != player_id {
                    let damage = if wand_name.contains("death") {
                        100
                    } else if wand_name.contains("cold") {
                        18
                    } else {
                        12
                    };
                    if let Some(target) = actors.actor(target_id)? {
                        if wand_name.contains("digging") || wand_name.contains("teleport") {
                            events.extend(message(format!(
                                "The {} has no effect on {}.",
                                wand_name, target.name
                            )));
                        } else if wand_name.contains("polymorph") {
                            if !target.is_unique && !target.is_player {
                                if let Some(def) = session
                                    .ruleset
                                    .monster_by_id(nethacked_data::MonsterSpeciesId::GOBLIN)
                                {
                                    actors.polymorph(target_id, def)?;
                                }
                                let Some(target) = actors.actor(target_id)? else {
                                    return Ok(());
                                };
                                let name = target.name;
                                events.extend(message(format!("The monster turns into a {name}!")));
                            } else {
                                events.extend(message("The monster shudders but is unaffected."));
                            }
                        } else if let Some(lethal) = actors.damage(target_id, damage)? {
                            events.push(GameEvent::AttackLanded {
                                attacker: player_id,
                                target: target_id,
                                damage,
                                lethal,
                            });
                        }
                    }
                    if actors.actor(target_id)?.is_some_and(|t| t.is_dead) {
                        let name = actors
                            .actor(target_id)?
                            .map(|a| a.name)
                            .unwrap_or_else(|| "The monster".into());
                        events.extend(message(format!("{name} is destroyed by the wand beam!")));
                        effects::killed(
                            player_id,
                            target_id,
                            objects,
                            actors,
                            hero,
                            clock,
                            rng,
                            session,
                            &mut events,
                        )?;
                    }
                    if !wand_name.contains("digging")
                        && actors.actor(target_id)?.is_some_and(|t| !t.is_dead)
                    {
                        effects::anger(
                            player_id,
                            target_id,
                            actors,
                            geography,
                            hero,
                            clock,
                            rng,
                            session,
                            &mut events,
                        )?;
                    }
                    break;
                }
            }
        }
        clock.hero_act()?;
        Ok(())
    })();
    match result {
        Ok(()) => Ok(events),
        Err(error) => Err(WandFailure { error, events }),
    }
}

/// Compatibility entrypoint delegates to the single fallible algorithm.
#[allow(clippy::too_many_arguments)]
pub fn zap(
    selection: Selection,
    player_id: ActorId,
    dir: Direction,
    energy: u32,
    objects: &mut impl Objects,
    actors: &mut impl Actors,
    geography: &mut impl Geography,
    hero: &mut impl Hero,
    clock: &mut impl Clock,
    rng: &mut impl Rng,
    session: &Session<'_>,
) -> Vec<GameEvent> {
    match try_zap(
        selection,
        player_id,
        dir,
        energy,
        &mut InfalliblePort(objects),
        &mut InfalliblePort(actors),
        &mut InfalliblePort(geography),
        &mut InfalliblePort(hero),
        &mut InfalliblePort(clock),
        &mut InfalliblePort(rng),
        session,
    ) {
        Ok(events) => events,
        Err(WandFailure {
            error: WandError::Authority(error),
            ..
        }) => match error {},
        Err(WandFailure {
            error: WandError::Timer(error),
            ..
        }) => panic!("death corpse deadline fits the world clock: {error:?}"),
    }
}
