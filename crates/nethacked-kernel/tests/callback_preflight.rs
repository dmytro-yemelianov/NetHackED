mod callbacks_support;
use callbacks_support::*;
use nethacked_kernel as kernel_api;
use nethacked_kernel::*;
use std::collections::BTreeSet;

#[test]
fn readonly_callback_preflight_matches_adopted_pipeline_and_context_bounds() {
    let mut h = Host::new(false);
    let (callback, _) = h.capture();
    let request = 5u32.to_le_bytes();
    let baseline = h.k.lifecycle_state();
    let values = h.state.borrow().clone();
    for (bytes, authority, phase, cleanup, expected) in [
        (request.to_vec(), auth(), "timer".to_owned(), vec![], Ok(())),
        (
            vec![0; 4097],
            auth(),
            "timer".to_owned(),
            vec![],
            Err(Error::Buffer),
        ),
        (
            request.to_vec(),
            Authority {
                access: Access::Read,
                scope: Scope::Keys(BTreeSet::from([key("value")])),
            },
            "timer".to_owned(),
            vec![],
            Err(Error::Authority),
        ),
        (
            request.to_vec(),
            auth(),
            "x".repeat(Limits::default().payload_bytes as usize + 1),
            vec![],
            Err(Error::Buffer),
        ),
        (
            request.to_vec(),
            auth(),
            "timer".to_owned(),
            vec![CleanupIdentity::issue().unwrap(); h.k.operation_cleanup_limit() + 1],
            Err(Error::Limit),
        ),
    ] {
        assert_eq!(
            h.k.preflight_callback(&callback, &bytes, &authority, &phase, &cleanup),
            expected
        );
        assert_eq!(h.k.lifecycle_state(), baseline);
        assert_eq!(*h.state.borrow(), values);
    }
    h.k.revoke_grant(h.actor, 0).unwrap();
    let baseline = h.k.lifecycle_state();
    assert_eq!(
        h.k.preflight_callback(&callback, &request, &auth(), "timer", &[]),
        Err(Error::Revoked)
    );
    assert_eq!(h.k.lifecycle_state(), baseline);
    h.k.issue_grant(h.actor, 0, auth()).unwrap();
    let foreign = Kernel::new(Limits::default()).unwrap();
    let before = foreign.lifecycle_state();
    assert_eq!(
        foreign.preflight_callback(&callback, &request, &auth(), "timer", &[]),
        Err(Error::InvalidHandle)
    );
    assert_eq!(foreign.lifecycle_state(), before);
    let (op, lease) = h.k.adopt_callback(&callback).unwrap();
    for bytes in [request.to_vec(), vec![0; 4097]] {
        assert_eq!(
            h.k.preflight_callback(&callback, &bytes, &auth(), "timer", &[]),
            h.k.preflight_pipeline(lease, &bytes, &auth())
        );
    }
    let binding = h.k.operation_binding(op, lease, 0, "timer").unwrap();
    assert_eq!(
        abi(&h.k.invoke_operation(&binding, &request, auth()).unwrap()),
        6
    );
    drop(callback);
    h.k.release_pipeline(lease).unwrap();
    assert_eq!(h.k.lifecycle_state().references, 0);
}
