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

/-- What an object is, as far as C `mbag_explodes` cares (`obj->otyp`, `obj->spe`). -/
inductive BagCheckKind where
  | BagOfHolding
  | BagOfTricks (charges : Int)
  | WandOfCancellation (charges : Int)
  | Other
deriving Repr, DecidableEq

/-- An object being put into a Bag of Holding with its recursive contents (`obj->cobj`). -/
inductive BagCheckItem where
  | mk (kind : BagCheckKind) (children : List BagCheckItem)
deriving Repr

/-- Empty wands of cancellation / bags of tricks never explode (`pickup.c:2491-2493`). -/
def bagEmpty : BagCheckKind → Bool
  | .BagOfTricks c => decide (c ≤ 0)
  | .WandOfCancellation c => decide (c ≤ 0)
  | _ => false

/-- `Is_mbag(obj) || otyp == WAN_CANCELLATION` (`pickup.c:2496`). -/
def bagMagical : BagCheckKind → Bool
  | .Other => false
  | _ => true

/--
  The C draw `rn2(1 << min(depth, 7))` (`pickup.c:2497`), with the caller-supplied
  oracle `roll` (given the bound) reduced into `0..bound-1`.
-/
def bagDraw (roll : Nat → Nat) (depth : Nat) : Nat :=
  roll (1 <<< Nat.min depth 7) % (1 <<< Nat.min depth 7)

mutual
  /--
    C `mbag_explodes(obj, depthin)` (`pickup.c:2488-2507`). `roll` models `rn2(bound)`.
    (The Lean model shares one oracle across draws; Rust consumes one roll per draw.)
  -/
  def mbagExplodes (roll : Nat → Nat) (depth : Nat) : BagCheckItem → Bool
    | .mk k cs =>
      if bagEmpty k then false
      else if bagMagical k && decide (bagDraw roll depth ≤ depth) then true
      else anyExplodes roll (depth + 1) cs

  def anyExplodes (roll : Nat → Nat) (depth : Nat) : List BagCheckItem → Bool
    | [] => false
    | c :: cs => mbagExplodes roll depth c || anyExplodes roll depth cs
end

/-- Safe iff `mbag_explodes(obj, 0)` is false (`pickup.c:2658`). -/
def canInsertSafe (roll : Nat → Nat) (item : BagCheckItem) : Bool :=
  !mbagExplodes roll 0 item

/-- At depth 0 the draw is `rn2(1) = 0 <= 0`, so a BoH always explodes. -/
theorem depth_zero_boh_explodes (roll : Nat → Nat) (cs : List BagCheckItem) :
  mbagExplodes roll 0 (BagCheckItem.mk BagCheckKind.BagOfHolding cs) = true := by
  simp [mbagExplodes, bagEmpty, bagMagical, bagDraw, Nat.mod_one]

/-- A charged wand of cancellation explodes at depth 0 regardless of the roll. -/
theorem cancellation_wand_explodes (roll : Nat → Nat) (c : Int) (h : 0 < c)
    (cs : List BagCheckItem) :
  mbagExplodes roll 0 (BagCheckItem.mk (BagCheckKind.WandOfCancellation c) cs) = true := by
  have : ¬ c ≤ 0 := by omega
  simp [mbagExplodes, bagEmpty, bagMagical, bagDraw, Nat.mod_one, this]

/--
  Theorem: Bag of Holding Explosion Prevention.
  A Bag of Holding cannot be safely inserted into a Bag of Holding (any roll).
-/
theorem boh_cannot_contain_boh (roll : Nat → Nat) (cs : List BagCheckItem) :
  canInsertSafe roll (BagCheckItem.mk BagCheckKind.BagOfHolding cs) = false := by
  simp [canInsertSafe, depth_zero_boh_explodes]

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

/-- Tier from rank (ranks >= 5 saturate at Overloaded) -/
def tierOfRank : Nat → EncumbranceTier
  | 0 => .Unencumbered
  | 1 => .Burdened
  | 2 => .Stressed
  | 3 => .Strained
  | 4 => .Overtaxed
  | _ => .Overloaded

theorem tierRank_tierOfRank (k : Nat) : tierRank (tierOfRank k) = min k 5 := by
  match k with
  | 0 => rfl
  | 1 => rfl
  | 2 => rfl
  | 3 => rfl
  | 4 => rfl
  | k + 5 => simp [tierOfRank, tierRank]

/--
  Encumbrance tier given total weight and carrying capacity (C `hack.c:4372`
  `calc_capacity`): unencumbered at or under capacity; Overloaded if cap <= 1;
  else `min ((wt - cap) * 2 / cap + 1) 5`.
-/
def calculateEncumbrance (wt : Nat) (cap : Nat) : EncumbranceTier :=
  if cap == 0 then
    EncumbranceTier.Overloaded
  else if wt ≤ cap then
    EncumbranceTier.Unencumbered
  else if cap ≤ 1 then
    EncumbranceTier.Overloaded
  else
    tierOfRank ((wt - cap) * 2 / cap + 1)

/--
  Theorem: At or under capacity is always Unencumbered (for positive cap).
-/
theorem unencumbered_when_le_cap (wt cap : Nat) (hcap : 0 < cap) (hwt : wt ≤ cap) :
  calculateEncumbrance wt cap = EncumbranceTier.Unencumbered := by
  have hne : ¬(cap = 0) := Nat.ne_of_gt hcap
  have hf : (cap == 0) = false := beq_eq_false_iff_ne.mpr hne
  simp [calculateEncumbrance, hf, hwt]

/-- Theorem: closed-form tier rank above capacity (cap >= 2) -/
theorem encumbrance_tier_rank (wt cap : Nat) (hcap : 2 ≤ cap) (hwt : cap < wt) :
  tierRank (calculateEncumbrance wt cap) = min ((wt - cap) * 2 / cap + 1) 5 := by
  have hf : (cap == 0) = false := beq_eq_false_iff_ne.mpr (by omega)
  have h1 : ¬ wt ≤ cap := by omega
  have h2 : ¬ cap ≤ 1 := by omega
  simp only [calculateEncumbrance, hf, h1, h2]
  simpa using tierRank_tierOfRank _

/-- Carrying capacity (C `hack.c:4295` `weight_cap`); `str` is ACURRSTR,
  `legs` is the number of wounded legs (0..2). -/
def weightCap (str con : Int) (levitating : Bool) (legs : Nat) : Nat :=
  (max (if levitating then (1000 : Int)
        else (if 25 * (str + con) + 50 > 1000 then 1000 else 25 * (str + con) + 50)
              - 100 * ((min legs 2 : Nat) : Int)) 1).toNat

/-- Theorem: carrying capacity never exceeds MAX_CARR_CAP (1000) -/
theorem weight_cap_le_1000 (str con : Int) (lev : Bool) (legs : Nat) :
  weightCap str con lev legs ≤ 1000 := by
  unfold weightCap
  repeat' split
  all_goals omega

/-- Theorem: carrying capacity is at least 1 -/
theorem weight_cap_pos (str con : Int) (lev : Bool) (legs : Nat) :
  1 ≤ weightCap str con lev legs := by
  unfold weightCap
  omega

end NetMechanics
