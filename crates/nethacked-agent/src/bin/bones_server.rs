//! Standalone NetHackED Graveyard & Networked Bones Server Daemon.

use nethacked_agent::bones::run_bones_server;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let host = env::var("NETHACKED_BONES_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("NETHACKED_BONES_PORT").unwrap_or_else(|_| "7777".to_string());

    let args: Vec<String> = env::args().collect();
    let addr = nethacked_agent::netconfig::resolve_bind_addr(
        &args,
        env::var("NETHACKED_BIND").ok(),
        &format!("{host}:{port}"),
    );
    println!("Starting NetHackED Networked Bones Server on {addr}");
    run_bones_server(&addr, None).await?;
    Ok(())
}
