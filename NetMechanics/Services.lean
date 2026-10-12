import NetMechanics.Authority
import NetMechanics.Workflow

/-!
# Optional kernel services: S4–S7

No service below is a mandatory game schema. A host selects its input policy,
time/residency policy, snapshot and wire types. Registry/caller/code identities,
owner validators, codec laws and atomic execution are explicit adapter duties.
This specifies protocols, not executable isolation, a concrete serializer or a
refinement of the Rust engine. Counters are unbounded; runtime identities must
not be reused and overflow must refuse before mutation.
-/

namespace NetMechanics.Services
open NetMechanics.Authority
open NetMechanics.Architecture.Workflow

/-- A bounded authority view; payloads, resources and pending plans are absent. -/
structure Registry where
  host : Nat
  callers : List Caller
  providers : List Provider
  grants : List Grant

def registryOf (s : NetMechanics.Authority.State Payload Event) : Registry :=
  ⟨s.host, s.bindings, s.providers, s.grants⟩

structure Binding where
  caller : Caller
  provider : Provider
  capability : NetMechanics.Authority.Capability
  access : Access
  deriving DecidableEq, BEq

def authorized (r : Registry) (b : Binding) : Bool :=
  b.caller.host == r.host && r.callers.contains b.caller &&
  r.providers.contains b.provider &&
  r.callers.contains ⟨r.host, b.provider.owner, b.provider.artifact⟩ &&
  b.provider.exports.contains b.capability &&
  r.grants.contains ⟨b.caller, b.provider, b.capability, b.access⟩

/-- Mutation services must match the selected provider/endpoint and a write grant. -/
def serves (provider : Provider) (capability : NetMechanics.Authority.Capability)
    (b : Binding) : Bool :=
  decide (b.provider = provider ∧ b.capability = capability ∧ b.access = .write)

namespace Input

/-- Operation namespace plus ordinal: the host allocates these, not the guest. -/
structure Key where
  operation : OperationId
  ordinal : Nat
  deriving DecidableEq

structure Record (Request Value : Type) where
  key : Key
  binding : Binding
  request : Request
  value : Value
  deriving DecidableEq

structure Ledger (Source Request Value : Type) where
  source : Source
  records : List (Record Request Value)
  deriving DecidableEq

/-- Provider policy may be a seeded stream, external input or a nondeterministic rule. -/
structure Policy (Source Request Value : Type) where
  provider : Provider
  capability : NetMechanics.Authority.Capability
  /-- The host binds issued request keys to authenticated workflow operations. -/
  belongs : Caller → Key → Bool
  operationOwned : Caller → Key → Prop
  belongsSound : ∀ caller key, belongs caller key = true → operationOwned caller key
  rule : Source → Request → Source → Value → Prop

def unseen (s : Ledger Source Request Value) (key : Key) : Prop :=
  ∀ rec ∈ s.records, rec.key ≠ key

/-- A fresh accepted input commits its provider state and recorded choice together. -/
def Accept (policy : Policy Source Request Value) (r : Registry) (b : Binding)
    (key : Key) (request : Request) (before after : Ledger Source Request Value)
    (nextSource : Source) (value : Value) : Prop :=
  authorized r b = true ∧ serves policy.provider policy.capability b = true ∧
  key.operation.host = r.host ∧ policy.belongs b.caller key = true ∧ unseen before key ∧
  policy.rule before.source request nextSource value ∧
  after = ⟨nextSource, before.records ++ [⟨key, b, request, value⟩]⟩

theorem accepted_prefix (h : Accept policy r b key request before after nextSource value) :
    after.records = before.records ++ [⟨key, b, request, value⟩] := by
  rcases h with ⟨_, _, _, _, _, _, rfl⟩
  rfl

theorem accepted_authority (h : Accept policy r b key request before after nextSource value) :
    authorized r b = true ∧ key.operation.host = r.host := ⟨h.1, h.2.2.1⟩

theorem accepted_endpoint (h : Accept policy r b key request before after nextSource value) :
    b.provider = policy.provider ∧ b.capability = policy.capability ∧ b.access = .write := by
  simpa only [serves, decide_eq_true_eq] using h.2.1

theorem accepted_operation_owned
    (h : Accept policy r b key request before after nextSource value) :
    policy.operationOwned b.caller key := policy.belongsSound _ _ h.2.2.2.1

/-- Replay resolves the recorded choice, checks exact request/binding and never samples. -/
def replay [DecidableEq Request] (policy : Policy Source Request Value)
    (r : Registry) (b : Binding) (key : Key)
    (request : Request) (s : Ledger Source Request Value) :
    Ledger Source Request Value × Option Value :=
  if authorized r b && serves policy.provider policy.capability b &&
      decide (key.operation.host = r.host) && policy.belongs b.caller key then
    match s.records.find? (fun rec => decide (rec.key = key)) with
    | some rec =>
      if rec.binding = b ∧ rec.request = request then (s, some rec.value) else (s, none)
    | none => (s, none)
  else (s, none)

theorem replay_preserves_ledger [DecidableEq Request]
    (policy : Policy Source Request Value) (s : Ledger Source Request Value) (request : Request) :
    (replay policy r b key request s).1 = s := by
  unfold replay
  split
  · split
    · split <;> rfl
    · rfl
  · rfl

theorem accepted_key_cannot_resample
    (h : Accept policy r b key request before after nextSource value) : ¬ unseen after key := by
  intro fresh
  unfold unseen at fresh
  rw [accepted_prefix h] at fresh
  exact fresh ⟨key, b, request, value⟩ (by simp) rfl

end Input

namespace Timer

structure Key where
  host : Nat
  serial : Nat
  deriving DecidableEq

/-- Time and residency are profile data; no universal world-turn interpretation. -/
structure Task (Time Residency Callback : Type) where
  key : Key
  binding : Binding
  pipeline : Pipeline
  targets : List Stamp
  deadline : Time
  residency : Residency
  callback : Callback
  deriving DecidableEq

structure Queue (Time Residency Callback : Type) where
  tasks : List (Task Time Residency Callback)
  delivered : List Key
  deriving DecidableEq

structure Policy (Time Residency Callback : Type) where
  provider : Provider
  capability : NetMechanics.Authority.Capability
  /-- Bind the stored caller and opaque pipeline to retained executable code. -/
  bound : Binding → Pipeline → Bool
  authenticPipeline : Binding → Pipeline → Prop
  boundSound : ∀ b pipeline, bound b pipeline = true → authenticPipeline b pipeline
  /-- Includes due/residency/order selection; its semantics belong to the profile. -/
  ready : List (Task Time Residency Callback) → Task Time Residency Callback → Bool
  eligible : List (Task Time Residency Callback) → Task Time Residency Callback → Prop
  readySound : ∀ queue task, ready queue task = true → eligible queue task

def unique (q : Queue Time Residency Callback) : Prop :=
  (q.tasks.map Task.key).Nodup

/-- The scheduler supplies an authoritative current resource-stamp check. -/
def deliver (policy : Policy Time Residency Callback) (registry : Registry)
    (currentStamp : Stamp → Bool) (q : Queue Time Residency Callback) (key : Key)
    (candidate : Pipeline) : Queue Time Residency Callback × Option (Task Time Residency Callback) :=
  match q.tasks.find? (fun task => decide (task.key = key)) with
  | none => (q, none)
  | some task =>
    if authorized registry task.binding && serves policy.provider policy.capability task.binding &&
        decide (key.host = registry.host) && policy.bound task.binding task.pipeline &&
        decide (task.pipeline = candidate) && task.targets.all currentStamp &&
        policy.ready q.tasks task && !q.delivered.contains key then
      (⟨q.tasks.filter (fun other => decide (other.key ≠ key)), q.delivered ++ [key]⟩,
        some task)
    else (q, none)

theorem refusal_preserves_queue (h : (deliver policy r currentStamp q key candidate).2 = none) :
    (deliver policy r currentStamp q key candidate).1 = q := by
  unfold deliver at h ⊢
  split
  · rfl
  · split <;> simp_all only [ite_true, ite_false, reduceCtorEq]

theorem replayed_key_refused (h : q.delivered.contains key = true) :
    deliver policy r currentStamp q key candidate = (q, none) := by
  simp only [List.contains_iff_mem] at h
  unfold deliver
  split
  · rfl
  · simp [h]

theorem delivered_marked (h : (deliver policy r currentStamp q key candidate).2 = some task) :
    (deliver policy r currentStamp q key candidate).1.delivered.contains key = true := by
  unfold deliver at h ⊢
  split
  · simp_all
  · split <;> simp_all only [ite_true, ite_false, Option.some.injEq,
      reduceCtorEq, List.contains_iff_mem, List.mem_append, List.mem_singleton, or_true]

theorem delivered_once (h : (deliver policy r currentStamp q key candidate).2 = some task) :
    deliver policy r currentStamp (deliver policy r currentStamp q key candidate).1 key candidate =
      ((deliver policy r currentStamp q key candidate).1, none) := by
  exact replayed_key_refused (delivered_marked h)

end Timer

namespace Lifecycle

/-- A pipeline pins actual artifacts separately from descriptive manifests. -/
structure BoundPipeline where
  program : Pipeline
  artifacts : List Artifact
  deriving DecidableEq

structure State (Host : Type) where
  host : Host
  loaded : List Artifact
  dispatch : Nat → Option BoundPipeline
  pending : List BoundPipeline
  timers : List BoundPipeline
  ownedReferences : List Artifact
  /-- (dependent executable, required executable), including capability providers. -/
  dependencies : List (Artifact × Artifact)
  /-- Host-maintained: no phase/callback is currently executing; suspended work may exist. -/
  atBoundary : Bool

def liveReferences (s : State Host) (artifact : Artifact) : Prop :=
  artifact ∈ s.ownedReferences ∨
  (∃ point binding, s.dispatch point = some binding ∧ artifact ∈ binding.artifacts) ∨
  (∃ binding ∈ s.pending ++ s.timers, artifact ∈ binding.artifacts) ∨
  (∃ dependent, (dependent, artifact) ∈ s.dependencies ∧ dependent ∈ s.loaded)

/-- Loader validation is a trusted check for manifests, grants, ABI and initialization. -/
def attach (s : State Host) (point : Nat) (binding : BoundPipeline)
    (newArtifacts : List Artifact) (newDependencies : List (Artifact × Artifact))
    (loaderValidated : Bool) : State Host × Bool :=
  if s.atBoundary && loaderValidated && decide newArtifacts.Nodup &&
      newArtifacts.all (fun a => !s.loaded.contains a) &&
      binding.artifacts.all (fun a => (s.loaded ++ newArtifacts).contains a) &&
      newDependencies.all (fun edge => (s.loaded ++ newArtifacts).contains edge.1 &&
        (s.loaded ++ newArtifacts).contains edge.2) then
    ({ s with loaded := s.loaded ++ newArtifacts
              dispatch := fun p => if p = point then some binding else s.dispatch p
              dependencies := s.dependencies ++ newDependencies }, true)
  else (s, false)

theorem attachment_preserves_existing (s : State Host) :
    (attach s point binding newArtifacts newDependencies validated).1.host = s.host ∧
    (attach s point binding newArtifacts newDependencies validated).1.pending = s.pending ∧
    (attach s point binding newArtifacts newDependencies validated).1.timers = s.timers ∧
    (attach s point binding newArtifacts newDependencies validated).1.ownedReferences =
      s.ownedReferences := by
  unfold attach
  split <;> simp

theorem attachment_refusal
    (h : (attach s point binding newArtifacts newDependencies validated).2 = false) :
    (attach s point binding newArtifacts newDependencies validated).1 = s := by
  unfold attach at h ⊢
  split <;> simp_all only [ite_true, ite_false, Bool.true_eq_false]

theorem busy_attachment_refused (h : s.atBoundary = false) :
    attach s point binding newArtifacts newDependencies validated = (s, false) := by
  simp [attach, h]

/-- Decidable live-reference evidence must come from authoritative host state. -/
def unload (s : State Host) (artifact : Artifact) [Decidable (liveReferences s artifact)] :
    State Host × Bool :=
  if liveReferences s artifact then (s, false)
  else if !s.atBoundary then (s, false)
  else if s.loaded.contains artifact then
    ({ s with loaded := s.loaded.filter (fun a => a != artifact)
              dependencies := s.dependencies.filter (fun edge => edge.1 != artifact) }, true)
  else (s, false)

theorem live_unload_refused (h : liveReferences s artifact)
    [Decidable (liveReferences s artifact)] : unload s artifact = (s, false) := by
  simp [unload, h]

theorem pending_pin_blocks_unload (h : binding ∈ s.pending)
    (ha : artifact ∈ binding.artifacts) [Decidable (liveReferences s artifact)] :
    unload s artifact = (s, false) := by
  apply live_unload_refused
  exact Or.inr (Or.inr (Or.inl ⟨binding, List.mem_append_left _ h, ha⟩))

theorem dependent_blocks_unload (h : (dependent, artifact) ∈ s.dependencies)
    (hd : dependent ∈ s.loaded) [Decidable (liveReferences s artifact)] :
    unload s artifact = (s, false) := by
  apply live_unload_refused
  exact Or.inr (Or.inr (Or.inr ⟨dependent, h, hd⟩))

end Lifecycle

namespace Persistence
variable {Snapshot Wire : Type}

structure Envelope (Snapshot : Type) where
  schema : Nat
  snapshot : Snapshot

/-- Assumed codec law, not a verified concrete binary/JSON serializer. -/
structure Codec (Snapshot Wire : Type) where
  encode : Envelope Snapshot → Wire
  decode : Wire → Option (Envelope Snapshot)
  roundtrip : ∀ envelope, decode (encode envelope) = some envelope

structure Policy (Snapshot : Type) where
  schema : Nat
  /-- Validate decoded old ownership/identities BEFORE migration or callbacks. -/
  beforeMigration : Envelope Snapshot → Bool
  beforeValid : Envelope Snapshot → Prop
  beforeSound : ∀ envelope, beforeMigration envelope = true → beforeValid envelope
  migrate : Envelope Snapshot → Option Snapshot
  /-- Validate current schemas, grants, pins, timers, provider/input state and namespaces. -/
  afterMigration : Snapshot → Bool
  afterValid : Snapshot → Prop
  afterSound : ∀ snapshot, afterMigration snapshot = true → afterValid snapshot

/-- Pure admission; the runtime publishes the admitted snapshot before any callbacks. -/
def restore (codec : Codec Snapshot Wire) (policy : Policy Snapshot) (current : Snapshot)
    (wire : Wire) : Snapshot × Bool :=
  match codec.decode wire with
  | none => (current, false)
  | some envelope =>
    if policy.beforeMigration envelope then
      let next := if envelope.schema = policy.schema then some envelope.snapshot
        else policy.migrate envelope
      match next with
      | none => (current, false)
      | some snapshot =>
        if policy.afterMigration snapshot then (snapshot, true) else (current, false)
    else (current, false)

theorem restore_refusal (codec : Codec Snapshot Wire) (policy : Policy Snapshot)
    (current : Snapshot) (wire : Wire)
    (h : (restore codec policy current wire).2 = false) :
    (restore codec policy current wire).1 = current := by
  unfold restore at h ⊢
  repeat' (first | split | simp_all)

theorem current_schema_roundtrip (codec : Codec Snapshot Wire) (policy : Policy Snapshot)
    (current snapshot : Snapshot)
    (hb : policy.beforeMigration ⟨policy.schema, snapshot⟩ = true)
    (ha : policy.afterMigration snapshot = true) :
    restore codec policy current (codec.encode ⟨policy.schema, snapshot⟩) = (snapshot, true) := by
  simp [restore, codec.roundtrip, hb, ha]

end Persistence

namespace Witness

def consumer : Caller := ⟨7, 5, ⟨"device", 1, 11⟩⟩
def entropy : Provider := ⟨2, ⟨"entropy", 1, 21⟩, [⟨"sample", 1⟩]⟩
def clock : Provider := ⟨3, ⟨"clock", 1, 31⟩, [⟨"callback", 1⟩]⟩
def inputBinding : Binding := ⟨consumer, entropy, ⟨"sample", 1⟩, .write⟩
def timerBinding : Binding := ⟨consumer, clock, ⟨"callback", 1⟩, .write⟩
def registry : Registry :=
  ⟨7, [consumer, ⟨7, 2, entropy.artifact⟩, ⟨7, 3, clock.artifact⟩], [entropy, clock],
    [⟨consumer, entropy, inputBinding.capability, .write⟩,
     ⟨consumer, clock, timerBinding.capability, .write⟩]⟩

def inputKey : Input.Key := ⟨⟨7, 100⟩, 0⟩
def inputPolicy : Input.Policy Nat Unit Nat where
  provider := entropy
  capability := inputBinding.capability
  belongs := fun caller key => decide (caller = consumer ∧ key = inputKey)
  operationOwned := fun caller key => caller = consumer ∧ key = inputKey
  belongsSound := by intro caller key h; simpa using h
  rule := fun before _ after value => after = before + 1 ∧ (value = 0 ∨ value = 1)
def inputStart : Input.Ledger Nat Unit Nat := ⟨0, []⟩
def sampled (value : Nat) : Input.Ledger Nat Unit Nat :=
  ⟨1, [⟨inputKey, inputBinding, (), value⟩]⟩

theorem sample_accepted (value : Nat) (hv : value = 0 ∨ value = 1) :
    Input.Accept inputPolicy registry inputBinding inputKey () inputStart (sampled value) 1 value := by
  refine ⟨by decide, by decide, rfl, by decide, ?_, ⟨rfl, hv⟩, rfl⟩
  simp [Input.unseen, inputStart]

theorem nonfunctional_input_witness :
    Input.Accept inputPolicy registry inputBinding inputKey () inputStart (sampled 0) 1 0 ∧
    Input.Accept inputPolicy registry inputBinding inputKey () inputStart (sampled 1) 1 1 ∧
    (0 : Nat) ≠ 1 := ⟨sample_accepted 0 (Or.inl rfl), sample_accepted 1 (Or.inr rfl), by decide⟩

theorem replay_preserves_choice_witness :
    Input.replay inputPolicy registry inputBinding inputKey () (sampled 0) =
      (sampled 0, some 0) := by decide

theorem other_endpoint_refused_witness :
    authorized registry timerBinding = true ∧
    Input.replay inputPolicy registry timerBinding inputKey () (sampled 0) =
      (sampled 0, none) := by decide

def oldPipeline : Lifecycle.BoundPipeline := {
  program := { owner := { id := "device" }
               point := "activate"
               codeIdentity := "code-11" }
  artifacts := [consumer.artifact] }
def newArtifact : Artifact := ⟨"device", 2, 12⟩
def newPipeline : Lifecycle.BoundPipeline := {
  program := { owner := { id := "device", version := 2 }
               point := "activate"
               codeIdentity := "code-12" }
  artifacts := [newArtifact] }
def existing : Lifecycle.State Nat :=
  ⟨41, [consumer.artifact], fun _ => some oldPipeline, [oldPipeline], [oldPipeline], [], [], true⟩

theorem attachment_pins_pending_witness :
    let next := (Lifecycle.attach existing 0 newPipeline [newArtifact] [] true).1
    next.host = 41 ∧ next.dispatch 0 = some newPipeline ∧
      next.pending = [oldPipeline] ∧ next.timers = [oldPipeline] := by decide

theorem missing_dependency_refused_witness :
    Lifecycle.attach existing 0 newPipeline [newArtifact]
      [(newArtifact, ⟨"missing", 1, 99⟩)] true = (existing, false) := by rfl

def timerTask : Timer.Task Nat Bool Unit :=
  ⟨⟨7, 10⟩, timerBinding, oldPipeline.program,
    [⟨⟨⟨5, 0⟩, 9⟩, 2⟩], 10, true, ()⟩
def timerQueue : Timer.Queue Nat Bool Unit := ⟨[timerTask], []⟩
def timerPolicy : Timer.Policy Nat Bool Unit where
  provider := clock
  capability := timerBinding.capability
  bound := fun b p => decide (b.caller.artifact = consumer.artifact ∧ p = oldPipeline.program)
  authenticPipeline := fun b p => b.caller.artifact = consumer.artifact ∧ p = oldPipeline.program
  boundSound := by intro b p h; simpa using h
  ready := fun _ task => decide (task.deadline ≤ 10 ∧ task.residency = true)
  eligible := fun _ task => task.deadline ≤ 10 ∧ task.residency = true
  readySound := by intro q task h; simpa using h

theorem stale_timer_target_refused_witness :
    Timer.deliver timerPolicy registry (fun _ => false) timerQueue timerTask.key
      oldPipeline.program = (timerQueue, none) := by decide

theorem replaced_timer_pipeline_refused_witness :
    Timer.deliver timerPolicy registry (fun _ => true) timerQueue timerTask.key
      newPipeline.program = (timerQueue, none) := by decide

/-- An identity wire for a protocol witness, not a production persistence format. -/
def codec : Persistence.Codec Nat (Persistence.Envelope Nat) := ⟨id, some, fun _ => rfl⟩
def restorePolicy : Persistence.Policy Nat where
  schema := 2
  beforeMigration := fun e => decide (e.snapshot > 0)
  beforeValid := fun e => e.snapshot > 0
  beforeSound := by intro e h; simpa using h
  migrate := fun e => if e.schema = 1 then some (e.snapshot + 1) else none
  afterMigration := fun value => decide (value > 0)
  afterValid := fun value => value > 0
  afterSound := by intro value h; simpa using h

theorem old_schema_migration_witness :
    Persistence.restore codec restorePolicy 41 ⟨1, 5⟩ = (6, true) := by decide

theorem unsupported_migration_refused_witness :
    Persistence.restore codec restorePolicy 41 ⟨3, 5⟩ = (41, false) := by decide

end Witness
end NetMechanics.Services
