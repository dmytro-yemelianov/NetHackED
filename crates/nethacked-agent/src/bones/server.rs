//! Networked Graveyard & Shared Bones Server.
//!
//! Provides a RESTful JSON API for uploading bones files on player demise,
//! claiming bones for level generation, inspecting memorials and gravestones,
//! and tracking graveyard statistics across distributed NetHackED game sessions.

use crate::bones::headstone::render_headstone;
use crate::netconfig::bearer_ok;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use nethacked_types::{BonesData, GraveRecord, GraveyardStats};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub const MAX_NAME_CHARS: usize = 32;
pub const MAX_KILLER_CHARS: usize = 64;
pub const MAX_ITEMS: usize = 64;
pub const MAX_DEPTH: u32 = 60;
pub const MAX_BONES_PER_DEPTH: usize = 16;
pub const MAX_GRAVES: usize = 1000;

/// Thread-safe in-memory graveyard repository.
#[derive(Debug, Default)]
pub struct GraveyardState {
    pub bones_pool: HashMap<u32, Vec<BonesData>>,
    pub graves: Vec<GraveRecord>,
    pub total_deaths: usize,
}

pub type SharedGraveyard = Arc<Mutex<GraveyardState>>;

#[derive(Clone)]
struct BonesApp {
    graveyard: SharedGraveyard,
    token: Option<Arc<str>>,
}

fn lock_graveyard(state: &SharedGraveyard) -> std::sync::MutexGuard<'_, GraveyardState> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn validate_bones(b: &BonesData) -> Result<(), String> {
    let name_len = b.hero_name.chars().count();
    if name_len == 0 || name_len > MAX_NAME_CHARS {
        return Err(format!("hero_name must be 1..={MAX_NAME_CHARS} characters"));
    }
    if b.killer.chars().count() > MAX_KILLER_CHARS {
        return Err(format!(
            "killer must be at most {MAX_KILLER_CHARS} characters"
        ));
    }
    if b.items.len() > MAX_ITEMS {
        return Err(format!("items must contain at most {MAX_ITEMS} entries"));
    }
    if b.depth < 1 || b.depth > MAX_DEPTH {
        return Err(format!("depth must be 1..={MAX_DEPTH}"));
    }
    Ok(())
}

/// Constructs the Axum Router for the Bones & Graves REST API, reading the
/// optional bearer token from `NETHACKED_TOKEN`.
pub fn create_bones_router(state: SharedGraveyard) -> Router {
    create_bones_router_with_token(state, crate::netconfig::token_from_env())
}

/// Constructs the router with an explicit bearer token guarding mutating routes.
pub fn create_bones_router_with_token(state: SharedGraveyard, token: Option<String>) -> Router {
    Router::new()
        .route("/api/v1/bones", post(store_bones))
        .route("/api/v1/bones/:depth", get(fetch_bones))
        .route("/api/v1/graves", get(list_graves))
        .route("/api/v1/graves/:hero_name", get(get_grave))
        .route("/api/v1/stats", get(get_stats))
        .route("/api/v1/reset", post(reset_graveyard))
        .layer(axum::extract::DefaultBodyLimit::max(64 * 1024))
        .with_state(BonesApp {
            graveyard: state,
            token: token.map(Arc::from),
        })
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": "missing or invalid bearer token"})),
    )
        .into_response()
}

/// POST /api/v1/bones: Store dead adventurer bones and produce a gravestone record.
async fn store_bones(
    State(app): State<BonesApp>,
    headers: HeaderMap,
    Json(bones): Json<BonesData>,
) -> Response {
    if !bearer_ok(&headers, app.token.as_deref()) {
        return unauthorized();
    }
    if let Err(msg) = validate_bones(&bones) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error": msg})),
        )
            .into_response();
    }

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

    let mut lock = lock_graveyard(&app.graveyard);
    let depth = bones.depth;
    let pool = lock.bones_pool.entry(depth).or_default();
    if pool.len() >= MAX_BONES_PER_DEPTH {
        return (
            StatusCode::CONFLICT,
            Json(json!({"error": "bones pool for this depth is full"})),
        )
            .into_response();
    }
    pool.push(bones);
    lock.graves.push(grave.clone());
    if lock.graves.len() > MAX_GRAVES {
        lock.graves.remove(0);
    }
    lock.total_deaths += 1;

    (StatusCode::CREATED, Json(grave)).into_response()
}

/// GET /api/v1/bones/:depth: Retrieve and claim a bones file for dungeon generation.
async fn fetch_bones(State(app): State<BonesApp>, Path(depth): Path<u32>) -> impl IntoResponse {
    let mut lock = lock_graveyard(&app.graveyard);
    if let Some(pool) = lock.bones_pool.get_mut(&depth) {
        if let Some(bones) = pool.pop() {
            return (StatusCode::OK, Json(Some(bones)));
        }
    }
    (StatusCode::OK, Json(None))
}

/// GET /api/v1/graves: List all memorials and gravestones.
async fn list_graves(State(app): State<BonesApp>) -> impl IntoResponse {
    let lock = lock_graveyard(&app.graveyard);
    (StatusCode::OK, Json(lock.graves.clone()))
}

/// GET /api/v1/graves/:hero_name: Look up specific gravestone memorial.
async fn get_grave(
    State(app): State<BonesApp>,
    Path(hero_name): Path<String>,
) -> impl IntoResponse {
    let lock = lock_graveyard(&app.graveyard);
    if let Some(grave) = lock
        .graves
        .iter()
        .find(|g| g.hero_name.eq_ignore_ascii_case(&hero_name))
    {
        (StatusCode::OK, Json(Some(grave.clone())))
    } else {
        (StatusCode::NOT_FOUND, Json(None))
    }
}

/// GET /api/v1/stats: Return aggregate graveyard statistics.
async fn get_stats(State(app): State<BonesApp>) -> impl IntoResponse {
    let lock = lock_graveyard(&app.graveyard);
    let active_bones_count: usize = lock.bones_pool.values().map(|v| v.len()).sum();
    let mut haunted_depths: Vec<u32> = lock
        .bones_pool
        .iter()
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
async fn reset_graveyard(State(app): State<BonesApp>, headers: HeaderMap) -> Response {
    if !bearer_ok(&headers, app.token.as_deref()) {
        return unauthorized();
    }
    let mut lock = lock_graveyard(&app.graveyard);
    lock.bones_pool.clear();
    lock.graves.clear();
    lock.total_deaths = 0;
    (StatusCode::OK, "Graveyard reset.").into_response()
}

/// Runs the bones HTTP service listening on the given socket address.
pub async fn run_bones_server(
    addr: &str,
    state: Option<SharedGraveyard>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let shared_state = state.unwrap_or_else(|| Arc::new(Mutex::new(GraveyardState::default())));
    let app = create_bones_router(shared_state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!(
        "NetHackED Graveyard & Bones Server listening on http://{}",
        listener.local_addr()?
    );
    axum::serve(listener, app).await?;
    Ok(())
}
