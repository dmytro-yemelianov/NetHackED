/-
  NetHack Mechanics Formalized in Lean 4: Turn Scheduling & Energy System
  Based on allmain.c moveloop_core() and mon.c mcalcmove().
-/

namespace NetMechanics

/-- Standard speed constant in NetHack (NORMAL_SPEED) -/
def NORMAL_SPEED : Nat := 12

/-- Turn State capturing scheduler variables -/
structure SchedulerState where
  turn : Nat
  heroEnergy : Nat
  heroSpeed : Nat
  monsterEnergy : Nat
  monsterSpeed : Nat
deriving Repr, DecidableEq

/-- Whether the hero has enough movement points to act -/
def heroCanAct (s : SchedulerState) : Bool :=
  s.heroEnergy ≥ NORMAL_SPEED

/-- Whether the monster has enough movement points to act -/
def monsterCanAct (s : SchedulerState) : Bool :=
  s.monsterEnergy ≥ NORMAL_SPEED

/-- Hero executes an action costing energy -/
def heroAct (s : SchedulerState) (cost : Nat := NORMAL_SPEED) : SchedulerState :=
  { s with heroEnergy := s.heroEnergy - cost }

/-- Monster executes an action costing energy -/
def monsterAct (s : SchedulerState) (cost : Nat := NORMAL_SPEED) : SchedulerState :=
  { s with monsterEnergy := s.monsterEnergy - cost }

/--
  Turn allocation phase (when both hero and monsters are out of steam):
  Corresponds to allmain.c lines 222-245:
    svm.moves++;
    hero.movement += speed;
    monster.movement += speed;
-/
def allocateNewTurn (s : SchedulerState) : SchedulerState :=
  { s with
    turn := s.turn + 1,
    heroEnergy := s.heroEnergy + s.heroSpeed,
    monsterEnergy := s.monsterEnergy + s.monsterSpeed
  }

/--
  Theorem: Turn advancement is strictly monotonic.
  Every turn allocation strictly increments the global turn counter.
-/
theorem turn_strictly_increases (s : SchedulerState) :
  (allocateNewTurn s).turn = s.turn + 1 := by
  rfl

/--
  Theorem: Energy Consumption.
  Executing an action strictly reduces energy when cost > 0 and energy >= cost.
-/
theorem hero_act_reduces_energy (s : SchedulerState) (cost : Nat)
    (hcost : 0 < cost) (he : cost ≤ s.heroEnergy) :
  (heroAct s cost).heroEnergy < s.heroEnergy := by
  unfold heroAct
  dsimp
  exact Nat.sub_lt_self hcost he

/--
  Theorem: Speed injection is monotonic.
  When speeds are positive, new turn allocation strictly increases available energy.
-/
theorem new_turn_increases_hero_energy (s : SchedulerState) (hspd : 0 < s.heroSpeed) :
  s.heroEnergy < (allocateNewTurn s).heroEnergy := by
  unfold allocateNewTurn
  exact Nat.lt_add_of_pos_right hspd

/--
  Simulation Step:
  1. If hero can act, hero acts.
  2. Else if monster can act, monster acts.
  3. Else, allocate new turn.
-/
inductive StepAction where
  | HeroStep
  | MonsterStep
  | TurnTick
deriving Repr, DecidableEq

def stepScheduler (s : SchedulerState) : SchedulerState × StepAction :=
  if s.heroEnergy ≥ NORMAL_SPEED then
    (heroAct s NORMAL_SPEED, StepAction.HeroStep)
  else if s.monsterEnergy ≥ NORMAL_SPEED then
    (monsterAct s NORMAL_SPEED, StepAction.MonsterStep)
  else
    (allocateNewTurn s, StepAction.TurnTick)

/--
  Theorem: Progress.
  Every step either consumes hero energy, consumes monster energy,
  or advances the global turn counter.
-/
theorem scheduler_progress (s : SchedulerState) :
  let res := stepScheduler s
  (res.2 = StepAction.HeroStep ∧ res.1.heroEnergy = s.heroEnergy - NORMAL_SPEED) ∨
  (res.2 = StepAction.MonsterStep ∧ res.1.monsterEnergy = s.monsterEnergy - NORMAL_SPEED) ∨
  (res.2 = StepAction.TurnTick ∧ res.1.turn = s.turn + 1) := by
  unfold stepScheduler heroAct monsterAct allocateNewTurn
  split
  · next hhero =>
    exact Or.inl ⟨rfl, rfl⟩
  · next hnot_hero =>
    split
    · next hmon =>
      exact Or.inr (Or.inl ⟨rfl, rfl⟩)
    · next hnot_mon =>
      exact Or.inr (Or.inr ⟨rfl, rfl⟩)


end NetMechanics
