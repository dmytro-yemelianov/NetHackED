use crate::roles::RoleId;
use nethacked_types::{Alignment, Deity, Pantheon};

/// Returns the 3-deity pantheon (Lawful, Neutral, Chaotic) for a character role.
pub fn get_pantheon_for_role(role: RoleId) -> Pantheon {
    match role {
        RoleId::Valkyrie => Pantheon {
            lawful: Deity {
                name: "Tyr".into(),
                align: Alignment::Lawful,
            },
            neutral: Deity {
                name: "Odin".into(),
                align: Alignment::Neutral,
            },
            chaotic: Deity {
                name: "Loki".into(),
                align: Alignment::Chaotic,
            },
        },
        RoleId::Wizard => Pantheon {
            lawful: Deity {
                name: "Ptah".into(),
                align: Alignment::Lawful,
            },
            neutral: Deity {
                name: "Thoth".into(),
                align: Alignment::Neutral,
            },
            chaotic: Deity {
                name: "Anhur".into(),
                align: Alignment::Chaotic,
            },
        },
        RoleId::Barbarian => Pantheon {
            lawful: Deity {
                name: "Mitra".into(),
                align: Alignment::Lawful,
            },
            neutral: Deity {
                name: "Crom".into(),
                align: Alignment::Neutral,
            },
            chaotic: Deity {
                name: "Set".into(),
                align: Alignment::Chaotic,
            },
        },
        RoleId::Rogue => Pantheon {
            lawful: Deity {
                name: "Issek".into(),
                align: Alignment::Lawful,
            },
            neutral: Deity {
                name: "Mog".into(),
                align: Alignment::Neutral,
            },
            chaotic: Deity {
                name: "Kos".into(),
                align: Alignment::Chaotic,
            },
        },
        RoleId::Knight => Pantheon {
            lawful: Deity {
                name: "Lugh".into(),
                align: Alignment::Lawful,
            },
            neutral: Deity {
                name: "Brigit".into(),
                align: Alignment::Neutral,
            },
            chaotic: Deity {
                name: "Manannan Mac Lir".into(),
                align: Alignment::Chaotic,
            },
        },
        RoleId::Monk => Pantheon {
            lawful: Deity {
                name: "Shan Lai Ching".into(),
                align: Alignment::Lawful,
            },
            neutral: Deity {
                name: "Chih Sung-tzu".into(),
                align: Alignment::Neutral,
            },
            chaotic: Deity {
                name: "Huan Ti".into(),
                align: Alignment::Chaotic,
            },
        },
        RoleId::Healer => Pantheon {
            lawful: Deity {
                name: "Athena".into(),
                align: Alignment::Lawful,
            },
            neutral: Deity {
                name: "Hermes".into(),
                align: Alignment::Neutral,
            },
            chaotic: Deity {
                name: "Poseidon".into(),
                align: Alignment::Chaotic,
            },
        },
        RoleId::Tourist => Pantheon {
            lawful: Deity {
                name: "Blind Io".into(),
                align: Alignment::Lawful,
            },
            neutral: Deity {
                name: "The Lady".into(),
                align: Alignment::Neutral,
            },
            chaotic: Deity {
                name: "Offler".into(),
                align: Alignment::Chaotic,
            },
        },
        RoleId::Archaeologist => Pantheon {
            lawful: Deity {
                name: "Quetzalcoatl".into(),
                align: Alignment::Lawful,
            },
            neutral: Deity {
                name: "Camaxtli".into(),
                align: Alignment::Neutral,
            },
            chaotic: Deity {
                name: "Huhetotl".into(),
                align: Alignment::Chaotic,
            },
        },
    }
}

/// Returns the primary patron deity for a role given their personal alignment.
pub fn get_patron_deity(role: RoleId, align: Alignment) -> Deity {
    let p = get_pantheon_for_role(role);
    match align {
        Alignment::Lawful => p.lawful,
        Alignment::Neutral => p.neutral,
        Alignment::Chaotic => p.chaotic,
        Alignment::Unaligned => p.neutral,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (role, lawful, neutral, chaotic) from NetHack C `role.c` urole[] god fields.
    const C_PANTHEONS: [(RoleId, &str, &str, &str); 9] = [
        (
            RoleId::Archaeologist,
            "Quetzalcoatl",
            "Camaxtli",
            "Huhetotl",
        ),
        (RoleId::Barbarian, "Mitra", "Crom", "Set"),
        (RoleId::Healer, "Athena", "Hermes", "Poseidon"),
        (RoleId::Knight, "Lugh", "Brigit", "Manannan Mac Lir"),
        (RoleId::Monk, "Shan Lai Ching", "Chih Sung-tzu", "Huan Ti"),
        (RoleId::Rogue, "Issek", "Mog", "Kos"),
        (RoleId::Tourist, "Blind Io", "The Lady", "Offler"),
        (RoleId::Valkyrie, "Tyr", "Odin", "Loki"),
        (RoleId::Wizard, "Ptah", "Thoth", "Anhur"),
    ];

    #[test]
    fn pantheons_match_role_c() {
        for (r, l, n, c) in C_PANTHEONS {
            let p = get_pantheon_for_role(r);
            assert_eq!(p.lawful.name, l, "{r:?} lawful");
            assert_eq!(p.neutral.name, n, "{r:?} neutral");
            assert_eq!(p.chaotic.name, c, "{r:?} chaotic");
            assert_eq!(get_patron_deity(r, Alignment::Chaotic).name, c);
        }
    }

    #[test]
    fn test_all_roles_have_valid_pantheons() {
        let roles = [
            RoleId::Valkyrie,
            RoleId::Wizard,
            RoleId::Barbarian,
            RoleId::Rogue,
            RoleId::Knight,
            RoleId::Monk,
            RoleId::Healer,
            RoleId::Tourist,
            RoleId::Archaeologist,
        ];

        for r in roles {
            let p = get_pantheon_for_role(r);
            assert_eq!(p.lawful.align, Alignment::Lawful);
            assert_eq!(p.neutral.align, Alignment::Neutral);
            assert_eq!(p.chaotic.align, Alignment::Chaotic);
            assert!(!p.lawful.name.is_empty());
            assert!(!p.neutral.name.is_empty());
            assert!(!p.chaotic.name.is_empty());
        }
    }
}
