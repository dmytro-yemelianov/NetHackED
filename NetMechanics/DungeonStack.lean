/-
  NetHack Mechanics Formalized in Lean 4: Multi-Floor Dungeon Stack & Level Transitions
  Formally proves depth invariants, invertibility of stair traversals,
  and bounded depth progression.
-/

namespace NetMechanics

/-- Valid dungeon depth is a positive natural number starting at 1 (Dungeons of Doom depth 1) -/
@[ext]
structure DungeonDepth where
  val : Nat
  h_pos : val >= 1
deriving Repr, DecidableEq

/-- Construct starting dungeon depth 1 -/
def surfaceDepth : DungeonDepth :=
  ⟨1, by decide⟩

/-- Descending down stairs strictly increases depth by 1 -/
def descendDepth (d : DungeonDepth) : DungeonDepth :=
  ⟨d.val + 1, by
    have h := d.h_pos
    omega⟩

/--
  Ascending stairs decreases depth if depth > 1.
  At depth 1, celestial forces bar escape, preserving depth 1.
-/
def ascendDepth (d : DungeonDepth) : DungeonDepth :=
  if h : d.val > 1 then
    ⟨d.val - 1, by omega⟩
  else
    d

/-- Theorem: Descending from depth d strictly increases the depth value -/
theorem descend_strictly_increases (d : DungeonDepth) :
  (descendDepth d).val > d.val := by
  dsimp [descendDepth]
  omega

/-- Theorem: Ascending from depth d > 1 strictly decreases the depth value -/
theorem ascend_strictly_decreases (d : DungeonDepth) (h : d.val > 1) :
  (ascendDepth d).val < d.val := by
  dsimp [ascendDepth]
  split
  · next _ =>
    dsimp
    omega
  · contradiction

/--
  Theorem: Ascend is the exact left inverse of Descend.
  Descending to d + 1 and immediately ascending returns to the exact same depth d.
-/
theorem ascend_descend_inverse (d : DungeonDepth) :
  ascendDepth (descendDepth d) = d := by
  dsimp [ascendDepth, descendDepth]
  split
  · ext
    dsimp
  · next h_not =>
    have hp := d.h_pos
    omega

/-- Theorem: Surface depth is invariant under Ascend (celestial force barrier) -/
theorem surface_ascend_invariant :
  ascendDepth surfaceDepth = surfaceDepth := by
  dsimp [ascendDepth, surfaceDepth]

end NetMechanics
