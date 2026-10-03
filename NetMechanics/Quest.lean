/-
  NetHack Mechanics Formalized in Lean 4: The Class Quest Branch & Nemesis
  Formalizing Role Leader qualification, Quest Artifact possession invariants,
  Nemesis defeat transitions, and quest completion progression.
-/

import NetMechanics.Basic

namespace NetMechanics

/-- The canonical NetHack player roles with quests -/
inductive QuestRole where
  | Archeologist
  | Barbarian
  | Caveman
  | Healer
  | Knight
  | Monk
  | Priest
  | Rogue
  | Ranger
  | Samurai
  | Tourist
  | Valkyrie
  | Wizard
  deriving Repr, DecidableEq

/-- The required minimum experience level to be accepted by the Quest Leader -/
def QUEST_MIN_LEVEL : Nat := 14

/-- The required minimum alignment record to be accepted by the Quest Leader -/
def QUEST_MIN_ALIGNMENT : Int := 20

/-- Hero qualification record for the Quest Leader -/
structure HeroQuestEligibility where
  experienceLevel : Nat
  alignmentRecord : Int
  isHostileToLeader : Bool
  deriving Repr, DecidableEq

/-- Check if a hero is qualified to be accepted by the Quest Leader -/
def isHeroEligibleForQuest (h : HeroQuestEligibility) : Bool :=
  h.experienceLevel ≥ QUEST_MIN_LEVEL &&
  h.alignmentRecord ≥ QUEST_MIN_ALIGNMENT &&
  !h.isHostileToLeader

/-- The lifecycle states of the Quest -/
inductive QuestProgress where
  | Unassigned
  | Assigned
  | NemesisDefeated
  | Completed
  deriving Repr, DecidableEq

/-- Numeric ranking for quest progression stages -/
def questProgressRank : QuestProgress → Nat
  | QuestProgress.Unassigned => 0
  | QuestProgress.Assigned => 1
  | QuestProgress.NemesisDefeated => 2
  | QuestProgress.Completed => 3

/-- The artifact possession status -/
inductive ArtifactLocation where
  | HeldByNemesis
  | DroppedOnFloor
  | CarriedByHero
  deriving Repr, DecidableEq

/-- Full state of the Quest branch -/
structure QuestState where
  progress : QuestProgress
  artifactLocation : ArtifactLocation
  nemesisHp : Nat
  deriving Repr, DecidableEq

/-- Consult the Quest Leader to receive the assignment -/
def consultLeader (state : QuestState) (hero : HeroQuestEligibility) : QuestState :=
  match state.progress with
  | QuestProgress.Unassigned =>
    if isHeroEligibleForQuest hero then
      { state with progress := QuestProgress.Assigned }
    else
      state
  | _ => state

/-- Resolve attack on the Nemesis boss -/
def attackNemesis (state : QuestState) (damage : Nat) : QuestState :=
  if state.progress == QuestProgress.Assigned then
    if damage ≥ state.nemesisHp then
      { state with
        progress := QuestProgress.NemesisDefeated,
        nemesisHp := 0,
        artifactLocation := ArtifactLocation.DroppedOnFloor }
    else
      { state with nemesisHp := state.nemesisHp - damage }
  else
    state

/-- Hero claims the dropped Quest Artifact -/
def pickUpQuestArtifact (state : QuestState) : QuestState :=
  if state.artifactLocation == ArtifactLocation.DroppedOnFloor then
    { state with artifactLocation := ArtifactLocation.CarriedByHero }
  else
    state

/-- Return to Quest Leader to receive final blessing and conclude quest -/
def returnToLeaderWithArtifact (state : QuestState) : QuestState :=
  if state.progress == QuestProgress.NemesisDefeated &&
     state.artifactLocation == ArtifactLocation.CarriedByHero then
    { state with progress := QuestProgress.Completed }
  else
    state

/- =========================================================================
   Machine-Verified Theorems: 0 sorrys
   ========================================================================= -/

/-- Theorem: Under-leveled hero is strictly rejected by the Leader -/
theorem leader_rejects_underleveled (state : QuestState) (hero : HeroQuestEligibility)
  (h_lvl : hero.experienceLevel < QUEST_MIN_LEVEL) :
  consultLeader state hero = state := by
  cases h : state.progress
  · simp [consultLeader, h]
    have h_not_elig : isHeroEligibleForQuest hero = false := by
      simp [isHeroEligibleForQuest]
      intro h_ge
      exact False.elim (Nat.lt_le_asymm h_lvl h_ge)
    simp [h_not_elig]
  · simp [consultLeader, h]
  · simp [consultLeader, h]
  · simp [consultLeader, h]

/-- Theorem: Low alignment record hero is strictly rejected by the Leader -/
theorem leader_rejects_low_alignment (state : QuestState) (hero : HeroQuestEligibility)
  (h_align : hero.alignmentRecord < QUEST_MIN_ALIGNMENT) :
  consultLeader state hero = state := by
  cases h : state.progress
  · simp [consultLeader, h]
    have h_not_elig : isHeroEligibleForQuest hero = false := by
      simp [isHeroEligibleForQuest]
      intro _
      intro h_ge
      exact False.elim (Int.lt_irrefl _ (Int.lt_of_lt_of_le h_align h_ge))
    simp [h_not_elig]
  · simp [consultLeader, h]
  · simp [consultLeader, h]
  · simp [consultLeader, h]

/-- Theorem: Fully eligible hero is accepted and advances progress to Assigned -/
theorem leader_accepts_eligible_hero (state : QuestState) (hero : HeroQuestEligibility)
  (h_prog : state.progress = QuestProgress.Unassigned)
  (h_elig : isHeroEligibleForQuest hero = true) :
  (consultLeader state hero).progress = QuestProgress.Assigned := by
  simp [consultLeader, h_prog, h_elig]

/-- Theorem: Non-fatal damage leaves the artifact securely in the hands of the Nemesis -/
theorem non_fatal_nemesis_retains_artifact (state : QuestState) (damage : Nat)
  (h_prog : state.progress = QuestProgress.Assigned)
  (h_art : state.artifactLocation = ArtifactLocation.HeldByNemesis)
  (h_dam : damage < state.nemesisHp) :
  (attackNemesis state damage).artifactLocation = ArtifactLocation.HeldByNemesis := by
  simp [attackNemesis, h_prog]
  have h_not_ge : ¬(damage ≥ state.nemesisHp) := Nat.not_le_of_gt h_dam
  simp [h_not_ge, h_art]

/-- Theorem: Fatal damage to Nemesis strictly drops the Quest Artifact on the floor -/
theorem fatal_nemesis_drops_artifact (state : QuestState) (damage : Nat)
  (h_prog : state.progress = QuestProgress.Assigned)
  (h_dam : damage ≥ state.nemesisHp) :
  (attackNemesis state damage).artifactLocation = ArtifactLocation.DroppedOnFloor := by
  simp [attackNemesis, h_prog, h_dam]

/-- Theorem: Fatal damage to Nemesis advances quest progression to NemesisDefeated -/
theorem fatal_nemesis_advances_progress (state : QuestState) (damage : Nat)
  (h_prog : state.progress = QuestProgress.Assigned)
  (h_dam : damage ≥ state.nemesisHp) :
  (attackNemesis state damage).progress = QuestProgress.NemesisDefeated := by
  simp [attackNemesis, h_prog, h_dam]

/-- Theorem: Quest progression is strictly monotonic non-decreasing under leader consultation -/
theorem consult_leader_monotonic (state : QuestState) (hero : HeroQuestEligibility) :
  questProgressRank state.progress ≤ questProgressRank (consultLeader state hero).progress := by
  cases h : state.progress
  · simp [consultLeader, h]
    by_cases he : isHeroEligibleForQuest hero = true
    · simp [he, questProgressRank]
    · simp [he, h]
  · simp [consultLeader, h]
  · simp [consultLeader, h]
  · simp [consultLeader, h]

/-- Theorem: Quest progression is strictly monotonic non-decreasing under nemesis combat -/
theorem attack_nemesis_monotonic (state : QuestState) (damage : Nat) :
  questProgressRank state.progress ≤ questProgressRank (attackNemesis state damage).progress := by
  cases h : state.progress
  · simp [attackNemesis, h]
  · simp [attackNemesis, h]
    by_cases hd : damage ≥ state.nemesisHp
    · simp [hd, questProgressRank]
    · simp [hd]
  · simp [attackNemesis, h]
  · simp [attackNemesis, h]

/-- Theorem: Full quest completion requires carried artifact -/
theorem completion_requires_carried_artifact (state : QuestState)
  (h_art : state.artifactLocation ≠ ArtifactLocation.CarriedByHero) :
  returnToLeaderWithArtifact state = state := by
  simp [returnToLeaderWithArtifact]
  intro _
  intro h_c
  exact False.elim (h_art h_c)

end NetMechanics
