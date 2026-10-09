// Additive PASS2 fixture; the PASS1 assertions above remain frozen.
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
struct PreparedAccounting(Rc<PreparedEffects>);
impl Accounting for PreparedAccounting {
    fn validate(&self, p: &[ProposalView], r: &[Observation]) -> Result<()> {
        self.0.accounting.set(self.0.accounting.get() + 1);
        if p.len() == 2
            && r.len() == 1
            && p[1].proposed == 51_u64.to_le_bytes()
            && p[0].proposed == bytes(17)
        {
            Ok(())
        } else {
            Err(Error::Invariant)
        }
    }
}
fn accounting_id() -> ArtifactIdentity {
    ArtifactIdentity {
        name: "prepared-accounting".into(),
        version: 1,
        artifact: [9; 32],
    }
}
fn accounting_catalog(e: Rc<PreparedEffects>) -> AccountingRestoreCatalog {
    accounting_catalog_tracked(e, None)
}
fn accounting_catalog_tracked(
    e: Rc<PreparedEffects>,
    registrations: Option<Rc<RefCell<AccountingCaptureCatalog>>>,
) -> AccountingRestoreCatalog {
    let mut c = AccountingRestoreCatalog::default();
    c.allow(accounting_id(), [9; 32], move || {
        e.accounting_factories.set(e.accounting_factories.get() + 1);
        let value: Rc<dyn Accounting> = Rc::new(PreparedAccounting(e.clone()));
        if let Some(c) = &registrations {
            c.borrow_mut()
                .register(accounting_id(), [9; 32], value.clone())?;
        }
        Ok(value)
    })
    .unwrap();
    c
}
fn recorded_catalog(
    e: Rc<PreparedEffects>,
    registrations: Option<Rc<RefCell<CaptureCatalog<u64>>>>,
) -> RestoreCatalog<u64> {
    let mut c = RestoreCatalog::default();
    let tracked = registrations.clone();
    c.allow(identity(6), QueryPolicy::Noncacheable, move |_| {
        e.native_factories.set(e.native_factories.get() + 1);
        let domain: Rc<dyn NativeBehavior<u64>> = Rc::new(RecordedNative(e.clone()));
        if let Some(c) = &tracked {
            c.borrow_mut().register(
                identity(6),
                QueryPolicy::Noncacheable,
                Backend::Native(domain.clone()),
            )?;
        }
        Ok(Backend::Native(domain))
    })
    .unwrap();
    c.allow(identity(5), QueryPolicy::Noncacheable, move |_| {
        let domain: Rc<dyn NativeBehavior<u64>> = Rc::new(NativeDomain);
        if let Some(c) = &registrations {
            c.borrow_mut().register(
                identity(5),
                QueryPolicy::Noncacheable,
                Backend::Native(domain.clone()),
            )?;
        }
        Ok(Backend::Native(domain))
    })
    .unwrap();
    c
}
fn input_catalog(e: Rc<PreparedEffects>) -> nethacked_inputs::persistence::Catalog {
    let mut c = nethacked_inputs::persistence::Catalog::default();
    c.allow("queue".into(), 1, move || {
        Ok(nethacked_inputs::Policy {
            identity: "queue".into(),
            version: 1,
            transition: Rc::new({
                let e = e.clone();
                move |s, r| {
                    e.samples.set(e.samples.get() + 1);
                    if r != b"choice" || s.is_empty() {
                        return Err(nethacked_kernel::Error::NativeRefusal);
                    }
                    Ok((s[1..].to_vec(), vec![s[0]]))
                }
            }),
        })
    })
    .unwrap();
    c
}
#[derive(Clone)]
struct PreparedFixture {
    owners: ExecutionCheckpoint,
    kernel: KernelSnapshot,
    materials: BTreeMap<[u8; 32], Material>,
    contexts: Vec<SavedOperationContext>,
    wasm: OwnerId,
    native: OwnerId,
    readonly: OwnerId,
    behavior: BehaviorId,
    serials: [u64; 2],
    ticket: u64,
    base: CapabilityPermit,
    new: CapabilityPermit,
    base_ids: Vec<u64>,
    new_ids: Vec<u64>,
    input_artifact: [u8; 32],
    abandoned_serial: u64,
    abandoned_ticket: u64,
}
fn prepared_fixture() -> PreparedFixture {
    let mut h = Harness::new(Limits::default(), KernelLimits::default(), "");
    let e = Rc::new(PreparedEffects::default());
    let wasm_effects = Rc::new(Cell::new(0));
    // The source policy is a fresh allocation too; the restore catalog below is
    // created independently after all source allocations have been destroyed.
    let service = nethacked_inputs::Service::new(
        nethacked_inputs::Limits::default(),
        nethacked_inputs::Policy {
            identity: "queue".into(),
            version: 1,
            transition: Rc::new({
                let e = e.clone();
                move |s, r| {
                    e.samples.set(e.samples.get() + 1);
                    assert_eq!(r, b"choice");
                    Ok((s[1..].to_vec(), vec![s[0]]))
                }
            }),
        },
        vec![10, 20],
    )
    .unwrap();
    let input = service.install(&mut h.kernel, "inputs-real").unwrap();
    let input_artifact = h.kernel.artifact(input).unwrap();
    let input_identity = serde_json::to_vec(
        &serde_json::to_value((
            "nethacked-inputs-v1",
            "0.1.0",
            "queue",
            1_u32,
            nethacked_inputs::Limits::default(),
        ))
        .unwrap(),
    )
    .unwrap();
    h.materials.insert(
        input_artifact,
        Material::Native(h.kernel.manifest(input).unwrap().clone(), input_identity),
    );
    let mut m = h.kernel.manifest(h.root).unwrap().clone();
    m.id = "prepared-caller".into();
    m.dependencies.push(nethacked_kernel::Dependency {
        capability: Capability {
            provider: "inputs-real".into(),
            endpoint: nethacked_inputs::ENDPOINT.into(),
            major: 1,
        },
        access: Access::Write,
    });
    let root_bytes=wat::parse_str(r#"(module
        (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (func (export "alloc") (param i32) (result i32) i32.const 0)
        (func (export "run") (param $p i32) (param $n i32) (result i64) (local $r i32)
          local.get $p i32.load8_u local.get $p i32.const 1 i32.add local.get $n i32.const 1 i32.sub i32.const 2048 i32.const 16384 call $call local.tee $r
          i32.const 0 i32.lt_s if unreachable end i64.const 8796093022208 local.get $r i64.extend_i32_u i64.or))"#).unwrap();
    h.root = h.kernel.register_guest(m.clone(), &root_bytes).unwrap();
    h.materials.insert(
        h.kernel.artifact(h.root).unwrap(),
        Material::Guest(m, root_bytes),
    );
    h.keys.insert(Resource {
        owner: "inputs-real".into(),
        key: "source".into(),
    });
    h.kernel
        .issue_grant(
            h.root,
            3,
            Authority {
                access: Access::Write,
                scope: Scope::Keys(h.keys.clone()),
            },
        )
        .unwrap();
    let base = h.binding();
    let old: Rc<dyn WasmCodec<Vec<u8>>> = Rc::new(Domain {
        add: 1,
        effects: wasm_effects.clone(),
    });
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
    let domain: Rc<dyn NativeBehavior<u64>> = Rc::new(RecordedNative(e.clone()));
    let n = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            41_u64,
            Backend::Native(domain.clone()),
        )
        .unwrap();
    n.declare_recorded_inputs(
        "prepare",
        vec![RecordedDeclaration {
            policy: "queue".into(),
            version: 1,
            provider: ProviderBinding {
                handle: input,
                artifact: input_artifact,
                endpoint: nethacked_inputs::ENDPOINT.into(),
            },
            request: b"choice".to_vec(),
        }],
    )
    .unwrap();
    let mut spec = h.spec(
        1,
        vec![Dependency {
            owner: w.id(),
            query: "query".into(),
            schema: [1; 32],
        }],
    );
    spec.ports.get_mut("prepare").unwrap().dependencies.clear();
    let rd_domain: Rc<dyn NativeBehavior<u64>> = Rc::new(NativeDomain);
    let rd = h
        .host
        .enroll(&h.kernel, spec, 9_u64, Backend::Native(rd_domain.clone()))
        .unwrap();
    for o in [&n, &rd] {
        h.admit(o);
    }
    h.admit(&w);
    h.refresh();
    w.set_query_policy(old_policy()).unwrap();
    let retained = Rc::new(RefCell::new(None));
    h.run(1, {
        let w = w.clone();
        let retained = retained.clone();
        move |v| {
            *retained.borrow_mut() = Some(w.retain_behavior("prepare", v).unwrap());
            Ok(vec![])
        }
    });
    let b = retained.borrow_mut().take().unwrap();
    let observed = Rc::new(RefCell::new(None));
    h.run(1, {
        let w = w.clone();
        let rd = rd.clone();
        let b = b.clone();
        let observed = observed.clone();
        move |v| {
            let observation = w.query_retained(&b, "query", &[], &[], v).unwrap();
            *observed.borrow_mut() = Some(rd.query("derived", &[], &[observation], v).unwrap());
            Ok(vec![])
        }
    });
    let readonly = observed.borrow_mut().take().unwrap();
    let extension_bytes=helper_wasm("i32.const 1024 local.get $p i32.load i32.const 100 i32.add i32.store i64.const 4398046511108");
    let extension_manifest = Manifest {
        id: "prepared-new-algorithm".into(),
        exports: [("compute".into(), endpoint(Access::Read))].into(),
        dependencies: vec![],
        extension_points: BTreeSet::new(),
        extension: Some(Extension {
            target: base.capability.clone(),
            export: "compute".into(),
        }),
    };
    let ext = h
        .kernel
        .register_guest(extension_manifest.clone(), &extension_bytes)
        .unwrap();
    h.materials.insert(
        h.kernel.artifact(ext).unwrap(),
        Material::Guest(extension_manifest, extension_bytes),
    );
    let new = CapabilityPermit {
        capability: base.capability.clone(),
        resolution: [
            base.resolution.clone(),
            vec![ProviderBinding {
                handle: ext,
                artifact: h.kernel.artifact(ext).unwrap(),
                endpoint: "compute".into(),
            }],
        ]
        .concat(),
    };
    let future: Rc<dyn WasmCodec<Vec<u8>>> = Rc::new(Domain {
        add: 20,
        effects: wasm_effects.clone(),
    });
    w.admit_compatible_wasm(&h.kernel, [1; 32], future.clone(), 0, new.clone())
        .unwrap();
    w.set_query_policy(new_policy()).unwrap();
    let mut request = vec![3];
    request.extend(
        serde_json::to_vec(&nethacked_inputs::Request::Sample {
            request: b"choice".to_vec(),
        })
        .unwrap(),
    );
    let sampled = h
        .kernel
        .invoke_operation(
            &h.operation,
            &request,
            Authority {
                access: Access::Write,
                scope: Scope::Keys(h.keys.clone()),
            },
        )
        .unwrap();
    let reply: nethacked_inputs::Reply = serde_json::from_slice(&sampled).unwrap();
    assert_eq!(e.samples.get(), 1);
    let result = Rc::new(RefCell::new(None));
    let accounting: Rc<dyn Accounting> = Rc::new(PreparedAccounting(e.clone()));
    h.run(1, {
        let w = w.clone();
        let n = n.clone();
        let b = b.clone();
        let readonly = readonly.clone();
        let service = service.clone();
        let accounting = accounting.clone();
        let host = h.host.clone();
        let result = result.clone();
        move |v| {
            let input = service.observation(reply.id, b"choice", v).unwrap();
            let abandoned = n
                .prepare_recorded(None, "prepare", &[], vec![], &[input.clone()], v)
                .unwrap();
            let abandoned_serial = abandoned.receipt().serial;
            let abandoned_batch = host
                .batch(
                    vec![abandoned.erase()],
                    vec![],
                    &[n.id()],
                    vec![n.id()],
                    [9; 32],
                    accounting.clone(),
                    cap("prepare"),
                    v,
                )
                .unwrap();
            let abandoned_ticket = abandoned_batch.ticket();
            drop(abandoned_batch);
            let wc = w.prepare_retained(&b, "prepare", &[], vec![], v).unwrap();
            let nc = n
                .prepare_recorded(None, "prepare", &[], vec![], &[input], v)
                .unwrap();
            let serials = [wc.receipt().serial, nc.receipt().serial];
            *result.borrow_mut() = Some((
                wc,
                nc,
                readonly.clone(),
                serials,
                abandoned_serial,
                abandoned_ticket,
            ));
            Ok(vec![])
        }
    });
    let (wc, nc, readonly, serials, abandoned_serial, abandoned_ticket) =
        result.borrow_mut().take().unwrap();
    let batch_cell = Rc::new(RefCell::new(None));
    h.run(1, {
        let host = h.host.clone();
        let batch_cell = batch_cell.clone();
        let mut candidates = Some(vec![wc.erase(), nc.erase()]);
        let mut readonly = Some(readonly);
        let accounting = accounting.clone();
        let writes = [w.id(), n.id()];
        move |v| {
            *batch_cell.borrow_mut() = Some(
                host.batch(
                    candidates.take().unwrap(),
                    vec![readonly.take().unwrap()],
                    &writes,
                    vec![writes[1], writes[0]],
                    [9; 32],
                    accounting.clone(),
                    cap("prepare"),
                    v,
                )
                .unwrap(),
            );
            Ok(vec![])
        }
    });
    let batch = batch_cell.borrow_mut().take().unwrap();
    let mut roots = ExecutionRoots::default();
    roots.behavior(&b).unwrap();
    roots.batch(&batch).unwrap();
    let live = roots.live_operations(&h.kernel).unwrap();
    let snapshot = h.kernel.kernel_snapshot(&roots.enrollment(&live)).unwrap();
    let index = h.kernel.execution_capture_index(&snapshot).unwrap();
    let mut wcatalog = CaptureCatalog::default();
    wcatalog
        .register(
            identity(1),
            old_policy(),
            Backend::Wasm {
                codec: old,
                slot: 0,
                binding: base.clone(),
            },
        )
        .unwrap();
    wcatalog
        .register(
            identity(2),
            new_policy(),
            Backend::Wasm {
                codec: future,
                slot: 0,
                binding: new.clone(),
            },
        )
        .unwrap();
    let mut ncatalog = CaptureCatalog::default();
    ncatalog
        .register(
            identity(6),
            QueryPolicy::Noncacheable,
            Backend::Native(domain),
        )
        .unwrap();
    ncatalog
        .register(
            identity(5),
            QueryPolicy::Noncacheable,
            Backend::Native(rd_domain),
        )
        .unwrap();
    let mut acatalog = AccountingCaptureCatalog::default();
    acatalog
        .register(accounting_id(), [9; 32], accounting)
        .unwrap();
    assert!(matches!(h.host.capture_state(), Err(Error::Unsupported)));
    let mut capture = h.host.capture_execution(&index).unwrap();
    let behavior = capture.owner(&w, &StateCodec, &wcatalog, &[&b]).unwrap()[0];
    capture.owner(&n, &NativeDomain, &ncatalog, &[]).unwrap();
    capture.owner(&rd, &NativeDomain, &ncatalog, &[]).unwrap();
    capture
        .batch_candidate::<Vec<u8>>(&batch, serials[0], &StateCodec, &wcatalog)
        .unwrap();
    capture
        .batch_candidate::<u64>(&batch, serials[1], &NativeDomain, &ncatalog)
        .unwrap();
    let ticket = capture.batch(&batch, &acatalog).unwrap();
    capture.inputs(&service).unwrap();
    let saved = capture.finish().unwrap();
    let owners = ExecutionCheckpoint::from_bytes(&saved.to_bytes(LIMIT).unwrap(), LIMIT).unwrap();
    let base_ids = base
        .resolution
        .iter()
        .map(|b| index.module(b.handle).unwrap())
        .collect();
    let new_ids = new
        .resolution
        .iter()
        .map(|b| index.module(b.handle).unwrap())
        .collect();
    let contexts = snapshot.operation_contexts().unwrap();
    let kernel = serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    assert_eq!(w.inspect().unwrap(), bytes(5));
    assert_eq!(n.inspect().unwrap(), 41_u64.to_le_bytes());
    assert_eq!(e.accounting.get(), 0);
    assert_eq!(e.samples.get(), 1);
    PreparedFixture {
        owners,
        kernel,
        materials: h.materials.clone(),
        contexts,
        wasm: w.id(),
        native: n.id(),
        readonly: rd.id(),
        behavior,
        serials,
        ticket,
        base,
        new,
        base_ids,
        new_ids,
        input_artifact,
        abandoned_serial,
        abandoned_ticket,
    }
}
struct PreparedApproved<'a> {
    base: Approved<'a>,
    input_artifact: [u8; 32],
    input_port: NativePort,
}
impl RegistryAuthority for PreparedApproved<'_> {
    fn resolve(&self, m: &SavedModule) -> nethacked_kernel::Result<ArtifactMaterial> {
        if m.artifact == self.input_artifact {
            let Material::Native(manifest, identity) = self
                .base
                .materials
                .get(&m.artifact)
                .ok_or(nethacked_kernel::Error::Authority)?
            else {
                return Err(nethacked_kernel::Error::Authority);
            };
            return Ok(ArtifactMaterial::Native {
                manifest: manifest.clone(),
                identity: identity.clone(),
                ports: [(nethacked_inputs::ENDPOINT.into(), self.input_port.clone())].into(),
            });
        }
        self.base.resolve(m)
    }
    fn bind_resource(&self, r: &Resource) -> nethacked_kernel::Result<Resource> {
        if r.owner == "inputs-real" && r.key == "source" {
            Ok(r.clone())
        } else {
            self.base.bind_resource(r)
        }
    }
    fn approve_grant(
        &self,
        a: &SavedModule,
        s: usize,
        b: &SavedModule,
        g: &Authority,
    ) -> nethacked_kernel::Result<Authority> {
        self.base.approve_grant(a, s, b, g)
    }
}
impl RetainedAuthority for PreparedApproved<'_> {
    fn approve_cleanup(&self, id: u64) -> nethacked_kernel::Result<()> {
        self.base.approve_cleanup(id)
    }
    fn approve_operation_context(&self, c: &SavedOperationContext) -> nethacked_kernel::Result<()> {
        self.base.approve_operation_context(c)
    }
}
struct PreparedLoaded {
    kernel: Kernel,
    host: Host,
    wasm: Rc<Owner<Vec<u8>>>,
    native: Rc<Owner<u64>>,
    readonly: Rc<Owner<u64>>,
    behavior: RetainedBehavior<Vec<u8>>,
    candidates: (Candidate<Vec<u8>>, Candidate<u64>),
    batch: Rc<Batch>,
    mapped: RestoredKernel,
    action: Rc<RefCell<Option<Action>>>,
    effects: Rc<PreparedEffects>,
    wasm_effects: Rc<Cell<usize>>,
    wc: Rc<RefCell<CaptureCatalog<Vec<u8>>>>,
    nc: Rc<RefCell<CaptureCatalog<u64>>>,
    service: nethacked_inputs::Service,
    ac: Rc<RefCell<AccountingCaptureCatalog>>,
}
fn prepared_load(f: &PreparedFixture) -> PreparedLoaded {
    let e = Rc::new(PreparedEffects::default());
    let wasm_effects = Rc::new(Cell::new(0));
    let input = nethacked_inputs::persistence::Restore::prepare(
        f.owners.input_checkpoint().unwrap().clone(),
        nethacked_inputs::Limits::default(),
        &f.kernel,
        &input_catalog(e.clone()),
    )
    .unwrap();
    let reservation = OwnerIdentityReservation::new(
        f.owners.clone(),
        &f.kernel,
        Limits::default(),
        PublicationLimits::default(),
    )
    .unwrap();
    let action = Rc::new(RefCell::new(None));
    let authority = PreparedApproved {
        base: Approved {
            reservation: &reservation,
            materials: &f.materials,
            contexts: &f.contexts,
            action: action.clone(),
            factories: Cell::new(0),
            deny: false,
        },
        input_artifact: f.input_artifact,
        input_port: input.port(),
    };
    let mut kernel = Kernel::new(KernelLimits::default()).unwrap();
    let mapped = kernel.restore_kernel(&f.kernel, &authority).unwrap();
    drop(authority);
    let input = input.finish(&kernel, &mapped).unwrap();
    let service = input.service().clone();
    let mut restore = reservation.bind(&kernel, &mapped).unwrap();
    restore.inputs(input).unwrap();
    let ac = Rc::new(RefCell::new(AccountingCaptureCatalog::default()));
    let accounting = accounting_catalog_tracked(e.clone(), Some(ac.clone()));
    restore.approve_accounting(&accounting).unwrap();
    let wc = Rc::new(RefCell::new(CaptureCatalog::default()));
    let catalog = factory_catalog_tracked(
        wasm_effects.clone(),
        &f.base,
        &f.base_ids,
        &f.new,
        &f.new_ids,
        true,
        Some(wc.clone()),
    );
    let nc = Rc::new(RefCell::new(CaptureCatalog::default()));
    let native_catalog = recorded_catalog(e.clone(), Some(nc.clone()));
    let wasm = restore
        .owner(&kernel, &mapped, f.wasm, &StateCodec, &catalog)
        .unwrap();
    let native = restore
        .owner(&kernel, &mapped, f.native, &NativeDomain, &native_catalog)
        .unwrap();
    let readonly = restore
        .owner(&kernel, &mapped, f.readonly, &NativeDomain, &native_catalog)
        .unwrap();
    let behavior = restore.behavior(f.behavior).unwrap();
    let w = restore
        .candidate(&kernel, &mapped, f.serials[0], &wasm, &StateCodec, &catalog)
        .unwrap();
    let n = restore
        .candidate(
            &kernel,
            &mapped,
            f.serials[1],
            &native,
            &NativeDomain,
            &native_catalog,
        )
        .unwrap();
    assert!(matches!(
        restore.candidate(&kernel, &mapped, f.serials[0], &wasm, &StateCodec, &catalog),
        Err(Error::Invalid)
    ));
    assert!(matches!(
        restore.candidate(
            &kernel,
            &mapped,
            f.abandoned_serial,
            &native,
            &NativeDomain,
            &native_catalog
        ),
        Err(Error::Stale)
    ));
    let batch = Rc::new(
        restore
            .batch(&kernel, &mapped, f.ticket, &accounting)
            .unwrap(),
    );
    assert!(matches!(
        restore.batch(&kernel, &mapped, f.ticket, &accounting),
        Err(Error::Invalid)
    ));
    assert!(matches!(
        restore.batch(&kernel, &mapped, f.abandoned_ticket, &accounting),
        Err(Error::Stale)
    ));
    assert!(wasm.inspect().is_err());
    let host = restore.finish().unwrap();
    assert_eq!(e.preparations.get(), 0);
    assert_eq!(e.queries.get(), 0);
    assert_eq!(e.samples.get(), 0);
    assert_eq!(e.accounting.get(), 0);
    assert_eq!(e.native_factories.get(), 2);
    assert_eq!(e.accounting_factories.get(), 1);
    assert_eq!(wasm_effects.get(), 0);
    PreparedLoaded {
        kernel,
        host,
        wasm,
        native,
        readonly,
        behavior,
        candidates: (w, n),
        batch,
        mapped,
        action,
        effects: e,
        wasm_effects,
        wc,
        nc,
        service,
        ac,
    }
}
fn prepared_invoke(
    l: &PreparedLoaded,
    slot: u8,
    action: impl FnMut(&CallerView<'_>) -> nethacked_kernel::Result<Vec<u8>> + 'static,
) -> nethacked_kernel::Result<Vec<u8>> {
    *l.action.borrow_mut() = Some(Box::new(action));
    let mut keys = [
        &l.wasm as &dyn ExecutionKeys,
        &l.native as &dyn ExecutionKeys,
        &l.readonly as &dyn ExecutionKeys,
    ]
    .into_iter()
    .flat_map(|o| o.keys())
    .chain([l.host.publication_resource()])
    .collect::<BTreeSet<_>>();
    keys.insert(Resource {
        owner: "inputs-real".into(),
        key: "source".into(),
    });
    let result = l.kernel.invoke_operation(
        l.behavior.source(),
        &[slot],
        Authority {
            access: Access::Write,
            scope: Scope::Keys(keys),
        },
    );
    l.action.borrow_mut().take();
    result
}
fn prepared_commit(l: &PreparedLoaded) -> CommitReceipt {
    let result = Rc::new(RefCell::new(None));
    prepared_invoke(l, 1, {
        let batch = l.batch.clone();
        let result = result.clone();
        move |v| {
            *result.borrow_mut() = Some(batch.commit(v).unwrap());
            assert_eq!(batch.commit(v), Err(Error::Stale));
            Ok(vec![])
        }
    })
    .unwrap();
    let receipt = result.borrow_mut().take().unwrap();
    receipt
}
#[test]
fn heterogeneous_prepared_real_bytes_fresh_ledger_atomic_commit_once_no_reprepare() {
    let f = prepared_fixture();
    let mut l = prepared_load(&f);
    assert_ne!(l.wasm.id(), f.wasm);
    assert_ne!(l.native.id(), f.native);
    assert_eq!(l.host.pending().unwrap(), 2);
    assert_eq!(l.wasm.inspect().unwrap(), bytes(5));
    assert_eq!(l.native.inspect().unwrap(), 41_u64.to_le_bytes());
    let receipt = prepared_commit(&l);
    assert_eq!(receipt.ticket, f.ticket);
    assert_eq!(
        receipt
            .participants
            .iter()
            .map(|r| r.serial)
            .collect::<Vec<_>>(),
        vec![f.serials[1], f.serials[0]]
    );
    assert_eq!(receipt.events, vec![vec![10], bytes(17)]);
    assert_eq!(
        receipt.reply,
        [
            f.ticket.to_le_bytes().to_vec(),
            receipt.epoch.to_le_bytes().to_vec(),
            1_u64.to_le_bytes().to_vec(),
            vec![10],
            4_u64.to_le_bytes().to_vec(),
            bytes(17)
        ]
        .concat()
    );
    assert_eq!(receipt.participants[0].recorded_inputs.len(), 1);
    assert_eq!(receipt.observations.len(), 1);
    assert_eq!(receipt.observations[0].dependencies.len(), 1);
    assert_eq!(l.wasm.inspect().unwrap(), bytes(17));
    assert_eq!(l.native.inspect().unwrap(), 51_u64.to_le_bytes());
    assert_eq!(l.readonly.inspect().unwrap(), 9_u64.to_le_bytes());
    assert_eq!(l.host.pending().unwrap(), 0);
    assert_eq!(l.effects.preparations.get(), 0);
    assert_eq!(l.effects.samples.get(), 0);
    assert_eq!(l.effects.accounting.get(), 1);
    assert_eq!(l.wasm_effects.get(), 0);
    assert!(l
        .kernel
        .unregister(receipt.participants[0].stamp.module.handle)
        .is_err());
}
fn prepared_recapture(l: &mut PreparedLoaded, f: &PreparedFixture) -> PreparedFixture {
    let mut roots = ExecutionRoots::default();
    roots.behavior(&l.behavior).unwrap();
    roots.batch(&l.batch).unwrap();
    let live = roots.live_operations(&l.kernel).unwrap();
    let snapshot = l.kernel.kernel_snapshot(&roots.enrollment(&live)).unwrap();
    let index = l.kernel.execution_capture_index(&snapshot).unwrap();
    let mut capture = l.host.capture_execution(&index).unwrap();
    let behavior = capture
        .owner(&l.wasm, &StateCodec, &l.wc.borrow(), &[&l.behavior])
        .unwrap()[0];
    capture
        .owner(&l.native, &NativeDomain, &l.nc.borrow(), &[])
        .unwrap();
    capture
        .owner(&l.readonly, &NativeDomain, &l.nc.borrow(), &[])
        .unwrap();
    capture
        .batch_candidate::<Vec<u8>>(&l.batch, f.serials[0], &StateCodec, &l.wc.borrow())
        .unwrap();
    capture
        .batch_candidate::<u64>(&l.batch, f.serials[1], &NativeDomain, &l.nc.borrow())
        .unwrap();
    capture.batch(&l.batch, &l.ac.borrow()).unwrap();
    capture.inputs(&l.service).unwrap();
    let checkpoint = capture.finish().unwrap();
    let owners =
        ExecutionCheckpoint::from_bytes(&checkpoint.to_bytes(LIMIT).unwrap(), LIMIT).unwrap();
    let fresh_binding = |b: &CapabilityPermit, ids: &[u64]| CapabilityPermit {
        capability: b.capability.clone(),
        resolution: b
            .resolution
            .iter()
            .zip(ids)
            .map(|(b, id)| ProviderBinding {
                handle: l.mapped.modules()[id],
                artifact: b.artifact,
                endpoint: b.endpoint.clone(),
            })
            .collect(),
    };
    let base = fresh_binding(&f.base, &f.base_ids);
    let new = fresh_binding(&f.new, &f.new_ids);
    let base_ids = base
        .resolution
        .iter()
        .map(|b| index.module(b.handle).unwrap())
        .collect();
    let new_ids = new
        .resolution
        .iter()
        .map(|b| index.module(b.handle).unwrap())
        .collect();
    let contexts = snapshot.operation_contexts().unwrap();
    let kernel = serde_json::from_slice(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
    PreparedFixture {
        owners,
        kernel,
        materials: f.materials.clone(),
        contexts,
        wasm: l.wasm.id(),
        native: l.native.id(),
        readonly: l.readonly.id(),
        behavior,
        serials: f.serials,
        ticket: f.ticket,
        base,
        new,
        base_ids,
        new_ids,
        input_artifact: f.input_artifact,
        abandoned_serial: f.abandoned_serial,
        abandoned_ticket: f.abandoned_ticket,
    }
}
#[test]
fn prepared_second_restart_shared_lifetimes_drop_release_and_readonly_stale_refusal() {
    let f = prepared_fixture();
    let mut l = prepared_load(&f);
    let first = l.native.id();
    let f = prepared_recapture(&mut l, &f);
    drop(l);
    let l = prepared_load(&f);
    assert_ne!(first, l.native.id());
    let receipt = prepared_commit(&l);
    assert_eq!(receipt.events, vec![vec![10], bytes(17)]);
    drop(l);
    let l = prepared_load(&f);
    // Drop the typed handle: the batch retains the same allocation/reservation.
    let PreparedLoaded {
        host,
        batch,
        candidates,
        wasm,
        native,
        readonly,
        behavior,
        kernel,
        mapped,
        action,
        effects,
        wasm_effects,
        wc,
        nc,
        service,
        ac,
    } = l;
    drop(candidates);
    assert_eq!(host.pending().unwrap(), 2);
    drop(batch);
    assert_eq!(host.pending().unwrap(), 0);
    assert_eq!(native.inspect().unwrap(), 41_u64.to_le_bytes());
    assert_eq!(effects.preparations.get(), 0);
    assert_eq!(effects.accounting.get(), 0);
    drop((
        wasm,
        native,
        readonly,
        behavior,
        kernel,
        mapped,
        action,
        effects,
        wasm_effects,
        wc,
        nc,
        service,
        ac,
    ));
    let l = prepared_load(&f);
    struct Accept;
    impl Accounting for Accept {
        fn validate(&self, _: &[ProposalView], _: &[Observation]) -> Result<()> {
            Ok(())
        }
    }
    prepared_invoke(&l, 1, {
        let owner = l.readonly.clone();
        let host = l.host.clone();
        move |v| {
            let c = owner.prepare("prepare", &[], vec![], v).unwrap();
            let id = owner.id();
            host.batch(
                vec![c.erase()],
                vec![],
                &[id],
                vec![id],
                [1; 32],
                Rc::new(Accept),
                cap("prepare"),
                v,
            )
            .unwrap()
            .commit(v)
            .unwrap();
            Ok(vec![])
        }
    })
    .unwrap();
    let before = (
        l.wasm.inspect().unwrap(),
        l.native.inspect().unwrap(),
        l.host.pending().unwrap(),
    );
    prepared_invoke(&l, 1, {
        let batch = l.batch.clone();
        move |v| {
            assert_eq!(batch.commit(v), Err(Error::Stale));
            Ok(vec![])
        }
    })
    .unwrap();
    assert_eq!(
        before,
        (
            l.wasm.inspect().unwrap(),
            l.native.inspect().unwrap(),
            l.host.pending().unwrap()
        )
    );
    assert_eq!(l.effects.preparations.get(), 0);
    assert_eq!(l.effects.accounting.get(), 0);
    assert_eq!(l.effects.samples.get(), 0);
}
fn prepared_build(
    f: &PreparedFixture,
) -> (
    Kernel,
    RestoredKernel,
    ExecutionRestore,
    Rc<PreparedEffects>,
) {
    let e = Rc::new(PreparedEffects::default());
    let input = nethacked_inputs::persistence::Restore::prepare(
        f.owners.input_checkpoint().unwrap().clone(),
        nethacked_inputs::Limits::default(),
        &f.kernel,
        &input_catalog(e.clone()),
    )
    .unwrap();
    let reservation = OwnerIdentityReservation::new(
        f.owners.clone(),
        &f.kernel,
        Limits::default(),
        PublicationLimits::default(),
    )
    .unwrap();
    let authority = PreparedApproved {
        base: Approved {
            reservation: &reservation,
            materials: &f.materials,
            contexts: &f.contexts,
            action: Rc::new(RefCell::new(None)),
            factories: Cell::new(0),
            deny: false,
        },
        input_artifact: f.input_artifact,
        input_port: input.port(),
    };
    let mut kernel = Kernel::new(KernelLimits::default()).unwrap();
    let mapped = kernel.restore_kernel(&f.kernel, &authority).unwrap();
    drop(authority);
    let input = input.finish(&kernel, &mapped).unwrap();
    let mut restore = reservation.bind(&kernel, &mapped).unwrap();
    restore.inputs(input).unwrap();
    (kernel, mapped, restore, e)
}
#[test]
fn prepared_forgery_bounds_allowlists_and_incomplete_builder_quarantine_preserve_destination() {
    let f = prepared_fixture();
    let destination = prepared_load(&f);
    let before = (
        destination.wasm.inspect().unwrap(),
        destination.native.inspect().unwrap(),
        destination.readonly.inspect().unwrap(),
        destination.service.inspect().unwrap(),
        destination.host.pending().unwrap(),
    );
    let mut mutations = vec![];
    for (field, value) in [
        ("serial", serde_json::json!(0)),
        ("caller", serde_json::json!(0)),
        ("pins", serde_json::json!([0, 0, 0])),
        ("retained_bytes", serde_json::json!(usize::MAX)),
    ] {
        let mut v = serde_json::to_value(&f.owners).unwrap();
        v["pending"]["candidates"][0][field] = value;
        mutations.push(v);
    }
    let mut v = serde_json::to_value(&f.owners).unwrap();
    v["pending"]["candidates"][0]["stamp"]["generation"] = serde_json::json!(99);
    mutations.push(v);
    let mut v = serde_json::to_value(&f.owners).unwrap();
    v["pending"]["candidates"][0]["source"]["checkpoint"] = serde_json::json!(99);
    mutations.push(v);
    let mut v = serde_json::to_value(&f.owners).unwrap();
    v["pending"]["candidates"][1]["recorded"] = serde_json::json!([999]);
    mutations.push(v);
    let mut v = serde_json::to_value(&f.owners).unwrap();
    v["pending"]["batches"][0]["order"] = serde_json::json!([]);
    mutations.push(v);
    let mut v = serde_json::to_value(&f.owners).unwrap();
    v["pending"]["batches"][0]["ticket"] = serde_json::json!(0);
    mutations.push(v);
    let mut v = serde_json::to_value(&f.owners).unwrap();
    v["pending"]["candidates"][0]["stamp"]["module"]["module"] = serde_json::json!(u64::MAX);
    mutations.push(v);
    for value in mutations {
        let c: ExecutionCheckpoint = serde_json::from_value(value).unwrap();
        assert!(OwnerIdentityReservation::new(
            c,
            &f.kernel,
            Limits::default(),
            PublicationLimits::default()
        )
        .is_err());
    }
    assert!(OwnerIdentityReservation::new(
        f.owners.clone(),
        &f.kernel,
        Limits {
            candidates: 1,
            ..Limits::default()
        },
        PublicationLimits::default()
    )
    .is_err());
    assert!(OwnerIdentityReservation::new(
        f.owners.clone(),
        &f.kernel,
        Limits::default(),
        PublicationLimits {
            retained_bytes: 1,
            ..PublicationLimits::default()
        }
    )
    .is_err());
    let (kernel, mapped, mut restore, e) = prepared_build(&f);
    assert!(restore
        .approve_accounting(&AccountingRestoreCatalog::default())
        .is_err());
    assert_eq!(e.native_factories.get(), 0);
    assert_eq!(e.preparations.get(), 0);
    assert_eq!(e.accounting_factories.get(), 0);
    restore
        .approve_accounting(&accounting_catalog(e.clone()))
        .unwrap();
    let effects = Rc::new(Cell::new(0));
    let catalog = factory_catalog(
        effects.clone(),
        &f.base,
        &f.base_ids,
        &f.new,
        &f.new_ids,
        true,
    );
    let owner = restore
        .owner(&kernel, &mapped, f.wasm, &StateCodec, &catalog)
        .unwrap();
    let missing = factory_catalog(
        effects.clone(),
        &f.base,
        &f.base_ids,
        &f.new,
        &f.new_ids,
        false,
    );
    assert!(matches!(
        restore.candidate(
            &kernel,
            &mapped,
            f.serials[0],
            &owner,
            &StateCodec,
            &missing
        ),
        Err(Error::Schema)
    ));
    assert_eq!(effects.get(), 0);
    assert!(owner.inspect().is_err());
    drop(restore);
    assert!(owner.inspect().is_err());
    // Profile, codec and policy forgery are rejected by independently supplied
    // catalogs before a matching factory or domain codec can run.
    for field in ["accounting", "policy"] {
        let mut checkpoint = serde_json::to_value(&f.owners).unwrap();
        if field == "accounting" {
            checkpoint["pending"]["batches"][0][field]["version"] = serde_json::json!(9);
        } else {
            checkpoint["pending"]["batches"][0][field] = serde_json::json!(vec![8; 32]);
        }
        let mut modified = f.clone();
        modified.owners = serde_json::from_value(checkpoint).unwrap();
        let (k, m, mut r, e) = prepared_build(&modified);
        assert!(r
            .approve_accounting(&accounting_catalog(e.clone()))
            .is_err());
        assert_eq!(e.native_factories.get(), 0);
        assert_eq!(e.accounting_factories.get(), 0);
        drop((k, m, r));
    }
    assert_eq!(
        before,
        (
            destination.wasm.inspect().unwrap(),
            destination.native.inspect().unwrap(),
            destination.readonly.inspect().unwrap(),
            destination.service.inspect().unwrap(),
            destination.host.pending().unwrap()
        )
    );
    assert_eq!(destination.effects.accounting.get(), 0);
    assert_eq!(destination.effects.samples.get(), 0);
    assert_eq!(destination.wasm_effects.get(), 0);
}
struct CountStateCodec(Rc<Cell<usize>>);
impl PersistenceCodec<Vec<u8>> for CountStateCodec {
    fn identity(&self) -> CodecIdentity {
        StateCodec.identity()
    }
    fn encode_state(&self, s: &Vec<u8>, l: usize) -> Result<Vec<u8>> {
        StateCodec.encode_state(s, l)
    }
    fn decode_state(&self, b: &[u8], l: usize, r: &Remap) -> Result<Vec<u8>> {
        self.0.set(self.0.get() + 1);
        StateCodec.decode_state(b, l, r)
    }
    fn validate_state(&self, s: &Vec<u8>) -> Result<()> {
        StateCodec.validate_state(s)
    }
    fn references(&self, s: &Vec<u8>) -> Result<Vec<StateReference>> {
        StateCodec.references(s)
    }
    fn validate_configuration(
        &self,
        s: &Vec<u8>,
        p: &BTreeMap<String, Port>,
        q: &QueryPolicy,
    ) -> Result<()> {
        StateCodec.validate_configuration(s, p, q)
    }
}
#[test]
fn prepared_mapping_codec_policy_and_record_prefix_refuse_before_domain_decode() {
    let f = prepared_fixture();
    let (kernel, mapped, mut restore, e) = prepared_build(&f);
    let (foreign_kernel, foreign_mapped, foreign_restore, _) = prepared_build(&f);
    restore
        .approve_accounting(&accounting_catalog(e.clone()))
        .unwrap();
    let effects = Rc::new(Cell::new(0));
    let catalog = factory_catalog(
        effects.clone(),
        &f.base,
        &f.base_ids,
        &f.new,
        &f.new_ids,
        true,
    );
    let calls = Rc::new(Cell::new(0));
    let codec = CountStateCodec(calls.clone());
    assert!(matches!(
        restore.owner(&foreign_kernel, &foreign_mapped, f.wasm, &codec, &catalog),
        Err(Error::Authority)
    ));
    assert_eq!(calls.get(), 0);
    assert_eq!(effects.get(), 0);
    let owner = restore
        .owner(&kernel, &mapped, f.wasm, &StateCodec, &catalog)
        .unwrap();
    assert!(matches!(
        restore.candidate(
            &foreign_kernel,
            &foreign_mapped,
            f.serials[0],
            &owner,
            &codec,
            &catalog
        ),
        Err(Error::Authority)
    ));
    assert_eq!(calls.get(), 0);
    assert_eq!(effects.get(), 0);
    drop(foreign_restore);
    for field in ["build", "state_codec", "policy"] {
        let mut modified = f.clone();
        let mut v = serde_json::to_value(&f.owners).unwrap();
        if field == "policy" {
            v["pending"]["candidates"][0][field] = serde_json::to_value(new_policy()).unwrap();
        } else {
            v["pending"]["candidates"][0][field]["codec"]["version"] = serde_json::json!(99);
        }
        modified.owners = serde_json::from_value(v).unwrap();
        let (k, m, mut r, e) = prepared_build(&modified);
        r.approve_accounting(&accounting_catalog(e)).unwrap();
        if field == "build" || field == "policy" {
            assert!(matches!(
                r.owner(&k, &m, f.wasm, &codec, &catalog),
                Err(Error::Schema)
            ));
            assert_eq!(calls.get(), 0);
            assert_eq!(effects.get(), 0);
            continue;
        }
        let owner = r.owner(&k, &m, f.wasm, &StateCodec, &catalog).unwrap();
        assert!(matches!(
            r.candidate(&k, &m, f.serials[0], &owner, &codec, &catalog),
            Err(Error::Schema)
        ));
        assert_eq!(calls.get(), 0);
        assert_eq!(effects.get(), 0);
    }
    // A real accepted record is tied to its caller, phase and declaration. A
    // wire index cannot substitute another prefix or invent a credential.
    for (section, field, value) in [
        ("inputs", "caller", serde_json::json!(0)),
        ("inputs", "request", serde_json::json!([99])),
        ("inputs", "operation", serde_json::json!(null)),
    ] {
        let mut v = serde_json::to_value(&f.owners).unwrap();
        if field == "operation" {
            v["pending"][section]["records"][0][field]["phase"] = serde_json::json!("forged");
        } else {
            v["pending"][section]["records"][0][field] = value;
        }
        let c: ExecutionCheckpoint = serde_json::from_value(v).unwrap();
        assert!(OwnerIdentityReservation::new(
            c,
            &f.kernel,
            Limits::default(),
            PublicationLimits::default()
        )
        .is_err());
    }
    let mut v = serde_json::to_value(&f.owners).unwrap();
    v["pending"]["observations"] = serde_json::json!([999]);
    let c: ExecutionCheckpoint = serde_json::from_value(v).unwrap();
    assert!(OwnerIdentityReservation::new(
        c,
        &f.kernel,
        Limits::default(),
        PublicationLimits::default()
    )
    .is_err());
    let mut v = serde_json::to_value(&f.owners).unwrap();
    v["pending"]["candidates"][0]["serial"] = serde_json::json!(f.abandoned_serial);
    let c: ExecutionCheckpoint = serde_json::from_value(v).unwrap();
    assert!(OwnerIdentityReservation::new(
        c,
        &f.kernel,
        Limits::default(),
        PublicationLimits::default()
    )
    .is_err());
    let mut v = serde_json::to_value(&f.owners).unwrap();
    v["pending"]["batches"][0]["ticket"] = serde_json::json!(f.abandoned_ticket);
    let c: ExecutionCheckpoint = serde_json::from_value(v).unwrap();
    assert!(OwnerIdentityReservation::new(
        c,
        &f.kernel,
        Limits::default(),
        PublicationLimits::default()
    )
    .is_err());
    // An old checkpoint cannot authenticate the original recorded candidate.
    let l = prepared_load(&f);
    let mut kernel = l.kernel;
    let op = kernel.issue_operation().unwrap();
    let wrong = kernel
        .operation_binding_with_cleanup(
            op,
            l.behavior.source().pipeline(),
            l.behavior.source().checkpoint(),
            l.behavior.source().phase(),
            &[l.host.cleanup_identity().unwrap()],
        )
        .unwrap();
    *l.action.borrow_mut() = Some(Box::new({
        let batch = l.batch.clone();
        let service = l.service.clone();
        let record = service.inspect().unwrap().records[0].id;
        move |v| {
            assert!(matches!(
                service.observation(record, b"choice", v),
                Err(nethacked_kernel::Error::Authority)
            ));
            assert_eq!(batch.commit(v), Err(Error::Authority));
            Ok(vec![])
        }
    }));
    let keys = [
        &l.wasm as &dyn ExecutionKeys,
        &l.native as &dyn ExecutionKeys,
        &l.readonly as &dyn ExecutionKeys,
    ]
    .into_iter()
    .flat_map(|o| o.keys())
    .chain([
        l.host.publication_resource(),
        Resource {
            owner: "inputs-real".into(),
            key: "source".into(),
        },
    ])
    .collect();
    kernel
        .invoke_operation(
            &wrong,
            &[1],
            Authority {
                access: Access::Write,
                scope: Scope::Keys(keys),
            },
        )
        .unwrap();
    l.action.borrow_mut().take();
    assert_eq!(l.native.inspect().unwrap(), 41_u64.to_le_bytes());
    assert_eq!(l.effects.preparations.get(), 0);
    assert_eq!(l.effects.samples.get(), 0);
    assert_eq!(l.effects.accounting.get(), 0);
}
