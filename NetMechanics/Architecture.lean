import Std

/-!
# Kernel → modules → extensions

An architectural specification, independent of the existing NetHack AST.
The kernel owns registration, not game rules or a mandatory world schema.
Rules are relations: determinism is an optional property of a profile.
`Scope` describes the authority a host grants, including a protected frame.
This is not a verified Rust implementation or an executable plugin loader.
-/

namespace NetMechanics.Architecture

structure Capability where
  name : String
  major : Nat
  deriving Repr, BEq, DecidableEq

structure ModuleManifest where
  id : String
  version : Nat := 1
  dependsOn : List String := []
  requires : List Capability := []
  provides : List Capability := []
  extensionPoints : List String := []
  deriving Repr, BEq, DecidableEq

structure ExtensionManifest where
  id : String
  version : Nat := 1
  target : String
  point : String
  after : List String := []
  requires : List Capability := []
  deriving Repr, BEq, DecidableEq

/-- No hero, map, entity arena, RNG, clock or gameplay command is mandatory. -/
structure Kernel where
  baseCapabilities : List Capability := []
  modules : List ModuleManifest := []
  extensions : List ExtensionManifest := []
  atBoundary : Bool := true
  deriving Repr, BEq, DecidableEq

def available (k : Kernel) : List Capability :=
  k.baseCapabilities ++ k.modules.flatMap ModuleManifest.provides

inductive InstallError where
  | busy | duplicateId | missingDependency | missingCapability | providerConflict
  | unknownExtensionPoint
  deriving Repr, BEq, DecidableEq

/-- Dependencies must already be installed; installation order is topological. -/
def installModule (k : Kernel) (m : ModuleManifest) : Except InstallError Kernel :=
  if !k.atBoundary then .error .busy
  else if k.modules.any (fun x => x.id == m.id) then .error .duplicateId
  else if !(m.dependsOn.all fun id => k.modules.any (fun x => x.id == id)) then
    .error .missingDependency
  else if !(m.requires.all fun cap => (available k).contains cap) then
    .error .missingCapability
  else if m.provides.length != m.provides.eraseDups.length ||
      m.provides.any (fun cap => (available k).contains cap) then
    .error .providerConflict
  else .ok { k with modules := k.modules ++ [m] }

/-- Extensions attach to an existing module and cannot silently replace providers. -/
def installExtension (k : Kernel) (e : ExtensionManifest) : Except InstallError Kernel :=
  if !k.atBoundary then .error .busy
  else if k.extensions.any (fun x => x.id == e.id) then .error .duplicateId
  else if !(k.modules.any (fun x => x.id == e.target)) ||
      !(e.after.all fun id => k.extensions.any (fun x => x.id == id)) then
    .error .missingDependency
  else if !(k.modules.any (fun x => x.id == e.target && x.extensionPoints.contains e.point)) then
    .error .unknownExtensionPoint
  else if !(e.requires.all fun cap => (available k).contains cap) then
    .error .missingCapability
  else .ok { k with extensions := k.extensions ++ [e] }

/-- A host supplies an arbitrary state type, including an entity-free one. -/
structure Session (State : Type) where
  kernel : Kernel
  state : State

def attachModule (s : Session State) (m : ModuleManifest) :
    Except InstallError (Session State) :=
  match installModule s.kernel m with
  | .error e => .error e
  | .ok k => .ok { s with kernel := k }

def attachExtension (s : Session State) (e : ExtensionManifest) :
    Except InstallError (Session State) :=
  match installExtension s.kernel e with
  | .error err => .error err
  | .ok k => .ok { s with kernel := k }

theorem attach_module_preserves_state (s s' : Session State) (m : ModuleManifest)
    (h : attachModule s m = .ok s') : s'.state = s.state := by
  cases hk : installModule s.kernel m with
  | error err => simp [attachModule, hk] at h
  | ok k =>
    simp [attachModule, hk] at h
    subst s'
    rfl

theorem attach_extension_preserves_state (s s' : Session State) (e : ExtensionManifest)
    (h : attachExtension s e = .ok s') : s'.state = s.state := by
  cases hk : installExtension s.kernel e with
  | error err => simp [attachExtension, hk] at h
  | ok k =>
    simp [attachExtension, hk] at h
    subst s'
    rfl

/--
A host-selected local view. `Frame` is everything this view must preserve.
The laws are obligations on the adapter, not assertions about arbitrary Rust code.
The host must choose a meaningful frame; `Frame = Unit` protects nothing.
-/
structure Scope (Global Local : Type) where
  Frame : Type
  read : Global → Local
  write : Global → Local → Global
  frame : Global → Frame
  read_write : ∀ s fragment, read (write s fragment) = fragment
  frame_write : ∀ s fragment, frame (write s fragment) = frame s
  write_read : ∀ s, write s (read s) = s
  write_write : ∀ s a b, write (write s a) b = write s b

/-- A rule may admit several outcomes; neither a fixed action enum nor RNG is required. -/
abbrev Handler (Local Context Input Event : Type) :=
  Local → Context → Input → Local → List Event → Prop

structure Module (Global Input Event : Type) where
  manifest : ModuleManifest
  point : String
  Local : Type
  Context : Type
  scope : Scope Global Local
  observe : Global → Context
  rule : Handler Local Context Input Event

/-- An extension can alter behavior, not just numeric definitions. -/
structure Extension (Local Context Input Event : Type) where
  manifest : ExtensionManifest
  amend : Handler Local Context Input Event → Handler Local Context Input Event

def effectiveRule (m : Module Global Input Event)
    (xs : List (Extension m.Local m.Context Input Event)) :
    Handler m.Local m.Context Input Event :=
  xs.foldl (fun rule ext => ext.amend rule) m.rule

/--
One accepted operation. It uses exactly the installed extensions, in order.
It cannot change registration. Cross-module operations need an explicitly granted
composite local view; there is no implicit event bus or access to the whole host.
-/
def ModuleRun (m : Module Global Input Event)
    (xs : List (Extension m.Local m.Context Input Event))
    (before : Session Global) (input : Input) (after : Session Global)
    (events : List Event) : Prop :=
  m.manifest ∈ before.kernel.modules ∧
  xs.map Extension.manifest =
    before.kernel.extensions.filter (fun e => e.target == m.manifest.id && e.point == m.point) ∧
  ∃ fragment, effectiveRule m xs (m.scope.read before.state)
    (m.observe before.state) input fragment events ∧
    after = { kernel := before.kernel, state := m.scope.write before.state fragment }

theorem module_run_preserves_kernel (m : Module Global Input Event)
    (xs : List (Extension m.Local m.Context Input Event))
    (before after : Session Global) (input : Input) (events : List Event)
    (h : ModuleRun m xs before input after events) : after.kernel = before.kernel := by
  rcases h with ⟨_, _, fragment, _, rfl⟩
  rfl

theorem module_run_preserves_frame (m : Module Global Input Event)
    (xs : List (Extension m.Local m.Context Input Event))
    (before after : Session Global) (input : Input) (events : List Event)
    (h : ModuleRun m xs before input after events) :
    m.scope.frame after.state = m.scope.frame before.state := by
  rcases h with ⟨_, _, fragment, _, rfl⟩
  exact m.scope.frame_write before.state fragment

/-- Optional replay/profile property, deliberately absent from Module and Extension. -/
def Functional (rule : Handler Local Context Input Event) : Prop :=
  ∀ s ctx input a b ea eb,
    rule s ctx input a ea → rule s ctx input b eb → a = b ∧ ea = eb

/-- A profile can protect a local invariant without freezing the gameplay algorithm. -/
def Preserves (invariant : Local → Prop) (rule : Handler Local Context Input Event) : Prop :=
  ∀ s ctx input next events, invariant s → rule s ctx input next events → invariant next

def SafeExtension (invariant : Local → Prop) (ext : Extension Local Context Input Event) : Prop :=
  ∀ rule, Preserves invariant rule → Preserves invariant (ext.amend rule)

theorem extensions_preserve_invariant (invariant : Local → Prop)
    (xs : List (Extension Local Context Input Event))
    (safe : ∀ ext ∈ xs, SafeExtension invariant ext)
    (rule : Handler Local Context Input Event) (base : Preserves invariant rule) :
    Preserves invariant (xs.foldl (fun r ext => ext.amend r) rule) := by
  induction xs generalizing rule with
  | nil => exact base
  | cons ext rest ih =>
    apply ih
    · intro next hn
      exact safe next (List.mem_cons_of_mem ext hn)
    · exact safe ext (List.mem_cons_self) rule base

namespace Examples

/-- A device state and an unrelated flag: no world, actors, map or inventory. -/
def deviceScope : Scope (Nat × Bool) Nat where
  Frame := Bool
  read := Prod.fst
  write := fun s n => (n, s.2)
  frame := Prod.snd
  read_write := by intros; rfl
  frame_write := by intros; rfl
  write_read := by intro s; cases s; rfl
  write_write := by intros; rfl

def device : Module (Nat × Bool) Unit Unit where
  manifest := { id := "device", extensionPoints := ["activate"] }
  point := "activate"
  Local := Nat
  Context := Unit
  scope := deviceScope
  observe := fun _ => ()
  rule := fun n _ _ n' events => n' = n + 1 ∧ events = []

def boost : Extension Nat Unit Unit Unit where
  manifest := { id := "boost", target := "device", point := "activate" }
  amend := fun base n ctx input n' events =>
    ∃ intermediate, base n ctx input intermediate events ∧ n' = intermediate + 1

def loaded : Session (Nat × Bool) :=
  { kernel := {}, state := (41, true) }

def withDevice : Session (Nat × Bool) :=
  { loaded with kernel := { modules := [device.manifest] } }

def withBoost : Session (Nat × Bool) :=
  { withDevice with kernel := { withDevice.kernel with extensions := [boost.manifest] } }

example : attachModule loaded device.manifest = .ok withDevice := by rfl
example : attachExtension withDevice boost.manifest = .ok withBoost := by rfl

example : ModuleRun device [] withDevice ()
    { withDevice with state := (42, true) } [] := by
  refine ⟨by simp [withDevice], by rfl, (42 : Nat), ?_, rfl⟩
  change 42 = 41 + 1 ∧ ([] : List Unit) = []
  exact ⟨rfl, rfl⟩

/-- The same live session and command now have extended behavior; the flag survives. -/
example : ModuleRun device [boost] withBoost ()
    { withBoost with state := (43, true) } [] := by
  refine ⟨by simp [withBoost, withDevice], by rfl, (43 : Nat), ?_, rfl⟩
  change ∃ n, (n = 41 + 1 ∧ ([] : List Unit) = []) ∧ 43 = n + 1
  exact ⟨42, ⟨rfl, rfl⟩, rfl⟩

def coin : Handler Nat Unit Unit Unit :=
  fun _ _ _ n events => (n = 0 ∨ n = 1) ∧ events = []

/-- The specification genuinely admits nondeterministic rules. -/
theorem coin_not_functional : ¬ Functional coin := by
  intro h
  have eq := h 7 () () 0 1 [] [] ⟨Or.inl rfl, rfl⟩ ⟨Or.inr rfl, rfl⟩
  exact Nat.noConfusion eq.1

example : installModule { atBoundary := false } device.manifest = .error .busy := by rfl
example : installModule withDevice.kernel device.manifest = .error .duplicateId := by rfl
example : installModule {} { id := "wand", dependsOn := ["objects"] } =
    .error .missingDependency := by rfl
example : installModule {} { id := "wand", requires := [{ name := "items", major := 1 }] } =
    .error .missingCapability := by rfl
example : installModule { baseCapabilities := [{ name := "items", major := 1 }] }
    { id := "other-items", provides := [{ name := "items", major := 1 }] } =
    .error .providerConflict := by rfl
example : installExtension {} boost.manifest = .error .missingDependency := by rfl
example : installExtension withBoost.kernel boost.manifest = .error .duplicateId := by rfl
example : installExtension withDevice.kernel
    { id := "late", target := "device", point := "activate", after := ["absent"] } =
    .error .missingDependency := by rfl
example : installExtension withDevice.kernel
    { id := "secret", target := "device", point := "not-exported" } =
    .error .unknownExtensionPoint := by rfl

end Examples
end NetMechanics.Architecture
