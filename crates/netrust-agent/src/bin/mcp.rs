//! NetRust Model Context Protocol (MCP) Server.
//!
//! Standard JSON-RPC 2.0 stdio server providing LLMs/agents direct tool control
//! over the deterministic NetHack simulation engine.

use netrust_agent::{mcp::handle_mcp_request, AgentSession};
use std::io::{self, BufRead, Write};

fn main() -> io::Result<()> {
    let mut session = AgentSession::new(42);
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line_res in stdin.lock().lines() {
        let line = line_res?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(resp) = handle_mcp_request(&mut session, trimmed) {
            writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
            stdout.flush()?;
        }
    }

    Ok(())
}
