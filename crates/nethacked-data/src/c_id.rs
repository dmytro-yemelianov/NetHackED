//! Table-index ids named like the C enums (`PM_*`, `objects[]`, `ART_*`).
//!
//! An id is the row index in its generated table. Code names specific rows by
//! the C constant (`MonsterSpeciesId::ALIGNED_CLERIC`); data files and packs
//! refer to them by the same C name as a string, so serialized data does not
//! depend on table order.

/// Define an index newtype with C-name (de)serialization.
macro_rules! c_id {
    ($ty:ident, $count:expr) => {
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $ty(pub u16);

        impl $ty {
            /// Number of ids in the generated table.
            pub const COUNT: usize = $count;

            /// Row index in the generated table.
            pub const fn index(self) -> usize {
                self.0 as usize
            }

            /// The C enum name (`"ALIGNED_CLERIC"`, `"WAN_WISHING"`).
            pub fn c_name(self) -> &'static str {
                Self::C_NAMES[self.index()]
            }

            /// Look up an id by its C enum name.
            pub fn from_c_name(name: &str) -> Option<Self> {
                Self::C_NAMES
                    .iter()
                    .position(|n| *n == name)
                    .map(|i| Self(i as u16))
            }

            /// Every id, in table order.
            pub fn all() -> impl Iterator<Item = Self> {
                (0..Self::COUNT as u16).map(Self)
            }
        }

        impl std::fmt::Debug for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.c_name())
            }
        }

        impl serde::Serialize for $ty {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.c_name())
            }
        }

        impl<'de> serde::Deserialize<'de> for $ty {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let name = <std::borrow::Cow<'de, str> as serde::Deserialize>::deserialize(d)?;
                Self::from_c_name(&name).ok_or_else(|| {
                    serde::de::Error::custom(format!(
                        concat!("unknown ", stringify!($ty), " {:?}"),
                        name
                    ))
                })
            }
        }

        #[cfg(feature = "schema")]
        impl schemars::JsonSchema for $ty {
            fn schema_name() -> String {
                stringify!($ty).to_string()
            }

            fn json_schema(_: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
                schemars::schema::SchemaObject {
                    instance_type: Some(schemars::schema::InstanceType::String.into()),
                    enum_values: Some(Self::C_NAMES.iter().map(|n| (*n).into()).collect()),
                    ..Default::default()
                }
                .into()
            }
        }
    };
}
