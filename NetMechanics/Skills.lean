namespace NetMechanics

inductive SkillLevel
  | Unskilled
  | Basic
  | Skilled
  | Expert
  deriving DecidableEq

def to_hit_bonus (s : SkillLevel) : Int :=
  match s with
  | SkillLevel.Unskilled => -4
  | SkillLevel.Basic => 0
  | SkillLevel.Skilled => 2
  | SkillLevel.Expert => 3

def damage_bonus (s : SkillLevel) : Int :=
  match s with
  | SkillLevel.Unskilled => -2
  | SkillLevel.Basic => 0
  | SkillLevel.Skilled => 1
  | SkillLevel.Expert => 2

def skill_level_le (s1 s2 : SkillLevel) : Prop :=
  match s1, s2 with
  | SkillLevel.Unskilled, _ => True
  | SkillLevel.Basic, SkillLevel.Unskilled => False
  | SkillLevel.Basic, _ => True
  | SkillLevel.Skilled, SkillLevel.Unskilled => False
  | SkillLevel.Skilled, SkillLevel.Basic => False
  | SkillLevel.Skilled, _ => True
  | SkillLevel.Expert, SkillLevel.Expert => True
  | SkillLevel.Expert, _ => False

theorem skill_to_hit_monotonic (s1 s2 : SkillLevel) (h : skill_level_le s1 s2) :
  to_hit_bonus s1 ≤ to_hit_bonus s2 := by
  cases s1 <;> cases s2 <;> (
    simp [skill_level_le, to_hit_bonus] at h ⊢
    try contradiction
    try decide
  )

theorem skill_damage_monotonic (s1 s2 : SkillLevel) (h : skill_level_le s1 s2) :
  damage_bonus s1 ≤ damage_bonus s2 := by
  cases s1 <;> cases s2 <;> (
    simp [skill_level_le, damage_bonus] at h ⊢
    try contradiction
    try decide
  )

structure PlayerSkillState where
  level : SkillLevel
  slots : Nat

inductive SkillAction
  | enhance

def apply_skill_action (state : PlayerSkillState) (a : SkillAction) : Option PlayerSkillState :=
  match a with
  | SkillAction.enhance =>
    match state.slots with
    | 0 => none
    | slots + 1 =>
      match state.level with
      | SkillLevel.Unskilled => some ⟨SkillLevel.Basic, slots⟩
      | SkillLevel.Basic => some ⟨SkillLevel.Skilled, slots⟩
      | SkillLevel.Skilled => some ⟨SkillLevel.Expert, slots⟩
      | SkillLevel.Expert => none

theorem enhance_skill_requires_unlocked_slot (lvl : SkillLevel) :
  apply_skill_action ⟨lvl, 0⟩ SkillAction.enhance = none := by
  rfl

end NetMechanics
