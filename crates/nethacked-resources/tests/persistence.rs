#![cfg(feature = "persistence")]
mod persistence_support;
use nethacked_kernel::*;
use nethacked_resources::{persistence::*, *};
use persistence_support::*;
use std::{cell::Cell, collections::BTreeMap, rc::Rc};
#[test]
fn sourcefree_cells_history_pending_and_once_only_publication() {
    let mut f = Fixture::new();
    let plan = f.plan();
    let accepted = f.prepare(&plan, None);
    abi(f
        .kernel
        .invoke(f.actor, "step", &command(b'C', &accepted), f.auth())
        .unwrap())
    .unwrap();
    let bound = f.binding();
    let mut plan = f.plan();
    plan.writes[0].replacement = None;
    let bound_id = f.prepare(&plan, Some(&bound));
    let unbound_id = f.prepare(&plan, None);
    let old = f.service.state().unwrap();
    let (k, c) = f.save();
    let old_actor = c.pending[0].context.caller.module;
    let op = c.pending[0].context.operation.clone();
    let materials = f.materials.clone();
    drop(f);
    let effects = Rc::new(Effects::default());
    let (kernel, mapped, r) = restore(&k, &c, materials, &effects);
    assert_eq!(effects.predicates.get(), 0);
    let state = r.service().state().unwrap();
    assert_eq!(state.history, old.history);
    assert_eq!(state.next_serial, old.next_serial);
    assert_eq!(state.next_generation, old.next_generation);
    assert_eq!(state.commits, 1);
    assert_eq!(state.resources[&resource("fresh-a")].stamp.revision, 1);
    let fresh = r.transaction(bound_id.serial).unwrap();
    assert_ne!(fresh.service, bound_id.service);
    let before = r.service().state().unwrap();
    assert!(abi(kernel
        .invoke(
            mapped.modules()[&old_actor],
            "step",
            &command(b'C', &bound_id),
            auth(before.resources.keys().cloned())
        )
        .unwrap())
    .is_err());
    assert_eq!(r.service().state().unwrap(), before);
    assert!(commit(&kernel, &mapped, &r, old_actor, bound_id.serial, None).is_err());
    assert_eq!(r.service().state().unwrap(), before);
    assert!(commit(
        &kernel,
        &mapped,
        &r,
        old_actor,
        unbound_id.serial,
        op.as_ref()
    )
    .is_err());
    assert_eq!(r.service().state().unwrap(), before);
    commit(
        &kernel,
        &mapped,
        &r,
        old_actor,
        bound_id.serial,
        op.as_ref(),
    )
    .unwrap();
    let published = r.service().state().unwrap();
    assert_eq!(published.commits, 2);
    assert_eq!(
        published.history,
        [
            b"one".to_vec(),
            b"two".to_vec(),
            b"one".to_vec(),
            b"two".to_vec()
        ]
    );
    assert!(!published.resources.contains_key(&resource("fresh-a")));
    assert!(commit(
        &kernel,
        &mapped,
        &r,
        old_actor,
        bound_id.serial,
        op.as_ref()
    )
    .is_err());
    assert!(commit(&kernel, &mapped, &r, old_actor, unbound_id.serial, None).is_err());
    assert_eq!(r.service().state().unwrap(), published);
    let regenerated = r
        .service()
        .put(resource("fresh-a"), b"again".to_vec())
        .unwrap();
    assert_eq!(regenerated.generation, old.next_generation);
    assert_eq!(regenerated.revision, 0);
}
#[test]
fn stale_and_conflicted_plans_survive_without_predicates() {
    let mut f = Fixture::new();
    let id = f.prepare(&f.plan(), None);
    f.service.delete(&resource("a")).unwrap();
    let new = f.service.put(resource("a"), b"recreated".to_vec()).unwrap();
    let (k, c) = f.save();
    assert_ne!(new.generation, c.pending[0].plan.writes[0].stamp.generation);
    let actor = c.pending[0].context.caller.module;
    let materials = f.materials.clone();
    drop(f);
    let effects = Rc::new(Effects::default());
    let (kernel, mapped, r) = restore(&k, &c, materials, &effects);
    assert_eq!(effects.predicates.get(), 0);
    let before = r.service().state().unwrap();
    assert!(commit(&kernel, &mapped, &r, actor, id.serial, None).is_err());
    assert_eq!(effects.predicates.get(), 0);
    assert_eq!(r.service().state().unwrap(), before);
}
#[test]
fn empty_uninstalled_service_roundtrips() {
    let mut kernel = fresh_kernel();
    let service = Service::new(limits(), Rc::new(|_| true)).unwrap();
    let effects = Rc::new(Effects::default());
    let mut capture = CaptureCatalog::new(&service);
    capture
        .accounting(codec(Role::Accounting, &effects))
        .unwrap();
    let (cleanup, addresses) = service.persistence_roots().unwrap();
    let k = kernel
        .kernel_snapshot_with_service_roots(&Default::default(), &[cleanup], &addresses)
        .unwrap();
    let index = kernel.execution_capture_index(&k).unwrap();
    let c = service.capture_local(&index, &k, &capture).unwrap();
    drop(capture);
    drop(service);
    drop(kernel);
    let (mut kernel, _mapped, r) = restore(&k, &c, BTreeMap::new(), &effects);
    assert!(r.service().state().unwrap().resources.is_empty());
    r.service().install(&mut kernel, "new-resource").unwrap();
    assert_eq!(r.service().state().unwrap().next_generation, 1);
}
#[test]
fn old_policy_and_artifact_are_retained_with_current_future_dispatch() {
    let mut f = Fixture::new();
    let op = f.binding();
    let id = f.prepare(&f.plan(), Some(&op));
    let mut m = manifest("future");
    m.extension = Some(Extension {
        target: cap("policy", "step"),
        export: "step".into(),
    });
    let h = f
        .kernel
        .register_native(
            m.clone(),
            b"policy-future",
            BTreeMap::from([(
                "step".into(),
                Rc::new(|_: &[u8], _: &CallerView| Ok(b"future".to_vec())) as NativePort,
            )]),
        )
        .unwrap();
    f.materials.insert(
        f.kernel.artifact(h).unwrap(),
        Material::Native(m, b"policy-future".to_vec()),
    );
    let (k, c) = f.save();
    let actor = c.pending[0].context.caller.module;
    let saved_op = c.pending[0].context.operation.clone().unwrap();
    let old_provider = c.owners[0].provider.module;
    let materials = f.materials.clone();
    drop(f);
    let effects = Rc::new(Effects::default());
    let (kernel, mapped, r) = restore(&k, &c, materials, &effects);
    assert_eq!(effects.predicates.get(), 0);
    assert_eq!(
        kernel
            .invoke(
                mapped.modules()[&old_provider],
                "step",
                &[],
                auth([resource("fresh-a"), resource("fresh-b")])
            )
            .unwrap(),
        b"future"
    );
    commit(&kernel, &mapped, &r, actor, id.serial, Some(&saved_op)).unwrap();
    assert_eq!(effects.predicates.get(), 3);
    assert_eq!(r.service().state().unwrap().commits, 1);
}
#[test]
fn disposed_and_unloaded_pending_are_inert_and_can_be_resaved() {
    for unload in [false, true] {
        let mut f = Fixture::new();
        let op = if unload { None } else { Some(f.binding()) };
        let id = f.prepare(&f.plan(), op.as_ref());
        if let Some(op) = &op {
            f.kernel.release_pipeline(op.pipeline()).unwrap();
        } else {
            f.kernel.unregister(f.actor).unwrap();
        }
        let (k, c) = f.save();
        assert!(c.pending[0].context.historical);
        let materials = f.materials.clone();
        drop(f);
        let effects = Rc::new(Effects::default());
        let (mut kernel, mapped, r) = restore(&k, &c, materials, &effects);
        assert!(r.transaction(id.serial).is_err());
        assert_eq!(effects.predicates.get(), 0);
        let mut capture = CaptureCatalog::new(r.service());
        for owner in ["fresh-a", "fresh-b"] {
            capture.owner(owner, codec(Role::Owner, &effects)).unwrap();
        }
        capture
            .accounting(codec(Role::Accounting, &effects))
            .unwrap();
        let (cleanup, roots) = r.service().persistence_roots().unwrap();
        let k2 = kernel
            .kernel_snapshot_with_resource_roots(
                &Default::default(),
                &[cleanup],
                &roots,
                &r.service().persistence_history().unwrap(),
            )
            .unwrap();
        let index = kernel.execution_capture_index(&k2).unwrap();
        let c2 = r.service().capture_local(&index, &k2, &capture).unwrap();
        assert_eq!(c2.pending.len(), 1);
        assert!(c2.pending[0].context.historical);
        assert!(r.matches_destination(&mapped.execution_destination()));
    }
}
#[test]
fn structural_catalog_and_context_refusals_precede_codecs() {
    let mut f = Fixture::new();
    f.prepare(&f.plan(), None);
    let (k, c) = f.save();
    let approved = Approved::new(&k, &c);
    let effects = Rc::new(Effects::default());
    let mut mutations = Vec::new();
    let mut bad = c.clone();
    bad.next_serial = 1;
    mutations.push(bad);
    let mut bad = c.clone();
    bad.next_generation = 1;
    mutations.push(bad);
    let mut bad = c.clone();
    bad.pending[0].context.historical = true;
    mutations.push(bad);
    let mut bad = c.clone();
    let repeated = bad.pending[0].context.permits[0].resolution[0].clone();
    bad.pending[0].context.permits[0].resolution.push(repeated);
    mutations.push(bad);
    let mut bad = c.clone();
    bad.accounting.codec = identity(Role::Owner, true);
    mutations.push(bad);
    let mut bad = c.clone();
    bad.cells.push(bad.cells[0].clone());
    mutations.push(bad);
    let mut bad = c.clone();
    bad.pending[0].context.caller.artifact = [99; 32];
    mutations.push(bad);
    let mut bad = c.clone();
    bad.namespace_high_water = u64::MAX;
    mutations.push(bad);
    let mut bad = c.clone();
    bad.pending[0].context.permits[0].resolution[0].endpoint = "commit".into();
    mutations.push(bad);
    for bad in mutations {
        assert!(bad
            .validate(&limits(), &k, catalog(&effects), &approved)
            .is_err());
    }
    assert!(c
        .clone()
        .validate(&limits(), &k, RestoreCatalog::default(), &approved)
        .is_err());
    assert_eq!(effects.validates.get(), 0);
    assert_eq!(effects.decodes.get(), 0);
    assert_eq!(effects.predicates.get(), 0);
    let mut revoked = k.clone();
    revoked.registry.grants[2].active = false;
    assert!(c
        .clone()
        .validate(&limits(), &revoked, catalog(&effects), &approved)
        .is_err());
    assert_eq!(effects.validates.get(), 0);
}
#[test]
fn bound_history_cannot_hide_live_enrollment() {
    let mut f = Fixture::new();
    let op = f.binding();
    f.prepare(&f.plan(), Some(&op));
    let (k, c) = f.save();
    let mut bad = c.clone();
    bad.pending[0].context.historical = true;
    let effects = Rc::new(Effects::default());
    let forged = Approved::new(&k, &bad);
    assert!(bad
        .validate(&limits(), &k, catalog(&effects), &forged)
        .is_err());
    let mut missing = k.clone();
    missing.operations.clear();
    let approved = Approved::new(&missing, &c);
    assert!(c
        .validate(&limits(), &missing, catalog(&effects), &approved)
        .is_err());
    assert_eq!(effects.validates.get(), 0);
}
#[test]
fn nonempty_destination_atomic_refusal_and_retry() {
    let mut f = Fixture::new();
    f.prepare(&f.plan(), None);
    let (k, c) = f.save();
    let effects = Rc::new(Effects::default());
    let approved = Approved::new(&k, &c);
    let staged = c
        .clone()
        .validate(&limits(), &k, catalog(&effects), &approved)
        .unwrap()
        .stage()
        .unwrap();
    let mut host = Host::fresh(f.materials.clone(), &k, staged.ports());
    let mut destination = fresh_kernel();
    let sentinel = destination
        .register_native(
            manifest("sentinel"),
            b"sentinel",
            BTreeMap::from([(
                "step".into(),
                Rc::new(|_: &[u8], _: &CallerView| Ok(b"kept".to_vec())) as NativePort,
            )]),
        )
        .unwrap();
    let before = destination.artifact(sentinel).unwrap();
    host.alias = true;
    assert!(destination.restore_kernel(&k, &host).is_err());
    assert_eq!(host.factories.get(), 0);
    assert_eq!(destination.artifact(sentinel).unwrap(), before);
    host.alias = false;
    host.deny = true;
    assert!(destination.restore_kernel(&k, &host).is_err());
    assert_eq!(destination.artifact(sentinel).unwrap(), before);
    host.deny = false;
    let mapped = destination.restore_kernel(&k, &host).unwrap();
    let actor = mapped.modules()[&c.pending[0].context.caller.module];
    destination.revoke_grant(actor, 2).unwrap();
    assert!(staged.finish(&destination, &mapped).is_err());
    assert_eq!(effects.decodes.get(), 0);
    let staged = c
        .clone()
        .validate(&limits(), &k, catalog(&effects), &approved)
        .unwrap()
        .stage()
        .unwrap();
    host.ports = Some(staged.ports());
    let mut destination = fresh_kernel();
    let mapped = destination.restore_kernel(&k, &host).unwrap();
    let r = staged.finish(&destination, &mapped).unwrap();
    assert_eq!(r.service().state().unwrap().commits, 0);
    let staged = c
        .clone()
        .validate(&limits(), &k, catalog(&effects), &approved)
        .unwrap()
        .stage()
        .unwrap();
    assert!(staged.finish(&destination, &mapped).is_err());
}
#[test]
fn failed_factory_releases_claim_and_quarantined_ports_never_run() {
    let mut f = Fixture::new();
    let id = f.prepare(&f.plan(), None);
    let (k, c) = f.save();
    let approved = Approved::new(&k, &c);
    let effects = Rc::new(Effects::default());
    let mut codecs = catalog(&effects);
    let mut failing = RestoreCatalog::default();
    failing
        .codec(Rc::new(Codec {
            role: Role::Owner,
            old: true,
            effects: effects.clone(),
            fail: true,
        }))
        .unwrap();
    failing.codec(codec(Role::Accounting, &effects)).unwrap();
    let staged = c
        .clone()
        .validate(&limits(), &k, failing, &approved)
        .unwrap()
        .stage()
        .unwrap();
    let host = Host::fresh(f.materials.clone(), &k, staged.ports());
    let mut kernel = fresh_kernel();
    let mapped = kernel.restore_kernel(&k, &host).unwrap();
    let actor = mapped.modules()[&c.pending[0].context.caller.module];
    assert!(abi(kernel
        .invoke(
            actor,
            "step",
            &command(b'C', &id),
            auth([resource("fresh-a"), resource("fresh-b")])
        )
        .unwrap())
    .is_err());
    assert!(staged.finish(&kernel, &mapped).is_err());
    let staged = c
        .validate(&limits(), &k, std::mem::take(&mut codecs), &approved)
        .unwrap()
        .stage()
        .unwrap();
    let retry_host = Host::fresh(f.materials.clone(), &k, staged.ports());
    let mut retry_kernel = fresh_kernel();
    let retry_map = retry_kernel.restore_kernel(&k, &retry_host).unwrap();
    let r = staged.finish(&retry_kernel, &retry_map).unwrap();
    assert!(r.transaction(id.serial).is_ok());
    // Fresh retry allocation must be installed by its own kernel factory. Ports of
    // the abandoned stage remain quarantined, so they cannot publish the new state.
    assert!(abi(kernel
        .invoke(
            actor,
            "step",
            &command(b'C', &r.transaction(id.serial).unwrap()),
            auth([resource("fresh-a"), resource("fresh-b")])
        )
        .unwrap())
    .is_err());
    assert_eq!(r.service().state().unwrap().commits, 0);
}
#[test]
fn callback_busy_and_capture_is_effect_free() {
    let mut f = Fixture::new();
    let (cleanup, roots) = f.service.persistence_roots().unwrap();
    let saved = f
        .kernel
        .kernel_snapshot_with_resource_roots(
            &Default::default(),
            &[cleanup],
            &roots,
            &f.service.persistence_history().unwrap(),
        )
        .unwrap();
    let index = f.kernel.execution_capture_index(&saved).unwrap();
    let calls = Rc::new(Cell::new(0));
    let calls2 = calls.clone();
    let service = f.service.clone();
    *f.effects.callback.borrow_mut() = Some(Box::new(move || {
        assert!(matches!(
            service.persistence_roots(),
            Err(PersistenceError::CaptureBusy)
        ));
        calls2.set(calls2.get() + 1);
    }));
    let before = f.service.state().unwrap();
    f.service.capture_local(&index, &saved, &f.capture).unwrap();
    assert_eq!(f.service.state().unwrap(), before);
    assert_eq!(calls.get(), 0);
    f.prepare(&f.plan(), None);
    assert_eq!(calls.get(), 3);
}
#[test]
fn canonical_bytes_reject_aliases_unknown_fields_duplicates_and_depth() {
    let mut f = Fixture::new();
    let (_, c) = f.save();
    let bytes = c.to_bytes(1 << 20).unwrap();
    assert!(Checkpoint::from_bytes(&bytes, bytes.len() - 1).is_err());
    let mut whitespace = bytes.clone();
    whitespace.push(b' ');
    assert!(Checkpoint::from_bytes(&whitespace, 1 << 20).is_err());
    let text = String::from_utf8(bytes).unwrap();
    let duplicate = text.replacen("{", "{\"version\":1,", 1);
    assert!(Checkpoint::from_bytes(duplicate.as_bytes(), 1 << 20).is_err());
    let unknown = text.replacen("{", "{\"closure_address\":1,", 1);
    assert!(Checkpoint::from_bytes(unknown.as_bytes(), 1 << 20).is_err());
    assert!(Checkpoint::from_bytes(
        format!("{}0{}", "[".repeat(33), "]".repeat(33)).as_bytes(),
        1 << 20
    )
    .is_err());
}
