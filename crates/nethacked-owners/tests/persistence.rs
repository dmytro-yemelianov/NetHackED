#![cfg(feature = "persistence")]
use nethacked_kernel::{
    Access, CallerView, Capability, CapabilityPermit, Kernel, ModuleHandle, OperationBinding,
    ProviderBinding, Resource,
};
use nethacked_owners::persistence::*;
use nethacked_owners::*;
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
include!("support/harness.rs");

const WIRE_LIMIT: usize = 1_048_576;
fn identity(schema: u8) -> CodecIdentity {
    let artifact = |name: &str| ArtifactIdentity {
        name: name.into(),
        version: u32::from(schema),
        artifact: [schema; 32],
    };
    CodecIdentity {
        schema: artifact("state"),
        codec: artifact("codec"),
        policy: artifact("pure"),
        schema_digest: [schema; 32],
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeState {
    amount: u64,
    capacity: u64,
    link: Option<StateReference>,
}
struct NativeCodec(u8);
struct NativeDomain;
impl Schema<NativeState> for NativeDomain {
    fn validate(&self, s: &NativeState) -> Result<()> {
        if s.amount > s.capacity {
            return Err(Error::Invariant);
        }
        Ok(())
    }
    fn encode(&self, s: &NativeState) -> Result<Vec<u8>> {
        serde_json::to_vec(s).map_err(|_| Error::Invalid)
    }
    fn query(&self, s: &NativeState, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        Ok(s.amount.to_le_bytes().to_vec())
    }
}
impl NativeBehavior<NativeState> for NativeDomain {
    fn prepare(
        &self,
        s: &NativeState,
        _: &str,
        req: &[u8],
        _: &[Observation],
    ) -> Result<Proposal<NativeState>> {
        Ok(Proposal {
            next: NativeState {
                amount: s.amount + u64::from(req[0]),
                capacity: s.capacity,
                link: s.link.clone(),
            },
            events: vec![req.to_vec()],
            reply: req.to_vec(),
        })
    }
}
impl PersistenceCodec<NativeState> for NativeCodec {
    fn identity(&self) -> CodecIdentity {
        identity(self.0)
    }
    fn encode_state(&self, s: &NativeState, limit: usize) -> Result<Vec<u8>> {
        let bytes = NativeDomain.encode(s)?;
        if bytes.len() > limit {
            return Err(Error::Limit);
        }
        Ok(bytes)
    }
    fn decode_state(&self, bytes: &[u8], limit: usize, remap: &Remap) -> Result<NativeState> {
        if bytes.len() > limit {
            return Err(Error::Limit);
        }
        let mut state: NativeState = serde_json::from_slice(bytes).map_err(|_| Error::Invalid)?;
        state.link = state
            .link
            .as_ref()
            .map(|r| remap.reference(r))
            .transpose()?;
        Ok(state)
    }
    fn validate_state(&self, s: &NativeState) -> Result<()> {
        NativeDomain.validate(s)
    }
    fn references(&self, s: &NativeState) -> Result<Vec<StateReference>> {
        Ok(s.link.iter().cloned().collect())
    }
    fn validate_configuration(
        &self,
        s: &NativeState,
        ports: &BTreeMap<String, Port>,
        policy: &QueryPolicy,
    ) -> Result<()> {
        let dependencies = s
            .link
            .iter()
            .filter(|r| !r.historical)
            .map(|r| Dependency {
                owner: r.owner,
                query: "query".into(),
                schema: r.schema,
            })
            .collect::<Vec<_>>();
        config(ports, policy, &dependencies)
    }
}
struct BlobDomain;
impl Schema<Vec<u8>> for BlobDomain {
    fn validate(&self, s: &Vec<u8>) -> Result<()> {
        if s.len() != 8 {
            Err(Error::Invariant)
        } else {
            Ok(())
        }
    }
    fn encode(&self, s: &Vec<u8>) -> Result<Vec<u8>> {
        Ok(s.clone())
    }
    fn query(&self, s: &Vec<u8>, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        Ok(s.clone())
    }
}
impl WasmCodec<Vec<u8>> for BlobDomain {
    fn input(&self, s: &Vec<u8>, _: &str, req: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        Ok([s.as_slice(), req].concat())
    }
    fn decode(&self, bytes: &[u8]) -> Result<Proposal<Vec<u8>>> {
        Ok(Proposal {
            next: bytes.to_vec(),
            events: vec![b"guest".to_vec()],
            reply: bytes.to_vec(),
        })
    }
}
impl PersistenceCodec<Vec<u8>> for BlobDomain {
    fn identity(&self) -> CodecIdentity {
        identity(2)
    }
    fn encode_state(&self, s: &Vec<u8>, limit: usize) -> Result<Vec<u8>> {
        if s.len() > limit {
            Err(Error::Limit)
        } else {
            Ok(s.clone())
        }
    }
    fn decode_state(&self, bytes: &[u8], limit: usize, _: &Remap) -> Result<Vec<u8>> {
        if bytes.len() > limit {
            Err(Error::Limit)
        } else {
            Ok(bytes.to_vec())
        }
    }
    fn validate_state(&self, s: &Vec<u8>) -> Result<()> {
        self.validate(s)
    }
    fn references(&self, _: &Vec<u8>) -> Result<Vec<StateReference>> {
        Ok(vec![])
    }
    fn validate_configuration(
        &self,
        _: &Vec<u8>,
        ports: &BTreeMap<String, Port>,
        policy: &QueryPolicy,
    ) -> Result<()> {
        config(ports, policy, &[])
    }
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
        identity(3)
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
        config(ports, policy, &[])
    }
}
fn config(
    ports: &BTreeMap<String, Port>,
    policy: &QueryPolicy,
    dependencies: &[Dependency],
) -> Result<()> {
    if ports.len() != 3 || *policy != QueryPolicy::Noncacheable {
        return Err(Error::Schema);
    }
    for (name, access, deps) in [
        ("query", Access::Read, &[][..]),
        ("prepare", Access::Write, dependencies),
        ("derived", Access::Read, dependencies),
    ] {
        let port = ports.get(name).ok_or(Error::Schema)?;
        if port.access != access || port.major != 1 || port.dependencies != deps {
            return Err(Error::Schema);
        }
    }
    Ok(())
}
struct Accept;
impl Accounting for Accept {
    fn validate(&self, _: &[ProposalView], _: &[Observation]) -> Result<()> {
        Ok(())
    }
}
fn publish<T: 'static>(h: &Harness, owner: Rc<Owner<T>>, delta: u8) -> Rc<RefCell<Option<Batch>>> {
    let staged = Rc::new(RefCell::new(None));
    let result = staged.clone();
    let host = h.host.clone();
    h.run(1, move |view| {
        let c = owner.prepare("prepare", &[delta], vec![], view).unwrap();
        let batch = host
            .batch(
                vec![c.erase()],
                vec![],
                &[owner.id()],
                vec![owner.id()],
                [9; 32],
                Rc::new(Accept),
                cap("prepare"),
                view,
            )
            .unwrap();
        batch.commit(view).unwrap();
        *result.borrow_mut() = Some(batch);
        Ok(vec![])
    });
    staged
}
fn bindings(h: &Harness) -> BTreeMap<[u8; 32], ModuleHandle> {
    BTreeMap::from([(h.kernel.artifact(h.module).unwrap(), h.module)])
}
fn fresh_restore(h: &Harness, checkpoint: Checkpoint) -> Restore {
    Restore::new(
        &h.kernel,
        checkpoint,
        Limits::default(),
        PublicationLimits::default(),
        &bindings(h),
    )
    .unwrap()
}
fn bytes_checkpoint(checkpoint: &Checkpoint) -> Checkpoint {
    Checkpoint::from_bytes(&checkpoint.to_bytes(WIRE_LIMIT).unwrap(), WIRE_LIMIT).unwrap()
}
fn mutate(checkpoint: &Checkpoint, f: impl FnOnce(&mut serde_json::Value)) -> Checkpoint {
    let mut value = serde_json::to_value(checkpoint).unwrap();
    f(&mut value);
    let changed: Checkpoint = serde_json::from_value(value).unwrap();
    bytes_checkpoint(&changed)
}
const GUEST: &str = "i32.const 32 local.get $p i64.load local.get $p i32.const 8 i32.add i32.load8_u i64.extend_i32_u i64.add i64.store i64.const 137438953480";

#[test]
fn bytes_reconstruct_independent_native_actual_wasm_and_private_fields() {
    let mut source = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    let native = source
        .host
        .enroll(
            &source.kernel,
            source.spec(1, vec![]),
            NativeState {
                amount: 2,
                capacity: 20,
                link: None,
            },
            Backend::Native(Rc::new(NativeDomain)),
        )
        .unwrap();
    let blob = source
        .host
        .enroll(
            &source.kernel,
            source.spec(2, vec![]),
            7u64.to_le_bytes().to_vec(),
            Backend::Wasm {
                codec: Rc::new(BlobDomain),
                slot: 0,
                binding: source.binding(),
            },
        )
        .unwrap();
    let field = source
        .host
        .enroll(
            &source.kernel,
            source.spec(3, vec![]),
            FieldState {
                private: [3, 4, 5, 6],
            },
            Backend::Native(Rc::new(FieldDomain)),
        )
        .unwrap();
    for name in ["query", "prepare", "derived"] {
        source.keys.insert(native.resource(name).unwrap());
        source.keys.insert(blob.resource(name).unwrap());
        source.keys.insert(field.resource(name).unwrap());
    }
    source.refresh();
    let used = publish(&source, native.clone(), 1);
    let guest_used = publish(&source, blob.clone(), 2);
    let native_origin = native.id();
    let blob_origin = blob.id();
    let field_origin = field.id();
    let mut capture = source.host.capture_state().unwrap();
    assert_eq!(native.inspect(), Err(Error::Busy));
    capture.owner(&native, &NativeCodec(1)).unwrap();
    capture.owner(&blob, &BlobDomain).unwrap();
    capture.owner(&field, &FieldDomain).unwrap();
    let checkpoint = bytes_checkpoint(&capture.finish().unwrap());
    let history = source.host.persistence_history().unwrap();
    assert_eq!(history.len(), 2);
    // Reconstruct with a separately registered kernel, new callbacks and codecs.
    let mut fresh = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    let mut restore = fresh_restore(&fresh, checkpoint);
    let n = restore
        .owner(
            &fresh.kernel,
            native_origin,
            fresh.module,
            &NativeCodec(1),
            || Ok(Backend::Native(Rc::new(NativeDomain))),
        )
        .unwrap();
    let b = restore
        .owner(
            &fresh.kernel,
            blob_origin,
            fresh.module,
            &BlobDomain,
            || {
                Ok(Backend::Wasm {
                    codec: Rc::new(BlobDomain),
                    slot: 0,
                    binding: fresh.binding(),
                })
            },
        )
        .unwrap();
    let f = restore
        .owner(
            &fresh.kernel,
            field_origin,
            fresh.module,
            &FieldDomain,
            || Ok(Backend::Native(Rc::new(FieldDomain))),
        )
        .unwrap();
    assert_eq!(n.inspect(), Err(Error::Busy));
    fresh.host = restore.finish().unwrap();
    assert_ne!(n.id(), native_origin);
    assert_eq!(n.stamp().unwrap().revision, 1);
    assert_eq!(b.stamp().unwrap().revision, 1);
    assert_eq!(fresh.host.persistence_history().unwrap(), history);
    fresh.admit(&n);
    fresh.admit(&b);
    fresh.admit(&f);
    fresh.refresh();
    let old_history = source.host.persistence_history().unwrap();
    let consumed = used.clone();
    source.run(1, move |view| {
        assert!(consumed.borrow().as_ref().unwrap().commit(view).is_err());
        Ok(vec![])
    });
    assert_eq!(source.host.persistence_history().unwrap(), old_history);
    let _a = publish(&fresh, n.clone(), 2);
    let _b = publish(&fresh, b.clone(), 4);
    let _c = publish(&fresh, f.clone(), 1);
    assert_eq!(b.inspect().unwrap(), 13u64.to_le_bytes());
    assert_eq!(f.inspect().unwrap(), [4, 4, 5, 6]);
    let queried = n.clone();
    fresh.run(0, move |view| {
        assert_eq!(
            queried.query("query", &[], &[], view).unwrap().bytes(),
            5u64.to_le_bytes()
        );
        Ok(vec![])
    });
    assert_eq!(fresh.host.persistence_history().unwrap().len(), 5);
    assert_eq!(
        native.inspect().unwrap(),
        NativeDomain
            .encode(&NativeState {
                amount: 3,
                capacity: 20,
                link: None
            })
            .unwrap()
    );
    assert_eq!(blob.inspect().unwrap(), 9u64.to_le_bytes());
    assert_eq!(field.inspect().unwrap(), [3, 4, 5, 6]);
    drop(guest_used);
    // Restored origins themselves survive another byte reconstruction.
    let mut again = fresh.host.capture_state().unwrap();
    again.owner(&n, &NativeCodec(1)).unwrap();
    again.owner(&b, &BlobDomain).unwrap();
    again.owner(&f, &FieldDomain).unwrap();
    let cp = bytes_checkpoint(&again.finish().unwrap());
    let third = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    let mut restore = fresh_restore(&third, cp);
    let _n = restore
        .owner(&third.kernel, n.id(), third.module, &NativeCodec(1), || {
            Ok(Backend::Native(Rc::new(NativeDomain)))
        })
        .unwrap();
    let _b = restore
        .owner(&third.kernel, b.id(), third.module, &BlobDomain, || {
            Ok(Backend::Wasm {
                codec: Rc::new(BlobDomain),
                slot: 0,
                binding: third.binding(),
            })
        })
        .unwrap();
    let _f = restore
        .owner(&third.kernel, f.id(), third.module, &FieldDomain, || {
            Ok(Backend::Native(Rc::new(FieldDomain)))
        })
        .unwrap();
    assert_eq!(
        restore.finish().unwrap().persistence_history().unwrap(),
        fresh.host.persistence_history().unwrap()
    );
}

fn native(
    h: &Harness,
    link: Option<StateReference>,
    dependencies: Vec<Dependency>,
) -> Rc<Owner<NativeState>> {
    h.host
        .enroll(
            &h.kernel,
            h.spec(1, dependencies),
            NativeState {
                amount: 2,
                capacity: 20,
                link,
            },
            Backend::Native(Rc::new(NativeDomain)),
        )
        .unwrap()
}
fn reference<T: 'static>(owner: &Owner<T>, historical: bool) -> StateReference {
    let stamp = owner.stamp().unwrap();
    StateReference {
        owner: stamp.owner,
        generation: stamp.generation,
        schema: stamp.schema,
        historical,
    }
}
fn capture_native(h: &Harness, owners: &[Rc<Owner<NativeState>>]) -> Checkpoint {
    let mut capture = h.host.capture_state().unwrap();
    for owner in owners {
        capture.owner(owner, &NativeCodec(1)).unwrap();
    }
    bytes_checkpoint(&capture.finish().unwrap())
}
#[test]
fn live_edges_dependencies_resources_and_disposed_origins_remap_without_id_reuse() {
    let mut source = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    let target = native(&source, None, vec![]);
    let tombstone = native(&source, None, vec![]);
    source.admit(&target);
    source.admit(&tombstone);
    source.refresh();
    let _used = publish(&source, tombstone.clone(), 1);
    let historical = reference(&tombstone, true);
    source.host.retire(tombstone.id()).unwrap();
    let linked = native(
        &source,
        Some(reference(&target, false)),
        vec![Dependency {
            owner: target.id(),
            query: "query".into(),
            schema: [1; 32],
        }],
    );
    let historical_owner = native(&source, Some(historical.clone()), vec![]);
    source.admit(&linked);
    source.admit(&historical_owner);
    source.refresh();
    let observation = Rc::new(RefCell::new(None));
    let captured = observation.clone();
    let queried = target.clone();
    source.run(0, move |view| {
        *captured.borrow_mut() = Some(queried.query("query", &[], &[], view).unwrap());
        Ok(vec![])
    });
    let cp = capture_native(
        &source,
        &[target.clone(), linked.clone(), historical_owner.clone()],
    );
    let old_resource = target.resource("query").unwrap();
    let mut fresh = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    let mut restore = fresh_restore(&fresh, cp.clone());
    let fresh_resource = restore.remap().resource(&old_resource, &cp).unwrap();
    let t = restore
        .owner(
            &fresh.kernel,
            target.id(),
            fresh.module,
            &NativeCodec(1),
            || Ok(Backend::Native(Rc::new(NativeDomain))),
        )
        .unwrap();
    let l = restore
        .owner(
            &fresh.kernel,
            linked.id(),
            fresh.module,
            &NativeCodec(1),
            || Ok(Backend::Native(Rc::new(NativeDomain))),
        )
        .unwrap();
    let h = restore
        .owner(
            &fresh.kernel,
            historical_owner.id(),
            fresh.module,
            &NativeCodec(1),
            || Ok(Backend::Native(Rc::new(NativeDomain))),
        )
        .unwrap();
    let retired = historical.owner;
    assert_eq!(restore.remap().owner(retired), Err(Error::Stale));
    assert_eq!(restore.remap().reference(&historical).unwrap(), historical);
    fresh.host = restore.finish().unwrap();
    assert_eq!(fresh_resource, t.resource("query").unwrap());
    let decoded: NativeState = serde_json::from_slice(&l.inspect().unwrap()).unwrap();
    assert_eq!(decoded.link.unwrap().owner, t.id());
    let decoded: NativeState = serde_json::from_slice(&h.inspect().unwrap()).unwrap();
    assert_eq!(decoded.link.unwrap(), historical);
    let registry = fresh.host.registry().unwrap();
    assert_eq!(registry.len(), 4);
    assert_eq!(registry[1].revision, 1);
    let next = native(&fresh, None, vec![]);
    assert!(next.id() > h.id());
    fresh.admit(&t);
    fresh.admit(&l);
    fresh.admit(&h);
    fresh.admit(&next);
    fresh.refresh();
    assert!(fresh
        .kernel
        .invoke_operation(
            &source.operation,
            &[0],
            Authority {
                access: Access::Write,
                scope: Scope::All
            }
        )
        .is_err());
    let linked_for_test = l.clone();
    let stale = observation.clone();
    fresh.run(1, move |view| {
        assert!(linked_for_test
            .prepare(
                "prepare",
                &[1],
                vec![stale.borrow().as_ref().unwrap().clone()],
                view
            )
            .is_err());
        Ok(vec![])
    });
    // Positive control uses a fresh observation and commits through real ports.
    let target_for_test = t.clone();
    let new_observation = Rc::new(RefCell::new(None));
    let observed = new_observation.clone();
    fresh.run(0, move |view| {
        *observed.borrow_mut() = Some(target_for_test.query("query", &[], &[], view).unwrap());
        Ok(vec![])
    });
    let link_for_test = l.clone();
    let host = fresh.host.clone();
    fresh.run(1, move |view| {
        let c = link_for_test
            .prepare(
                "prepare",
                &[1],
                vec![new_observation.borrow().as_ref().unwrap().clone()],
                view,
            )
            .unwrap();
        let batch = host
            .batch(
                vec![c.erase()],
                vec![],
                &[link_for_test.id()],
                vec![link_for_test.id()],
                [9; 32],
                Rc::new(Accept),
                cap("prepare"),
                view,
            )
            .unwrap();
        batch.commit(view).unwrap();
        Ok(vec![])
    });
    assert_eq!(l.stamp().unwrap().revision, 1);
    assert_eq!(source.host.persistence_history().unwrap().len(), 1);
    // Historical references and disposed origins also survive a second restart.
    let cp = capture_native(&fresh, &[t.clone(), l.clone(), h.clone(), next.clone()]);
    let third = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    let mut restore = fresh_restore(&third, cp);
    let mut retained = Vec::new();
    for old in [t.id(), l.id(), h.id(), next.id()] {
        retained.push(
            restore
                .owner(&third.kernel, old, third.module, &NativeCodec(1), || {
                    Ok(Backend::Native(Rc::new(NativeDomain)))
                })
                .unwrap(),
        );
    }
    let host = restore.finish().unwrap();
    let state: NativeState = serde_json::from_slice(&retained[2].inspect().unwrap()).unwrap();
    assert_eq!(state.link.unwrap(), historical);
    assert_eq!(
        host.persistence_history().unwrap(),
        fresh.host.persistence_history().unwrap()
    );
}

#[test]
fn exact_identity_and_structural_refusals_preserve_nonempty_destination_and_source() {
    let mut source = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    let owner = native(&source, None, vec![]);
    source.admit(&owner);
    source.refresh();
    let _used = publish(&source, owner.clone(), 1);
    let cp = capture_native(&source, std::slice::from_ref(&owner));
    let before = owner.inspect().unwrap();
    let history = source.host.persistence_history().unwrap();
    let mut fresh = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    let destination = native(&fresh, None, vec![]);
    fresh.admit(&destination);
    fresh.refresh();
    let dst = destination.inspect().unwrap();
    let dst_history = fresh.host.persistence_history().unwrap();
    let operation = fresh.operation.clone();
    for path in ["schema", "codec", "policy"] {
        for field in ["name", "version", "artifact"] {
            let changed = mutate(&cp, |value| {
                let id = &mut value["owners"][0]["codec"][path];
                match field {
                    "name" => id[field] = "different".into(),
                    "version" => id[field] = 2.into(),
                    _ => id[field][0] = 99.into(),
                }
            });
            let mut restore = fresh_restore(&fresh, changed);
            assert!(matches!(
                restore.owner(
                    &fresh.kernel,
                    owner.id(),
                    fresh.module,
                    &NativeCodec(1),
                    || Ok(Backend::Native(Rc::new(NativeDomain)))
                ),
                Err(Error::Schema)
            ));
        }
    }
    let malformed = [
        mutate(&cp, |v| {
            let row = v["owners"][0].clone();
            v["owners"].as_array_mut().unwrap().push(row);
        }),
        mutate(&cp, |v| v["next_owner"] = 1.into()),
        mutate(&cp, |v| v["next_candidate"] = 1.into()),
        mutate(&cp, |v| v["owners"][0]["schema"][0] = 99.into()),
        mutate(&cp, |v| v["owners"][0]["module_artifact"][0] = 99.into()),
        mutate(&cp, |v| v["owners"][0]["generation"] = 99.into()),
        mutate(&cp, |v| v["history"]["epoch"] = 99.into()),
        mutate(&cp, |v| v["history"]["bytes"] = 0.into()),
        mutate(&cp, |v| v["history"]["consumed"] = serde_json::json!([])),
        mutate(&cp, |v| {
            v["history"]["records"][0]["revisions"][0] = 99.into()
        }),
        mutate(&cp, |v| {
            v["history"]["records"][0]["event_counts"][0] = 99.into()
        }),
        mutate(&cp, |v| v["history"]["records"][0]["reply"][0] = 99.into()),
        mutate(&cp, |v| v["history"]["records"][0]["reply"][16] = 99.into()),
        mutate(&cp, |v| {
            v["history"]["records"][0]["origin_host"] = 999.into()
        }),
        mutate(
            &cp,
            |v| v["owners"][0]["references"] = serde_json::json!([{"owner":{"host":999,"serial":1},"generation":1,"schema":vec![1;32],"historical":false}]),
        ),
    ];
    for changed in malformed {
        assert!(Restore::new(
            &fresh.kernel,
            changed,
            Limits::default(),
            PublicationLimits::default(),
            &bindings(&fresh)
        )
        .is_err());
    }
    for changed in [
        mutate(&cp, |v| {
            v["owners"][0]["ports"]
                .as_object_mut()
                .unwrap()
                .remove("query")
                .map(|_| ())
                .unwrap()
        }),
        mutate(
            &cp,
            |v| v["owners"][0]["policy"] = serde_json::json!({"Deterministic":{"profile":vec![1;32],"input_policy":vec![2;32]}}),
        ),
    ] {
        let mut restore = fresh_restore(&fresh, changed);
        assert!(matches!(
            restore.owner(
                &fresh.kernel,
                owner.id(),
                fresh.module,
                &NativeCodec(1),
                || Ok(Backend::Native(Rc::new(NativeDomain)))
            ),
            Err(Error::Schema)
        ));
    }
    let mut restore = fresh_restore(&fresh, cp);
    let hidden = restore
        .owner(
            &fresh.kernel,
            owner.id(),
            fresh.module,
            &NativeCodec(1),
            || Ok(Backend::Native(Rc::new(NativeDomain))),
        )
        .unwrap();
    assert!(matches!(
        restore.owner(
            &fresh.kernel,
            owner.id(),
            fresh.module,
            &NativeCodec(1),
            || Ok(Backend::Native(Rc::new(NativeDomain)))
        ),
        Err(Error::Invalid)
    ));
    drop(restore);
    assert_eq!(hidden.inspect(), Err(Error::Quarantined));
    assert_eq!(owner.inspect().unwrap(), before);
    assert_eq!(source.host.persistence_history().unwrap(), history);
    assert_eq!(destination.inspect().unwrap(), dst);
    assert_eq!(fresh.host.persistence_history().unwrap(), dst_history);
    assert_eq!(fresh.operation, operation);
    let _proof = publish(&fresh, destination.clone(), 1);
    assert_eq!(destination.stamp().unwrap().revision, 1);
}

#[test]
fn local_bytes_bounds_corruption_and_live_edge_loss_refuse() {
    let source = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    let target = native(&source, None, vec![]);
    let link = native(
        &source,
        Some(reference(&target, false)),
        vec![Dependency {
            owner: target.id(),
            query: "query".into(),
            schema: [1; 32],
        }],
    );
    let cp = capture_native(&source, &[target.clone(), link.clone()]);
    let bytes = cp.to_bytes(WIRE_LIMIT).unwrap();
    assert!(matches!(
        Checkpoint::from_bytes(&bytes, bytes.len() - 1),
        Err(Error::Limit)
    ));
    assert!(matches!(
        Checkpoint::from_bytes(&bytes[..bytes.len() - 1], WIRE_LIMIT),
        Err(Error::Invalid)
    ));
    let mut trailing = bytes.clone();
    trailing.push(b' ');
    assert!(Checkpoint::from_bytes(&trailing, WIRE_LIMIT).is_err());
    let fresh = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    for changed in [
        mutate(&cp, |v| {
            v["owners"][1]["references"][0]["generation"] = 99.into()
        }),
        mutate(&cp, |v| {
            v["owners"]
                .as_array_mut()
                .unwrap()
                .remove(0)
                .as_object()
                .map(|_| ())
                .unwrap()
        }),
    ] {
        assert!(Restore::new(
            &fresh.kernel,
            changed,
            Limits::default(),
            PublicationLimits::default(),
            &bindings(&fresh)
        )
        .is_err());
    }
    source.host.retire(target.id()).unwrap();
    let mut capture = source.host.capture_state().unwrap();
    assert_eq!(capture.owner(&link, &NativeCodec(1)), Err(Error::Stale));
}

#[test]
fn migration_requires_exact_old_semantic_validation_before_transform_and_valid_target() {
    let source = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    let owner = native(&source, None, vec![]);
    let cp = capture_native(&source, std::slice::from_ref(&owner));
    let mut fresh = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    let calls = Rc::new(Cell::new(0));
    let malformed = mutate(&cp, |v| {
        let mut s: serde_json::Value = serde_json::from_slice(
            &serde_json::from_value::<Vec<u8>>(v["owners"][0]["payload"].clone()).unwrap(),
        )
        .unwrap();
        s["amount"] = 21.into();
        v["owners"][0]["payload"] = serde_json::to_value(serde_json::to_vec(&s).unwrap()).unwrap();
    });
    let mut restore = fresh_restore(&fresh, malformed);
    let count = calls.clone();
    assert!(matches!(
        restore.migrate(
            &fresh.kernel,
            owner.id(),
            fresh.module,
            &NativeCodec(1),
            &NativeCodec(4),
            move |s, _| {
                count.set(count.get() + 1);
                Ok(s)
            },
            || Ok(Backend::Native(Rc::new(NativeDomain)))
        ),
        Err(Error::Invariant)
    ));
    assert_eq!(calls.get(), 0);
    // A migrator cannot stand in for a missing exact old validator.
    let mut restore = fresh_restore(&fresh, cp.clone());
    let count = calls.clone();
    assert!(matches!(
        restore.migrate(
            &fresh.kernel,
            owner.id(),
            fresh.module,
            &NativeCodec(2),
            &NativeCodec(4),
            move |s, _| {
                count.set(count.get() + 1);
                Ok(s)
            },
            || Ok(Backend::Native(Rc::new(NativeDomain)))
        ),
        Err(Error::Schema)
    ));
    assert_eq!(calls.get(), 0);
    let mut restore = fresh_restore(&fresh, cp.clone());
    assert!(matches!(
        restore.migrate(
            &fresh.kernel,
            owner.id(),
            fresh.module,
            &NativeCodec(1),
            &NativeCodec(4),
            |mut s, _| {
                s.amount = 21;
                Ok(s)
            },
            || Ok(Backend::Native(Rc::new(NativeDomain)))
        ),
        Err(Error::Invariant)
    ));
    let mut restore = fresh_restore(&fresh, cp);
    let count = calls.clone();
    let migrated = restore
        .migrate(
            &fresh.kernel,
            owner.id(),
            fresh.module,
            &NativeCodec(1),
            &NativeCodec(4),
            move |mut s, _| {
                count.set(count.get() + 1);
                s.capacity = 30;
                Ok(s)
            },
            || Ok(Backend::Native(Rc::new(NativeDomain))),
        )
        .unwrap();
    fresh.host = restore.finish().unwrap();
    fresh.admit(&migrated);
    fresh.refresh();
    assert_eq!(calls.get(), 1);
    assert_eq!(migrated.stamp().unwrap().schema, [4; 32]);
    let _used = publish(&fresh, migrated.clone(), 3);
    let query = migrated.clone();
    fresh.run(0, move |view| {
        assert_eq!(
            query.query("query", &[], &[], view).unwrap().bytes(),
            5u64.to_le_bytes()
        );
        Ok(vec![])
    });
    assert_eq!(owner.stamp().unwrap().revision, 0);
}

#[test]
fn incomplete_duplicate_pending_and_changed_backend_refuse_without_visibility() {
    let mut source = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    let a = native(&source, None, vec![]);
    let b = native(&source, None, vec![]);
    let mut capture = source.host.capture_state().unwrap();
    capture.owner(&a, &NativeCodec(1)).unwrap();
    assert_eq!(capture.owner(&a, &NativeCodec(1)), Err(Error::Invalid));
    assert!(matches!(capture.finish(), Err(Error::Unsupported)));
    let cp = capture_native(&source, &[a.clone(), b.clone()]);
    let fresh = Harness::new(Limits::default(), KernelLimits::default(), GUEST);
    let mut restore = fresh_restore(&fresh, cp.clone());
    let escaped = restore
        .owner(&fresh.kernel, a.id(), fresh.module, &NativeCodec(1), || {
            Ok(Backend::Native(Rc::new(NativeDomain)))
        })
        .unwrap();
    assert!(matches!(restore.finish(), Err(Error::Unsupported)));
    assert_eq!(escaped.inspect(), Err(Error::Quarantined));
    let mut restore = fresh_restore(&fresh, cp);
    assert!(restore
        .owner(&fresh.kernel, a.id(), fresh.module, &NativeCodec(1), || Ok(
            Backend::CommitCallback
        ))
        .is_err());
    source.admit(&a);
    source.admit(&b);
    source.refresh();
    let pending = Rc::new(RefCell::new(None));
    let kept = pending.clone();
    let owner = a.clone();
    source.run(1, move |view| {
        *kept.borrow_mut() = Some(owner.prepare("prepare", &[1], vec![], view).unwrap());
        Ok(vec![])
    });
    assert!(matches!(
        source.host.capture_state(),
        Err(Error::Unsupported)
    ));
    pending.borrow_mut().take();
    assert_eq!(source.host.pending().unwrap(), 0);
    assert_eq!(a.stamp().unwrap().revision, 0);
}

include!("persistence_repair.inc");
