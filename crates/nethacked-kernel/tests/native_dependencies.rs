mod common;
use common::*;
use nethacked_kernel::*;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

fn forward_native(
    k: &mut Kernel,
    id: &str,
    provider: &str,
    endpoint: &str,
    access: Access,
) -> ModuleHandle {
    let mut m = manifest(id, access);
    m.dependencies
        .push(dependency(provider, endpoint, Access::Write));
    let port: NativePort = Rc::new(|request, view| view.call_dependency(0, request));
    k.register_native(m, id.as_bytes(), BTreeMap::from([("run".into(), port)]))
        .unwrap()
}

#[test]
fn native_dependencies_authenticate_actual_caller_and_live_scope() {
    let mut k = Kernel::new(Limits::default()).unwrap();
    let (_, state) = device(&mut k);
    let middle = forward_native(&mut k, "middle", "device", "set", Access::Write);
    grant(&mut k, middle, 0, Access::Write, &["scalar", "other"]);
    k.invoke(
        middle,
        "run",
        &bytes(12),
        authority(Access::Write, &["scalar"]),
    )
    .unwrap();
    assert_eq!(state.borrow().value, 12);
    assert_eq!(state.borrow().callers, vec![Some(middle)]);
    assert_eq!(
        state.borrow().artifacts,
        vec![Some(k.artifact(middle).unwrap())]
    );
    let before = state.borrow().clone();
    assert_eq!(
        k.invoke(
            middle,
            "run",
            &bytes(13),
            authority(Access::Write, &["other"])
        ),
        Err(Error::Authority)
    );
    k.revoke_grant(middle, 0).unwrap();
    assert_eq!(
        k.invoke(middle, "run", &bytes(14), trusted(Access::Write)),
        Err(Error::Revoked)
    );
    assert_eq!(*state.borrow(), before);
    let read = forward_native(&mut k, "read", "device", "set", Access::Read);
    grant(&mut k, read, 0, Access::Write, &["scalar"]);
    assert_eq!(
        k.invoke(read, "run", &bytes(15), trusted(Access::Write)),
        Err(Error::Authority)
    );
    assert_eq!(*state.borrow(), before);
}

#[test]
fn native_dependencies_share_call_and_depth_budgets() {
    for limits in [
        Limits {
            calls: 2,
            ..Limits::default()
        },
        Limits {
            depth: 1,
            ..Limits::default()
        },
    ] {
        let mut k = Kernel::new(limits).unwrap();
        let (_, state) = device(&mut k);
        let middle = forward_native(&mut k, "middle", "device", "set", Access::Write);
        let outer = forward_native(&mut k, "outer", "middle", "run", Access::Write);
        grant(&mut k, middle, 0, Access::Write, &["scalar"]);
        grant(&mut k, outer, 0, Access::Write, &["scalar"]);
        let before = state.borrow().clone();
        assert_eq!(
            k.invoke(outer, "run", &bytes(99), trusted(Access::Write)),
            Err(Error::Limit)
        );
        assert_eq!(*state.borrow(), before);
    }
}

#[test]
fn detached_views_cannot_dispatch_and_unknown_slots_refuse() {
    let mut k = Kernel::new(Limits::default()).unwrap();
    let (_, state) = device(&mut k);
    let saved = Rc::new(RefCell::new(None::<CallerView<'static>>));
    let saved_view = saved.clone();
    let port: NativePort = Rc::new(move |_, view| {
        saved_view.replace(Some(view.clone()));
        assert_eq!(view.call_dependency(1, &bytes(9)), Err(Error::InvalidSlot));
        view.call_dependency(0, &bytes(8))
    });
    let mut m = manifest("middle", Access::Write);
    m.dependencies
        .push(dependency("device", "set", Access::Write));
    let middle = k
        .register_native(m, b"middle", BTreeMap::from([("run".into(), port)]))
        .unwrap();
    grant(&mut k, middle, 0, Access::Write, &["scalar"]);
    k.invoke(middle, "run", &[], trusted(Access::Write))
        .unwrap();
    let before = state.borrow().clone();
    assert_eq!(
        saved
            .borrow()
            .as_ref()
            .unwrap()
            .call_dependency(0, &bytes(9)),
        Err(Error::Authority)
    );
    assert_eq!(*state.borrow(), before);
}

#[test]
fn native_dependency_keeps_captured_code_after_future_extension() {
    let mut k = Kernel::new(Limits::default()).unwrap();
    let mut source = manifest("source", Access::Read);
    source.extension_points.insert("run".into());
    k.register_native(source, b"source-v1", ports(1)).unwrap();
    let mut m = manifest("middle", Access::Read);
    m.dependencies
        .push(dependency("source", "run", Access::Read));
    let port: NativePort = Rc::new(|request, view| view.call_dependency(0, request));
    let middle = k
        .register_native(m, b"middle", BTreeMap::from([("run".into(), port)]))
        .unwrap();
    grant(&mut k, middle, 0, Access::Read, &["scalar"]);
    let old = k
        .capture_pipeline(middle, "run", trusted(Access::Read))
        .unwrap();
    let mut extension = manifest("extension", Access::Read);
    extension.extension = Some(Extension {
        target: dependency("source", "run", Access::Read).capability,
        export: "run".into(),
    });
    k.register_native(extension, b"source-v2", ports(2))
        .unwrap();
    assert_eq!(
        number(&k.invoke_pipeline(old, &[], trusted(Access::Read)).unwrap()),
        1
    );
    assert_eq!(
        number(&k.invoke(middle, "run", &[], trusted(Access::Read)).unwrap()),
        2
    );
    k.revoke_grant(middle, 0).unwrap();
    assert_eq!(
        k.invoke_pipeline(old, &[], trusted(Access::Read)),
        Err(Error::Revoked)
    );
    k.release_pipeline(old).unwrap();
}

#[test]
fn approved_manifest_lookup_is_metadata_and_tracks_instance_and_generation() {
    let mut k = Kernel::new(Limits::default()).unwrap();
    let m = manifest("owner", Access::Read);
    let port: NativePort = Rc::new(|_, view| {
        assert_eq!(view.caller(), None);
        assert_eq!(view.call_dependency(0, &[]), Err(Error::InvalidSlot));
        Err(Error::Authority)
    });
    let owner = k
        .register_native(
            m.clone(),
            b"owner",
            BTreeMap::from([("run".into(), port.clone())]),
        )
        .unwrap();
    let before = k.lifecycle_state();
    assert_eq!(k.registered_module(&m), Ok(owner));
    assert_eq!(k.lifecycle_state(), before);
    let mut changed = m.clone();
    changed.exports.get_mut("run").unwrap().max_reply += 1;
    assert_eq!(k.registered_module(&changed), Err(Error::Authority));
    assert_eq!(
        Kernel::new(Limits::default())
            .unwrap()
            .registered_module(&m),
        Err(Error::MissingDependency)
    );
    assert_eq!(
        k.invoke(owner, "run", &[], trusted(Access::Read)),
        Err(Error::Authority)
    );
    k.unregister(owner).unwrap();
    assert_eq!(k.registered_module(&m), Err(Error::MissingDependency));
    let replacement = k
        .register_native(
            m.clone(),
            b"replacement",
            BTreeMap::from([("run".into(), port)]),
        )
        .unwrap();
    assert_ne!(owner, replacement);
    assert_eq!(k.registered_module(&m), Ok(replacement));
    assert_eq!(
        k.invoke(owner, "run", &[], trusted(Access::Read)),
        Err(Error::InvalidHandle)
    );
}

#[test]
fn native_read_dependency_preserves_operation_permits_and_retained_scope() {
    let mut k = Kernel::new(Limits::default()).unwrap();
    let views = Rc::new(RefCell::new(Vec::<CallerView<'static>>::new()));
    let captured = views.clone();
    let cap = dependency("owner", "run", Access::Read).capability;
    let expected_cap = cap.clone();
    let port: NativePort = Rc::new(move |_, view| {
        if !view.authority().scope.allows(&resource("scalar")) {
            return Err(Error::Authority);
        }
        let permit = view
            .permit(&expected_cap, &resource("scalar"), Access::Read)
            .ok_or(Error::Authority)?;
        assert_eq!(permit.resolution.last().unwrap().handle, view.provider());
        assert_eq!(
            permit.resolution.last().unwrap().artifact,
            view.provider_artifact()
        );
        assert_eq!(view.authority().access, Access::Read);
        assert!(view
            .permit(&expected_cap, &resource("scalar"), Access::Write)
            .is_none());
        captured.borrow_mut().push(view.clone());
        Ok(bytes(7))
    });
    let owner = k
        .register_native(
            manifest("owner", Access::Read),
            b"owner",
            BTreeMap::from([("run".into(), port)]),
        )
        .unwrap();
    let middle = forward_native(&mut k, "middle", "owner", "run", Access::Write);
    grant(&mut k, middle, 0, Access::Read, &["scalar"]);
    let (id, lease) = k
        .capture_operation(middle, "run", trusted(Access::Write))
        .unwrap();
    let operation = k.operation_binding(id, lease, 3, "observe").unwrap();
    assert_eq!(
        k.invoke_operation(&operation, &[], trusted(Access::Write))
            .unwrap(),
        bytes(7)
    );
    let view = views.borrow()[0].clone();
    assert_eq!(view.caller(), Some(middle));
    assert_eq!(view.caller_artifact(), Some(k.artifact(middle).unwrap()));
    assert_eq!(view.provider(), owner);
    assert_eq!(view.operation(), Some(&operation));
    assert_eq!(view.call_dependency(0, &[]), Err(Error::Authority));
    assert_eq!(k.managed_unload(owner), Err(Error::Referenced));
    // Regranting may broaden future calls, never the retained observation chain.
    grant(&mut k, middle, 0, Access::Read, &["scalar", "other"]);
    assert_eq!(
        k.invoke_operation(&operation, &[], authority(Access::Write, &["other"])),
        Err(Error::Authority)
    );
    assert_eq!(views.borrow().len(), 1);
    k.invoke(middle, "run", &[], trusted(Access::Write))
        .unwrap();
    let fresh = views.borrow()[1].clone();
    assert_eq!(fresh.operation(), None);
    assert!(fresh
        .permit(&cap, &resource("other"), Access::Read)
        .is_some());
    assert!(view
        .permit(&cap, &resource("other"), Access::Read)
        .is_none());
    k.revoke_grant(middle, 0).unwrap();
    assert_eq!(
        k.invoke_operation(&operation, &[], trusted(Access::Write)),
        Err(Error::Revoked)
    );
    assert_eq!(views.borrow().len(), 2);
    k.release_pipeline(lease).unwrap();
}

#[test]
fn native_dependency_uses_shared_fuel_and_refuses_pure_native_entry() {
    let mut k = Kernel::new(Limits {
        fuel: 1,
        ..Limits::default()
    })
    .unwrap();
    let (_, state) = device(&mut k);
    let middle = forward_native(&mut k, "middle", "device", "set", Access::Write);
    grant(&mut k, middle, 0, Access::Write, &["scalar"]);
    let before = state.borrow().clone();
    assert_eq!(
        k.invoke(middle, "run", &bytes(99), trusted(Access::Write)),
        Err(Error::Limit)
    );
    assert_eq!(*state.borrow(), before);

    let mut k = Kernel::new(Limits::default()).unwrap();
    let (_, state) = device(&mut k);
    let middle = forward_native(&mut k, "middle", "device", "read", Access::Read);
    grant(&mut k, middle, 0, Access::Read, &["scalar"]);
    let mut m = manifest("pure", Access::Read);
    m.dependencies
        .push(dependency("middle", "run", Access::Read));
    let pure: NativePort =
        Rc::new(|request, view| view.call_pure(0, request).map(|reply| reply.bytes));
    let pure = k
        .register_native(m, b"pure", BTreeMap::from([("run".into(), pure)]))
        .unwrap();
    grant(&mut k, pure, 0, Access::Read, &["scalar"]);
    let before = state.borrow().clone();
    let lifecycle = k.lifecycle_state();
    assert_eq!(
        k.invoke(pure, "run", &[], trusted(Access::Write)),
        Err(Error::Authority)
    );
    assert_eq!(*state.borrow(), before);
    assert_eq!(k.lifecycle_state(), lifecycle);
    assert_eq!(
        k.invoke(middle, "run", &[], trusted(Access::Read)).unwrap(),
        bytes(before.value)
    );
}

#[test]
fn native_sibling_calls_share_one_budget_and_enforce_the_target_buffer() {
    let mut k = Kernel::new(Limits {
        calls: 2,
        ..Limits::default()
    })
    .unwrap();
    let (_, state) = device(&mut k);
    let mut m = manifest("reader", Access::Read);
    m.exports.get_mut("run").unwrap().max_request = 8;
    m.dependencies
        .push(dependency("device", "read", Access::Read));
    let port: NativePort = Rc::new(|request, view| {
        view.call_dependency(0, request)?;
        view.call_dependency(0, request)
    });
    let reader = k
        .register_native(m, b"reader", BTreeMap::from([("run".into(), port)]))
        .unwrap();
    grant(&mut k, reader, 0, Access::Read, &["scalar"]);
    let before = state.borrow().clone();
    assert_eq!(
        k.invoke(reader, "run", &[], trusted(Access::Read)),
        Err(Error::Limit)
    );
    assert_eq!(
        k.invoke(reader, "run", &[0; 5], trusted(Access::Read)),
        Err(Error::Buffer)
    );
    assert_eq!(*state.borrow(), before);
}
