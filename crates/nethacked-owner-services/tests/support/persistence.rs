use nethacked_kernel::{
    persistence::*, Access, Authority, CallerView, Capability, CapabilityPermit, Extension, Kernel,
    Manifest, ModuleHandle, NativePort, OperationBinding, ProviderBinding, Resource, Scope,
};
use nethacked_owners::{persistence::*, *};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
#[derive(Clone)]
enum Material {
    Guest(Manifest, Vec<u8>),
    Native(Manifest, Vec<u8>),
}
include!("../../../nethacked-owners/tests/support/persistence_execution.rs");
const LIMIT: usize = 1_048_576;
fn identity(n: u8) -> CodecIdentity {
    let id = |name: &str| ArtifactIdentity {
        name: name.into(),
        version: 1,
        artifact: [n; 32],
    };
    CodecIdentity {
        schema: id("schema"),
        codec: id("codec"),
        policy: id("policy"),
        schema_digest: [1; 32],
    }
}
fn old_policy() -> QueryPolicy {
    QueryPolicy::Deterministic {
        profile: [1; 32],
        input_policy: [1; 32],
    }
}
fn new_policy() -> QueryPolicy {
    QueryPolicy::Deterministic {
        profile: [2; 32],
        input_policy: [2; 32],
    }
}
fn bytes(n: u32) -> Vec<u8> {
    n.to_le_bytes().to_vec()
}
fn number(b: &[u8]) -> u32 {
    u32::from_le_bytes(b.try_into().unwrap())
}
struct Domain {
    add: u32,
    effects: Rc<Cell<usize>>,
}
impl Schema<Vec<u8>> for Domain {
    fn validate(&self, s: &Vec<u8>) -> Result<()> {
        if s.len() == 4 {
            Ok(())
        } else {
            Err(Error::Invariant)
        }
    }
    fn encode(&self, s: &Vec<u8>) -> Result<Vec<u8>> {
        Ok(s.clone())
    }
    fn query(&self, s: &Vec<u8>, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        self.effects.set(self.effects.get() + 1);
        Ok(s.clone())
    }
}
impl WasmCodec<Vec<u8>> for Domain {
    fn input(&self, s: &Vec<u8>, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        self.effects.set(self.effects.get() + 1);
        Ok(bytes(number(s).checked_add(self.add).ok_or(Error::Invariant)?))
    }
    fn decode(&self, b: &[u8]) -> Result<Proposal<Vec<u8>>> {
        self.effects.set(self.effects.get() + 1);
        Ok(Proposal {
            next: b.to_vec(),
            reply: b.to_vec(),
            events: vec![b.to_vec()],
        })
    }
}
struct StateCodec;
impl PersistenceCodec<Vec<u8>> for StateCodec {
    fn identity(&self) -> CodecIdentity {
        identity(3)
    }
    fn encode_state(&self, s: &Vec<u8>, limit: usize) -> Result<Vec<u8>> {
        if s.len() > limit {
            Err(Error::Limit)
        } else {
            Ok(s.clone())
        }
    }
    fn decode_state(&self, b: &[u8], limit: usize, _: &Remap) -> Result<Vec<u8>> {
        if b.len() > limit {
            Err(Error::Limit)
        } else {
            Ok(b.to_vec())
        }
    }
    fn validate_state(&self, s: &Vec<u8>) -> Result<()> {
        if s.len() == 4 {
            Ok(())
        } else {
            Err(Error::Invariant)
        }
    }
    fn references(&self, _: &Vec<u8>) -> Result<Vec<StateReference>> {
        Ok(vec![])
    }
    fn validate_configuration(
        &self,
        _: &Vec<u8>,
        p: &BTreeMap<String, Port>,
        _: &QueryPolicy,
    ) -> Result<()> {
        config(p)
    }
}
fn config(p: &BTreeMap<String, Port>) -> Result<()> {
    if p.len() == 3
        && p.get("query").is_some_and(|p| p.access == Access::Read)
        && p.get("prepare").is_some_and(|p| p.access == Access::Write)
        && p.get("derived").is_some_and(|p| p.access == Access::Read)
    {
        Ok(())
    } else {
        Err(Error::Schema)
    }
}
thread_local! {static NATIVE_EXECUTIONS:Cell<usize>=const {Cell::new(0)};}
struct NativeDomain;
impl Schema<u64> for NativeDomain {
    fn validate(&self, _: &u64) -> Result<()> {
        Ok(())
    }
    fn encode(&self, s: &u64) -> Result<Vec<u8>> {
        Ok(s.to_le_bytes().to_vec())
    }
    fn query(&self, s: &u64, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        NATIVE_EXECUTIONS.with(|n| n.set(n.get() + 1));
        self.encode(s)
    }
}
impl NativeBehavior<u64> for NativeDomain {
    fn prepare(&self, s: &u64, _: &str, _: &[u8], _: &[Observation]) -> Result<Proposal<u64>> {
        NATIVE_EXECUTIONS.with(|n| n.set(n.get() + 1));
        Ok(Proposal {
            next: s + 1,
            reply: vec![],
            events: vec![],
        })
    }
}
impl PersistenceCodec<u64> for NativeDomain {
    fn identity(&self) -> CodecIdentity {
        identity(4)
    }
    fn encode_state(&self, s: &u64, _: usize) -> Result<Vec<u8>> {
        self.encode(s)
    }
    fn decode_state(&self, b: &[u8], limit: usize, _: &Remap) -> Result<u64> {
        if b.len() > limit {
            return Err(Error::Limit);
        }
        Ok(u64::from_le_bytes(
            b.try_into().map_err(|_| Error::Invalid)?,
        ))
    }
    fn validate_state(&self, s: &u64) -> Result<()> {
        self.validate(s)
    }
    fn references(&self, _: &u64) -> Result<Vec<StateReference>> {
        Ok(vec![])
    }
    fn validate_configuration(
        &self,
        _: &u64,
        p: &BTreeMap<String, Port>,
        policy: &QueryPolicy,
    ) -> Result<()> {
        if policy != &QueryPolicy::Noncacheable {
            return Err(Error::Schema);
        }
        config(p)
    }
}
struct Approved<'a> {
    reservation: &'a OwnerIdentityReservation,
    materials: &'a BTreeMap<[u8; 32], Material>,
    contexts: &'a [SavedOperationContext],
    action: Rc<RefCell<Option<Action>>>,
    factories: Cell<usize>,
    deny: bool,
    timer: Option<NativePort>,
    grants: &'a [SavedGrant],
    cleanups: &'a BTreeSet<u64>,
}
impl RegistryAuthority for Approved<'_> {
    fn resolve(&self, row: &SavedModule) -> nethacked_kernel::Result<ArtifactMaterial> {
        self.factories.set(self.factories.get() + 1);
        match self
            .materials
            .get(&row.artifact)
            .ok_or(nethacked_kernel::Error::InvalidModule)?
        {
            Material::Guest(m, b) => Ok(ArtifactMaterial::Guest {
                manifest: m.clone(),
                bytes: b.clone(),
            }),
            Material::Native(m, b) => {
                let ports = if m.id=="timers" {BTreeMap::from([("queue".into(),self.timer.clone().ok_or(nethacked_kernel::Error::Authority)?)])} else {m
                    .exports
                    .keys()
                    .map(|name| {
                        let a = self.action.clone();
                        let p: NativePort = Rc::new(move |_: &[u8], view: &CallerView| {
                            a.borrow_mut()
                                .as_mut()
                                .ok_or(nethacked_kernel::Error::NativeRefusal)?(
                                view
                            )
                        });
                        (name.clone(), p)
                    })
                    .collect()};
                Ok(ArtifactMaterial::Native {
                    manifest: m.clone(),
                    identity: b.clone(),
                    ports,
                })
            }
        }
    }
    fn bind_resource(&self, r: &Resource) -> nethacked_kernel::Result<Resource> {
        if r.owner=="timers" && r.key=="queue" {return Ok(r.clone());}
        self.reservation
            .resource(r)
            .map_err(|_| nethacked_kernel::Error::Authority)
    }
    fn approve_grant(
        &self,
        caller: &SavedModule,
        slot: usize,
        target: &SavedModule,
        a: &Authority,
    ) -> nethacked_kernel::Result<Authority> {
        if self.deny {return Err(nethacked_kernel::Error::Authority);}
        let expected=self.grants.iter().find(|g|g.caller==caller.id && g.slot==slot && g.target==target.id).ok_or(nethacked_kernel::Error::Authority)?;
        let approved=Authority {access:expected.requested.access,scope:match &expected.requested.scope {Scope::All=>Scope::All,Scope::Keys(keys)=>Scope::Keys(keys.iter().map(|r|self.bind_resource(r)).collect::<nethacked_kernel::Result<_>>()?)}};
        if &approved==a {Ok(approved)} else {Err(nethacked_kernel::Error::Authority)}
    }
}
impl RetainedAuthority for Approved<'_> {
    fn approve_cleanup(&self, id: u64) -> nethacked_kernel::Result<()> {
        if self.deny || !self.cleanups.contains(&id) {
            Err(nethacked_kernel::Error::Authority)
        } else {
            Ok(())
        }
    }
    fn approve_operation_context(&self, c: &SavedOperationContext) -> nethacked_kernel::Result<()> {
        if !self.deny && self.contexts.contains(c) {
            Ok(())
        } else {
            Err(nethacked_kernel::Error::Authority)
        }
    }
}
fn factory_catalog(
    effects: Rc<Cell<usize>>,
    base: &CapabilityPermit,
    base_ids: &[u64],
    new: &CapabilityPermit,
    new_ids: &[u64],
    old: bool,
) -> RestoreCatalog<Vec<u8>> {
    factory_catalog_tracked(effects, base, base_ids, new, new_ids, old, None)
}
#[allow(clippy::too_many_arguments)]
fn factory_catalog_tracked(
    effects: Rc<Cell<usize>>,
    base: &CapabilityPermit,
    base_ids: &[u64],
    new: &CapabilityPermit,
    new_ids: &[u64],
    old: bool,
    registrations: Option<Rc<RefCell<CaptureCatalog<Vec<u8>>>>>,
) -> RestoreCatalog<Vec<u8>> {
    let mut c = RestoreCatalog::default();
    for (n, add, p, b, ids) in [
        (1, 1, old_policy(), base, base_ids),
        (2, 20, new_policy(), new, new_ids),
    ] {
        if n == 1 && !old {
            continue;
        }
        let b = b.clone();
        let ids = ids.to_vec();
        let effects = effects.clone();
        let registrations = registrations.clone();
        let policy = p.clone();
        c.allow(identity(n), p, move |r| {
            let binding = CapabilityPermit {
                capability: b.capability.clone(),
                resolution: b
                    .resolution
                    .iter()
                    .zip(&ids)
                    .map(|(b, id)| r.binding(b, *id))
                    .collect::<Result<_>>()?,
            };
            let codec: Rc<dyn WasmCodec<Vec<u8>>> = Rc::new(Domain {
                add,
                effects: effects.clone(),
            });
            if let Some(c) = &registrations {
                c.borrow_mut().register(
                    identity(n),
                    policy.clone(),
                    Backend::Wasm {
                        codec: codec.clone(),
                        slot: 0,
                        binding: binding.clone(),
                    },
                )?;
            }
            Ok(Backend::Wasm {
                codec,
                slot: 0,
                binding,
            })
        })
        .unwrap();
    }
    c
}
