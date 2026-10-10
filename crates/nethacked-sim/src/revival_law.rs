//! Shared current-Rust ordinary Troll revival and placement laws.
//! Immutable domain facts and native RNG only; owner effects stay in adapters.
use nethacked_arena::{
    ActorId, ActorRecord, ActorSex, ActorStorage, ItemId, ItemLocation, ItemRecord, ItemStorage,
};
use nethacked_data::{
    corpses::CorpseDefinition,
    monsters::{CorpseThawPolicy, CorpseTimeoutPolicy},
    ruleset::{MonsterDef, Ruleset},
    ItemKindId, RaceId,
};
use nethacked_dungeon::DungeonLevel;
use nethacked_types::{Alignment, BranchId, Coord, GenocideRegistry, Tile, COLNO, ROWNO};
use rand::Rng;

pub(crate) struct RevivalInput<'a> {
    pub ruleset: &'a Ruleset,
    pub items: &'a ItemStorage,
    pub actors: &'a ActorStorage,
    pub level: &'a DungeonLevel,
    pub genocide_registry: &'a GenocideRegistry,
    pub player: ActorId,
    pub branch: BranchId,
    pub depth: usize,
    pub hero_alignment: Alignment,
    pub hero_race: RaceId,
    pub alignment_record: i32,
    pub role_name: &'a str,
}

#[derive(Debug)]
pub(crate) enum RevivalAttempt {
    Unsupported,
    Failed { rot_delay: u64 },
    Revived { actor: ActorRecord },
}

pub(crate) fn ordinary_troll_attempt(
    id: ItemId,
    input: &RevivalInput<'_>,
    rng: &mut impl Rng,
) -> RevivalAttempt {
    let Some(item) = input.items.get(id).filter(|i| i.is_corpse()) else {
        return RevivalAttempt::Unsupported;
    };
    let CorpseDefinition::Owned(body) = input.ruleset.corpse_body_definition(item) else {
        return RevivalAttempt::Unsupported;
    };
    if body.glyph != 'T'
        || body.corpse_timeout_policy != CorpseTimeoutPolicy::Ordinary
        || body.corpse_thaw_policy != CorpseThawPolicy::Ordinary
    {
        return RevivalAttempt::Unsupported;
    }
    if let Some(actor) = try_revive_troll(id, item, body, input, rng) {
        return RevivalAttempt::Revived { actor };
    }
    // Sampling happens even if the adapter finds an existing RotCorpse timer.
    let sum: i64 = (0..5)
        .map(|_| i64::from(rng.random_range(0..50u32)) + 1)
        .sum();
    RevivalAttempt::Failed {
        rot_delay: (sum - i64::from(item.corpse_age)).max(1) as u64,
    }
}

fn actor_at(coord: Coord, input: &RevivalInput<'_>) -> Option<ActorId> {
    input
        .actors
        .iter()
        .find_map(|(id, actor)| (actor.coord == coord && !actor.is_dead).then_some(id))
}

fn actor_is_genocided(name: &str, input: &RevivalInput<'_>) -> bool {
    let lower = name.to_lowercase();
    let class = input.ruleset.monster_class_of(name);
    input.genocide_registry.genocided_species.contains(&lower)
        || class
            .map(|c| input.genocide_registry.genocided_classes.contains(&c))
            .unwrap_or(false)
}

fn revival_origin(
    input: &RevivalInput<'_>,
    item: &ItemRecord,
    rng: &mut impl Rng,
) -> Option<Coord> {
    let mut location = item.location.clone();
    let mut parents = Vec::new();
    while let ItemLocation::InContainer(parent) = location {
        if parents.contains(&parent) {
            return None;
        }
        parents.push(parent);
        location = input.items.get(parent)?.location.clone();
    }
    let coord = match location {
        ItemLocation::Floor(coord) => coord,
        ItemLocation::CarriedBy(actor) => input.actors.get(actor).filter(|a| !a.is_dead)?.coord,
        _ => return None,
    };
    if coord.x == 0 || coord.x >= COLNO || coord.y >= ROWNO {
        return None;
    }
    if let Some(immediate) = parents.first() {
        let container = &input.items[*immediate];
        // get_container_location counts enclosing parents of the immediate
        // container, not the corpse's immediate link itself.
        if container.locked
            || parents.len() > 3
            || input
                .ruleset
                .item(&container.name)
                .is_some_and(|d| d.id == Some(ItemKindId::STATUE))
            || (container.is_bag_of_holding && rng.random_range(0..40u32) != 0)
        {
            return None;
        }
    }
    Some(coord)
}

fn try_revive_troll(
    id: ItemId,
    item: &ItemRecord,
    body: &MonsterDef,
    input: &RevivalInput<'_>,
    rng: &mut impl Rng,
) -> Option<ActorRecord> {
    let mut coord = revival_origin(input, item, rng)?;
    // C MON_AT excludes the hero. BODY placement precedes norevive.
    if input
        .actors
        .iter()
        .any(|(id, a)| id != input.player && !a.is_dead && a.coord == coord)
    {
        if let Some(next) = monster_adjacent(coord, body, input, rng) {
            coord = next;
        }
    }
    if item.corpse_no_revive
        || input
            .items
            .iter()
            .any(|(_, child)| child.location == ItemLocation::InContainer(id))
    {
        return None;
    }
    let rs = input.ruleset;
    let CorpseDefinition::Owned(def) = rs.corpse_revival_definition(item) else {
        return None;
    };
    if item
        .saved_monster_traits
        .as_ref()
        .is_some_and(|t| t.actor.max_hp == 0)
    {
        return None;
    }
    if input.actors[input.player].coord == coord {
        let next = monster_adjacent(coord, def, input, rng)?;
        coord = next;
    }
    if actor_at(coord, input).is_some() || actor_is_genocided(&def.name, input) {
        return None;
    }
    let forced = if item.saved_monster_traits.is_some() {
        None
    } else {
        match item.enchantment & 3 {
            1 => Some(ActorSex::Female),
            2 => Some(ActorSex::Male),
            _ => None,
        }
    };
    let hero_level = input.actors.get(input.player).map_or(1, |p| p.level);
    let level_difficulty = input.depth.min(u32::MAX as usize) as u32;
    let plane = nethacked_data::monsters::MonsterSpawnContext {
        endgame_plane: (input.branch == BranchId::AstralPlane)
            .then_some(nethacked_types::EndgamePlane::Astral),
    };
    let mut dummy = rs.create_revival_record_from_def(
        def,
        coord,
        plane,
        (level_difficulty, hero_level),
        forced,
        rng,
    );
    let peace = crate::peace::peace_input(
        def,
        input.hero_alignment,
        input.hero_race,
        input.alignment_record,
        input.items.iter().any(|(_, item)| {
            item.location == ItemLocation::CarriedBy(input.player)
                && crate::actions::items::is_real_amulet(item)
        }),
    );
    dummy.is_peaceful = crate::peace::roll_peace_minded(&peace, rng);
    let quest_cfg = nethacked_core::get_role_quest_config_or_default(input.role_name);
    let is_leader = matches!(def.msound, nethacked_data::MonsterSound::Leader)
        || dummy.name.eq_ignore_ascii_case(quest_cfg.leader_name);
    dummy.malign = nethacked_core::calculate_malign(
        def.maligntyp,
        input.hero_alignment,
        dummy.is_peaceful,
        is_leader,
        def.peaceful_by_default,
        def.always_hostile,
    );
    let actor = if let Some(traits) = &item.saved_monster_traits {
        let mut restored = traits.actor.clone();
        if dummy.level < def.level {
            let repair = rng.random_range(0..def.level + 1) + 1;
            if repair > dummy.level {
                while dummy.level < repair {
                    dummy.level += 1;
                    dummy.max_hp += rng.random_range(0..8u32) + 1;
                }
                restored.level = dummy.level;
            }
        }
        restored.max_hp = restored.max_hp.max(dummy.max_hp);
        restored.hp = restored.max_hp;
        restored.coord = coord;
        restored.is_dead = false;
        restored.is_fleeing = false;
        restored.is_cancelled = false;
        restored.intrinsics.blind = false;
        restored
    } else {
        dummy
    };
    Some(actor)
}

pub(crate) fn monster_adjacent(
    origin: Coord,
    def: &MonsterDef,
    input: &RevivalInput<'_>,
    rng: &mut impl Rng,
) -> Option<Coord> {
    for scary in [true, false] {
        let near = monster_placement_rings(origin, 3, rng);
        if let Some(coord) = near
            .iter()
            .copied()
            .find(|c| monster_goodpos_in_branch(*c, def, scary, input.branch, input))
        {
            return Some(coord);
        }
        let all = monster_placement_rings(origin, COLNO.max(ROWNO), rng);
        if let Some(coord) = all
            .into_iter()
            .skip(near.len())
            .find(|c| monster_goodpos_in_branch(*c, def, scary, input.branch, input))
        {
            return Some(coord);
        }
    }
    None
}

fn monster_placement_rings(origin: Coord, radius: usize, rng: &mut impl Rng) -> Vec<Coord> {
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
            let k = rng.random_range(0..(ring.len() - i) as u32) as usize;
            ring.swap(i, i + k);
        }
        all.extend(ring);
    }
    all
}

pub(crate) fn monster_goodpos_in_branch(
    coord: Coord,
    def: &MonsterDef,
    scary: bool,
    branch: BranchId,
    input: &RevivalInput<'_>,
) -> bool {
    if actor_at(coord, input).is_some() {
        return false;
    }
    let tile = input.level.get_tile(coord);
    let air = def.has_flag("fly") || def.has_flag("float");
    match tile {
        Tile::Moat | Tile::Pool { frozen: false } => return def.has_flag("swim") || air,
        Tile::Lava => return air || def.has_flag("likes_lava"),
        _ => {}
    }
    if def.has_flag("wallwalk") {
        return true;
    }
    if matches!(tile, Tile::Door { .. }) && def.has_flag("amorphous") {
        return true;
    }
    if scary && def.glyph != '@' && def.glyph != 'A' && !def.is_unique {
        let scroll = input.items.iter().any(|(_, i)| {
            i.location == ItemLocation::Floor(coord)
                && input
                    .ruleset
                    .item(&i.name)
                    .is_some_and(|d| d.id == Some(ItemKindId::SCR_SCARE_MONSTER))
        });
        let ward = !matches!(branch, BranchId::Gehennom | BranchId::AstralPlane)
            && !def.has_flag("noeyes")
            && input
                .level
                .get_engraving(coord)
                .is_some_and(|e| e.text.contains("Elbereth"));
        if scroll || ward {
            return false;
        }
    }
    if !tile.is_passable() && !matches!(tile, Tile::Pit { .. }) {
        return false;
    }
    !input.items.iter().any(|(_, i)| {
        i.location == ItemLocation::Floor(coord)
            && input
                .ruleset
                .item(&i.name)
                .is_some_and(|d| d.id == Some(ItemKindId::BOULDER))
            && !def.has_flag("rockthrow")
    })
}

#[cfg(test)]
mod tests;
