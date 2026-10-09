//! Detached, exhaustively partitioned native transfer state. Runtime ownership
//! is installed separately; this type never executes gameplay commands.
use crate::world::BillingState as NativeBillingState;
use crate::{GameEvent, LoadError, SimulationWorld, StoredLevel};
use nethacked_arena::{ActorId, EntityArena, ItemId};
use nethacked_core::{energy::SchedulerState, SpellKind};
use nethacked_data::{RoleId, Ruleset, RulesetRef};
use nethacked_dungeon::DungeonLevel;
use nethacked_types::Coord;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Temporary typed transfer from a completed native import. Contains nine
/// separate payloads and no World. Stored-level routing is owned by Geography;
/// the other payloads retain ordered vectors indexed by that single routing.
/// Live commands will use independently enrolled owners, never this container.
#[derive(Debug, Clone)]
pub struct NativePartitions {
    pub(super) geography: GeographyState,
    pub(super) objects: ObjectsState,
    pub(super) actors: ActorsState,
    pub(super) hero: HeroState,
    pub(super) equipment: EquipmentState,
    pub(super) billing: BillingState,
    pub(super) clock: ClockState,
    pub(super) random: RandomState,
    pub(super) session: SessionState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GeographyState {
    pub(super) level: DungeonLevel,
    pub(super) levels: Vec<DungeonLevel>,
    pub(super) current_branch: nethacked_types::BranchId,
    pub(super) depth: usize,
    pub(super) bones_storage: Vec<nethacked_types::BonesData>,
    pub(super) vibrating_square: Option<Coord>,
    pub(super) stored_keys: Vec<(nethacked_types::BranchId, usize)>,
    pub(super) stored_levels: Vec<DungeonLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ObjectsState {
    pub(super) items: nethacked_arena::ItemStorage,
    pub(super) artifact_history: std::collections::BTreeSet<String>,
    pub(super) stored_items: Vec<Vec<(ItemId, nethacked_arena::ItemRecord)>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActorsState {
    pub(super) actors: nethacked_arena::ActorStorage,
    pub(super) stored_monsters: Vec<Vec<(ActorId, nethacked_arena::ActorRecord)>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HeroState {
    pub(super) player_id: ActorId,
    pub(super) hero: nethacked_types::Hero,
    pub(super) player_nutrition: i32,
    pub(super) player_pw: u32,
    pub(super) player_max_pw: u32,
    pub(super) known_spells: Vec<(SpellKind, u32)>,
    pub(super) divine_state: nethacked_types::DivineState,
    pub(super) divine_protection: u32,
    pub(super) priest_cheapskate: u32,
    pub(super) player_luck: i32,
    pub(super) hero_race: nethacked_data::RaceId,
    pub(super) alignment_record: i32,
    pub(super) progress: crate::progress::HeroProgress,
    pub(super) genocide_registry: nethacked_types::GenocideRegistry,
    pub(super) conducts: nethacked_types::ConductTracker,
    pub(super) candelabrum_state: nethacked_core::CandelabrumState,
    pub(super) ritual_progress: nethacked_core::RitualProgress,
    pub(super) quest_state: nethacked_core::QuestState,
    pub(super) mysterious_force_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EquipmentState {
    pub(super) wielded_item: Option<ItemId>,
    pub(super) equipment: crate::actions::equipment::EquipmentState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BillingState {
    pub(super) unpaid_items: Vec<(ItemId, u32)>,
    pub(super) billing: NativeBillingState,
    pub(super) player_gold: u32,
    pub(super) stored_unpaid_items: Vec<Vec<(ItemId, u32)>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ClockState {
    pub(super) scheduler: SchedulerState,
    pub(super) object_timers: crate::object_timers::ObjectTimerQueue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RandomState {
    pub(super) rng: ChaCha8Rng,
    pub(super) seed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SessionState {
    pub(super) ruleset_ref: RulesetRef,
    pub(super) locale: nethacked_types::Locale,
    pub(super) role_name: String,
    pub(super) role_id: Option<RoleId>,
    pub(super) event_log: Vec<GameEvent>,
    pub(super) max_event_log_len: Option<usize>,
}

impl NativePartitions {
    pub(super) fn from_native(world: SimulationWorld) -> Self {
        let SimulationWorld {
            ruleset: _,
            ruleset_ref,
            level,
            levels,
            current_branch,
            stored_levels,
            depth,
            arena,
            player_id,
            wielded_item,
            equipment,
            scheduler,
            object_timers,
            unpaid_items,
            billing,
            player_gold,
            player_nutrition,
            player_pw,
            player_max_pw,
            known_spells,
            divine_state,
            divine_protection,
            priest_cheapskate,
            player_luck,
            hero,
            locale,
            bones_storage,
            candelabrum_state,
            ritual_progress,
            vibrating_square,
            quest_state,
            alignment_record,
            hero_race,
            mysterious_force_count,
            role_name,
            role_id,
            rng,
            seed,
            event_log,
            max_event_log_len,
            genocide_registry,
            conducts,
            progress,
        } = world;
        // Exhaustive destructuring also protects arena/cached fields from loss.
        let EntityArena {
            items,
            actors,
            artifact_history,
        } = arena;
        let mut stored_keys = Vec::with_capacity(stored_levels.len());
        let mut cached_levels = Vec::with_capacity(stored_levels.len());
        let mut stored_items = Vec::with_capacity(stored_levels.len());
        let mut stored_monsters = Vec::with_capacity(stored_levels.len());
        let mut stored_unpaid_items = Vec::with_capacity(stored_levels.len());
        for (key, stored) in stored_levels {
            let StoredLevel {
                level,
                monsters,
                items,
                unpaid_items,
            } = stored;
            stored_keys.push(key);
            cached_levels.push(level);
            stored_items.push(items);
            stored_monsters.push(monsters);
            stored_unpaid_items.push(unpaid_items);
        }
        Self {
            geography: GeographyState {
                level,
                levels,
                current_branch,
                depth,
                bones_storage,
                vibrating_square,
                stored_keys,
                stored_levels: cached_levels,
            },
            objects: ObjectsState {
                items,
                artifact_history,
                stored_items,
            },
            actors: ActorsState {
                actors,
                stored_monsters,
            },
            hero: HeroState {
                player_id,
                hero,
                player_nutrition,
                player_pw,
                player_max_pw,
                known_spells,
                divine_state,
                divine_protection,
                priest_cheapskate,
                player_luck,
                hero_race,
                alignment_record,
                progress,
                genocide_registry,
                conducts,
                candelabrum_state,
                ritual_progress,
                quest_state,
                mysterious_force_count,
            },
            equipment: EquipmentState {
                wielded_item,
                equipment,
            },
            billing: BillingState {
                unpaid_items,
                billing,
                player_gold,
                stored_unpaid_items,
            },
            clock: ClockState {
                scheduler,
                object_timers,
            },
            random: RandomState { rng, seed },
            session: SessionState {
                ruleset_ref,
                locale,
                role_name,
                role_id,
                event_log,
                max_event_log_len,
            },
        }
    }

    /// Reassemble detached native compatibility state without scheduling,
    /// normalization or catchup. The host supplies its approved immutable ruleset.
    pub fn into_native(
        self,
        ruleset: Arc<Ruleset>,
        approved: &RulesetRef,
    ) -> Result<SimulationWorld, LoadError> {
        if self.session.ruleset_ref != *approved {
            return Err(LoadError::RulesetMismatch {
                expected: Box::new(approved.clone()),
                found: Box::new(self.session.ruleset_ref),
            });
        }
        let count = self.geography.stored_keys.len();
        if [
            self.geography.stored_levels.len(),
            self.objects.stored_items.len(),
            self.actors.stored_monsters.len(),
            self.billing.stored_unpaid_items.len(),
        ]
        .into_iter()
        .any(|n| n != count)
        {
            return Err(LoadError::Json(
                "owner cached routing length mismatch".into(),
            ));
        }
        let mut stored_levels = Vec::with_capacity(count);
        for ((((key, level), items), monsters), unpaid_items) in self
            .geography
            .stored_keys
            .into_iter()
            .zip(self.geography.stored_levels)
            .zip(self.objects.stored_items)
            .zip(self.actors.stored_monsters)
            .zip(self.billing.stored_unpaid_items)
        {
            stored_levels.push((
                key,
                StoredLevel {
                    level,
                    items,
                    monsters,
                    unpaid_items,
                },
            ));
        }
        Ok(SimulationWorld {
            ruleset,
            stored_levels,
            arena: EntityArena {
                items: self.objects.items,
                actors: self.actors.actors,
                artifact_history: self.objects.artifact_history,
            },
            level: self.geography.level,
            levels: self.geography.levels,
            current_branch: self.geography.current_branch,
            depth: self.geography.depth,
            bones_storage: self.geography.bones_storage,
            vibrating_square: self.geography.vibrating_square,
            player_id: self.hero.player_id,
            hero: self.hero.hero,
            player_nutrition: self.hero.player_nutrition,
            player_pw: self.hero.player_pw,
            player_max_pw: self.hero.player_max_pw,
            known_spells: self.hero.known_spells,
            divine_state: self.hero.divine_state,
            divine_protection: self.hero.divine_protection,
            priest_cheapskate: self.hero.priest_cheapskate,
            player_luck: self.hero.player_luck,
            hero_race: self.hero.hero_race,
            alignment_record: self.hero.alignment_record,
            progress: self.hero.progress,
            genocide_registry: self.hero.genocide_registry,
            conducts: self.hero.conducts,
            candelabrum_state: self.hero.candelabrum_state,
            ritual_progress: self.hero.ritual_progress,
            quest_state: self.hero.quest_state,
            mysterious_force_count: self.hero.mysterious_force_count,
            wielded_item: self.equipment.wielded_item,
            equipment: self.equipment.equipment,
            unpaid_items: self.billing.unpaid_items,
            billing: self.billing.billing,
            player_gold: self.billing.player_gold,
            scheduler: self.clock.scheduler,
            object_timers: self.clock.object_timers,
            rng: self.random.rng,
            seed: self.random.seed,
            ruleset_ref: self.session.ruleset_ref,
            locale: self.session.locale,
            role_name: self.session.role_name,
            role_id: self.session.role_id,
            event_log: self.session.event_log,
            max_event_log_len: self.session.max_event_log_len,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::owner_persistence::PreparedNativeImport;
    use crate::world::BillTerms;
    use nethacked_arena::ItemLocation;
    use nethacked_data::MonsterSpeciesId;
    use rand::RngCore;

    fn fixture() -> (NativePartitions, Arc<Ruleset>, RulesetRef) {
        let mut w = SimulationWorld::new_with_seed(804);
        w.hero.skills.skills.clear();
        w.player_gold = 171;
        w.player_nutrition = 613;
        w.player_luck = -3;
        w.priest_cheapskate = 7;
        w.max_event_log_len = Some(43);
        let name = &w
            .ruleset
            .monster_by_id(MonsterSpeciesId::HUMAN)
            .unwrap()
            .name;
        let item = w
            .ruleset
            .create_corpse_record(name, ItemLocation::CarriedBy(w.player_id))
            .unwrap();
        // Vacant generations in both arenas must survive separate codecs.
        let dead_item = w.arena.spawn_item(item.clone());
        w.arena.items.remove(dead_item);
        let live_item = w.arena.spawn_item(item.clone());
        let dead_item = w.arena.spawn_item(item.clone());
        w.arena.items.remove(dead_item);
        let actor = w.arena.actors[w.player_id].clone();
        let dead_actor = w.arena.spawn_actor(actor);
        w.arena.actors.remove(dead_actor);
        let wand = w
            .ruleset
            .create_item_record(
                "wand of fire",
                ItemLocation::CarriedBy(w.player_id),
                nethacked_types::Buc::Uncursed,
            )
            .unwrap();
        let billed_item = w.arena.spawn_item(wand);
        w.unpaid_items.push((billed_item, 19));
        w.record_bill_terms(
            billed_item,
            BillTerms {
                billed_quantity: Some(1),
                unit_price: Some(19),
                creditor: None,
                legacy_base_cost: 19,
            },
        )
        .unwrap();
        for depth in [9, 3] {
            w.stored_levels.push((
                (w.current_branch, depth),
                StoredLevel {
                    level: w.level.clone(),
                    items: vec![(live_item, item.clone())],
                    monsters: vec![(w.player_id, w.arena.actors[w.player_id].clone())],
                    unpaid_items: vec![],
                },
            ));
        }
        w.start_object_timer(
            crate::ObjectTimerTarget::Active(live_item),
            crate::ObjectTimerKind::RotCorpse,
            77,
        )
        .unwrap();
        let ruleset = w.ruleset.clone();
        let reference = w.ruleset_ref.clone();
        let json = w.to_save_json().unwrap();
        let parts = PreparedNativeImport::bounded(&json, json.len(), ruleset.clone(), &reference)
            .unwrap()
            .partition()
            .unwrap();
        (parts, ruleset, reference)
    }

    #[test]
    fn nine_independent_codecs_preserve_native_state_order_and_allocation() {
        let (parts, ruleset, reference) = fixture();
        let before = parts
            .clone()
            .into_native(ruleset.clone(), &reference)
            .unwrap();
        let wire = before.to_save_json().unwrap();
        // Native serialization is the oracle for allocator-state continuation.
        let mut expected: SimulationWorld = serde_json::from_str(&wire).unwrap();
        macro_rules! codec {
            ($field:ident) => {
                serde_json::from_slice(
                    &serde_json::to_vec(&serde_json::to_value(&parts.$field).unwrap()).unwrap(),
                )
                .unwrap()
            };
        }
        let decoded = NativePartitions {
            geography: codec!(geography),
            objects: codec!(objects),
            actors: codec!(actors),
            hero: codec!(hero),
            equipment: codec!(equipment),
            billing: codec!(billing),
            clock: codec!(clock),
            random: codec!(random),
            session: codec!(session),
        };
        let mut restored = decoded.into_native(ruleset, &reference).unwrap();
        assert_eq!(
            serde_json::to_value(&restored).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
        assert_eq!(restored.to_save_json().unwrap(), wire);
        assert_eq!(restored.outstanding_bills(), before.outstanding_bills());
        assert_eq!(restored.rng.next_u64(), expected.rng.next_u64());
        assert_eq!(
            restored
                .stored_levels
                .iter()
                .map(|(key, _)| *key)
                .collect::<Vec<_>>(),
            before
                .stored_levels
                .iter()
                .map(|(key, _)| *key)
                .collect::<Vec<_>>()
        );
        let item = before.arena.items.values().next().unwrap().clone();
        let actor = before.arena.actors[before.player_id].clone();
        assert_eq!(
            restored.arena.spawn_item(item.clone()),
            expected.arena.spawn_item(item)
        );
        assert_eq!(
            restored.arena.spawn_actor(actor.clone()),
            expected.arena.spawn_actor(actor)
        );
    }

    #[test]
    fn owner_payloads_reject_foreign_fields_and_incomplete_cached_routing() {
        let (parts, ruleset, reference) = fixture();
        let mut payload = serde_json::to_value(&parts.objects).unwrap();
        payload["player_gold"] = serde_json::json!(123);
        assert!(serde_json::from_value::<ObjectsState>(payload).is_err());
        let mut truncated = parts.clone();
        truncated.objects.stored_items.pop();
        assert!(truncated.into_native(ruleset.clone(), &reference).is_err());
        let mut foreign = reference.clone();
        foreign.id.push_str("-foreign");
        assert!(matches!(
            parts.into_native(ruleset, &foreign),
            Err(LoadError::RulesetMismatch { .. })
        ));
    }
}
