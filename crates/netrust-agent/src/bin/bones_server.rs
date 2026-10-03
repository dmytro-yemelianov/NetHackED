//! Standalone NetRust Graveyard & Networked Bones Server Daemon.

use std::env;
use netrust_agent::bones::run_bones_server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let host = env::var("NETRUST_BONES_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("NETRUST_BONES_PORT").unwrap_or_else(|_| "7777".to_string());

    let addr = format!("{}:{}", host, port);
    println!("Starting NetRust Networked Bones Server on {}", addr);
    run_bones_server(&addr, None).await?;
    Ok(())
}
