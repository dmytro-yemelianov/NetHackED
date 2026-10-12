use nethacked_persistence::*;
use sha2::{Digest, Sha256};
const EMPTY: &[u8] = include_bytes!("fixtures/empty.wire");
const GRAPH: &[u8] = include_bytes!("fixtures/graph.wire");
fn frame(body: &[u8]) -> Vec<u8> {
    let mut v = MAGIC.to_vec();
    v.extend_from_slice(&1u32.to_le_bytes());
    v.extend_from_slice(&(body.len() as u64).to_le_bytes());
    v.extend_from_slice(&Sha256::digest(body));
    v.extend_from_slice(body);
    v
}
fn mutation(f: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
    let mut v = serde_json::from_slice(&GRAPH[HEADER_LEN..]).unwrap();
    f(&mut v);
    frame(&serde_json::to_vec(&v).unwrap())
}
#[test]
fn independent_vectors_and_aliases() {
    for bytes in [EMPTY, GRAPH] {
        let save = decode(bytes, SaveLimits::default()).unwrap();
        assert_eq!(encode(&save, SaveLimits::default()).unwrap(), bytes);
    }
    let save = decode(GRAPH, SaveLimits::default()).unwrap();
    assert_eq!(
        save.catalog.callbacks[0].reservation,
        save.catalog.callbacks[1].reservation
    );
    assert_eq!(save.catalog.reservations.len(), 1);
    assert_eq!(save.catalog.modules[0].imports[0].name, "second");
    assert_eq!(save.catalog.current_dispatch[0].routes[0].name, "z");
    assert_eq!(
        save.catalog.workflows[0].trace,
        vec![Hex("02".into()), Hex("01".into())]
    );
}
#[test]
fn every_truncation_and_envelope_boundary() {
    for bytes in [EMPTY, GRAPH] {
        for end in 0..bytes.len() {
            assert_eq!(
                decode(&bytes[..end], SaveLimits::default()),
                Err(if end < HEADER_LEN {
                    Error::Truncated
                } else {
                    Error::Length
                })
            );
        }
    }
    let mut bytes = EMPTY.to_vec();
    bytes.push(0);
    assert_eq!(decode(&bytes, SaveLimits::default()), Err(Error::Length));
    let mut bytes = EMPTY.to_vec();
    bytes[12..20].copy_from_slice(&u64::MAX.to_le_bytes());
    assert_eq!(decode(&bytes, SaveLimits::default()), Err(Error::Length));
    let mut bytes = EMPTY.to_vec();
    bytes[8] = 2;
    assert_eq!(decode(&bytes, SaveLimits::default()), Err(Error::Version));
    let mut bytes = EMPTY.to_vec();
    bytes[20] ^= 1;
    assert_eq!(decode(&bytes, SaveLimits::default()), Err(Error::Digest));
    let mut bytes = EMPTY.to_vec();
    bytes[0] = 0;
    assert_eq!(decode(&bytes, SaveLimits::default()), Err(Error::Magic));
}
#[test]
fn canonical_syntax_refusals() {
    let body = std::str::from_utf8(&EMPTY[HEADER_LEN..]).unwrap();
    for altered in [
        body.replacen("{", "{ ", 1),
        body.replace(":9", ":9.0"),
        body.replace(":9", ":09"),
        body.replace(":9", ":9e0"),
        body.replace(":9", ":-9"),
        body.replace(":9", ":18446744073709551616"),
        body.replace("\"catalog\":", "\"catalog\":{},\"catalog\":"),
        body.replace("\"catalog\"", "\"\\u0063atalog\""),
    ] {
        assert!(
            decode(&frame(altered.as_bytes()), SaveLimits::default()).is_err(),
            "{altered}"
        );
    }
    assert_eq!(
        decode(&frame(&[0xff]), SaveLimits::default()),
        Err(Error::Json)
    );
    let unknown = mutation(|v| v["catalog"]["owners"][0]["codec"]["unknown"] = 1.into());
    assert_eq!(decode(&unknown, SaveLimits::default()), Err(Error::Schema));
    let body = std::str::from_utf8(&GRAPH[HEADER_LEN..]).unwrap();
    let duplicate = body.replacen(
        "\"name\":\"codec\"",
        "\"name\":\"codec\",\"name\":\"codec\"",
        1,
    );
    assert_eq!(
        decode(&frame(duplicate.as_bytes()), SaveLimits::default()),
        Err(Error::Noncanonical)
    );
    let ordered = frame(b"{\"profile_version\":1,\"format_version\":1}");
    assert_eq!(
        decode(&ordered, SaveLimits::default()),
        Err(Error::Noncanonical)
    );
}
#[test]
fn limits_both_directions() {
    let save = decode(GRAPH, SaveLimits::default()).unwrap();
    for limits in [
        SaveLimits {
            wire_bytes: GRAPH.len() - 1,
            ..SaveLimits::default()
        },
        SaveLimits {
            payload_bytes: 3,
            ..SaveLimits::default()
        },
        SaveLimits {
            rows_per_table: 1,
            ..SaveLimits::default()
        },
        SaveLimits {
            aggregate_edges: 1,
            ..SaveLimits::default()
        },
        SaveLimits {
            name_bytes: 3,
            ..SaveLimits::default()
        },
        SaveLimits {
            depth: 3,
            ..SaveLimits::default()
        },
    ] {
        assert!(decode(GRAPH, limits).is_err());
        assert!(encode(&save, limits).is_err());
    }
    let exact = SaveLimits {
        wire_bytes: GRAPH.len(),
        ..SaveLimits::default()
    };
    assert_eq!(encode(&save, exact).unwrap(), GRAPH);
    assert_eq!(decode(GRAPH, exact).unwrap(), save);
    assert!(decode(
        EMPTY,
        SaveLimits {
            wire_bytes: usize::MAX,
            ..SaveLimits::default()
        }
    )
    .is_err());
    let mut minimal = SaveLimits {
        rows_per_table: 0,
        aggregate_edges: 0,
        ..SaveLimits::default()
    };
    assert!(decode(EMPTY, minimal).is_ok());
    minimal.depth = 2;
    assert_eq!(decode(EMPTY, minimal), Err(Error::Depth));
    minimal.depth = 3;
    assert!(decode(EMPTY, minimal).is_ok());
    assert_eq!(
        Hex("00ff".into())
            .bytes(SaveLimits {
                payload_bytes: 2,
                ..SaveLimits::default()
            })
            .unwrap(),
        vec![0, 255]
    );
    assert_eq!(
        Hex("00ff".into()).bytes(SaveLimits {
            payload_bytes: 1,
            ..SaveLimits::default()
        }),
        Err(Error::Payload)
    );
}
#[test]
fn graph_refusals_have_specific_causes() {
    for (bytes, error) in [
        (
            mutation(|v| v["catalog"]["owners"][0]["id"]["kind"] = "module".into()),
            Error::Id,
        ),
        (
            mutation(|v| v["catalog"]["owners"][0]["id"]["serial"] = 0.into()),
            Error::Id,
        ),
        (
            mutation(|v| v["catalog"]["counters"][0]["high_water"] = 0.into()),
            Error::Counter,
        ),
        (
            mutation(|v| v["catalog"]["callbacks"][0]["source"]["mode"] = "live".into()),
            Error::Reference,
        ),
        (
            mutation(|v| v["catalog"]["callbacks"][0]["source"]["target"]["serial"] = 99.into()),
            Error::Reference,
        ),
        (
            mutation(|v| {
                v["catalog"]["callbacks"][0]["reservation"]["target"]["kind"] = "owner".into()
            }),
            Error::Reference,
        ),
        (
            mutation(|v| v["catalog"]["batches"][0]["ticket"] = 2.into()),
            Error::Counter,
        ),
        (
            mutation(|v| v["catalog"]["consumed_tickets"][0]["ticket"] = 6.into()),
            Error::Counter,
        ),
        (
            mutation(|v| v["catalog"]["publications"][0]["delivery_cursor"] = 2.into()),
            Error::Counter,
        ),
        (
            mutation(|v| v["catalog"]["input_services"][0]["ordinals"][0]["high_water"] = 3.into()),
            Error::Counter,
        ),
        (
            mutation(|v| v["catalog"]["owners"][0]["payload"] = "FF".into()),
            Error::Hex,
        ),
        (
            mutation(|v| v["catalog"]["owners"][0]["payload"] = "f".into()),
            Error::Hex,
        ),
        (
            mutation(|v| v["catalog"]["owners"][0]["codec"]["artifact"] = "00".into()),
            Error::Hex,
        ),
    ] {
        assert_eq!(decode(&bytes, SaveLimits::default()), Err(error));
    }
    assert!(decode(GRAPH, SaveLimits::default()).is_ok());
}

#[test]
fn every_typed_object_rejects_unknown_and_duplicate_fields() {
    fn objects(value: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
        match value {
            serde_json::Value::Object(map) => {
                out.push(value.clone());
                for v in map.values() {
                    objects(v, out);
                }
            }
            serde_json::Value::Array(array) => {
                for v in array {
                    objects(v, out);
                }
            }
            _ => {}
        }
    }
    let body = std::str::from_utf8(&GRAPH[HEADER_LEN..]).unwrap();
    let value = serde_json::from_str(body).unwrap();
    let mut nodes = Vec::new();
    objects(&value, &mut nodes);
    assert!(nodes.len() > 100);
    for node in nodes {
        let raw = serde_json::to_string(&node).unwrap();
        let pos = body.find(&raw).unwrap();
        let mut unknown = body.to_owned();
        unknown.insert_str(pos + 1, "\"__unknown\":0,");
        assert_eq!(
            decode(&frame(unknown.as_bytes()), SaveLimits::default()),
            Err(Error::Schema),
            "{raw}"
        );
        let (key, val) = node.as_object().unwrap().iter().next().unwrap();
        let pair = format!(
            "{}:{},",
            serde_json::to_string(key).unwrap(),
            serde_json::to_string(val).unwrap()
        );
        let mut duplicate = body.to_owned();
        duplicate.insert_str(pos + 1, &pair);
        assert_eq!(
            decode(&frame(duplicate.as_bytes()), SaveLimits::default()),
            Err(Error::Noncanonical),
            "{key}"
        );
    }
}
#[test]
fn exact_string_payload_and_table_edges() {
    let mut save = decode(GRAPH, SaveLimits::default()).unwrap();
    save.catalog.modules[0].manifest.name = "x".repeat(256);
    let limits = SaveLimits {
        payload_bytes: 4,
        rows_per_table: 15,
        ..SaveLimits::default()
    };
    let bytes = encode(&save, limits).unwrap();
    assert_eq!(decode(&bytes, limits).unwrap(), save);
    assert_eq!(
        decode(
            &bytes,
            SaveLimits {
                name_bytes: 255,
                ..limits
            }
        ),
        Err(Error::String)
    );
    save.catalog.modules[0].manifest.name.push('x');
    assert_eq!(encode(&save, limits), Err(Error::String));
    let mut save = decode(GRAPH, SaveLimits::default()).unwrap();
    save.catalog.callbacks.swap(0, 1);
    assert_eq!(encode(&save, SaveLimits::default()), Err(Error::Id));
    save.catalog.callbacks.swap(0, 1);
    save.catalog
        .callbacks
        .push(save.catalog.callbacks[1].clone());
    assert_eq!(encode(&save, SaveLimits::default()), Err(Error::Id));
    let widened = SaveLimits {
        depth: 33,
        ..SaveLimits::default()
    };
    assert_eq!(decode(EMPTY, widened), Err(Error::Limits));
    let valid = frame(b"{\"a\":\"\\u0001\\b\\t\\n\\f\\r\\\"\\\\\"}");
    assert_eq!(decode(&valid, SaveLimits::default()), Err(Error::Schema));
    let unicode = mutation(|v| v["catalog"]["modules"][0]["manifest"]["name"] = "é🚀".into());
    assert!(decode(&unicode, SaveLimits::default()).is_ok());
}
#[test]
fn counter_edges_and_missing_declared_rows() {
    for (bytes, error) in [
        (
            mutation(|v| v["catalog"]["current_dispatch"] = serde_json::json!([])),
            Error::Reference,
        ),
        (
            mutation(|v| v["catalog"]["reservations"] = serde_json::json!([])),
            Error::Reference,
        ),
        (
            mutation(|v| v["catalog"]["input_services"][0]["ledger"] = serde_json::json!([])),
            Error::Reference,
        ),
        (
            mutation(|v| {
                v["catalog"]["batches"][0]["candidates"] = serde_json::json!([
                    v["catalog"]["batches"][0]["candidates"][0].clone(),
                    v["catalog"]["batches"][0]["candidates"][0].clone()
                ])
            }),
            Error::Reference,
        ),
        (
            mutation(|v| {
                v["catalog"]["retired"] = serde_json::json!([{"kind":"owner","serial":1}])
            }),
            Error::Id,
        ),
    ] {
        assert_eq!(decode(&bytes, SaveLimits::default()), Err(error));
    }
    let bytes = mutation(|v| {
        v["catalog"]["consumed_tickets"].as_array_mut().unwrap().push(
            serde_json::json!({"issuing_host":17,"operation":{"mode":"historical","target":{"kind":"operation","serial":2}},"ticket":3,"checkpoint":1}));
        v["catalog"]["tombstones"].as_array_mut().unwrap().push(serde_json::json!({"id":{"kind":"operation","serial":2},"disposed_epoch":8,"last_sequence":2,"last_checkpoint":1}));
        v["catalog"]["counters"][2]["high_water"] = 2.into();
    });
    let mut save = decode(&bytes, SaveLimits::default()).unwrap();
    assert_eq!(encode(&save, SaveLimits::default()).unwrap(), bytes);
    save.catalog.tombstones[1].last_checkpoint = 0;
    assert_eq!(encode(&save, SaveLimits::default()), Err(Error::Counter));
}

#[test]
fn explicit_option_fields_are_required() {
    for field in ["guest_bytes", "extension_target"] {
        let bytes = mutation(|v| {
            v["catalog"]["modules"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
        });
        assert_eq!(decode(&bytes, SaveLimits::default()), Err(Error::Schema));
    }
    let bytes = mutation(|v| {
        v["catalog"]["publications"][0]
            .as_object_mut()
            .unwrap()
            .remove("batch");
    });
    assert_eq!(decode(&bytes, SaveLimits::default()), Err(Error::Schema));
    let nulls = mutation(|v| {
        v["catalog"]["modules"][0]["guest_bytes"] = serde_json::Value::Null;
        v["catalog"]["publications"][0]["batch"] = serde_json::Value::Null;
    });
    assert!(decode(&nulls, SaveLimits::default()).is_ok());
}

#[test]
fn exact_depth_edge_and_utf8_limits() {
    fn measure(v: &serde_json::Value, depth: usize) -> (usize, usize) {
        let mut deepest = depth;
        let mut edges = 0;
        match v {
            serde_json::Value::Object(map) => {
                if map.contains_key("target") && map.contains_key("mode") {
                    edges += 1;
                }
                for child in map.values() {
                    let (d, e) = measure(child, depth + 1);
                    deepest = deepest.max(d);
                    edges += e;
                }
            }
            serde_json::Value::Array(array) => {
                for child in array {
                    let (d, e) = measure(child, depth + 1);
                    deepest = deepest.max(d);
                    edges += e;
                }
            }
            _ => {}
        }
        (deepest, edges)
    }
    let v: serde_json::Value = serde_json::from_slice(&GRAPH[HEADER_LEN..]).unwrap();
    let (depth, edges) = measure(&v, 1);
    let exact = SaveLimits {
        depth,
        aggregate_edges: edges,
        ..SaveLimits::default()
    };
    let mut save = decode(GRAPH, exact).unwrap();
    assert_eq!(encode(&save, exact).unwrap(), GRAPH);
    assert_eq!(
        decode(
            GRAPH,
            SaveLimits {
                depth: depth - 1,
                ..exact
            }
        ),
        Err(Error::Depth)
    );
    assert_eq!(
        decode(
            GRAPH,
            SaveLimits {
                aggregate_edges: edges - 1,
                ..exact
            }
        ),
        Err(Error::Edges)
    );
    assert_eq!(
        encode(
            &save,
            SaveLimits {
                aggregate_edges: edges - 1,
                ..exact
            }
        ),
        Err(Error::Edges)
    );
    save.catalog.modules[0].manifest.name = "🚀".repeat(64);
    let bytes = encode(&save, SaveLimits::default()).unwrap();
    assert!(decode(&bytes, SaveLimits::default()).is_ok());
    assert_eq!(
        decode(
            &bytes,
            SaveLimits {
                name_bytes: 255,
                ..SaveLimits::default()
            }
        ),
        Err(Error::String)
    );
}
