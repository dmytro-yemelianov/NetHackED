//! Rule pack directory format and TOML deserialization models.

use std::collections::BTreeMap;
use std::path::Path;

use nethacked_data::ruleset::{ArmorDef, QuestDef, StartingItem};
use nethacked_data::{AiBehavior, MonsterSize, MonsterSound, RaceId, WandDir};
use nethacked_types::{
    Alignment, Attack, Intrinsics, ItemClass, MonsterAbility, SkillClass, SkillLevel,
};
use serde::{Deserialize, Serialize};

use crate::PackError;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gen_flags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flags: Option<Vec<String>>,
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
    crate::files::parse_pack_files(&crate::files::read_pack_files_from_dir(dir)?)
}
