inductive InvocationItem
  | BookOfTheDead
  | CandelabrumOfInvocation
  | BellOfOpening
deriving DecidableEq

structure PlayerInventory where
  hasBook : Bool
  hasCandelabrum : Bool
  hasBell : Bool

def hasAllInvocationItems (inv : PlayerInventory) : Bool :=
  inv.hasBook && inv.hasCandelabrum && inv.hasBell

def canPerformRitual (inv : PlayerInventory) : Bool :=
  hasAllInvocationItems inv

theorem invocation_trio_completeness (inv : PlayerInventory) :
    canPerformRitual inv = true ↔ (inv.hasBook = true ∧ inv.hasCandelabrum = true ∧ inv.hasBell = true) := by
  unfold canPerformRitual hasAllInvocationItems
  simp [Bool.and_eq_true, and_assoc]

theorem missing_any_invocation_item_fails (inv : PlayerInventory) (h : inv.hasBook = false ∨ inv.hasCandelabrum = false ∨ inv.hasBell = false) :
    canPerformRitual inv = false := by
  unfold canPerformRitual hasAllInvocationItems
  cases h with
  | inl hb => simp [hb]
  | inr hrest =>
    cases hrest with
    | inl hc => simp [hc]
    | inr hbe =>
      cases inv.hasBook <;> cases inv.hasCandelabrum <;> simp [hbe]

structure WizardState where
  disturbed : Bool
  timer : Nat

def decrementTimer (s : WizardState) : WizardState :=
  if s.disturbed && s.timer > 0 then
    { s with timer := s.timer - 1 }
  else
    s

theorem rodney_resurrection_monotonic (s : WizardState) (h1 : s.disturbed = true) (h2 : s.timer > 0) :
    (decrementTimer s).timer < s.timer := by
  unfold decrementTimer
  simp [h1]
  split
  · simp
    omega
  · rename_i h
    simp at h
    omega
