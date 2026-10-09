#![cfg(feature = "session")]
//! The registry feeds the existing owner restore seam; it does not bypass its
//! old-codec checks, graph remapping, target checks or hidden installation.
use nethacked_kernel::{Access, Endpoint, Kernel, Manifest, ModuleHandle, NativePort};
use nethacked_owners::{self as owners, persistence::*, *};
use nethacked_persistence::migration::*;
use std::{cell::Cell, collections::BTreeMap, rc::Rc};

struct Codec(u8);
impl Schema<u64> for Codec {
    fn validate(&self, state: &u64) -> owners::Result<()> {
        if *state > if self.0 == 1 { 100 } else { 50 } {
            Err(owners::Error::Invariant)
        } else {
            Ok(())
        }
    }
    fn encode(&self, state: &u64) -> owners::Result<Vec<u8>> {
        Ok(state.to_le_bytes().to_vec())
    }
    fn query(&self, state: &u64, _: &str, _: &[u8], _: &[Observation]) -> owners::Result<Vec<u8>> {
        self.encode(state)
    }
}
impl NativeBehavior<u64> for Codec {
    fn prepare(
        &self,
        _: &u64,
        _: &str,
        _: &[u8],
        _: &[Observation],
    ) -> owners::Result<Proposal<u64>> {
        Err(owners::Error::Unsupported)
    }
}
impl PersistenceCodec<u64> for Codec {
    fn identity(&self) -> CodecIdentity {
        let id = |name: &str| ArtifactIdentity {
            name: name.into(),
            version: self.0.into(),
            artifact: [self.0; 32],
        };
        CodecIdentity {
            schema: id("schema"),
            codec: id("codec"),
            policy: id("policy"),
            schema_digest: [self.0; 32],
        }
    }
    fn encode_state(&self, state: &u64, limit: usize) -> owners::Result<Vec<u8>> {
        if limit < 8 {
            return Err(owners::Error::Limit);
        }
        self.encode(state)
    }
    fn decode_state(&self, bytes: &[u8], limit: usize, _: &Remap) -> owners::Result<u64> {
        if bytes.len() > limit {
            return Err(owners::Error::Limit);
        }
        Ok(u64::from_le_bytes(
            bytes.try_into().map_err(|_| owners::Error::Invalid)?,
        ))
    }
    fn validate_state(&self, state: &u64) -> owners::Result<()> {
        self.validate(state)
    }
    fn references(&self, _: &u64) -> owners::Result<Vec<StateReference>> {
        Ok(vec![])
    }
    fn validate_configuration(
        &self,
        _: &u64,
        actual: &BTreeMap<String, Port>,
        policy: &QueryPolicy,
    ) -> owners::Result<()> {
        if actual.len() != 1
            || !actual.get("query").is_some_and(|port| {
                port.major == 1 && port.access == Access::Read && port.dependencies.is_empty()
            })
            || *policy != QueryPolicy::Noncacheable
        {
            Err(owners::Error::Schema)
        } else {
            Ok(())
        }
    }
}
fn ports() -> BTreeMap<String, Port> {
    [(
        "query".into(),
        Port {
            major: 1,
            access: Access::Read,
            dependencies: vec![],
        },
    )]
    .into()
}
fn kernel() -> (Kernel, ModuleHandle) {
    let mut kernel = Kernel::new(Default::default()).unwrap();
    let module = kernel
        .register_native(
            Manifest {
                id: "migration-test".into(),
                exports: [(
                    "query".into(),
                    Endpoint {
                        major: 1,
                        access: Access::Read,
                        max_request: 8,
                        max_reply: 8,
                    },
                )]
                .into(),
                dependencies: vec![],
                extension_points: Default::default(),
                extension: None,
            },
            b"migration-test-v1",
            [(
                "query".into(),
                Rc::new(|_: &[u8], _: &nethacked_kernel::CallerView<'_>| Ok(vec![])) as NativePort,
            )]
            .into(),
        )
        .unwrap();
    (kernel, module)
}
fn binding() -> MigrationBinding {
    let format = |tag| {
        let id = Codec(tag).identity();
        FormatIdentity {
            schema: id.schema_digest,
            codec_artifact: id.codec.artifact,
            codec_version: id.codec.version,
        }
    };
    MigrationBinding {
        source: format(1),
        target: format(2),
        transform_artifact: [3; 32],
        transform_version: 1,
    }
}

#[test]
fn approved_schema_change_reconstructs_fresh_owner_and_rejects_before_factories() {
    let (source_kernel, source_module) = kernel();
    let host = Host::new(Default::default()).unwrap();
    let owner = host
        .enroll(
            &source_kernel,
            OwnerSpec {
                module: source_module,
                schema: [1; 32],
                ports: ports(),
            },
            3,
            Backend::Native(Rc::new(Codec(1))),
        )
        .unwrap();
    let old = owner.id();
    let mut capture = host.capture_state().unwrap();
    capture.owner(&owner, &Codec(1)).unwrap();
    let bytes = capture.finish().unwrap().to_bytes(1_000_000).unwrap();
    drop(owner);
    drop(host);
    drop(source_kernel);

    let mut registry = MigrationRegistry::default();
    registry
        .register(Migration {
            binding: binding(),
            decode: |bytes: &[u8], _: &Cell<usize>| {
                Ok(u64::from_le_bytes(
                    bytes.try_into().map_err(|_| owners::Error::Invalid)?,
                ))
            },
            validate_source: |old, _| Codec(1).validate(old),
            transform: |old, count| {
                count.set(count.get() + 1);
                Ok(old * 10)
            },
            validate_target: |new, _| Codec(2).validate(new),
        })
        .unwrap();
    let (fresh, module) = kernel();
    let bindings = [(fresh.artifact(module).unwrap(), module)].into();
    let count = Cell::new(0);
    let factories = Cell::new(0);
    // Source corruption, wrong old codec, unavailable registration, bad target,
    // then a successful source-disposed roundtrip through the same public seam.
    for case in 0..5 {
        let mut checkpoint = Checkpoint::from_bytes(&bytes, 1_000_000).unwrap();
        if case == 0 || case == 3 {
            let mut value = serde_json::to_value(&checkpoint).unwrap();
            let amount: u64 = if case == 0 { 101 } else { 6 };
            value["owners"][0]["payload"] =
                serde_json::to_value(amount.to_le_bytes().to_vec()).unwrap();
            checkpoint = serde_json::from_value(value).unwrap();
        }
        let mut restore = Restore::new(
            &fresh,
            checkpoint,
            Default::default(),
            Default::default(),
            &bindings,
        )
        .unwrap();
        let mut selected = binding();
        if case == 2 {
            selected.transform_version += 1;
        }
        count.set(0);
        factories.set(0);
        let result = restore.migrate(
            &fresh,
            old,
            module,
            &Codec(if case == 1 { 2 } else { 1 }),
            &Codec(2),
            |state, _| {
                registry
                    .transform(selected, state, &count)
                    .map_err(|_| owners::Error::Schema)
            },
            || {
                factories.set(factories.get() + 1);
                Ok(Backend::Native(Rc::new(Codec(2))))
            },
        );
        if case < 4 {
            assert!(result.is_err());
            assert_eq!(factories.get(), 0);
            assert_eq!(count.get(), usize::from(case == 3));
        } else {
            let restored = result.unwrap();
            assert_eq!(restored.inspect(), Err(owners::Error::Busy));
            let _host = restore.finish().unwrap();
            assert_ne!(restored.id(), old);
            assert_eq!(restored.stamp().unwrap().schema, [2; 32]);
            assert_eq!(restored.inspect().unwrap(), 30u64.to_le_bytes());
            assert_eq!(count.get(), 1);
            assert_eq!(factories.get(), 1);
        }
    }
}
