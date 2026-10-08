mod common;
use common::*;
use nethacked_kernel::*;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

#[test]
fn inspection_uses_invoker_live_grants_and_read_ceiling_without_dispatch() {
    let mut k = Kernel::new(Limits::default()).unwrap();
    let (owner, state) = device(&mut k);
    let captured = Rc::new(RefCell::new(Vec::<CallerView>::new()));
    let storage = captured.clone();
    let callback: NativePort = Rc::new(move |_, view| {
        storage.borrow_mut().push(view.clone());
        Ok(bytes(1))
    });
    let mut coordinator = manifest("coordinator", Access::Write);
    coordinator
        .exports
        .insert("peek".into(), endpoint(Access::Read));
    coordinator
        .dependencies
        .push(dependency("device", "set", Access::Write));
    let coordinator = k
        .register_native(
            coordinator,
            b"coordinator",
            BTreeMap::from([("run".into(), callback.clone()), ("peek".into(), callback)]),
        )
        .unwrap();
    grant(&mut k, coordinator, 0, Access::Write, &["scalar", "other"]);
    let mut m = manifest("caller", Access::Write);
    m.dependencies = vec![
        dependency("coordinator", "run", Access::Write),
        dependency("device", "set", Access::Write),
    ];
    let caller = k.register_guest(m, &forward(0)).unwrap();
    grant(&mut k, caller, 0, Access::Write, &["scalar", "other"]);
    let capability = dependency("device", "set", Access::Write).capability;
    let before = state.borrow().clone();
    k.invoke(caller, "run", &[], trusted(Access::Write))
        .unwrap();
    assert!(captured
        .borrow()
        .last()
        .unwrap()
        .permit(&capability, &resource("scalar"), Access::Read)
        .is_none());
    grant(&mut k, caller, 1, Access::Write, &["scalar", "other"]);
    k.invoke(caller, "run", &[], authority(Access::Write, &["scalar"]))
        .unwrap();
    let snapshot = captured.borrow().last().unwrap().clone();
    let permit = snapshot
        .permit(&capability, &resource("scalar"), Access::Write)
        .unwrap();
    assert_eq!(permit.resolution[0].handle, owner);
    assert_eq!(permit.resolution[0].artifact, k.artifact(owner).unwrap());
    assert!(snapshot
        .permit(&capability, &resource("other"), Access::Write)
        .is_none());
    k.revoke_grant(caller, 1).unwrap();
    k.invoke(caller, "run", &[], trusted(Access::Write))
        .unwrap();
    assert!(captured
        .borrow()
        .last()
        .unwrap()
        .permit(&capability, &resource("scalar"), Access::Read)
        .is_none());
    // Immutable snapshots carry no authority into future calls.
    assert_eq!(
        snapshot.permit(&capability, &resource("scalar"), Access::Write),
        Some(permit)
    );
    let mut read = manifest("read-caller", Access::Read);
    read.dependencies = vec![
        dependency("coordinator", "peek", Access::Read),
        dependency("device", "set", Access::Write),
    ];
    let read = k.register_guest(read, &forward(0)).unwrap();
    grant(&mut k, read, 0, Access::Read, &["scalar"]);
    grant(&mut k, read, 1, Access::Write, &["scalar"]);
    k.invoke(read, "run", &[], trusted(Access::Write)).unwrap();
    let view = captured.borrow().last().unwrap().clone();
    assert!(view
        .permit(&capability, &resource("scalar"), Access::Read)
        .is_some());
    assert!(view
        .permit(&capability, &resource("scalar"), Access::Write)
        .is_none());
    k.invoke(coordinator, "run", &[], trusted(Access::Write))
        .unwrap();
    assert!(captured
        .borrow()
        .last()
        .unwrap()
        .permit(&capability, &resource("scalar"), Access::Write)
        .is_none());
    assert_eq!(*state.borrow(), before);
}
