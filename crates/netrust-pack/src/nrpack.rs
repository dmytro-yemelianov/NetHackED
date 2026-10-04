//! NrPack container format, canonical serialization, hashing, load/build, and diffing.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;

use netrust_data::ruleset::{PackManifest, Ruleset, RulesetRef};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::format::read_pack_dir;
use crate::merge::resolve;
use crate::validate::{validate, Diagnostic, Report};
use crate::PackError;

/// Number of the .nrpack format specification.
pub const PACK_FORMAT_VERSION: u32 = 1;

/// Packed rule pack bundle containing the manifest, resolved ruleset, and SHA-256 hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NrPack {
    pub format: u32,
    pub manifest: PackManifest,
    pub ruleset: Ruleset,
    pub hash: String,
}

/// Compute the deterministic SHA-256 hash ("sha256:<hex>") of a Ruleset's canonical JSON.
pub fn ruleset_hash(rs: &Ruleset) -> String {
    let val = serde_json::to_value(rs).expect("Ruleset must serialize to serde_json::Value");
    let canonical_bytes =
        serde_json::to_vec(&val).expect("serde_json::Value must serialize to vec");
    let mut hasher = Sha256::new();
    hasher.update(&canonical_bytes);
    format!("sha256:{:x}", hasher.finalize())
}

/// Builds an NrPack from a pack directory: reads, resolves against vanilla, validates, and warns on missing i18n.
pub fn build(dir: &Path) -> Result<(NrPack, Report), PackError> {
    let pack = read_pack_dir(dir)?;
    let base = Ruleset::vanilla();
    let ruleset = resolve(&pack, &base)?;
    let mut report = validate(&ruleset);

    // Warnings: added entries without a translation in each present i18n file
    if !pack.i18n.is_empty() {
        let added_monsters: Vec<&str> = pack
            .monsters
            .iter()
            .filter(|m| m.new)
            .map(|m| m.name.as_str())
            .collect();
        let added_items: Vec<&str> = pack
            .items
            .iter()
            .filter(|i| i.new)
            .map(|i| i.name.as_str())
            .collect();

        for (lang, translations) in &pack.i18n {
            let file = format!("i18n/{lang}.toml");
            for name in &added_monsters {
                if !translations.contains_key(*name) {
                    report.warnings.push(Diagnostic {
                        file: file.clone(),
                        entry: (*name).to_string(),
                        field: None,
                        message: format!("missing translation for added monster: {name}"),
                    });
                }
            }
            for name in &added_items {
                if !translations.contains_key(*name) {
                    report.warnings.push(Diagnostic {
                        file: file.clone(),
                        entry: (*name).to_string(),
                        field: None,
                        message: format!("missing translation for added item: {name}"),
                    });
                }
            }
        }
    }

    if report.has_errors() {
        return Err(PackError::Invalid(report));
    }

    let hash = ruleset_hash(&ruleset);
    let nrpack = NrPack {
        format: PACK_FORMAT_VERSION,
        manifest: ruleset.manifest.clone(),
        ruleset,
        hash,
    };

    Ok((nrpack, report))
}

/// Serializes and writes an NrPack as canonical formatted JSON to the target output file.
pub fn write_nrpack(p: &NrPack, out: &Path) -> io::Result<()> {
    let val = serde_json::to_value(p).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let json_bytes = serde_json::to_vec_pretty(&val)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    fs::write(out, json_bytes)
}

/// Loads an NrPack from disk, verifies format and cryptographic hash integrity, and re-indexes the ruleset.
pub fn load_nrpack(path: &Path) -> Result<(Arc<Ruleset>, RulesetRef), PackError> {
    let content =
        fs::read_to_string(path).map_err(|e| PackError::Io(format!("{}: {e}", path.display())))?;
    let nrpack: NrPack = serde_json::from_str(&content)
        .map_err(|e| PackError::Io(format!("Failed to parse {}: {e}", path.display())))?;

    if nrpack.format != PACK_FORMAT_VERSION {
        return Err(PackError::Format(nrpack.format));
    }

    let computed = ruleset_hash(&nrpack.ruleset);
    if computed != nrpack.hash {
        return Err(PackError::Hash {
            expected: nrpack.hash,
            found: computed,
        });
    }

    let mut ruleset = nrpack.ruleset;
    ruleset.reindex();

    let ruleset_ref = RulesetRef {
        id: nrpack.manifest.id,
        version: nrpack.manifest.version,
        hash: nrpack.hash,
    };

    Ok((Arc::new(ruleset), ruleset_ref))
}

/// A single field difference between two Rulesets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffEntry {
    pub table: String,
    pub entry: String,
    pub field: String,
    pub before: String,
    pub after: String,
}

/// Computes the diff between two Rulesets, sorted deterministically by (table, entry, field).
pub fn diff(a: &Ruleset, b: &Ruleset) -> Vec<DiffEntry> {
    let mut diffs = Vec::new();

    // Manifest
    let a_m = serde_json::to_value(&a.manifest).unwrap();
    let b_m = serde_json::to_value(&b.manifest).unwrap();
    if let (serde_json::Value::Object(a_obj), serde_json::Value::Object(b_obj)) = (a_m, b_m) {
        diff_json_objects("manifest", &a.manifest.id, &a_obj, &b_obj, &mut diffs);
    }

    // Monsters
    diff_named_list(
        "monsters",
        &a.monsters,
        &b.monsters,
        |m| &m.name,
        &mut diffs,
    );

    // Items
    diff_named_list("items", &a.items, &b.items, |i| &i.name, &mut diffs);

    // Roles
    diff_named_list("roles", &a.roles, &b.roles, |r| &r.name, &mut diffs);

    // Races
    diff_named_list("races", &a.races, &b.races, |r| &r.name, &mut diffs);

    diffs.sort_by(|x, y| (&x.table, &x.entry, &x.field).cmp(&(&y.table, &y.entry, &y.field)));

    diffs
}

fn diff_json_objects(
    table: &str,
    entry: &str,
    a_obj: &serde_json::Map<String, serde_json::Value>,
    b_obj: &serde_json::Map<String, serde_json::Value>,
    diffs: &mut Vec<DiffEntry>,
) {
    let keys: BTreeSet<&String> = a_obj.keys().chain(b_obj.keys()).collect();
    for k in keys {
        let v_a = a_obj.get(k);
        let v_b = b_obj.get(k);
        if v_a != v_b {
            let before = v_a
                .map(|v| v.to_string())
                .unwrap_or_else(|| "<absent>".into());
            let after = v_b
                .map(|v| v.to_string())
                .unwrap_or_else(|| "<absent>".into());
            diffs.push(DiffEntry {
                table: table.to_string(),
                entry: entry.to_string(),
                field: k.clone(),
                before,
                after,
            });
        }
    }
}

fn diff_named_list<T: Serialize>(
    table: &str,
    a_list: &[T],
    b_list: &[T],
    name_fn: impl Fn(&T) -> &str,
    diffs: &mut Vec<DiffEntry>,
) {
    let mut a_map: BTreeMap<String, &T> = BTreeMap::new();
    for item in a_list {
        a_map.insert(name_fn(item).to_ascii_lowercase(), item);
    }
    let mut b_map: BTreeMap<String, &T> = BTreeMap::new();
    for item in b_list {
        b_map.insert(name_fn(item).to_ascii_lowercase(), item);
    }

    let all_names: BTreeSet<&String> = a_map.keys().chain(b_map.keys()).collect();
    for name_key in all_names {
        match (a_map.get(name_key), b_map.get(name_key)) {
            (Some(a_item), Some(b_item)) => {
                let a_val = serde_json::to_value(a_item).unwrap();
                let b_val = serde_json::to_value(b_item).unwrap();
                if let (serde_json::Value::Object(a_obj), serde_json::Value::Object(b_obj)) =
                    (a_val, b_val)
                {
                    diff_json_objects(table, name_fn(a_item), &a_obj, &b_obj, diffs);
                }
            }
            (Some(a_item), None) => {
                diffs.push(DiffEntry {
                    table: table.to_string(),
                    entry: name_fn(a_item).to_string(),
                    field: "<removed>".into(),
                    before: "<present>".into(),
                    after: "<absent>".into(),
                });
            }
            (None, Some(b_item)) => {
                diffs.push(DiffEntry {
                    table: table.to_string(),
                    entry: name_fn(b_item).to_string(),
                    field: "<added>".into(),
                    before: "<absent>".into(),
                    after: "<present>".into(),
                });
            }
            (None, None) => unreachable!(),
        }
    }
}
