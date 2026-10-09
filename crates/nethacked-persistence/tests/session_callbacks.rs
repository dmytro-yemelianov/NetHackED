#![cfg(feature = "session")]
#![allow(dead_code)]
include!("../../nethacked-owner-services/tests/support/persistence.rs");
use nethacked_kernel::CleanupIdentity;
use nethacked_owner_services::{persistence as adapter, CallbackBehaviors, Targets};
use nethacked_persistence::{runtime, session as composed, SaveLimits};
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
struct CurrentNativeCodec;
impl Schema<u64> for CurrentNativeCodec {
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
impl WasmCodec<u64> for CurrentNativeCodec {
    fn input(&self, state: &u64, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        NATIVE_EXECUTIONS.with(|n| n.set(n.get() + 1));
        Ok(bytes(
            u32::try_from(*state)
                .ok()
                .and_then(|v| v.checked_add(30))
                .ok_or(Error::Invariant)?,
        ))
    }
    fn decode(&self, b: &[u8]) -> Result<Proposal<u64>> {
        OldNativeCodec.decode(b)
    }
}
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

#[test]
fn queued_disposed_source_and_terminal_busy_restore_old_closure() {
    callbacks_roundtrip(false);
    callbacks_roundtrip(true);
}
fn callbacks_roundtrip(terminal: bool) {
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
    let current_native: Rc<dyn WasmCodec<u64>> = Rc::new(CurrentNativeCodec);
    n.admit_compatible_wasm(
        &h.kernel,
        [1; 32],
        current_native.clone(),
        0,
        future_binding.clone(),
    )
    .unwrap();
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
    // Independent private-field charge owner, sharing the real owner host.
    let field_backend: Rc<dyn NativeBehavior<FieldState>> = Rc::new(FieldDomain);
    let charge = h
        .host
        .enroll(
            &h.kernel,
            h.spec(3, vec![]),
            FieldState {
                private: [3, 0, 0, 0],
            },
            Backend::Native(field_backend.clone()),
        )
        .unwrap();
    let input_calls = Rc::new(Cell::new(0));
    let policy = queue_policy(input_calls.clone());
    let input_limits = nethacked_inputs::Limits::default();
    let inputs =
        nethacked_inputs::Service::new(input_limits.clone(), policy.clone(), vec![10, 20, 30])
            .unwrap();
    let input_module = inputs.install(&mut h.kernel, "recorded-inputs").unwrap();
    let input_identity = serde_json::to_vec(
        &serde_json::to_value((
            "nethacked-inputs-v1",
            env!("CARGO_PKG_VERSION"),
            &policy.identity,
            policy.version,
            &input_limits,
        ))
        .unwrap(),
    )
    .unwrap();
    h.materials.insert(
        h.kernel.artifact(input_module).unwrap(),
        Material::Native(
            h.kernel.manifest(input_module).unwrap().clone(),
            input_identity,
        ),
    );
    let flow_bytes = paused_guest("old", "", 10);
    let flow_manifest = Manifest {
        id: "paused-flow".into(),
        exports: [("step".into(), endpoint(Access::Write))].into(),
        dependencies: vec![
            KernelDependency {
                capability: Capability {
                    provider: "recorded-inputs".into(),
                    endpoint: "input".into(),
                    major: 1,
                },
                access: Access::Write,
            },
            KernelDependency {
                capability: cap("prepare"),
                access: Access::Write,
            },
        ],
        extension_points: BTreeSet::new(),
        extension: None,
    };
    let flow = h
        .kernel
        .register_guest(flow_manifest.clone(), &flow_bytes)
        .unwrap();
    h.materials.insert(
        h.kernel.artifact(flow).unwrap(),
        Material::Guest(flow_manifest, flow_bytes),
    );
    let charge_keys: BTreeSet<Resource> = [
        charge.resource("prepare").unwrap(),
        h.host.publication_resource(),
        Resource {
            owner: "recorded-inputs".into(),
            key: "source".into(),
        },
    ]
    .into();
    for slot in 0..2 {
        h.kernel
            .issue_grant(
                flow,
                slot,
                Authority {
                    access: Access::Write,
                    scope: Scope::Keys(charge_keys.clone()),
                },
            )
            .unwrap();
    }
    let workflows =
        nethacked_workflows::Workflows::new(nethacked_workflows::Limits::default()).unwrap();
    let participant = timers.persistence_participants().unwrap()[0].clone();
    let operation = workflows
        .start(
            &mut h.kernel,
            nethacked_workflows::Program {
                provider: flow,
                endpoint: "step".into(),
                authority: Authority {
                    access: Access::Write,
                    scope: Scope::Keys(charge_keys),
                },
                initial: "select".into(),
                phases: ["select".into(), "reply".into()].into(),
                allowed: [("select".into(), "reply".into())].into(),
            },
            vec![],
            vec![participant.clone()],
        )
        .unwrap();
    let c = charge.clone();
    let host = h.host.clone();
    *h.action.borrow_mut() = Some(Box::new(move |view| {
        let prepared = c.prepare("prepare", &[1], vec![], view).unwrap();
        host.batch(
            vec![prepared.erase()],
            vec![],
            &[c.id()],
            vec![c.id()],
            [8; 32],
            Rc::new(AccountingFixture),
            cap("prepare"),
            view,
        )
        .unwrap()
        .commit(view)
        .unwrap();
        Ok(vec![])
    }));
    source_busy.set(false);
    let paused = workflows.step(&h.kernel, operation, &[]).unwrap();
    assert!(
        matches!(paused, nethacked_workflows::Status::Suspended(_)),
        "{paused:?}; terminal={terminal}"
    );
    h.action.borrow_mut().take();
    assert_eq!(input_calls.get(), 1);
    assert_eq!(charge.inspect().unwrap(), vec![4, 0, 0, 0]);
    let mut workflow_capture = nethacked_workflows::persistence::CaptureCatalog::new(&workflows);
    workflow_capture
        .program(
            operation,
            timer_identity("paused-program"),
            Rc::new(workflow_support::Opaque),
        )
        .unwrap();
    workflow_capture
        .participant(timer_identity("owners-cleanup"), participant)
        .unwrap();
    // A separate terminal workflow retains cleanup while the same host is Busy.
    source_busy.set(true);
    let terminal_operation = workflows
        .start(
            &mut h.kernel,
            nethacked_workflows::Program {
                provider: flow,
                endpoint: "step".into(),
                authority: Authority {
                    access: Access::Write,
                    scope: Scope::All,
                },
                initial: "reply".into(),
                phases: ["reply".into()].into(),
                allowed: BTreeSet::new(),
            },
            vec![],
            vec![timers.persistence_participants().unwrap()[0].clone()],
        )
        .unwrap();
    assert!(matches!(
        workflows.step(&h.kernel, terminal_operation, &[]).unwrap(),
        nethacked_workflows::Status::Faulted(_)
    ));
    assert!(
        workflows
            .inspect(terminal_operation)
            .unwrap()
            .cleanup_pending
    );
    workflow_capture
        .program(
            terminal_operation,
            timer_identity("terminal-program"),
            Rc::new(workflow_support::Opaque),
        )
        .unwrap();
    let mut charge_catalog = CaptureCatalog::default();
    charge_catalog
        .register(
            field_identity(),
            QueryPolicy::Noncacheable,
            Backend::Native(field_backend),
        )
        .unwrap();
    // A disjoint sealed heterogeneous batch carries readonly and input evidence.
    let prep_effects = Rc::new(PreparedEffects::default());
    let nb: Rc<dyn NativeBehavior<u64>> = Rc::new(RecordedNative(prep_effects.clone()));
    let bb: Rc<dyn NativeBehavior<Vec<u8>>> = Rc::new(NativeBlob);
    let fb: Rc<dyn NativeBehavior<FieldState>> = Rc::new(FieldDomain);
    let wb: Rc<dyn WasmCodec<Vec<u8>>> = Rc::new(Domain {
        add: 20,
        effects: Rc::new(Cell::new(0)),
    });
    let rb: Rc<dyn NativeBehavior<u64>> = Rc::new(NativeDomain);
    let pn = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            9_u64,
            Backend::Native(nb.clone()),
        )
        .unwrap();
    let pb = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            bytes(6),
            Backend::Native(bb.clone()),
        )
        .unwrap();
    let pf = h
        .host
        .enroll(
            &h.kernel,
            h.spec(3, vec![]),
            FieldState {
                private: [3, 0, 0, 0],
            },
            Backend::Native(fb.clone()),
        )
        .unwrap();
    let pw = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            bytes(5),
            Backend::Wasm {
                codec: wb.clone(),
                slot: 0,
                binding: future_binding.clone(),
            },
        )
        .unwrap();
    let rd = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            9_u64,
            Backend::Native(rb.clone()),
        )
        .unwrap();
    pw.set_query_policy(new_policy()).unwrap();
    pn.declare_recorded_inputs(
        "prepare",
        vec![RecordedDeclaration {
            policy: "queue".into(),
            version: 1,
            provider: ProviderBinding {
                handle: input_module,
                artifact: h.kernel.artifact(input_module).unwrap(),
                endpoint: "input".into(),
            },
            request: b"prepared-choice".to_vec(),
        }],
    )
    .unwrap();
    for name in ["query", "prepare", "derived"] {
        for resource in [
            pn.resource(name).unwrap(),
            pb.resource(name).unwrap(),
            pf.resource(name).unwrap(),
            pw.resource(name).unwrap(),
            rd.resource(name).unwrap(),
        ] {
            h.keys.insert(resource);
        }
    }
    h.keys.insert(Resource {
        owner: "recorded-inputs".into(),
        key: "source".into(),
    });
    let prep_bytes=wat::parse_str(r#"(module (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32))) (memory (export "memory") 1) (func (export "alloc") (param i32) (result i32) i32.const 0) (func (export "run") (param $p i32) (param $n i32) (result i64) (local $r i32) local.get $p i32.load8_u local.get $p i32.const 1 i32.add local.get $n i32.const 1 i32.sub i32.const 2048 i32.const 16384 call $call local.tee $r i32.const 0 i32.lt_s if unreachable end i64.const 8796093022208 local.get $r i64.extend_i32_u i64.or))"#).unwrap();
    let prep_manifest = Manifest {
        id: "prepared-caller".into(),
        exports: [("run".into(), endpoint(Access::Write))].into(),
        dependencies: vec![
            KernelDependency {
                capability: cap("query"),
                access: Access::Read,
            },
            KernelDependency {
                capability: cap("prepare"),
                access: Access::Write,
            },
            KernelDependency {
                capability: cap("derived"),
                access: Access::Read,
            },
            KernelDependency {
                capability: Capability {
                    provider: "recorded-inputs".into(),
                    endpoint: "input".into(),
                    major: 1,
                },
                access: Access::Write,
            },
        ],
        extension_points: BTreeSet::new(),
        extension: None,
    };
    let prep_root = h
        .kernel
        .register_guest(prep_manifest.clone(), &prep_bytes)
        .unwrap();
    h.materials.insert(
        h.kernel.artifact(prep_root).unwrap(),
        Material::Guest(prep_manifest, prep_bytes),
    );
    for slot in 0..4 {
        h.kernel
            .issue_grant(
                prep_root,
                slot,
                Authority {
                    access: if slot == 0 || slot == 2 {
                        Access::Read
                    } else {
                        Access::Write
                    },
                    scope: Scope::Keys(h.keys.clone()),
                },
            )
            .unwrap();
    }
    let (prep_op, prep_lease) = h
        .kernel
        .capture_operation(
            prep_root,
            "run",
            Authority {
                access: Access::Write,
                scope: Scope::All,
            },
        )
        .unwrap();
    h.operation = h
        .kernel
        .operation_binding_with_cleanup(
            prep_op,
            prep_lease,
            1,
            "prepared",
            &[h.host.cleanup_identity().unwrap()],
        )
        .unwrap();
    let mut request = vec![3];
    request.extend(
        serde_json::to_vec(&nethacked_inputs::Request::Sample {
            request: b"prepared-choice".to_vec(),
        })
        .unwrap(),
    );
    let reply: nethacked_inputs::Reply = serde_json::from_slice(
        &h.kernel
            .invoke_operation(
                &h.operation,
                &request,
                Authority {
                    access: Access::Write,
                    scope: Scope::All,
                },
            )
            .unwrap(),
    )
    .unwrap();
    assert_eq!(reply.value, vec![20]);
    assert_eq!(input_calls.get(), 2);
    let readonly = Rc::new(RefCell::new(None));
    let out = readonly.clone();
    let r = rd.clone();
    h.run(0, move |view| {
        *out.borrow_mut() = Some(r.query("query", &[], &[], view).unwrap());
        Ok(vec![])
    });
    let readonly = readonly.borrow_mut().take().unwrap();
    let output = Rc::new(RefCell::new(None));
    let out = output.clone();
    let (prep_n, prep_b, prep_f, prep_w, service, host) = (
        pn.clone(),
        pb.clone(),
        pf.clone(),
        pw.clone(),
        inputs.clone(),
        h.host.clone(),
    );
    let accounting: Rc<dyn Accounting> = Rc::new(MixedAccounting);
    let a = accounting.clone();
    h.run(1, move |view| {
        let input = service
            .observation(reply.id, b"prepared-choice", view)
            .unwrap();
        let n = prep_n
            .prepare_recorded(None, "prepare", &[], vec![], &[input], view)
            .unwrap();
        let b = prep_b.prepare("prepare", &[], vec![], view).unwrap();
        let f = prep_f.prepare("prepare", &[1], vec![], view).unwrap();
        let w = prep_w.prepare("prepare", &[], vec![], view).unwrap();
        let writes = vec![
            n.receipt().stamp.owner,
            b.receipt().stamp.owner,
            f.receipt().stamp.owner,
            w.receipt().stamp.owner,
        ];
        *out.borrow_mut() = Some(
            host.batch(
                vec![n.erase(), b.erase(), f.erase(), w.erase()],
                vec![readonly.clone()],
                &writes,
                writes.clone(),
                [29; 32],
                a.clone(),
                cap("prepare"),
                view,
            )
            .unwrap(),
        );
        Ok(vec![])
    });
    let batch = output.borrow_mut().take().unwrap();
    let serials = batch.persistence_candidate_serials();
    let mut pc_n = CaptureCatalog::default();
    pc_n.register(identity(6), QueryPolicy::Noncacheable, Backend::Native(nb))
        .unwrap();
    let mut pc_b = CaptureCatalog::default();
    pc_b.register(identity(7), QueryPolicy::Noncacheable, Backend::Native(bb))
        .unwrap();
    let mut pc_f = CaptureCatalog::default();
    pc_f.register(
        field_identity(),
        QueryPolicy::Noncacheable,
        Backend::Native(fb),
    )
    .unwrap();
    let mut pc_w = CaptureCatalog::default();
    pc_w.register(
        identity(2),
        new_policy(),
        Backend::Wasm {
            codec: wb,
            slot: 0,
            binding: future_binding.clone(),
        },
    )
    .unwrap();
    let mut pc_r = CaptureCatalog::default();
    pc_r.register(identity(4), QueryPolicy::Noncacheable, Backend::Native(rb))
        .unwrap();
    let mut ac = AccountingCaptureCatalog::default();
    ac.register(mixed_id(), [29; 32], accounting).unwrap();
    let mut roots = ExecutionRoots::default();
    wh.persistence_roots(&mut roots).unwrap();
    alias_wh.persistence_roots(&mut roots).unwrap();
    nh.persistence_roots(&mut roots).unwrap();
    roots.batch(&batch).unwrap();
    let mut live = timers.persistence_enrollment().unwrap();
    live.extend(workflows.persistence_enrollment(&h.kernel).unwrap());
    live.extend(inputs.persistence_operations_for(&h.kernel).unwrap());
    let ks = h
        .kernel
        .kernel_snapshot_with_service_roots(
            &roots.enrollment(&live),
            &timers.persistence_cleanup().unwrap(),
            &timers.persistence_resources().unwrap(),
        )
        .unwrap();
    let index = h.kernel.execution_capture_index(&ks).unwrap();
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
        identity(8),
        QueryPolicy::Noncacheable,
        Backend::Wasm {
            codec: current_native,
            slot: 0,
            binding: future_binding.clone(),
        },
    )
    .unwrap();
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
    tc.reference(1, reference.clone()).unwrap();
    let resource_effects = Rc::new(resource_support::Effects::default());
    let resources = nethacked_resources::Service::new(
        nethacked_resources::Limits::default(),
        resource_support::predicate(resource_effects.clone(), true),
    )
    .unwrap();
    let resource_module = resources.install(&mut h.kernel, "resources").unwrap();
    let resource_identity = serde_json::to_vec(
        &serde_json::to_value((
            "nethacked-resources-ports-v1",
            env!("CARGO_PKG_VERSION"),
            nethacked_resources::Limits::default(),
        ))
        .unwrap(),
    )
    .unwrap();
    h.materials.insert(
        h.kernel.artifact(resource_module).unwrap(),
        Material::Native(
            h.kernel.manifest(resource_module).unwrap().clone(),
            resource_identity,
        ),
    );
    let mut resource_capture = nethacked_resources::persistence::CaptureCatalog::new(&resources);
    resource_capture
        .accounting(resource_support::codec(
            nethacked_resources::persistence::Role::Accounting,
            &resource_effects,
        ))
        .unwrap();
    let materials = h.materials.clone();
    let weak_w = Rc::downgrade(&w);
    let enrolled: Vec<Rc<dyn composed::OwnerAllocation>> = vec![
        Rc::new(composed::TypedOwner {
            key: "p-native".into(),
            owner: pn.clone(),
            codec: Rc::new(NativeDomain),
            catalog: pc_n,
            retained: vec![],
            callbacks: vec![],
            target_port: None,
            candidates: vec![],
            batch_candidates: vec![serials[0]],
        }),
        Rc::new(composed::TypedOwner {
            key: "p-blob".into(),
            owner: pb.clone(),
            codec: Rc::new(StateCodec),
            catalog: pc_b,
            retained: vec![],
            callbacks: vec![],
            target_port: None,
            candidates: vec![],
            batch_candidates: vec![serials[1]],
        }),
        Rc::new(composed::TypedOwner {
            key: "p-field".into(),
            owner: pf.clone(),
            codec: Rc::new(FieldDomain),
            catalog: pc_f,
            retained: vec![],
            callbacks: vec![],
            target_port: None,
            candidates: vec![],
            batch_candidates: vec![serials[2]],
        }),
        Rc::new(composed::TypedOwner {
            key: "p-wasm".into(),
            owner: pw.clone(),
            codec: Rc::new(StateCodec),
            catalog: pc_w,
            retained: vec![],
            callbacks: vec![],
            target_port: None,
            candidates: vec![],
            batch_candidates: vec![serials[3]],
        }),
        Rc::new(composed::TypedOwner {
            key: "readonly".into(),
            owner: rd.clone(),
            codec: Rc::new(NativeDomain),
            catalog: pc_r,
            retained: vec![],
            callbacks: vec![],
            target_port: None,
            candidates: vec![],
            batch_candidates: vec![],
        }),
        Rc::new(composed::TypedOwner {
            key: "charge".into(),
            owner: charge.clone(),
            codec: Rc::new(FieldDomain),
            catalog: charge_catalog,
            retained: vec![],
            callbacks: vec![],
            target_port: None,
            candidates: vec![],
            batch_candidates: vec![],
        }),
        Rc::new(composed::TypedOwner {
            key: "wasm".into(),
            owner: w.clone(),
            codec: Rc::new(StateCodec),
            catalog: wc,
            retained: vec![],
            callbacks: vec![wh, alias_wh],
            target_port: Some("prepare".into()),
            candidates: vec![],
            batch_candidates: vec![],
        }),
        Rc::new(composed::TypedOwner {
            key: "native".into(),
            owner: n.clone(),
            codec: Rc::new(NativeDomain),
            catalog: nc,
            retained: vec![],
            callbacks: vec![nh],
            target_port: Some("prepare".into()),
            candidates: vec![],
            batch_candidates: vec![],
        }),
    ];
    let services = composed::Services {
        resources: Some((resources.clone(), resource_capture)),
        workflows: Some((workflows.clone(), workflow_capture)),
        inputs: Some(inputs.clone()),
        timers: Some((timers.clone(), tc)),
        adapters: Some((
            targets.clone(),
            callbacks.clone(),
            adapter_identity(),
            timer,
        )),
    };
    let empty =
        runtime::ApprovedConfiguration::new(BTreeSet::new(), BTreeSet::new(), vec![]).unwrap();
    let (source, key, _) =
        composed::Session::new(h.kernel, h.host, enrolled, vec![batch], ac, services, empty)
            .unwrap();
    let wire = source.capture(&key, SaveLimits::default()).unwrap();
    drop(roots);
    drop(source);
    drop(resources);
    drop(pn);
    drop(pb);
    drop(pf);
    drop(pw);
    drop(rd);
    drop(policy);
    drop(workflows);
    drop(inputs);
    drop(charge);
    drop(w);
    drop(n);
    drop(timers);
    drop(callbacks);
    drop(targets);
    drop(reference);
    drop(h.action);
    assert!(weak_w.upgrade().is_none());
    let effects = Rc::new(Cell::new(0));
    let busy = Rc::new(Cell::new(true));
    let destination_preparations = Rc::new(PreparedEffects::default());
    let resolutions = Rc::new(Cell::new(0));
    // Refresh only from an accepted local capture. Each invocation constructs
    // fresh immutable typed catalogs; it never examines an untrusted load input.
    let missing_catalog = Cell::new("");
    let configuration = |trusted_wire: &[u8]| {
        let trusted_adapter = trusted_section::<adapter::Checkpoint>(trusted_wire, "adapters");
        let trusted_timer =
            trusted_section::<nethacked_timers::persistence::Checkpoint>(trusted_wire, "timers");
        let approved = trusted_section::<KernelSnapshot>(trusted_wire, "kernel");
        let trusted_workflow = trusted_section::<nethacked_workflows::persistence::Checkpoint>(
            trusted_wire,
            "workflows",
        );
        let trusted_resources = trusted_section::<nethacked_resources::persistence::Checkpoint>(
            trusted_wire,
            "resources",
        );
        let resource_authority = resource_support::Approved::new(&approved, &trusted_resources);
        let wf = factory_catalog(
            effects.clone(),
            &base,
            &base_ids,
            &future_binding,
            &future_ids,
            true,
        );
        let mut nf = RestoreCatalog::default();
        let b = base.clone();
        let ids = base_ids.clone();
        nf.allow(identity(6), QueryPolicy::Noncacheable, move |r| {
            Ok(Backend::Wasm {
                codec: Rc::new(OldNativeCodec),
                slot: 0,
                binding: CapabilityPermit {
                    capability: b.capability.clone(),
                    resolution: b
                        .resolution
                        .iter()
                        .zip(&ids)
                        .map(|(b, id)| r.binding(b, *id))
                        .collect::<Result<_>>()?,
                },
            })
        })
        .unwrap();
        let mut cf = RestoreCatalog::default();
        let b = future_binding.clone();
        let ids = future_ids.clone();
        nf.allow(identity(8), QueryPolicy::Noncacheable, move |r| {
            Ok(Backend::Wasm {
                codec: Rc::new(CurrentNativeCodec),
                slot: 0,
                binding: CapabilityPermit {
                    capability: b.capability.clone(),
                    resolution: b
                        .resolution
                        .iter()
                        .zip(&ids)
                        .map(|(b, id)| r.binding(b, *id))
                        .collect::<Result<_>>()?,
                },
            })
        })
        .unwrap();
        cf.allow(field_identity(), QueryPolicy::Noncacheable, |_| {
            Ok(Backend::Native(Rc::new(FieldDomain)))
        })
        .unwrap();
        let mut rc_n = RestoreCatalog::default();
        let e = destination_preparations.clone();
        rc_n.allow(identity(6), QueryPolicy::Noncacheable, move |_| {
            Ok(Backend::Native(Rc::new(RecordedNative(e.clone()))))
        })
        .unwrap();
        let mut rc_b = RestoreCatalog::default();
        rc_b.allow(identity(7), QueryPolicy::Noncacheable, |_| {
            Ok(Backend::Native(Rc::new(NativeBlob)))
        })
        .unwrap();
        let mut rc_f = RestoreCatalog::default();
        rc_f.allow(field_identity(), QueryPolicy::Noncacheable, |_| {
            Ok(Backend::Native(Rc::new(FieldDomain)))
        })
        .unwrap();
        let rc_w = factory_catalog(
            Rc::new(Cell::new(0)),
            &base,
            &base_ids,
            &future_binding,
            &future_ids,
            true,
        );
        let mut rc_r = RestoreCatalog::default();
        rc_r.allow(identity(4), QueryPolicy::Noncacheable, |_| {
            Ok(Backend::Native(Rc::new(NativeDomain)))
        })
        .unwrap();
        let mut accounting = AccountingRestoreCatalog::default();
        accounting
            .allow(mixed_id(), [29; 32], || Ok(Rc::new(MixedAccounting)))
            .unwrap();
        let mut authority = composed::RestoreAuthority {
            bindings: [composer_id()].into(),
            kernel: Rc::new(CallbackKernel {
                materials: materials.clone(),
                approved,
                resolutions: resolutions.clone(),
            }),
            owners: [
                (
                    "p-native".into(),
                    Rc::new(composed::TypedFactory {
                        codec: Rc::new(NativeDomain),
                        catalog: rc_n,
                    }) as Rc<dyn composed::OwnerFactory>,
                ),
                (
                    "p-blob".into(),
                    Rc::new(composed::TypedFactory {
                        codec: Rc::new(StateCodec),
                        catalog: rc_b,
                    }) as Rc<dyn composed::OwnerFactory>,
                ),
                (
                    "p-field".into(),
                    Rc::new(composed::TypedFactory {
                        codec: Rc::new(FieldDomain),
                        catalog: rc_f,
                    }) as Rc<dyn composed::OwnerFactory>,
                ),
                (
                    "p-wasm".into(),
                    Rc::new(composed::TypedFactory {
                        codec: Rc::new(StateCodec),
                        catalog: rc_w,
                    }) as Rc<dyn composed::OwnerFactory>,
                ),
                (
                    "readonly".into(),
                    Rc::new(composed::TypedFactory {
                        codec: Rc::new(NativeDomain),
                        catalog: rc_r,
                    }) as Rc<dyn composed::OwnerFactory>,
                ),
                (
                    "charge".into(),
                    Rc::new(composed::TypedFactory {
                        codec: Rc::new(FieldDomain),
                        catalog: cf,
                    }) as Rc<dyn composed::OwnerFactory>,
                ),
                (
                    "wasm".into(),
                    Rc::new(composed::TypedFactory {
                        codec: Rc::new(StateCodec),
                        catalog: wf,
                    }) as Rc<dyn composed::OwnerFactory>,
                ),
                (
                    "native".into(),
                    Rc::new(composed::TypedFactory {
                        codec: Rc::new(NativeDomain),
                        catalog: nf,
                    }) as Rc<dyn composed::OwnerFactory>,
                ),
            ]
            .into(),
            accounting: Rc::new(accounting),
            workflows: Some((
                Rc::new(PausedFactory {
                    checkpoint: trusted_workflow.clone(),
                    busy: busy.clone(),
                }),
                Rc::new(workflow_support::Approved(trusted_workflow)),
                nethacked_workflows::Limits::default(),
            )),
            inputs: Some((queue_catalog(effects.clone()), input_limits.clone())),
            timers: Some((
                Rc::new(CallbackTimer { busy: busy.clone() }),
                Rc::new(TimerApproved(trusted_timer)),
                nethacked_timers::Limits::default(),
            )),
            adapters: Some((
                adapter_identity(),
                Rc::new(AdapterApproved(trusted_adapter)),
            )),
            resources: Some((
                resource_support::catalog(&Rc::new(resource_support::Effects::default())),
                Rc::new(resource_authority),
                nethacked_resources::Limits::default(),
            )),
            owner_limits: Limits::default(),
            publication_limits: PublicationLimits::default(),
            limits: SaveLimits::default(),
        };
        match missing_catalog.get() {
            "workflows" => authority.workflows = None,
            "inputs" => authority.inputs = None,
            "timers" => authority.timers = None,
            "adapters" => authority.adapters = None,
            "resources" => authority.resources = None,
            "" => (),
            _ => unreachable!(),
        }
        composed::SessionFactory::new(authority)
            .configuration(composer_id())
            .unwrap()
    };
    let config = configuration(&wire);
    let host = Host::new(Limits::default()).unwrap();
    let (destination, key, old_credential) = composed::Session::new(
        Kernel::new(KernelLimits::default()).unwrap(),
        host,
        vec![],
        vec![],
        AccountingCaptureCatalog::default(),
        Default::default(),
        config,
    )
    .unwrap();
    let before = resolutions.get();
    let source_namespace = trusted_section::<KernelSnapshot>(&wire, "kernel").source_namespace;
    let bad_pair = rewrite_section(&wire, "timers", |v| {
        v["kernel_namespace"] = serde_json::json!(source_namespace + 1)
    });
    let bad_reference = rewrite_section(&wire, "timers", |v| {
        v["tasks"][0]["reference"] = serde_json::json!(u64::MAX)
    });
    for invalid in [&bad_pair, &bad_reference] {
        assert!(destination
            .prepare_restore(&key, &composer_id(), invalid)
            .is_err());
        assert_eq!(resolutions.get(), before);
        assert_eq!(effects.get(), 0);
        assert_eq!(destination_preparations.preparations.get(), 0);
    }
    let mut stage = destination
        .prepare_restore(&key, &composer_id(), &wire)
        .unwrap();
    assert_eq!(effects.get(), 0);
    assert_eq!(destination_preparations.preparations.get(), 0);
    destination.activate(&mut stage).unwrap();
    assert!(destination.with_runtime(&old_credential, |_| ()).is_err());
    let mut links = ReferenceCounter::new();
    let body: serde_json::Value =
        serde_json::from_slice(&wire[nethacked_persistence::HEADER_LEN..]).unwrap();
    let rows: Vec<composed::OwnerRow> = serde_json::from_value(body["owners"].clone()).unwrap();
    for row in rows {
        links.add(1);
        links.add(usize::from(row.target.is_some()));
        for b in row.behaviors {
            links.add(1);
            links.add(usize::from(b.scope.is_some()));
        }
        links.add(row.routes.len());
        links.add(row.candidates.len());
    }
    trusted_section::<KernelSnapshot>(&wire, "kernel").count_references(&mut links);
    trusted_section::<ExecutionCheckpoint>(&wire, "owners").count_references(&mut links);
    trusted_section::<nethacked_workflows::persistence::Checkpoint>(&wire, "workflows")
        .count_references(&mut links);
    trusted_section::<nethacked_inputs::persistence::Checkpoint>(&wire, "inputs")
        .count_references(&mut links);
    trusted_section::<nethacked_timers::persistence::Checkpoint>(&wire, "timers")
        .count_references(&mut links);
    trusted_section::<adapter::Checkpoint>(&wire, "adapters").count_references(&mut links);
    trusted_section::<nethacked_resources::persistence::Checkpoint>(&wire, "resources")
        .count_references(&mut links);
    let exact = links.total().unwrap();
    {
        let c = trusted_section::<ExecutionCheckpoint>(&wire, "owners");
        let mut encoded = serde_json::to_value(&c).unwrap();
        let behaviors = encoded["behaviors"].as_array().unwrap().clone();
        assert!(!behaviors.is_empty());
        let mut total = ReferenceCounter::new();
        c.count_references(&mut total);
        let with_behaviors = total.total().unwrap();
        encoded["behaviors"] = serde_json::json!([]);
        let without: ExecutionCheckpoint = serde_json::from_value(encoded).unwrap();
        let mut total = ReferenceCounter::new();
        without.count_references(&mut total);
        let delta = with_behaviors - total.total().unwrap();
        // Each serialized BehaviorRow refers to owner/caller/target/pin,
        // kernel pure-capture index, three qualified build identities,
        // operation/pipeline and every cleanup participant occurrence.
        let expected: usize = behaviors
            .iter()
            .map(|b| {
                assert!(b["capture"].is_u64());
                10 + b["source"]["cleanup"].as_array().unwrap().len()
            })
            .sum();
        assert_eq!(
            delta, expected,
            "every encoded pure-capture index must cost one reference"
        );
    }
    {
        let r = trusted_section::<nethacked_resources::persistence::Checkpoint>(&wire, "resources");
        let mut n = ReferenceCounter::new();
        r.count_references(&mut n);
        // Walk encoded fields independently. PolicyRow has ONE qualified
        // codec identity (name/version/artifact), not three identity fields.
        let policy_links = r.owners.len() + 1;
        let mut expected =
            1 + usize::from(r.binding.is_some()) + r.owners.len() + policy_links + r.cells.len();
        for p in &r.pending {
            expected += p.plan.readonly.len() + p.plan.writes.len();
            expected += 2; // caller/provider qualified bindings
            if let Some(o) = &p.context.operation {
                expected += 2 + o.cleanup.len();
            }
            for permit in &p.context.permits {
                expected += 1 + permit.resolution.len();
            }
        }
        let actual = n.total();
        assert_eq!(
            actual,
            Some(expected),
            "each encoded qualified resource policy codec must cost one reference"
        );
    }
    for section in ["workflows", "inputs", "timers", "adapters", "resources"] {
        missing_catalog.set(section);
        destination
            .replace_configuration(&key, configuration(&wire))
            .unwrap();
        let credential = destination.issue_credential(&key).unwrap();
        let state_before = destination.capture(&key, SaveLimits::default()).unwrap();
        let owner_before = destination
            .with_runtime(&credential, |b| {
                b.owner::<Vec<u8>>("wasm").unwrap().owner.inspect().unwrap()
            })
            .unwrap();
        let resolutions_before = resolutions.get();
        let effects_before = effects.get();
        let native_before = NATIVE_EXECUTIONS.with(Cell::get);
        let e = match destination.prepare_restore_diagnosed(&key, &composer_id(), &wire) {
            Err(e) => e,
            Ok(_) => panic!("missing catalog admitted"),
        };
        assert_eq!(e.error, runtime::Error::MissingBinding);
        assert!(matches!(
            destination.prepare_restore(&key, &composer_id(), &wire),
            Err(runtime::Error::MissingBinding)
        ));
        assert_eq!(resolutions.get(), resolutions_before);
        assert_eq!(effects.get(), effects_before);
        assert_eq!(NATIVE_EXECUTIONS.with(Cell::get), native_before);
        assert_eq!(
            destination.capture(&key, SaveLimits::default()).unwrap(),
            state_before
        );
        destination
            .with_runtime(&credential, |b| {
                assert!(b.services().inputs.is_some());
                assert_eq!(
                    b.owner::<Vec<u8>>("wasm").unwrap().owner.inspect().unwrap(),
                    owner_before
                );
            })
            .unwrap();
        let input: serde_json::Value = trusted_section(&wire, "inputs");
        let required = input["policy"].as_str().unwrap();
        assert!(
            e.diagnostics.iter().any(|d| d.section == section
                && d.dependency.as_ref().is_some_and(
                    |s| !s.is_empty() && (section != "inputs" || s.contains(required))
                )),
            "exact required input policy/catalog must be named"
        );
    }
    missing_catalog.set("");
    destination
        .replace_configuration(&key, configuration(&wire))
        .unwrap();
    assert!(destination
        .capture(
            &key,
            SaveLimits {
                aggregate_edges: exact - 1,
                ..SaveLimits::default()
            }
        )
        .is_err());
    assert!(destination
        .capture(
            &key,
            SaveLimits {
                aggregate_edges: exact,
                ..SaveLimits::default()
            }
        )
        .is_ok());
    let pending_wire = destination.capture(&key, SaveLimits::default()).unwrap();
    assert_eq!(effects.get(), 0);
    assert_eq!(destination_preparations.preparations.get(), 0);
    let credential = destination.issue_credential(&key).unwrap();
    let retired_services = destination
        .with_runtime(&credential, |b| {
            (
                b.services().workflows.as_ref().unwrap().0.clone(),
                b.services().inputs.as_ref().unwrap().clone(),
                b.services().timers.as_ref().unwrap().0.clone(),
                b.services().resources.as_ref().unwrap().0.clone(),
            )
        })
        .unwrap();
    destination
        .replace_configuration(&key, configuration(&pending_wire))
        .unwrap();
    let mut restaged = destination
        .prepare_restore(&key, &composer_id(), &pending_wire)
        .unwrap();
    assert_eq!(effects.get(), 0);
    assert_eq!(destination_preparations.preparations.get(), 0);
    let activation = destination.activate(&mut restaged).unwrap();
    assert!(retired_services.0.operations().is_err());
    assert!(retired_services.1.inspect().is_err());
    assert!(retired_services.2.state().is_err());
    assert!(retired_services.2.persistence_references().is_err());
    assert!(retired_services.3.state().is_err());
    drop(retired_services);
    destination
        .with_runtime(activation.credential(), |bundle| {
            let timers = &bundle.services().timers.as_ref().unwrap().0;
            let state = timers.state().unwrap();
            let fresh = *state.tasks.keys().next().unwrap();
            assert_ne!(fresh, task);
            assert_eq!(
                bundle
                    .owner::<Vec<u8>>("wasm")
                    .unwrap()
                    .owner
                    .inspect()
                    .unwrap(),
                bytes(if terminal { 17 } else { 5 })
            );
            assert_eq!(
                bundle.host().persistence_history().unwrap().len(),
                1 + usize::from(terminal)
            );
            if terminal {
                assert!(timers.retry_cleanup(fresh).is_err());
            }
            bundle
                .update_kernel(|kernel| {
                    if !terminal {
                        let delivered = timers.deliver_next(kernel, &[1, 1]).unwrap().unwrap();
                        assert!(delivered.cleanup_pending);
                    }
                })
                .unwrap();
            assert_eq!(bundle.host().persistence_history().unwrap().len(), 2);
            assert_eq!(
                bundle
                    .owner::<Vec<u8>>("wasm")
                    .unwrap()
                    .owner
                    .inspect()
                    .unwrap(),
                bytes(17)
            );
            assert_eq!(
                bundle
                    .owner::<u64>("native")
                    .unwrap()
                    .owner
                    .inspect()
                    .unwrap(),
                55u64.to_le_bytes()
            );
            let accepted = bundle.host().persistence_history().unwrap();
            busy.set(false);
            timers.retry_cleanup(fresh).unwrap();
            assert_eq!(bundle.host().persistence_history().unwrap().len(), 2);
            assert!(!timers.state().unwrap().tasks[&fresh].cleanup_pending);
            assert_eq!(bundle.host().persistence_history().unwrap(), accepted);
            bundle
                .update_kernel(|kernel| {
                    assert!(timers.deliver_next(kernel, &[1, 1]).unwrap().is_none())
                })
                .unwrap();
        })
        .unwrap();
    assert_eq!(effects.get(), if terminal { 0 } else { 2 });
    let credential = destination.issue_credential(&key).unwrap();
    destination
        .with_runtime(&credential, |bundle| {
            let workflows = &bundle.services().workflows.as_ref().unwrap().0;
            let fresh = workflows
                .operations()
                .unwrap()
                .into_iter()
                .find(|id| {
                    matches!(
                        workflows.inspect(*id).unwrap().status,
                        nethacked_workflows::Status::Suspended(_)
                    )
                })
                .unwrap();
            assert_ne!(fresh, operation);
            assert!(workflows.ticket(operation).is_err());
            let ticket = workflows.ticket(fresh).unwrap();
            workflows.resume(&ticket).unwrap();
            let inputs = bundle.services().inputs.as_ref().unwrap();
            let accepted = inputs.inspect().unwrap();
            let replay = serde_json::to_vec(&nethacked_inputs::Request::Replay {
                id: accepted.records[0].id,
                request: b"request".to_vec(),
            })
            .unwrap();
            let prefix = bundle.host().persistence_history().unwrap();
            assert!(matches!(
                workflows
                    .step(&bundle.kernel().unwrap(), fresh, &replay)
                    .unwrap(),
                nethacked_workflows::Status::Finished(_)
            ));
            assert_eq!(inputs.inspect().unwrap(), accepted);
            assert_eq!(bundle.host().persistence_history().unwrap(), prefix);
            assert_eq!(
                bundle
                    .owner::<FieldState>("charge")
                    .unwrap()
                    .owner
                    .inspect()
                    .unwrap(),
                vec![4, 0, 0, 0]
            );
            assert!(workflows.resume(&ticket).is_err());
            let terminal_id = workflows
                .operations()
                .unwrap()
                .into_iter()
                .find(|id| {
                    matches!(
                        workflows.inspect(*id).unwrap().status,
                        nethacked_workflows::Status::Faulted(_)
                    )
                })
                .unwrap();
            let prefix = bundle.host().persistence_history().unwrap();
            workflows.retry_cleanup(terminal_id).unwrap();
            assert!(!workflows.inspect(terminal_id).unwrap().cleanup_pending);
            workflows.retry_cleanup(terminal_id).unwrap();
            assert_eq!(bundle.host().persistence_history().unwrap(), prefix);
            assert_eq!(effects.get(), if terminal { 0 } else { 2 });
        })
        .unwrap();
    let credential = destination.issue_credential(&key).unwrap();
    destination
        .with_runtime(&credential, |bundle| {
            let operation = bundle.owner::<u64>("p-native").unwrap().candidates[0]
                .receipt()
                .operation
                .clone();
            let inputs = bundle.services().inputs.as_ref().unwrap();
            let accepted = inputs.inspect().unwrap();
            bundle
                .kernel()
                .unwrap()
                .invoke_operation(
                    &operation,
                    &[1],
                    Authority {
                        access: Access::Write,
                        scope: Scope::All,
                    },
                )
                .unwrap();
            assert_eq!(bundle.host().persistence_history().unwrap().len(), 3);
            assert_eq!(
                bundle
                    .owner::<u64>("p-native")
                    .unwrap()
                    .owner
                    .inspect()
                    .unwrap(),
                29_u64.to_le_bytes()
            );
            assert_eq!(
                bundle
                    .owner::<Vec<u8>>("p-blob")
                    .unwrap()
                    .owner
                    .inspect()
                    .unwrap(),
                bytes(8)
            );
            assert_eq!(
                bundle
                    .owner::<FieldState>("p-field")
                    .unwrap()
                    .owner
                    .inspect()
                    .unwrap(),
                vec![4, 0, 0, 0]
            );
            assert_eq!(
                bundle
                    .owner::<Vec<u8>>("p-wasm")
                    .unwrap()
                    .owner
                    .inspect()
                    .unwrap(),
                bytes(125)
            );
            assert_eq!(inputs.inspect().unwrap(), accepted);
            assert_eq!(destination_preparations.preparations.get(), 0);
            assert!(bundle
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
    // Fresh work after source destruction, two reloads, and all pending
    // continuations. Both owners use their saved CURRENT codec/helper binding.
    destination
        .with_runtime(&credential, |bundle| {
            let w = bundle.owner::<Vec<u8>>("wasm").unwrap();
            let n = bundle.owner::<u64>("native").unwrap();
            let caller = n.callbacks[0].behavior().caller().0;
            assert_eq!(w.owner.inspect().unwrap(), bytes(17));
            assert_eq!(n.owner.inspect().unwrap(), 55u64.to_le_bytes());
            let input_before = bundle
                .services()
                .inputs
                .as_ref()
                .unwrap()
                .inspect()
                .unwrap();
            let native_before = NATIVE_EXECUTIONS.with(Cell::get);
            let effect_before = effects.get();
            let ceiling = Authority {
                access: Access::Write,
                scope: Scope::All,
            };
            bundle
                .update_kernel(|kernel| {
                    let lease = kernel
                        .capture_pipeline(caller, "run", ceiling.clone())
                        .unwrap();
                    let id = kernel.issue_operation().unwrap();
                    let operation = kernel
                        .operation_binding_with_cleanup(
                            id,
                            lease,
                            1,
                            "prepare",
                            &[bundle.host().cleanup_identity().unwrap()],
                        )
                        .unwrap();
                    kernel
                        .invoke_operation(&operation, &[0, 3], ceiling)
                        .unwrap();
                    kernel.release_pipeline(lease).unwrap();
                })
                .unwrap();
            assert_eq!(w.owner.inspect().unwrap(), bytes(137)); // 17 + current codec20 + current helper100
            assert_eq!(n.owner.inspect().unwrap(), 185u64.to_le_bytes()); // 55 + current native codec30 + helper100
            assert_eq!(NATIVE_EXECUTIONS.with(Cell::get), native_before + 2);
            assert_eq!(effects.get(), effect_before + 2); // fresh current Wasm input/decode
            assert_eq!(
                bundle
                    .services()
                    .inputs
                    .as_ref()
                    .unwrap()
                    .inspect()
                    .unwrap(),
                input_before
            );
            assert_eq!(destination_preparations.preparations.get(), 0);
            assert_eq!(bundle.host().persistence_history().unwrap().len(), 4);
        })
        .unwrap();
}
fn composer_id() -> runtime::Binding {
    runtime::Binding {
        name: "callback-composition".into(),
        version: 1,
        artifact: [27; 32],
    }
}
struct CallbackTimer {
    busy: Rc<Cell<bool>>,
}
impl composed::TimerFactory for CallbackTimer {
    fn catalog(
        &self,
        handles: Rc<composed::Handles>,
        adapter: Rc<composed::AdapterPort>,
    ) -> std::result::Result<nethacked_timers::persistence::RestoreCatalog, runtime::Error> {
        let mut c = nethacked_timers::persistence::RestoreCatalog::new(
            timer_identity("clock"),
            Rc::new(timer_support::Clock::default()),
            adapter_identity(),
            adapter.clone(),
            adapter.clone(),
        )
        .unwrap();
        c.retention(adapter_identity(), adapter.clone(), adapter)
            .unwrap();
        let busy = self.busy.clone();
        c.participant(timer_identity("owners-cleanup"), move |id| {
            let p = handles.owner_cleanup(|host| {
                Rc::new(BusyHost {
                    host,
                    busy: busy.clone(),
                })
            })?;
            if p.identity()? != id {
                return Err(nethacked_kernel::Error::Authority);
            }
            Ok(p)
        })
        .unwrap();
        Ok(c)
    }
}
struct CallbackKernel {
    materials: BTreeMap<[u8; 32], Material>,
    approved: KernelSnapshot,
    resolutions: Rc<Cell<usize>>,
}
struct CallbackResolver {
    materials: BTreeMap<[u8; 32], Material>,
    approved: KernelSnapshot,
    handles: Rc<composed::Handles>,
    ports: BTreeMap<(String, String), NativePort>,
    resolutions: Rc<Cell<usize>>,
}
impl composed::KernelFactory for CallbackKernel {
    fn resolver(
        &self,
        handles: Rc<composed::Handles>,
        ports: BTreeMap<(String, String), NativePort>,
    ) -> std::result::Result<Rc<dyn RetainedAuthority>, runtime::Error> {
        Ok(Rc::new(CallbackResolver {
            materials: self.materials.clone(),
            approved: self.approved.clone(),
            handles,
            ports,
            resolutions: self.resolutions.clone(),
        }))
    }
}
impl RegistryAuthority for CallbackResolver {
    fn resolve(&self, m: &SavedModule) -> nethacked_kernel::Result<ArtifactMaterial> {
        self.resolutions.set(self.resolutions.get() + 1);
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
                let mut ports = BTreeMap::new();
                for name in manifest.exports.keys() {
                    let port =
                        if let Some(port) = self.ports.get(&(manifest.id.clone(), name.clone())) {
                            port.clone()
                        } else {
                            let h = self.handles.clone();
                            let prepared_artifact = self
                                .approved
                                .registry
                                .modules
                                .iter()
                                .find(|m| m.manifest.id == "prepared-caller")
                                .unwrap()
                                .artifact;
                            Rc::new(move |request: &[u8], view: &CallerView| {
                                if request == [3] && view.callback_origin().is_none() {
                                    let w = h.owner::<Vec<u8>>("wasm")?;
                                    let n = h.owner::<u64>("native")?;
                                    let a = w
                                        .prepare("prepare", &[], vec![], view)
                                        .map_err(|_| nethacked_kernel::Error::NativeRefusal)?;
                                    let b = n
                                        .prepare("prepare", &[], vec![], view)
                                        .map_err(|_| nethacked_kernel::Error::NativeRefusal)?;
                                    h.host()?
                                        .batch(
                                            vec![a.erase(), b.erase()],
                                            vec![],
                                            &[w.id(), n.id()],
                                            vec![w.id(), n.id()],
                                            [7; 32],
                                            Rc::new(AccountingFixture),
                                            cap("prepare"),
                                            view,
                                        )
                                        .map_err(|_| nethacked_kernel::Error::NativeRefusal)?
                                        .commit(view)
                                        .map_err(|_| nethacked_kernel::Error::NativeRefusal)?;
                                    return Ok(vec![]);
                                }
                                if view.caller_artifact() == Some(prepared_artifact) {
                                    h.commit_prepared(view)
                                        .map_err(|_| nethacked_kernel::Error::NativeRefusal)?;
                                    return Ok(vec![]);
                                }
                                if view.callback_origin().is_none() {
                                    let c = h.owner::<FieldState>("charge")?;
                                    let p = c
                                        .prepare("prepare", &[1], vec![], view)
                                        .map_err(|_| nethacked_kernel::Error::NativeRefusal)?;
                                    h.host()?
                                        .batch(
                                            vec![p.erase()],
                                            vec![],
                                            &[c.id()],
                                            vec![c.id()],
                                            [8; 32],
                                            Rc::new(AccountingFixture),
                                            cap("prepare"),
                                            view,
                                        )
                                        .map_err(|_| nethacked_kernel::Error::NativeRefusal)?
                                        .commit(view)
                                        .map_err(|_| nethacked_kernel::Error::NativeRefusal)?;
                                    return Ok(vec![]);
                                }
                                let w = h.owner::<Vec<u8>>("wasm")?;
                                let n = h.owner::<u64>("native")?;
                                let wb = h.callback_behavior::<Vec<u8>>("wasm", 0, view)?;
                                let nb = h.callback_behavior::<u64>("native", 0, view)?;
                                let a = w
                                    .prepare_retained(&wb, "prepare", &[], vec![], view)
                                    .map_err(|_| nethacked_kernel::Error::NativeRefusal)?;
                                let b = n
                                    .prepare_retained(&nb, "prepare", &[], vec![], view)
                                    .map_err(|_| nethacked_kernel::Error::NativeRefusal)?;
                                h.host()?
                                    .batch(
                                        vec![a.erase(), b.erase()],
                                        vec![],
                                        &[w.id(), n.id()],
                                        vec![w.id(), n.id()],
                                        [7; 32],
                                        Rc::new(AccountingFixture),
                                        cap("prepare"),
                                        view,
                                    )
                                    .map_err(|_| nethacked_kernel::Error::NativeRefusal)?
                                    .commit(view)
                                    .map_err(|_| nethacked_kernel::Error::NativeRefusal)?;
                                Ok(br#"{"kind":"Completed","result":[1]}"#.to_vec())
                            }) as NativePort
                        };
                    ports.insert(name.clone(), port);
                }
                Ok(ArtifactMaterial::Native {
                    manifest: manifest.clone(),
                    identity: identity.clone(),
                    ports,
                })
            }
        }
    }
    fn bind_resource(&self, r: &Resource) -> nethacked_kernel::Result<Resource> {
        if r == &timer_support::queue_key() || (r.owner == "recorded-inputs" && r.key == "source") {
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
        if self.approved.registry.grants.iter().any(|g| {
            g.caller == caller.id && g.slot == slot && g.target == target.id && &g.requested == a
        }) {
            Ok(a.clone())
        } else {
            Err(nethacked_kernel::Error::Authority)
        }
    }
}
impl RetainedAuthority for CallbackResolver {
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

#[path = "../../nethacked-inputs/tests/support/mod.rs"]
mod input_support;
#[path = "../../nethacked-resources/tests/persistence_support/mod.rs"]
mod resource_support;
#[path = "../../nethacked-workflows/tests/support/persistence.rs"]
mod workflow_support;
use nethacked_workflows::Outcome;
const INPUT_WIRE: u32 = input_support::WIRE;
fn paused_guest(version: &str, fault: &str, replay_value: u8) -> Vec<u8> {
    let mut data = String::new();
    let mut offset = 8000;
    let mut store = |bytes: &[u8]| {
        let p = offset;
        offset += bytes.len() + 16;
        data.push_str(&format!(
            "(data (i32.const {p}) \"{}\")",
            input_support::escaped(bytes)
        ));
        (p, bytes.len())
    };
    let (select, sn) = store(b"\"select\"");
    let (reply, rn) = store(b"\"reply\"");
    let (input, inn) = store(b"\"input\":[");
    let (replay_marker, replay_marker_n) = store(b"\"kind\":\"replay\"");
    let (sp, sl) = store(&serde_json::to_vec(&input_support::sample(b"request")).unwrap());
    let (vp, vl) = store(format!("\"value\":[{replay_value}]").as_bytes());
    let suspend = Outcome::Suspend {
        next: "reply".into(),
        prompt: b"choose".to_vec(),
        state: vec![],
        events: vec![version.as_bytes().to_vec()],
    };
    let (op, on) = store(
        if fault == "malformed" {
            b"invalid".to_vec()
        } else if fault == "reject" {
            serde_json::to_vec(&Outcome::Reject { events: vec![] }).unwrap()
        } else {
            serde_json::to_vec(&suspend).unwrap()
        }
        .as_slice(),
    );
    let (fp, fl) = store(
        &serde_json::to_vec(&Outcome::Finish {
            result: version.as_bytes().to_vec(),
            state: vec![],
            events: vec![],
        })
        .unwrap(),
    );
    let trap = if fault == "trap" { "unreachable" } else { "" };
    let theft = if version == "new" {
        format!(
            "local.get $n call $decode local.set $len
        i32.const 0 i32.const 20000 local.get $len i32.const 32768 i32.const {INPUT_WIRE} call $call
        i32.const {} i32.ne if unreachable end",
            nethacked_kernel::Error::Authority.abi_code()
        )
    } else {
        String::new()
    };
    wat::parse_str(format!(r#"(module
      (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "alloc") (param i32) (result i32) i32.const 0)
      {data}
      (func $find (param $base i32) (param $n i32) (param $p i32) (param $l i32) (result i32)
        (local $i i32) (local $j i32)
        block $no loop $outer
          local.get $i local.get $l i32.add local.get $n i32.gt_u br_if $no
          i32.const 0 local.set $j
          block $mismatch loop $inner
            local.get $j local.get $l i32.eq if local.get $base local.get $i i32.add return end
            local.get $base local.get $i i32.add local.get $j i32.add i32.load8_u
            local.get $p local.get $j i32.add i32.load8_u i32.ne br_if $mismatch
            local.get $j i32.const 1 i32.add local.set $j br $inner
          end end local.get $i i32.const 1 i32.add local.set $i br $outer
        end end i32.const -1)
      (func $decode (param $n i32) (result i32) (local $p i32) (local $len i32) (local $v i32) (local $c i32)
        i32.const 0 local.get $n i32.const {input} i32.const {inn} call $find
        local.tee $p i32.const 0 i32.lt_s if unreachable end
        local.get $p i32.const {inn} i32.add local.set $p
        block $done loop $read
          local.get $p local.get $n i32.ge_u if unreachable end
          local.get $p i32.load8_u local.tee $c i32.const 93 i32.eq br_if $done
          local.get $c i32.const 44 i32.eq if
            i32.const 20000 local.get $len i32.add local.get $v i32.store8
            local.get $len i32.const 1 i32.add local.set $len i32.const 0 local.set $v
          else
            local.get $v i32.const 10 i32.mul local.get $c i32.const 48 i32.sub i32.add local.set $v
          end local.get $p i32.const 1 i32.add local.set $p br $read
        end end
        i32.const 20000 local.get $len i32.add local.get $v i32.store8
        local.get $len i32.const 1 i32.add)
      (func $sample
        i32.const 0 i32.const {sp} i32.const {sl} i32.const 32768 i32.const {INPUT_WIRE} call $call
        i32.const 0 i32.lt_s if unreachable end)
      (func (export "step") (param i32) (param $n i32) (result i64) (local $len i32) (local $r i32)
        i32.const 0 local.get $n i32.const {replay_marker} i32.const {replay_marker_n} call $find i32.const 0 i32.ge_s if
          i32.const 0 i32.const 0 local.get $n i32.const 32768 i32.const {INPUT_WIRE} call $call local.tee $r
          i32.const 0 i32.lt_s if unreachable end
          i64.const 140737488355328 local.get $r i64.extend_i32_u i64.or return end
        i32.const 0 local.get $n i32.const {select} i32.const {sn} call $find i32.const 0 i32.ge_s if
          {theft} i32.const 1 i32.const 0 i32.const 0 i32.const 30000 i32.const 16384 call $call i32.const 0 i32.lt_s if unreachable end call $sample {trap} i64.const {} return end
        i32.const 0 local.get $n i32.const {reply} i32.const {rn} call $find i32.const 0 i32.ge_s if
          local.get $n call $decode local.set $len
          i32.const 0 i32.const 20000 local.get $len i32.const 32768 i32.const {INPUT_WIRE} call $call local.tee $r
          i32.const 0 i32.lt_s if unreachable end
          i32.const 32768 local.get $r i32.const {vp} i32.const {vl} call $find
          i32.const 0 i32.lt_s if unreachable end
          i64.const {} return end unreachable))"#,
        ((op as u64)<<32)|on as u64, ((fp as u64)<<32)|fl as u64)).unwrap()
}
struct FieldState {
    private: [u8; 4],
}
struct FieldDomain;
impl Schema<FieldState> for FieldDomain {
    fn validate(&self, s: &FieldState) -> Result<()> {
        if s.private[0] > 20 {
            Err(Error::Invariant)
        } else {
            Ok(())
        }
    }
    fn encode(&self, s: &FieldState) -> Result<Vec<u8>> {
        Ok(s.private.to_vec())
    }
    fn query(&self, s: &FieldState, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        Ok(s.private.to_vec())
    }
}
impl NativeBehavior<FieldState> for FieldDomain {
    fn prepare(
        &self,
        s: &FieldState,
        _: &str,
        req: &[u8],
        _: &[Observation],
    ) -> Result<Proposal<FieldState>> {
        let mut private = s.private;
        private[0] += req[0];
        Ok(Proposal {
            next: FieldState { private },
            events: vec![],
            reply: vec![],
        })
    }
}
impl PersistenceCodec<FieldState> for FieldDomain {
    fn identity(&self) -> CodecIdentity {
        field_identity()
    }
    fn encode_state(&self, s: &FieldState, limit: usize) -> Result<Vec<u8>> {
        if 4 > limit {
            Err(Error::Limit)
        } else {
            Ok(s.private.to_vec())
        }
    }
    fn decode_state(&self, bytes: &[u8], limit: usize, _: &Remap) -> Result<FieldState> {
        if bytes.len() > limit {
            return Err(Error::Limit);
        }
        Ok(FieldState {
            private: bytes.try_into().map_err(|_| Error::Invalid)?,
        })
    }
    fn validate_state(&self, s: &FieldState) -> Result<()> {
        self.validate(s)
    }
    fn references(&self, _: &FieldState) -> Result<Vec<StateReference>> {
        Ok(vec![])
    }
    fn validate_configuration(
        &self,
        _: &FieldState,
        ports: &BTreeMap<String, Port>,
        policy: &QueryPolicy,
    ) -> Result<()> {
        if *policy != QueryPolicy::Noncacheable {
            Err(Error::Schema)
        } else {
            config(ports)
        }
    }
}

fn field_identity() -> CodecIdentity {
    let mut id = identity(3);
    id.schema_digest = [3; 32];
    id
}

fn queue_policy(calls: Rc<Cell<usize>>) -> nethacked_inputs::Policy {
    nethacked_inputs::Policy {
        identity: "queue".into(),
        version: 1,
        transition: Rc::new(move |s, _| {
            calls.set(calls.get() + 1);
            let (value, rest) = s
                .split_first()
                .ok_or(nethacked_kernel::Error::NativeRefusal)?;
            Ok((rest.to_vec(), vec![*value]))
        }),
    }
}
fn queue_catalog(calls: Rc<Cell<usize>>) -> nethacked_inputs::persistence::Catalog {
    let mut c = nethacked_inputs::persistence::Catalog::default();
    c.allow("queue".into(), 1, move || Ok(queue_policy(calls.clone())))
        .unwrap();
    c
}
struct PausedFactory {
    checkpoint: nethacked_workflows::persistence::Checkpoint,
    busy: Rc<Cell<bool>>,
}
impl composed::WorkflowFactory for PausedFactory {
    fn catalog(
        &self,
        handles: Rc<composed::Handles>,
    ) -> std::result::Result<nethacked_workflows::persistence::RestoreCatalog, runtime::Error> {
        let mut c = nethacked_workflows::persistence::RestoreCatalog::default();
        for frame in &self.checkpoint.frames {
            c.program(frame.program.clone(), Rc::new(workflow_support::Opaque))
                .unwrap();
        }
        let busy = self.busy.clone();
        c.participant(timer_identity("owners-cleanup"), move |id| {
            let p = handles.owner_cleanup(|host| {
                Rc::new(BusyHost {
                    host,
                    busy: busy.clone(),
                })
            })?;
            if p.identity()? != id {
                return Err(nethacked_workflows::Error::Kernel(
                    nethacked_kernel::Error::Authority,
                ));
            }
            Ok(p)
        })
        .unwrap();
        Ok(c)
    }
}

#[derive(Default)]
struct PreparedEffects {
    queries: Cell<usize>,
    preparations: Cell<usize>,
    samples: Cell<usize>,
    accounting: Cell<usize>,
    native_factories: Cell<usize>,
    accounting_factories: Cell<usize>,
    native_drops: Cell<usize>,
}
struct RecordedNative(Rc<PreparedEffects>);
impl Drop for RecordedNative {
    fn drop(&mut self) {
        self.0.native_drops.set(self.0.native_drops.get() + 1);
    }
}
impl Schema<u64> for RecordedNative {
    fn validate(&self, _: &u64) -> Result<()> {
        Ok(())
    }
    fn encode(&self, s: &u64) -> Result<Vec<u8>> {
        Ok(s.to_le_bytes().to_vec())
    }
    fn query(&self, s: &u64, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        self.0.queries.set(self.0.queries.get() + 1);
        self.encode(s)
    }
}
impl NativeBehavior<u64> for RecordedNative {
    fn prepare(&self, s: &u64, _: &str, _: &[u8], _: &[Observation]) -> Result<Proposal<u64>> {
        self.0.preparations.set(self.0.preparations.get() + 1);
        Ok(Proposal {
            next: s + 1,
            events: vec![],
            reply: vec![],
        })
    }
    fn prepare_recorded(
        &self,
        s: &u64,
        _: &str,
        _: &[u8],
        _: &[Observation],
        recorded: &[nethacked_inputs::Observation],
    ) -> Result<Proposal<u64>> {
        self.0.preparations.set(self.0.preparations.get() + 1);
        let b = recorded.first().ok_or(Error::Invalid)?.bytes();
        Ok(Proposal {
            next: s + u64::from(b[0]),
            events: vec![b.to_vec()],
            reply: b.to_vec(),
        })
    }
}

struct NativeBlob;
impl Schema<Vec<u8>> for NativeBlob {
    fn validate(&self, s: &Vec<u8>) -> Result<()> {
        StateCodec.validate_state(s)
    }
    fn encode(&self, s: &Vec<u8>) -> Result<Vec<u8>> {
        Ok(s.clone())
    }
    fn query(&self, s: &Vec<u8>, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        Ok(s.clone())
    }
}
impl NativeBehavior<Vec<u8>> for NativeBlob {
    fn prepare(
        &self,
        s: &Vec<u8>,
        _: &str,
        _: &[u8],
        _: &[Observation],
    ) -> Result<Proposal<Vec<u8>>> {
        let next = number(s).checked_add(2).ok_or(Error::Invariant)?;
        Ok(Proposal {
            next: bytes(next),
            reply: vec![],
            events: vec![b"blob".to_vec()],
        })
    }
}
struct MixedAccounting;
impl Accounting for MixedAccounting {
    fn validate(&self, p: &[ProposalView], r: &[Observation]) -> Result<()> {
        if p.len() == 4
            && r.len() == 1
            && p[0].proposed == 29_u64.to_le_bytes()
            && p[1].proposed == bytes(8)
            && p[2].proposed == [4, 0, 0, 0]
            && p[3].proposed == bytes(125)
            && r[0].bytes() == 9_u64.to_le_bytes()
        {
            Ok(())
        } else {
            Err(Error::Invariant)
        }
    }
}
fn mixed_id() -> ArtifactIdentity {
    ArtifactIdentity {
        name: "mixed-accounting".into(),
        version: 1,
        artifact: [29; 32],
    }
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
