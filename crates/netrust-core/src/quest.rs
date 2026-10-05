//! The Class Quest Branch, Leader qualification, and Nemesis mechanics.
//!
//! Modeled in Lean 4 (`NetMechanics.Quest`).

use serde::{Deserialize, Serialize};

/// Minimum experience level required for the Quest Leader to grant the quest.
pub const QUEST_MIN_LEVEL: u32 = 14;

/// Minimum alignment record required for the Quest Leader to accept the hero.
pub const QUEST_MIN_ALIGNMENT: i32 = 20;

/// Hero quest qualification record for Leader assessment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeroQuestEligibility {
    pub experience_level: u32,
    pub alignment_record: i32,
    pub is_hostile_to_leader: bool,
}

/// Check if a hero meets the canonical qualification standards for the Quest Leader.
pub fn is_hero_eligible_for_quest(h: &HeroQuestEligibility) -> bool {
    h.experience_level >= QUEST_MIN_LEVEL
        && h.alignment_record >= QUEST_MIN_ALIGNMENT
        && !h.is_hostile_to_leader
}

/// Lifecycle progression state of the Class Quest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum QuestProgress {
    #[default]
    Unassigned,
    Assigned,
    NemesisDefeated,
    Completed,
}

/// Numeric ranking for quest progression stages ensuring monotonic progress.
pub fn quest_progress_rank(p: QuestProgress) -> usize {
    match p {
        QuestProgress::Unassigned => 0,
        QuestProgress::Assigned => 1,
        QuestProgress::NemesisDefeated => 2,
        QuestProgress::Completed => 3,
    }
}

/// Quest Artifact possession state tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ArtifactLocation {
    #[default]
    HeldByNemesis,
    DroppedOnFloor,
    CarriedByHero,
}

/// Complete state of the Class Quest subsystem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestState {
    pub progress: QuestProgress,
    pub artifact_location: ArtifactLocation,
    pub nemesis_hp: u32,
    /// C `svq.quest_status.killed_leader` (`mon.c:3679`). Tracks if the player murdered the Quest Leader.
    #[serde(default)]
    pub killed_leader: bool,
}

impl Default for QuestState {
    fn default() -> Self {
        Self {
            progress: QuestProgress::Unassigned,
            artifact_location: ArtifactLocation::HeldByNemesis,
            nemesis_hp: 120,
            killed_leader: false,
        }
    }
}

/// Consult the Quest Leader to receive assignment and unlock the portal stairs.
pub fn consult_leader(
    state: &mut QuestState,
    hero: &HeroQuestEligibility,
) -> Result<(), &'static str> {
    match state.progress {
        QuestProgress::Unassigned => {
            if hero.is_hostile_to_leader {
                Err("The Leader refuses to speak with a hostile blasphemer!")
            } else if hero.experience_level < QUEST_MIN_LEVEL {
                Err("You are not yet experienced enough to undertake your trial. Return when level 14.")
            } else if hero.alignment_record < QUEST_MIN_ALIGNMENT {
                Err("Your alignment record is too low. Prove your devotion before undertaking the Quest.")
            } else {
                state.progress = QuestProgress::Assigned;
                Ok(())
            }
        }
        QuestProgress::Assigned => Ok(()),
        QuestProgress::NemesisDefeated => Ok(()),
        QuestProgress::Completed => Ok(()),
    }
}

/// Resolve damage on the Nemesis boss. Returns true if the Nemesis was defeated on this hit.
pub fn attack_nemesis(state: &mut QuestState, damage: u32) -> bool {
    if state.progress == QuestProgress::Assigned {
        if damage >= state.nemesis_hp {
            state.progress = QuestProgress::NemesisDefeated;
            state.nemesis_hp = 0;
            state.artifact_location = ArtifactLocation::DroppedOnFloor;
            true
        } else {
            state.nemesis_hp -= damage;
            false
        }
    } else {
        false
    }
}

/// Hero claims the dropped Quest Artifact from the dungeon floor.
pub fn pick_up_quest_artifact(state: &mut QuestState) -> bool {
    if state.artifact_location == ArtifactLocation::DroppedOnFloor {
        state.artifact_location = ArtifactLocation::CarriedByHero;
        true
    } else {
        false
    }
}

/// Return to the Quest Leader with the Quest Artifact to conclude the quest.
pub fn return_to_leader_with_artifact(state: &mut QuestState) -> bool {
    if state.progress == QuestProgress::NemesisDefeated
        && state.artifact_location == ArtifactLocation::CarriedByHero
    {
        state.progress = QuestProgress::Completed;
        true
    } else {
        false
    }
}

/// Canonical metadata for a character role's Quest trial.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleQuestConfig {
    pub role_name: &'static str,
    pub leader_name: &'static str,
    pub nemesis_name: &'static str,
    /// Quest guardian species name (C `role.c` `guardnum`); a BESTIARY name.
    pub guardian_name: &'static str,
    pub artifact_name: &'static str,
    pub home_desc: &'static str,
    pub goal_desc: &'static str,
}

/// Quest leader, nemesis, artifact and location names for a role.
///
/// C: `role.c` `urole[]` (Arc :45, Bar :86, Hea :168, Kni :208, Mon :248,
/// Rog :332, Tou :467, Val :507, Wiz :547): leader at +2, nemesis at +4,
/// artifact at +9, home/goal strings two lines above the leader.
/// Leader/nemesis/artifact keep the project's display convention (leading
/// "The" for Norn / Dark One / artifacts) so they equal the BESTIARY and item
/// names. Matching is case-insensitive. Returns `None` for roles NetRust has
/// no quest data for (previously a silent Archaeologist fallback).
pub fn get_role_quest_config(role_name: &str) -> Option<RoleQuestConfig> {
    let (role, leader, nemesis, artifact, home, goal, guardian) =
        match role_name.to_lowercase().as_str() {
            "valkyrie" => (
                "Valkyrie",
                "The Norn",
                "Lord Surtur",
                "The Orb of Fate",
                "the Shrine of Destiny",
                "the cave of Surtur",
                "warrior",
            ),
            "wizard" => (
                "Wizard",
                "Neferet the Green",
                "The Dark One",
                "The Eye of the Aethiopica",
                "the Lonely Tower",
                "the Tower of Darkness",
                "apprentice",
            ),
            "barbarian" => (
                "Barbarian",
                "Pelias",
                "Thoth Amon",
                "The Heart of Ahriman",
                "the Camp of the Duali Tribe",
                "the Duali Oasis",
                "chieftain",
            ),
            "knight" => (
                "Knight",
                "King Arthur",
                "Ixoth",
                "The Magic Mirror of Merlin",
                "Camelot Castle",
                "the Isle of Glass",
                "page",
            ),
            "monk" => (
                "Monk",
                "Grand Master",
                "Master Kaen",
                "The Eyes of the Overworld",
                "the Monastery of Chan-Sune",
                "the Monastery of the Earth-Lord",
                "abbot",
            ),
            "rogue" => (
                "Rogue",
                "Master of Thieves",
                "Master Assassin",
                "The Master Key of Thievery",
                "the Thieves' Guild Hall",
                "the Assassins' Guild Hall",
                "thug",
            ),
            "tourist" => (
                "Tourist",
                "Twoflower",
                "Master of Thieves",
                "The Platinum Yendorian Express Card",
                "Ankh-Morpork",
                "the Thieves' Guild Hall",
                "guide",
            ),
            "healer" => (
                "Healer",
                "Hippocrates",
                "Cyclops",
                "The Staff of Aesculapius",
                "the Temple of Epidaurus",
                "the Temple of Coeus",
                "attendant",
            ),
            "archaeologist" => (
                "Archaeologist",
                "Lord Carnarvon",
                "Minion of Huhetotl",
                "The Orb of Detection",
                "the College of Archeology",
                "the Tomb of the Toltec Kings",
                "student",
            ),
            _ => return None,
        };
    Some(RoleQuestConfig {
        role_name: role,
        leader_name: leader,
        nemesis_name: nemesis,
        guardian_name: guardian,
        artifact_name: artifact,
        home_desc: home,
        goal_desc: goal,
    })
}

/// Quest config for `role_name`, falling back to the Archaeologist quest for
/// roles without quest data (documented default; keeps quest levels playable).
pub fn get_role_quest_config_or_default(role_name: &str) -> RoleQuestConfig {
    get_role_quest_config(role_name)
        .or_else(|| get_role_quest_config("archaeologist"))
        .expect("archaeologist quest data exists")
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// (role, leader, nemesis, artifact) written from role.c urole table (C ref §15).
    const C_TABLE: [(&str, &str, &str, &str); 9] = [
        (
            "Archaeologist",
            "Lord Carnarvon",
            "Minion of Huhetotl",
            "The Orb of Detection",
        ),
        ("Barbarian", "Pelias", "Thoth Amon", "The Heart of Ahriman"),
        (
            "Healer",
            "Hippocrates",
            "Cyclops",
            "The Staff of Aesculapius",
        ),
        (
            "Knight",
            "King Arthur",
            "Ixoth",
            "The Magic Mirror of Merlin",
        ),
        (
            "Monk",
            "Grand Master",
            "Master Kaen",
            "The Eyes of the Overworld",
        ),
        (
            "Rogue",
            "Master of Thieves",
            "Master Assassin",
            "The Master Key of Thievery",
        ),
        (
            "Tourist",
            "Twoflower",
            "Master of Thieves",
            "The Platinum Yendorian Express Card",
        ),
        ("Valkyrie", "The Norn", "Lord Surtur", "The Orb of Fate"),
        (
            "Wizard",
            "Neferet the Green",
            "The Dark One",
            "The Eye of the Aethiopica",
        ),
    ];

    #[test]
    fn quest_table_matches_c() {
        for (role, leader, nemesis, artifact) in C_TABLE {
            let c = get_role_quest_config(role).expect(role);
            assert_eq!(c.role_name, role);
            assert_eq!(c.leader_name, leader, "{role} leader");
            assert_eq!(c.nemesis_name, nemesis, "{role} nemesis");
            assert_eq!(c.artifact_name, artifact, "{role} artifact");
        }
    }

    /// (role, guardian) from role.c `guardnum` (Arc :48, Bar :89, Hea :171, Kni :211,
    /// Mon :251, Rog :335, Tou :470, Val :510, Wiz :550).
    #[test]
    fn guardian_names_match_c() {
        for (role, g) in [
            ("Archaeologist", "student"),
            ("Barbarian", "chieftain"),
            ("Healer", "attendant"),
            ("Knight", "page"),
            ("Monk", "abbot"),
            ("Rogue", "thug"),
            ("Tourist", "guide"),
            ("Valkyrie", "warrior"),
            ("Wizard", "apprentice"),
        ] {
            assert_eq!(
                get_role_quest_config(role).unwrap().guardian_name,
                g,
                "{role}"
            );
        }
    }

    #[test]
    fn unknown_role_is_none() {
        assert!(get_role_quest_config("Samurai").is_none());
        assert!(get_role_quest_config("").is_none());
    }

    #[test]
    fn home_goal_use_c_location_names() {
        let r = get_role_quest_config("Rogue").unwrap();
        assert_eq!(r.home_desc, "the Thieves' Guild Hall");
        assert_eq!(r.goal_desc, "the Assassins' Guild Hall");
        let t = get_role_quest_config("Tourist").unwrap();
        assert_eq!(t.home_desc, "Ankh-Morpork");
        assert_eq!(t.goal_desc, "the Thieves' Guild Hall");
    }

    proptest! {
        /// Lookup is case-insensitive and agrees with the C table for any casing;
        /// arbitrary non-role strings yield None.
        #[test]
        fn prop_lookup_matches_c_table(idx in 0usize..9, mask in any::<u16>(), junk in "[a-z]{0,12}") {
            let (role, leader, nemesis, artifact) = C_TABLE[idx];
            let cased: String = role
                .chars()
                .enumerate()
                .map(|(i, ch)| if mask >> (i % 16) & 1 == 1 { ch.to_ascii_uppercase() } else { ch.to_ascii_lowercase() })
                .collect();
            let c = get_role_quest_config(&cased).unwrap();
            prop_assert_eq!((c.role_name, c.leader_name, c.nemesis_name, c.artifact_name), (role, leader, nemesis, artifact));
            let is_role = C_TABLE.iter().any(|r| r.0.eq_ignore_ascii_case(&junk));
            prop_assert_eq!(get_role_quest_config(&junk).is_some(), is_role);
        }
    }

    #[test]
    fn test_quest_state_serde_default_killed_leader() {
        let legacy_json =
            r#"{"progress":"Assigned","artifact_location":"HeldByNemesis","nemesis_hp":120}"#;
        let state: QuestState =
            serde_json::from_str(legacy_json).expect("legacy QuestState should deserialize");
        assert_eq!(state.progress, QuestProgress::Assigned);
        assert_eq!(state.artifact_location, ArtifactLocation::HeldByNemesis);
        assert_eq!(state.nemesis_hp, 120);
        assert!(!state.killed_leader);
    }
}
