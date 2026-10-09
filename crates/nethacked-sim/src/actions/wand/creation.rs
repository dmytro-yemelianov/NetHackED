//! Shared ordinary creation and placement, preserving the legacy draw sequence.
use super::ports::*;
use crate::peace::peace_input;
use nethacked_arena::{ActorId, ActorRecord};
use nethacked_data::{ruleset::MonsterDef, ItemKindId, MonsterSound};
use nethacked_types::{Alignment, BranchId, Coord, Tile, COLNO, ROWNO};
use rand::Rng;

pub(super) fn birth_context(
    player: Option<&ActorRecord>,
    turn: u64,
    branch: BranchId,
    depth: usize,
    in_mklev: bool,
) -> (
    nethacked_data::containers::ContainerContext,
    nethacked_data::generation::MonsterGenContext,
) {
    use nethacked_data::generation::{DungeonAlign, MonsterGenContext};
    let hero_level = player.map_or(1, |p| p.level);
    (
        nethacked_data::containers::ContainerContext {
            hero_level,
            level_difficulty: depth.min(u32::MAX as usize) as u32,
            moves: turn,
            in_mklev,
        },
        MonsterGenContext {
            hero_level: hero_level.min(i32::MAX as u32) as i32,
            level_difficulty: depth.min(i32::MAX as usize) as i32,
            in_hell: branch == BranchId::Gehennom,
            align: if branch == BranchId::GnomishMines {
                DungeonAlign::Lawful
            } else {
                DungeonAlign::None
            },
            temperature: 0,
        },
    )
}
fn alignment<E>(actors: &impl TryActors<E>, player: ActorId) -> Result<Alignment, E> {
    Ok(actors
        .actor(player)?
        .map(|p| p.alignment)
        .unwrap_or(Alignment::Neutral))
}
fn malign(
    actor: &ActorRecord,
    def: &MonsterDef,
    hero_alignment: Alignment,
    role_name: &str,
) -> i32 {
    let quest = nethacked_core::get_role_quest_config_or_default(role_name);
    let leader =
        def.msound == MonsterSound::Leader || actor.name.eq_ignore_ascii_case(quest.leader_name);
    nethacked_core::calculate_malign(
        def.maligntyp,
        hero_alignment,
        actor.is_peaceful,
        leader,
        def.peaceful_by_default,
        def.always_hostile,
    )
}
fn rings<E>(origin: Coord, radius: usize, rng: &mut impl Random<E>) -> Result<Vec<Coord>, E> {
    let mut all = Vec::new();
    for r in 1..=radius {
        let mut ring = Vec::new();
        for y in origin.y.saturating_sub(r)..=(origin.y + r).min(ROWNO - 1) {
            for x in origin.x.saturating_sub(r).max(1)..=(origin.x + r).min(COLNO - 1) {
                if x.abs_diff(origin.x).max(y.abs_diff(origin.y)) == r {
                    ring.push(Coord::new_unchecked(x, y));
                }
            }
        }
        for i in 0..ring.len().saturating_sub(1) {
            let k = rng.sample(WandDraw::Below((ring.len() - i) as u32))? as usize;
            ring.swap(i, i + k);
        }
        all.extend(ring);
    }
    Ok(all)
}
/// Terrain part of ordinary placement. Overrides preserve the shared helper's
/// early returns before wards and floor objects are queried.
pub(crate) enum TilePlacement {
    Override(bool),
    Ordinary(bool),
}
impl TilePlacement {
    pub(crate) fn allowed(&self) -> bool {
        match self {
            Self::Override(allowed) | Self::Ordinary(allowed) => *allowed,
        }
    }
}
pub(crate) fn tile_placement(def: &MonsterDef, tile: &Tile) -> TilePlacement {
    let air = def.has_flag("fly") || def.has_flag("float");
    match tile {
        Tile::Moat | Tile::Pool { frozen: false } => {
            return TilePlacement::Override(def.has_flag("swim") || air);
        }
        Tile::Lava => return TilePlacement::Override(air || def.has_flag("likes_lava")),
        _ => {}
    }
    if def.has_flag("wallwalk") || (matches!(tile, Tile::Door { .. }) && def.has_flag("amorphous"))
    {
        return TilePlacement::Override(true);
    }
    TilePlacement::Ordinary(tile.is_passable() || matches!(tile, Tile::Pit { .. }))
}
#[allow(clippy::too_many_arguments)]
fn goodpos<E>(
    coord: Coord,
    def: &MonsterDef,
    scary: bool,
    branch: BranchId,
    objects: &impl TryObjectFacts<E>,
    actors: &impl TryActors<E>,
    geography: &impl TryGeographyFacts<E>,
    session: &Session<'_>,
) -> Result<bool, E> {
    if actors.at(coord)?.is_some() {
        return Ok(false);
    }
    let tile = geography.tile(coord)?;
    let placement = tile_placement(def, &tile);
    if let TilePlacement::Override(allowed) = placement {
        return Ok(allowed);
    }
    if scary && def.glyph != '@' && def.glyph != 'A' && !def.is_unique {
        let scroll = objects.floor_kind(coord, ItemKindId::SCR_SCARE_MONSTER, session.ruleset)?;
        let ward = !matches!(branch, BranchId::Gehennom | BranchId::AstralPlane)
            && !def.has_flag("noeyes")
            && geography
                .engraving(coord)?
                .is_some_and(|e| e.text.contains("Elbereth"));
        if scroll || ward {
            return Ok(false);
        }
    }
    if !placement.allowed() {
        return Ok(false);
    }
    Ok(
        !objects.floor_kind(coord, ItemKindId::BOULDER, session.ruleset)?
            || def.has_flag("rockthrow"),
    )
}
#[allow(clippy::too_many_arguments)]
fn adjacent<E>(
    origin: Coord,
    def: &MonsterDef,
    objects: &impl TryObjectFacts<E>,
    actors: &impl TryActors<E>,
    geography: &impl TryGeographyFacts<E>,
    rng: &mut impl Random<E>,
    session: &Session<'_>,
) -> Result<Option<Coord>, E> {
    for scary in [true, false] {
        let near = rings(origin, 3, rng)?;
        for coord in near.iter().copied() {
            if goodpos(
                coord,
                def,
                scary,
                session.branch,
                objects,
                actors,
                geography,
                session,
            )? {
                return Ok(Some(coord));
            }
        }
        let all = rings(origin, COLNO.max(ROWNO), rng)?;
        for coord in all.into_iter().skip(near.len()) {
            if goodpos(
                coord,
                def,
                scary,
                session.branch,
                objects,
                actors,
                geography,
                session,
            )? {
                return Ok(Some(coord));
            }
        }
    }
    Ok(None)
}
#[allow(clippy::too_many_arguments)]
fn insert<E>(
    def: &MonsterDef,
    coord: Coord,
    branch: BranchId,
    level_context: (u32, u32),
    player: ActorId,
    objects: &impl TryObjectFacts<E>,
    actors: &mut impl TryActors<E>,
    hero: &impl TryHeroFacts<E>,
    rng: &mut impl Random<E>,
    session: &Session<'_>,
) -> Result<ActorId, E> {
    let mut actor = session.ruleset.try_create_random_monster_record_from_def(
        def,
        coord,
        nethacked_data::monsters::MonsterSpawnContext {
            endgame_plane: (branch == BranchId::AstralPlane)
                .then_some(nethacked_types::EndgamePlane::Astral),
        },
        level_context,
        |draw| rng.sample(WandDraw::Monster(draw)),
    )?;
    let input = peace_input(
        def,
        alignment(actors, player)?,
        hero.race()?,
        hero.alignment_record()?,
        objects.has_amulet(player)?,
    );
    actor.is_peaceful =
        nethacked_core::peace::try_peace_minded(&input, |n| rng.sample(WandDraw::Below(n)))?;
    actor.malign = malign(&actor, def, alignment(actors, player)?, hero.role_name()?);
    actors.spawn_initialized(actor)
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn try_spawn_at<E>(
    coord: Coord,
    branch: BranchId,
    depth: usize,
    player: ActorId,
    objects: &impl TryObjectFacts<E>,
    actors: &mut impl TryActors<E>,
    geography: &impl TryGeographyFacts<E>,
    hero: &impl TryHeroFacts<E>,
    clock: &impl TryWorldClock<E>,
    rng: &mut impl Random<E>,
    session: &Session<'_>,
) -> Result<Option<ActorId>, E> {
    if coord.x == 0 || coord.x >= COLNO || coord.y >= ROWNO || actors.at(coord)?.is_some() {
        return Ok(None);
    }
    let (birth, context) = birth_context(
        actors.actor(player)?.as_ref(),
        clock.turn()?,
        branch,
        depth,
        true,
    );
    let mut selected = None;
    for attempt in 0..51 {
        let gone = actors.genocide_filter()?;
        let Some(def) = nethacked_data::generation::try_rndmonst_adj(
            session.ruleset,
            &context,
            0,
            0,
            |def| gone(&def.name.to_lowercase(), def.glyph),
            |n| rng.sample(WandDraw::Below(n)),
        )?
        else {
            return Ok(None);
        };
        if attempt == 50
            || goodpos(
                coord, def, true, branch, objects, actors, geography, session,
            )?
        {
            selected = Some(def);
            break;
        }
    }
    let Some(selected) = selected else {
        return Ok(None);
    };
    Ok(Some(insert(
        selected,
        coord,
        branch,
        (birth.level_difficulty, birth.hero_level),
        player,
        objects,
        actors,
        hero,
        rng,
        session,
    )?))
}
#[allow(clippy::too_many_arguments)]
fn expand<E>(
    leader_id: ActorId,
    player: ActorId,
    objects: &impl TryObjectFacts<E>,
    actors: &mut impl TryActors<E>,
    geography: &impl TryGeographyFacts<E>,
    hero: &impl TryHeroFacts<E>,
    clock: &impl TryWorldClock<E>,
    rng: &mut impl Random<E>,
    session: &Session<'_>,
) -> Result<(), E> {
    let Some(leader) = actors.actor(leader_id)? else {
        return Ok(());
    };
    let Some(def) = session.ruleset.monster_for_actor(&leader) else {
        return Ok(());
    };
    if !def.gen_flags.iter().any(|flag| flag == "sgroup") || rng.sample(WandDraw::Below(2))? == 0 {
        return Ok(());
    }
    let mut origin = leader.coord;
    let raw = rng.sample(WandDraw::Below(3))? + 1;
    let Some(player_actor) = actors.actor(player)? else {
        return Ok(());
    };
    let divisor = match player_actor.level {
        0..=2 => 4,
        3..=4 => 2,
        _ => 1,
    };
    let count = (raw / divisor).max(1);
    let (birth, _) = birth_context(
        actors.actor(player)?.as_ref(),
        clock.turn()?,
        session.branch,
        session.depth,
        true,
    );
    for _ in 0..count {
        let input = peace_input(
            def,
            alignment(actors, player)?,
            hero.race()?,
            hero.alignment_record()?,
            objects.has_amulet(player)?,
        );
        if nethacked_core::peace::try_peace_minded(&input, |n| rng.sample(WandDraw::Below(n)))? {
            continue;
        }
        let Some(coord) = adjacent(origin, def, objects, actors, geography, rng, session)? else {
            continue;
        };
        origin = coord;
        if actors.genocided(&def.name.to_lowercase(), def.glyph)? {
            continue;
        }
        let id = insert(
            def,
            coord,
            session.branch,
            (birth.level_difficulty, birth.hero_level),
            player,
            objects,
            actors,
            hero,
            rng,
            session,
        )?;
        let Some(mut follower) = actors.actor(id)? else {
            continue;
        };
        follower.is_peaceful = false;
        let malign = malign(
            &follower,
            def,
            alignment(actors, player)?,
            hero.role_name()?,
        );
        actors.force_hostile_malign(id, malign)?;
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn try_create_by_hero<E>(
    player: ActorId,
    objects: &impl TryObjectFacts<E>,
    actors: &mut impl TryActors<E>,
    geography: &impl TryGeographyFacts<E>,
    hero: &impl TryHeroFacts<E>,
    clock: &impl TryWorldClock<E>,
    rng: &mut impl Random<E>,
    session: &Session<'_>,
) -> Result<Result<(), &'static str>, E> {
    let role = match hero.role_id()? {
        Some(role) => Some(role),
        None => serde_json::from_value(serde_json::Value::String(hero.role_name()?.into())).ok(),
    };
    let Some(role) = role else {
        return Ok(Err("Your original form is unknown."));
    };
    let Some(original) = session.ruleset.original_monster_for_role(role) else {
        return Ok(Err("Your original form is unknown."));
    };
    let count = if rng.sample(WandDraw::Below(23))? != 0 {
        1
    } else {
        rng.sample(WandDraw::Below(7))? + 2
    };
    for _ in 0..count {
        let Some(actor) = actors.actor(player)? else {
            return Ok(Ok(()));
        };
        let origin = actor.coord;
        if let Some(coord) = adjacent(origin, original, objects, actors, geography, rng, session)? {
            if let Some(leader) = try_spawn_at(
                coord,
                session.branch,
                session.depth,
                player,
                objects,
                actors,
                geography,
                hero,
                clock,
                rng,
                session,
            )? {
                expand(
                    leader, player, objects, actors, geography, hero, clock, rng, session,
                )?;
            }
        }
    }
    Ok(Ok(()))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_at(
    coord: Coord,
    branch: BranchId,
    depth: usize,
    player: ActorId,
    objects: &impl ObjectFacts,
    actors: &mut impl Actors,
    geography: &impl GeographyFacts,
    hero: &impl HeroFacts,
    clock: &impl WorldClock,
    rng: &mut impl Rng,
    session: &Session<'_>,
) -> Option<ActorId> {
    let mut objects = objects;
    let mut geography = geography;
    let mut hero = hero;
    let mut clock = clock;
    match try_spawn_at(
        coord,
        branch,
        depth,
        player,
        &InfalliblePort(&mut objects),
        &mut InfalliblePort(actors),
        &InfalliblePort(&mut geography),
        &InfalliblePort(&mut hero),
        &InfalliblePort(&mut clock),
        &mut InfalliblePort(rng),
        session,
    ) {
        Ok(value) => value,
        Err(error) => match error {},
    }
}

/// Retained Vec-era creation helper; wand execution uses the fallible helper.
#[allow(dead_code, clippy::too_many_arguments)]
pub(crate) fn create_by_hero(
    player: ActorId,
    objects: &impl ObjectFacts,
    actors: &mut impl Actors,
    geography: &impl GeographyFacts,
    hero: &impl HeroFacts,
    clock: &impl WorldClock,
    rng: &mut impl Rng,
    session: &Session<'_>,
) -> Result<(), &'static str> {
    let mut objects = objects;
    let mut geography = geography;
    let mut hero = hero;
    let mut clock = clock;
    match try_create_by_hero(
        player,
        &InfalliblePort(&mut objects),
        &mut InfalliblePort(actors),
        &InfalliblePort(&mut geography),
        &InfalliblePort(&mut hero),
        &InfalliblePort(&mut clock),
        &mut InfalliblePort(rng),
        session,
    ) {
        Ok(value) => value,
        Err(error) => match error {},
    }
}
