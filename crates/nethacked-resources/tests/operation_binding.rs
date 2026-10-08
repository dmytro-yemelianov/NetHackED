#[path = "../../nethacked-workflows/tests/support/mod.rs"]
mod support;
use nethacked_kernel::*;
use support::*;
#[test]
fn live_plan_requires_exact_authenticated_execution() {
    let mut h = Host::new();
    let module = h.guest("actor", &transaction_guest(), None);
    let (a, lease) = h.k.capture_operation(module, "step", authority()).unwrap();
    let b = h.k.issue_operation().unwrap();
    let keys = [h.service.cleanup_identity().unwrap()];
    let bound =
        h.k.operation_binding_with_cleanup(a, lease, 0, "charge", &keys)
            .unwrap();
    let prepare = command(
        b'P',
        &serde_json::to_vec(&plan(h.charge.clone(), b"1")).unwrap(),
    );
    let unenrolled = h.k.operation_binding(a, lease, 0, "charge").unwrap();
    let empty_before = h.service.state().unwrap();
    assert_eq!(
        abi(h
            .k
            .invoke_operation(&unenrolled, &prepare, authority())
            .unwrap()),
        Err(Error::Authority.abi_code())
    );
    assert_eq!(h.service.state().unwrap(), empty_before);
    let id = abi(h.k.invoke_operation(&bound, &prepare, authority()).unwrap()).unwrap();
    let commit = command(b'C', &id);
    let before = h.service.state().unwrap();
    for binding in [
        h.k.operation_binding_with_cleanup(b, lease, 0, "charge", &keys)
            .unwrap(),
        h.k.operation_binding_with_cleanup(a, lease, 1, "charge", &keys)
            .unwrap(),
        h.k.operation_binding_with_cleanup(a, lease, 0, "other", &keys)
            .unwrap(),
    ] {
        assert_eq!(
            abi(h
                .k
                .invoke_operation(&binding, &commit, authority())
                .unwrap()),
            Err(Error::Authority.abi_code())
        );
        assert_eq!(h.service.state().unwrap(), before);
    }
    let other_lease = h.k.capture_pipeline(module, "step", authority()).unwrap();
    let other =
        h.k.operation_binding_with_cleanup(a, other_lease, 0, "charge", &keys)
            .unwrap();
    assert_eq!(
        abi(h.k.invoke_operation(&other, &commit, authority()).unwrap()),
        Err(Error::Authority.abi_code())
    );
    assert_eq!(
        abi(h.k.invoke_pipeline(lease, &commit, authority()).unwrap()),
        Err(Error::Authority.abi_code())
    );
    let mut forged: serde_json::Value = serde_json::from_slice(&id).unwrap();
    forged["operation"] =
        serde_json::json!({"host": a.namespace(), "serial": a.serial(), "checkpoint": 0});
    assert_eq!(
        abi(h
            .k
            .invoke_operation(
                &other,
                &command(b'C', &serde_json::to_vec(&forged).unwrap()),
                authority()
            )
            .unwrap()),
        Err(Error::NativeRefusal.abi_code())
    );
    assert_eq!(h.service.state().unwrap(), before);
    let other_host = Kernel::new(Limits::default()).unwrap();
    assert_eq!(
        other_host.invoke_operation(&bound, &commit, authority()),
        Err(Error::Authority)
    );
    assert_eq!(h.service.state().unwrap(), before);
    // The original live, same-phase operation still commits after all attacks.
    abi(h.k.invoke_operation(&bound, &commit, authority()).unwrap()).unwrap();
    assert_eq!(h.service.state().unwrap().commits, 1);
    assert_eq!(
        h.service.state().unwrap().resources[&address("charge")].value,
        b"1"
    );
    assert_eq!(
        abi(h.k.invoke_operation(&bound, &commit, authority()).unwrap()),
        Err(Error::NativeRefusal.abi_code())
    );
    // Legacy unbound prepare/commit remains usable and cannot be adopted bound.
    let unbound_plan = command(
        b'P',
        &serde_json::to_vec(&plan(h.time.clone(), b"1")).unwrap(),
    );
    let legacy_id = abi(h
        .k
        .invoke_pipeline(lease, &unbound_plan, authority())
        .unwrap())
    .unwrap();
    let legacy_commit = command(b'C', &legacy_id);
    assert_eq!(
        abi(h
            .k
            .invoke_operation(&bound, &legacy_commit, authority())
            .unwrap()),
        Err(Error::Authority.abi_code())
    );
    abi(h
        .k
        .invoke_pipeline(lease, &legacy_commit, authority())
        .unwrap())
    .unwrap();
    h.k.release_pipeline(other_lease).unwrap();
    h.k.release_pipeline(lease).unwrap();
}
