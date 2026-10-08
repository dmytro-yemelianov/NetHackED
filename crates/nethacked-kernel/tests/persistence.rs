mod common;
use common::*;
use nethacked_kernel::{persistence::*, *};
use std::{cell::Cell, collections::BTreeMap, rc::Rc};

#[derive(Clone)]
enum Material {
    Native(Manifest, Vec<u8>, u32),
    Guest(Manifest, Vec<u8>),
}
struct Host {
    materials: BTreeMap<[u8; 32], Material>,
    effects: Rc<Cell<usize>>,
    factories: Cell<usize>,
    deny: bool,
    broaden: bool,
    narrow: bool,
}
impl RegistryAuthority for Host {
    fn resolve(&self, row: &SavedModule) -> Result<ArtifactMaterial> {
        self.factories.set(self.factories.get() + 1);
        match self
            .materials
            .get(&row.artifact)
            .ok_or(Error::InvalidModule)?
        {
            Material::Native(manifest, identity, value) => {
                let effects = self.effects.clone();
                let value = *value;
                let fresh_state = Rc::new(Cell::new(value));
                let port: NativePort = Rc::new(move |_, view| {
                    if view.caller().is_some() {
                        assert_eq!(
                            view.authority().scope,
                            Scope::Keys(
                                [Resource {
                                    owner: "destination".into(),
                                    key: "checked".into()
                                }]
                                .into()
                            )
                        );
                    }
                    effects.set(effects.get() + 1);
                    let reply = fresh_state.get();
                    fresh_state.set(reply + 1);
                    Ok(bytes(reply))
                });
                Ok(ArtifactMaterial::Native {
                    manifest: manifest.clone(),
                    identity: identity.clone(),
                    ports: BTreeMap::from([("run".into(), port)]),
                })
            }
            Material::Guest(manifest, bytes) => Ok(ArtifactMaterial::Guest {
                manifest: manifest.clone(),
                bytes: bytes.clone(),
            }),
        }
    }
    fn bind_resource(&self, source: &Resource) -> Result<Resource> {
        match (source.owner.as_str(), source.key.as_str()) {
            ("device", "old") => Ok(Resource {
                owner: "destination".into(),
                key: "checked".into(),
            }),
            ("destination", "checked") => Ok(Resource {
                owner: "restart".into(),
                key: "approved".into(),
            }),
            _ => Err(Error::Authority),
        }
    }
    fn approve_grant(
        &self,
        _: &SavedModule,
        _: usize,
        _: &SavedModule,
        requested: &Authority,
    ) -> Result<Authority> {
        if self.deny {
            return Err(Error::Authority);
        }
        if self.broaden {
            return Ok(trusted(Access::Write));
        }
        if self.narrow {
            return Ok(Authority {
                access: Access::Read,
                scope: Scope::Keys(Default::default()),
            });
        }
        Ok(requested.clone())
    }
}
fn add_native(k: &mut Kernel, h: &mut Host, m: Manifest, value: u32) -> ModuleHandle {
    let identity = m.id.as_bytes().to_vec();
    let effects = h.effects.clone();
    let port: NativePort = Rc::new(move |_, _| {
        effects.set(effects.get() + 1);
        Ok(bytes(value))
    });
    let handle = k
        .register_native(m.clone(), &identity, BTreeMap::from([("run".into(), port)]))
        .unwrap();
    h.materials.insert(
        k.artifact(handle).unwrap(),
        Material::Native(m, identity, value),
    );
    handle
}
fn add_guest(k: &mut Kernel, h: &mut Host, m: Manifest, code: Vec<u8>) -> ModuleHandle {
    let handle = k.register_guest(m.clone(), &code).unwrap();
    h.materials
        .insert(k.artifact(handle).unwrap(), Material::Guest(m, code));
    handle
}
fn fixture() -> (Kernel, Host, Vec<ModuleHandle>, RegistrySnapshot) {
    let mut k = Kernel::new(Limits::default()).unwrap();
    let mut h = Host {
        materials: BTreeMap::new(),
        effects: Rc::new(Cell::new(0)),
        factories: Cell::new(0),
        deny: false,
        broaden: false,
        narrow: false,
    };
    let hole = add_native(&mut k, &mut h, manifest("hole", Access::Read), 0);
    k.managed_unload(hole).unwrap();
    let mut m = manifest("base", Access::Read);
    m.extension_points.insert("run".into());
    let base = add_native(&mut k, &mut h, m, 1);
    let mut extensions = Vec::new();
    for id in ["old1", "old2", "current"] {
        let mut m = manifest(id, Access::Read);
        m.extension = Some(Extension {
            target: dependency("base", "run", Access::Read).capability,
            export: "run".into(),
        });
        let ext = if id == "current" {
            add_guest(
                &mut k,
                &mut h,
                m,
                wasm("i32.const 256 i32.const 42 i32.store i64.const 1099511627780"),
            )
        } else {
            add_native(&mut k, &mut h, m, 7)
        };
        if id != "current" {
            k.managed_detach(ext).unwrap();
        }
        extensions.push(ext);
    }
    let mut m = manifest("client", Access::Read);
    m.dependencies = vec![
        dependency("old1", "run", Access::Read),
        dependency("base", "run", Access::Read),
    ];
    let client = add_guest(&mut k, &mut h, m, forward(1));
    grant(&mut k, client, 0, Access::Read, &["old"]);
    grant(&mut k, client, 1, Access::Read, &["old"]);
    k.revoke_grant(client, 0).unwrap();
    let owner = k.retain_owner(&[base]).unwrap();
    k.release_owner(owner).unwrap();
    k.issue_operation().unwrap();
    let saved = k.registry_snapshot().unwrap();
    let saved = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
    (
        k,
        h,
        vec![base, extensions[0], extensions[1], extensions[2], client],
        saved,
    )
}
fn roundtrip(saved: &RegistrySnapshot) -> RegistrySnapshot {
    serde_json::from_slice(&serde_json::to_vec(saved).unwrap()).unwrap()
}

#[test]
fn native_guest_detached_dispatch_imports_scopes_and_restarts() {
    let (mut source, mut host, old, saved) = fixture();
    let original_effects = host.effects.clone();
    host.effects = Rc::new(Cell::new(0));
    let before = source.lifecycle_state();
    let mut destination = Kernel::new(Limits::default()).unwrap();
    let handles = destination.restore_registry(&saved, &host).unwrap();
    assert_eq!(host.effects.get(), 0);
    assert_eq!(original_effects.get(), 0);
    assert_eq!(destination.lifecycle_state(), before);
    assert_eq!(
        destination.invoke(old[0], "run", &[], trusted(Access::Read)),
        Err(Error::InvalidHandle)
    );
    let base = handles[&saved.modules[0].id];
    let old1 = handles[&saved.modules[1].id];
    let client = handles[&saved.modules.last().unwrap().id];
    assert_eq!(
        number(
            &destination
                .invoke(base, "run", &[], trusted(Access::Read))
                .unwrap()
        ),
        42
    );
    assert_eq!(
        number(
            &destination
                .invoke(old1, "run", &[], trusted(Access::Read))
                .unwrap()
        ),
        7
    );
    assert_eq!(
        number(
            &destination
                .invoke(client, "run", &[], trusted(Access::Read))
                .unwrap()
        ),
        42
    );
    assert_eq!(host.effects.get(), 1);
    assert_eq!(original_effects.get(), 0);
    let current = handles[&saved.modules[3].id];
    destination.managed_detach(current).unwrap();
    assert_eq!(
        number(
            &destination
                .invoke(client, "run", &[], trusted(Access::Read))
                .unwrap()
        ),
        1
    );
    destination.managed_attach(current).unwrap();
    assert_eq!(original_effects.get(), 0);
    let restored = destination.registry_snapshot().unwrap();
    assert!(!restored.grants[0].active);
    assert_eq!(
        restored.grants[1].requested.scope,
        Scope::Keys(
            [Resource {
                owner: "destination".into(),
                key: "checked".into()
            }]
            .into()
        )
    );
    // Independent repeated restart from identical bytes; no source Rc reuse.
    let mut again = Kernel::new(Limits::default()).unwrap();
    let next = again.restore_registry(&roundtrip(&saved), &host).unwrap();
    assert_ne!(next[&saved.modules[0].id], base);
    assert_eq!(again.manifest(base), Err(Error::InvalidHandle));
    assert_eq!(
        number(
            &again
                .invoke(
                    next[&saved.modules[1].id],
                    "run",
                    &[],
                    trusted(Access::Read)
                )
                .unwrap()
        ),
        7
    );
    assert_eq!(
        number(
            &destination
                .invoke(old1, "run", &[], trusted(Access::Read))
                .unwrap()
        ),
        8
    );
    // Fresh snapshot includes all historical namespaces. Its keys require another
    // explicit host binding rather than reinterpreting the previous strings.
    assert!(restored.namespaces.is_superset(&saved.namespaces));
    let mut restarted = Kernel::new(Limits::default()).unwrap();
    let rebuilt = restarted
        .restore_registry(&roundtrip(&restored), &host)
        .unwrap();
    let operation = restarted.issue_operation().unwrap();
    assert!(!restored.namespaces.contains(&operation.namespace()));
    assert_eq!(restarted.manifest(base), Err(Error::InvalidHandle));
    assert_eq!(
        number(
            &restarted
                .invoke(
                    rebuilt[&saved.modules[1].id],
                    "run",
                    &[],
                    trusted(Access::Read)
                )
                .unwrap()
        ),
        7
    );
    assert_eq!(
        restarted.registry_snapshot().unwrap().grants[1]
            .requested
            .scope,
        Scope::Keys(
            [Resource {
                owner: "restart".into(),
                key: "approved".into()
            }]
            .into()
        )
    );
    assert_eq!(
        source.registry_snapshot().unwrap().next_operation,
        saved.next_operation
    );
    host.narrow = true;
    let mut narrowed = Kernel::new(Limits::default()).unwrap();
    narrowed.restore_registry(&saved, &host).unwrap();
    assert_eq!(
        narrowed.registry_snapshot().unwrap().grants[1]
            .requested
            .scope,
        Scope::Keys(Default::default())
    );
}

#[test]
fn refusals_preserve_populated_destination_and_effects() {
    let (mut destination, mut host, handles, saved) = fixture();
    let before = destination.lifecycle_state();
    let artifact = destination.artifact(handles[0]).unwrap();
    let mut bad = Vec::new();
    let mut s = roundtrip(&saved);
    s.modules[0].artifact[0] ^= 1;
    bad.push(s);
    let mut s = roundtrip(&saved);
    s.modules[0].executable = Executable::WasmAbiV1;
    bad.push(s);
    let mut s = roundtrip(&saved);
    s.modules[0].manifest.exports.get_mut("run").unwrap().major = 2;
    bad.push(s);
    let mut s = roundtrip(&saved);
    s.modules[1].target = Some(s.modules[1].id);
    bad.push(s);
    let mut s = roundtrip(&saved);
    s.modules.last_mut().unwrap().dependencies.swap(0, 1);
    bad.push(s);
    let mut s = roundtrip(&saved);
    s.dispatch[0].export = "forged".into();
    bad.push(s);
    let mut s = roundtrip(&saved);
    s.grants[1].target = s.modules[1].id;
    bad.push(s);
    let mut s = roundtrip(&saved);
    s.grants[1].requested = trusted(Access::Write);
    bad.push(s);
    let mut s = roundtrip(&saved);
    s.grants[1].requested.scope = Scope::Keys([resource("forged")].into());
    bad.push(s);
    let mut s = roundtrip(&saved);
    s.next_generation = s.modules.last().unwrap().id;
    bad.push(s);
    let mut s = roundtrip(&saved);
    s.retention_counters[2] = 0;
    bad.push(s);
    let mut s = roundtrip(&saved);
    s.modules[0].manifest.id = "x".repeat(129);
    bad.push(s);
    let mut s = roundtrip(&saved);
    s.modules[1].id = s.modules[0].id;
    bad.push(s);
    let mut s = roundtrip(&saved);
    s.grants[1].slot = usize::MAX;
    bad.push(s);
    let mut s = roundtrip(&saved);
    s.namespaces = (1..=65_536).collect();
    bad.push(s);
    let mut s = roundtrip(&saved);
    s.retention_limits.nodes = usize::MAX;
    bad.push(s);
    let mut s = roundtrip(&saved);
    s.limits.module_bytes += 1;
    bad.push(s);
    for (index, s) in bad.into_iter().enumerate() {
        assert!(
            destination.restore_registry(&s, &host).is_err(),
            "fixture {index}"
        );
        assert_eq!(destination.lifecycle_state(), before);
        assert_eq!(destination.artifact(handles[0]).unwrap(), artifact);
        assert_eq!(host.effects.get(), 0);
    }
    host.deny = true;
    assert_eq!(
        destination.restore_registry(&saved, &host),
        Err(Error::Authority)
    );
    host.deny = false;
    host.broaden = true;
    assert_eq!(
        destination.restore_registry(&saved, &host),
        Err(Error::Authority)
    );
    assert_eq!(destination.lifecycle_state(), before);
    assert_eq!(host.effects.get(), 0);
}

#[test]
fn retention_boundary_and_revocation_are_not_dropped() {
    let (mut k, host, handles, saved) = fixture();
    let reference = k.retain_owner(&[handles[0]]).unwrap();
    let before = k.lifecycle_state();
    assert!(matches!(k.registry_snapshot(), Err(Error::Referenced)));
    assert_eq!(k.restore_registry(&saved, &host), Err(Error::Referenced));
    assert_eq!(k.lifecycle_state(), before);
    assert_eq!(host.factories.get(), 0);
    k.release_owner(reference).unwrap();
    let captured = k.registry_snapshot().unwrap();
    let fresh = k.restore_registry(&captured, &host).unwrap();
    let client = fresh[&captured.modules.last().unwrap().id];
    // A guest targeting the revoked slot is still admitted, but live grant
    // checks refuse its import (ABI refusal bytes are returned by this fixture).
    let state = k.registry_snapshot().unwrap();
    assert!(!state.grants[0].active);
    assert_eq!(k.manifest(handles[4]), Err(Error::InvalidHandle));
    k.revoke_grant(client, 1).unwrap();
    assert_ne!(
        number(&k.invoke(client, "run", &[], trusted(Access::Read)).unwrap()),
        42
    );
    assert_eq!(host.effects.get(), 0);
}

#[test]
fn namespace_child() {
    let Ok(path) = std::env::var("KERNEL_D3A_FIXTURE") else {
        return;
    };
    let saved: RegistrySnapshot = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let (_, host, _, _) = fixture();
    let mut k = Kernel::new(Limits::default()).unwrap();
    k.restore_registry(&saved, &host).unwrap();
    let operation = k.issue_operation().unwrap();
    assert!(!saved.namespaces.contains(&operation.namespace()));
    assert_eq!(operation.serial(), saved.next_operation);
    assert_eq!(host.effects.get(), 0);
}
#[test]
fn separate_process_excludes_source_and_historical_namespaces() {
    let (_, _, _, mut saved) = fixture();
    saved.namespaces.extend(1..=100);
    let path = std::env::temp_dir().join(format!("kernel-d3a-{}.json", std::process::id()));
    std::fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "namespace_child", "--nocapture"])
        .env("KERNEL_D3A_FIXTURE", &path)
        .output()
        .unwrap();
    // Keep actual bytes/logs if a child refusal ever exposes a defect.
    if !output.status.success() {
        std::fs::write(path.with_extension("stdout"), &output.stdout).unwrap();
        std::fs::write(path.with_extension("stderr"), &output.stderr).unwrap();
    }
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn changed_native_and_guest_material_refuse_without_domain_calls() {
    let (mut k, mut h, handles, saved) = fixture();
    let before = k.lifecycle_state();
    let original = h.materials.clone();
    let native = saved.modules[0].artifact;
    if let Material::Native(_, identity, _) = h.materials.get_mut(&native).unwrap() {
        identity.push(99);
    }
    assert_eq!(k.restore_registry(&saved, &h), Err(Error::InvalidModule));
    h.materials = original.clone();
    let guest = saved.modules[3].artifact;
    if let Material::Guest(_, code) = h.materials.get_mut(&guest).unwrap() {
        *code = vec![1, 2, 3];
    }
    assert_eq!(k.restore_registry(&saved, &h), Err(Error::InvalidModule));
    h.materials = original.clone();
    if let Material::Guest(_, code) = h.materials.get_mut(&guest).unwrap() {
        *code = wasm("i64.const 0");
    }
    assert_eq!(k.restore_registry(&saved, &h), Err(Error::InvalidModule));
    h.materials = original;
    if let Material::Native(m, _, _) = h.materials.get_mut(&native).unwrap() {
        m.exports.get_mut("run").unwrap().max_reply = 3;
    }
    assert_eq!(k.restore_registry(&saved, &h), Err(Error::InvalidModule));
    assert_eq!(k.lifecycle_state(), before);
    assert!(k.manifest(handles[0]).is_ok());
    assert_eq!(h.effects.get(), 0);
}

#[test]
fn live_leases_pins_pure_captures_callbacks_refuse_and_empty_history_survives() {
    use std::cell::RefCell;
    let mut k = Kernel::new(Limits::default()).unwrap();
    let pin = Rc::new(RefCell::new(None));
    let pure = Rc::new(RefCell::new(None));
    let callback = Rc::new(RefCell::new(None));
    let mut host = Host {
        materials: BTreeMap::new(),
        effects: Rc::new(Cell::new(0)),
        factories: Cell::new(0),
        deny: false,
        broaden: false,
        narrow: false,
    };
    let helper = add_guest(
        &mut k,
        &mut host,
        manifest("helper", Access::Read),
        wasm("i64.const 0"),
    );
    let mut service = manifest("service", Access::Read);
    service
        .dependencies
        .push(dependency("helper", "run", Access::Read));
    let (p, c, b) = (pin.clone(), pure.clone(), callback.clone());
    let port: NativePort = Rc::new(move |_, v| {
        *p.borrow_mut() = Some(v.pin_invocation()?);
        *c.borrow_mut() = Some(v.capture_pure(0)?);
        *b.borrow_mut() = Some(v.capture_callback("run", authority(Access::Read, &["old"]))?);
        Ok(bytes(0))
    });
    let service_handle = k
        .register_native(
            service.clone(),
            b"service",
            BTreeMap::from([("run".into(), port)]),
        )
        .unwrap();
    host.materials.insert(
        k.artifact(service_handle).unwrap(),
        Material::Native(service, b"service".to_vec(), 0),
    );
    let mut caller = manifest("caller", Access::Read);
    caller
        .dependencies
        .push(dependency("service", "run", Access::Read));
    let caller = add_guest(&mut k, &mut host, caller, forward(0));
    grant(&mut k, service_handle, 0, Access::Read, &["old"]);
    grant(&mut k, caller, 0, Access::Read, &["old"]);
    let lease = k
        .capture_pipeline(caller, "run", trusted(Access::Read))
        .unwrap();
    assert!(matches!(k.registry_snapshot(), Err(Error::Referenced)));
    let op = k.issue_operation().unwrap();
    let binding = k.operation_binding(op, lease, 0, "phase").unwrap();
    k.invoke_operation(&binding, &[], trusted(Access::Read))
        .unwrap();
    k.release_pipeline(lease).unwrap();
    assert!(callback.borrow().is_some());
    assert!(matches!(k.registry_snapshot(), Err(Error::Referenced)));
    pin.borrow_mut().take();
    assert!(matches!(k.registry_snapshot(), Err(Error::Referenced)));
    pure.borrow_mut().take();
    assert!(matches!(k.registry_snapshot(), Err(Error::Referenced)));
    callback.borrow_mut().take();
    let saved = k.registry_snapshot().unwrap();
    assert!(saved.retention_counters.iter().all(|n| *n > 1));
    let fresh = k.restore_registry(&roundtrip(&saved), &host).unwrap();
    assert_eq!(
        k.registry_snapshot().unwrap().retention_counters,
        saved.retention_counters
    );
    assert_eq!(k.issue_operation().unwrap().serial(), saved.next_operation);
    assert_eq!(k.manifest(helper), Err(Error::InvalidHandle));
    let new = k.retain_owner(&[fresh[&saved.modules[0].id]]).unwrap();
    k.release_owner(new).unwrap();
    assert_eq!(
        k.registry_snapshot().unwrap().retention_counters[0],
        saved.retention_counters[0] + 1
    );
}

#[test]
fn retired_holes_and_checked_counter_exhaustion() {
    let (mut k, mut host, _, mut saved) = fixture();
    let fresh = k.restore_registry(&saved, &host).unwrap();
    let old_generation = saved.next_generation;
    let added = add_native(&mut k, &mut host, manifest("after", Access::Read), 1);
    let later = k.registry_snapshot().unwrap();
    assert_eq!(later.modules.last().unwrap().id, old_generation);
    assert_eq!(later.modules[0].id, 2); // Retired generation 1 stays a hole.
    assert_ne!(added, fresh[&saved.modules[0].id]);
    saved.next_generation = u64::MAX;
    saved.next_operation = u64::MAX;
    saved.retention_counters = [u64::MAX; 3];
    k.restore_registry(&saved, &host).unwrap();
    let before = k.lifecycle_state();
    assert_eq!(k.issue_operation(), Err(Error::Limit));
    let h = k.registry_snapshot().unwrap().modules[0].id;
    let handles = k.restore_registry(&saved, &host).unwrap();
    assert_eq!(k.retain_owner(&[handles[&h]]), Err(Error::Limit));
    let m = manifest("exhausted", Access::Read);
    let port: NativePort = Rc::new(|_, _| panic!("must not invoke"));
    assert_eq!(
        k.register_native(m, b"exhausted", BTreeMap::from([("run".into(), port)])),
        Err(Error::Limit)
    );
    assert_eq!(k.lifecycle_state(), before);
    assert_eq!(host.effects.get(), 0);
}
