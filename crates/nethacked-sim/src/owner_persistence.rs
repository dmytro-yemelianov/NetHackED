//! Native save compatibility boundary and current-schema runtime owner bridge.
//!
//! The completed import can be moved into nine separate typed codec payloads.
//! The transfer installs in independently enrolled typed owners using D4 session
//! composition. The caller supplies the approved immutable ruleset/reference.
//! Native live-reference admission precedes executable restore factories.
//! Native parser/normalization selection uses a host-approved exact migration
//! registry. Gameplay continuation beyond receipt admission migrates separately.
//! Public legacy loading shares preparation/catchup but retains its original
//! acceptance contract without post-catchup rejection. Explicit completion
//! here can refuse a legacy input whose timer effects invalidate its receipts.

mod admission;
mod migration;
pub use migration::{
    native_migration_binding, native_wand_gameplay_migration_binding, NativeImportRegistry,
};
mod partitions;
pub use partitions::NativePartitions;
mod runtime;
pub mod wand_ports;
pub use runtime::{OwnerBridge, OwnerBridgeConfig, OwnerBridgeError};

use std::sync::Arc;

use crate::{LoadError, Ruleset, RulesetRef, SimulationWorld};

/// Validated and normalized native import with timer catchup deferred.
///
/// State is private and cannot be mutated or extracted before completion.
/// Native ruleset/receipt/queue validation precedes absence/tag/artifact/
/// equipment normalization and timer restoration. Queue checks do not need tag
/// normalization. No user or registry migration runs in this boundary.
///
/// Preparation may perform the legacy absent-timer RNG sampling, but never
/// dispatches due timers. Completion consumes the handle, catches up on hidden
/// state exactly once and revalidates native billing and queue invariants.
/// This is native validation, not future host/owner admission.
pub struct PreparedNativeImport {
    world: SimulationWorld,
}

impl PreparedNativeImport {
    /// Check UTF-8 byte length before any parsing or decoded-state allocation.
    /// Native unknown-field, duplicate-field and default policies are preserved.
    pub fn bounded(
        json: &str,
        byte_limit: usize,
        ruleset: Arc<Ruleset>,
        ruleset_ref: &RulesetRef,
    ) -> Result<Self, LoadError> {
        if json.len() > byte_limit {
            return Err(LoadError::Json("native save exceeds byte limit".into()));
        }
        Self::unbounded(json, ruleset, ruleset_ref)
    }

    pub(crate) fn unbounded(
        json: &str,
        ruleset: Arc<Ruleset>,
        ruleset_ref: &RulesetRef,
    ) -> Result<Self, LoadError> {
        Ok(Self {
            world: SimulationWorld::prepare_native_import(json, ruleset, ruleset_ref)?,
        })
    }

    /// Complete the compatibility import and move every native field into its
    /// typed domain payload. No live World is retained by the returned transfer.
    pub fn partition(self) -> Result<NativePartitions, LoadError> {
        self.partition_with_policy(admission::AdmissionPolicy::Strict)
    }

    fn partition_with_policy(
        self,
        policy: admission::AdmissionPolicy,
    ) -> Result<NativePartitions, LoadError> {
        NativePartitions::from_native(self.world.clone())
            .admit_references_with_policy(&self.world.ruleset, policy)
            .map_err(|e| LoadError::Json(format!("native owner references: {e:?}")))?;
        let ruleset = self.world.ruleset.clone();
        let parts = NativePartitions::from_native(self.complete()?);
        parts
            .admit_references_with_policy(&ruleset, policy)
            .map_err(|e| LoadError::Json(format!("native owner references: {e:?}")))?;
        Ok(parts)
    }

    /// Complete native catchup and yield detached compatibility state.
    /// The owner bridge must transfer this state into owners and discard it;
    /// it must never retain this World to execute live owner commands.
    pub fn complete(self) -> Result<SimulationWorld, LoadError> {
        self.world.complete_native_import()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ObjectTimerKind, ObjectTimerTarget};
    use nethacked_arena::ItemLocation;
    use nethacked_data::MonsterSpeciesId;
    use rand::RngCore;

    #[test]
    fn preparation_keeps_due_fixture_hidden_and_undispatched() {
        // Same due corpse fixture as object_timer_tests' load catchup test.
        let mut world = SimulationWorld::new_with_seed(804);
        world.arena.actors.retain(|id, _| id == world.player_id);
        world.arena.items.clear();
        world.equipment = Default::default();
        world.wielded_item = None;
        let name = &world
            .ruleset
            .monster_by_id(MonsterSpeciesId::HUMAN)
            .unwrap()
            .name;
        let corpse = world
            .ruleset
            .create_corpse_record(name, ItemLocation::Limbo)
            .unwrap();
        let item = world.arena.spawn_item(corpse);
        world
            .start_object_timer(
                ObjectTimerTarget::Active(item),
                ObjectTimerKind::RotCorpse,
                0,
            )
            .unwrap();
        let json = world.to_save_json().unwrap();
        let prepared = PreparedNativeImport::bounded(
            &json,
            json.len(),
            world.ruleset.clone(),
            &world.ruleset_ref,
        )
        .unwrap();
        assert!(prepared.world.arena.items.contains_key(item));
        assert_eq!(prepared.world.object_timers.entries().len(), 1);
        assert_eq!(
            serde_json::to_value(&prepared.world.event_log).unwrap(),
            serde_json::to_value(&world.event_log).unwrap()
        );
        assert_eq!(
            prepared.world.rng.clone().next_u64(),
            world.rng.clone().next_u64()
        );
        let complete = prepared.complete().unwrap();
        assert!(!complete.arena.items.contains_key(item));
        assert!(complete.object_timers.entries().is_empty());
    }
}
