// Exact review probes, converted to repair regressions. Originals are retained in repair evidence.
#[test]
fn review_probe_missing_recursive_readonly_edge() {
    let mut f = prepared_fixture();
    let mut v = serde_json::to_value(&f.owners).unwrap();
    assert_eq!(
        v["pending"]["batches"][0]["readonly"][0]["dependencies"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    v["pending"]["batches"][0]["readonly"][0]["dependencies"] = serde_json::json!([]);
    f.owners = ExecutionCheckpoint::from_bytes(&serde_json::to_vec(&v).unwrap(), LIMIT).unwrap();
    assert!(OwnerIdentityReservation::new(
        f.owners,
        &f.kernel,
        Limits::default(),
        PublicationLimits::default()
    )
    .is_err());
}

#[test]
fn review_probe_stale_stamp_tamper_rejected() {
    let mut f = prepared_fixture();
    let mut v = serde_json::to_value(&f.owners).unwrap();
    let old = v["pending"]["candidates"][0]["stamp"]["revision"]
        .as_u64()
        .unwrap();
    // Raise the owner revision only; the prepared allocation still targets old state.
    let owner = v["pending"]["candidates"][0]["stamp"]["owner"].clone();
    let row = v["state"]["owners"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["old"] == owner)
        .unwrap();
    row["revision"] = serde_json::json!(old + 1);
    f.owners = ExecutionCheckpoint::from_bytes(&serde_json::to_vec(&v).unwrap(), LIMIT).unwrap();
    assert!(OwnerIdentityReservation::new(
        f.owners.clone(),
        &f.kernel,
        Limits::default(),
        PublicationLimits::default()
    )
    .is_err());
    println!("NEGATIVE CONTROL: edited owner revision is rejected before domain codecs");
}

#[test]
fn review_probe_revoked_grant_reaches_decode() {
    let f = prepared_fixture();
    let (mut k, m, mut r, e) = prepared_build(&f);
    r.approve_accounting(&accounting_catalog(e.clone()))
        .unwrap();
    let saved = f
        .kernel
        .registry
        .modules
        .iter()
        .find(|m| m.manifest.id == "prepared-caller")
        .unwrap()
        .id;
    k.revoke_grant(m.modules()[&saved], 1).unwrap();
    let calls = Rc::new(Cell::new(0));
    let codec = CountStateCodec(calls.clone());
    let catalog = factory_catalog(
        Rc::new(Cell::new(0)),
        &f.base,
        &f.base_ids,
        &f.new,
        &f.new_ids,
        true,
    );
    assert!(r.owner(&k, &m, f.wasm, &codec, &catalog).is_err());
    assert_eq!(calls.get(), 0);
    assert_eq!(e.native_factories.get(), 0);
}

#[derive(Clone)]
struct ReviewLinkedState {
    value: u64,
    link: OwnerId,
}
struct ReviewLinkedCodec;
impl Schema<ReviewLinkedState> for ReviewLinkedCodec {
    fn validate(&self, _: &ReviewLinkedState) -> Result<()> {
        Ok(())
    }
    fn encode(&self, s: &ReviewLinkedState) -> Result<Vec<u8>> {
        let id = serde_json::to_value(s.link).unwrap();
        Ok([
            s.value.to_le_bytes(),
            id["host"].as_u64().unwrap().to_le_bytes(),
            id["serial"].as_u64().unwrap().to_le_bytes(),
        ]
        .concat())
    }
    fn query(
        &self,
        s: &ReviewLinkedState,
        _: &str,
        _: &[u8],
        _: &[Observation],
    ) -> Result<Vec<u8>> {
        self.encode(s)
    }
}
impl NativeBehavior<ReviewLinkedState> for ReviewLinkedCodec {
    fn prepare(
        &self,
        s: &ReviewLinkedState,
        _: &str,
        _: &[u8],
        _: &[Observation],
    ) -> Result<Proposal<ReviewLinkedState>> {
        Ok(Proposal {
            next: ReviewLinkedState {
                value: s.value + 1,
                link: s.link,
            },
            events: vec![],
            reply: vec![],
        })
    }
}
impl PersistenceCodec<ReviewLinkedState> for ReviewLinkedCodec {
    fn identity(&self) -> CodecIdentity {
        identity(7)
    }
    fn encode_state(&self, s: &ReviewLinkedState, l: usize) -> Result<Vec<u8>> {
        let b = self.encode(s)?;
        if b.len() > l {
            Err(Error::Limit)
        } else {
            Ok(b)
        }
    }
    fn decode_state(&self, b: &[u8], l: usize, r: &Remap) -> Result<ReviewLinkedState> {
        if b.len() != 24 || b.len() > l {
            return Err(Error::Invalid);
        }
        let read = |a: usize| u64::from_le_bytes(b[a..a + 8].try_into().unwrap());
        Ok(ReviewLinkedState {
            value: read(0),
            link: r.owner(
                serde_json::from_value(serde_json::json!({"host":read(8),"serial":read(16)}))
                    .unwrap(),
            )?,
        })
    }
    fn validate_state(&self, s: &ReviewLinkedState) -> Result<()> {
        self.validate(s)
    }
    fn references(&self, s: &ReviewLinkedState) -> Result<Vec<StateReference>> {
        Ok(vec![StateReference {
            owner: s.link,
            generation: 1,
            schema: [1; 32],
            historical: false,
        }])
    }
    fn validate_configuration(
        &self,
        _: &ReviewLinkedState,
        p: &BTreeMap<String, Port>,
        q: &QueryPolicy,
    ) -> Result<()> {
        if q != &QueryPolicy::Noncacheable {
            return Err(Error::Schema);
        }
        config(p)
    }
}
#[test]
fn review_probe_prepared_live_reference_fresh_remap_digest() {
    let mut h = Harness::new(Limits::default(), KernelLimits::default(), "");
    let target_backend: Rc<dyn NativeBehavior<u64>> = Rc::new(NativeDomain);
    let target = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            41u64,
            Backend::Native(target_backend.clone()),
        )
        .unwrap();
    let linked_backend: Rc<dyn NativeBehavior<ReviewLinkedState>> = Rc::new(ReviewLinkedCodec);
    let owner = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            ReviewLinkedState {
                value: 9,
                link: target.id(),
            },
            Backend::Native(linked_backend.clone()),
        )
        .unwrap();
    h.admit(&target);
    h.admit(&owner);
    h.refresh();
    let cell = Rc::new(RefCell::new(None));
    h.run(1, {
        let cell = cell.clone();
        let owner = owner.clone();
        move |v| {
            *cell.borrow_mut() = Some(owner.prepare("prepare", &[], vec![], v).unwrap());
            Ok(vec![])
        }
    });
    let candidate = cell.borrow_mut().take().unwrap();
    let mut roots = ExecutionRoots::default();
    roots.candidate(&candidate).unwrap();
    let live = roots.live_operations(&h.kernel).unwrap();
    let snapshot = h.kernel.kernel_snapshot(&roots.enrollment(&live)).unwrap();
    let index = h.kernel.execution_capture_index(&snapshot).unwrap();
    let mut tc = CaptureCatalog::default();
    tc.register(
        identity(5),
        QueryPolicy::Noncacheable,
        Backend::Native(target_backend),
    )
    .unwrap();
    let mut lc = CaptureCatalog::default();
    lc.register(
        identity(8),
        QueryPolicy::Noncacheable,
        Backend::Native(linked_backend),
    )
    .unwrap();
    let mut capture = h.host.capture_execution(&index).unwrap();
    capture.owner(&target, &NativeDomain, &tc, &[]).unwrap();
    capture.owner(&owner, &ReviewLinkedCodec, &lc, &[]).unwrap();
    let serial = capture
        .candidate(&candidate, &ReviewLinkedCodec, &lc)
        .unwrap();
    let checkpoint = capture.finish().unwrap();
    let checkpoint =
        ExecutionCheckpoint::from_bytes(&checkpoint.to_bytes(LIMIT).unwrap(), LIMIT).unwrap();
    let target_id = target.id();
    let owner_id = owner.id();
    let materials = h.materials.clone();
    let contexts = snapshot.operation_contexts().unwrap();
    drop((target, owner, candidate, h));
    // Payload, schema digest and declared references each remain authenticated.
    let original = serde_json::to_value(&checkpoint).unwrap();
    for field in ["payload", "next_digest", "references"] {
        let mut value = original.clone();
        let row = &mut value["pending"]["candidates"][0];
        match field {
            "payload" => row[field][0] = serde_json::json!(77),
            "next_digest" => row[field][0] = serde_json::json!(row[field][0].as_u64().unwrap() ^ 1),
            _ => row[field][0]["owner"] = serde_json::to_value(owner_id).unwrap(),
        }
        let tampered =
            ExecutionCheckpoint::from_bytes(&serde_json::to_vec(&value).unwrap(), LIMIT).unwrap();
        if OwnerIdentityReservation::new(
            tampered.clone(),
            &snapshot,
            Limits::default(),
            PublicationLimits::default(),
        )
        .is_err()
        {
            assert_eq!(field, "references");
            continue;
        }
        let (kernel, mapped, mut restore, tc, lc, _, _, _) =
            repair_linked_stage(tampered, &snapshot, &materials, &contexts);
        let _target = restore
            .owner(&kernel, &mapped, target_id, &NativeDomain, &tc)
            .unwrap();
        let owner = restore
            .owner(&kernel, &mapped, owner_id, &ReviewLinkedCodec, &lc)
            .unwrap();
        assert!(
            restore
                .candidate(&kernel, &mapped, serial, &owner, &ReviewLinkedCodec, &lc)
                .is_err(),
            "{field}"
        );
        assert!(owner.inspect().is_err());
    }
    let (mut kernel, mapped, mut restore, tc, lc, tc_capture, lc_capture, action) =
        repair_linked_stage(checkpoint, &snapshot, &materials, &contexts);
    let target = restore
        .owner(&kernel, &mapped, target_id, &NativeDomain, &tc)
        .unwrap();
    let owner = restore
        .owner(&kernel, &mapped, owner_id, &ReviewLinkedCodec, &lc)
        .unwrap();
    assert_ne!(target.id(), target_id);
    let candidate = restore
        .candidate(&kernel, &mapped, serial, &owner, &ReviewLinkedCodec, &lc)
        .unwrap();
    // The fresh digest is the exact mapped prepared value, not a new proposal.
    use sha2::Digest;
    let expected = ReviewLinkedCodec
        .encode(&ReviewLinkedState {
            value: 10,
            link: target.id(),
        })
        .unwrap();
    assert_eq!(
        candidate.receipt().next_digest,
        <[u8; 32]>::from(sha2::Sha256::digest(&expected))
    );
    let host = restore.finish().unwrap();
    assert_eq!(host.pending().unwrap(), 1);
    let next = Rc::new(RefCell::new(Some(candidate)));
    let committed = Rc::new(Cell::new(false));
    *action.borrow_mut() = Some(Box::new({
        let next = next.clone();
        let host = host.clone();
        let owner = owner.clone();
        let committed = committed.clone();
        move |view| {
            let batch = host
                .batch(
                    vec![next.borrow_mut().take().unwrap().erase()],
                    vec![],
                    &[owner.id()],
                    vec![owner.id()],
                    [9; 32],
                    Rc::new(RepairLinkedAccounting),
                    cap("prepare"),
                    view,
                )
                .unwrap();
            let receipt = batch.commit(view).unwrap();
            assert_eq!(receipt.participants.len(), 1);
            assert_eq!(batch.commit(view), Err(Error::Stale));
            committed.set(true);
            Ok(vec![])
        }
    }));
    kernel
        .invoke_operation(
            &mapped.operations()[0],
            &[1],
            Authority {
                access: Access::Write,
                scope: Scope::All,
            },
        )
        .unwrap();
    action.borrow_mut().take();
    assert!(committed.get());
    assert_eq!(owner.inspect().unwrap(), expected);
    assert_eq!(host.pending().unwrap(), 0);
    let roots = ExecutionRoots::default();
    let second_snapshot = kernel
        .kernel_snapshot(&roots.enrollment(mapped.operations()))
        .unwrap();
    let second_contexts = second_snapshot.operation_contexts().unwrap();
    let index = kernel.execution_capture_index(&second_snapshot).unwrap();
    let mut capture = host.capture_execution(&index).unwrap();
    capture
        .owner(&target, &NativeDomain, &tc_capture.borrow(), &[])
        .unwrap();
    capture
        .owner(&owner, &ReviewLinkedCodec, &lc_capture.borrow(), &[])
        .unwrap();
    let second = capture.finish().unwrap();
    let second = ExecutionCheckpoint::from_bytes(&second.to_bytes(LIMIT).unwrap(), LIMIT).unwrap();
    let first_target = target.id();
    let first_owner = owner.id();
    drop((
        target, owner, host, mapped, kernel, tc, lc, tc_capture, lc_capture, action, next,
    ));
    let (kernel, mapped, mut restore, tc, lc, _, _, _) =
        repair_linked_stage(second, &second_snapshot, &materials, &second_contexts);
    let target = restore
        .owner(&kernel, &mapped, first_target, &NativeDomain, &tc)
        .unwrap();
    let owner = restore
        .owner(&kernel, &mapped, first_owner, &ReviewLinkedCodec, &lc)
        .unwrap();
    let host = restore.finish().unwrap();
    assert_ne!(target.id(), first_target);
    assert_eq!(
        owner.inspect().unwrap(),
        ReviewLinkedCodec
            .encode(&ReviewLinkedState {
                value: 10,
                link: target.id()
            })
            .unwrap()
    );
    assert_eq!(target.inspect().unwrap(), 41_u64.to_le_bytes());
    assert_eq!(host.pending().unwrap(), 0);
}
struct RepairLinkedAccounting;
impl Accounting for RepairLinkedAccounting {
    fn validate(&self, proposals: &[ProposalView], readonly: &[Observation]) -> Result<()> {
        if proposals.len() != 1
            || !readonly.is_empty()
            || proposals[0].proposed.len() != 24
            || proposals[0].proposed[..8] != 10_u64.to_le_bytes()
        {
            return Err(Error::Invariant);
        }
        Ok(())
    }
}
type RepairLinkedStage = (
    Kernel,
    RestoredKernel,
    ExecutionRestore,
    RestoreCatalog<u64>,
    RestoreCatalog<ReviewLinkedState>,
    Rc<RefCell<CaptureCatalog<u64>>>,
    Rc<RefCell<CaptureCatalog<ReviewLinkedState>>>,
    Rc<RefCell<Option<Action>>>,
);
fn repair_linked_stage(
    checkpoint: ExecutionCheckpoint,
    snapshot: &KernelSnapshot,
    materials: &BTreeMap<[u8; 32], Material>,
    contexts: &[SavedOperationContext],
) -> RepairLinkedStage {
    let reservation = OwnerIdentityReservation::new(
        checkpoint,
        snapshot,
        Limits::default(),
        PublicationLimits::default(),
    )
    .unwrap();
    let action = Rc::new(RefCell::new(None));
    let authority = Approved {
        reservation: &reservation,
        materials,
        contexts,
        action: action.clone(),
        factories: Cell::new(0),
        deny: false,
    };
    let mut kernel = Kernel::new(KernelLimits::default()).unwrap();
    let mapped = kernel.restore_kernel(snapshot, &authority).unwrap();
    drop(authority);
    let restore = reservation.bind(&kernel, &mapped).unwrap();
    let tc_capture = Rc::new(RefCell::new(CaptureCatalog::default()));
    let lc_capture = Rc::new(RefCell::new(CaptureCatalog::default()));
    let mut tc = RestoreCatalog::default();
    tc.allow(identity(5), QueryPolicy::Noncacheable, {
        let c = tc_capture.clone();
        move |_| {
            let backend = Rc::new(NativeDomain) as Rc<dyn NativeBehavior<u64>>;
            c.borrow_mut().register(
                identity(5),
                QueryPolicy::Noncacheable,
                Backend::Native(backend.clone()),
            )?;
            Ok(Backend::Native(backend))
        }
    })
    .unwrap();
    let mut lc = RestoreCatalog::default();
    lc.allow(identity(8), QueryPolicy::Noncacheable, {
        let c = lc_capture.clone();
        move |_| {
            let backend = Rc::new(ReviewLinkedCodec) as Rc<dyn NativeBehavior<ReviewLinkedState>>;
            c.borrow_mut().register(
                identity(8),
                QueryPolicy::Noncacheable,
                Backend::Native(backend.clone()),
            )?;
            Ok(Backend::Native(backend))
        }
    })
    .unwrap();
    (
        kernel, mapped, restore, tc, lc, tc_capture, lc_capture, action,
    )
}

fn review_input_kernel(
    f: &PreparedFixture,
) -> (
    Kernel,
    RestoredKernel,
    ExecutionRestore,
    nethacked_inputs::persistence::Restored,
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
    let mut k = Kernel::new(KernelLimits::default()).unwrap();
    let m = k.restore_kernel(&f.kernel, &authority).unwrap();
    drop(authority);
    let input = input.finish(&k, &m).unwrap();
    let r = reservation.bind(&k, &m).unwrap();
    (k, m, r, input, e)
}
#[test]
fn review_probe_foreign_ledger_same_checkpoint_admitted() {
    let f = prepared_fixture();
    let (k, m, mut r, own_input, e) = review_input_kernel(&f);
    let (_foreign_k, _foreign_m, _foreign_r, foreign_input, _) = review_input_kernel(&f);
    let own_ns =
        serde_json::to_value(own_input.service().inspect().unwrap().records[0].id).unwrap();
    let foreign_ns =
        serde_json::to_value(foreign_input.service().inspect().unwrap().records[0].id).unwrap();
    assert_ne!(own_ns, foreign_ns);
    assert!(r.inputs(foreign_input).is_err());
    assert_eq!(e.native_factories.get(), 0);
    r.inputs(own_input).unwrap();
    r.approve_accounting(&accounting_catalog(e.clone()))
        .unwrap();
    let wc = factory_catalog(
        Rc::new(Cell::new(0)),
        &f.base,
        &f.base_ids,
        &f.new,
        &f.new_ids,
        true,
    );
    let nc = recorded_catalog(e.clone(), None);
    let w = r.owner(&k, &m, f.wasm, &StateCodec, &wc).unwrap();
    let n = r.owner(&k, &m, f.native, &NativeDomain, &nc).unwrap();
    let rd = r.owner(&k, &m, f.readonly, &NativeDomain, &nc).unwrap();
    let cw = r
        .candidate(&k, &m, f.serials[0], &w, &StateCodec, &wc)
        .unwrap();
    let cn = r
        .candidate(&k, &m, f.serials[1], &n, &NativeDomain, &nc)
        .unwrap();
    let b = r
        .batch(&k, &m, f.ticket, &accounting_catalog(e.clone()))
        .unwrap();
    assert_eq!(
        cn.receipt().recorded_inputs[0].operation,
        cn.receipt().operation
    );
    let host = r.finish().unwrap();
    assert_eq!(host.pending().unwrap(), 2);
    assert_eq!(e.native_factories.get(), 2);
    drop((cw, cn, b, rd));
}

#[test]
fn repair_recursive_readonly_extra_duplicate_wrong_and_reordered_edges_refuse() {
    let f = prepared_fixture();
    let original = serde_json::to_value(&f.owners).unwrap();
    let edge = original["pending"]["batches"][0]["readonly"][0]["dependencies"][0].clone();
    for edges in [
        serde_json::json!([edge.clone(), edge.clone()]),
        {
            let mut wrong = edge.clone();
            wrong["name"] = serde_json::json!("derived");
            serde_json::json!([wrong])
        },
        {
            let mut wrong = edge.clone();
            wrong["stamp"]["schema"] = serde_json::json!(vec![9; 32]);
            serde_json::json!([wrong])
        },
        {
            let mut wrong = edge.clone();
            wrong["stamp"]["owner"] =
                original["pending"]["batches"][0]["readonly"][0]["stamp"]["owner"].clone();
            serde_json::json!([wrong])
        },
    ] {
        let mut value = original.clone();
        value["pending"]["batches"][0]["readonly"][0]["dependencies"] = edges;
        let checkpoint =
            ExecutionCheckpoint::from_bytes(&serde_json::to_vec(&value).unwrap(), LIMIT).unwrap();
        assert!(OwnerIdentityReservation::new(
            checkpoint,
            &f.kernel,
            Limits::default(),
            PublicationLimits::default()
        )
        .is_err());
    }
    // The original transitive old-behavior graph still loads and commits.
    let loaded = prepared_load(&f);
    let receipt = prepared_commit(&loaded);
    assert_eq!(receipt.observations[0].dependencies.len(), 1);
}

#[test]
fn repair_live_grant_narrowing_and_read_recorded_revocation_precede_decode() {
    let f = prepared_fixture();
    for slot in [0, 1, 3] {
        for narrow in [false, true] {
            let (mut k, m, mut r, e) = prepared_build(&f);
            r.approve_accounting(&accounting_catalog(e.clone()))
                .unwrap();
            let saved = f
                .kernel
                .registry
                .modules
                .iter()
                .find(|m| m.manifest.id == "prepared-caller")
                .unwrap()
                .id;
            let caller = m.modules()[&saved];
            if narrow {
                k.issue_grant(
                    caller,
                    slot,
                    Authority {
                        access: if slot == 0 {
                            Access::Read
                        } else {
                            Access::Write
                        },
                        scope: Scope::Keys(BTreeSet::new()),
                    },
                )
                .unwrap();
            } else {
                k.revoke_grant(caller, slot).unwrap();
            }
            let calls = Rc::new(Cell::new(0));
            let codec = CountStateCodec(calls.clone());
            let catalog = factory_catalog(
                Rc::new(Cell::new(0)),
                &f.base,
                &f.base_ids,
                &f.new,
                &f.new_ids,
                true,
            );
            assert!(
                r.owner(&k, &m, f.wasm, &codec, &catalog).is_err(),
                "slot {slot}, narrow {narrow}"
            );
            assert_eq!(calls.get(), 0);
            assert_eq!(e.native_factories.get(), 0);
        }
    }
    // Sufficient independently approved original grants remain admissible.
    let loaded = prepared_load(&f);
    assert_eq!(
        prepared_commit(&loaded).observations[0].dependencies.len(),
        1
    );
}

#[test]
fn repair_ordered_recursive_readonly_declaration_is_exact() {
    let f = prepared_fixture();
    let mut value = serde_json::to_value(&f.owners).unwrap();
    let parent = value["pending"]["batches"][0]["readonly"][0]["stamp"]["owner"].clone();
    let mut child = value["pending"]["batches"][0]["readonly"][0]["dependencies"][0].clone();
    let native = value["state"]["owners"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["old"] == serde_json::to_value(f.native).unwrap())
        .unwrap()
        .clone();
    for field in ["owner", "generation", "revision", "schema"] {
        child["stamp"][field] = native[if field == "owner" { "old" } else { field }].clone();
    }
    child["revision"] = serde_json::json!(0);
    child["helper"] = serde_json::Value::Null;
    child["retained_execution"] = serde_json::json!(false);
    child["data"] = serde_json::json!(41_u64.to_le_bytes());
    let declaring = value["state"]["owners"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["old"] == parent)
        .unwrap();
    let mut declared = declaring["ports"]["derived"]["dependencies"][0].clone();
    declared["owner"] = native["old"].clone();
    declaring["ports"]["derived"]["dependencies"]
        .as_array_mut()
        .unwrap()
        .push(declared);
    value["pending"]["batches"][0]["readonly"][0]["dependencies"]
        .as_array_mut()
        .unwrap()
        .push(child);
    let checkpoint =
        ExecutionCheckpoint::from_bytes(&serde_json::to_vec(&value).unwrap(), LIMIT).unwrap();
    assert!(OwnerIdentityReservation::new(
        checkpoint,
        &f.kernel,
        Limits::default(),
        PublicationLimits::default()
    )
    .is_ok());
    value["pending"]["batches"][0]["readonly"][0]["dependencies"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let checkpoint =
        ExecutionCheckpoint::from_bytes(&serde_json::to_vec(&value).unwrap(), LIMIT).unwrap();
    assert!(OwnerIdentityReservation::new(
        checkpoint,
        &f.kernel,
        Limits::default(),
        PublicationLimits::default()
    )
    .is_err());
}

#[test]
fn repair_live_revocation_at_candidate_and_batch_admission() {
    let f = prepared_fixture();
    for at_batch in [false, true] {
        let (mut k, m, mut r, e) = prepared_build(&f);
        r.approve_accounting(&accounting_catalog(e.clone()))
            .unwrap();
        let calls = Rc::new(Cell::new(0));
        let codec = CountStateCodec(calls.clone());
        let wc = factory_catalog(
            Rc::new(Cell::new(0)),
            &f.base,
            &f.base_ids,
            &f.new,
            &f.new_ids,
            true,
        );
        let nc = recorded_catalog(e.clone(), None);
        let w = r.owner(&k, &m, f.wasm, &codec, &wc).unwrap();
        let n = r.owner(&k, &m, f.native, &NativeDomain, &nc).unwrap();
        let rd = r.owner(&k, &m, f.readonly, &NativeDomain, &nc).unwrap();
        let candidates = if at_batch {
            Some((
                r.candidate(&k, &m, f.serials[0], &w, &codec, &wc).unwrap(),
                r.candidate(&k, &m, f.serials[1], &n, &NativeDomain, &nc)
                    .unwrap(),
            ))
        } else {
            None
        };
        let saved = f
            .kernel
            .registry
            .modules
            .iter()
            .find(|m| m.manifest.id == "prepared-caller")
            .unwrap()
            .id;
        k.revoke_grant(m.modules()[&saved], 1).unwrap();
        calls.set(0);
        let factories = e.accounting_factories.get();
        if at_batch {
            assert!(r
                .batch(&k, &m, f.ticket, &accounting_catalog(e.clone()))
                .is_err());
        } else {
            assert!(r.candidate(&k, &m, f.serials[0], &w, &codec, &wc).is_err());
        }
        assert_eq!(calls.get(), 0);
        assert_eq!(e.accounting_factories.get(), factories);
        drop((candidates, rd));
    }
}
