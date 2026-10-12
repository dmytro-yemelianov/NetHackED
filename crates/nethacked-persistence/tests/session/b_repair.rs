use super::*;

#[test]
fn concrete_edges_exact_one_over_multisection_and_later_refusal_preserve_stage() {
    let f = simple_fixture();
    let owner: ExecutionCheckpoint = trusted_section(&f.wire, "owners");
    let mut kernel_links = ReferenceCounter::new();
    f.approved.count_references(&mut kernel_links);
    let kernel_total = kernel_links.total().unwrap();
    let mut owner_links = ReferenceCounter::new();
    owner.count_references(&mut owner_links);
    let owner_total = owner_links.total().unwrap();
    let exact = kernel_total
        .checked_add(owner_total)
        .unwrap()
        .checked_add(1)
        .unwrap(); // outer owner root
    assert!(kernel_total > 0 && owner_total > 0);
    assert!(exact > kernel_total && exact > owner_total);
    for (limit, admitted) in [
        (0, false),
        (exact - 1, false),
        (exact, true),
        (exact + 1, true),
    ] {
        let factories = Rc::new(Cell::new(0));
        let mut a = authority(&f, factories.clone());
        a.limits.aggregate_edges = limit;
        let (s, k, c) = destination(a);
        let state = owner_state(&s, &k);
        let result = s.prepare_restore(&k, &composer_identity(), &f.wire);
        assert_eq!(result.is_ok(), admitted);
        if !admitted {
            assert_eq!(factories.get(), 0);
        }
        assert_eq!(owner_state(&s, &k), state);
        s.with_runtime(&c, |b| assert_eq!(b.host().pending().unwrap(), 0))
            .unwrap();
    }
    let factories = Rc::new(Cell::new(0));
    let mut a = authority(&f, factories.clone());
    a.limits.aggregate_edges = exact;
    let (s, k, _) = destination(a);
    let mut initial = s
        .prepare_restore(&k, &composer_identity(), &f.wire)
        .unwrap();
    s.activate(&mut initial).unwrap();
    let credential = s.issue_credential(&k).unwrap();
    let mut stage = s
        .prepare_restore(&k, &composer_identity(), &f.wire)
        .unwrap();
    let before = owner_state(&s, &k);
    factories.set(0);
    let malformed = rewrite_section(&f.wire, "owners", |v| {
        v["unknown_later_section"] = serde_json::json!(1);
    });
    assert!(s
        .prepare_restore(&k, &composer_identity(), &malformed)
        .is_err());
    assert_eq!(factories.get(), 0);
    assert_eq!(owner_state(&s, &k), before);
    s.with_runtime(&credential, |b| {
        assert_eq!(
            b.owner::<u64>("native").unwrap().owner.inspect().unwrap(),
            7u64.to_le_bytes()
        )
    })
    .unwrap();
    s.activate(&mut stage).unwrap();
    let mut n = ReferenceCounter::new();
    n.add(usize::MAX);
    n.add(1);
    assert_eq!(n.total(), None);
}

#[test]
fn exact_dependency_diagnostics_preserve_nonempty_destination_credentials_and_stage() {
    let f = simple_fixture();
    let factories = Rc::new(Cell::new(0));
    let (s, k, _) = destination(authority(&f, factories.clone()));
    let mut first = s
        .prepare_restore(&k, &composer_identity(), &f.wire)
        .unwrap();
    s.activate(&mut first).unwrap();
    let credential = s.issue_credential(&k).unwrap();
    let mut stage = s
        .prepare_restore(&k, &composer_identity(), &f.wire)
        .unwrap();
    let before = owner_state(&s, &k);
    for (field, label) in [
        ("schema", "schema"),
        ("codec", "codec"),
        ("policy", "policy"),
    ] {
        let missing = rewrite_section(&f.wire, "owners", |v| {
            v["current"][0]["build"][field]["name"] =
                serde_json::json!(format!("missing-{field}-exact"));
        });
        factories.set(0);
        let error = match s.prepare_restore_diagnosed(&k, &composer_identity(), &missing) {
            Err(e) => e,
            Ok(_) => panic!("missing dependency admitted"),
        };
        assert_eq!(error.error, runtime::Error::FactoryRefused);
        assert!(error.diagnostics.iter().any(|d| d
            .dependency
            .as_ref()
            .is_some_and(|s| s.contains(&format!("missing-{label}-exact")))));
        assert_eq!(factories.get(), 0);
        assert_eq!(owner_state(&s, &k), before);
        s.with_runtime(&credential, |b| {
            assert_eq!(
                b.owner::<u64>("native").unwrap().owner.inspect().unwrap(),
                7u64.to_le_bytes()
            )
        })
        .unwrap();
    }
    // Missing executable artifact is named at the actual resolver boundary.
    let mut a = authority(&f, factories.clone());
    let mut materials = f.materials.clone();
    let artifact = f.approved.registry.modules[0].artifact;
    materials.remove(&artifact);
    a.kernel = Rc::new(CompositionKernel {
        materials,
        factories: factories.clone(),
        approved: f.approved.clone(),
    });
    let (missing, k2, _) = destination(a);
    let error = match missing.prepare_restore_diagnosed(&k2, &composer_identity(), &f.wire) {
        Err(e) => e,
        Ok(_) => panic!("missing artifact admitted"),
    };
    assert!(error.diagnostics.iter().any(|d| d.section == "kernel"
        && d.dependency
            .as_ref()
            .is_some_and(|s| s.contains(&format!("{artifact:?}")))));
    s.activate(&mut stage).unwrap();
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

struct SimpleFixture {
    wire: Vec<u8>,
    materials: BTreeMap<[u8; 32], Material>,
    binding: CapabilityPermit,
    approved: KernelSnapshot,
}
fn simple_fixture() -> SimpleFixture {
    let mut h = Harness::new(Limits::default(), KernelLimits::default(), "");
    let backend: Rc<dyn NativeBehavior<u64>> = Rc::new(NativeDomain);
    let owner = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            7_u64,
            Backend::Native(backend.clone()),
        )
        .unwrap();
    h.admit(&owner);
    h.refresh();
    let binding = h.binding();
    let materials = h.materials.clone();
    h.kernel.release_pipeline(h.lease).unwrap();
    drop(h.operation);
    let mut catalog = CaptureCatalog::default();
    catalog
        .register(
            identity(4),
            QueryPolicy::Noncacheable,
            Backend::Native(backend),
        )
        .unwrap();
    let config =
        runtime::ApprovedConfiguration::new(BTreeSet::new(), BTreeSet::new(), vec![]).unwrap();
    let (source, key, _) = composed::Session::new(
        h.kernel,
        h.host,
        vec![Rc::new(composed::TypedOwner {
            key: "native".into(),
            owner: owner.clone(),
            codec: Rc::new(NativeDomain),
            catalog,
            retained: vec![],
            callbacks: vec![],
            target_port: None,
            candidates: vec![],
            batch_candidates: vec![],
        })],
        vec![],
        AccountingCaptureCatalog::default(),
        Default::default(),
        config,
    )
    .unwrap();
    let wire = source.capture(&key, SaveLimits::default()).unwrap();
    drop(source);
    drop(owner);
    drop(h.action);
    let approved = trusted_section(&wire, "kernel");
    SimpleFixture {
        wire,
        materials,
        binding,
        approved,
    }
}
fn authority(f: &SimpleFixture, factories: Rc<Cell<usize>>) -> composed::RestoreAuthority {
    composition_authority(
        f.materials.clone(),
        f.binding.clone(),
        vec![],
        Rc::new(Cell::new(0)),
        factories,
        f.approved.clone(),
    )
}
fn destination(
    a: composed::RestoreAuthority,
) -> (
    composed::Session,
    runtime::AuthorityKey,
    runtime::RuntimeCredential,
) {
    composed::Session::new(
        Kernel::new(KernelLimits::default()).unwrap(),
        Host::new(Limits::default()).unwrap(),
        vec![],
        vec![],
        AccountingCaptureCatalog::default(),
        Default::default(),
        composed::SessionFactory::new(a)
            .configuration(composer_identity())
            .unwrap(),
    )
    .unwrap()
}
fn owner_state(s: &composed::Session, k: &runtime::AuthorityKey) -> serde_json::Value {
    trusted_section(&s.capture(k, SaveLimits::default()).unwrap(), "owners")
}

struct PolicyAccounting {
    reject: bool,
    calls: Rc<Cell<usize>>,
}
impl Accounting for PolicyAccounting {
    fn validate(&self, _: &[ProposalView], _: &[Observation]) -> Result<()> {
        self.calls.set(self.calls.get() + 1);
        if self.reject {
            Err(Error::Invariant)
        } else {
            Ok(())
        }
    }
}
fn account_id(name: &str, n: u8) -> ArtifactIdentity {
    ArtifactIdentity {
        name: name.into(),
        version: 1,
        artifact: [n; 32],
    }
}

#[test]
fn required_good_resave_preserves_exact_accounting_identity_when_policy_is_shared() {
    let mut h = Harness::new(Limits::default(), KernelLimits::default(), "");
    let backend: Rc<dyn NativeBehavior<u64>> = Rc::new(NativeDomain);
    let owner = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            7_u64,
            Backend::Native(backend.clone()),
        )
        .unwrap();
    h.admit(&owner);
    h.refresh();
    let output = Rc::new(RefCell::new(None));
    let out = output.clone();
    let o = owner.clone();
    let host = h.host.clone();
    let source_accounting: Rc<dyn Accounting> = Rc::new(PolicyAccounting {
        reject: false,
        calls: Rc::new(Cell::new(0)),
    });
    let a = source_accounting.clone();
    h.run(1, move |view| {
        let c = o.prepare("prepare", &[], vec![], view).unwrap();
        *out.borrow_mut() = Some(
            host.batch(
                vec![c.erase()],
                vec![],
                &[o.id()],
                vec![o.id()],
                [41; 32],
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
    let materials = h.materials.clone();
    let binding = h.binding();
    let mut catalog = CaptureCatalog::default();
    catalog
        .register(
            identity(4),
            QueryPolicy::Noncacheable,
            Backend::Native(backend),
        )
        .unwrap();
    let mut ac = AccountingCaptureCatalog::default();
    ac.register(account_id("B-accept", 42), [41; 32], source_accounting)
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
            catalog,
            retained: vec![],
            callbacks: vec![],
            target_port: None,
            candidates: vec![],
            batch_candidates: serials,
        })],
        vec![batch],
        ac,
        Default::default(),
        empty,
    )
    .unwrap();
    let wire = source.capture(&key, SaveLimits::default()).unwrap();
    drop(source);
    drop(owner);
    drop(h.action);
    let reject_calls = Rc::new(Cell::new(0));
    let accept_calls = Rc::new(Cell::new(0));
    let config = |trusted: &[u8]| {
        let mut a = composition_authority(
            materials.clone(),
            binding.clone(),
            vec![],
            Rc::new(Cell::new(0)),
            Rc::new(Cell::new(0)),
            trusted_section(trusted, "kernel"),
        );
        let mut accounting = AccountingRestoreCatalog::default();
        let count = reject_calls.clone();
        accounting
            .allow(account_id("A-reject", 41), [41; 32], move || {
                Ok(Rc::new(PolicyAccounting {
                    reject: true,
                    calls: count.clone(),
                }))
            })
            .unwrap();
        let count = accept_calls.clone();
        accounting
            .allow(account_id("B-accept", 42), [41; 32], move || {
                Ok(Rc::new(PolicyAccounting {
                    reject: false,
                    calls: count.clone(),
                }))
            })
            .unwrap();
        a.accounting = Rc::new(accounting);
        composed::SessionFactory::new(a)
            .configuration(composer_identity())
            .unwrap()
    };
    let (s, k, _) = composed::Session::new(
        Kernel::new(KernelLimits::default()).unwrap(),
        Host::new(Limits::default()).unwrap(),
        vec![],
        vec![],
        AccountingCaptureCatalog::default(),
        Default::default(),
        config(&wire),
    )
    .unwrap();
    let mut stage = s.prepare_restore(&k, &composer_identity(), &wire).unwrap();
    s.activate(&mut stage).unwrap();
    let resaved = s.capture(&k, SaveLimits::default()).unwrap();
    let first: serde_json::Value = trusted_section(&wire, "owners");
    let second: serde_json::Value = trusted_section(&resaved, "owners");
    let before_id = first["pending"]["batches"][0]["accounting"].clone();
    let after_id = second["pending"]["batches"][0]["accounting"].clone();
    println!("ACCOUNTING: before={before_id}, after={after_id}");
    assert_eq!(before_id["name"], "B-accept");
    assert_eq!(accept_calls.get(), 0);
    assert_eq!(reject_calls.get(), 0);
    s.replace_configuration(&k, config(&resaved)).unwrap();
    let mut stage = s
        .prepare_restore(&k, &composer_identity(), &resaved)
        .unwrap();
    let activation = s.activate(&mut stage).unwrap();
    let commit_ok = s
        .with_runtime(activation.credential(), |b| {
            let op = b.owner::<u64>("native").unwrap().candidates[0]
                .receipt()
                .operation
                .clone();
            let result = b.kernel().unwrap().invoke_operation(
                &op,
                &[1],
                Authority {
                    access: Access::Write,
                    scope: Scope::All,
                },
            );
            println!(
                "AFTER SECOND LOAD: commit_ok={}, reject calls={}, accepted calls={}, history={}",
                result.is_ok(),
                reject_calls.get(),
                accept_calls.get(),
                b.host().history().unwrap().len()
            );
            result.is_ok()
        })
        .unwrap();
    assert_eq!(
        after_id, before_id,
        "resave must bind the actual restored accounting allocation to its exact approved identity"
    );
    assert!(
        commit_ok,
        "second byte-only load must preserve B's accepting semantics"
    );
}
#[test]
fn shared_accounting_repeated_resave_and_exact_missing_policy_preserve_stage() {
    let mut h = Harness::new(Limits::default(), KernelLimits::default(), "");
    let backend: Rc<dyn NativeBehavior<u64>> = Rc::new(NativeDomain);
    let owner = h
        .host
        .enroll(
            &h.kernel,
            h.spec(1, vec![]),
            7_u64,
            Backend::Native(backend.clone()),
        )
        .unwrap();
    h.admit(&owner);
    h.refresh();
    let output = Rc::new(RefCell::new(None));
    let out = output.clone();
    let o = owner.clone();
    let host = h.host.clone();
    let source_accounting: Rc<dyn Accounting> = Rc::new(PolicyAccounting {
        reject: false,
        calls: Rc::new(Cell::new(0)),
    });
    let a = source_accounting.clone();
    h.run(1, move |view| {
        let mut batches = vec![];
        for _ in 0..2 {
            let c = o.prepare("prepare", &[], vec![], view).unwrap();
            batches.push(
                host.batch(
                    vec![c.erase()],
                    vec![],
                    &[o.id()],
                    vec![o.id()],
                    [41; 32],
                    a.clone(),
                    cap("prepare"),
                    view,
                )
                .unwrap(),
            );
        }
        *out.borrow_mut() = Some(batches);
        Ok(vec![])
    });
    let batches = output.borrow_mut().take().unwrap();
    let serials = batches
        .iter()
        .flat_map(Batch::persistence_candidate_serials)
        .collect();
    let materials = h.materials.clone();
    let binding = h.binding();
    let mut catalog = CaptureCatalog::default();
    catalog
        .register(
            identity(4),
            QueryPolicy::Noncacheable,
            Backend::Native(backend),
        )
        .unwrap();
    let mut ac = AccountingCaptureCatalog::default();
    ac.register(account_id("B-accept", 42), [41; 32], source_accounting)
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
            catalog,
            retained: vec![],
            callbacks: vec![],
            target_port: None,
            candidates: vec![],
            batch_candidates: serials,
        })],
        batches,
        ac,
        Default::default(),
        empty,
    )
    .unwrap();
    let wire = source.capture(&key, SaveLimits::default()).unwrap();
    drop(source);
    drop(owner);
    drop(h.action);
    let builds = Rc::new(Cell::new(0));
    let validations = Rc::new(Cell::new(0));
    let factories = Rc::new(Cell::new(0));
    let config = |trusted: &[u8]| {
        let mut a = composition_authority(
            materials.clone(),
            binding.clone(),
            vec![],
            Rc::new(Cell::new(0)),
            factories.clone(),
            trusted_section(trusted, "kernel"),
        );
        let mut ac = AccountingRestoreCatalog::default();
        ac.allow(account_id("A-reject", 41), [41; 32], || {
            Ok(Rc::new(PolicyAccounting {
                reject: true,
                calls: Rc::new(Cell::new(0)),
            }))
        })
        .unwrap();
        let b = builds.clone();
        let v = validations.clone();
        ac.allow(account_id("B-accept", 42), [41; 32], move || {
            b.set(b.get() + 1);
            Ok(Rc::new(PolicyAccounting {
                reject: false,
                calls: v.clone(),
            }))
        })
        .unwrap();
        a.accounting = Rc::new(ac);
        composed::SessionFactory::new(a)
            .configuration(composer_identity())
            .unwrap()
    };
    let (s, k, _) = composed::Session::new(
        Kernel::new(KernelLimits::default()).unwrap(),
        Host::new(Limits::default()).unwrap(),
        vec![],
        vec![],
        AccountingCaptureCatalog::default(),
        Default::default(),
        config(&wire),
    )
    .unwrap();
    let mut saved = wire;
    for iteration in 1..=4 {
        if iteration > 1 {
            s.replace_configuration(&k, config(&saved)).unwrap();
        }
        let mut stage = s.prepare_restore(&k, &composer_identity(), &saved).unwrap();
        s.activate(&mut stage).unwrap();
        saved = s.capture(&k, SaveLimits::default()).unwrap();
        let owners: serde_json::Value = trusted_section(&saved, "owners");
        let batches = owners["pending"]["batches"].as_array().unwrap();
        assert_eq!(batches.len(), 2);
        assert!(batches
            .iter()
            .all(|b| b["accounting"]["name"] == "B-accept"));
        assert_eq!(builds.get(), iteration);
        assert_eq!(validations.get(), 0);
    }
    s.replace_configuration(&k, config(&saved)).unwrap();
    let credential = s.issue_credential(&k).unwrap();
    let mut stage = s.prepare_restore(&k, &composer_identity(), &saved).unwrap();
    let before = owner_state(&s, &k);
    let built = builds.get();
    factories.set(0);
    let missing = rewrite_section(&saved, "owners", |v| {
        v["pending"]["batches"][0]["accounting"]["name"] =
            serde_json::json!("missing-exact-accounting");
    });
    let error = match s.prepare_restore_diagnosed(&k, &composer_identity(), &missing) {
        Err(e) => e,
        Ok(_) => panic!("missing accounting admitted"),
    };
    assert!(error.diagnostics.iter().any(|d| d
        .dependency
        .as_ref()
        .is_some_and(|s| s.contains("missing-exact-accounting") && s.contains("policy"))));
    assert_eq!(factories.get(), 0);
    assert_eq!(builds.get(), built);
    assert_eq!(validations.get(), 0);
    assert_eq!(owner_state(&s, &k), before);
    s.with_runtime(&credential, |b| {
        assert_eq!(b.host().history().unwrap().len(), 0)
    })
    .unwrap();
    s.activate(&mut stage).unwrap();
}
