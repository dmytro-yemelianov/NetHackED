//! NetRust GraphQL HTTP Server & GraphiQL Explorer for autonomous agent swarms.

use netrust_agent::graphql::{create_router, create_schema, AppState};
use netrust_agent::netconfig::{resolve_bind_addr, token_from_env};
use netrust_agent::AgentSession;
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() {
    let port = std::env::var("PORT").unwrap_or_else(|_| "4000".to_string());
    let args: Vec<String> = std::env::args().collect();
    let addr = resolve_bind_addr(&args, std::env::var("NETRUST_BIND").ok(), &format!("127.0.0.1:{}", port));

    let state = AppState { session: Arc::new(Mutex::new(AgentSession::new(42))) };
    let app = create_router(create_schema(state), token_from_env());

    println!("🗡️ NetRust GraphQL Server listening on http://{}", addr);
    println!("📊 GraphiQL Interactive Explorer: http://{}/graphql", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind GraphQL address");
    axum::serve(listener, app).await.expect("GraphQL server error");
}
