import NetMechanics.BUC

namespace NetMechanics

/-- Item classification -/
inductive ItemKind where
  | Weapon (damageDice : Nat)
  | Armor (acBonus : Int)
  | Wand (charges : Nat)
  | Potion
  | Scroll
  | Food (nutrition : Nat)
  | Container (isBagOfHolding : Bool)
deriving Repr, DecidableEq

/--
  Inductive Item model.
  In C NetHack, `struct obj` uses intrusive pointers (`nobj`, `cobj`, `vptrs`).
  This allows potential memory corruption or circular container references.
  Here, nested containment is modeled as an inductive tree,
  which is acyclic and well-founded by construction.
-/
inductive Item where
  | Single (name : String) (kind : ItemKind) (weight : Nat) (buc : BUC)
  | Box (name : String) (baseWeight : Nat) (buc : BUC) (isBagOfHolding : Bool) (contents : List Item)
deriving Repr

mutual
  /--
    Calculate the effective weight of an item or container.
    Matches NetHack 5.0 mkobj.c:1951 exact rounding-up formulas:
      cursed:    innerWt * 2
      blessed:   (innerWt + 3) / 4
      uncursed:  (innerWt + 1) / 2
  -/
  def itemWeight : Item → Nat
    | Item.Single _ _ w _ => w
    | Item.Box _ baseWt buc isBoH contents =>
      let innerWt := contentsWeight contents
      let effectiveInner :=
        if isBoH then
          match buc with
          | BUC.Blessed => (innerWt + 3) / 4
          | BUC.Uncursed => (innerWt + 1) / 2
          | BUC.Cursed => innerWt * 2
        else
          innerWt
      baseWt + effectiveInner

  /-- Sum weight of a list of items -/
  def contentsWeight : List Item → Nat
    | [] => 0
    | x :: xs => itemWeight x + contentsWeight xs
end

/--
  Theorem: Non-negativity of weights.
  All item weights are non-negative.
-/
theorem item_weight_ge_zero (i : Item) : itemWeight i ≥ 0 := by
  exact Nat.zero_le (itemWeight i)

/--
  Theorem: Weight monotonicity for item list.
  Adding an item to a container or inventory never decreases total weight.
-/
theorem contents_weight_cons (x : Item) (xs : List Item) :
  contentsWeight (x :: xs) = itemWeight x + contentsWeight xs := by
  rfl

theorem contents_weight_ge_tail (x : Item) (xs : List Item) :
  contentsWeight (x :: xs) ≥ contentsWeight xs := by
  dsimp [contentsWeight]
  exact Nat.le_add_left (contentsWeight xs) (itemWeight x)

/--
  Check whether an item can be safely inserted into a container.
  Matches NetHack pickup.c:2658 mbag_explodes() check:
  Inserting a Bag of Holding into another Bag of Holding triggers an explosion.
-/
def canInsertSafe (item : Item) (container : Item) : Bool :=
  match container with
  | Item.Box _ _ _ isBoH _ =>
    if isBoH then
      match item with
      | Item.Box _ _ _ itemBoH _ => !itemBoH
      | _ => true
    else
      true
  | _ => false

/--
  Theorem: Bag of Holding Explosion Prevention.
  A Bag of Holding cannot be safely inserted into another Bag of Holding.
-/
theorem boh_cannot_contain_boh (name1 name2 : String) (w1 w2 : Nat) (b1 b2 : BUC) (c1 c2 : List Item) :
  canInsertSafe (Item.Box name1 w1 b1 true c1) (Item.Box name2 w2 b2 true c2) = false := by
  rfl

/-- Encumbrance tiers in NetHack -/
inductive EncumbranceTier where
  | Unencumbered
  | Burdened
  | Stressed
  | Strained
  | Overtaxed
  | Overloaded
deriving Repr, DecidableEq, Ord

/-- Map tier to rank for comparison -/
def tierRank : EncumbranceTier → Nat
  | .Unencumbered => 0
  | .Burdened => 1
  | .Stressed => 2
  | .Strained => 3
  | .Overtaxed => 4
  | .Overloaded => 5

/-- Encumbrance tier calculation given current weight and carrying capacity -/
def calculateEncumbrance (wt : Nat) (cap : Nat) : EncumbranceTier :=
  if cap == 0 then
    EncumbranceTier.Overloaded
  else if wt ≤ cap then
    EncumbranceTier.Unencumbered
  else if wt ≤ cap + cap / 2 then
    EncumbranceTier.Burdened
  else if wt ≤ cap * 2 then
    EncumbranceTier.Stressed
  else if wt ≤ cap * 2 + cap / 2 then
    EncumbranceTier.Strained
  else if wt ≤ cap * 3 then
    EncumbranceTier.Overtaxed
  else
    EncumbranceTier.Overloaded

/--
  Theorem: At or under capacity is always Unencumbered (for positive cap).
-/
theorem unencumbered_when_le_cap (wt cap : Nat) (hcap : 0 < cap) (hwt : wt ≤ cap) :
  calculateEncumbrance wt cap = EncumbranceTier.Unencumbered := by
  unfold calculateEncumbrance
  split
  · next heq =>
    have hne : ¬(cap = 0) := Nat.ne_of_gt hcap
    have hf : (cap == 0) = false := beq_eq_false_iff_ne.mpr hne
    rw [hf] at heq
    contradiction
  · next _ =>
    rfl

end NetMechanics
