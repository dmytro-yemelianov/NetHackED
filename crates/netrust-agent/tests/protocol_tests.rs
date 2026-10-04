//! Protocol-level hardening tests for session, rpc helpers and stdio loop.

use netrust_agent::jsonrpc::{handle_jsonrpc_line, handle_jsonrpc_request};
use netrust_agent::mcp::{handle_mcp_line, handle_mcp_request};
use netrust_agent::rpc::{
    parse_direction, parse_request, INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND, PARSE_ERROR,
};
use netrust_agent::stdio::serve_lines;
use netrust_agent::AgentSession;
use netrust_sim::Direction;
use serde_json::json;
use std::io::Cursor;

#[test]
fn inspect_tile_out_of_bounds_is_error_not_panic() {
    let session = AgentSession::new(42);
    assert!(session.inspect_tile(1000, 0).is_err());
    assert!(session.inspect_tile(0, 21).is_err());
    assert!(session.inspect_tile(5, 5).is_ok());
}

#[test]
fn parse_request_classifies_errors() {
    let e = parse_request(Ok("{not json")).unwrap_err();
    assert_eq!(e["error"]["code"], PARSE_ERROR);
    assert!(e["id"].is_null());

    let e = parse_request(Err(())).unwrap_err();
    assert_eq!(e["error"]["code"], PARSE_ERROR);

    let e = parse_request(Ok(r#"{"jsonrpc":"2.0","id":7}"#)).unwrap_err();
    assert_eq!(e["error"]["code"], INVALID_REQUEST);
    assert_eq!(e["id"], 7);

    let e = parse_request(Ok(r#"[1,2]"#)).unwrap_err();
    assert_eq!(e["error"]["code"], INVALID_REQUEST);

    let r = parse_request(Ok(r#"{"jsonrpc":"2.0","id":0,"method":"ping"}"#)).unwrap();
    assert_eq!(r.id, Some(serde_json::json!(0)));
    assert_eq!(r.method, "ping");

    let r = parse_request(Ok(
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    ))
    .unwrap();
    assert!(r.id.is_none());
}

#[test]
fn parse_direction_covers_eight_ways() {
    assert_eq!(parse_direction("northeast"), Some(Direction::NorthEast));
    assert_eq!(parse_direction("b"), Some(Direction::SouthWest));
    assert_eq!(parse_direction("up"), None);
}

#[test]
fn serve_lines_survives_invalid_utf8() {
    let mut input: Vec<u8> = vec![0xff, 0xfe, b'\n'];
    input.extend_from_slice(b"hello\n\n");
    let mut out = Vec::new();
    serve_lines(Cursor::new(input), &mut out, |line| match line {
        Ok(s) => Some(format!("ok:{s}")),
        Err(()) => Some("bad".to_string()),
    })
    .unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), "bad\nok:hello\n");
}

fn mcp(session: &mut AgentSession, v: serde_json::Value) -> Option<serde_json::Value> {
    handle_mcp_request(session, &v.to_string())
}

#[test]
fn mcp_malformed_input_gets_errors() {
    let mut s = AgentSession::new(42);
    let r = handle_mcp_request(&mut s, "{oops").unwrap();
    assert_eq!(r["error"]["code"], PARSE_ERROR);
    let r = handle_mcp_line(&mut s, Err(())).unwrap();
    assert_eq!(r["error"]["code"], PARSE_ERROR);
    let r = mcp(&mut s, json!({"jsonrpc":"2.0","id":"a"})).unwrap();
    assert_eq!(r["error"]["code"], INVALID_REQUEST);
    assert_eq!(r["id"], "a");
}

#[test]
fn mcp_notifications_never_answered() {
    let mut s = AgentSession::new(42);
    assert!(mcp(
        &mut s,
        json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{}})
    )
    .is_none());
    assert!(mcp(&mut s, json!({"jsonrpc":"2.0","method":"no/such"})).is_none());
}

#[test]
fn mcp_ping_and_unknown_method() {
    let mut s = AgentSession::new(42);
    let r = mcp(&mut s, json!({"jsonrpc":"2.0","id":0,"method":"ping"})).unwrap();
    assert_eq!(r["id"], 0);
    assert_eq!(r["result"], json!({}));
    let r = mcp(&mut s, json!({"jsonrpc":"2.0","id":1,"method":"no/such"})).unwrap();
    assert_eq!(r["error"]["code"], METHOD_NOT_FOUND);
}

#[test]
fn mcp_invalid_params_paths() {
    let mut s = AgentSession::new(42);
    let call = |name: &str, args: serde_json::Value| json!({"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":name,"arguments":args}});
    let r = mcp(
        &mut s,
        json!({"jsonrpc":"2.0","id":9,"method":"tools/call"}),
    )
    .unwrap();
    assert_eq!(r["error"]["code"], INVALID_PARAMS);
    let r = mcp(&mut s, call("nope", json!({}))).unwrap();
    assert_eq!(r["error"]["code"], INVALID_PARAMS);
    let r = mcp(&mut s, call("netrust_step", json!({"action":"dance"}))).unwrap();
    assert_eq!(r["error"]["code"], INVALID_PARAMS);
    let r = mcp(&mut s, call("netrust_step", json!({}))).unwrap();
    assert_eq!(r["error"]["code"], INVALID_PARAMS);
    let r = mcp(
        &mut s,
        call("netrust_inspect_tile", json!({"x":1000,"y":0})),
    )
    .unwrap();
    assert_eq!(r["error"]["code"], INVALID_PARAMS);
    let r = mcp(&mut s, call("netrust_inspect_tile", json!({"x":5}))).unwrap();
    assert_eq!(r["error"]["code"], INVALID_PARAMS);
}

#[test]
fn mcp_kick_off_map_is_invalid_params() {
    let mut s = AgentSession::new(42);
    let pid = s.world.player_id;
    s.world.arena.actors.get_mut(pid).unwrap().coord = netrust_sim::Coord::new(79, 20).unwrap();
    let r = mcp(&mut s, json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"netrust_step","arguments":{"action":"kick_east"}}})).unwrap();
    assert_eq!(r["error"]["code"], INVALID_PARAMS);
}

#[test]
fn mcp_step_enum_matches_accepted_actions() {
    let mut s = AgentSession::new(42);
    let r = mcp(
        &mut s,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    )
    .unwrap();
    let tools = r["result"]["tools"].as_array().unwrap();
    let step = tools.iter().find(|t| t["name"] == "netrust_step").unwrap();
    let actions: Vec<String> = step["inputSchema"]["properties"]["action"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    for a in ["descend", "ascend", "eat", "cast", "kick_east"] {
        assert!(actions.contains(&a.to_string()), "enum missing {a}");
    }
    for a in &actions {
        let mut fresh = AgentSession::new(42);
        let r = mcp(&mut fresh, json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"netrust_step","arguments":{"action":a}}})).unwrap();
        assert!(r.get("result").is_some(), "enum action {a} rejected: {r}");
    }
}

#[test]
fn mcp_null_id_is_a_request_and_errors_carry_jsonrpc() {
    let mut s = AgentSession::new(42);
    let r = mcp(&mut s, json!({"jsonrpc":"2.0","id":null,"method":"ping"})).unwrap();
    assert!(r["id"].is_null());
    assert_eq!(r["result"], json!({}));
    let r = mcp(
        &mut s,
        json!({"jsonrpc":"2.0","id":null,"method":"no/such"}),
    )
    .unwrap();
    assert_eq!(r["jsonrpc"], "2.0");
    assert!(r["id"].is_null());
    assert_eq!(r["error"]["code"], METHOD_NOT_FOUND);
}

#[test]
fn mcp_step_schema_declares_index_and_direction() {
    let mut s = AgentSession::new(42);
    let r = mcp(
        &mut s,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    )
    .unwrap();
    let step = r["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "netrust_step")
        .unwrap();
    assert_eq!(
        step["inputSchema"]["properties"]["index"]["type"],
        "integer"
    );
    assert_eq!(
        step["inputSchema"]["properties"]["direction"]["type"],
        "string"
    );
}

#[test]
fn jsonrpc_robustness() {
    let mut s = AgentSession::new(42);
    let r = handle_jsonrpc_request(&mut s, "nope").unwrap();
    assert_eq!(r["error"]["code"], PARSE_ERROR);
    let r = handle_jsonrpc_line(&mut s, Err(())).unwrap();
    assert_eq!(r["error"]["code"], PARSE_ERROR);
    assert!(handle_jsonrpc_request(
        &mut s,
        r#"{"jsonrpc":"2.0","method":"netrust.step","params":{"action":"wait"}}"#
    )
    .is_none());
    let r = handle_jsonrpc_request(
        &mut s,
        r#"{"jsonrpc":"2.0","id":1,"method":"netrust.inspectTile","params":{"x":1000,"y":0}}"#,
    )
    .unwrap();
    assert_eq!(r["error"]["code"], INVALID_PARAMS);
    let r = handle_jsonrpc_request(
        &mut s,
        r#"{"jsonrpc":"2.0","id":2,"method":"netrust.step","params":{"action":"dance"}}"#,
    )
    .unwrap();
    assert_eq!(r["error"]["code"], INVALID_PARAMS);
    let r = handle_jsonrpc_request(
        &mut s,
        r#"{"jsonrpc":"2.0","id":3,"method":"netrust.step","params":{"action":"northeast"}}"#,
    )
    .unwrap();
    assert!(r.get("result").is_some());
}
