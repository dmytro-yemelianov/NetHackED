//! The Class Quest Branch, Leader qualification, and Nemesis mechanics.
//!
//! Formally verified in `NetMechanics.Quest`.

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
}

impl Default for QuestState {
    fn default() -> Self {
        Self {
            progress: QuestProgress::Unassigned,
            artifact_location: ArtifactLocation::HeldByNemesis,
            nemesis_hp: 120,
        }
    }
}

/// Consult the Quest Leader to receive assignment and unlock the portal stairs.
pub fn consult_leader(state: &mut QuestState, hero: &HeroQuestEligibility) -> Result<(), &'static str> {
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
    pub artifact_name: &'static str,
    pub home_desc: &'static str,
    pub goal_desc: &'static str,
}

pub fn get_role_quest_config(role_name: &str) -> RoleQuestConfig {
    match role_name.to_lowercase().as_str() {
        "valkyrie" => RoleQuestConfig {
            role_name: "Valkyrie",
            leader_name: "The Norn",
            nemesis_name: "Lord Surtur",
            artifact_name: "The Orb of Fate",
            home_desc: "The Temple of the Norn in the Glade of the Valkyries",
            goal_desc: "The fiery volcano lair of Lord Surtur",
        },
        "wizard" => RoleQuestConfig {
            role_name: "Wizard",
            leader_name: "Neferet the Green",
            nemesis_name: "The Dark One",
            artifact_name: "The Eye of the Aethiopica",
            home_desc: "The Lonely Tower of the High Wizards",
            goal_desc: "The Nether Vault of the Dark One",
        },
        "barbarian" => RoleQuestConfig {
            role_name: "Barbarian",
            leader_name: "Pelias",
            nemesis_name: "Thoth Amon",
            artifact_name: "The Heart of Ahriman",
            home_desc: "The Camp of the Great Warlord Pelias",
            goal_desc: "The Sunken Citadel of Thoth Amon",
        },
        "knight" => RoleQuestConfig {
            role_name: "Knight",
            leader_name: "King Arthur",
            nemesis_name: "Ixoth",
            artifact_name: "The Magic Mirror of Merlin",
            home_desc: "Camelot Great Hall of the Round Table",
            goal_desc: "The Dragon Lair of Ixoth",
        },
        "monk" => RoleQuestConfig {
            role_name: "Monk",
            leader_name: "Grand Master",
            nemesis_name: "Master Kaen",
            artifact_name: "The Eyes of the Overworld",
            home_desc: "The Monastery of the Silent Order",
            goal_desc: "The Mountain Caverns of Master Kaen",
        },
        "rogue" => RoleQuestConfig {
            role_name: "Rogue",
            leader_name: "Master Assassin",
            nemesis_name: "Master of Thieves",
            artifact_name: "The Master Key of Thievery",
            home_desc: "The Shadow Guild of Thieves",
            goal_desc: "The Vault of the Master of Thieves",
        },
        "tourist" => RoleQuestConfig {
            role_name: "Tourist",
            leader_name: "Twoflower",
            nemesis_name: "Master Kaen",
            artifact_name: "The Platinum Yendorian Express Card",
            home_desc: "The Tourist Welcome Center",
            goal_desc: "The Exotic Temple of Doom",
        },
        "healer" => RoleQuestConfig {
            role_name: "Healer",
            leader_name: "Hippocrates",
            nemesis_name: "Cyclops",
            artifact_name: "The Staff of Aesculapius",
            home_desc: "The Temple of Epidaurus",
            goal_desc: "The Island Cave of the Cyclops",
        },
        _ => RoleQuestConfig {
            role_name: "Archaeologist",
            leader_name: "Lord Carnarvon",
            nemesis_name: "Minion of Huhetotl",
            artifact_name: "The Orb of Detection",
            home_desc: "The Royal Archaeological Society",
            goal_desc: "The Tomb of the Ancient Mayan Gods",
        },
    }
}
