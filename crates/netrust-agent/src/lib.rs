//! Agent and LLM Interfaces for NetRust.
//!
//! Provides structured observations, ASCII viewport rendering, JSON-RPC streaming,
//! Model Context Protocol (MCP) server support, and automated evaluation arena.

pub mod arena;
pub mod ascii;
pub mod jsonrpc;
pub mod mcp;
pub mod rpc;
pub mod stdio;
pub mod observation;
pub mod session;
pub mod conducts;
pub mod bones;

#[cfg(not(target_arch = "wasm32"))]
pub mod graphql;

pub use ascii::render_ascii_map;
pub use jsonrpc::{handle_jsonrpc_request, run_jsonrpc_server};
pub use mcp::{handle_mcp_request, run_mcp_server};
pub use observation::{ActorObservation, GameObservation, TileInspection};
pub use session::AgentSession;
pub use arena::{
    run_evaluation_suite, run_game_with_trajectory, run_single_game, AgentPolicy, ArenaSummary,
    BenchmarkReport, PetTesterTacticalPolicy, PolicyStats, RandomPolicy, RunResult, SpeedrunPolicy,
    SurvivalPolicy, TrajectoryRecording, TrajectoryStep,
};

#[cfg(test)]
mod tests {
    use super::*;
    use netrust_sim::ActionAst;
    use serde_json::json;

    #[test]
    fn test_agent_session_observation() {
        let session = AgentSession::new(42);
        let obs = session.get_observation();

        assert_eq!(obs.turn, 1);
        assert_eq!(obs.player_hp, 18);
        assert!(!obs.ascii_map.is_empty());
        assert!(!obs.is_game_over);
    }

    #[test]
    fn test_agent_session_step() {
        let mut session = AgentSession::new(42);
        let next_obs = session.step(ActionAst::Wait);

        assert_eq!(next_obs.turn, 2);
    }

    #[test]
    fn test_mcp_initialize_and_tools_list() {
        let mut session = AgentSession::new(42);

        let init_req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        })
        .to_string();

        let resp = mcp::handle_mcp_request(&mut session, &init_req).unwrap();
        assert_eq!(resp["result"]["serverInfo"]["name"], "netrust-mcp");

        let list_req = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        })
        .to_string();

        let resp2 = mcp::handle_mcp_request(&mut session, &list_req).unwrap();
        let tools = resp2["result"]["tools"].as_array().unwrap();
        assert!(tools.iter().any(|t| t["name"] == "netrust_get_observation"));
        assert!(tools.iter().any(|t| t["name"] == "netrust_step"));
    }

    #[test]
    fn test_mcp_tool_call_observation() {
        let mut session = AgentSession::new(42);

        let call_req = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "netrust_get_observation",
                "arguments": {}
            }
        })
        .to_string();

        let resp = mcp::handle_mcp_request(&mut session, &call_req).unwrap();
        let content_text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(content_text.contains("player_hp"));
    }
}
