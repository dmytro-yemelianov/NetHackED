//! Optional, in-memory, serialized resource transactions over real kernel ports.
#![forbid(unsafe_code)]
use nethacked_kernel::{
    Access, CallerView, Capability, CapabilityPermit, Endpoint, Error, Kernel, Manifest,
    ModuleHandle, NativePort, Resource, Result,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_SERVICE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Serialize)]
pub struct Limits {
    pub owners: usize,
    pub resources: usize,
    pub participants: usize,
    pub pending: usize,
    pub events: usize,
    pub history: usize,
    pub payload: usize,
    pub wire: u32,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            owners: 32,
            resources: 256,
            participants: 32,
            pending: 32,
            events: 32,
            history: 1024,
            payload: 1024,
            wire: 16 * 1024,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stamp {
    pub address: Resource,
    pub generation: u64,
    pub revision: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Write {
    pub stamp: Stamp,
    pub replacement: Option<Vec<u8>>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub readonly: Vec<Stamp>,
    pub writes: Vec<Write>,
    pub events: Vec<Vec<u8>>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub service: u64,
    pub serial: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub stamp: Stamp,
    pub value: Vec<u8>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadRequest {
    owner: String,
    key: String,
}
/// Only explicitly declared values and proposed changes reach trusted policies.
#[derive(Clone, Debug)]
pub struct Declared {
    pub snapshots: Vec<Snapshot>,
    pub plan: Plan,
}
pub type Validator = Rc<dyn Fn(&Declared) -> bool>;
#[derive(Clone)]
pub struct OwnerPolicy {
    pub capability: Capability,
    pub provider: ModuleHandle,
    pub artifact: [u8; 32],
    pub validate: Validator,
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct Prepared {
    caller: ModuleHandle,
    artifact: [u8; 32],
    provider: ModuleHandle,
    provider_artifact: [u8; 32],
    plan: Plan,
    permits: Vec<CapabilityPermit>,
}
/// Exact data-state evidence, including counters and immutable pending plans.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct State {
    pub resources: BTreeMap<Resource, Snapshot>,
    pub history: Vec<Vec<u8>>,
    pending: BTreeMap<u64, Prepared>,
    pub next_serial: u64,
    pub next_generation: u64,
    pub commits: u64,
}
struct Inner {
    namespace: u64,
    limits: Limits,
    binding: Option<(ModuleHandle, [u8; 32])>,
    owners: BTreeMap<String, OwnerPolicy>,
    accounting: Validator,
    state: State,
}
#[derive(Clone)]
pub struct Service(Rc<RefCell<Inner>>);
impl Service {
    pub fn new(limits: Limits, accounting: Validator) -> Result<Self> {
        if limits.owners == 0
            || limits.resources == 0
            || limits.participants == 0
            || limits.pending == 0
            || limits.payload == 0
            || limits.wire == 0
            || limits.wire > i32::MAX as u32
        {
            return Err(Error::Limit);
        }
        let namespace = NEXT_SERVICE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| Error::Limit)?;
        Ok(Self(Rc::new(RefCell::new(Inner {
            namespace,
            limits,
            binding: None,
            owners: BTreeMap::new(),
            accounting,
            state: State {
                resources: BTreeMap::new(),
                history: vec![],
                pending: BTreeMap::new(),
                next_serial: 1,
                next_generation: 1,
                commits: 0,
            },
        }))))
    }
    /// Trusted host installation. One service is bound to one exact incarnation.
    pub fn install(&self, kernel: &mut Kernel, id: &str) -> Result<ModuleHandle> {
        let mut inner = self.0.try_borrow_mut().map_err(|_| Error::NativeRefusal)?;
        if inner.binding.is_some() {
            return Err(Error::Duplicate);
        }
        let mut exports = BTreeMap::new();
        let mut ports = BTreeMap::new();
        for (name, access) in [
            ("read", Access::Read),
            ("prepare", Access::Write),
            ("commit", Access::Write),
        ] {
            exports.insert(
                name.into(),
                Endpoint {
                    major: 1,
                    access,
                    max_request: inner.limits.wire,
                    max_reply: inner.limits.wire,
                },
            );
            let service = self.clone();
            let port: NativePort =
                Rc::new(move |request, view| service.dispatch(name, request, view));
            ports.insert(name.into(), port);
        }
        let native_identity = serde_json::to_vec(
            &serde_json::to_value((
                "nethacked-resources-ports-v1",
                env!("CARGO_PKG_VERSION"),
                &inner.limits,
            ))
            .map_err(|_| Error::NativeRefusal)?,
        )
        .map_err(|_| Error::NativeRefusal)?;
        let handle = kernel.register_native(
            Manifest {
                id: id.into(),
                exports,
                dependencies: vec![],
                extension_points: BTreeSet::new(),
                extension: None,
            },
            &native_identity,
            ports,
        )?;
        inner.binding = Some((handle, kernel.artifact(handle)?));
        Ok(handle)
    }
    pub fn register_owner(&self, owner: &str, policy: OwnerPolicy) -> Result<()> {
        let mut inner = self.0.try_borrow_mut().map_err(|_| Error::NativeRefusal)?;
        if !name(owner)
            || !name(&policy.capability.provider)
            || !name(&policy.capability.endpoint)
            || policy.capability.major == 0
        {
            return Err(Error::Authority);
        }
        if inner.owners.contains_key(owner) {
            return Err(Error::Duplicate);
        }
        if inner.owners.len() >= inner.limits.owners {
            return Err(Error::Limit);
        }
        inner.owners.insert(owner.into(), policy);
        Ok(())
    }
    /// Trusted initialization/update; deletion and recreation allocate a new generation.
    pub fn put(&self, address: Resource, value: Vec<u8>) -> Result<Stamp> {
        let mut inner = self.0.try_borrow_mut().map_err(|_| Error::NativeRefusal)?;
        inner.address(&address)?;
        if value.len() > inner.limits.payload {
            return Err(Error::Limit);
        }
        let (generation, revision, next_generation) = match inner.state.resources.get(&address) {
            Some(old) => (
                old.stamp.generation,
                old.stamp.revision.checked_add(1).ok_or(Error::Limit)?,
                inner.state.next_generation,
            ),
            None => {
                if inner.state.resources.len() >= inner.limits.resources {
                    return Err(Error::Limit);
                }
                (
                    inner.state.next_generation,
                    0,
                    inner
                        .state
                        .next_generation
                        .checked_add(1)
                        .ok_or(Error::Limit)?,
                )
            }
        };
        let stamp = Stamp {
            address: address.clone(),
            generation,
            revision,
        };
        inner.state.resources.insert(
            address,
            Snapshot {
                stamp: stamp.clone(),
                value,
            },
        );
        inner.state.next_generation = next_generation;
        Ok(stamp)
    }
    pub fn delete(&self, address: &Resource) -> Result<()> {
        let mut inner = self.0.try_borrow_mut().map_err(|_| Error::NativeRefusal)?;
        if !inner.state.resources.contains_key(address) {
            return Err(Error::NativeRefusal);
        }
        inner.state.resources.remove(address);
        Ok(())
    }
    pub fn state(&self) -> Result<State> {
        Ok(self
            .0
            .try_borrow()
            .map_err(|_| Error::NativeRefusal)?
            .state
            .clone())
    }
    fn dispatch(&self, endpoint: &str, bytes: &[u8], view: &CallerView) -> Result<Vec<u8>> {
        // Hold the sole borrow across validation, serialization and publication.
        // Every reentrant admin/port/inspection attempt refuses instead of panicking.
        let mut inner = self.0.try_borrow_mut().map_err(|_| Error::NativeRefusal)?;
        if inner.binding != Some((view.provider(), view.provider_artifact()))
            || view.caller().is_none()
            || view.caller_artifact().is_none()
        {
            return Err(Error::Authority);
        }
        if bytes.len() > inner.limits.wire as usize {
            return Err(Error::Buffer);
        }
        match endpoint {
            "read" => {
                let request: ReadRequest = decode(bytes)?;
                let address = Resource {
                    owner: request.owner,
                    key: request.key,
                };
                inner.permit(&address, Access::Read, view)?;
                let snapshot = inner
                    .state
                    .resources
                    .get(&address)
                    .ok_or(Error::NativeRefusal)?;
                inner.reply(snapshot, view)
            }
            "prepare" => {
                if view.authority().access != Access::Write {
                    return Err(Error::Authority);
                }
                let plan: Plan = decode(bytes)?;
                let permits = inner.validate(&plan, view)?;
                if inner.state.pending.len() >= inner.limits.pending {
                    return Err(Error::Limit);
                }
                let next = inner.state.next_serial.checked_add(1).ok_or(Error::Limit)?;
                let id = Identity {
                    service: inner.namespace,
                    serial: inner.state.next_serial,
                };
                let output = inner.reply(&id, view)?;
                let prepared = Prepared {
                    caller: view.caller().ok_or(Error::Authority)?,
                    artifact: view.caller_artifact().ok_or(Error::Authority)?,
                    provider: view.provider(),
                    provider_artifact: view.provider_artifact(),
                    plan,
                    permits,
                };
                inner.state.pending.insert(id.serial, prepared);
                inner.state.next_serial = next;
                Ok(output)
            }
            "commit" => {
                if view.authority().access != Access::Write {
                    return Err(Error::Authority);
                }
                let id: Identity = decode(bytes)?;
                if id.service != inner.namespace {
                    return Err(Error::Authority);
                }
                let prepared = inner
                    .state
                    .pending
                    .get(&id.serial)
                    .ok_or(Error::NativeRefusal)?;
                if Some(prepared.caller) != view.caller()
                    || Some(prepared.artifact) != view.caller_artifact()
                    || prepared.provider != view.provider()
                    || prepared.provider_artifact != view.provider_artifact()
                {
                    return Err(Error::Authority);
                }
                if inner.validate(&prepared.plan, view)? != prepared.permits {
                    return Err(Error::Authority);
                }
                let commits = inner.state.commits.checked_add(1).ok_or(Error::Limit)?;
                let history = inner
                    .state
                    .history
                    .len()
                    .checked_add(prepared.plan.events.len())
                    .ok_or(Error::Limit)?;
                if history > inner.limits.history {
                    return Err(Error::Limit);
                }
                let output = inner.reply(&id, view)?;
                let mut staged = inner.state.clone();
                for write in &prepared.plan.writes {
                    match &write.replacement {
                        Some(value) => {
                            let mut stamp = write.stamp.clone();
                            stamp.revision = stamp.revision.checked_add(1).ok_or(Error::Limit)?;
                            staged.resources.insert(
                                stamp.address.clone(),
                                Snapshot {
                                    stamp,
                                    value: value.clone(),
                                },
                            );
                        }
                        None => {
                            staged.resources.remove(&write.stamp.address);
                        }
                    }
                }
                staged.history.extend(prepared.plan.events.clone());
                staged.pending.remove(&id.serial);
                staged.commits = commits;
                inner.state = staged;
                Ok(output)
            }
            _ => Err(Error::MissingDependency),
        }
    }
}
fn name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
}
fn decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T> {
    serde_json::from_slice(bytes).map_err(|_| Error::NativeRefusal)
}
impl Inner {
    fn address(&self, address: &Resource) -> Result<&OwnerPolicy> {
        if !name(&address.key) {
            return Err(Error::Authority);
        }
        self.owners.get(&address.owner).ok_or(Error::Authority)
    }
    fn permit(
        &self,
        address: &Resource,
        access: Access,
        view: &CallerView,
    ) -> Result<CapabilityPermit> {
        let policy = self.address(address)?;
        let permit = view
            .permit(&policy.capability, address, access)
            .ok_or(Error::Authority)?;
        let provider = permit.resolution.first().ok_or(Error::Authority)?;
        if provider.handle != policy.provider || provider.artifact != policy.artifact {
            return Err(Error::Authority);
        }
        Ok(permit)
    }
    fn validate(&self, plan: &Plan, view: &CallerView) -> Result<Vec<CapabilityPermit>> {
        let count = plan
            .readonly
            .len()
            .checked_add(plan.writes.len())
            .ok_or(Error::Limit)?;
        if count > self.limits.participants
            || plan.events.len() > self.limits.events
            || plan.events.iter().any(|e| e.len() > self.limits.payload)
            || plan.writes.iter().any(|w| {
                w.replacement
                    .as_ref()
                    .is_some_and(|v| v.len() > self.limits.payload)
            })
        {
            return Err(Error::Limit);
        }
        let mut addresses = BTreeSet::new();
        let mut snapshots = Vec::new();
        let mut permits = Vec::new();
        for (stamp, access) in plan
            .readonly
            .iter()
            .map(|s| (s, Access::Read))
            .chain(plan.writes.iter().map(|w| (&w.stamp, Access::Write)))
        {
            if !addresses.insert(stamp.address.clone()) {
                return Err(Error::Duplicate);
            }
            permits.push(self.permit(&stamp.address, access, view)?);
            let current = self
                .state
                .resources
                .get(&stamp.address)
                .ok_or(Error::NativeRefusal)?;
            if &current.stamp != stamp {
                return Err(Error::NativeRefusal);
            }
            snapshots.push(current.clone());
        }
        for owner in addresses.iter().map(|a| &a.owner).collect::<BTreeSet<_>>() {
            let declared = Declared {
                snapshots: snapshots
                    .iter()
                    .filter(|s| &s.stamp.address.owner == owner)
                    .cloned()
                    .collect(),
                plan: Plan {
                    readonly: plan
                        .readonly
                        .iter()
                        .filter(|s| &s.address.owner == owner)
                        .cloned()
                        .collect(),
                    writes: plan
                        .writes
                        .iter()
                        .filter(|w| &w.stamp.address.owner == owner)
                        .cloned()
                        .collect(),
                    events: plan.events.clone(),
                },
            };
            if !(self.owners[owner].validate)(&declared) {
                return Err(Error::NativeRefusal);
            }
        }
        if !(self.accounting)(&Declared {
            snapshots,
            plan: plan.clone(),
        }) {
            return Err(Error::NativeRefusal);
        }
        Ok(permits)
    }
    fn reply<T: Serialize>(&self, value: &T, view: &CallerView) -> Result<Vec<u8>> {
        let output =
            serde_json::to_vec(&serde_json::to_value(value).map_err(|_| Error::NativeRefusal)?)
                .map_err(|_| Error::NativeRefusal)?;
        if output.len() > self.limits.wire.min(view.max_reply()) as usize {
            return Err(Error::Buffer);
        }
        Ok(output)
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    use nethacked_kernel::{Authority, Dependency, Scope};
    fn fixture() -> (Service, CallerView, CallerView, Stamp) {
        let captured = Rc::new(RefCell::new(Vec::new()));
        let output = captured.clone();
        let mut k = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
        let endpoint = Endpoint {
            major: 1,
            access: Access::Write,
            max_request: 4096,
            max_reply: 4096,
        };
        let native: NativePort = Rc::new(move |_, v| {
            output.borrow_mut().push(v.clone());
            Ok(vec![])
        });
        let small_endpoint = Endpoint {
            max_reply: 1,
            ..endpoint.clone()
        };
        let provider = k
            .register_native(
                Manifest {
                    id: "probe".into(),
                    exports: BTreeMap::from([
                        ("run".into(), endpoint.clone()),
                        ("small".into(), small_endpoint),
                    ]),
                    dependencies: vec![],
                    extension_points: BTreeSet::new(),
                    extension: None,
                },
                b"probe",
                BTreeMap::from([("run".into(), native.clone()), ("small".into(), native)]),
            )
            .unwrap();
        let capability = Capability {
            provider: "probe".into(),
            endpoint: "run".into(),
            major: 1,
        };
        let wasm = wat::parse_str(r#"(module
            (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1) (func (export "alloc") (param i32) (result i32) i32.const 0)
            (func (export "run") (param i32 i32) (result i64)
                i32.const 0 i32.const 0 i32.const 0 i32.const 100 i32.const 4096 call $call drop
                i32.const 1 i32.const 0 i32.const 0 i32.const 100 i32.const 4096 call $call drop i64.const 0))"#).unwrap();
        let caller = k
            .register_guest(
                Manifest {
                    id: "caller".into(),
                    exports: BTreeMap::from([("run".into(), endpoint)]),
                    dependencies: vec![
                        Dependency {
                            capability: capability.clone(),
                            access: Access::Write,
                        },
                        Dependency {
                            capability: Capability {
                                endpoint: "small".into(),
                                ..capability.clone()
                            },
                            access: Access::Write,
                        },
                    ],
                    extension_points: BTreeSet::new(),
                    extension: None,
                },
                &wasm,
            )
            .unwrap();
        let address = Resource {
            owner: "owner".into(),
            key: "key".into(),
        };
        let auth = Authority {
            access: Access::Write,
            scope: Scope::Keys(BTreeSet::from([address.clone()])),
        };
        k.issue_grant(caller, 0, auth.clone()).unwrap();
        k.issue_grant(caller, 1, auth.clone()).unwrap();
        k.invoke(caller, "run", &[], auth).unwrap();
        let service = Service::new(Limits::default(), Rc::new(|_| true)).unwrap();
        service.0.borrow_mut().binding = Some((provider, k.artifact(provider).unwrap()));
        service
            .register_owner(
                "owner",
                OwnerPolicy {
                    capability,
                    provider,
                    artifact: k.artifact(provider).unwrap(),
                    validate: Rc::new(|_| true),
                },
            )
            .unwrap();
        let stamp = service.put(address, vec![1]).unwrap();
        let views = captured.borrow();
        (service, views[0].clone(), views[1].clone(), stamp)
    }
    fn plan(stamp: &Stamp) -> Plan {
        Plan {
            readonly: vec![],
            writes: vec![Write {
                stamp: stamp.clone(),
                replacement: Some(vec![2]),
            }],
            events: vec![vec![3]],
        }
    }
    fn prepare(s: &Service, v: &CallerView, p: &Plan) -> Identity {
        serde_json::from_slice(
            &s.dispatch("prepare", &serde_json::to_vec(p).unwrap(), v)
                .unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn checked_counters_refuse_before_any_mutation() {
        for kind in ["serial", "generation", "revision", "commits"] {
            let (s, v, _, stamp) = fixture();
            match kind {
                "serial" => {
                    s.0.borrow_mut().state.next_serial = u64::MAX;
                    let before = s.state().unwrap();
                    assert_eq!(
                        s.dispatch("prepare", &serde_json::to_vec(&plan(&stamp)).unwrap(), &v),
                        Err(Error::Limit)
                    );
                    assert_eq!(s.state().unwrap(), before);
                }
                "generation" => {
                    s.0.borrow_mut().state.next_generation = u64::MAX;
                    let before = s.state().unwrap();
                    assert_eq!(
                        s.put(
                            Resource {
                                owner: "owner".into(),
                                key: "new".into()
                            },
                            vec![]
                        ),
                        Err(Error::Limit)
                    );
                    assert_eq!(s.state().unwrap(), before);
                }
                "revision" => {
                    s.0.borrow_mut()
                        .state
                        .resources
                        .get_mut(&stamp.address)
                        .unwrap()
                        .stamp
                        .revision = u64::MAX;
                    let mut changed = stamp.clone();
                    changed.revision = u64::MAX;
                    let before = s.state().unwrap();
                    assert_eq!(s.put(stamp.address.clone(), vec![4]), Err(Error::Limit));
                    assert_eq!(s.state().unwrap(), before);
                    let id = prepare(&s, &v, &plan(&changed));
                    let before = s.state().unwrap();
                    assert_eq!(
                        s.dispatch("commit", &serde_json::to_vec(&id).unwrap(), &v),
                        Err(Error::Limit)
                    );
                    assert_eq!(s.state().unwrap(), before);
                }
                "commits" => {
                    let id = prepare(&s, &v, &plan(&stamp));
                    s.0.borrow_mut().state.commits = u64::MAX;
                    let before = s.state().unwrap();
                    assert_eq!(
                        s.dispatch("commit", &serde_json::to_vec(&id).unwrap(), &v),
                        Err(Error::Limit)
                    );
                    assert_eq!(s.state().unwrap(), before);
                }
                _ => unreachable!(),
            }
        }
    }
    #[test]
    fn serialization_and_output_bounds_are_staged_before_effects() {
        struct Fail;
        impl Serialize for Fail {
            fn serialize<S: serde::Serializer>(
                &self,
                _: S,
            ) -> std::result::Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("fixture serializer refusal"))
            }
        }
        let (s, v, _, _) = fixture();
        let before = s.state().unwrap();
        assert_eq!(s.0.borrow().reply(&Fail, &v), Err(Error::NativeRefusal));
        assert_eq!(s.state().unwrap(), before);
        let (s, v, small, stamp) = fixture();
        let before = s.state().unwrap();
        assert_eq!(
            s.dispatch(
                "prepare",
                &serde_json::to_vec(&plan(&stamp)).unwrap(),
                &small
            ),
            Err(Error::Buffer)
        );
        assert_eq!(s.state().unwrap(), before);
        let id = prepare(&s, &v, &plan(&stamp));
        let before = s.state().unwrap();
        assert_eq!(
            s.dispatch("commit", &serde_json::to_vec(&id).unwrap(), &small),
            Err(Error::Buffer)
        );
        assert_eq!(s.state().unwrap(), before);
        assert!(s
            .dispatch("commit", &serde_json::to_vec(&id).unwrap(), &v)
            .is_ok());
    }
}
