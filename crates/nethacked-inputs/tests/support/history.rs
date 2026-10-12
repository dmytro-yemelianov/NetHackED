use super::*;
use crate::support::{authority, cap, forward, manifest, replay, resource, sample};
use nethacked_kernel::{persistence::*, Authority};
use std::cell::Cell;

const LIMIT: usize = 1_048_576;
struct Approved {
    native_manifest: Manifest,
    native_identity: Vec<u8>,
    port: NativePort,
    root_manifest: Manifest,
    root_bytes: Vec<u8>,
    contexts: Vec<SavedOperationContext>,
    denied: bool,
}
impl RegistryAuthority for Approved {
    fn resolve(&self, m: &SavedModule) -> Result<ArtifactMaterial> {
        match m.manifest.id.as_str() {
            "inputs" if m.manifest == self.native_manifest => Ok(ArtifactMaterial::Native {
                manifest: self.native_manifest.clone(),
                identity: self.native_identity.clone(),
                ports: BTreeMap::from([(ENDPOINT.into(), self.port.clone())]),
            }),
            "root" if m.manifest == self.root_manifest => Ok(ArtifactMaterial::Guest {
                manifest: self.root_manifest.clone(),
                bytes: self.root_bytes.clone(),
            }),
            _ => Err(Error::Authority),
        }
    }
    fn bind_resource(&self, r: &Resource) -> Result<Resource> {
        if *r == resource() {
            Ok(r.clone())
        } else {
            Err(Error::Authority)
        }
    }
    fn approve_grant(
        &self,
        _: &SavedModule,
        _: usize,
        _: &SavedModule,
        a: &Authority,
    ) -> Result<Authority> {
        if !self.denied && *a == authority() {
            Ok(a.clone())
        } else {
            Err(Error::Authority)
        }
    }
}
impl RetainedAuthority for Approved {
    fn approve_cleanup(&self, _: u64) -> Result<()> {
        Err(Error::Authority)
    }
    fn approve_operation_context(&self, c: &SavedOperationContext) -> Result<()> {
        if !self.denied && self.contexts.contains(c) {
            Ok(())
        } else {
            Err(Error::Authority)
        }
    }
}
struct Fixture {
    kernel: Kernel,
    service: Service,
    sampled: Rc<Cell<usize>>,
    observations: Rc<RefCell<Vec<Observation>>>,
    native_manifest: Manifest,
    native_identity: Vec<u8>,
    root_manifest: Manifest,
    root_bytes: Vec<u8>,
}
fn fixture() -> Fixture {
    fixture_with_extension(false)
}
fn fixture_with_extension(extension: bool) -> Fixture {
    let sampled = Rc::new(Cell::new(0));
    let calls = sampled.clone();
    let service = Service::new(
        Limits::default(),
        Policy {
            identity: "queue".into(),
            version: 1,
            transition: Rc::new(move |s, r| {
                calls.set(calls.get() + 1);
                assert_eq!(r, b"choice");
                Ok((s[1..].to_vec(), vec![s[0]]))
            }),
        },
        vec![10, 20, 30],
    )
    .unwrap();
    let observations = Rc::new(RefCell::new(vec![]));
    let saved = observations.clone();
    let s = service.clone();
    let port: NativePort = Rc::new(move |bytes, view| {
        let reply = s.dispatch(bytes, view)?;
        let Reply { id, .. } = serde_json::from_slice(&reply).unwrap();
        saved.borrow_mut().push(s.observation(id, b"choice", view)?);
        Ok(reply)
    });
    let mut kernel = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
    let native_manifest = Manifest {
        id: "inputs".into(),
        exports: BTreeMap::from([(
            ENDPOINT.into(),
            Endpoint {
                major: MAJOR,
                access: Access::Write,
                max_request: 16384,
                max_reply: 16384,
            },
        )]),
        dependencies: vec![],
        extension_points: if extension {
            BTreeSet::from([ENDPOINT.into()])
        } else {
            BTreeSet::new()
        },
        extension: None,
    };
    let native_identity = b"approved-input-test-v1".to_vec();
    let provider = kernel
        .register_native(
            native_manifest.clone(),
            &native_identity,
            BTreeMap::from([(ENDPOINT.into(), port)]),
        )
        .unwrap();
    service.0.borrow_mut().binding = Some((
        provider,
        kernel.artifact(provider).unwrap(),
        cap(),
        resource(),
    ));
    let root_manifest = manifest("root");
    let root_bytes = forward("");
    let root = kernel
        .register_guest(root_manifest.clone(), &root_bytes)
        .unwrap();
    kernel.issue_grant(root, 0, authority()).unwrap();
    let (op, lease) = kernel.capture_operation(root, "step", authority()).unwrap();
    let operation = kernel.operation_binding(op, lease, 3, "choice").unwrap();
    let out = kernel
        .invoke_operation(
            &operation,
            &encode(&sample(b"choice")).unwrap(),
            authority(),
        )
        .unwrap();
    assert_eq!(serde_json::from_slice::<Reply>(&out).unwrap().value, [10]);
    Fixture {
        kernel,
        service,
        sampled,
        observations,
        native_manifest,
        native_identity,
        root_manifest,
        root_bytes,
    }
}
fn capture_history(
    kernel: &mut Kernel,
    service: &Service,
    observations: &[&Observation],
) -> (KernelSnapshot, Checkpoint, Vec<ObservationId>) {
    let operations = service.persistence_operations_for(kernel).unwrap();
    let k = kernel
        .kernel_snapshot_with_service_roots(
            &KernelEnrollment {
                operations: &operations,
                pure_captures: &[],
            },
            &service.persistence_cleanups().unwrap(),
            &service.persistence_resources().unwrap(),
        )
        .unwrap();
    let index = kernel.execution_capture_index(&k).unwrap();
    let (c, ids) = service.capture_persistence(&index, observations).unwrap();
    c.validate_checkpoint(&Limits::default(), &k).unwrap();
    (k, c, ids)
}
fn approved(f: &Fixture, k: &KernelSnapshot, port: NativePort) -> Approved {
    Approved {
        native_manifest: f.native_manifest.clone(),
        native_identity: f.native_identity.clone(),
        port,
        root_manifest: f.root_manifest.clone(),
        root_bytes: f.root_bytes.clone(),
        contexts: k.operation_contexts().unwrap(),
        denied: false,
    }
}
fn dispose(f: &mut Fixture, unload: bool) {
    let record = f.service.inspect().unwrap().records[0].clone();
    f.observations.borrow_mut().clear();
    f.kernel
        .release_pipeline(record.operation.pipeline())
        .unwrap();
    if unload {
        f.kernel.unregister(record.caller).unwrap();
    }
}
fn load(
    k: &KernelSnapshot,
    c: Checkpoint,
    mut a: Approved,
    calls: Rc<Cell<usize>>,
    factories: Rc<Cell<usize>>,
) -> (Kernel, RestoredKernel, Restored) {
    let build = Restore::prepare(c, Limits::default(), k, &catalog(calls, factories)).unwrap();
    a.port = build.port();
    let mut kernel = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
    let mapped = kernel.restore_kernel(k, &a).unwrap();
    let restored = build.finish(&kernel, &mapped).unwrap();
    (kernel, mapped, restored)
}
fn catalog(calls: Rc<Cell<usize>>, factories: Rc<Cell<usize>>) -> Catalog {
    let mut c = Catalog::default();
    c.allow("queue".into(), 1, move || {
        factories.set(factories.get() + 1);
        let calls = calls.clone();
        Ok(Policy {
            identity: "queue".into(),
            version: 1,
            transition: Rc::new(move |s, _| {
                calls.set(calls.get() + 1);
                Ok((s[1..].to_vec(), vec![s[0]]))
            }),
        })
    })
    .unwrap();
    c
}

#[test]
fn disposed_unloaded_history_bytes_preserve_cursor_and_refuse_fresh_replay() {
    let mut f = fixture();
    assert_eq!(f.sampled.get(), 1);
    let original = f.service.inspect().unwrap();
    dispose(&mut f, true);
    let (k, c, ids) = capture_history(&mut f.kernel, &f.service, &[]);
    assert!(ids.is_empty());
    assert!(c.records[0].historical);
    assert!(k.operation_contexts().unwrap().is_empty());
    assert!(!k
        .registry
        .modules
        .iter()
        .any(|m| m.id == c.records[0].caller));
    let c = Checkpoint::from_bytes(&c.to_bytes(LIMIT).unwrap(), LIMIT).unwrap();
    let a = approved(&f, &k, Rc::new(|_, _| Err(Error::Authority)));
    let calls = Rc::new(Cell::new(0));
    let factories = Rc::new(Cell::new(0));
    drop(f);
    let (mut kernel, mapped, restored) = load(
        &k,
        c.clone(),
        a.clone_for_test(),
        calls.clone(),
        factories.clone(),
    );
    let before = restored.service().inspect().unwrap();
    assert_eq!(before.source, original.source);
    assert_eq!(before.next_serial, 2);
    assert_eq!(before.records[0].operation.operation().namespace(), 0);
    assert_eq!(
        before.records[0].operation.operation().serial(),
        original.records[0].operation.operation().serial()
    );
    assert_eq!(before.records[0].operation.checkpoint(), 3);
    assert_eq!(before.records[0].operation.phase(), "choice");
    assert_eq!(before.records[0].request, original.records[0].request);
    assert_eq!(before.records[0].value, [10]);
    assert!(restored.observation(ObservationId(0)).is_err());
    assert!(kernel
        .invoke_operation(
            &before.records[0].operation,
            &encode(&sample(b"choice")).unwrap(),
            authority()
        )
        .is_err());
    let root = kernel
        .register_guest(a.root_manifest.clone(), &a.root_bytes)
        .unwrap();
    assert!(root != before.records[0].caller);
    kernel.issue_grant(root, 0, authority()).unwrap();
    let (op, lease) = kernel.capture_operation(root, "step", authority()).unwrap();
    assert!(op.serial() >= k.registry.next_operation);
    let binding = kernel.operation_binding(op, lease, 0, "fresh").unwrap();
    for id in [original.records[0].id, restored.record(1).unwrap()] {
        let out = kernel
            .invoke_operation(
                &binding,
                &encode(&Request::Replay {
                    id,
                    request: b"choice".to_vec(),
                })
                .unwrap(),
                authority(),
            )
            .unwrap();
        assert!(i32::from_le_bytes(out.try_into().unwrap()) < 0);
    }
    assert_eq!(calls.get(), 0);
    assert_eq!(restored.service().inspect().unwrap(), before);
    let out = kernel
        .invoke_operation(&binding, &encode(&sample(b"choice")).unwrap(), authority())
        .unwrap();
    let reply: Reply = serde_json::from_slice(&out).unwrap();
    assert_eq!(reply.value, [20]);
    assert_eq!(reply.id.serial, 2);
    assert_eq!(calls.get(), 1);
    assert_eq!(factories.get(), 1);
    assert!(restored.matches_destination(&mapped.execution_destination()));
    let (k2, c2, _) = capture_history(&mut kernel, restored.service(), &[]);
    assert!(c2.records[0].historical);
    assert!(!c2.records[1].historical);
    assert_eq!(c2.records[0].operation, c.records[0].operation);
    assert_eq!(c2.next_serial, 3);
    assert_eq!(c2.source, [30]);
    assert_eq!(c2.next_ordinals.len(), 2);
    assert!(k2.registry.next_operation > k.registry.next_operation);
}
impl Approved {
    fn clone_for_test(&self) -> Self {
        Self {
            native_manifest: self.native_manifest.clone(),
            native_identity: self.native_identity.clone(),
            port: self.port.clone(),
            root_manifest: self.root_manifest.clone(),
            root_bytes: self.root_bytes.clone(),
            contexts: self.contexts.clone(),
            denied: self.denied,
        }
    }
}
#[test]
fn mixed_history_live_observation_replay_and_counter_preflight() {
    let mut f = fixture();
    dispose(&mut f, false);
    let caller = f.service.inspect().unwrap().records[0].caller;
    let (op, lease) = f
        .kernel
        .capture_operation(caller, "step", authority())
        .unwrap();
    let binding = f
        .kernel
        .operation_binding(op, lease, 8, "retained")
        .unwrap();
    f.kernel
        .invoke_operation(&binding, &encode(&sample(b"choice")).unwrap(), authority())
        .unwrap();
    let obs = f.observations.borrow()[0].clone();
    let (k, c, ids) = capture_history(&mut f.kernel, &f.service, &[&obs]);
    assert!(c.records[0].historical);
    assert!(!c.records[1].historical);
    let calls = Rc::new(Cell::new(0));
    let factories = Rc::new(Cell::new(0));
    let catalog = catalog(calls.clone(), factories.clone());
    for case in 0..8 {
        let mut bad = c.clone();
        match case {
            0 => bad.records[0].operation.operation = k.registry.next_operation,
            1 => bad.records[0].caller = k.registry.next_generation,
            2 => bad.records[0].operation.pipeline = k.registry.retention_counters[0],
            3 => bad.next_serial += 1,
            4 => {
                bad.next_ordinals
                    .insert(bad.records[0].operation.operation, 7);
            }
            5 => bad.observations[0].serial = 1,
            6 => bad.records[0].permit.resolution[0].artifact = [0; 32],
            _ => bad.records[1].historical = true,
        }
        assert!(
            Restore::prepare(bad, Limits::default(), &k, &catalog).is_err(),
            "case {case}"
        );
    }
    assert!(Restore::prepare(c.clone(), Limits::default(), &k, &Catalog::default()).is_err());
    assert_eq!(factories.get(), 0);
    let a = approved(&f, &k, Rc::new(|_, _| Err(Error::Authority)));
    drop(obs);
    drop(f);
    let (kernel, _, restored) = load(&k, c, a, calls.clone(), factories);
    let observation = restored.observation(ids[0]).unwrap();
    assert_eq!(observation.bytes(), [20]);
    let state = restored.service().inspect().unwrap();
    let reply = Reply {
        id: restored.record(2).unwrap(),
        value: vec![20],
    };
    let out = kernel
        .invoke_operation(
            &state.records[1].operation,
            &encode(&replay(&reply, b"choice")).unwrap(),
            authority(),
        )
        .unwrap();
    assert_eq!(serde_json::from_slice::<Reply>(&out).unwrap(), reply);
    assert_eq!(calls.get(), 0);
    assert_eq!(state.next_serial, 3);
    assert_eq!(state.next_ordinals.len(), 2);
}
#[test]
fn empty_service_resource_root_and_busy_capture() {
    let mut f = fixture();
    dispose(&mut f, true);
    // Separate empty allocation installed through the actual public installer.
    let service = Service::new(
        Limits::default(),
        Policy {
            identity: "queue".into(),
            version: 1,
            transition: Rc::new(|_, _| panic!("capture must not sample")),
        },
        vec![42],
    )
    .unwrap();
    let mut kernel = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
    service.install(&mut kernel, "inputs").unwrap();
    let (k, c, _) = capture_history(&mut kernel, &service, &[]);
    assert!(c.records.is_empty());
    assert_eq!(c.source, [42]);
    assert_eq!(c.next_serial, 1);
    assert!(c.next_ordinals.is_empty());
    let guard = service.0.borrow_mut();
    let index = kernel.execution_capture_index(&k).unwrap();
    assert!(service.capture_persistence(&index, &[]).is_err());
    drop(guard);
    let calls = Rc::new(Cell::new(0));
    let factories = Rc::new(Cell::new(0));
    for case in 0..3 {
        let mut missing = k.clone();
        match case {
            0 => missing.service_resources.clear(),
            1 => missing.registry.next_generation = 1,
            _ => missing.registry.retention_counters[0] = 0,
        }
        assert!(Restore::prepare(
            c.clone(),
            Limits::default(),
            &missing,
            &catalog(calls.clone(), factories.clone())
        )
        .is_err());
    }
    assert_eq!(factories.get(), 0);
    let mut a = approved(&f, &k, Rc::new(|_, _| Err(Error::Authority)));
    a.native_manifest = k.registry.modules[0].manifest.clone();
    a.native_identity = encode(&(
        "nethacked-inputs-v1",
        env!("CARGO_PKG_VERSION"),
        "queue",
        1u32,
        &Limits::default(),
    ))
    .unwrap();
    drop(f);
    drop(service);
    drop(kernel);
    let (mut kernel, _, restored) = load(
        &k,
        Checkpoint::from_bytes(&c.to_bytes(LIMIT).unwrap(), LIMIT).unwrap(),
        a.clone_for_test(),
        calls.clone(),
        factories.clone(),
    );
    assert_eq!(calls.get(), 0);
    assert_eq!(restored.service().inspect().unwrap().source, [42]);
    let root = kernel
        .register_guest(a.root_manifest, &a.root_bytes)
        .unwrap();
    kernel.issue_grant(root, 0, authority()).unwrap();
    let (op, lease) = kernel.capture_operation(root, "step", authority()).unwrap();
    let binding = kernel.operation_binding(op, lease, 0, "fresh").unwrap();
    let out = kernel
        .invoke_operation(&binding, &encode(&sample(b"choice")).unwrap(), authority())
        .unwrap();
    assert_eq!(serde_json::from_slice::<Reply>(&out).unwrap().value, [42]);
    assert_eq!(calls.get(), 1);
    assert_eq!(factories.get(), 1);
}
#[test]
fn repeated_source_free_processes_preserve_holes_and_nonreuse() {
    use std::process::Command;
    let base = std::env::temp_dir().join(format!("input-history-process-{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    for stage in ["save", "load", "load"] {
        let out = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "persistence::history_tests::history_process_worker",
                "--nocapture",
            ])
            .env("INPUT_HISTORY_PROCESS", stage)
            .env("INPUT_HISTORY_DIRECTORY", &base)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    std::fs::remove_dir_all(base).unwrap();
}
#[test]
fn history_process_worker() {
    let Ok(stage) = std::env::var("INPUT_HISTORY_PROCESS") else {
        return;
    };
    let base = std::path::PathBuf::from(std::env::var("INPUT_HISTORY_DIRECTORY").unwrap());
    let mut f = fixture();
    // Allocation holes not represented by ledger records must survive in kernel.
    for _ in 0..3 {
        f.kernel.issue_operation().unwrap();
    }
    dispose(&mut f, true);
    if stage == "save" {
        let (k, c, _) = capture_history(&mut f.kernel, &f.service, &[]);
        std::fs::write(base.join("kernel"), encode(&k).unwrap()).unwrap();
        std::fs::write(base.join("input"), c.to_bytes(LIMIT).unwrap()).unwrap();
        return;
    }
    let k = serde_json::from_slice(&std::fs::read(base.join("kernel")).unwrap()).unwrap();
    let c = Checkpoint::from_bytes(&std::fs::read(base.join("input")).unwrap(), LIMIT).unwrap();
    let a = approved(&f, &k, Rc::new(|_, _| Err(Error::Authority)));
    drop(f);
    let calls = Rc::new(Cell::new(0));
    let (mut kernel, _, restored) = load(
        &k,
        c.clone(),
        a.clone_for_test(),
        calls.clone(),
        Rc::new(Cell::new(0)),
    );
    assert_eq!(calls.get(), 0);
    let root = kernel
        .register_guest(a.root_manifest, &a.root_bytes)
        .unwrap();
    kernel.issue_grant(root, 0, authority()).unwrap();
    let (op, lease) = kernel.capture_operation(root, "step", authority()).unwrap();
    assert!(op.serial() >= k.registry.next_operation);
    let binding = kernel.operation_binding(op, lease, 0, "fresh").unwrap();
    for id in [
        RecordId {
            namespace: c.namespace,
            serial: 1,
        },
        restored.record(1).unwrap(),
    ] {
        let out = kernel
            .invoke_operation(
                &binding,
                &encode(&Request::Replay {
                    id,
                    request: b"choice".to_vec(),
                })
                .unwrap(),
                authority(),
            )
            .unwrap();
        assert!(i32::from_le_bytes(out.try_into().unwrap()) < 0);
    }
    assert_eq!(calls.get(), 0);
    let before = restored.service().inspect().unwrap();
    let out = kernel
        .invoke_operation(&binding, &encode(&sample(b"choice")).unwrap(), authority())
        .unwrap();
    let reply: Reply = serde_json::from_slice(&out).unwrap();
    assert_eq!(reply.id.serial, before.next_serial);
    assert_eq!(reply.value, [before.source[0]]);
    assert_eq!(calls.get(), 1);
    kernel.release_pipeline(lease).unwrap();
    kernel.unregister(root).unwrap();
    let (k2, c2, _) = capture_history(&mut kernel, restored.service(), &[]);
    assert!(c2.records.iter().all(|r| r.historical));
    assert_eq!(c2.next_serial, c.next_serial + 1);
    assert_eq!(c2.records[0].operation, c.records[0].operation);
    std::fs::write(base.join("kernel"), encode(&k2).unwrap()).unwrap();
    std::fs::write(base.join("input"), c2.to_bytes(LIMIT).unwrap()).unwrap();
}

use crate::support::{escaped, WIRE};
use nethacked_workflows::{Fault, Outcome, Program, Status, Workflows};
fn program(provider: ModuleHandle) -> Program {
    Program {
        provider,
        endpoint: "step".into(),
        authority: authority(),
        initial: "select".into(),
        phases: BTreeSet::from(["select".into(), "reply".into()]),
        allowed: BTreeSet::from([("select".into(), "reply".into())]),
    }
}
// Current fixture pattern: fresh guest memory, phase search, real kernel imports.
// Host StepRequest.input carries opaque replay bytes, never operation credentials.
fn guest(version: &str, fault: &str, replay_value: u8) -> Vec<u8> {
    let mut data = String::new();
    let mut offset = 8000;
    let mut store = |bytes: &[u8]| {
        let p = offset;
        offset += bytes.len() + 16;
        data.push_str(&format!("(data (i32.const {p}) \"{}\")", escaped(bytes)));
        (p, bytes.len())
    };
    let (select, sn) = store(b"\"select\"");
    let (reply, rn) = store(b"\"reply\"");
    let (input, inn) = store(b"\"input\":[");
    let (replay_marker, replay_marker_n) = store(b"\"kind\":\"replay\"");
    let (sp, sl) = store(&serde_json::to_vec(&sample(b"choice")).unwrap());
    let (vp, vl) = store(format!("\"value\":[{replay_value}]").as_bytes());
    let suspend = Outcome::Suspend {
        next: "reply".into(),
        prompt: b"choose".to_vec(),
        state: vec![],
        events: vec![version.as_bytes().to_vec()],
    };
    let (op, on) = store(
        if fault == "malformed" {
            b"invalid".to_vec()
        } else if fault == "finish" {
            serde_json::to_vec(&Outcome::Finish {
                result: vec![],
                state: vec![],
                events: vec![],
            })
            .unwrap()
        } else if fault == "reject" {
            serde_json::to_vec(&Outcome::Reject { events: vec![] }).unwrap()
        } else {
            serde_json::to_vec(&suspend).unwrap()
        }
        .as_slice(),
    );
    let (fp, fl) = store(
        &serde_json::to_vec(&Outcome::Finish {
            result: version.as_bytes().to_vec(),
            state: vec![],
            events: vec![],
        })
        .unwrap(),
    );
    let trap = if fault == "trap" { "unreachable" } else { "" };
    let theft = if version == "new" {
        format!(
            "local.get $n call $decode local.set $len
        i32.const 0 i32.const 20000 local.get $len i32.const 32768 i32.const {WIRE} call $call
        i32.const {} i32.ne if unreachable end",
            nethacked_kernel::Error::Authority.abi_code()
        )
    } else {
        String::new()
    };
    wat::parse_str(format!(r#"(module
      (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (func (export "alloc") (param i32) (result i32) i32.const 0)
      {data}
      (func $find (param $base i32) (param $n i32) (param $p i32) (param $l i32) (result i32)
        (local $i i32) (local $j i32)
        block $no loop $outer
          local.get $i local.get $l i32.add local.get $n i32.gt_u br_if $no
          i32.const 0 local.set $j
          block $mismatch loop $inner
            local.get $j local.get $l i32.eq if local.get $base local.get $i i32.add return end
            local.get $base local.get $i i32.add local.get $j i32.add i32.load8_u
            local.get $p local.get $j i32.add i32.load8_u i32.ne br_if $mismatch
            local.get $j i32.const 1 i32.add local.set $j br $inner
          end end local.get $i i32.const 1 i32.add local.set $i br $outer
        end end i32.const -1)
      (func $decode (param $n i32) (result i32) (local $p i32) (local $len i32) (local $v i32) (local $c i32)
        i32.const 0 local.get $n i32.const {input} i32.const {inn} call $find
        local.tee $p i32.const 0 i32.lt_s if unreachable end
        local.get $p i32.const {inn} i32.add local.set $p
        block $done loop $read
          local.get $p local.get $n i32.ge_u if unreachable end
          local.get $p i32.load8_u local.tee $c i32.const 93 i32.eq br_if $done
          local.get $c i32.const 44 i32.eq if
            i32.const 20000 local.get $len i32.add local.get $v i32.store8
            local.get $len i32.const 1 i32.add local.set $len i32.const 0 local.set $v
          else
            local.get $v i32.const 10 i32.mul local.get $c i32.const 48 i32.sub i32.add local.set $v
          end local.get $p i32.const 1 i32.add local.set $p br $read
        end end
        i32.const 20000 local.get $len i32.add local.get $v i32.store8
        local.get $len i32.const 1 i32.add)
      (func $sample
        i32.const 0 i32.const {sp} i32.const {sl} i32.const 32768 i32.const {WIRE} call $call
        i32.const 0 i32.lt_s if unreachable end)
      (func (export "step") (param i32) (param $n i32) (result i64) (local $len i32) (local $r i32)
        i32.const 0 local.get $n i32.const {replay_marker} i32.const {replay_marker_n} call $find i32.const 0 i32.ge_s if
          i32.const 0 i32.const 0 local.get $n i32.const 32768 i32.const {WIRE} call $call local.tee $r
          i32.const 0 i32.lt_s if unreachable end
          i64.const 140737488355328 local.get $r i64.extend_i32_u i64.or return end
        i32.const 0 local.get $n i32.const {select} i32.const {sn} call $find i32.const 0 i32.ge_s if
          {theft} call $sample {trap} i64.const {} return end
        i32.const 0 local.get $n i32.const {reply} i32.const {rn} call $find i32.const 0 i32.ge_s if
          local.get $n call $decode local.set $len
          i32.const 0 i32.const 20000 local.get $len i32.const 32768 i32.const {WIRE} call $call local.tee $r
          i32.const 0 i32.lt_s if unreachable end
          i32.const 32768 local.get $r i32.const {vp} i32.const {vl} call $find
          i32.const 0 i32.lt_s if unreachable end
          call $sample i64.const {} return end unreachable))"#,
        ((op as u64)<<32)|on as u64, ((fp as u64)<<32)|fl as u64)).unwrap()
}

#[test]
fn actual_workflow_terminal_prefixes_roundtrip_after_disposal_and_unload() {
    for mode in ["trap", "malformed", "reject", "cancel", "finish"] {
        let mut f = fixture();
        dispose(&mut f, true);
        let bytes = guest("history", mode, 20);
        let root = f
            .kernel
            .register_guest(manifest("workflow"), &bytes)
            .unwrap();
        f.kernel.issue_grant(root, 0, authority()).unwrap();
        let workflows = Workflows::new(nethacked_workflows::Limits::default()).unwrap();
        let id = workflows
            .start(&mut f.kernel, program(root), vec![], vec![])
            .unwrap();
        let status = workflows.step(&f.kernel, id, b"").unwrap();
        match mode {
            "trap" => assert_eq!(status, Status::Faulted(Fault::Invocation(Error::Trap))),
            "malformed" => assert_eq!(status, Status::Faulted(Fault::InvalidOutcome)),
            "reject" => assert_eq!(status, Status::Rejected),
            "cancel" => {
                assert_eq!(status, Status::Suspended(b"choose".to_vec()));
                workflows.abandon(id).unwrap();
            }
            "finish" => assert_eq!(status, Status::Finished(vec![])),
            _ => unreachable!(),
        }
        let before = f.service.inspect().unwrap();
        assert_eq!(before.records.len(), 2, "{mode}");
        assert_eq!(before.records[1].value, [20]);
        assert_eq!(f.sampled.get(), 2);
        workflows.dispose(&mut f.kernel, id).unwrap();
        f.observations.borrow_mut().clear();
        f.kernel.unregister(root).unwrap();
        let (k, c, _) = capture_history(&mut f.kernel, &f.service, &[]);
        assert!(c.records.iter().all(|r| r.historical));
        let a = approved(&f, &k, Rc::new(|_, _| Err(Error::Authority)));
        let bytes = c.to_bytes(LIMIT).unwrap();
        drop(workflows);
        drop(f);
        let calls = Rc::new(Cell::new(0));
        let (_, _, r) = load(
            &k,
            Checkpoint::from_bytes(&bytes, LIMIT).unwrap(),
            a,
            calls.clone(),
            Rc::new(Cell::new(0)),
        );
        let after = r.service().inspect().unwrap();
        assert_eq!(calls.get(), 0);
        assert_eq!(after.source, before.source);
        assert_eq!(after.next_serial, before.next_serial);
        assert_eq!(after.records[1].ordinal, before.records[1].ordinal);
        assert_eq!(after.records[1].request, before.records[1].request);
        assert_eq!(after.records[1].value, before.records[1].value);
    }
}

#[test]
fn live_grant_refusal_quarantine_nonempty_destination_and_valid_retry() {
    let mut f = fixture();
    let obs = f.observations.borrow()[0].clone();
    let (k, c, ids) = capture_history(&mut f.kernel, &f.service, &[&obs]);
    let calls = Rc::new(Cell::new(0));
    let factories = Rc::new(Cell::new(0));
    let catalog = catalog(calls.clone(), factories.clone());
    let build = Restore::prepare(c.clone(), Limits::default(), &k, &catalog).unwrap();
    let port = build.port();
    let weak = Rc::downgrade(&build.service.0);
    let mut a = approved(&f, &k, port);
    let mut kernel = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
    let mut occupied = manifest("occupied");
    occupied.dependencies.clear();
    let root = kernel.register_guest(occupied, &forward("")).unwrap();
    let before = kernel.artifact(root).unwrap();
    a.denied = true;
    assert!(kernel.restore_kernel(&k, &a).is_err());
    assert_eq!(kernel.artifact(root).unwrap(), before);
    kernel.unregister(root).unwrap();
    a.denied = true;
    assert!(kernel.restore_kernel(&k, &a).is_err());
    assert!(weak.upgrade().unwrap().borrow().quarantine);
    a.denied = false;
    let mapped = kernel.restore_kernel(&k, &a).unwrap();
    let caller = mapped.modules()[&c.records[0].caller];
    kernel.revoke_grant(caller, 0).unwrap();
    assert!(build.finish(&kernel, &mapped).is_err());
    assert!(weak.upgrade().unwrap().borrow().quarantine);
    assert_eq!(calls.get(), 0);
    drop(kernel);
    drop(a);
    assert!(weak.upgrade().is_none());
    let a = approved(&f, &k, Rc::new(|_, _| Err(Error::Authority)));
    drop(obs);
    drop(f);
    let (_, _, restored) = load(&k, c, a, calls.clone(), factories);
    assert_eq!(restored.observation(ids[0]).unwrap().bytes(), [10]);
    assert_eq!(calls.get(), 0);
}
#[test]
fn capture_during_policy_transition_is_busy_and_capture_is_observational() {
    let mut f = fixture();
    let weak = Rc::downgrade(&f.service.0);
    let mut dummy = Kernel::new(nethacked_kernel::Limits::default()).unwrap();
    let snapshot = dummy.kernel_snapshot(&KernelEnrollment::default()).unwrap();
    let transition = f.service.0.borrow().policy.transition.clone();
    f.service.0.borrow_mut().policy.transition = Rc::new(move |s, r| {
        let service = Service(weak.upgrade().unwrap());
        let index = dummy.execution_capture_index(&snapshot).unwrap();
        assert!(service.capture_persistence(&index, &[]).is_err());
        assert!(service.persistence_operations().is_err());
        transition(s, r)
    });
    let op = f.service.inspect().unwrap().records[0].operation.clone();
    f.kernel
        .invoke_operation(&op, &encode(&sample(b"choice")).unwrap(), authority())
        .unwrap();
    let before = f.service.inspect().unwrap();
    let (k, c, _) = capture_history(&mut f.kernel, &f.service, &[]);
    assert_eq!(f.service.inspect().unwrap(), before);
    assert_eq!(f.sampled.get(), 2);
    assert_eq!(c.source, before.source);
    // A missing live enrollment cannot silently turn a live row into history.
    let mut missing = k;
    missing.operations.clear();
    let index = f.kernel.execution_capture_index(&missing).unwrap();
    assert!(f.service.capture_persistence(&index, &[]).is_err());
}

#[test]
fn legacy_live_bytes_and_history_policy_catalog_refusals() {
    let mut f = fixture();
    let observation = f.observations.borrow()[0].clone();
    let (k, c, _) = capture_history(&mut f.kernel, &f.service, &[&observation]);
    let bytes = c.to_bytes(LIMIT).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(value["records"][0].get("historical").is_none());
    // Old version-one canonical live rows omit the newly defaulted field.
    let old = Checkpoint::from_bytes(&bytes, LIMIT).unwrap();
    old.validate_checkpoint(&Limits::default(), &k).unwrap();
    drop(observation);
    dispose(&mut f, true);
    let (k, c, _) = capture_history(&mut f.kernel, &f.service, &[]);
    let factories = Rc::new(Cell::new(0));
    let calls = Rc::new(Cell::new(0));
    let catalog = catalog(calls.clone(), factories.clone());
    for case in 0..4 {
        let mut bad = c.clone();
        match case {
            0 => bad.records.clear(),
            1 => bad.permit_swap_for_test(),
            2 => bad.records[0].permit.resolution[0].endpoint = "swapped".into(),
            _ => bad.kernel_namespace += 1,
        }
        assert!(Restore::prepare(bad, Limits::default(), &k, &catalog).is_err());
    }
    assert_eq!(factories.get(), 0);
    let mut swapped = Catalog::default();
    let count = factories.clone();
    let draws = calls.clone();
    swapped
        .allow("queue".into(), 1, move || {
            count.set(count.get() + 1);
            let draws = draws.clone();
            Ok(Policy {
                identity: "swapped".into(),
                version: 1,
                transition: Rc::new(move |_, _| {
                    draws.set(draws.get() + 1);
                    Err(Error::NativeRefusal)
                }),
            })
        })
        .unwrap();
    assert!(Restore::prepare(c, Limits::default(), &k, &swapped).is_err());
    assert_eq!(factories.get(), 1);
    assert_eq!(calls.get(), 0);
}
impl Checkpoint {
    fn permit_swap_for_test(&mut self) {
        self.records[0].permit.capability.major += 1;
    }
}

#[test]
fn impossible_historical_route_refuses_before_factory() {
    let mut f = fixture();
    dispose(&mut f, true);
    let (k, mut c, _) = capture_history(&mut f.kernel, &f.service, &[]);
    assert_eq!(c.records[0].permit.resolution.len(), 1);
    let duplicate = c.records[0].permit.resolution[0].clone();
    // The sole native provider has no extensions or extension points. It cannot
    // occur twice in this route; both identities nevertheless pass membership.
    let provider = k
        .registry
        .modules
        .iter()
        .find(|m| m.id == duplicate.module)
        .unwrap();
    assert!(provider.manifest.extension.is_none());
    assert!(provider.manifest.extension_points.is_empty());
    c.records[0].permit.resolution.push(duplicate);
    let c = Checkpoint::from_bytes(&c.to_bytes(LIMIT).unwrap(), LIMIT).unwrap();
    let factories = Rc::new(Cell::new(0));
    let result = Restore::prepare(
        c,
        Limits::default(),
        &k,
        &catalog(Rc::new(Cell::new(0)), factories.clone()),
    );
    println!(
        "impossible duplicate-native historical route accepted={} factories={}",
        result.is_ok(),
        factories.get()
    );
    assert!(
        matches!(result, Err(Error::Authority)),
        "structurally impossible history route accepted"
    );
    assert_eq!(factories.get(), 0);
}

#[test]
fn live_source_cannot_be_relabelled_history_without_observations() {
    let mut f = fixture();
    let (k, mut c, _) = capture_history(&mut f.kernel, &f.service, &[]);
    assert!(!c.records[0].historical);
    assert!(k.operations.contains(&c.records[0].operation));
    c.records[0].historical = true;
    let c = Checkpoint::from_bytes(&c.to_bytes(LIMIT).unwrap(), LIMIT).unwrap();
    let calls = Rc::new(Cell::new(0));
    let factories = Rc::new(Cell::new(0));
    let result = Restore::prepare(
        c,
        Limits::default(),
        &k,
        &catalog(calls.clone(), factories.clone()),
    );
    println!(
        "historical=true on enrolled live source: accepted={} factories={} calls={}",
        result.is_ok(),
        factories.get(),
        calls.get()
    );
    assert!(
        matches!(result, Err(Error::Authority)),
        "live source was silently retired by historical flag"
    );
    assert_eq!(factories.get(), 0);
}

#[test]
fn confirm_live_history_downgrade_through_finish() {
    let mut f = fixture();
    let (mut k, mut c, _) = capture_history(&mut f.kernel, &f.service, &[]);
    let source = c.records[0].operation.clone();
    assert!(k.operations.contains(&source));
    c.records[0].historical = true;
    // Removing the enrollment must not permit disposal of a still-live lease.
    k.operations.clear();
    let c = Checkpoint::from_bytes(&c.to_bytes(LIMIT).unwrap(), LIMIT).unwrap();
    let factories = Rc::new(Cell::new(0));
    let calls = Rc::new(Cell::new(0));
    assert!(matches!(
        Restore::prepare(
            c,
            Limits::default(),
            &k,
            &catalog(calls.clone(), factories.clone())
        ),
        Err(Error::Authority)
    ));
    assert_eq!(factories.get(), 0);
    assert_eq!(calls.get(), 0);
}

#[test]
fn namespace_nonreuse_in_subsequent_ordinary_allocations() {
    let base = std::env::temp_dir().join(format!("input-review-process-{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    for stage in ["save", "load", "load", "load", "load"] {
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "persistence::history_tests::review_process_worker",
                "--nocapture",
            ])
            .env("INPUT_REVIEW_STAGE", stage)
            .env("INPUT_REVIEW_DIRECTORY", &base)
            .output()
            .unwrap();
        println!(
            "stage={stage}\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            out.status.success(),
            "fresh-process identity nonreuse failure; saved files at {}",
            base.display()
        );
    }
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn review_process_worker() {
    let Ok(stage) = std::env::var("INPUT_REVIEW_STAGE") else {
        return;
    };
    let base = std::path::PathBuf::from(std::env::var("INPUT_REVIEW_DIRECTORY").unwrap());
    if stage == "save" {
        for _ in 0..5 {
            drop(fixture());
        }
        let mut f = fixture();
        dispose(&mut f, true);
        for _ in 0..3 {
            drop(fixture());
        }
        let (k, c, _) = capture_history(&mut f.kernel, &f.service, &[]);
        assert_eq!(c.namespace, 6);
        assert_eq!(c.namespace_high_water, 9);
        println!("saved namespace={} origins={:?}", c.namespace, c.origins);
        std::fs::write(base.join("kernel"), encode(&k).unwrap()).unwrap();
        std::fs::write(base.join("input"), c.to_bytes(LIMIT).unwrap()).unwrap();
        return;
    }
    let k = serde_json::from_slice(&std::fs::read(base.join("kernel")).unwrap()).unwrap();
    let c = Checkpoint::from_bytes(&std::fs::read(base.join("input")).unwrap(), LIMIT).unwrap();
    let f = fixture();
    let a = approved(&f, &k, Rc::new(|_, _| Err(Error::Authority)));
    drop(f);
    let (mut kernel, _, restored) = load(
        &k,
        c.clone(),
        a,
        Rc::new(Cell::new(0)),
        Rc::new(Cell::new(0)),
    );
    println!(
        "restored namespace={} saved origins={:?}",
        restored.service.0.borrow().namespace,
        c.origins
    );
    assert!(restored.service.0.borrow().namespace > c.namespace_high_water);
    let mut last = 0;
    for _ in 0..8 {
        let fresh = fixture();
        let row = fresh.service.inspect().unwrap().records[0].clone();
        assert!(row.id.namespace > c.namespace_high_water);
        assert!(!c.origins.contains(&row.id.namespace));
        last = row.id.namespace;
        // Include allocation holes, not only namespaces represented by rows.
        for namespace in [1, 6, c.namespace, c.namespace_high_water] {
            let out = fresh
                .kernel
                .invoke_operation(
                    &row.operation,
                    &encode(&Request::Replay {
                        id: RecordId {
                            namespace,
                            serial: 1,
                        },
                        request: b"choice".to_vec(),
                    })
                    .unwrap(),
                    authority(),
                )
                .unwrap();
            assert!(i32::from_le_bytes(out.try_into().unwrap()) < 0);
        }
        let out = fresh
            .kernel
            .invoke_operation(
                &row.operation,
                &encode(&Request::Replay {
                    id: row.id,
                    request: b"choice".to_vec(),
                })
                .unwrap(),
                authority(),
            )
            .unwrap();
        let reply: Reply = serde_json::from_slice(&out).unwrap();
        assert_eq!(reply.id, row.id);
        assert_eq!(reply.value, row.value);
        assert_eq!(fresh.sampled.get(), 1);
    }
    let (k2, c2, _) = capture_history(&mut kernel, restored.service(), &[]);
    assert!(c2.namespace_high_water >= last);
    assert!(c2
        .origins
        .iter()
        .zip(c2.origins.iter().skip(1))
        .any(|(a, b)| b - a > 1));
    std::fs::write(base.join("kernel"), encode(&k2).unwrap()).unwrap();
    std::fs::write(base.join("input"), c2.to_bytes(LIMIT).unwrap()).unwrap();
}

fn input_extension(base: &Manifest) -> Manifest {
    Manifest {
        id: "input-extension".into(),
        exports: base.exports.clone(),
        dependencies: vec![],
        extension_points: BTreeSet::new(),
        extension: Some(nethacked_kernel::Extension {
            target: cap(),
            export: ENDPOINT.into(),
        }),
    }
}
#[test]
fn legal_old_extension_routes_survive_retention_and_unload() {
    let mut f = fixture_with_extension(true);
    let extension = f
        .kernel
        .register_native(
            input_extension(&f.native_manifest),
            b"extension-v1",
            BTreeMap::from([(
                ENDPOINT.into(),
                Rc::new(|b: &[u8], _: &nethacked_kernel::CallerView<'_>| Ok(b.to_vec()))
                    as NativePort,
            )]),
        )
        .unwrap();
    // Exercise structurally legal old routes using authenticated immutable
    // registry metadata, independent of the current attachment/grant state.
    f.kernel.managed_detach(extension).unwrap();
    dispose(&mut f, true);
    let (k, mut c, _) = capture_history(&mut f.kernel, &f.service, &[]);
    let module = k
        .registry
        .modules
        .iter()
        .find(|m| m.manifest.id == "input-extension")
        .unwrap();
    let binding = Binding {
        module: module.id,
        artifact: module.artifact,
        endpoint: ENDPOINT.into(),
    };
    c.records[0].permit.resolution.push(binding.clone());
    let c = Checkpoint::from_bytes(&c.to_bytes(LIMIT).unwrap(), LIMIT).unwrap();
    let factories = Rc::new(Cell::new(0));
    let calls = Rc::new(Cell::new(0));
    let build = Restore::prepare(
        c.clone(),
        Limits::default(),
        &k,
        &catalog(calls.clone(), factories.clone()),
    )
    .unwrap();
    assert_eq!(factories.get(), 1);
    assert_eq!(calls.get(), 0);
    drop(build);
    f.kernel.managed_unload(extension).unwrap();
    let (k, mut c, _) = capture_history(&mut f.kernel, &f.service, &[]);
    c.records[0].permit.resolution.push(binding);
    let c = Checkpoint::from_bytes(&c.to_bytes(LIMIT).unwrap(), LIMIT).unwrap();
    assert!(!k
        .registry
        .modules
        .iter()
        .any(|m| m.id == c.records[0].permit.resolution[1].module));
    let a = approved(&f, &k, Rc::new(|_, _| Err(Error::Authority)));
    let (mut kernel, _, restored) = load(&k, c.clone(), a, calls.clone(), factories.clone());
    let (_, recaptured, _) = capture_history(&mut kernel, restored.service(), &[]);
    assert_eq!(
        serde_json::to_value(&recaptured.records).unwrap(),
        serde_json::to_value(&c.records).unwrap()
    );
    assert_eq!(calls.get(), 0);
    assert_eq!(factories.get(), 2);
}
#[test]
fn namespace_highwater_exhaustion_never_wraps() {
    let counter = AtomicU64::new(1);
    assert_eq!(issue_namespace_after(&counter, 9).unwrap(), 10);
    assert_eq!(issue_namespace_after(&counter, 0).unwrap(), 11);
    assert_eq!(issue_namespace_after(&counter, 4).unwrap(), 12);
    assert_eq!(
        issue_namespace_after(&counter, u64::MAX - 2).unwrap(),
        u64::MAX - 1
    );
    for retired in [0, 6, u64::MAX - 1, u64::MAX] {
        assert_eq!(issue_namespace_after(&counter, retired), Err(Error::Limit));
        assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
    }
    let mut f = fixture();
    dispose(&mut f, true);
    let (k, mut c, _) = capture_history(&mut f.kernel, &f.service, &[]);
    c.namespace_high_water = u64::MAX;
    let factories = Rc::new(Cell::new(0));
    assert!(matches!(
        Restore::prepare(
            c,
            Limits::default(),
            &k,
            &catalog(Rc::new(Cell::new(0)), factories.clone())
        ),
        Err(Error::Limit)
    ));
    assert_eq!(factories.get(), 0);
}
