namespace NetMechanics.Ranged

-- 1. Formal Representations

inductive LauncherType
| Bow
| Crossbow
| Sling
deriving Repr, DecidableEq

inductive AmmoClass
| Arrow
| Bolt
| Rock
deriving Repr, DecidableEq

def ammo_matches_launcher (l : LauncherType) (a : AmmoClass) : Bool :=
  match l, a with
  | LauncherType.Bow, AmmoClass.Arrow => true
  | LauncherType.Crossbow, AmmoClass.Bolt => true
  | LauncherType.Sling, AmmoClass.Rock => true
  | _, _ => false

inductive TileType
| Passable
| Obstruction
deriving Repr, DecidableEq

inductive EntityType
| None
| Target
deriving Repr, DecidableEq

structure Tile where
  type : TileType
  entity : EntityType
deriving Repr, DecidableEq

def evaluate_trajectory (tiles : List Tile) : List Tile :=
  match tiles with
  | [] => []
  | t :: ts =>
    match t.type, t.entity with
    | TileType.Obstruction, _ => [t]
    | _, EntityType.Target => [t]
    | _, _ => t :: evaluate_trajectory ts

inductive ProjectileOutcome
| Ground
| Destroyed
deriving Repr, DecidableEq

def resolve_impact (breakage_prob : Nat) (roll : Nat) : ProjectileOutcome :=
  if roll < breakage_prob then ProjectileOutcome.Destroyed
  else ProjectileOutcome.Ground

structure Creature where
  hasSaddle : Bool
  movementCost : Nat

structure Hero where
  ridingSkill : Nat
  unmountedCost : Nat
  mounted : Option Creature

def effectiveMovementCost (h : Hero) : Nat :=
  match h.mounted with
  | none => h.unmountedCost
  | some m => min h.unmountedCost m.movementCost

def CanMount (m : Creature) : Prop := m.hasSaddle = true

-- 2. Theorems

theorem projectile_stops_at_obstacle (t : Tile) (ts : List Tile) :
    t.type = TileType.Obstruction →
    evaluate_trajectory (t :: ts) = [t] := by
  intro h
  unfold evaluate_trajectory
  cases h2 : t.entity
  · rw [h]
  · rw [h]

theorem ammo_breakage_preserves_or_destroys (prob roll : Nat) :
    resolve_impact prob roll = ProjectileOutcome.Ground ∨ resolve_impact prob roll = ProjectileOutcome.Destroyed := by
  unfold resolve_impact
  split
  · right; rfl
  · left; rfl

theorem mount_speed_cost_monotonic (h : Hero) (m : Creature) :
    h.mounted = some m → effectiveMovementCost h ≤ h.unmountedCost := by
  intro h1
  unfold effectiveMovementCost
  rw [h1]
  exact Nat.min_le_left h.unmountedCost m.movementCost

theorem saddle_required_for_riding (m : Creature) :
    m.hasSaddle = false → ¬ CanMount m := by
  intro h1 h2
  unfold CanMount at h2
  rw [h1] at h2
  contradiction

end NetMechanics.Ranged
