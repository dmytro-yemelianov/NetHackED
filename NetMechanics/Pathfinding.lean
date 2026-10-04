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
  Helper (one step): stepping to a strictly closer neighbour lowers the
  distance by at least 1. Used by `pathfinding_converges_within`.
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

/--
  Iterated gradient descent: `descend oracle k s` applies `descentStep`
  `k` times, asking `oracle` for the reduced neighbour at each state.
-/
def descend (oracle : MetricState → Option MetricState) : Nat → MetricState → MetricState
  | 0, s => s
  | k + 1, s => descend oracle k (descentStep s (oracle s))

/-- At the target (distance 0) a descent step is the identity. -/
theorem descentStep_at_target (s : MetricState) (n : Option MetricState)
    (h : s.distToTarget = 0) : descentStep s n = s := by
  simp [descentStep, h]

/--
  Theorem: Finite Pathfinding Convergence.
  If the oracle always offers a strictly closer neighbour away from the target
  (an admissible gradient), then iterating the descent step `k ≥ D` times from
  initial distance `D` reaches the target (distance 0).
-/
theorem pathfinding_converges_within (oracle : MetricState → Option MetricState)
    (hgrad : ∀ s : MetricState, s.distToTarget > 0 →
      ∃ n, oracle s = some n ∧ n.distToTarget < s.distToTarget)
    (k : Nat) (s : MetricState) (hk : s.distToTarget ≤ k) :
    (descend oracle k s).distToTarget = 0 := by
  induction k generalizing s with
  | zero => simp [descend]; omega
  | succ k ih =>
    simp only [descend]
    by_cases h0 : s.distToTarget = 0
    · rw [descentStep_at_target s _ h0]
      exact ih s (by omega)
    · obtain ⟨n, hn, hlt⟩ := hgrad s (by omega)
      have hb := pathfinding_step_bounded s n hlt
      rw [hn]
      exact ih _ (by omega)

end NetMechanics

