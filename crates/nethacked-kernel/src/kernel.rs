use crate::model::*;
use crate::wasm;
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};
use wasmi::{Config, Engine, Module};

/// Trusted bounded native code. A refusal must precede its own effects.
/// Callbacks receive no dispatcher, mutable kernel, or ambient host services.
pub type NativePort = Rc<dyn Fn(&[u8], &CallerView) -> Result<Vec<u8>>>;

static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

pub(crate) enum Code {
    Native(BTreeMap<String, NativePort>),
    Guest(Module),
}
pub(crate) struct Registration {
    pub manifest: Manifest,
    pub artifact: [u8; 32],
    pub code: Code,
}
struct Grant {
    target: ModuleHandle,
    authority: Authority,
    active: bool,
}
pub(crate) struct Budget {
    pub fuel: u64,
    calls: u32,
}
#[derive(Clone)]
struct Invocation {
    caller: Option<ModuleHandle>,
    authority: Authority,
    depth: u32,
}
pub(crate) type SharedBudget = Rc<RefCell<Budget>>;
#[derive(Clone)]
pub(crate) struct Frame {
    pub provider: ModuleHandle,
    pub authority: Authority,
    pub depth: u32,
}

pub struct Kernel {
    instance: u64,
    next_generation: u64,
    pub(crate) limits: Limits,
    pub(crate) engine: Engine,
    registrations: BTreeMap<ModuleHandle, Registration>,
    ids: BTreeMap<String, ModuleHandle>,
    grants: BTreeMap<(ModuleHandle, usize), Grant>,
    extensions: BTreeMap<(ModuleHandle, String), (ModuleHandle, String)>,
}

impl Kernel {
    pub fn new(limits: Limits) -> Result<Self> {
        if limits.memory_bytes < 65536
            || limits.memory_bytes > i32::MAX as usize
            || limits.module_bytes == 0
            || limits.payload_bytes > i32::MAX as u32
            || limits.payload_bytes == 0
            || limits.stack_bytes == 0
            || limits.wasm_recursion == 0
            || limits.depth == 0
            || limits.depth > 64
            || limits.calls == 0
            || limits.fuel == 0
            || limits.registrations == 0
            || limits.manifest_entries == 0
            || limits.scope_keys == 0
        {
            return Err(Error::Limit);
        }
        let instance = NEXT_INSTANCE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| Error::Limit)?;
        let mut config = Config::default();
        config
            .consume_fuel(true)
            .allow_start_fn(false)
            .wasm_multi_memory(false)
            .floats(false)
            .set_min_stack_height(limits.stack_bytes.min(128))
            .set_max_stack_height(limits.stack_bytes)
            .set_max_recursion_depth(limits.wasm_recursion)
            .set_max_cached_stacks(0);
        Ok(Self {
            instance,
            next_generation: 1,
            engine: Engine::new(&config),
            limits,
            registrations: BTreeMap::new(),
            ids: BTreeMap::new(),
            grants: BTreeMap::new(),
            extensions: BTreeMap::new(),
        })
    }

    pub(crate) fn registration(&self, handle: ModuleHandle) -> Result<&Registration> {
        if handle.instance != self.instance {
            return Err(Error::InvalidHandle);
        }
        self.registrations.get(&handle).ok_or(Error::InvalidHandle)
    }
    pub fn manifest(&self, handle: ModuleHandle) -> Result<&Manifest> {
        Ok(&self.registration(handle)?.manifest)
    }
    pub fn artifact(&self, handle: ModuleHandle) -> Result<[u8; 32]> {
        Ok(self.registration(handle)?.artifact)
    }
    pub fn register_native(
        &mut self,
        manifest: Manifest,
        artifact: &[u8],
        ports: BTreeMap<String, NativePort>,
    ) -> Result<ModuleHandle> {
        self.validate_manifest(&manifest)?;
        if ports.keys().ne(manifest.exports.keys()) {
            return Err(Error::Signature);
        }
        self.publish(manifest, artifact, Code::Native(ports), b"native-v1")
    }
    pub fn register_guest(&mut self, manifest: Manifest, bytes: &[u8]) -> Result<ModuleHandle> {
        self.validate_manifest(&manifest)?;
        if bytes.len() > self.limits.module_bytes {
            return Err(Error::Limit);
        }
        let module = wasm::validate(self, &manifest, bytes)?;
        self.publish(manifest, bytes, Code::Guest(module), b"wasm-abi-v1")
    }
    fn valid_name(name: &str) -> bool {
        !name.is_empty()
            && name.len() <= 128
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    }
    fn validate_manifest(&self, manifest: &Manifest) -> Result<()> {
        if self.ids.contains_key(&manifest.id) {
            return Err(Error::Duplicate);
        }
        if self.registrations.len() >= self.limits.registrations {
            return Err(Error::Limit);
        }
        if !Self::valid_name(&manifest.id)
            || manifest.exports.is_empty()
            || manifest.exports.len() > self.limits.manifest_entries
            || manifest.dependencies.len() > self.limits.manifest_entries
            || manifest.extension_points.len() > self.limits.manifest_entries
        {
            return Err(Error::InvalidManifest);
        }
        for (name, ep) in &manifest.exports {
            if !Self::valid_name(name)
                || name == "memory"
                || name == "alloc"
                || ep.major == 0
                || ep.max_request > self.limits.payload_bytes
                || ep.max_reply > self.limits.payload_bytes
            {
                return Err(Error::InvalidManifest);
            }
        }
        for point in &manifest.extension_points {
            if !manifest.exports.contains_key(point) {
                return Err(Error::InvalidManifest);
            }
        }
        for dep in &manifest.dependencies {
            if dep.capability.provider == manifest.id {
                let ep = manifest
                    .exports
                    .get(&dep.capability.endpoint)
                    .ok_or(Error::MissingDependency)?;
                if ep.major != dep.capability.major {
                    return Err(Error::MissingDependency);
                }
            } else {
                self.capability(&dep.capability)?;
            }
        }
        if let Some(ext) = &manifest.extension {
            let (host, ep) = self.capability(&ext.target)?;
            let host_manifest = &self.registration(host)?.manifest;
            if !host_manifest
                .extension_points
                .contains(&ext.target.endpoint)
            {
                return Err(Error::InvalidManifest);
            }
            if self
                .extensions
                .contains_key(&(host, ext.target.endpoint.clone()))
            {
                return Err(Error::Duplicate);
            }
            if manifest.exports.get(&ext.export) != Some(ep) {
                return Err(Error::Signature);
            }
        }
        Ok(())
    }
    fn publish(
        &mut self,
        manifest: Manifest,
        bytes: &[u8],
        code: Code,
        kind: &[u8],
    ) -> Result<ModuleHandle> {
        if bytes.len() > self.limits.module_bytes {
            return Err(Error::Limit);
        }
        let next = self.next_generation.checked_add(1).ok_or(Error::Limit)?;
        let handle = ModuleHandle {
            instance: self.instance,
            generation: self.next_generation,
        };
        let attachment = manifest
            .extension
            .as_ref()
            .map(|ext| {
                self.capability(&ext.target)
                    .map(|(h, _)| (h, ext.target.endpoint.clone(), ext.export.clone()))
            })
            .transpose()?;
        let target_artifact = attachment
            .as_ref()
            .map(|(h, _, _)| self.artifact(*h))
            .transpose()?;
        let canonical = serde_json::to_vec(
            &serde_json::to_value((&manifest, &self.limits, target_artifact))
                .map_err(|_| Error::InvalidManifest)?,
        )
        .map_err(|_| Error::InvalidManifest)?;
        let mut digest = Sha256::new();
        for part in [kind, canonical.as_slice(), bytes] {
            digest.update((part.len() as u64).to_le_bytes());
            digest.update(part);
        }
        let artifact = digest.finalize().into();
        self.ids.insert(manifest.id.clone(), handle);
        self.registrations.insert(
            handle,
            Registration {
                manifest,
                artifact,
                code,
            },
        );
        if let Some((host, point, export)) = attachment {
            self.extensions.insert((host, point), (handle, export));
        }
        self.next_generation = next;
        Ok(handle)
    }
    fn capability(&self, cap: &Capability) -> Result<(ModuleHandle, &Endpoint)> {
        let handle = *self
            .ids
            .get(&cap.provider)
            .ok_or(Error::MissingDependency)?;
        let ep = self
            .registration(handle)?
            .manifest
            .exports
            .get(&cap.endpoint)
            .ok_or(Error::MissingDependency)?;
        if ep.major != cap.major {
            return Err(Error::MissingDependency);
        }
        Ok((handle, ep))
    }
    fn validate_authority(&self, authority: &Authority, bounded: bool) -> Result<()> {
        match &authority.scope {
            Scope::All if bounded => Err(Error::Authority),
            Scope::Keys(keys)
                if keys.len() > self.limits.scope_keys
                    || keys
                        .iter()
                        .any(|r| !Self::valid_name(&r.owner) || !Self::valid_name(&r.key)) =>
            {
                Err(Error::Authority)
            }
            _ => Ok(()),
        }
    }
    /// Only the trusted host can issue or revoke grants. Keys are bounded,
    /// including the empty set for endpoints with no resource schema.
    pub fn issue_grant(
        &mut self,
        caller: ModuleHandle,
        slot: usize,
        authority: Authority,
    ) -> Result<()> {
        self.validate_authority(&authority, true)?;
        let dep = self
            .registration(caller)?
            .manifest
            .dependencies
            .get(slot)
            .ok_or(Error::InvalidSlot)?;
        if authority.access > dep.access {
            return Err(Error::Authority);
        }
        let (target, _) = self.capability(&dep.capability)?;
        self.grants.insert(
            (caller, slot),
            Grant {
                target,
                authority,
                active: true,
            },
        );
        Ok(())
    }
    pub fn revoke_grant(&mut self, caller: ModuleHandle, slot: usize) -> Result<()> {
        self.registration(caller)?;
        self.grants
            .get_mut(&(caller, slot))
            .ok_or(Error::MissingGrant)?
            .active = false;
        Ok(())
    }
    pub fn unregister(&mut self, handle: ModuleHandle) -> Result<()> {
        let id = self.registration(handle)?.manifest.id.clone();
        self.ids.remove(&id);
        self.registrations.remove(&handle);
        self.grants.retain(|(caller, _), _| *caller != handle);
        self.extensions
            .retain(|(host, _), (extension, _)| *host != handle && *extension != handle);
        Ok(())
    }
    pub fn invoke(
        &self,
        provider: ModuleHandle,
        endpoint: &str,
        request: &[u8],
        authority: Authority,
    ) -> Result<Vec<u8>> {
        self.validate_authority(&authority, false)?;
        let budget = Rc::new(RefCell::new(Budget {
            fuel: self.limits.fuel,
            calls: 0,
        }));
        self.charge(&budget, 1)?;
        self.dispatch_inner(
            provider,
            endpoint,
            request,
            Invocation {
                caller: None,
                authority,
                depth: 1,
            },
            &budget,
        )
    }
    fn charge(&self, budget: &SharedBudget, depth: u32) -> Result<()> {
        let mut b = budget.borrow_mut();
        b.calls = b.calls.checked_add(1).ok_or(Error::Limit)?;
        if b.calls > self.limits.calls || depth > self.limits.depth {
            return Err(Error::Limit);
        }
        b.fuel = b.fuel.checked_sub(1).ok_or(Error::Limit)?;
        Ok(())
    }
    pub(crate) fn slot_target(
        &self,
        frame: &Frame,
        slot: usize,
    ) -> Result<(ModuleHandle, String, Authority, &Endpoint)> {
        let dep = self
            .registration(frame.provider)?
            .manifest
            .dependencies
            .get(slot)
            .ok_or(Error::InvalidSlot)?;
        let grant = self
            .grants
            .get(&(frame.provider, slot))
            .ok_or(Error::MissingGrant)?;
        if !grant.active {
            return Err(Error::Revoked);
        }
        let (target, ep) = self.capability(&dep.capability)?;
        if target != grant.target {
            return Err(Error::InvalidHandle);
        }
        let authority = frame
            .authority
            .intersect(&grant.authority)
            .intersect(&Authority {
                access: dep.access,
                scope: Scope::All,
            });
        if authority.access < ep.access {
            return Err(Error::Authority);
        }
        Ok((target, dep.capability.endpoint.clone(), authority, ep))
    }
    pub(crate) fn attempt_slot(&self, frame: &Frame, budget: &SharedBudget) -> Result<()> {
        self.charge(budget, frame.depth.checked_add(1).ok_or(Error::Limit)?)
    }
    pub(crate) fn call_slot(
        &self,
        frame: &Frame,
        slot: usize,
        request: &[u8],
        budget: &SharedBudget,
    ) -> Result<Vec<u8>> {
        let depth = frame.depth.checked_add(1).ok_or(Error::Limit)?;
        let (target, endpoint, authority, _) = self.slot_target(frame, slot)?;
        self.dispatch_inner(
            target,
            &endpoint,
            request,
            Invocation {
                caller: Some(frame.provider),
                authority,
                depth,
            },
            budget,
        )
    }
    fn dispatch_inner(
        &self,
        mut target: ModuleHandle,
        endpoint: &str,
        request: &[u8],
        invocation: Invocation,
        budget: &SharedBudget,
    ) -> Result<Vec<u8>> {
        let Invocation {
            caller,
            mut authority,
            depth,
        } = invocation;
        let original = self
            .registration(target)?
            .manifest
            .exports
            .get(endpoint)
            .ok_or(Error::MissingDependency)?;
        if request.len() > original.max_request as usize {
            return Err(Error::Buffer);
        }
        if authority.access < original.access {
            return Err(Error::Authority);
        }
        // A read endpoint itself imposes a ceiling, even for a trusted write invocation.
        authority.access = authority.access.min(original.access);
        let mut endpoint = endpoint.to_owned();
        let mut hops = 0;
        while let Some((next, export)) = self.extensions.get(&(target, endpoint.clone())) {
            hops += 1;
            if hops > self.limits.depth {
                return Err(Error::Limit);
            }
            target = *next;
            endpoint = export.clone();
        }
        let reg = self.registration(target)?;
        let ep = reg
            .manifest
            .exports
            .get(&endpoint)
            .ok_or(Error::MissingDependency)?;
        let view = CallerView {
            caller,
            caller_artifact: caller.map(|h| self.artifact(h)).transpose()?,
            provider: target,
            provider_artifact: reg.artifact,
            authority: authority.clone(),
            permits: self.inspect_permits(caller, &authority)?,
            max_reply: ep.max_reply,
        };
        let reply = match &reg.code {
            Code::Native(ports) => ports.get(&endpoint).ok_or(Error::Signature)?(request, &view)?,
            Code::Guest(module) => wasm::execute(
                self,
                module,
                &endpoint,
                request,
                Frame {
                    provider: target,
                    authority,
                    depth,
                },
                budget,
            )?,
        };
        if reply.len() > ep.max_reply as usize {
            return Err(Error::Buffer);
        }
        Ok(reply)
    }

    fn inspect_permits(
        &self,
        caller: Option<ModuleHandle>,
        ceiling: &Authority,
    ) -> Result<Vec<(CapabilityPermit, Authority)>> {
        let Some(caller) = caller else {
            return Ok(Vec::new());
        };
        let mut permits = Vec::new();
        'slots: for (slot, dep) in self
            .registration(caller)?
            .manifest
            .dependencies
            .iter()
            .enumerate()
        {
            let Some(grant) = self.grants.get(&(caller, slot)).filter(|g| g.active) else {
                continue;
            };
            let Ok((mut target, _)) = self.capability(&dep.capability) else {
                continue;
            };
            if target != grant.target {
                continue;
            }
            let mut endpoint = dep.capability.endpoint.clone();
            let mut resolution = Vec::new();
            loop {
                if resolution.len() > self.limits.depth as usize {
                    continue 'slots;
                }
                resolution.push(ProviderBinding {
                    handle: target,
                    artifact: self.artifact(target)?,
                    endpoint: endpoint.clone(),
                });
                let Some((next, export)) = self.extensions.get(&(target, endpoint.clone())) else {
                    break;
                };
                target = *next;
                endpoint = export.clone();
            }
            // Participant access is distinct from endpoint admission access: a
            // read dependency may inspect a write-capable owner's resource.
            let authority = ceiling.intersect(&grant.authority).intersect(&Authority {
                access: dep.access,
                scope: Scope::All,
            });
            permits.push((
                CapabilityPermit {
                    capability: dep.capability.clone(),
                    resolution,
                },
                authority,
            ));
        }
        Ok(permits)
    }
}

#[cfg(test)]
mod admission_tests {
    use super::*;

    #[test]
    fn start_functions_are_rejected_before_instantiation() {
        let kernel = Kernel::new(Limits::default()).unwrap();
        let abi = r#"(memory (export "memory") 1)
            (func (export "alloc") (param i32) (result i32) i32.const 0)
            (func (export "run") (param i32 i32) (result i64) i64.const 0)"#;
        let control = wat::parse_str(format!("(module {abi})")).unwrap();
        assert!(wasmi::Module::new(&kernel.engine, &control).is_ok());
        let start = wat::parse_str(format!("(module {abi} (func $start) (start $start))")).unwrap();
        // Observe parsing directly: dry-instantiation fuel errors must not
        // mask a regression that admits a start function into the engine.
        assert!(wasmi::Module::new(&kernel.engine, &start).is_err());
    }
}
