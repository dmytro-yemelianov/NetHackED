//! Rule pack directory format and TOML deserialization models.

use std::collections::BTreeMap;
use std::path::Path;

use netrust_data::ruleset::{ArmorDef, QuestDef, StartingItem};
use netrust_data::{AiBehavior, MonsterSize, MonsterSound, RaceId, WandDir};
use netrust_types::{
    Alignment, Attack, Intrinsics, ItemClass, MonsterAbility, SkillClass, SkillLevel,
};
use serde::{Deserialize, Serialize};

use crate::{Diagnostic, PackError};

/// Manifest header deserialized from `pack.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PackToml {
    pub id: String,
    pub name: String,
    pub version: String,
    pub base: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// A patch or addition entry for a monster species.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct MonsterPatch {
    pub name: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub new: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub remove: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyph: Option<char>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_hp: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_hp: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ac: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<Alignment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intrinsics: Option<Intrinsics>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attacks: Option<Vec<Attack>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<MonsterSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peaceful_by_default: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub always_hostile: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maligntyp: Option<i8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msound: Option<MonsterSound>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub m2_race: Option<Option<RaceId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_human: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_unique: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mindless: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ai_behavior: Option<AiBehavior>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abilities: Option<Vec<MonsterAbility>>,
}

/// Container for `[[monster]]` entries in `monsters.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct MonstersToml {
    #[serde(default)]
    pub monster: Vec<MonsterPatch>,
}

/// A patch or addition entry for an item kind.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ItemPatch {
    pub name: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub new: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub remove: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<ItemClass>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage_small: Option<(u32, u32)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damage_large: Option<(u32, u32)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ac_bonus: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_container: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_bag_of_holding: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oc_magic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wand_dir: Option<Option<WandDir>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nutrition: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub armor: Option<Option<ArmorDef>>,
}

/// Container for `[[item]]` entries in `items.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ItemsToml {
    #[serde(default)]
    pub item: Vec<ItemPatch>,
}

/// A patch entry for an existing player character role.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct RolePatch {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remove: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_hp: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ac: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_alignment: Option<Alignment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starting_items: Option<Vec<StartingItem>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<(SkillClass, SkillLevel)>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pantheon: Option<[String; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quest: Option<Option<QuestDef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_alignment_record: Option<i32>,
}

/// Container for `[[role]]` entries in `roles.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct RolesToml {
    #[serde(default)]
    pub role: Vec<RolePatch>,
}

/// Parsed in-memory representation of a rule pack directory.
#[derive(Debug, Clone)]
pub struct PackDir {
    pub manifest: PackToml,
    pub monsters: Vec<MonsterPatch>,
    pub items: Vec<ItemPatch>,
    pub roles: Vec<RolePatch>,
    pub i18n: BTreeMap<String, BTreeMap<String, String>>,
}

/// Read and parse all TOML documents from a rule pack directory.
pub fn read_pack_dir(dir: &Path) -> Result<PackDir, PackError> {
    let manifest_path = dir.join("pack.toml");
    if !manifest_path.exists() {
        return Err(PackError::Io(format!(
            "Missing manifest file at {}",
            manifest_path.display()
        )));
    }

    let manifest_content = std::fs::read_to_string(&manifest_path)
        .map_err(|e| PackError::Io(format!("{}: {e}", manifest_path.display())))?;
    let manifest: PackToml = toml::from_str(&manifest_content).map_err(|e| PackError::Toml {
        file: "pack.toml".into(),
        message: e.to_string(),
    })?;

    if manifest.base != "vanilla" {
        return Err(PackError::Toml {
            file: "pack.toml".into(),
            message: "only base = \"vanilla\" is supported in this version".into(),
        });
    }

    // monsters.toml
    let monsters_path = dir.join("monsters.toml");
    let monsters = if monsters_path.exists() {
        let content = std::fs::read_to_string(&monsters_path)
            .map_err(|e| PackError::Io(format!("{}: {e}", monsters_path.display())))?;
        let parsed: MonstersToml = toml::from_str(&content).map_err(|e| PackError::Toml {
            file: "monsters.toml".into(),
            message: e.to_string(),
        })?;
        parsed.monster
    } else {
        Vec::new()
    };

    // items.toml
    let items_path = dir.join("items.toml");
    let items = if items_path.exists() {
        let content = std::fs::read_to_string(&items_path)
            .map_err(|e| PackError::Io(format!("{}: {e}", items_path.display())))?;
        let parsed: ItemsToml = toml::from_str(&content).map_err(|e| PackError::Toml {
            file: "items.toml".into(),
            message: e.to_string(),
        })?;
        parsed.item
    } else {
        Vec::new()
    };

    // roles.toml
    let roles_path = dir.join("roles.toml");
    let roles = if roles_path.exists() {
        let content = std::fs::read_to_string(&roles_path)
            .map_err(|e| PackError::Io(format!("{}: {e}", roles_path.display())))?;
        let parsed: RolesToml = toml::from_str(&content).map_err(|e| PackError::Toml {
            file: "roles.toml".into(),
            message: e.to_string(),
        })?;
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
    } else {
        Vec::new()
    };

    // mechanics.toml
    let mechanics_path = dir.join("mechanics.toml");
    if mechanics_path.exists() {
        let content = std::fs::read_to_string(&mechanics_path)
            .map_err(|e| PackError::Io(format!("{}: {e}", mechanics_path.display())))?;
        let has_content = content
            .lines()
            .any(|l| !l.trim().is_empty() && !l.trim().starts_with('#'));
        if has_content {
            return Err(PackError::Toml {
                file: "mechanics.toml".into(),
                message: "mechanics knobs arrive in P2".into(),
            });
        }
    }

    // i18n/<lang>.toml
    let mut i18n = BTreeMap::new();
    let i18n_dir = dir.join("i18n");
    if i18n_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&i18n_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        let content = std::fs::read_to_string(&path)
                            .map_err(|e| PackError::Io(format!("{}: {e}", path.display())))?;
                        let lang_map: BTreeMap<String, String> =
                            toml::from_str(&content).map_err(|e| PackError::Toml {
                                file: format!("i18n/{stem}.toml"),
                                message: e.to_string(),
                            })?;
                        i18n.insert(stem.to_string(), lang_map);
                    }
                }
            }
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
