//! Selected ordinary Troll family, source-only C 16ff5911: zap.c:713–1122,
//! do.c:2251–2294, teleport.c:53–275,578–712, makemon.c:1179–1299.
//! Global IDs/wrap, mextra/pet abuse, apparent hero position, unique
//! substitution, burial/contents and Rider/Ice families remain separate owners.
use crate::{ObjectTimerKind, ObjectTimerTarget, SimulationWorld};
use nethacked_arena::{ActorSex, ItemId, ItemLocation};
use nethacked_data::{
    corpses::CorpseDefinition,
    monsters::{CorpseThawPolicy, CorpseTimeoutPolicy},
    ruleset::MonsterDef,
    ItemKindId,
};
use nethacked_types::{BranchId, Coord, Tile, COLNO, ROWNO};
use rand::Rng;

impl SimulationWorld {
    /// Expiry was consumed and reported before this callback. Unsupported
    /// families remain neutral; ordinary failures reschedule fresh five-d50 rot.
    pub(crate) fn revive_ordinary_troll(&mut self, id: ItemId) {
        let Some(item) = self.arena.items.get(id).cloned().filter(|i| i.is_corpse()) else {
            return;
        };
        let rs = std::sync::Arc::clone(&self.ruleset);
        let CorpseDefinition::Owned(body) = rs.corpse_body_definition(&item) else {
            return;
        };
        if body.glyph != 'T'
            || body.corpse_timeout_policy != CorpseTimeoutPolicy::Ordinary
            || body.corpse_thaw_policy != CorpseThawPolicy::Ordinary
        {
            return;
        }
        if self.try_revive_troll(id, &item, body) {
            return;
        }
        let sum: i64 = (0..5)
            .map(|_| i64::from(self.rng.random_range(0..50u32)) + 1)
            .sum();
        let delay = (sum - i64::from(item.corpse_age)).max(1) as u64;
        let target = ObjectTimerTarget::Active(id);
        if !self
            .object_timers
            .entries()
            .iter()
            .any(|t| t.target == target && t.kind == ObjectTimerKind::RotCorpse)
        {
            self.start_object_timer(target, ObjectTimerKind::RotCorpse, delay)
                .expect("revival rot deadline fits");
        }
    }

    fn revival_origin(&mut self, item: &nethacked_arena::ItemRecord) -> Option<Coord> {
        let mut location = item.location.clone();
        let mut parents = Vec::new();
        while let ItemLocation::InContainer(parent) = location {
            if parents.contains(&parent) {
                return None;
            }
            parents.push(parent);
            location = self.arena.items.get(parent)?.location.clone();
        }
        let coord = match location {
            ItemLocation::Floor(coord) => coord,
            ItemLocation::CarriedBy(actor) => {
                self.arena.actors.get(actor).filter(|a| !a.is_dead)?.coord
            }
            _ => return None,
        };
        if coord.x == 0 || coord.x >= COLNO || coord.y >= ROWNO {
            return None;
        }
        if let Some(immediate) = parents.first() {
            let container = &self.arena.items[*immediate];
            // get_container_location counts enclosing parents of the immediate
            // container, not the corpse's immediate link itself.
            if container.locked
                || parents.len() > 3
                || self
                    .ruleset
                    .item(&container.name)
                    .is_some_and(|d| d.id == Some(ItemKindId::STATUE))
                || (container.is_bag_of_holding && self.rng.random_range(0..40u32) != 0)
            {
                return None;
            }
        }
        Some(coord)
    }

    fn try_revive_troll(
        &mut self,
        id: ItemId,
        item: &nethacked_arena::ItemRecord,
        body: &MonsterDef,
    ) -> bool {
        let Some(mut coord) = self.revival_origin(item) else {
            return false;
        };
        // C MON_AT excludes the hero. BODY placement precedes norevive.
        if self
            .arena
            .actors
            .iter()
            .any(|(id, a)| id != self.player_id && !a.is_dead && a.coord == coord)
        {
            if let Some(next) = self.monster_adjacent(coord, body) {
                coord = next;
            }
        }
        if item.corpse_no_revive || !self.arena.items_in_container(id).is_empty() {
            return false;
        }
        let rs = std::sync::Arc::clone(&self.ruleset);
        let CorpseDefinition::Owned(def) = rs.corpse_revival_definition(item) else {
            return false;
        };
        if item
            .saved_monster_traits
            .as_ref()
            .is_some_and(|t| t.actor.max_hp == 0)
        {
            return false;
        }
        if self.arena.actors[self.player_id].coord == coord {
            let Some(next) = self.monster_adjacent(coord, def) else {
                return false;
            };
            coord = next;
        }
        if self.actor_at(coord).is_some() || self.actor_is_genocided(&def.name) {
            return false;
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
        let (context, _) = self.item_birth_context(self.current_branch, self.depth, false);
        let plane = self.monster_spawn_context();
        let mut dummy = rs.create_revival_record_from_def(
            def,
            coord,
            plane,
            (context.level_difficulty, context.hero_level),
            forced,
            &mut self.rng,
        );
        let input = crate::peace::peace_input(
            def,
            self.hero_alignment(),
            self.hero_race,
            self.alignment_record,
            self.hero_has_amulet(),
        );
        dummy.is_peaceful = crate::peace::roll_peace_minded(&input, &mut self.rng);
        self.set_monster_malign(&mut dummy, def);
        let actor = if let Some(traits) = &item.saved_monster_traits {
            let mut restored = traits.actor.clone();
            if dummy.level < def.level {
                let repair = self.rng.random_range(0..def.level + 1) + 1;
                if repair > dummy.level {
                    while dummy.level < repair {
                        dummy.level += 1;
                        dummy.max_hp += self.rng.random_range(0..8u32) + 1;
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
        self.normalize_revival_actor_allocator();
        self.arena.spawn_actor(actor);
        let target = ObjectTimerTarget::Active(id);
        if item.quantity > 1 {
            let _split_identity = self.rng.random_range(0..2u32);
            self.consume_split_timer_serials(target);
            self.arena.items[id].quantity -= 1;
        } else {
            self.cancel_object_timers(target);
            self.arena.destroy_item(id);
            self.forget_rot_corpse_references(id);
        }
        true
    }

    pub(crate) fn monster_adjacent(&mut self, origin: Coord, def: &MonsterDef) -> Option<Coord> {
        for scary in [true, false] {
            let near = self.monster_placement_rings(origin, 3);
            if let Some(coord) = near
                .iter()
                .copied()
                .find(|c| self.monster_goodpos(*c, def, scary))
            {
                return Some(coord);
            }
            let all = self.monster_placement_rings(origin, COLNO.max(ROWNO));
            if let Some(coord) = all
                .into_iter()
                .skip(near.len())
                .find(|c| self.monster_goodpos(*c, def, scary))
            {
                return Some(coord);
            }
        }
        None
    }

    fn monster_placement_rings(&mut self, origin: Coord, radius: usize) -> Vec<Coord> {
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
                let k = self.rng.random_range(0..(ring.len() - i) as u32) as usize;
                ring.swap(i, i + k);
            }
            all.extend(ring);
        }
        all
    }

    pub(crate) fn monster_goodpos(&self, coord: Coord, def: &MonsterDef, scary: bool) -> bool {
        self.monster_goodpos_in_branch(coord, def, scary, self.current_branch)
    }

    pub(crate) fn monster_goodpos_in_branch(
        &self,
        coord: Coord,
        def: &MonsterDef,
        scary: bool,
        branch: BranchId,
    ) -> bool {
        if self.actor_at(coord).is_some() {
            return false;
        }
        let tile = self.level.get_tile(coord);
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
            let scroll = self.arena.items.iter().any(|(_, i)| {
                i.location == ItemLocation::Floor(coord)
                    && self
                        .ruleset
                        .item(&i.name)
                        .is_some_and(|d| d.id == Some(ItemKindId::SCR_SCARE_MONSTER))
            });
            let ward = !matches!(branch, BranchId::Gehennom | BranchId::AstralPlane)
                && !def.has_flag("noeyes")
                && self
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
        !self.arena.items.iter().any(|(_, i)| {
            i.location == ItemLocation::Floor(coord)
                && self
                    .ruleset
                    .item(&i.name)
                    .is_some_and(|d| d.id == Some(ItemKindId::BOULDER))
                && !def.has_flag("rockthrow")
        })
    }
}
