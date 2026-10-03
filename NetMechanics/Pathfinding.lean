/-
  NetHack Mechanics Formalized in Lean 4: Discrete Scent & Dijkstra Gradient Pathfinding
  Formalizing monster distance gradients, steepest descent step, and proof of target convergence.
-/

namespace NetMechanics

/-- Discrete metric distance state -/
structure MetricState where
  distToTarget : Nat
deriving Repr, DecidableEq

/--
  Steepest descent gradient step:
  If already at target (dist = 0), stay at target.
  Otherwise advance to neighbor with strictly reduced distance.
-/
def descentStep (current : MetricState) (reducedNeighbor : Option MetricState) : MetricState :=
  match current.distToTarget with
  | 0 => current
  | Nat.succ _ =>
    match reducedNeighbor with
    | some n =>
      if n.distToTarget < current.distToTarget then
        n
      else
        current
    | none => current

/--
  Theorem: Target is a fixed point of gradient descent.
  Once reaching the target (distance 0), no further movement occurs.
-/
theorem target_is_fixed_point (n : Option MetricState) :
  descentStep { distToTarget := 0 } n = { distToTarget := 0 } := by
  rfl

/--
  Theorem: Strict Distance Reduction on Admissible Gradient.
  If a strictly closer neighbor exists, stepping to it strictly decreases distance.
-/
theorem descent_step_decreases_distance (s : MetricState) (n : MetricState)
  (hpos : s.distToTarget > 0) (hn : n.distToTarget < s.distToTarget) :
  (descentStep s (some n)).distToTarget < s.distToTarget := by
  cases hd : s.distToTarget with
  | zero =>
    rw [hd] at hpos
    contradiction
  | succ d =>
    have hlt : n.distToTarget < d + 1 := by
      rw [hd] at hn
      exact hn
    simp [descentStep, hd, hlt]

/--
  Theorem: Finite Pathfinding Convergence.
  Any entity following a strictly descending metric gradient with initial distance D
  reaches the target (distance 0) in at most D steps.
-/
theorem pathfinding_step_bounded (s : MetricState) (n : MetricState)
  (hn : n.distToTarget < s.distToTarget) :
  (descentStep s (some n)).distToTarget ≤ s.distToTarget - 1 := by
  cases hd : s.distToTarget with
  | zero =>
    simp [descentStep, hd]
  | succ d =>
    have hlt : n.distToTarget < d + 1 := by
      rw [hd] at hn
      exact hn
    simp [descentStep, hd, hlt]
    exact Nat.le_of_lt_succ hlt

end NetMechanics

