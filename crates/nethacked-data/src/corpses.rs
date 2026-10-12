//! Represented corpse identity/weight and C mkobj.c:388,898 species selection.
//! Initial rot/Troll recipes are transient; simulation callbacks and glob
//! conversion remain separate.
use crate::generation::{try_rndmonst_adj, MonsterGenContext};
use crate::roles::RaceId;
use crate::{MonsterDef, MonsterSpeciesId as M, Ruleset};
use nethacked_arena::{
    ActorRecord, ActorSex, CorpseSpecies, ItemLocation, ItemRecord, SavedMonsterTraits,
};
use nethacked_types::{Buc, DamageType};

/// A generated object's single initial sample. This is transient construction
/// data; the simulation's existing object queue owns installed deadlines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorpseTimeoutRecipe {
    Rot {
        sampled_at: u64,
        delay: i64,
    },
    Revive {
        sampled_at: u64,
        delay: i64,
    },
    /// Riders need their separate source replacement; never install generic rot.
    UnsupportedRider,
}

/// Construction instructions only. The simulation queue remains the single
/// persisted timer owner; replacements must retain the initial insertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorpseTimerOperation {
    Start(CorpseTimeoutRecipe),
    Cancel,
}

/// Owned lifecycle identity. Unspecified/race-only legacy corpses use ordinary
/// rot compatibility without inventing a species; bad supplied keys do not.
#[derive(Debug, Clone, Copy)]
pub enum CorpseDefinition<'a> {
    Owned(&'a MonsterDef),
    OrdinaryUnspecified,
    Unresolved,
}

fn special_corpse(def: &MonsterDef) -> bool {
    def.corpse_timeout_policy != crate::monsters::CorpseTimeoutPolicy::Ordinary || def.glyph == 'T'
}

/// Supported C KEEPTRAITS predicates. Shopkeeper mextra and the original
/// dynamic quest-leader ID are not owned yet; names/sounds cannot substitute.
fn keep_monster_traits(actor: &ActorRecord, def: &MonsterDef) -> bool {
    actor.is_tame
        || def.is_unique
        || def.glyph == 'T'
        || def.corpse_timeout_policy == crate::monsters::CorpseTimeoutPolicy::Rider
        || def
            .attacks
            .iter()
            .any(|attack| matches!(attack.ad, DamageType::Seduce | DamageType::SeduceSex))
}

/// C start_corpse_timeout checks Never before drawing, then computes ordinary
/// noise before replacing it for a revival family. Owned Troll class trials
/// replace that sample on success; norevive affects later execution only.
pub fn sample_corpse_timeout(
    species: &MonsterDef,
    context: nethacked_core::corpse_timeout::CorpseTimeoutContext,
    mut rn2: impl FnMut(u32) -> u32,
) -> Option<CorpseTimeoutRecipe> {
    try_sample_corpse_timeout(species, context, |n| {
        Ok::<_, std::convert::Infallible>(rn2(n))
    })
    .unwrap()
}

/// Fallible timeout sampling, retaining the ordinary noise and revival order.
pub fn try_sample_corpse_timeout<E>(
    species: &MonsterDef,
    context: nethacked_core::corpse_timeout::CorpseTimeoutContext,
    mut rn2: impl FnMut(u32) -> Result<u32, E>,
) -> Result<Option<CorpseTimeoutRecipe>, E> {
    use crate::monsters::CorpseTimeoutPolicy;
    if species.corpse_timeout_policy == CorpseTimeoutPolicy::Never {
        return Ok(None);
    }
    let delay = nethacked_core::corpse_timeout::try_initial_rot_delay(context, &mut rn2)?;
    Ok(Some(match species.corpse_timeout_policy {
        CorpseTimeoutPolicy::Ordinary if species.glyph == 'T' => {
            match nethacked_core::corpse_timeout::try_initial_troll_revival_delay(rn2)? {
                Some(delay) => CorpseTimeoutRecipe::Revive {
                    sampled_at: context.moves,
                    delay,
                },
                None => CorpseTimeoutRecipe::Rot {
                    sampled_at: context.moves,
                    delay,
                },
            }
        }
        CorpseTimeoutPolicy::Ordinary => CorpseTimeoutRecipe::Rot {
            sampled_at: context.moves,
            delay,
        },
        CorpseTimeoutPolicy::Rider => CorpseTimeoutRecipe::UnsupportedRider,
        CorpseTimeoutPolicy::Never => unreachable!(),
    }))
}

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
    /// The body's species controls timeout families and freeze uncancellation.
    /// Display names never replace a supplied canonical/custom corpse key.
    pub fn corpse_body_definition<'a>(&'a self, item: &ItemRecord) -> CorpseDefinition<'a> {
        let definition = match &item.corpse_species {
            Some(CorpseSpecies::Vanilla(key)) => {
                M::from_c_name(key).and_then(|id| self.monster_by_id(id))
            }
            Some(CorpseSpecies::Custom(key)) => self
                .monsters
                .iter()
                .find(|def| def.id.is_none() && def.name.eq_ignore_ascii_case(key)),
            Some(CorpseSpecies::Unspecified) => return CorpseDefinition::OrdinaryUnspecified,
            None if item.corpse_race.is_some() => return CorpseDefinition::OrdinaryUnspecified,
            None => return CorpseDefinition::Unresolved,
        };
        definition.map_or(CorpseDefinition::Unresolved, CorpseDefinition::Owned)
    }

    /// C pickup.c:2790–2797 uses saved monster species first for thaw norevive;
    /// mkobj.c:1401–1425 still samples the timeout family from the body's species.
    pub fn corpse_revival_definition<'a>(&'a self, item: &ItemRecord) -> CorpseDefinition<'a> {
        if let Some(traits) = &item.saved_monster_traits {
            // Unknown original identity cannot be replaced by body or name.
            if traits.actor.species.is_none() {
                return CorpseDefinition::Unresolved;
            }
            self.monster_for_actor(&traits.actor)
                .map_or(CorpseDefinition::Unresolved, CorpseDefinition::Owned)
        } else {
            self.corpse_body_definition(item)
        }
    }

    /// C mksobj: owned species selection, gender, then the initial timeout.
    /// Corpse gender has neuter/female/male precedence, unlike makemon.
    pub(crate) fn initialize_corpse<'a>(
        &'a self,
        item: &mut ItemRecord,
        ctx: crate::containers::ContainerContext,
        monsters: &MonsterGenContext,
        gone: impl Fn(&MonsterDef) -> bool,
        mut rn2: impl FnMut(u32) -> u32,
    ) -> Option<(&'a MonsterDef, Option<CorpseTimeoutRecipe>)> {
        self.try_initialize_corpse(item, ctx, monsters, gone, |n| {
            Ok::<_, std::convert::Infallible>(rn2(n))
        })
        .unwrap()
    }

    fn try_initialize_corpse<'a, E>(
        &'a self,
        item: &mut ItemRecord,
        ctx: crate::containers::ContainerContext,
        monsters: &MonsterGenContext,
        gone: impl Fn(&MonsterDef) -> bool,
        mut rn2: impl FnMut(u32) -> Result<u32, E>,
    ) -> Result<Option<(&'a MonsterDef, Option<CorpseTimeoutRecipe>)>, E> {
        let Some(species) = self.try_random_corpse_definition(monsters, gone, &mut rn2)? else {
            return Ok(None);
        };
        self.set_corpse_species(item, species);
        item.enchantment = if species.has_flag("neuter") {
            3
        } else if species.has_flag("female") {
            1
        } else if species.has_flag("male") {
            2
        } else if rn2(2)?.min(1) != 0 {
            1
        } else {
            2
        };
        let recipe = try_sample_corpse_timeout(
            species,
            nethacked_core::corpse_timeout::CorpseTimeoutContext {
                elapsed_age: item.corpse_age,
                moves: ctx.moves,
                hero_level: ctx.hero_level,
                in_mklev: ctx.in_mklev,
            },
            rn2,
        )?;
        Ok(Some((species, recipe)))
    }

    /// C initialized mkcorpstat: initial random corpse before the actual death
    /// overrides. Ordinary-to-ordinary retains the initial sampled timeout.
    pub fn create_initialized_corpse_for_actor(
        &self,
        actor: &ActorRecord,
        location: ItemLocation,
        ctx: crate::containers::ContainerContext,
        monsters: &MonsterGenContext,
        gone: impl Fn(&MonsterDef) -> bool,
        mut rn2: impl FnMut(u32) -> u32,
    ) -> Option<crate::containers::GeneratedItem> {
        self.try_create_initialized_corpse_for_actor(actor, location, ctx, monsters, gone, |n| {
            Ok::<_, std::convert::Infallible>(rn2(n))
        })
        .unwrap()
    }

    /// Fallible initialized mkcorpstat. Refusal retains consumed draws and
    /// returns no item or timer operations; no-corpse cases draw nothing.
    pub fn try_create_initialized_corpse_for_actor<E>(
        &self,
        actor: &ActorRecord,
        location: ItemLocation,
        ctx: crate::containers::ContainerContext,
        monsters: &MonsterGenContext,
        gone: impl Fn(&MonsterDef) -> bool,
        mut rn2: impl FnMut(u32) -> Result<u32, E>,
    ) -> Result<Option<crate::containers::GeneratedItem>, E> {
        let actual_def = self.monster_for_actor(actor);
        // C mon.c:892–900: no-corpse deaths return before CORPSTAT_INIT.
        if actual_def.is_some_and(|def| def.gen_flags.iter().any(|flag| flag == "nocorpse")) {
            return Ok(None);
        }
        let Some(mut item) = self.create_corpse_record_for_actor(actor, location) else {
            return Ok(None);
        };
        let mut operations = Vec::new();
        let mut recipe = None;
        // Unresolved legacy/scripted actors retain unspecified pure records;
        // an unrelated random species must not invent their actual identity.
        if let Some(def) = actual_def {
            let initial_special = if let Some((initial, sampled)) =
                self.try_initialize_corpse(&mut item, ctx, monsters, gone, &mut rn2)?
            {
                recipe = sampled;
                if let Some(sampled) = sampled {
                    operations.push(CorpseTimerOperation::Start(sampled));
                }
                special_corpse(initial)
            } else {
                false
            };
            // C mkcorpstat always overwrites spe, including zero gender flags.
            // Unknown sex stays unknown, with no guesses or extra sex draws.
            item.enchantment = match actor.sex {
                Some(ActorSex::Female) => 1,
                Some(ActorSex::Male) if !def.has_flag("neuter") => 2,
                Some(ActorSex::Male | ActorSex::Neuter) | None => 0,
            };
            if keep_monster_traits(actor, def) {
                item.saved_monster_traits =
                    Some(Box::new(SavedMonsterTraits::from_actor(actor, def.level)));
                if actor.is_cancelled
                    && def.corpse_timeout_policy != crate::monsters::CorpseTimeoutPolicy::Rider
                {
                    item.corpse_no_revive = true;
                }
            }
            let final_def = def
                .id
                .and_then(|id| self.monster_by_id(undead_to_corpse(id)))
                .unwrap_or(def);
            self.set_corpse_species(&mut item, final_def);
            if initial_special || special_corpse(final_def) {
                operations.push(CorpseTimerOperation::Cancel);
                recipe = try_sample_corpse_timeout(
                    final_def,
                    nethacked_core::corpse_timeout::CorpseTimeoutContext {
                        elapsed_age: item.corpse_age,
                        moves: ctx.moves,
                        hero_level: ctx.hero_level,
                        in_mklev: ctx.in_mklev,
                    },
                    rn2,
                )?;
                if let Some(sampled) = recipe {
                    operations.push(CorpseTimerOperation::Start(sampled));
                }
            }
        }
        Ok(Some(crate::containers::GeneratedItem {
            item,
            contents: Vec::new(),
            corpse_timeout: recipe,
            corpse_timer_operations: operations,
        }))
    }

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
    fn corpse_monster_number<'a, E>(
        &'a self,
        ctx: &MonsterGenContext,
        gone: &impl Fn(&MonsterDef) -> bool,
        rn2: &mut impl FnMut(u32) -> Result<u32, E>,
    ) -> Result<Option<&'a MonsterDef>, E> {
        if let Some(def) = try_rndmonst_adj(self, ctx, 0, 0, gone, &mut *rn2)? {
            return Ok(Some(def));
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
            let id = M(rn2(M::LONG_WORM_TAIL.index() as u32)?
                .min(M::LONG_WORM_TAIL.index() as u32 - 1) as u16);
            if let Some(def) = self.monster_by_id(id).filter(eligible) {
                return Ok(Some(def));
            }
        }
        Ok(self.monsters.iter().filter(eligible).min_by_key(|d| d.id))
    }

    /// C corpse branch: 50 attempts after undead conversion, then human. A
    /// missing owned conversion target is an invalid candidate, not a name guess.
    pub fn random_corpse_definition<'a>(
        &'a self,
        ctx: &MonsterGenContext,
        gone: impl Fn(&MonsterDef) -> bool,
        mut rn2: impl FnMut(u32) -> u32,
    ) -> Option<&'a MonsterDef> {
        self.try_random_corpse_definition(ctx, gone, |n| Ok::<_, std::convert::Infallible>(rn2(n)))
            .unwrap()
    }

    /// Fallible species selection, including every nested fallback attempt.
    pub fn try_random_corpse_definition<'a, E>(
        &'a self,
        ctx: &MonsterGenContext,
        gone: impl Fn(&MonsterDef) -> bool,
        mut rn2: impl FnMut(u32) -> Result<u32, E>,
    ) -> Result<Option<&'a MonsterDef>, E> {
        for _ in 0..50 {
            let Some(def) = self.corpse_monster_number(ctx, &gone, &mut rn2)? else {
                break;
            };
            let def = match def.id {
                Some(id) => self.monster_by_id(undead_to_corpse(id)),
                None => Some(def),
            };
            if let Some(def) = def.filter(|d| !d.gen_flags.iter().any(|f| f == "nocorpse")) {
                return Ok(Some(def));
            }
        }
        Ok(self.monster_by_id(M::HUMAN))
    }
}
