//! Networked Bones & Graveyard Subsystem.

pub mod client;
pub mod headstone;

#[cfg(not(target_arch = "wasm32"))]
pub mod server;

pub use client::BonesClient;
pub use headstone::render_headstone;

#[cfg(not(target_arch = "wasm32"))]
pub use server::{
    create_bones_router, create_bones_router_with_token, run_bones_server, GraveyardState,
    SharedGraveyard,
};

use nethacked_sim::{GameEvent, SimulationWorld};
use nethacked_types::GraveRecord;

/// Synchronizes bones on hero demise: extracts dead hero data, corrupts equipment,
/// uploads payload to the networked bones server, and returns the generated headstone memorial.
pub fn sync_bones_on_death(
    world: &mut SimulationWorld,
    client: &BonesClient,
    killer: &str,
) -> Result<Option<GraveRecord>, String> {
    if let Some(bones) = world.save_bones(killer) {
        let grave = client.upload_bones(&bones)?;
        Ok(Some(grave))
    } else {
        Ok(None)
    }
}

/// Queries the remote bones server for bones files matching the current dungeon depth.
/// If found, loads them into local world storage and spawns the ghost and cursed relics.
pub fn load_remote_bones_for_current_depth(
    world: &mut SimulationWorld,
    client: &BonesClient,
) -> Result<Option<Vec<GameEvent>>, String> {
    let depth = world.depth as u32;
    if let Some(bones) = client.fetch_bones(depth)? {
        world.bones_storage.push(bones);
        let events = world.check_and_load_bones();
        Ok(Some(events))
    } else {
        Ok(None)
    }
}
