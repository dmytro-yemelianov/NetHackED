use nethacked_kernel::*;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

fn ep(access: Access) -> Endpoint {
    Endpoint {
        major: 1,
        access,
        max_request: 4,
        max_reply: 4,
    }
}
fn manifest(id: &str, access: Access) -> Manifest {
    Manifest {
        id: id.into(),
        exports: BTreeMap::from([("run".into(), ep(access))]),
        dependencies: vec![],
        extension_points: BTreeSet::from(["run".into()]),
        extension: None,
    }
}
fn dep(id: &str, access: Access) -> Dependency {
    Dependency {
        capability: Capability {
            provider: id.into(),
            endpoint: "run".into(),
            major: 1,
        },
        access,
    }
}
fn key() -> Resource {
    Resource {
        owner: "device".into(),
        key: "value".into(),
    }
}
fn auth(access: Access) -> Authority {
    Authority {
        access,
        scope: Scope::Keys(BTreeSet::from([key()])),
    }
}
fn wasm(body: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
      (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "alloc") (param i32) (result i32) i32.const 0)
      (func (export "run") (param $p i32) (param $n i32) (result i64)
        (local $r i32) {body}))"#
    ))
    .unwrap()
}
fn forward(capacity: u32, add: u32) -> Vec<u8> {
    wasm(&format!(
        r#"local.get $p local.get $p i32.load i32.const {add} i32.add i32.store
        i32.const 0 local.get $p local.get $n i32.const 256 i32.const {capacity} call $call local.set $r
        local.get $r i32.const 0 i32.lt_s if i32.const 256 local.get $r i32.store end
        i64.const 1099511627780"#
    ))
}
fn guest(k: &mut Kernel, id: &str, target: &str, access: Access, add: u32) -> ModuleHandle {
    let mut m = manifest(id, access);
    m.dependencies.push(dep(target, Access::Write));
    let h = k.register_guest(m, &forward(4, add)).unwrap();
    k.issue_grant(h, 0, auth(Access::Write)).unwrap();
    h
}
fn extension(k: &mut Kernel, id: &str, point: &str, target: &str, add: u32) -> ModuleHandle {
    let mut m = manifest(id, Access::Write);
    m.extension = Some(Extension {
        target: dep(point, Access::Write).capability,
        export: "run".into(),
    });
    m.dependencies.push(dep(target, Access::Write));
    let h = k.register_guest(m, &forward(4, add)).unwrap();
    k.issue_grant(h, 0, auth(Access::Write)).unwrap();
    h
}
#[derive(Clone, Debug, PartialEq)]
struct State {
    value: u32,
    history: Vec<u32>,
    unrelated: String,
}
struct Fixture {
    k: Kernel,
    root: ModuleHandle,
    algo: ModuleHandle,
    leaf: ModuleHandle,
    device: ModuleHandle,
    state: Rc<RefCell<State>>,
}
impl Fixture {
    fn new(limits: Limits) -> Self {
        let mut k = Kernel::new(limits).unwrap();
        let state = Rc::new(RefCell::new(State {
            value: 6,
            history: vec![6],
            unrelated: "populated".into(),
        }));
        let s = state.clone();
        let p: NativePort = Rc::new(move |bytes, view| {
            if view.authority().access != Access::Write || !view.authority().scope.allows(&key()) {
                return Err(Error::Authority);
            }
            let value = u32::from_le_bytes(bytes.try_into().map_err(|_| Error::NativeRefusal)?);
            let mut state = s.borrow_mut();
            state.value = value;
            state.history.push(value);
            Ok(value.to_le_bytes().to_vec())
        });
        let device = k
            .register_native(
                manifest("device", Access::Write),
                b"device-v1",
                BTreeMap::from([("run".into(), p)]),
            )
            .unwrap();
        let leaf = guest(&mut k, "leaf", "device", Access::Write, 1);
        let algo = guest(&mut k, "algo", "leaf", Access::Write, 0);
        let root = guest(&mut k, "root", "algo", Access::Write, 0);
        Self {
            k,
            root,
            algo,
            leaf,
            device,
            state,
        }
    }
    fn capture(&mut self) -> PipelineLease {
        self.k
            .capture_pipeline(self.root, "run", auth(Access::Write))
            .unwrap()
    }
    fn retained(&self, lease: PipelineLease, n: u32) -> i32 {
        i32::from_le_bytes(
            self.k
                .invoke_pipeline(lease, &n.to_le_bytes(), auth(Access::Write))
                .unwrap()
                .try_into()
                .unwrap(),
        )
    }
    fn ordinary(&self, n: u32) -> i32 {
        i32::from_le_bytes(
            self.k
                .invoke(self.root, "run", &n.to_le_bytes(), auth(Access::Write))
                .unwrap()
                .try_into()
                .unwrap(),
        )
    }
}

#[test]
fn real_wasm_transitive_old_new_chains_and_populated_host() {
    let mut f = Fixture::new(Limits::default());
    let old = f.capture();
    let descriptor = f.k.pipeline_descriptor(old).unwrap().clone();
    assert_eq!(f.retained(old, 10), 11);
    let leaf_ext = extension(&mut f.k, "leaf-new", "leaf", "device", 10);
    let algo_ext = extension(&mut f.k, "algo-new", "algo", "leaf", 100);
    assert_eq!(f.ordinary(10), 120);
    assert_eq!(f.retained(old, 10), 11);
    let new = f.capture();
    assert_eq!(f.retained(new, 10), 120);
    assert_ne!(f.k.pipeline_descriptor(new).unwrap().hash, descriptor.hash);
    assert_eq!(f.k.pipeline_descriptor(old).unwrap(), &descriptor);
    assert!(f
        .k
        .pipeline_descriptor(new)
        .unwrap()
        .routes
        .iter()
        .flatten()
        .any(|b| b.handle == leaf_ext));
    assert!(f
        .k
        .pipeline_descriptor(new)
        .unwrap()
        .routes
        .iter()
        .flatten()
        .any(|b| b.handle == algo_ext));
    let root_ext = extension(&mut f.k, "root-new", "root", "algo", 1000);
    assert_eq!(f.ordinary(10), 1120);
    assert_eq!(f.retained(old, 10), 11);
    assert_eq!(f.retained(new, 10), 120);
    let latest = f.capture();
    assert_eq!(f.retained(latest, 10), 1120);
    assert!(f
        .k
        .pipeline_descriptor(latest)
        .unwrap()
        .routes
        .iter()
        .flatten()
        .any(|b| b.handle == root_ext));
    assert_eq!(
        f.state.borrow().history,
        vec![6, 11, 120, 11, 120, 1120, 11, 120, 1120]
    );
    assert_eq!(f.state.borrow().unrelated, "populated");
}

#[test]
fn shared_leases_detach_all_removal_paths_and_owner_references() {
    let mut f = Fixture::new(Limits::default());
    let e = extension(&mut f.k, "new", "leaf", "device", 10);
    let a = f.capture();
    let b = f.capture();
    let owner = f.k.retain_owner(&[e, e]).unwrap();
    let mut other = Fixture::new(Limits::default());
    let foreign = other.capture();
    let state = f.k.lifecycle_state();
    assert_eq!(state.references, 11); // two five-module leases and one deduplicated owner
    assert_eq!(f.k.release_pipeline(foreign), Err(Error::InvalidHandle));
    assert_eq!(
        f.k.invoke_pipeline(foreign, &[0; 4], auth(Access::Write)),
        Err(Error::InvalidHandle)
    );
    let foreign_owner = other.k.retain_owner(&[other.device]).unwrap();
    assert_eq!(f.k.release_owner(foreign_owner), Err(Error::InvalidHandle));
    assert_eq!(f.k.lifecycle_state(), state);
    for h in [f.root, f.algo, f.leaf, f.device, e] {
        assert_eq!(f.k.unregister(h), Err(Error::Referenced));
        assert_eq!(f.k.managed_unload(h), Err(Error::Referenced));
        assert_eq!(f.k.lifecycle_state(), state);
    }
    f.k.managed_detach(e).unwrap();
    assert_eq!(f.ordinary(10), 11);
    assert_eq!(f.retained(a, 10), 20);
    f.k.release_pipeline(a).unwrap();
    let state = f.k.lifecycle_state();
    assert_eq!(state.references, 6);
    assert_eq!(f.k.release_pipeline(a), Err(Error::InvalidHandle));
    assert_eq!(
        f.k.invoke_pipeline(a, &[0; 4], auth(Access::Write)),
        Err(Error::InvalidHandle)
    );
    assert_eq!(f.k.lifecycle_state(), state);
    assert_eq!(f.k.unregister(e), Err(Error::Referenced));
    assert_eq!(f.retained(b, 10), 20);
    f.k.release_pipeline(b).unwrap();
    assert_eq!(f.k.lifecycle_state().references, 1);
    assert_eq!(f.k.managed_unload(e), Err(Error::Referenced));
    f.k.release_owner(owner).unwrap();
    assert_eq!(f.k.lifecycle_state().references, 0);
    // Current dispatch is independently protected, even without pins.
    f.k.managed_attach(e).unwrap();
    let state = f.k.lifecycle_state();
    assert_eq!(f.k.managed_unload(e), Err(Error::Referenced));
    assert_eq!(f.k.lifecycle_state(), state);
    f.k.managed_detach(e).unwrap();
    f.k.managed_unload(e).unwrap();
    assert_eq!(f.k.managed_unload(f.device), Err(Error::Referenced));
    f.k.managed_unload(f.root).unwrap();
    f.k.managed_unload(f.algo).unwrap();
    f.k.managed_unload(f.leaf).unwrap();
    f.k.managed_unload(f.device).unwrap();
    assert_eq!(f.k.lifecycle_state().registrations, 0);
    let replacement =
        f.k.register_native(
            manifest("device", Access::Write),
            b"device-v1",
            BTreeMap::from([(
                "run".into(),
                Rc::new(|_: &[u8], _: &CallerView| Ok(vec![0u8; 4])) as NativePort,
            )]),
        )
        .unwrap();
    assert_ne!(replacement, f.device);
}

#[test]
fn retained_grants_ceilings_and_absence_never_freeze_or_widen_authority() {
    let mut f = Fixture::new(Limits::default());
    let p = f.capture();
    assert_eq!(f.retained(p, 10), 11);
    f.k.revoke_grant(f.leaf, 0).unwrap();
    let before = f.state.borrow().clone();
    assert_eq!(f.retained(p, 10), Error::Revoked.abi_code());
    assert_eq!(*f.state.borrow(), before);
    f.k.issue_grant(
        f.leaf,
        0,
        Authority {
            access: Access::Write,
            scope: Scope::Keys(BTreeSet::new()),
        },
    )
    .unwrap();
    assert_eq!(f.retained(p, 10), Error::Authority.abi_code());
    assert_eq!(*f.state.borrow(), before);
    f.k.issue_grant(f.leaf, 0, auth(Access::Read)).unwrap();
    assert_eq!(f.retained(p, 10), Error::Authority.abi_code());
    assert_eq!(*f.state.borrow(), before);
    f.k.issue_grant(f.leaf, 0, auth(Access::Write)).unwrap();
    assert_eq!(f.retained(p, 10), 11);
    assert_eq!(
        f.k.invoke_pipeline(
            p,
            &[0; 4],
            Authority {
                access: Access::Write,
                scope: Scope::All
            }
        ),
        Err(Error::Authority)
    );
    // A real Read ancestor cannot forward Write, despite every grant being live.
    let read = guest(&mut f.k, "read-root", "algo", Access::Read, 0);
    let r =
        f.k.capture_pipeline(read, "run", auth(Access::Write))
            .unwrap();
    let before = f.state.borrow().clone();
    assert_eq!(
        i32::from_le_bytes(
            f.k.invoke_pipeline(r, &[0; 4], auth(Access::Write))
                .unwrap()
                .try_into()
                .unwrap()
        ),
        Error::Authority.abi_code()
    );
    assert_eq!(*f.state.borrow(), before);
    assert_eq!(f.retained(p, 10), 11);
    // Capture before an issued grant: later issuance must not add authority.
    let mut m = manifest("ungranted", Access::Write);
    m.dependencies.push(dep("device", Access::Write));
    let u = f.k.register_guest(m, &forward(4, 0)).unwrap();
    let absent = f.k.capture_pipeline(u, "run", auth(Access::Write)).unwrap();
    f.k.issue_grant(u, 0, auth(Access::Write)).unwrap();
    let before = f.state.borrow().clone();
    assert_eq!(
        i32::from_le_bytes(
            f.k.invoke_pipeline(absent, &[0; 4], auth(Access::Write))
                .unwrap()
                .try_into()
                .unwrap()
        ),
        Error::MissingGrant.abi_code()
    );
    assert_eq!(*f.state.borrow(), before);
    assert_eq!(
        f.k.invoke(u, "run", &42u32.to_le_bytes(), auth(Access::Write))
            .unwrap(),
        42u32.to_le_bytes()
    );
    f.k.issue_grant(f.leaf, 0, auth(Access::Read)).unwrap();
    let narrow = f.capture();
    f.k.issue_grant(f.leaf, 0, auth(Access::Write)).unwrap();
    let before = f.state.borrow().clone();
    assert_eq!(f.retained(narrow, 10), Error::Authority.abi_code());
    assert_eq!(*f.state.borrow(), before);
    assert_eq!(f.retained(p, 10), 11);
}

#[test]
fn retained_nested_fuel_and_memory_limits_have_live_successful_controls() {
    let mut f = Fixture::new(Limits {
        // Diagnostic 20-call baseline: cold small=3161, warm heavy=13145.
        // Controls below verify this budget with explicit ABI refusal
        // propagation; each leaf70 also fits independently.
        fuel: 8153,
        ..Limits::default()
    });
    let burn = r#"local.get $p i32.load local.set $r
        (block $done (loop $burn local.get $r i32.eqz br_if $done
        local.get $r i32.const 1 i32.sub local.set $r br $burn))
        i32.const 256 local.get $p i32.load i32.store
        i64.const 1099511627780"#;
    let mut m = manifest("burn-leaf", Access::Write);
    m.dependencies.push(dep("device", Access::Write));
    let leaf = f.k.register_guest(m, &wasm(burn)).unwrap();
    f.k.issue_grant(leaf, 0, auth(Access::Write)).unwrap();
    let mut m = manifest("burn-root", Access::Write);
    m.dependencies.push(dep("burn-leaf", Access::Write));
    let call = "i32.const 0 local.get $p local.get $n i32.const 256 i32.const 4 call $call i32.const 0 i32.lt_s if unreachable end ";
    let body = call.repeat(20) + "i64.const 1099511627780";
    let root = f.k.register_guest(m, &wasm(&body)).unwrap();
    f.k.issue_grant(root, 0, auth(Access::Write)).unwrap();
    let control =
        f.k.capture_pipeline(leaf, "run", auth(Access::Write))
            .unwrap();
    assert_eq!(
        f.k.invoke_pipeline(control, &70u32.to_le_bytes(), auth(Access::Write))
            .unwrap(),
        70u32.to_le_bytes()
    );
    let p =
        f.k.capture_pipeline(root, "run", auth(Access::Write))
            .unwrap();
    let before = f.state.borrow().clone();
    assert_eq!(
        f.k.invoke_pipeline(p, &1u32.to_le_bytes(), auth(Access::Write))
            .unwrap(),
        1u32.to_le_bytes()
    );
    assert_eq!(*f.state.borrow(), before);
    assert_eq!(
        f.k.invoke_pipeline(p, &70u32.to_le_bytes(), auth(Access::Write)),
        Err(Error::Trap)
    );
    assert_eq!(*f.state.borrow(), before);
    assert_eq!(
        f.k.invoke_pipeline(p, &1u32.to_le_bytes(), auth(Access::Write))
            .unwrap(),
        1u32.to_le_bytes()
    );
    let mut f = Fixture::new(Limits {
        memory_bytes: 65536,
        ..Limits::default()
    });
    for (id, pages) in [("grow-control", 0), ("grow-refusal", 1)] {
        let body = format!(
            "i32.const {pages} memory.grow i32.const -1 i32.eq if unreachable end i64.const 0"
        );
        let h =
            f.k.register_guest(manifest(id, Access::Write), &wasm(&body))
                .unwrap();
        let p = f.k.capture_pipeline(h, "run", auth(Access::Write)).unwrap();
        let before = f.state.borrow().clone();
        assert_eq!(
            f.k.invoke_pipeline(p, &[], auth(Access::Write)),
            if pages == 0 {
                Ok(vec![])
            } else {
                Err(Error::Trap)
            }
        );
        assert_eq!(*f.state.borrow(), before);
    }
    assert_eq!(f.ordinary(10), 11);
}

#[test]
fn retained_budget_abi_preflight_and_committed_prefix() {
    for limits in [
        Limits {
            calls: 3,
            ..Limits::default()
        },
        Limits {
            depth: 3,
            ..Limits::default()
        },
    ] {
        let mut f = Fixture::new(limits);
        // Short valid live path supplies a successful control under these bounds.
        let direct = guest(&mut f.k, "direct", "device", Access::Write, 0);
        let d =
            f.k.capture_pipeline(direct, "run", auth(Access::Write))
                .unwrap();
        assert_eq!(
            f.k.invoke_pipeline(d, &7u32.to_le_bytes(), auth(Access::Write))
                .unwrap(),
            7u32.to_le_bytes()
        );
        if f.k.capture_pipeline(f.root, "run", auth(Access::Write)) == Err(Error::Limit) {
            continue;
        }
        let p = f.capture();
        let before = f.state.borrow().clone();
        assert_eq!(f.retained(p, 10), Error::Limit.abi_code());
        assert_eq!(*f.state.borrow(), before);
    }
    let mut f = Fixture::new(Limits::default());
    let mut m = manifest("small", Access::Write);
    m.dependencies.push(dep("device", Access::Write));
    let small = f.k.register_guest(m, &forward(3, 0)).unwrap();
    f.k.issue_grant(small, 0, auth(Access::Write)).unwrap();
    let p =
        f.k.capture_pipeline(small, "run", auth(Access::Write))
            .unwrap();
    let before = f.state.borrow().clone();
    assert_eq!(
        i32::from_le_bytes(
            f.k.invoke_pipeline(p, &[0; 4], auth(Access::Write))
                .unwrap()
                .try_into()
                .unwrap()
        ),
        Error::Buffer.abi_code()
    );
    assert_eq!(*f.state.borrow(), before);
    assert_eq!(f.ordinary(10), 11);
    let mut m = manifest("prefix", Access::Write);
    m.dependencies.push(dep("device", Access::Write));
    let body = "i32.const 0 local.get $p local.get $n i32.const 256 i32.const 4 call $call drop unreachable";
    let prefix = f.k.register_guest(m, &wasm(body)).unwrap();
    f.k.issue_grant(prefix, 0, auth(Access::Write)).unwrap();
    let p =
        f.k.capture_pipeline(prefix, "run", auth(Access::Write))
            .unwrap();
    assert_eq!(
        f.k.invoke_pipeline(p, &77u32.to_le_bytes(), auth(Access::Write)),
        Err(Error::Trap)
    );
    assert_eq!(f.state.borrow().value, 77);
    let mut m = manifest("spin", Access::Write);
    m.dependencies.push(dep("device", Access::Write));
    let spin =
        f.k.register_guest(m, &wasm("(loop $l br $l) i64.const 0"))
            .unwrap();
    f.k.issue_grant(spin, 0, auth(Access::Write)).unwrap();
    let p =
        f.k.capture_pipeline(spin, "run", auth(Access::Write))
            .unwrap();
    let before = f.state.borrow().clone();
    assert_eq!(
        f.k.invoke_pipeline(p, &[0; 4], auth(Access::Write)),
        Err(Error::Trap)
    );
    assert_eq!(*f.state.borrow(), before);
}

#[test]
fn bounded_capture_owner_sets_and_cycles_are_atomic() {
    for limits in [
        RetentionLimits {
            nodes: 3,
            ..RetentionLimits::default()
        },
        RetentionLimits {
            slots: 2,
            ..RetentionLimits::default()
        },
        RetentionLimits {
            references: 3,
            ..RetentionLimits::default()
        },
    ] {
        let mut f = Fixture::new(Limits::default());
        f.k.set_retention_limits(limits).unwrap();
        let before = f.k.lifecycle_state();
        assert_eq!(
            f.k.capture_pipeline(f.root, "run", auth(Access::Write)),
            Err(Error::Limit)
        );
        assert_eq!(f.k.lifecycle_state(), before);
        let p =
            f.k.capture_pipeline(f.device, "run", auth(Access::Write))
                .unwrap();
        assert_eq!(f.retained(p, 10), 10);
        f.k.release_pipeline(p).unwrap();
    }
    let mut f = Fixture::new(Limits::default());
    f.k.set_retention_limits(RetentionLimits {
        leases: 1,
        owners: 1,
        references: 5,
        ..RetentionLimits::default()
    })
    .unwrap();
    let p = f.capture();
    let before = f.k.lifecycle_state();
    assert_eq!(
        f.k.capture_pipeline(f.root, "run", auth(Access::Write)),
        Err(Error::Limit)
    );
    assert_eq!(f.k.lifecycle_state(), before);
    let o = f.k.retain_owner(&[f.device]).unwrap();
    let before = f.k.lifecycle_state();
    assert_eq!(f.k.retain_owner(&[f.leaf]), Err(Error::Limit));
    assert_eq!(f.k.lifecycle_state(), before);
    f.k.release_owner(o).unwrap();
    f.k.release_pipeline(p).unwrap();
    let before = f.k.lifecycle_state();
    assert_eq!(f.k.retain_owner(&[]), Err(Error::Limit));
    assert_eq!(f.k.lifecycle_state(), before);
    let mut other = Fixture::new(Limits::default());
    let _control = other.k.retain_owner(&[other.device]).unwrap();
    assert_eq!(f.k.retain_owner(&[other.device]), Err(Error::InvalidHandle));
    assert_eq!(f.k.lifecycle_state(), before);
    let mut m = manifest("cycle", Access::Write);
    m.dependencies.push(dep("cycle", Access::Write));
    let h = f.k.register_guest(m, &forward(4, 0)).unwrap();
    f.k.issue_grant(h, 0, auth(Access::Write)).unwrap();
    let before = f.k.lifecycle_state();
    assert_eq!(
        f.k.capture_pipeline(h, "run", auth(Access::Write)),
        Err(Error::Cycle)
    );
    assert_eq!(f.k.lifecycle_state(), before);
    let p = f.capture();
    assert_eq!(f.retained(p, 10), 11);
}

#[test]
fn reference_capacity_and_failed_managed_attachment_keep_valid_controls() {
    let mut f = Fixture::new(Limits::default());
    f.k.set_retention_limits(RetentionLimits {
        nodes: 4,
        references: 4,
        ..RetentionLimits::default()
    })
    .unwrap();
    let p = f.capture();
    let before = f.k.lifecycle_state();
    assert_eq!(f.k.retain_owner(&[f.device]), Err(Error::Limit));
    assert_eq!(f.k.lifecycle_state(), before);
    assert_eq!(f.k.retain_owner(&[f.device; 5]), Err(Error::Limit));
    assert_eq!(f.k.lifecycle_state(), before);
    assert_eq!(f.retained(p, 10), 11);
    f.k.release_pipeline(p).unwrap();
    let owner = f.k.retain_owner(&[f.device]).unwrap();
    f.k.release_owner(owner).unwrap();
    let e = extension(&mut f.k, "new", "leaf", "device", 10);
    let before = f.k.lifecycle_state();
    assert_eq!(f.k.managed_attach(e), Err(Error::Duplicate));
    assert_eq!(f.k.lifecycle_state(), before);
    assert_eq!(f.ordinary(10), 20);
    f.k.managed_detach(e).unwrap();
    let before = f.k.lifecycle_state();
    assert_eq!(f.k.managed_detach(e), Err(Error::MissingDependency));
    assert_eq!(f.k.lifecycle_state(), before);
    f.k.managed_attach(e).unwrap();
    assert_eq!(f.ordinary(10), 20);
    f.k.managed_detach(e).unwrap();
    // Legacy removal is compatible only when unpinned; reattach cannot adopt
    // the replacement's otherwise valid, live handle with the same module ID.
    f.k.unregister(f.leaf).unwrap();
    let replacement = guest(&mut f.k, "leaf", "device", Access::Write, 2);
    assert_ne!(replacement, f.leaf);
    let before = f.k.lifecycle_state();
    assert_eq!(f.k.managed_attach(e), Err(Error::InvalidHandle));
    assert_eq!(f.k.lifecycle_state(), before);
    assert_eq!(
        f.k.invoke(
            replacement,
            "run",
            &10u32.to_le_bytes(),
            auth(Access::Write)
        )
        .unwrap(),
        12u32.to_le_bytes()
    );
    assert_eq!(f.k.managed_unload(replacement), Err(Error::Referenced));
}
