//! Actual producer/consumer processes reuse the accepted combined fixture.
//! Transport contains bytes and immutable host approvals, never live handles.
use super::*;
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Command};

pub(super) fn domain_state(bundle: &composed::HostBundle) -> serde_json::Value {
    macro_rules! owner {
        ($t:ty, $key:literal) => {{
            let owner = &bundle.owner::<$t>($key).unwrap().owner;
            serde_json::json!([
                format!("{:?}", owner.stamp().unwrap()),
                owner.inspect().unwrap()
            ])
        }};
    }
    let services = bundle.services();
    let workflows = &services.workflows.as_ref().unwrap().0;
    let frames: Vec<_> = workflows
        .operations()
        .unwrap()
        .into_iter()
        .map(|id| workflows.inspect(id).unwrap())
        .collect();
    serde_json::json!({
        "owners": [owner!(u64, "native"), owner!(Vec<u8>, "wasm"),
            owner!(u64, "p-native"), owner!(Vec<u8>, "p-blob"),
            owner!(FieldState, "p-field"), owner!(Vec<u8>, "p-wasm"),
            owner!(u64, "readonly"), owner!(FieldState, "charge")],
        "history": format!("{:?}", bundle.host().persistence_history().unwrap()),
        "workflows": format!("{frames:?}"),
        "inputs": format!("{:?}", services.inputs.as_ref().unwrap().inspect().unwrap()),
        "timers": format!("{:?}", services.timers.as_ref().unwrap().0.state().unwrap()),
        "resources": format!("{:?}", services.resources.as_ref().unwrap().0.state().unwrap()),
    })
}

pub(super) struct Fixture {
    pub wire: Vec<u8>,
    pub materials: BTreeMap<[u8; 32], Material>,
    pub base: CapabilityPermit,
    pub base_ids: Vec<u64>,
    pub future_binding: CapabilityPermit,
    pub future_ids: Vec<u64>,
    pub input_limits: nethacked_inputs::Limits,
    pub task: nethacked_timers::TaskId,
    pub operation: Option<nethacked_kernel::OperationId>,
    pub old_operation: (u64, u64),
}

#[derive(Serialize, Deserialize)]
enum ApprovedMaterial {
    Guest(Manifest, Vec<u8>),
    Native(Manifest, Vec<u8>),
}

#[derive(Serialize, Deserialize)]
struct ApprovedBinding {
    capability: Capability,
    providers: Vec<([u8; 32], String)>,
    modules: Vec<u64>,
}
impl ApprovedBinding {
    fn from_runtime(binding: &CapabilityPermit, modules: Vec<u64>) -> Self {
        Self {
            capability: binding.capability.clone(),
            providers: binding
                .resolution
                .iter()
                .map(|p| (p.artifact, p.endpoint.clone()))
                .collect(),
            modules,
        }
    }
    fn runtime(self, handles: &BTreeMap<[u8; 32], ModuleHandle>) -> (CapabilityPermit, Vec<u64>) {
        assert_eq!(self.providers.len(), self.modules.len());
        (
            CapabilityPermit {
                capability: self.capability,
                resolution: self
                    .providers
                    .into_iter()
                    .map(|(artifact, endpoint)| ProviderBinding {
                        handle: handles[&artifact],
                        artifact,
                        endpoint,
                    })
                    .collect(),
            },
            self.modules,
        )
    }
}

#[derive(Serialize, Deserialize)]
struct Transport {
    wire: Vec<u8>,
    materials: Vec<([u8; 32], ApprovedMaterial)>,
    base: ApprovedBinding,
    future: ApprovedBinding,
    limits: serde_json::Value,
    task: nethacked_timers::TaskId,
    old_operation: (u64, u64),
    producer_pid: u32,
}

impl Fixture {
    fn write(self, path: &Path) {
        let materials = self
            .materials
            .into_iter()
            .map(|(hash, m)| {
                (
                    hash,
                    match m {
                        Material::Guest(manifest, bytes) => {
                            ApprovedMaterial::Guest(manifest, bytes)
                        }
                        Material::Native(manifest, bytes) => {
                            ApprovedMaterial::Native(manifest, bytes)
                        }
                    },
                )
            })
            .collect();
        let t = Transport {
            wire: self.wire,
            materials,
            base: ApprovedBinding::from_runtime(&self.base, self.base_ids),
            future: ApprovedBinding::from_runtime(&self.future_binding, self.future_ids),
            limits: serde_json::to_value(self.input_limits).unwrap(),
            task: self.task,
            old_operation: self.old_operation,
            producer_pid: std::process::id(),
        };
        std::fs::write(path, serde_json::to_vec(&t).unwrap()).unwrap();
    }

    fn read(path: &Path) -> Self {
        let bytes = std::fs::read(path).unwrap();
        assert!(bytes.len() <= 64 * 1024 * 1024);
        let t: Transport = serde_json::from_slice(&bytes).unwrap();
        assert_ne!(t.producer_pid, std::process::id());
        let materials: BTreeMap<_, _> = t
            .materials
            .into_iter()
            .map(|(hash, m)| {
                (
                    hash,
                    match m {
                        ApprovedMaterial::Guest(manifest, bytes) => {
                            Material::Guest(manifest, bytes)
                        }
                        ApprovedMaterial::Native(manifest, bytes) => {
                            Material::Native(manifest, bytes)
                        }
                    },
                )
            })
            .collect();
        // These host-created handles replace transport identities only in the
        // immutable approved factory templates. Restore remaps each saved
        // module ID and checks its exact artifact again before installation.
        let mut kernel = Kernel::new(KernelLimits::default()).unwrap();
        let mut handles = BTreeMap::new();
        let mut registered = BTreeSet::new();
        let mut pending: Vec<_> = materials.iter().collect();
        while !pending.is_empty() {
            let ready = pending
                .iter()
                .position(|(_, material)| {
                    let manifest = match material {
                        Material::Guest(manifest, _) | Material::Native(manifest, _) => manifest,
                    };
                    manifest
                        .dependencies
                        .iter()
                        .all(|d| registered.contains(&d.capability.provider))
                        && manifest
                            .extension
                            .as_ref()
                            .is_none_or(|e| registered.contains(&e.target.provider))
                })
                .expect("approved fixture artifact dependencies must resolve");
            let (hash, material) = pending.remove(ready);
            let (manifest, handle) = match material {
                Material::Guest(manifest, bytes) => (
                    manifest,
                    kernel.register_guest(manifest.clone(), bytes).unwrap(),
                ),
                Material::Native(manifest, identity) => {
                    let ports = manifest
                        .exports
                        .keys()
                        .map(|name| {
                            let port: NativePort =
                                Rc::new(|_, _| Err(nethacked_kernel::Error::NativeRefusal));
                            (name.clone(), port)
                        })
                        .collect();
                    (
                        manifest,
                        kernel
                            .register_native(manifest.clone(), identity, ports)
                            .unwrap(),
                    )
                }
            };
            assert_eq!(kernel.artifact(handle).unwrap(), *hash);
            registered.insert(manifest.id.clone());
            handles.insert(*hash, handle);
        }
        let (base, base_ids) = t.base.runtime(&handles);
        let (future_binding, future_ids) = t.future.runtime(&handles);
        let input_limits = nethacked_inputs::Limits::default();
        assert_eq!(serde_json::to_value(&input_limits).unwrap(), t.limits);
        Self {
            wire: t.wire,
            materials,
            base,
            base_ids,
            future_binding,
            future_ids,
            input_limits,
            task: t.task,
            operation: None,
            old_operation: t.old_operation,
        }
    }
}

pub(super) fn run() {
    if let Ok(mode) = std::env::var("NETHACKED_A5_CALLBACK_MODE") {
        let path = std::env::var_os("NETHACKED_A5_CALLBACK_FILE").unwrap();
        let terminal = std::env::var("NETHACKED_A5_CALLBACK_TERMINAL").unwrap() == "true";
        match mode.as_str() {
            "produce" => callback_fixture(terminal).write(Path::new(&path)),
            "consume" => callback_resume(terminal, Fixture::read(Path::new(&path))),
            _ => panic!("unknown child phase"),
        }
        return;
    }
    let directory = std::env::temp_dir().join(format!(
        "netrust-a5-callbacks-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    for terminal in [false, true] {
        let fixture = directory.join(format!("{terminal}.json"));
        for mode in ["produce", "consume"] {
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "a5_fresh_process_pending_and_terminal_acceptance",
                    "--nocapture",
                ])
                .env("NETHACKED_A5_CALLBACK_MODE", mode)
                .env("NETHACKED_A5_CALLBACK_FILE", &fixture)
                .env("NETHACKED_A5_CALLBACK_TERMINAL", terminal.to_string())
                .output()
                .unwrap();
            let log = directory.join(format!("{terminal}-{mode}.log"));
            let bytes = [output.stdout, output.stderr].concat();
            std::fs::write(&log, &bytes).unwrap();
            assert!(
                output.status.success(),
                "child failed; log: {}",
                log.display()
            );
            assert!(
                String::from_utf8_lossy(&bytes).contains("1 passed; 0 failed;"),
                "child did not execute exactly one test"
            );
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}
