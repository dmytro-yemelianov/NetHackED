# NetHack & NetRust Domain Ontology and Taxonomy

This document establishes the authoritative formal ontology, taxonomic classifications, and semantic relationships for the NetHack domain and its verified Rust rewrite (**NetRust**).

---

## 1. Top-Level Entity Ontology

```
                              WorldEntity
                                   │
      ┌────────────────────────────┼────────────────────────────┐
      │                            │                            │
   Spatial                      Dynamic                      Abstract
      │                            │                            │
 ┌────┴────┐                  ┌────┴────┐                  ┌────┴────┐
Branch   Level              Actor     Item               Conduct  Achievement
  │        │                  │         │                  │         │
Rooms    Grid               Hero     Ground              Rules     Trophies
           │                  │         │
         Tile              Monster  Container
```

### Primary Entity Classes

1. **Spatial Entities**:
   * **Dungeon**: Directed acyclic graph (DAG) of dungeon branches connected by stairs and portals.
   * **Branch**: Sequential or tree-like chain of dungeon levels with specific dungeon themes and generation algorithms.
   * **Level**: Discrete $80 \times 21$ coordinate grid representing a single dungeon floor, containing static terrain, dynamic tiles, traps, items, and actors.
   * **Tile**: The atomic coordinate cell $(x, y) \in [0, 79] \times [0, 20]$, carrying structural terrain properties, visibility, lighting, and embedded features.

2. **Dynamic Entities**:
   * **Actor**: An active agent in the turn scheduler with energy points, attributes, HP, inventory, and location.
     * **Hero (`u`)**: The player character, possessing role, race, alignment, conducts, and experience.
     * **Monster (`monst`)**: Autonomous NPC or creature driven by AI behaviors (hostile, peaceful, tame/pet, shopkeeper, priest, guard).
   * **Item (`obj`)**: Physical artifacts, consumables, equipment, and containers.
     * **Atomic Item**: Single indivisible object or homogeneous stack (arrows, gold, potions).
     * **Container**: Item holding a collection of other items, forming an acyclic tree hierarchy.

3. **Abstract & Epistemic Entities**:
   * **Epistemic State**: Player knowledge of the world (unmapped vs seen vs remembered glyphs; item identification states).
   * **Conduct**: Strict self-imposed or rule-imposed behavioural restrictions (Atheist, Pacifist, Vegetarian, etc.).
   * **Intrinsic / Extrinsic**: Persistent or temporary capability flags attached to actors.

---

## 2. Item Taxonomy & Classification

In NetHack C (`include/objclass.h`), items are grouped into 15 fundamental classes:

```
                                  Item
                                   │
      ┌────────────────┬───────────┴───────────┬────────────────┐
      │                │                       │                │
  Equipment       Consumables             Tools/Utility       Valuables
   (Worn)       (Single-Use)              (Multi-Use)       (Treasure)
      │                │                       │                │
 ┌────┴────┐      ┌────┴────┐             ┌────┴────┐      ┌────┴────┐
Weapon   Armor  Potion   Scroll         Wand    Container Gem      Gold
                  │        │              │         │
                Food    Spellbook       Tools      Bags
```

### Complete Item Class Matrix

| Class ID | C Constant | Symbol | Description | Key Sub-Types / Mechanics |
| :--- | :--- | :---: | :--- | :--- |
| 1 | `ILLOBJ_CLASS` | `]` | Illegal / Glitch Item | Internal sentinel |
| 2 | `WEAPON_CLASS` | `)` | Weapons & Missiles | Melee, Polearms, Thrown, Projectiles (arrows, bolts), Artifacts |
| 3 | `ARMOR_CLASS` | `[` | Defensive Apparel | Body Armor, Cloaks, Helmets, Shields, Gloves, Boots, Shirts |
| 4 | `RING_CLASS` | `=` | Finger Jewelry | Stat modifiers, Intrinsics, Regeneration, Sustenance, Polymorph |
| 5 | `AMULET_CLASS` | `"` | Neck Jewelry | Amulet of Yendor, Life Saving, Reflection, ESP, Strangulation |
| 6 | `TOOL_CLASS` | `(` | Devices & Implements | Bags, Containers, Keys, Lamps, Whistles, Horns, Instruments |
| 7 | `FOOD_CLASS` | `%` | Sustenance & Corpses | Rations, Fresh Corpses (intrinsic conferring), Tins, Fruits |
| 8 | `POTION_CLASS` | `!` | Liquid Mixtures | Dipping reagents, Quaffable buffs/debuffs, Holy/Unholy water |
| 9 | `SCROLL_CLASS` | `?` | Inscribed Parchment | Magic mapping, Enchantment, Teleportation, Identify, Remove Curse |
| 10 | `SPBOOK_CLASS` | `+` | Spell Manuals | Readable books to memorize spells; forgotten after 20,000 turns |
| 11 | `WAND_CLASS` | `/` | Charged Ray Projectors | Raycasting wands (death, fire, cold), utility (digging, wishing) |
| 12 | `COIN_CLASS` | `$` | Currency | Gold pieces; zero weight per unit or 100 per pound depending on cfg |
| 13 | `GEM_CLASS` | `*` | Minerals & Glass | Valuable gemstones, Dilithium, Soft rocks, Glass imitations |
| 14 | `ROCK_CLASS` | `` ` `` | Boulders & Statues | Heavy obstacles, Sokoban pushables, Trapped statue containers |
| 15 | `BALL_CLASS` | `0` | Iron Ball | Punishment tether ball |
| 16 | `CHAIN_CLASS` | `_` | Iron Chain | Links iron ball to player ankle |
| 17 | `VENOM_CLASS` | `.` | Weapon Poison Coating | Blinding venom, Acid venom |

---

## 3. Epistemic Taxonomy: The Knowledge States

Unlike most modern RPGs, NetHack features deep information hiding where player knowledge is an explicit dimension of game state.

```
                              Item Knowledge
                                     │
      ┌──────────────────────────────┼──────────────────────────────┐
      │                              │                              │
 Appearance                     BUC Status                     Properties
      │                              │                              │
 ┌────┴────┐                    ┌────┴────┐                    ┌────┴────┐
Randomized Known              Unknown   Known               Charges   Enchantment
 (e.g. "red potion")         (B / U / C)                  (e.g. "/ (0:3)") (+1, +2)
```

### Formal Epistemic Levels:

1. **Unidentified (`Unknown`)**:
   * Appearance is randomized per game (e.g., "bubbly potion", "scroll labeled ZELGO MER").
   * True object type (`otyp`), enchantment (`spe`), and BUC status are hidden.
2. **Type-Identified (`NameKnown`)**:
   * Player knows the true object archetype (e.g., "potion of extra healing").
   * Individual enchantment (`+2`) or remaining charges (`(0:4)`) may remain unknown.
3. **BUC-Identified (`BucKnown`)**:
   * Player has confirmed whether the item is Blessed, Uncursed, or Cursed (e.g., through altar drop or priest inspection).
4. **Fully Discovered (`FullyKnown`)**:
   * Archetype, BUC, enchantment, charges, and historical attributes are completely known.

---

## 4. Property & Intrinsic Taxonomy

Intrinsics represent boolean or integer abilities conferred upon actors. In NetHack, an ability can originate from four orthogonal sources:

```
                            Intrinsic Capability
                                     │
      ┌──────────────────────────────┼──────────────────────────────┐
      │                              │                              │
   Innate                         Extrinsic                      Temporary
  (Natural)                       (Equipment)                     (Timed)
      │                              │                              │
  Role/Race                      Worn Rings                     Quaffed Potions
  Level-Up                       Armor/Amulets                  Spells / Casts
  Corpse Eating                  Wielded Weapons                Status Ailments
```

### Core Intrinsics Classification

#### 1. Elemental & Attack Resistances
* **Fire Resistance**: Negates fire rays, fireball damage, lava insta-death (with fire immunity).
* **Cold Resistance**: Negates cold rays, prevents potion shattering from frost.
* **Shock / Electrical Resistance**: Negates lightning bolts, wand explosion shocks.
* **Sleep Resistance**: Immune to sleep gas, sleep wands, sleep spells.
* **Disintegration Resistance**: Immune to black dragon breath and disintegration beams.
* **Poison Resistance**: Immune to poison spikes, dart traps, toxic corpses.
* **Acid Resistance**: Immune to acid blobs, acid venom splashes.
* **Magic Resistance**: Blocks cancellation, death magic, touch of death, polymorph beams.
* **Reflection**: Bounces rays (magic missile, fire, cold, death, lightning) back along incidence angle.

#### 2. Sensory & Spatial Capabilities
* **Telepathy (ESP)**: Detects minded creatures across the entire level unless blind + non-innate.
* **Warning**: Detects adjacent monsters with danger level glyphs (1..5).
* **See Invisible**: Perceives invisible actors and stalks.
* **Invisibility**: Unseen by normal vision (negated by see invisible).
* **Stealth**: Prevents waking sleeping monsters upon entry into rooms.
* **Fast / Very Fast**: Modifies speed ration ($12 \to 18 \to 24$).
* **Searching**: Automatic passive detection of hidden secret doors and traps.

#### 3. Movement & Surface Interaction
* **Levitation**: Floats above terrain, avoiding traps, water, and lava; cannot pick up ground items.
* **Flying**: Voluntary controlled airborne movement; can descend to ground at will.
* **Water Walking**: Walking on surface of pools and moats without drowning.
* **Climbing**: Traverses sheer surfaces and spiderwebs.
* **Pass Walls (Phasing)**: Traverses solid stone, unpassable walls, and locked iron bars.

---

## 5. Combat & Damage Taxonomy

### Attack Modality (`aatyp`)

```
                                Attack Modality
                                       │
      ┌─────────────┬──────────────────┼──────────────────┬─────────────┐
      │             │                  │                  │             │
  Weapon Melee  Bare Hands          Natural            Gaze / Ray     Passive
   (AT_WEAP)     (AT_CLAW)             │               (AT_GAZ)      (AT_BOOM)
                                ┌──────┴──────┐
                               Bite         Breath
                             (AT_BITE)     (AT_BREA)
```

### Damage Types (`adtyp`)

* `AD_PHYS`: Standard kinetic physical damage (slashing, piercing, bludgeoning).
* `AD_MAGM`: Pure magical energy (Magic Missile).
* `AD_FIRE`: Thermal damage; destroys scrolls/potions in inventory.
* `AD_COLD`: Cryogenic damage; shatters potions.
* `AD_ELEC`: High-voltage electrical discharge; destroys wands and rings.
* `AD_DRST`: Strength draining via toxic poison.
* `AD_DRLI`: Level draining / Experience drain.
* `AD_RUST`: Water/corrosion rust damage to iron equipment.
* `AD_CORR`: Acidic corrosion damage to metallic items.
* `AD_DCAY`: Rotting organic matter (wood, leather).
* `AD_DISN`: Disintegration into dust.

---

## 6. Dungeon Hierarchy & Spatial Taxonomy

```
                           Dungeon Topology (DAG)
                                     │
                             Dungeons of Doom
                                     │
            ┌────────────────────────┼────────────────────────┐
            │                        │                        │
      Gnomish Mines               Sokoban                   Quest
            │                        │                        │
       Mine's End                 Finish                  Nemesis
            │
      Fort Ludios (branch portal)
            │
      Castle & Drawbridge
            │
     Valley of the Dead
            │
         Gehennom
            │
      Vlad's Tower & Wizard's Tower
            │
     Moloch's Sanctum (Vibrating Square)
            │
     Elemental Planes (Earth, Air, Fire, Water)
            │
       Astral Plane (Three Altars: Law, Neutral, Chaos)
```

### Room & Terrain Taxonomy

* **Room Types**:
  * `NORMAL`: Standard rectangular chambers.
  * `SHOP`: Commercial shopkeeper venue with inventory billing boundaries.
  * `TEMPLE`: Sanctuary containing consecrated altar and high priest.
  * `VAULT`: 2x2 treasure chamber enclosed by impenetrable stone.
  * `BARRACKS`: Soldier garrison with beds and armaments.
  * `ZOO`: Monster concentration enclosure.
  * `GRAVEYARD`: Undead vault with inscribed headstones.
  * `BEEHIVE`: Honeycomb complex with Queen Bee and royal jelly.
  * `SWAMP`: Waterlogged wetland level.
  * `LEPRECHAUN_HALL`: Gold hoard surrounded by leprechauns.

* **Tile Classifications**:
  * **Passable Walkways**: `Room`, `Corr`, `OpenDoor`, `BrokenDoor`, `IcePool`, `StairsUp`, `StairsDown`, `Altar`.
  * **Impassable Boundaries**: `Stone`, `WallHorizontal`, `WallVertical`, `ClosedDoor`, `LockedDoor`, `SecretDoor`, `IronBars`.
  * **Hazardous Surfaces**: `WaterPool`, `Moat`, `LavaPool`, `DrawbridgeGap`, `Fumarole`.

---

## 7. Turn Scheduler & Action Taxonomy

```
                               Turn Cycle
                                   │
                    ┌──────────────┴──────────────┐
                    │                             │
               Hero Phase                   Monster Phase
                    │                             │
            Check umovement >= 12         Iterate fmon queue
                    │                             │
          Execute rhack() command         mcalcmove() >= 12
                    │                             │
            Deduct umovement              Deduct monster movement
                    │                             │
                    └──────────────┬──────────────┘
                                   │
                        Both out of energy?
                                   │
                                Turn Tick
                                   │
                      svm.moves++ (Turn Advances)
                      Reallocate energy from speeds
                      Run nh_timeout() & hunger
```

### Action Energy Costs

| Action | Energy Cost | NetHack C Function | Rust Operational Effect |
| :--- | :---: | :--- | :--- |
| **Move / Step** | 12 (`NORMAL_SPEED`) | `domove()` | `EffectAst::SpendEnergy(12)` + `MovePlayer` |
| **Melee Strike** | 12 (`NORMAL_SPEED`) | `do_attack()` | `EffectAst::SpendEnergy(12)` + `InflictDamage` |
| **Open / Close Door** | 12 (`NORMAL_SPEED`) | `doopen()`, `doclose()` | `EffectAst::SpendEnergy(12)` + `SetTile` |
| **Quaff Potion** | 12 (`NORMAL_SPEED`) | `dopoison()`, `peffects()` | `EffectAst::SpendEnergy(12)` + `ConsumeItem` |
| **Read Scroll** | 12 (`NORMAL_SPEED`) | `doscr()` | `EffectAst::SpendEnergy(12)` + `SpellEffect` |
| **Zap Wand** | 12 (`NORMAL_SPEED`) | `dozap()` | `EffectAst::SpendEnergy(12)` + `Raycast` |
| **Search (1 turn)** | 12 (`NORMAL_SPEED`) | `dosearch()` | `EffectAst::SpendEnergy(12)` + `RevealCheck` |
| **Take Off Armor** | $12 \times \text{delay}$ | `Armor_off()` | Multi-turn occupation (`gm.multi`) |
| **Put On Armor** | $12 \times \text{delay}$ | `Armor_on()` | Multi-turn occupation (`gm.multi`) |
| **Wait (rest)** | 12 (`NORMAL_SPEED`) | `donothing()` | `EffectAst::SpendEnergy(12)` |
