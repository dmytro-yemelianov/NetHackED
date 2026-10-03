import NetMechanics.Basic
import NetMechanics.BUC
import NetMechanics.Pet

namespace NetMechanics

/-!
# Multi-Agent Co-op: Tactical Hero + Pet Co-op Dynamics

Formalizes companion tactical AI mechanics from NetHack (dog.c, dogmove.c):
1. Pet BUC Detection: Pet refuses to step on cursed items under calm conditions.
2. Pet Growth & Promotion: Monotone promotion ladder (LittleDog -> Dog -> WarDog; Kitten -> Housecat -> LargeCat).
3. Co-op Defense Invariant: Threat neutralization priority over wandering.
-/

-- ============================================================================
-- Section 1: Pet BUC Detection & Reluctance
-- ============================================================================

/-- Decides whether an item's BUC is calm and acceptable to a pet. -/
def isNonCursed (b : BUC) : Bool :=
  match b with
  | BUC.Cursed => false
  | _ => true

/-- Evaluates whether a floor tile is safe for a calm pet to step onto.
    If ANY item on the tile is Cursed, the pet senses unholy malice and refuses to step. -/
def petTileSteppable (tileItems : List BUC) : Bool :=
  tileItems.all isNonCursed

/-- Theorem: If a tile contains at least one cursed item, the calm pet strictly refuses to step. -/
theorem pet_rejects_cursed_tile (items : List BUC) (h : BUC.Cursed ∈ items) :
    petTileSteppable items = false := by
  unfold petTileSteppable
  rw [List.all_eq_false]
  exact ⟨BUC.Cursed, h, by decide⟩

/-- Theorem: If all items on a tile are non-cursed (blessed or uncursed), the pet accepts the tile. -/
theorem pet_accepts_safe_tile (items : List BUC) (h : ∀ b ∈ items, b ≠ BUC.Cursed) :
    petTileSteppable items = true := by
  unfold petTileSteppable
  rw [List.all_eq_true]
  intro b hb
  have hnc := h b hb
  cases b
  · rfl
  · rfl
  · contradiction

/-- Empty floor tile is always steppable by pet. -/
theorem pet_accepts_empty_tile : petTileSteppable [] = true := rfl

-- ============================================================================
-- Section 2: Pet Species Growth & Promotion Hierarchy
-- ============================================================================

inductive PetFamily where
  | Canine
  | Feline
  deriving DecidableEq, Repr

inductive PetSpeciesTier where
  | LittleDog
  | Dog
  | WarDog
  | Kitten
  | Housecat
  | LargeCat
  deriving DecidableEq, Repr

/-- Classify species into family. -/
def petFamily (p : PetSpeciesTier) : PetFamily :=
  match p with
  | PetSpeciesTier.LittleDog | PetSpeciesTier.Dog | PetSpeciesTier.WarDog => PetFamily.Canine
  | PetSpeciesTier.Kitten | PetSpeciesTier.Housecat | PetSpeciesTier.LargeCat => PetFamily.Feline

/-- Numeric power tier (0, 1, 2) for monotonic growth comparison. -/
def petPowerTier (p : PetSpeciesTier) : Nat :=
  match p with
  | PetSpeciesTier.LittleDog | PetSpeciesTier.Kitten => 0
  | PetSpeciesTier.Dog | PetSpeciesTier.Housecat => 1
  | PetSpeciesTier.WarDog | PetSpeciesTier.LargeCat => 2

/-- Species promotion based on pet level.
    Level < 4: Base tier
    Level 4..6: Mid tier
    Level >= 7: Apex companion tier -/
def promotePet (species : PetSpeciesTier) (level : Nat) : PetSpeciesTier :=
  match species with
  | PetSpeciesTier.LittleDog =>
      if level ≥ 7 then PetSpeciesTier.WarDog
      else if level ≥ 4 then PetSpeciesTier.Dog
      else PetSpeciesTier.LittleDog
  | PetSpeciesTier.Dog =>
      if level ≥ 7 then PetSpeciesTier.WarDog
      else PetSpeciesTier.Dog
  | PetSpeciesTier.WarDog => PetSpeciesTier.WarDog
  | PetSpeciesTier.Kitten =>
      if level ≥ 7 then PetSpeciesTier.LargeCat
      else if level ≥ 4 then PetSpeciesTier.Housecat
      else PetSpeciesTier.Kitten
  | PetSpeciesTier.Housecat =>
      if level ≥ 7 then PetSpeciesTier.LargeCat
      else PetSpeciesTier.Housecat
  | PetSpeciesTier.LargeCat => PetSpeciesTier.LargeCat

/-- Theorem: Pet promotion strictly preserves biological family (Canines stay Canines, Felines stay Felines). -/
theorem promote_preserves_family (p : PetSpeciesTier) (level : Nat) :
    petFamily (promotePet p level) = petFamily p := by
  cases p with
  | LittleDog =>
      unfold promotePet
      by_cases h7 : level ≥ 7
      · simp [h7, petFamily]
      · by_cases h4 : level ≥ 4
        · simp [h7, h4, petFamily]
        · simp [h7, h4, petFamily]
  | Dog =>
      unfold promotePet
      by_cases h7 : level ≥ 7
      · simp [h7, petFamily]
      · simp [h7, petFamily]
  | WarDog =>
      unfold promotePet
      rfl
  | Kitten =>
      unfold promotePet
      by_cases h7 : level ≥ 7
      · simp [h7, petFamily]
      · by_cases h4 : level ≥ 4
        · simp [h7, h4, petFamily]
        · simp [h7, h4, petFamily]
  | Housecat =>
      unfold promotePet
      by_cases h7 : level ≥ 7
      · simp [h7, petFamily]
      · simp [h7, petFamily]
  | LargeCat =>
      unfold promotePet
      rfl

/-- Theorem: Higher experience level never causes pet demotion (monotonic power progression). -/
theorem promote_monotonic_level (p : PetSpeciesTier) (l1 l2 : Nat) (h : l1 ≤ l2) :
    petPowerTier (promotePet p l1) ≤ petPowerTier (promotePet p l2) := by
  cases p with
  | LittleDog =>
      unfold promotePet petPowerTier
      by_cases h1_7 : l1 ≥ 7
      · have h2_7 : l2 ≥ 7 := by omega
        simp [h1_7, h2_7]
      · by_cases h1_4 : l1 ≥ 4
        · simp [h1_7, h1_4]
          by_cases h2_7 : l2 ≥ 7
          · simp [h2_7]
          · have h2_4 : l2 ≥ 4 := by omega
            simp [h2_7, h2_4]
        · simp [h1_7, h1_4]
  | Dog =>
      unfold promotePet petPowerTier
      by_cases h1_7 : l1 ≥ 7
      · have h2_7 : l2 ≥ 7 := by omega
        simp [h1_7, h2_7]
      · by_cases h2_7 : l2 ≥ 7 <;> simp [h1_7, h2_7]
  | WarDog =>
      exact Nat.le_refl _
  | Kitten =>
      unfold promotePet petPowerTier
      by_cases h1_7 : l1 ≥ 7
      · have h2_7 : l2 ≥ 7 := by omega
        simp [h1_7, h2_7]
      · by_cases h1_4 : l1 ≥ 4
        · simp [h1_7, h1_4]
          by_cases h2_7 : l2 ≥ 7
          · simp [h2_7]
          · have h2_4 : l2 ≥ 4 := by omega
            simp [h2_7, h2_4]
        · simp [h1_7, h1_4]
  | Housecat =>
      unfold promotePet petPowerTier
      by_cases h1_7 : l1 ≥ 7
      · have h2_7 : l2 ≥ 7 := by omega
        simp [h1_7, h2_7]
      · by_cases h2_7 : l2 ≥ 7 <;> simp [h1_7, h2_7]
  | LargeCat =>
      exact Nat.le_refl _

/-- Theorem: Apex tier companions (WarDog, LargeCat) are fixed points under promotion. -/
theorem promote_apex_fixed_point (level : Nat) :
    promotePet PetSpeciesTier.WarDog level = PetSpeciesTier.WarDog ∧
    promotePet PetSpeciesTier.LargeCat level = PetSpeciesTier.LargeCat := by
  dsimp [promotePet]
  exact ⟨rfl, rfl⟩

-- ============================================================================
-- Section 3: Tactical Multi-Agent Co-op Priority
-- ============================================================================

/-- Pet tactical goal selection -/
inductive PetGoal where
  | AttackHostile (targetId : Nat)
  | FetchItem (coord : PetCoord)
  | FollowHero
  deriving DecidableEq, Repr

/-- Co-op decision function:
    If a hostile is adjacent to hero (within melee threat), pet prioritizes attacking it.
    Otherwise, if there is a safe item on floor nearby, pet considers fetching.
    Otherwise, pet loyally follows the hero. -/
def choosePetGoal (hostileNearHero : Option Nat) (safeItemNearby : Option PetCoord) : PetGoal :=
  match hostileNearHero with
  | some hostileId => PetGoal.AttackHostile hostileId
  | none =>
      match safeItemNearby with
      | some itemCoord => PetGoal.FetchItem itemCoord
      | none => PetGoal.FollowHero

/-- Theorem: Tactical priority invariant: When a hostile threatens the hero, pet ALWAYS chooses defense over item fetching or following. -/
theorem pet_prioritizes_hero_defense (hostileId : Nat) (itemOpt : Option PetCoord) :
    choosePetGoal (some hostileId) itemOpt = PetGoal.AttackHostile hostileId := by
  rfl

end NetMechanics
