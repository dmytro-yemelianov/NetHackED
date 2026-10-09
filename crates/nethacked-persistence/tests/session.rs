#![cfg(feature = "session")]
#![allow(dead_code, unused_imports)]
// Reuse the accepted native, blob and full Wasm helper/import fixtures verbatim.
include!("../../nethacked-owner-services/tests/support/persistence.rs");
use nethacked_persistence::{runtime, session as composed, SaveLimits};

struct CompositionResolver {
    materials: BTreeMap<[u8; 32], Material>,
    handles: Rc<composed::Handles>,
    factories: Rc<Cell<usize>>,
    approved: KernelSnapshot,
}
impl RegistryAuthority for CompositionResolver {
    fn resolve(&self, m: &SavedModule) -> nethacked_kernel::Result<ArtifactMaterial> {
        self.factories.set(self.factories.get() + 1);
        match self
            .materials
            .get(&m.artifact)
            .ok_or(nethacked_kernel::Error::Authority)?
        {
            Material::Guest(manifest, bytes) => Ok(ArtifactMaterial::Guest {
                manifest: manifest.clone(),
                bytes: bytes.clone(),
            }),
            Material::Native(manifest, identity) => {
                let ports = manifest
                    .exports
                    .keys()
                    .map(|name| {
                        let handles = self.handles.clone();
                        let endpoint = name.clone();
                        let port: NativePort = Rc::new(move |_: &[u8], view: &CallerView| {
                            if endpoint == "prepare" {
                                handles
                                    .commit_prepared(view)
                                    .map_err(|_| nethacked_kernel::Error::NativeRefusal)?;
                                Ok(vec![])
                            } else {
                                handles
                                    .owner::<u64>("native")?
                                    .query("query", &[], &[], view)
                                    .map(|o| o.bytes().to_vec())
                                    .map_err(|_| nethacked_kernel::Error::NativeRefusal)
                            }
                        });
                        (name.clone(), port)
                    })
                    .collect();
                Ok(ArtifactMaterial::Native {
                    manifest: manifest.clone(),
                    identity: identity.clone(),
                    ports,
                })
            }
        }
    }
    fn bind_resource(&self, r: &Resource) -> nethacked_kernel::Result<Resource> {
        if self
            .approved
            .registry
            .grants
            .iter()
            .any(|g| match &g.requested.scope {
                Scope::Keys(k) => k.contains(r),
                Scope::All => false,
            })
        {
            Ok(r.clone())
        } else {
            Err(nethacked_kernel::Error::Authority)
        }
    }
    fn approve_grant(
        &self,
        caller: &SavedModule,
        slot: usize,
        target: &SavedModule,
        a: &Authority,
    ) -> nethacked_kernel::Result<Authority> {
        let grant = self
            .approved
            .registry
            .grants
            .iter()
            .find(|g| g.caller == caller.id && g.slot == slot && g.target == target.id && g.active)
            .ok_or(nethacked_kernel::Error::Authority)?;
        if grant.requested.access > a.access {
            return Err(nethacked_kernel::Error::Authority);
        }
        Ok(grant.requested.clone())
    }
}
impl RetainedAuthority for CompositionResolver {
    fn approve_cleanup(&self, id: u64) -> nethacked_kernel::Result<()> {
        if self.approved.cleanup_history.contains(&id) {
            Ok(())
        } else {
            Err(nethacked_kernel::Error::Authority)
        }
    }
    fn approve_operation_context(&self, c: &SavedOperationContext) -> nethacked_kernel::Result<()> {
        if self.approved.operation_contexts()?.contains(c) {
            Ok(())
        } else {
            Err(nethacked_kernel::Error::Authority)
        }
    }
}
struct CompositionKernel {
    materials: BTreeMap<[u8; 32], Material>,
    factories: Rc<Cell<usize>>,
    approved: KernelSnapshot,
}
impl composed::KernelFactory for CompositionKernel {
    fn resolver(
        &self,
        handles: Rc<composed::Handles>,
        ports: BTreeMap<(String, String), NativePort>,
    ) -> std::result::Result<Rc<dyn RetainedAuthority>, runtime::Error> {
        assert!(ports.is_empty());
        Ok(Rc::new(CompositionResolver {
            materials: self.materials.clone(),
            handles,
            factories: self.factories.clone(),
            approved: self.approved.clone(),
        }))
    }
}
struct AnyAccounting;
impl Accounting for AnyAccounting {
    fn validate(&self, p: &[ProposalView], r: &[Observation]) -> Result<()> {
        if p.len() == 2 && r.len() == 1 {
            Ok(())
        } else {
            Err(Error::Invariant)
        }
    }
}
fn accounting_identity() -> ArtifactIdentity {
    ArtifactIdentity {
        name: "combined".into(),
        version: 1,
        artifact: [11; 32],
    }
}
fn composer_identity() -> runtime::Binding {
    runtime::Binding {
        name: "composition".into(),
        version: 1,
        artifact: [14; 32],
    }
}
fn composition_config(
    materials: BTreeMap<[u8; 32], Material>,
    binding: CapabilityPermit,
    ids: Vec<u64>,
    effects: Rc<Cell<usize>>,
    factories: Rc<Cell<usize>>,
    approved: KernelSnapshot,
) -> runtime::ApprovedConfiguration<composed::HostBundle> {
    composed::SessionFactory::new(composition_authority(
        materials, binding, ids, effects, factories, approved,
    ))
    .configuration(composer_identity())
    .unwrap()
}
fn composition_authority(
    materials: BTreeMap<[u8; 32], Material>,
    binding: CapabilityPermit,
    ids: Vec<u64>,
    effects: Rc<Cell<usize>>,
    factories: Rc<Cell<usize>>,
    approved: KernelSnapshot,
) -> composed::RestoreAuthority {
    let mut native = RestoreCatalog::default();
    native
        .allow(identity(4), QueryPolicy::Noncacheable, |_| {
            Ok(Backend::Native(Rc::new(NativeDomain)))
        })
        .unwrap();
    let wasm = factory_catalog(effects, &binding, &ids, &binding, &ids, true);
    let mut accounting = AccountingRestoreCatalog::default();
    accounting
        .allow(accounting_identity(), [11; 32], || {
            Ok(Rc::new(AnyAccounting))
        })
        .unwrap();
    composed::RestoreAuthority {
        bindings: [composer_identity()].into(),
        kernel: Rc::new(CompositionKernel {
            materials,
            factories,
            approved,
        }),
        owners: [
            (
                "native".into(),
                Rc::new(composed::TypedFactory {
                    codec: Rc::new(NativeDomain),
                    catalog: native,
                }) as Rc<dyn composed::OwnerFactory>,
            ),
            (
                "wasm".into(),
                Rc::new(composed::TypedFactory {
                    codec: Rc::new(StateCodec),
                    catalog: wasm,
                }) as Rc<dyn composed::OwnerFactory>,
            ),
        ]
        .into(),
        accounting: Rc::new(accounting),
        workflows: None,
        inputs: None,
        timers: None,
        adapters: None,
        resources: None,
        owner_limits: Limits::default(),
        publication_limits: PublicationLimits::default(),
        limits: SaveLimits::default(),
    }
}
type CaptureProbe = Rc<RefCell<Option<Box<dyn Fn()>>>>;
struct ProbeCodec {
    probe: CaptureProbe,
    refuse: Cell<bool>,
    calls: Cell<usize>,
}
impl PersistenceCodec<u64> for ProbeCodec {
    fn identity(&self) -> CodecIdentity {
        identity(4)
    }
    fn encode_state(&self, state: &u64, limit: usize) -> Result<Vec<u8>> {
        self.calls.set(self.calls.get() + 1);
        if let Some(probe) = self.probe.borrow().as_ref() {
            probe();
        }
        if self.refuse.get() {
            return Err(Error::Schema);
        }
        NativeDomain.encode_state(state, limit)
    }
    fn decode_state(&self, bytes: &[u8], limit: usize, remap: &Remap) -> Result<u64> {
        NativeDomain.decode_state(bytes, limit, remap)
    }
    fn validate_state(&self, state: &u64) -> Result<()> {
        NativeDomain.validate_state(state)
    }
    fn references(&self, state: &u64) -> Result<Vec<StateReference>> {
        NativeDomain.references(state)
    }
    fn validate_configuration(
        &self,
        state: &u64,
        ports: &BTreeMap<String, Port>,
        policy: &QueryPolicy,
    ) -> Result<()> {
        NativeDomain.validate_configuration(state, ports, policy)
    }
}
#[test]
fn concrete_prepared_native_wasm_readonly_sourcefree_resave_and_explicit_once() {
    NATIVE_EXECUTIONS.with(|n| n.set(0));
    let mut h = Harness::new(Limits::default(), KernelLimits::default(), "");
    let effects = Rc::new(Cell::new(0));
    let behavior: Rc<dyn WasmCodec<Vec<u8>>> = Rc::new(Domain {
        add: 1,
        effects: effects.clone(),
    });
    let binding = h.binding();
    let native_backend: Rc<dyn NativeBehavior<u64>> = Rc::new(NativeDomain);
    let native = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            41,
            Backend::Native(native_backend.clone()),
        )
        .unwrap();
    let wasm = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            bytes(5),
            Backend::Wasm {
                codec: behavior.clone(),
                slot: 0,
                binding: binding.clone(),
            },
        )
        .unwrap();
    h.admit(&native);
    h.admit(&wasm);
    h.refresh();
    wasm.set_query_policy(old_policy()).unwrap();
    let prepared: Rc<RefCell<Option<Batch>>> = Rc::new(RefCell::new(None));
    let output = prepared.clone();
    let n = native.clone();
    let w = wasm.clone();
    let host = h.host.clone();
    let accounting: Rc<dyn Accounting> = Rc::new(AnyAccounting);
    let a = accounting.clone();
    h.run(1, move |view| {
        let readonly = n.query("query", &[], &[], view).unwrap();
        let c1 = n.prepare("prepare", &[], vec![], view).unwrap();
        let c2 = w.prepare("prepare", &[], vec![], view).unwrap();
        *output.borrow_mut() = Some(
            host.batch(
                vec![c1.erase(), c2.erase()],
                vec![readonly],
                &[n.id(), w.id()],
                vec![n.id(), w.id()],
                [11; 32],
                a.clone(),
                cap("prepare"),
                view,
            )
            .unwrap(),
        );
        Ok(vec![])
    });
    let batch = prepared.borrow_mut().take().unwrap();
    let serials = batch.persistence_candidate_serials();
    let mut native_capture = CaptureCatalog::default();
    native_capture
        .register(
            identity(4),
            QueryPolicy::Noncacheable,
            Backend::Native(native_backend),
        )
        .unwrap();
    let mut wasm_capture = CaptureCatalog::default();
    wasm_capture
        .register(
            identity(1),
            old_policy(),
            Backend::Wasm {
                codec: behavior,
                slot: 0,
                binding: binding.clone(),
            },
        )
        .unwrap();
    let mut accounting_capture = AccountingCaptureCatalog::default();
    accounting_capture
        .register(accounting_identity(), [11; 32], accounting)
        .unwrap();
    let probe: CaptureProbe = Rc::new(RefCell::new(None));
    let native_codec = Rc::new(ProbeCodec {
        probe: probe.clone(),
        refuse: Cell::new(false),
        calls: Cell::new(0),
    });
    let owners: Vec<Rc<dyn composed::OwnerAllocation>> = vec![
        Rc::new(composed::TypedOwner {
            key: "native".into(),
            owner: native.clone(),
            codec: native_codec.clone(),
            catalog: native_capture,
            retained: vec![],
            callbacks: vec![],
            target_port: None,
            candidates: vec![],
            batch_candidates: vec![serials[0]],
        }),
        Rc::new(composed::TypedOwner {
            key: "wasm".into(),
            owner: wasm.clone(),
            codec: Rc::new(StateCodec),
            catalog: wasm_capture,
            retained: vec![],
            callbacks: vec![],
            target_port: None,
            candidates: vec![],
            batch_candidates: vec![serials[1]],
        }),
    ];
    let materials = h.materials.clone();
    let old_operation = h.operation.clone();
    let old_native = native.id();
    let mut roots = ExecutionRoots::default();
    roots.batch(&batch).unwrap();
    let live = roots.live_operations(&h.kernel).unwrap();
    let snapshot = h
        .kernel
        .kernel_snapshot_with_cleanup(
            &roots.enrollment(&live),
            &[h.host.cleanup_identity().unwrap()],
        )
        .unwrap();
    let index = h.kernel.execution_capture_index(&snapshot).unwrap();
    let ids = binding
        .resolution
        .iter()
        .map(|b| index.module(b.handle).unwrap())
        .collect::<Vec<_>>();
    let omitted = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            1,
            Backend::Native(Rc::new(NativeDomain)),
        )
        .unwrap();
    let config =
        runtime::ApprovedConfiguration::new(BTreeSet::new(), BTreeSet::new(), vec![]).unwrap();
    let (source, key, credential) = composed::Session::new(
        h.kernel,
        h.host,
        owners,
        vec![batch],
        accounting_capture,
        composed::Services::default(),
        config,
    )
    .unwrap();
    let source = Rc::new(source);
    let key = Rc::new(key);
    let credential = Rc::new(credential);
    assert!(matches!(
        source.capture(&key, SaveLimits::default()),
        Err(runtime::Error::FactoryRefused)
    ));
    assert_eq!(native_codec.calls.get(), 0);
    drop(omitted);
    // Valid dispatch is permitted while the entire session is enrolled.
    let query_owner = native.clone();
    *h.action.borrow_mut() = Some(Box::new(move |view| {
        query_owner
            .query("query", &[], &[], view)
            .map(|o| o.bytes().to_vec())
            .map_err(|_| nethacked_kernel::Error::NativeRefusal)
    }));
    source
        .with_runtime(&credential, |b| {
            assert_eq!(b.host().pending().unwrap(), 2);
            assert!(b
                .kernel()
                .unwrap()
                .invoke_operation(
                    &old_operation,
                    &[0],
                    Authority {
                        access: Access::Write,
                        scope: Scope::All
                    }
                )
                .is_ok());
        })
        .unwrap();
    let weak = Rc::downgrade(&source);
    let capture_key = key.clone();
    let runtime_credential = credential.clone();
    let alias_host = source
        .with_runtime(&credential, |b| b.host().clone())
        .unwrap();
    let alias_owner = native.clone();
    *probe.borrow_mut() = Some(Box::new(move || {
        assert!(matches!(alias_host.pending(), Err(Error::Busy)));
        assert!(alias_owner.stamp().is_err());
        let session = weak.upgrade().unwrap();
        assert!(matches!(
            session.capture(&capture_key, SaveLimits::default()),
            Err(runtime::Error::Busy)
        ));
        assert!(matches!(
            session.with_runtime(&runtime_credential, |_| ()),
            Err(runtime::Error::Busy)
        ));
    }));
    native_codec.refuse.set(true);
    assert!(matches!(
        source.capture(&key, SaveLimits::default()),
        Err(runtime::Error::FactoryRefused)
    ));
    source
        .with_runtime(&credential, |b| assert_eq!(b.host().pending().unwrap(), 2))
        .unwrap();
    native_codec.refuse.set(false);
    let bytes = source.capture(&key, SaveLimits::default()).unwrap();
    probe.borrow_mut().take();
    drop(native_codec);
    let weak_source = Rc::downgrade(&native);
    drop(source);
    drop(native);
    drop(wasm);
    drop(h.action);
    assert!(weak_source.upgrade().is_none());
    let restored_effects = Rc::new(Cell::new(0));
    let factories = Rc::new(Cell::new(0));
    let config = composition_config(
        materials.clone(),
        binding.clone(),
        ids.clone(),
        restored_effects.clone(),
        factories.clone(),
        snapshot,
    );
    let destination_host = Host::new(Limits::default()).unwrap();
    let (destination, dkey, dcredential) = composed::Session::new(
        Kernel::new(KernelLimits::default()).unwrap(),
        destination_host,
        vec![],
        vec![],
        AccountingCaptureCatalog::default(),
        composed::Services::default(),
        config,
    )
    .unwrap();
    let mut stage = destination
        .prepare_restore(&dkey, &composer_identity(), &bytes)
        .unwrap();
    assert_eq!(restored_effects.get(), 0);
    NATIVE_EXECUTIONS.with(|n| assert_eq!(n.get(), 3));
    destination
        .with_runtime(&dcredential, |b| assert_eq!(b.host().pending().unwrap(), 0))
        .unwrap();
    let _activation = destination.activate(&mut stage).unwrap();
    assert!(factories.get() > 0);
    let pending_resave = destination.capture(&dkey, SaveLimits::default()).unwrap();
    // This approval is enrolled from the trusted capture above, never the load input.
    let refreshed = trusted_section::<KernelSnapshot>(&pending_resave, "kernel");
    let full_config = || {
        composition_config(
            materials.clone(),
            binding.clone(),
            ids.clone(),
            restored_effects.clone(),
            factories.clone(),
            refreshed.clone(),
        )
    };
    destination
        .replace_configuration(&dkey, full_config())
        .unwrap();
    let unchanged = trusted_section::<serde_json::Value>(
        &destination.capture(&dkey, SaveLimits::default()).unwrap(),
        "owners",
    );
    // Validly framed hostile metadata and source-pair changes refuse before any
    // artifact factory is reached, with a nonempty destination left untouched.
    let bad_pair = rewrite_section(&pending_resave, "owners", |v| {
        v["kernel_namespace"] = serde_json::json!(refreshed.source_namespace + 1);
    });
    let missing_reference = rewrite_wire(&pending_resave, |v| {
        v["owners"][0]["candidates"] = serde_json::json!([u64::MAX]);
    });
    let unknown = rewrite_wire(&pending_resave, |v| {
        v["unknown"] = serde_json::json!(true);
    });
    let missing_section = rewrite_wire(&pending_resave, |v| {
        v["sections"]
            .as_array_mut()
            .unwrap()
            .retain(|s| s["name"] != "kernel");
    });
    for bad in [&bad_pair, &missing_reference, &unknown, &missing_section] {
        let before = factories.get();
        assert!(destination
            .prepare_restore(&dkey, &composer_identity(), bad)
            .is_err());
        assert_eq!(factories.get(), before);
        assert_eq!(
            trusted_section::<serde_json::Value>(
                &destination.capture(&dkey, SaveLimits::default()).unwrap(),
                "owners"
            ),
            unchanged
        );
    }
    let mut revoked_stage = destination
        .prepare_restore(&dkey, &composer_identity(), &pending_resave)
        .unwrap();
    let missing = composed::SessionFactory::new({
        let mut a = composition_authority(
            materials.clone(),
            binding.clone(),
            ids.clone(),
            restored_effects.clone(),
            factories.clone(),
            refreshed.clone(),
        );
        a.owners.remove("native");
        a
    })
    .configuration(composer_identity())
    .unwrap();
    destination.replace_configuration(&dkey, missing).unwrap();
    assert!(matches!(
        destination.activate(&mut revoked_stage),
        Err(runtime::Error::AuthorityChanged)
    ));
    let before = factories.get();
    assert!(destination
        .prepare_restore(&dkey, &composer_identity(), &pending_resave)
        .is_err());
    assert_eq!(factories.get(), before);
    destination
        .replace_configuration(&dkey, full_config())
        .unwrap();
    let mut raw_change_stage = destination
        .prepare_restore(&dkey, &composer_identity(), &pending_resave)
        .unwrap();
    let fresh = destination.issue_credential(&dkey).unwrap();
    destination
        .with_runtime(&fresh, |b| {
            let module = b
                .owner::<u64>("native")
                .unwrap()
                .owner
                .stamp()
                .unwrap()
                .module
                .handle;
            b.update_kernel(|k| k.revoke_grant(module, 0))
                .unwrap()
                .unwrap();
        })
        .unwrap();
    assert!(matches!(
        destination.activate(&mut raw_change_stage),
        Err(runtime::Error::AuthorityChanged)
    ));
    assert!(matches!(
        destination.prepare_restore(&dkey, &composer_identity(), &pending_resave),
        Err(runtime::Error::AuthorityChanged)
    ));
    // Reenroll independently sufficient authority before a fresh reconstruction.
    destination
        .replace_configuration(&dkey, full_config())
        .unwrap();
    let fresh = destination.issue_credential(&dkey).unwrap();
    let (retired_host, retired_owner, retired_batch) = destination
        .with_runtime(&fresh, |b| {
            (
                b.host().clone(),
                b.owner::<u64>("native").unwrap().owner.clone(),
                b.batches()[0].clone(),
            )
        })
        .unwrap();
    let mut before_narrow = destination
        .prepare_restore(&dkey, &composer_identity(), &pending_resave)
        .unwrap();
    let mut narrower = refreshed.clone();
    let mut removed = 0;
    for grant in &mut narrower.registry.grants {
        if grant.slot == 0
            && narrower
                .registry
                .modules
                .iter()
                .any(|m| m.id == grant.caller && m.manifest.id == "caller")
        {
            if let Scope::Keys(keys) = &mut grant.requested.scope {
                let count = keys.len();
                keys.retain(|r| !r.key.ends_with("derived"));
                removed += count - keys.len();
            }
        }
    }
    assert!(removed > 0);
    destination
        .replace_configuration(
            &dkey,
            composition_config(
                materials.clone(),
                binding.clone(),
                ids.clone(),
                restored_effects.clone(),
                factories.clone(),
                narrower,
            ),
        )
        .unwrap();
    assert!(matches!(
        destination.activate(&mut before_narrow),
        Err(runtime::Error::AuthorityChanged)
    ));
    // This pending graph retained the original broad slot ceiling. It cannot be
    // rebuilt under a narrower live grant, even when its next call uses less.
    assert!(destination
        .prepare_restore(&dkey, &composer_identity(), &pending_resave)
        .is_err());
    assert_eq!(restored_effects.get(), 0);
    destination
        .replace_configuration(&dkey, full_config())
        .unwrap();
    let mut pending_reloaded = destination
        .prepare_restore(&dkey, &composer_identity(), &pending_resave)
        .unwrap();
    let activation = destination.activate(&mut pending_reloaded).unwrap();
    destination
        .with_runtime(activation.credential(), |b| {
            assert!(retired_host.pending().is_err());
            assert!(retired_owner.stamp().is_err());
            drop(retired_batch);
            let n = b.owner::<u64>("native").unwrap();
            assert_ne!(n.owner.id(), old_native);
            let operation = n.candidates[0].receipt().operation.clone();
            assert_ne!(operation.operation(), old_operation.operation());
            assert!(b
                .kernel()
                .unwrap()
                .invoke_operation(
                    &old_operation,
                    &[1],
                    Authority {
                        access: Access::Write,
                        scope: Scope::All
                    }
                )
                .is_err());
            assert!(b
                .kernel()
                .unwrap()
                .invoke_operation(
                    &operation,
                    &[1],
                    Authority {
                        access: Access::Write,
                        scope: Scope::All
                    }
                )
                .is_ok());
            assert_eq!(b.host().history().unwrap().len(), 1);
            assert!(b
                .kernel()
                .unwrap()
                .invoke_operation(
                    &operation,
                    &[1],
                    Authority {
                        access: Access::Write,
                        scope: Scope::All
                    }
                )
                .is_err());
        })
        .unwrap();
    assert_eq!(restored_effects.get(), 0);
    // Keep this variable for an explicit foreign-operation refusal below.

    destination.capture(&dkey, SaveLimits::default()).unwrap();
}

fn rewrite_wire(wire: &[u8], edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut body: serde_json::Value =
        serde_json::from_slice(&wire[nethacked_persistence::HEADER_LEN..]).unwrap();
    edit(&mut body);
    let body = serde_json::to_vec(&body).unwrap();
    let mut out = wire[..12].to_vec();
    out.extend_from_slice(&(body.len() as u64).to_le_bytes());
    out.extend_from_slice(&Sha256::digest(&body));
    out.extend_from_slice(&body);
    out
}
fn rewrite_section(wire: &[u8], name: &str, edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
    rewrite_wire(wire, |body| {
        let s = body["sections"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|s| s["name"] == name)
            .unwrap();
        let hex = s["local_payload"].as_str().unwrap();
        let bytes = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect::<Vec<_>>();
        let mut value = serde_json::from_slice(&bytes).unwrap();
        edit(&mut value);
        s["local_payload"] = serde_json::json!(serde_json::to_vec(&value)
            .unwrap()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>());
    })
}

fn trusted_section<T: serde::de::DeserializeOwned>(wire: &[u8], name: &str) -> T {
    let body: serde_json::Value =
        serde_json::from_slice(&wire[nethacked_persistence::HEADER_LEN..]).unwrap();
    let hex = body["sections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == name)
        .unwrap()["local_payload"]
        .as_str()
        .unwrap();
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn empty_session_uses_concrete_capture_restore_and_fresh_cleanup_identity() {
    let empty =
        || runtime::ApprovedConfiguration::new(BTreeSet::new(), BTreeSet::new(), vec![]).unwrap();
    let (source, key, _) = composed::Session::new(
        Kernel::new(KernelLimits::default()).unwrap(),
        Host::new(Limits::default()).unwrap(),
        vec![],
        vec![],
        AccountingCaptureCatalog::default(),
        Default::default(),
        empty(),
    )
    .unwrap();
    let wire = source.capture(&key, SaveLimits::default()).unwrap();
    let approved = trusted_section::<KernelSnapshot>(&wire, "kernel");
    let old_cleanup = approved.cleanup_history.clone();
    drop(source);
    let authority = composed::RestoreAuthority {
        bindings: [composer_identity()].into(),
        kernel: Rc::new(CompositionKernel {
            materials: BTreeMap::new(),
            factories: Rc::new(Cell::new(0)),
            approved,
        }),
        owners: BTreeMap::new(),
        accounting: Rc::new(AccountingRestoreCatalog::default()),
        workflows: None,
        inputs: None,
        timers: None,
        adapters: None,
        resources: None,
        owner_limits: Limits::default(),
        publication_limits: PublicationLimits::default(),
        limits: SaveLimits::default(),
    };
    let config = composed::SessionFactory::new(authority)
        .configuration(composer_identity())
        .unwrap();
    let (destination, key, _) = composed::Session::new(
        Kernel::new(KernelLimits::default()).unwrap(),
        Host::new(Limits::default()).unwrap(),
        vec![],
        vec![],
        AccountingCaptureCatalog::default(),
        Default::default(),
        config,
    )
    .unwrap();
    let mut stage = destination
        .prepare_restore(&key, &composer_identity(), &wire)
        .unwrap();
    let activation = destination.activate(&mut stage).unwrap();
    destination
        .with_runtime(activation.credential(), |b| {
            assert_eq!(b.host().pending().unwrap(), 0)
        })
        .unwrap();
    let resave = destination.capture(&key, SaveLimits::default()).unwrap();
    assert_ne!(
        trusted_section::<KernelSnapshot>(&resave, "kernel").cleanup_history,
        old_cleanup
    );
}

#[test]
fn raw_kernel_dispatch_capture_and_administration_share_epochs_and_retirement() {
    use nethacked_kernel::session::{Domain, Error as BarrierError};
    let mut h = Harness::new(Limits::default(), KernelLimits::default(), "");
    let base = h.binding();
    let extension = h
        .kernel
        .register_guest(
            Manifest {
                id: "replacement".into(),
                exports: [("compute".into(), endpoint(Access::Read))].into(),
                dependencies: vec![],
                extension_points: BTreeSet::new(),
                extension: Some(Extension {
                    target: base.capability,
                    export: "compute".into(),
                }),
            },
            &helper_wasm("i32.const 1024 local.get $p i32.load i32.store i64.const 4398046511108"),
        )
        .unwrap();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    *h.action.borrow_mut() = Some(Box::new(move |_| {
        observed.set(observed.get() + 1);
        Ok(vec![])
    }));
    let domain = Rc::new(Domain::new());
    let token = domain.enrollment(1);
    h.kernel.enroll_session(&token).unwrap();
    let epoch = domain.epoch();
    {
        let _capture = domain.capture().unwrap();
        assert!(matches!(token.changed(), Err(BarrierError::Busy)));
        assert!(matches!(
            h.kernel.invoke_operation(
                &h.operation,
                &[1],
                Authority {
                    access: Access::Write,
                    scope: Scope::All
                }
            ),
            Err(nethacked_kernel::Error::Busy)
        ));
        assert!(matches!(
            h.kernel.revoke_grant(h.root, 0),
            Err(nethacked_kernel::Error::Busy)
        ));
        assert!(matches!(
            h.kernel.managed_detach(extension),
            Err(nethacked_kernel::Error::Busy)
        ));
    }
    assert_eq!(domain.epoch(), epoch);
    assert_eq!(calls.get(), 0);
    {
        let _runtime = domain.runtime().unwrap();
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
    }
    assert_eq!(calls.get(), 1);
    h.kernel.revoke_grant(h.root, 0).unwrap();
    let revoked = domain.epoch();
    assert!(revoked > epoch);
    h.kernel
        .issue_grant(
            h.root,
            0,
            Authority {
                access: Access::Read,
                scope: Scope::Keys(BTreeSet::new()),
            },
        )
        .unwrap();
    assert!(domain.epoch() > revoked);
    let granted = domain.epoch();
    h.kernel.managed_detach(extension).unwrap();
    assert!(domain.epoch() > granted);
    let detached = domain.epoch();
    h.kernel.managed_attach(extension).unwrap();
    assert!(domain.epoch() > detached);
    let attached = domain.epoch();
    h.kernel.release_pipeline(h.lease).unwrap();
    assert!(domain.epoch() > attached);
    domain.exchange(2);
    assert!(h.kernel.manifest(h.root).is_err());
    assert!(h
        .kernel
        .invoke(
            h.root,
            "run",
            &[1],
            Authority {
                access: Access::Write,
                scope: Scope::All
            }
        )
        .is_err());
    assert!(matches!(token.enter(), Err(BarrierError::Retired)));
}

#[test]
fn compatible_checkpoint_restages_under_narrow_current_scopes_without_regaining_them() {
    let mut h = Harness::new(Limits::default(), KernelLimits::default(), "");
    let backend: Rc<dyn NativeBehavior<u64>> = Rc::new(NativeDomain);
    let owner = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            9,
            Backend::Native(backend.clone()),
        )
        .unwrap();
    h.admit(&owner);
    h.refresh();
    let binding = h.binding();
    let materials = h.materials.clone();
    h.kernel.release_pipeline(h.lease).unwrap();
    drop(h.operation);
    let mut capture = CaptureCatalog::default();
    capture
        .register(
            identity(4),
            QueryPolicy::Noncacheable,
            Backend::Native(backend),
        )
        .unwrap();
    let empty =
        runtime::ApprovedConfiguration::new(BTreeSet::new(), BTreeSet::new(), vec![]).unwrap();
    let (source, key, _) = composed::Session::new(
        h.kernel,
        h.host,
        vec![Rc::new(composed::TypedOwner {
            key: "native".into(),
            owner: owner.clone(),
            codec: Rc::new(NativeDomain),
            catalog: capture,
            retained: vec![],
            callbacks: vec![],
            target_port: None,
            candidates: vec![],
            batch_candidates: vec![],
        })],
        vec![],
        AccountingCaptureCatalog::default(),
        Default::default(),
        empty,
    )
    .unwrap();
    let wire = source.capture(&key, SaveLimits::default()).unwrap();
    drop(source);
    drop(owner);
    drop(h.action);
    let approved = trusted_section::<KernelSnapshot>(&wire, "kernel");
    let caller = approved
        .registry
        .modules
        .iter()
        .find(|m| m.manifest.id == "caller")
        .unwrap()
        .id;
    let config = |approved| {
        composition_config(
            materials.clone(),
            binding.clone(),
            vec![],
            Rc::new(Cell::new(0)),
            Rc::new(Cell::new(0)),
            approved,
        )
    };
    let (destination, key, _) = composed::Session::new(
        Kernel::new(KernelLimits::default()).unwrap(),
        Host::new(Limits::default()).unwrap(),
        vec![],
        vec![],
        AccountingCaptureCatalog::default(),
        Default::default(),
        config(approved.clone()),
    )
    .unwrap();
    let mut old_stage = destination
        .prepare_restore(&key, &composer_identity(), &wire)
        .unwrap();
    let mut narrowed = approved.clone();
    let grant = narrowed
        .registry
        .grants
        .iter_mut()
        .find(|g| g.caller == caller && g.slot == 0)
        .unwrap();
    let Scope::Keys(keys) = &mut grant.requested.scope else {
        panic!("fixture has exact resource scopes")
    };
    let previous = keys.len();
    keys.retain(|r| !r.key.ends_with("derived"));
    assert!(keys.len() < previous);
    destination
        .replace_configuration(&key, config(narrowed))
        .unwrap();
    assert!(matches!(
        destination.activate(&mut old_stage),
        Err(runtime::Error::AuthorityChanged)
    ));
    let mut stage = destination
        .prepare_restore(&key, &composer_identity(), &wire)
        .unwrap();
    let activation = destination.activate(&mut stage).unwrap();
    destination
        .with_runtime(activation.credential(), |b| {
            assert_eq!(
                b.owner::<u64>("native").unwrap().owner.inspect().unwrap(),
                9u64.to_le_bytes()
            )
        })
        .unwrap();
    let saved = trusted_section::<KernelSnapshot>(
        &destination.capture(&key, SaveLimits::default()).unwrap(),
        "kernel",
    );
    let caller = saved
        .registry
        .modules
        .iter()
        .find(|m| m.manifest.id == "caller")
        .unwrap()
        .id;
    let Scope::Keys(keys) = &saved
        .registry
        .grants
        .iter()
        .find(|g| g.caller == caller && g.slot == 0)
        .unwrap()
        .requested
        .scope
    else {
        panic!("restored scope stays finite")
    };
    assert!(keys.iter().all(|r| !r.key.ends_with("derived")));
}

#[path = "session/b_repair.rs"]
mod b_repair;
