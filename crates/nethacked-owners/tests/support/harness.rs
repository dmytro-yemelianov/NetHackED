use nethacked_kernel::{
    Authority, Dependency as KernelDependency, Endpoint, Limits as KernelLimits, Manifest,
    NativePort, Scope,
};


type Action = Box<dyn FnMut(&CallerView<'_>) -> nethacked_kernel::Result<Vec<u8>>>;
struct Harness {
    kernel: Kernel,
    module: ModuleHandle,
    root: ModuleHandle,
    host: Host,
    action: Rc<RefCell<Option<Action>>>,
    lease: nethacked_kernel::PipelineLease,
    operation: OperationBinding,
    keys: BTreeSet<Resource>,
    helper: ModuleHandle,
}
fn cap(name: &str) -> Capability {
    Capability {
        provider: "owners".into(),
        endpoint: name.into(),
        major: 1,
    }
}
fn endpoint(access: Access) -> Endpoint {
    Endpoint {
        major: 1,
        access,
        max_request: 1024,
        max_reply: 1024,
    }
}
fn helper_wasm(body: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module (memory (export "memory") 1)
        (func (export "alloc") (param i32) (result i32) i32.const 0)
        (func (export "compute") (param $p i32) (param i32) (result i64) {body}))"#
    ))
    .unwrap()
}
impl Harness {
    fn new(limits: Limits, kernel_limits: KernelLimits, body: &str) -> Self {
        let host = Host::new(limits).unwrap();
        let mut kernel = Kernel::new(kernel_limits).unwrap();
        let helper = kernel
            .register_guest(
                Manifest {
                    id: "algorithm".into(),
                    exports: BTreeMap::from([("compute".into(), endpoint(Access::Read))]),
                    dependencies: vec![],
                    extension_points: BTreeSet::new(),
                    extension: None,
                },
                &helper_wasm(body),
            )
            .unwrap();
        let action: Rc<RefCell<Option<Action>>> = Rc::new(RefCell::new(None));
        let ports: BTreeMap<_, _> = ["query", "prepare", "derived"]
            .into_iter()
            .map(|name| {
                let a = action.clone();
                (
                    name.into(),
                    Rc::new(move |_: &[u8], view: &CallerView| {
                        a.borrow_mut().as_mut().unwrap()(view)
                    }) as NativePort,
                )
            })
            .collect();
        let module = kernel
            .register_native(
                Manifest {
                    id: "owners".into(),
                    exports: BTreeMap::from([
                        ("query".into(), endpoint(Access::Read)),
                        ("prepare".into(), endpoint(Access::Write)),
                        ("derived".into(), endpoint(Access::Read)),
                    ]),
                    dependencies: vec![KernelDependency {
                        capability: Capability {
                            provider: "algorithm".into(),
                            endpoint: "compute".into(),
                            major: 1,
                        },
                        access: Access::Read,
                    }],
                    extension_points: BTreeSet::new(),
                    extension: None,
                },
                b"trusted owner adapters",
                ports,
            )
            .unwrap();
        kernel
            .issue_grant(
                module,
                0,
                Authority {
                    access: Access::Read,
                    scope: Scope::Keys(BTreeSet::new()),
                },
            )
            .unwrap();
        let bytes = wat::parse_str(r#"(module
            (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (func (export "alloc") (param i32) (result i32) i32.const 0)
            (func (export "run") (param $p i32) (param i32) (result i64) (local $n i32)
                local.get $p i32.load8_u i32.const 0 i32.const 0 i32.const 2048 i32.const 1024 call $call local.tee $n
                i32.const 0 i32.lt_s if unreachable end
                i64.const 8796093022208 local.get $n i64.extend_i32_u i64.or))"#).unwrap();
        let root = kernel
            .register_guest(
                Manifest {
                    id: "caller".into(),
                    exports: BTreeMap::from([("run".into(), endpoint(Access::Write))]),
                    dependencies: vec![
                        KernelDependency {
                            capability: cap("query"),
                            access: Access::Read,
                        },
                        KernelDependency {
                            capability: cap("prepare"),
                            access: Access::Write,
                        },
                        KernelDependency {
                            capability: cap("derived"),
                            access: Access::Read,
                        },
                    ],
                    extension_points: BTreeSet::new(),
                    extension: None,
                },
                &bytes,
            )
            .unwrap();
        // Install empty grants before capture; refresh the lease after enrollment.
        for slot in 0..3 {
            kernel
                .issue_grant(
                    root,
                    slot,
                    Authority {
                        access: if slot == 1 {
                            Access::Write
                        } else {
                            Access::Read
                        },
                        scope: Scope::Keys(BTreeSet::new()),
                    },
                )
                .unwrap();
        }
        let ceiling = Authority {
            access: Access::Write,
            scope: Scope::All,
        };
        let lease = kernel.capture_pipeline(root, "run", ceiling).unwrap();
        let id = kernel.issue_operation().unwrap();
        let operation = kernel
            .operation_binding_with_cleanup(
                id,
                lease,
                1,
                "prepare",
                &[host.cleanup_identity().unwrap()],
            )
            .unwrap();
        Self {
            kernel,
            module,
            root,
            host,
            action,
            lease,
            operation,
            keys: BTreeSet::new(),
            helper,
        }
    }
    fn spec(&self, schema: u8, dependencies: Vec<Dependency>) -> OwnerSpec {
        OwnerSpec {
            module: self.module,
            schema: [schema; 32],
            ports: BTreeMap::from([
                (
                    "query".into(),
                    Port {
                        access: Access::Read,
                        major: 1,
                        dependencies: vec![],
                    },
                ),
                (
                    "prepare".into(),
                    Port {
                        access: Access::Write,
                        major: 1,
                        dependencies: dependencies.clone(),
                    },
                ),
                (
                    "derived".into(),
                    Port {
                        access: Access::Read,
                        major: 1,
                        dependencies,
                    },
                ),
            ]),
        }
    }
    fn admit<T: 'static>(&mut self, owner: &Owner<T>) {
        for name in ["query", "prepare", "derived"] {
            self.keys.insert(owner.resource(name).unwrap());
        }
    }
    fn refresh(&mut self) {
        self.keys.insert(self.host.publication_resource());
        for slot in 0..3 {
            self.kernel
                .issue_grant(
                    self.root,
                    slot,
                    Authority {
                        access: if slot == 1 {
                            Access::Write
                        } else {
                            Access::Read
                        },
                        scope: Scope::Keys(self.keys.clone()),
                    },
                )
                .unwrap();
        }
        self.kernel.release_pipeline(self.lease).unwrap();
        self.lease = self
            .kernel
            .capture_pipeline(
                self.root,
                "run",
                Authority {
                    access: Access::Write,
                    scope: Scope::All,
                },
            )
            .unwrap();
        let op = self.kernel.issue_operation().unwrap();
        self.operation = self
            .kernel
            .operation_binding_with_cleanup(
                op,
                self.lease,
                1,
                "prepare",
                &[self.host.cleanup_identity().unwrap()],
            )
            .unwrap();
    }
    fn run(
        &self,
        slot: u8,
        action: impl FnMut(&CallerView<'_>) -> nethacked_kernel::Result<Vec<u8>> + 'static,
    ) {
        self.run_result(slot, action).unwrap();
    }
    fn run_result(
        &self,
        slot: u8,
        action: impl FnMut(&CallerView<'_>) -> nethacked_kernel::Result<Vec<u8>> + 'static,
    ) -> nethacked_kernel::Result<Vec<u8>> {
        *self.action.borrow_mut() = Some(Box::new(action));
        let result = self.kernel.invoke_operation(
            &self.operation,
            &[slot],
            Authority {
                access: Access::Write,
                scope: Scope::All,
            },
        );
        self.action.borrow_mut().take();
        result
    }
    fn binding(&self) -> CapabilityPermit {
        CapabilityPermit {
            capability: Capability {
                provider: "algorithm".into(),
                endpoint: "compute".into(),
                major: 1,
            },
            resolution: vec![ProviderBinding {
                handle: self.helper,
                artifact: self.kernel.artifact(self.helper).unwrap(),
                endpoint: "compute".into(),
            }],
        }
    }

}
