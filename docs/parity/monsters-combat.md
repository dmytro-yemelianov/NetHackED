# Gap audit: Monsters and Combat (NetHack 5.0 C vs NetHackED)

Audited 2026-10-05. C tree: the NetHack 5.0.0 source tree (src/, include/). Rust: `crates/nethacked-*`.
Paths: `sim/` = `crates/nethacked-sim/src/`, `core/` = `crates/nethacked-core/src/`, `data/` = `crates/nethacked-data/src/`, `types/` = `crates/nethacked-types/src/lib.rs`.
"Spec div." means the gap is already listed under *Known divergences* in `docs/formal-mechanics-spec.md`.

## Bestiary count

| | Count | Source |
|---|---|---|
| C species | **383** | `include/monsters.h` expanded by the C preprocessor (`scripts/extract/c_tables.py`; a plain `grep -c MON(` counts 394, including `#if 0` blocks) |
| NetHackED archetypes | **383** | `data/nethack-5.0/monsters.toml` → `nethacked-data/src/generated/monsters.rs` (all generated, no hand entries) |
| Matching C names | 383 of 383 ("The Norn"/"The Dark One" carry an extra article; a repeated C name becomes its neutral name or "human …") | the generator asserts every C species appears once |
| Coverage | 100% of the species data. Behaviour is not covered by this: attack side effects, AI and generation rules are tracked in the rows below | |

What exists: goblin, hobgoblin, hill orc, kobold, jackal, giant ant, floating eye, skeleton, vampire, silver/red dragon, Medusa, master lich, shopkeeper, 6 pet tiers (dogs/cats), ghost, djinni, gnome, dwarf, priest, watchman, 9 quest leaders, 9 nemeses (counting the Cyclops), 9 guardians, Wizard of Yendor, Vlad, Croesus.
Classes missing completely include: nymphs, leprechauns, soldier ants and bees, all `e` lights and spheres, cockatrices, all `j` jellies, `F` lichens and molds, mimics, long worms, trolls, unicorns, all `&` demons (beyond nemeses), Angels, golems, zombies and mummies, elves, giants, `:` lizards, mind flayers, the Riders, `@` player monsters, and the soldiers and the rest of the watch.

Only 9 of the 12 `DamageType` variants are AD types (`types/lib.rs:591-610`: Phys, Fire, Cold, DrainStr, Stone, Slow, Paralyze, DrainLife, StealAmulet, Clerical, Spell, Stun). C has 50 `AD_*` (`include/monattk.h`).
`AttackType` has 9 variants (Claw, Bite, Kick, Touch, Breath, Gaze, Weapon, Magic, Passive). C has 16 `AT_*`. Missing: BUTT, STNG, HUGS, SPIT, ENGL, EXPL, BOOM, TENT.

## Table

| # | Behavior | C reference | Status | Evidence | Player impact |
|---|---|---|---|---|---|
| **monst.c / monsters.h** |||||
| 1 | Full bestiary (383 species) | include/monsters.h | ✅ PRESENT | data/nethack-5.0/monsters.toml, generated/monsters.rs; agent/tests/golden_determinism.rs `golden_vanilla_bestiary_combat_is_unchanged` (all 383 fight) | Every C species exists as data, with Ukrainian names. Whether it shows up in play depends on monster generation (makemon rows) |
| 2 | Per-monster stats (lvl, spd, AC, align, size, G_UNIQ) | monsters.h | ✅ PRESENT | data/monsters.rs; pinned by data/tests/bestiary_c_table.rs (spec div. "Bestiary data") | Stats of the existing 55 match C |
| 3 | Per-monster MR %, stone res, M2_MAGIC flags | monsters.h `mr`, `mflags2` | ❌ MISSING | spec div. "Bestiary data" | Spells and wands are resisted differently from C |
| 4 | Full AD_* set (50) | include/monattk.h | 🟡 PARTIAL | types/lib.rs:591-610 (12 variants) | Most special attacks cannot even be represented |
| 5 | Full AT_* set (16) | include/monattk.h | 🟡 PARTIAL | types/lib.rs:576-584 (9 variants) | No engulfers, exploders, huggers, stingers, spitters |
| **makemon.c** |||||
| 6 | Random generation by difficulty (`rndmonst`, `mkclass`) | makemon.c:1652, 1873 | 🟡 PARTIAL | data/generation.rs `rndmonst_adj` (difficulty window, G_FREQ, uncommon, align/temperature shift) over ruleset data; sim/actions/stairs.rs fills rooms 1/3 like mklev.c:973 | All 383 species can appear; `mkclass`, quest `qt_montype`, rogue and elemental-plane rules are still missing |
| 7 | Ongoing random spawns over time (1/50 or 1/70 per turn) | allmain.c `makemon(NULL…)` | ❌ MISSING | sim/turns.rs has no spawn call | Cleared levels stay empty, so there is no time pressure |
| 8 | Group generation (G_SGROUP/G_LGROUP, `m_initgrp`) | makemon.c:79 | ❌ MISSING | no match for grep `SGROUP\|m_initgrp` | No jackal packs and no hordes |
| 9 | Monster HP `d(lvl,8)` (`newmonhp`) | makemon.c:1012 | ❌ MISSING | data/monsters.rs:2340 uses fixed `base_hp` (spec div.) | Every monster of a species has identical HP |
| 10 | Starting weapons (`m_initweap`) | makemon.c:161 | ❌ MISSING | create_monster_record (data/monsters.rs:2335) gives no items | Orcs and dwarves do not carry or drop weapons; no early-game dwarvish mattock or pick |
| 11 | Starting inventory (`m_initinv`) | makemon.c:589 | ❌ MISSING | same | No loot from kills; shopkeepers carry no gold; no wands on soldiers |
| 12 | `peace_minded` at creation | makemon.c:2268-2308 | ✅ PRESENT | sim/peace.rs:99, core/peace.rs:111 | Co-aligned peacefuls work as in C |
| 13 | `set_malign` | makemon.c:2320 | ✅ PRESENT | sim/peace.rs:115, core/peace.rs:149 | Kill alignment matches C |
| 14 | Monster growth from kills (`grow_up`) | makemon.c:2051 | ❌ MISSING | no match for grep `grow_up`; pets grow only via feed_companion_pet (sim/monsters.rs:653) | Pets and monsters never level up by fighting |
| 15 | Genocide blocks spawns | makemon.c | ✅ PRESENT | sim/actions/stairs.rs:65, 740 | Works |
| **mon.c** |||||
| 16 | Corpse on death (`make_corpse`, `corpse_chance`) | mon.c:564, 3186 | 🟡 PARTIAL | sim/combat.rs:503-509 always drops a generic `Corpse` (no species or race name) | A corpse appears 100% of the time but carries no species, so eating it gives no intrinsics and cannot be cannibalism |
| 17 | Hero experience from kills (`experience`, `more_experienced`, `pluslvl`) | exper.c:85, 169, 307; mon.c:3484 `xkilled` | ❌ MISSING | no match for grep `experience\|uexp\|pluslvl` in sim | **The hero never gains levels**, and the quest (needs XL 14, core/quest.rs:88) cannot be reached by playing |
| 18 | Kill alignment, luck, and favor adjustments | mon.c:3676-3726 | ✅ PRESENT | sim/combat.rs:420-480 | Matches C |
| 19 | Death drops monster inventory (`relobj`) | steal.c:875 | ❌ MISSING | monsters carry nothing | No loot |
| 20 | Shapeshifters (chameleon, `newcham`) | mon.c:5287 | ❌ MISSING | no match for grep `chameleon\|newcham` | None |
| 21 | Monster speed (`mcalcmove`) | mon.c:1126 | ❌ MISSING | sim/turns.rs:100-104: every monster acts once per NORMAL_SPEED tick; `ActorRecord.speed` is unused | Fast monsters (dragons, soldier ants) are no faster than the hero, and slow ones are no slower |
| 22 | Monster HP regeneration (`mon_regen`) | monmove.c:311 | ❌ MISSING | sim/turns.rs:26-30 decrements only `mspec_used` | Wounded monsters stay wounded |
| 23 | Monster item pickup (`mpickstuff`) | mon.c:1852 | ❌ MISSING | no match for grep `mpickstuff` | Monsters ignore items |
| 24 | Medusa gaze response | mon.c:4118 | 🟡 PARTIAL | sim/monsters.rs:274-284, 454-492 (30 damage instead of stoning; spec div.) | Medusa does not stone you; she just hits hard |
| 25 | `setmangry` (anger, Elbereth hypocrisy) | mon.c:4274 | ✅ PRESENT | sim/peace.rs:168 | Matches C, without `growl` or `peacefuls_respond` |
| 26 | Lifesaving, vampire shifting, stoning monsters (`monstone`) | mon.c:2844, 3292, 3773 | ❌ MISSING | no match | None |
| **monmove.c / dogmove.c (AI)** |||||
| 27 | Approach hero | monmove.c `m_move` | 🟡 PARTIAL | sim/monsters.rs:386-404: a global Dijkstra field toward the hero, with no sight or memory | Monsters know where you are everywhere on the level, even when you are invisible |
| 28 | Fleeing (`monflee`, `distfleeck`) | monmove.c:462, 533 | 🟡 PARTIAL | sim/monsters.rs:388 flees whenever `hp <= max/3`; no flee timer | Flee behavior is predictable and differs from C |
| 29 | Elbereth (`onscary`) | monmove.c:241 | 🟡 PARTIAL | sim/peace.rs:135, sim/monsters.rs:322-345: works only while adjacent, no `monflee` timer, no Gehennom suppression (spec div.) | Elbereth is usable but stronger and simpler than C |
| 30 | Monsters open, unlock, or break doors | monmove.c `m_move` door handling | ❌ MISSING | types/lib.rs:416: closed doors are impassable to everything | Closing a door is a perfect escape |
| 31 | Sleeping or waiting monsters, `disturb` | monmove.c:327 | ❌ MISSING | ActorRecord has no sleep or wait state | Every monster is always awake and hunting |
| 32 | Peaceful wandering | monmove.c | 🟡 PARTIAL | sim/monsters.rs:219-249: random step | Mostly cosmetic |
| 33 | Monster digging, tunneling, phasing, swimming, flying | monmove.c:1108 `m_digweapon_check` | ❌ MISSING | movement uses only `is_passable` | Terrain blocks every monster equally |
| 34 | Invisible or displaced monsters | various | ❌ MISSING | no match for grep `invisible` in sim | None |
| **mhitu.c (monster vs hero); per AD_ type** |||||
| 35 | Melee slot loop, to-hit `rnd(20+i)`, `d(n,d)` | mhitu.c:768-912, 1187 | ✅ PRESENT | sim/combat.rs:558-656, core/combat.rs:228-325 | Melee numbers match C |
| 36 | AD_PHYS | mhitu.c `hitmu` | ✅ PRESENT | core/combat.rs:293 | Matches C |
| 37 | AD_FIRE / AD_COLD (resistance zeroes damage) | mhitu.c, uhitm.c `mhitm_ad_fire/cold` | 🟡 PARTIAL | core/combat.rs:278 `resisted`; no item burning or freezing | Potions and scrolls are never destroyed |
| 38 | AD_DRST / DRDX / DRCO (poison, attribute loss, instadeath) | uhitm.c `mhitm_ad_drst` | ❌ MISSING | enum DrainStr exists, no effect (spec div. (a)) | Poison is plain damage, so killer bees and soldier ants cannot kill outright |
| 39 | AD_DRLI (level drain) | `mhitm_ad_drli` | ❌ MISSING | spec div. (a) | Vampires and wraiths do not drain levels |
| 40 | AD_STON (stoning touch) | `mhitm_ad_ston` | ❌ MISSING | spec div. (a) | No cockatrice stoning (no cockatrices either) |
| 41 | AD_SLOW / AD_PLYS / AD_STUN | `mhitm_ad_slow/plys/stun` | 🟡 PARTIAL | sim/ad_effects.rs: AD_SLOW (`u_slow_down`) and AD_STUN (stun timer, half damage) on the hero; AD_PLYS melee still missing | Slowing and stunning hits work against the hero; no paralysis from melee yet |
| 42 | AD_SAMU (Wizard steals the Amulet) | `mhitm_ad_samu` | ❌ MISSING | enum StealAmulet, no effect | The Amulet is never stolen |
| 43 | AD_SGLD / AD_SITM / AD_SEDU (gold and item theft) | `mhitm_ad_sgld/sitm/sedu`, steal.c:58, 343 | ❌ MISSING | no match for grep `nymph\|leprechaun` | No nymph or leprechaun theft |
| 44 | AD_SSEX (seduction, foocubi) | mhitu.c:1985 `doseduce` | ❌ MISSING | no match for grep `seduc` | None |
| 45 | AD_ELEC / AD_ACID / AD_MAGM / AD_SLEE / AD_DISN | `mhitm_ad_*` | 🟡 PARTIAL | sim/ad_effects.rs: AD_ELEC (shock resistance, MC), AD_ACID (1/3, acid resistance), AD_SLEE (sleep `rnd(10)` turns, sleep resistance, MC) on the hero; AD_MAGM and AD_DISN missing; no item destruction | Shock, acid and sleep hits work against the hero |
| 46 | AD_RUST / AD_CORR / AD_DCAY / AD_ENCH (equipment damage) | `mhitm_ad_rust` etc. | ❌ MISSING | not in enum | Armor and weapons never erode in melee |
| 47 | AD_BLND / AD_CONF / AD_HALU | `mhitm_ad_blnd` etc. | 🟡 PARTIAL | sim/ad_effects.rs: AD_BLND (blindness timeout), AD_CONF (confusion, `mspec_used`) on the hero; AD_HALU missing; `can_blnd` approximated | Blinding and confusing hits work; confusion/stun randomize movement (hack.c:2420) |
| 48 | AD_TLPT / AD_LEGS / AD_STCK / AD_WRAP / AD_DGST (engulf-digest) | `mhitm_ad_*`, mhitu.c:1289 `gulpmu` | ❌ MISSING | AT_ENGL absent | No engulfing, no drowning by eels, no sticking |
| 49 | AD_WERE (lycanthropy) | `mhitm_ad_were` | ❌ MISSING | polymorph.rs mentions lycanthropy but there is no were species | None |
| 50 | AD_DISE / AD_PEST / AD_FAMN / AD_DETH (Riders) | `mhitm_ad_*` | ❌ MISSING | no Riders | Endgame Riders are absent |
| 51 | AD_SLIM / AD_POLY / AD_HEAL / AD_DRIN / AD_CURS / AD_RBRE | `mhitm_ad_*` | ❌ MISSING | not in enum (sliming exists only as a hero affliction) | None |
| 52 | AD_CLRC / AD_SPEL via AT_MAGC | mhitu.c:926 to `castmu` | 🟡 PARTIAL | see mcastu row 70 | Casters only summon skeletons or curse items |
| 53 | Explode-on-hit (AT_EXPL, `explmu`) | mhitu.c:1591 | ❌ MISSING | no AT_EXPL | No yellow lights or gas spores |
| 54 | Gaze attacks (`gazemu`) | mhitu.c:1668 | 🟡 PARTIAL | sim/monsters.rs:454-492: Medusa only, flat damage | Simplified |
| 55 | Hero passive vs monster (`passiveum`) | mhitu.c:2435 | ❌ MISSING | no match | Polymorphed-hero passives do nothing |
| 56 | Magic cancellation (MC) | mhitu.c `mhitm_mgc_atk_negated` | 🟡 PARTIAL | sim/ad_effects.rs `hero_negates`: best `a_can` of carried armor (from objects.h data), `rn2(10) < 3*mc`; Protection and amulet of guarding not counted | Cloaks protect against the handled special attacks |
| 57 | AC damage reduction `rnd(-u.uac)` | mhitu.c:1208 | ✅ PRESENT | core/combat.rs:181 | Matches C |
| **mhitm.c (monster vs monster)** |||||
| 58 | `mattackm` to-hit and damage | mhitm.c:293, 375-441, 1016 | ✅ PRESENT | core/combat.rs:238 `mhitm_to_hit`, sim/combat.rs:558 | Pet fights use C numbers |
| 59 | Hostiles attacking pets, monster infighting (`fightm`) | mhitm.c:106 | ❌ MISSING | sim/monsters.rs:318: hostiles only attack the hero | Pets are never attacked, so they cannot die in combat |
| 60 | `passivemm`, `gulpmm` | mhitm.c:1304, 849 | ❌ MISSING | no match | Pets suffer no passive damage, for example from acid blobs |
| **uhitm.c (hero attacks)** |||||
| 61 | `find_roll_to_hit`, `rnd(20)` | uhitm.c:365, 780 | 🟡 PARTIAL | core/combat.rs:55, sim/combat.rs:111-118; no `abon`, no monster-state modifiers (spec div.) | Close to C |
| 62 | `dmgval` weapon damage | weapon.c:216 | 🟡 PARTIAL | core/combat.rs:151; no per-weapon extras, silver, or blessed bonuses (spec div.) | Silver and blessed weapons have no edge against demons and undead |
| 63 | Bare-handed / martial arts | uhitm.c:838 | 🟡 PARTIAL | core/combat.rs:123; no damage doubling (spec div.) | Monks are weaker |
| 64 | Artifact special damage (Excalibur, Vorpal beheading, etc.) | artifact.c `artifact_hit` | 🟡 PARTIAL | sim/combat.rs:59-75, 191-217; name-matched; 1/20 Vorpal | Only 5 artifacts are handled |
| 65 | Floating-eye passive paralysis, acid passive, etc. (`passive`) | uhitm.c:5865 | ❌ MISSING | spec div. (g) | Hitting a floating eye is safe, which removes a classic death |
| 66 | Polymorphed-hero attacks (`hmonas`) | uhitm.c `hmonas` | ❌ MISSING | PolymorphForm has no archetype (spec div. "Worn armor") | Polymorph gives no new attacks |
| 67 | Thrown and fired missiles (hero) | dothrow.c `thitmonst` | 🟡 PARTIAL | sim/actions/ranged.rs:68: always hits for a flat 2 | Ranged combat is nearly useless |
| 68 | Peaceful attack confirm and safe displacement | uhitm.c:462-509 | ✅ PRESENT | sim/actions/movement.rs; spec div. "Peacefulness" | Matches C, with the listed gaps |
| 69 | `wakeup` on attack | uhitm.c:1923 | 🟡 PARTIAL | sim/combat.rs:188-195 calls setmangry; there is no sleep to wake from | Only peaceful-angering works |
| **mcastu.c** |||||
| 70 | Mage and cleric spell lists (`castmu`, `choose_magic_spell`) | mcastu.c:130 | 🟡 PARTIAL | sim/monsters.rs:497-575: only SummonMonsters (skeletons) and CurseItems, on a turn-modulo cooldown | No psi bolt, destroy armor, touch of death, haste self, or other spells |
| 71 | `buzzmu` (ray spells) | mcastu.c:989 | ❌ MISSING | no match | Liches do not cast cone of cold or fire |
| **muse.c** |||||
| 72 | Defensive item use (healing potions, teleport, digging to escape) | muse.c:441, 796 | ❌ MISSING | no match for grep `find_defensive` | Monsters never quaff or flee by item |
| 73 | Offensive item use (attack wands, potions, scrolls) | muse.c:1421, 1824 | ❌ MISSING | no match | No "The gnome zaps a wand of striking" |
| 74 | Misc item use (speed, invisibility, polymorph) | muse.c:2095, 2383 | ❌ MISSING | no match | None |
| **mthrowu.c** |||||
| 75 | Monster throws or fires missiles (`thrwmu`, `m_throw`, `monmulti`) | mthrowu.c:1174, 572, 201 | ❌ MISSING | spec div. (b) | No arrows, daggers, or spears thrown at you |
| 76 | Spit (`spitmu`) | mthrowu.c:1268 | ❌ MISSING | no AT_SPIT | None |
| 77 | Breath (`breamm`) | mthrowu.c:1093 | 🟡 PARTIAL | sim/monsters.rs:407-452, 576-651: fire and cold only; no bounce, range, or item destruction (spec div. (f)) | Dragon breath works in a basic form |
| **dog.c / dogmove.c (pets)** |||||
| 78 | Starting pet (`makedog`) | dog.c:219 | ❌ MISSING | data/roles.rs:422 `spawn_starting_pet` is never called (grep finds only the export in data/lib.rs:24); sim/world.rs:179 spawns no pet | **The hero starts with no pet** |
| 79 | Taming by thrown food or scroll (`tamedog`, `dogfood`) | dog.c:1147, 999 | ❌ MISSING | core/pet.rs:47 `feed_pet` is used only by `feed_companion_pet`, which only tests call | Pets cannot be made |
| 80 | Pets follow on stairs (`keepdogs`, `losedogs`) | dog.c:792, 304 | ❌ MISSING | sim/actions/stairs.rs:122-188 packs every non-steed actor into the level | Pets are left behind |
| 81 | Pet hunger, eating, starvation (`dog_hunger`, `dog_eat`) | dogmove.c:364, 218 | ❌ MISSING | no match | Pets never eat, starve, or go feral |
| 82 | Apport and item fetching (`dog_invent`, `droppables`) | dogmove.c:402, 28 | ❌ MISSING | PetGoal::FetchItem exists (core/pet_coop.rs) but is never chosen (`choose_pet_goal(.., None)` at sim/monsters.rs:116) | No shop-stealing with pets |
| 83 | Pet goal and movement (`dog_goal`, `dog_move`) | dogmove.c:485, 979 | 🟡 PARTIAL | sim/monsters.rs:77-210: attacks an adjacent hostile or follows within 2 tiles | Basic following works |
| 84 | Pet avoids cursed items | dogmove.c | ✅ PRESENT | core/pet_coop.rs:10, sim/monsters.rs:150 | Matches C |
| 85 | Pet displacement | hack.c | ✅ PRESENT | core/pet.rs:22, sim/actions/movement.rs:159-183 | Works (always swaps; spec div.) |
| 86 | Pet level-up and promotion | makemon.c:2121 `grow_up` | 🟡 PARTIAL | sim/monsters.rs:653-700: by feeding, at 4/7 instead of 4/6 (spec div.) | Unreachable in play |
| 87 | Abuse, `wary_dog`, untaming, killing-pet penalty | dog.c:1366, 1296; mon.c:3706 | 🟡 PARTIAL | only the -15 alignment for killing a pet (sim/combat.rs:465) | No tameness loss from abuse |
| 88 | Riding (`mount_steed`) | steed.c | 🟡 PARTIAL | sim/actions/ranged.rs:132-170 | Present in basic form (outside core scope) |
| **steal.c** |||||
| 89 | Nymph item theft and teleport away | steal.c:343 | ❌ MISSING | no nymph species | None |
| 90 | Leprechaun gold theft | steal.c:58 | ❌ MISSING | no leprechaun | None |
| 91 | `stealarm` (armor removal) | steal.c:165 | ❌ MISSING | no match | None |
| **minion.c** |||||
| 92 | Demon summoning (`msummon`) and bribery (`demon_talk`) | minion.c:57, 261 | ❌ MISSING | only a bribe-cap comment at sim/actions/religion.rs:294 | No demon lords or bribes |
| 93 | `summon_minion` (god's minion on prayer or anger) | minion.c:196 | ❌ MISSING | no Angel species | Angry gods send no minions |
| **priest.c** |||||
| 94 | Temple priest donation | priest.c:558 `priest_talk` | 🟡 PARTIAL | sim/actions/religion.rs:251+; extra uncurse and favor rules, no clairvoyance (spec div.) | Donations work, but the rules are not C's |
| 95 | Priest stays in temple (`pri_move`) | priest.c:177 | 🟡 PARTIAL | sim/monsters.rs:221-227: stationary | Never moves at all |
| 96 | Temple entry messages and Sanctum (`intemple`) | priest.c:410 | 🟡 PARTIAL | spec div. "Peacefulness": Sanctum high priest only | No "You have a forbidding feeling" |
| 97 | Angry priest god-smite (`ghod_hitsu`) | priest.c:796 | ❌ MISSING | spec div. | Attacking priests is safer than in C |
| **shk.c / shknam.c** |||||
| 98 | Buy: pick up an item, get a quote, pay (`addtobill`, `dopay`) | shk.c:3512, 1759 | 🟡 PARTIAL | sim/actions/inventory.rs:56-60, sim/actions/economy.rs:113-150; price computed at pay time (spec div.) | Buying works |
| 99 | Price formula (`get_cost`; CHA, dunce, tourist) | shk.c:2899 | 🟡 PARTIAL | sim/actions/economy.rs:96-111; fixed CHA, no o_id surcharge | Prices are close to C |
| 100 | Sell: drop an item, get an offer (`sellobj`, `set_cost`) | shk.c:3935, 3170 | ❌ MISSING | sim/actions/inventory.rs:72-104 `handle_drop` has no shop logic | You cannot sell loot |
| 101 | Price identification | shk.c `get_cost` o_id variance | 🟡 PARTIAL | sim/actions/economy.rs:152 non-C "appraise" command; no unidentified variance, so there is nothing to identify | Price-ID strategy is not possible |
| 102 | Shoplifting (`u_left_shop`, `rob_shop`, `call_kops`) | shk.c:579, 687, 510 | 🟡 PARTIAL | sim/actions/movement.rs:389-418: shopkeeper turns hostile; no Kops, no stolen_value | Theft is easier than in C |
| 103 | Shopkeeper blocks the door or follows (`shk_move`) | shk.c `shk_move` | ❌ MISSING | shopkeeper is stationary (sim/monsters.rs:221-227) | You can walk out freely while carrying a pick |
| 104 | Shop damage (dig, break door, `pay_for_damage`) | shk.c `costly_damage` / `pay_for_damage` | ❌ MISSING | no match for grep `costly\|pay_for` | No repair bills |
| 105 | Used-up merchandise fees (`stolen_value`, usage fee) | shk.c:3776 | ❌ MISSING | no match | Eating or using unpaid items is free |
| 106 | Shopkeeper names (shknam.c) and shop types | shknam.c | ❌ MISSING (UNSURE about stock) | every shopkeeper is literally named "shopkeeper" | Flavor loss |
| 107 | Angry shopkeeper surcharge and pacify | shk.c:1483 `make_angry_shk` | ❌ MISSING | sim/actions/economy.rs:96 passes `false` | None |
| **vault.c** |||||
| 108 | Vaults and vault guard (`invault`, `gd_move`, `paygd`) | vault.c:321, 892, 1209 | ❌ MISSING | no vault room type (dungeon/room.rs:41-43 has only Normal, Shop, Temple) | No vault gold or guard |
| **worm.c** |||||
| 109 | Long worms (segments, `cutworm`) | worm.c:120, 196, 373 | ❌ MISSING | only a "not modelled" note at sim/actions/movement.rs:43 | None |
| **wizard.c** |||||
| 110 | Wizard of Yendor placement and resurrection (`resurrect`) | wizard.c:715 | ❌ MISSING | the archetype exists (data/monsters.rs:2185) but nothing spawns it (no match in sim/) | No Wizard fights |
| 111 | Post-Amulet harassment (`intervene`, `nasty`, `aggravate`, `cuss`) | wizard.c:785, 591, 494, 846 | ❌ MISSING | no match for grep `intervene\|nasty` | The ascension run is trivial |
| 112 | Covetous `tactics` (steal and teleport) | wizard.c:369 | ❌ MISSING | no match | Nemeses and Wizard just walk at you |
| **explode.c** |||||
| 113 | Monster explosions (gas spore, light) and fireball area damage | explode.c:199 | ❌ MISSING | explosion code exists only for wands and bags of holding (core/artifacts_wands.rs, sim/actions/inventory.rs:168) | None |
| **mplayer.c** |||||
| 114 | Player monsters (Astral Plane, `mk_mplayer`) | mplayer.c:118, 327 | ❌ MISSING | no match for grep `mplayer` | Astral Plane has no rival adventurers |
| **quest.c** |||||
| 115 | Leader chat, level gate, artifact return | quest.c:282 `chat_with_leader`, 371 | 🟡 PARTIAL | core/quest.rs:80-131, sim/actions/stairs.rs:834 | Works, but needs XL 14, which is unreachable (row 17) |
| 116 | Expulsion for bad alignment | quest.c:186 `expulsion` | ❔ UNSURE | consult_leader returns Err strings; no permanent expulsion was found | Possibly missing |
| 117 | Nemesis speech, nemesis artifact drop | quest.c:403 `nemesis_speaks` | 🟡 PARTIAL | sim/combat.rs:483-500 drops the artifact on kill; no speech | Works mechanically |
| 118 | Guardian anger on attacking or killing the leader | mon.c:3733 | ✅ PRESENT | sim/combat.rs:349 | Matches C |
| **Bosses and other** |||||
| 119 | Vlad, Croesus, Medusa, master lich placement | special levels | ❌ MISSING (UNSURE for Medusa) | archetypes exist; no spawn found in sim/actions/stairs.rs | Bosses are not in play |
| 120 | Bones ghost | bones.c | ✅ PRESENT | sim/bones.rs:67-90 | Works |
| 121 | Summon-skeleton spell (non-C approximation) | mcastu.c | 🟡 PARTIAL | sim/monsters.rs:504-540 | Not C (C uses `nasty` or `makemon`) |

## Status counts (rows 1-121)

PRESENT: 15 · PARTIAL: 34 · MISSING: 71 (rows 106 and 119 have UNSURE sub-notes) · UNSURE: 1 (row 116)
