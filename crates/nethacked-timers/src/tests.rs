use super::*;
#[path = "../tests/support.rs"]
mod support;
use support::{context, Host};

#[test]
fn checked_task_counter_last_identity_and_exhaustion_use_real_guest() {
    let mut h = Host::new();
    h.s.0.borrow_mut().next_serial = u64::MAX - 1;
    let id = h.schedule(vec![1], 1, 1);
    assert_eq!(id.serial, u64::MAX - 1);
    let (lease, binding) = h.source(h.actor);
    let before = h.snapshot();
    let req = h.request(vec![2], 1, 1);
    assert_eq!(
        support::abi(&h.invoke(&binding, &req)),
        Error::Limit.abi_code()
    );
    assert_eq!(h.snapshot(), before);
    h.k.release_pipeline(lease).unwrap();
    assert_eq!(
        h.s.deliver_next(&mut h.k, &context(1, 1))
            .unwrap()
            .unwrap()
            .id,
        id
    );
    h.s.dispose(&mut h.k, id).unwrap();
    let (lease, binding) = h.source(h.actor);
    let before = h.snapshot();
    assert_eq!(
        support::abi(&h.invoke(&binding, &req)),
        Error::Limit.abi_code()
    );
    assert_eq!(h.snapshot(), before);
    h.k.release_pipeline(lease).unwrap();
}
#[test]
fn queue_namespace_exhaustion_never_reuses_identity() {
    // Exercise the same checked atomic operation locally; do not perturb parallel services.
    let ids = AtomicU64::new(u64::MAX - 1);
    assert_eq!(issue_namespace(&ids), Ok(u64::MAX - 1));
    assert_eq!(issue_namespace(&ids), Err(Error::Limit));
    assert_eq!(ids.load(Ordering::Relaxed), u64::MAX);
}
#[test]
fn private_queue_borrow_refusals_have_valid_controls() {
    let mut h = Host::new();
    let (lease, binding) = h.source(h.actor);
    let req = h.request(vec![1], 1, 1);
    let before = h.snapshot();
    let borrow = h.s.0.borrow_mut();
    assert_eq!(
        support::abi(&h.invoke(&binding, &req)),
        Error::NativeRefusal.abi_code()
    );
    assert_eq!(
        h.s.deliver_next(&mut h.k, &context(1, 1)),
        Err(Error::NativeRefusal)
    );
    drop(borrow);
    assert_eq!(h.snapshot(), before);
    let id: TaskId = serde_json::from_slice(&h.invoke(&binding, &req)).unwrap();
    h.k.release_pipeline(lease).unwrap();
    assert_eq!(
        h.s.deliver_next(&mut h.k, &context(1, 1))
            .unwrap()
            .unwrap()
            .id,
        id
    );
    h.s.dispose(&mut h.k, id).unwrap();
    assert_eq!(h.k.lifecycle_state().references, 0);
    // A disposed record cannot be delivered by a selector referring to its former ID.
    h.clock.forced.set(Some(id));
    let before = h.snapshot();
    assert_eq!(
        h.s.deliver_next(&mut h.k, &context(1, 1)),
        Err(Error::InvalidHandle)
    );
    assert_eq!(h.snapshot(), before);
}
