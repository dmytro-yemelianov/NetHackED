//! Optional owned input source and immutable recorded choices.
#![forbid(unsafe_code)]
use nethacked_kernel::{
    Access, CallerView, Capability, CapabilityPermit, Endpoint, Error, Kernel, Manifest,
    ModuleHandle, NativePort, OperationBinding, OperationId, Resource, Result,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_SERVICE: AtomicU64 = AtomicU64::new(1);
pub const ENDPOINT: &str = "input";
pub const MAJOR: u32 = 1;

#[derive(Clone, Debug, Serialize)]
pub struct Limits {
    pub source: usize,
    pub request: usize,
    pub value: usize,
    pub records: usize,
    pub operations: usize,
    pub wire: u32,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            source: 1024,
            request: 1024,
            value: 1024,
            records: 1024,
            operations: 32,
            wire: 16384,
        }
    }
}
/// Trusted staging code: no external authoritative mutation, destructive I/O or panic.
pub type Transition = Rc<dyn Fn(&[u8], &[u8]) -> Result<(Vec<u8>, Vec<u8>)>>;
#[derive(Clone)]
pub struct Policy {
    pub identity: String,
    pub version: u32,
    pub transition: Transition,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordId {
    pub namespace: u64,
    pub serial: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Sample { request: Vec<u8> },
    Replay { id: RecordId, request: Vec<u8> },
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub id: RecordId,
    pub value: Vec<u8>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    pub id: RecordId,
    pub ordinal: u64,
    pub operation: OperationBinding,
    pub caller: ModuleHandle,
    pub caller_artifact: [u8; 32],
    pub provider: ModuleHandle,
    pub provider_artifact: [u8; 32],
    pub permit: CapabilityPermit,
    pub request: Vec<u8>,
    pub value: Vec<u8>,
}
/// Detached evidence; editing this clone cannot modify service state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct State {
    pub source: Vec<u8>,
    pub records: Vec<Record>,
    pub next_serial: u64,
    pub next_ordinals: BTreeMap<OperationId, u64>,
}
struct Inner {
    namespace: u64,
    limits: Limits,
    policy: Policy,
    binding: Option<(ModuleHandle, [u8; 32], Capability, Resource)>,
    state: State,
}
#[derive(Clone)]
pub struct Service(Rc<RefCell<Inner>>);
/// Immutable host-issued observation of an accepted record. Neither wire IDs
/// nor Reply.value are credentials. This owns code and service lifetime only.
#[derive(Clone)]
pub struct Observation {
    service: Service,
    identity: ObservationIdentity,
    record: Record,
    _code: nethacked_kernel::ModulePin,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservationIdentity {
    pub policy: String,
    pub version: u32,
    pub record: RecordId,
    pub provider: nethacked_kernel::ProviderBinding,
    pub request: Vec<u8>,
    pub operation: OperationBinding,
    pub caller: ModuleHandle,
    pub caller_artifact: [u8; 32],
    pub permit: CapabilityPermit,
}
impl Observation {
    pub fn identity(&self) -> &ObservationIdentity {
        &self.identity
    }
    pub fn bytes(&self) -> &[u8] {
        &self.record.value
    }
    /// Fresh authority and exact legacy replay scope, without sampling or policy
    /// callbacks. Called before pure computation and at publication preflight.
    pub fn validate(&self, view: &CallerView<'_>) -> Result<()> {
        let inner = self
            .service
            .0
            .try_borrow()
            .map_err(|_| Error::NativeRefusal)?;
        validate_record(&inner, &self.record, view)?;
        Ok(())
    }
}
fn validate_record(inner: &Inner, r: &Record, view: &CallerView<'_>) -> Result<()> {
    let operation = view.operation().ok_or(Error::Authority)?;
    let (provider, artifact, capability, resource) =
        inner.binding.as_ref().ok_or(Error::Authority)?;
    let permit = view
        .permit(capability, resource, Access::Write)
        .ok_or(Error::Authority)?;
    if view.caller() != Some(r.caller)
        || view.caller_artifact() != Some(r.caller_artifact)
        || r.provider != *provider
        || r.provider_artifact != *artifact
        || r.permit != permit
        || r.operation.operation() != operation.operation()
        || r.operation.pipeline() != operation.pipeline()
        || operation.checkpoint() < r.operation.checkpoint()
        || (operation.checkpoint() == r.operation.checkpoint()
            && operation.phase() != r.operation.phase())
        || !inner.state.records.iter().any(|stored| stored == r)
    {
        return Err(Error::Authority);
    }
    // Detached metadata clones cannot issue or validate usable observations.
    view.pin_invocation()?;
    Ok(())
}
impl Service {
    /// An ID is only a lookup key. Authenticated caller/operation/permit and exact
    /// request select the accepted record; no guest value is used here.
    pub fn observation(
        &self,
        id: RecordId,
        request: &[u8],
        view: &CallerView<'_>,
    ) -> Result<Observation> {
        let inner = self.0.try_borrow().map_err(|_| Error::NativeRefusal)?;
        let record = inner
            .state
            .records
            .iter()
            .find(|r| r.id == id && r.request == request)
            .ok_or(Error::Authority)?;
        validate_record(&inner, record, view)?;
        let identity = ObservationIdentity {
            policy: inner.policy.identity.clone(),
            version: inner.policy.version,
            record: record.id,
            provider: nethacked_kernel::ProviderBinding {
                handle: record.provider,
                artifact: record.provider_artifact,
                endpoint: ENDPOINT.into(),
            },
            request: record.request.clone(),
            operation: record.operation.clone(),
            caller: record.caller,
            caller_artifact: record.caller_artifact,
            permit: record.permit.clone(),
        };
        Ok(Observation {
            service: self.clone(),
            identity,
            record: record.clone(),
            _code: view.pin_invocation()?,
        })
    }
    pub fn new(limits: Limits, policy: Policy, source: Vec<u8>) -> Result<Self> {
        if source.len() > limits.source
            || policy.identity.is_empty()
            || policy.identity.len() > limits.wire as usize
            || limits.wire == 0
        {
            return Err(Error::Buffer);
        }
        let namespace = NEXT_SERVICE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| Error::Limit)?;
        Ok(Self(Rc::new(RefCell::new(Inner {
            namespace,
            limits,
            policy,
            binding: None,
            state: State {
                source,
                records: vec![],
                next_serial: 1,
                next_ordinals: BTreeMap::new(),
            },
        }))))
    }
    /// One immutable policy and one installed incarnation; replacement uses a new service.
    pub fn install(&self, kernel: &mut Kernel, id: &str) -> Result<ModuleHandle> {
        let mut inner = self.0.try_borrow_mut().map_err(|_| Error::NativeRefusal)?;
        if inner.binding.is_some() {
            return Err(Error::Duplicate);
        }
        let identity = encode(&(
            "nethacked-inputs-v1",
            env!("CARGO_PKG_VERSION"),
            &inner.policy.identity,
            inner.policy.version,
            &inner.limits,
        ))?;
        let service = self.clone();
        let port: NativePort = Rc::new(move |b, v| service.dispatch(b, v));
        let handle = kernel.register_native(
            Manifest {
                id: id.into(),
                exports: BTreeMap::from([(
                    ENDPOINT.into(),
                    Endpoint {
                        major: MAJOR,
                        access: Access::Write,
                        max_request: inner.limits.wire,
                        max_reply: inner.limits.wire,
                    },
                )]),
                dependencies: vec![],
                extension_points: BTreeSet::new(),
                extension: None,
            },
            &identity,
            BTreeMap::from([(ENDPOINT.into(), port)]),
        )?;
        inner.binding = Some((
            handle,
            kernel.artifact(handle)?,
            Capability {
                provider: id.into(),
                endpoint: ENDPOINT.into(),
                major: MAJOR,
            },
            Resource {
                owner: id.into(),
                key: "source".into(),
            },
        ));
        Ok(handle)
    }
    /// Both actions require declared write permission for `{installed_id}/source`.
    pub fn inspect(&self) -> Result<State> {
        Ok(self
            .0
            .try_borrow()
            .map_err(|_| Error::NativeRefusal)?
            .state
            .clone())
    }
    fn dispatch(&self, bytes: &[u8], view: &CallerView) -> Result<Vec<u8>> {
        // The sole fallible borrow covers staging and publication, including callbacks.
        let mut inner = self.0.try_borrow_mut().map_err(|_| Error::NativeRefusal)?;
        if bytes.len() > inner.limits.wire as usize {
            return Err(Error::Buffer);
        }
        let req: Request = serde_json::from_slice(bytes).map_err(|_| Error::NativeRefusal)?;
        let request = match &req {
            Request::Sample { request } | Request::Replay { request, .. } => request,
        };
        if request.len() > inner.limits.request {
            return Err(Error::Buffer);
        }
        let operation = view.operation().ok_or(Error::Authority)?;
        let caller = view.caller().ok_or(Error::Authority)?;
        let caller_artifact = view.caller_artifact().ok_or(Error::Authority)?;
        let (provider, artifact, capability, resource) =
            inner.binding.as_ref().ok_or(Error::Authority)?;
        if (*provider, *artifact) != (view.provider(), view.provider_artifact()) {
            return Err(Error::Authority);
        }
        let permit = view
            .permit(capability, resource, Access::Write)
            .ok_or(Error::Authority)?;
        let first = permit.resolution.first().ok_or(Error::Authority)?;
        if first.handle != *provider || first.artifact != *artifact {
            return Err(Error::Authority);
        }
        let capacity = inner.limits.wire.min(view.max_reply()) as usize;
        match req {
            Request::Replay { id, request } => {
                if id.namespace != inner.namespace {
                    return Err(Error::NativeRefusal);
                }
                let r = inner
                    .state
                    .records
                    .iter()
                    .find(|r| r.id == id)
                    .ok_or(Error::NativeRefusal)?;
                if r.request != request
                    || r.caller != caller
                    || r.caller_artifact != caller_artifact
                    || r.provider != *provider
                    || r.provider_artifact != *artifact
                    || r.permit != permit
                    || r.operation.operation() != operation.operation()
                    || r.operation.pipeline() != operation.pipeline()
                    || operation.checkpoint() < r.operation.checkpoint()
                    || (operation.checkpoint() == r.operation.checkpoint()
                        && operation.phase() != r.operation.phase())
                {
                    return Err(Error::Authority);
                }
                reply(
                    &Reply {
                        id,
                        value: r.value.clone(),
                    },
                    capacity,
                )
            }
            Request::Sample { request } => {
                let op = operation.operation();
                if inner.state.records.len() >= inner.limits.records
                    || (!inner.state.next_ordinals.contains_key(&op)
                        && inner.state.next_ordinals.len() >= inner.limits.operations)
                {
                    return Err(Error::Limit);
                }
                let serial = inner.state.next_serial;
                let next_serial = serial.checked_add(1).ok_or(Error::Limit)?;
                let ordinal = inner.state.next_ordinals.get(&op).copied().unwrap_or(0);
                let next_ordinal = ordinal.checked_add(1).ok_or(Error::Limit)?;
                // Exact upper bound: u8 JSON uses at most three digits plus a comma.
                // Serialize the actual ID with an empty value, then checked growth.
                let id = RecordId {
                    namespace: inner.namespace,
                    serial,
                };
                let base = encode(&Reply { id, value: vec![] })?.len();
                let growth = inner.limits.value.checked_mul(4).ok_or(Error::Buffer)?;
                let maximum = base
                    .checked_add(growth.saturating_sub(usize::from(inner.limits.value > 0)))
                    .ok_or(Error::Buffer)?;
                if maximum > capacity {
                    return Err(Error::Buffer);
                }
                let (source, value) = (inner.policy.transition)(&inner.state.source, &request)?;
                if source.len() > inner.limits.source || value.len() > inner.limits.value {
                    return Err(Error::Buffer);
                }
                let output = reply(
                    &Reply {
                        id,
                        value: value.clone(),
                    },
                    capacity,
                )?;
                let record = Record {
                    id,
                    ordinal,
                    operation: operation.clone(),
                    caller,
                    caller_artifact,
                    provider: *provider,
                    provider_artifact: *artifact,
                    permit,
                    request,
                    value,
                };
                let mut staged = inner.state.clone();
                staged.source = source;
                staged.records.push(record);
                staged.next_serial = next_serial;
                staged.next_ordinals.insert(op, next_ordinal);
                inner.state = staged;
                Ok(output)
            }
        }
    }
}
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(&serde_json::to_value(value).map_err(|_| Error::NativeRefusal)?)
        .map_err(|_| Error::NativeRefusal)
}
fn reply<T: Serialize>(value: &T, capacity: usize) -> Result<Vec<u8>> {
    let output = encode(value)?;
    if output.len() > capacity {
        Err(Error::Buffer)
    } else {
        Ok(output)
    }
}

#[cfg(test)]
extern crate self as nethacked_inputs;
#[cfg(test)]
#[path = "../tests/support/mod.rs"]
mod support;
#[cfg(test)]
mod tests {
    use super::*;
    use support::*;
    #[test]
    fn checked_serial_and_ordinal_overflow_precede_policy() {
        for serial in [true, false] {
            let h = Host::new();
            let b = h.binding(0, "select");
            let saved = h.call(&b, &sample(b"ok"));
            let original = h.s.inspect().unwrap();
            {
                let mut i = h.s.0.borrow_mut();
                if serial {
                    i.state.next_serial = u64::MAX;
                } else {
                    i.state.next_ordinals.insert(h.operation, u64::MAX);
                }
            }
            h.refuse(&b, &sample(b"overflow"), authority());
            assert_eq!(h.call(&b, &replay(&saved, b"ok")), saved);
            // Restore only the artificial counter to demonstrate an authorized control.
            {
                let mut i = h.s.0.borrow_mut();
                i.state.next_serial = original.next_serial;
                i.state.next_ordinals = original.next_ordinals;
            }
            assert_eq!(h.call(&b, &sample(b"ok")).value, [20]);
        }
    }
    #[test]
    fn serializer_error_and_complete_reply_size_refuse() {
        struct Broken;
        impl Serialize for Broken {
            fn serialize<S: serde::Serializer>(
                &self,
                _: S,
            ) -> std::result::Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("test serialization rejection"))
            }
        }
        let h = Host::new();
        let b = h.binding(0, "select");
        let r = h.call(&b, &sample(b"ok"));
        let before = h.s.inspect().unwrap();
        let calls = h.calls.get();
        assert_eq!(reply(&Broken, 16384), Err(Error::NativeRefusal));
        let exact = encode(&r).unwrap();
        assert_eq!(reply(&r, exact.len() - 1), Err(Error::Buffer));
        assert_eq!(reply(&r, exact.len()).unwrap(), exact);
        assert_eq!(h.s.inspect().unwrap(), before);
        assert_eq!(h.calls.get(), calls);
        assert_eq!(h.call(&b, &replay(&r, b"ok")), r);
    }
    #[test]
    fn other_provider_and_host_ports_cannot_consume_a_live_record() {
        let mut h = Host::new();
        // A real installed alias delegates to the SAME service. Its current input
        // grant is valid, so namespace/record/permission cannot mask provider refusal.
        let s = h.s.clone();
        let port: NativePort = Rc::new(move |bytes, view| {
            assert!(view.operation().is_some());
            assert!(view.permit(&cap(), &resource(), Access::Write).is_some());
            let Request::Replay { id, request } = serde_json::from_slice(bytes).unwrap() else {
                panic!("expected live replay")
            };
            assert!(s
                .inspect()
                .unwrap()
                .records
                .iter()
                .any(|r| r.id == id && r.request == request));
            s.dispatch(bytes, view)
        });
        let mut m = manifest("alias");
        m.dependencies.clear();
        h.k.register_native(
            m.clone(),
            b"alias-v1",
            BTreeMap::from([("step".into(), port.clone())]),
        )
        .unwrap();
        let mut root = manifest("multiplex");
        root.dependencies.push(nethacked_kernel::Dependency {
            capability: Capability {
                provider: "alias".into(),
                endpoint: "step".into(),
                major: 1,
            },
            access: Access::Write,
        });
        let g = h.k.register_guest(root, &forward("")).unwrap();
        h.k.issue_grant(g, 0, authority()).unwrap();
        h.k.issue_grant(g, 1, authority()).unwrap();
        let (op, lease) = h.k.capture_operation(g, "step", authority()).unwrap();
        let binding = h.k.operation_binding(op, lease, 0, "select").unwrap();
        let saved = h.call(&binding, &sample(b"ok"));
        let request = replay(&saved, b"ok");
        let bytes = serde_json::to_vec(&request).unwrap();
        let mut aliased = vec![b' '];
        aliased.extend(&bytes);
        let before = h.s.inspect().unwrap();
        let calls = h.calls.get();
        assert_eq!(
            i32::from_le_bytes(h.raw(&binding, &aliased, authority()).try_into().unwrap()),
            Error::Authority.abi_code()
        );
        assert_eq!(h.s.inspect().unwrap(), before);
        assert_eq!(h.calls.get(), calls);
        assert_eq!(h.call(&binding, &request), saved);
        // The same live ledger exposed to another host still refuses its authentic views.
        let mut other = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
        other
            .register_native(m, b"alias-v1", BTreeMap::from([("step".into(), port)]))
            .unwrap();
        // A loaded original-capability provider gives this host a valid current
        // input permit as well, while the alias attempts to consume the live ID.
        let s = Service::new(
            Limits::default(),
            Policy {
                identity: "other-host".into(),
                version: 1,
                transition: Rc::new(|s, r| Ok((s.to_vec(), r.to_vec()))),
            },
            vec![],
        )
        .unwrap();
        s.install(&mut other, "inputs").unwrap();
        let mut thief = manifest("thief");
        thief.dependencies.push(nethacked_kernel::Dependency {
            capability: Capability {
                provider: "alias".into(),
                endpoint: "step".into(),
                major: 1,
            },
            access: Access::Write,
        });
        let tg = other.register_guest(thief, &forward("")).unwrap();
        other.issue_grant(tg, 0, authority()).unwrap();
        other.issue_grant(tg, 1, authority()).unwrap();
        let (tid, tl) = other.capture_operation(tg, "step", authority()).unwrap();
        let tb = other.operation_binding(tid, tl, 0, "select").unwrap();
        let local: Reply = serde_json::from_slice(
            &other
                .invoke_operation(
                    &tb,
                    &serde_json::to_vec(&sample(b"local")).unwrap(),
                    authority(),
                )
                .unwrap(),
        )
        .unwrap();
        let out = other.invoke_operation(&tb, &aliased, authority()).unwrap();
        assert_eq!(
            i32::from_le_bytes(out.try_into().unwrap()),
            Error::Authority.abi_code()
        );
        assert_eq!(h.s.inspect().unwrap(), before);
        assert_eq!(h.calls.get(), calls);
        assert_eq!(h.call(&binding, &request), saved);
        assert_eq!(
            serde_json::from_slice::<Reply>(
                &other
                    .invoke_operation(
                        &tb,
                        &serde_json::to_vec(&replay(&local, b"local")).unwrap(),
                        authority()
                    )
                    .unwrap()
            )
            .unwrap(),
            local
        );
    }
}
