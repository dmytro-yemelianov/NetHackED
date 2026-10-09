use nethacked_persistence::runtime::*;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;

struct Owner {
    marker: u64,
}
struct Service {
    marker: u64,
    paused: bool,
}
// Independent allocations and a host-erased third participant, with no entity or World.
struct Bundle {
    owner: Box<Owner>,
    service: Box<Service>,
    erased: Box<dyn Any>,
    retired: Rc<Cell<u64>>,
    panic_drop: bool,
    retire_probe: Option<Rc<dyn Fn()>>,
}
impl Bundle {
    fn new(marker: u64, retired: Rc<Cell<u64>>) -> Self {
        Self {
            owner: Box::new(Owner { marker }),
            service: Box::new(Service {
                marker,
                paused: true,
            }),
            erased: Box::new(vec![marker]),
            retired,
            panic_drop: false,
            retire_probe: None,
        }
    }
    fn markers(&self) -> (u64, u64, u64) {
        (
            self.owner.marker,
            self.service.marker,
            self.erased.downcast_ref::<Vec<u64>>().unwrap()[0],
        )
    }
}
impl Drop for Bundle {
    fn drop(&mut self) {
        if let Some(probe) = &self.retire_probe {
            probe();
        }
        self.retired.set(self.retired.get() + 1);
        assert!(!self.panic_drop, "retirement panic");
    }
}
fn binding(name: &str, version: u64) -> Binding {
    Binding {
        name: name.into(),
        version,
        artifact: [version as u8; 32],
    }
}
fn permissions(names: &[&str]) -> BTreeSet<Permission> {
    names
        .iter()
        .map(|name| Permission((*name).into()))
        .collect()
}
struct Factory {
    retired: Rc<Cell<u64>>,
    calls: Rc<Cell<u64>>,
    resolver: Binding,
    dependencies: Vec<Binding>,
    probe: Option<Rc<dyn Fn()>>,
}
impl TrustedFactory<Bundle> for Factory {
    fn build(
        &self,
        context: &mut BuildContext<'_, Bundle>,
        metadata: &[u8],
    ) -> Result<Bundle, Error> {
        self.calls.set(self.calls.get() + 1);
        context.require_permission(&Permission("keep".into()))?;
        context.require_binding(&self.resolver)?;
        for binding in &self.dependencies {
            context.require_binding(binding)?;
        }
        if let Some(probe) = &self.probe {
            probe();
        }
        if metadata == b"extra" {
            context.require_permission(&Permission("extra".into()))?;
        }
        let mut hidden = Bundle::new(99, self.retired.clone());
        match metadata {
            b"error" => return Err(Error::FactoryRefused),
            b"panic" => panic!("factory panic with hidden allocations"),
            b"drop-panic" => hidden.panic_drop = true,
            b"ignored-binding" => {
                let _ = context.require_binding(&binding("forged", 1));
            }
            b"ignored-proof" => {
                let _ = context.require_permission(&Permission("forged".into()));
            }
            _ => {}
        }
        Ok(hidden)
    }
}
fn config(
    names: &[&str],
    resolver: u64,
    retired: Rc<Cell<u64>>,
    calls: Rc<Cell<u64>>,
) -> ApprovedConfiguration<Bundle> {
    config_with_probe(names, resolver, retired, calls, None)
}
fn config_with_probe(
    names: &[&str],
    resolver: u64,
    retired: Rc<Cell<u64>>,
    calls: Rc<Cell<u64>>,
    probe: Option<Rc<dyn Fn()>>,
) -> ApprovedConfiguration<Bundle> {
    config_versions(names, [resolver, 1, 1, 1], retired, calls, probe)
}
fn config_versions(
    names: &[&str],
    versions: [u64; 4],
    retired: Rc<Cell<u64>>,
    calls: Rc<Cell<u64>>,
    probe: Option<Rc<dyn Fn()>>,
) -> ApprovedConfiguration<Bundle> {
    let resolver = binding("resolver", versions[0]);
    let dependencies: Vec<_> = ["codec", "policy", "module"]
        .into_iter()
        .zip(versions[1..].iter())
        .map(|(name, version)| binding(name, *version))
        .collect();
    let bindings = dependencies
        .iter()
        .cloned()
        .chain([resolver.clone()])
        .collect();
    ApprovedConfiguration::new(
        permissions(names),
        bindings,
        vec![(
            binding("factory", 1),
            Rc::new(Factory {
                retired,
                calls,
                resolver,
                dependencies,
                probe,
            }),
        )],
    )
    .unwrap()
}

type Setup = (
    Rc<HostSlot<Bundle>>,
    Rc<AuthorityKey>,
    RuntimeCredential,
    Rc<Cell<u64>>,
    Rc<Cell<u64>>,
);
fn setup() -> Setup {
    let retired = Rc::new(Cell::new(0));
    let calls = Rc::new(Cell::new(0));
    let (slot, key, credential) = HostSlot::new(
        Bundle::new(7, retired.clone()),
        config(&["keep", "extra"], 1, retired.clone(), calls.clone()),
        Backend::Serial,
    )
    .unwrap();
    (Rc::new(slot), Rc::new(key), credential, retired, calls)
}
fn error<T>(result: Result<T, Error>, expected: Error) {
    assert_eq!(result.err(), Some(expected));
}
fn current(slot: &HostSlot<Bundle>, key: &AuthorityKey) -> (u64, u64, u64) {
    slot.capture(key, |bundle| Ok(bundle.markers())).unwrap()
}

#[test]
fn entity_free_typed_erased_bundle_capture_is_coherent_and_paused_work_supported() {
    let (slot, key, credential, _, _) = setup();
    slot.with_runtime(&credential, |bundle| {
        bundle.owner.marker = 8;
        error(slot.capture(&key, |_| Ok(())), Error::Busy);
        bundle.service.marker = 8;
        bundle.erased.downcast_mut::<Vec<u64>>().unwrap()[0] = 8;
    })
    .unwrap();
    let snapshot = slot
        .capture(&key, |bundle| {
            assert!(bundle.service.paused);
            error(slot.with_runtime(&credential, |_| ()), Error::Busy);
            Ok(bundle.markers())
        })
        .unwrap();
    assert_eq!(snapshot, (8, 8, 8));
}

#[test]
fn held_guard_and_active_invocation_publication_delivery_block_all_operations_and_retry() {
    let (slot, key, credential, retired, calls) = setup();
    let mut stage = slot
        .prepare_restore(&key, &binding("factory", 1), b"ok")
        .unwrap();
    let guard = slot.try_hold(&key).unwrap();
    error(slot.capture(&key, |_| Ok(())), Error::Busy);
    error(slot.with_runtime(&credential, |_| ()), Error::Busy);
    error(
        slot.prepare_restore(&key, &binding("factory", 1), b"ok"),
        Error::Busy,
    );
    error(slot.activate(&mut stage), Error::Busy);
    error(
        slot.replace_configuration(&key, config(&[], 1, retired.clone(), calls.clone())),
        Error::Busy,
    );
    error(
        slot.rotate_authority(&key, config(&[], 1, retired.clone(), calls.clone())),
        Error::Busy,
    );
    error(slot.issue_credential(&key, permissions(&[])), Error::Busy);
    drop(guard);
    // These three concrete host operations use exactly the same guarded API.
    for _operation in ["invocation", "publication", "delivery"] {
        slot.with_runtime(&credential, |_| {
            error(slot.activate(&mut stage), Error::Busy);
            error(slot.capture(&key, |_| Ok(())), Error::Busy);
            error(
                slot.replace_configuration(&key, config(&[], 1, retired.clone(), calls.clone())),
                Error::Busy,
            );
        })
        .unwrap();
    }
    assert_eq!(current(&slot, &key), (7, 7, 7));
    assert_eq!(
        slot.activate(&mut stage).unwrap().retirement,
        Retirement::Complete
    );
    assert_eq!(current(&slot, &key), (99, 99, 99));
}

#[test]
fn encoder_error_panic_and_unwind_release_guard_preserve_state_and_credentials() {
    let (slot, key, credential, _, _) = setup();
    error(slot.capture::<()>(&key, |_| Err(Error::Busy)), Error::Busy);
    error(
        slot.capture::<()>(&key, |_| Err(Error::FactoryRefused)),
        Error::FactoryRefused,
    );
    error(
        slot.capture::<()>(&key, |_| {
            error(slot.try_hold(&key), Error::Busy);
            panic!("encoder panic")
        }),
        Error::EncoderPanicked,
    );
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = slot.try_hold(&key).unwrap();
        panic!("external unwind");
    }));
    assert!(unwind.is_err());
    assert_eq!(current(&slot, &key), (7, 7, 7));
    slot.with_runtime(&credential, |_| ()).unwrap();
}

#[test]
fn foreign_domain_authority_old_stage_and_old_credentials_refuse_without_exchange() {
    let (slot, key, credential, retired, calls) = setup();
    let (foreign, foreign_key, foreign_credential, _, _) = setup();
    let mut stage = slot
        .prepare_restore(&key, &binding("factory", 1), b"ok")
        .unwrap();
    error(foreign.activate(&mut stage), Error::ForeignDomain);
    error(slot.capture(&foreign_key, |_| Ok(())), Error::ForeignDomain);
    error(
        slot.with_runtime(&foreign_credential, |_| ()),
        Error::ForeignDomain,
    );
    let new_key = slot
        .rotate_authority(&key, config(&["keep"], 1, retired.clone(), calls))
        .unwrap();
    error(slot.activate(&mut stage), Error::ForeignAuthority);
    error(slot.capture(&key, |_| Ok(())), Error::ForeignAuthority);
    error(
        slot.with_runtime(&credential, |_| ()),
        Error::ForeignAuthority,
    );
    assert_eq!(current(&slot, &new_key), (7, 7, 7));
    let mut first = slot
        .prepare_restore(&new_key, &binding("factory", 1), b"ok")
        .unwrap();
    let mut second = slot
        .prepare_restore(&new_key, &binding("factory", 1), b"ok")
        .unwrap();
    let result = slot.activate(&mut first).unwrap();
    error(slot.activate(&mut first), Error::ConsumedStage);
    error(slot.activate(&mut second), Error::StaleStage);
    slot.with_runtime(result.credential(), |_| ()).unwrap();
    assert_eq!(retired.get(), 1);
}

#[test]
fn each_configuration_change_invalidates_stage_and_safe_restage_never_regains_scope() {
    // Revoke, insufficient narrowing, sufficient narrowing, resolver/codec/policy,
    // and factory/module admission changes all go through immutable replacement.
    for change in [
        "revoke",
        "narrow-bad",
        "narrow-safe",
        "resolver",
        "codec",
        "policy",
        "module",
        "factory",
    ] {
        let (slot, key, credential, retired, calls) = setup();
        let mut stage = slot
            .prepare_restore(&key, &binding("factory", 1), b"ok")
            .unwrap();
        let names: &[&str] = match change {
            "revoke" => &[],
            "narrow-bad" => &["extra"],
            _ => &["keep"],
        };
        let next = if change == "factory" {
            ApprovedConfiguration::new(
                permissions(names),
                BTreeSet::from([binding("resolver", 1)]),
                vec![],
            )
            .unwrap()
        } else {
            let mut versions = [1, 1, 1, 1];
            if let Some(index) = ["resolver", "codec", "policy", "module"]
                .iter()
                .position(|name| *name == change)
            {
                versions[index] = 2;
            }
            config_versions(names, versions, retired.clone(), calls.clone(), None)
        };
        slot.replace_configuration(&key, next).unwrap();
        error(slot.activate(&mut stage), Error::AuthorityChanged);
        error(
            slot.with_runtime(&credential, |_| ()),
            Error::AuthorityChanged,
        );
        assert_eq!(current(&slot, &key), (7, 7, 7));
        error(
            slot.issue_credential(&key, permissions(&["keep", "extra"])),
            Error::Unauthorized,
        );
        match change {
            "revoke" | "narrow-bad" => error(
                slot.prepare_restore(&key, &binding("factory", 1), b"ok"),
                Error::Unauthorized,
            ),
            "factory" => error(
                slot.prepare_restore(&key, &binding("factory", 1), b"ok"),
                Error::MissingFactory,
            ),
            _ => {
                let mut restaged = slot
                    .prepare_restore(&key, &binding("factory", 1), b"ok")
                    .unwrap();
                let result = slot.activate(&mut restaged).unwrap();
                slot.with_runtime(result.credential(), |_| ()).unwrap();
                assert_eq!(current(&slot, &key), (99, 99, 99));
                error(
                    slot.prepare_restore(&key, &binding("factory", 1), b"extra"),
                    Error::Unauthorized,
                );
                assert_eq!(current(&slot, &key), (99, 99, 99));
            }
        }
    }
}

#[test]
fn hidden_factory_error_panic_and_ignored_proofs_preserve_nonempty_destination() {
    let (slot, key, credential, retired, calls) = setup();
    for (bytes, expected) in [
        (b"error".as_slice(), Error::FactoryRefused),
        (b"panic", Error::FactoryPanicked),
        (b"ignored-proof", Error::Unauthorized),
        (b"ignored-binding", Error::MissingBinding),
    ] {
        error(
            slot.prepare_restore(&key, &binding("factory", 1), bytes),
            expected,
        );
        assert_eq!(current(&slot, &key), (7, 7, 7));
        slot.with_runtime(&credential, |_| ()).unwrap();
    }
    assert_eq!(calls.get(), 4);
    assert_eq!(retired.get(), 4); // Only hidden allocations were destroyed.
    error(
        slot.prepare_restore(&key, &binding("untrusted-metadata-factory", 1), b"ok"),
        Error::MissingFactory,
    );
    assert_eq!(calls.get(), 4);
}

#[test]
fn hidden_build_reentry_cannot_mutate_capture_update_or_activate() {
    let (slot, key, credential, retired, calls) = setup();
    let pending = RefCell::new(
        slot.prepare_restore(&key, &binding("factory", 1), b"ok")
            .unwrap(),
    );
    let weak_slot = Rc::downgrade(&slot);
    let key_probe = key.clone();
    let retired_probe = retired.clone();
    let calls_probe = calls.clone();
    let probe = Rc::new(move || {
        let slot = weak_slot.upgrade().unwrap();
        error(slot.capture(&key_probe, |_| Ok(())), Error::Busy);
        error(slot.with_runtime(&credential, |_| ()), Error::Busy);
        error(
            slot.replace_configuration(
                &key_probe,
                config(&[], 1, retired_probe.clone(), calls_probe.clone()),
            ),
            Error::Busy,
        );
        error(slot.activate(&mut pending.borrow_mut()), Error::Busy);
    });
    slot.replace_configuration(
        &key,
        config_with_probe(&["keep"], 1, retired, calls, Some(probe)),
    )
    .unwrap();
    let mut built = slot
        .prepare_restore(&key, &binding("factory", 1), b"ok")
        .unwrap();
    slot.activate(&mut built).unwrap();
    assert_eq!(current(&slot, &key), (99, 99, 99));
}

#[test]
fn success_replaces_every_part_once_and_never_reuses_old_stage_or_credentials() {
    let (slot, key, credential, retired, calls) = setup();
    let mut stage = slot
        .prepare_restore(&key, &binding("factory", 1), b"ok")
        .unwrap();
    assert_eq!(current(&slot, &key), (7, 7, 7)); // Hidden build exposed no new part.
    let result = slot.activate(&mut stage).unwrap();
    assert_eq!(current(&slot, &key), (99, 99, 99));
    assert_eq!(calls.get(), 1);
    assert_eq!(retired.get(), 1);
    error(slot.activate(&mut stage), Error::ConsumedStage);
    error(slot.with_runtime(&credential, |_| ()), Error::StaleStage);
    slot.with_runtime(result.credential(), |_| ()).unwrap();
    assert_eq!(retired.get(), 1);
}

#[test]
fn stage_drop_panics_are_contained_and_retirement_panic_is_committed_fail_stop() {
    let (slot, key, credential, retired, _) = setup();
    let hidden = slot
        .prepare_restore(&key, &binding("factory", 1), b"drop-panic")
        .unwrap();
    drop(hidden); // Detached stage quarantine, no destination effects.
    assert_eq!(current(&slot, &key), (7, 7, 7));
    slot.with_runtime(&credential, |bundle| {
        bundle.panic_drop = true;
    })
    .unwrap();
    let mut stage = slot
        .prepare_restore(&key, &binding("factory", 1), b"ok")
        .unwrap();
    let outcome = slot.activate(&mut stage).unwrap();
    assert_eq!(outcome.retirement, Retirement::PanickedFailStop);
    assert_eq!(retired.get(), 2);
    error(slot.capture(&key, |_| Ok(())), Error::FailStop);
    error(
        slot.with_runtime(outcome.credential(), |_| ()),
        Error::FailStop,
    );
    error(slot.activate(&mut stage), Error::FailStop);
}

#[test]
fn mutable_runtime_panic_is_fail_stop_and_threaded_backend_is_unsupported() {
    let (slot, key, credential, retired, calls) = setup();
    error(
        slot.with_runtime::<()>(&credential, |bundle| {
            bundle.owner.marker = 42;
            panic!("mutable panic")
        }),
        Error::RuntimePanicked,
    );
    error(slot.capture(&key, |_| Ok(())), Error::FailStop);
    error(
        HostSlot::new(
            Bundle::new(7, retired.clone()),
            config(&["keep"], 1, retired, calls),
            Backend::Threaded,
        ),
        Error::Unsupported,
    );
}

#[test]
fn old_bundle_retirement_reentry_is_blocked_until_complete_exchange_finishes() {
    let (slot, key, credential, retired, calls) = setup();
    let weak_slot = Rc::downgrade(&slot);
    let key_probe = key.clone();
    let retired_probe = retired.clone();
    let calls_probe = calls.clone();
    let probe_count = Rc::new(Cell::new(0));
    let observed = probe_count.clone();
    slot.with_runtime(&credential, |bundle| {
        bundle.retire_probe = Some(Rc::new(move || {
            let slot = weak_slot.upgrade().unwrap();
            error(
                slot.replace_configuration(
                    &key_probe,
                    config(&[], 1, retired_probe.clone(), calls_probe.clone()),
                ),
                Error::Busy,
            );
            error(slot.capture(&key_probe, |_| Ok(())), Error::Busy);
            observed.set(observed.get() + 1);
        }));
    })
    .unwrap();
    let mut stage = slot
        .prepare_restore(&key, &binding("factory", 1), b"ok")
        .unwrap();
    let result = slot.activate(&mut stage).unwrap();
    assert_eq!(result.retirement, Retirement::Complete);
    assert_eq!(probe_count.get(), 1);
    assert_eq!(retired.get(), 1);
    assert_eq!(current(&slot, &key), (99, 99, 99));
    slot.with_runtime(result.credential(), |_| ()).unwrap();
}

// Immutable factory; counters observe lifetime only, not domain/service effects.
struct ConfigurationDropFactory {
    drops: Rc<Cell<usize>>,
    panic_drop: bool,
    payload_drops: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
}
impl TrustedFactory<()> for ConfigurationDropFactory {
    fn build(&self, _: &mut BuildContext<'_, ()>, _: &[u8]) -> Result<(), Error> {
        Ok(())
    }
}
struct ConfigurationDropPayload(std::sync::Arc<std::sync::atomic::AtomicUsize>);
impl Drop for ConfigurationDropPayload {
    fn drop(&mut self) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        panic!("payload destructor must be quarantined");
    }
}
impl Drop for ConfigurationDropFactory {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
        if let Some(payload_drops) = &self.payload_drops {
            std::panic::panic_any(ConfigurationDropPayload(payload_drops.clone()));
        }
        assert!(!self.panic_drop, "immutable factory destructor panic");
    }
}
fn drop_factory(drops: &Rc<Cell<usize>>, panic_drop: bool) -> Rc<dyn TrustedFactory<()>> {
    Rc::new(ConfigurationDropFactory {
        drops: drops.clone(),
        panic_drop,
        payload_drops: None,
    })
}
fn detached_config(
    factories: Vec<(Binding, Rc<dyn TrustedFactory<()>>)>,
) -> ApprovedConfiguration<()> {
    ApprovedConfiguration::new(BTreeSet::new(), BTreeSet::new(), factories).unwrap()
}

#[test]
fn public_detached_configuration_drop_is_panic_contained() {
    let drops = Rc::new(Cell::new(0));
    let approved = detached_config(vec![(binding("factory", 1), drop_factory(&drops, true))]);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(approved)));
    assert!(
        result.is_ok(),
        "destructor panic escaped configuration ownership"
    );
    assert_eq!(drops.get(), 1);
}

#[test]
fn detached_configuration_normal_and_shared_factory_lifetimes() {
    let drops = Rc::new(Cell::new(0));
    let normal = detached_config(vec![(binding("normal", 1), drop_factory(&drops, false))]);
    drop(normal);
    assert_eq!(drops.get(), 1);
    let shared = drop_factory(&drops, true);
    let weak = Rc::downgrade(&shared);
    let first = detached_config(vec![
        (binding("first", 1), shared.clone()),
        (binding("second", 1), shared.clone()),
    ]);
    let last = detached_config(vec![(binding("last", 1), shared)]);
    drop(first);
    assert_eq!(drops.get(), 1);
    assert_eq!(weak.strong_count(), 1);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(last))).is_ok());
    assert_eq!(drops.get(), 2);
    assert_eq!(weak.strong_count(), 0);
}

#[test]
fn duplicate_configuration_refusal_contains_all_factory_cleanup() {
    let drops = Rc::new(Cell::new(0));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ApprovedConfiguration::new(
            BTreeSet::new(),
            BTreeSet::new(),
            vec![
                (binding("duplicate", 1), drop_factory(&drops, true)),
                (binding("duplicate", 1), drop_factory(&drops, true)),
                (binding("pending", 1), drop_factory(&drops, true)),
            ],
        )
    }));
    error(
        result.expect("duplicate cleanup escaped"),
        Error::DuplicateFactory,
    );
    assert_eq!(
        drops.get(),
        3,
        "displaced, enrolled and unprocessed factories each drop once"
    );
}

#[test]
fn duplicate_configuration_refusal_preserves_caller_shared_reference() {
    let drops = Rc::new(Cell::new(0));
    let shared = drop_factory(&drops, false);
    let weak = Rc::downgrade(&shared);
    error(
        ApprovedConfiguration::new(
            BTreeSet::new(),
            BTreeSet::new(),
            vec![
                (binding("duplicate", 1), shared.clone()),
                (binding("duplicate", 1), shared.clone()),
                (binding("pending", 1), shared.clone()),
            ],
        ),
        Error::DuplicateFactory,
    );
    assert_eq!(drops.get(), 0);
    assert_eq!(weak.strong_count(), 1);
    drop(shared);
    assert_eq!(drops.get(), 1);
}

#[test]
fn caller_retained_naked_rc_drop_is_outside_configuration_boundary() {
    let drops = Rc::new(Cell::new(0));
    let retained = drop_factory(&drops, true);
    drop(detached_config(vec![(
        binding("factory", 1),
        retained.clone(),
    )]));
    assert_eq!(drops.get(), 0);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(retained))).is_err());
    assert_eq!(drops.get(), 1);
}

#[test]
fn configuration_drop_and_refusal_quarantine_panicking_payloads() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    let drops = Rc::new(Cell::new(0));
    let payload_drops = Arc::new(AtomicUsize::new(0));
    let factory = || -> Rc<dyn TrustedFactory<()>> {
        Rc::new(ConfigurationDropFactory {
            drops: drops.clone(),
            panic_drop: false,
            payload_drops: Some(payload_drops.clone()),
        })
    };
    let approved = detached_config(vec![(binding("factory", 1), factory())]);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(approved))).is_ok());
    error(
        ApprovedConfiguration::new(
            BTreeSet::new(),
            BTreeSet::new(),
            vec![
                (binding("duplicate", 1), factory()),
                (binding("duplicate", 1), factory()),
                (binding("pending", 1), factory()),
            ],
        ),
        Error::DuplicateFactory,
    );
    assert_eq!(drops.get(), 4);
    assert_eq!(payload_drops.load(Ordering::SeqCst), 0);
}
