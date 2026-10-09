#![cfg(feature = "persistence")]
mod persistence_support;
use nethacked_kernel::{persistence::*, *};
use nethacked_resources::{persistence::*, *};
use persistence_support::*;
use std::{collections::BTreeSet, path::Path, process::Command, rc::Rc};

#[derive(serde::Serialize, serde::Deserialize)]
struct Packet {
    kernel: KernelSnapshot,
    resources: Checkpoint,
    materials: Vec<([u8; 32], Material)>,
    mode: String,
}
fn capture(
    kernel: &mut Kernel,
    service: &Service,
    effects: &Rc<Effects>,
) -> (KernelSnapshot, Checkpoint) {
    let mut catalog = CaptureCatalog::new(service);
    for owner in service
        .state()
        .unwrap()
        .resources
        .keys()
        .map(|r| r.owner.clone())
        .collect::<BTreeSet<_>>()
    {
        catalog.owner(&owner, codec(Role::Owner, effects)).unwrap();
    }
    catalog
        .accounting(codec(Role::Accounting, effects))
        .unwrap();
    let ops = service.persistence_enrollment(kernel).unwrap();
    let (cleanup, roots) = service.persistence_roots().unwrap();
    let saved = kernel
        .kernel_snapshot_with_resource_roots(
            &KernelEnrollment {
                operations: &ops,
                pure_captures: &[],
            },
            &[cleanup],
            &roots,
            &service.persistence_history().unwrap(),
        )
        .unwrap();
    let c = service
        .capture_local(
            &kernel.execution_capture_index(&saved).unwrap(),
            &saved,
            &catalog,
        )
        .unwrap();
    (saved, c)
}
fn write(path: &Path, packet: &Packet) {
    std::fs::write(path, serde_json::to_vec(packet).unwrap()).unwrap();
}

#[test]
fn resource_process_child() {
    let Some(input) = std::env::var_os("RESOURCE_PROCESS_INPUT") else {
        return;
    };
    let output = std::env::var_os("RESOURCE_PROCESS_OUTPUT").unwrap();
    let mut packet: Packet = serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
    let effects = Rc::new(Effects::default());
    if packet.mode == "service-overflow" {
        packet.resources.namespace_high_water = u64::MAX - 1;
        let approved = Approved::new(&packet.kernel, &packet.resources);
        assert!(packet
            .resources
            .clone()
            .validate(&limits(), &packet.kernel, catalog(&effects), &approved)
            .unwrap()
            .stage()
            .is_err());
        assert_eq!(effects.decodes.get(), 0);
        std::fs::write(output, b"overflow-refused").unwrap();
        return;
    }
    if packet.mode == "cleanup-overflow" {
        packet.kernel.cleanup_high_water = u64::MAX - 1;
        let approved = Approved::new(&packet.kernel, &packet.resources);
        assert!(packet
            .resources
            .clone()
            .validate(&limits(), &packet.kernel, catalog(&effects), &approved)
            .unwrap()
            .stage()
            .is_err());
        assert_eq!(effects.decodes.get(), 0);
        std::fs::write(output, b"overflow-refused").unwrap();
        return;
    }
    let approved = Approved::new(&packet.kernel, &packet.resources);
    let staged = packet
        .resources
        .clone()
        .validate(&limits(), &packet.kernel, catalog(&effects), &approved)
        .unwrap()
        .stage()
        .unwrap();
    let mut host = Host::fresh(
        packet.materials.iter().cloned().collect(),
        &packet.kernel,
        staged.ports(),
    );
    host.prefix.clear();
    let mut kernel = fresh_kernel();
    let mapped = kernel.restore_kernel(&packet.kernel, &host).unwrap();
    let restored = staged.finish(&kernel, &mapped).unwrap();
    let old = &packet.resources.pending[0];
    let fresh = restored.transaction(old.serial).unwrap();
    assert!(fresh.service > packet.resources.namespace_high_water);
    let before = restored.service().state().unwrap();
    let actor = mapped.modules()[&old.context.caller.module];
    let retired = nethacked_resources::Identity {
        service: packet.resources.namespace,
        serial: old.serial,
    };
    assert!(abi(kernel
        .invoke(
            actor,
            "step",
            &command(b'C', &retired),
            auth(before.resources.keys().cloned())
        )
        .unwrap())
    .is_err());
    assert_eq!(before, restored.service().state().unwrap());
    assert_eq!(effects.predicates.get(), 0);
    // A subsequent ordinary constructor uses the same global high-water allocator.
    // Capture through a separate kernel so no retained source enrollment is reused.
    let ordinary = Service::new(limits(), Rc::new(|_| true)).unwrap();
    let mut ordinary_kernel = fresh_kernel();
    let (ordinary_k, ordinary_c) = capture(&mut ordinary_kernel, &ordinary, &effects);
    assert!(ordinary_c.namespace > fresh.service);
    assert!(ordinary_c.cleanup > packet.kernel.cleanup_high_water);
    assert!(ordinary_c.cleanup > packet.resources.cleanup);
    let extra = CleanupIdentity::issue().unwrap();
    assert_ne!(extra, restored.service().cleanup_identity().unwrap());
    drop(ordinary);
    drop(ordinary_kernel);
    let (next_k, next_c) = capture(&mut kernel, restored.service(), &effects);
    assert!(next_c.namespace_high_water >= ordinary_c.namespace);
    assert!(next_k.cleanup_high_water > ordinary_k.cleanup_high_water);
    assert!(next_c.cleanup > packet.kernel.cleanup_high_water);
    assert_eq!(next_c.history, packet.resources.history);
    assert_eq!(next_c.next_serial, packet.resources.next_serial);
    assert_eq!(next_c.pending.len(), packet.resources.pending.len());
    packet.kernel = next_k;
    packet.resources = next_c;
    write(Path::new(&output), &packet);
}

#[test]
fn three_fresh_process_load_save_cycles_and_ordinary_allocations_with_holes_and_overflow() {
    let mut f = Fixture::new();
    f.prepare(&f.plan(), None);
    for _ in 0..5 {
        drop(Service::new(limits(), Rc::new(|_| true)).unwrap());
        CleanupIdentity::issue().unwrap();
    }
    let (k, c) = f.save();
    assert!(c.namespace_high_water > c.namespace);
    assert!(k.cleanup_high_water > c.cleanup);
    let mut packet = Packet {
        kernel: k,
        resources: c,
        materials: f.materials.iter().map(|(id, m)| (*id, m.clone())).collect(),
        mode: "roundtrip".into(),
    };
    drop(f);
    let evidence = std::env::var_os("RESOURCE_PROCESS_EVIDENCE");
    let dir = evidence
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("resource-process-{}", std::process::id()))
        });
    std::fs::create_dir_all(&dir).unwrap();
    for round in 0..3 {
        let input = dir.join(format!("input-{round}.json"));
        let output = dir.join(format!("output-{round}.json"));
        write(&input, &packet);
        let result = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "resource_process_child", "--nocapture"])
            .env("RESOURCE_PROCESS_INPUT", &input)
            .env("RESOURCE_PROCESS_OUTPUT", &output)
            .output()
            .unwrap();
        std::fs::write(dir.join(format!("child-{round}.stdout")), &result.stdout).unwrap();
        std::fs::write(dir.join(format!("child-{round}.stderr")), &result.stderr).unwrap();
        std::fs::write(dir.join(format!("child-{round}-command.json")), serde_json::to_vec(&serde_json::json!({ "program": std::env::current_exe().unwrap(), "args": ["--exact", "resource_process_child", "--nocapture"], "env": {"RESOURCE_PROCESS_INPUT": input, "RESOURCE_PROCESS_OUTPUT": output}, "exit": result.status.code() })).unwrap()).unwrap();
        assert!(
            result.status.success(),
            "child {round}: {}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        packet = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    }
    for mode in ["service-overflow", "cleanup-overflow"] {
        packet.mode = mode.into();
        let input = dir.join(format!("{mode}-input.json"));
        let output = dir.join(format!("{mode}-output.txt"));
        write(&input, &packet);
        let result = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "resource_process_child", "--nocapture"])
            .env("RESOURCE_PROCESS_INPUT", &input)
            .env("RESOURCE_PROCESS_OUTPUT", &output)
            .output()
            .unwrap();
        std::fs::write(dir.join(format!("{mode}.stdout")), &result.stdout).unwrap();
        std::fs::write(dir.join(format!("{mode}.stderr")), &result.stderr).unwrap();
        std::fs::write(dir.join(format!("{mode}-command.json")), serde_json::to_vec(&serde_json::json!({ "program": std::env::current_exe().unwrap(), "args": ["--exact", "resource_process_child", "--nocapture"], "env": {"RESOURCE_PROCESS_INPUT": input, "RESOURCE_PROCESS_OUTPUT": output}, "exit": result.status.code() })).unwrap()).unwrap();
        assert!(
            result.status.success(),
            "{mode}: {}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(std::fs::read(&output).unwrap(), b"overflow-refused");
    }
    if evidence.is_none() {
        std::fs::remove_dir_all(dir).unwrap();
    }
}
