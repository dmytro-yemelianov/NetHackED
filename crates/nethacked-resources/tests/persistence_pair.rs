#![cfg(feature = "persistence")]
#[path = "persistence_support/mod.rs"]
mod support;
use nethacked_kernel::{persistence::*, *};
use nethacked_resources::persistence::*;
use std::{collections::BTreeMap, rc::Rc};
use support::*;

fn stage(c: &Checkpoint, k: &KernelSnapshot, e: &Rc<Effects>) -> Staged {
    c.clone()
        .validate(&limits(), k, catalog(e), &Approved::new(k, c))
        .unwrap()
        .stage()
        .unwrap()
}
fn reset(e: &Effects) {
    e.validates.set(0);
    e.decodes.set(0);
    e.predicates.set(0);
    e.encodes.set(0);
}
fn no_callbacks(e: &Effects) {
    assert_eq!(e.validates.get(), 0);
    assert_eq!(e.decodes.get(), 0);
    assert_eq!(e.predicates.get(), 0);
    assert_eq!(e.encodes.get(), 0);
}

#[test]
fn substituted_complete_pair_refuses_before_claim_and_allows_exact_pair_retry() {
    let mut f = Fixture::new();
    f.prepare(&f.plan(), None);
    let (k, c) = f.save();
    for extra_root in [false, true] {
        let bad_effects = Rc::new(Effects::default());
        let rejected = stage(&c, &k, &bad_effects);
        let mut alternate = k.clone();
        alternate.cleanup_high_water += 10;
        if extra_root {
            alternate
                .cleanup_history
                .insert(alternate.cleanup_high_water);
            alternate
                .service_cleanup
                .insert(alternate.cleanup_high_water);
        }
        assert_eq!(alternate.source_namespace, k.source_namespace);
        assert!(Approved::new(&k, &c).approve(&c, &alternate).is_err());
        alternate.validate_execution_metadata().unwrap();
        // The substituted kernel uses the rejected stage's EXACT ports, as
        // in the supplied repro: allocation identity cannot mask this defect.
        // Independently approving the legal alternate pair remains supported.
        let good_effects = Rc::new(Effects::default());
        let accepted = stage(&c, &alternate, &good_effects);
        let mut host = Host::fresh(f.materials.clone(), &alternate, rejected.ports());
        let mut kernel = fresh_kernel();
        let mapped = kernel.restore_kernel(&alternate, &host).unwrap();
        reset(&bad_effects);
        let factories = host.factories.get();
        assert!(matches!(
            rejected.finish(&kernel, &mapped),
            Err(Error::Authority)
        ));
        no_callbacks(&bad_effects);
        assert_eq!(host.factories.get(), factories);
        // Both probes drop their reservation. A failed provenance check must
        // never consume the one-use enrollment, even on a valid alternate map.
        drop(mapped.enroll_resources(c.cleanup).unwrap());
        drop(mapped.enroll_resources(c.cleanup).unwrap());
        host.ports = Some(accepted.ports());
        let mapped = accepted
            .restore_kernel(&mut kernel, &alternate, &host)
            .unwrap();
        let restored = accepted.finish(&kernel, &mapped).unwrap();
        assert_eq!(good_effects.decodes.get(), 3);
        assert_eq!(restored.service().state().unwrap().commits, 0);
        assert!(mapped.enroll_resources(c.cleanup).is_err());
    }
}

#[test]
fn guarded_restore_preserves_nonempty_destination_and_retries_without_callbacks() {
    let mut f = Fixture::new();
    f.prepare(&f.plan(), None);
    let (k, c) = f.save();
    let effects = Rc::new(Effects::default());
    let staged = stage(&c, &k, &effects);
    let host = Host::fresh(f.materials.clone(), &k, staged.ports());
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
    let mut alternate = k.clone();
    alternate.cleanup_high_water += 10;
    let extra = alternate.cleanup_high_water;
    alternate.cleanup_history.insert(extra);
    alternate.service_cleanup.insert(extra);
    reset(&effects);
    assert!(matches!(
        staged.restore_kernel(&mut destination, &alternate, &host),
        Err(Error::Authority)
    ));
    assert_eq!(host.factories.get(), 0);
    no_callbacks(&effects);
    assert_eq!(destination.artifact(sentinel).unwrap(), before);
    assert_eq!(
        destination.invoke(sentinel, "step", b"", auth([])).unwrap(),
        b"kept"
    );
    let mapped = staged.restore_kernel(&mut destination, &k, &host).unwrap();
    let restored = staged.finish(&destination, &mapped).unwrap();
    assert_eq!(effects.decodes.get(), 3);
    assert_eq!(restored.service().state().unwrap().commits, 0);
}

#[test]
fn exact_source_provenance_allows_sufficient_destination_narrowing() {
    let mut f = Fixture::new();
    f.prepare(&f.plan(), None);
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
    let effects = Rc::new(Effects::default());
    let staged = stage(&c, &k, &effects);
    let host = Host::fresh(f.materials.clone(), &k, staged.ports());
    let mut kernel = fresh_kernel();
    let mapped = staged.restore_kernel(&mut kernel, &k, &host).unwrap();
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
    let restored = staged.finish(&kernel, &mapped).unwrap();
    assert_eq!(effects.decodes.get(), 3);
    assert_eq!(effects.predicates.get(), 0);
    assert_eq!(restored.service().state().unwrap().commits, 0);
}
