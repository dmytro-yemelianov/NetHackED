use netrust_types::{SkillClass, SkillLevel, SkillTree};

pub fn skill_to_hit_bonus(level: SkillLevel) -> i32 {
    match level {
        SkillLevel::Unskilled => -4,
        SkillLevel::Basic => 0,
        SkillLevel::Skilled => 2,
        SkillLevel::Expert => 3,
    }
}

/// Bare-handed (non-martial-arts) to-hit bonus, C `weapon_hit_bonus`
/// (`weapon.c:1601`): `((max(P_SKILL, P_UNSKILLED) - 1) + 2) / 2` with
/// `P_UNSKILLED = 1 .. P_EXPERT = 4`, giving Unskilled/Basic +1, Skilled/Expert +2.
/// Martial arts (Monk/Samurai doubling) is not modelled.
pub fn bare_handed_hit_bonus(level: SkillLevel) -> i32 {
    let p = match level {
        SkillLevel::Unskilled => 1,
        SkillLevel::Basic => 2,
        SkillLevel::Skilled => 3,
        SkillLevel::Expert => 4,
    };
    ((p - 1) + 2) / 2
}

pub fn skill_damage_bonus(level: SkillLevel) -> i32 {
    match level {
        SkillLevel::Unskilled => -2,
        SkillLevel::Basic => 0,
        SkillLevel::Skilled => 1,
        SkillLevel::Expert => 2,
    }
}

pub fn enhance_skill(tree: &mut SkillTree, skill: SkillClass) -> Result<(), &'static str> {
    let current = tree
        .skills
        .get(&skill)
        .copied()
        .unwrap_or(SkillLevel::Unskilled);

    if tree.available_slots == 0 {
        return Err("No skill slots available");
    }

    let next = match current {
        SkillLevel::Unskilled => SkillLevel::Basic,
        SkillLevel::Basic => SkillLevel::Skilled,
        SkillLevel::Skilled => SkillLevel::Expert,
        SkillLevel::Expert => return Err("Skill is already at maximum level"),
    };

    tree.available_slots -= 1;
    tree.skills.insert(skill, next);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_handed_hit_bonus_matches_c() {
        // weapon.c:1601: ((max(P,1) - 1) + 2) * (martial ? 2 : 1) / 2, non-martial
        assert_eq!(bare_handed_hit_bonus(SkillLevel::Unskilled), 1);
        assert_eq!(bare_handed_hit_bonus(SkillLevel::Basic), 1);
        assert_eq!(bare_handed_hit_bonus(SkillLevel::Skilled), 2);
        assert_eq!(bare_handed_hit_bonus(SkillLevel::Expert), 2);
    }
}
