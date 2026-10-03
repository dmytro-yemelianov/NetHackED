/-
  NetHack Mechanics Formalized in Lean 4: Dynamic Lighting, Darkness & Enhanced Vision
  Formalizing light raycasting, dark caverns, oil lamp fuel consumption, blindness, and telepathy.
-/

import NetMechanics.Grid

namespace NetMechanics

/-- Light source properties (oil lamp, lantern, magic lamp, candle) -/
structure LightSource where
  radius : Nat
  fuel : Nat
  isLit : Bool
  deriving Repr, DecidableEq

/-- Standard luminescence radii in NetHack -/
def OIL_LAMP_RADIUS : Nat := 3
def LANTERN_RADIUS : Nat := 2
def CANDLE_RADIUS : Nat := 1

/-- Check if a light source is actively emitting illumination -/
def isSourceActive (ls : LightSource) : Bool :=
  ls.isLit && ls.fuel > 0

/-- Effective luminescence radius of a light source -/
def effectiveRadius (ls : LightSource) : Nat :=
  if isSourceActive ls then ls.radius else 0

/-- Consume 1 unit of lamp fuel per turn -/
def tickLightFuel (fuel : Nat) : Nat :=
  fuel - 1

/--
  Perception of a dungeon tile.
  - Blind heroes cannot see any tiles visually.
  - In dark rooms, tiles are invisible unless illuminated by a light source or felt at melee distance (<= 1).
-/
def canSeeTile (blind : Bool) (dist : Nat) (isDark : Bool) (isIlluminated : Bool) : Bool :=
  if blind then
    false
  else if !isDark then
    true
  else
    isIlluminated || dist ≤ 1

/--
  Perception of a monster.
  - Visual sight if the tile is visible.
  - Telepathic ESP sensing if hero has telepathy and monster has a conscious mind, even when blind.
-/
def canDetectMonster (blind : Bool) (telepathy : Bool) (hasMind : Bool) (tileVisible : Bool) : Bool :=
  if tileVisible then
    true
  else if blind then
    telepathy && hasMind
  else
    telepathy && hasMind

-- =========================================================================
-- THEOREMS: Dynamic Lighting & Perception
-- =========================================================================

/-- Theorem: Blindness unconditionally extinguishes direct visual sight of tiles. -/
theorem blind_blocks_sight (dist : Nat) (isDark : Bool) (isIlluminated : Bool) :
    canSeeTile true dist isDark isIlluminated = false := by
  simp [canSeeTile]

/-- Theorem: Telepathy detects conscious minds regardless of blindness or tile visibility. -/
theorem telepathy_detects_thinking_monsters (blind : Bool) (tileVisible : Bool) :
    canDetectMonster blind true true tileVisible = true := by
  cases tileVisible <;> cases blind <;> simp [canDetectMonster]

/-- Theorem: Mindless monsters (e.g. undead, golems) cannot be detected by telepathy when blind. -/
theorem mindless_undetected_when_blind (telepathy : Bool) :
    canDetectMonster true telepathy false false = false := by
  cases telepathy <;> simp [canDetectMonster]

/-- Theorem: Effective luminescence radius is strictly bounded by lamp maximum radius. -/
theorem light_radius_bounded (ls : LightSource) :
    effectiveRadius ls ≤ ls.radius := by
  unfold effectiveRadius
  split
  · exact Nat.le_refl _
  · exact Nat.zero_le _

/-- Theorem: An unlit or empty light source emits 0 radius. -/
theorem inactive_light_radius_zero (ls : LightSource) (h : ¬(isSourceActive ls)) :
    effectiveRadius ls = 0 := by
  unfold effectiveRadius
  simp [h]

/-- Theorem: In a dark room without illumination beyond melee distance, tiles are invisible. -/
theorem dark_room_requires_light (dist : Nat) (hdist : dist > 1) :
    canSeeTile false dist true false = false := by
  simp [canSeeTile]
  exact hdist

/-- Theorem: An illuminated tile in a dark room is clearly visible. -/
theorem dark_room_illuminated_visible (dist : Nat) :
    canSeeTile false dist true true = true := by
  simp [canSeeTile]

/-- Theorem: Fuel consumption monotonically decreases remaining fuel. -/
theorem fuel_monotonically_decreases (fuel : Nat) :
    tickLightFuel fuel ≤ fuel := by
  unfold tickLightFuel
  exact Nat.sub_le fuel 1

/-- Theorem: Depleted fuel reaches 0 and stays at 0. -/
theorem fuel_exhaustion_terminates_light (fuel : Nat) (h : fuel = 0) :
    tickLightFuel fuel = 0 := by
  subst h
  rfl

end NetMechanics
