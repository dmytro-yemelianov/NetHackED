#[path = "../../nethacked-kernel/tests/callbacks_support/mod.rs"]
mod callbacks_support;
use callbacks_support::*;
use nethacked_kernel as kernel_api;
use nethacked_kernel::*;
use nethacked_resources::{OwnerPolicy, Plan, Service, Write};
use std::rc::Rc;

fn command(prefix: u8, bytes: &[u8]) -> Vec<u8> {
    let mut request = vec![prefix];
    request.extend_from_slice(bytes);
    request
}
fn transaction_guest() -> Vec<u8> {
    wat::parse_str(
        r#"(module
      (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "alloc") (param i32) (result i32) i32.const 0)
      (func $forward (param $s i32) (param $p i32) (param $n i32) (result i64) (local $r i32)
        local.get $s local.get $p local.get $n i32.const 4096 i32.const 4096 call $call local.set $r
        local.get $r i32.const 0 i32.lt_s if
          i32.const 4096 local.get $r i32.store i64.const 17592186044420 return end
        i64.const 17592186044416 local.get $r i64.extend_i32_u i64.or)
      (func (export "run") (param i32 i32) (result i64)
        i32.const 0 local.get 0 local.get 1 call $forward)
      (func (export "callback") (param $p i32) (param $n i32) (result i64)
        local.get $p i32.load8_u i32.const 80 i32.eq
        if (result i32) i32.const 1 else i32.const 2 end
        local.get $p i32.const 1 i32.add local.get $n i32.const 1 i32.sub call $forward))"#,
    )
    .unwrap()
}
#[test]
fn adopted_guest_resource_prepare_commit_live_permissions_and_exact_context() {
    let mut h = Host::new(false);
    h.k.managed_unload(h.actor).unwrap();
    let service = Service::new(
        nethacked_resources::Limits {
            wire: 4096,
            ..nethacked_resources::Limits::default()
        },
        Rc::new(|_| true),
    )
    .unwrap();
    service.install(&mut h.k, "resources").unwrap();
    service
        .register_owner(
            "device",
            OwnerPolicy {
                capability: cap("observer", "run"),
                provider: h.observer,
                artifact: h.k.artifact(h.observer).unwrap(),
                validate: Rc::new(|_| true),
            },
        )
        .unwrap();
    let stamp = service.put(key("value"), b"10".to_vec()).unwrap();
    service
        .put(key("unrelated"), b"untouched".to_vec())
        .unwrap();
    let mut m = manifest("actor", &["run", "callback"]);
    m.dependencies = vec![
        dep("schedule", "run"),
        dep("resources", "prepare"),
        dep("resources", "commit"),
        dep("observer", "run"),
    ];
    h.actor = h.k.register_guest(m, &transaction_guest()).unwrap();
    for slot in 0..4 {
        h.k.issue_grant(h.actor, slot, auth()).unwrap();
    }
    let before = service.state().unwrap();
    let (callback, source) = h.capture();
    let (op, lease) = h.k.adopt_callback(&callback).unwrap();
    assert_ne!(op, source.operation());
    drop(callback);
    let participant = [service.cleanup_identity().unwrap()];
    let binding =
        h.k.operation_binding_with_cleanup(op, lease, 3, "transaction", &participant)
            .unwrap();
    let plan = Plan {
        readonly: vec![],
        writes: vec![Write {
            stamp,
            replacement: Some(b"11".to_vec()),
        }],
        events: vec![b"changed".to_vec()],
    };
    let prepare = command(b'P', &serde_json::to_vec(&plan).unwrap());
    let unenrolled = h.k.operation_binding(op, lease, 3, "transaction").unwrap();
    assert_eq!(
        abi(&h.k.invoke_operation(&unenrolled, &prepare, auth()).unwrap()),
        Error::Authority.abi_code()
    );
    assert_eq!(service.state().unwrap(), before);
    h.k.revoke_grant(h.actor, 3).unwrap();
    assert_eq!(
        abi(&h.k.invoke_operation(&binding, &prepare, auth()).unwrap()),
        Error::Authority.abi_code()
    );
    assert_eq!(service.state().unwrap(), before);
    h.k.issue_grant(h.actor, 3, auth()).unwrap();
    let id = h.k.invoke_operation(&binding, &prepare, auth()).unwrap();
    let identity: nethacked_resources::Identity = serde_json::from_slice(&id).unwrap();
    assert_eq!(identity.serial, before.next_serial);
    let pending = service.state().unwrap();
    assert_eq!(pending.resources, before.resources);
    assert_eq!(pending.commits, 0);
    assert_eq!(pending.next_serial, before.next_serial + 1);
    let commit = command(b'C', &id);
    for wrong in [
        h.k.operation_binding_with_cleanup(
            source.operation(),
            lease,
            3,
            "transaction",
            &participant,
        )
        .unwrap(),
        h.k.operation_binding_with_cleanup(op, lease, 4, "transaction", &participant)
            .unwrap(),
        h.k.operation_binding_with_cleanup(op, lease, 3, "wrong", &participant)
            .unwrap(),
    ] {
        assert_eq!(
            abi(&h.k.invoke_operation(&wrong, &commit, auth()).unwrap()),
            Error::Authority.abi_code()
        );
        assert_eq!(service.state().unwrap(), pending);
    }
    h.k.revoke_grant(h.actor, 0).unwrap();
    let metadata = h.k.lifecycle_state();
    assert_eq!(
        h.k.invoke_operation(&binding, &commit, auth()),
        Err(Error::Revoked)
    );
    assert_eq!(h.k.lifecycle_state(), metadata);
    assert_eq!(service.state().unwrap(), pending);
    h.k.issue_grant(h.actor, 0, auth()).unwrap();
    assert_eq!(h.k.invoke_operation(&binding, &commit, auth()).unwrap(), id);
    let after = service.state().unwrap();
    assert_eq!(after.commits, 1);
    assert_eq!(after.resources[&key("value")].value, b"11");
    assert_eq!(
        after.resources[&key("unrelated")],
        before.resources[&key("unrelated")]
    );
    assert_eq!(after.history, vec![b"changed".to_vec()]);
    let mut expected = before.clone();
    expected.next_serial += 1;
    expected.commits += 1;
    expected.resources.get_mut(&key("value")).unwrap().value = b"11".to_vec();
    expected
        .resources
        .get_mut(&key("value"))
        .unwrap()
        .stamp
        .revision += 1;
    expected.history.push(b"changed".to_vec());
    assert_eq!(after, expected);
    h.k.release_pipeline(lease).unwrap();
    assert_eq!(h.k.lifecycle_state().references, 0);
}
