//! In-memory rule pack sources: parse, read from disk, export vanilla.

use std::collections::BTreeMap;
use std::path::Path;

use nethacked_data::ruleset::Ruleset;

use crate::format::{
    ItemPatch, ItemsToml, MonsterPatch, MonstersToml, PackDir, PackToml, RolePatch, RolesToml,
};
use crate::validate::Diagnostic;
use crate::PackError;

/// Pack source files keyed by `/`-separated relative path (`pack.toml`,
/// `monsters.toml`, `items.toml`, `roles.toml`, `mechanics.toml`,
/// `i18n/<lang>.toml`), valued by UTF-8 text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackFiles(pub BTreeMap<String, String>);

impl PackFiles {
    /// Normalize keys: `\` -> `/`, drop a leading `./`, and strip a single
    /// top-level directory shared by every file when `pack.toml` is not at
    /// the root (a browser folder upload yields `my-pack/pack.toml`).
    fn normalized(&self) -> BTreeMap<String, String> {
        let mut m: BTreeMap<String, String> = self
            .0
            .iter()
            .map(|(k, v)| {
                let k = k.replace('\\', "/");
                let k = k.strip_prefix("./").unwrap_or(&k).to_string();
                let v = v
                    .strip_prefix('\u{feff}')
                    .unwrap_or(v)
                    .replace("\r\n", "\n");
                (k, v)
            })
            .collect();
        if !m.contains_key("pack.toml") {
            let roots: std::collections::BTreeSet<&str> = m
                .keys()
                .filter_map(|k| k.split_once('/').map(|(r, _)| r))
                .collect();
            if roots.len() == 1 && m.keys().all(|k| k.contains('/')) {
                let root = format!("{}/", roots.into_iter().next().unwrap());
                m = m
                    .into_iter()
                    .map(|(k, v)| (k[root.len()..].to_string(), v))
                    .collect();
            }
        }
        m
    }
}

fn ident_ok(s: &str, max: usize, extra: &[char]) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphanumeric())
        && s.len() <= max
        && chars.all(|c| c.is_ascii_alphanumeric() || extra.contains(&c))
}

/// Pack `id` and `version` end up in file names (`<id>-<version>.nhpack`) and
/// UI labels, so restrict them to safe slugs: no path separators, no leading
/// dot. `id`: `[A-Za-z0-9][A-Za-z0-9._-]{0,63}`; `version`:
/// `[A-Za-z0-9][A-Za-z0-9.+-]{0,31}`.
pub(crate) fn check_manifest_ident(id: &str, version: &str) -> Result<(), PackError> {
    let bad = |field: &str, value: &str, rule: &str| PackError::Toml {
        file: "pack.toml".into(),
        message: format!("invalid {field} {value:?}: must match {rule}"),
    };
    if !ident_ok(id, 64, &['.', '_', '-']) {
        return Err(bad("id", id, "[A-Za-z0-9][A-Za-z0-9._-]{0,63}"));
    }
    if !ident_ok(version, 32, &['.', '+', '-']) {
        return Err(bad("version", version, "[A-Za-z0-9][A-Za-z0-9.+-]{0,31}"));
    }
    Ok(())
}

fn parse_toml<T: serde::de::DeserializeOwned>(file: &str, text: &str) -> Result<T, PackError> {
    toml::from_str(text).map_err(|e| PackError::Toml {
        file: file.into(),
        message: e.to_string(),
    })
}

/// Parse pack sources into a [`PackDir`]. Same rules as the on-disk reader.
pub fn parse_pack_files(files: &PackFiles) -> Result<PackDir, PackError> {
    let m = files.normalized();
    let manifest_text = m
        .get("pack.toml")
        .ok_or_else(|| PackError::Io("Missing manifest file pack.toml".into()))?;
    let manifest: PackToml = parse_toml("pack.toml", manifest_text)?;
    if manifest.base != "vanilla" {
        return Err(PackError::Toml {
            file: "pack.toml".into(),
            message: "only base = \"vanilla\" is supported in this version".into(),
        });
    }
    check_manifest_ident(&manifest.id, &manifest.version)?;

    let monsters = match m.get("monsters.toml") {
        Some(t) => parse_toml::<MonstersToml>("monsters.toml", t)?.monster,
        None => Vec::new(),
    };
    let items = match m.get("items.toml") {
        Some(t) => parse_toml::<ItemsToml>("items.toml", t)?.item,
        None => Vec::new(),
    };
    let roles = match m.get("roles.toml") {
        Some(t) => {
            let parsed: RolesToml = parse_toml("roles.toml", t)?;
            for r in &parsed.role {
                if r.new == Some(true) || r.remove == Some(true) {
                    return Err(PackError::Resolve(Diagnostic {
                        file: "roles.toml".into(),
                        entry: r.name.clone(),
                        field: None,
                        message: "roles cannot be added or removed in P1, only patched".into(),
                    }));
                }
            }
            parsed.role
        }
        None => Vec::new(),
    };
    if let Some(t) = m.get("mechanics.toml") {
        let has_content = t
            .lines()
            .any(|l| !l.trim().is_empty() && !l.trim().starts_with('#'));
        if has_content {
            return Err(PackError::Toml {
                file: "mechanics.toml".into(),
                message: "mechanics knobs arrive in P2".into(),
            });
        }
    }

    let mut i18n = BTreeMap::new();
    for (k, t) in &m {
        if let Some(stem) = k
            .strip_prefix("i18n/")
            .and_then(|rest| rest.strip_suffix(".toml"))
            .filter(|s| !s.contains('/'))
        {
            let lang_map: BTreeMap<String, String> = parse_toml(k, t)?;
            i18n.insert(stem.to_string(), lang_map);
        }
    }

    Ok(PackDir {
        manifest,
        monsters,
        items,
        roles,
        i18n,
    })
}

/// Read the recognized pack files from `dir` into memory.
pub fn read_pack_files_from_dir(dir: &Path) -> Result<PackFiles, PackError> {
    let read = |p: &Path| {
        std::fs::read_to_string(p).map_err(|e| PackError::Io(format!("{}: {e}", p.display())))
    };
    let manifest_path = dir.join("pack.toml");
    if !manifest_path.exists() {
        return Err(PackError::Io(format!(
            "Missing manifest file at {}",
            manifest_path.display()
        )));
    }
    let mut m = BTreeMap::new();
    for name in [
        "pack.toml",
        "monsters.toml",
        "items.toml",
        "roles.toml",
        "mechanics.toml",
    ] {
        let p = dir.join(name);
        if p.exists() {
            m.insert(name.to_string(), read(&p)?);
        }
    }
    let i18n_dir = dir.join("i18n");
    if i18n_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&i18n_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) == Some("toml") {
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        m.insert(format!("i18n/{stem}.toml"), read(&p)?);
                    }
                }
            }
        }
    }
    Ok(PackFiles(m))
}

/// The vanilla ruleset written out as full patch files (`export-vanilla`).
pub fn vanilla_pack_files() -> PackFiles {
    let vanilla = Ruleset::vanilla();
    let manifest = PackToml {
        id: "vanilla-export".into(),
        name: "Vanilla Export".into(),
        version: "0.1.0".into(),
        base: "vanilla".into(),
        description: Some("Exported vanilla NetHackED data".into()),
    };
    let monsters = MonstersToml {
        monster: vanilla
            .monsters
            .iter()
            .map(|m| MonsterPatch {
                name: m.name.clone(),
                new: false,
                remove: false,
                glyph: Some(m.glyph),
                base_hp: Some(m.base_hp),
                max_hp: Some(m.max_hp),
                ac: Some(m.ac),
                level: Some(m.level),
                speed: Some(m.speed),
                alignment: Some(m.alignment),
                intrinsics: Some(m.intrinsics),
                attacks: Some(m.attacks.clone()),
                size: Some(m.size),
                peaceful_by_default: Some(m.peaceful_by_default),
                always_hostile: Some(m.always_hostile),
                maligntyp: Some(m.maligntyp),
                msound: Some(m.msound),
                m2_race: m.m2_race.map(Some),
                is_human: Some(m.is_human),
                is_unique: Some(m.is_unique),
                mindless: Some(m.mindless),
                ai_behavior: Some(m.ai_behavior),
                abilities: Some(m.abilities.clone()),
                difficulty: Some(m.difficulty),
                frequency: Some(m.frequency),
                gen_flags: Some(m.gen_flags.clone()),
                flags: Some(m.flags.clone()),
            })
            .collect(),
    };
    let items = ItemsToml {
        item: vanilla
            .items
            .iter()
            .map(|i| ItemPatch {
                name: i.name.clone(),
                new: false,
                remove: false,
                class: Some(i.class),
                weight: Some(i.weight),
                cost: Some(i.cost),
                damage_small: Some(i.damage_small),
                damage_large: Some(i.damage_large),
                ac_bonus: Some(i.ac_bonus),
                is_container: Some(i.is_container),
                is_bag_of_holding: Some(i.is_bag_of_holding),
                oc_magic: Some(i.oc_magic),
                wand_dir: i.wand_dir.map(Some),
                nutrition: Some(i.nutrition),
                armor: i.armor.clone().map(Some),
            })
            .collect(),
    };
    let roles = RolesToml {
        role: vanilla
            .roles
            .iter()
            .map(|r| RolePatch {
                name: r.name.clone(),
                new: None,
                remove: None,
                base_hp: Some(r.base_hp),
                ac: Some(r.ac),
                speed: Some(r.speed),
                default_alignment: Some(r.default_alignment),
                starting_items: Some(r.starting_items.clone()),
                skills: Some(r.skills.clone()),
                pantheon: Some(r.pantheon.clone()),
                quest: r.quest.clone().map(Some),
                initial_alignment_record: Some(r.initial_alignment_record),
            })
            .collect(),
    };
    let mut m = BTreeMap::new();
    m.insert(
        "pack.toml".into(),
        toml::to_string_pretty(&manifest).expect("manifest toml"),
    );
    m.insert(
        "monsters.toml".into(),
        toml::to_string_pretty(&monsters).expect("monsters toml"),
    );
    m.insert(
        "items.toml".into(),
        toml::to_string_pretty(&items).expect("items toml"),
    );
    m.insert(
        "roles.toml".into(),
        toml::to_string_pretty(&roles).expect("roles toml"),
    );
    PackFiles(m)
}
