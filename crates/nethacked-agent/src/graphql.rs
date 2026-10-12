//! GraphQL Schema and Resolvers for NetHackED remote agent swarms.

use crate::observation::{GameObservation, TileInspection};
use crate::{parse_action, parse_character, render_ascii_map, ActionArgs, AgentSession};
use async_graphql::{Context, EmptySubscription, Object, Schema, SimpleObject};
use nethacked_data::roles::CharacterConfig;
use nethacked_data::ruleset::Ruleset;
use nethacked_sim::{ActionAst, Coord};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

pub type NetHackEDSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

/// Requests handled by the dedicated session actor thread.
///
/// The owner runtime is single-threaded (`OwnerBridge` is `!Send`), so the
/// `AgentSession` lives on one dedicated thread and every resolver round-trips
/// through this channel. All payloads are plain `Send` data.
enum SessionRequest {
    Observation(oneshot::Sender<GameObservation>),
    AsciiMap(oneshot::Sender<String>),
    InspectTile {
        x: usize,
        y: usize,
        respond: oneshot::Sender<Result<TileInspection, String>>,
    },
    Ruleset(oneshot::Sender<Arc<Ruleset>>),
    PlayerCoord(oneshot::Sender<Option<Coord>>),
    Step {
        action: ActionAst,
        respond: oneshot::Sender<Result<(GameObservation, String), String>>,
    },
    Reset {
        seed: u64,
        respond: oneshot::Sender<Result<(), String>>,
    },
    ResetWithCharacter {
        seed: u64,
        config: CharacterConfig,
        respond: oneshot::Sender<Result<(GameObservation, String), String>>,
    },
}

/// Owns the single-threaded `AgentSession` on a dedicated OS thread.
pub struct SessionActor {
    tx: Mutex<std::sync::mpsc::Sender<SessionRequest>>,
}

impl SessionActor {
    /// Spawn the session thread with a fresh game at `seed`.
    ///
    /// The `AgentSession` is constructed inside the thread: the owner runtime
    /// is `!Send`, so it must never cross a thread boundary.
    pub fn spawn(seed: u64) -> Result<Self, String> {
        let (tx, rx) = std::sync::mpsc::channel::<SessionRequest>();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        std::thread::Builder::new()
            .name("nethacked-graphql-session".to_string())
            .spawn(move || {
                let session = match AgentSession::new(seed) {
                    Ok(session) => {
                        let _ = ready_tx.send(Ok(()));
                        session
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                run_session(session, rx)
            })
            .map_err(|e| format!("failed to spawn session actor thread: {e}"))?;
        ready_rx
            .recv()
            .map_err(|_| "session actor terminated".to_string())??;
        Ok(Self { tx: Mutex::new(tx) })
    }

    /// Round-trip one request to the session thread and await its reply.
    async fn call<T: Send>(
        &self,
        build: impl FnOnce(oneshot::Sender<T>) -> SessionRequest,
    ) -> Result<T, String> {
        let (respond, rx) = oneshot::channel();
        self.tx
            .lock()
            .unwrap()
            .send(build(respond))
            .map_err(|_| "session actor terminated".to_string())?;
        rx.await.map_err(|_| "session actor terminated".to_string())
    }
}

fn run_session(mut session: AgentSession, rx: std::sync::mpsc::Receiver<SessionRequest>) {
    while let Ok(request) = rx.recv() {
        match request {
            SessionRequest::Observation(respond) => {
                let _ = respond.send(session.get_observation());
            }
            SessionRequest::AsciiMap(respond) => {
                let _ = respond.send(render_ascii_map(session.world()));
            }
            SessionRequest::InspectTile { x, y, respond } => {
                let _ = respond.send(session.inspect_tile(x, y));
            }
            SessionRequest::Ruleset(respond) => {
                let _ = respond.send(Arc::clone(&session.world().ruleset));
            }
            SessionRequest::PlayerCoord(respond) => {
                let _ = respond.send(session.player_coord());
            }
            SessionRequest::Step { action, respond } => {
                let result = session
                    .step(action)
                    .map(|obs| (obs, render_ascii_map(session.world())));
                let _ = respond.send(result);
            }
            SessionRequest::Reset { seed, respond } => {
                let result = match AgentSession::new(seed) {
                    Ok(fresh) => {
                        session = fresh;
                        Ok(())
                    }
                    Err(e) => Err(e),
                };
                let _ = respond.send(result);
            }
            SessionRequest::ResetWithCharacter {
                seed,
                config,
                respond,
            } => {
                let result = match AgentSession::new_with_character(seed, config) {
                    Ok(fresh) => {
                        session = fresh;
                        Ok((session.get_observation(), render_ascii_map(session.world())))
                    }
                    Err(e) => Err(e),
                };
                let _ = respond.send(result);
            }
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub actor: Arc<SessionActor>,
}

impl AppState {
    /// Create the schema state: spawns the dedicated session actor thread.
    pub fn new(seed: u64) -> Result<Self, String> {
        Ok(Self {
            actor: Arc::new(SessionActor::spawn(seed)?),
        })
    }
}

#[derive(SimpleObject)]
pub struct PlayerStateGql {
    pub x: usize,
    pub y: usize,
    pub hp: u32,
    pub max_hp: u32,
    pub ac: i32,
    pub depth: usize,
    pub nutrition: i32,
    pub pw: u32,
    pub max_pw: u32,
    pub gold: u32,
}

#[derive(SimpleObject)]
pub struct RoleEntryGql {
    pub id: String,
    pub name: String,
    pub base_hp: u32,
    pub ac: i32,
    pub speed: u32,
    pub default_alignment: String,
    pub starting_items: Vec<String>,
}

#[derive(SimpleObject)]
pub struct RaceEntryGql {
    pub id: String,
    pub name: String,
}

#[derive(SimpleObject)]
pub struct BestiaryEntryGql {
    pub name: String,
    pub glyph: String,
    pub base_hp: u32,
    pub ac: i32,
    pub speed: u32,
    pub level: u32,
}

#[derive(SimpleObject)]
pub struct ItemEntryGql {
    pub name: String,
    pub class: String,
    pub weight: u32,
    pub cost: u32,
    pub is_container: bool,
    pub is_bag_of_holding: bool,
}

#[derive(SimpleObject)]
pub struct StepResultGql {
    pub success: bool,
    pub events: Vec<String>,
    pub ascii_map: String,
    pub hp: u32,
    pub max_hp: u32,
    pub turn: u64,
    pub is_game_over: bool,
}

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Returns current player stats and coordinates.
    async fn player_state(&self, ctx: &Context<'_>) -> async_graphql::Result<PlayerStateGql> {
        let state = ctx.data_unchecked::<AppState>();
        let obs = state
            .actor
            .call(SessionRequest::Observation)
            .await
            .map_err(async_graphql::Error::new)?;
        Ok(PlayerStateGql {
            x: obs.player_coord.x,
            y: obs.player_coord.y,
            hp: obs.player_hp,
            max_hp: obs.player_max_hp,
            ac: obs.player_ac,
            depth: obs.depth,
            nutrition: obs.player_nutrition,
            pw: obs.player_pw,
            max_pw: obs.player_max_pw,
            gold: obs.player_gold,
        })
    }

    /// Returns full JSON observation for autonomous agents.
    async fn observation_json(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        let state = ctx.data_unchecked::<AppState>();
        let obs = state
            .actor
            .call(SessionRequest::Observation)
            .await
            .map_err(async_graphql::Error::new)?;
        Ok(serde_json::to_string_pretty(&obs).unwrap_or_default())
    }

    /// Renders classic 80x21 ASCII dungeon map.
    async fn ascii_map(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        let state = ctx.data_unchecked::<AppState>();
        state
            .actor
            .call(SessionRequest::AsciiMap)
            .await
            .map_err(async_graphql::Error::new)
    }

    /// Inspect a specific tile coordinate.
    async fn inspect_tile(&self, ctx: &Context<'_>, x: usize, y: usize) -> String {
        let state = ctx.data_unchecked::<AppState>();
        match state
            .actor
            .call(|respond| SessionRequest::InspectTile { x, y, respond })
            .await
        {
            Ok(Ok(info)) => serde_json::to_string_pretty(&info).unwrap_or_default(),
            Ok(Err(e)) => e,
            Err(_) => "session actor terminated".to_string(),
        }
    }

    /// Returns the declarative bestiary.
    async fn bestiary(&self, ctx: &Context<'_>) -> Vec<BestiaryEntryGql> {
        let rs = session_ruleset(ctx).await;
        rs.monsters
            .iter()
            .map(|m| BestiaryEntryGql {
                name: m.name.to_string(),
                glyph: m.glyph.to_string(),
                base_hp: m.base_hp,
                ac: m.ac,
                speed: m.speed,
                level: m.level,
            })
            .collect()
    }

    /// Returns the declarative item catalog.
    async fn item_catalog(&self, ctx: &Context<'_>) -> Vec<ItemEntryGql> {
        let rs = session_ruleset(ctx).await;
        rs.items
            .iter()
            .map(|i| ItemEntryGql {
                name: i.name.to_string(),
                class: format!("{:?}", i.class),
                weight: i.weight,
                cost: i.cost,
                is_container: i.is_container,
                is_bag_of_holding: i.is_bag_of_holding,
            })
            .collect()
    }

    /// Returns available classic player roles.
    async fn roles(&self, ctx: &Context<'_>) -> Vec<RoleEntryGql> {
        let rs = session_ruleset(ctx).await;
        rs.roles
            .iter()
            .map(|r| RoleEntryGql {
                id: format!("{:?}", r.id),
                name: r.name.to_string(),
                base_hp: r.base_hp,
                ac: r.ac,
                speed: r.speed,
                default_alignment: format!("{:?}", r.default_alignment),
                starting_items: r
                    .starting_items
                    .iter()
                    .map(|item| item.item.clone())
                    .collect(),
            })
            .collect()
    }

    /// Returns available player races.
    async fn races(&self, ctx: &Context<'_>) -> Vec<RaceEntryGql> {
        let rs = session_ruleset(ctx).await;
        rs.races
            .iter()
            .map(|r| RaceEntryGql {
                id: format!("{:?}", r.id),
                name: r.name.to_string(),
            })
            .collect()
    }
}

/// The session's active ruleset, falling back to the vanilla catalog when the
/// actor is unavailable.
async fn session_ruleset(ctx: &Context<'_>) -> Arc<Ruleset> {
    match ctx.data_opt::<AppState>() {
        Some(state) => state
            .actor
            .call(SessionRequest::Ruleset)
            .await
            .unwrap_or_else(|_| Ruleset::vanilla()),
        None => Ruleset::vanilla(),
    }
}

pub const MAX_BODY_BYTES: usize = 64 * 1024;

/// Per-request authorization decision inserted by the HTTP handler.
pub struct Authorized(pub bool);

fn require_mutation_auth(ctx: &Context<'_>) -> async_graphql::Result<()> {
    match ctx.data_opt::<Authorized>() {
        Some(Authorized(false)) => Err("unauthorized: missing or invalid bearer token".into()),
        _ => Ok(()),
    }
}

pub struct MutationRoot;

#[Object]
impl MutationRoot {
    /// Step the simulation world with a player action.
    #[allow(clippy::too_many_arguments)] // GraphQL resolver signature is public API
    async fn step_action(
        &self,
        ctx: &Context<'_>,
        action: String,
        direction: Option<String>,
        target_x: Option<usize>,
        target_y: Option<usize>,
        index: Option<usize>,
        text: Option<String>,
    ) -> async_graphql::Result<StepResultGql> {
        require_mutation_auth(ctx)?;
        let state = ctx.data_unchecked::<AppState>();

        let player = state
            .actor
            .call(SessionRequest::PlayerCoord)
            .await
            .map_err(async_graphql::Error::new)?;
        let args = ActionArgs {
            index,
            expected_item: None,
            direction,
            target: target_x.zip(target_y),
            text,
            player,
        };
        let action_ast = parse_action(&action, &args).map_err(async_graphql::Error::new)?;

        let (obs, ascii_map) = state
            .actor
            .call(|respond| SessionRequest::Step {
                action: action_ast,
                respond,
            })
            .await
            .map_err(async_graphql::Error::new)?
            .map_err(async_graphql::Error::new)?;

        Ok(StepResultGql {
            success: true,
            events: obs
                .last_events
                .into_iter()
                .map(|e| format!("{e:?}"))
                .collect(),
            ascii_map,
            hp: obs.player_hp,
            max_hp: obs.player_max_hp,
            turn: obs.turn,
            is_game_over: obs.is_game_over,
        })
    }

    /// Reset game simulation with an optional seed.
    async fn reset_game(
        &self,
        ctx: &Context<'_>,
        seed: Option<u64>,
    ) -> async_graphql::Result<bool> {
        require_mutation_auth(ctx)?;
        let state = ctx.data_unchecked::<AppState>();
        state
            .actor
            .call(|respond| SessionRequest::Reset {
                seed: seed.unwrap_or(42),
                respond,
            })
            .await
            .map_err(async_graphql::Error::new)?
            .map_err(async_graphql::Error::new)?;
        Ok(true)
    }

    /// Reset game simulation with customized character role, race, and attributes.
    #[allow(clippy::too_many_arguments)] // GraphQL resolver signature is public API
    async fn reset_with_character(
        &self,
        ctx: &Context<'_>,
        seed: Option<u64>,
        name: Option<String>,
        role: Option<String>,
        race: Option<String>,
        gender: Option<String>,
        alignment: Option<String>,
    ) -> async_graphql::Result<StepResultGql> {
        require_mutation_auth(ctx)?;
        let state = ctx.data_unchecked::<AppState>();

        let config = parse_character(
            Some(name.as_deref().unwrap_or("Hero")),
            role.as_deref(),
            race.as_deref(),
            gender.as_deref(),
            alignment.as_deref(),
        )
        .map_err(async_graphql::Error::new)?;

        let (obs, ascii_map) = state
            .actor
            .call(|respond| SessionRequest::ResetWithCharacter {
                seed: seed.unwrap_or(42),
                config,
                respond,
            })
            .await
            .map_err(async_graphql::Error::new)?
            .map_err(async_graphql::Error::new)?;

        Ok(StepResultGql {
            success: true,
            events: obs
                .last_events
                .into_iter()
                .map(|e| format!("{e:?}"))
                .collect(),
            ascii_map,
            hp: obs.player_hp,
            max_hp: obs.player_max_hp,
            turn: obs.turn,
            is_game_over: obs.is_game_over,
        })
    }
}

pub fn create_schema(state: AppState) -> NetHackEDSchema {
    Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(state)
        .limit_depth(16)
        .limit_complexity(2000)
        .finish()
}

use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};

#[derive(Clone)]
struct GqlApp {
    schema: NetHackEDSchema,
    token: Option<Arc<str>>,
}

pub fn create_router(schema: NetHackEDSchema, token: Option<String>) -> Router {
    Router::new()
        .route("/graphql", get(graphiql).post(graphql_post))
        .with_state(GqlApp {
            schema,
            token: token.map(Arc::from),
        })
}

async fn graphiql() -> impl IntoResponse {
    Html(
        async_graphql::http::GraphiQLSource::build()
            .endpoint("/graphql")
            .finish(),
    )
}

async fn graphql_post(State(app): State<GqlApp>, headers: HeaderMap, body: Body) -> Response {
    let bytes = match axum::body::to_bytes(body, MAX_BODY_BYTES).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response(),
    };
    let request: async_graphql::Request = match serde_json::from_slice(&bytes) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                format!("invalid GraphQL request: {e}"),
            )
                .into_response()
        }
    };
    let authorized = crate::netconfig::bearer_ok(&headers, app.token.as_deref());
    let response = app
        .schema
        .execute(request.data(Authorized(authorized)))
        .await;
    Json(response).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_graphql_query_player_state_and_bestiary() {
        let schema = create_schema(AppState::new(42).unwrap());

        let res = schema
            .execute("{ playerState { hp ac } bestiary { name } roles { name baseHp } }")
            .await;
        assert!(res.is_ok());
        let data = res.data.into_json().unwrap();
        assert_eq!(data["playerState"]["hp"], 18);
        assert!(data["bestiary"].as_array().unwrap().len() >= 10);
        assert_eq!(data["roles"].as_array().unwrap().len(), 9);
    }

    #[tokio::test]
    async fn test_graphql_races_and_item_catalog() {
        let schema = create_schema(AppState::new(42).unwrap());

        let res = schema
            .execute("{ races { name } itemCatalog { name } }")
            .await;
        assert!(res.is_ok());
        let data = res.data.into_json().unwrap();
        assert_eq!(data["races"].as_array().unwrap().len(), 5);
        assert!(data["itemCatalog"].as_array().unwrap().len() >= 20);
    }

    #[tokio::test]
    async fn test_graphql_mutation_step_action() {
        let schema = create_schema(AppState::new(42).unwrap());

        let res = schema
            .execute(r#"mutation { stepAction(action: "wait") { success turn } }"#)
            .await;
        assert!(res.is_ok());
        let data = res.data.into_json().unwrap();
        assert_eq!(data["stepAction"]["success"], true);
    }

    #[tokio::test]
    async fn test_graphql_mutation_reset_with_barbarian() {
        let schema = create_schema(AppState::new(42).unwrap());

        let res = schema
            .execute(
                r#"mutation { resetWithCharacter(role: "barbarian", race: "orc") { hp maxHp } }"#,
            )
            .await;
        assert!(res.is_ok());
        let data = res.data.into_json().unwrap();
        assert_eq!(data["resetWithCharacter"]["hp"], 20);
        assert_eq!(data["resetWithCharacter"]["maxHp"], 20);
    }

    #[tokio::test]
    async fn test_graphql_unknown_action_is_error() {
        let schema = create_schema(AppState::new(42).unwrap());
        let res = schema
            .execute(r#"mutation { stepAction(action: "dance") { success } }"#)
            .await;
        assert!(!res.errors.is_empty());
        let res = schema
            .execute(r#"mutation { resetWithCharacter(role: "samurai") { hp } }"#)
            .await;
        assert!(!res.errors.is_empty());
    }
}
