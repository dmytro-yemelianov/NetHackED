#![cfg(feature = "persistence")]
#![allow(dead_code)]
include!("support/persistence.rs");
use nethacked_kernel::{CleanupIdentity, RetainedCallback};
use nethacked_owner_services::{persistence as adapter, CallbackBehaviors, Targets};
use nethacked_timers::CallbackRetention;
fn adapter_identity() -> nethacked_timers::persistence::Identity {
    nethacked_timers::persistence::Identity {
        name: "owner-adapter".into(),
        version: 1,
        artifact: [19; 32],
    }
}
struct AdapterApproved(adapter::Checkpoint);
impl adapter::AdapterAuthority for AdapterApproved {
    fn approve(
        &self,
        c: &adapter::Checkpoint,
        _: &KernelSnapshot,
        _: &ExecutionCheckpoint,
    ) -> nethacked_kernel::Result<()> {
        if &self.0 == c {
            Ok(())
        } else {
            Err(nethacked_kernel::Error::Authority)
        }
    }
}
struct OldNativeCodec;
impl Schema<u64> for OldNativeCodec {
    fn validate(&self, state: &u64) -> Result<()> {
        NativeDomain.validate(state)
    }
    fn encode(&self, state: &u64) -> Result<Vec<u8>> {
        NativeDomain.encode(state)
    }
    fn query(
        &self,
        state: &u64,
        name: &str,
        request: &[u8],
        observations: &[Observation],
    ) -> Result<Vec<u8>> {
        NativeDomain.query(state, name, request, observations)
    }
}
impl WasmCodec<u64> for OldNativeCodec {
    fn input(&self, state: &u64, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        NATIVE_EXECUTIONS.with(|n| n.set(n.get() + 1));
        Ok(bytes(
            u32::try_from(*state)
                .ok()
                .and_then(|v| v.checked_add(3))
                .ok_or(Error::Invariant)?,
        ))
    }
    fn decode(&self, b: &[u8]) -> Result<Proposal<u64>> {
        NATIVE_EXECUTIONS.with(|n| n.set(n.get() + 1));
        Ok(Proposal {
            next: number(b) as u64,
            reply: b.to_vec(),
            events: vec![],
        })
    }
}
#[path = "../../nethacked-timers/tests/support.rs"]
mod timer_support;
use nethacked_timers::persistence::{RetentionRestore, TargetCodec};
struct Proxy {
    old: Vec<nethacked_resources::Stamp>,
    adapter: RefCell<Option<Rc<adapter::Restored>>>,
}
impl Proxy {
    fn get(&self) -> nethacked_kernel::Result<Rc<adapter::Restored>> {
        self.adapter
            .borrow()
            .clone()
            .ok_or(nethacked_kernel::Error::Authority)
    }
}
impl nethacked_timers::TargetValidator for Proxy {
    fn validate(&self, t: &[nethacked_resources::Stamp]) -> nethacked_kernel::Result<()> {
        nethacked_timers::TargetValidator::validate(self.get()?.targets().as_ref(), t)
    }
}
impl TargetCodec for Proxy {
    fn validate_old(&self, t: &[nethacked_resources::Stamp]) -> nethacked_kernel::Result<()> {
        if t.iter().all(|t| {
            self.old.iter().any(|s| {
                s.address == t.address && s.generation == t.generation && t.revision <= s.revision
            })
        }) {
            Ok(())
        } else {
            Err(nethacked_kernel::Error::Authority)
        }
    }
    fn remap(
        &self,
        t: &[nethacked_resources::Stamp],
        m: &RestoredKernel,
    ) -> nethacked_kernel::Result<Vec<nethacked_resources::Stamp>> {
        TargetCodec::remap(self.get()?.as_ref(), t, m)
    }
}
impl CallbackRetention for Proxy {
    fn preflight(
        &self,
        v: &CallerView<'_>,
        t: &[nethacked_resources::Stamp],
    ) -> nethacked_kernel::Result<()> {
        self.get()?.callbacks().preflight(v, t)
    }
    fn retain(
        &self,
        c: &RetainedCallback,
        t: &[nethacked_resources::Stamp],
    ) -> nethacked_kernel::Result<Rc<dyn nethacked_timers::CallbackReference>> {
        self.get()?.callbacks().retain(c, t)
    }
}
impl RetentionRestore for Proxy {
    fn destination(&self) -> ExecutionDestination {
        self.get().unwrap().destination()
    }
    fn check(
        &self,
        r: &nethacked_timers::persistence::TaskRow,
        m: &RestoredKernel,
    ) -> nethacked_kernel::Result<()> {
        self.get()?.check(r, m)
    }
    fn reference(
        &self,
        id: u64,
    ) -> nethacked_kernel::Result<Rc<dyn nethacked_timers::CallbackReference>> {
        self.get()?.reference(id)
    }
    fn finish_rebuild(&self) {
        self.get().unwrap().finish_rebuild();
    }
}
struct TimerApproved(nethacked_timers::persistence::Checkpoint);
impl nethacked_timers::persistence::TimerAuthority for TimerApproved {
    fn approve(
        &self,
        c: &nethacked_timers::persistence::Checkpoint,
        _: &KernelSnapshot,
    ) -> nethacked_kernel::Result<()> {
        if &self.0 == c {
            Ok(())
        } else {
            Err(nethacked_kernel::Error::Authority)
        }
    }
}
fn timer_identity(name: &str) -> nethacked_timers::persistence::Identity {
    nethacked_timers::persistence::Identity {
        name: name.into(),
        version: 1,
        artifact: [21; 32],
    }
}
#[test]
fn actual_bytes_source_free_adapter_old_wasm_native_shared_routes_and_fresh_dispatch() {
    combined(false)
}
#[test]
fn actual_bytes_terminal_busy_owner_prefix_restores_and_discards_without_delivery() {
    combined(true)
}
struct BusyHost {
    host: Host,
    busy: Rc<Cell<bool>>,
}
impl nethacked_workflows::CleanupParticipant for BusyHost {
    fn identity(&self) -> nethacked_kernel::Result<Option<CleanupIdentity>> {
        nethacked_workflows::CleanupParticipant::identity(&self.host)
    }
    fn discard(&self, b: &OperationBinding) -> nethacked_kernel::Result<()> {
        if self.busy.get() {
            Err(nethacked_kernel::Error::NativeRefusal)
        } else {
            nethacked_workflows::CleanupParticipant::discard(&self.host, b)
        }
    }
}
struct AccountingFixture;
impl Accounting for AccountingFixture {
    fn validate(&self, _: &[ProposalView], _: &[Observation]) -> Result<()> {
        Ok(())
    }
}
fn combined(terminal: bool) {
    NATIVE_EXECUTIONS.with(|n| n.set(0));
    let mut h = Harness::new(Limits::default(), KernelLimits::default(), "");
    let effects = Rc::new(Cell::new(0));
    let old: Rc<dyn WasmCodec<Vec<u8>>> = Rc::new(Domain {
        add: 1,
        effects: effects.clone(),
    });
    let base = h.binding();
    let w = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            bytes(5),
            Backend::Wasm {
                codec: old.clone(),
                slot: 0,
                binding: base.clone(),
            },
        )
        .unwrap();
    let native: Rc<dyn WasmCodec<u64>> = Rc::new(OldNativeCodec);
    let n = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            41,
            Backend::Wasm {
                codec: native.clone(),
                slot: 0,
                binding: base.clone(),
            },
        )
        .unwrap();
    h.admit(&w);
    h.admit(&n);
    h.refresh();
    w.set_query_policy(old_policy()).unwrap();
    let targets = Rc::new(Targets::new(16).unwrap());
    let wt = targets.register(&w, "prepare").unwrap();
    let nt = targets.register(&n, "prepare").unwrap();
    let callbacks = Rc::new(CallbackBehaviors::new(16).unwrap());
    let source_busy = Rc::new(Cell::new(false));
    let timers = nethacked_timers::Service::new(
        nethacked_timers::Limits::default(),
        timer_support::queue_key(),
        Rc::new(timer_support::Clock::default()),
        targets.clone(),
        true,
        vec![Rc::new(BusyHost {
            host: h.host.clone(),
            busy: source_busy.clone(),
        })],
    )
    .unwrap();
    timers.retain_callbacks(callbacks.clone()).unwrap();
    let timer = timers.install(&mut h.kernel, "timers").unwrap();
    let timer_manifest = h.kernel.manifest(timer).unwrap().clone();
    let timer_artifact = h.kernel.artifact(timer).unwrap();
    let timer_bytes = serde_json::to_vec(
        &serde_json::to_value((
            "nethacked-timers-queue-v1",
            env!("CARGO_PKG_VERSION"),
            nethacked_timers::Limits::default(),
            timer_support::queue_key(),
        ))
        .unwrap(),
    )
    .unwrap();
    h.materials.insert(
        timer_artifact,
        Material::Native(timer_manifest, timer_bytes),
    );
    h.keys.insert(timer_support::queue_key());
    let actor_bytes=wat::parse_str(r#"(module
  (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1) (func (export "alloc") (param i32) (result i32) i32.const 0)
  (func $forward (param $slot i32) (param $p i32) (param $n i32) (result i64) (local $r i32)
    local.get $slot local.get $p local.get $n i32.const 20000 i32.const 16384 call $call local.tee $r i32.const 0 i32.lt_s if unreachable end
    i64.const 85899345920000 local.get $r i64.extend_i32_u i64.or)
  (func (export "run") (param $p i32) (param $n i32) (result i64)
    local.get $p i32.load8_u local.get $p i32.const 1 i32.add local.get $n i32.const 1 i32.sub call $forward)
  (func (export "callback") (param $p i32) (param $n i32) (result i64) i32.const 0 local.get $p local.get $n call $forward))"#).unwrap();
    let manifest = Manifest {
        id: "timer-actor".into(),
        exports: [
            ("run".into(), endpoint(Access::Write)),
            ("callback".into(), endpoint(Access::Write)),
        ]
        .into(),
        dependencies: vec![
            KernelDependency {
                capability: cap("prepare"),
                access: Access::Write,
            },
            KernelDependency {
                capability: Capability {
                    provider: "timers".into(),
                    endpoint: "queue".into(),
                    major: 1,
                },
                access: Access::Write,
            },
        ],
        extension_points: BTreeSet::new(),
        extension: None,
    };
    let actor = h
        .kernel
        .register_guest(manifest.clone(), &actor_bytes)
        .unwrap();
    h.materials.insert(
        h.kernel.artifact(actor).unwrap(),
        Material::Guest(manifest, actor_bytes),
    );
    for slot in 0..2 {
        h.kernel
            .issue_grant(
                actor,
                slot,
                Authority {
                    access: Access::Write,
                    scope: Scope::Keys(h.keys.clone()),
                },
            )
            .unwrap();
    }
    h.kernel.release_pipeline(h.lease).unwrap();
    h.root = actor;
    let (op, lease) = h
        .kernel
        .capture_operation(
            actor,
            "run",
            Authority {
                access: Access::Write,
                scope: Scope::All,
            },
        )
        .unwrap();
    h.lease = lease;
    h.operation = h
        .kernel
        .operation_binding_with_cleanup(
            op,
            lease,
            1,
            "prepare",
            &[h.host.cleanup_identity().unwrap()],
        )
        .unwrap();
    let wb = Rc::new(RefCell::new(None));
    let nb = Rc::new(RefCell::new(None));
    let cb = Rc::new(RefCell::new(None));
    h.run(0, {
        let w = w.clone();
        let n = n.clone();
        let wb = wb.clone();
        let nb = nb.clone();
        let cb = cb.clone();
        let keys = h.keys.clone();
        move |view| {
            *wb.borrow_mut() = Some(w.retain_behavior("prepare", view).unwrap());
            *nb.borrow_mut() = Some(n.retain_behavior("prepare", view).unwrap());
            *cb.borrow_mut() = Some(view.capture_callback(
                "callback",
                Authority {
                    access: Access::Write,
                    scope: Scope::Keys(keys.clone()),
                },
            )?);
            Ok(vec![])
        }
    });
    let wh = callbacks.register(wb.borrow_mut().take().unwrap()).unwrap();
    let alias_wh = callbacks.register(wh.behavior().clone()).unwrap();
    let nh = callbacks.register(nb.borrow_mut().take().unwrap()).unwrap();
    let callback = cb.borrow_mut().take().unwrap();
    drop(callback);
    let request = nethacked_timers::Request::Schedule {
        callback: "callback".into(),
        payload: vec![1],
        targets: vec![wt, nt],
        deadline: vec![1],
        residency: vec![1],
        ceiling: Authority {
            access: Access::Write,
            scope: Scope::Keys(h.keys.clone()),
        },
    };
    let payload = [vec![1], serde_json::to_vec(&request).unwrap()].concat();
    let response = h
        .kernel
        .invoke_operation(
            &h.operation,
            &payload,
            Authority {
                access: Access::Write,
                scope: Scope::Keys(h.keys.clone()),
            },
        )
        .unwrap();
    let task: nethacked_timers::TaskId = serde_json::from_slice(&response).unwrap();
    let reference = timers.persistence_references().unwrap()[0].1.clone();
    // Current future-dispatch behavior differs before first old preparation.
    let new_bytes=helper_wasm("i32.const 1024 local.get $p i32.load i32.const 100 i32.add i32.store i64.const 4398046511108");
    let manifest = Manifest {
        id: "new-algorithm".into(),
        exports: [("compute".into(), endpoint(Access::Read))].into(),
        dependencies: vec![],
        extension_points: BTreeSet::new(),
        extension: Some(Extension {
            target: base.capability.clone(),
            export: "compute".into(),
        }),
    };
    let extension = h
        .kernel
        .register_guest(manifest.clone(), &new_bytes)
        .unwrap();
    h.materials.insert(
        h.kernel.artifact(extension).unwrap(),
        Material::Guest(manifest, new_bytes),
    );
    let future_binding = CapabilityPermit {
        capability: base.capability.clone(),
        resolution: [
            base.resolution.clone(),
            vec![ProviderBinding {
                handle: extension,
                artifact: h.kernel.artifact(extension).unwrap(),
                endpoint: "compute".into(),
            }],
        ]
        .concat(),
    };
    let future: Rc<dyn WasmCodec<Vec<u8>>> = Rc::new(Domain {
        add: 20,
        effects: effects.clone(),
    });
    w.admit_compatible_wasm(
        &h.kernel,
        [1; 32],
        future.clone(),
        0,
        future_binding.clone(),
    )
    .unwrap();
    w.set_query_policy(new_policy()).unwrap();
    h.kernel.release_pipeline(h.lease).unwrap();
    if terminal {
        source_busy.set(true);
        let weak_w = wh.downgrade();
        let weak_n = nh.downgrade();
        let callback_w = w.clone();
        let callback_n = n.clone();
        let owner_host = h.host.clone();
        *h.action.borrow_mut() = Some(Box::new(move |view| {
            let a = callback_w
                .prepare_retained(&weak_w.for_execution(view)?, "prepare", &[], vec![], view)
                .unwrap();
            let b = callback_n
                .prepare_retained(&weak_n.for_execution(view)?, "prepare", &[], vec![], view)
                .unwrap();
            let writes = vec![callback_w.id(), callback_n.id()];
            owner_host
                .batch(
                    vec![a.erase(), b.erase()],
                    vec![],
                    &writes,
                    writes.clone(),
                    [7; 32],
                    Rc::new(AccountingFixture),
                    cap("prepare"),
                    view,
                )
                .unwrap()
                .commit(view)
                .unwrap();
            Ok(br#"{"kind":"Completed","result":[1]}"#.to_vec())
        }));
        let outcome = timers
            .deliver_next(&mut h.kernel, &[1, 1])
            .unwrap()
            .unwrap();
        assert!(outcome.cleanup_pending);
        h.action.borrow_mut().take();
        assert_eq!(w.inspect().unwrap(), bytes(17));
        assert_eq!(n.inspect().unwrap(), 55u64.to_le_bytes());
    }
    let mut roots = ExecutionRoots::default();
    wh.persistence_roots(&mut roots).unwrap();
    alias_wh.persistence_roots(&mut roots).unwrap();
    nh.persistence_roots(&mut roots).unwrap();
    let live = roots.live_operations(&h.kernel).unwrap();
    assert!(live.is_empty());
    let live = timers.persistence_enrollment().unwrap();
    let ks = h
        .kernel
        .kernel_snapshot_with_service_roots(
            &roots.enrollment(&live),
            &timers.persistence_cleanup().unwrap(),
            &timers.persistence_resources().unwrap(),
        )
        .unwrap();
    let index = h.kernel.execution_capture_index(&ks).unwrap();
    let mut wc = CaptureCatalog::default();
    wc.register(
        identity(1),
        old_policy(),
        Backend::Wasm {
            codec: old,
            slot: 0,
            binding: base.clone(),
        },
    )
    .unwrap();
    wc.register(
        identity(2),
        new_policy(),
        Backend::Wasm {
            codec: future,
            slot: 0,
            binding: future_binding.clone(),
        },
    )
    .unwrap();
    let mut nc = CaptureCatalog::default();
    nc.register(
        identity(6),
        QueryPolicy::Noncacheable,
        Backend::Wasm {
            codec: native,
            slot: 0,
            binding: base.clone(),
        },
    )
    .unwrap();
    let mut owners = h.host.capture_execution(&index).unwrap();
    let mut capture =
        adapter::Capture::new(targets.clone(), callbacks.clone(), adapter_identity()).unwrap();
    capture
        .owner(
            &w,
            "prepare",
            &[&wh, &alias_wh],
            &mut owners,
            &StateCodec,
            &wc,
        )
        .unwrap();
    capture
        .owner(&n, "prepare", &[&nh], &mut owners, &NativeDomain, &nc)
        .unwrap();
    let callback_id = ks.callbacks[0].id;
    let callback = ks.callbacks[0].clone();
    let bundle = capture.bundle(reference.clone(), callback_id).unwrap();
    assert_eq!(
        capture.bundle(reference.clone(), callback_id).unwrap(),
        bundle
    );
    let mut tc = nethacked_timers::persistence::CaptureCatalog::new(
        &timers,
        timer_identity("clock"),
        adapter_identity(),
        Some(adapter_identity()),
    )
    .unwrap();
    for p in timers.persistence_participants().unwrap() {
        tc.participant(timer_identity("owners-cleanup"), p).unwrap();
    }
    tc.reference(bundle, reference.clone()).unwrap();
    let timer_checkpoint = timers.capture_local(&index, &tc).unwrap();
    let timer_trusted = TimerApproved(timer_checkpoint.clone());
    let timer_wire = timer_checkpoint.to_bytes(LIMIT).unwrap();
    let c = capture
        .finish(index.namespace(), index.module(timer).unwrap())
        .unwrap();
    let oc = owners.finish().unwrap();
    let base_ids = base
        .resolution
        .iter()
        .map(|b| index.module(b.handle).unwrap())
        .collect::<Vec<_>>();
    let future_ids = future_binding
        .resolution
        .iter()
        .map(|b| index.module(b.handle).unwrap())
        .collect::<Vec<_>>();
    let wid = w.id();
    let nid = n.id();
    let contexts = ks.operation_contexts().unwrap();
    let known_grants = ks.registry.grants.clone();
    let known_cleanups = ks.cleanup_history.clone();
    let materials = h.materials.clone();
    let trusted = AdapterApproved(c.clone());
    let ow = oc.to_bytes(LIMIT).unwrap();
    let aw = c.to_bytes(LIMIT).unwrap();
    let kw = nethacked_workflows::persistence::encode_local_checkpoint(&ks, LIMIT).unwrap();
    assert_eq!(effects.get(), if terminal { 2 } else { 0 });
    drop(tc);
    drop(timers);
    drop(reference);
    drop(callback);
    drop(wh);
    drop(alias_wh);
    drop(nh);
    drop(callbacks);
    drop(targets);
    drop(w);
    drop(n);
    drop(h);
    NATIVE_EXECUTIONS.with(|n| n.set(0));
    let oc = ExecutionCheckpoint::from_bytes(&ow, LIMIT).unwrap();
    let c = adapter::Checkpoint::from_bytes(&aw, LIMIT).unwrap();
    let ks: KernelSnapshot =
        nethacked_workflows::persistence::decode_local_checkpoint(&kw, LIMIT).unwrap();
    for bad_case in 0..4 {
        let mut bad = c.clone();
        match bad_case {
            0 => bad.bundles[0].scopes.swap(0, 2),
            1 => bad.routes[0].behavior = bad.routes[2].behavior.clone(),
            2 => bad.targets[0].revision += 1,
            _ => bad.identity.version += 1,
        };
        assert!(bad
            .validate(&ks, &oc, &adapter_identity(), &trusted)
            .is_err());
    }
    let stage = c.validate(&ks, &oc, &adapter_identity(), &trusted).unwrap();
    let proxy = Rc::new(Proxy {
        old: trusted.0.targets.clone(),
        adapter: RefCell::new(None),
    });
    let mut timer_catalog = nethacked_timers::persistence::RestoreCatalog::new(
        timer_identity("clock"),
        Rc::new(timer_support::Clock::default()),
        adapter_identity(),
        proxy.clone(),
        proxy.clone(),
    )
    .unwrap();
    timer_catalog
        .retention(adapter_identity(), proxy.clone(), proxy.clone())
        .unwrap();
    let cleanup_slot: Rc<RefCell<Option<Rc<dyn nethacked_workflows::CleanupParticipant>>>> =
        Rc::new(RefCell::new(None));
    let factory = cleanup_slot.clone();
    timer_catalog
        .participant(timer_identity("owners-cleanup"), move |identity| {
            let p = factory
                .borrow()
                .clone()
                .ok_or(nethacked_kernel::Error::Authority)?;
            if p.identity()? != identity {
                return Err(nethacked_kernel::Error::Authority);
            }
            Ok(p)
        })
        .unwrap();
    let timer_checkpoint =
        nethacked_timers::persistence::Checkpoint::from_bytes(&timer_wire, LIMIT).unwrap();
    let timer_stage = timer_checkpoint
        .validate(
            &ks,
            &timer_catalog,
            &timer_trusted,
            &nethacked_timers::Limits::default(),
        )
        .unwrap();
    let reservation = OwnerIdentityReservation::new(
        oc.clone(),
        &ks,
        Limits::default(),
        PublicationLimits::default(),
    )
    .unwrap();
    let action = Rc::new(RefCell::new(None));
    let host_authority = Approved {
        reservation: &reservation,
        materials: &materials,
        contexts: &contexts,
        action: action.clone(),
        factories: Cell::new(0),
        deny: false,
        timer: Some(timer_catalog.native_port()),
        grants: &known_grants,
        cleanups: &known_cleanups,
    };
    let mut kernel = Kernel::new(KernelLimits::default()).unwrap();
    let mapped = kernel.restore_kernel(&ks, &host_authority).unwrap();
    drop(host_authority);
    let mut restored_owners = reservation.bind(&kernel, &mapped).unwrap();
    let calls = Rc::new(Cell::new(0));
    let catalog = factory_catalog(
        calls.clone(),
        &base,
        &base_ids,
        &future_binding,
        &future_ids,
        true,
    );
    let fresh_w = restored_owners
        .owner(&kernel, &mapped, wid, &StateCodec, &catalog)
        .unwrap();
    let mut native_catalog = RestoreCatalog::default();
    native_catalog
        .allow(identity(6), QueryPolicy::Noncacheable, {
            let base = base.clone();
            let ids = base_ids.clone();
            move |r| {
                let binding = CapabilityPermit {
                    capability: base.capability.clone(),
                    resolution: base
                        .resolution
                        .iter()
                        .zip(&ids)
                        .map(|(b, id)| r.binding(b, *id))
                        .collect::<Result<_>>()?,
                };
                Ok(Backend::Wasm {
                    codec: Rc::new(OldNativeCodec),
                    slot: 0,
                    binding,
                })
            }
        })
        .unwrap();
    let fresh_n = restored_owners
        .owner(&kernel, &mapped, nid, &NativeDomain, &native_catalog)
        .unwrap();
    let mut failed = c
        .validate(&ks, &oc, &adapter_identity(), &trusted)
        .unwrap()
        .rebuild(&kernel, &mapped, &restored_owners)
        .unwrap();
    let lifecycle = kernel.lifecycle_state();
    assert!(failed
        .target(
            &c.routes[0].target,
            &fresh_n,
            "prepare",
            &mapped,
            &restored_owners,
            &kernel
        )
        .is_err());
    drop(failed);
    assert_eq!(kernel.lifecycle_state(), lifecycle);
    let mut adapter_restore = stage.rebuild(&kernel, &mapped, &restored_owners).unwrap();
    adapter_restore
        .target(
            &c.routes[0].target,
            &fresh_w,
            "prepare",
            &mapped,
            &restored_owners,
            &kernel,
        )
        .unwrap();
    adapter_restore
        .target(
            &c.routes[2].target,
            &fresh_n,
            "prepare",
            &mapped,
            &restored_owners,
            &kernel,
        )
        .unwrap();
    let fresh_wh = adapter_restore
        .behavior::<Vec<u8>>(1, &restored_owners, &kernel, &mapped)
        .unwrap();
    let alias_fresh_wh = adapter_restore
        .behavior::<Vec<u8>>(2, &restored_owners, &kernel, &mapped)
        .unwrap();
    let fresh_nh = adapter_restore
        .behavior::<u64>(3, &restored_owners, &kernel, &mapped)
        .unwrap();
    let adapter = adapter_restore.finish().unwrap();
    let owner_host = restored_owners.finish().unwrap();
    *proxy.adapter.borrow_mut() = Some(adapter.clone());
    let destination_busy = Rc::new(Cell::new(terminal));
    *cleanup_slot.borrow_mut() = Some(Rc::new(BusyHost {
        host: owner_host.clone(),
        busy: destination_busy.clone(),
    }));
    let fresh_timers = timer_stage
        .rebuild(&kernel, &mapped, &timer_catalog)
        .unwrap();
    assert_eq!(calls.get(), 0);
    NATIVE_EXECUTIONS.with(|n| assert_eq!(n.get(), 0));
    let weak_w = fresh_wh.downgrade();
    let weak_n = fresh_nh.downgrade();
    drop(fresh_wh);
    drop(alias_fresh_wh);
    drop(fresh_nh);
    assert!(weak_w.is_retained());
    assert!(weak_n.is_retained());

    if terminal {
        let fresh = *fresh_timers.state().unwrap().tasks.keys().next().unwrap();
        assert_eq!(fresh_w.inspect().unwrap(), bytes(17));
        assert_eq!(fresh_n.inspect().unwrap(), 55u64.to_le_bytes());
        let prefix = owner_host.persistence_history().unwrap();
        assert_eq!(prefix.len(), 1);
        assert!(fresh_timers
            .deliver_next(&mut kernel, &[9, 1])
            .unwrap()
            .is_none());
        assert!(fresh_timers.retry_cleanup(fresh).is_err());
        destination_busy.set(false);
        fresh_timers.retry_cleanup(fresh).unwrap();
        assert_eq!(owner_host.persistence_history().unwrap(), prefix);
        assert_eq!(fresh_w.inspect().unwrap(), bytes(17));
        assert_eq!(fresh_n.inspect().unwrap(), 55u64.to_le_bytes());
        assert_eq!(calls.get(), 0);
        NATIVE_EXECUTIONS.with(|n| assert_eq!(n.get(), 0));
        fresh_timers.dispose(&mut kernel, fresh).unwrap();
        return;
    }
    let result = Rc::new(RefCell::new(None));
    let sink = result.clone();
    let w = fresh_w.clone();
    let n = fresh_n.clone();
    let weak_w_call = weak_w;
    let weak_n_call = weak_n;
    *action.borrow_mut() = Some(Box::new(move |view| {
        let old_w = weak_w_call.for_execution(view)?;
        let old_n = weak_n_call.for_execution(view)?;
        let a = w
            .prepare_retained(&old_w, "prepare", &bytes(1), vec![], view)
            .unwrap();
        let b = n
            .prepare_retained(&old_n, "prepare", &bytes(1), vec![], view)
            .unwrap();
        *sink.borrow_mut() = Some((a.receipt().clone(), b.receipt().clone()));
        Ok(br#"{"kind":"Completed","result":[1]}"#.to_vec())
    }));
    let actor_id = ks
        .registry
        .modules
        .iter()
        .find(|m| m.manifest.id == "timer-actor")
        .unwrap()
        .id;
    let fresh_actor = mapped.modules()[&actor_id];
    assert_eq!(
        kernel.unregister(fresh_actor),
        Err(nethacked_kernel::Error::Referenced)
    );
    let delivered = fresh_timers
        .deliver_next(&mut kernel, &[1, 1])
        .unwrap()
        .unwrap();
    assert_eq!(delivered.id.serial, task.serial);
    assert!(!delivered.cleanup_pending);
    assert!(fresh_timers
        .deliver_next(&mut kernel, &[1, 1])
        .unwrap()
        .is_none());
    assert!(result.borrow().is_some());
    assert_eq!(calls.get(), 2);
    NATIVE_EXECUTIONS.with(|n| assert_eq!(n.get(), 2));
    // Weak routing expires when the independently stored owner is dropped.
    let old_route = result.borrow().as_ref().unwrap().0.behavior.clone();
    let w = fresh_w.clone();
    let future_receipt = Rc::new(RefCell::new(None));
    let future_sink = future_receipt.clone();
    *action.borrow_mut() = Some(Box::new(move |view| {
        let next = w.prepare("prepare", &[], vec![], view).unwrap();
        *future_sink.borrow_mut() = Some(next.receipt().clone());
        Ok(vec![])
    }));
    let auth = Authority {
        access: Access::Write,
        scope: timer_checkpoint.tasks[0].ceiling.scope.clone(),
    };
    let auth = Authority {
        access: auth.access,
        scope: match auth.scope {
            Scope::Keys(keys) => Scope::Keys(
                keys.iter()
                    .map(|r| mapped.execution_resource(r).unwrap().clone())
                    .collect(),
            ),
            Scope::All => Scope::All,
        },
    };
    let (op, lease) = kernel
        .capture_operation(fresh_actor, "run", auth.clone())
        .unwrap();
    let next = kernel
        .operation_binding_with_cleanup(
            op,
            lease,
            0,
            "future",
            &[owner_host.cleanup_identity().unwrap()],
        )
        .unwrap();
    kernel.invoke_operation(&next, &[0], auth).unwrap();
    assert_ne!(
        future_receipt.borrow().as_ref().unwrap().behavior,
        old_route
    );
    kernel.release_pipeline(lease).unwrap();
    let address = fresh_n.resource("prepare").unwrap();
    drop(action.borrow_mut().take());
    drop(fresh_n);
    assert!(adapter.targets().current(&address).is_err());
    fresh_timers.dispose(&mut kernel, delivered.id).unwrap();
    drop(mapped);
    assert_eq!(kernel.unregister(fresh_actor), Ok(()));
}
