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

use std::rc::Rc;

#[test]
fn pins_and_callbacks_survive_complete_capture_then_release_on_success_and_unwind() {
    for unwind in [false, true] {
        let mut h = Host::new(false);
        let (callback, source) = h.capture();
        let pin = h.k.pin_modules(&[h.actor]).unwrap();
        let domain = Rc::new(session::Domain::new());
        h.k.enroll_session(&domain.enrollment(1)).unwrap();
        let before = h.k.lifecycle_state();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = domain.capture().unwrap();
            drop(callback);
            drop(pin);
            assert_eq!(h.k.lifecycle_state(), before);
            assert_eq!(h.k.invoke_operation(&source, &[], auth()), Err(Error::Busy));
            assert_eq!(h.k.pin_modules(&[h.actor]).err(), Some(Error::Busy));
            if unwind {
                panic!("encoder unwinds after dropping infrastructure roots");
            }
        }));
        assert_eq!(result.is_err(), unwind);
        let after = h.k.lifecycle_state();
        assert_eq!(before.callbacks, after.callbacks + 1);
        assert_eq!(before.owners, after.owners + 1);
        assert!(after.references < before.references);
        assert!(domain.runtime().is_ok());
    }
}

#[test]
fn aggregate_pin_reservations_preserve_history_on_every_refusal() {
    for case in 0..5 {
        let mut h = Host::new(false);
        h.source();
        let module = h.actor;
        match case {
            0 => h.k.retention.limits.owners = h.k.lifecycle_state().owners + 1,
            1 => h.k.retention.limits.references = h.k.lifecycle_state().references + 1,
            2 => h.k.retention.pins.borrow_mut().next = u64::MAX - 1,
            3 => h.k.retention.pins.borrow_mut().next = u64::MAX,
            _ => (),
        }
        let before = (h.k.lifecycle_state(), h.k.retention.history().1);
        let first = [module];
        let second = if case == 4 { vec![] } else { vec![module] };
        assert_eq!(
            h.k.pin_module_groups(&[&first, &second]).err(),
            Some(Error::Limit)
        );
        assert_eq!((h.k.lifecycle_state(), h.k.retention.history().1), before);
        // Single admission still works when only the aggregate reservation refuses.
        if case < 3 {
            assert!(h.k.pin_modules(&first).is_ok());
        }
    }
}
