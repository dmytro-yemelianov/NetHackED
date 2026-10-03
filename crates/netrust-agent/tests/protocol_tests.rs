//! Protocol-level hardening tests for session, rpc helpers and stdio loop.

use netrust_agent::rpc::{parse_direction, parse_request, INVALID_REQUEST, PARSE_ERROR};
use netrust_agent::stdio::serve_lines;
use netrust_agent::AgentSession;
use netrust_sim::Direction;
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

    let r = parse_request(Ok(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)).unwrap();
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
        Ok(s) => Some(format!("ok:{}", s)),
        Err(()) => Some("bad".to_string()),
    })
    .unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), "bad\nok:hello\n");
}
