/-
  NetHack Mechanics Formalized in Lean 4: Epistemic Item Identification Lattice
  Formalizing the 4-tier knowledge progression and proving knowledge monotonicity.
-/

namespace NetMechanics

/--
  Epistemic identification states in NetHack:
  1. Unidentified: Player only perceives cosmetic appearance (e.g. "ruby ring").
  2. TypeIdentified: Base effect is known (e.g. "ring of protection").
  3. BucKnown: BUC status has been determined (e.g. via altar testing).
  4. FullyIdentified: All hidden properties known (enchantment bonus, charges).
-/
inductive KnowledgeLevel where
  | Unidentified
  | TypeIdentified
  | BucKnown
  | FullyIdentified
deriving Repr, DecidableEq

/-- Numeric rank of knowledge state -/
def KnowledgeLevel.rank : KnowledgeLevel → Nat
  | KnowledgeLevel.Unidentified => 0
  | KnowledgeLevel.TypeIdentified => 1
  | KnowledgeLevel.BucKnown => 2
  | KnowledgeLevel.FullyIdentified => 3

/-- Partial order on epistemic knowledge: k1 ≤ k2 iff rank k1 ≤ rank k2 -/
def KnowledgeLevel.le (k1 k2 : KnowledgeLevel) : Prop :=
  k1.rank ≤ k2.rank

instance : LE KnowledgeLevel where
  le := KnowledgeLevel.le

instance : DecidableRel (α := KnowledgeLevel) (· ≤ ·) :=
  fun k1 k2 => inferInstanceAs (Decidable (k1.rank ≤ k2.rank))

/--
  Theorem: Knowledge order is reflexive.
-/
theorem knowledge_le_refl (k : KnowledgeLevel) : k ≤ k := by
  exact Nat.le_refl k.rank

/--
  Theorem: Knowledge order is transitive.
-/
theorem knowledge_le_trans {k1 k2 k3 : KnowledgeLevel} (h12 : k1 ≤ k2) (h23 : k2 ≤ k3) :
  k1 ≤ k3 := by
  exact Nat.le_trans h12 h23

/--
  Theorem: Knowledge order is antisymmetric.
-/
theorem knowledge_le_antisymm {k1 k2 : KnowledgeLevel} (h12 : k1 ≤ k2) (h21 : k2 ≤ k1) :
  k1 = k2 := by
  have hr : k1.rank = k2.rank := Nat.le_antisymm h12 h21
  cases k1 <;> cases k2 <;> (try rfl) <;> (simp [KnowledgeLevel.rank] at hr)

/-- Knowledge state update operators -/
def learnType (k : KnowledgeLevel) : KnowledgeLevel :=
  match k with
  | KnowledgeLevel.Unidentified => KnowledgeLevel.TypeIdentified
  | _ => k

def learnBUC (k : KnowledgeLevel) : KnowledgeLevel :=
  match k with
  | KnowledgeLevel.Unidentified => KnowledgeLevel.BucKnown
  | KnowledgeLevel.TypeIdentified => KnowledgeLevel.BucKnown
  | _ => k

def identifyFully (_k : KnowledgeLevel) : KnowledgeLevel :=
  KnowledgeLevel.FullyIdentified

/--
  Theorem: Monotonicity of Type Identification.
  Learning the item type never degrades player knowledge.
-/
theorem learn_type_monotone (k : KnowledgeLevel) : k ≤ learnType k := by
  cases k <;> decide

/--
  Theorem: Monotonicity of BUC Testing.
  Testing BUC status on altars never degrades player knowledge.
-/
theorem learn_buc_monotone (k : KnowledgeLevel) : k ≤ learnBUC k := by
  cases k <;> decide

/--
  Theorem: Monotonicity of Full Identification.
  Full identification maximizes epistemic knowledge.
-/
theorem identify_fully_monotone (k : KnowledgeLevel) : k ≤ identifyFully k := by
  cases k <;> decide

/--
  Theorem: Idempotency of Full Identification.
  Repeated full identification is invariant.
-/
theorem identify_fully_idempotent (k : KnowledgeLevel) :
  identifyFully (identifyFully k) = identifyFully k := by
  rfl

end NetMechanics
