//! NetRust GraphQL HTTP Server & GraphiQL Explorer for autonomous agent swarms.

use async_graphql::http::GraphiQLSource;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    response::{Html, IntoResponse},
    routing::get,
    Extension, Router,
};
use netrust_agent::graphql::{create_schema, AppState, NetRustSchema};
use netrust_agent::AgentSession;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

async fn graphql_handler(
    schema: Extension<NetRustSchema>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

async fn graphiql() -> impl IntoResponse {
    Html(GraphiQLSource::build().endpoint("/graphql").finish())
}

#[tokio::main]
async fn main() {
    let port = std::env::var("PORT").unwrap_or_else(|_| "4000".to_string());
    let addr: SocketAddr = format!("0.0.0.0:{}", port)
        .parse()
        .expect("Valid socket address");

    let session = AgentSession::new(42);
    let state = AppState {
        session: Arc::new(Mutex::new(session)),
    };
    let schema = create_schema(state);

    let app = Router::new()
        .route("/graphql", get(graphiql).post(graphql_handler))
        .layer(Extension(schema));

    println!("🗡️ NetRust GraphQL Server listening on http://{}", addr);
    println!("📊 GraphiQL Interactive Explorer: http://{}/graphql", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
