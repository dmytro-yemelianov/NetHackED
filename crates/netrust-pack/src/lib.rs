//! Rule pack library, parser, patch merger, validator, and pack format for NetRust.

pub mod format;
pub mod merge;
pub mod nrpack;
pub mod validate;

pub use format::{
    read_pack_dir, ItemPatch, ItemsToml, MonsterPatch, MonstersToml, PackDir, PackToml, RolePatch,
    RolesToml,
};
pub use merge::resolve;
pub use nrpack::{
    build, diff, load_nrpack, ruleset_hash, write_nrpack, DiffEntry, NrPack, PACK_FORMAT_VERSION,
};
pub use validate::{validate, Diagnostic, Report};

/// Enumeration of all rule pack parsing, resolution, validation, or integrity errors.
#[derive(Debug)]
pub enum PackError {
    Io(String),
    Toml { file: String, message: String },
    Resolve(Diagnostic),
    Invalid(Report),
    Hash { expected: String, found: String },
    Format(u32),
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "IO error: {msg}"),
            Self::Toml { file, message } => write!(f, "TOML parse error in {file}: {message}"),
            Self::Resolve(diag) => write!(
                f,
                "Resolution error in {}{}: {}",
                diag.file,
                diag.field
                    .as_deref()
                    .map(|f| format!(".{f}"))
                    .unwrap_or_default(),
                diag.message
            ),
            Self::Invalid(report) => {
                write!(f, "Validation failed with {} error(s)", report.errors.len())
            }
            Self::Hash { expected, found } => {
                write!(
                    f,
                    "Ruleset hash mismatch: expected {expected}, found {found}"
                )
            }
            Self::Format(v) => write!(f, "Unsupported rule pack format version: {v}"),
        }
    }
}

impl std::error::Error for PackError {}

impl From<std::io::Error> for PackError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}
