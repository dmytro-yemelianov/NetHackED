use nethacked_persistence::migration::*;
use serde::Deserialize;
use std::cell::Cell;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V1 {
    quantity: u32,
    price: u32,
}
#[derive(Debug, PartialEq, Eq)]
struct V2 {
    quantity: u64,
    total: u64,
}
#[derive(Default)]
struct Calls {
    decode: Cell<usize>,
    transform: Cell<usize>,
}
type Registry = MigrationRegistry<V1, V2, Calls, &'static str>;

fn binding() -> MigrationBinding {
    MigrationBinding {
        source: FormatIdentity {
            schema: [1; 32],
            codec_artifact: [2; 32],
            codec_version: 1,
        },
        target: FormatIdentity {
            schema: [3; 32],
            codec_artifact: [4; 32],
            codec_version: 2,
        },
        transform_artifact: [5; 32],
        transform_version: 1,
    }
}
fn entry() -> Migration<V1, V2, Calls, &'static str> {
    Migration {
        binding: binding(),
        decode: |bytes, calls| {
            calls.decode.set(calls.decode.get() + 1);
            serde_json::from_slice(bytes).map_err(|_| "decode")
        },
        validate_source: |old, _| {
            if old.quantity == 0 {
                Err("quantity")
            } else {
                Ok(())
            }
        },
        transform: |old, calls| {
            calls.transform.set(calls.transform.get() + 1);
            Ok(V2 {
                quantity: u64::from(old.quantity),
                total: u64::from(old.quantity) * u64::from(old.price),
            })
        },
        validate_target: |new, _| {
            if new.total > 100 {
                Err("total")
            } else {
                Ok(())
            }
        },
    }
}

#[test]
fn supported_schema_change_validates_both_sides_before_returning_state() {
    let mut registry = Registry::default();
    registry.register(entry()).unwrap();
    let calls = Calls::default();
    for bytes in [b"{".as_slice(), br#"{"quantity":0,"price":9}"#] {
        assert!(matches!(
            registry.migrate(binding(), bytes, 128, &calls),
            Err(MigrationError::Source(_))
        ));
    }
    assert_eq!(calls.transform.get(), 0);
    assert_eq!(
        registry.migrate(binding(), br#"{"quantity":3,"price":9}"#, 128, &calls),
        Ok(V2 {
            quantity: 3,
            total: 27
        })
    );
    assert_eq!(calls.transform.get(), 1);
    assert_eq!(
        registry.migrate(binding(), br#"{"quantity":30,"price":9}"#, 128, &calls),
        Err(MigrationError::Target("total"))
    );
    assert_eq!(calls.transform.get(), 2);
}

#[test]
fn missing_or_changed_exact_binding_and_oversize_never_decode() {
    let calls = Calls::default();
    let mut registry = Registry::default();
    assert_eq!(
        registry.migrate(binding(), b"{", 128, &calls),
        Err(MigrationError::Unavailable)
    );
    registry.register(entry()).unwrap();
    assert_eq!(registry.register(entry()), Err(MigrationError::Duplicate));
    for change in 0..8 {
        let mut b = binding();
        match change {
            0 => b.source.schema[0] ^= 1,
            1 => b.source.codec_artifact[0] ^= 1,
            2 => b.source.codec_version += 1,
            3 => b.target.schema[0] ^= 1,
            4 => b.target.codec_artifact[0] ^= 1,
            5 => b.target.codec_version += 1,
            6 => b.transform_artifact[0] ^= 1,
            7 => b.transform_version += 1,
            _ => unreachable!(),
        }
        assert_eq!(
            registry.migrate(b, b"{", 128, &calls),
            Err(MigrationError::Unavailable)
        );
    }
    assert_eq!(
        registry.migrate(binding(), b"{", 0, &calls),
        Err(MigrationError::InputLimit)
    );
    assert_eq!(calls.decode.get(), 0);
    assert_eq!(calls.transform.get(), 0);
}
