/-
  NetHack Mechanics Formalized in Lean 4: Wand Beam Propagation & Wall Reflection
  Formalizing discrete beam raycasting, specular reflection on axis-aligned walls,
  energy attenuation, and proof of guaranteed loop termination.
-/

import NetMechanics.Grid

namespace NetMechanics

/-- Surface orientation of a reflective obstacle -/
inductive SurfaceOrientation where
  | Horizontal  -- Wall running along x-axis (reflects y velocity)
  | Vertical    -- Wall running along y-axis (reflects x velocity)
  | Corner      -- Orthogonal corner (reflects both velocities)
deriving Repr, DecidableEq

/-- Velocity vector on discrete integer grid -/
structure Velocity where
  dx : Int
  dy : Int
deriving Repr, DecidableEq

/-- Specular reflection operator on discrete velocity -/
def reflect (v : Velocity) : SurfaceOrientation → Velocity
  | SurfaceOrientation.Horizontal => { dx := v.dx, dy := -v.dy }
  | SurfaceOrientation.Vertical   => { dx := -v.dx, dy := v.dy }
  | SurfaceOrientation.Corner     => { dx := -v.dx, dy := -v.dy }

/-- Wand ray beam state with position, velocity, and remaining energy/range -/
structure BeamRay where
  x : Int
  y : Int
  vel : Velocity
  energy : Nat
deriving Repr, DecidableEq

/--
  Single step of beam propagation:
  Consumes exactly 1 unit of energy.
  If hitting reflective surface, reverses velocity component.
  Otherwise advances position.
-/
inductive StepResult where
  | Terminated : StepResult
  | Advanced (ray : BeamRay) : StepResult
  | Reflected (ray : BeamRay) : StepResult
deriving Repr, DecidableEq

def stepRay (ray : BeamRay) (hitWall : Option SurfaceOrientation) : StepResult :=
  match ray.energy with
  | 0 => StepResult.Terminated
  | Nat.succ e =>
    match hitWall with
    | none =>
      StepResult.Advanced {
        x := ray.x + ray.vel.dx,
        y := ray.y + ray.vel.dy,
        vel := ray.vel,
        energy := e
      }
    | some orientation =>
      StepResult.Reflected {
        x := ray.x,
        y := ray.y,
        vel := reflect ray.vel orientation,
        energy := e
      }

/--
  Theorem: Reflection is an Involution.
  Applying the same reflection twice returns the original velocity vector.
-/
theorem reflect_involution (v : Velocity) (s : SurfaceOrientation) :
  reflect (reflect v s) s = v := by
  cases s
  · simp [reflect, Int.neg_neg]
  · simp [reflect, Int.neg_neg]
  · simp [reflect, Int.neg_neg]

/--
  Theorem: Kinetic Energy / Speed Vector Magnitude Preservation.
  Reflection preserves discrete Euclidean speed squared: dx^2 + dy^2.
-/
theorem reflect_preserves_speed_sq (v : Velocity) (s : SurfaceOrientation) :
  let v' := reflect v s
  v'.dx * v'.dx + v'.dy * v'.dy = v.dx * v.dx + v.dy * v.dy := by
  cases s
  · simp [reflect, Int.neg_mul_neg]
  · simp [reflect, Int.neg_mul_neg]
  · simp [reflect, Int.neg_mul_neg]

/--
  Theorem: Strict Energy Attenuation.
  Every step of beam propagation (whether advancing or reflecting)
  strictly decreases ray energy by 1.
-/
theorem step_decreases_energy (ray : BeamRay) (wall : Option SurfaceOrientation)
  (ray' : BeamRay)
  (h : stepRay ray wall = StepResult.Advanced ray' ∨ stepRay ray wall = StepResult.Reflected ray') :
  ray'.energy < ray.energy := by
  cases he : ray.energy with
  | zero =>
    simp [stepRay, he] at h
  | succ e =>
    cases wall with
    | none =>
      simp [stepRay, he] at h
      subst h
      exact Nat.lt_succ_self e
    | some orientation =>
      simp [stepRay, he] at h
      subst h
      exact Nat.lt_succ_self e

/--
  Helper (one step): a ray with zero energy terminates on its next step.
  Used as the base case of `beam_terminates_within`.
-/
theorem beam_terminates_after_energy_steps (ray : BeamRay) (he : ray.energy = 0)
  (wall : Option SurfaceOrientation) :
  stepRay ray wall = StepResult.Terminated := by
  simp [stepRay, he]

/--
  Fuel-bounded beam runner: performs at most `fuel` calls of `stepRay`,
  feeding `walls i` as the obstacle seen on the `i`-th call, and stops as
  soon as a step returns `Terminated`. If the fuel runs out first the
  still-live ray is returned as `Advanced`.
-/
def runRay (walls : Nat → Option SurfaceOrientation) : Nat → BeamRay → StepResult
  | 0, ray => StepResult.Advanced ray
  | fuel + 1, ray =>
    match stepRay ray (walls 0) with
    | StepResult.Terminated => StepResult.Terminated
    | StepResult.Advanced r => runRay (fun i => walls (i + 1)) fuel r
    | StepResult.Reflected r => runRay (fun i => walls (i + 1)) fuel r

/-- A live step (advance or reflect) consumes exactly one unit of energy. -/
theorem stepRay_succ (ray : BeamRay) (e : Nat) (he : ray.energy = e + 1)
    (wall : Option SurfaceOrientation) :
    ∃ r, r.energy = e ∧
      (stepRay ray wall = StepResult.Advanced r ∨ stepRay ray wall = StepResult.Reflected r) := by
  cases wall with
  | none =>
    exact ⟨{ x := ray.x + ray.vel.dx, y := ray.y + ray.vel.dy, vel := ray.vel, energy := e },
      rfl, Or.inl (by simp [stepRay, he])⟩
  | some o =>
    exact ⟨{ x := ray.x, y := ray.y, vel := reflect ray.vel o, energy := e },
      rfl, Or.inr (by simp [stepRay, he])⟩

/--
  Theorem: Guaranteed Termination of Wand Beams (multi-step).
  For any wall sequence, a ray with energy E has terminated after at most
  E + 1 calls of `stepRay` (E energy-consuming advance/reflect steps, then
  the terminating call). Beam bouncing can never loop forever.
-/
theorem beam_terminates_within (walls : Nat → Option SurfaceOrientation)
    (fuel : Nat) (ray : BeamRay) (hfuel : ray.energy < fuel) :
    runRay walls fuel ray = StepResult.Terminated := by
  induction fuel generalizing ray walls with
  | zero => exact absurd hfuel (Nat.not_lt_zero _)
  | succ n ih =>
    cases he : ray.energy with
    | zero =>
      simp [runRay, beam_terminates_after_energy_steps ray he]
    | succ e =>
      obtain ⟨r, hr, hstep⟩ := stepRay_succ ray e he (walls 0)
      have hr' : r.energy < n := by omega
      rcases hstep with hs | hs <;> simp [runRay, hs, ih _ r hr']

/--
  Theorem: The bound is tight — with fuel at most E the beam is still live,
  so a ray of energy E performs exactly E non-terminal steps.
-/
theorem beam_live_within_energy (walls : Nat → Option SurfaceOrientation)
    (fuel : Nat) (ray : BeamRay) (hfuel : fuel ≤ ray.energy) :
    runRay walls fuel ray ≠ StepResult.Terminated := by
  induction fuel generalizing ray walls with
  | zero => simp [runRay]
  | succ n ih =>
    obtain ⟨e, he⟩ : ∃ e, ray.energy = e + 1 := ⟨ray.energy - 1, by omega⟩
    obtain ⟨r, hr, hstep⟩ := stepRay_succ ray e he (walls 0)
    have hr' : n ≤ r.energy := by omega
    rcases hstep with hs | hs <;> simp [runRay, hs, ih _ r hr']

end NetMechanics
