use crate as kernel_api;
#[path = "../tests/callbacks_support/mod.rs"]
mod callbacks_support;
use crate::*;
use callbacks_support::*;

#[test]
fn capture_counter_overflow_and_private_borrow_refusal_have_valid_guest_controls() {
    let mut h = Host::new(false);
    let (_, lease, source) = h.source();
    h.k.retention.callbacks.borrow_mut().next = u64::MAX - 1;
    assert_eq!(
        abi(&h
            .k
            .invoke_operation(&source, &request("callback", auth()), auth())
            .unwrap()),
        0
    );
    let callback = h.stored.borrow_mut().pop().unwrap();
    let before = h.k.lifecycle_state();
    assert_eq!(before.next_callback, u64::MAX);
    assert_eq!(
        abi(&h
            .k
            .invoke_operation(&source, &request("callback", auth()), auth())
            .unwrap()),
        Error::Limit.abi_code()
    );
    assert_eq!(h.k.lifecycle_state(), before);
    drop(callback);
    h.k.release_pipeline(lease).unwrap();
    assert_eq!(h.k.lifecycle_state().references, 0);

    let mut h = Host::new(false);
    let (_, lease, source) = h.source();
    let before = h.k.lifecycle_state();
    let guard = h.k.retention.callbacks.borrow_mut();
    assert_eq!(
        abi(&h
            .k
            .invoke_operation(&source, &request("callback", auth()), auth())
            .unwrap()),
        Error::NativeRefusal.abi_code()
    );
    drop(guard);
    assert_eq!(h.k.lifecycle_state(), before);
    assert_eq!(
        abi(&h
            .k
            .invoke_operation(&source, &request("callback", auth()), auth())
            .unwrap()),
        0
    );
    h.stored.borrow_mut().clear();
    h.k.release_pipeline(lease).unwrap();
    assert_eq!(h.k.retention.callbacks.borrow().live.len(), 1);
    assert_eq!(h.k.lifecycle_state().references, 0);
}

#[test]
fn adoption_both_counter_overflows_are_atomic_without_identity_reuse() {
    for operation_overflow in [false, true] {
        let mut h = Host::new(false);
        let (callback, _) = h.capture();
        if operation_overflow {
            h.k.next_operation = u64::MAX - 1;
        } else {
            h.k.retention.next_serial = u64::MAX - 1;
        }
        let (op, lease) = h.k.adopt_callback(&callback).unwrap();
        let before = h.k.lifecycle_state();
        let next_operation = h.k.next_operation;
        assert_eq!(h.k.adopt_callback(&callback), Err(Error::Limit));
        assert_eq!(h.k.lifecycle_state(), before);
        assert_eq!(h.k.next_operation, next_operation);
        let binding = h.k.operation_binding(op, lease, 0, "callback").unwrap();
        assert_eq!(
            abi(&h
                .k
                .invoke_operation(&binding, &5u32.to_le_bytes(), auth())
                .unwrap()),
            6
        );
        drop(callback);
        h.k.release_pipeline(lease).unwrap();
        assert_eq!(h.k.lifecycle_state().references, 0);
    }
}
