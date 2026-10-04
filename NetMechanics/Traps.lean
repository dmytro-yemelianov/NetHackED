inductive TrapType
  | Arrow
  | Dart
  | RockFall
  | Pit
  | SpikedPit
  | Teleport
  | Fire
  | LevelTeleport
  | Polymorph
  | AntiMagic
  | SleepingGas
  | Rust
  | Web
deriving DecidableEq

inductive TrapState
  | Hidden
  | Revealed
  | Disarmed
deriving DecidableEq

/-- NetHack 3.7 `trap.c:1061 floor_trigger` restricted to the variants modelled here.
Teleport, LevelTeleport, Polymorph, AntiMagic and Web are not floor traps. -/
def isFloorTrap : TrapType → Bool
  | TrapType.Arrow => true
  | TrapType.Dart => true
  | TrapType.RockFall => true
  | TrapType.Pit => true
  | TrapType.SpikedPit => true
  | TrapType.Fire => true
  | TrapType.SleepingGas => true
  | TrapType.Rust => true
  | _ => false

structure Trap where
  type : TrapType
  state : TrapState

structure Entity where
  isFlying : Bool

inductive TriggerResult
  | Avoided
  | Triggered
  | Inactive
deriving DecidableEq

/-- `dotrap` (trap.c:2996-3046). `rn2_5` is the C `rn2(5)` draw; a seen (Revealed)
trap is escaped when it is 0. -/
def attemptTrigger (e : Entity) (t : Trap) (rn2_5 : Nat) : TriggerResult × TrapState :=
  match t.state with
  | TrapState.Disarmed => (TriggerResult.Inactive, TrapState.Disarmed)
  | s =>
    if e.isFlying && isFloorTrap t.type then
      (TriggerResult.Avoided, s)
    else if s == TrapState.Revealed && rn2_5 % 5 == 0 then
      (TriggerResult.Avoided, s)
    else
      (TriggerResult.Triggered, TrapState.Revealed)

def untrap (t : Trap) : TrapState :=
  match t.state with
  | TrapState.Revealed => TrapState.Disarmed
  | s => s

theorem flying_avoids_floor_traps (e : Entity) (t : Trap) (rn2_5 : Nat) (h1 : e.isFlying = true) (h2 : isFloorTrap t.type = true) (h3 : t.state ≠ TrapState.Disarmed) :
    (attemptTrigger e t rn2_5).1 = TriggerResult.Avoided := by
  unfold attemptTrigger
  cases h4 : t.state <;> simp [h4] at h3 ⊢ <;> simp [h1, h2]

/-- Web (and the other non-floor traps) are not avoided by flying, unless a seen-trap escape applies. -/
theorem flying_does_not_avoid_non_floor_trap (e : Entity) (t : Trap) (rn2_5 : Nat) (h2 : isFloorTrap t.type = false) (h3 : t.state = TrapState.Hidden) :
    (attemptTrigger e t rn2_5).1 = TriggerResult.Triggered := by
  unfold attemptTrigger
  simp [h3, h2]

theorem web_not_floor_trap : isFloorTrap TrapType.Web = false := rfl

theorem non_flying_triggers_floor_trap (e : Entity) (t : Trap) (rn2_5 : Nat) (h1 : e.isFlying = false) (h3 : t.state ≠ TrapState.Disarmed) (h4 : t.state = TrapState.Hidden ∨ rn2_5 % 5 ≠ 0) :
    (attemptTrigger e t rn2_5).1 = TriggerResult.Triggered := by
  unfold attemptTrigger
  cases h5 : t.state <;> simp [h5] at h3 h4 ⊢ <;> simp [h1, h4]

/-- A seen (Revealed) trap is escaped exactly when `rn2(5) = 0`, for a non-flying entity. -/
theorem seen_trap_escape_iff (e : Entity) (t : Trap) (rn2_5 : Nat) (h1 : e.isFlying = false) (h2 : t.state = TrapState.Revealed) :
    (attemptTrigger e t rn2_5).1 = TriggerResult.Avoided ↔ rn2_5 % 5 = 0 := by
  unfold attemptTrigger
  by_cases h : rn2_5 % 5 = 0 <;> simp [h2, h1, h]

theorem triggering_reveals_hidden_trap (e : Entity) (t : Trap) (rn2_5 : Nat) (h1 : e.isFlying = false) (h2 : t.state = TrapState.Hidden) :
    (attemptTrigger e t rn2_5).2 = TrapState.Revealed := by
  unfold attemptTrigger
  simp [h2, h1]

theorem disarm_trap_neutralizes (e : Entity) (t : Trap) (rn2_5 : Nat) (h1 : t.state = TrapState.Disarmed) :
    (attemptTrigger e t rn2_5).1 = TriggerResult.Inactive := by
  unfold attemptTrigger
  simp [h1]
