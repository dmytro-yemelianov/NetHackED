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

def isFloorTrap : TrapType → Bool
  | TrapType.Pit => true
  | TrapType.SpikedPit => true
  | TrapType.Web => true
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

def attemptTrigger (e : Entity) (t : Trap) : TriggerResult × TrapState :=
  match t.state with
  | TrapState.Disarmed => (TriggerResult.Inactive, TrapState.Disarmed)
  | s =>
    if e.isFlying && isFloorTrap t.type then
      (TriggerResult.Avoided, s)
    else
      (TriggerResult.Triggered, TrapState.Revealed)

def untrap (t : Trap) : TrapState :=
  match t.state with
  | TrapState.Revealed => TrapState.Disarmed
  | s => s

theorem flying_avoids_floor_traps (e : Entity) (t : Trap) (h1 : e.isFlying = true) (h2 : isFloorTrap t.type = true) (h3 : t.state ≠ TrapState.Disarmed) :
    (attemptTrigger e t).1 = TriggerResult.Avoided := by
  unfold attemptTrigger
  cases h4 : t.state <;> simp [h4] at h3 ⊢ <;> simp [h1, h2]

theorem non_flying_triggers_floor_trap (e : Entity) (t : Trap) (h1 : e.isFlying = false) (_h2 : isFloorTrap t.type = true) (h3 : t.state ≠ TrapState.Disarmed) :
    (attemptTrigger e t).1 = TriggerResult.Triggered := by
  unfold attemptTrigger
  cases h4 : t.state <;> simp [h4] at h3 ⊢ <;> simp [h1]

theorem triggering_reveals_hidden_trap (e : Entity) (t : Trap) (h1 : e.isFlying = false) (h2 : t.state = TrapState.Hidden) :
    (attemptTrigger e t).2 = TrapState.Revealed := by
  unfold attemptTrigger
  simp [h2, h1]

theorem disarm_trap_neutralizes (e : Entity) (t : Trap) (h1 : t.state = TrapState.Disarmed) :
    (attemptTrigger e t).1 = TriggerResult.Inactive := by
  unfold attemptTrigger
  simp [h1]
