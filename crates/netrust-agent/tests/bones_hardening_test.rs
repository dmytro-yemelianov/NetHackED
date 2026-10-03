//! Hardening tests for the bones HTTP server: validation, poisoning resistance, caps, auth.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use netrust_agent::bones::{create_bones_router_with_token, GraveyardState};
use netrust_agent::netconfig::resolve_bind_addr;
use serde_json::json;

async fn spawn(token: Option<&str>) -> (String, Arc<Mutex<GraveyardState>>) {
    let state = Arc::new(Mutex::new(GraveyardState::default()));
    let app = create_bones_router_with_token(state.clone(), token.map(|t| t.to_string()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (addr, state)
}

fn raw(addr: &str, method: &str, path: &str, extra_headers: &str, body: &str) -> (u16, String) {
    let mut s = TcpStream::connect(addr).unwrap();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra_headers}\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let code = out.split_whitespace().nth(1).unwrap().parse().unwrap();
    let body = out.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (code, body)
}

fn bones(name: &str, depth: u32) -> String {
    json!({"depth":depth,"hero_name":name,"hero_level":3,"max_hp":20,"ac":5,
           "death_coord":{"x":10,"y":5},"items":[],"killer":"jackal"}).to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn multibyte_name_does_not_brick_server() {
    let (addr, _) = spawn(None).await;
    let a = addr.clone();
    let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "POST", "/api/v1/bones", "", &bones("Святослав Хоробрий", 3))).await.unwrap();
    assert_eq!(code, 201);
    let a = addr.clone();
    let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "GET", "/api/v1/stats", "", "")).await.unwrap();
    assert_eq!(code, 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn validation_rejects_bad_payloads() {
    let (addr, _) = spawn(None).await;
    for body in [
        bones(&"x".repeat(33), 3),
        bones("", 3),
        bones("Ok", 0),
        bones("Ok", 61),
        json!({"depth":3,"hero_name":"Ok","hero_level":1,"max_hp":1,"ac":0,"death_coord":{"x":10,"y":5},"items":[],"killer":"k".repeat(65)}).to_string(),
    ] {
        let a = addr.clone();
        let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "POST", "/api/v1/bones", "", &body)).await.unwrap();
        assert_eq!(code, 422);
    }
    // Out-of-range coordinate fails JSON extraction (4xx), never panics.
    let a = addr.clone();
    let bad = bones("Ok", 3).replace(r#""x":10"#, r#""x":1000"#);
    let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "POST", "/api/v1/bones", "", &bad)).await.unwrap();
    assert!((400..500).contains(&code));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn per_depth_cap_and_poison_recovery() {
    let (addr, state) = spawn(None).await;
    for i in 0..16 {
        let a = addr.clone();
        let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "POST", "/api/v1/bones", "", &bones(&format!("H{}", i), 4))).await.unwrap();
        assert_eq!(code, 201);
    }
    let a = addr.clone();
    let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "POST", "/api/v1/bones", "", &bones("Overflow", 4))).await.unwrap();
    assert_eq!(code, 409);

    // Poison the mutex from another thread; server must keep serving.
    let st = state.clone();
    let _ = std::thread::spawn(move || {
        let _g = st.lock().unwrap();
        panic!("poison");
    })
    .join();
    let a = addr.clone();
    let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "GET", "/api/v1/stats", "", "")).await.unwrap();
    assert_eq!(code, 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn token_protects_mutating_routes() {
    let (addr, _) = spawn(Some("s3cret")).await;
    let a = addr.clone();
    let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "POST", "/api/v1/reset", "", "")).await.unwrap();
    assert_eq!(code, 401);
    let a = addr.clone();
    let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "POST", "/api/v1/bones", "", &bones("Ok", 3))).await.unwrap();
    assert_eq!(code, 401);
    let a = addr.clone();
    let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "POST", "/api/v1/bones", "Authorization: Bearer s3cret\r\n", &bones("Ok", 3))).await.unwrap();
    assert_eq!(code, 201);
    let a = addr.clone();
    let (code, _) = tokio::task::spawn_blocking(move || raw(&a, "GET", "/api/v1/stats", "", "")).await.unwrap();
    assert_eq!(code, 200);
}

#[test]
fn bind_addr_resolution() {
    let args = vec!["bin".to_string(), "--bind".to_string(), "0.0.0.0:9".to_string()];
    assert_eq!(resolve_bind_addr(&args, Some("1.2.3.4:5".into()), "127.0.0.1:7777"), "0.0.0.0:9");
    assert_eq!(resolve_bind_addr(&[], Some("1.2.3.4:5".into()), "127.0.0.1:7777"), "1.2.3.4:5");
    assert_eq!(resolve_bind_addr(&[], Some("".into()), "127.0.0.1:7777"), "127.0.0.1:7777");
    assert_eq!(resolve_bind_addr(&[], None, "127.0.0.1:7777"), "127.0.0.1:7777");
}
