import NetMechanics

open NetMechanics

def main : IO Unit := do
  IO.println "=== NetHack Mechanics Formally Defined & Verified in Lean 4 ==="

  -- 1. BUC demonstration
  let cursedItem := BUC.Cursed
  let dippedInHolyWater := dipWater WaterType.Holy cursedItem
  IO.println s!"[BUC] Cursed item dipped in Holy Water -> {repr dippedInHolyWater}"

  -- 2. Container & Encumbrance demonstration
  let dagger := Item.Single "silver dagger" (ItemKind.Weapon 6) 12 BUC.Blessed
  let potion := Item.Single "potion of healing" ItemKind.Potion 20 BUC.Uncursed
  let boh := Item.Box "bag of holding" 15 BUC.Blessed true [dagger, potion]
  let totalWeight := itemWeight boh
  let enc := calculateEncumbrance totalWeight 100
  IO.println s!"[Inventory] Blessed Bag of Holding containing items total weight: {totalWeight}"
  IO.println s!"[Encumbrance] Encumbrance status for capacity 100: {repr enc}"

  -- 3. Turn & Energy Scheduler demonstration
  let initSched : SchedulerState := {
    turn := 1,
    heroEnergy := 12,
    heroSpeed := 12,
    monsterEnergy := 6,
    monsterSpeed := 6
  }
  let (schedAfterHero, actionTaken) := stepScheduler initSched
  IO.println s!"[Scheduler] Hero acted -> action: {repr actionTaken}, remaining energy: {schedAfterHero.heroEnergy}"
  let (schedNext, nextAction) := stepScheduler schedAfterHero
  IO.println s!"[Scheduler] Next step -> action: {repr nextAction}, turn: {schedNext.turn}, heroEnergy: {schedNext.heroEnergy}"

  -- 4. Combat demonstration
  let goblin : Combatant := {
    hp := 12,
    maxHp := 12,
    ac := 7,
    level := 1,
    toHitBonus := 0,
    damageBonus := 0,
    isDead := false
  }
  let attackRes := resolveMeleeAttack 4 2 goblin 15 8 1
  IO.println s!"[Combat] Melee Attack -> Hit: {attackRes.hit}, Damage: {attackRes.damageDealt}, Target HP: {attackRes.defenderAfter.hp}, Dead: {attackRes.defenderAfter.isDead}"

  -- 5. AST & Operational Semantics demonstration
  let dummyCoord : Coord := { x := ⟨10, by decide⟩, y := ⟨10, by decide⟩ }
  let world0 : WorldState := {
    playerCoord := dummyCoord,
    playerEnergy := 12,
    playerCombat := goblin,
    turn := 1
  }
  let (worldAfterWait, effects) := evalAction world0 ActionAST.Wait
  IO.println s!"[AST] Executed ActionAST.Wait -> Effects: {repr effects}, Remaining Energy: {worldAfterWait.playerEnergy}"

  -- 6. Raycast & Specular Reflection demonstration
  let ray0 : BeamRay := { x := 10, y := 10, vel := { dx := 1, dy := 1 }, energy := 5 }
  let rayStepped := stepRay ray0 (some SurfaceOrientation.Vertical)
  IO.println s!"[Raycast] Ray (dx=1, dy=1, energy=5) reflected on Vertical wall -> {repr rayStepped}"

  -- 7. Engraving & Elbereth Ward demonstration
  let elbereth : Engraving := { text := "Elbereth", medium := EngravingMedium.Burned }
  let wardActive := isElberethWardActive (some elbereth) false false
  let smudged := smudge elbereth
  IO.println s!"[Engraving] Burned 'Elbereth' ward active vs visible monster: {wardActive}, smudge invariant: {repr smudged}"

  -- 8. Epistemic Identification Lattice demonstration
  let unident := KnowledgeLevel.Unidentified
  let afterType := learnType unident
  let afterBuc := learnBUC afterType
  let fully := identifyFully afterBuc
  IO.println s!"[Identification] Progression: {repr unident} -> {repr afterType} -> {repr afterBuc} -> {repr fully}"

  -- 9. Polymorph & Revert on Death demonstration
  let heroBase : FormStats := { hp := 20, maxHp := 20, name := "Hero" }
  let vampirePoly : FormStats := { hp := 15, maxHp := 15, name := "Vampire Bat" }
  let polyHero : PolyEntity := { baseForm := heroBase, polyForm := some vampirePoly }
  let (revertedHero, isDead) := applyPolyDamage polyHero 18
  IO.println s!"[Polymorph] Bat (HP=15) took 18 damage -> Reverted to Base: {revertedHero.polyForm.isNone}, Base HP: {revertedHero.baseForm.hp}/20, Dead: {isDead}"

  -- 10. Dijkstra Metric Gradient Descent demonstration
  let startMetric : MetricState := { distToTarget := 5 }
  let closerNeighbor : MetricState := { distToTarget := 4 }
  let steppedMetric := descentStep startMetric (some closerNeighbor)
  IO.println s!"[Pathfinding] Steepest descent step: dist={startMetric.distToTarget} -> dist={steppedMetric.distToTarget}"

  -- 11. Multi-Floor Dungeon Stack demonstration
  let depth1 := surfaceDepth
  let depth2 := descendDepth depth1
  let backTo1 := ascendDepth depth2
  IO.println s!"[DungeonStack] Transitions: surface={depth1.val} -> descend={depth2.val} -> ascend={backTo1.val}"

  -- 12. Nutrition & Hunger Clock demonstration
  let nut0 := 0
  let nutFood := nut0 + 400
  IO.println s!"[Nutrition] Hunger states: 0 nutrition -> {repr (hungerOfNutrition nut0)}, +400 nutrition -> {repr (hungerOfNutrition nutFood)}"

  -- 13. Magic Spells & Mana demonstration
  let casterPw := 12
  let castRes := castSpell casterPw SpellKind.ForceBolt
  IO.println s!"[Magic] Caster with {casterPw} Pw cast Force Bolt -> remaining Pw: {repr castRes}"




