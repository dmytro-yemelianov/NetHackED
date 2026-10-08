use nethacked_persistence::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
const GRAPH: &[u8] = include_bytes!("fixtures/graph.wire");
fn value() -> Value {
    serde_json::from_slice(include_bytes!("fixtures/graph.json")).unwrap()
}
fn frame(v: &Value) -> Vec<u8> {
    frame_body(&serde_json::to_vec(v).unwrap())
}
fn frame_body(body: &[u8]) -> Vec<u8> {
    let mut wire = MAGIC.to_vec();
    wire.extend(1u32.to_le_bytes());
    wire.extend((body.len() as u64).to_le_bytes());
    wire.extend(Sha256::digest(body));
    wire.extend(body);
    wire
}
fn both(v: &Value, expected: Result<(), Error>) {
    let limits = SaveLimits::default();
    let save: OwnerSaveV1 = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(encode(&save, limits).map(|_| ()), expected);
    assert_eq!(decode(&frame(v), limits).map(|_| ()), expected);
    if expected.is_ok() {
        assert_eq!(
            encode(&decode(&frame(v), limits).unwrap(), limits).unwrap(),
            frame(v)
        );
    }
}
fn duplicate(v: &mut Value) {
    let c = &mut v["catalog"];
    let mut candidate = c["candidates"][0].clone();
    candidate["id"]["serial"] = 2.into();
    candidate["receipt"]["serial"] = 2.into();
    c["candidates"].as_array_mut().unwrap().push(candidate);
    let mut batch = c["batches"][0].clone();
    batch["id"]["serial"] = 2.into();
    batch["candidates"][0]["target"]["serial"] = 2.into();
    c["batches"].as_array_mut().unwrap().push(batch);
    for counter in c["counters"].as_array_mut().unwrap() {
        if counter["kind"] == "batch" || counter["kind"] == "candidate" {
            counter["high_water"] = 2.into();
        }
    }
}
#[test]
fn f1_typed_independent_outer_declarations_and_nested_references() {
    let v = value();
    both(&v, Ok(()));
    assert!(v["catalog"]["publications"][0]["receipt"].is_object());
    // Declared writes/order must survive independently of candidate enrollment.
    let mut independent = v.clone();
    independent["catalog"]["batches"][0]["candidates"] = json!([]);
    both(&independent, Ok(()));
    for path in [
        "/catalog/batches/0/declaration/readonly/0/dependencies/0/code/target/serial",
        "/catalog/batches/0/declaration/readonly/0/receipt/recorded_inputs/0/record/target/serial",
        "/catalog/candidates/0/receipt/behavior/0/module/target/serial",
        "/catalog/candidates/0/prepared/helper/closure/0/permit/resolution/0/module/target/serial",
        "/catalog/publications/0/receipt/participants/0/caller/target/serial",
        "/catalog/publications/0/receipt/observations/0/dependencies/0/stamp/owner/target/serial",
        "/catalog/batches/0/declaration/required_writes/0/target/serial",
        "/catalog/batches/0/declaration/effect_order/0/target/serial",
    ] {
        let mut bad = v.clone();
        *bad.pointer_mut(path).unwrap() = 99.into();
        both(&bad, Err(Error::Reference));
    }
}
#[test]
fn f2a_host_issuance_is_independent_and_history_survives_disposal() {
    let v: Value = serde_json::from_slice(include_bytes!("fixtures/host-ticket.json")).unwrap();
    assert_eq!(frame(&v), include_bytes!("fixtures/host-ticket.wire"));
    both(&v, Ok(()));
    for path in [
        "/catalog/publication_domains/0/high_water",
        "/catalog/batches/0/ticket",
    ] {
        let mut bad = v.clone();
        *bad.pointer_mut(path).unwrap() = u64::MAX.into();
        both(&bad, Err(Error::Counter));
    }
    for (path, val) in [
        ("/catalog/publication_domains/0/high_water", 4),
        ("/catalog/publication_domains/0/issuing_host", 18),
        ("/catalog/batches/0/issuing_host", 0),
        ("/catalog/batches/0/ticket", 0),
    ] {
        let mut bad = v.clone();
        *bad.pointer_mut(path).unwrap() = val.into();
        both(&bad, Err(Error::Counter));
    }
    let mut historical = value();
    let c = &mut historical["catalog"];
    for key in [
        "workflows",
        "batches",
        "candidates",
        "leases",
        "pins",
        "cleanup",
        "input_records",
        "input_services",
        "timers",
    ] {
        c[key] = json!([]);
    }
    c["callbacks"] = json!([]);
    c["retention"] = json!([]);
    c["operations"] = json!([]);
    c["tombstones"].as_array_mut().unwrap().extend([
        json!({"id":{"kind":"operation","serial":1},"disposed_epoch":8,"last_sequence":0,"last_checkpoint":3}),
        json!({"id":{"kind":"lease","serial":1},"disposed_epoch":8,"last_sequence":0,"last_checkpoint":0}),
        json!({"id":{"kind":"pin","serial":1},"disposed_epoch":8,"last_sequence":0,"last_checkpoint":0}),
        json!({"id":{"kind":"input_record","serial":1},"disposed_epoch":8,"last_sequence":0,"last_checkpoint":0})]);
    fn historical_refs(v: &mut Value) {
        match v {
            Value::Object(m) => {
                if m.contains_key("target") && m.contains_key("mode") {
                    let k = m["target"]["kind"].as_str().unwrap();
                    if matches!(k, "operation" | "lease" | "pin" | "input_record") {
                        m.insert("mode".into(), "historical".into());
                    }
                } else {
                    for x in m.values_mut() {
                        historical_refs(x);
                    }
                }
            }
            Value::Array(a) => {
                for x in a {
                    historical_refs(x);
                }
            }
            _ => {}
        }
    }
    historical_refs(&mut c["publications"]);
    historical_refs(&mut c["consumed_tickets"]);
    both(&historical, Ok(()));
    historical["catalog"]["tombstones"][1]["last_checkpoint"] = 0.into();
    both(&historical, Err(Error::Counter));
}
#[test]
fn f2b_initial_suspended_and_resumed_tickets() {
    let initial: Value = serde_json::from_slice(include_bytes!("fixtures/initial.json")).unwrap();
    assert_eq!(frame(&initial), include_bytes!("fixtures/initial.wire"));
    both(&initial, Ok(()));
    let mut suspended = value();
    let w = &mut suspended["catalog"]["workflows"][0];
    w["status"] = "paused".into();
    w["ticket"]["sequence"] = 4.into();
    w["ticket"]["phase"] = "apply".into();
    both(&suspended, Ok(()));
    let mut resumed = suspended.clone();
    resumed["catalog"]["workflows"][0]["status"] = "running".into();
    resumed["catalog"]["workflows"][0]["sequence"] = 5.into();
    both(&resumed, Ok(()));
    for (path, val) in [
        ("/catalog/workflows/0/ticket", Value::Null),
        ("/catalog/workflows/0/ticket/sequence", 0.into()),
        ("/catalog/workflows/0/ticket/sequence", 5.into()),
        ("/catalog/workflows/0/ticket/phase", "wrong".into()),
        (
            "/catalog/workflows/0/ticket/pipeline_digest",
            "07".repeat(32).into(),
        ),
        ("/catalog/workflows/0/checkpoint", 4.into()),
    ] {
        let mut bad = suspended.clone();
        *bad.pointer_mut(path).unwrap() = val;
        both(&bad, Err(Error::Counter));
    }
    let mut lease_bad = suspended.clone();
    lease_bad["catalog"]["workflows"][0]["ticket"]["lease"]["target"]["serial"] = 2.into();
    both(&lease_bad, Err(Error::Reference));
    // A present but different lease tests ticket identity independently of graph existence.
    let mut other_lease = suspended.clone();
    let mut lease = other_lease["catalog"]["leases"][0].clone();
    lease["id"]["serial"] = 2.into();
    other_lease["catalog"]["leases"]
        .as_array_mut()
        .unwrap()
        .push(lease);
    for counter in other_lease["catalog"]["counters"].as_array_mut().unwrap() {
        if counter["kind"] == "lease" {
            counter["high_water"] = 2.into();
        }
    }
    other_lease["catalog"]["workflows"][0]["ticket"]["lease"]["target"]["serial"] = 2.into();
    both(&other_lease, Err(Error::Counter));
    let mut bad = resumed;
    bad["catalog"]["workflows"][0]["ticket"]["sequence"] = 5.into();
    both(&bad, Err(Error::Counter));
}
#[test]
fn f3_ticket_uniqueness_covers_candidates_operations_domains_and_consumption() {
    let mut dup = value();
    duplicate(&mut dup);
    both(&dup, Err(Error::Counter));
    let mut distinct = dup.clone();
    distinct["catalog"]["batches"][1]["ticket"] = 4.into();
    both(&distinct, Ok(()));
    let mut domains = dup.clone();
    domains["catalog"]["publication_domains"]
        .as_array_mut()
        .unwrap()
        .push(json!({"issuing_host":18,"high_water":5}));
    domains["catalog"]["batches"][1]["issuing_host"] = 18.into();
    both(&domains, Ok(()));
    let mut collision = distinct.clone();
    collision["catalog"]["batches"][1]["ticket"] = 2.into();
    both(&collision, Err(Error::Counter));
    let mut cross = dup;
    let c = &mut cross["catalog"];
    let mut op = c["operations"][0].clone();
    op["id"]["serial"] = 2.into();
    c["operations"].as_array_mut().unwrap().push(op);
    c["counters"][2]["high_water"] = 2.into();
    c["batches"][1]["operation"]["target"]["serial"] = 2.into();
    c["batches"][1]["declaration"]["binding"]["operation"]["target"]["serial"] = 2.into();
    c["candidates"][1]["operation"]["target"]["serial"] = 2.into();
    both(&cross, Err(Error::Counter));
    let mut used = value();
    used["catalog"]["consumed_tickets"].as_array_mut().unwrap().push(json!({"issuing_host":17,"operation":{"mode":"live","target":{"kind":"operation","serial":1}},"ticket":5,"checkpoint":3}));
    both(&used, Err(Error::Counter));
}
// One representative of every object shape, including both scope variants/nulls,
// avoids retesting repeated nested receipts while checking each required field.
fn shapes(v: &Value, path: String, out: &mut std::collections::BTreeMap<Vec<String>, String>) {
    match v {
        Value::Object(m) => {
            let keys = m.keys().cloned().collect();
            out.entry(keys).or_insert(path.clone());
            for (k, x) in m {
                shapes(x, format!("{path}/{k}"), out);
            }
        }
        Value::Array(a) => {
            for (i, x) in a.iter().enumerate() {
                shapes(x, format!("{path}/{i}"), out);
            }
        }
        _ => {}
    }
}
#[test]
fn every_nested_field_is_required_unknown_and_duplicate_refused() {
    let mut v = value();
    v["catalog"]["batches"][0]["declaration"]["coordinator"]["effective"]["scope"] =
        json!({"kind":"all"});
    let mut paths = std::collections::BTreeMap::new();
    shapes(&v, String::new(), &mut paths);
    for path in paths.values() {
        let object = v.pointer(path).unwrap().as_object().unwrap();
        for key in object.keys() {
            let mut bad = v.clone();
            bad.pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(
                decode(&frame(&bad), SaveLimits::default()).is_err(),
                "missing {path}/{key}"
            );
        }
        let mut bad = v.clone();
        bad.pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), Value::Null);
        assert!(
            decode(&frame(&bad), SaveLimits::default()).is_err(),
            "unknown {path}"
        );
        let encoded = serde_json::to_string(object).unwrap();
        for (key, x) in object {
            let duplicated = format!(
                "{{{}:{},{}",
                serde_json::to_string(key).unwrap(),
                x,
                &encoded[1..]
            );
            let body = serde_json::to_string(&v)
                .unwrap()
                .replacen(&encoded, &duplicated, 1);
            assert!(
                decode(&frame_body(body.as_bytes()), SaveLimits::default()).is_err(),
                "duplicate {path}"
            );
        }
    }
}
fn edges(v: &Value) -> usize {
    match v {
        Value::Object(m) if m.contains_key("mode") && m.contains_key("target") => 1,
        Value::Object(m) => m.values().map(edges).sum(),
        Value::Array(a) => a.iter().map(edges).sum(),
        _ => 0,
    }
}
#[test]
fn exact_edge_preflight_and_digest_payload_bounds() {
    let v = value();
    let n = edges(&v);
    let save: OwnerSaveV1 = serde_json::from_value(v.clone()).unwrap();
    for (bound, expected) in [(n, Ok(())), (n - 1, Err(Error::Edges))] {
        let limits = SaveLimits {
            aggregate_edges: bound,
            ..SaveLimits::default()
        };
        assert_eq!(encode(&save, limits).map(|_| ()), expected);
        assert_eq!(decode(&frame(&v), limits).map(|_| ()), expected);
    }
    // provider_name/owner_name/target capability labels cannot add synthetic edges.
    let mut label = v.clone();
    label["catalog"]["modules"][0]["manifest"]["extension"] = Value::Null;
    let ln = edges(&label);
    let ls: OwnerSaveV1 = serde_json::from_value(label.clone()).unwrap();
    assert_eq!(
        encode(
            &ls,
            SaveLimits {
                aggregate_edges: ln,
                ..SaveLimits::default()
            }
        )
        .unwrap(),
        frame(&label)
    );
    for path in [
        "/catalog/candidates/0/receipt/next_digest",
        "/catalog/candidates/0/receipt/request_digest",
        "/catalog/batches/0/declaration/caller_artifact",
        "/catalog/publications/0/receipt/observations/0/data_digest",
        "/catalog/workflows/0/pipeline_digest",
    ] {
        let mut bad = v.clone();
        *bad.pointer_mut(path).unwrap() = "00".into();
        both(&bad, Err(Error::Hex));
    }
    assert!(decode(GRAPH, SaveLimits::default()).is_ok());
}
#[test]
fn every_added_semantic_array_has_exact_limit_and_order_fidelity() {
    fn first_shape(v: &Value) -> String {
        let first = &v[0];
        match first {
            Value::Object(m) => m.keys().cloned().collect::<Vec<_>>().join(","),
            _ => first.to_string(),
        }
    }
    let v = value();
    let mut paths = Vec::new();
    fn arrays(v: &Value, p: String, out: &mut Vec<String>) {
        match v {
            Value::Object(m) => {
                for (k, x) in m {
                    arrays(x, format!("{p}/{k}"), out);
                }
            }
            Value::Array(a) => {
                if !a.is_empty() {
                    out.push(p.clone());
                }
                for (i, x) in a.iter().enumerate() {
                    arrays(x, format!("{p}/{i}"), out);
                }
            }
            _ => {}
        }
    }
    arrays(&v, String::new(), &mut paths);
    let mut seen = std::collections::BTreeSet::new();
    for path in paths {
        let key = path.rsplit('/').next().unwrap();
        if !matches!(
            key,
            "required_writes"
                | "effect_order"
                | "readonly"
                | "participants"
                | "dependencies"
                | "recorded_inputs"
                | "resolution"
                | "resources"
                | "behavior"
                | "code"
                | "closure"
                | "events"
                | "cleanup_identities"
                | "publication_domains"
        ) || !seen.insert((key.to_string(), first_shape(v.pointer(&path).unwrap())))
        {
            continue;
        }
        let first = v.pointer(&path).unwrap()[0].clone();
        let mut exact = v.clone();
        let entries: Vec<Value> = (0..15)
            .map(|i| {
                if key == "cleanup_identities" {
                    (i + 1).into()
                } else if key == "publication_domains" {
                    json!({"issuing_host":17+i,"high_water":5})
                } else {
                    first.clone()
                }
            })
            .collect();
        *exact.pointer_mut(&path).unwrap() = entries.into();
        // Shared live/history declarations stay independently encoded. Arrays of
        // receipt observations are bounded before allocation just like tables.
        let limits = SaveLimits {
            rows_per_table: 15,
            ..SaveLimits::default()
        };
        let save: OwnerSaveV1 = serde_json::from_value(exact.clone()).unwrap();
        assert_eq!(encode(&save, limits).map(|_| ()), Ok(()), "{path}");
        assert_eq!(
            encode(&decode(&frame(&exact), limits).unwrap(), limits).unwrap(),
            frame(&exact)
        );
        exact
            .pointer_mut(&path)
            .unwrap()
            .as_array_mut()
            .unwrap()
            .push(first);
        let over: OwnerSaveV1 = serde_json::from_value(exact.clone()).unwrap();
        assert_eq!(encode(&over, limits), Err(Error::Rows), "{path}");
        assert_eq!(decode(&frame(&exact), limits), Err(Error::Rows), "{path}");
    }
    // Profile order is retained verbatim, independently of lexical resource names.
    let mut order = v.clone();
    order["catalog"]["batches"][0]["declaration"]["coordinator"]["effective"]["scope"]["resources"]
        .as_array_mut()
        .unwrap()
        .reverse();
    both(&order, Ok(()));
}

#[test]
fn independent_multi_owner_effect_order_is_exact() {
    let v: Value = serde_json::from_slice(include_bytes!("fixtures/multi-owner.json")).unwrap();
    assert_eq!(frame(&v), include_bytes!("fixtures/multi-owner.wire"));
    both(&v, Ok(()));
    assert_ne!(
        v["catalog"]["batches"][0]["declaration"]["required_writes"],
        v["catalog"]["batches"][0]["declaration"]["effect_order"]
    );
    let mut order = v.clone();
    order["catalog"]["batches"][0]["declaration"]["effect_order"]
        .as_array_mut()
        .unwrap()
        .reverse();
    both(&order, Ok(()));
    // The initial no-ticket branch still validates checkpoint and lease provenance.
    let mut initial: Value =
        serde_json::from_slice(include_bytes!("fixtures/initial.json")).unwrap();
    initial["catalog"]["operations"][0]["checkpoint"] = 1.into();
    initial["catalog"]["workflows"][0]["checkpoint"] = 1.into();
    both(&initial, Err(Error::Counter));
}
