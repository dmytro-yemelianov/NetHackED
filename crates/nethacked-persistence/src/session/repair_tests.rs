#![allow(dead_code, unused_imports)]
include!("../../../nethacked-owner-services/tests/support/persistence.rs");
use crate as nethacked_persistence;
use nethacked_persistence::runtime;

// Required-good regressions adapted from immutable review-A characterization probes.
#[test]
fn pin_lifetime_is_retained_until_capture_finishes() {
    let h = Harness::new(Limits::default(), KernelLimits::default(), "");
    let config =
        runtime::ApprovedConfiguration::new(BTreeSet::new(), BTreeSet::new(), vec![]).unwrap();
    let (slot, key, _) =
        runtime::HostSlot::new(h.kernel, config, runtime::Backend::Serial).unwrap();
    let token = slot.enrollment(&key).unwrap();
    // A preexisting public alias, not an external native payload alias.
    let pin = slot
        .capture(&key, |k| Ok(k.pin_modules(&[h.module]).unwrap()))
        .unwrap();
    slot.capture(&key, |k| {
        k.enroll_session(&token).unwrap();
        Ok(())
    })
    .unwrap();
    slot.capture(&key, |k| {
        let before = k.lifecycle_state();
        drop(pin);
        let after = k.lifecycle_state();
        assert_eq!(before.references, after.references);
        assert_eq!(before.owners, after.owners);
        Ok(())
    })
    .unwrap();
}

#[test]
fn compatible_codec_update_invalidates_runtime_credential() {
    let h = Harness::new(Limits::default(), KernelLimits::default(), "");
    let binding = h.binding();
    let old: Rc<dyn WasmCodec<Vec<u8>>> = Rc::new(Domain {
        add: 1,
        effects: Rc::new(Cell::new(0)),
    });
    let owner = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            bytes(5),
            Backend::Wasm {
                codec: old,
                slot: 0,
                binding: binding.clone(),
            },
        )
        .unwrap();
    let config =
        runtime::ApprovedConfiguration::new(BTreeSet::new(), BTreeSet::new(), vec![]).unwrap();
    let (slot, key, credential) =
        runtime::HostSlot::new((h.kernel, h.host), config, runtime::Backend::Serial).unwrap();
    let token = slot.enrollment(&key).unwrap();
    slot.capture(&key, |(k, host)| {
        k.enroll_session(&token).unwrap();
        host.enroll_session(&token).unwrap();
        Ok(())
    })
    .unwrap();
    let new: Rc<dyn WasmCodec<Vec<u8>>> = Rc::new(Domain {
        add: 100,
        effects: Rc::new(Cell::new(0)),
    });
    slot.with_runtime(&credential, |(k, _)| {
        owner.admit_compatible_wasm(k, [1; 32], new, 0, binding)
    })
    .unwrap()
    .unwrap();
    assert_eq!(
        slot.with_runtime(&credential, |_| ()).err(),
        Some(runtime::Error::AuthorityChanged)
    );
}

#[test]
fn candidate_discard_during_capture_defers_and_releases_accounting() {
    let mut h = Harness::new(Limits::default(), KernelLimits::default(), "");
    let owner = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            1_u64,
            Backend::Native(Rc::new(NativeDomain)),
        )
        .unwrap();
    h.admit(&owner);
    h.refresh();
    let held = Rc::new(RefCell::new(None));
    let out = held.clone();
    *h.action.borrow_mut() = Some(Box::new(move |view| {
        *out.borrow_mut() = Some(owner.prepare("prepare", &[], vec![], view).unwrap());
        Ok(vec![])
    }));
    h.kernel
        .invoke_operation(
            &h.operation,
            &[1],
            Authority {
                access: Access::Write,
                scope: Scope::All,
            },
        )
        .unwrap();
    assert_eq!(h.host.pending().unwrap(), 1);
    let config =
        runtime::ApprovedConfiguration::new(BTreeSet::new(), BTreeSet::new(), vec![]).unwrap();
    let (slot, key, credential) =
        runtime::HostSlot::new((h.kernel, h.host), config, runtime::Backend::Serial).unwrap();
    let token = slot.enrollment(&key).unwrap();
    slot.capture(&key, |(k, host)| {
        k.enroll_session(&token).unwrap();
        host.enroll_session(&token).unwrap();
        Ok(())
    })
    .unwrap();
    slot.capture(&key, |(_, host)| {
        held.borrow_mut().take().unwrap().discard();
        assert_eq!(host.pending().err(), Some(Error::Busy));
        Ok(())
    })
    .unwrap();
    let leaked = slot
        .with_runtime(&credential, |(_, host)| host.pending().unwrap())
        .unwrap();
    assert_eq!(leaked, 0);
}

struct PinDroppingEncoder {
    pin: RefCell<Option<nethacked_kernel::ModulePin>>,
    kernel: RefCell<Option<Rc<RefCell<Kernel>>>>,
    mode: Cell<u8>,
}
impl PersistenceCodec<u64> for PinDroppingEncoder {
    fn identity(&self) -> CodecIdentity {
        identity(4)
    }
    fn encode_state(&self, s: &u64, limit: usize) -> Result<Vec<u8>> {
        let kernel = self.kernel.borrow();
        assert!(kernel.as_ref().unwrap().try_borrow_mut().is_err());
        drop(self.pin.borrow_mut().take());
        match self.mode.get() {
            1 => return Err(Error::Invariant),
            2 | 3 => panic!("encoder unwind after last pin alias release"),
            _ => (),
        }
        NativeDomain.encode_state(s, limit)
    }
    fn decode_state(&self, b: &[u8], limit: usize, r: &Remap) -> Result<u64> {
        NativeDomain.decode_state(b, limit, r)
    }
    fn validate_state(&self, s: &u64) -> Result<()> {
        NativeDomain.validate_state(s)
    }
    fn references(&self, s: &u64) -> Result<Vec<StateReference>> {
        NativeDomain.references(s)
    }
    fn validate_configuration(
        &self,
        s: &u64,
        p: &BTreeMap<String, Port>,
        q: &QueryPolicy,
    ) -> Result<()> {
        NativeDomain.validate_configuration(s, p, q)
    }
}
#[test]
fn concrete_session_encoder_retains_pin_then_releases_at_success_error_and_unwind() {
    for mode in 0..4 {
        concrete_pin_capture(mode);
    }
}
fn concrete_pin_capture(mode: u8) {
    use nethacked_persistence::{session as composed, SaveLimits};
    let h = Harness::new(Limits::default(), KernelLimits::default(), "");
    let backend: Rc<dyn NativeBehavior<u64>> = Rc::new(NativeDomain);
    let owner = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            7,
            Backend::Native(backend.clone()),
        )
        .unwrap();
    let external_pin = h.kernel.pin_modules(&[h.module]).unwrap();
    let encoder = Rc::new(PinDroppingEncoder {
        pin: RefCell::new(Some(external_pin)),
        kernel: RefCell::new(None),
        mode: Cell::new(mode),
    });
    let mut catalog = CaptureCatalog::default();
    catalog
        .register(
            identity(4),
            QueryPolicy::Noncacheable,
            Backend::Native(backend),
        )
        .unwrap();
    let allocations: Vec<Rc<dyn composed::OwnerAllocation>> = vec![Rc::new(composed::TypedOwner {
        key: "native".into(),
        owner,
        codec: encoder.clone(),
        catalog,
        retained: vec![],
        callbacks: vec![],
        target_port: None,
        candidates: vec![],
        batch_candidates: vec![],
    })];
    let config =
        runtime::ApprovedConfiguration::new(BTreeSet::new(), BTreeSet::new(), vec![]).unwrap();
    let (session, key, credential) = composed::Session::new(
        h.kernel,
        h.host,
        allocations,
        vec![],
        AccountingCaptureCatalog::default(),
        Default::default(),
        config,
    )
    .unwrap();
    let before = session
        .with_runtime(&credential, |b| b.kernel().unwrap().lifecycle_state())
        .unwrap();
    session
        .slot
        .capture(&key, |b| {
            *encoder.kernel.borrow_mut() = Some(b.kernel.clone());
            Ok(())
        })
        .unwrap();
    let captured = if mode == 3 {
        session.capture(&key, SaveLimits::default())
    } else {
        session.slot.capture(&key, |b| {
            let captured = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                b.save(session.slot.capture_epoch(), SaveLimits::default())
            }))
            .unwrap_or(Err(runtime::Error::EncoderPanicked));
            assert_eq!(b.kernel().unwrap().lifecycle_state(), before);
            assert!(encoder.pin.borrow().is_none());
            assert_eq!(
                b.update_kernel(|_| panic!("capture must refuse update"))
                    .err(),
                Some(runtime::Error::Busy)
            );
            captured
        })
    };
    if mode != 0 {
        assert_eq!(
            captured.err(),
            Some(if mode == 1 {
                runtime::Error::FactoryRefused
            } else {
                runtime::Error::EncoderPanicked
            })
        );
        let after = session
            .with_runtime(&credential, |b| b.kernel().unwrap().lifecycle_state())
            .unwrap();
        assert_eq!(before.references, after.references + 1);
        assert_eq!(before.owners, after.owners + 1);
        assert!(encoder.pin.borrow().is_none());
        encoder.mode.set(0);
        assert!(session.capture(&key, SaveLimits::default()).is_ok());
        return;
    }
    let wire = captured.unwrap();
    let after = session
        .with_runtime(&credential, |b| b.kernel().unwrap().lifecycle_state())
        .unwrap();
    assert_eq!(before.references, after.references + 1);
    assert_eq!(before.owners, after.owners + 1);
    let value: serde_json::Value = serde_json::from_slice(&wire[52..]).unwrap();
    let hex = value["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "kernel")
        .unwrap()["local_payload"]
        .as_str()
        .unwrap();
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    let snapshot: KernelSnapshot = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(snapshot.pins.len(), before.owners);
    assert_eq!(snapshot.pins.len(), after.owners + 1);
}

type CompleteBundle = (Kernel, Host, Rc<Owner<Vec<u8>>>);
struct CompleteFactory;
fn complete_factory_id() -> runtime::Binding {
    runtime::Binding {
        name: "fresh-complete".into(),
        version: 1,
        artifact: [42; 32],
    }
}
impl runtime::TrustedFactory<CompleteBundle> for CompleteFactory {
    fn build(
        &self,
        c: &mut runtime::BuildContext<'_, CompleteBundle>,
        _: &[u8],
    ) -> std::result::Result<CompleteBundle, runtime::Error> {
        c.require_binding(&complete_factory_id())?;
        let h = Harness::new(Limits::default(), KernelLimits::default(), "");
        let owner = h
            .host
            .enroll(
                &h.kernel,
                h.spec(1, vec![]),
                bytes(5),
                Backend::Wasm {
                    codec: Rc::new(Domain {
                        add: 1,
                        effects: Rc::new(Cell::new(0)),
                    }),
                    slot: 0,
                    binding: h.binding(),
                },
            )
            .unwrap();
        h.kernel.enroll_session(c.enrollment()?).unwrap();
        h.host.enroll_session(c.enrollment()?).unwrap();
        Ok((h.kernel, h.host, owner))
    }
}
#[test]
fn ready_stage_refuses_changed_owner_codec_epoch() {
    let h = Harness::new(Limits::default(), KernelLimits::default(), "");
    let binding = h.binding();
    let owner = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            bytes(5),
            Backend::Wasm {
                codec: Rc::new(Domain {
                    add: 1,
                    effects: Rc::new(Cell::new(0)),
                }),
                slot: 0,
                binding: binding.clone(),
            },
        )
        .unwrap();
    let id = complete_factory_id();
    let config = runtime::ApprovedConfiguration::new(
        BTreeSet::new(),
        [id.clone()].into(),
        vec![(id.clone(), Rc::new(CompleteFactory))],
    )
    .unwrap();
    let (slot, key, credential) = runtime::HostSlot::new(
        (h.kernel, h.host, owner.clone()),
        config,
        runtime::Backend::Serial,
    )
    .unwrap();
    let token = slot.enrollment(&key).unwrap();
    slot.capture(&key, |(k, host, _)| {
        k.enroll_session(&token).unwrap();
        host.enroll_session(&token).unwrap();
        Ok(())
    })
    .unwrap();
    let mut stage = slot.prepare_restore(&key, &id, &[]).unwrap();
    slot.with_runtime(&credential, |(k, _, o)| {
        o.admit_compatible_wasm(
            k,
            [1; 32],
            Rc::new(Domain {
                add: 100,
                effects: Rc::new(Cell::new(0)),
            }),
            0,
            binding,
        )
    })
    .unwrap()
    .unwrap();
    assert_eq!(
        slot.with_runtime(&credential, |_| ()).err(),
        Some(runtime::Error::AuthorityChanged)
    );
    assert_eq!(
        slot.activate(&mut stage).err(),
        Some(runtime::Error::AuthorityChanged)
    );
    assert!(owner.stamp().is_ok());
}

#[test]
fn refused_kernel_borrow_preserves_epoch_without_callback() {
    use nethacked_persistence::session as composed;
    let config =
        runtime::ApprovedConfiguration::new(BTreeSet::new(), BTreeSet::new(), vec![]).unwrap();
    let (session, _, credential) = composed::Session::new(
        Kernel::new(KernelLimits::default()).unwrap(),
        Host::new(Limits::default()).unwrap(),
        vec![],
        vec![],
        AccountingCaptureCatalog::default(),
        Default::default(),
        config,
    )
    .unwrap();
    let calls = Cell::new(0);
    session
        .with_runtime(&credential, |bundle| {
            let borrowed = bundle.kernel().unwrap();
            assert_eq!(
                bundle.update_kernel(|_| calls.set(calls.get() + 1)).err(),
                Some(runtime::Error::Busy)
            );
            assert_eq!(calls.get(), 0);
            drop(borrowed);
        })
        .unwrap();
    assert!(session.with_runtime(&credential, |_| ()).is_ok());
}

struct RefusingCodec;
impl Schema<Vec<u8>> for RefusingCodec {
    fn validate(&self, _: &Vec<u8>) -> Result<()> {
        Err(Error::Invariant)
    }
    fn encode(&self, _: &Vec<u8>) -> Result<Vec<u8>> {
        Err(Error::Invariant)
    }
    fn query(&self, _: &Vec<u8>, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        Err(Error::Invariant)
    }
}
impl WasmCodec<Vec<u8>> for RefusingCodec {
    fn input(&self, _: &Vec<u8>, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        Err(Error::Invariant)
    }
    fn decode(&self, _: &[u8]) -> Result<Proposal<Vec<u8>>> {
        Err(Error::Invariant)
    }
}
#[test]
fn failed_codec_admission_preserves_state_effects_pins_credential_and_ready_stage() {
    let h = Harness::new(Limits::default(), KernelLimits::default(), "");
    let binding = h.binding();
    let effects = Rc::new(Cell::new(0));
    let owner = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            bytes(5),
            Backend::Wasm {
                codec: Rc::new(Domain {
                    add: 1,
                    effects: effects.clone(),
                }),
                slot: 0,
                binding: binding.clone(),
            },
        )
        .unwrap();
    let id = complete_factory_id();
    let config = runtime::ApprovedConfiguration::new(
        BTreeSet::new(),
        [id.clone()].into(),
        vec![(id.clone(), Rc::new(CompleteFactory))],
    )
    .unwrap();
    let (slot, key, credential) = runtime::HostSlot::new(
        (h.kernel, h.host, owner.clone()),
        config,
        runtime::Backend::Serial,
    )
    .unwrap();
    let token = slot.enrollment(&key).unwrap();
    slot.capture(&key, |(k, host, _)| {
        k.enroll_session(&token).unwrap();
        host.enroll_session(&token).unwrap();
        Ok(())
    })
    .unwrap();
    let mut stage = slot.prepare_restore(&key, &id, &[]).unwrap();
    let before = slot
        .with_runtime(&credential, |(k, host, o)| {
            (
                k.lifecycle_state(),
                host.pending().unwrap(),
                o.inspect().unwrap(),
            )
        })
        .unwrap();
    for case in 0..4 {
        slot.with_runtime(&credential, |(k, _, o)| {
            let codec: Rc<dyn WasmCodec<Vec<u8>>> = if case == 2 {
                Rc::new(RefusingCodec)
            } else {
                Rc::new(Domain {
                    add: 100,
                    effects: effects.clone(),
                })
            };
            let mut denied = binding.clone();
            if case == 3 {
                denied.resolution.clear();
            }
            assert!(o
                .admit_compatible_wasm(
                    k,
                    if case == 0 { [2; 32] } else { [1; 32] },
                    codec,
                    if case == 1 { 1 } else { 0 },
                    denied
                )
                .is_err());
        })
        .unwrap();
        assert_eq!(
            slot.with_runtime(&credential, |(k, host, o)| (
                k.lifecycle_state(),
                host.pending().unwrap(),
                o.inspect().unwrap()
            ))
            .unwrap(),
            before
        );
        assert_eq!(effects.get(), 0);
    }
    assert!(slot.activate(&mut stage).is_ok());
}
struct EmptyBundleFactory;
impl runtime::TrustedFactory<super::HostBundle> for EmptyBundleFactory {
    fn build(
        &self,
        c: &mut runtime::BuildContext<'_, super::HostBundle>,
        _: &[u8],
    ) -> std::result::Result<super::HostBundle, runtime::Error> {
        c.require_binding(&complete_factory_id())?;
        let bundle = super::HostBundle::new(
            Kernel::new(KernelLimits::default()).unwrap(),
            Host::new(Limits::default()).unwrap(),
            vec![],
            vec![],
            AccountingCaptureCatalog::default(),
            Default::default(),
        );
        bundle.enroll(c.enrollment()?)?;
        Ok(bundle)
    }
}
#[test]
fn busy_kernel_update_preserves_complete_capture_and_ready_stage() {
    let id = complete_factory_id();
    let config = runtime::ApprovedConfiguration::new(
        BTreeSet::new(),
        [id.clone()].into(),
        vec![(id.clone(), Rc::new(EmptyBundleFactory))],
    )
    .unwrap();
    let (session, key, credential) = super::Session::new(
        Kernel::new(KernelLimits::default()).unwrap(),
        Host::new(Limits::default()).unwrap(),
        vec![],
        vec![],
        AccountingCaptureCatalog::default(),
        Default::default(),
        config,
    )
    .unwrap();
    let mut stage = session.slot.prepare_restore(&key, &id, &[]).unwrap();
    let before = session.capture(&key, crate::SaveLimits::default()).unwrap();
    let calls = Cell::new(0);
    session
        .with_runtime(&credential, |bundle| {
            let kernel = bundle.kernel().unwrap();
            assert_eq!(
                bundle.update_kernel(|_| calls.set(calls.get() + 1)).err(),
                Some(runtime::Error::Busy)
            );
            assert_eq!(calls.get(), 0);
            drop(kernel);
        })
        .unwrap();
    assert!(session.with_runtime(&credential, |_| ()).is_ok());
    let after = session.capture(&key, crate::SaveLimits::default()).unwrap();
    let mut before: super::Wire = serde_json::from_slice(&before[52..]).unwrap();
    let mut after: super::Wire = serde_json::from_slice(&after[52..]).unwrap();
    // Cleanup identities are process-wide: parallel unrelated tests can advance
    // the allocator watermark. All bundle state and its epoch must be identical.
    let before_kernel = before
        .sections
        .iter_mut()
        .find(|s| s.name == "kernel")
        .unwrap();
    let after_kernel = after
        .sections
        .iter_mut()
        .find(|s| s.name == "kernel")
        .unwrap();
    let old: KernelSnapshot = serde_json::from_slice(
        &before_kernel
            .local_payload
            .bytes(crate::SaveLimits::default())
            .unwrap(),
    )
    .unwrap();
    let mut new: KernelSnapshot = serde_json::from_slice(
        &after_kernel
            .local_payload
            .bytes(crate::SaveLimits::default())
            .unwrap(),
    )
    .unwrap();
    assert!(new.cleanup_high_water >= old.cleanup_high_water);
    new.cleanup_high_water = old.cleanup_high_water;
    after_kernel.local_payload = super::Hex::from_bytes(&super::canonical(&new).unwrap());
    assert_eq!(
        super::canonical(&after).unwrap(),
        super::canonical(&before).unwrap()
    );
    assert!(session.slot.activate(&mut stage).is_ok());
}
#[test]
fn kernel_update_replacement_enrolls_and_mutating_panic_fail_stops() {
    let config =
        runtime::ApprovedConfiguration::new(BTreeSet::new(), BTreeSet::new(), vec![]).unwrap();
    let (session, key, credential) = super::Session::new(
        Kernel::new(KernelLimits::default()).unwrap(),
        Host::new(Limits::default()).unwrap(),
        vec![],
        vec![],
        AccountingCaptureCatalog::default(),
        Default::default(),
        config,
    )
    .unwrap();
    session
        .with_runtime(&credential, |b| {
            b.update_kernel(|k| *k = Kernel::new(KernelLimits::default()).unwrap())
        })
        .unwrap()
        .unwrap();
    assert_eq!(
        session.with_runtime(&credential, |_| ()).err(),
        Some(runtime::Error::AuthorityChanged)
    );
    let credential = session.issue_credential(&key).unwrap();
    session
        .slot
        .capture(&key, |b| {
            assert_eq!(
                b.kernel().unwrap().pin_modules(&[]).err(),
                Some(nethacked_kernel::Error::Busy)
            );
            Ok(())
        })
        .unwrap();
    assert_eq!(
        session
            .with_runtime(&credential, |b| b
                .update_kernel::<()>(|_| panic!("mutating kernel callback")))
            .err(),
        Some(runtime::Error::RuntimePanicked)
    );
    assert_eq!(
        session.with_runtime(&credential, |_| ()).err(),
        Some(runtime::Error::FailStop)
    );
}

#[test]
fn sibling_owner_and_service_authority_updates_invalidate_only_after_admission() {
    for case in 0..7 {
        let mut h = Harness::new(Limits::default(), KernelLimits::default(), "");
        let owner = h
            .host
            .enroll(
                &h.kernel,
                h.spec(1, vec![]),
                bytes(5),
                Backend::Wasm {
                    codec: Rc::new(Domain {
                        add: 1,
                        effects: Rc::new(Cell::new(0)),
                    }),
                    slot: 0,
                    binding: h.binding(),
                },
            )
            .unwrap();
        let spec = h.spec(1, vec![]);
        let foreign = Host::new(Limits::default())
            .unwrap()
            .enroll(
                &h.kernel,
                spec.clone(),
                1_u64,
                Backend::Native(Rc::new(NativeDomain)),
            )
            .unwrap()
            .id();
        let targets = nethacked_owner_services::Targets::new(8).unwrap();
        let callbacks = nethacked_owner_services::CallbackBehaviors::new(1).unwrap();
        let address = targets.register(&owner, "prepare").unwrap().address;
        let retained = Rc::new(RefCell::new(None));
        if case == 6 {
            h.admit(&owner);
            h.refresh();
            h.run(1, {
                let owner = owner.clone();
                let retained = retained.clone();
                move |view| {
                    *retained.borrow_mut() = Some(owner.retain_behavior("prepare", view).unwrap());
                    Ok(vec![])
                }
            });
        }
        let id = complete_factory_id();
        let config = runtime::ApprovedConfiguration::new(
            BTreeSet::new(),
            [id.clone()].into(),
            vec![(id.clone(), Rc::new(CompleteFactory))],
        )
        .unwrap();
        let (slot, key, credential) = runtime::HostSlot::new(
            (h.kernel, h.host, owner.clone()),
            config,
            runtime::Backend::Serial,
        )
        .unwrap();
        let token = slot.enrollment(&key).unwrap();
        slot.capture(&key, |(k, host, _)| {
            k.enroll_session(&token).unwrap();
            host.enroll_session(&token).unwrap();
            Ok(())
        })
        .unwrap();
        targets.enroll_session(&token).unwrap();
        callbacks.enroll_session(&token).unwrap();
        let mut stage = slot.prepare_restore(&key, &id, &[]).unwrap();
        if case != 6 {
            slot.with_runtime(&credential, |(k, host, o)| match case {
                0 => {
                    let mut bad = spec.clone();
                    bad.ports.clear();
                    assert!(host
                        .enroll(k, bad, 1_u64, Backend::Native(Rc::new(NativeDomain)))
                        .is_err());
                }
                1 => assert!(host.retire(foreign).is_err()),
                2 => {
                    let bad = PublicationLimits {
                        transactions: 0,
                        ..PublicationLimits::default()
                    };
                    assert!(host.configure_publication(bad).is_err());
                }
                3 => assert!(o.declare_recorded_inputs("missing", vec![]).is_err()),
                4 => assert!(targets.register(o, "prepare").is_err()),
                5 => {
                    let mut bad = address.clone();
                    bad.key += "missing";
                    assert!(targets.remove(&bad).is_err());
                }
                _ => unreachable!(),
            })
            .unwrap();
            assert!(slot.with_runtime(&credential, |_| ()).is_ok());
        }
        slot.with_runtime(&credential, |(k, host, o)| match case {
            0 => {
                host.enroll(k, spec, 1_u64, Backend::Native(Rc::new(NativeDomain)))
                    .unwrap();
            }
            1 => host.retire(o.id()).unwrap(),
            2 => host
                .configure_publication(PublicationLimits::default())
                .unwrap(),
            3 => o.declare_recorded_inputs("prepare", vec![]).unwrap(),
            4 => {
                targets.register(o, "derived").unwrap();
            }
            5 => targets.remove(&address).unwrap(),
            6 => {
                callbacks
                    .register(retained.borrow_mut().take().unwrap())
                    .unwrap();
            }
            _ => unreachable!(),
        })
        .unwrap();
        assert_eq!(
            slot.with_runtime(&credential, |_| ()).err(),
            Some(runtime::Error::AuthorityChanged)
        );
        assert_eq!(
            slot.activate(&mut stage).err(),
            Some(runtime::Error::AuthorityChanged)
        );
    }
}

type ResidualSession = (
    super::Session,
    runtime::AuthorityKey,
    runtime::RuntimeCredential,
    OwnerSpec,
    CapabilityPermit,
    Rc<Cell<usize>>,
);

fn residual_populated_session(host_limits: Limits, pin_owners: Option<usize>) -> ResidualSession {
    NATIVE_EXECUTIONS.with(|n| n.set(0));
    let mut h = Harness::new(host_limits, KernelLimits::default(), "");
    if let Some(owners) = pin_owners {
        h.kernel.release_pipeline(h.lease).unwrap();
        h.kernel
            .set_retention_limits(nethacked_kernel::RetentionLimits {
                owners,
                ..Default::default()
            })
            .unwrap();
    }
    let spec = h.spec(1, vec![]);
    let binding = h.binding();
    let backend: Rc<dyn NativeBehavior<u64>> = Rc::new(NativeDomain);
    let owner = h
        .host
        .enroll(
            &h.kernel,
            spec.clone(),
            7u64,
            Backend::Native(backend.clone()),
        )
        .unwrap();
    let mut catalog = CaptureCatalog::default();
    catalog
        .register(
            identity(4),
            QueryPolicy::Noncacheable,
            Backend::Native(backend),
        )
        .unwrap();
    let allocation = Rc::new(super::TypedOwner {
        key: "native".into(),
        owner,
        codec: Rc::new(NativeDomain),
        catalog,
        retained: vec![],
        callbacks: vec![],
        target_port: None,
        candidates: vec![],
        batch_candidates: vec![],
    });
    let id = complete_factory_id();
    let config = runtime::ApprovedConfiguration::new(
        BTreeSet::new(),
        [id.clone()].into(),
        vec![(id, Rc::new(EmptyBundleFactory))],
    )
    .unwrap();
    let (session, key, credential) = super::Session::new(
        h.kernel,
        h.host,
        vec![allocation],
        vec![],
        AccountingCaptureCatalog::default(),
        Default::default(),
        config,
    )
    .unwrap();
    (
        session,
        key,
        credential,
        spec,
        binding,
        Rc::new(Cell::new(0)),
    )
}

#[test]
fn residual_owner_and_aggregate_pin_refusals_preserve_full_capture_and_ready_stage() {
    for aggregate in [false, true] {
        let (session, key, credential, spec, binding, effects) = residual_populated_session(
            Limits {
                owners: if aggregate { 8 } else { 1 },
                ..Default::default()
            },
            if aggregate { Some(2) } else { None },
        );
        let mut stage = session
            .slot
            .prepare_restore(&key, &complete_factory_id(), &[])
            .unwrap();
        let before = session.capture(&key, crate::SaveLimits::default()).unwrap();
        session
            .with_runtime(&credential, |b| {
                let kernel = b.kernel().unwrap();
                if aggregate {
                    assert_eq!(
                        b.host()
                            .enroll(
                                &kernel,
                                spec,
                                bytes(9),
                                Backend::Wasm {
                                    codec: Rc::new(Domain {
                                        add: 1,
                                        effects: effects.clone()
                                    }),
                                    slot: 0,
                                    binding
                                }
                            )
                            .err(),
                        Some(Error::Kernel(nethacked_kernel::Error::Limit))
                    );
                } else {
                    assert_eq!(
                        b.host()
                            .enroll(&kernel, spec, 9u64, Backend::Native(Rc::new(NativeDomain)))
                            .err(),
                        Some(Error::Limit)
                    );
                }
            })
            .unwrap();
        assert!(session.with_runtime(&credential, |_| ()).is_ok());
        let after = session.capture(&key, crate::SaveLimits::default()).unwrap();
        assert_complete_capture_equal(&before, &after);
        assert_eq!(effects.get(), 0);
        NATIVE_EXECUTIONS.with(|n| assert_eq!(n.get(), 0));
        assert!(session.slot.activate(&mut stage).is_ok());
    }
}

fn assert_complete_capture_equal(before: &[u8], after: &[u8]) {
    let old: super::Wire = serde_json::from_slice(&before[52..]).unwrap();
    let mut new: super::Wire = serde_json::from_slice(&after[52..]).unwrap();
    let old_section = old.sections.iter().find(|s| s.name == "kernel").unwrap();
    let new_section = new
        .sections
        .iter_mut()
        .find(|s| s.name == "kernel")
        .unwrap();
    let old_kernel: KernelSnapshot = serde_json::from_slice(
        &old_section
            .local_payload
            .bytes(crate::SaveLimits::default())
            .unwrap(),
    )
    .unwrap();
    let mut new_kernel: KernelSnapshot = serde_json::from_slice(
        &new_section
            .local_payload
            .bytes(crate::SaveLimits::default())
            .unwrap(),
    )
    .unwrap();
    assert!(new_kernel.cleanup_high_water >= old_kernel.cleanup_high_water);
    new_kernel.cleanup_high_water = old_kernel.cleanup_high_water;
    new_section.local_payload = super::Hex::from_bytes(&super::canonical(&new_kernel).unwrap());
    assert_eq!(
        super::canonical(&old).unwrap(),
        super::canonical(&new).unwrap()
    );
}

#[test]
fn residual_foreign_replacement_and_locally_caught_panic_fail_stop_populated_destination() {
    for panic in [false, true] {
        let (session, key, credential, _, _, effects) =
            residual_populated_session(Limits::default(), None);
        let mut stage = session
            .slot
            .prepare_restore(&key, &complete_factory_id(), &[])
            .unwrap();
        assert!(!session
            .capture(&key, crate::SaveLimits::default())
            .unwrap()
            .is_empty());
        let foreign = Rc::new(nethacked_kernel::session::Domain::new());
        let replacement = Kernel::new(KernelLimits::default()).unwrap();
        if !panic {
            replacement.enroll_session(&foreign.enrollment(1)).unwrap();
        }
        session
            .with_runtime(&credential, |b| {
                assert_eq!(
                    b.owner::<u64>("native").unwrap().owner.inspect().unwrap(),
                    7u64.to_le_bytes()
                );
                let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    b.update_kernel(|k| {
                        *k = replacement;
                        if panic {
                            panic!("locally caught replacement panic");
                        }
                    })
                }));
                if panic {
                    assert!(caught.is_err());
                } else {
                    assert_eq!(caught.unwrap().err(), Some(runtime::Error::FactoryRefused));
                }
                assert_eq!(b.kernel().err(), Some(runtime::Error::FailStop));
            })
            .unwrap();
        assert_eq!(
            session.capture(&key, crate::SaveLimits::default()).err(),
            Some(runtime::Error::FailStop)
        );
        assert_eq!(
            session.with_runtime(&credential, |_| ()).err(),
            Some(runtime::Error::FailStop)
        );
        assert_eq!(
            session.issue_credential(&key).err(),
            Some(runtime::Error::FailStop)
        );
        assert_eq!(
            session.slot.activate(&mut stage).err(),
            Some(runtime::Error::FailStop)
        );
        assert_eq!(effects.get(), 0);
        NATIVE_EXECUTIONS.with(|n| assert_eq!(n.get(), 0));
        assert!(foreign.runtime().is_ok());
    }
}
