#![cfg(feature = "persistence")]
mod persistence_support;
use nethacked_kernel::{persistence::*, *};
use nethacked_resources::persistence::*;
use persistence_support::*;
use std::{collections::BTreeSet, rc::Rc};

fn resave(
    kernel: &mut Kernel,
    restored: &Restored,
    effects: &Rc<Effects>,
) -> (KernelSnapshot, Checkpoint) {
    let service = restored.service();
    let mut capture = CaptureCatalog::new(service);
    for owner in service
        .state()
        .unwrap()
        .resources
        .keys()
        .map(|r| r.owner.clone())
        .collect::<BTreeSet<_>>()
    {
        capture.owner(&owner, codec(Role::Owner, effects)).unwrap();
    }
    capture
        .accounting(codec(Role::Accounting, effects))
        .unwrap();
    let operations = service.persistence_enrollment(kernel).unwrap();
    let (cleanup, roots) = service.persistence_roots().unwrap();
    let k = kernel
        .kernel_snapshot_with_resource_roots(
            &KernelEnrollment {
                operations: &operations,
                pure_captures: &[],
            },
            &[cleanup],
            &roots,
            &service.persistence_history().unwrap(),
        )
        .unwrap();
    let index = kernel.execution_capture_index(&k).unwrap();
    let c = service.capture_local(&index, &k, &capture).unwrap();
    (k, c)
}

#[test]
fn disposed_extra_cleanup_is_independently_approved_and_fresh_on_two_resaves() {
    let mut f = Fixture::new();
    let extra = CleanupIdentity::issue().unwrap();
    let (id, lease) = f
        .kernel
        .capture_operation(f.actor, "step", f.auth())
        .unwrap();
    let op = f
        .kernel
        .operation_binding_with_cleanup(
            id,
            lease,
            0,
            "charge",
            &[f.service.cleanup_identity().unwrap(), extra],
        )
        .unwrap();
    let receipt = f.prepare(&f.plan(), Some(&op));
    f.kernel.release_pipeline(op.pipeline()).unwrap();
    let (mut k, mut c) = f.save();
    let materials = f.materials.clone();
    drop(f);
    assert!(k.callbacks.is_empty());
    assert!(k.operations.is_empty());
    assert!(k.disposed_sources.is_empty());
    let extra_saved = *c.pending[0]
        .context
        .operation
        .as_ref()
        .unwrap()
        .cleanup
        .iter()
        .find(|id| **id != c.cleanup)
        .unwrap();
    let effects = Rc::new(Effects::default());
    let approved = Approved::new(&k, &c);
    let mut forged = k.clone();
    forged
        .disposed_sources
        .insert(c.pending[0].context.operation.as_ref().unwrap().pipeline);
    assert!(forged.validate_execution_metadata().is_err());
    let mut altered = c.clone();
    altered.pending[0].context.operation.as_mut().unwrap().phase = "forged".into();
    assert!(altered
        .validate(&limits(), &k, catalog(&effects), &approved)
        .is_err());
    let mut altered_kernel = k.clone();
    altered_kernel.service_cleanup.remove(&extra_saved);
    assert!(c
        .clone()
        .validate(&limits(), &altered_kernel, catalog(&effects), &approved)
        .is_err());
    assert_eq!(effects.validates.get(), 0);
    let mut all: BTreeSet<_> = c.pending[0]
        .context
        .operation
        .as_ref()
        .unwrap()
        .cleanup
        .iter()
        .copied()
        .collect();
    for _ in 0..2 {
        let source = c.pending[0].context.operation.as_ref().unwrap().clone();
        let (mut kernel, mapped, restored) = restore(&k, &c, materials.clone(), &effects);
        assert!(mapped.execution_source(&source).is_err());
        assert!(restored.transaction(receipt.serial).is_err());
        let snapshot = kernel
            .kernel_snapshot_with_resource_roots(
                &Default::default(),
                &[restored.service().cleanup_identity().unwrap()],
                &restored.service().persistence_roots().unwrap().1,
                &restored.service().persistence_history().unwrap(),
            )
            .unwrap();
        let index = kernel.execution_capture_index(&snapshot).unwrap();
        let fresh: BTreeSet<_> = source
            .cleanup
            .iter()
            .map(|old| index.cleanup(mapped.cleanup()[old]).unwrap())
            .collect();
        assert_eq!(fresh.len(), 2);
        assert!(fresh.is_disjoint(&all));
        all.extend(&fresh);
        // Reuse only immutable artifact bytes; no source service or closures.
        let (next_k, next_c) = resave(&mut kernel, &restored, &effects);
        assert!(next_k.disposed_sources.is_empty());
        assert_eq!(
            next_c.pending[0]
                .context
                .operation
                .as_ref()
                .unwrap()
                .cleanup,
            fresh.into_iter().collect::<Vec<_>>()
        );
        k = next_k;
        c = next_c;
    }
    assert_eq!(effects.predicates.get(), 0);
}

#[test]
fn unbound_service_and_owner_current_grants_refuse_revocation_and_narrowing_before_factories() {
    let mut f = Fixture::new();
    let receipt = f.prepare(&f.plan(), None);
    // A legal unbound plan does not freeze a broad retained routing ceiling.
    for slot in 0..3 {
        f.kernel
            .issue_grant(
                f.actor,
                slot,
                auth([resource("a"), resource("b"), resource("unused")]),
            )
            .unwrap();
    }
    let (k, c) = f.save();
    let materials = f.materials.clone();
    drop(f);
    for slot in 0..3 {
        for revoke in [false, true] {
            let effects = Rc::new(Effects::default());
            let mut bad = k.clone();
            let grant = bad
                .registry
                .grants
                .iter_mut()
                .find(|g| g.caller == c.pending[0].context.caller.module && g.slot == slot)
                .unwrap();
            if revoke {
                grant.active = false;
            } else {
                grant.requested.scope = Scope::Keys(BTreeSet::from([resource("a")]));
            }
            let approved_bad = Approved::new(&bad, &c);
            assert!(c
                .clone()
                .validate(&limits(), &bad, catalog(&effects), &approved_bad)
                .is_err());
            assert_eq!(effects.validates.get(), 0);
            let approved = Approved::new(&k, &c);
            let stage = c
                .clone()
                .validate(&limits(), &k, catalog(&effects), &approved)
                .unwrap()
                .stage()
                .unwrap();
            let host = Host::fresh(materials.clone(), &k, stage.ports());
            let mut kernel = fresh_kernel();
            let mapped = kernel.restore_kernel(&k, &host).unwrap();
            let actor = mapped.modules()[&c.pending[0].context.caller.module];
            if revoke {
                kernel.revoke_grant(actor, slot).unwrap();
            } else {
                kernel
                    .issue_grant(actor, slot, auth([resource("fresh-a")]))
                    .unwrap();
            }
            assert!(stage.finish(&kernel, &mapped).is_err());
            assert_eq!(effects.decodes.get(), 0);
            assert_eq!(effects.predicates.get(), 0);
        }
    }
    let effects = Rc::new(Effects::default());
    let approved = Approved::new(&k, &c);
    let stage = c
        .clone()
        .validate(&limits(), &k, catalog(&effects), &approved)
        .unwrap()
        .stage()
        .unwrap();
    let host = Host::fresh(materials, &k, stage.ports());
    let mut kernel = fresh_kernel();
    let mapped = kernel.restore_kernel(&k, &host).unwrap();
    let actor = mapped.modules()[&c.pending[0].context.caller.module];
    for slot in 0..3 {
        kernel
            .issue_grant(
                actor,
                slot,
                auth([resource("fresh-a"), resource("fresh-b")]),
            )
            .unwrap();
    }
    let restored = stage.finish(&kernel, &mapped).unwrap();
    assert_eq!(effects.predicates.get(), 0);
    commit(
        &kernel,
        &mapped,
        &restored,
        c.pending[0].context.caller.module,
        receipt.serial,
        None,
    )
    .unwrap();
    assert_eq!(restored.service().state().unwrap().commits, 1);
}

#[test]
fn catalog_enrollment_and_complete_approval_cannot_be_supplied_by_wire() {
    let mut f = Fixture::new();
    f.prepare(&f.plan(), None);
    let (k, c) = f.save();
    let effects = Rc::new(Effects::default());
    let approved = Approved::new(&k, &c);
    for missing in [Role::Owner, Role::Accounting] {
        let mut incomplete = RestoreCatalog::default();
        incomplete
            .codec(codec(
                if missing == Role::Owner {
                    Role::Accounting
                } else {
                    Role::Owner
                },
                &effects,
            ))
            .unwrap();
        assert!(c
            .clone()
            .validate(&limits(), &k, incomplete, &approved)
            .is_err());
    }
    assert_eq!(effects.validates.get(), 0);
    let alien = nethacked_resources::Service::new(limits(), Rc::new(|_| true)).unwrap();
    let mut unrelated = CaptureCatalog::new(&alien);
    unrelated
        .accounting(codec(Role::Accounting, &effects))
        .unwrap();
    let index = f.kernel.execution_capture_index(&k).unwrap();
    assert!(f.service.capture_local(&index, &k, &unrelated).is_err());
    assert_eq!(effects.encodes.get(), 0);
    let mut reference = c.clone();
    reference.owners[0].provider.module = k.registry.next_generation;
    let trusted_bad = Approved::new(&k, &reference);
    assert!(reference
        .validate(&limits(), &k, catalog(&effects), &trusted_bad)
        .is_err());
    let mut changed = k.clone();
    changed.cleanup_high_water += 1;
    assert!(c
        .clone()
        .validate(&limits(), &changed, catalog(&effects), &approved)
        .is_err());
    assert_eq!(effects.validates.get(), 0);
    assert_eq!(effects.decodes.get(), 0);
}

#[test]
fn abandoned_bound_plan_discard_requires_exact_operation_epoch_phase_and_lease() {
    let mut f = Fixture::new();
    let accepted = f.prepare(&f.plan(), None);
    abi(f
        .kernel
        .invoke(f.actor, "step", &command(b'C', &accepted), f.auth())
        .unwrap())
    .unwrap();
    let op = f.binding();
    let doomed = f.prepare(&f.plan(), Some(&op));
    let other = f.binding();
    let surviving = f.prepare(&f.plan(), Some(&other));
    let legacy = f.prepare(&f.plan(), None);
    let (k, c) = f.save();
    let source = c
        .pending
        .iter()
        .find(|p| p.serial == doomed.serial)
        .unwrap()
        .context
        .operation
        .clone()
        .unwrap();
    let materials = f.materials.clone();
    drop(f);
    let effects = Rc::new(Effects::default());
    let (mut kernel, mapped, restored) = restore(&k, &c, materials, &effects);
    let exact = mapped.execution_source(&source).unwrap();
    let before = restored.service().state().unwrap();
    for (epoch, phase) in [
        (exact.checkpoint() + 1, exact.phase()),
        (exact.checkpoint(), "other"),
    ] {
        let wrong = kernel
            .operation_binding_with_cleanup(
                exact.operation(),
                exact.pipeline(),
                epoch,
                phase,
                exact.cleanup_identities(),
            )
            .unwrap();
        restored.service().discard_operation(&wrong).unwrap();
        assert_eq!(restored.service().state().unwrap(), before);
    }
    restored.service().discard_operation(&exact).unwrap();
    restored.service().discard_operation(&exact).unwrap();
    kernel.release_pipeline(exact.pipeline()).unwrap();
    let after = restored.service().state().unwrap();
    assert_eq!(after.resources, before.resources);
    assert_eq!(after.history, before.history);
    assert_eq!(after.commits, before.commits);
    assert_eq!(after.next_serial, before.next_serial);
    assert_eq!(after.next_generation, before.next_generation);
    let (_, saved) = resave(&mut kernel, &restored, &effects);
    assert_eq!(
        saved.pending.iter().map(|p| p.serial).collect::<Vec<_>>(),
        [surviving.serial, legacy.serial]
    );
    assert_eq!(effects.predicates.get(), 0);
}

#[test]
fn ordinary_kernel_capture_is_observational_while_service_capture_carries_allocator_holes() {
    let mut kernel = fresh_kernel();
    let before = serde_json::to_vec(&kernel.kernel_snapshot(&Default::default()).unwrap()).unwrap();
    for _ in 0..8 {
        CleanupIdentity::issue().unwrap();
    }
    let after = serde_json::to_vec(&kernel.kernel_snapshot(&Default::default()).unwrap()).unwrap();
    assert_eq!(before, after);
    let service = nethacked_resources::Service::new(limits(), Rc::new(|_| true)).unwrap();
    let effects = Rc::new(Effects::default());
    let mut capture = CaptureCatalog::new(&service);
    capture
        .accounting(codec(Role::Accounting, &effects))
        .unwrap();
    let (cleanup, roots) = service.persistence_roots().unwrap();
    let saved = kernel
        .kernel_snapshot_with_resource_roots(&Default::default(), &[cleanup], &roots, &[])
        .unwrap();
    let index = kernel.execution_capture_index(&saved).unwrap();
    let c = service.capture_local(&index, &saved, &capture).unwrap();
    assert!(saved.cleanup_high_water >= c.cleanup);
    assert!(saved.cleanup_high_water > 8);
    assert_eq!(
        before,
        serde_json::to_vec(&kernel.kernel_snapshot(&Default::default()).unwrap()).unwrap()
    );
}
