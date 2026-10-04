//! Validation of ruleset integrity, constraints, ranges, and required entities.

use std::collections::HashSet;

use netrust_data::ruleset::{Ruleset, ENGINE_REQUIRED_ITEMS, ENGINE_REQUIRED_MONSTERS};
use netrust_types::{AttackType, ItemClass};
use serde::{Deserialize, Serialize};

/// A diagnostic message indicating an error or warning in a rule pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub file: String,
    pub entry: String,
    pub field: Option<String>,
    pub message: String,
}

/// Aggregated validation report containing errors and warnings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

impl Report {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Validates a Ruleset against all game constraints and invariants.
pub fn validate(rs: &Ruleset) -> Report {
    let mut report = Report::new();

    // 1. Monster unique names (case-insensitive)
    let mut seen_monsters = HashSet::new();
    for m in &rs.monsters {
        let name_lower = m.name.to_ascii_lowercase();
        if !seen_monsters.insert(name_lower) {
            report.errors.push(Diagnostic {
                file: "monsters.toml".into(),
                entry: m.name.clone(),
                field: None,
                message: format!("duplicate monster name: {}", m.name),
            });
        }
    }

    // 2. Item unique names (case-insensitive)
    let mut seen_items = HashSet::new();
    for i in &rs.items {
        let name_lower = i.name.to_ascii_lowercase();
        if !seen_items.insert(name_lower) {
            report.errors.push(Diagnostic {
                file: "items.toml".into(),
                entry: i.name.clone(),
                field: None,
                message: format!("duplicate item name: {}", i.name),
            });
        }
    }

    // 3. Engine-required monsters cannot be removed
    for &req in ENGINE_REQUIRED_MONSTERS {
        if rs.monster(req).is_none() {
            report.errors.push(Diagnostic {
                file: "monsters.toml".into(),
                entry: req.to_string(),
                field: None,
                message: format!("engine-required monster cannot be removed: {req}"),
            });
        }
    }

    // 4. Engine-required items cannot be removed
    for &req in ENGINE_REQUIRED_ITEMS {
        if rs.item(req).is_none() {
            report.errors.push(Diagnostic {
                file: "items.toml".into(),
                entry: req.to_string(),
                field: None,
                message: format!("engine-required item cannot be removed: {req}"),
            });
        }
    }

    // 5. Starting items for all roles must exist in rs.items
    for role in &rs.roles {
        for si in &role.starting_items {
            if rs.item(&si.item).is_none() {
                report.errors.push(Diagnostic {
                    file: "roles.toml".into(),
                    entry: role.name.clone(),
                    field: Some("starting_items".into()),
                    message: format!("starting item does not exist: {}", si.item),
                });
            }
        }
    }

    // 6. Quest leader, nemesis, guardian, and artifact must exist for all roles
    for role in &rs.roles {
        if let Some(quest) = &role.quest {
            if rs.monster(&quest.leader).is_none() {
                report.errors.push(Diagnostic {
                    file: "roles.toml".into(),
                    entry: role.name.clone(),
                    field: Some("quest.leader".into()),
                    message: format!("quest leader does not exist: {}", quest.leader),
                });
            }
            if rs.monster(&quest.nemesis).is_none() {
                report.errors.push(Diagnostic {
                    file: "roles.toml".into(),
                    entry: role.name.clone(),
                    field: Some("quest.nemesis".into()),
                    message: format!("quest nemesis does not exist: {}", quest.nemesis),
                });
            }
            if rs.monster(&quest.guardian).is_none() {
                report.errors.push(Diagnostic {
                    file: "roles.toml".into(),
                    entry: role.name.clone(),
                    field: Some("quest.guardian".into()),
                    message: format!("quest guardian does not exist: {}", quest.guardian),
                });
            }
            if rs.item(&quest.artifact).is_none() {
                report.errors.push(Diagnostic {
                    file: "roles.toml".into(),
                    entry: role.name.clone(),
                    field: Some("quest.artifact".into()),
                    message: format!("quest artifact does not exist: {}", quest.artifact),
                });
            }
        }
    }

    // 7. Value ranges
    for m in &rs.monsters {
        if m.level > 49 {
            report.errors.push(Diagnostic {
                file: "monsters.toml".into(),
                entry: m.name.clone(),
                field: Some("level".into()),
                message: format!("level out of range (0..=49): {}", m.level),
            });
        }
        if m.speed > 60 {
            report.errors.push(Diagnostic {
                file: "monsters.toml".into(),
                entry: m.name.clone(),
                field: Some("speed".into()),
                message: format!("speed out of range (0..=60): {}", m.speed),
            });
        }
        if m.ac < -20 || m.ac > 20 {
            report.errors.push(Diagnostic {
                file: "monsters.toml".into(),
                entry: m.name.clone(),
                field: Some("ac".into()),
                message: format!("ac out of range (-20..=20): {}", m.ac),
            });
        }
        if m.base_hp < 1 || m.base_hp > 10000 {
            report.errors.push(Diagnostic {
                file: "monsters.toml".into(),
                entry: m.name.clone(),
                field: Some("base_hp".into()),
                message: format!("base_hp out of range (1..=10000): {}", m.base_hp),
            });
        }
        if m.max_hp < 1 || m.max_hp > 10000 {
            report.errors.push(Diagnostic {
                file: "monsters.toml".into(),
                entry: m.name.clone(),
                field: Some("max_hp".into()),
                message: format!("max_hp out of range (1..=10000): {}", m.max_hp),
            });
        }
        if m.base_hp > m.max_hp {
            report.errors.push(Diagnostic {
                file: "monsters.toml".into(),
                entry: m.name.clone(),
                field: Some("base_hp".into()),
                message: format!(
                    "base_hp ({}) cannot exceed max_hp ({})",
                    m.base_hp, m.max_hp
                ),
            });
        }
        if !m.glyph.is_ascii() || m.glyph.is_ascii_control() {
            report.errors.push(Diagnostic {
                file: "monsters.toml".into(),
                entry: m.name.clone(),
                field: Some("glyph".into()),
                message: format!("glyph must be printable ASCII: {:?}", m.glyph),
            });
        }
        for (i, atk) in m.attacks.iter().enumerate() {
            let is_special_attack = matches!(
                atk.at,
                AttackType::Passive | AttackType::Gaze | AttackType::Magic
            );
            if !is_special_attack {
                if atk.n < 1 {
                    report.errors.push(Diagnostic {
                        file: "monsters.toml".into(),
                        entry: m.name.clone(),
                        field: Some("attacks".into()),
                        message: format!(
                            "attack {} dice count out of range (1..=255): {}",
                            i + 1,
                            atk.n
                        ),
                    });
                }
                if atk.d < 1 {
                    report.errors.push(Diagnostic {
                        file: "monsters.toml".into(),
                        entry: m.name.clone(),
                        field: Some("attacks".into()),
                        message: format!(
                            "attack {} dice sides out of range (1..=255): {}",
                            i + 1,
                            atk.d
                        ),
                    });
                }
            }
        }
    }

    for item in &rs.items {
        if item.weight > 10000 {
            report.errors.push(Diagnostic {
                file: "items.toml".into(),
                entry: item.name.clone(),
                field: Some("weight".into()),
                message: format!("weight out of range (0..=10000): {}", item.weight),
            });
        }
        if item.cost > 1_000_000 {
            report.errors.push(Diagnostic {
                file: "items.toml".into(),
                entry: item.name.clone(),
                field: Some("cost".into()),
                message: format!("cost out of range (0..=1000000): {}", item.cost),
            });
        }
        // damage dice sides: 0..=255 (0 only for non-weapons)
        let (_s_n, s_d) = item.damage_small;
        let (_l_n, l_d) = item.damage_large;
        if s_d > 255 {
            report.errors.push(Diagnostic {
                file: "items.toml".into(),
                entry: item.name.clone(),
                field: Some("damage_small".into()),
                message: format!("damage dice sides out of range (0..=255): {s_d}"),
            });
        }
        if l_d > 255 {
            report.errors.push(Diagnostic {
                file: "items.toml".into(),
                entry: item.name.clone(),
                field: Some("damage_large".into()),
                message: format!("damage dice sides out of range (0..=255): {l_d}"),
            });
        }
        if item.class == ItemClass::Weapon && (s_d == 0 || l_d == 0) {
            report.errors.push(Diagnostic {
                file: "items.toml".into(),
                entry: item.name.clone(),
                field: Some("damage_small".into()),
                message: "weapon damage dice sides cannot be 0".into(),
            });
        }
    }

    report
}
