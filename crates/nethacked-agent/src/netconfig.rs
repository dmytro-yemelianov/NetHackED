//! Shared network server configuration: bind address resolution and bearer-token auth.

use axum::http::{header::AUTHORIZATION, HeaderMap};

/// `--bind <addr>` on the command line wins, then a non-empty env value, else the default.
pub fn resolve_bind_addr(
    cli_args: &[String],
    env_bind: Option<String>,
    default_addr: &str,
) -> String {
    if let Some(pos) = cli_args.iter().position(|a| a == "--bind") {
        if let Some(addr) = cli_args.get(pos + 1) {
            return addr.clone();
        }
    }
    match env_bind {
        Some(addr) if !addr.trim().is_empty() => addr,
        _ => default_addr.to_string(),
    }
}

/// The shared secret from `NETHACKED_TOKEN`, if set and non-empty.
pub fn token_from_env() -> Option<String> {
    std::env::var("NETHACKED_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
}

/// True when no token is configured, or the request carries `Authorization: Bearer <token>`.
pub fn bearer_ok(headers: &HeaderMap, token: Option<&str>) -> bool {
    let Some(expected) = token else { return true };
    headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|got| got == expected)
        .unwrap_or(false)
}
