//! Networked Graveyard & Shared Bones Server.
//!
//! Provides a RESTful JSON API for uploading bones files on player demise,
//! claiming bones for level generation, inspecting memorials and gravestones,
//! and tracking graveyard statistics across distributed NetRust game sessions.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use netrust_types::{BonesData, GraveRecord, GraveyardStats};
use crate::bones::headstone::render_headstone;

/// Thread-safe in-memory graveyard repository.
#[derive(Debug, Default)]
pub struct GraveyardState {
    pub bones_pool: HashMap<u32, Vec<BonesData>>,
    pub graves: Vec<GraveRecord>,
    pub total_deaths: usize,
}

pub type SharedGraveyard = Arc<Mutex<GraveyardState>>;

/// Constructs the Axum Router for the Bones & Graves REST API.
pub fn create_bones_router(state: SharedGraveyard) -> Router {
    Router::new()
        .route("/api/v1/bones", post(store_bones))
        .route("/api/v1/bones/:depth", get(fetch_bones))
        .route("/api/v1/graves", get(list_graves))
        .route("/api/v1/graves/:hero_name", get(get_grave))
        .route("/api/v1/stats", get(get_stats))
        .route("/api/v1/reset", post(reset_graveyard))
        .with_state(state)
}

/// POST /api/v1/bones: Store dead adventurer bones and produce a gravestone record.
async fn store_bones(
    State(state): State<SharedGraveyard>,
    Json(bones): Json<BonesData>,
) -> impl IntoResponse {
    let mut lock = state.lock().unwrap();
    lock.total_deaths += 1;

    let headstone = render_headstone(
        &bones.hero_name,
        bones.hero_level,
        bones.depth,
        &bones.killer,
        "2026-10-04",
        "A brave adventurer fallen",
    );

    let grave = GraveRecord {
        hero_name: bones.hero_name.clone(),
        hero_level: bones.hero_level,
        depth: bones.depth,
        killer: bones.killer.clone(),
        date: "2026-10-04".to_string(),
        epitaph: "A brave adventurer fallen".to_string(),
        ascii_headstone: headstone,
    };

    let depth = bones.depth;
    lock.bones_pool.entry(depth).or_default().push(bones);
    lock.graves.push(grave.clone());

    (StatusCode::CREATED, Json(grave))
}

/// GET /api/v1/bones/:depth: Retrieve and claim a bones file for dungeon generation.
async fn fetch_bones(
    State(state): State<SharedGraveyard>,
    Path(depth): Path<u32>,
) -> impl IntoResponse {
    let mut lock = state.lock().unwrap();
    if let Some(pool) = lock.bones_pool.get_mut(&depth) {
        if let Some(bones) = pool.pop() {
            return (StatusCode::OK, Json(Some(bones)));
        }
    }
    (StatusCode::OK, Json(None))
}

/// GET /api/v1/graves: List all memorials and gravestones.
async fn list_graves(
    State(state): State<SharedGraveyard>,
) -> impl IntoResponse {
    let lock = state.lock().unwrap();
    (StatusCode::OK, Json(lock.graves.clone()))
}

/// GET /api/v1/graves/:hero_name: Look up specific gravestone memorial.
async fn get_grave(
    State(state): State<SharedGraveyard>,
    Path(hero_name): Path<String>,
) -> impl IntoResponse {
    let lock = state.lock().unwrap();
    if let Some(grave) = lock.graves.iter().find(|g| g.hero_name.eq_ignore_ascii_case(&hero_name)) {
        (StatusCode::OK, Json(Some(grave.clone())))
    } else {
        (StatusCode::NOT_FOUND, Json(None))
    }
}

/// GET /api/v1/stats: Return aggregate graveyard statistics.
async fn get_stats(
    State(state): State<SharedGraveyard>,
) -> impl IntoResponse {
    let lock = state.lock().unwrap();
    let active_bones_count: usize = lock.bones_pool.values().map(|v| v.len()).sum();
    let mut haunted_depths: Vec<u32> = lock.bones_pool.iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(&d, _)| d)
        .collect();
    haunted_depths.sort_unstable();

    let stats = GraveyardStats {
        total_deaths: lock.total_deaths,
        active_bones_count,
        total_graves: lock.graves.len(),
        haunted_depths,
    };
    (StatusCode::OK, Json(stats))
}

/// POST /api/v1/reset: Reset state (useful for automated testing).
async fn reset_graveyard(
    State(state): State<SharedGraveyard>,
) -> impl IntoResponse {
    let mut lock = state.lock().unwrap();
    lock.bones_pool.clear();
    lock.graves.clear();
    lock.total_deaths = 0;
    (StatusCode::OK, "Graveyard reset.")
}

/// Runs the bones HTTP service listening on the given socket address.
pub async fn run_bones_server(
    addr: &str,
    state: Option<SharedGraveyard>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let shared_state = state.unwrap_or_else(|| Arc::new(Mutex::new(GraveyardState::default())));
    let app = create_bones_router(shared_state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("NetRust Graveyard & Bones Server listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
