import NetMechanics.Architecture

/-!
# Ordered module workflows and owned continuations

The module supplies phases, control-flow edges and a relational rule. The kernel
does not prescribe a game's action order or deterministic choices. A checkpoint
retains the committed prefix, including state changes made before suspension.
Pipeline identities are opaque bindings: the runtime must bind them to real code.
This model is not a Rust runner, serializer or complete NetHack zap model.
-/

namespace NetMechanics.Architecture.Workflow

structure Pipeline where
  owner : ModuleManifest
  point : String
  providers : List ModuleManifest := []
  extensions : List ExtensionManifest := []
  codeIdentity : String
  deriving Repr, DecidableEq

/-- Host identity is an execution namespace, not a required world or entity. -/
structure OperationId where
  host : Nat
  serial : Nat
  deriving Repr, DecidableEq

inductive Status (Result Prompt : Type) where
  | running
  | suspended (prompt : Prompt)
  | finished (result : Result)
  | rejected

def Status.isRunning : Status Result Prompt → Bool
  | .running => true
  | _ => false

def Status.isSuspended : Status Result Prompt → Bool
  | .suspended _ => true
  | _ => false

structure Frame (State Phase Event Result Prompt : Type) where
  operation : OperationId
  sequence : Nat
  pipeline : Pipeline
  state : State
  phase : Phase
  trace : List Event
  status : Status Result Prompt

inductive Outcome (State Phase Event Result Prompt : Type) where
  | advance (next : Phase) (state : State) (events : List Event)
  | suspend (next : Phase) (prompt : Prompt) (state : State) (events : List Event)
  | finish (result : Result) (state : State) (events : List Event)
  | reject (events : List Event)

def Outcome.events : Outcome State Phase Event Result Prompt → List Event
  | .advance _ _ es | .suspend _ _ _ es | .finish _ _ es | .reject es => es

/-- Reject preserves the latest checkpoint, not necessarily the action's initial state. -/
def applyOutcome (before : Frame State Phase Event Result Prompt)
    (outcome : Outcome State Phase Event Result Prompt) : Frame State Phase Event Result Prompt :=
  let checkpoint := { before with sequence := before.sequence + 1, trace := before.trace ++ outcome.events }
  match outcome with
  | .advance next state _ => { checkpoint with state := state, phase := next, status := .running }
  | .suspend next prompt state _ =>
    { checkpoint with state := state, phase := next, status := .suspended prompt }
  | .finish result state _ => { checkpoint with state := state, status := .finished result }
  | .reject _ => { checkpoint with status := .rejected }

structure Program (State Context Input Phase Event Result Prompt : Type) where
  pipeline : Pipeline
  allowed : Phase → Phase → Prop
  rule : Phase → State → Context → Input → Outcome State Phase Event Result Prompt → Prop

def admissible (program : Program State Context Input Phase Event Result Prompt)
    (phase : Phase) : Outcome State Phase Event Result Prompt → Prop
  | .advance next _ _ | .suspend next _ _ _ => program.allowed phase next
  | .finish _ _ _ | .reject _ => True

/-- The caller grants only module-local state and a read-only context. -/
def Step (program : Program State Context Input Phase Event Result Prompt)
    (before : Frame State Phase Event Result Prompt) (ctx : Context) (input : Input)
    (outcome : Outcome State Phase Event Result Prompt)
    (after : Frame State Phase Event Result Prompt) : Prop :=
  before.status.isRunning = true ∧ before.pipeline = program.pipeline ∧
  program.rule before.phase before.state ctx input outcome ∧
  admissible program before.phase outcome ∧ after = applyOutcome before outcome

theorem step_preserves_pipeline (program : Program State Context Input Phase Event Result Prompt)
    (before after : Frame State Phase Event Result Prompt) (ctx : Context) (input : Input)
    (outcome : Outcome State Phase Event Result Prompt)
    (h : Step program before ctx input outcome after) : after.pipeline = before.pipeline := by
  rcases h with ⟨_, _, _, _, rfl⟩
  cases outcome <;> rfl

theorem step_preserves_owner (program : Program State Context Input Phase Event Result Prompt)
    (before after : Frame State Phase Event Result Prompt) (ctx : Context) (input : Input)
    (outcome : Outcome State Phase Event Result Prompt)
    (h : Step program before ctx input outcome after) : after.operation = before.operation := by
  rcases h with ⟨_, _, _, _, rfl⟩
  cases outcome <;> rfl

theorem step_appends_trace (program : Program State Context Input Phase Event Result Prompt)
    (before after : Frame State Phase Event Result Prompt) (ctx : Context) (input : Input)
    (outcome : Outcome State Phase Event Result Prompt)
    (h : Step program before ctx input outcome after) :
    after.trace = before.trace ++ outcome.events := by
  rcases h with ⟨_, _, _, _, rfl⟩
  cases outcome <;> rfl

theorem step_advances_sequence (program : Program State Context Input Phase Event Result Prompt)
    (before after : Frame State Phase Event Result Prompt) (ctx : Context) (input : Input)
    (outcome : Outcome State Phase Event Result Prompt)
    (h : Step program before ctx input outcome after) :
    after.sequence = before.sequence + 1 := by
  rcases h with ⟨_, _, _, _, rfl⟩
  cases outcome <;> rfl

/-- Connect the workflow to the existing capability-view model, without a world type. -/
def ScopedStep (scope : Scope Host State)
    (program : Program State Context Input Phase Event Result Prompt)
    (hostBefore hostAfter : Host) (before after : Frame State Phase Event Result Prompt)
    (ctx : Context) (input : Input) (outcome : Outcome State Phase Event Result Prompt) : Prop :=
  before.state = scope.read hostBefore ∧ Step program before ctx input outcome after ∧
  hostAfter = scope.write hostBefore after.state

theorem scoped_step_preserves_unrelated_state (scope : Scope Host State)
    (program : Program State Context Input Phase Event Result Prompt)
    (hostBefore hostAfter : Host) (before after : Frame State Phase Event Result Prompt)
    (ctx : Context) (input : Input) (outcome : Outcome State Phase Event Result Prompt)
    (h : ScopedStep scope program hostBefore hostAfter before after ctx input outcome) :
    scope.frame hostAfter = scope.frame hostBefore := by
  rcases h with ⟨_, _, rfl⟩
  exact scope.frame_write hostBefore after.state

theorem reject_preserves_checkpoint (before : Frame State Phase Event Result Prompt)
    (events : List Event) : (applyOutcome before (.reject events)).state = before.state := rfl

theorem two_steps_preserve_event_order
    (program : Program State Context Input Phase Event Result Prompt)
    (a b c : Frame State Phase Event Result Prompt) (ctx₁ ctx₂ : Context) (i₁ i₂ : Input)
    (o₁ o₂ : Outcome State Phase Event Result Prompt)
    (h₁ : Step program a ctx₁ i₁ o₁ b) (h₂ : Step program b ctx₂ i₂ o₂ c) :
    c.trace = (a.trace ++ o₁.events) ++ o₂.events := by
  rw [step_appends_trace program b c ctx₂ i₂ o₂ h₂,
    step_appends_trace program a b ctx₁ i₁ o₁ h₁]

theorem advance_follows_declared_edge
    (program : Program State Context Input Phase Event Result Prompt)
    (before after : Frame State Phase Event Result Prompt) (ctx : Context) (input : Input)
    (next : Phase) (state : State) (events : List Event)
    (h : Step program before ctx input (.advance next state events) after) :
    program.allowed before.phase next := h.2.2.2.1

theorem suspended_cannot_step
    (program : Program State Context Input Phase Event Result Prompt)
    (before after : Frame State Phase Event Result Prompt) (ctx : Context) (input : Input)
    (outcome : Outcome State Phase Event Result Prompt) (prompt : Prompt)
    (waiting : before.status = .suspended prompt) :
    ¬ Step program before ctx input outcome after := by
  intro h
  have active := h.1
  simp [waiting, Status.isRunning] at active

theorem finished_cannot_step
    (program : Program State Context Input Phase Event Result Prompt)
    (before after : Frame State Phase Event Result Prompt) (ctx : Context) (input : Input)
    (outcome : Outcome State Phase Event Result Prompt) (result : Result)
    (done : before.status = .finished result) :
    ¬ Step program before ctx input outcome after := by
  intro h
  have active := h.1
  simp [done, Status.isRunning] at active

structure Ticket (Phase : Type) where
  operation : OperationId
  sequence : Nat
  pipeline : Pipeline
  phase : Phase
  deriving DecidableEq

def ticketFor (frame : Frame State Phase Event Result Prompt) : Ticket Phase :=
  ⟨frame.operation, frame.sequence, frame.pipeline, frame.phase⟩

inductive ResumeError where
  | notSuspended | staleTicket | pipelineChanged
  deriving Repr, DecidableEq

/-- Resume uses the saved pipeline, not whatever extensions were installed later. -/
def resume [DecidableEq Phase] (before : Frame State Phase Event Result Prompt)
    (ticket : Ticket Phase) (candidate : Pipeline) :
    Except ResumeError (Frame State Phase Event Result Prompt) :=
  if !before.status.isSuspended then .error .notSuspended
  else if ticket ≠ ticketFor before then .error .staleTicket
  else if candidate ≠ before.pipeline then .error .pipelineChanged
  else .ok { before with status := .running, sequence := before.sequence + 1 }

theorem resume_checkpoint [DecidableEq Phase]
    (before after : Frame State Phase Event Result Prompt)
    (ticket : Ticket Phase) (candidate : Pipeline)
    (h : resume before ticket candidate = .ok after) :
    after.state = before.state ∧ after.phase = before.phase ∧
    after.trace = before.trace ∧ after.pipeline = before.pipeline ∧
    after.operation = before.operation ∧ after.sequence = before.sequence + 1 ∧
    after.status = .running := by
  unfold resume at h
  split at h
  · cases h
  · split at h
    · cases h
    · split at h
      · cases h
      · cases h
        exact ⟨rfl, rfl, rfl, rfl, rfl, rfl, rfl⟩

theorem resume_cannot_replay_immediately [DecidableEq Phase]
    (before after : Frame State Phase Event Result Prompt)
    (ticket : Ticket Phase) (candidate : Pipeline)
    (h : resume before ticket candidate = .ok after) :
    resume after ticket candidate = .error .notSuspended := by
  have checkpoint := resume_checkpoint before after ticket candidate h
  simp [resume, checkpoint.2.2.2.2.2.2, Status.isSuspended]

theorem wrong_ticket_rejected [DecidableEq Phase]
    (before : Frame State Phase Event Result Prompt) (ticket : Ticket Phase) (candidate : Pipeline)
    (waiting : before.status.isSuspended = true) (wrong : ticket ≠ ticketFor before) :
    resume before ticket candidate = .error .staleTicket := by
  simp [resume, waiting, wrong]

theorem old_sequence_rejected [DecidableEq Phase]
    (before : Frame State Phase Event Result Prompt) (ticket : Ticket Phase) (candidate : Pipeline)
    (waiting : before.status.isSuspended = true) (older : ticket.sequence < before.sequence) :
    resume before ticket candidate = .error .staleTicket := by
  apply wrong_ticket_rejected before ticket candidate waiting
  intro same
  have seq := congrArg Ticket.sequence same
  exact (Nat.ne_of_lt older) seq

theorem pipeline_replacement_rejected [DecidableEq Phase]
    (before : Frame State Phase Event Result Prompt) (candidate : Pipeline)
    (waiting : before.status.isSuspended = true) (different : candidate ≠ before.pipeline) :
    resume before (ticketFor before) candidate = .error .pipelineChanged := by
  simp [resume, waiting, different]

/-- A codec must establish this law; no concrete serializer is certified here. -/
structure CheckpointCodec (State Phase Event Result Prompt Saved : Type) where
  encode : Frame State Phase Event Result Prompt → Saved
  decode : Saved → Option (Frame State Phase Event Result Prompt)
  roundtrip : ∀ frame, decode (encode frame) = some frame

theorem restored_checkpoint_preserves_phase
    (codec : CheckpointCodec State Phase Event Result Prompt Saved)
    (frame : Frame State Phase Event Result Prompt) :
    (codec.decode (codec.encode frame)).map Frame.phase = some frame.phase := by
  rw [codec.roundtrip]
  rfl

theorem restored_checkpoint_preserves_resume [DecidableEq Phase]
    (codec : CheckpointCodec State Phase Event Result Prompt Saved)
    (frame : Frame State Phase Event Result Prompt) (ticket : Ticket Phase) (candidate : Pipeline) :
    (codec.decode (codec.encode frame)).map (fun saved => resume saved ticket candidate) =
      some (resume frame ticket candidate) := by
  rw [codec.roundtrip]
  rfl

namespace Examples

/- A narrow non-backfiring positive-charge/direction-cancel witness from the accepted C contract.
   Other charge signs, backfire, effects, blindness and naming are not modeled here. -/
inductive Phase where
  | charge | direction | retirement | spendTime
  deriving DecidableEq

inductive Input where
  | begin | cancel | continueAction

inductive Event where
  | charged | cancelled | timeSpent
  deriving DecidableEq

inductive Result where
  | time
  deriving DecidableEq

structure WandState where
  charges : Int
  chargeWrites : Nat
  timeCharges : Nat
  deriving DecidableEq

abbrev WandFrame := Frame WandState Phase Event Result Unit
abbrev WandOutcome := Outcome WandState Phase Event Result Unit

def pipeline : Pipeline :=
  { owner := { id := "example.wands", extensionPoints := ["use"] },
    point := "use", codeIdentity := "positive-direction-cancel-v1" }

def program : Program WandState Unit Input Phase Event Result Unit where
  pipeline := pipeline
  allowed := fun a b =>
    (a = .charge ∧ b = .direction) ∨ (a = .direction ∧ b = .retirement) ∨
    (a = .retirement ∧ b = .spendTime)
  rule := fun phase state _ input outcome =>
    match phase, input with
    | .charge, .begin => state.charges > 0 ∧ outcome = .suspend .direction ()
        { state with charges := state.charges - 1, chargeWrites := state.chargeWrites + 1 }
        [.charged]
    | .direction, .cancel => outcome = .advance .retirement state [.cancelled]
    | .retirement, .continueAction => outcome = .advance .spendTime state []
    | .spendTime, .continueAction =>
        outcome = .finish .time { state with timeCharges := state.timeCharges + 1 } [.timeSpent]
    | _, _ => False

def initial : WandFrame :=
  { operation := { host := 1, serial := 7 }, sequence := 0, pipeline := pipeline,
    state := { charges := 1, chargeWrites := 0, timeCharges := 0 },
    phase := .charge, trace := [], status := .running }

def charged : WandOutcome := .suspend .direction ()
  { charges := 0, chargeWrites := 1, timeCharges := 0 } [.charged]
def waiting : WandFrame := applyOutcome initial charged
def continued : WandFrame := { waiting with status := .running, sequence := 2 }
def cancelled : WandFrame := applyOutcome continued (.advance .retirement continued.state [.cancelled])
def retired : WandFrame := applyOutcome cancelled (.advance .spendTime cancelled.state [])
def completed : WandFrame := applyOutcome retired
  (.finish .time { retired.state with timeCharges := retired.state.timeCharges + 1 } [.timeSpent])

example : Step program initial () .begin charged waiting := by
  refine ⟨rfl, rfl, ?_, ?_, rfl⟩
  · exact ⟨by decide, rfl⟩
  · exact Or.inl ⟨rfl, rfl⟩

example : resume waiting (ticketFor waiting) pipeline = .ok continued := by rfl
example : Step program continued () .cancel
    (.advance .retirement continued.state [.cancelled]) cancelled :=
  ⟨rfl, rfl, rfl, Or.inr (Or.inl ⟨rfl, rfl⟩), rfl⟩
example : Step program cancelled () .continueAction
    (.advance .spendTime cancelled.state []) retired :=
  ⟨rfl, rfl, rfl, Or.inr (Or.inr ⟨rfl, rfl⟩), rfl⟩
example : Step program retired () .continueAction
    (.finish .time { retired.state with timeCharges := retired.state.timeCharges + 1 } [.timeSpent])
    completed := ⟨rfl, rfl, rfl, True.intro, rfl⟩

theorem direction_cancel_retains_charge_and_time :
    completed.state.charges = 0 ∧ completed.state.chargeWrites = 1 ∧
    completed.state.timeCharges = 1 ∧ completed.trace = [.charged, .cancelled, .timeSpent] ∧
    completed.status = .finished .time := by
  exact ⟨rfl, rfl, rfl, rfl, rfl⟩

example : resume continued (ticketFor waiting) pipeline = .error .notSuspended := by rfl
example : resume ({ continued with status := .suspended () } : WandFrame) (ticketFor waiting) pipeline =
    .error .staleTicket := by rfl
example : resume waiting
    (ticketFor { waiting with operation := { host := 1, serial := 8 } }) pipeline =
    .error .staleTicket := by rfl
example : resume waiting
    (ticketFor { waiting with operation := { host := 2, serial := 7 } }) pipeline =
    .error .staleTicket := by rfl
example : resume waiting (ticketFor waiting)
    { pipeline with owner := { pipeline.owner with version := 2 } } =
    .error .pipelineChanged := by rfl
example : resume waiting (ticketFor waiting)
    { pipeline with codeIdentity := "different-rule" } = .error .pipelineChanged := by rfl
example : resume waiting (ticketFor waiting)
    { pipeline with extensions := [{ id := "new-extension", target := "example.wands", point := "use" }] } =
    .error .pipelineChanged := by rfl
example : resume waiting (ticketFor waiting)
    { pipeline with providers := [{ id := "different-provider" }] } =
    .error .pipelineChanged := by rfl

end Examples
end NetMechanics.Architecture.Workflow
