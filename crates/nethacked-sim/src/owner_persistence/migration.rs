//! Approved production native parser and normalization, before timer catchup.
use super::{NativePartitions, PreparedNativeImport};
use crate::{LoadError, Ruleset, RulesetRef, SimulationWorld};
use nethacked_persistence::migration::{
    FormatIdentity, Migration, MigrationBinding, MigrationError, MigrationRegistry,
};
use sha2::{Digest, Sha256};
use std::{rc::Rc, sync::Arc};

struct Context {
    ruleset: Arc<Ruleset>,
    reference: RulesetRef,
}
type Source = (SimulationWorld, bool, bool);
type Registry = MigrationRegistry<Source, PreparedNativeImport, Context, LoadError>;

fn digest(parts: &[&[u8]]) -> [u8; 32] {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    hash.finalize().into()
}

/// Identity of the linked native compatibility implementation, never read from
/// guest JSON. Schema versions describe the full accepted absence/default
/// policy, not a new required field in old saves. The codec source manifest and
/// dependency lock bind this build's parser; approved RulesetRef is checked by
/// its production decoder for every input.
pub fn native_migration_binding() -> MigrationBinding {
    static BINDING: std::sync::OnceLock<MigrationBinding> = std::sync::OnceLock::new();
    *BINDING.get_or_init(compiled_binding)
}

fn compiled_binding() -> MigrationBinding {
    let parser = digest(&[
        include_bytes!("../world.rs"),
        include_bytes!("../../../nethacked-arena/src/lib.rs"),
        include_bytes!("../../../nethacked-types/src/lib.rs"),
        include_bytes!("../../../../Cargo.lock"),
    ]);
    let target = digest(&[
        &parser,
        include_bytes!("partitions.rs"),
        include_bytes!("admission.rs"),
    ]);
    MigrationBinding {
        source: FormatIdentity {
            schema: digest(&[b"NetHackED native serde legacy/default policy v1"]),
            codec_artifact: parser,
            codec_version: 1,
        },
        target: FormatIdentity {
            schema: digest(&[b"NetHackED nine native owner partitions v1"]),
            codec_artifact: target,
            codec_version: 1,
        },
        transform_artifact: digest(&[
            &parser,
            &target,
            include_bytes!("migration.rs"),
            include_bytes!("../object_timers.rs"),
            include_bytes!("../actions/equipment.rs"),
        ]),
        transform_version: 1,
    }
}

/// Host configuration holds only compiled functions and exact identities, no
/// World, source owner, mutable state, or guest-selected executable closure.
#[derive(Clone)]
pub struct NativeImportRegistry(Rc<Registry>);

impl Default for NativeImportRegistry {
    /// An empty registry deliberately supports no native imports.
    fn default() -> Self {
        Self(Rc::new(Registry::default()))
    }
}

impl NativeImportRegistry {
    pub fn current() -> Self {
        let mut registry = Registry::default();
        registry
            .register(Migration {
                binding: native_migration_binding(),
                decode: |bytes, context| {
                    let json =
                        std::str::from_utf8(bytes).map_err(|e| LoadError::Json(e.to_string()))?;
                    // Includes original old-schema receipt/queue validation.
                    // Invalid old saves cannot reach the transformation.
                    SimulationWorld::decode_native_import(
                        json,
                        context.ruleset.clone(),
                        &context.reference,
                    )
                },
                validate_source: |(world, _, _), context| {
                    if world.ruleset_ref != context.reference {
                        return Err(LoadError::RulesetMismatch {
                            expected: Box::new(context.reference.clone()),
                            found: Box::new(world.ruleset_ref.clone()),
                        });
                    }
                    Ok(())
                },
                transform: |source, _| {
                    #[cfg(test)]
                    TRANSFORMS.with(|n| n.set(n.get() + 1));
                    Ok(PreparedNativeImport {
                        world: SimulationWorld::normalize_native_import(source)?,
                    })
                },
                validate_target: |prepared, context| {
                    NativePartitions::from_native(prepared.world.clone())
                        .admit_references(&context.ruleset)
                        .map_err(|e| LoadError::Json(format!("native owner references: {e:?}")))?;
                    Ok(())
                },
            })
            .expect("compiled native migration binding is valid and unique");
        Self(Rc::new(registry))
    }

    /// Prepare only. Due timers remain hidden and undispatched until the
    /// returned handle is consumed by `complete` or `partition`.
    pub fn prepare(
        &self,
        binding: MigrationBinding,
        json: &str,
        byte_limit: usize,
        ruleset: Arc<Ruleset>,
        reference: &RulesetRef,
    ) -> Result<PreparedNativeImport, MigrationError<LoadError>> {
        self.0.migrate(
            binding,
            json.as_bytes(),
            byte_limit,
            &Context {
                ruleset,
                reference: reference.clone(),
            },
        )
    }
}

#[cfg(test)]
thread_local! {
    pub(crate) static TRANSFORMS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests;
