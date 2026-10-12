//! M6-I10 owner-backed game session facade.
//!
//! The world is imported into the independent owners exactly once at
//! construction (or `reset`); every `ActionAst` branch dispatches through
//! owner execution via [`OwnerBridge::dispatch_default_action`], and the
//! native world is re-exported after each step for observation only. Gameplay
//! never falls back to reconstructing a `SimulationWorld` for execution.

use super::{OwnerBridge, OwnerBridgeConfig, OwnerBridgeError};
use crate::owner_persistence::NativePartitions;
use crate::{ActionAst, GameEvent, SimulationWorld};

/// Owner-backed game session: initialize/import once, dispatch every action
/// through the owner runtime, no gameplay World reconstruction fallback.
///
/// The full I1-I9 profile chain (`with_traversal`) is installed, so every
/// `ActionAst` branch reaches owner execution. The session event log is
/// carried forward across steps exactly like the legacy `step_player_action`
/// `record_events` path: the gameplay transports keep the Session owner
/// read-only, so the facade owns the cumulative log and appends each reply's
/// events under the world's configured cap.
pub struct OwnerGameSession {
    bridge: OwnerBridge,
    config: OwnerBridgeConfig,
    world: SimulationWorld,
}

impl OwnerGameSession {
    /// Import `world` into the owner runtime once and keep it as the
    /// read-only observation surface.
    pub fn new(world: SimulationWorld) -> Result<Self, OwnerBridgeError> {
        let config = OwnerBridgeConfig::new(world.ruleset.clone(), world.ruleset_ref.clone())?
            .with_traversal();
        let bridge =
            OwnerBridge::install(NativePartitions::from_native(world.clone()), config.clone())?;
        Ok(Self {
            bridge,
            config,
            world,
        })
    }

    /// Re-import a replacement world (new game). The previous owner runtime
    /// is dropped and the new world is imported exactly once.
    pub fn reset(&mut self, world: SimulationWorld) -> Result<(), OwnerBridgeError> {
        let config = OwnerBridgeConfig::new(world.ruleset.clone(), world.ruleset_ref.clone())?
            .with_traversal();
        self.bridge =
            OwnerBridge::install(NativePartitions::from_native(world.clone()), config.clone())?;
        self.config = config;
        self.world = world;
        Ok(())
    }

    /// Dispatch one action through owner execution. A transport refusal
    /// returns an error and leaves the session state untouched; a successful
    /// action re-exports the native world and records the reply's events.
    /// The frame's `reply.error` path (unreachable under the facade's full
    /// authority) preserves the committed prefix before surfacing an
    /// invariant error.
    pub fn step(&mut self, action: ActionAst) -> Result<Vec<GameEvent>, OwnerBridgeError> {
        let reply = self.bridge.dispatch_default_action(action)?;
        let mut world = self.bridge.export_native()?;
        // The gameplay transports keep the Session event log read-only, so
        // carry the cumulative log forward and record exactly like the legacy
        // `step_player_action` did (the bridge's own Session log is discarded;
        // it only ever gains the I1 cancellation receipt).
        world.event_log = std::mem::take(&mut self.world.event_log);
        world.record_events(&reply.events);
        self.world = world;
        if let Some(_error) = reply.error {
            // Unreachable under the facade's full authority: the frame only
            // reports an error when a delegated scope ceiling refuses the
            // Session publication. The committed prefix is preserved above;
            // surface the violation as an invariant error.
            return Err(OwnerBridgeError::Owner(nethacked_owners::Error::Invariant));
        }
        Ok(reply.events)
    }

    /// Change the active locale. The Session owner holds the locale, so the
    /// world is re-imported once (the same contract as `reset`); gameplay
    /// still dispatches through owners afterwards.
    pub fn set_locale(&mut self, locale: nethacked_types::Locale) -> Result<(), OwnerBridgeError> {
        self.world.set_locale(locale);
        let config = self.config.clone();
        self.bridge =
            OwnerBridge::install(NativePartitions::from_native(self.world.clone()), config)?;
        Ok(())
    }

    /// Read-only observation surface: the native world as of the last step.
    pub fn world(&self) -> &SimulationWorld {
        &self.world
    }
}
