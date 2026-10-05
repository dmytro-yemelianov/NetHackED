//! Local packs directory: install, list, resolve by id, summarize.

use std::path::{Path, PathBuf};

use nethacked_data::ruleset::{PackManifest, Ruleset};

use crate::nhpack::{build, diff, load_nhpack_bytes, nhpack_to_bytes, NhPack};
use crate::PackError;

/// `$NETHACKED_PACKS_DIR`, else `$HOME/.nethacked/packs`.
pub fn default_packs_dir() -> Option<PathBuf> {
    // `NETRUST_PACKS_DIR` and `~/.netrust/packs` are the pre-rename (NetRust) names.
    for var in ["NETHACKED_PACKS_DIR", "NETRUST_PACKS_DIR"] {
        if let Some(d) = std::env::var_os(var) {
            return Some(PathBuf::from(d));
        }
    }
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let current = home.join(".nethacked").join("packs");
    let legacy = home.join(".netrust").join("packs");
    Some(if !current.exists() && legacy.is_dir() {
        legacy
    } else {
        current
    })
}

/// Pack file extensions: `.nhpack`, and the pre-rename `.nrpack`.
pub const PACK_EXTENSIONS: [&str; 2] = ["nhpack", "nrpack"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledPack {
    pub id: String,
    pub version: String,
    pub hash: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct PackInfo {
    pub manifest: PackManifest,
    pub hash: String,
    pub monsters: usize,
    pub items: usize,
    pub roles: usize,
    pub warnings: usize,
    pub diffs_vs_vanilla: usize,
}

/// Numeric semver key; non-numeric parts sort as 0.
fn semver_key(v: &str) -> Vec<u64> {
    v.split(['.', '-', '+'])
        .take(3)
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

/// Load a pack from a `.nhpack` file or build it from a directory.
fn load_any(src: &Path) -> Result<(NhPack, usize), PackError> {
    if src.is_dir() {
        let (p, report) = build(src)?;
        Ok((p, report.warnings.len()))
    } else {
        let bytes =
            std::fs::read(src).map_err(|e| PackError::Io(format!("{}: {e}", src.display())))?;
        let (_, _, p) = load_nhpack_bytes(&bytes)?;
        Ok((p, 0))
    }
}

pub fn list_installed(dir: &Path) -> Result<Vec<InstalledPack>, PackError> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(PackError::Io(format!("{}: {e}", dir.display()))),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|e| PACK_EXTENSIONS.contains(&e))
        {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok((_, rref, _)) = load_nhpack_bytes(&bytes) else {
            continue;
        };
        out.push(InstalledPack {
            id: rref.id,
            version: rref.version,
            hash: rref.hash,
            path,
        });
    }
    out.sort_by(|a, b| (&a.id, semver_key(&a.version)).cmp(&(&b.id, semver_key(&b.version))));
    Ok(out)
}

pub fn resolve_installed(dir: &Path, id: &str) -> Option<InstalledPack> {
    list_installed(dir)
        .ok()?
        .into_iter()
        .filter(|p| p.id == id)
        .max_by_key(|p| semver_key(&p.version))
}

pub fn install_pack(src: &Path, dir: &Path, force: bool) -> Result<InstalledPack, PackError> {
    let (p, _) = load_any(src)?;
    std::fs::create_dir_all(dir).map_err(|e| PackError::Io(format!("{}: {e}", dir.display())))?;
    let file_name = format!("{}-{}.nhpack", p.manifest.id, p.manifest.version);
    let path = dir.join(&file_name);
    // load_any already enforced safe id/version; never write outside `dir`.
    if path.parent() != Some(dir) || file_name.contains(['/', '\\']) {
        return Err(PackError::Io(format!(
            "unsafe pack file name {file_name:?}"
        )));
    }
    if path.exists() && !force {
        let existing = std::fs::read(&path)
            .ok()
            .and_then(|b| load_nhpack_bytes(&b).ok());
        match existing {
            Some((_, rref, _)) if rref.hash == p.hash => {}
            _ => {
                return Err(PackError::Conflict(format!(
                    "{} {} is already installed with a different hash (use --force)",
                    p.manifest.id, p.manifest.version
                )))
            }
        }
    }
    std::fs::write(&path, nhpack_to_bytes(&p))
        .map_err(|e| PackError::Io(format!("{}: {e}", path.display())))?;
    Ok(InstalledPack {
        id: p.manifest.id,
        version: p.manifest.version,
        hash: p.hash,
        path,
    })
}

pub fn pack_info(src: &Path) -> Result<PackInfo, PackError> {
    let (p, warnings) = load_any(src)?;
    Ok(PackInfo {
        diffs_vs_vanilla: diff(&Ruleset::vanilla(), &p.ruleset).len(),
        monsters: p.ruleset.monsters.len(),
        items: p.ruleset.items.len(),
        roles: p.ruleset.roles.len(),
        warnings,
        hash: p.hash,
        manifest: p.manifest,
    })
}
