//! NetHackED Model Context Protocol (MCP) Server.
//!
//! Standard JSON-RPC 2.0 stdio server providing LLMs/agents direct tool control
//! over the deterministic NetHack simulation engine.

fn main() -> std::io::Result<()> {
    nethacked_agent::run_mcp_server(42)
}
