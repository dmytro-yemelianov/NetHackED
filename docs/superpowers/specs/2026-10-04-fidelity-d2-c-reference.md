# D2 research: monster and item data fidelity (C reference vs NetHackED)

Notation: C = `NetHack-5.0.0/` (paths `include/…`, `src/…`). Data = `crates/nethacked-data/src`. Sim = `crates/nethacked-sim/src`. Core = `crates/nethacked-core/src`.
`d(n,m)` = n dice of m sides; `rnd(n)` = 1..n; `rn2(n)` = 0..n-1; `rn1(x,y)` = y..y+x-1.
C `LVL(lvl, mov, ac, mr, aln)` (src/monst.c:34). `mr` is the monster's magic-resistance percentage, not an intrinsic. Alignment is a signed number (<0 chaotic, 0 neutral, >0 lawful, A_NONE = unaligned).
Mismatch markers: **X** = value differs, **!** = invented or not in NetHack, ok = matches.

---------------------------------------------------------------------
## 1. Bestiary (`Data/monsters.rs:98` BESTIARY, 47 entries)

NetHackED fields: `glyph, base_hp=max_hp, ac, level, speed, alignment, intrinsics, damage_dice (single), ai_behavior, abilities`. C monster HP is rolled, not fixed: `d(m_lev, 8)`, or `rnd(4)` at level 0 (makemon.c:1012-1043 `newmonhp`). `m_lev` is depth-adjusted by `adj_lev`. NetHackED uses a fixed `base_hp`.

Attack notation: `W`=AT_WEAP, `C`=AT_CLAW, `B`=AT_BITE, `T`=AT_TUCH, `K`=AT_KICK, `Br`=AT_BREA, `G`=AT_GAZE, `M`=AT_MAGC, `N`=AT_NONE (passive). The damage type is AD_PHYS unless one is given.

### 1a. Ordinary monsters

| NetHackED entry | C (monsters.h line) | sym C/NR | Lvl C/NR | Spd C/NR | AC C/NR | MR | Aln C/NR | C attacks | NR dice | Wt/Nut, size | Relevant C flags | Mismatches |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Goblin | goblin :727 | o/o | 0/1 | 6/9 | 10/6 | 0 | -3/C | W1d4 | 1d6 | 400/100 SMALL | M2_ORC, G_GENO | **X** lvl, spd, AC, dice |
| Hobgoblin | hobgoblin :734 | o/o | 1/2 | 9/9 | 10/5 | 0 | -4/C | W1d6 | 1d8 | 1000/200 HUMAN | M2_ORC STRONG | **X** lvl, AC, dice |
| Orc ("hill orc") | hill orc :752 | o/o | 2/2 | 9/9 | 10/4 | 0 | -4/C | W1d6 | 1d8 | 1000/200 HUMAN | MR_POISON, G_LGROUP | **X** AC, dice, missing poison res |
| Kobold | kobold :624 | k/k | 0/1 | 6/6 | 10/7 | 0 | -2/C | W1d4 | 1d4 | 400/100 SMALL | MR_POISON, M1_POIS, M2_HOSTILE | **X** lvl, AC |
| Jackal | jackal :199 | d/d | 0/1 | 12/12 | 7/7 | 0 | 0/N | B1d2 | 1d4 | 300/250 SMALL | M2_HOSTILE, G_SGROUP | **X** lvl, dice |
| GiantAnt | giant ant :89 | a/a | 2/3 | 18/18 | 3/3 | 0 | 0/N | B1d4 | 2d4 | 10/10 TINY | M2_HOSTILE | **X** lvl, dice, spurious poison res |
| FloatingEye | floating eye :333 | e/e | 2/2 | 1/1 | 9/9 | 10 | 0/N | **N AD_PLYS 0d70 (passive)** | 0d0 | 10/10 SMALL | M1_FLY NOLIMBS NOHEAD NOTAKE, M2_HOSTILE | **!** active Gaze ability is invented (range 4, 8 dmg); passive missing; spurious see_invisible |
| Skeleton | skeleton :2495 | **Z/z** | 12/3 | 8/10 | 4/4 | 0 | 0/C | W2d6, T AD_SLOW 1d6 | 1d8 | 300/5 HUMAN | MR cold/sleep/poison/stone, **M1_MINDLESS**, M2_UNDEAD HOSTILE, **G_NOGEN** | **X** glyph ('z' is zruty), lvl, spd, dice, resists; C never generates skeletons at random (NR spawns them at depth 3 and as lich summons) |
| Vampire | vampire :2281 | V/V | 10/8 | 12/12 | 1/1 | 25 | -8/C | C1d6, B AD_DRLI 1d6 | 2d8 | HUMAN/400 | MR sleep/poison, M1_FLY REGEN, M2_UNDEAD STALK HOSTILE SHAPESHIFTER | **X** lvl, dice, cold res is spurious and sleep/poison are missing |
| SilverDragon | silver dragon :1455 | D/D | 15/15 | 9/12 | -1/-1 | 20 | 4/L | Br AD_COLD 4d6, B3d8, C1d4, C1d4 | 4d8 + breath 3d6 | DRAGON/1500 GIGANTIC | MR_COLD, M1_SEE_INVIS; reflects (muse.c:2825) | **X** spd, attacks, breath dice, spurious fire res |
| RedDragon | red dragon :1484 | D/D | 15/15 | 9/12 | -1/-1 | 20 | -4/C | Br AD_FIRE 6d6, B3d8, C1d4, C1d4 | 4d8 + breath 3d6 | as above | MR_FIRE, M1_SEE_INVIS | **X** spd, attacks, breath dice |
| Medusa ("medusa") | Medusa :2836 | @/@ | 20/13 | 12/12 | 2/2 | 50 | -15/C | W2d4, C1d8, G AD_STON, B AD_DRST 1d6 | 2d6 + gaze | HUMAN/400 LARGE | MR poison/stone, M2_NOPOLY HOSTILE PNAME FEMALE, M3_WAITFORU, **G_UNIQ** | **X** name is lowercase, so `is_unique`=false (monsters.rs:886); lvl; attacks; the gaze deals 30 dmg instead of stoning (C mhitu.c:1702-1756 `done(STONING)` unless the hero reflects or can't see) |
| Lich ("master lich") | master lich :1880 | L/L | 17/14 | 9/12 | -4/0 | 90 | -15/C | T AD_COLD 3d6, M AD_SPEL | 3d8 | 1200/100 HUMAN | MR fire/cold/sleep/poison, M1_REGEN, M2_UNDEAD MAGIC, M3_WANTSBOOK, G_HELL | **X** lvl, spd, AC, attack type, resists, spurious telepathy; summoning on an 8-turn cooldown is an approximation of AD_SPEL |
| Shopkeeper | shopkeeper :2707 | @/@ | 12/12 | 16/12 | 0/0 | 50 | 0/N | W4d4, W4d4 | 2d6 | HUMAN | **M2_PEACEFUL**, M2_NOPOLY HUMAN STRONG MAGIC, G_NOGEN | **X** spd, attacks; mr 50 is modelled as the `magic_resistance` intrinsic |
| LittleDog | little dog :228 | d/d | 2/2 | 18/12 | 6/6 | 0 | 0/N | B1d6 | 1d6 | 150/150 SMALL | M2_DOMESTIC | **X** spd |
| Dog | dog :242 | d/d | 4/4 | 16/12 | 5/5 | 0 | 0/N | B1d6 | 2d6 | 400/200 MEDIUM | M2_DOMESTIC | **X** spd, dice |
| WarDog ("war dog") | **large dog :249** | d/d | 6/7 | 15/12 | 4/4 | 0 | 0/N | B2d4 | 3d6 | 800/250 MEDIUM | M2_STRONG DOMESTIC | **!** "war dog" is not a NetHack monster (it comes from Slash'EM). C grows dog into large dog (mondata.c:1231). **X** lvl, spd, dice |
| Kitten | kitten :381 | f/f | 2/2 | 18/12 | 6/6 | 0 | 0/N | B1d6 | 1d4 | 150/150 SMALL | M2_WANDER DOMESTIC | **X** spd, dice |
| Housecat | housecat :389 | f/f | 4/4 | 16/12 | 5/5 | 0 | 0/N | B1d6 | 2d4 | 200/200 SMALL | M2_DOMESTIC | **X** spd, dice |
| LargeCat | large cat :421 | f/f | 6/7 | 15/12 | 4/4 | 0 | 0/N | B2d4 | 3d4 | 250/250 SMALL | M2_STRONG DOMESTIC | **X** lvl, spd, dice |
| Ghost | ghost :2888 | **' '/G** | 10/10 | 3/12 | -5/-2 | 50 | -5/N | T1d1 | 1d8 | HUMAN/0 | MR cold/disint/sleep/poison/stone, M1_FLY WALLWALK UNSOLID, M2_NOPOLY UNDEAD STALK HOSTILE, G_NOGEN | **X** glyph (C S_GHOST is ' ', defsym.h:358; 'G' is the gnome class), spd, AC, dice, resists, alignment |
| Djinni | djinni :3188 | &/& | 7/7 | 12/12 | 4/0 | 30 | 0/N | W2d8 | 2d8 | 1500/400 HUMAN | MR poison/stone, M1_FLY POIS, M2_NOPOLY STALK, G_NOGEN | **X** AC, resists (NR has fire+telepathy) |
| Gnome | gnome :1681 | **G/g** | 1/1 | 6/10 | 10/10 | 4 | 0/N | W1d6 | 1d6 | 650/100 SMALL | M2_GNOME, G_SGROUP | **X** glyph (defsym.h:333), spd |
| Dwarf | dwarf :485 | h/h | 2/3 | 6/8 | 10/6 | 10 | 4/L | W1d8 | 1d8 | 900/300 HUMAN | M1_TUNNEL NEEDPICK, M2_DWARF STRONG GREEDY | **X** lvl, spd, AC (C 10 base; worn dwarvish gear lowers it through `find_mac`) |
| Priest ("priest") | **aligned cleric** (NAMS priest/priestess/aligned cleric) :2749 | @/@ | 12/10 | 12/12 | 10/2 | 50 | 0 (per-temple via EPRI)/L | W4d10, K1d4, M AD_CLRC | 2d8 | HUMAN | MR_ELEC, **M2_PEACEFUL** LORD NOPOLY HUMAN, G_NOGEN | **X** lvl, AC, attacks, resists. Do not confuse with the player-monster "priest/cleric" at :3396 (lvl 10, W1d6). |
| Watchman | watchman :2816 | @/@ | 6/4 | 10/12 | 10/5 | 0 | -2/**L** | W1d8 | 1d8 | HUMAN | **M2_PEACEFUL** MERC STALK, G_NOGEN SGROUP | **X** lvl, spd, AC, alignment sign |

### 1b. Quest leaders, nemeses, guardians, bosses

All C entries below are `G_NOGEN | G_UNIQ`. Leaders: `MS_LEADER`, `M2_PEACEFUL`, `M3_CLOSE`. Nemeses: `MS_NEMESIS`, `M2_HOSTILE`, `M3_WANTSARTI | M3_WAITFORU`. `AD_SAMU` = steal quest artifact or Amulet.

| NetHackED entry | C (line) | Lvl C/NR | Spd C/NR | AC C/NR | MR | Aln C/NR | C attacks | NR dice / abilities | Mismatches |
|---|---|---|---|---|---|---|---|---|---|
| TheNorn "The Norn" | **Norn** :3595 (HUGE, MR_COLD) | 20/20 | 15/14 | 0/-5 | 90 | 0/N | W4d10 ×2 | 3d8 | **X** name ("Norn"; C prints "the Norn"), spd, AC, dice |
| NeferetTheGreen | Neferet the Green :3604 | 20/22 | 15/15 | 0/-6 | 90 | 0/N | W4d10, M AD_SPEL 2d8 ×2 | 3d10 | **X** lvl, AC, attacks |
| Pelias | Pelias :3473 (MR_POISON) | 20/20 | 15/14 | 0/-4 | 90 | 0/N | W4d10 ×2 | 4d8 | **X** spd, AC, dice |
| KingArthur | King Arthur :3524 | 20/21 | 15/12 | 0/-7 | 90 | 20/L | W4d10 ×2 | 4d8 | **X** lvl, spd, AC, dice |
| GrandMaster | Grand Master :3533 (MR fire/elec/sleep/poison, SEE_INVIS) | 25/21 | 15/18 | 0/-6 | 90 | 0/**L** | C4d10, K2d8, M AD_CLRC 2d8 ×2 | 4d6 | **X** lvl, spd, AC, alignment, attacks |
| MasterAssassin (listed with the leaders, Stationary) | **Master Assassin :3723 = Rogue NEMESIS** (MS_NEMESIS, M2_HOSTILE) | 15/20 | 12/16 | 0/-4 | 30 | **18 (lawful)**/C | W AD_DRST 2d6, W2d8, C AD_SAMU 2d6 | 3d8 | **X** wrong group: it is the nemesis, not a leader; alignment, lvl, spd, AC, attacks. (quest.rs already names it as the Rogue nemesis.) |
| Hippocrates | Hippocrates :3515 (MR_POISON) | 20/20 | 15/12 | 0/-3 | 90 | 0/N | W1d6, M AD_CLRC 3d8 ×2 | 2d8 | **X** spd, AC, attacks |
| Twoflower | Twoflower :3584 | 20/18 | 15/12 | **10**/0 | 90 | 0/N | W4d10 | 2d6 | **X** lvl, spd, AC, dice |
| LordCarnarvon | Lord Carnarvon :3464 | 20/20 | 15/12 | 0/-4 | 90 | 20/L | W4d10, M AD_SPEL 4d8 | 3d8 | **X** spd, AC, attacks |
| LordSurtur | Lord Surtur :3749 (S_GIANT 'H', HUGE, MR fire/stone, ROCKTHROW) | 15/24 | 12/14 | 2/-8 | 50 | **12 (lawful)**/C | W2d10 ×2, C AD_SAMU 2d6 | 4d10 + **fire breath 4d8** | **!** breath is invented (no AT_BREA); fire res missing; **X** lvl, spd, AC, alignment |
| TheDarkOne "The Dark One" | **Dark One** :3759 (MR_STONE) | 15/25 | 12/15 | 0/-7 | 80 | -10/C | W1d6 ×2, C AD_SAMU 1d4, M AD_SPEL | 3d10 + curse + summon | **X** name, lvl, spd, AC, dice |
| ThothAmon | Thoth Amon :3627 (MR poison/stone) | 16/23 | 12/14 | 0/-6 | 10 | -14/C | W1d6, M AD_SPEL ×2, C AD_SAMU 1d4 | 4d8 + summon | **X** lvl, spd, AC, dice |
| Ixoth | Ixoth :3679 ('D', MR fire/stone, SEE_INVIS) | 15/25 | 12/15 | -1/-9 | 20 | -14/C | Br AD_FIRE 8d6, B4d8, M AD_SPEL, C2d4, C AD_SAMU 2d4 | 5d8 + breath 5d8 r7 | **X** lvl, spd, AC, attacks |
| MasterKaen | Master Kaen :3691 (MR poison/stone, SEE_INVIS) | 25/25 | 12/18 | -10/-10 | 10 | -20/C | C16d2 ×2, M AD_CLRC, C AD_SAMU 1d4 | 5d6 | **X** spd, attacks |
| MasterOfThieves (listed with the nemeses, MeleeHunter) | **Master of Thieves :3566 = Rogue LEADER** (MS_LEADER, M2_PEACEFUL); also the Tourist nemesis (role.c:471) | 20/22 | 15/16 | 0/-6 | 90 | -20/C | W4d10, W2d6, C AD_SAMU 2d4 | 4d8 | **X** grouping (peaceful as the Rogue leader, hostile on the Tourist quest through the quest level), lvl, AC |
| Cyclops | Cyclops :3669 (S_GIANT, MR_STONE, ROCKTHROW) | 18/22 | 12/12 | 0/-5 | 0 | -15/C | W4d8 ×2, C AD_SAMU 2d6 | 4d10 | **X** lvl, AC, dice |
| MinionOfHuhetotl | Minion of Huhetotl :3616 ('&', MR fire/poison/stone, FLY) | 16/24 | 12/14 | -2/-7 | 75 | -14/C | W8d4, W4d6, M AD_SPEL, C AD_SAMU 2d6 | 4d8 + **cold breath** | **!** breath is invented; **X** lvl, spd, AC |
| QuestGuardian "quest guardian" | **not a C monster.** C has one guardian species per role (role.c): Arc student :3773, Bar chieftain :3782, Cav neanderthal :3791, Hea attendant :3813, Kni page :3822, Mon abbot :3831, Pri acolyte :3840, Ran hunter :3849, Rog thug :3858, Sam roshi :3876, Tou guide, Val warrior :3897, Wiz apprentice. All are lvl 5, spd 12, AC 10, MS_GUARDIAN, M2_PEACEFUL, mostly W1d6/1d8. | –/14 | 12/12 | 10/0 | 10–30 | role/L | e.g. warrior W1d8 ×2 | 2d8 | **!** replace with the per-role guardian species |
| WizardOfYendor | Wizard of Yendor :2847 (MR fire/poison, FLY REGEN SEE_INVIS TPORT, M3_COVETOUS WAITFORU, M2_PRINCE) | 30/30 | 12/12 | -8/-8 | 100 | **A_NONE**/C | C AD_SAMU 2d12, M AD_SPEL | 4d8 + summon/5 | **X** alignment (unaligned), dice; covetous, so it ignores Elbereth (monmove.c:241) |
| VladTheImpaler | Vlad the Impaler :2313 (MR sleep/poison, FLY REGEN, M2_PRINCE) | 28/25 | **26**/18 | -6/-3 | 80 | -10/C | W2d10, B AD_DRLI 1d12 | 3d10 | **X** lvl, spd, AC, dice, spurious cold res |
| Croesus | Croesus :2859 (SEE_INVIS, M2_PRINCE, MS_GUARD) | 20/20 | 15/15 | 0/-5 | 40 | 15/L | W4d10 | 4d6 | **X** AC, dice |

Name and identity issues:
- `is_unique` comes from a capitalised name (monsters.rs:886). This makes "medusa" non-unique, and it would wrongly make any capitalised non-unique monster unique. Use a `G_UNIQ` flag instead.
- Renaming "The Norn" or "The Dark One" to the C names requires matching changes in `core/quest.rs:150-230`, `quest_species_by_name` (sim/actions/stairs.rs:30) and the nemesis kill check (sim/combat.rs:267).
- Renaming WarDog to LargeDog touches `PetSpeciesTier::WarDog` (core/pet_coop.rs:23-30, `NetMechanics/PetCoop.lean:72-108`), `feed_companion_pet` (sim/monsters.rs:522-547) and i18n.
- C grow-up thresholds: a pet promotes when `++m_lev >= mons[newtype].mlevel` (makemon.c:2121). Dog→large dog and housecat→large cat happen at level 6, little dog→dog and kitten→housecat at 4, one step per level gain. NR uses levels 4 and 7 (pet_coop.rs:56-100).
- Glyph mismatches (skeleton, ghost, gnome) change class-genocide results: `genocide::is_genocided(.., arch.glyph)`.
- Starting pet: C `pet_type` (dog.c:91-101) uses the role's `petnum`: Cav/Ran/Sam get a little dog, Kni a pony, Wiz a kitten, and every other role picks randomly. NR (roles.rs:353-361) gives Wizard and Healer a kitten and everyone else a little dog. C starting tameness is 10 for domestic pets (dog.c:49); NR uses 5 (monsters.rs:889).

---------------------------------------------------------------------
## 2. Item catalog (`Data/items.rs`, 66 ItemKindIds)

NetHackED `ItemArchetype` has `class, weight, cost, damage_small, damage_large, ac_bonus, is_container, is_bag_of_holding`. It has **no** fields for oc_magic, wand direction, nutrition, material, MC (`a_can`) or the two-hander flag. C damage values are dice sides: `rnd(oc_wsdam)` / `rnd(oc_wldam)`. C armor `a_ac = 10 - ac_param` (objects.h:427). Wand weight 7 and nutrition 30 come from objects.h:1448. Scrolls weigh 5 (:1185), potions 20 (:1124), spellbooks 50 with cost = level×100 (:1280).

| ItemKindId | C object (objects.h line) | Class C/NR | Cost C/NR | Wt C/NR | Dmg sm/lg C (NR) | a_ac C/NR | mgc | Dir / nutrition | Notes |
|---|---|---|---|---|---|---|---|---|---|
| Dagger | dagger :200 | weapon | 4/4 | 10/10 | d4/d3 (ok) | – | 0 | – | ok |
| ShortSword | short sword :244 | weapon | 10/10 | 30/30 | d6/d8 (ok) | – | 0 | – | ok |
| LongSword | long sword :270 | weapon | 15/15 | 40/40 | d8/d12 (ok) | – | 0 | – | ok |
| SilverSaber | silver saber :259 | weapon | 75/75 | 40/40 | d8/d8 (ok) | – | 0 | – | silver: +d20 vs silver-haters (weapon.c dmgval) |
| Mace | mace :355 | weapon | 5/5 | 30/30 | d6+1/d6 (NR d6/d6) | – | 0 | – | **X** +1 small bonus (weapon.c dmgval small-target switch) |
| LeatherArmor | leather armor :595 | armor | 5/5 | 150/150 | – | 2/2 | 0 | MC1 | ok (a_ac never used, §7) |
| ChainMail | chain mail :577 | armor | 75/75 | 300/300 | – | 5/5 | 0 | MC1 | ok |
| PlateMail | plate mail :556 | armor | 600/600 | 450/450 | – | 7/7 | 0 | MC2, bulky | ok |
| SilverDragonScaleMail | silver dragon scale mail :507 | armor | 1200/1200 | 40/40 | – | 9/9 | 1 | REFLECTING | ok |
| CloakOfMagicResistance | cloak of magic resistance :644 | armor | **60/50** | 10/10 | – | 1/1 | 1 | ANTIMAGIC, MC1 | **X** cost |
| WandOfStriking | striking :1464 | wand | 150/150 | 7/7 | NR (2,12) | – | 1 | IMMEDIATE | NR deals a fixed 12 dmg (sim/actions/items.rs:1025-1031). C: d(2,12) unless the target resists magic (zap.c:205) |
| WandOfDigging | digging :1486 | wand | 150/150 | 7/7 | – | – | 1 | RAY (via zap_dig, zap.c:3459) | **!** NR deals 12 dmg to the first actor hit (items.rs:1031 else-branch); C digging does not hurt monsters |
| WandOfTeleportation | teleportation :1478 | wand | **200/175** | 7/7 | – | – | 1 | IMMEDIATE | **X** cost; **!** NR deals 12 dmg where C teleports the monster |
| WandOfDeath | death :1497 | wand | 500/500 | 7/7 | NR (100,1) | – | 1 | RAY | NR deals 100 dmg; C kills outright unless the target is nonliving/demon or resists |
| WandOfWishing | wishing :1458 | wand | 500/500 | 7/7 | – | – | 1 | NODIR | C spe=1 (mkobj.c:1116) ok |
| WandOfCold | cold :1493 | wand | 175/175 | 7/7 | NR (6,6) | – | 1 | RAY | NR deals a fixed 18; C deals d(6,6) (zap.c:3464 nd=6), resisted by cold res |
| WandOfSecretDoorDetection | secret door detection :1451 | wand | 150/150 | 7/7 | – | – | 1 | NODIR | NR 13 charges; C rn1(5,11)=11..15 (mkobj.c:1123) |
| WandOfPolymorph | polymorph :1474 | wand | 200/200 | 7/7 | – | – | 1 | IMMEDIATE | NR always turns the target into a goblin (items.rs:1036) |
| PotionOfPolymorph | polymorph :1163 | potion | 200/200 | 20/20 | – | – | 1 | – | ok |
| SprigOfWolfsbane | sprig of wolfsbane :1088 | food | **7/300** | 1/1 | – | – | 0 | nut 40 | **X** cost (FOOD cost = nut/20+5); NR eating gives 400 (items.rs:733) |
| PotionOfHolyWater | **not a separate object**: blessed POT_WATER :1177 | potion | 100/**50** | 20/20 | – | – | 0 | – | **!** model as `PotionOfWater` + `Buc::Blessed` |
| PotionOfHealing | healing :1145 | potion | **20/100** | 20/20 | – | – | 1 | – | **X** cost |
| PotionOfExtraHealing | extra healing :1147 | potion | **100/150** | 20/20 | – | – | 1 | – | **X** cost |
| PotionOfSpeed | speed :1135 | potion | **200/150** | 20/20 | – | – | 1 | – | **X** cost |
| PotionOfWater | water :1177 | potion | 100/100 | 20/20 | – | – | 0 | – | ok |
| ScrollOfIdentify | identify :1213 | scroll | 20/20 | 5/5 | – | – | 1 | – | ok |
| ScrollOfTeleportation | teleportation :1207 | scroll | 100/100 | 5/5 | – | – | 1 | – | ok |
| ScrollOfRemoveCurse | remove curse :1195 | scroll | 80/80 | 5/5 | – | – | 1 | – | ok |
| ScrollOfEnchantWeapon | enchant weapon :1197 | scroll | 60/60 | 5/5 | – | – | 1 | – | ok |
| ScrollOfEnchantArmor | enchant armor :1187 | scroll | 80/80 | 5/5 | – | – | 1 | – | ok |
| ScrollOfCharging | charging :1225 | scroll | **300/80** | 5/5 | – | – | 1 | – | **X** cost |
| ScrollOfGenocide | genocide :1203 | scroll | 300/300 | 5/5 | – | – | 1 | – | ok |
| MagicMarker | magic marker :968 | tool | 50/50 | 2/2 | – | – | 1 | – | ok |
| Sack | sack :905 | tool (container) | 2/2 | 15/15 | – | – | 0 | – | ok |
| BagOfHolding | bag of holding :909 | tool (container) | 100/100 | 15/15 | – | – | 1 | – | ok |
| Chest | chest :901 | tool (container) | 16/16 | 600/600 | – | – | 0 | – | ok |
| MagicLamp | magic lamp :931 | tool | **50/1000** | 20/20 | – | – | 1 | – | **X** cost |
| OilLamp | oil lamp :929 | tool | 10/10 | 20/20 | – | – | 0 | – | ok |
| Boulder | boulder :1617 | rock | 0/0 | 6000/6000 | d20/d20 (NR 0) | – | 0 | – | **X** damage (thrown/dropped boulders) |
| FoodRation | food ration :1110 | food | 45/45 | 20/20 | – | – | 0 | nut 800, delay 5 | ok (NR hardcodes 800 by name, items.rs:728) |
| Apple | apple :1080 | food | **7/5** | 2/2 | – | – | 0 | nut 50 | **X** cost |
| Corpse | corpse :1050 | food | **5/0** | from monster `cwt` / NR 50 | – | – | 0 | nut = monster `cnutrit` | **X** NR uses a flat 400 nut (items.rs:733) and weight 50; C takes both from the monster's SIZ (see §1 Wt/Nut) |
| SpellbookOfForceBolt | force bolt :1319 | spellbook | 100/100 | 50/50 | – | – | 1 | IMMEDIATE, lvl 1 | ok |
| SpellbookOfHealing | healing :1313 | spellbook | 100/100 | 50/50 | – | – | 1 | IMMEDIATE, lvl 1 | ok |
| AmuletOfYendor | Amulet of Yendor :870 | amulet | 30000/30000 | 20/20 | – | – | 1 | – | ok |
| AmuletOfReflection | amulet of reflection :850 | amulet | 150/150 | 20/20 | – | – | 1 | REFLECTING | ok |
| Excalibur | artilist.h:85, base long sword | weapon | 4000/4000 | **40/30** | d8/d12 (ok) + PHYS(5,10) | – | – | – | **X** weight |
| VorpalBlade | artilist.h:191, base long sword | weapon | 4000/4000 | **40/30** | d8/d12 (ok) + PHYS(5,1) | – | – | – | **X** weight |
| Mjollnir | artilist.h:109, base war hammer :367 | weapon | 4000/4000 | 50/50 | d4+1/d4 (NR d4/d4) + ELEC(5,24) | – | – | – | **X** +1 small |
| Magicbane | artilist.h:145, base athame :212 | weapon | 3500/3500 | 10/10 | d4/d3 (ok) + STUN(3,4) | – | – | – | ok |
| EyeOfTheAethiopica | artilist.h:303, base amulet of ESP | amulet | 4000/4000 | 20/20 | – | – | – | – | ok |
| OrbOfFate | artilist.h:297, base crystal ball :938 | tool | 3500/3500 | **150/30** | none (NR d6) | – | 1 | – | **X** weight; **!** a non-weapon tool has no damage dice |
| HeartOfAhriman | artilist.h:225, base luckstone :1598 | gem | 2500/2500 | 10/10 | d3/d3 (NR d4) | – | 1 | – | **X** dice |
| MagicMirrorOfMerlin | artilist.h:255, base mirror :936 | tool | 1500/1500 | **13/15** | none (NR d4) | – | 0 | – | **X** weight, **!** dice |
| EyesOfTheOverworld | artilist.h:260, base lenses :944 | tool | 2500/2500 | **3/10** | none (NR d2) | – | 0 | – | **X** |
| MasterKeyOfThievery | artilist.h:279, base skeleton key :916 | tool | 3500/3500 | **3/10** | none (NR d6) | – | 0 | – | **X** |
| TsurugiOfMuramasa | artilist.h:285, base tsurugi :282 | weapon | 4500/4500 | 60/60 | **d16 / d8+2d6** (NR 2d8/3d8) | – | 0 | two-handed | **X** dice |
| PlatinumYendorianExpressCard | artilist.h:291, base credit card :920 | tool | 7000/7000 | **1/5** | none (NR d2) | – | 0 | – | **X** |
| StaffOfAesculapius | artilist.h:248, base quarterstaff :377 | weapon | 5000/5000 | **40/30** | **d6/d6** (NR 2d6/2d6) | – | 0 | two-handed | **X** weight, dice |
| OrbOfDetection | artilist.h:219, base crystal ball | tool | 2500/2500 | **150/30** | none (NR d4) | – | 1 | – | **X** |
| BellOfOpening | Bell of Opening :1025 | tool | 5000/5000 | 10/10 | none (NR d4) | – | 1 | – | **!** dice |
| CandelabrumOfInvocation | Candelabrum of Invocation :1021 | tool | 5000/5000 | **10/20** | none (NR d4) | – | 1 | – | **X** |
| BookOfTheDead | Book of the Dead :1438 | spellbook | 10000/10000 | **50/20** | none (NR d4) | – | 1 | – | **X** |
| WaxCandle | wax candle :925 | tool | **20/10** | 2/2 | none (NR d2) | – | 0 | – | **X** |
| GoldPieces | gold piece :1512 | coin | 1/1 | 1/1 (C stack weight is (n+50)/100) | – | – | 0 | – | NR weighs gold per item |
| Luckstone | luckstone :1598 | gem | 60/60 | 10/10 | d3/d3 (NR 0) | – | 1 | – | minor |

Item takeaways:
- C wands start with `rn1(5,4)` = 4..8 charges if directional and `rn1(5,11)` = 11..15 if NODIR (mkobj.c:1115-1124). NR (items.rs:924) gives a fixed 6 and 13.
- `ItemRecord` (arena lib.rs:20) stores no `ItemKindId`. Any lookup of `damage_*` or `ac_bonus` at combat time has to go by name, and the catalog has no name→id function. **Add `kind: Option<ItemKindId>` (serde default) or `fn item_kind_by_name`.**
- `core/inventory.rs:11-12` has its own `ItemKind::Weapon{damage_dice}` / `Armor{ac_bonus}` (hardcoded 6 at :189), which duplicates the catalog.

---------------------------------------------------------------------
## 3. Pantheons (C `src/role.c` vs `Data/pantheons.rs`)

C lists the gods lawful/neutral/chaotic. A leading `_` marks a goddess.

| Role | C (role.c line) | NetHackED | Status |
|---|---|---|---|
| Archeologist | Quetzalcoatl / Camaxtli / Huhetotl :41 | same | ok |
| Barbarian | Mitra / Crom / Set :82 | same | ok |
| Caveman | Anu / _Ishtar / Anshar :123 | role absent | – |
| Healer | _Athena / Hermes / Poseidon :164 | same | ok (no gender) |
| Knight | Lugh / _Brigit / Manannan Mac Lir :204 | same | ok |
| Monk | Shan Lai Ching / Chih Sung-tzu / Huan Ti :244 | same | ok |
| Priest | none: uses a random other role's gods (:285) | role absent | – |
| **Rogue** | **Issek / Mog / Kos** :328 | **Ishtar / Kos / Mog** (pantheons.rs:49-62) | **X** lawful should be Issek (Ishtar belongs to the Caveman pantheon); neutral and chaotic are swapped |
| Ranger | Mercury / _Venus / Mars :382 | role absent | – |
| Samurai | _Amaterasu Omikami / Raijin / Susanowo :423 | role absent | – |
| Tourist | Blind Io / _The Lady / Offler :463 | same | ok |
| Valkyrie | Tyr / Odin / Loki :503 | same | ok |
| Wizard | Ptah / Thoth / Anhur :543 | same | ok |

`get_patron_deity` maps Unaligned to neutral (pantheons.rs:136). C has no unaligned hero, so this is harmless.

---------------------------------------------------------------------
## 4. Monster attacks in the sim today

### 4a. Current path
- All melee goes through `resolve_combat` (sim/combat.rs:16), which is shared by hero→monster, monster→hero, pet→monster and monster→pet.
- **Damage:** `base_roll = rnd(6)` for every attacker (combat.rs:137). `melee_damage = base_roll + weapon_ench + skill_dmg_bonus`, minimum 1 (core/combat.rs:98). Monsters get enchant=0 and bonus=0, so **every monster hits for d6**: a jackal, Medusa or the Wizard of Yendor all deal 1..6. The hero gets d6+spe+skill whatever the weapon, bare hands included.
- **To-hit:** a hero attacker uses `to_hit_value`. **Any** non-hero attacker uses `monster_to_hit_value = AC_VALUE + 10 + m_lev` (combat.rs:120-135), including pet→monster and monster→pet. C mhitm (monster vs monster) uses `tmp = find_mac(mdef) + magr->m_lev` with no +10 (mhitm.c:320), plus 4 if the defender is confused or helpless. **X**: pets and the monsters they fight hit far too often.
- **One attack per turn**, with no `rnd(20+i)` die for the i-th attack.
- Defender AC: `defender.ac - Σ enchantment of ALL carried armor` (combat.rs:30-42). See §7.
- Special abilities (sim/monsters.rs:218-440) run before melee and replace it for the turn:
  - Gaze: range ≤4 with a clear line; deals a fixed 8 (paralysis) or 30 (stoning) dmg (:264-265).
  - Breath: `raw = n*m`, the maximum value with no roll (:291).
  - Spellcaster: fires on `turn % cooldown`.

### 4b. Data never read by the sim
| Field | Defined | Read? | Needs wiring at |
|---|---|---|---|
| `MonsterArchetype.damage_dice` | data/monsters.rs:93 | **never** (not copied into ActorRecord) | add `attacks: Vec<Attack>` (or `species` id) to ActorRecord (arena lib.rs:40); roll it in combat.rs:137 |
| `MonsterArchetype.ai_behavior` | :94 | only `is_tame` init (:888) | sim/monsters.rs:65-496 (Stationary/Shopkeeper/CautiousHunter are not distinguished) |
| `MonsterArchetype.abilities` | :95 | yes (copied into ActorRecord.abilities) | – |
| `MonsterArchetype.speed` | | copied; read only for steed move cost (sim/actions/movement.rs:29) | scheduler has one global `monster_speed` (core/energy.rs:15-56); C `mcalcmove` (mon.c:1126) is per monster |
| `ItemArchetype.damage_small/large` | data/items.rs:100 | **never** | combat.rs:137 (hero wielded weapon) |
| `ItemArchetype.ac_bonus` | :102 | **never** | combat.rs:30-37 (armor) |
| `ItemArchetype.cost` | | shop code (by catalog) | – |

### 4c. C rules to model
- `mattacku` (mhitu.c:491) loops `for (i = 0; i < NATTK; i++)` (mhitu.c:768) over the six attack slots.
  - Melee AT types AT_CLAW, KICK, BITE, STNG, TUCH, BUTT, TENT hit when `tmp > rnd(20+i)` (mhitu.c:794-806).
  - AT_WEAP (:883-912) uses the same die plus `hitval` of the monster's weapon. It throws instead (`thrwmu`) at range.
  - AT_HUGS (:823) hits automatically if the previous two attacks hit.
  - AT_GAZE (:832 → `gazemu` mhitu.c:1668) is active at range.
  - AT_BREA and AT_SPIT (:873-878) are ranged.
  - AT_MAGC (:926).
- `hitmu` (mhitu.c:1144):
  - `dmg = d(damn, damd)` (:1187), doubled for undead at midnight (:1189).
  - Then `mhitm_adtyping` (uhitm.c:4782) applies the AD_ effect.
  - Then negative hero AC subtracts `rnd(-u.uac)`, minimum 1 (mhitu.c:1208-1211; already in D1).
  - For AT_WEAP with a wielded weapon, AD_PHYS adds `dmgval(weapon)` (uhitm.c:4061 and :4152 in `mhitm_ad_phys`, from :3981).
- `mattackm` (mhitm.c:293, loop :375) does the same for monster vs monster with `tmp > rnd(20+i)` (mhitm.c:441/528); damage `d(damn,damd)` in `mdamagem` (mhitm.c:1025).
- Floating eye is **passive**, not active. The hero meleeing it triggers `passive()` (uhitm.c:5865):
  - Paralysis duration `tmp = d(m_lev+1, 70)` when damn=0 (uhitm.c:5885-5890).
  - It takes effect only if the eye is alive and not cancelled, with chance `rn2(3)` (:6019).
  - It needs the hero to see the eye (`canseemon`) and the eye to be able to see (`mcansee`).
  - Reflection blocks it. Free action reduces it to "momentarily stiffen".
  - Otherwise `nomul(-tmp)`, or -127 if Wis ≤ 12 and `!rn2(4)` (:6042).
  - Pets avoid floating eyes 9 times in 10 (dogmove.c:1130).
- AD types NetHackED can support now, with the existing Intrinsics fields (types lib.rs:586):

| AD | C handler (uhitm.c) | Hero-side effect (summary) | NR support |
|---|---|---|---|
| AD_PHYS | mhitm_ad_phys :3981 | plain dmg (+weapon dmgval) | yes |
| AD_FIRE | :2521 | dmg 0 if Fire_resistance; burns items | `fire_resistance` |
| AD_COLD | :2626 | dmg 0 if Cold_resistance | `cold_resistance` |
| AD_ELEC | :2684 | dmg 0 if Shock_resistance | `shock_resistance` |
| AD_SLEE | :3479 | if !Sleep_resistance, `fall_asleep(-rnd(10))` | `sleep_resistance` + needs a helpless counter |
| AD_DRST | :3122 | `!rn2(8)`: poison, Str loss or extra dmg, unless Poison_resistance | `poison_resistance` (no attributes yet: use extra dmg only) |
| AD_ACID | :2742 | dmg 0 if Acid_resistance; corrodes armor | `acid_resistance` |
| AD_PLYS | :3431 | `!rn2(3)`, if !Free_action, `nomul(-rnd(10))` | needs a helpless counter; Free_action not modelled |
| defer | AD_DRLI :2445, AD_STON :4203, AD_SAMU :4570, AD_SLOW :3652, AD_STUN :4388, AD_SPEL/CLRC | | treat as AD_PHYS dmg for D2 |

---------------------------------------------------------------------
## 5. Peacefulness

C `peace_minded` (makemon.c:2268-2310) is called at creation (makemon.c:1299 `mpeaceful = MM_ANGRY ? FALSE : peace_minded(ptr)`):
1. `M2_PEACEFUL` → peaceful. Covers shopkeeper, aligned cleric, watchman/captain, quest leaders and guardians.
2. `M2_HOSTILE` → hostile.
3. `msound == MS_LEADER || MS_GUARDIAN` → peaceful. `MS_NEMESIS` → hostile.
4. Racial: `race_peaceful` (`lovemask`) → peaceful, `race_hostile` → hostile. Gnomes/dwarves are peaceful to gnome/dwarf heroes; orcs are hostile to elves (also makemon.c:1335).
5. `sgn(mal) != sgn(ual)` → hostile.
6. A chaotic monster is hostile if the hero carries the Amulet. Minions are peaceful iff `ualign.record >= 0`.
7. Otherwise peaceful iff `rn2(16 + clamp(record, -15..)) && rn2(2 + abs(mal))`.

Special cases: a temple priest's alignment comes from its temple (EPRI). Bribing demon princes start peaceful (:1397). The Tourist-quest Master of Thieves (MS_LEADER) is made hostile by the quest level, not by `peace_minded`.

Other C behaviour:
- Elbereth `onscary` (monmove.c:241-302) does **not** scare:
  - shopkeepers, guards, priests in their temple (:266-267);
  - `@` humans and minotaurs;
  - peaceful or blind monsters (:299-301);
  - anything in Gehennom or the endgame.
- Elbereth only acts on a monster that is *nearby*: `distfleeck` (monmove.c:560-564) calls `monflee(rnd(rn2(7)?10:100))`.
- Pets skip a target if:
  - its level ≥ the pet's balk level;
  - it is tame;
  - its passive damage could kill the pet;
  - it is peaceful and the pet is below 25% HP, or it is peaceful and a leader or guardian (dogmove.c:1119-1128).

NetHackED today:
- There is **no peaceful flag**. Everything non-tame is hostile.
- The only exception is the shopkeeper, skipped by name when its alignment is Neutral (sim/monsters.rs:213-216). Shoplifting "angers" it by setting `alignment = Chaotic` (sim/actions/movement.rs:280-283).
- Priests and watchmen (stairs.rs:294-301) walk up to the hero and attack.
- The quest leader and guardians are made `is_tame = true` (stairs.rs:495-509), so they follow the hero and fight like pets, and the hero swaps places with them.
- Elbereth (sim/monsters.rs:446-475) repels every monster, shopkeeper included. Any monster that is not adjacent flees while Elbereth is active, which C does not do. `is_elbereth_ward_active` takes only blind and covetous flags (core/engraving.rs:69, Lean `Engraving.lean:51`).
- Pets (sim/monsters.rs:74-117) attack any adjacent non-tame actor, shopkeepers and priests included.
- Bumping a peaceful monster attacks it with no confirmation and no alignment penalty (sim/actions/movement.rs:78-82).

Minimal faithful model:
- Add `peaceful: bool` to ActorRecord (`#[serde(default)]`).
- Add species flags `always_peaceful`, `always_hostile`, `msound_class ∈ {Leader, Guardian, Nemesis, Other}` and `human`.
- Port `peace_minded` steps 1–3, 5 and 7 (race and Amulet optional), using the hero alignment record.
- Peaceful monsters do not attack or approach. They take a random step, or stay put for Stationary.
- Pets skip peacefuls under the dogmove rule above.
- Hero bump on a peaceful: ask for confirmation, or no-op in the agent API; attacking sets `peaceful=false` with an alignment penalty.
- Shopkeeper anger sets `peaceful=false` instead of the alignment hack.
- Quest leader and guardians become peaceful instead of tame.
- Elbereth exemptions: peaceful, `@` human, minotaur, shopkeeper; adjacent monsters only.

---------------------------------------------------------------------
## 6. Stationary monsters and AI

| C mechanism | Source | Who | NetHackED |
|---|---|---|---|
| `M3_WAITFORU` → `STRAT_WAITFORU`: doesn't act until it sees you or is hurt | makemon.c:1461; monmove.c:710-724 | Medusa, all nemeses, Wizard, Vlad | not modelled (nemeses hunt from spawn) |
| `M3_CLOSE` → `STRAT_CLOSE`: waits and lets you approach; quest_talk | makemon.c:1463; monmove.c:720 | quest leaders | leaders are tame followers (**X**) |
| `shk_move` / `pri_move` / `gd_move` | monmove.c:1806-1808 | shopkeeper at the shop door, priest stays in temple | shopkeeper skipped by name; priest has AiBehavior Stationary but **walks and attacks** (Stationary is never read) |
| Speed 1 | floating eye mov=1 (monsters.h:334) | floating eye | moves every MonsterStep like everyone else (one global monster speed) |
| `monflee`: timed flee | monmove.c:462-490 | after a hero hit: `!rn2(25) && mhp < mhpmax/2` → `monflee(!rn2(3) ? rnd(100) : 0)` (uhitm.c:625-628); Elbereth when nearby (monmove.c:564); pets hit by the hero (uhitm.c:1599) | NR: every monster flees each turn while `hp <= max_hp/3` (sim/monsters.rs:475); no flee timer; CautiousHunter and MeleeHunter behave the same |
| `M1_MINDLESS` | monflag.h:101 | skeleton, zombies, golems | no flag (it matters for Elbereth and sanctuary in some cases) |

AiBehavior mapping proposal:
- Replace `AiBehavior` with C-derived data: `waitforu`, `close`, `peaceful`, `role ∈ {Shopkeeper, Priest, Guard, Normal}` and per-monster `speed`.
- Keep `CompanionPet` only as a spawn hint. Tameness is per actor.
- Add a `flee_timer: u8` to ActorRecord to replace the HP-fraction rule.

---------------------------------------------------------------------
## 7. Armor AC in combat

C:
- `ARM_BONUS(obj) = a_ac + spe - min(greatest_erosion, a_ac)` (hack.h:1526-1528).
- Hero `find_ac` (do_wear.c:2473-2507): `uac = mons[u.umonnum].ac` (10 for human forms), then:
  - subtract ARM_BONUS for each **worn** slot (suit, cloak, helm, boots, shield, gloves, shirt);
  - subtract the spe of each ring of protection;
  - subtract 2 for an amulet of guarding;
  - subtract `u.ublessed` (divine protection) and `u.uspellprot`;
  - clamp to ±99.
- Monster AC `find_mac` (worn.c:717) is the base AC minus its worn armor's ARM_BONUS.

NetHackED:
- `defender.ac - Σ enchantment of all carried Armor-class items` (sim/combat.rs:30-42). `a_ac` (`ac_bonus`) is ignored, erosion is ignored, carried and worn are not distinguished, and the sum is recomputed per attack rather than stored.
- The hero's `ac` comes from a hardcoded per-role value (data/roles.rs:44, e.g. Valkyrie 7, Barbarian 6, Knight 5), which partly builds in the starting armor. If D2 starts adding `a_ac`, starting armor gets counted twice.
- Divine protection lowers `p.ac` directly (sim/actions/religion.rs:339), which is correct in spirit (`u.ublessed`).

Proposal:
- Hero base AC = 10. Compute `hero_ac = 10 - Σ ARM_BONUS(armor counted as worn) - protection`.
- For "worn": either add a `worn: bool` / slot to ItemRecord, or treat carried armor as worn, at most one per slot by catalog subtype. The arena needs the item kind or slot for this.
- Remove `RoleArchetype.ac`, or set it to 10.
- The Lean side needs a new `armBonus` def (Enchantment.lean or Combat.lean) with lemmas `armBonus ≥ spe` and `armBonus ≤ a_ac + spe`.

---------------------------------------------------------------------
## 8. Hero weapon damage

C `dmgval` (weapon.c:216-293):
- Base roll: large target (`bigmonst`, size ≥ MZ_LARGE) rolls `rnd(oc_wldam)` plus the large extras; otherwise `rnd(oc_wsdam)` plus the small extras.
  - Large extras: broadsword/runesword +1, flail +d4, battle-axe +2d4, tsurugi/two-handed sword/mattock +2d6.
  - Small extras: mace/war hammer/flail +1, broadsword/morning star +d4.
- `+= spe`, clamped to ≥ 0.
- Leather-or-softer weapons do 0 against thick-skinned targets.
- Bonuses: blessed vs undead/demon +d4; silver vs silver-haters +d20; axe vs wooden +d4.
- If tmp > 0: subtract `greatest_erosion`, minimum 1 (weapon.c:285-292).
- `hmon` then adds `dbon()` (Str) and `weapon_dam_bonus` (skill, already in D1), with a final minimum of 1 (uhitm.c:1505).
- Bare hands: `rnd(2)`, or `rnd(4)` with martial arts (uhitm.c:847). Non-weapon objects (tools such as the Orb or credit card): `rnd(2)` (uhitm.c:895).

NetHackED: `rnd(6) + spe + skill` for every weapon and for bare hands (combat.rs:137). **X**: a dagger should be 1..4 and a long sword 1..8 or 1..12 against large targets. Bare hands should be 1..2, not 1..6. Erosion is ignored.

Wiring:
- combat.rs:49-72 already finds the wielded item. Look up its catalog dice (needs a kind id, §2), choose small or large from defender size (needs a `size` field), then roll.
- Silver vs undead/demon could reuse the name-based `is_demon_or_undead` check (combat.rs:160).

---------------------------------------------------------------------
## 9. Proposed D2 scope (ordered by player impact)

| # | Item | Effort | Lean impact |
|---|---|---|---|
| 1 | **Monster attack data + multi-attack melee.** Port C attack lists into the bestiary (`attacks: [Attack{at, ad, n, d}]`). Monster→hero and monster→monster roll `d(n,d)` per melee attack. The i-th attack hits on `tmp > rnd(20+i)`. Replaces the d6 at combat.rs:137 for monsters. | M | Combat.lean: `meleeDamage` keeps `baseRoll` as a parameter (no change). `attackHits` generalises to a die of size `20+i`; existing theorems take the die size. Add a `diceRoll` range lemma (`n ≤ d(n,m) ≤ n·m`). |
| 2 | **Hero weapon damage = dmgval.** Small/large dice from the catalog, the small/large extras for the five weapons we have (mace, war hammer, tsurugi), `+spe` clamped ≥0, minus erosion with minimum 1. Bare hands `rnd(2)`. | S–M | Combat.lean `meleeDamage` base roll is still a parameter. Add `dmgval` def + lemmas (non-negative; erosion min 1 when positive). Proptests for the dice ranges. |
| 3 | **Armor AC via ARM_BONUS + hero base AC 10.** Use `ac_bonus`, erosion and a worn/slot notion. Remove the hardcoded role AC. | M | New `armBonus` def + bounds theorems. The AC feeds `toHitValue` and `monsterToHitValue` unchanged. |
| 4 | **Peaceful flag + peace_minded subset + Elbereth exemptions.** Shopkeeper, priest, watch, leader and guardians are peaceful. Pets and monsters don't attack peacefuls. Elbereth only works on adjacent, non-peaceful, non-@, non-shopkeeper monsters. Leader and guardians stop being tame. | M | `Engraving.lean isElberethWardActive` gains `peaceful`/`human`/`shopkeeper` params and new theorems (peaceful ignores ward). Optional `Peace.lean` for the `peace_minded` decision table (monotone in alignment sign). |
| 5 | **Bestiary data correction.** C lvl, spd, AC, MR resists (`mr1` → Intrinsics), alignment, glyph (skeleton Z, ghost ' ', gnome G), `G_UNIQ` flag, names (Norn, Dark One, Medusa). Leader and nemesis regrouping: Master Assassin is the nemesis, Master of Thieves the leader. Replace "war dog" with "large dog" and "quest guardian" with the per-role guardians. Fix the grow-up levels (4/6). HP from `d(lvl,8)`. | M (data + test churn) | PetCoop.lean `PetSpeciesTier.WarDog` → `LargeDog`, `promotePet` thresholds 4/6; re-prove `promote_*`. Quest.lean if names are modelled there. |
| 6 | **Floating eye passive.** Remove the invented active gaze. Add the passive paralysis on a hero melee hit (rn2(3), `d(lvl+1,70)` turns, reflection/blind immune). Needs a hero helpless counter. Medusa gaze becomes stoning (instadeath unless reflection or blind). | M | MonsterAbilities.lean `resolveGaze` gets re-scoped: passive vs active split, theorem "reflection ⇒ no paralysis". |
| 7 | **AD-type effects** for FIRE/COLD/ELEC/ACID (resisted → 0), DRST (`!rn2(8)` extra), SLEE/PLYS (helpless counter). Defer DRLI/STON/SAMU/SPEL (treat as PHYS). | M | Small new defs plus "resistance ⇒ 0 dmg" theorems, mirroring `MonsterAbilities.resolveBreathDamage`. |
| 8 | **Item catalog fixes.** Costs: potions of healing/extra healing/speed, charging, magic lamp, cloak of MR, wand of teleportation, wolfsbane, apple, corpse. Artifact base weights. Tsurugi/staff/Heart dice, no dice on tools. Holy water → blessed water. Add `nutrition` and `wand_dir` fields; corpse nutrition and weight from the species. Wand charges `rn1`. Wand damage rolls (striking 2d12, cold 6d6); digging and teleportation stop doing damage. | S | None, or Nutrition.lean only if nutrition values appear in theorems. |
| 9 | **Monster-vs-monster to-hit** (mhitm.c:320: `find_mac + m_lev`, no +10). | S | Combat.lean: new `monsterVsMonsterToHit` def + monotonicity theorem. |
| 10 | **Rogue pantheon** Issek/Mog/Kos. | S | none |
| 11 | **Breath and gaze rolls.** Breath `d(n,6)` from the C attack (silver 4d6, red 6d6, Ixoth 8d6) instead of `n*m`. Drop the invented breaths (Surtur, Minion). | S | MonsterAbilities.lean `breath_damage_le_raw` still holds (the raw roll is a parameter). |
| 12 | **Per-monster speed + WAITFORU/CLOSE + flee timer + Stationary priests/shopkeepers.** Replaces the global `monster_speed` scheduler. | L | Energy.lean scheduler model becomes per-actor movement points (`mcalcmove`), so its theorems need rework. Consider deferring to D3. |

Recommended D2 cut: items 1–5 and 8–11. Items 6–7 depend on a hero "helpless turns" counter. Item 12 (scheduler) is a candidate for D3.
