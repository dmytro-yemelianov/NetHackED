use netrust_types::{SkillClass, SkillLevel, SkillTree};

pub fn skill_to_hit_bonus(level: SkillLevel) -> i32 {
    match level {
        SkillLevel::Unskilled => -4,
        SkillLevel::Basic => 0,
        SkillLevel::Skilled => 2,
        SkillLevel::Expert => 3,
    }
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
    let current = tree.skills.get(&skill).copied().unwrap_or(SkillLevel::Unskilled);
    
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
