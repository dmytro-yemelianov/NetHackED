// Repair fixtures; the six prior fixtures above remain verbatim.
fn repaired_restore(h: &mut Harness, cp: Checkpoint, schema: u8) -> Vec<Rc<Owner<NativeState>>> {
    let ids = cp.owners();
    let mut restore = fresh_restore(h, cp);
    let owners = ids
        .iter()
        .map(|id| {
            restore
                .owner(&h.kernel, *id, h.module, &NativeCodec(schema), || {
                    Ok(Backend::Native(Rc::new(NativeDomain)))
                })
                .unwrap()
        })
        .collect::<Vec<_>>();
    h.host = restore.finish().unwrap();
    h.keys.clear();
    for owner in &owners {
        h.admit(owner);
    }
    h.refresh();
    owners
}
#[test]
#[ignore]
fn repair_process_child() {
    let mode = std::env::var("OWNER_REPAIR_MODE").unwrap();
    let path = std::env::var("OWNER_REPAIR_FILE").unwrap();
    let mut h = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    if mode.starts_with("write") {
        h.host = Host::new(Limits::default()).unwrap(); // exact original process-local host 2
        let owner = native(&h, None, vec![]);
        h.admit(&owner);
        h.refresh();
        let _batch = publish(&h, owner.clone(), 1);
        let mut cp = capture_native(&h, &[owner]);
        if mode == "write-history" {
            let mut owners = repaired_restore(&mut h, cp, 1);
            h.host.retire(owners[0].id()).unwrap();
            owners.clear();
            let owner = native(&h, None, vec![]);
            h.admit(&owner);
            h.refresh();
            let _batch = publish(&h, owner.clone(), 2);
            cp = capture_native(&h, &[owner]);
            let mut owners = repaired_restore(&mut h, cp, 1);
            h.host.retire(owners[0].id()).unwrap();
            owners.clear();
            let owner = native(&h, None, vec![]);
            cp = capture_native(&h, &[owner]);
        }
        std::fs::write(path, cp.to_bytes(WIRE_LIMIT).unwrap()).unwrap();
    } else {
        let cp = Checkpoint::from_bytes(&std::fs::read(path).unwrap(), WIRE_LIMIT).unwrap();
        let value = serde_json::to_value(&cp).unwrap();
        let origins = value["origins"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect::<Vec<_>>();
        let old = cp.owners()[0];
        let mut restore = fresh_restore(&h, cp);
        let owner = restore
            .owner(&h.kernel, old, h.module, &NativeCodec(1), || {
                Ok(Backend::Native(Rc::new(NativeDomain)))
            })
            .unwrap();
        assert_ne!(old, owner.id(), "F1 immediate origin must be excluded");
        let new_host = serde_json::to_value(owner.id()).unwrap()["host"]
            .as_u64()
            .unwrap();
        assert!(
            !origins.contains(&new_host),
            "F1 disposed historical origins must be excluded"
        );
        let host = restore.finish().unwrap();
        assert_eq!(host.retire(old), Err(Error::Stale));
    }
}
#[test]
fn repair_separate_process_bytes_and_disposed_origins() {
    let path = std::env::temp_dir().join(format!("owners-repair-{}.json", std::process::id()));
    for mode in ["write", "read", "write-history", "read"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "repair_process_child",
                "--nocapture",
            ])
            .env("OWNER_REPAIR_MODE", mode)
            .env("OWNER_REPAIR_FILE", &path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{mode}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    std::fs::remove_file(path).unwrap();
}
fn publish_pair(h: &Harness, a: Rc<Owner<NativeState>>, b: Rc<Owner<NativeState>>, reverse: bool) {
    let host = h.host.clone();
    h.run(1, move |view| {
        let ca = a.prepare("prepare", &[1], vec![], view).unwrap();
        let cb = b.prepare("prepare", &[2], vec![], view).unwrap();
        let order = if reverse {
            vec![b.id(), a.id()]
        } else {
            vec![a.id(), b.id()]
        };
        host.batch(
            vec![ca.erase(), cb.erase()],
            vec![],
            &[a.id(), b.id()],
            order,
            [9; 32],
            Rc::new(Accept),
            cap("prepare"),
            view,
        )
        .unwrap()
        .commit(view)
        .unwrap();
        Ok(vec![])
    });
}
#[test]
fn repair_complete_ordered_history_refusals_and_restart_lineage() {
    let mut source = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    let a = native(&source, None, vec![]);
    let b = native(&source, None, vec![]);
    source.admit(&a);
    source.admit(&b);
    source.refresh();
    publish_pair(&source, a.clone(), b.clone(), false);
    publish_pair(&source, a.clone(), b.clone(), true);
    let _different_participants = publish(&source, a.clone(), 3);
    let cp = capture_native(&source, &[a.clone(), b.clone()]);
    let mut destination = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    let existing = native(&destination, None, vec![]);
    let before = existing.inspect().unwrap();
    let source_before = a.inspect().unwrap();
    type HistoryMutation = (&'static str, Box<dyn Fn(&mut serde_json::Value)>);
    let mutations: Vec<HistoryMutation> = vec![
        (
            "original diagnostic",
            Box::new(|v| {
                let a = v["history"]["records"][0]["origin_encoding"]
                    .as_array_mut()
                    .unwrap();
                *a.last_mut().unwrap() = 88.into();
            }),
        ),
        (
            "event payload",
            Box::new(|v| v["history"]["records"][0]["events"][0][0] = 99.into()),
        ),
        (
            "reply payload",
            Box::new(|v| {
                let a = v["history"]["records"][0]["reply"].as_array_mut().unwrap();
                *a.last_mut().unwrap() = 99.into();
            }),
        ),
        (
            "participant schema",
            Box::new(|v| v["history"]["records"][0]["participants"][0]["schema"][0] = 99.into()),
        ),
        (
            "participant generation",
            Box::new(|v| v["history"]["records"][0]["participants"][0]["generation"] = 99.into()),
        ),
        (
            "second revision",
            Box::new(|v| v["history"]["records"][1]["revisions"][0] = 0.into()),
        ),
        (
            "consistent second revision regression",
            Box::new(|v| {
                let r = &mut v["history"]["records"][1];
                r["revisions"] = serde_json::json!([0, 0]);
                for p in r["evidence"]["participants"].as_array_mut().unwrap() {
                    p["stamp"]["revision"] = 0.into();
                }
                let bytes =
                    serde_json::from_value::<Vec<u8>>(r["origin_encoding"].clone()).unwrap();
                let diagnostic = String::from_utf8(bytes)
                    .unwrap()
                    .replace("revision: 1", "revision: 0");
                r["origin_encoding"] = serde_json::to_value(diagnostic.into_bytes()).unwrap();
            }),
        ),
        (
            "policy",
            Box::new(|v| v["history"]["records"][0]["policy"][0] = 99.into()),
        ),
        (
            "participant order",
            Box::new(|v| {
                v["history"]["records"][0]["participants"]
                    .as_array_mut()
                    .unwrap()
                    .swap(0, 1)
            }),
        ),
        (
            "candidate ID",
            Box::new(|v| v["history"]["records"][0]["candidate_serials"][0] = 2.into()),
        ),
        (
            "origin",
            Box::new(|v| v["history"]["records"][0]["origin_host"] = 0.into()),
        ),
        (
            "event contribution",
            Box::new(|v| {
                v["history"]["records"][0]["contributions"][0]["events"][0][0] = 99.into()
            }),
        ),
        (
            "reply contribution",
            Box::new(|v| v["history"]["records"][0]["contributions"][0]["reply"][0] = 99.into()),
        ),
        (
            "typed stamp",
            Box::new(|v| {
                v["history"]["records"][0]["evidence"]["participants"][0]["stamp"]["schema"][0] =
                    99.into()
            }),
        ),
    ];
    for (name, change) in mutations {
        let changed = mutate(&cp, change);
        assert!(
            matches!(
                Restore::new(
                    &destination.kernel,
                    changed,
                    Limits::default(),
                    PublicationLimits::default(),
                    &bindings(&destination)
                ),
                Err(Error::Invalid)
            ),
            "specific history refusal before any factory: {name}"
        );
        assert_eq!(existing.inspect().unwrap(), before);
        assert_eq!(a.inspect().unwrap(), source_before);
        let positive = repaired_restore(&mut destination, bytes_checkpoint(&cp), 1);
        assert_eq!(positive[0].stamp().unwrap().revision, 3);
    }
    let owners = repaired_restore(&mut destination, cp, 1);
    publish_pair(&destination, owners[0].clone(), owners[1].clone(), false);
    destination.host.retire(owners[1].id()).unwrap();
    let retired = owners[1].id();
    let new = native(&destination, None, vec![]);
    assert_ne!(new.id(), retired);
    let cp = capture_native(&destination, &[owners[0].clone(), new]);
    let restored = repaired_restore(&mut source, bytes_checkpoint(&cp), 1);
    assert_eq!(restored[0].stamp().unwrap().revision, 4);
    assert_eq!(source.host.persistence_history().unwrap().len(), 4);
    let cp = capture_native(&source, &restored);
    let _ = repaired_restore(&mut destination, bytes_checkpoint(&cp), 1);
}
#[test]
fn repair_legitimate_migration_retains_historical_schema() {
    let mut source = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    let owner = native(&source, None, vec![]);
    source.admit(&owner);
    source.refresh();
    let _batch = publish(&source, owner.clone(), 1);
    let cp = capture_native(&source, &[owner.clone()]);
    let mut fresh = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    let mut restore = fresh_restore(&fresh, cp);
    let migrated = restore
        .migrate(
            &fresh.kernel,
            owner.id(),
            fresh.module,
            &NativeCodec(1),
            &NativeCodec(4),
            |s, _| Ok(s),
            || Ok(Backend::Native(Rc::new(NativeDomain))),
        )
        .unwrap();
    fresh.host = restore.finish().unwrap();
    fresh.admit(&migrated);
    fresh.refresh();
    let _batch = publish(&fresh, migrated.clone(), 2);
    let mut capture = fresh.host.capture_state().unwrap();
    capture.owner(&migrated, &NativeCodec(4)).unwrap();
    let cp = bytes_checkpoint(&capture.finish().unwrap());
    let restored = repaired_restore(&mut source, bytes_checkpoint(&cp), 4);
    let history = source.host.persistence_history().unwrap();
    assert_eq!(history[0].participants[0].schema, [1; 32]);
    assert_eq!(history[1].participants[0].schema, [4; 32]);
    assert_eq!(restored[0].stamp().unwrap().revision, 2);
    let _batch = publish(&source, restored[0].clone(), 1);
    let mut capture = source.host.capture_state().unwrap();
    capture.owner(&restored[0], &NativeCodec(4)).unwrap();
    let cp = bytes_checkpoint(&capture.finish().unwrap());
    let restored = repaired_restore(&mut fresh, bytes_checkpoint(&cp), 4);
    assert_eq!(restored[0].stamp().unwrap().revision, 3);
}

#[test]
fn repair_bounded_namespace_admission_with_sparse_maximum() {
    let source = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    let owner = native(&source, None, vec![]);
    let cp = capture_native(&source, &[owner]);
    let sparse = mutate(&cp, |v| {
        v["origins"].as_array_mut().unwrap().push(u64::MAX.into());
    });
    let mut fresh = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    assert_eq!(repaired_restore(&mut fresh, sparse, 1).len(), 1);
    let overflow = mutate(&cp, |v| {
        v["origins"] =
            serde_json::to_value((1..=Limits::default().owners as u64 + 1).collect::<Vec<_>>())
                .unwrap();
    });
    assert!(matches!(
        Restore::new(
            &fresh.kernel,
            overflow,
            Limits::default(),
            PublicationLimits::default(),
            &bindings(&fresh)
        ),
        Err(Error::Limit)
    ));
}
