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
  let attackRes := resolveMeleeAttack (toHitValue 1 0 1 0 goblin.ac) goblin 9 8 1 2 none
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
  let (revertedHero, isDead) := applyPolyDamage polyHero 18 false
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
  let nut0 : Int := 0
  let nutFood := nut0 + 400
  IO.println s!"[Nutrition] Hunger states: 0 nutrition -> {repr (hungerOfNutrition nut0 10)}, +400 nutrition -> {repr (hungerOfNutrition nutFood 10)}"

  -- 13. Magic Spells & Mana demonstration
  let casterPw := 12
  let castRes := castSpell casterPw SpellKind.ForceBolt
  IO.println s!"[Magic] Caster with {casterPw} Pw cast Force Bolt -> remaining Pw: {repr castRes}"

  -- 14. Sokoban & Boulder Dynamics demonstration
  let sokoPush := pushBoulder (5, 5) PushDir.East SokoTile.Floor
  let sokoPitPush := pushBoulder (5, 5) PushDir.North SokoTile.Pit
  IO.println s!"[Sokoban] Push onto floor -> {repr sokoPush}, Push into pit -> {repr sokoPitPush}"

  -- 15. Dungeon Branching & Topology demonstration
  let minesEntrance := enterBranch BranchId.GnomishMines 3
  let minesExit := exitBranch ⟨BranchId.GnomishMines, 1⟩
  IO.println s!"[Branch] Enter Mines from depth 3 -> {repr minesEntrance}, Exit Mines -> {repr minesExit}"

  -- 16. Pet & Companion Dynamics demonstration
  let heroPos : PetCoord := ⟨10, 10⟩
  let petPos : PetCoord := ⟨11, 10⟩
  let (newH, newP) := swapPositions heroPos petPos
  let interaction := interactWithOccupant heroPos petPos 42 true
  IO.println s!"[Pet] Hero at {repr heroPos} displaces tame pet at {repr petPos} -> Hero: {repr newH}, Pet: {repr newP}, Action: {repr interaction}"

  -- 17. Enchantment, Erosion & Alchemy demonstration
  let ench0 := enchantWeapon 2 BUC.Blessed 0 3
  let erosion0 := applyErosion ⟨0, false⟩
  let alchemyRes := mixAlchemy AlchemyPotion.Healing AlchemyPotion.GainEnergy
  let ench1 := enchantArmor 4 BUC.Uncursed false false 1 1
  IO.println s!"[Enchantment] +2 sword enchanted with blessed scroll (rnd(3) = 3) -> {repr ench0}"
  IO.println s!"[Enchantment] +4 plain armor, uncursed scroll, rn2(4) = 1 -> {repr ench1}"
  IO.println s!"[Erosion] Unproofed armor exposed to acid -> {repr erosion0}"
  IO.println s!"[Alchemy] Dipping healing into gain energy -> {repr alchemyRes}"

  -- 18. Castle Drawbridge & Astral Plane Ascension demonstration
  let (bridgeAfterRaise, bridgeTransition) := toggleDrawbridge DrawbridgeState.Open true
  let ascensionRes := offerAmuletOnHighAltar true Alignment.Neutral Alignment.Neutral
  let rejectedRes := offerAmuletOnHighAltar true Alignment.Neutral Alignment.Chaotic
  IO.println s!"[Endgame] Open Drawbridge raised with occupant -> {repr bridgeAfterRaise}, Transition: {repr bridgeTransition}"
  IO.println s!"[Ascension] Amulet offered on co-aligned Neutral High Altar -> {repr ascensionRes}"
  IO.println s!"[Ascension] Amulet offered on cross-aligned Chaotic High Altar -> {repr rejectedRes}"

  -- 19. Religion, Divine Favor & Holy Water demonstration
  let div0 : DivineState := { favor := 5, prayerTimeout := 0, giftCount := 0 }
  let (div1, sacRes) := resolveSacrifice div0 Alignment.Neutral Alignment.Neutral 300
  let holyWater := consecrateWater BUC.Uncursed true div1.favor
  IO.println s!"[Religion] Valkyrie Pantheon: {repr valkyriePantheon.neutral}"
  IO.println s!"[Sacrifice] Offered corpse on co-aligned altar -> New Favor: {div1.favor}, Result: {repr sacRes}"
  IO.println s!"[HolyWater] Consecrated water on altar with favor {div1.favor} -> {repr holyWater}"

  -- 20. Signature Artifacts, Wishing & Wand Recharging demonstration
  let excalDmg := resolveArtifactDamage ArtifactKind.Excalibur 8 true
  let vorpalDefender : Combatant := { hp := 40, maxHp := 40, ac := 2, level := 5, toHitBonus := 0, damageBonus := 0, isDead := false }
  let vorpalResult := applyVorpalStrike vorpalDefender true
  let wand0 : WandCharges := { charges := 3, recharges := 0 }
  let wand1 := zapWand wand0
  let wandRecharged := rechargeWand (wand1.getD wand0) ChargeBuc.Blessed 8 false 100 1 1
  IO.println s!"[Artifact] Excalibur damage vs demon: {excalDmg}"
  IO.println s!"[Artifact] Vorpal Blade decapitation strike -> Defender HP: {vorpalResult.hp}, Dead: {vorpalResult.isDead}"
  -- 21. Class Quest Branch & Nemesis demonstration
  let questHero : HeroQuestEligibility := { experienceLevel := 14, alignmentRecord := 25, isHostileToLeader := false }
  let q0 : QuestState := { progress := QuestProgress.Unassigned, artifactLocation := ArtifactLocation.HeldByNemesis, nemesisHp := 120 }
  let qAssigned := consultLeader q0 questHero
  let qDamaged := attackNemesis qAssigned 50
  let qDefeated := attackNemesis qDamaged 100
  let qClaimed := pickUpQuestArtifact qDefeated
  let qComplete := returnToLeaderWithArtifact qClaimed
  IO.println s!"[Quest] Eligible Hero level 14 consults Leader -> Progress: {repr qAssigned.progress}"
  IO.println s!"[Quest] Nemesis Lord Surtur defeated -> Artifact: {repr qDefeated.artifactLocation}, Progress: {repr qDefeated.progress}"
  IO.println s!"[Quest] Hero claims Quest Artifact & Leader blesses -> Progress: {repr qComplete.progress}"







