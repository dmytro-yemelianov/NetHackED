//! GraphQL Schema and Resolvers for NetRust remote agent swarms.

use crate::{parse_action, parse_character, render_ascii_map, ActionArgs, AgentSession};
use async_graphql::{Context, EmptySubscription, Object, Schema, SimpleObject};
use netrust_data::{
    roles::{RACES, ROLES},
    BESTIARY, ITEM_CATALOG,
};
use std::sync::{Arc, Mutex};

pub type NetRustSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

#[derive(Clone)]
pub struct AppState {
    pub session: Arc<Mutex<AgentSession>>,
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
    async fn player_state(&self, ctx: &Context<'_>) -> PlayerStateGql {
        let state = ctx.data_unchecked::<AppState>();
        let session = state.session.lock().unwrap();
        let obs = session.get_observation();
        PlayerStateGql {
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
        }
    }

    /// Returns full JSON observation for autonomous agents.
    async fn observation_json(&self, ctx: &Context<'_>) -> String {
        let state = ctx.data_unchecked::<AppState>();
        let session = state.session.lock().unwrap();
        let obs = session.get_observation();
        serde_json::to_string_pretty(&obs).unwrap_or_default()
    }

    /// Renders classic 80x21 ASCII dungeon map.
    async fn ascii_map(&self, ctx: &Context<'_>) -> String {
        let state = ctx.data_unchecked::<AppState>();
        let session = state.session.lock().unwrap();
        render_ascii_map(&session.world)
    }

    /// Inspect a specific tile coordinate.
    async fn inspect_tile(&self, ctx: &Context<'_>, x: usize, y: usize) -> String {
        let state = ctx.data_unchecked::<AppState>();
        let session = state.session.lock().unwrap();
        match session.inspect_tile(x, y) {
            Ok(info) => serde_json::to_string_pretty(&info).unwrap_or_default(),
            Err(e) => e,
        }
    }

    /// Returns the declarative bestiary.
    async fn bestiary(&self) -> Vec<BestiaryEntryGql> {
        BESTIARY
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
    async fn item_catalog(&self) -> Vec<ItemEntryGql> {
        ITEM_CATALOG
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
    async fn roles(&self) -> Vec<RoleEntryGql> {
        ROLES
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
                    .map(|item| format!("{item:?}"))
                    .collect(),
            })
            .collect()
    }

    /// Returns available player races.
    async fn races(&self) -> Vec<RaceEntryGql> {
        RACES
            .iter()
            .map(|r| RaceEntryGql {
                id: format!("{:?}", r.id),
                name: r.name.to_string(),
            })
            .collect()
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
        let mut session = state.session.lock().unwrap();

        let player = session.player_coord();
        let args = ActionArgs {
            index,
            direction,
            target: target_x.zip(target_y),
            text,
            player,
        };
        let action_ast = parse_action(&action, &args).map_err(async_graphql::Error::new)?;

        let obs = session.step(action_ast);
        let ascii_map = render_ascii_map(&session.world);

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
        let mut session = state.session.lock().unwrap();
        *session = AgentSession::new(seed.unwrap_or(42));
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
        let mut session = state.session.lock().unwrap();

        let config = parse_character(
            Some(name.as_deref().unwrap_or("Hero")),
            role.as_deref(),
            race.as_deref(),
            gender.as_deref(),
            alignment.as_deref(),
        )
        .map_err(async_graphql::Error::new)?;

        *session = AgentSession::new_with_character(seed.unwrap_or(42), config);
        let obs = session.get_observation();
        let ascii_map = render_ascii_map(&session.world);

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

pub fn create_schema(state: AppState) -> NetRustSchema {
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
    schema: NetRustSchema,
    token: Option<Arc<str>>,
}

pub fn create_router(schema: NetRustSchema, token: Option<String>) -> Router {
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
        let session = AgentSession::new(42);
        let state = AppState {
            session: Arc::new(Mutex::new(session)),
        };
        let schema = create_schema(state);

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
    async fn test_graphql_mutation_step_action() {
        let session = AgentSession::new(42);
        let state = AppState {
            session: Arc::new(Mutex::new(session)),
        };
        let schema = create_schema(state);

        let res = schema
            .execute(r#"mutation { stepAction(action: "wait") { success turn } }"#)
            .await;
        assert!(res.is_ok());
        let data = res.data.into_json().unwrap();
        assert_eq!(data["stepAction"]["success"], true);
    }

    #[tokio::test]
    async fn test_graphql_mutation_reset_with_barbarian() {
        let session = AgentSession::new(42);
        let state = AppState {
            session: Arc::new(Mutex::new(session)),
        };
        let schema = create_schema(state);

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
        let schema = create_schema(AppState {
            session: Arc::new(Mutex::new(AgentSession::new(42))),
        });
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
