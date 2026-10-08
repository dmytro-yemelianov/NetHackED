mod callbacks_support;
use callbacks_support::*;
use nethacked_kernel as kernel_api;
use nethacked_kernel::*;
use std::collections::BTreeSet;

fn invoke(k: &Kernel, op: OperationId, lease: PipelineLease) -> Result<Vec<u8>> {
    let binding = k.operation_binding(op, lease, 0, "callback")?;
    k.invoke_operation(&binding, &5u32.to_le_bytes(), auth())
}
#[test]
fn real_guest_capture_retired_source_fresh_identity_and_shared_ownership() {
    let mut h = Host::new(false);
    let before = h.state.borrow().clone();
    let (callback, source) = h.capture();
    let provenance = callback.provenance().clone();
    assert_eq!(provenance.source, source);
    assert_eq!(provenance.caller.handle, h.actor);
    assert_eq!(provenance.caller.artifact, h.k.artifact(h.actor).unwrap());
    assert_eq!(provenance.service.handle, h.schedule);
    assert_eq!(
        provenance.service.artifact,
        h.k.artifact(h.schedule).unwrap()
    );
    assert_eq!(provenance.capability, cap("schedule", "run"));
    assert_eq!(provenance.slot, 0);
    assert_eq!(*h.state.borrow(), before);
    assert_eq!(h.k.lifecycle_state().leases, 0);
    assert_eq!(h.k.lifecycle_state().callbacks, 1);
    let shared = callback.clone();
    let snapshot = h.k.lifecycle_state();
    drop(callback);
    assert_eq!(h.k.lifecycle_state(), snapshot);
    let (a, first) = h.k.adopt_callback(&shared).unwrap();
    let (b, second) = h.k.adopt_callback(&shared).unwrap();
    assert_ne!(a, b);
    assert_ne!(a, source.operation());
    assert_ne!(first, second);
    drop(shared);
    assert_eq!(h.k.lifecycle_state().callbacks, 0);
    assert_eq!(h.k.callback_provenance(first).unwrap(), Some(&provenance));
    assert_eq!(h.k.callback_provenance(second).unwrap(), Some(&provenance));
    assert_eq!(abi(&invoke(&h.k, a, first).unwrap()), 6);
    h.k.release_pipeline(first).unwrap();
    assert_eq!(abi(&invoke(&h.k, b, second).unwrap()), 6);
    for record in h.observations.borrow().iter() {
        assert_eq!(record.caller, h.actor);
        assert_eq!(record.artifact, provenance.caller.artifact);
        assert_eq!(record.binding.as_ref().unwrap().phase(), "callback");
        assert_eq!(record.binding.as_ref().unwrap().checkpoint(), 0);
    }
    assert_eq!(
        h.observations.borrow()[0]
            .binding
            .as_ref()
            .unwrap()
            .operation(),
        a
    );
    assert_eq!(
        h.observations.borrow()[1]
            .binding
            .as_ref()
            .unwrap()
            .operation(),
        b
    );
    assert_eq!(
        h.state.borrow()[&key("unrelated")],
        before[&key("unrelated")]
    );
    h.k.release_pipeline(second).unwrap();
    assert_eq!(h.k.lifecycle_state().references, 0);
    h.k.managed_unload(h.actor).unwrap();
    h.k.managed_unload(h.schedule).unwrap();
    h.k.managed_unload(h.observer).unwrap();
}
#[test]
fn changed_dispatch_old_code_all_removal_paths_and_transitive_references() {
    let mut h = Host::new(false);
    let old_extension = h.extension("old", 20);
    let (old, _) = h.capture();
    h.k.managed_detach(old_extension).unwrap();
    let new_extension = h.extension("new", 40);
    let (new, _) = h.capture();
    let before = h.k.lifecycle_state();
    for handle in [
        h.actor,
        h.schedule,
        h.observer,
        old_extension,
        new_extension,
    ] {
        assert_eq!(h.k.unregister(handle), Err(Error::Referenced));
        assert_eq!(h.k.managed_unload(handle), Err(Error::Referenced));
        assert_eq!(h.k.lifecycle_state(), before);
    }
    let (a, la) = h.k.adopt_callback(&old).unwrap();
    let (b, lb) = h.k.adopt_callback(&new).unwrap();
    assert_eq!(abi(&invoke(&h.k, a, la).unwrap()), 25);
    assert_eq!(abi(&invoke(&h.k, b, lb).unwrap()), 45);
    assert_eq!(h.observations.borrow()[0].caller, old_extension);
    assert_eq!(h.observations.borrow()[1].caller, new_extension);
    h.k.revoke_grant(old_extension, 0).unwrap();
    let before = h.k.lifecycle_state();
    let state = h.state.borrow().clone();
    assert_eq!(
        abi(&invoke(&h.k, a, la).unwrap()),
        Error::Revoked.abi_code()
    );
    assert_eq!(h.k.lifecycle_state(), before);
    assert_eq!(*h.state.borrow(), state);
    h.k.issue_grant(old_extension, 0, auth()).unwrap();
    assert_eq!(abi(&invoke(&h.k, a, la).unwrap()), 25);
    drop(old);
    drop(new);
    h.k.release_pipeline(la).unwrap();
    h.k.managed_unload(old_extension).unwrap();
    assert_eq!(h.k.unregister(new_extension), Err(Error::Referenced));
    assert_eq!(abi(&invoke(&h.k, b, lb).unwrap()), 45);
    h.k.release_pipeline(lb).unwrap();
    h.k.managed_detach(new_extension).unwrap();
    h.k.managed_unload(new_extension).unwrap();
    assert_eq!(h.k.lifecycle_state().references, 0);
}
#[test]
fn missing_export_forged_json_plain_call_and_broader_ceiling_are_atomic() {
    let mut h = Host::new(false);
    let (_, lease, source) = h.source();
    let baseline = h.k.lifecycle_state();
    let state = h.state.borrow().clone();
    let mut broad = auth();
    broad.scope = Scope::All;
    let mut read = auth();
    read.access = Access::Read;
    for (bytes, error) in [
        (request("absent", auth()), Error::MissingDependency),
        (request("callback", broad), Error::Authority),
        (request("callback", read), Error::Authority),
        (br#"{"endpoint":"callback","ceiling":{"access":"Write","scope":"All"},"caller":1,"operation":1}"#.to_vec(), Error::NativeRefusal),
    ] {
        assert_eq!(abi(&h.k.invoke_operation(&source, &bytes, auth()).unwrap()), error.abi_code());
        assert_eq!(h.k.lifecycle_state(), baseline); assert_eq!(*h.state.borrow(), state);
        assert!(h.stored.borrow().is_empty());
    }
    assert_eq!(
        abi(&h
            .k
            .invoke(h.actor, "run", &request("callback", auth()), auth())
            .unwrap()),
        Error::Authority.abi_code()
    );
    assert_eq!(
        h.k.invoke(h.schedule, "run", &request("callback", auth()), auth()),
        Err(Error::Authority)
    );
    assert_eq!(h.k.lifecycle_state(), baseline);
    assert_eq!(
        abi(&h
            .k
            .invoke_operation(&source, &request("callback", auth()), auth())
            .unwrap()),
        0
    );
    h.stored.borrow_mut().clear();
    h.k.release_pipeline(lease).unwrap();
}
#[test]
fn wrong_host_revoked_narrowed_and_nested_live_grants_with_valid_controls() {
    let mut h = Host::new(false);
    let (callback, _) = h.capture();
    let mut other = Host::new(false);
    let foreign = other.k.lifecycle_state();
    assert_eq!(other.k.adopt_callback(&callback), Err(Error::InvalidHandle));
    assert_eq!(other.k.lifecycle_state(), foreign);
    let (op, lease) = h.k.adopt_callback(&callback).unwrap();
    let state = h.state.borrow().clone();
    h.k.revoke_grant(h.actor, 0).unwrap();
    let before = h.k.lifecycle_state();
    assert_eq!(h.k.adopt_callback(&callback), Err(Error::Revoked));
    assert_eq!(invoke(&h.k, op, lease), Err(Error::Revoked));
    assert_eq!(h.k.lifecycle_state(), before);
    assert_eq!(*h.state.borrow(), state);
    let empty = Authority {
        access: Access::Write,
        scope: Scope::Keys(BTreeSet::new()),
    };
    h.k.issue_grant(h.actor, 0, empty.clone()).unwrap();
    let before = h.k.lifecycle_state();
    assert_eq!(h.k.adopt_callback(&callback), Err(Error::Authority));
    assert_eq!(invoke(&h.k, op, lease), Err(Error::Authority));
    assert_eq!(h.k.lifecycle_state(), before);
    h.k.issue_grant(h.actor, 0, auth()).unwrap();
    h.k.revoke_grant(h.actor, 1).unwrap();
    let before = h.k.lifecycle_state();
    assert_eq!(
        abi(&invoke(&h.k, op, lease).unwrap()),
        Error::Revoked.abi_code()
    );
    assert_eq!(h.k.lifecycle_state(), before);
    assert_eq!(*h.state.borrow(), state);
    h.k.issue_grant(h.actor, 1, empty).unwrap();
    assert_eq!(
        abi(&invoke(&h.k, op, lease).unwrap()),
        Error::Authority.abi_code()
    );
    assert_eq!(*h.state.borrow(), state);
    h.k.issue_grant(
        h.actor,
        1,
        Authority {
            access: Access::Write,
            scope: Scope::Keys(BTreeSet::from([key("value"), key("unrelated")])),
        },
    )
    .unwrap();
    assert_eq!(abi(&invoke(&h.k, op, lease).unwrap()), 6);
    let before = h.k.lifecycle_state();
    assert_eq!(
        h.k.invoke_pipeline(
            lease,
            &5u32.to_le_bytes(),
            Authority {
                access: Access::Write,
                scope: Scope::All
            }
        ),
        Err(Error::Authority)
    );
    assert_eq!(h.k.lifecycle_state(), before);
}
#[test]
fn unpublished_drop_native_refusal_and_guest_trap_preserve_committed_prefix() {
    for (trap, mode, stored) in [
        (false, Mode::DropRefuse, false),
        (true, Mode::Drop, false),
        (true, Mode::Store, true),
        (false, Mode::StoreRefuse, true),
    ] {
        let mut h = Host::new(trap);
        *h.mode.borrow_mut() = mode;
        let (_, lease, source) = h.source();
        let before = h.state.borrow().clone();
        let result =
            h.k.invoke_operation(&source, &request("callback", auth()), auth());
        if trap {
            assert_eq!(result, Err(Error::Trap));
        } else {
            assert_eq!(abi(&result.unwrap()), Error::NativeRefusal.abi_code());
        }
        assert_eq!(*h.state.borrow(), before);
        h.k.release_pipeline(lease).unwrap();
        assert_eq!(h.k.lifecycle_state().callbacks, usize::from(stored));
        if stored {
            let callback = h.stored.borrow_mut().pop().unwrap();
            let (op, adopted) = h.k.adopt_callback(&callback).unwrap();
            drop(callback);
            assert_eq!(abi(&invoke(&h.k, op, adopted).unwrap()), 6);
            h.k.release_pipeline(adopted).unwrap();
        }
        assert_eq!(h.k.lifecycle_state().references, 0);
        h.k.managed_unload(h.actor).unwrap();
    }
}
#[test]
fn native_storage_borrow_refusal_drops_capture_and_shared_index_stays_bounded() {
    let mut h = Host::new(false);
    let (_, lease, source) = h.source();
    let guard = h.stored.borrow_mut();
    let before = h.k.lifecycle_state();
    assert_eq!(
        abi(&h
            .k
            .invoke_operation(&source, &request("callback", auth()), auth())
            .unwrap()),
        Error::NativeRefusal.abi_code()
    );
    let mut expected = before;
    expected.next_callback += 1;
    assert_eq!(h.k.lifecycle_state(), expected);
    drop(guard);
    for _ in 0..140 {
        assert_eq!(
            abi(&h
                .k
                .invoke_operation(&source, &request("callback", auth()), auth())
                .unwrap()),
            0
        );
        h.stored.borrow_mut().clear();
        assert_eq!(h.k.lifecycle_state().callbacks, 0);
    }
    h.k.release_pipeline(lease).unwrap();
    assert_eq!(h.k.lifecycle_state().references, 0);
}
#[test]
fn capture_and_adoption_capacity_refusals_leave_exact_metadata() {
    let mut h = Host::new(false);
    h.k.set_retention_limits(RetentionLimits {
        owners: 1,
        leases: 1,
        ..RetentionLimits::default()
    })
    .unwrap();
    let (_, source_lease, source) = h.source();
    assert_eq!(
        abi(&h
            .k
            .invoke_operation(&source, &request("callback", auth()), auth())
            .unwrap()),
        0
    );
    let callback = h.stored.borrow()[0].clone();
    let before = h.k.lifecycle_state();
    assert_eq!(
        abi(&h
            .k
            .invoke_operation(&source, &request("callback", auth()), auth())
            .unwrap()),
        Error::Limit.abi_code()
    );
    assert_eq!(h.k.adopt_callback(&callback), Err(Error::Limit));
    assert_eq!(h.k.lifecycle_state(), before);
    h.k.release_pipeline(source_lease).unwrap();
    let (op, adopted) = h.k.adopt_callback(&callback).unwrap();
    assert_eq!(abi(&invoke(&h.k, op, adopted).unwrap()), 6);
    h.k.release_pipeline(adopted).unwrap();
    h.stored.borrow_mut().clear();
    drop(callback);
    h.k.set_retention_limits(RetentionLimits {
        references: 3,
        ..RetentionLimits::default()
    })
    .unwrap();
    let (_, lease, source) = h.source();
    let before = h.k.lifecycle_state();
    assert_eq!(
        abi(&h
            .k
            .invoke_operation(&source, &request("callback", auth()), auth())
            .unwrap()),
        Error::Limit.abi_code()
    );
    assert_eq!(h.k.lifecycle_state(), before);
    h.k.release_pipeline(lease).unwrap();
    h.k.set_retention_limits(RetentionLimits {
        references: 6,
        ..RetentionLimits::default()
    })
    .unwrap();
    let (callback, _) = h.capture();
    let owner = h.k.retain_owner(&[h.observer]).unwrap();
    let before = h.k.lifecycle_state();
    assert_eq!(h.k.adopt_callback(&callback), Err(Error::Limit));
    assert_eq!(h.k.lifecycle_state(), before);
    drop(callback);
    h.k.release_owner(owner).unwrap();
}

#[test]
fn missing_historical_nested_authority_cannot_be_restored_beyond_capture_ceiling() {
    let mut h = Host::new(false);
    h.k.revoke_grant(h.actor, 1).unwrap();
    let (old, _) = h.capture();
    h.k.issue_grant(h.actor, 1, auth()).unwrap();
    let (op, lease) = h.k.adopt_callback(&old).unwrap();
    let before = h.k.lifecycle_state();
    let state = h.state.borrow().clone();
    assert_eq!(
        abi(&invoke(&h.k, op, lease).unwrap()),
        Error::MissingGrant.abi_code()
    );
    assert_eq!(h.k.lifecycle_state(), before);
    assert_eq!(*h.state.borrow(), state);
    let (new, _) = h.capture();
    let (new_op, new_lease) = h.k.adopt_callback(&new).unwrap();
    assert_eq!(abi(&invoke(&h.k, new_op, new_lease).unwrap()), 6);
    drop(old);
    drop(new);
    h.k.release_pipeline(lease).unwrap();
    h.k.release_pipeline(new_lease).unwrap();
    assert_eq!(h.k.lifecycle_state().references, 0);
}

#[test]
fn capture_nodes_and_slots_refuse_without_publication_after_source_admission() {
    for limits in [
        RetentionLimits {
            nodes: 3,
            ..RetentionLimits::default()
        },
        RetentionLimits {
            slots: 2,
            ..RetentionLimits::default()
        },
    ] {
        let mut h = Host::new(false);
        h.k.set_retention_limits(limits).unwrap();
        let (_, lease, source) = h.source();
        let ext = h.extension("extra", 20);
        let before = h.k.lifecycle_state();
        let state = h.state.borrow().clone();
        assert_eq!(
            abi(&h
                .k
                .invoke_operation(&source, &request("callback", auth()), auth())
                .unwrap()),
            Error::Limit.abi_code()
        );
        assert_eq!(h.k.lifecycle_state(), before);
        assert_eq!(*h.state.borrow(), state);
        h.k.release_pipeline(lease).unwrap();
        h.k.managed_detach(ext).unwrap();
        h.k.managed_unload(ext).unwrap();
        h.k.set_retention_limits(RetentionLimits::default())
            .unwrap();
        let (callback, _) = h.capture();
        let (op, adopted) = h.k.adopt_callback(&callback).unwrap();
        assert_eq!(abi(&invoke(&h.k, op, adopted).unwrap()), 6);
    }
}

#[test]
fn active_source_retains_nested_resolution_and_authenticates_actual_extension_caller() {
    let mut h = Host::new(false);
    let mut m = manifest("source_extension", &["run", "callback"]);
    m.dependencies = vec![dep("schedule", "run"), dep("observer", "run")];
    m.extension = Some(Extension {
        target: cap("actor", "run"),
        export: "run".into(),
    });
    let source_extension = h.k.register_guest(m, &guest_code(false)).unwrap();
    for slot in [0, 1] {
        h.k.issue_grant(source_extension, slot, auth()).unwrap();
    }
    let (_, source_lease, source) = h.source();
    let mut replacement = manifest("observer_extension", &["run"]);
    replacement.extension = Some(Extension {
        target: cap("observer", "run"),
        export: "run".into(),
    });
    let observer_extension =
        h.k.register_native(
            replacement,
            b"changed-observer",
            std::collections::BTreeMap::from([(
                "run".into(),
                std::rc::Rc::new(|_: &[u8], _: &CallerView| Ok(99u32.to_le_bytes().to_vec()))
                    as NativePort,
            )]),
        )
        .unwrap();
    assert_eq!(
        abi(&h
            .k
            .invoke_operation(&source, &request("callback", auth()), auth())
            .unwrap()),
        0
    );
    let old = h.stored.borrow_mut().pop().unwrap();
    assert_eq!(old.provenance().caller.handle, source_extension);
    assert_eq!(old.provenance().source, source);
    h.k.release_pipeline(source_lease).unwrap();
    let (new, _) = h.capture();
    let (a, la) = h.k.adopt_callback(&old).unwrap();
    let (b, lb) = h.k.adopt_callback(&new).unwrap();
    assert_eq!(abi(&invoke(&h.k, a, la).unwrap()), 6);
    assert_eq!(abi(&invoke(&h.k, b, lb).unwrap()), 99);
    assert_eq!(h.observations.borrow()[0].caller, source_extension);
    assert_eq!(h.k.unregister(source_extension), Err(Error::Referenced));
    assert_eq!(h.k.unregister(observer_extension), Err(Error::Referenced));
    drop(old);
    drop(new);
    h.k.release_pipeline(la).unwrap();
    h.k.release_pipeline(lb).unwrap();
    assert_eq!(h.k.lifecycle_state().references, 0);
}

#[test]
fn outer_kernel_borrow_refuses_reentrant_mutation_while_capture_is_available() {
    use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
    let h = Host::new(false);
    let actor = h.actor;
    let owner = Rc::new(RefCell::new(h.k));
    let weak = Rc::downgrade(&owner);
    let captures = Rc::new(RefCell::new(vec![]));
    let stored = captures.clone();
    let snapshots = Rc::new(RefCell::new(Vec::<CallerView<'static>>::new()));
    let views = snapshots.clone();
    let schedule = owner
        .borrow_mut()
        .register_native(
            manifest("reentrant", &["run"]),
            b"reentrant",
            BTreeMap::from([(
                "run".into(),
                Rc::new(move |_: &[u8], view: &CallerView| {
                    let owner = weak.upgrade().unwrap();
                    assert!(owner.try_borrow_mut().is_err());
                    views.borrow_mut().push(view.clone());
                    assert_eq!(
                        view.clone()
                            .capture_callback("callback", auth())
                            .unwrap_err(),
                        Error::Authority
                    );
                    stored
                        .borrow_mut()
                        .push(view.capture_callback("callback", auth())?);
                    Ok(0i32.to_le_bytes().to_vec())
                }) as NativePort,
            )]),
        )
        .unwrap();
    let mut m = manifest("reentrant_actor", &["run", "callback"]);
    m.dependencies = vec![dep("reentrant", "run"), dep("observer", "run")];
    let caller = owner
        .borrow_mut()
        .register_guest(m, &guest_code(false))
        .unwrap();
    for slot in [0, 1] {
        owner
            .borrow_mut()
            .issue_grant(caller, slot, auth())
            .unwrap();
    }
    let (op, lease) = owner
        .borrow_mut()
        .capture_operation(caller, "run", auth())
        .unwrap();
    let binding = owner
        .borrow()
        .operation_binding(op, lease, 0, "schedule")
        .unwrap();
    assert_eq!(
        abi(&owner
            .borrow()
            .invoke_operation(&binding, &request("callback", auth()), auth())
            .unwrap()),
        0
    );
    owner.borrow_mut().release_pipeline(lease).unwrap();
    assert_eq!(snapshots.borrow()[0].caller(), Some(caller));
    assert_eq!(snapshots.borrow()[0].operation(), Some(&binding));
    assert_eq!(
        snapshots.borrow()[0]
            .capture_callback("callback", auth())
            .unwrap_err(),
        Error::Authority
    );
    let callback = captures.borrow_mut().pop().unwrap();
    let (a, adopted) = owner.borrow_mut().adopt_callback(&callback).unwrap();
    drop(callback);
    assert_eq!(abi(&invoke(&owner.borrow(), a, adopted).unwrap()), 6);
    assert_eq!(
        owner.borrow_mut().unregister(schedule),
        Err(Error::Referenced)
    );
    owner.borrow_mut().release_pipeline(adopted).unwrap();
    owner.borrow_mut().managed_unload(caller).unwrap();
    owner.borrow_mut().managed_unload(schedule).unwrap();
    assert_eq!(owner.borrow().lifecycle_state().references, 0);
    assert!(owner.borrow().manifest(actor).is_ok());
}
