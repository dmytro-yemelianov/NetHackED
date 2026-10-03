/-
  NetHack Mechanics Formalized in Lean 4: Gehennom & The Invocation Ritual
  Formalizing the Bell of Opening, Candelabrum of Invocation, Book of the Dead,
  the Vibrating Square, and the passage into Moloch's Sanctum.
-/

import NetMechanics.Grid

namespace NetMechanics

/-- State of the Candelabrum of Invocation -/
structure CandelabrumState where
  candleCount : Nat
  isLit : Bool
  deriving Repr, DecidableEq

/-- The canonical number of candles required for the Candelabrum of Invocation -/
def REQUIRED_CANDLES : Nat := 7

/-- Check if the Candelabrum of Invocation is ready for the ritual -/
def isCandelabrumReady (c : CandelabrumState) : Bool :=
  c.candleCount = REQUIRED_CANDLES && c.isLit

/-- The three steps of the Invocation Ritual -/
inductive InvocationStep where
  | RingBell
  | LightCandelabrum
  | ReadBook
  deriving Repr, DecidableEq

/-- Ritual progression state -/
inductive RitualProgress where
  | Uninitiated
  | BellResounding
  | CandlesBurning
  | SanctumOpened
  deriving Repr, DecidableEq

/--
  Execute one step of the Invocation Ritual.
  - Step 1: Ring Bell of Opening -> advances Uninitiated to BellResounding.
  - Step 2: Light Candelabrum (with 7 candles) -> advances BellResounding to CandlesBurning.
  - Step 3: Read Book of the Dead on Vibrating Square -> advances CandlesBurning to SanctumOpened.
-/
def stepRitual (curr : RitualProgress) (step : InvocationStep) (onVibratingSquare : Bool) (candelabrum : CandelabrumState) : RitualProgress :=
  match curr, step with
  | RitualProgress.Uninitiated, InvocationStep.RingBell =>
    RitualProgress.BellResounding
  | RitualProgress.BellResounding, InvocationStep.LightCandelabrum =>
    if isCandelabrumReady candelabrum then
      RitualProgress.CandlesBurning
    else
      curr
  | RitualProgress.CandlesBurning, InvocationStep.ReadBook =>
    if onVibratingSquare then
      RitualProgress.SanctumOpened
    else
      curr
  | RitualProgress.SanctumOpened, _ =>
    RitualProgress.SanctumOpened
  | _, _ =>
    curr

/-- Moloch's Sanctum portal status -/
def isSanctumAccessible (r : RitualProgress) : Bool :=
  r = RitualProgress.SanctumOpened

-- =========================================================================
-- THEOREMS: Gehennom Invocation Ritual
-- =========================================================================

/-- Theorem: Candelabrum cannot be ready without exactly 7 candles. -/
theorem candelabrum_requires_seven_candles (c : CandelabrumState) (h : c.candleCount ≠ REQUIRED_CANDLES) :
    isCandelabrumReady c = false := by
  unfold isCandelabrumReady
  simp [h]

/-- Theorem: An unlit candelabrum is never ready regardless of candle count. -/
theorem unlit_candelabrum_not_ready (count : Nat) :
    isCandelabrumReady { candleCount := count, isLit := false } = false := by
  simp [isCandelabrumReady]

/-- Theorem: Sanctum cannot be opened without standing on the Vibrating Square. -/
theorem reading_book_off_vibrating_square_fails (curr : RitualProgress) (c : CandelabrumState) :
    stepRitual curr InvocationStep.ReadBook false c ≠ RitualProgress.SanctumOpened ↔ curr ≠ RitualProgress.SanctumOpened := by
  cases curr <;> simp [stepRitual]

/-- Theorem: The completed 3-step ritual on the Vibrating Square successfully unlocks the Sanctum. -/
theorem full_ritual_unlocks_sanctum (c : CandelabrumState) (hready : isCandelabrumReady c = true) :
    let s1 := stepRitual RitualProgress.Uninitiated InvocationStep.RingBell true c
    let s2 := stepRitual s1 InvocationStep.LightCandelabrum true c
    let s3 := stepRitual s2 InvocationStep.ReadBook true c
    s3 = RitualProgress.SanctumOpened := by
  dsimp [stepRitual]
  simp [hready]

/-- Theorem: Once opened, the subterranean stairway to the Sanctum remains permanently accessible. -/
theorem sanctum_opening_is_permanent (step : InvocationStep) (onSquare : Bool) (c : CandelabrumState) :
    stepRitual RitualProgress.SanctumOpened step onSquare c = RitualProgress.SanctumOpened := by
  rfl

end NetMechanics
