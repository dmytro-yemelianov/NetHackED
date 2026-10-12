import NetMechanics.Services

/-!
M4-O1: independently typed owner regions, pure domain preparation, and gated
small-step publication. `observeUnion` is a mathematical view, never a registry.
All memory regions below are abstract names; no Rust alias/allocation/drop/panic,
Wasm isolation, authentic invocation, bounded-counter, or crash refinement follows.
Lean revisions/generations/serials are unbounded Nat. O2–O4 must refuse exhaustion
before any counter, slot, event or pending-state mutation, including after restore.
The publication primitives are explicit functional slot replacement, history append,
and ticket removal, with no owner callback. Runtime correspondence is O2–O4.
-/
namespace NetMechanics.Ownership
open NetMechanics.Architecture

abbrev OwnerId := Nat
variable {Payload : OwnerId → Type} {Event : Type}

structure BehaviorBinding where
  module : Authority.Artifact
  extensions : List Authority.Artifact
  schema : Nat
  abi : Nat
  policy : Nat
  inputRecord : Nat
  deriving DecidableEq, BEq

def BehaviorBinding.chain (b : BehaviorBinding) := b.module :: b.extensions

structure OwnerLocal (Payload : OwnerId → Type) (owner : OwnerId) where
  generation : Nat
  revision : Nat
  schema : Nat
  value : Payload owner

abbrev Owners (Payload : OwnerId → Type) := (i : OwnerId) → OwnerLocal Payload i

/-- Read-only denotation of independent slots; never passed to a domain command. -/
def observeUnion (s : Owners Payload) : (i : OwnerId) → Payload i := fun i => (s i).value

/-- No alias is represented: each slot, registry and coordinator has a distinct region.
A concrete implementation must establish this layout correspondence, not assume it. -/
structure RegionLayout where
  slot : OwnerId → Nat
  registry : Nat
  coordinator : Nat
  independent : Function.Injective slot
  registrySeparate : ∀ i, slot i ≠ registry
  coordinatorSeparate : ∀ i, slot i ≠ coordinator
  metadataSeparate : registry ≠ coordinator

def separateRegions : RegionLayout where
  slot := fun i => i + 2
  registry := 0
  coordinator := 1
  independent := by intro a b h; exact Nat.add_right_cancel h
  registrySeparate := by intro i; omega
  coordinatorSeparate := by intro i; omega
  metadataSeparate := by decide

theorem regions_disjoint (r : RegionLayout) (h : i ≠ j) : r.slot i ≠ r.slot j :=
  fun eq => h (r.independent eq)

structure Route where
  owner : OwnerId
  generation : Nat
  schema : Nat
  future : BehaviorBinding
  gate : Nat
  sealedBackend : Bool
  deriving DecidableEq

structure Invocation where
  caller : Authority.Caller
  operation : Workflow.OperationId
  phase : Nat
  lease : Nat
  deriving DecidableEq, BEq

/-- Authoritative metadata only. No Payload parameter, slot, observation or state pointer. -/
structure Registry where
  host : Nat
  gate : Nat
  callers : List Authority.Caller
  providers : List Authority.Provider
  grants : List Authority.Grant
  invocations : List Invocation
  routes : List Route
  available : List Authority.Artifact
  pins : List Authority.Artifact

/-- Reuses Authority authentication/permissions without its resource database. -/
def authorityView (r : Registry) : Authority.State Unit Unit :=
  ⟨r.host, r.callers, r.providers, r.grants, fun _ => none, fun _ => none, 0, [], 0⟩

def route (r : Registry) (i : OwnerId) : Option Route :=
  r.routes.find? fun p => p.owner == i

structure Capture where
  owner : OwnerId
  generation : Nat
  revision : Nat
  schema : Nat
  binding : BehaviorBinding
  capability : Authority.Capability
  access : Authority.Access
  deriving DecidableEq

def Capture.dependency (c : Capture) : Authority.Dependency :=
  ⟨⟨⟨⟨c.owner, 0⟩, c.generation⟩, c.revision⟩,
    ⟨c.owner, c.binding.module, [c.capability]⟩, c.capability⟩

def capture (r : Registry) (s : Owners Payload) (i : OwnerId)
    (cap : Authority.Capability) (access : Authority.Access) : Option Capture := do
  let p ← route r i
  return ⟨i, (s i).generation, (s i).revision, (s i).schema, p.future, cap, access⟩

theorem capture_owner (h : capture r s i cap access = some c) : c.owner = i := by
  unfold capture at h
  cases hr : route r i <;> simp [hr] at h
  subst c
  rfl

/-- Future dispatch deliberately absent: a retained old chain remains admissible. -/
def current (r : Registry) (s : Owners Payload) (c : Capture) : Bool :=
  match route r c.owner with
  | none => false
  | some p => decide (p.generation = c.generation ∧ p.schema = c.schema ∧
      p.gate = r.gate ∧ p.sealedBackend = true ∧
      (s c.owner).generation = c.generation ∧ (s c.owner).revision = c.revision ∧
      (s c.owner).schema = c.schema ∧ c.binding.schema = c.schema) &&
      c.binding.chain.all (fun a => r.available.contains a)

def authorized (r : Registry) (inv : Invocation) (c : Capture) : Bool :=
  Authority.authority (authorityView r) inv.caller &&
  Authority.permitted (authorityView r) inv.caller c.dependency c.access

structure Observation (Payload : OwnerId → Type) where
  owner : OwnerId
  stamp : Capture
  belongs : stamp.owner = owner
  value : Payload owner

structure ReadRequest where
  owner : OwnerId
  capability : Authority.Capability

/-- Detached observations; no domain is given a locator for other live slots. -/
def readDeclared (r : Registry) (s : Owners Payload) (requests : List ReadRequest) :
    Option (List (Observation Payload)) :=
  requests.mapM fun request =>
    match hc : capture r s request.owner request.capability .read with
    | none => none
    | some c => some ⟨request.owner, c, capture_owner hc, (s request.owner).value⟩

/-- Domain command semantics and a certified pure proposal producer. Command carries
requests, never replacement payloads. Validation is against only own state + declared
observations and the exact executable binding. Implementations must realize this
pure interface; a proof-producing Lean function does not certify native purity. -/
structure Domain (Payload : OwnerId → Type) (Event : Type) where
  invariant : (i : OwnerId) → Payload i → Prop
  Command : OwnerId → Type
  capability : (i : OwnerId) → Command i → Authority.Capability
  legal : (i : OwnerId) → BehaviorBinding → Payload i → List (Observation Payload) →
    Command i → Payload i → List Event → Prop
  propose : (i : OwnerId) → (b : BehaviorBinding) → (old : Payload i) →
    (view : List (Observation Payload)) → (cmd : Command i) →
    Option { result : Payload i × List Event //
      legal i b old view cmd result.1 result.2 ∧ invariant i result.1 }

variable {d : Domain Payload Event} {view : List (Observation Payload)}

structure Call (d : Domain Payload Event) where
  owner : OwnerId
  command : d.Command owner

/-- Sealed evidence, not a public setter. Only preparation constructs it. -/
structure Candidate (d : Domain Payload Event) (view : List (Observation Payload)) where
  private mk ::
  owner : OwnerId
  stamp : Capture
  belongs : stamp.owner = owner
  old : OwnerLocal Payload owner
  command : d.Command owner
  value : Payload owner
  events : List Event
  commandValid : d.legal owner stamp.binding old.value view command value events
  valueValid : d.invariant owner value

structure ParticipantSet where
  writes : List OwnerId
  readonly : List OwnerId
  deriving DecidableEq

def ParticipantSet.valid (p : ParticipantSet) : Prop :=
  (p.writes ++ p.readonly).Nodup ∧ p.writes.Pairwise (· < ·)

instance (p : ParticipantSet) : Decidable p.valid := by unfold ParticipantSet.valid; infer_instance

structure Coordination (d : Domain Payload Event) where
  maxParticipants : Nat
  check : (view : List (Observation Payload)) → List (Candidate d view) → Bool
  consistent : (view : List (Observation Payload)) → List (Candidate d view) → Prop
  sound : ∀ view writes, check view writes = true → consistent view writes
  /-- Profile/module event order is independent of canonical slot acquisition order. -/
  order : (view : List (Observation Payload)) → List (Candidate d view) → List (Candidate d view)
  orderExact : ∀ view writes, (order view writes).Perm writes

structure PreparedToken (d : Domain Payload Event) where
  private mk ::
  identity : Authority.Identity
  invocation : Invocation
  observations : List (Observation Payload)
  writes : List (Candidate d observations)
  ordered : List (Candidate d observations)
  orderExact : ordered.Perm writes
  participants : ParticipantSet
  exactWrites : writes.map Candidate.owner = participants.writes
  exactReads : observations.map Observation.owner = participants.readonly
  shape : participants.valid
  /-- Payloads in this object are temporary candidates/retention only, never registry state. -/
  crossChecked : Bool

/-- Coordinator cannot hold an authoritative payload map; only temporary sealed work. -/
structure Coordinator (d : Domain Payload Event) where
  gate : Nat
  staged : Option (PreparedToken d)

structure Host (d : Domain Payload Event) where
  registry : Registry
  owners : Owners Payload
  history : List Event
  pending : Authority.Identity → Option (PreparedToken d)
  nextSerial : Nat
  epoch : Nat
  visible : Bool

def fresh (s : Host d) : Authority.Identity := ⟨s.registry.host, s.nextSerial⟩

def stageWrites (d : Domain Payload Event) (r : Registry) (s : Owners Payload)
    (view : List (Observation Payload)) (calls : List (Call d)) :
    Option (List (Candidate d view)) :=
  calls.mapM fun call =>
    match hc : capture r s call.owner (d.capability call.owner call.command) .write with
    | none => none
    | some c => do
      let result ← d.propose call.owner c.binding (s call.owner).value view call.command
      return ⟨call.owner, c, capture_owner hc, s call.owner, call.command,
        result.val.1, result.val.2, result.property.1, result.property.2⟩

variable [∀ i, DecidableEq (Payload i)]

instance (i : OwnerId) : DecidableEq (OwnerLocal Payload i) := fun a b =>
  if h : a.generation = b.generation ∧ a.revision = b.revision ∧
      a.schema = b.schema ∧ a.value = b.value then
    isTrue (by cases a; cases b; simp_all)
  else isFalse (by intro eq; apply h; cases eq; exact ⟨rfl, rfl, rfl, rfl⟩)

/-- Exact values are checked as well as stamps. This avoids assuming that arbitrary
functions differing at equal revisions are legal owner evolutions. Runtime stamped
query fidelity/version discipline must establish the equivalent check. -/
def observationCurrent (r : Registry) (s : Owners Payload) (o : Observation Payload) : Bool :=
  current r s o.stamp && decide ((s o.owner).value = o.value)

def candidateCurrent (r : Registry) (s : Owners Payload)
    (c : Candidate d view) : Bool :=
  current r s c.stamp && decide (s c.owner = c.old)

/-- All fallible checks are here, before the first sealed swap. -/
def tokenValid (policy : Coordination d) (s : Host d) (inv : Invocation)
    (t : PreparedToken d) : Bool :=
  s.visible && decide (inv = t.invocation ∧ t.identity.host = s.registry.host ∧
    inv.operation.host = s.registry.host) && s.registry.invocations.contains inv &&
  decide ((t.participants.writes ++ t.participants.readonly).length ≤ policy.maxParticipants) &&
  t.observations.all (fun o => observationCurrent s.registry s.owners o &&
    authorized s.registry inv o.stamp) &&
  t.writes.all (fun c => candidateCurrent s.registry s.owners c &&
    authorized s.registry inv c.stamp) && policy.check t.observations t.writes

def tokenArtifacts (t : PreparedToken d) : List Authority.Artifact :=
  t.observations.flatMap (fun o => o.stamp.binding.chain) ++
    t.writes.flatMap (fun c => c.stamp.binding.chain)

/-- Commands build their proposals. Callers declare participants, not candidate bytes. -/
def prepare (policy : Coordination d) (s : Host d) (inv : Invocation)
    (p : ParticipantSet) (reads : List ReadRequest) (calls : List (Call d)) :
    Host d × Option Authority.Identity :=
  match readDeclared s.registry s.owners reads with
  | none => (s, none)
  | some view =>
    match stageWrites d s.registry s.owners view calls with
    | none => (s, none)
    | some writes =>
      if shape : writes.map Candidate.owner = p.writes ∧
          view.map Observation.owner = p.readonly ∧ p.valid then
        let t : PreparedToken d := ⟨fresh s, inv, view, writes, policy.order view writes,
          policy.orderExact view writes, p,
          shape.1, shape.2.1, shape.2.2, policy.check view writes⟩
        if tokenValid policy s inv t && (s.pending (fresh s)).isNone then
          ({ s with pending := fun id => if id = fresh s then some t else s.pending id
                    nextSerial := s.nextSerial + 1
                    registry := { s.registry with pins := s.registry.pins ++ tokenArtifacts t } },
            some (fresh s))
        else (s, none)
      else (s, none)

/-- The sole payload-writing primitive. It writes one independently typed region. -/
def swap (s : Owners Payload) (c : Candidate d view) : Owners Payload :=
  fun i => if h : i = c.owner then
    h.symm ▸ (⟨c.old.generation, c.old.revision + 1, c.old.schema, c.value⟩ :
      OwnerLocal Payload c.owner)
    else s i

def swapAll (s : Owners Payload) (writes : List (Candidate d view)) : Owners Payload :=
  writes.foldl swap s

def events (t : PreparedToken d) : List Event := t.ordered.flatMap Candidate.events

def wholeCommit (s : Host d) (t : PreparedToken d) : Host d :=
  { s with owners := swapAll s.owners t.writes
           history := s.history ++ events t
           pending := fun id => if id = t.identity then none else s.pending id
           epoch := s.epoch + 1
           visible := true }

/-- Indexed ghost description of progress. This is not stored authoritative payload.
`done` retains old slots/candidates; all publication steps have closed visibility. -/
inductive PublicationPhase (t : PreparedToken d) where
  | swapping (done todo : List (Candidate d t.observations))
      (partition : done ++ todo = t.writes)
  | events | consume | release | opened

def materialize (before : Host d) (t : PreparedToken d) : PublicationPhase t → Host d
  | .swapping done _ _ => { before with owners := swapAll before.owners done, visible := false }
  | .events => { before with owners := swapAll before.owners t.writes, visible := false }
  | .consume =>
    { before with
      owners := swapAll before.owners t.writes
      history := before.history ++ events t
      visible := false }
  | .release => { wholeCommit before t with visible := false }
  | .opened => wholeCommit before t

inductive PublicationStep (t : PreparedToken d) : PublicationPhase t → PublicationPhase t → Prop
  | swap (done : List (Candidate d t.observations)) (c : Candidate d t.observations)
      (todo : List (Candidate d t.observations)) (h : done ++ c :: todo = t.writes) :
      PublicationStep t (.swapping done (c :: todo) h)
        (.swapping (done ++ [c]) todo (by simpa [List.append_assoc] using h))
  | slotsDone (done : List (Candidate d t.observations)) (h : done ++ [] = t.writes) :
      PublicationStep t (.swapping done [] h) .events
  | appendEvents : PublicationStep t .events .consume
  | consumeTicket : PublicationStep t .consume .release
  | openGate : PublicationStep t .release .opened

inductive PublicationSteps (t : PreparedToken d) : PublicationPhase t → PublicationPhase t → Prop
  | refl (p) : PublicationSteps t p p
  | next {a b c} : PublicationStep t a b → PublicationSteps t b c → PublicationSteps t a c

/-- Only a stored issued identity can enter publication; no replacement plan argument. -/
def beginCommit (policy : Coordination d) (s : Host d) (inv : Invocation) (id : Authority.Identity) :
    Host d × Option (PreparedToken d) :=
  match s.pending id with
  | none => (s, none)
  | some t =>
    if tokenValid policy s inv t && decide (id = t.identity) then
      ({ s with visible := false }, some t)
    else (s, none)

def externalObserve (s : Host d) : Option ((i : OwnerId) → Payload i) :=
  if s.visible then some (observeUnion s.owners) else none

section Laws
variable {d : Domain Payload Event} {policy : Coordination d} {s : Host d}
  {t : PreparedToken d} {inv : Invocation} {id : Authority.Identity}

theorem prepare_preserves_live :
    (prepare policy s inv p reads calls).1.owners = s.owners ∧
    (prepare policy s inv p reads calls).1.history = s.history ∧
    (prepare policy s inv p reads calls).1.epoch = s.epoch ∧
    (prepare policy s inv p reads calls).1.visible = s.visible := by
  unfold prepare
  repeat' (first | split | simp_all)

theorem prepare_refusal (h : (prepare policy s inv p reads calls).2 = none) :
    (prepare policy s inv p reads calls).1 = s := by
  unfold prepare at h ⊢
  repeat' (first | split | simp_all)

theorem token_exact_participants (t : PreparedToken d) :
    t.writes.map Candidate.owner = t.participants.writes ∧
    t.observations.map Observation.owner = t.participants.readonly ∧
    (t.participants.writes ++ t.participants.readonly).Nodup :=
  ⟨t.exactWrites, t.exactReads, t.shape.1⟩

theorem begin_refusal (h : (beginCommit policy s inv id).2 = none) :
    (beginCommit policy s inv id).1 = s := by
  unfold beginCommit at h ⊢
  repeat' (first | split | simp_all)

theorem begin_issued (h : (beginCommit policy s inv id).2 = some t) :
    s.pending id = some t ∧ tokenValid policy s inv t = true ∧ id = t.identity := by
  unfold beginCommit at h
  split at h
  · simp at h
  · split at h
    · simp_all
    · simp at h

theorem preflight_cross_valid (h : tokenValid policy s inv t = true) :
    policy.consistent t.observations t.writes := by
  apply policy.sound
  simp only [tokenValid, Bool.and_eq_true] at h
  exact h.2

theorem swap_local (s : Owners Payload) (c : Candidate d view) :
    (swap s c c.owner).value = c.value ∧
    (swap s c c.owner).revision = c.old.revision + 1 := by simp [swap]

theorem swap_frame (s : Owners Payload) (c : Candidate d view) (h : i ≠ c.owner) :
    swap s c i = s i := by simp [swap, h]

theorem swap_region_frame (r : RegionLayout) (s : Owners Payload) (c : Candidate d view)
    (h : r.slot i ≠ r.slot c.owner) : swap s c i = s i := by
  apply swap_frame
  intro eq
  exact h (congrArg r.slot eq)

theorem swapAll_frame (s : Owners Payload) (writes : List (Candidate d view))
    (h : ∀ c ∈ writes, i ≠ c.owner) : swapAll s writes i = s i := by
  induction writes generalizing s with
  | nil => rfl
  | cons c cs ih =>
    rw [swapAll, List.foldl_cons]
    rw [show cs.foldl swap (swap s c) i = swap s c i from
      ih (swap s c) (fun w hw => h w (by simp [hw]))]
    exact swap_frame s c (h c (by simp))

def LocallyValid (d : Domain Payload Event) (s : Owners Payload) : Prop :=
  ∀ i, d.invariant i (s i).value

theorem swap_invariants {locals : Owners Payload} (h : LocallyValid d locals)
    (c : Candidate d view) : LocallyValid d (swap locals c) := by
  intro i
  by_cases eq : i = c.owner
  · subst i; simpa [swap] using c.valueValid
  · simpa [swap, eq] using h i

theorem swapAll_invariants {locals : Owners Payload} (h : LocallyValid d locals)
    (writes : List (Candidate d view)) : LocallyValid d (swapAll locals writes) := by
  induction writes generalizing locals with
  | nil => exact h
  | cons c cs ih => exact ih (swap_invariants h c)

theorem publication_swap_is_one_slot (before : Host d)
    (done : List (Candidate d t.observations)) (c : Candidate d t.observations)
    (todo : List (Candidate d t.observations)) (h : done ++ c :: todo = t.writes) :
    (materialize before t (.swapping (done ++ [c]) todo
      (by simpa [List.append_assoc] using h))).owners =
      swap (materialize before t (.swapping done (c :: todo) h)).owners c := by
  simp [materialize, swapAll, List.foldl_append]

theorem publication_completes (done todo : List (Candidate d t.observations))
    (h : done ++ todo = t.writes) :
    PublicationSteps t (.swapping done todo h) .opened := by
  induction todo generalizing done with
  | nil =>
    exact .next (.slotsDone done h)
      (.next .appendEvents (.next .consumeTicket (.next .openGate (.refl _))))
  | cons c cs ih =>
    exact .next (.swap done c cs h)
      (ih (done ++ [c]) (by simpa [List.append_assoc] using h))

theorem no_partial_observation (before : Host d) (phase : PublicationPhase t)
    (h : phase ≠ .opened) : externalObserve (materialize before t phase) = none := by
  cases phase <;> simp_all [externalObserve, materialize, wholeCommit]

theorem open_is_whole_commit (before : Host d) :
    materialize before t .opened = wholeCommit before t := rfl

theorem opened_observation (before : Host d) :
    externalObserve (materialize before t .opened) =
      some (observeUnion (swapAll before.owners t.writes)) := rfl

theorem publication_frame (before : Host d) (phase : PublicationPhase t)
    (h : i ∉ t.participants.writes) : (materialize before t phase).owners i = before.owners i := by
  have all : ∀ c ∈ t.writes, i ≠ c.owner := by
    intro c hc eq
    apply h
    rw [← t.exactWrites]
    subst i
    exact List.mem_map.mpr ⟨c, hc, rfl⟩
  cases phase with
  | swapping done todo partition =>
    apply swapAll_frame
    intro c hc
    apply all c
    rw [← partition]
    simp [hc]
  | events => exact swapAll_frame _ _ all
  | consume => exact swapAll_frame _ _ all
  | release => exact swapAll_frame _ _ all
  | opened => exact swapAll_frame _ _ all

theorem completion_invariants (before : Host d) (h : LocallyValid d before.owners) :
    LocallyValid d (materialize before t .opened).owners := swapAll_invariants h t.writes

theorem completed_history (before : Host d) :
    (materialize before t .opened).history = before.history ++ events t := rfl

theorem completed_replay_refused (before : Host d) (inv : Invocation) :
    beginCommit policy (materialize before t .opened) inv t.identity =
      (materialize before t .opened, none) := by simp [beginCommit, materialize, wholeCommit]

theorem busy_refused (h : s.visible = false) : beginCommit policy s inv id = (s, none) := by
  unfold beginCommit
  split <;> simp [tokenValid, h]

theorem stale_generation (c : Capture) (h : (s.owners c.owner).generation ≠ c.generation) :
    current s.registry s.owners c = false := by
  unfold current
  split <;> simp [h]

end Laws


section Revalidation
variable {policy : Coordination d} {s : Host d} {inv : Invocation} {t : PreparedToken d}
  {id : Authority.Identity}

theorem preflight_writes (h : tokenValid policy s inv t = true) :
    ∀ c ∈ t.writes, current s.registry s.owners c.stamp = true ∧
      s.owners c.owner = c.old ∧ authorized s.registry inv c.stamp = true := by
  simp only [tokenValid, Bool.and_eq_true, List.all_eq_true] at h
  intro c hc
  have checked := h.1.2 c hc
  simpa only [candidateCurrent, Bool.and_eq_true, decide_eq_true_eq, and_assoc] using checked

theorem preflight_observations (h : tokenValid policy s inv t = true) :
    ∀ o ∈ t.observations, current s.registry s.owners o.stamp = true ∧
      (s.owners o.owner).value = o.value ∧ authorized s.registry inv o.stamp = true := by
  simp only [tokenValid, Bool.and_eq_true, List.all_eq_true] at h
  intro o ho
  have checked := h.1.1.2 o ho
  simpa only [observationCurrent, Bool.and_eq_true, decide_eq_true_eq, and_assoc] using checked

theorem live_domain_proposal (h : tokenValid policy s inv t = true)
    (hc : c ∈ t.writes) :
    d.legal c.owner c.stamp.binding (s.owners c.owner).value t.observations
      c.command c.value c.events := by
  rw [(preflight_writes h c hc).2.1]
  exact c.commandValid

theorem live_authority_laws (h : authorized r inv c = true) :
    Authority.authority (authorityView r) inv.caller = true ∧
    Authority.permitted (authorityView r) inv.caller c.dependency c.access = true := by
  simpa only [authorized, Bool.and_eq_true] using h

theorem invalid_token_refused (hp : s.pending id = some t)
    (h : tokenValid policy s inv t = false) : beginCommit policy s inv id = (s, none) := by
  simp [beginCommit, hp, h]

theorem invalid_readonly_refused (hp : s.pending id = some t)
    (ho : o ∈ t.observations) (h : observationCurrent s.registry s.owners o = false) :
    beginCommit policy s inv id = (s, none) := by
  apply invalid_token_refused hp
  apply Bool.eq_false_iff.mpr
  intro valid
  have checked := preflight_observations valid o ho
  simp [observationCurrent, checked.1, checked.2.1] at h

theorem revoked_write_refused (hp : s.pending id = some t)
    (hc : c ∈ t.writes) (h : authorized s.registry inv c.stamp = false) :
    beginCommit policy s inv id = (s, none) := by
  apply invalid_token_refused hp
  apply Bool.eq_false_iff.mpr
  intro valid
  have checked := (preflight_writes valid c hc).2.2
  simp [h] at checked

theorem wrong_invocation_refused (h : inv ≠ t.invocation) (hp : s.pending id = some t) :
    beginCommit policy s inv id = (s, none) := by
  apply invalid_token_refused hp
  simp [tokenValid, h]

/-- Future resolution may change while the captured chain/schema/authority survive. -/
theorem compatible_future_current (r r' : Registry) (s : Owners Payload) (c : Capture)
    (p p' : Route) (old : route r c.owner = some p) (new : route r' c.owner = some p')
    (metadata : p'.generation = p.generation ∧ p'.schema = p.schema ∧
      p'.gate = p.gate ∧ p'.sealedBackend = p.sealedBackend)
    (gate : r'.gate = r.gate) (code : r'.available = r.available) :
    current r' s c = current r s c := by
  simp [current, old, new, metadata.1, metadata.2.1, metadata.2.2.1,
    metadata.2.2.2, gate, code]

/-- This is a one-address observational adapter, not a whole-model refinement. -/
def stampView (s : Owners Payload) (stamp : Authority.Stamp) : Option (Authority.Cell Unit) :=
  if stamp.handle.address.key = 0 then
    some ⟨(s stamp.handle.address.owner).generation, (s stamp.handle.address.owner).revision, ()⟩
  else none

theorem authority_stamp_adapter (s : Owners Payload) (r : Registry) (c : Capture) :
    Authority.current { authorityView r with resources := (fun a =>
      if a.key = 0 then some ⟨(s a.owner).generation, (s a.owner).revision, ()⟩ else none) }
      c.dependency.stamp =
    (decide ((s c.owner).generation = c.generation) &&
      decide ((s c.owner).revision = c.revision)) := by
  by_cases hg : (s c.owner).generation = c.generation <;>
    by_cases hr : (s c.owner).revision = c.revision <;>
    simp [Authority.current, Capture.dependency, hg, hr]

end Revalidation

/-- Query behavior is bound to exact code/policy/input identity. Cache results are
pure functions of detached declared observations, never authoritative stored fields. -/
structure DerivedDependencies (Payload : OwnerId → Type) where
  observations : List (Observation Payload)
  query : Capture
  cacheable : Bool

def DerivedDependencies.queryBinding (deps : DerivedDependencies Payload) := deps.query.binding
def DerivedDependencies.queryOwner (deps : DerivedDependencies Payload) := deps.query.owner

structure DerivedCache (Payload : OwnerId → Type) (Result : Type) where
  dependencies : DerivedDependencies Payload
  evaluate : List (Observation Payload) → BehaviorBinding → Result
  result : Result
  correct : result = evaluate dependencies.observations dependencies.queryBinding

variable {Result : Type} {s : Host d} {cache : DerivedCache Payload Result}
  {deps : DerivedDependencies Payload}

/-- Cached query code follows current dispatch; prepared commands follow retained code.
An external input is cacheable only with a fixed recorded input identity. -/
def dependenciesValid (s : Host d) (deps : DerivedDependencies Payload) : Bool :=
  (s.visible && deps.cacheable) && deps.observations.all (fun o =>
    observationCurrent s.registry s.owners o &&
      match route s.registry o.owner with
      | none => false
      | some p => decide (p.future = o.stamp.binding)) &&
    current s.registry s.owners deps.query && decide (deps.query.access = .read) &&
    match route s.registry deps.queryOwner with
    | none => false
    | some p => decide (p.future = deps.queryBinding) &&
        deps.queryBinding.chain.all (fun a => s.registry.available.contains a)

def readDerived (s : Host d) (cache : DerivedCache Payload Result) : Option Result :=
  if dependenciesValid s cache.dependencies then some cache.result else none

theorem cached_result_bound (h : readDerived s cache = some result) :
    result = cache.evaluate cache.dependencies.observations cache.dependencies.queryBinding := by
  unfold readDerived at h
  split at h
  · simp only [Option.some.injEq] at h
    exact h.symm.trans cache.correct
  · simp at h

theorem invalid_cache_refused (h : dependenciesValid s cache.dependencies = false) :
    readDerived s cache = none := by simp [readDerived, h]

theorem closed_cache_refused (h : s.visible = false) : readDerived s cache = none := by
  simp [readDerived, dependenciesValid, h]

theorem noncacheable_refused (h : cache.dependencies.cacheable = false) :
    readDerived s cache = none := by simp [readDerived, dependenciesValid, h]

theorem valid_dependencies_current (h : dependenciesValid s deps = true) :
    ∀ o ∈ deps.observations, current s.registry s.owners o.stamp = true ∧
      (s.owners o.owner).value = o.value := by
  simp only [dependenciesValid, Bool.and_eq_true, List.all_eq_true] at h
  intro o ho
  have checked := (h.1.1.1.2 o ho).1
  simpa only [observationCurrent, Bool.and_eq_true, decide_eq_true_eq] using checked

/-- Unload is conservatively blocked by dispatch, owner schema, pending/cache/timer pins.
Pins may be retained after consumption; safe over-retention is not liveness. -/
def requiredArtifacts (r : Registry) :=
  r.pins ++ r.routes.flatMap (fun p => p.future.chain)

def unload (r : Registry) (a : Authority.Artifact) : Registry × Bool :=
  if (requiredArtifacts r).any (fun b => decide (b = a)) then (r, false)
  else ({ r with available := r.available.filter (· != a) }, true)

theorem retained_unload_refused (h : a ∈ requiredArtifacts r) : unload r a = (r, false) := by
  have found : (requiredArtifacts r).any (fun b => decide (b = a)) = true := by
    simp only [List.any_eq_true, decide_eq_true_eq]
    exact ⟨a, h, rfl⟩
  simp [unload, found]

theorem pending_chain_unload_refused (h : a ∈ tokenArtifacts t)
    (pin : tokenArtifacts t ⊆ r.pins) : unload r a = (r, false) := by
  apply retained_unload_refused
  exact List.mem_append_left _ (pin h)

theorem prepare_pins_chain {policy : Coordination d} {id : Authority.Identity}
    (h : (prepare policy s inv p reads calls).2 = some id) :
    ∃ t, (prepare policy s inv p reads calls).1.pending id = some t ∧
      tokenArtifacts t ⊆ (prepare policy s inv p reads calls).1.registry.pins := by
  unfold prepare at h ⊢
  repeat' (first | split | simp_all [List.subset_def])

/-- An admitted whole snapshot must validate owner-local states AND metadata/references.
Schemas, non-reused counters, grants, pins, timers, input and pending-token backend
support belong in `references`. Unsupported restoration cannot be declared valid. -/
structure RestoreAdmission (d : Domain Payload Event) where
  references : Host d → Prop
  check : Host d → Bool
  sound : ∀ s, check s = true → LocallyValid d s.owners ∧ references s

def restoreOwners (policy : RestoreAdmission d) (before candidate : Host d) : Host d × Bool :=
  if before.visible && candidate.visible && policy.check candidate then (candidate, true)
  else (before, false)

theorem restore_refuses_all {policy : RestoreAdmission d} {candidate : Host d}
    (h : (restoreOwners policy s candidate).2 = false) :
    (restoreOwners policy s candidate).1 = s := by
  unfold restoreOwners at h ⊢
  split <;> simp_all

theorem restored_owners_valid {policy : RestoreAdmission d} {candidate : Host d}
    (h : (restoreOwners policy s candidate).2 = true) :
    LocallyValid d (restoreOwners policy s candidate).1.owners ∧
      policy.references (restoreOwners policy s candidate).1 := by
  unfold restoreOwners at h ⊢
  split
  · rename_i checked
    simp only [Bool.and_eq_true] at checked
    exact policy.sound candidate checked.2
  · simp_all

/-- Bind owner timer targets to generations/revisions through the existing stamp API;
retained schema/code/authority must additionally pass the owner's captured validation.
Callbacks remain ordinary commands after gate opening, never publication callbacks. -/
def timerTargetCurrent (s : Host d) (stamp : Authority.Stamp) : Bool :=
  s.visible && decide (stamp.handle.address.key = 0 ∧
    (s.owners stamp.handle.address.owner).generation = stamp.handle.generation ∧
    (s.owners stamp.handle.address.owner).revision = stamp.revision)

theorem timer_target_closed (h : s.visible = false) : timerTargetCurrent s stamp = false := by
  simp [timerTargetCurrent, h]

theorem timer_target_recreated
    (h : (s.owners stamp.handle.address.owner).generation ≠ stamp.handle.generation) :
    timerTargetCurrent s stamp = false := by simp [timerTargetCurrent, h]

/-- Publication appends to the already committed prefix, and later refusal keeps it. -/
theorem workflow_prefix_composition (before : Host d) (t : PreparedToken d)
    (policy : Coordination d) (inv : Invocation) (id : Authority.Identity)
    (later : (beginCommit policy (wholeCommit before t) inv id).2 = none) :
    (beginCommit policy (wholeCommit before t) inv id).1.history = before.history ++ events t := by
  rw [begin_refusal later]
  rfl

/-- Existing Workflow resumes and rejects preserve their owned checkpoint. Connecting
that checkpoint to a Host is an explicit codec/adapter task, not inferred refinement. -/
theorem workflow_reject_prefix (frame : Workflow.Frame State Phase Event Result Prompt)
    (output : List Event) :
    (Workflow.applyOutcome frame (.reject output)).state = frame.state ∧
    (Workflow.applyOutcome frame (.reject output)).trace = frame.trace ++ output :=
  ⟨Workflow.reject_preserves_checkpoint frame output, rfl⟩


/-! Checked software witnesses. Inventory/location and charge/debt constraints come
from the ownership contract and Authority lifetime fixture. Scalar activation uses
Architecture's base increment + boost increment. These are not NetHack formulas. -/
namespace Witness

/-- Certified proposals use executable domain relations, not assumed success laws. -/
def checkedDomain (Command : OwnerId → Type)
    (cap : (i : OwnerId) → Command i → Authority.Capability)
    (valid : (i : OwnerId) → Payload i → Bool)
    (transition : (i : OwnerId) → BehaviorBinding → Payload i → List (Observation Payload) →
      Command i → Option (Payload i × List String)) : Domain Payload String where
  invariant := fun i value => valid i value = true
  Command := Command
  capability := cap
  legal := fun i b old view cmd value output => transition i b old view cmd = some (value, output)
  propose := fun i b old view cmd =>
    match h : transition i b old view cmd with
    | none => none
    | some proposed =>
      if hv : valid i proposed.1 = true then some ⟨proposed, by rfl, hv⟩ else none

def proposalValue (writes : List (Candidate d view)) (i : OwnerId) : Option (Payload i) :=
  writes.foldr (fun c rest => if h : c.owner = i then some (h ▸ c.value) else rest) none

def observedValue (view : List (Observation Payload)) (i : OwnerId) : Option (Payload i) :=
  view.foldr (fun o rest => if h : o.owner = i then some (h ▸ o.value) else rest) none

def policy (max : Nat) (check : (view : List (Observation Payload)) →
    List (Candidate d view) → Bool) : Coordination d :=
  ⟨max, check, fun view writes => check view writes = true, fun _ _ h => h,
    fun _ writes => writes, fun _ _ => .refl _⟩

def queryCap : Authority.Capability := ⟨"observe", 1⟩
def oldExtension : Authority.Artifact := ⟨"boost", 1, 500⟩
def newExtension : Authority.Artifact := ⟨"double", 2, 501⟩
def binding (i : OwnerId) : BehaviorBinding :=
  ⟨⟨"owner", 1, i + 800⟩, [oldExtension], 1, 1, 17, 29⟩
def caller : Authority.Caller := ⟨42, 100, ⟨"command", 1, 700⟩⟩
def invocation : Invocation := ⟨caller, ⟨42, 99⟩, 3, 5⟩
def ticket : Authority.Identity := ⟨42, 0⟩
def provider (i : OwnerId) (cap : Authority.Capability) : Authority.Provider :=
  ⟨i, (binding i).module, [cap]⟩

def registry (ids : List OwnerId) (commandCap : Authority.Capability) : Registry where
  host := 42
  gate := 7
  callers := caller :: ids.map (fun i => ⟨42, i, (binding i).module⟩)
  providers := ids.flatMap fun i => [provider i commandCap, provider i queryCap]
  grants := ids.flatMap fun i =>
    [⟨caller, provider i commandCap, commandCap, .write⟩,
      ⟨caller, provider i queryCap, queryCap, .read⟩]
  invocations := [invocation]
  routes := ids.map fun i => ⟨i, 9, 1, binding i, 7, true⟩
  available := oldExtension :: newExtension :: ids.map (fun i => (binding i).module)
  pins := []

def host (d : Domain Payload String) (ids : List OwnerId) (cap : Authority.Capability)
    (values : (i : OwnerId) → Payload i) : Host d :=
  ⟨registry ids cap, fun i => ⟨9, 2, 1, values i⟩, ["prior"], fun _ => none, 0, 0, true⟩

def replaceFuture (r : Registry) : Registry :=
  { r with routes := r.routes.map fun p =>
      { p with future := { p.future with extensions := [newExtension] } } }

namespace Transfer
structure Inventory where
  held : Bool
  capacity : Nat
  deriving DecidableEq

abbrev Family : OwnerId → Type
  | 0 => Inventory
  | 1 => Inventory
  | 2 => Option OwnerId
  | _ => Unit

instance : (i : OwnerId) → DecidableEq (Family i) := fun i => by
  unfold Family
  repeat' (first | split | infer_instance)

inductive Command where
  | remove | receive | relocate
  deriving DecidableEq

def cap : Authority.Capability := ⟨"transfer_item", 1⟩

def valid : (i : OwnerId) → Family i → Bool
  | 0, v => decide ((if v.held then 1 else 0) ≤ v.capacity)
  | 1, v => decide ((if v.held then 1 else 0) ≤ v.capacity)
  | _, _ => true

def transition : (i : OwnerId) → BehaviorBinding → Family i → List (Observation Family) →
    Command → Option (Family i × List String)
  | 0, _, v, _, .remove => if v.held then some (⟨false, v.capacity⟩, ["remove"]) else none
  | 1, _, v, _, .receive =>
    if !v.held && decide (0 < v.capacity) then some (⟨true, v.capacity⟩, ["receive"]) else none
  | 2, _, v, _, .relocate => if v = some 0 then some (some 1, ["locate"]) else none
  | _, _, _, _, _ => none

def domain : Domain Family String := checkedDomain (fun _ => Command) (fun _ _ => cap) valid transition

def cross (view : List (Observation Family)) (writes : List (Candidate domain view)) : Bool :=
  match proposalValue writes 0, proposalValue writes 1, proposalValue writes 2 with
  | some src, some dst, some loc => !src.held && dst.held && decide (loc = some 1)
  | _, _, _ => false

def coordination := policy 4 cross

def values : (i : OwnerId) → Family i
  | 0 => ⟨true, 1⟩
  | 1 => ⟨false, 1⟩
  | 2 => some 0
  | _ + 3 => ()

def initial := host domain [0, 1, 2, 3] cap values

def participants : ParticipantSet := ⟨[0, 1, 2], [3]⟩
def reads : List ReadRequest := [⟨3, queryCap⟩]
def calls : List (Call domain) := [⟨0, .remove⟩, ⟨1, .receive⟩, ⟨2, .relocate⟩]
def ready := (prepare coordination initial invocation participants reads calls).1

theorem prepare_success : (prepare coordination initial invocation participants reads calls).2 =
    some ticket := by decide

def token : PreparedToken domain := (ready.pending ticket).get (by decide)
def finished := materialize ready token .opened

theorem commit_success : (beginCommit coordination ready invocation ticket).2.isSome = true := by decide

theorem transfer_result :
    (finished.owners 0).value.held = false ∧ (finished.owners 1).value.held = true ∧
    (finished.owners 2).value = some 1 ∧ finished.history = ["prior", "remove", "receive", "locate"] := by decide

theorem transfer_invariants : LocallyValid domain finished.owners := by
  apply completion_invariants
  rw [show ready.owners = initial.owners from (prepare_preserves_live
    (policy := coordination) (s := initial) (inv := invocation)
    (p := participants) (reads := reads) (calls := calls)).1]
  intro i
  change valid i (values i) = true
  unfold valid values
  repeat' (first | split | decide)

def noCapacity : Host domain := { initial with owners := fun i =>
  if h : i = 1 then h.symm ▸ (⟨9, 2, 1, ⟨false, 0⟩⟩ : OwnerLocal Family 1) else initial.owners i }

theorem capacity_rejection : prepare coordination noCapacity invocation participants reads calls =
    (noCapacity, none) := by rfl

def staleDestination : Host domain := { ready with owners := fun i =>
  if h : i = 1 then h.symm ▸ { ready.owners 1 with revision := 3 } else ready.owners i }

theorem destination_rejection : beginCommit coordination staleDestination invocation ticket =
    (staleDestination, none) := by rfl

def staleReadonly : Host domain := { ready with owners := fun i =>
  if h : i = 3 then h.symm ▸ { ready.owners 3 with revision := 3 } else ready.owners i }

theorem readonly_rejection : beginCommit coordination staleReadonly invocation ticket =
    (staleReadonly, none) := by rfl

def revoked : Host domain := { ready with registry := { ready.registry with grants := [] } }

theorem authority_rejection : beginCommit coordination revoked invocation ticket = (revoked, none) := by rfl

def compatible : Host domain := { ready with registry := replaceFuture ready.registry }

theorem retained_chain_success : (beginCommit coordination compatible invocation ticket).2.isSome = true := by decide

def unavailable : Host domain := { compatible with registry :=
  { compatible.registry with available := compatible.registry.available.filter (fun a => decide (a ≠ oldExtension)) } }

theorem retained_code_rejection : beginCommit coordination unavailable invocation ticket = (unavailable, none) := by rfl

theorem wrong_lease_rejection : beginCommit coordination ready { invocation with lease := 6 } ticket =
    (ready, none) := by rfl

theorem duplicate_rejection : prepare coordination initial invocation ⟨[0, 0, 1, 2], [3]⟩
    reads (⟨0, .remove⟩ :: calls) = (initial, none) := by rfl

theorem replay_rejection : beginCommit coordination finished invocation ticket = (finished, none) :=
  completed_replay_refused ready invocation

end Transfer

namespace Retirement
structure Item where
  quantity : Nat
  charges : Int
  retired : Bool
  deriving DecidableEq

abbrev Family : OwnerId → Type
  | 1 => Item
  | 2 => Option Authority.Handle
  | 3 => Option Authority.Handle
  | 4 => Authority.Witness.Debt
  | 5 => Nat
  | _ => Unit

instance : (i : OwnerId) → DecidableEq (Family i) := fun i => by
  unfold Family
  repeat' (first | split | infer_instance)

inductive Command where
  | consumeCharge | retire | clearEquipment | cancelTimer | account
  deriving DecidableEq

def cap : Authority.Capability := ⟨"charge_debt_retirement", 1⟩

def valid : (i : OwnerId) → Family i → Bool
  | 1, v => decide (0 < v.quantity ∧ -1 ≤ v.charges)
  | 4, v => decide (0 < v.quantity ∧ 0 < v.liability)
  | _, _ => true

def transition : (i : OwnerId) → BehaviorBinding → Family i → List (Observation Family) →
    Command → Option (Family i × List String)
  | 1, _, v, _, .consumeCharge =>
    if !v.retired && decide (v.charges = 0) then
      some ({ v with charges := v.charges - 1 }, ["charge"]) else none
  | 1, _, v, _, .retire =>
    if !v.retired && decide (v.charges < 0) then
      some ({ v with retired := true }, ["dust", "retired"]) else none
  | 2, _, v, _, .clearEquipment =>
    if v = some (Authority.Witness.handle 1) then some (none, ["unequip"]) else none
  | 3, _, v, _, .cancelTimer =>
    if v = some (Authority.Witness.handle 1) then some (none, ["cancel-timer"]) else none
  | 4, _, v, view, .account =>
    if !v.consumed && decide (v.origin = Authority.Witness.handle 1) &&
        decide (observedValue view 5 = some v.terms) then
      some ({ v with consumed := true }, ["account"]) else none
  | _, _, _, _, _ => none

def domain : Domain Family String := checkedDomain (fun _ => Command) (fun _ _ => cap) valid transition

def chargeCheck (view : List (Observation Family)) (writes : List (Candidate domain view)) : Bool :=
  match proposalValue writes 1 with
  | some item => !item.retired && decide (item.charges = -1)
  | none => false

def retirementCheck (view : List (Observation Family)) (writes : List (Candidate domain view)) : Bool :=
  match proposalValue writes 1, proposalValue writes 2, proposalValue writes 3,
      proposalValue writes 4, observedValue view 5 with
  | some item, some equipment, some timer, some debt, some terms =>
    item.retired && decide (item.charges = -1 ∧ equipment = none ∧ timer = none ∧
      item.quantity = debt.quantity ∧ terms = debt.terms ∧ 0 < debt.liability) && debt.consumed
  | _, _, _, _, _ => false

def chargePolicy := policy 1 chargeCheck
def retirementPolicy := policy 5 retirementCheck

def values : (i : OwnerId) → Family i
  | 0 => ()
  | 1 => ⟨3, 0, false⟩
  | 2 => some (Authority.Witness.handle 1)
  | 3 => some (Authority.Witness.handle 1)
  | 4 => Authority.Witness.debt
  | 5 => 17
  | _ + 6 => ()

def initial := host domain [1, 2, 3, 4, 5] cap values
def chargeReady := (prepare chargePolicy initial invocation ⟨[1], []⟩ [] [⟨1, .consumeCharge⟩]).1

theorem charge_prepare_success : (prepare chargePolicy initial invocation ⟨[1], []⟩ []
    [⟨1, .consumeCharge⟩]).2 = some ticket := by decide

def chargeToken : PreparedToken domain := (chargeReady.pending ticket).get (by decide)
def charged := materialize chargeReady chargeToken .opened

theorem charge_commit_success : (beginCommit chargePolicy chargeReady invocation ticket).2.isSome = true := by decide

def participants : ParticipantSet := ⟨[1, 2, 3, 4], [5]⟩
def reads : List ReadRequest := [⟨5, queryCap⟩]
def calls : List (Call domain) :=
  [⟨1, .retire⟩, ⟨2, .clearEquipment⟩, ⟨3, .cancelTimer⟩, ⟨4, .account⟩]
def retirementTicket : Authority.Identity := ⟨42, 1⟩
def ready := (prepare retirementPolicy charged invocation participants reads calls).1

theorem retirement_prepare_success :
    (prepare retirementPolicy charged invocation participants reads calls).2 = some retirementTicket := by decide

def token : PreparedToken domain := (ready.pending retirementTicket).get (by decide)
def finished := materialize ready token .opened

theorem retirement_commit_success :
    (beginCommit retirementPolicy ready invocation retirementTicket).2.isSome = true := by decide

theorem debt_retirement_result :
    (finished.owners 1).value.retired = true ∧ (finished.owners 1).value.quantity = 3 ∧
    (finished.owners 1).value.charges = -1 ∧ (finished.owners 2).value = none ∧
    (finished.owners 3).value = none ∧
    (finished.owners 4).value = { Authority.Witness.debt with consumed := true } := by decide

theorem retained_accounting_fields :
    (finished.owners 4).value.quantity = 3 ∧ (finished.owners 4).value.terms = 17 ∧
    (finished.owners 4).value.creditor = 91 ∧ (finished.owners 4).value.liability = 120 ∧
    (finished.owners 4).value.origin = Authority.Witness.handle 1 := by decide

def refusedAccounting : Host domain := { charged with owners := fun i =>
  if h : i = 4 then h.symm ▸ { charged.owners 4 with
    value := { (charged.owners 4).value with quantity := 4 } } else charged.owners i }

theorem accounting_rejection : prepare retirementPolicy refusedAccounting invocation participants reads calls =
    (refusedAccounting, none) := by rfl

theorem refused_retirement_keeps_charge_prefix :
    (prepare retirementPolicy refusedAccounting invocation participants reads calls).1.history =
      ["prior", "charge"] ∧
    ((prepare retirementPolicy refusedAccounting invocation participants reads calls).1.owners 1).value.charges = -1 := by decide

theorem premature_retirement_rejection :
    prepare retirementPolicy initial invocation participants reads calls = (initial, none) := by rfl

end Retirement

namespace Equipment

abbrev Family : OwnerId → Type
  | 0 => Option OwnerId
  | _ + 1 => Nat

instance : (i : OwnerId) → DecidableEq (Family i) := fun i => by
  unfold Family
  repeat' (first | split | infer_instance)

inductive Command where
  | equip | contribute | retireContribution
  deriving DecidableEq

def cap : Authority.Capability := ⟨"effective_equipment", 1⟩

def transition : (i : OwnerId) → BehaviorBinding → Family i → List (Observation Family) →
    Command → Option (Family i × List String)
  | 0, _, none, _, .equip => some (some 1, ["equip"])
  | 1, _, n, _, .contribute => some (n + 1, ["contribution"])
  | 1, _, _, _, .retireContribution => some (0, ["retire-contribution"])
  | _, _, _, _, _ => none

def domain : Domain Family String := checkedDomain (fun _ => Command) (fun _ _ => cap)
  (fun _ _ => true) transition

def cross (view : List (Observation Family)) (writes : List (Candidate domain view)) : Bool :=
  match proposalValue writes 0, proposalValue writes 1 with
  | some (some 1), some n => decide (0 < n)
  | _, _ => false

def coordination := policy 2 cross

def values : (i : OwnerId) → Family i
  | 0 => none
  | 1 => 5
  | n + 2 => by simpa [Family] using (88 : Nat)

def initial := host domain [0, 1, 2] cap values

def query (view : List (Observation Family)) (b : BehaviorBinding) : Nat :=
  match observedValue view 0, observedValue view 1 with
  | some (some 1), some n =>
    if b.extensions = [oldExtension] then n + 1 else n + n
  | _, _ => 0

def queryReads : List ReadRequest := [⟨0, queryCap⟩, ⟨1, queryCap⟩]

def cacheFor (s : Host domain) : Option (DerivedCache Family Nat) := do
  let view ← readDeclared s.registry s.owners queryReads
  let q ← capture s.registry s.owners 0 queryCap .read
  return ⟨⟨view, q, true⟩, query, query view q.binding, rfl⟩

def oldCache := (cacheFor initial).get (by decide)

theorem initial_derived_success : readDerived initial oldCache = some 0 := by decide

def calls : List (Call domain) := [⟨0, .equip⟩, ⟨1, .contribute⟩]
def ready := (prepare coordination initial invocation ⟨[0, 1], []⟩ [] calls).1

theorem equipment_prepare_success :
    (prepare coordination initial invocation ⟨[0, 1], []⟩ [] calls).2 = some ticket := by decide

def token : PreparedToken domain := (ready.pending ticket).get (by decide)
def finished := materialize ready token .opened

theorem equipment_commit_success : (beginCommit coordination ready invocation ticket).2.isSome = true := by decide

def newCache := (cacheFor finished).get (by decide)

theorem derived_recomputed_result : readDerived finished newCache = some 7 := by decide

theorem stale_derived_rejection : readDerived finished oldCache = none := by decide

theorem incomplete_equipment_rejection :
    prepare coordination initial invocation ⟨[0], []⟩ [] [⟨0, .equip⟩] = (initial, none) := by rfl

def unrelatedChange : Host domain := { initial with owners := fun i =>
  if h : i = 2 then h.symm ▸ (⟨9, 3, 1, 99⟩ : OwnerLocal Family 2) else initial.owners i }

theorem unrelated_cache_stays_valid : readDerived unrelatedChange oldCache = some 0 := by decide

def codeChange : Host domain := { initial with registry := replaceFuture initial.registry }

theorem derived_code_rejection : readDerived codeChange oldCache = none := by decide

def schemaChange : Host domain := { initial with owners := fun i =>
  if h : i = 1 then h.symm ▸ { initial.owners 1 with schema := 2 } else initial.owners i }

theorem derived_schema_rejection : readDerived schemaChange oldCache = none := by decide

def recreated : Host domain := { initial with owners := fun i =>
  if h : i = 1 then h.symm ▸ { initial.owners 1 with generation := 10 } else initial.owners i }

theorem derived_generation_rejection : readDerived recreated oldCache = none := by decide

theorem gated_derived_rejection : readDerived { ready with visible := false } oldCache = none := by decide

end Equipment

namespace EntityFree

abbrev Family : OwnerId → Type
  | 0 => Nat
  | 1 => Bool
  | 2 => List Nat
  | _ => Unit

instance : (i : OwnerId) → DecidableEq (Family i) := fun i => by
  unfold Family
  repeat' (first | split | infer_instance)

inductive Command where
  | activate | configure | advanceField
  deriving DecidableEq

def cap : Authority.Capability := ⟨"activate_field", 1⟩

def transition : (i : OwnerId) → BehaviorBinding → Family i → List (Observation Family) →
    Command → Option (Family i × List String)
  | 0, b, n, view, .activate =>
    if observedValue view 1 = some true then
      if b.extensions = [oldExtension] then some (n + 1 + 1, ["activate"])
      else if b.extensions = [newExtension] then some ((n + 1) + (n + 1), ["activate"])
      else none
    else none
  | 1, _, mode, _, .configure => some (!mode, ["configure"])
  | 2, _, buffer, _, .advanceField => some (buffer.map (· + 1), ["field"])
  | _, _, _, _, _ => none

def valid : (i : OwnerId) → Family i → Bool
  | 2, buffer => decide (buffer.length = 2)
  | _, _ => true

def domain : Domain Family String := checkedDomain (fun _ => Command) (fun _ _ => cap) valid transition

def coordination : Coordination domain := policy 3 (fun _ _ => true)

def values : (i : OwnerId) → Family i
  | 0 => 41
  | 1 => true
  | 2 => [1, 2]
  | _ + 3 => ()

def initial := host domain [0, 1, 2] cap values

def participants : ParticipantSet := ⟨[0, 2], [1]⟩
def reads : List ReadRequest := [⟨1, queryCap⟩]
def calls : List (Call domain) := [⟨0, .activate⟩, ⟨2, .advanceField⟩]
def ready := (prepare coordination initial invocation participants reads calls).1

theorem scalar_field_prepare_success :
    (prepare coordination initial invocation participants reads calls).2 = some ticket := by decide

def token : PreparedToken domain := (ready.pending ticket).get (by decide)
def finished := materialize ready token .opened

theorem scalar_field_commit_success : (beginCommit coordination ready invocation ticket).2.isSome = true := by decide

theorem scalar_field_result : (finished.owners 0).value = 43 ∧
    (finished.owners 2).value = [2, 3] ∧ (finished.owners 1).value = true := by decide

theorem hidden_read_rejection : prepare coordination initial invocation ⟨[0, 2], []⟩ [] calls =
    (initial, none) := by rfl

def compatible : Host domain := { ready with registry := replaceFuture ready.registry }

theorem old_chain_after_future_replacement :
    (beginCommit coordination compatible invocation ticket).2.isSome = true ∧
    ((wholeCommit compatible token).owners 0).value = 43 := by decide

def newDispatch : Host domain := { initial with registry := replaceFuture initial.registry }
def newReady := (prepare coordination newDispatch invocation participants reads calls).1

theorem new_dispatch_prepare_success :
    (prepare coordination newDispatch invocation participants reads calls).2 = some ticket := by decide

def newToken : PreparedToken domain := (newReady.pending ticket).get (by decide)

theorem new_dispatch_result : ((wholeCommit newReady newToken).owners 0).value = 84 := by decide

def malformedField : Host domain := { initial with owners := fun i =>
  if h : i = 2 then h.symm ▸ (⟨9, 2, 1, [1]⟩ : OwnerLocal Family 2) else initial.owners i }

theorem field_invariant_rejection : prepare coordination malformedField invocation participants reads calls =
    (malformedField, none) := by rfl

end EntityFree
end Witness

/-! Structural projection into Authority's homogeneous atomic model. Sigma values
are a mathematical encoding, never a proposed runtime payload database. Swaps are
last-write-wins; reversing the write list makes Authority.patch's first-match rule
agree. Issued tokens already prohibit duplicates, so this does not reorder events.
A full policy/loader/Rust refinement is deliberately not asserted. -/
section AuthorityProjection

def projectOwners (owners : Owners Payload) (a : Authority.Address) :
    Option (Authority.Cell (Sigma Payload)) :=
  if a.key = 0 then
    some ⟨(owners a.owner).generation, (owners a.owner).revision, ⟨a.owner, (owners a.owner).value⟩⟩
  else none

def abstractWrite (c : Candidate d view) : Authority.Write (Sigma Payload) :=
  ⟨{ c.stamp.dependency with stamp :=
      ⟨⟨⟨c.owner, 0⟩, c.old.generation⟩, c.old.revision⟩ }, some ⟨c.owner, c.value⟩⟩

def abstractPlan (t : PreparedToken d) : Authority.Plan (Sigma Payload) Event :=
  ⟨t.observations.map (fun o => o.stamp.dependency), t.writes.reverse.map abstractWrite, events t⟩

def projectAuthority (s : Host d) : Authority.State (Sigma Payload) Event :=
  ⟨s.registry.host, s.registry.callers, s.registry.providers, s.registry.grants,
    projectOwners s.owners, fun id => (s.pending id).map fun t =>
      ⟨t.invocation.caller, abstractPlan t⟩, s.nextSerial, s.history, s.epoch⟩

theorem patch_append (xs ys : List (Authority.Write Value)) (resources) :
    Authority.patch (xs ++ ys) resources = Authority.patch xs (Authority.patch ys resources) := by
  funext a
  induction xs with
  | nil => rfl
  | cons w ws ih =>
    simp only [List.cons_append, Authority.patch]
    split <;> simp_all

theorem project_swap (owners : Owners Payload) (c : Candidate d view) :
    projectOwners (swap owners c) = Authority.patch [abstractWrite c] (projectOwners owners) := by
  funext a
  rcases a with ⟨owner, key⟩
  by_cases hk : key = 0
  · subst key
    by_cases ho : owner = c.owner
    · subst owner
      simp [projectOwners, swap, abstractWrite, Authority.patch, Authority.writtenCell]
    · simp [projectOwners, swap, ho, Ne.symm ho, abstractWrite, Authority.patch,
        Authority.writtenCell]
  · simp [projectOwners, hk, Ne.symm hk, abstractWrite, Authority.patch]

theorem project_all_swaps (owners : Owners Payload) (writes : List (Candidate d view)) :
    projectOwners (swapAll owners writes) =
      Authority.patch (writes.reverse.map abstractWrite) (projectOwners owners) := by
  induction writes generalizing owners with
  | nil => rfl
  | cons c cs ih =>
    change projectOwners (swapAll (swap owners c) cs) = _
    rw [ih, project_swap, List.reverse_cons, List.map_append, patch_append]
    rfl

theorem current_local_stamps (h : current r owners c = true) :
    (owners c.owner).generation = c.generation ∧ (owners c.owner).revision = c.revision := by
  unfold current at h
  split at h
  · simp at h
  · simp only [Bool.and_eq_true, decide_eq_true_eq] at h
    exact ⟨h.1.2.2.2.2.1, h.1.2.2.2.2.2.1⟩

theorem projected_capture_exact (h : tokenValid policy s inv t = true) (hc : c ∈ t.writes) :
    (abstractWrite c).dependency = c.stamp.dependency := by
  have checks := preflight_writes h c hc
  have stamps := current_local_stamps checks.1
  rw [c.belongs, checks.2.1] at stamps
  simp [abstractWrite, Capture.dependency, c.belongs, stamps.1, stamps.2]

theorem publication_authority_apply (before : Host d) (t : PreparedToken d) :
    projectAuthority (materialize before t .opened) =
      Authority.applyPrepared (projectAuthority before) t.identity
        ⟨t.invocation.caller, abstractPlan t⟩ := by
  simp only [materialize, wholeCommit, projectAuthority, Authority.applyPrepared, abstractPlan]
  congr 1
  · exact project_all_swaps before.owners t.writes
  · funext id
    by_cases h : id = t.identity <;> simp [h]

/-- Additional Authority policy validation is an adapter obligation. The equality of
whole publication to applyPrepared above is proved without assuming that equality. -/
theorem publication_authority_commit (before : Host d) (t : PreparedToken d)
    (policy : Authority.Policy (Sigma Payload) Event)
    (issued : before.pending t.identity = some t)
    (adapter : Authority.commitValid policy (projectAuthority before) t.invocation.caller
      t.identity ⟨t.invocation.caller, abstractPlan t⟩ = true) :
    Authority.commit policy (projectAuthority before) t.invocation.caller t.identity =
      (projectAuthority (materialize before t .opened), true) := by
  rw [publication_authority_apply]
  have pending : (projectAuthority before).pending t.identity =
      some ⟨t.invocation.caller, abstractPlan t⟩ := by simp [projectAuthority, issued]
  simp only [Authority.commit, pending, adapter, Bool.true_eq, ite_true]

end AuthorityProjection

/-- Runtime query ports additionally authenticate the invocation and its read grants.
`externalObserve`/`readDerived` alone are mathematical host/cache views. -/
def derivedPort (s : Host d) (inv : Invocation) (cache : DerivedCache Payload Result) : Option Result :=
  if Authority.authority (authorityView s.registry) inv.caller &&
      s.registry.invocations.contains inv &&
      authorized s.registry inv cache.dependencies.query &&
      cache.dependencies.observations.all (fun o => authorized s.registry inv o.stamp) then
    readDerived s cache
  else none

theorem derived_port_closed (h : s.visible = false) : derivedPort s inv cache = none := by
  unfold derivedPort
  split <;> simp [closed_cache_refused h]

theorem derived_query_authority_refused
    (h : authorized s.registry inv cache.dependencies.query = false) :
    derivedPort s inv cache = none := by
  simp [derivedPort, h]

theorem derived_query_stale_refused
    (h : current s.registry s.owners cache.dependencies.query = false) :
    readDerived s cache = none := by
  simp [readDerived, dependenciesValid, h]

theorem valid_query_capture_current (h : dependenciesValid s deps = true) :
    current s.registry s.owners deps.query = true := by
  simp only [dependenciesValid, Bool.and_eq_true] at h
  exact h.1.1.2

theorem valid_query_requires_read (h : dependenciesValid s deps = true) :
    deps.query.access = .read := by
  simp only [dependenciesValid, Bool.and_eq_true, decide_eq_true_eq] at h
  exact h.1.2

def serviceRegistry (r : Registry) : Services.Registry :=
  ⟨r.host, r.callers, r.providers, r.grants⟩

/-- A timer's captured targets must exactly match its existing Services target vector.
Delivery admission changes only its queue. Its callback must subsequently prepare a
normal owner command, freshly checking invocation/lease/grants and captured code. -/
def deliverOwnerTimer (s : Host d) (policy : Services.Timer.Policy Time Residency Callback)
    (q : Services.Timer.Queue Time Residency Callback) (key : Services.Timer.Key)
    (candidate : Workflow.Pipeline) (captures : List Capture) :
    Services.Timer.Queue Time Residency Callback × Option (Services.Timer.Task Time Residency Callback) :=
  match q.tasks.find? (fun task => decide (task.key = key)) with
  | none => (q, none)
  | some task =>
    if s.visible && decide (captures.map (fun c => c.dependency.stamp) = task.targets) &&
        captures.all (current s.registry s.owners) then
      Services.Timer.deliver policy (serviceRegistry s.registry) (timerTargetCurrent s) q key candidate
    else (q, none)

theorem closed_timer_delivery (h : s.visible = false) :
    deliverOwnerTimer s policy q key candidate captures = (q, none) := by
  unfold deliverOwnerTimer
  split <;> simp [h]

theorem timer_delivery_refusal (h : (deliverOwnerTimer s policy q key candidate captures).2 = none) :
    (deliverOwnerTimer s policy q key candidate captures).1 = q := by
  unfold deliverOwnerTimer at h ⊢
  split
  · rfl
  · rename_i task found
    simp only [found] at h
    split
    · rename_i accepted
      apply Services.Timer.refusal_preserves_queue
      simpa only [accepted, ite_true] using h
    · rfl


section ProtocolCompletion
variable {policy : Coordination d} {s : Host d} {inv : Invocation}
  {id : Authority.Identity} {t : PreparedToken d}

theorem begin_materializes_first_phase (h : (beginCommit policy s inv id).2 = some t) :
    (beginCommit policy s inv id).1 = materialize s t (.swapping [] t.writes rfl) := by
  unfold beginCommit at h ⊢
  split
  · simp_all
  · split <;> simp_all [materialize, swapAll]

theorem opening_requires_completed_metadata (before : Host d)
    (h : PublicationStep t phase .opened) :
    phase = .release ∧ (materialize before t phase).pending t.identity = none ∧
      (materialize before t phase).history = before.history ++ events t ∧
      (materialize before t phase).epoch = before.epoch + 1 := by
  cases h
  simp [materialize, wholeCommit]

theorem preparation_serial_advances (h : (prepare policy s inv p reads calls).2 = some id) :
    (prepare policy s inv p reads calls).1.nextSerial = s.nextSerial + 1 := by
  unfold prepare at h ⊢
  repeat' (first | split | simp_all)

end ProtocolCompletion

namespace Witness

theorem retained_pin_after_future_change :
    unload Transfer.compatible.registry oldExtension = (Transfer.compatible.registry, false) := by rfl

def foreignGate : Host Transfer.domain := { Transfer.ready with registry :=
  { Transfer.ready.registry with routes := Transfer.ready.registry.routes.map fun p => { p with gate := 8 } } }

theorem foreign_gate_rejection : beginCommit Transfer.coordination foreignGate invocation ticket =
    (foreignGate, none) := by rfl

def unsupportedBackend : Host Transfer.domain := { Transfer.ready with registry :=
  { Transfer.ready.registry with routes := Transfer.ready.registry.routes.map fun p => { p with sealedBackend := false } } }

theorem callback_backend_rejection : beginCommit Transfer.coordination unsupportedBackend invocation ticket =
    (unsupportedBackend, none) := by rfl

def inputPolicyChange : Host Equipment.domain := { Equipment.initial with registry :=
  { Equipment.initial.registry with routes := Equipment.initial.registry.routes.map fun p =>
      { p with future := { p.future with inputRecord := 30, policy := 18 } } } }

theorem derived_input_policy_rejection : readDerived inputPolicyChange Equipment.oldCache = none := by decide

theorem explicitly_noncacheable_rejection : readDerived Equipment.initial
    { Equipment.oldCache with dependencies := { Equipment.oldCache.dependencies with cacheable := false } } = none := by decide

end Witness

-- Build evidence checks foundational dependencies, including the constructive fixtures.
#print axioms publication_completes
#print axioms publication_authority_apply
#print axioms Witness.Transfer.transfer_result
#print axioms Witness.Retirement.retained_accounting_fields
#print axioms Witness.Equipment.derived_recomputed_result
#print axioms Witness.EntityFree.scalar_field_result


namespace Witness

def reverseEventPolicy : Coordination Transfer.domain :=
  { Transfer.coordination with
    order := fun _ writes => writes.reverse
    orderExact := fun _ writes => List.reverse_perm writes }

def reverseEventReady := (prepare reverseEventPolicy Transfer.initial invocation
  Transfer.participants Transfer.reads Transfer.calls).1

theorem reverse_event_prepare_success : (prepare reverseEventPolicy Transfer.initial invocation
    Transfer.participants Transfer.reads Transfer.calls).2 = some ticket := by decide

def reverseEventToken : PreparedToken Transfer.domain :=
  (reverseEventReady.pending ticket).get (by decide)

theorem independent_event_order_result :
    reverseEventToken.participants.writes = [0, 1, 2] ∧
    (wholeCommit reverseEventReady reverseEventToken).history = ["prior", "locate", "receive", "remove"] ∧
    ((wholeCommit reverseEventReady reverseEventToken).owners 2).value = some 1 := by decide

end Witness

namespace Witness.QuerySafety

/-- The query owner is deliberately absent from the observed property inputs. -/
def queryCapture : Capture :=
  (capture Equipment.finished.registry Equipment.finished.owners 2 queryCap .read).get (by decide)

def propertyCache : DerivedCache Equipment.Family Nat :=
  ⟨⟨Equipment.newCache.dependencies.observations, queryCapture, true⟩,
    Equipment.query, 7, by decide⟩

theorem query_owner_not_observed :
    propertyCache.dependencies.observations.all (fun o => decide (o.owner ≠ 2)) = true := by decide

theorem authorized_query_success :
    derivedPort Equipment.finished invocation propertyCache = some 7 := by decide

def revoked : Host Equipment.domain :=
  { Equipment.finished with registry := { Equipment.finished.registry with
    grants := Equipment.finished.registry.grants.filter (fun g => decide (g.provider.owner ≠ 2)) } }

def narrowed : Host Equipment.domain :=
  { Equipment.finished with registry := { Equipment.finished.registry with
    grants := Equipment.finished.registry.grants.filter (fun g =>
      decide (g.provider.owner ≠ 2 ∨ g.capability ≠ queryCap)) } }

theorem revoked_query_refusal : derivedPort revoked invocation propertyCache = none := by decide
theorem narrowed_query_refusal : derivedPort narrowed invocation propertyCache = none := by decide
theorem dependency_grants_survive_narrowing :
    propertyCache.dependencies.observations.all (fun o => authorized narrowed.registry invocation o.stamp) = true := by decide

def changedOwner (generation revision schema : Nat) : Host Equipment.domain :=
  { Equipment.finished with
    owners := fun i => if h : i = 2 then
      h.symm ▸ { Equipment.finished.owners 2 with generation, revision, schema }
      else Equipment.finished.owners i
    registry := { Equipment.finished.registry with routes := Equipment.finished.registry.routes.map (fun p => if p.owner = 2 then
        { p with generation, schema } else p) } }

def recreated := changedOwner (queryCapture.generation + 1) queryCapture.revision queryCapture.schema
def revised := changedOwner queryCapture.generation (queryCapture.revision + 1) queryCapture.schema
def reschemed := changedOwner queryCapture.generation queryCapture.revision (queryCapture.schema + 1)

theorem recreated_query_refusal :
    readDerived recreated propertyCache = none ∧ derivedPort recreated invocation propertyCache = none := by decide
theorem revised_query_refusal :
    readDerived revised propertyCache = none ∧ derivedPort revised invocation propertyCache = none := by decide
theorem schema_query_refusal :
    readDerived reschemed propertyCache = none ∧ derivedPort reschemed invocation propertyCache = none := by decide

def foreignGate : Host Equipment.domain :=
  { Equipment.finished with registry := { Equipment.finished.registry with routes := Equipment.finished.registry.routes.map (fun p => if p.owner = 2 then { p with gate := 8 } else p) } }

def unsupported : Host Equipment.domain :=
  { Equipment.finished with registry := { Equipment.finished.registry with routes := Equipment.finished.registry.routes.map (fun p => if p.owner = 2 then { p with sealedBackend := false } else p) } }

theorem query_gate_refusal : derivedPort foreignGate invocation propertyCache = none := by decide
theorem query_backend_refusal : derivedPort unsupported invocation propertyCache = none := by decide

end Witness.QuerySafety

end NetMechanics.Ownership
