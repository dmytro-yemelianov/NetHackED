mod support;
use nethacked_inputs::{Reply, Service};
use nethacked_kernel::*;
use nethacked_resources::{OwnerPolicy, Service as Resources};
use nethacked_workflows::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
use support::*;

fn program(provider: ModuleHandle) -> Program {
    Program {
        provider,
        endpoint: "step".into(),
        authority: authority(),
        initial: "select".into(),
        phases: BTreeSet::from(["select".into(), "reply".into()]),
        allowed: BTreeSet::from([("select".into(), "reply".into())]),
    }
}
// Current fixture pattern: fresh guest memory, phase search, real kernel imports.
// Host StepRequest.input carries opaque replay bytes, never operation credentials.
fn guest(version: &str, fault: &str, replay_value: u8) -> Vec<u8> {
    let mut data = String::new();
    let mut offset = 8000;
    let mut store = |bytes: &[u8]| {
        let p = offset;
        offset += bytes.len() + 16;
        data.push_str(&format!("(data (i32.const {p}) \"{}\")", escaped(bytes)));
        (p, bytes.len())
    };
    let (select, sn) = store(b"\"select\"");
    let (reply, rn) = store(b"\"reply\"");
    let (input, inn) = store(b"\"input\":[");
    let (replay_marker, replay_marker_n) = store(b"\"kind\":\"replay\"");
    let (sp, sl) = store(&serde_json::to_vec(&sample(b"request")).unwrap());
    let (vp, vl) = store(format!("\"value\":[{replay_value}]").as_bytes());
    let suspend = Outcome::Suspend {
        next: "reply".into(),
        prompt: b"choose".to_vec(),
        state: vec![],
        events: vec![version.as_bytes().to_vec()],
    };
    let (op, on) = store(
        if fault == "malformed" {
            b"invalid".to_vec()
        } else if fault == "reject" {
            serde_json::to_vec(&Outcome::Reject { events: vec![] }).unwrap()
        } else {
            serde_json::to_vec(&suspend).unwrap()
        }
        .as_slice(),
    );
    let (fp, fl) = store(
        &serde_json::to_vec(&Outcome::Finish {
            result: version.as_bytes().to_vec(),
            state: vec![],
            events: vec![],
        })
        .unwrap(),
    );
    let trap = if fault == "trap" { "unreachable" } else { "" };
    let theft = if version == "new" {
        format!(
            "local.get $n call $decode local.set $len
        i32.const 0 i32.const 20000 local.get $len i32.const 32768 i32.const {WIRE} call $call
        i32.const {} i32.ne if unreachable end",
            nethacked_kernel::Error::Authority.abi_code()
        )
    } else {
        String::new()
    };
    wat::parse_str(format!(r#"(module
      (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "alloc") (param i32) (result i32) i32.const 0)
      {data}
      (func $find (param $base i32) (param $n i32) (param $p i32) (param $l i32) (result i32)
        (local $i i32) (local $j i32)
        block $no loop $outer
          local.get $i local.get $l i32.add local.get $n i32.gt_u br_if $no
          i32.const 0 local.set $j
          block $mismatch loop $inner
            local.get $j local.get $l i32.eq if local.get $base local.get $i i32.add return end
            local.get $base local.get $i i32.add local.get $j i32.add i32.load8_u
            local.get $p local.get $j i32.add i32.load8_u i32.ne br_if $mismatch
            local.get $j i32.const 1 i32.add local.set $j br $inner
          end end local.get $i i32.const 1 i32.add local.set $i br $outer
        end end i32.const -1)
      (func $decode (param $n i32) (result i32) (local $p i32) (local $len i32) (local $v i32) (local $c i32)
        i32.const 0 local.get $n i32.const {input} i32.const {inn} call $find
        local.tee $p i32.const 0 i32.lt_s if unreachable end
        local.get $p i32.const {inn} i32.add local.set $p
        block $done loop $read
          local.get $p local.get $n i32.ge_u if unreachable end
          local.get $p i32.load8_u local.tee $c i32.const 93 i32.eq br_if $done
          local.get $c i32.const 44 i32.eq if
            i32.const 20000 local.get $len i32.add local.get $v i32.store8
            local.get $len i32.const 1 i32.add local.set $len i32.const 0 local.set $v
          else
            local.get $v i32.const 10 i32.mul local.get $c i32.const 48 i32.sub i32.add local.set $v
          end local.get $p i32.const 1 i32.add local.set $p br $read
        end end
        i32.const 20000 local.get $len i32.add local.get $v i32.store8
        local.get $len i32.const 1 i32.add)
      (func $sample
        i32.const 0 i32.const {sp} i32.const {sl} i32.const 32768 i32.const {WIRE} call $call
        i32.const 0 i32.lt_s if unreachable end)
      (func (export "step") (param i32) (param $n i32) (result i64) (local $len i32) (local $r i32)
        i32.const 0 local.get $n i32.const {replay_marker} i32.const {replay_marker_n} call $find i32.const 0 i32.ge_s if
          i32.const 0 i32.const 0 local.get $n i32.const 32768 i32.const {WIRE} call $call local.tee $r
          i32.const 0 i32.lt_s if unreachable end
          i64.const 140737488355328 local.get $r i64.extend_i32_u i64.or return end
        i32.const 0 local.get $n i32.const {select} i32.const {sn} call $find i32.const 0 i32.ge_s if
          {theft} call $sample {trap} i64.const {} return end
        i32.const 0 local.get $n i32.const {reply} i32.const {rn} call $find i32.const 0 i32.ge_s if
          local.get $n call $decode local.set $len
          i32.const 0 i32.const 20000 local.get $len i32.const 32768 i32.const {WIRE} call $call local.tee $r
          i32.const 0 i32.lt_s if unreachable end
          i32.const 32768 local.get $r i32.const {vp} i32.const {vl} call $find
          i32.const 0 i32.lt_s if unreachable end
          call $sample i64.const {} return end unreachable))"#,
        ((op as u64)<<32)|on as u64, ((fp as u64)<<32)|fl as u64)).unwrap()
}
fn install_guest(
    h: &mut Host,
    id: &str,
    version: &str,
    fault: &str,
    expected: u8,
    extension: bool,
) -> ModuleHandle {
    let mut m = manifest(id);
    if extension {
        m.extension = Some(Extension {
            target: Capability {
                provider: "workflow".into(),
                endpoint: "step".into(),
                major: 1,
            },
            export: "step".into(),
        });
    }
    let g =
        h.k.register_guest(m, &guest(version, fault, expected))
            .unwrap();
    h.k.issue_grant(g, 0, authority()).unwrap();
    g
}
fn first(s: &Service) -> Reply {
    let r = s.inspect().unwrap().records[0].clone();
    Reply {
        id: r.id,
        value: r.value,
    }
}
fn populated_resources(k: &mut Kernel) -> Resources {
    let r = Resources::new(nethacked_resources::Limits::default(), Rc::new(|_| true)).unwrap();
    r.install(k, "resources").unwrap();
    let mut m = manifest("ledger");
    m.dependencies.clear();
    let guard = k
        .register_native(
            m,
            b"ledger-v1",
            BTreeMap::from([(
                "step".into(),
                Rc::new(|_: &[u8], _: &CallerView| Ok(vec![])) as NativePort,
            )]),
        )
        .unwrap();
    r.register_owner(
        "ledger",
        OwnerPolicy {
            capability: Capability {
                provider: "ledger".into(),
                endpoint: "step".into(),
                major: 1,
            },
            provider: guard,
            artifact: k.artifact(guard).unwrap(),
            validate: Rc::new(|_| true),
        },
    )
    .unwrap();
    r.put(
        Resource {
            owner: "ledger".into(),
            key: "unrelated".into(),
        },
        b"populated".to_vec(),
    )
    .unwrap();
    r
}

#[test]
fn authoritative_pause_resume_replays_retained_code_and_new_operation_cannot_steal() {
    let mut h = Host::new();
    let resources = populated_resources(&mut h.k);
    let unrelated = resources.state().unwrap();
    let g = install_guest(&mut h, "workflow", "old", "", 10, false);
    let w = Workflows::new(nethacked_workflows::Limits::default()).unwrap();
    let old = w.start(&mut h.k, program(g), vec![], vec![]).unwrap();
    assert_eq!(
        w.step(&h.k, old, b"").unwrap(),
        Status::Suspended(b"choose".to_vec())
    );
    let recorded = first(&h.s);
    let prefix = h.s.inspect().unwrap();
    assert_eq!(h.calls.get(), 1);
    let old_ticket = w.ticket(old).unwrap();
    let old_pipeline = w.inspect(old).unwrap().pipeline;
    let ext = install_guest(&mut h, "workflow-new", "new", "", 20, true);
    let new = w.start(&mut h.k, program(g), vec![], vec![]).unwrap();
    assert_ne!(w.inspect(new).unwrap().pipeline.hash, old_pipeline.hash);
    let theft = serde_json::to_vec(&replay(&recorded, b"request")).unwrap();
    assert_eq!(
        w.step(&h.k, new, &theft).unwrap(),
        Status::Suspended(b"choose".to_vec())
    );
    let new_record = h.s.inspect().unwrap().records[1].clone();
    assert_eq!(new_record.caller, ext);
    assert_eq!(new_record.value, [20]);
    // The negative theft call did not sample; only the explicit following sample did.
    assert_eq!(h.calls.get(), 2);
    assert_eq!(h.s.inspect().unwrap().records[0], prefix.records[0]);
    w.resume(&old_ticket).unwrap();
    assert_eq!(
        w.step(&h.k, old, &theft).unwrap(),
        Status::Finished(b"old".to_vec())
    );
    let state = h.s.inspect().unwrap();
    assert_eq!(h.calls.get(), 3);
    assert_eq!(state.source, [40]);
    assert_eq!(state.records[2].caller, g);
    assert_eq!(state.records[2].ordinal, 1);
    assert_eq!(
        state.records[2].operation.pipeline(),
        prefix.records[0].operation.pipeline()
    );
    assert_eq!(state.records[0], prefix.records[0]);
    assert_eq!(resources.state().unwrap(), unrelated);
    // Its own ID succeeds under the new retained pipeline after resume.
    w.resume(&w.ticket(new).unwrap()).unwrap();
    assert_eq!(
        w.step(
            &h.k,
            new,
            &serde_json::to_vec(&replay(
                &Reply {
                    id: new_record.id,
                    value: new_record.value
                },
                b"request"
            ))
            .unwrap()
        )
        .unwrap(),
        Status::Finished(b"new".to_vec())
    );
    assert_eq!(h.calls.get(), 4);
    assert!(h.s.inspect().unwrap().source.is_empty());
    assert_eq!(resources.state().unwrap(), unrelated);
}

#[test]
fn committed_prefix_survives_guest_trap_malformed_reject_and_cancellation() {
    for mode in ["trap", "malformed", "reject", "cancel"] {
        let mut h = Host::new();
        let g = install_guest(&mut h, "workflow", "old", mode, 10, false);
        let w = Workflows::new(nethacked_workflows::Limits::default()).unwrap();
        let id = w.start(&mut h.k, program(g), vec![], vec![]).unwrap();
        let result = w.step(&h.k, id, b"").unwrap();
        match mode {
            "trap" => assert_eq!(
                result,
                Status::Faulted(Fault::Invocation(nethacked_kernel::Error::Trap))
            ),
            "malformed" => assert_eq!(result, Status::Faulted(Fault::InvalidOutcome)),
            "reject" => assert_eq!(result, Status::Rejected),
            "cancel" => {
                assert_eq!(result, Status::Suspended(b"choose".to_vec()));
                w.abandon(id).unwrap();
            }
            _ => unreachable!(),
        }
        let state = h.s.inspect().unwrap();
        let r = first(&h.s);
        assert_eq!(state.records.len(), 1);
        assert_eq!(state.source, [20, 30, 40]);
        assert_eq!(h.calls.get(), 1);
        assert_eq!(
            w.step(&h.k, id, b""),
            Err(nethacked_workflows::Error::Status)
        );
        let record = &state.records[0];
        let ticket = Ticket {
            operation: id,
            sequence: 1,
            pipeline: record.operation.pipeline(),
            code: w.inspect(id).unwrap().pipeline.hash,
            phase: "reply".into(),
        };
        assert_eq!(w.resume(&ticket), Err(nethacked_workflows::Error::Status));
        // A separate forwarding guest pipeline cannot consume the live receipt.
        h.refuse(&h.binding(1, "reply"), &replay(&r, b"request"), authority());
        assert_eq!(h.s.inspect().unwrap(), state);
        assert_eq!(h.calls.get(), 1);
        // Retained original operation can replay even after its workflow terminates.
        let later =
            h.k.operation_binding(
                id,
                record.operation.pipeline(),
                record.operation.checkpoint() + 1,
                "later",
            )
            .unwrap();
        let encoded = serde_json::to_vec(&replay(&r, b"request")).unwrap();
        for binding in [&record.operation, &later] {
            assert_eq!(
                serde_json::from_slice::<Reply>(
                    &h.k.invoke_operation(binding, &encoded, authority())
                        .unwrap()
                )
                .unwrap(),
                r
            );
            assert_eq!(h.s.inspect().unwrap(), state);
            assert_eq!(h.calls.get(), 1);
        }
        w.dispose(&mut h.k, id).unwrap();
        assert_eq!(h.s.inspect().unwrap(), state);
        assert_eq!(
            h.k.invoke_operation(
                &record.operation,
                &serde_json::to_vec(&replay(&r, b"request")).unwrap(),
                authority()
            ),
            Err(nethacked_kernel::Error::InvalidHandle)
        );
        assert_eq!(h.calls.get(), 1);
        assert_eq!(h.s.inspect().unwrap(), state);
    }
}
