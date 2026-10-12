mod support;
use nethacked_kernel::{CallerView, Error as KError, NativePort};
use nethacked_workflows::*;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
use support::*;
fn program(h: nethacked_kernel::ModuleHandle, version: &str) -> Program {
    let cleanup = format!("{version}_cleanup");
    Program {
        provider: h,
        endpoint: "step".into(),
        authority: authority(),
        initial: "select".into(),
        phases: ["select", "reply", &cleanup, "time", "finish"]
            .into_iter()
            .map(String::from)
            .collect(),
        allowed: [
            ("select", "reply"),
            ("reply", &cleanup),
            (&cleanup, "time"),
            ("time", "finish"),
        ]
        .into_iter()
        .map(|(a, b)| (a.into(), b.into()))
        .collect(),
    }
}
fn events(s: &str) -> Vec<Vec<u8>> {
    vec![s.as_bytes().to_vec()]
}
fn suspend() -> Outcome {
    Outcome::Suspend {
        next: "reply".into(),
        prompt: b"direction".to_vec(),
        state: b"local".to_vec(),
        events: events("charged"),
    }
}
fn advance(next: &str, event: &str) -> Outcome {
    Outcome::Advance {
        next: next.into(),
        state: b"local".to_vec(),
        events: events(event),
    }
}
fn finish(v: &str) -> Outcome {
    Outcome::Finish {
        result: v.as_bytes().to_vec(),
        state: v.as_bytes().to_vec(),
        events: events("finished"),
    }
}
/// Actual Wasm phases call real resource ports. Guest RAM is fresh per invocation.
fn workflow_guest(h: &Host, version: &str, fault: &str) -> String {
    let mut data = String::new();
    let mut offset = 8000;
    let mut store = |b: &[u8]| {
        let p = offset;
        offset += b.len() + 16;
        data.push_str(&format!("(data (i32.const {p}) \"{}\")", bytes(b)));
        (p, b.len())
    };
    let (charge_p, charge_n) = store(&serde_json::to_vec(&plan(h.charge.clone(), b"1")).unwrap());
    let (time_p, time_n) = store(&serde_json::to_vec(&plan(h.time.clone(), b"1")).unwrap());
    let mut checks = String::new();
    let cleanup = format!("{version}_cleanup");
    for (phase, outcome, action) in [
        (
            "select",
            suspend(),
            if fault == "abandon" {
                format!("i32.const 0 i32.const {charge_p} i32.const {charge_n} i32.const 32768 i32.const {WIRE} call $call drop")
            } else {
                format!("i32.const {charge_p} i32.const {charge_n} call $tx")
            },
        ),
        ("reply", advance(&cleanup, "cancel"), String::new()),
        (&cleanup, advance("time", "cleanup"), String::new()),
        (
            "time",
            advance("finish", "time"),
            if version == "new" {
                String::new()
            } else {
                format!("i32.const {time_p} i32.const {time_n} call $tx")
            },
        ),
        ("finish", finish(version), String::new()),
    ] {
        let (p, n) = store(format!("\"{phase}\"").as_bytes());
        let result = if phase == "select" && fault == "malformed" {
            b"invalid".to_vec()
        } else if phase == "select" && fault == "illegal" {
            serde_json::to_vec(&advance("missing", "bad")).unwrap()
        } else if phase == "select" && fault == "abandon" {
            serde_json::to_vec(&Outcome::Reject {
                events: events("abandoned"),
            })
            .unwrap()
        } else {
            serde_json::to_vec(&outcome).unwrap()
        };
        let (r, rn) = store(&result);
        let reject = if phase == "select" && fault.is_empty() {
            let (pattern, pn) = store(b"\"input\":[1]");
            let (rj, rjn) = store(
                &serde_json::to_vec(&Outcome::Reject {
                    events: events("selection_refused"),
                })
                .unwrap(),
            );
            format!("local.get $n i32.const {pattern} i32.const {pn} call $contains i32.eqz if i64.const {} return end", ((rj as u64) << 32) | rjn as u64)
        } else {
            String::new()
        };
        let trap = if phase == "select" && fault == "trap" {
            "unreachable"
        } else {
            ""
        };
        checks.push_str(&format!("local.get $n i32.const {p} i32.const {n} call $contains if {reject} {action} {trap} i64.const {} return end ", ((r as u64) << 32) | rn as u64));
    }
    format!(
        r#"(module
    (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
    (memory (export "memory") 1) (func (export "alloc") (param i32) (result i32) i32.const 0)
    {data}
    (func $contains (param $n i32) (param $p i32) (param $l i32) (result i32) (local $i i32) (local $j i32)
      block $no loop $outer
        local.get $i local.get $l i32.add local.get $n i32.gt_u br_if $no
        i32.const 0 local.set $j
        block $mismatch loop $inner
          local.get $j local.get $l i32.eq if i32.const 1 return end
          local.get $i local.get $j i32.add i32.load8_u
          local.get $p local.get $j i32.add i32.load8_u i32.ne br_if $mismatch
          local.get $j i32.const 1 i32.add local.set $j br $inner
        end end
        local.get $i i32.const 1 i32.add local.set $i br $outer
      end end i32.const 0)
    (func $tx (param $p i32) (param $n i32) (local $r i32)
      i32.const 0 local.get $p local.get $n i32.const 32768 i32.const {WIRE} call $call local.tee $r
      i32.const 0 i32.lt_s if unreachable end
      i32.const 1 i32.const 32768 local.get $r i32.const 49152 i32.const {WIRE} call $call
      i32.const 0 i32.lt_s if unreachable end)
    (func (export "step") (param i32) (param $n i32) (result i64) {checks} unreachable))"#
    )
}
fn start(
    w: &Workflows,
    h: &mut Host,
    module: nethacked_kernel::ModuleHandle,
    version: &str,
) -> nethacked_kernel::OperationId {
    w.start(
        &mut h.k,
        program(module, version),
        vec![],
        vec![Rc::new(h.service.clone())],
    )
    .unwrap()
}
#[test]
fn actual_wasm_charge_cancel_ordered_time_once_and_early_rejection() {
    let mut h = Host::new();
    let w = Workflows::new(Limits::default()).unwrap();
    let source = workflow_guest(&h, "old", "");
    let module = h.guest("actor", &source, None);
    let rejected = start(&w, &mut h, module, "old");
    let resources = h.service.state().unwrap();
    assert_eq!(w.step(&h.k, rejected, &[]), Ok(Status::Rejected));
    assert_eq!(h.service.state().unwrap(), resources);
    assert_eq!(w.inspect(rejected).unwrap().state, Vec::<u8>::new());
    let id = start(&w, &mut h, module, "old");
    assert!(matches!(w.step(&h.k, id, &[1]), Ok(Status::Suspended(_))));
    let before = w.inspect(id).unwrap();
    assert_eq!(w.step(&h.k, id, b"cancel"), Err(Error::Status));
    assert_eq!(w.inspect(id).unwrap(), before);
    let ticket = w.ticket(id).unwrap();
    w.resume(&ticket).unwrap();
    for _ in 0..4 {
        w.step(&h.k, id, b"cancel").unwrap();
    }
    let f = w.inspect(id).unwrap();
    assert_eq!(f.status, Status::Finished(b"old".to_vec()));
    assert_eq!(f.sequence, 6);
    assert_eq!(
        f.trace,
        ["charged", "cancel", "cleanup", "time", "finished"].map(|s| s.as_bytes().to_vec())
    );
    let state = h.service.state().unwrap();
    assert_eq!(state.commits, 2);
    assert_eq!(state.resources[&address("charge")].value, b"1");
    assert_eq!(state.resources[&address("time")].value, b"1");
    assert_eq!(state.resources[&address("unrelated")].value, b"untouched");
    assert_eq!(w.step(&h.k, id, &[1]), Err(Error::Status));
    assert_eq!(w.resume(&ticket), Err(Error::Status));
    assert_eq!(w.inspect(id).unwrap(), f);
    assert_eq!(h.service.state().unwrap(), state);
    w.dispose(&mut h.k, id).unwrap();
    w.dispose(&mut h.k, rejected).unwrap();
    assert_eq!(h.k.lifecycle_state().leases, 0);
}
#[test]
fn old_and_new_paused_wasm_keep_actual_code_and_frozen_graphs() {
    let mut h = Host::new();
    let w = Workflows::new(Limits::default()).unwrap();
    let old_src = workflow_guest(&h, "old", "");
    let root = h.guest("actor", &old_src, None);
    let old = start(&w, &mut h, root, "old");
    w.step(&h.k, old, &[1]).unwrap();
    // New program/code uses current stamps. Both operations have live valid plans.
    h.charge = h.service.state().unwrap().resources[&address("charge")]
        .stamp
        .clone();
    let new_src = workflow_guest(&h, "new", "");
    let ext = h.guest("extension", &new_src, Some("actor"));
    let new = start(&w, &mut h, root, "new");
    w.step(&h.k, new, &[1]).unwrap();
    let old_before = w.inspect(old).unwrap();
    let new_before = w.inspect(new).unwrap();
    assert_ne!(old_before.pipeline.hash, new_before.pipeline.hash);
    assert_eq!(h.k.managed_unload(ext), Err(KError::Referenced));
    assert_eq!(h.k.managed_unload(root), Err(KError::Referenced));
    w.resume(&w.ticket(old).unwrap()).unwrap();
    w.resume(&w.ticket(new).unwrap()).unwrap();
    w.step(&h.k, old, b"cancel").unwrap();
    w.step(&h.k, new, b"cancel").unwrap();
    assert_eq!(w.inspect(old).unwrap().phase, "old_cleanup");
    assert_eq!(w.inspect(new).unwrap().phase, "new_cleanup");
    // Changed executable behavior omits the old time debit and finishes new.
    for _ in 0..3 {
        w.step(&h.k, old, &[]).unwrap();
    }
    for _ in 0..3 {
        w.step(&h.k, new, &[]).unwrap();
    }
    assert_eq!(
        w.inspect(new).unwrap().status,
        Status::Finished(b"new".to_vec())
    );
    assert_eq!(
        w.inspect(old).unwrap().status,
        Status::Finished(b"old".to_vec())
    );
    assert_eq!(h.service.state().unwrap().commits, 3);
    w.dispose(&mut h.k, new).unwrap();
    h.k.managed_detach(ext).unwrap();
    h.k.managed_unload(ext).unwrap();
    assert_eq!(h.k.managed_unload(root), Err(KError::Referenced));
    w.dispose(&mut h.k, old).unwrap();
    h.k.managed_unload(root).unwrap();
    assert_eq!(h.k.lifecycle_state().leases, 0);
}
#[test]
fn commit_then_trap_malformed_or_illegal_is_terminal_and_never_replays() {
    for fault in ["trap", "malformed", "illegal"] {
        let mut h = Host::new();
        let w = Workflows::new(Limits::default()).unwrap();
        let source = workflow_guest(&h, "old", fault);
        let module = h.guest("actor", &source, None);
        let id = start(&w, &mut h, module, "old");
        let before = w.inspect(id).unwrap();
        let lifecycle = h.k.lifecycle_state();
        assert!(matches!(w.step(&h.k, id, &[1]), Ok(Status::Faulted(_))));
        let after = w.inspect(id).unwrap();
        assert_eq!(after.state, before.state);
        assert_eq!(after.phase, before.phase);
        assert_eq!(after.sequence, 0);
        assert_eq!(after.trace, before.trace);
        assert!(!after.cleanup_pending);
        let resources = h.service.state().unwrap();
        assert_eq!(resources.commits, 1);
        assert_eq!(resources.resources[&address("charge")].value, b"1");
        assert_eq!(w.step(&h.k, id, &[1]), Err(Error::Status));
        assert_eq!(w.ticket(id), Err(Error::Status));
        assert_eq!(w.inspect(id).unwrap(), after);
        assert_eq!(h.service.state().unwrap(), resources);
        assert_eq!(h.k.lifecycle_state(), lifecycle);
        w.dispose(&mut h.k, id).unwrap();
    }
}
struct FailCleanup {
    fail: Cell<bool>,
    service: nethacked_resources::Service,
}
impl CleanupParticipant for FailCleanup {
    fn identity(&self) -> nethacked_kernel::Result<Option<nethacked_kernel::CleanupIdentity>> {
        self.service.cleanup_identity().map(Some)
    }
    fn discard(&self, b: &nethacked_kernel::OperationBinding) -> nethacked_kernel::Result<()> {
        if self.fail.get() {
            Err(KError::NativeRefusal)
        } else {
            self.service.discard_operation(b)
        }
    }
}
#[test]
fn abandoned_plan_cleanup_failure_retains_code_and_retry_only_discards() {
    let mut h = Host::new();
    let w = Workflows::new(Limits::default()).unwrap();
    let source = workflow_guest(&h, "old", "abandon");
    let module = h.guest("actor", &source, None);
    let cleanup = Rc::new(FailCleanup {
        fail: Cell::new(true),
        service: h.service.clone(),
    });
    let id = w
        .start(
            &mut h.k,
            program(module, "old"),
            vec![],
            vec![cleanup.clone()],
        )
        .unwrap();
    assert_eq!(w.step(&h.k, id, &[]), Ok(Status::Faulted(Fault::Cleanup)));
    assert!(w.inspect(id).unwrap().cleanup_pending);
    let resources = h.service.state().unwrap();
    assert_eq!(resources.commits, 0);
    assert_eq!(resources.next_serial, 2);
    assert_eq!(w.dispose(&mut h.k, id), Err(Error::Cleanup));
    assert_eq!(h.k.managed_unload(module), Err(KError::Referenced));
    assert_eq!(w.retry_cleanup(id), Err(Error::Cleanup));
    assert_eq!(h.service.state().unwrap(), resources);
    cleanup.fail.set(false);
    w.retry_cleanup(id).unwrap();
    w.retry_cleanup(id).unwrap();
    assert!(!w.inspect(id).unwrap().cleanup_pending);
    assert_eq!(w.step(&h.k, id, &[]), Err(Error::Status));
    let clean = h.service.state().unwrap();
    assert_ne!(clean, resources);
    assert_eq!(clean.resources, resources.resources);
    assert_eq!(clean.history, resources.history);
    w.dispose(&mut h.k, id).unwrap();
    h.k.managed_unload(module).unwrap();
}
#[test]
fn tickets_bounds_replay_and_native_reentrancy_have_valid_controls() {
    let mut h = Host::new();
    let w = Workflows::new(Limits {
        operations: 2,
        input_bytes: 4,
        ..Limits::default()
    })
    .unwrap();
    let observed = Rc::new(RefCell::new(None));
    let observe = observed.clone();
    let reentrant = w.clone();
    let port: NativePort = Rc::new(move |b, view: &CallerView| {
        *observe.borrow_mut() = Some(reentrant.resume(&Ticket {
            operation: view.operation().unwrap().operation(),
            sequence: view.operation().unwrap().checkpoint(),
            pipeline: view.operation().unwrap().pipeline(),
            code: [0; 32],
            phase: view.operation().unwrap().phase().into(),
        }));
        let req: StepRequest = serde_json::from_slice(b).unwrap();
        Ok(serde_json::to_vec(&Outcome::Suspend {
            next: if req.sequence >= 4 {
                "missing"
            } else {
                "reply"
            }
            .into(),
            prompt: vec![],
            state: req.state,
            events: events("pause"),
        })
        .unwrap())
    });
    let module =
        h.k.register_native(
            manifest("native"),
            b"native",
            BTreeMap::from([("step".into(), port)]),
        )
        .unwrap();
    let mut p = program(module, "old");
    p.allowed.insert(("reply".into(), "reply".into()));
    let a = w.start(&mut h.k, p.clone(), vec![], vec![]).unwrap();
    let b = w.start(&mut h.k, p, vec![], vec![]).unwrap();
    let life = h.k.lifecycle_state();
    assert_eq!(
        w.start(&mut h.k, program(module, "old"), vec![], vec![]),
        Err(Error::Bounds)
    );
    assert_eq!(h.k.lifecycle_state(), life);
    let before = w.inspect(a).unwrap();
    let resources = h.service.state().unwrap();
    assert_eq!(w.step(&h.k, a, b"large"), Err(Error::Bounds));
    assert_eq!(w.inspect(a).unwrap(), before);
    assert_eq!(h.service.state().unwrap(), resources);
    assert_eq!(h.k.lifecycle_state(), life);
    w.step(&h.k, a, &[]).unwrap();
    assert_eq!(*observed.borrow(), Some(Err(Error::Busy)));
    w.step(&h.k, b, &[]).unwrap();
    let t = w.ticket(a).unwrap();
    for changed in 0..5 {
        let mut bad = t.clone();
        match changed {
            0 => bad.sequence += 1,
            1 => bad.phase = "select".into(),
            2 => bad.code[0] ^= 1,
            3 => bad.operation = b,
            _ => bad.pipeline = w.ticket(b).unwrap().pipeline,
        }
        let a_before = w.inspect(a).unwrap();
        let b_before = w.inspect(b).unwrap();
        assert_eq!(w.resume(&bad), Err(Error::Ticket));
        assert_eq!(w.inspect(a).unwrap(), a_before);
        assert_eq!(w.inspect(b).unwrap(), b_before);
    }
    let other = Workflows::new(Limits::default()).unwrap();
    let mut other_host = Host::new();
    let source = workflow_guest(&other_host, "old", "");
    let other_module = other_host.guest("actor", &source, None);
    let other_id = start(&other, &mut other_host, other_module, "old");
    other.step(&other_host.k, other_id, &[1]).unwrap();
    let other_ticket = other.ticket(other_id).unwrap();
    let other_before = other.inspect(other_id).unwrap();
    assert_eq!(other.resume(&t), Err(Error::Unknown));
    let mut foreign = t.clone();
    foreign.operation = other_id;
    assert_eq!(other.resume(&foreign), Err(Error::Ticket));
    assert_eq!(other.inspect(other_id).unwrap(), other_before);
    other.resume(&other_ticket).unwrap(); // Valid live control on that host.

    w.resume(&t).unwrap();
    w.step(&h.k, a, &[]).unwrap();
    let later = w.inspect(a).unwrap();
    assert_eq!(w.resume(&t), Err(Error::Ticket));
    assert_eq!(w.inspect(a).unwrap(), later);
    w.resume(&w.ticket(a).unwrap()).unwrap();
    // Native tries select->reply again from reply: illegal graph edge faults.
    w.step(&h.k, a, &[]).unwrap();
    assert_eq!(
        w.inspect(a).unwrap().status,
        Status::Faulted(Fault::InvalidOutcome)
    );
}
