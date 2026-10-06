//! C existing-object artifact generation; divine gifts and hack_artifacts excluded.
use crate::{ItemKindId, Ruleset};
use nethacked_arena::ItemRecord;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ArtifactDef {
    pub id: String,
    pub item: ItemKindId,
    pub base: ItemKindId,
    pub nogen: bool,
    pub gen_spe: i16,
    pub gift_value: u8,
    pub role: Option<String>,
    /// C artifact.c:2838 permapoisoned (canonical Grimtooth predicate).
    pub permanent_poison: bool,
}
type ArtifactRow = (
    &'static str,
    ItemKindId,
    ItemKindId,
    bool,
    i16,
    u8,
    Option<&'static str>,
    bool,
);
include!("generated/artifacts.rs");
pub fn canonical_artifact_id(item: ItemKindId) -> Option<&'static str> {
    ARTIFACT_ROWS.iter().find(|a| a.1 == item).map(|a| a.0)
}
pub fn vanilla_artifacts() -> Vec<ArtifactDef> {
    ARTIFACT_ROWS
        .iter()
        .map(|a| ArtifactDef {
            id: a.0.to_owned(),
            item: a.1,
            base: a.2,
            nogen: a.3,
            gen_spe: a.4,
            gift_value: a.5,
            role: a.6.map(str::to_owned),
            permanent_poison: a.7,
        })
        .collect()
}
impl Ruleset {
    /// Resolve only an owned definition's canonical ID, including old packs
    /// lacking random-generation metadata. No display-name guessing.
    pub fn artifact_id_for_item(&self, name: &str) -> Option<&'static str> {
        self.item(name)
            .and_then(|d| d.id)
            .and_then(canonical_artifact_id)
    }

    /// Chance follows all supported weapon/armor draws, including weapon poison.
    /// Children and explicit inventory never call this hook.
    pub fn random_existing_artifact(
        &self,
        item: &mut ItemRecord,
        base: ItemKindId,
        role: &str,
        history: &mut BTreeSet<String>,
        mut rn2: impl FnMut(u32) -> u32,
    ) {
        let Some(def) = self.item_by_id(base) else {
            return;
        };
        let chance = match def.initialization {
            crate::item_init::ItemInitProfile::Weapon { .. } => 20u32,
            crate::item_init::ItemInitProfile::Armor { .. } => 40u32,
            _ => return,
        }
        .saturating_add(10u32.saturating_mul(history.len().min(u32::MAX as usize) as u32));
        if rn2(chance).min(chance - 1) == 0 {
            self.make_existing_artifact(item, base, role, history, rn2);
        }
    }
    /// C artifact.c:172 mk_artifact(obj,A_NONE,99,TRUE), in extracted C order.
    pub fn make_existing_artifact(
        &self,
        item: &mut ItemRecord,
        base: ItemKindId,
        role: &str,
        history: &mut BTreeSet<String>,
        mut rn2: impl FnMut(u32) -> u32,
    ) {
        if self.item_by_id(base).is_none_or(|d| d.unique) {
            return;
        }
        let candidates: Vec<_> = self
            .artifacts
            .iter()
            .filter(|a| {
                a.base == base
                    && !a.nogen
                    && !history.contains(&a.id)
                    && (a.gift_value <= 99
                        || a.role
                            .as_deref()
                            .is_some_and(|r| artifact_role_matches(r, role)))
                    && self.item_by_id(a.item).is_some()
            })
            .collect();
        if candidates.is_empty() {
            return;
        }
        let n = candidates.len() as u32;
        let a = candidates[rn2(n).min(n - 1) as usize];
        let def = self.item_by_id(a.item).expect("filtered owned definition");
        item.name = def.name.clone();
        item.artifact_id = Some(a.id.clone());
        history.insert(a.id.clone());
        item.erosion = 0;
        let spe = i32::from(item.enchantment) + i32::from(a.gen_spe);
        if (-10..10).contains(&spe) {
            item.enchantment = spe as i8;
        }
        if a.permanent_poison {
            item.poisoned = true;
        }
    }
}

/// C role monster symbols differ from represented hero role identifiers.
fn artifact_role_matches(c_role: &str, hero_role: &str) -> bool {
    let role = match c_role {
        "ARCHEOLOGIST" => "Archaeologist",
        "CAVE_DWELLER" => "Caveman",
        "CLERIC" => "Priest",
        other => other,
    };
    role.eq_ignore_ascii_case(hero_role)
}
