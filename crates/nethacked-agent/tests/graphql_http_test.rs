//! HTTP-level tests for the GraphQL server: body cap, depth/complexity limits, mutation auth.

use nethacked_agent::graphql::{create_router, create_schema, AppState};
use nethacked_agent::AgentSession;
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

async fn spawn(token: Option<&str>) -> String {
    let schema = create_schema(AppState {
        session: Arc::new(Mutex::new(AgentSession::new(42))),
    });
    let app = create_router(schema, token.map(String::from));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}

fn post(addr: &str, extra_headers: &str, body: &str) -> (u16, String) {
    let mut s = TcpStream::connect(addr).unwrap();
    let req = format!(
        "POST /graphql HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra_headers}\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).unwrap();
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    let code = out.split_whitespace().nth(1).unwrap().parse().unwrap();
    (code, out.split("\r\n\r\n").nth(1).unwrap_or("").to_string())
}

async fn run(addr: &str, headers: &'static str, body: String) -> (u16, String) {
    let a = addr.to_string();
    tokio::task::spawn_blocking(move || post(&a, headers, &body))
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn query_works_and_oversize_body_rejected() {
    let addr = spawn(None).await;
    let (code, body) = run(
        &addr,
        "",
        json!({"query":"{ playerState { hp } }"}).to_string(),
    )
    .await;
    assert_eq!(code, 200);
    assert!(body.contains("\"hp\":18"));
    let huge = json!({"query": format!("{{ playerState {{ hp }} }} #{}", "x".repeat(70 * 1024))})
        .to_string();
    let (code, _) = run(&addr, "", huge).await;
    assert_eq!(code, 413);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn alias_amplification_is_limited() {
    let addr = spawn(None).await;
    // 1500 aliases x 2 fields = complexity 3000 > 2000, body ~36 KiB < 64 KiB.
    let q: String = (0..1500)
        .map(|i| format!("a{i}: playerState {{ hp }} "))
        .collect();
    let (code, body) = run(
        &addr,
        "",
        json!({"query": format!("{{ {} }}", q)}).to_string(),
    )
    .await;
    assert_eq!(code, 200);
    assert!(body.contains("errors"), "complexity limit not enforced");
    assert!(!body.contains("\"a1499\""), "complexity limit not enforced");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mutations_require_token_when_configured() {
    let addr = spawn(Some("tok")).await;
    let m = json!({"query":"mutation { resetGame(seed: 1) }"}).to_string();
    let (_, body) = run(&addr, "", m.clone()).await;
    assert!(
        body.contains("errors"),
        "unauthenticated mutation succeeded: {body}"
    );
    let (_, body) = run(&addr, "Authorization: Bearer tok\r\n", m).await;
    assert!(body.contains("\"resetGame\":true"), "{}", body);
    let (code, body) = run(
        &addr,
        "",
        json!({"query":"{ playerState { hp } }"}).to_string(),
    )
    .await;
    assert_eq!(code, 200);
    assert!(body.contains("hp"));
}
