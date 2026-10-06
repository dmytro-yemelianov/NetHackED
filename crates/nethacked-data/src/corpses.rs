//! Represented corpse identity/weight and C mkobj.c:388,898 species selection.
//! Revival, glob conversion and rot-deletion timers are not represented.
use crate::generation::{rndmonst, MonsterGenContext};
use crate::roles::RaceId;
use crate::{MonsterDef, MonsterSpeciesId as M, Ruleset};
use nethacked_arena::{CorpseSpecies, ItemLocation, ItemRecord};
use nethacked_types::Buc;

/// C mon.c:417 undead_to_corpse: dispatch by canonical identity, never name.
pub fn undead_to_corpse(id: M) -> M {
    match id {
        M::KOBOLD_ZOMBIE | M::KOBOLD_MUMMY => M::KOBOLD,
        M::DWARF_ZOMBIE | M::DWARF_MUMMY => M::DWARF,
        M::GNOME_ZOMBIE | M::GNOME_MUMMY => M::GNOME,
        M::ORC_ZOMBIE | M::ORC_MUMMY => M::ORC,
        M::ELF_ZOMBIE | M::ELF_MUMMY => M::ELF,
        M::VAMPIRE | M::VAMPIRE_LEADER | M::HUMAN_ZOMBIE | M::HUMAN_MUMMY => M::HUMAN,
        M::GIANT_ZOMBIE | M::GIANT_MUMMY => M::GIANT,
        M::ETTIN_ZOMBIE | M::ETTIN_MUMMY => M::ETTIN,
        _ => id,
    }
}

impl Ruleset {
    /// Apply owned corpse facts without modifying BUC, age or other food state.
    pub fn set_corpse_species(&self, item: &mut ItemRecord, def: &MonsterDef) {
        item.corpse_species = Some(match def.id {
            Some(id) => CorpseSpecies::Vanilla(id.c_name().to_owned()),
            None => CorpseSpecies::Custom(def.name.clone()),
        });
        item.corpse_race = def.m2_race.map(|race| {
            match race {
                RaceId::Human => "human",
                RaceId::Elf => "elf",
                RaceId::Dwarf => "dwarf",
                RaceId::Gnome => "gnome",
                RaceId::Orc => "orc",
            }
            .to_owned()
        });
        item.weight =
            nethacked_core::item_init::corpse_stack_weight(def.body_weight, item.quantity);
    }

    /// RNG-free death/scripted factory. Unknown species retain the old catalog
    /// weight and an unspecified payload rather than guessing from a display name.
    pub fn create_corpse_record(&self, name: &str, location: ItemLocation) -> Option<ItemRecord> {
        let mut item =
            self.create_item_record_by_id(crate::ItemKindId::CORPSE, location, Buc::Uncursed)?;
        if let Some(def) = self.monster(name) {
            let def = def
                .id
                .and_then(|id| self.monster_by_id(undead_to_corpse(id)))
                .unwrap_or(def);
            self.set_corpse_species(&mut item, def);
        }
        Some(item)
    }

    /// Death factory using authoritative actor identity, including renamed actors.
    pub fn create_corpse_record_for_actor(
        &self,
        actor: &nethacked_arena::ActorRecord,
        location: ItemLocation,
    ) -> Option<ItemRecord> {
        let mut item =
            self.create_item_record_by_id(crate::ItemKindId::CORPSE, location, Buc::Uncursed)?;
        if let Some(def) = self.monster_for_actor(actor) {
            let def = def
                .id
                .and_then(|id| self.monster_by_id(undead_to_corpse(id)))
                .unwrap_or(def);
            self.set_corpse_species(&mut item, def);
        }
        Some(item)
    }

    /// C rndmonnum: actual level-context selection, then uniform canonical range
    /// rejection when no level-appropriate common monster exists. Plan B ignores
    /// genocide/extinction like C. Cap at 256 trials for malformed pack flags;
    /// then use the first eligible canonical species, without another draw.
    fn corpse_monster_number<'a>(
        &'a self,
        ctx: &MonsterGenContext,
        gone: &impl Fn(&MonsterDef) -> bool,
        rn2: &mut impl FnMut(u32) -> u32,
    ) -> Option<&'a MonsterDef> {
        if let Some(def) = rndmonst(self, ctx, gone, &mut *rn2) {
            return Some(def);
        }
        let eligible = |def: &&MonsterDef| {
            def.id.is_some_and(|id| id < M::LONG_WORM_TAIL)
                && !def.gen_flags.iter().any(|flag| {
                    flag == "uniq"
                        || flag == "nogen"
                        || flag == if ctx.in_hell { "nohell" } else { "hell" }
                })
        };
        for _ in 0..256 {
            let id = M(rn2(M::LONG_WORM_TAIL.index() as u32)
                .min(M::LONG_WORM_TAIL.index() as u32 - 1) as u16);
            if let Some(def) = self.monster_by_id(id).filter(eligible) {
                return Some(def);
            }
        }
        self.monsters.iter().filter(eligible).min_by_key(|d| d.id)
    }

    /// C corpse branch: 50 attempts after undead conversion, then human. A
    /// missing owned conversion target is an invalid candidate, not a name guess.
    pub fn random_corpse_definition<'a>(
        &'a self,
        ctx: &MonsterGenContext,
        gone: impl Fn(&MonsterDef) -> bool,
        mut rn2: impl FnMut(u32) -> u32,
    ) -> Option<&'a MonsterDef> {
        for _ in 0..50 {
            let Some(def) = self.corpse_monster_number(ctx, &gone, &mut rn2) else {
                break;
            };
            let def = match def.id {
                Some(id) => self.monster_by_id(undead_to_corpse(id)),
                None => Some(def),
            };
            if let Some(def) = def.filter(|d| !d.gen_flags.iter().any(|f| f == "nocorpse")) {
                return Some(def);
            }
        }
        self.monster_by_id(M::HUMAN)
    }
}
