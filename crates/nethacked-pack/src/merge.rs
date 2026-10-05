//! Rule pack resolution: applying additions, removals, and patches onto a base Ruleset.

use nethacked_data::ruleset::{ItemDef, MonsterDef, PackManifest, Ruleset};

use crate::format::PackDir;
use crate::{Diagnostic, PackError};

/// Resolves a parsed PackDir against a base Ruleset to produce a new Ruleset.
pub fn resolve(pack: &PackDir, base: &Ruleset) -> Result<Ruleset, PackError> {
    let mut monsters = base.monsters.clone();
    let mut items = base.items.clone();
    let mut roles = base.roles.clone();

    // 1. Process monsters
    for patch in &pack.monsters {
        let name_lower = patch.name.to_ascii_lowercase();
        if patch.remove {
            let initial_len = monsters.len();
            monsters.retain(|m| m.name.to_ascii_lowercase() != name_lower);
            if monsters.len() == initial_len {
                return Err(PackError::Resolve(Diagnostic {
                    file: "monsters.toml".into(),
                    entry: patch.name.clone(),
                    field: None,
                    message: "cannot remove non-existent monster".into(),
                }));
            }
        } else if patch.new {
            if monsters
                .iter()
                .any(|m| m.name.to_ascii_lowercase() == name_lower)
            {
                return Err(PackError::Resolve(Diagnostic {
                    file: "monsters.toml".into(),
                    entry: patch.name.clone(),
                    field: None,
                    message: "monster with this name already exists".into(),
                }));
            }

            macro_rules! req_m_field {
                ($field:ident) => {
                    patch.$field.clone().ok_or_else(|| {
                        PackError::Resolve(Diagnostic {
                            file: "monsters.toml".into(),
                            entry: patch.name.clone(),
                            field: Some(stringify!($field).into()),
                            message: "missing required field for new monster".into(),
                        })
                    })?
                };
            }

            let def = MonsterDef {
                id: None,
                name: patch.name.clone(),
                glyph: req_m_field!(glyph),
                base_hp: req_m_field!(base_hp),
                max_hp: req_m_field!(max_hp),
                ac: req_m_field!(ac),
                level: req_m_field!(level),
                speed: req_m_field!(speed),
                alignment: req_m_field!(alignment),
                intrinsics: req_m_field!(intrinsics),
                attacks: req_m_field!(attacks),
                size: req_m_field!(size),
                peaceful_by_default: req_m_field!(peaceful_by_default),
                always_hostile: req_m_field!(always_hostile),
                maligntyp: req_m_field!(maligntyp),
                msound: req_m_field!(msound),
                m2_race: patch.m2_race.unwrap_or(None),
                is_human: req_m_field!(is_human),
                is_unique: req_m_field!(is_unique),
                mindless: req_m_field!(mindless),
                ai_behavior: req_m_field!(ai_behavior),
                abilities: req_m_field!(abilities),
                // Optional: a new species is never generated at random unless given a weight.
                difficulty: patch.difficulty.unwrap_or(0),
                frequency: patch.frequency.unwrap_or(0),
                gen_flags: patch.gen_flags.clone().unwrap_or_default(),
            };
            monsters.push(def);
        } else {
            // Patch existing
            let target = monsters
                .iter_mut()
                .find(|m| m.name.to_ascii_lowercase() == name_lower)
                .ok_or_else(|| {
                    PackError::Resolve(Diagnostic {
                        file: "monsters.toml".into(),
                        entry: patch.name.clone(),
                        field: None,
                        message: "cannot patch non-existent monster".into(),
                    })
                })?;

            if let Some(v) = patch.glyph {
                target.glyph = v;
            }
            if let Some(v) = patch.base_hp {
                target.base_hp = v;
            }
            if let Some(v) = patch.max_hp {
                target.max_hp = v;
            }
            if let Some(v) = patch.ac {
                target.ac = v;
            }
            if let Some(v) = patch.level {
                target.level = v;
            }
            if let Some(v) = patch.speed {
                target.speed = v;
            }
            if let Some(v) = patch.alignment {
                target.alignment = v;
            }
            if let Some(v) = patch.intrinsics {
                target.intrinsics = v;
            }
            if let Some(v) = &patch.attacks {
                target.attacks = v.clone();
            }
            if let Some(v) = patch.size {
                target.size = v;
            }
            if let Some(v) = patch.peaceful_by_default {
                target.peaceful_by_default = v;
            }
            if let Some(v) = patch.always_hostile {
                target.always_hostile = v;
            }
            if let Some(v) = patch.maligntyp {
                target.maligntyp = v;
            }
            if let Some(v) = patch.msound {
                target.msound = v;
            }
            if let Some(v) = patch.m2_race {
                target.m2_race = v;
            }
            if let Some(v) = patch.is_human {
                target.is_human = v;
            }
            if let Some(v) = patch.is_unique {
                target.is_unique = v;
            }
            if let Some(v) = patch.mindless {
                target.mindless = v;
            }
            if let Some(v) = patch.ai_behavior {
                target.ai_behavior = v;
            }
            if let Some(v) = &patch.abilities {
                target.abilities = v.clone();
            }
            if let Some(v) = patch.difficulty {
                target.difficulty = v;
            }
            if let Some(v) = patch.frequency {
                target.frequency = v;
            }
            if let Some(v) = &patch.gen_flags {
                target.gen_flags = v.clone();
            }
        }
    }

    // 2. Process items
    for patch in &pack.items {
        let name_lower = patch.name.to_ascii_lowercase();
        if patch.remove {
            let initial_len = items.len();
            items.retain(|i| i.name.to_ascii_lowercase() != name_lower);
            if items.len() == initial_len {
                return Err(PackError::Resolve(Diagnostic {
                    file: "items.toml".into(),
                    entry: patch.name.clone(),
                    field: None,
                    message: "cannot remove non-existent item".into(),
                }));
            }
        } else if patch.new {
            if items
                .iter()
                .any(|i| i.name.to_ascii_lowercase() == name_lower)
            {
                return Err(PackError::Resolve(Diagnostic {
                    file: "items.toml".into(),
                    entry: patch.name.clone(),
                    field: None,
                    message: "item with this name already exists".into(),
                }));
            }

            macro_rules! req_i_field {
                ($field:ident) => {
                    patch.$field.clone().ok_or_else(|| {
                        PackError::Resolve(Diagnostic {
                            file: "items.toml".into(),
                            entry: patch.name.clone(),
                            field: Some(stringify!($field).into()),
                            message: "missing required field for new item".into(),
                        })
                    })?
                };
            }

            let def = ItemDef {
                id: None,
                name: patch.name.clone(),
                class: req_i_field!(class),
                weight: req_i_field!(weight),
                cost: req_i_field!(cost),
                damage_small: req_i_field!(damage_small),
                damage_large: req_i_field!(damage_large),
                ac_bonus: req_i_field!(ac_bonus),
                is_container: req_i_field!(is_container),
                is_bag_of_holding: req_i_field!(is_bag_of_holding),
                oc_magic: req_i_field!(oc_magic),
                wand_dir: patch.wand_dir.unwrap_or(None),
                nutrition: req_i_field!(nutrition),
                armor: patch.armor.clone().unwrap_or(None),
            };
            items.push(def);
        } else {
            // Patch existing
            let target = items
                .iter_mut()
                .find(|i| i.name.to_ascii_lowercase() == name_lower)
                .ok_or_else(|| {
                    PackError::Resolve(Diagnostic {
                        file: "items.toml".into(),
                        entry: patch.name.clone(),
                        field: None,
                        message: "cannot patch non-existent item".into(),
                    })
                })?;

            if let Some(v) = patch.class {
                target.class = v;
            }
            if let Some(v) = patch.weight {
                target.weight = v;
            }
            if let Some(v) = patch.cost {
                target.cost = v;
            }
            if let Some(v) = patch.damage_small {
                target.damage_small = v;
            }
            if let Some(v) = patch.damage_large {
                target.damage_large = v;
            }
            if let Some(v) = patch.ac_bonus {
                target.ac_bonus = v;
            }
            if let Some(v) = patch.is_container {
                target.is_container = v;
            }
            if let Some(v) = patch.is_bag_of_holding {
                target.is_bag_of_holding = v;
            }
            if let Some(v) = patch.oc_magic {
                target.oc_magic = v;
            }
            if let Some(v) = patch.wand_dir {
                target.wand_dir = v;
            }
            if let Some(v) = patch.nutrition {
                target.nutrition = v;
            }
            if let Some(v) = &patch.armor {
                target.armor = v.clone();
            }
        }
    }

    // 3. Process roles
    for patch in &pack.roles {
        let name_lower = patch.name.to_ascii_lowercase();
        let target = roles
            .iter_mut()
            .find(|r| r.name.to_ascii_lowercase() == name_lower)
            .ok_or_else(|| {
                PackError::Resolve(Diagnostic {
                    file: "roles.toml".into(),
                    entry: patch.name.clone(),
                    field: None,
                    message: "cannot patch non-existent role".into(),
                })
            })?;

        if let Some(v) = patch.base_hp {
            target.base_hp = v;
        }
        if let Some(v) = patch.ac {
            target.ac = v;
        }
        if let Some(v) = patch.speed {
            target.speed = v;
        }
        if let Some(v) = patch.default_alignment {
            target.default_alignment = v;
        }
        if let Some(v) = &patch.starting_items {
            target.starting_items = v.clone();
        }
        if let Some(v) = &patch.skills {
            target.skills = v.clone();
        }
        if let Some(v) = &patch.pantheon {
            target.pantheon = v.clone();
        }
        if let Some(v) = &patch.quest {
            target.quest = v.clone();
        }
        if let Some(v) = patch.initial_alignment_record {
            target.initial_alignment_record = v;
        }
    }

    let manifest = PackManifest {
        id: pack.manifest.id.clone(),
        name: pack.manifest.name.clone(),
        version: pack.manifest.version.clone(),
        base: pack.manifest.base.clone(),
        description: pack.manifest.description.clone().unwrap_or_default(),
    };

    Ok(Ruleset::from_parts(
        manifest,
        monsters,
        items,
        roles,
        base.races.clone(),
        base.mechanics.clone(),
    ))
}
