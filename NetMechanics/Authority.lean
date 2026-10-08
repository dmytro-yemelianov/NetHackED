import Std

/-!
# M1 / S1 + S3: host authority and immutable prepared transactions

World-independent specification, not a plugin loader or a production/C proof.
The trusted adapter authenticates the invocation's host, principal and exact module
artifact; callers cannot supply these as self-attested credentials. Registry,
bindings, grants, resources and pending plans are authoritative host-owned state.
Artifact tokens must bind injectively to actual loaded code (including version and
configuration); replacing code must replace its binding. These are runtime trust
obligations, not cryptography or guest-memory isolation proved by this model.

Owners supply sound validation/accounting predicates. They see only declared
participant snapshots, never a universal context. A pure transition represents
one atomic host operation; the runtime must serialize it, including revalidation
and event publication, with no callbacks or partial cleanup between checks.

Host namespaces and serials must be allocated injectively and never reused across
restart/restore. `Allocated` expresses the pending-state allocation invariant;
generation tokens must likewise identify distinct incarnations, with revisions
changing on every mutation. All counters are unbounded `Nat`. A bounded runtime
must check overflow BEFORE prepare/commit and refuse without changes, or use an
equivalent non-reusing representation. No such machine-integer refinement is claimed.
-/

namespace NetMechanics.Authority

structure Artifact where
  moduleName : String
  version : Nat
  code : Nat
  deriving Repr, DecidableEq, BEq

structure Caller where
  host : Nat
  principal : Nat
  artifact : Artifact
  deriving Repr, DecidableEq, BEq

structure Capability where
  name : String
  version : Nat
  deriving Repr, DecidableEq, BEq

structure Provider where
  owner : Nat
  artifact : Artifact
  exports : List Capability
  deriving Repr, DecidableEq, BEq

inductive Access where
  | read | write
  deriving Repr, DecidableEq, BEq

structure Grant where
  caller : Caller
  provider : Provider
  capability : Capability
  access : Access
  deriving Repr, DecidableEq, BEq

structure Address where
  owner : Nat
  key : Nat
  deriving Repr, DecidableEq, BEq

structure Handle where
  address : Address
  generation : Nat
  deriving Repr, DecidableEq, BEq

structure Stamp where
  handle : Handle
  revision : Nat
  deriving Repr, DecidableEq, BEq

structure Cell (Payload : Type) where
  generation : Nat
  revision : Nat
  value : Payload
  deriving Repr, DecidableEq

structure Dependency where
  stamp : Stamp
  provider : Provider
  capability : Capability
  deriving Repr, DecidableEq, BEq

structure Write (Payload : Type) where
  dependency : Dependency
  replacement : Option Payload
  deriving Repr, DecidableEq

structure Plan (Payload Event : Type) where
  readonly : List Dependency
  writes : List (Write Payload)
  events : List Event
  deriving Repr, DecidableEq

structure Identity where
  host : Nat
  serial : Nat
  deriving Repr, DecidableEq, BEq

structure Prepared (Payload Event : Type) where
  caller : Caller
  plan : Plan Payload Event
  deriving Repr, DecidableEq

structure State (Payload Event : Type) where
  host : Nat
  bindings : List Caller
  providers : List Provider
  grants : List Grant
  resources : Address → Option (Cell Payload)
  pending : Identity → Option (Prepared Payload Event)
  nextSerial : Nat
  history : List Event
  commits : Nat

/-- Explicit external obligations; these hypotheses are not manufactured by checks. -/
structure RuntimeTrust (s : State Payload Event) (invocation : Caller)
    (authenticCaller : Caller → Prop) (authenticProvider : Provider → Prop)
    (authorizedGrant : Grant → Prop) : Prop where
  invocationAuthenticated : authenticCaller invocation
  currentCodeBinding : ∀ c ∈ s.bindings, authenticCaller c
  registeredProviderBinding : ∀ p ∈ s.providers, authenticProvider p
  issuedGrants : ∀ g ∈ s.grants, authorizedGrant g

abbrev Snapshot (Payload : Type) := List (Handle × Option Payload)

/-- Soundness is supplied by the trusted owners, not inferred from arbitrary code. -/
structure Policy (Payload Event : Type) where
  maxParticipants : Nat
  ownerCheck : Nat → Write Payload → Option Payload → Bool
  ownerValid : Nat → Write Payload → Option Payload → Prop
  ownerSound : ∀ o w old, ownerCheck o w old = true → ownerValid o w old
  accountingCheck : Snapshot Payload → Plan Payload Event → Bool
  accountingValid : Snapshot Payload → Plan Payload Event → Prop
  accountingSound : ∀ values plan,
    accountingCheck values plan = true → accountingValid values plan

def authority (s : State Payload Event) (c : Caller) : Bool :=
  c.host == s.host && s.bindings.contains c

def permitted (s : State Payload Event) (c : Caller) (d : Dependency)
    (access : Access) : Bool :=
  d.provider.owner == d.stamp.handle.address.owner &&
  s.providers.contains d.provider &&
  s.bindings.contains ⟨s.host, d.provider.owner, d.provider.artifact⟩ &&
  d.provider.exports.contains d.capability &&
  s.grants.contains ⟨c, d.provider, d.capability, access⟩

def current (s : State Payload Event) (stamp : Stamp) : Bool :=
  match s.resources stamp.handle.address with
  | none => false
  | some cell => cell.generation == stamp.handle.generation && cell.revision == stamp.revision

def oldValue (s : State Payload Event) (d : Dependency) : Option Payload :=
  (s.resources d.stamp.handle.address).map Cell.value

def participants (p : Plan Payload Event) : List Dependency :=
  p.readonly ++ p.writes.map Write.dependency

def snapshot (s : State Payload Event) (p : Plan Payload Event) : Snapshot Payload :=
  (participants p).map fun d => (d.stamp.handle, oldValue s d)

def shape (policy : Policy Payload Event) (p : Plan Payload Event) : Bool :=
  decide (((participants p).map fun d => d.stamp.handle.address).Nodup ∧
    (participants p).length ≤ policy.maxParticipants)

/-- Every check precedes every mutation, including cleanup and event emission. -/
def valid (policy : Policy Payload Event) (s : State Payload Event) (c : Caller)
    (p : Plan Payload Event) : Bool :=
  authority s c &&
  ((participants p).all (fun d => current s d.stamp) &&
  (p.readonly.all (fun d => permitted s c d .read) &&
  (p.writes.all (fun w => permitted s c w.dependency .write) &&
  (shape policy p &&
  (p.writes.all (fun w => policy.ownerCheck w.dependency.provider.owner w
    (oldValue s w.dependency)) && policy.accountingCheck (snapshot s p) p)))))

def fresh (s : State Payload Event) : Identity := ⟨s.host, s.nextSerial⟩

/-- The host stores the exact plan; preparing publishes no events or resource changes. -/
def prepare (policy : Policy Payload Event) (s : State Payload Event) (c : Caller)
    (p : Plan Payload Event) : State Payload Event × Option Identity :=
  if valid policy s c p && (s.pending (fresh s)).isNone then
    ({ s with pending := fun id => if id = fresh s then some ⟨c, p⟩ else s.pending id
              nextSerial := s.nextSerial + 1 }, some (fresh s))
  else (s, none)

def writtenCell (w : Write Payload) : Option (Cell Payload) :=
  w.replacement.map fun value =>
    ⟨w.dependency.stamp.handle.generation, w.dependency.stamp.revision + 1, value⟩

/-- One patch per address; shape validation prohibits duplicate participants. -/
def patch (writes : List (Write Payload))
    (resources : Address → Option (Cell Payload)) (a : Address) : Option (Cell Payload) :=
  match writes with
  | [] => resources a
  | w :: ws =>
    if w.dependency.stamp.handle.address = a then writtenCell w else patch ws resources a

def applyPrepared (s : State Payload Event) (id : Identity) (p : Prepared Payload Event) :
    State Payload Event :=
  { s with resources := patch p.plan.writes s.resources
           pending := fun other => if other = id then none else s.pending other
           history := s.history ++ p.plan.events
           commits := s.commits + 1 }

def commitValid (policy : Policy Payload Event) (s : State Payload Event) (c : Caller)
    (id : Identity) (p : Prepared Payload Event) : Bool :=
  id.host == s.host && (decide (c = p.caller) && valid policy s c p.plan)

/-- Commit takes only the identity, never a caller-supplied replacement plan. -/
def commit (policy : Policy Payload Event) (s : State Payload Event) (c : Caller)
    (id : Identity) : State Payload Event × Bool :=
  match s.pending id with
  | none => (s, false)
  | some p =>
    if commitValid policy s c id p then (applyPrepared s id p, true) else (s, false)

section Laws
variable {Payload Event : Type} {policy : Policy Payload Event} {s : State Payload Event}
  {c : Caller} {id : Identity} {writes : List (Write Payload)}
  {resources : Address → Option (Cell Payload)} {w : Write Payload} {a : Address}

theorem valid_authority (h : valid policy s c p = true) : authority s c = true := by
  simp only [valid, Bool.and_eq_true] at h
  exact h.1

theorem valid_stamps (h : valid policy s c p = true) :
    ∀ d ∈ participants p, current s d.stamp = true := by
  simp only [valid, Bool.and_eq_true, List.all_eq_true] at h
  exact h.2.1

theorem valid_shape (h : valid policy s c p = true) :
    ((participants p).map fun d => d.stamp.handle.address).Nodup ∧
      (participants p).length ≤ policy.maxParticipants := by
  simp only [valid, Bool.and_eq_true] at h
  simpa only [shape, decide_eq_true_eq] using h.2.2.2.2.1

theorem valid_owner_obligations (h : valid policy s c p = true) :
    ∀ w ∈ p.writes, policy.ownerValid w.dependency.provider.owner w
      (oldValue s w.dependency) := by
  simp only [valid, Bool.and_eq_true, List.all_eq_true] at h
  intro w hw
  exact policy.ownerSound _ _ _ (h.2.2.2.2.2.1 w hw)

theorem valid_accounting (h : valid policy s c p = true) :
    policy.accountingValid (snapshot s p) p := by
  simp only [valid, Bool.and_eq_true] at h
  exact policy.accountingSound _ _ h.2.2.2.2.2.2

theorem prepare_refusal (h : (prepare policy s c p).2 = none) :
    (prepare policy s c p).1 = s := by
  unfold prepare at h ⊢
  split <;> simp_all

theorem prepare_invalid (h : valid policy s c p = false) :
    prepare policy s c p = (s, none) := by
  simp [prepare, h]

theorem prepare_exact (h : (prepare policy s c p).2 = some id) :
    id = fresh s ∧ valid policy s c p = true ∧
    (prepare policy s c p).1.pending id = some ⟨c, p⟩ ∧
    (prepare policy s c p).1.resources = s.resources ∧
    (prepare policy s c p).1.history = s.history ∧
    (prepare policy s c p).1.commits = s.commits := by
  unfold prepare at *
  split at h <;> simp_all

theorem commit_refusal (h : (commit policy s c id).2 = false) :
    (commit policy s c id).1 = s := by
  cases hp : s.pending id with
  | none => simp [commit, hp]
  | some p =>
    by_cases hv : commitValid policy s c id p = true
    · simp [commit, hp, hv] at h
    · simp [commit, hp, hv]

theorem commit_success (h : (commit policy s c id).2 = true) :
    ∃ p, s.pending id = some p ∧ commitValid policy s c id p = true ∧
      (commit policy s c id).1 = applyPrepared s id p := by
  cases hp : s.pending id with
  | none => simp [commit, hp] at h
  | some p =>
    by_cases hv : commitValid policy s c id p = true
    · exact ⟨p, rfl, hv, by simp [commit, hp, hv]⟩
    · simp [commit, hp, hv] at h

theorem commit_valid_plan (h : commitValid policy s c id p = true) :
    id.host = s.host ∧ c = p.caller ∧ valid policy s c p.plan = true := by
  simpa only [commitValid, Bool.and_eq_true, beq_iff_eq, decide_eq_true_eq] using h

theorem commit_authority (h : (commit policy s c id).2 = true) :
    authority s c = true := by
  obtain ⟨p, _, hv, _⟩ := commit_success h
  exact valid_authority (commit_valid_plan hv).2.2

theorem authority_refusal (h : authority s c = false) :
    prepare policy s c plan = (s, none) ∧ commit policy s c id = (s, false) := by
  constructor
  · simp [prepare, valid, h]
  · cases hp : s.pending id <;> simp [commit, hp, commitValid, valid, h]

theorem different_caller_refusal (hp : s.pending id = some p) (h : c ≠ p.caller) :
    commit policy s c id = (s, false) := by
  simp [commit, hp, commitValid, h]

theorem patch_unrelated (h : ∀ w ∈ writes, w.dependency.stamp.handle.address ≠ a) :
    patch writes resources a = resources a := by
  induction writes with
  | nil => rfl
  | cons w ws ih =>
    simp only [List.mem_cons, forall_eq_or_imp] at h
    simp [patch, h.1, ih h.2]

theorem patch_written (unique : (writes.map fun w => w.dependency.stamp.handle.address).Nodup)
    (hw : w ∈ writes) : patch writes resources w.dependency.stamp.handle.address = writtenCell w := by
  induction writes with
  | nil => simp at hw
  | cons head tail ih =>
    simp only [List.map_cons, List.nodup_cons] at unique
    simp only [List.mem_cons] at hw
    rcases hw with heq | htail
    · subst w
      simp [patch]
    · have hne : head.dependency.stamp.handle.address ≠ w.dependency.stamp.handle.address := by
        intro heq
        apply unique.1
        rw [heq]
        exact List.mem_map.mpr ⟨w, htail, rfl⟩
      simp [patch, hne, ih unique.2 htail]

theorem successful_writes (h : (commit policy s c id).2 = true) :
    ∃ p, s.pending id = some p ∧ ∀ w ∈ p.plan.writes,
      (commit policy s c id).1.resources w.dependency.stamp.handle.address = writtenCell w := by
  obtain ⟨p, hp, hv, hs⟩ := commit_success h
  have hu := (valid_shape (commit_valid_plan hv).2.2).1
  have huw : (p.plan.writes.map fun w => w.dependency.stamp.handle.address).Nodup := by
    simp only [participants, List.map_append, List.map_map, List.nodup_append] at hu
    exact hu.2.1
  refine ⟨p, hp, ?_⟩
  intro w hw
  rw [hs]
  exact patch_written huw hw

theorem successful_frame (h : (commit policy s c id).2 = true)
    (frame : ∀ p, s.pending id = some p →
      ∀ w ∈ p.plan.writes, w.dependency.stamp.handle.address ≠ a) :
    (commit policy s c id).1.resources a = s.resources a := by
  obtain ⟨p, hp, _, hs⟩ := commit_success h
  rw [hs]
  exact patch_unrelated (frame p hp)

theorem successful_history (h : (commit policy s c id).2 = true) :
    ∃ p, s.pending id = some p ∧
      (commit policy s c id).1.history = s.history ++ p.plan.events ∧
      (commit policy s c id).1.commits = s.commits + 1 ∧
      (commit policy s c id).1.nextSerial = s.nextSerial ∧
      (commit policy s c id).1.bindings = s.bindings ∧
      (commit policy s c id).1.providers = s.providers ∧
      (commit policy s c id).1.grants = s.grants := by
  obtain ⟨p, hp, _, hs⟩ := commit_success h
  exact ⟨p, hp, by simp [hs, applyPrepared]⟩

theorem replay_refusal (h : (commit policy s c id).2 = true) (nextCaller : Caller) :
    commit policy (commit policy s c id).1 nextCaller id =
      ((commit policy s c id).1, false) := by
  obtain ⟨p, _, _, hs⟩ := commit_success h
  rw [hs]
  simp [applyPrepared, commit]

/-- A fresh serial is greater than all identities still pending in this host. -/
def Allocated (s : State Payload Event) : Prop :=
  ∀ id p, s.pending id = some p → id.host = s.host ∧ id.serial < s.nextSerial

theorem fresh_not_pending (h : Allocated s) : s.pending (fresh s) = none := by
  cases hp : s.pending (fresh s) with
  | none => rfl
  | some p =>
    have hn := (h _ _ hp).2
    simp [fresh] at hn

theorem prepare_allocation (allocated : Allocated s) :
    Allocated (prepare policy s c plan).1 := by
  unfold prepare
  split
  · intro id p hp
    simp only at hp ⊢
    by_cases hi : id = fresh s
    · subst id
      simp [fresh]
    · simp only [hi, if_false] at hp
      obtain ⟨hh, hn⟩ := allocated id p hp
      exact ⟨hh, Nat.lt_trans hn (Nat.lt_succ_self _)⟩
  · exact allocated

theorem commit_allocation (allocated : Allocated s) :
    Allocated (commit policy s c id).1 := by
  cases hp : s.pending id with
  | none => simpa [commit, hp] using allocated
  | some p =>
    by_cases hv : commitValid policy s c id p = true
    · simp only [commit, hp, hv, if_true]
      intro other q hq
      by_cases hi : other = id
      · simp [applyPrepared, hi] at hq
      · exact allocated other q (by simpa [applyPrepared, hi] using hq)
    · simpa [commit, hp, hv] using allocated

theorem identity_injective (a b : Identity) (host : a.host = b.host)
    (serial : a.serial = b.serial) : a = b := by
  cases a
  cases b
  simp_all

theorem prepare_serial_increases (h : (prepare policy s c p).2 = some id) :
    id.serial < (prepare policy s c p).1.nextSerial := by
  unfold prepare at *
  split at h
  · simp only [Prod.snd, Option.some.injEq] at h
    subst id
    simp_all [fresh]
  · simp at h

end Laws

/-! The following finite fixtures model retained lifetime/billing facts only.
They are not a second domain implementation or evidence of production correspondence. -/
namespace Witness

def cap : Capability := ⟨"retire", 1⟩
def caller : Caller := ⟨42, 100, ⟨"lifetime", 1, 700⟩⟩
def provider (owner : Nat) : Provider := ⟨owner, ⟨"owner", 1, owner + 800⟩, [cap]⟩
def address (owner : Nat) : Address := ⟨owner, 0⟩
def handle (owner : Nat) : Handle := ⟨address owner, 9⟩
def dependency (owner : Nat) : Dependency := ⟨⟨handle owner, 2⟩, provider owner, cap⟩

structure Debt where
  origin : Handle
  quantity : Nat
  terms : Nat
  creditor : Nat
  liability : Nat
  consumed : Bool
  deriving Repr, DecidableEq, BEq

inductive Value where
  | item (quantity : Nat) (charges : Int)
  | equipment (target : Handle)
  | timer (target : Handle)
  | debt (record : Debt)
  | terms (id : Nat)
  | unrelated (value : Nat)
  deriving Repr, DecidableEq, BEq

def debt : Debt := ⟨handle 1, 3, 17, 91, 120, false⟩

def plan : Plan Value String :=
  { readonly := [dependency 5]
    writes := [⟨dependency 1, none⟩, ⟨dependency 2, none⟩, ⟨dependency 3, none⟩,
               ⟨dependency 4, some (.debt { debt with consumed := true })⟩]
    events := ["dust", "retired"] }

def ownerCheck (owner : Nat) (w : Write Value) (old : Option Value) : Bool :=
  match owner, old, w.replacement with
  | 1, some (.item quantity charges), none => quantity > 0 && charges < 0
  | 2, some (.equipment target), none => target == handle 1
  | 3, some (.timer target), none => target == handle 1
  | 4, some (.debt before), some (.debt after) =>
    !before.consumed && before.origin == handle 1 && after == { before with consumed := true }
  | _, _, _ => false

/-- Cross-owner accounting uses only the declared, stamped snapshots. -/
def accountingCheck (values : Snapshot Value) (_ : Plan Value String) : Bool :=
  match values.map Prod.snd with
  | [some (.terms terms), some (.item quantity _), some (.equipment _),
      some (.timer _), some (.debt record)] =>
    quantity == record.quantity && terms == record.terms && record.liability > 0
  | _ => false

def policy : Policy Value String :=
  { maxParticipants := 5
    ownerCheck := ownerCheck
    ownerValid := fun o w old => ownerCheck o w old = true
    ownerSound := fun _ _ _ h => h
    accountingCheck := accountingCheck
    accountingValid := fun values p => accountingCheck values p = true
    accountingSound := fun _ _ h => h }

def resources (a : Address) : Option (Cell Value) :=
  if a.key ≠ 0 then none else
    (match a.owner with
      | 1 => some (.item 3 (-1))
      | 2 => some (.equipment (handle 1))
      | 3 => some (.timer (handle 1))
      | 4 => some (.debt debt)
      | 5 => some (.terms 17)
      | 6 => some (.unrelated 88)
      | _ => none : Option Value).map fun value => ⟨9, 2, value⟩

def initial : State Value String :=
  { host := 42
    bindings := caller :: ([1, 2, 3, 4, 5].map fun o => ⟨42, o, (provider o).artifact⟩)
    providers := [1, 2, 3, 4, 5].map provider
    grants := ([1, 2, 3, 4].map fun o => ⟨caller, provider o, cap, .write⟩) ++
      [⟨caller, provider 5, cap, .read⟩]
    resources := resources
    pending := fun _ => none
    nextSerial := 0
    history := ["prior"]
    commits := 0 }

def id : Identity := ⟨42, 0⟩
def ready : State Value String := (prepare policy initial caller plan).1
def finished : State Value String := (commit policy ready caller id).1

theorem prepare_witness : (prepare policy initial caller plan).2 = some id := by decide
theorem stored_exact_witness : ready.pending id = some ⟨caller, plan⟩ := by decide
theorem commit_witness : (commit policy ready caller id).2 = true := by decide

theorem retirement_witness :
    finished.resources (address 1) = none ∧
    finished.resources (address 2) = none ∧
    finished.resources (address 3) = none ∧
    finished.resources (address 4) = some ⟨9, 3, .debt { debt with consumed := true }⟩ ∧
    finished.resources (address 5) = initial.resources (address 5) ∧
    finished.resources (address 6) = initial.resources (address 6) ∧
    finished.history = ["prior", "dust", "retired"] ∧ finished.commits = 1 := by decide

theorem consumed_debt_witness :
    ({ debt with consumed := true } : Debt).quantity = 3 ∧
    ({ debt with consumed := true } : Debt).terms = 17 ∧
    ({ debt with consumed := true } : Debt).creditor = 91 ∧
    ({ debt with consumed := true } : Debt).liability = 120 ∧
    ({ debt with consumed := true } : Debt).origin = handle 1 := by decide

theorem replay_witness : commit policy finished caller id = (finished, false) := by rfl

def wrongHost : Caller := { caller with host := 43 }
def wrongCode : Caller := { caller with artifact := { caller.artifact with code := 701 } }

theorem wrong_host_prepare_witness :
    prepare policy initial wrongHost plan = (initial, none) := by rfl
theorem wrong_host_commit_witness :
    commit policy ready wrongHost id = (ready, false) := by rfl
theorem wrong_code_prepare_witness :
    prepare policy initial wrongCode plan = (initial, none) := by rfl
theorem wrong_code_commit_witness :
    commit policy ready wrongCode id = (ready, false) := by rfl

def revoked : State Value String := { ready with grants := [] }
def replaced : State Value String :=
  { ready with bindings := wrongCode :: ready.bindings.tail }

theorem grant_revoked_witness : commit policy revoked caller id = (revoked, false) := by rfl
theorem code_replaced_witness : commit policy replaced caller id = (replaced, false) := by rfl
theorem replacement_cannot_adopt_witness :
    commit policy replaced wrongCode id = (replaced, false) := by rfl

def wrongGrant : State Value String :=
  { initial with grants := initial.grants.map fun g => { g with access := .read } }
theorem read_grant_cannot_write_witness :
    prepare policy wrongGrant caller plan = (wrongGrant, none) := by rfl

def wrongVersion : State Value String :=
  { initial with grants := initial.grants.map fun g =>
      { g with capability := { cap with version := 2 } } }
theorem named_versioned_grant_witness :
    prepare policy wrongVersion caller plan = (wrongVersion, none) := by rfl

def providerRemoved : State Value String :=
  { ready with providers := ready.providers.filter fun p => p.owner != 3 }
theorem unregistered_provider_witness :
    commit policy providerRemoved caller id = (providerRemoved, false) := by rfl

def generationReused : State Value String :=
  { ready with resources := fun a =>
      if a = address 1 then some ⟨10, 2, .item 3 (-1)⟩ else ready.resources a }
theorem reused_generation_witness :
    commit policy generationReused caller id = (generationReused, false) := by rfl

def staleWrite : State Value String :=
  { ready with resources := fun a =>
      if a = address 4 then some ⟨9, 3, .debt debt⟩ else ready.resources a }
theorem stale_write_revision_witness :
    commit policy staleWrite caller id = (staleWrite, false) := by rfl

def staleReadonly : State Value String :=
  { ready with resources := fun a =>
      if a = address 5 then some ⟨9, 3, .terms 17⟩ else ready.resources a }
theorem stale_readonly_revision_witness :
    commit policy staleReadonly caller id = (staleReadonly, false) := by rfl

def badAccounting : State Value String :=
  { initial with resources := fun a =>
      if a = address 4 then some ⟨9, 2, .debt { debt with quantity := 4 }⟩
      else initial.resources a }
theorem accounting_refusal_before_cleanup_witness :
    prepare policy badAccounting caller plan = (badAccounting, none) := by rfl

def badOwnerPlan : Plan Value String :=
  { plan with writes := [⟨dependency 1, none⟩, ⟨dependency 2, some (.unrelated 0)⟩,
      ⟨dependency 3, none⟩, ⟨dependency 4, some (.debt { debt with consumed := true })⟩] }
theorem owner_refusal_before_cleanup_witness :
    prepare policy initial caller badOwnerPlan = (initial, none) := by rfl

theorem duplicate_participant_witness :
    prepare policy initial caller { plan with readonly := [dependency 5, dependency 5] } =
      (initial, none) := by rfl
theorem participant_bound_witness :
    prepare { policy with maxParticipants := 4 } initial caller plan = (initial, none) := by rfl

end Witness
end NetMechanics.Authority
