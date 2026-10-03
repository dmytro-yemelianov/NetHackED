namespace NetMechanics

inductive StatusState
  | clean
  | petrifying (timer : Nat)
  | sliming (timer : Nat)
  | transient (duration : Nat)
  | fatal_stone_death
  | green_slime

inductive Action
  | eat_lizard
  | take_fire_damage
  | pass_turn

def apply_action (s : StatusState) (a : Action) : StatusState :=
  match s, a with
  | StatusState.petrifying _, Action.eat_lizard => StatusState.clean
  | StatusState.sliming _, Action.take_fire_damage => StatusState.clean
  | StatusState.petrifying (t + 1), Action.pass_turn => StatusState.petrifying t
  | StatusState.petrifying 0, Action.pass_turn => StatusState.fatal_stone_death
  | StatusState.sliming (t + 1), Action.pass_turn => StatusState.sliming t
  | StatusState.sliming 0, Action.pass_turn => StatusState.green_slime
  | StatusState.transient (t + 1), Action.pass_turn => StatusState.transient t
  | StatusState.transient 0, Action.pass_turn => StatusState.clean
  | s, _ => s

theorem petrification_timer_decrements_strictly (t : Nat) :
  apply_action (StatusState.petrifying (t + 1)) Action.pass_turn = StatusState.petrifying t := by
  rfl

theorem petrification_reaches_zero_is_fatal :
  apply_action (StatusState.petrifying 0) Action.pass_turn = StatusState.fatal_stone_death := by
  rfl

theorem lizard_cure_restores_unpetrified (t : Nat) :
  apply_action (StatusState.petrifying t) Action.eat_lizard = StatusState.clean := by
  rfl

theorem sliming_fire_cure_cleans (t : Nat) :
  apply_action (StatusState.sliming t) Action.take_fire_damage = StatusState.clean := by
  rfl

theorem transient_status_decays_to_zero (t : Nat) :
  apply_action (StatusState.transient (t + 1)) Action.pass_turn = StatusState.transient t := by
  rfl

end NetMechanics
